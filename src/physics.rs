#![allow(dead_code)]
//! 2D rigid-body physics, destructible castle structures, explosions, particles,
//! and scoring calibrated to Rovio's original Angry Birds 1.0 (`blocks.lua` & `gamelogic.lua`).

use crate::gamedata::{GameData, LevelDef, PHYSICS_SCALE, PHYSICS_TO_WORLD};
use macroquad::prelude::*;
use rapier2d::prelude::*;
use std::collections::{HashMap, HashSet};

pub const GRAVITY_Y: f32 = 20.0;
pub const SHOOT_MAX_LENGTH: f32 = 5.4; // meters (2.2 + 3.2 from gamelogic.lua)
pub const LAUNCH_SPEED_SCALE: f32 = 43.5; // m/s at 100% slingshot pull

#[derive(Clone, Debug)]
pub struct Particle {
    pub sprite: String,
    pub x: f32, // world pixels
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub rot: f32,
    pub vrot: f32,
    pub scale: f32,
    pub grow: f32,
    pub age: f32,
    pub lifetime: f32,
    pub gravity: f32,
}

#[derive(Clone, Debug)]
pub struct FloatingScore {
    pub x: f32, // world pixels
    pub y: f32,
    pub sprite: Option<String>, // e.g., "5K_GREEN", "10K_RED"
    pub text: String,           // numeric fallback / collision score
    pub color: Color,
    pub age: f32,
    pub lifetime: f32,
}

#[derive(Clone, Debug)]
pub struct TrailDot {
    pub x: f32, // world pixels
    pub y: f32,
    pub sprite: String,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct QueuedBird {
    pub name: String,
    pub definition: String,
    pub sprite: String,
    pub blink_sprite: String,
    pub radius: f32,
    pub ground_x: f32, // physics meters
    pub ground_y: f32,
    pub hop_timer: f32,
    pub hop_offset: f32,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub name: String,
    pub definition: String,
    pub material: String,
    pub sprite: String,
    pub base_sprite: String,
    pub blink_sprite: String,
    pub smile_sprite: String,
    pub is_bird: bool,
    pub is_pig: bool,
    pub is_static_prop: bool,
    pub max_health: f32,
    pub health: f32,
    pub defence: f32,
    pub radius: f32,
    pub half_w: f32,
    pub half_h: f32,
    pub specialty: String,
    pub has_collided: bool,
    pub bomb_fuse: Option<f32>,
    pub blink_timer: f32,
    pub blinking: bool,
    pub smiling: bool,
    pub oink_timer: f32,
}

#[derive(Clone, Debug)]
pub struct VisualProp {
    pub sprite: String,
    pub x: f32, // physics meters
    pub y: f32,
    pub angle: f32,
}

#[derive(Clone, Debug)]
pub struct PendingExplosion {
    pub x: f32,
    pub y: f32,
    pub force: f32,
    pub radius: f32,
    pub damage: f32,
    pub damage_radius: f32,
    pub sound: String,
    pub spawn_eggshells: bool,
}

pub struct LevelSimulation {
    pub level_index: usize,
    pub theme: String,
    pub silver_score: u32,
    pub gold_score: u32,

    pub rigid_bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub ccd_solver: CCDSolver,

    pub entities: HashMap<RigidBodyHandle, Entity>,
    pub body_by_name: HashMap<String, RigidBodyHandle>,
    pub visual_props: Vec<VisualProp>,
    pub joint_visuals: Vec<(RigidBodyHandle, Vec2, RigidBodyHandle, Vec2)>,

    pub slingshot_pos: Vec2, // physics meters (fork anchor)
    pub left_limit: f32,
    pub right_limit: f32,

    pub bird_queue: Vec<QueuedBird>,
    pub active_slingshot_bird: Option<QueuedBird>,
    pub load_bird_anim: Option<(QueuedBird, Vec2, f32)>, // (bird, start_pos, progress 0..1)

    pub flying_birds: Vec<RigidBodyHandle>,
    pub primary_flying_bird: Option<RigidBodyHandle>,
    pub specialty_available: bool,

    pub prev_trail: Vec<TrailDot>,
    pub cur_trail: Vec<TrailDot>,
    pub trail_timer: f32,

    pub particles: Vec<Particle>,
    pub floating_scores: Vec<FloatingScore>,
    pub pending_sounds: Vec<(String, f32)>,
    pub camera_shake: f32,

    pub score: u32,
    pub warmup_timer: f32,
    pub birds_fired: usize,
    pub post_shot_timer: f32,
    pub settle_timer: f32,
    pub bonus_awarded: bool,
}

impl LevelSimulation {
    pub fn new(level: &LevelDef, gdata: &GameData) -> Self {
        let mut rigid_bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut impulse_joints = ImpulseJointSet::new();
        let mut entities = HashMap::new();
        let mut body_by_name = HashMap::new();
        let mut visual_props = Vec::new();

        let mut min_x = -15.0f32;
        let mut max_x = 55.0f32;

        // Collect queued birds sorted by startNumber
        let mut raw_birds: Vec<&crate::gamedata::LevelObject> = level
            .objects
            .iter()
            .filter(|o| o.controllable || o.start_number > 0)
            .collect();
        raw_birds.sort_by_key(|o| {
            if o.start_number == 0 {
                999
            } else {
                o.start_number
            }
        });

        // The 1st bird (startNumber == 1) defines the slingshot; every waiting bird is lined up
        // behind it. 1.2.0 level files often keep stale editor coordinates for the queue
        // (Level 1 stores bird 2 at x = +3.1, i.e. inside the castle), so the authored x/y of a
        // waiting bird cannot be trusted - only the first bird's ground position is real.
        let (anchor_x, anchor_y, anchor_r) = raw_birds
            .first()
            .map(|b| {
                let r = if b.radius > 0.0 {
                    b.radius
                } else {
                    gdata.block_for(&b.definition).radius.unwrap_or(0.85)
                };
                (b.x, b.y, r)
            })
            .unwrap_or((-12.0, -1.0, 0.85));

        let mut bird_queue = Vec::new();
        for (i, bo) in raw_birds.iter().enumerate() {
            let bdef = gdata.block_for(&bo.definition);
            let r = if bo.radius > 0.0 {
                bo.radius
            } else {
                bdef.radius.unwrap_or(0.85)
            };
            // Waiting birds stand on the ground behind the slingshot, evenly spaced.
            let step = (anchor_r * 1.75).max(1.15);
            let queue_x = anchor_x - (i as f32) * step;
            min_x = min_x.min(queue_x - 8.0);
            bird_queue.push(QueuedBird {
                name: bo.name.clone(),
                definition: bo.definition.clone(),
                sprite: if !bo.sprite.is_empty() {
                    bo.sprite.clone()
                } else {
                    bdef.sprite.clone()
                },
                blink_sprite: bdef.sprite_blink.clone(),
                radius: r,
                ground_x: queue_x,
                ground_y: if i == 0 { anchor_y } else { bo.y.min(anchor_y) },
                hop_timer: 1.2 + (i as f32) * 0.85,
                hop_offset: 0.0,
            });
        }

        // Exact Rovio slingshot placement formula from gamelogic.lua lines 797..833:
        // SLING_SHOT_01_BACK has h=199, pivotY=30 -> (199 - 30) * 0.05 = 8.45m
        let slingshot_pos = if let Some(b1) = bird_queue.first() {
            vec2(anchor_x, anchor_y - 8.45 + b1.radius + 0.2)
        } else {
            vec2(-12.0, -8.3)
        };

        // Spawn blocks, pigs, terrain, and decorations
        for obj in &level.objects {
            if obj.controllable || obj.start_number > 0 {
                continue;
            }
            let bdef = gdata.block_for(&obj.definition);
            let mat_name = if !obj.material.is_empty() {
                obj.material.as_str()
            } else {
                bdef.material.as_str()
            };
            let mdef = gdata.material_for(mat_name);

            min_x = min_x.min(obj.x - 6.0);
            max_x = max_x.max(obj.x + 6.0);

            // Visual-only props (decorations, poles, start markers)
            if mat_name == "decoration" || bdef.collision == Some(false) {
                if !obj.sprite.is_empty() && obj.definition != "SpecialBlockStart" {
                    visual_props.push(VisualProp {
                        sprite: obj.sprite.clone(),
                        x: obj.x,
                        y: obj.y,
                        angle: obj.angle,
                    });
                }
                continue;
            }

            let density = obj.density.or(bdef.density).unwrap_or(mdef.density);
            let friction = obj
                .friction
                .or(bdef.friction)
                .unwrap_or(mdef.friction)
                .clamp(0.05, 2.5);
            let restitution = obj
                .restitution
                .or(bdef.restitution)
                .unwrap_or(mdef.restitution)
                .clamp(0.0, 0.85);
            let strength = obj
                .strength
                .or(bdef.strength)
                .unwrap_or(mdef.strength)
                .max(1.0);
            let defence = obj
                .defence
                .or(bdef.defence)
                .unwrap_or(mdef.defence)
                .max(0.5);

            let is_static = density <= 0.0 || mat_name == "immovable";
            let rb = if is_static {
                RigidBodyBuilder::fixed()
                    .translation(vector![obj.x, obj.y])
                    .rotation(obj.angle)
                    .build()
            } else {
                RigidBodyBuilder::dynamic()
                    .translation(vector![obj.x, obj.y])
                    .rotation(obj.angle)
                    .linear_damping(0.08)
                    .angular_damping(0.18)
                    .build()
            };
            let rb_handle = rigid_bodies.insert(rb);

            let is_circle = obj.shape_type == "circle" || bdef.shape_type == "circle";
            let r = if obj.radius > 0.0 {
                obj.radius
            } else {
                bdef.radius.unwrap_or(0.5)
            };
            let hw = obj.width.max(0.15);
            let hh = obj.height.max(0.15);

            // Triangle / polygon blocks (WoodBlock11/12, StoneBlock11/12, LightBlock11/12) are
            // authored as normalised vertices inside the w x h box. Giving them a full cuboid
            // made neighbouring triangles interpenetrate, which blew those castles apart the
            // instant a level loaded (this hit 49 of the 105 levels, every chapter 4-5 stage).
            let poly_points: Vec<Point<f32>> =
                if obj.shape_type == "polygon" || bdef.shape_type == "polygon" {
                    bdef.vertices
                        .iter()
                        .map(|v| point![(v.x - 0.5) * 2.0 * hw, (v.y - 0.5) * 2.0 * hh])
                        .collect()
                } else {
                    Vec::new()
                };
            let mut col_builder = if is_circle {
                ColliderBuilder::ball(r)
            } else if poly_points.len() >= 3 {
                ColliderBuilder::convex_hull(&poly_points)
                    .unwrap_or_else(|| ColliderBuilder::cuboid(hw, hh))
            } else {
                ColliderBuilder::cuboid(hw, hh)
            };
            col_builder = col_builder.friction(friction).restitution(restitution);
            if !is_static {
                col_builder = col_builder
                    .density(density.max(0.2))
                    .active_events(
                        ActiveEvents::COLLISION_EVENTS | ActiveEvents::CONTACT_FORCE_EVENTS,
                    )
                    .contact_force_event_threshold(2.0);
            }
            colliders.insert_with_parent(col_builder.build(), rb_handle, &mut rigid_bodies);

            let is_pig = obj.level_goal || bdef.level_goal || mat_name == "piglette";
            let init_sprite = if !obj.damage_sprite.is_empty() {
                obj.damage_sprite.clone()
            } else if !obj.sprite.is_empty() {
                obj.sprite.clone()
            } else {
                bdef.sprite.clone()
            };

            body_by_name.insert(obj.name.clone(), rb_handle);
            entities.insert(
                rb_handle,
                Entity {
                    name: obj.name.clone(),
                    definition: obj.definition.clone(),
                    material: mat_name.to_string(),
                    sprite: init_sprite.clone(),
                    base_sprite: if !obj.sprite.is_empty() {
                        obj.sprite.clone()
                    } else {
                        bdef.sprite.clone()
                    },
                    blink_sprite: obj.blink_sprite.clone(),
                    smile_sprite: obj.smile_sprite.clone(),
                    is_bird: false,
                    is_pig,
                    is_static_prop: is_static,
                    max_health: strength,
                    health: strength,
                    defence,
                    radius: r,
                    half_w: hw,
                    half_h: hh,
                    specialty: bdef.specialty.clone(),
                    has_collided: false,
                    bomb_fuse: None,
                    blink_timer: 1.5 + (entities.len() as f32 * 0.73) % 3.5,
                    blinking: false,
                    smiling: false,
                    oink_timer: 2.5 + (entities.len() as f32 * 1.37) % 6.0,
                },
            );
        }

        // Ground body (top surface at y = 0.0)
        let mid_x = (min_x + max_x) * 0.5;
        let ground_rb = RigidBodyBuilder::fixed()
            .translation(vector![mid_x, 5.0])
            .build();
        let ground_handle = rigid_bodies.insert(ground_rb);
        let ground_col = ColliderBuilder::cuboid(1000.0, 5.0)
            .friction(0.85)
            .restitution(0.05)
            .build();
        colliders.insert_with_parent(ground_col, ground_handle, &mut rigid_bodies);

        // Joints (chains, tire swings, hanging structures)
        let mut joint_visuals = Vec::new();
        for j in &level.joints {
            if let (Some(&h1), Some(&h2)) = (body_by_name.get(&j.end1), body_by_name.get(&j.end2)) {
                let b1 = &rigid_bodies[h1];
                let b2 = &rigid_bodies[h2];
                let p1_world = point![j.x1, j.y1];
                let p2_world = point![j.x2, j.y2];
                let p1_local = b1.position().inverse_transform_point(&p1_world);
                let p2_local = b2.position().inverse_transform_point(&p2_world);
                let dist = ((j.x2 - j.x1).powi(2) + (j.y2 - j.y1).powi(2)).sqrt();

                if j.joint_type == 2 || dist < 0.15 {
                    let joint = RevoluteJointBuilder::new()
                        .local_anchor1(p1_local)
                        .local_anchor2(p2_local)
                        .build();
                    impulse_joints.insert(h1, h2, joint, true);
                } else {
                    let joint = RopeJointBuilder::new(dist.max(0.2))
                        .local_anchor1(p1_local)
                        .local_anchor2(p2_local)
                        .build();
                    impulse_joints.insert(h1, h2, joint, true);
                    // Also add slight spring/distance constraint via spherical or keep rope
                }
                joint_visuals.push((
                    h1,
                    vec2(p1_local.x, p1_local.y),
                    h2,
                    vec2(p2_local.x, p2_local.y),
                ));
            }
        }

        // Load first bird directly into slingshot
        let active_slingshot_bird = if !bird_queue.is_empty() {
            Some(bird_queue.remove(0))
        } else {
            None
        };

        let silver_score = if level.stars.silver_score > 0 {
            level.stars.silver_score
        } else {
            25_000
        };
        let gold_score = if level.stars.gold_score > 0 {
            level.stars.gold_score
        } else {
            silver_score + 10_000
        };

        let mut sim = Self {
            level_index: level.index,
            theme: level.theme.clone(),
            silver_score,
            gold_score,
            rigid_bodies,
            colliders,
            integration_parameters: {
                // Angry Birds levels author blocks so that neighbours often touch (and a few
                // round blocks deliberately overlap by a few centimetres). Rapier's stock
                // settings shove such bodies apart at up to ~10 units/s, which used to blow
                // whole castles apart on the first frame. Box2D (the original engine) pushes
                // overlapping bodies apart slowly, so match that instead.
                // Angry Birds castles are tall stacks of thin planks; Rapier's stock four
                // velocity iterations let them creep apart over ~10 s and topple on their
                // own. Extra solver + friction iterations keep loaded towers standing.
                let mut ip = IntegrationParameters::default();
                ip.num_solver_iterations = std::num::NonZeroUsize::new(8).unwrap();
                ip.num_additional_friction_iterations = 4;
                ip
            },
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints,
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            entities,
            body_by_name,
            visual_props,
            joint_visuals,
            slingshot_pos,
            left_limit: min_x - 30.0,
            right_limit: max_x + 35.0,
            bird_queue,
            active_slingshot_bird,
            load_bird_anim: None,
            flying_birds: Vec::new(),
            primary_flying_bird: None,
            specialty_available: false,
            prev_trail: Vec::new(),
            cur_trail: Vec::new(),
            trail_timer: 0.0,
            particles: Vec::new(),
            floating_scores: Vec::new(),
            pending_sounds: Vec::new(),
            camera_shake: 0.0,
            score: 0,
            warmup_timer: 0.55,
            birds_fired: 0,
            post_shot_timer: 0.0,
            settle_timer: 0.0,
            bonus_awarded: false,
        };

        // Pre-settle 20 physics substeps silently so stacked blocks rest snugly on frame 1
        sim.pre_settle();
        sim
    }

    fn pre_settle(&mut self) {
        let gravity = vector![0.0, GRAVITY_Y];
        for _ in 0..20 {
            self.physics_pipeline.step(
                &gravity,
                &self.integration_parameters,
                &mut self.island_manager,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.rigid_bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.ccd_solver,
                None,
                &(),
                &(),
            );
        }
        // Zero tiny residual velocities after pre-settle
        for (_, rb) in self.rigid_bodies.iter_mut() {
            if rb.is_dynamic() {
                rb.set_linvel(vector![0.0, 0.0], false);
                rb.set_angvel(0.0, false);
            }
        }
    }

    /// Safety net: swaps any sprite that is missing from the loaded 1.2.0 atlases for the
    /// block definition's own sprite (or its first damage tier). Without this a level that
    /// references an old editor sprite id would draw nothing at all.
    pub fn repair_sprites(&mut self, gdata: &GameData, assets: &mut crate::assets::Assets) {
        for ent in self.entities.values_mut() {
            if assets.has_sprite(&ent.sprite) {
                continue;
            }
            let bdef = gdata.block_for(&ent.definition);
            let mut candidates = vec![bdef.sprite.clone()];
            for tier in bdef.damage_sprites.values() {
                candidates.push(tier.sprite.clone());
            }
            if let Some(good) = candidates
                .into_iter()
                .find(|s| !s.is_empty() && assets.has_sprite(s))
            {
                ent.sprite = good.clone();
                if !assets.has_sprite(&ent.base_sprite) {
                    ent.base_sprite = good;
                }
            }
        }
        self.visual_props.retain(|vp| assets.has_sprite(&vp.sprite));

        // Ground / static terrain blocks are texture-filled silhouettes in 1.2.0. Bake each
        // distinct (mask, texture) pair once so cliffs render as dirt instead of flat brown.
        let mut jobs: Vec<(RigidBodyHandle, String, String)> = Vec::new();
        for (h, ent) in self.entities.iter() {
            if ent.sprite.is_empty() {
                continue;
            }
            let bdef = gdata.block_for(&ent.definition);
            if bdef.texture.is_empty() {
                continue;
            }
            jobs.push((*h, ent.sprite.clone(), bdef.texture.clone()));
        }
        let mut baked: HashMap<String, String> = HashMap::new();
        for (_, mask, fill) in jobs.iter() {
            let cache_key = format!("{}#{}", mask, fill);
            if !baked.contains_key(&cache_key) {
                if let Some(name) = assets.bake_texture_fill(mask, fill) {
                    baked.insert(cache_key, name);
                }
            }
        }
        for (h, mask, fill) in jobs.iter() {
            if let Some(name) = baked.get(&format!("{}#{}", mask, fill)) {
                if let Some(ent) = self.entities.get_mut(h) {
                    ent.sprite = name.clone();
                    ent.base_sprite = name.clone();
                }
            }
        }
        for qb in &mut self.bird_queue {
            if !assets.has_sprite(&qb.sprite) {
                let bdef = gdata.block_for(&qb.definition);
                if assets.has_sprite(&bdef.sprite) {
                    qb.sprite = bdef.sprite.clone();
                }
            }
        }
        if let Some(sb) = &mut self.active_slingshot_bird {
            if !assets.has_sprite(&sb.sprite) {
                let bdef = gdata.block_for(&sb.definition);
                if assets.has_sprite(&bdef.sprite) {
                    sb.sprite = bdef.sprite.clone();
                }
            }
        }
    }

    /// Runs the simulation forward without the player so a freshly loaded castle is already
    /// resting on its supports before the first frame is shown (the original does the same;
    /// otherwise blocks that are authored a few centimetres off the ground visibly drop).
    pub fn settle(&mut self, gdata: &GameData, steps: usize) {
        for _ in 0..steps {
            self.step(1.0 / 60.0, gdata);
        }
        self.pending_sounds.clear();
        self.cur_trail.clear();
        self.prev_trail.clear();
        self.floating_scores.clear();
        self.particles.clear();
    }

    pub fn remaining_pigs(&self) -> usize {
        self.entities.values().filter(|e| e.is_pig).count()
    }

    pub fn unused_birds_count(&self) -> usize {
        self.bird_queue.len()
            + usize::from(self.active_slingshot_bird.is_some())
            + usize::from(self.load_bird_anim.is_some())
    }

    /// Launches the bird currently in the slingshot from `release_pos` (in physics meters).
    pub fn launch_bird(&mut self, release_pos: Vec2, gdata: &GameData) {
        let Some(qbird) = self.active_slingshot_bird.take() else {
            return;
        };
        let pull_vec = self.slingshot_pos - release_pos;
        let pull_len = pull_vec.length().min(SHOOT_MAX_LENGTH);
        if pull_len < 0.35 {
            // Put back in slingshot
            self.active_slingshot_bird = Some(qbird);
            return;
        }
        let dir = pull_vec / pull_len;
        let ratio = (pull_len / SHOOT_MAX_LENGTH).clamp(0.0, 1.0);
        let speed = LAUNCH_SPEED_SCALE * ratio;
        let vel = dir * speed;

        if !self.cur_trail.is_empty() {
            self.prev_trail = std::mem::take(&mut self.cur_trail);
        }
        self.trail_timer = 0.0;

        let bdef = gdata.block_for(&qbird.definition);
        let density = bdef.density.unwrap_or(6.0);
        let friction = bdef.friction.unwrap_or(0.3);
        let restitution = bdef.restitution.unwrap_or(0.35);

        let rb = RigidBodyBuilder::dynamic()
            .translation(vector![release_pos.x, release_pos.y])
            .linvel(vector![vel.x, vel.y])
            .linear_damping(0.05)
            .angular_damping(0.25)
            .ccd_enabled(true)
            .build();
        let rb_h = self.rigid_bodies.insert(rb);

        let col = ColliderBuilder::ball(qbird.radius)
            .density(density)
            .friction(friction)
            .restitution(restitution)
            .active_events(ActiveEvents::COLLISION_EVENTS | ActiveEvents::CONTACT_FORCE_EVENTS)
            .contact_force_event_threshold(1.0)
            .build();
        self.colliders
            .insert_with_parent(col, rb_h, &mut self.rigid_bodies);

        let flying_sprite = if !bdef.sprite_flying.is_empty() {
            bdef.sprite_flying.clone()
        } else {
            qbird.sprite.clone()
        };

        self.entities.insert(
            rb_h,
            Entity {
                name: qbird.name.clone(),
                definition: qbird.definition.clone(),
                material: bdef.material.clone(),
                sprite: flying_sprite,
                base_sprite: qbird.sprite,
                blink_sprite: qbird.blink_sprite,
                smile_sprite: String::new(),
                is_bird: true,
                is_pig: false,
                is_static_prop: false,
                max_health: 1000.0,
                health: 1000.0,
                defence: 100.0,
                radius: qbird.radius,
                half_w: qbird.radius,
                half_h: qbird.radius,
                specialty: bdef.specialty.clone(),
                has_collided: false,
                bomb_fuse: None,
                blink_timer: 99.0,
                blinking: false,
                smiling: false,
                oink_timer: 99.0,
            },
        );

        self.flying_birds.clear();
        self.flying_birds.push(rb_h);
        self.primary_flying_bird = Some(rb_h);
        self.specialty_available = !bdef.specialty.is_empty();
        self.birds_fired += 1;
        self.post_shot_timer = 0.0;
        self.settle_timer = 0.0;

        self.pending_sounds.push(("bird_shot".into(), 0.85));
        if !bdef.launch_sound.is_empty() {
            self.pending_sounds.push((bdef.launch_sound, 0.95));
        }
    }

    /// Triggers mid-air specialty of the active launched bird (Red yell, Blue 3x split,
    /// Yellow speed boost, Black bomb explosion, White egg-grenade drop + upward recoil).
    pub fn trigger_specialty(&mut self, gdata: &GameData) {
        if !self.specialty_available {
            return;
        }
        let Some(rb_h) = self.primary_flying_bird else {
            return;
        };
        let Some(ent) = self.entities.get(&rb_h).cloned() else {
            return;
        };
        if ent.has_collided && ent.specialty != "BOMB" {
            return;
        }
        let Some(rb) = self.rigid_bodies.get(rb_h) else {
            return;
        };
        let pos = vec2(rb.translation().x, rb.translation().y);
        let vel = vec2(rb.linvel().x, rb.linvel().y);
        self.specialty_available = false;

        let mut explosions = Vec::new();

        match ent.specialty.as_str() {
            "SOUND" => {
                // Red Bird battle cry
                let bdef = gdata.block_for(&ent.definition);
                if let Some(e) = self.entities.get_mut(&rb_h) {
                    if !bdef.sprite_yell.is_empty() {
                        e.sprite = bdef.sprite_yell;
                    }
                }
                self.spawn_feathers(pos * PHYSICS_TO_WORLD, &ent.definition, 5);
                self.pending_sounds.push(("red_special".into(), 0.95));
            }
            "BOOST" => {
                // Yellow Chuck turbo dash
                let speed = vel.length().max(8.0);
                let dir = if vel.length_squared() > 0.01 {
                    vel.normalize()
                } else {
                    vec2(1.0, 0.0)
                };
                let boosted = dir * (speed * 1.75).clamp(48.0, 68.0);
                if let Some(rb_mut) = self.rigid_bodies.get_mut(rb_h) {
                    rb_mut.set_linvel(vector![boosted.x, boosted.y], true);
                }
                if let Some(e) = self.entities.get_mut(&rb_h) {
                    e.sprite = "BIRD_YELLOW_SPECIAL".into();
                }
                self.spawn_smoke_puff(pos * PHYSICS_TO_WORLD, 6);
                self.spawn_feathers(pos * PHYSICS_TO_WORLD, &ent.definition, 6);
                self.pending_sounds.push(("special_boost".into(), 0.95));
            }
            "CLUSTER_BOMB" => {
                // Blue Jay/Jake/Jim 3-way split
                let base_angle = vel.y.atan2(vel.x);
                let speed = vel.length().max(15.0);
                let bdef = gdata.block_for(&ent.definition);
                for &delta_deg in &[-15.0f32, 15.0f32] {
                    let ang = base_angle + delta_deg.to_radians();
                    let nvel = vec2(ang.cos() * speed, ang.sin() * speed);
                    let offset = vec2(ang.cos(), ang.sin()) * 0.65;
                    let child_rb = RigidBodyBuilder::dynamic()
                        .translation(vector![pos.x + offset.x, pos.y + offset.y])
                        .linvel(vector![nvel.x, nvel.y])
                        .linear_damping(0.05)
                        .angular_damping(0.25)
                        .ccd_enabled(true)
                        .build();
                    let ch_h = self.rigid_bodies.insert(child_rb);
                    let col = ColliderBuilder::ball(ent.radius)
                        .density(bdef.density.unwrap_or(4.5))
                        .friction(bdef.friction.unwrap_or(0.3))
                        .restitution(bdef.restitution.unwrap_or(0.25))
                        .active_events(
                            ActiveEvents::COLLISION_EVENTS | ActiveEvents::CONTACT_FORCE_EVENTS,
                        )
                        .contact_force_event_threshold(1.0)
                        .build();
                    self.colliders
                        .insert_with_parent(col, ch_h, &mut self.rigid_bodies);

                    let mut clone_ent = ent.clone();
                    clone_ent.sprite = "BIRD_BLUE_YELL".into();
                    self.entities.insert(ch_h, clone_ent);
                    self.flying_birds.push(ch_h);
                }
                if let Some(e) = self.entities.get_mut(&rb_h) {
                    e.sprite = "BIRD_BLUE_YELL".into();
                }
                self.spawn_smoke_puff(pos * PHYSICS_TO_WORLD, 8);
                self.spawn_feathers(pos * PHYSICS_TO_WORLD, &ent.definition, 6);
                self.pending_sounds.push(("special_group".into(), 0.95));
            }
            "BOMB" => {
                // Black Bomb instant detonation
                let bdef = gdata.block_for(&ent.definition);
                explosions.push(PendingExplosion {
                    x: pos.x,
                    y: pos.y,
                    force: bdef.explosion_force.max(35000.0),
                    radius: bdef.explosion_radius.max(14.0),
                    damage: bdef.explosion_damage.max(300.0),
                    damage_radius: bdef.explosion_damage_radius.max(6.5),
                    sound: "special_explosion".into(),
                    spawn_eggshells: false,
                });
                self.remove_body(rb_h);
            }
            "GRENADE" => {
                // White Matilda drops explosive EggGranade + recoils up-right!
                let edef = gdata.block_for("EggGranade");
                let egg_r = edef.radius.unwrap_or(1.1);
                let egg_rb = RigidBodyBuilder::dynamic()
                    .translation(vector![pos.x, pos.y + ent.radius + 0.6])
                    .linvel(vector![vel.x * 0.15, 18.0])
                    .ccd_enabled(true)
                    .build();
                let egg_h = self.rigid_bodies.insert(egg_rb);
                let egg_col = ColliderBuilder::ball(egg_r)
                    .density(edef.density.unwrap_or(3.0))
                    .friction(0.3)
                    .restitution(0.1)
                    .active_events(
                        ActiveEvents::COLLISION_EVENTS | ActiveEvents::CONTACT_FORCE_EVENTS,
                    )
                    .contact_force_event_threshold(0.5)
                    .build();
                self.colliders
                    .insert_with_parent(egg_col, egg_h, &mut self.rigid_bodies);

                self.entities.insert(
                    egg_h,
                    Entity {
                        name: "EggGranade".into(),
                        definition: "EggGranade".into(),
                        material: "red".into(),
                        sprite: "DROPPABLE_EGG".into(),
                        base_sprite: "DROPPABLE_EGG".into(),
                        blink_sprite: String::new(),
                        smile_sprite: String::new(),
                        is_bird: true,
                        is_pig: false,
                        is_static_prop: false,
                        max_health: 5.0,
                        health: 5.0,
                        defence: 1.0,
                        radius: egg_r,
                        half_w: egg_r,
                        half_h: egg_r,
                        specialty: "EGG_BOMB".into(),
                        has_collided: false,
                        bomb_fuse: None,
                        blink_timer: 99.0,
                        blinking: false,
                        smiling: false,
                        oink_timer: 99.0,
                    },
                );
                self.flying_birds.push(egg_h);
                self.primary_flying_bird = Some(egg_h); // Follow the egg drop!

                // Recoil Matilda sharply upward & forward
                if let Some(rb_mut) = self.rigid_bodies.get_mut(rb_h) {
                    rb_mut.set_linvel(vector![26.0, -32.0], true);
                }
                if let Some(e) = self.entities.get_mut(&rb_h) {
                    e.sprite = "BIRD_GREEN_SPECIAL".into();
                    e.has_collided = true;
                }
                self.spawn_feathers(pos * PHYSICS_TO_WORLD, &ent.definition, 6);
                self.pending_sounds.push(("special_egg".into(), 0.95));
            }
            _ => {}
        }

        for exp in explosions {
            self.execute_explosion(exp, gdata);
        }
    }

    pub fn step(&mut self, dt: f32, gdata: &GameData) {
        let clamped_dt = dt.min(1.0 / 30.0);
        self.integration_parameters.dt = 1.0 / 60.0;

        if self.warmup_timer > 0.0 {
            self.warmup_timer = (self.warmup_timer - clamped_dt).max(0.0);
        }
        if self.camera_shake > 0.0 {
            self.camera_shake = (self.camera_shake - clamped_dt * 28.0).max(0.0);
        }

        // Animate next queued bird hopping into the slingshot
        if self.active_slingshot_bird.is_none()
            && self.flying_birds.is_empty()
            && self.load_bird_anim.is_none()
            && !self.bird_queue.is_empty()
            && self.remaining_pigs() > 0
        {
            let next_bird = self.bird_queue.remove(0);
            let start = vec2(next_bird.ground_x, next_bird.ground_y);
            self.load_bird_anim = Some((next_bird, start, 0.0));
            self.pending_sounds.push(("bird_next".into(), 0.75));
        }

        if let Some((mut qb, start_p, mut t)) = self.load_bird_anim.take() {
            t += clamped_dt * 2.4;
            if t >= 1.0 {
                qb.hop_offset = 0.0;
                self.active_slingshot_bird = Some(qb);
            } else {
                self.load_bird_anim = Some((qb, start_p, t));
            }
        }

        // Idle hop animation for waiting birds
        for qb in &mut self.bird_queue {
            qb.hop_timer -= clamped_dt;
            if qb.hop_timer <= 0.0 {
                qb.hop_timer = 2.0 + (macroquad::rand::rand() % 250) as f32 * 0.01;
            }
            qb.hop_offset = if qb.hop_timer < 0.25 {
                -(qb.hop_timer / 0.25 * std::f32::consts::PI).sin() * 10.0
            } else {
                0.0
            };
        }

        // Record smoke trail dots while primary bird is flying before collision
        if let Some(fb_h) = self.primary_flying_bird {
            let uncollided = self
                .entities
                .get(&fb_h)
                .map(|e| !e.has_collided)
                .unwrap_or(false);
            if uncollided {
                self.trail_timer += clamped_dt;
                if self.trail_timer >= 0.032 {
                    self.trail_timer = 0.0;
                    if let Some(rb) = self.rigid_bodies.get(fb_h) {
                        let idx = (self.cur_trail.len() % 3) + 1;
                        self.cur_trail.push(TrailDot {
                            x: rb.translation().x * PHYSICS_TO_WORLD,
                            y: rb.translation().y * PHYSICS_TO_WORLD,
                            sprite: format!("SMOKE_TRAIL_{}", idx),
                            scale: if idx == 1 { 0.85 } else { 1.0 },
                        });
                    }
                }
            }
        }

        // Snapshot pre-step velocities so true impact speed is known even after inelastic stop
        let mut pre_vel: HashMap<RigidBodyHandle, Vec2> =
            HashMap::with_capacity(self.entities.len());
        let mut body_mass: HashMap<RigidBodyHandle, f32> =
            HashMap::with_capacity(self.entities.len());
        for &h in self.entities.keys() {
            if let Some(rb) = self.rigid_bodies.get(h) {
                pre_vel.insert(h, vec2(rb.linvel().x, rb.linvel().y));
                body_mass.insert(h, rb.mass().max(1.0));
            }
        }

        // Step Rapier2D with contact event collection
        let (collision_send, collision_recv) = rapier2d::crossbeam::channel::unbounded();
        let (force_send, force_recv) = rapier2d::crossbeam::channel::unbounded();
        let event_handler = ChannelEventCollector::new(collision_send, force_send);
        let gravity = vector![0.0, GRAVITY_Y];

        self.physics_pipeline.step(
            &gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &event_handler,
        );

        let mut pending_explosions: Vec<PendingExplosion> = Vec::new();
        let mut to_destroy: HashSet<RigidBodyHandle> = HashSet::new();

        // Handle collision start for EggGranade & bird has_collided flags
        while let Ok(col_ev) = collision_recv.try_recv() {
            if let CollisionEvent::Started(c1, c2, _) = col_ev {
                let b1 = self.colliders.get(c1).and_then(|c| c.parent());
                let b2 = self.colliders.get(c2).and_then(|c| c.parent());
                for bh in [b1, b2].into_iter().flatten() {
                    if let Some(ent) = self.entities.get_mut(&bh) {
                        if ent.is_bird && !ent.has_collided {
                            ent.has_collided = true;
                            let bdef = gdata.block_for(&ent.definition);
                            if !bdef.sprite_collision.is_empty() && ent.specialty != "BOMB" {
                                ent.sprite = bdef.sprite_collision.clone();
                            }
                            if ent.specialty == "BOMB" && ent.bomb_fuse.is_none() {
                                ent.bomb_fuse = Some(1.15);
                            } else if ent.specialty == "EGG_BOMB" {
                                if let Some(rb) = self.rigid_bodies.get(bh) {
                                    let edef = gdata.block_for("EggGranade");
                                    pending_explosions.push(PendingExplosion {
                                        x: rb.translation().x,
                                        y: rb.translation().y,
                                        force: edef.explosion_force.max(15000.0),
                                        radius: edef.explosion_radius.max(10.0),
                                        damage: edef.explosion_damage.max(400.0),
                                        damage_radius: edef.explosion_damage_radius.max(8.0),
                                        sound: "special_egg_explosion".into(),
                                        spawn_eggshells: true,
                                    });
                                    to_destroy.insert(bh);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Process dynamic impact impulses once the player has launched at least one bird
        if self.warmup_timer <= 0.0 && self.birds_fired > 0 {
            let step_dt = self.integration_parameters.dt;
            while let Ok(fev) = force_recv.try_recv() {
                let b1 = self.colliders.get(fev.collider1).and_then(|c| c.parent());
                let b2 = self.colliders.get(fev.collider2).and_then(|c| c.parent());

                let v1 = b1
                    .and_then(|h| pre_vel.get(&h).copied())
                    .unwrap_or(Vec2::ZERO);
                let v2 = b2
                    .and_then(|h| pre_vel.get(&h).copied())
                    .unwrap_or(Vec2::ZERO);
                let rel_speed = (v1 - v2).length();

                // Ignore static resting contacts where relative approach speed is negligible
                if rel_speed < 2.1 {
                    continue;
                }

                let total_impulse = fev.total_force_magnitude * step_dt;
                let e1_snapshot = b1.and_then(|h| self.entities.get(&h).cloned());
                let e2_snapshot = b2.and_then(|h| self.entities.get(&h).cloned());

                // Apply damage to b1 and b2
                for (target_h_opt, other_h_opt, other_opt) in [
                    (b1, b2, e2_snapshot.as_ref()),
                    (b2, b1, e1_snapshot.as_ref()),
                ] {
                    let Some(th) = target_h_opt else {
                        continue;
                    };
                    if to_destroy.contains(&th) {
                        continue;
                    }
                    let Some(target) = self.entities.get(&th).cloned() else {
                        continue;
                    };
                    let m_self = body_mass.get(&th).copied().unwrap_or(2.0);
                    let weight_impulse = m_self * GRAVITY_Y * step_dt * 1.5;
                    let impact_impulse = (total_impulse - weight_impulse).max(0.0);
                    if impact_impulse < 3.0 {
                        continue;
                    }

                    if target.is_static_prop || target.is_bird {
                        if target.is_bird && impact_impulse > 10.0 && rel_speed > 3.5 {
                            let bdef = gdata.block_for(&target.definition);
                            if !bdef.collision_sound.is_empty() {
                                self.queue_sound_throttled(&bdef.collision_sound, 0.7);
                            }
                        }
                        continue;
                    }

                    // Rovio colorDamageFactor = 4.0 when bird specialty matches block material
                    let mut affinity = 1.0f32;
                    if let Some(other) = other_opt {
                        if other.is_bird {
                            if (other.definition == "SmallBlueBird" && target.material == "light")
                                || (other.definition == "YellowBird" && target.material == "wood")
                                || (other.definition == "BlackBird" && target.material == "rock")
                            {
                                affinity = 4.0;
                            } else {
                                affinity = 1.45;
                            }
                        }
                    }

                    let raw_dmg = (impact_impulse * 0.62 * affinity) / target.defence;
                    if raw_dmg < 1.0 {
                        continue;
                    }

                    let mdef = gdata.material_for(&target.material);
                    if impact_impulse > 8.0 && rel_speed > 3.0 && !mdef.collision_sound.is_empty() {
                        self.queue_sound_throttled(&mdef.collision_sound, 0.5);
                    }

                    let new_hp = target.health - raw_dmg;
                    if new_hp <= 0.0 {
                        to_destroy.insert(th);
                        // Overkill momentum carry-through so birds slice cleanly through shattered glass/wood
                        if let Some(oh) = other_h_opt {
                            if let (Some(&pv), Some(orb)) =
                                (pre_vel.get(&oh), self.rigid_bodies.get_mut(oh))
                            {
                                let frac_used = (target.health / raw_dmg).clamp(0.10, 0.90);
                                let post_v = vec2(orb.linvel().x, orb.linvel().y);
                                let carried = pv.lerp(post_v, frac_used * 0.65);
                                orb.set_linvel(vector![carried.x, carried.y], true);
                            }
                        }
                    } else if let Some(ent_mut) = self.entities.get_mut(&th) {
                        ent_mut.health = new_hp;
                        let pct = (new_hp / ent_mut.max_health) * 100.0;
                        let (nsp, nblink, nsmile) = gdata.select_damage_sprites(
                            &ent_mut.definition,
                            pct,
                            &ent_mut.base_sprite,
                        );
                        if nsp != ent_mut.sprite {
                            ent_mut.sprite = nsp;
                            ent_mut.blink_sprite = nblink;
                            ent_mut.smile_sprite = nsmile;
                            if !mdef.damage_sound.is_empty() {
                                self.queue_sound_throttled(&mdef.damage_sound, 0.55);
                            }
                        }
                        // Award minor impact score when a bird causes direct structural damage
                        if other_opt.map(|o| o.is_bird).unwrap_or(false) && raw_dmg >= 3.0 {
                            let bonus = ((raw_dmg * 8.0) as u32).clamp(10, 250);
                            self.score += bonus;
                        }
                    }
                }
            }
        }

        // Update Black Bird bomb fuses, pig blinking/oinking, and out-of-bounds checks
        let handles: Vec<RigidBodyHandle> = self.entities.keys().copied().collect();
        for h in handles {
            let Some(rb) = self.rigid_bodies.get(h) else {
                continue;
            };
            let px = rb.translation().x;
            let py = rb.translation().y;

            if px < self.left_limit || px > self.right_limit || py > 18.0 {
                to_destroy.insert(h);
                continue;
            }

            if let Some(ent) = self.entities.get_mut(&h) {
                if let Some(mut fuse) = ent.bomb_fuse {
                    fuse -= clamped_dt;
                    if fuse > 0.75 {
                        ent.sprite = "BIRD_GREY_1".into();
                    } else if fuse > 0.35 {
                        ent.sprite = "BIRD_GREY_2".into();
                    } else {
                        ent.sprite = "BIRD_GREY_3".into();
                    }
                    if fuse <= 0.0 {
                        let bdef = gdata.block_for(&ent.definition);
                        pending_explosions.push(PendingExplosion {
                            x: px,
                            y: py,
                            force: bdef.explosion_force.max(35000.0),
                            radius: bdef.explosion_radius.max(14.0),
                            damage: bdef.explosion_damage.max(300.0),
                            damage_radius: bdef.explosion_damage_radius.max(6.5),
                            sound: "special_explosion".into(),
                            spawn_eggshells: false,
                        });
                        to_destroy.insert(h);
                    } else {
                        ent.bomb_fuse = Some(fuse);
                    }
                }

                // Pig blinking and occasional oinks
                if ent.is_pig {
                    ent.blink_timer -= clamped_dt;
                    if ent.blink_timer <= 0.0 {
                        if ent.blinking {
                            ent.blinking = false;
                            ent.blink_timer = 2.0 + (macroquad::rand::rand() % 300) as f32 * 0.01;
                        } else {
                            ent.blinking = true;
                            ent.blink_timer = 0.18;
                        }
                    }
                    ent.oink_timer -= clamped_dt;
                    if ent.oink_timer <= 0.0 {
                        ent.oink_timer = 5.0 + (macroquad::rand::rand() % 600) as f32 * 0.01;
                        self.pending_sounds.push(("piglette_oink".into(), 0.45));
                    }
                }
            }
        }

        // Destroy dead blocks/pigs/TNT
        for h in to_destroy {
            if let Some(ent) = self.entities.get(&h).cloned() {
                if let Some(rb) = self.rigid_bodies.get(h) {
                    let wx = rb.translation().x * PHYSICS_TO_WORLD;
                    let wy = rb.translation().y * PHYSICS_TO_WORLD;
                    if ent.definition == "ExtraBlockTNT" {
                        let bdef = gdata.block_for("ExtraBlockTNT");
                        pending_explosions.push(PendingExplosion {
                            x: rb.translation().x,
                            y: rb.translation().y,
                            force: bdef.explosion_force.max(30000.0),
                            radius: bdef.explosion_radius.max(10.0),
                            damage: bdef.explosion_damage.max(180.0),
                            damage_radius: bdef.explosion_damage_radius.max(5.5),
                            sound: "special_explosion".into(),
                            spawn_eggshells: false,
                        });
                    } else if ent.is_pig {
                        self.score += 5000;
                        self.floating_scores.push(FloatingScore {
                            x: wx,
                            y: wy - 16.0,
                            sprite: Some("5K_GREEN".into()),
                            text: "5000".into(),
                            color: GREEN,
                            age: 0.0,
                            lifetime: 1.15,
                        });
                        self.spawn_smoke_puff(vec2(wx, wy), 10);
                        self.pending_sounds
                            .push(("piglette_destroyed".into(), 0.85));
                    } else if !ent.is_bird && !ent.is_static_prop {
                        self.score += 500;
                        self.floating_scores.push(FloatingScore {
                            x: wx,
                            y: wy - 10.0,
                            sprite: None,
                            text: "500".into(),
                            color: Color::from_rgba(255, 240, 150, 255),
                            age: 0.0,
                            lifetime: 0.75,
                        });
                        self.spawn_debris(vec2(wx, wy), &ent.material);
                        let mdef = gdata.material_for(&ent.material);
                        if !mdef.destroyed_sound.is_empty() {
                            self.queue_sound_throttled(&mdef.destroyed_sound, 0.7);
                        }
                    }
                }
            }
            self.remove_body(h);
        }

        // Execute queued explosions (including chain-reaction TNTs!)
        let mut depth = 0;
        while !pending_explosions.is_empty() && depth < 6 {
            depth += 1;
            let batch = std::mem::take(&mut pending_explosions);
            for exp in batch {
                let extra = self.execute_explosion(exp, gdata);
                pending_explosions.extend(extra);
            }
        }

        // Check if flying birds have come to rest or timed out so next bird can load
        if !self.flying_birds.is_empty() {
            self.post_shot_timer += clamped_dt;
            let mut all_birds_slow = true;
            let mut stopped = Vec::new();

            for &bh in &self.flying_birds {
                if let Some(rb) = self.rigid_bodies.get(bh) {
                    let spd = rb.linvel().norm();
                    let has_fuse = self.entities.get(&bh).and_then(|e| e.bomb_fuse).is_some();
                    if spd > 0.75 || has_fuse {
                        all_birds_slow = false;
                    }
                    if (spd < 0.35 && self.post_shot_timer > 1.8 && !has_fuse)
                        || self.post_shot_timer > 7.5
                    {
                        stopped.push(bh);
                    }
                }
            }

            if all_birds_slow {
                self.settle_timer += clamped_dt;
            } else {
                self.settle_timer = 0.0;
            }

            if self.settle_timer > 0.9 || self.post_shot_timer > 7.5 {
                for bh in stopped {
                    if let Some(rb) = self.rigid_bodies.get(bh) {
                        let wx = rb.translation().x * PHYSICS_TO_WORLD;
                        let wy = rb.translation().y * PHYSICS_TO_WORLD;
                        if let Some(def_name) = self.entities.get(&bh).map(|e| e.definition.clone())
                        {
                            self.spawn_feathers(vec2(wx, wy), &def_name, 5);
                        }
                        self.spawn_smoke_puff(vec2(wx, wy), 5);
                    }
                    self.remove_body(bh);
                }
                self.flying_birds.retain(|h| self.entities.contains_key(h));
                if self.flying_birds.is_empty() {
                    self.primary_flying_bird = None;
                    self.specialty_available = false;
                }
            }
        }

        // Update particles & floating scores
        for p in &mut self.particles {
            p.age += clamped_dt;
            p.vy += p.gravity * clamped_dt;
            p.x += p.vx * clamped_dt;
            p.y += p.vy * clamped_dt;
            p.rot += p.vrot * clamped_dt;
            p.scale = (p.scale + p.grow * clamped_dt).max(0.05);
        }
        self.particles.retain(|p| p.age < p.lifetime);

        for fs in &mut self.floating_scores {
            fs.age += clamped_dt;
            fs.y -= 28.0 * clamped_dt;
        }
        self.floating_scores.retain(|fs| fs.age < fs.lifetime);
    }

    pub fn is_world_settled(&self) -> bool {
        if !self.flying_birds.is_empty() {
            return false;
        }
        for (&h, ent) in &self.entities {
            if ent.is_static_prop {
                continue;
            }
            if let Some(rb) = self.rigid_bodies.get(h) {
                if rb.linvel().norm() > 0.65 {
                    return false;
                }
            }
        }
        true
    }

    /// Awards 10,000 bonus points per unused bird when level is cleared!
    pub fn award_unused_bird_bonuses(&mut self, gdata: &GameData) {
        if self.bonus_awarded {
            return;
        }
        self.bonus_awarded = true;

        let mut remaining: Vec<(String, Vec2)> = Vec::new();
        if let Some(sb) = &self.active_slingshot_bird {
            remaining.push((sb.definition.clone(), self.slingshot_pos * PHYSICS_TO_WORLD));
        }
        if let Some((lb, sp, _)) = &self.load_bird_anim {
            remaining.push((lb.definition.clone(), *sp * PHYSICS_TO_WORLD));
        }
        for qb in &self.bird_queue {
            remaining.push((
                qb.definition.clone(),
                vec2(
                    qb.ground_x * PHYSICS_TO_WORLD,
                    qb.ground_y * PHYSICS_TO_WORLD,
                ),
            ));
        }

        for (i, (def_name, wpos)) in remaining.into_iter().enumerate() {
            self.score += 10_000;
            let bdef = gdata.block_for(&def_name);
            let score_sprite = if !bdef.sprite_score.is_empty() {
                Some(bdef.sprite_score)
            } else {
                Some("10K_RED".into())
            };
            self.floating_scores.push(FloatingScore {
                x: wpos.x,
                y: wpos.y - 25.0 - (i as f32) * 18.0,
                sprite: score_sprite,
                text: "10000".into(),
                color: YELLOW,
                age: 0.0,
                lifetime: 1.6,
            });
        }
    }

    pub fn earned_stars(&self) -> u8 {
        if self.remaining_pigs() > 0 {
            return 0;
        }
        if self.score >= self.gold_score {
            3
        } else if self.score >= self.silver_score {
            2
        } else {
            1
        }
    }

    fn execute_explosion(
        &mut self,
        exp: PendingExplosion,
        gdata: &GameData,
    ) -> Vec<PendingExplosion> {
        let mut chain = Vec::new();
        let center_w = vec2(exp.x * PHYSICS_TO_WORLD, exp.y * PHYSICS_TO_WORLD);

        self.camera_shake = 14.0;
        self.pending_sounds.push((exp.sound, 1.0));

        // Central flash + expanding smoke rings
        self.particles.push(Particle {
            sprite: "EXPLOSION".into(),
            x: center_w.x,
            y: center_w.y,
            vx: 0.0,
            vy: 0.0,
            rot: 0.0,
            vrot: 2.0,
            scale: 0.6,
            grow: 3.5,
            age: 0.0,
            lifetime: 0.35,
            gravity: 0.0,
        });
        for i in 0..10 {
            let ang = (i as f32 / 10.0) * std::f32::consts::TAU;
            let spd = 65.0 + (macroquad::rand::rand() % 90) as f32;
            let cloud_idx = (i % 6) + 1;
            self.particles.push(Particle {
                sprite: format!("EXPLOSION_CLOUD_{}", cloud_idx),
                x: center_w.x + ang.cos() * 8.0,
                y: center_w.y + ang.sin() * 8.0,
                vx: ang.cos() * spd,
                vy: ang.sin() * spd - 20.0,
                rot: ang,
                vrot: 1.2,
                scale: 0.55,
                grow: 0.85,
                age: 0.0,
                lifetime: 0.65,
                gravity: -15.0,
            });
        }
        if exp.spawn_eggshells {
            for i in 0..6 {
                let ang = -std::f32::consts::PI * (0.15 + 0.7 * (i as f32 / 5.0));
                self.particles.push(Particle {
                    sprite: format!("EGG_SHELL_{}", (i % 3) + 1),
                    x: center_w.x,
                    y: center_w.y,
                    vx: ang.cos() * 140.0,
                    vy: ang.sin() * 160.0,
                    rot: 0.0,
                    vrot: 8.0,
                    scale: 1.0,
                    grow: 0.0,
                    age: 0.0,
                    lifetime: 0.9,
                    gravity: 340.0,
                });
            }
        }

        let handles: Vec<RigidBodyHandle> = self.entities.keys().copied().collect();
        let mut killed = Vec::new();

        for h in handles {
            let Some(rb) = self.rigid_bodies.get_mut(h) else {
                continue;
            };
            let dx = rb.translation().x - exp.x;
            let dy = rb.translation().y - exp.y;
            let dist = (dx * dx + dy * dy).sqrt().max(0.25);

            if dist <= exp.radius && rb.is_dynamic() {
                let falloff = (1.0 - dist / exp.radius).clamp(0.0, 1.0);
                let impulse_mag = exp.force * PHYSICS_SCALE * 0.045 * falloff;
                let nx = dx / dist;
                let ny = (dy / dist) - 0.25; // slight upward lift
                rb.apply_impulse(vector![nx * impulse_mag, ny * impulse_mag], true);
            }

            if dist <= exp.damage_radius {
                if let Some(ent) = self.entities.get_mut(&h) {
                    if !ent.is_static_prop && !ent.is_bird {
                        let dmg_falloff = (1.0 - (dist / exp.damage_radius) * 0.65).clamp(0.2, 1.0);
                        let dmg = (exp.damage * dmg_falloff) / ent.defence.min(6.0);
                        ent.health -= dmg;
                        if ent.health <= 0.0 {
                            killed.push(h);
                        } else {
                            let pct = (ent.health / ent.max_health) * 100.0;
                            let (nsp, nblink, nsmile) =
                                gdata.select_damage_sprites(&ent.definition, pct, &ent.base_sprite);
                            ent.sprite = nsp;
                            ent.blink_sprite = nblink;
                            ent.smile_sprite = nsmile;
                        }
                    }
                }
            }
        }

        for kh in killed {
            if let Some(ent) = self.entities.get(&kh).cloned() {
                if let Some(rb) = self.rigid_bodies.get(kh) {
                    let wx = rb.translation().x * PHYSICS_TO_WORLD;
                    let wy = rb.translation().y * PHYSICS_TO_WORLD;
                    if ent.definition == "ExtraBlockTNT" {
                        let bdef = gdata.block_for("ExtraBlockTNT");
                        chain.push(PendingExplosion {
                            x: rb.translation().x,
                            y: rb.translation().y,
                            force: bdef.explosion_force.max(30000.0),
                            radius: bdef.explosion_radius.max(10.0),
                            damage: bdef.explosion_damage.max(180.0),
                            damage_radius: bdef.explosion_damage_radius.max(5.5),
                            sound: "special_explosion".into(),
                            spawn_eggshells: false,
                        });
                    } else if ent.is_pig {
                        self.score += 5000;
                        self.floating_scores.push(FloatingScore {
                            x: wx,
                            y: wy - 16.0,
                            sprite: Some("5K_GREEN".into()),
                            text: "5000".into(),
                            color: GREEN,
                            age: 0.0,
                            lifetime: 1.15,
                        });
                        self.spawn_smoke_puff(vec2(wx, wy), 8);
                        self.pending_sounds
                            .push(("piglette_destroyed".into(), 0.85));
                    } else {
                        self.score += 500;
                        self.spawn_debris(vec2(wx, wy), &ent.material);
                    }
                }
            }
            self.remove_body(kh);
        }

        chain
    }

    fn remove_body(&mut self, h: RigidBodyHandle) {
        self.entities.remove(&h);
        self.flying_birds.retain(|&x| x != h);
        if self.primary_flying_bird == Some(h) {
            self.primary_flying_bird = self.flying_birds.first().copied();
        }
        self.joint_visuals
            .retain(|&(h1, _, h2, _)| h1 != h && h2 != h);
        self.rigid_bodies.remove(
            h,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    fn queue_sound_throttled(&mut self, name: &str, vol: f32) {
        if self.pending_sounds.len() < 6 && !self.pending_sounds.iter().any(|(n, _)| n == name) {
            self.pending_sounds.push((name.to_string(), vol));
        }
    }

    fn spawn_smoke_puff(&mut self, wpos: Vec2, count: usize) {
        for i in 0..count {
            let ang = (i as f32 / count.max(1) as f32) * std::f32::consts::TAU;
            let spd = 25.0 + (macroquad::rand::rand() % 55) as f32;
            let idx = (i % 6) + 1;
            self.particles.push(Particle {
                sprite: format!("SMOKE_CLOUD_{}", idx),
                x: wpos.x,
                y: wpos.y,
                vx: ang.cos() * spd,
                vy: ang.sin() * spd - 12.0,
                rot: ang,
                vrot: 1.5,
                scale: 0.45,
                grow: 0.55,
                age: 0.0,
                lifetime: 0.55,
                gravity: -10.0,
            });
        }
    }

    fn spawn_feathers(&mut self, wpos: Vec2, def_name: &str, count: usize) {
        let prefix = match def_name {
            "SmallBlueBird" => "BLUE_FEATHER",
            "YellowBird" => "YELLOW_FEATHER",
            "BlackBird" => "BLACK_FEATHER",
            "BasicBird2" => "GREEN_FEATHER",
            _ => "RED_FEATHER",
        };
        for i in 0..count {
            let ang = (macroquad::rand::rand() % 360) as f32 * std::f32::consts::PI / 180.0;
            let spd = 40.0 + (macroquad::rand::rand() % 80) as f32;
            self.particles.push(Particle {
                sprite: format!("{}_{}", prefix, (i % 3) + 1),
                x: wpos.x,
                y: wpos.y,
                vx: ang.cos() * spd,
                vy: ang.sin() * spd - 35.0,
                rot: ang,
                vrot: 5.5,
                scale: 0.9,
                grow: 0.0,
                age: 0.0,
                lifetime: 0.85,
                gravity: 160.0,
            });
        }
    }

    fn spawn_debris(&mut self, wpos: Vec2, material: &str) {
        let (prefix, variants) = match material {
            "light" => ("LIGHT_PARTICLE", 5),
            "rock" => ("ROCK_PARTICLE", 3),
            _ => ("WOOD_PARTICLE", 3),
        };
        for i in 0..6 {
            let ang = (macroquad::rand::rand() % 360) as f32 * std::f32::consts::PI / 180.0;
            let spd = 55.0 + (macroquad::rand::rand() % 110) as f32;
            self.particles.push(Particle {
                sprite: format!("{}_{}", prefix, (i % variants) + 1),
                x: wpos.x,
                y: wpos.y,
                vx: ang.cos() * spd,
                vy: ang.sin() * spd - 65.0,
                rot: ang,
                vrot: 7.0,
                scale: 1.0,
                grow: -0.25,
                age: 0.0,
                lifetime: 0.75,
                gravity: 380.0,
            });
        }
    }
}
