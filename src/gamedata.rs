#![allow(dead_code)]
//! Typed deserialization and lookup helpers for Angry Birds 1.2.0 game definitions,
//! materials, 5 themes, star thresholds, and all 105 levels (5 chapters x 21).

use serde::Deserialize;
use std::collections::HashMap;

pub const PHYSICS_TO_WORLD: f32 = 20.0;
pub const PHYSICS_SCALE: f32 = 1.0 / PHYSICS_TO_WORLD;

#[derive(Debug, Clone, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeDef {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub image: String,
    pub color: Option<RgbColor>,
    #[serde(rename = "groundColor")]
    pub ground_color: Option<RgbColor>,
    /// `[[texture, sprite, parallax, ...], ...]` from `blocks.lua` -> `themes.themeN.bgLayers`
    #[serde(default, rename = "bgLayers")]
    pub bg_layers: Vec<ThemeBgLayer>,
    /// `[[texture, sprite], ...]` -> `themes.themeN.fgLayers`
    #[serde(default, rename = "fgLayers")]
    pub fg_layers: Vec<ThemeFgLayer>,
}

/// `[texture, sprite, parallax, x]` — 1.2.0 `bgLayers` entry (extra fields tolerated).
#[derive(Debug, Clone, Default)]
pub struct ThemeBgLayer {
    pub texture: String,
    pub sprite: String,
    pub parallax: f32,
}

impl<'de> Deserialize<'de> for ThemeBgLayer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
        let get_str = |i: usize| {
            raw.get(i)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let parallax = raw
            .get(2)
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.5);
        Ok(ThemeBgLayer {
            texture: get_str(0),
            sprite: get_str(1),
            parallax,
        })
    }
}

/// `[texture, sprite]` — 1.2.0 `fgLayers` entry.
#[derive(Debug, Clone, Default)]
pub struct ThemeFgLayer {
    pub texture: String,
    pub sprite: String,
}

impl<'de> Deserialize<'de> for ThemeFgLayer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw: Vec<serde_json::Value> = Vec::deserialize(d)?;
        let get_str = |i: usize| {
            raw.get(i)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        Ok(ThemeFgLayer {
            texture: get_str(0),
            sprite: get_str(1),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MaterialDef {
    #[serde(default)]
    pub density: f32,
    #[serde(default)]
    pub friction: f32,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default)]
    pub strength: f32,
    #[serde(default)]
    pub defence: f32,
    #[serde(default, rename = "collisionSound")]
    pub collision_sound: String,
    #[serde(default, rename = "damageSound")]
    pub damage_sound: String,
    #[serde(default, rename = "destroyedSound")]
    pub destroyed_sound: String,
    #[serde(default)]
    pub particles: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DamageSpriteTier {
    #[serde(default)]
    pub min: f32,
    #[serde(default)]
    pub max: f32,
    #[serde(default)]
    pub sprite: String,
    #[serde(default, rename = "blinkSprite")]
    pub blink_sprite: String,
    #[serde(default, rename = "smileSprite")]
    pub smile_sprite: String,
}

/// One normalised vertex (0..1 inside the object's width/height box, y measured downward)
/// from `blocks.lua`, used by the triangle/polygon blocks (WoodBlock11/12 and friends).
#[derive(Clone, Copy, Debug, Deserialize, Default)]
pub struct PolyVertex {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BlockDef {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub material: String,
    #[serde(default, rename = "type")]
    pub shape_type: String,
    #[serde(default)]
    pub sprite: String,
    #[serde(default, rename = "spriteBlink")]
    pub sprite_blink: String,
    #[serde(default, rename = "spriteFlying")]
    pub sprite_flying: String,
    #[serde(default, rename = "spriteCollision")]
    pub sprite_collision: String,
    #[serde(default, rename = "spriteYell")]
    pub sprite_yell: String,
    #[serde(default, rename = "spriteScore")]
    pub sprite_score: String,
    #[serde(default, rename = "spriteAction")]
    pub sprite_action: String,
    /// Tileable fill texture for ground/static terrain blocks (e.g. `INGAME_THEME_GROUND_1`)
    #[serde(default)]
    pub texture: String,
    /// Normalised outline for `type = "polygon"` blocks (triangles)
    #[serde(default)]
    pub vertices: Vec<PolyVertex>,
    #[serde(default)]
    pub specialty: String,
    #[serde(default, rename = "tutorialInfo")]
    pub tutorial_info: String,
    #[serde(default)]
    pub controllable: bool,
    #[serde(default, rename = "levelGoal")]
    pub level_goal: bool,
    pub collision: Option<bool>,
    pub density: Option<f32>,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    pub strength: Option<f32>,
    pub defence: Option<f32>,
    pub radius: Option<f32>,
    #[serde(default, rename = "selectionSound")]
    pub selection_sound: String,
    #[serde(default, rename = "launchSound")]
    pub launch_sound: String,
    #[serde(default, rename = "collisionSound")]
    pub collision_sound: String,
    #[serde(default)]
    pub particles: String,
    #[serde(default, rename = "particlesTrail")]
    pub particles_trail: String,
    #[serde(default, rename = "explosionDamage")]
    pub explosion_damage: f32,
    #[serde(default, rename = "explosionForce")]
    pub explosion_force: f32,
    #[serde(default, rename = "explosionRadius")]
    pub explosion_radius: f32,
    #[serde(default, rename = "explosionDamageRadius")]
    pub explosion_damage_radius: f32,
    #[serde(default, rename = "damageSprites")]
    pub damage_sprites: HashMap<String, DamageSpriteTier>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CameraPreset {
    #[serde(default)]
    pub px: f32,
    #[serde(default)]
    pub py: f32,
    #[serde(default)]
    pub sx: f32,
    #[serde(default)]
    pub sy: f32,
    #[serde(default)]
    pub left: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub top: f32,
    #[serde(default)]
    pub bottom: f32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StarThresholds {
    #[serde(default, rename = "silverScore")]
    pub silver_score: u32,
    #[serde(default, rename = "goldScore")]
    pub gold_score: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelObject {
    pub name: String,
    pub definition: String,
    pub sprite: String,
    #[serde(default, rename = "damageSprite")]
    pub damage_sprite: String,
    #[serde(default, rename = "blinkSprite")]
    pub blink_sprite: String,
    #[serde(default, rename = "smileSprite")]
    pub smile_sprite: String,
    pub shape_type: String,
    pub x: f32,
    pub y: f32,
    pub angle: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub density: Option<f32>,
    pub friction: Option<f32>,
    pub restitution: Option<f32>,
    pub strength: Option<f32>,
    pub defence: Option<f32>,
    pub controllable: bool,
    #[serde(rename = "levelGoal")]
    pub level_goal: bool,
    #[serde(rename = "startNumber")]
    pub start_number: usize,
    pub material: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelJoint {
    pub name: String,
    pub joint_type: u32,
    pub end1: String,
    pub end2: String,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelDef {
    pub index: usize,
    pub chapter: usize,
    pub stage: usize,
    pub id: String,
    pub theme: String,
    pub stars: StarThresholds,
    #[serde(default, rename = "birdCamera")]
    pub bird_camera: CameraPreset,
    #[serde(default, rename = "castleCamera")]
    pub castle_camera: CameraPreset,
    pub objects: Vec<LevelObject>,
    pub joints: Vec<LevelJoint>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawGameData {
    materials: HashMap<String, MaterialDef>,
    themes: HashMap<String, serde_json::Value>,
    blocks: HashMap<String, BlockDef>,
    levels: Vec<LevelDef>,
}

pub struct GameData {
    pub materials: HashMap<String, MaterialDef>,
    pub themes: HashMap<String, ThemeDef>,
    pub blocks: HashMap<String, BlockDef>,
    pub levels: Vec<LevelDef>,
}

impl GameData {
    pub fn load_embedded() -> Self {
        let json_str = include_str!("../assets/gamedata.json");
        let raw: RawGameData =
            serde_json::from_str(json_str).expect("Valid embedded gamedata.json");
        let mut themes = HashMap::new();
        for (k, v) in raw.themes {
            if k.starts_with("theme") {
                if let Ok(td) = serde_json::from_value::<ThemeDef>(v) {
                    themes.insert(k, td);
                }
            }
        }
        Self {
            materials: raw.materials,
            themes,
            blocks: raw.blocks,
            levels: raw.levels,
        }
    }

    pub fn material_for(&self, mat_name: &str) -> MaterialDef {
        self.materials.get(mat_name).cloned().unwrap_or_default()
    }

    pub fn block_for(&self, def_name: &str) -> BlockDef {
        self.blocks.get(def_name).cloned().unwrap_or_default()
    }

    /// Evaluates current damage sprite tier based on health percentage (0..=100).
    pub fn select_damage_sprites(
        &self,
        def_name: &str,
        health_pct: f32,
        fallback_sprite: &str,
    ) -> (String, String, String) {
        if let Some(bdef) = self.blocks.get(def_name) {
            if !bdef.damage_sprites.is_empty() {
                for tier in bdef.damage_sprites.values() {
                    if health_pct <= tier.max && health_pct > tier.min {
                        let sp = if tier.sprite.is_empty() {
                            fallback_sprite.to_string()
                        } else {
                            tier.sprite.clone()
                        };
                        return (sp, tier.blink_sprite.clone(), tier.smile_sprite.clone());
                    }
                }
            }
        }
        (fallback_sprite.to_string(), String::new(), String::new())
    }
}
