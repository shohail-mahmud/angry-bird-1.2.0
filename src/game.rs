//! Active gameplay controller: camera system, authentic 1.2.0 parallax world renderer
//! (5 themes driven by `blocks.lua` `bgLayers`/`fgLayers`), slingshot aiming, tutorial
//! cards, HUD, and Pause / Victory / Defeat overlays.

use crate::assets::{safe_clamp, Assets};
use crate::gamedata::{GameData, LevelDef, PHYSICS_TO_WORLD};
use crate::physics::{LevelSimulation, GRAVITY_Y, LAUNCH_SPEED_SCALE, SHOOT_MAX_LENGTH};
use crate::save::SaveState;
use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LevelOutcome {
    None,
    BackToLevelSelect,
    RestartLevel,
    NextLevel(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ModalState {
    None,
    Tutorial,
    Paused,
    Victory,
    Defeat,
}

/// Maps a 1.2.0 theme id ("theme1".."theme5") to its numeric art index.
fn theme_index(theme: &str, gdata: &GameData) -> usize {
    if let Some(t) = gdata.themes.get(theme) {
        if t.index >= 1 && t.index <= 5 {
            return t.index;
        }
    }
    match theme {
        "theme2" => 2,
        "theme3" => 3,
        "theme4" => 4,
        "theme5" => 5,
        _ => 1,
    }
}

/// 1.2.0 ships three ambient loops for five chapters; pick the closest original track.
fn theme_ambient(theme: &str) -> &'static str {
    match theme {
        "theme2" | "theme5" => "ambient_green_jungleish",
        "theme3" => "ambient_red_savannah",
        _ => "ambient_white_dryforest",
    }
}

/// Draws one ground-anchored, horizontally tiled theme layer (bgLayers / fgLayers).
///
/// * the sprite is anchored at world `y = 0` using Rovio's original pivot,
/// * `parallax` is the original `bgLayers` scroll factor (0.125 / 0.5 / 1.0),
/// * `scale` converts 480x320 design pixels into current world pixels,
/// * tiles start one screen to the left and stop one screen past the right edge.
#[allow(clippy::too_many_arguments)]
fn draw_ground_layer(
    assets: &Assets,
    name: &str,
    parallax: f32,
    scale: f32,
    cam_x: f32,
    vis_left: f32,
    vis_right: f32,
    color: Color,
) {
    let (w, h) = assets.sprite_size(name);
    if w <= 1.0 || h <= 1.0 {
        return;
    }
    let step = safe_clamp(w * scale, 24.0, 40000.0);
    // bgLayers scroll factor straight from blocks.lua (0.125 far sky / 0.5 mid / 1.0 hills).
    //
    // These layers are drawn in WORLD space (the camera already applies -cam_center), so a
    // layer that should appear to move at `parallax` x camera speed must be placed at
    // world_x = i*step + cam_x*(1 - parallax). The old code used world_x = i*step - cam_x*parallax,
    // which made every layer scroll at (1 + parallax) x -- the ground ran at DOUBLE speed and
    // slid out from under the castle, so the structures looked pinned while the ground moved.
    let p = safe_clamp(parallax, 0.0, 2.0);
    let scroll = cam_x * (1.0 - p);
    let start = (((vis_left - scroll) / step).floor() as i64 - 1).clamp(-20000, 20000);
    let end = ((((vis_right - scroll) / step).ceil() as i64) + 1)
        .clamp(start, start + 96)
        .max(start);
    if std::env::var("AB_DEBUG_LAYER").is_ok()
        && name.contains("FOREGROUND")
        && name.ends_with("_1")
    {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        if N.fetch_add(1, Ordering::Relaxed) % 40 == 0 {
            eprintln!(
                "LAYER {} p={:.3} cam={:.1} scroll={:.1} world0={:.1} screen0={:.1} step={:.1}",
                name,
                parallax,
                cam_x,
                scroll,
                start as f32 * step,
                start as f32 * step - scroll,
                step
            );
        }
    }
    let sp = match assets.sprites.get(name) {
        Some(sp) => sp,
        None => return,
    };
    // Always draw at the artwork's native aspect: 1 art px == 1 world px. Stretching the
    // ground cross-section vertically (done to fill a tall window) distorted every rock.
    let _ = &sp;
    for i in start..=end {
        let x = scroll + (i as f32) * step;
        assets.draw_sprite(name, x, 0.0, scale, scale, 0.0, color);
    }
}

pub struct GameSession {
    pub sim: LevelSimulation,
    pub chapter: usize,
    pub stage: usize,
    pub cam_center: Vec2, // world pixels
    pub cam_target: Vec2,
    /// Left-over real time that has not been simulated yet (fixed 60 Hz physics).
    pub physics_accum: f32,
    pub debug_timer: f32,
    pub span_left_dbg: f32,
    pub span_right_dbg: f32,
    /// Cached framing inputs so the camera can re-fit when the window is resized
    pub fit_span_w: f32,
    pub fit_needed_vis_h: f32,
    pub fit_last_screen: (f32, f32),
    /// Deep underground fill colour (the ground cross-section's own bottom tone)
    pub deep_ground_col: Color,
    pub cam_zoom: f32, // world pixels -> screen scale multiplier
    pub cam_target_zoom: f32,
    pub default_zoom: f32,
    pub bird_top_wy: f32,
    pub castle_min_wy: f32,
    pub level_max_wy: f32,
    pub manual_y_offset: f32,
    pub intro_pan_timer: f32,
    #[allow(dead_code)]
    pub castle_focus_x: f32,
    pub sling_focus_x: f32,
    /// Camera X the level settles on when idle (frames both slingshot and castle).
    pub rest_focus_x: f32,

    pub dragging_sling: bool,
    pub sling_pull_pos: Vec2, // physics meters
    pub panning_cam: bool,
    pub last_mouse_screen: Vec2,
    pub show_full_arc: bool,

    modal: ModalState,
    active_tutorial_sprite: Option<String>,
    end_delay_timer: f32,
    is_new_highscore: bool,
}

impl GameSession {
    pub fn new(
        level: &LevelDef,
        gdata: &GameData,
        save: &mut SaveState,
        assets: &mut Assets,
    ) -> Self {
        let mut sim = LevelSimulation::new(level, gdata);
        sim.repair_sprites(gdata, assets);
        // Let the castle settle before the player ever sees it
        sim.settle(gdata, 45);

        // Compute castle center, tallest structure height, lowest terrain/object depth, & slingshot focus
        let mut castle_sum_x = 0.0f32;
        let mut castle_cnt = 0usize;
        let mut castle_min_wy = -200.0f32;
        let mut level_max_wy = 0.0f32;
        let mut castle_min_wx = f32::INFINITY;
        let mut castle_max_wx = f32::NEG_INFINITY;
        for o in &level.objects {
            if !o.controllable && o.start_number == 0 {
                castle_min_wx = castle_min_wx.min(o.x * PHYSICS_TO_WORLD - 40.0);
                castle_max_wx = castle_max_wx.max(o.x * PHYSICS_TO_WORLD + 40.0);
            }
            let owy_top = o.y * PHYSICS_TO_WORLD - 60.0;
            let owy_bot = o.y * PHYSICS_TO_WORLD + 35.0;
            if owy_top < castle_min_wy {
                castle_min_wy = owy_top;
            }
            if owy_bot > level_max_wy {
                level_max_wy = owy_bot;
            }
            if !o.controllable && o.start_number == 0 {
                castle_sum_x += o.x * PHYSICS_TO_WORLD;
                castle_cnt += 1;
            }
        }
        let sling_wx = sim.slingshot_pos.x * PHYSICS_TO_WORLD;
        let sling_wy = sim.slingshot_pos.y * PHYSICS_TO_WORLD;
        let r1 = sim
            .active_slingshot_bird
            .as_ref()
            .map(|b| b.radius)
            .unwrap_or(1.0);
        // Top of the bird's head/crest + slingshot fork tips in world pixels
        let bird_top_wy = sling_wy - r1 * PHYSICS_TO_WORLD - 30.0;

        // Compute ideal per-level default zoom so BOTH:
        // 1) Top has >=130 world px of open sky above the slingshot bird (even on tall cliffs 2-14, 3-3)
        // 2) Downside has generous ground/underground breathing room (~25% of screen height + >=165 world px)
        let desired_top = (bird_top_wy - 130.0).min(castle_min_wy * 0.72).min(-400.0);
        let desired_bot_world = (level_max_wy + 115.0).max(165.0);
        let needed_vis_h = ((-desired_top) / 0.73)
            .max((desired_bot_world - desired_top) * 1.04)
            .max(580.0);
        // 1.0 framing: fit the level by HEIGHT only (0.24..0.84, same range the 1.0 build uses)
        // and let the player pan across the level. The old code additionally shrank the zoom
        // until the whole slingshot->castle span fitted the width, which made everything look
        // small and distant next to 1.0.
        let vertical_zoom = safe_clamp(480.0 / needed_vis_h, 0.24, 0.84);

        let sw0 = screen_width().max(320.0);
        let sh0 = screen_height().max(240.0);
        // Include the waiting-bird queue so it is never clipped at the left edge
        // Frame the slingshot plus the waiting birds behind it (use their real positions so
        // the leftmost bird is never half off-screen).
        let queue_left_wx = sim
            .bird_queue
            .iter()
            .map(|b| b.ground_x * PHYSICS_TO_WORLD)
            .fold(sling_wx, f32::min)
            - 95.0;
        let span_left = queue_left_wx.min(sling_wx - 90.0);
        let span_right = if castle_max_wx.is_finite() {
            castle_max_wx + 130.0
        } else {
            sling_wx + 1100.0
        };
        let span_w = (span_right - span_left).max(620.0);
        let _ = (sw0, sh0);
        let default_zoom = vertical_zoom;

        let castle_wx = if castle_cnt > 0 {
            castle_sum_x / (castle_cnt as f32)
        } else {
            sling_wx + 900.0
        };

        let sling_focus_x = sling_wx + 240.0;
        let castle_focus_x = castle_wx;
        // Rest framing: centre of the slingshot..castle span so both are always on screen.
        // Rest on the slingshot, not on the middle of the level: fitting the whole
        // slingshot->castle span on screen made the camera almost static (the level
        // looked frozen while only the tiled ground slid past). The castle is reached
        // by the intro pan and by following the bird, like the original.
        let rest_focus_x = sling_focus_x;

        // Ambient music per 1.2.0 chapter theme (5 themes)
        let ambient = theme_ambient(&level.theme);
        assets.play_music(ambient, true, 0.55);
        assets.play_sfx("level_start_military", 0.75);

        // Check if active slingshot bird has an unseen tutorial card
        let mut modal = ModalState::None;
        let mut active_tutorial_sprite = None;
        if let Some(sb) = &sim.active_slingshot_bird {
            let bdef = gdata.block_for(&sb.definition);
            if !bdef.tutorial_info.is_empty() && !save.seen_tutorials.contains(&bdef.tutorial_info)
            {
                save.seen_tutorials.insert(bdef.tutorial_info.clone());
                save.save();
                active_tutorial_sprite = Some(bdef.tutorial_info);
                modal = ModalState::Tutorial;
            }
        }

        // Deep underground fill: the ground cross-section's own bottom tone so the area under
        // the artwork continues seamlessly (falls back to the theme's groundColor).
        let deep_ground_col_pre = {
            let theme_def = gdata.themes.get(&level.theme);
            let fg0 = theme_def
                .and_then(|t| t.fg_layers.first())
                .map(|l| l.sprite.clone())
                .unwrap_or_default();
            let fallback = theme_def
                .and_then(|t| t.ground_color.as_ref())
                .map(|c| Color::from_rgba(c.r, c.g, c.b, 255))
                .unwrap_or(Color::from_rgba(10, 19, 57, 255));
            assets
                .sprite_bottom_color(&fg0)
                .map(|c| Color::new(c.r, c.g, c.b, 1.0))
                .unwrap_or(fallback)
        };

        let vis_h0 = 480.0 / default_zoom;
        let bot0 = (level_max_wy + 115.0).max(165.0).max(vis_h0 * 0.25);
        let castle_start_y = (bot0 - vis_h0 * 0.5).min(castle_min_wy + vis_h0 * 0.5 + 40.0);
        let start_center = vec2(castle_focus_x, castle_start_y);
        Self {
            sim,
            chapter: level.chapter,
            stage: level.stage,
            cam_center: start_center,
            cam_target: start_center,
            physics_accum: 0.0,
            debug_timer: 0.0,
            span_left_dbg: span_left,
            span_right_dbg: span_right,
            fit_span_w: span_w,
            fit_needed_vis_h: needed_vis_h,
            fit_last_screen: (sw0, sh0),
            deep_ground_col: deep_ground_col_pre,
            cam_zoom: safe_clamp(default_zoom * 1.05, 0.24, 1.0),
            cam_target_zoom: default_zoom,
            default_zoom,
            bird_top_wy,
            castle_min_wy,
            level_max_wy,
            manual_y_offset: 0.0,
            intro_pan_timer: 1.25,
            castle_focus_x,
            sling_focus_x,
            rest_focus_x,
            dragging_sling: false,
            sling_pull_pos: Vec2::ZERO,
            panning_cam: false,
            last_mouse_screen: mouse_position().into(),
            show_full_arc: false,
            modal,
            active_tutorial_sprite,
            end_delay_timer: 0.0,
            is_new_highscore: false,
        }
    }

    /// Computes vertical camera target when viewing the slingshot so BOTH the bird's head at the top
    /// AND the ground/downside at the bottom (~25% of viewport height) always have generous room.
    pub fn sling_focus_y(&self, zoom: f32) -> f32 {
        let vis_h = 480.0 / zoom.max(0.24);
        let downside_bot_wy = (self.level_max_wy + 115.0).max(165.0).max(vis_h * 0.25);
        let ground_anchored = downside_bot_wy - vis_h * 0.5;
        let head_safe_max_y = (self.bird_top_wy - 95.0) + vis_h * 0.5;
        ground_anchored.min(head_safe_max_y)
    }

    /// Screen pixels per world pixel for the current window + camera zoom.
    #[inline]
    /// Re-fits the default camera zoom to the current window size. Without this, resizing the
    /// window kept the zoom that was computed for the original 1280x720 client area, so the
    /// top of tall castles (or the slingshot head) got clipped.
    fn refit_zoom(&mut self) {
        let sw = screen_width().max(320.0);
        let sh = screen_height().max(240.0);
        if (sw - self.fit_last_screen.0).abs() < 1.0 && (sh - self.fit_last_screen.1).abs() < 1.0 {
            return;
        }
        self.fit_last_screen = (sw, sh);
        // Height-fit only, 1.0's clamp range: resizing the window must not zoom the world
        // out to fit a whole level (that is what made the level look small on wide windows).
        let _ = (sw, sh);
        let z = safe_clamp(480.0 / self.fit_needed_vis_h.max(1.0), 0.24, 0.84);
        self.default_zoom = z;
        self.cam_target_zoom = z;
    }

    /// Fixed 60 Hz physics with a catch-up accumulator. Rapier is only stable with a small,
    /// constant timestep: feeding it the raw frame delta made tall castles collapse on slow
    /// machines, and running exactly one 1/60 s step per frame made the whole game play in
    /// slow motion whenever the renderer fell below 60 fps.
    fn step_physics(&mut self, dt: f32, gdata: &GameData) {
        const FIXED_DT: f32 = 1.0 / 60.0;
        const MAX_STEPS: u32 = 5;
        self.physics_accum = (self.physics_accum + dt.max(0.0)).min(0.25);
        let mut steps = 0;
        while self.physics_accum >= FIXED_DT && steps < MAX_STEPS {
            self.sim.step(FIXED_DT, gdata);
            self.physics_accum -= FIXED_DT;
            steps += 1;
        }
        if steps == MAX_STEPS {
            // Do not let a long stall snowball into a spiral of death
            self.physics_accum = 0.0;
        }
    }

    #[allow(dead_code)]
    fn px_per_world(&self) -> f32 {
        let sh = screen_height().max(64.0);
        ((sh / 480.0) * self.cam_zoom.max(0.20)).max(0.0001)
    }

    /// Screen pixels per world unit (height-fit), capped so that a very wide window can
    /// never zoom out far enough to show an entire level at once. When the whole level fits
    /// on screen the camera has nowhere to pan to, and the level looks frozen while only the
    /// parallax/ground slides past -- the "structure stays in place" bug.
    fn base_scale(&self, _sw: f32, sh: f32) -> f32 {
        // Pure height fit, exactly like the 1.0 build: nothing stops the player from zooming
        // right out until the whole level fits on screen. (A width cap used to sit here; it
        // was a workaround for the ground-scroll bug, which turned out to be the ground layer
        // being anchored to the camera instead of the world.)
        ((sh / 480.0) * self.cam_zoom.max(0.10)).max(0.0001)
    }

    fn build_world_camera(&self) -> Camera2D {
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        // Base virtual height = 480 world pixels at zoom=1.0 (never fits a whole level)
        let base_scale = self.base_scale(sw, sh);
        let shake_offset = if self.sim.camera_shake > 0.0 {
            let ang = (macroquad::rand::rand() % 360) as f32 * 0.01745;
            vec2(ang.cos(), ang.sin()) * self.sim.camera_shake
        } else {
            Vec2::ZERO
        };
        Camera2D {
            target: self.cam_center + shake_offset,
            zoom: vec2(2.0 * base_scale / sw, 2.0 * base_scale / sh),
            ..Default::default()
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        gdata: &GameData,
        save: &mut SaveState,
        assets: &mut Assets,
    ) -> LevelOutcome {
        // Flush pending simulation sound effects
        let sounds = std::mem::take(&mut self.sim.pending_sounds);
        for (sname, vol) in sounds {
            assets.play_sfx(&sname, vol);
        }

        // Quick hotkeys
        if is_key_pressed(KeyCode::R) {
            assets.play_sfx("menu confirm", 0.8);
            return LevelOutcome::RestartLevel;
        }
        if is_key_pressed(KeyCode::T) {
            self.show_full_arc = !self.show_full_arc;
        }
        if is_key_pressed(KeyCode::M) {
            let new_mute = !save.muted;
            save.muted = new_mute;
            save.save();
            let ambient = theme_ambient(&self.sim.theme);
            assets.set_muted(new_mute, Some((ambient, 0.55)));
        }

        // Handle modal UI states
        match self.modal {
            ModalState::Tutorial => {
                if is_mouse_button_pressed(MouseButton::Left)
                    || is_key_pressed(KeyCode::Space)
                    || is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::Escape)
                {
                    assets.play_sfx("menu confirm", 0.8);
                    self.modal = ModalState::None;
                }
                return LevelOutcome::None;
            }
            ModalState::Paused => {
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
                    assets.play_sfx("menu back", 0.8);
                    self.modal = ModalState::None;
                    return LevelOutcome::None;
                }
                return self.update_pause_modal(save, assets);
            }
            ModalState::Victory => {
                self.step_physics(dt, gdata);
                return self.update_end_modal(true, save, assets);
            }
            ModalState::Defeat => {
                self.step_physics(dt, gdata);
                return self.update_end_modal(false, save, assets);
            }
            ModalState::None => {}
        }

        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
            assets.play_sfx("menu select", 0.8);
            self.modal = ModalState::Paused;
            return LevelOutcome::None;
        }

        // Check pause / restart HUD buttons top-left
        let mpos: Vec2 = mouse_position().into();
        let ui_scale = save.gui_scale();
        let pause_btn = Rect::new(
            12.0 * ui_scale,
            11.0 * ui_scale,
            46.0 * ui_scale,
            46.0 * ui_scale,
        );
        let restart_btn = Rect::new(
            66.0 * ui_scale,
            11.0 * ui_scale,
            46.0 * ui_scale,
            46.0 * ui_scale,
        );

        if is_mouse_button_pressed(MouseButton::Left) {
            if pause_btn.contains(mpos) {
                assets.play_sfx("menu select", 0.8);
                self.modal = ModalState::Paused;
                return LevelOutcome::None;
            }
            if restart_btn.contains(mpos) {
                assets.play_sfx("menu confirm", 0.8);
                return LevelOutcome::RestartLevel;
            }
        }

        // Check if newly loaded slingshot bird triggers an unseen tutorial
        if let Some(sb) = &self.sim.active_slingshot_bird {
            let bdef = gdata.block_for(&sb.definition);
            if !bdef.tutorial_info.is_empty() && !save.seen_tutorials.contains(&bdef.tutorial_info)
            {
                save.seen_tutorials.insert(bdef.tutorial_info.clone());
                save.save();
                self.active_tutorial_sprite = Some(bdef.tutorial_info);
                self.modal = ModalState::Tutorial;
                return LevelOutcome::None;
            }
        }

        // Camera zoom via mouse wheel or +/-/Z/Home keys
        let (_, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 {
            let step = if wheel_y > 0.0 { 1.10 } else { 0.90 };
            self.cam_target_zoom = (self.cam_target_zoom * step).clamp(0.25, 1.45);
        }
        if is_key_pressed(KeyCode::Z) {
            let zoomed_out = (self.default_zoom * 0.55).clamp(0.25, 0.55);
            self.cam_target_zoom = if self.cam_target_zoom > zoomed_out + 0.08 {
                zoomed_out
            } else {
                self.default_zoom
            };
        }
        if is_key_down(KeyCode::Equal) {
            self.cam_target_zoom = (self.cam_target_zoom + dt * 0.8).clamp(0.25, 1.45);
        }
        if is_key_down(KeyCode::Minus) {
            self.cam_target_zoom = (self.cam_target_zoom - dt * 0.8).clamp(0.25, 1.45);
        }
        if is_key_pressed(KeyCode::Home) || is_key_pressed(KeyCode::Backspace) {
            self.intro_pan_timer = 0.0;
            self.manual_y_offset = 0.0;
            self.cam_target_zoom = self.default_zoom;
            self.cam_target = vec2(self.rest_focus_x, self.sling_focus_y(self.default_zoom));
        }

        let cam = self.build_world_camera();
        let mouse_world = cam.screen_to_world(mpos);
        let mouse_phys = mouse_world / PHYSICS_TO_WORLD;

        // Intro castle-to-slingshot camera pan
        if self.intro_pan_timer > 0.0 {
            self.intro_pan_timer -= dt;
            if is_mouse_button_pressed(MouseButton::Left) || is_key_pressed(KeyCode::Space) {
                self.intro_pan_timer = 0.0;
            }
            if self.intro_pan_timer <= 0.0 {
                self.cam_target_zoom = self.default_zoom;
                self.cam_target = vec2(self.rest_focus_x, self.sling_focus_y(self.default_zoom));
            }
        }

        // Slingshot drag / Mid-air bird specialty / Camera pan
        if is_mouse_button_pressed(MouseButton::Left) {
            if self.sim.primary_flying_bird.is_some() && self.sim.specialty_available {
                self.sim.trigger_specialty(gdata);
            } else if self.sim.active_slingshot_bird.is_some()
                && self.sim.flying_birds.is_empty()
                && mouse_phys.distance(self.sim.slingshot_pos) < 4.5
            {
                self.dragging_sling = true;
                self.intro_pan_timer = 0.0;
                self.manual_y_offset = 0.0;
                self.sling_pull_pos = self.sim.slingshot_pos;
                if let Some(sb) = &self.sim.active_slingshot_bird {
                    let bdef = gdata.block_for(&sb.definition);
                    assets.play_sfx(&bdef.selection_sound, 0.85);
                }
                assets.play_sfx("slingshot_stretched", 0.75);
            } else {
                self.panning_cam = true;
                self.intro_pan_timer = 0.0;
            }
        }

        if is_key_pressed(KeyCode::Space) && self.sim.primary_flying_bird.is_some() {
            self.sim.trigger_specialty(gdata);
        }

        if is_mouse_button_pressed(MouseButton::Right)
            || is_mouse_button_pressed(MouseButton::Middle)
        {
            self.panning_cam = true;
            self.intro_pan_timer = 0.0;
        }

        if self.dragging_sling {
            if is_mouse_button_down(MouseButton::Left) {
                let mut delta = mouse_phys - self.sim.slingshot_pos;
                let len = delta.length();
                if len > SHOOT_MAX_LENGTH {
                    delta = delta / len * SHOOT_MAX_LENGTH;
                }
                self.sling_pull_pos = self.sim.slingshot_pos + delta;
            } else {
                self.dragging_sling = false;
                self.manual_y_offset = 0.0;
                let release = self.sling_pull_pos;
                self.sim.launch_bird(release, gdata);
            }
        }

        if self.panning_cam {
            if is_mouse_button_down(MouseButton::Left)
                || is_mouse_button_down(MouseButton::Right)
                || is_mouse_button_down(MouseButton::Middle)
            {
                let delta_screen = mpos - self.last_mouse_screen;
                let base_scale = self
                    .base_scale(screen_width().max(1.0), screen_height().max(1.0))
                    .max(0.0001);
                self.cam_target.x -= delta_screen.x / base_scale;
                self.manual_y_offset -= delta_screen.y / base_scale;
                self.manual_y_offset = self.manual_y_offset.clamp(-550.0, 350.0);
                self.cam_center.x = self.cam_target.x;
            } else {
                self.panning_cam = false;
            }
        }

        // Keyboard arrow / WASD 2D camera pan
        let pan_speed = 650.0 * dt / self.cam_zoom.max(0.32);
        if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            self.cam_target.x -= pan_speed;
            self.intro_pan_timer = 0.0;
        }
        if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            self.cam_target.x += pan_speed;
            self.intro_pan_timer = 0.0;
        }
        if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            self.manual_y_offset = (self.manual_y_offset - pan_speed).clamp(-550.0, 350.0);
            self.intro_pan_timer = 0.0;
        }
        if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            self.manual_y_offset = (self.manual_y_offset + pan_speed).clamp(-550.0, 350.0);
            self.intro_pan_timer = 0.0;
        }

        self.last_mouse_screen = mpos;

        // Step physics simulation
        self.refit_zoom();
        self.step_physics(dt, gdata);

        if std::env::var("AB_DEBUG_CAM").is_ok() {
            self.debug_timer += dt;
            if self.debug_timer > 1.0 {
                self.debug_timer = 0.0;
                let sw = screen_width();
                let sh = screen_height();
                let pxw = self.base_scale(sw, sh);
                let left = self.cam_center.x - sw * 0.5 / pxw;
                let right = self.cam_center.x + sw * 0.5 / pxw;
                let qx: Vec<f32> = self
                    .sim
                    .bird_queue
                    .iter()
                    .map(|b| b.ground_x * PHYSICS_TO_WORLD)
                    .collect();
                eprintln!(
                    "CAM zoom={:.3} center=({:.1},{:.1}) vis=[{:.1}..{:.1}] span=[{:.1}..{:.1}] rest={:.1} queue=[{:?}]",
                    self.cam_zoom, self.cam_center.x, self.cam_center.y, left, right,
                    self.span_left_dbg, self.span_right_dbg, self.rest_focus_x, qx
                );
            }
        }

        // Automatic camera tracking of flying bird (including high arcs & White Bird recoil!)
        let rest_y = self.sling_focus_y(self.cam_target_zoom);
        if let Some(fb_h) = self.sim.primary_flying_bird {
            if !self.panning_cam {
                if let Some(rb) = self.sim.rigid_bodies.get(fb_h) {
                    let bx = rb.translation().x * PHYSICS_TO_WORLD;
                    let by = rb.translation().y * PHYSICS_TO_WORLD;
                    let vis_h = 480.0 / self.cam_zoom.max(0.24);
                    let downside_bot_wy = (self.level_max_wy + 115.0).max(165.0).max(vis_h * 0.25);
                    self.cam_target.x = bx + 110.0;
                    // Follow bird upward whenever it rises toward the upper third of the viewport
                    let follow_top_y = by + vis_h * 0.30;
                    let ground_max_y = downside_bot_wy - vis_h * 0.5;
                    self.cam_target.y = safe_clamp(
                        rest_y.min(follow_top_y).max(ground_max_y.min(follow_top_y)),
                        -2200.0,
                        ground_max_y + 60.0,
                    );
                }
            }
        } else if self.intro_pan_timer <= 0.0 {
            let vis_h = 480.0 / self.cam_target_zoom.max(0.24);
            let downside_bot_wy = (self.level_max_wy + 115.0).max(165.0).max(vis_h * 0.25);
            let max_y = (downside_bot_wy + 60.0) - vis_h * 0.5;
            let min_y = self.bird_top_wy.min(self.castle_min_wy) - 450.0;
            let lo_y = min_y.min(rest_y - 350.0);
            let hi_y = max_y.max(rest_y + 220.0);
            self.cam_target.y = safe_clamp(rest_y + self.manual_y_offset, lo_y, hi_y);
            // Return camera toward slingshot when next bird loads
            if !self.panning_cam && self.sim.load_bird_anim.is_some() {
                self.cam_target.x = self.rest_focus_x;
                self.manual_y_offset = 0.0;
            }
        }

        // Pan limits: the level's own left/right extent (plus a little air), so the
        // camera can actually travel to the castle and follow a shot there.
        let min_cam_x = self.span_left_dbg - 220.0;
        let max_cam_x = self.span_right_dbg + 260.0;
        self.cam_target.x = safe_clamp(self.cam_target.x, min_cam_x, max_cam_x);

        let smooth = safe_clamp(dt * 6.5, 0.0, 1.0);
        self.cam_center = self.cam_center.lerp(self.cam_target, smooth);
        self.cam_zoom += (self.cam_target_zoom - self.cam_zoom) * safe_clamp(dt * 7.0, 0.0, 1.0);

        // Check Victory / Defeat conditions
        if self.sim.remaining_pigs() == 0 {
            self.end_delay_timer += dt;
            if self.end_delay_timer > 1.35
                && (self.sim.is_world_settled() || self.end_delay_timer > 3.2)
            {
                self.sim.award_unused_bird_bonuses(gdata);
                let stars = self.sim.earned_stars();
                self.is_new_highscore =
                    save.record_win(self.sim.level_index, self.sim.score, stars);
                assets.play_sfx("level_clear_military", 0.9);
                assets.play_sfx("level_complete", 0.85);
                self.modal = ModalState::Victory;
            }
        } else if self.sim.unused_birds_count() == 0 && self.sim.flying_birds.is_empty() {
            self.end_delay_timer += dt;
            if self.end_delay_timer > 1.6
                && (self.sim.is_world_settled() || self.end_delay_timer > 4.2)
            {
                for ent in self.sim.entities.values_mut() {
                    if ent.is_pig {
                        ent.smiling = true;
                    }
                }
                assets.play_sfx("level_failed_piglets", 0.9);
                self.modal = ModalState::Defeat;
            }
        } else {
            self.end_delay_timer = 0.0;
        }

        LevelOutcome::None
    }

    pub fn draw(&self, gdata: &GameData, save: &SaveState, assets: &Assets) {
        let theme_idx = theme_index(&self.sim.theme, gdata);
        let tdef = gdata.themes.get(&self.sim.theme);
        let sky_col = tdef
            .and_then(|t| t.color.as_ref())
            .map(|c| Color::from_rgba(c.r, c.g, c.b, 255))
            .unwrap_or(Color::from_rgba(148, 206, 222, 255));
        let _ground_col = tdef
            .and_then(|t| t.ground_color.as_ref())
            .map(|c| Color::from_rgba(c.r, c.g, c.b, 255))
            .unwrap_or(Color::from_rgba(25, 20, 15, 255));

        clear_background(sky_col);

        let cam = self.build_world_camera();
        set_camera(&cam);

        // Visible world bounds (safely clamped so ultra-wide/tiny windows never over-tile)
        let sw_safe = screen_width().max(64.0);
        let sh_safe = screen_height().max(64.0);
        let top_left = cam.screen_to_world(vec2(0.0, 0.0));
        let bot_right = cam.screen_to_world(vec2(sw_safe, sh_safe));
        let half_span = ((bot_right.x - top_left.x).abs() * 0.5 + 140.0).min(6500.0);
        let vis_left = self.cam_center.x - half_span;
        let vis_right = self.cam_center.x + half_span;

        // ---- Authentic Angry Birds 1.2.0 theme stack ----
        // Draw order taken from the original renderer:
        //   1) solid `groundColor` fill below y = 0 (blocks.lua themes -> groundColor)
        //   2) bgLayers  -> sky sheet, tree silhouettes, grass fringe (their own scroll factors)
        //   3) objects / birds
        //   4) fgLayers  -> ground cross-section (rocks), then the front grass blades
        // 1.2.0 in-game art is authored 1 art px == 1 world px, so layers never get rescaled.
        let layer_scale = 1.0f32;
        let ground_depth = (bot_right.y + 400.0).max(500.0);

        // 1) Solid ground colour. Themes with a blue groundColor get the cross-section's own
        //    deepest tone so the fill blends perfectly into the bottom of the rock artwork.
        draw_rectangle(
            vis_left,
            0.0,
            vis_right - vis_left,
            ground_depth,
            self.deep_ground_col,
        );

        // 3) Original bgLayers (sky -> trees -> grass)
        let bg_layers: Vec<(String, f32)> = tdef
            .map(|t| {
                t.bg_layers
                    .iter()
                    .map(|l| (l.sprite.clone(), l.parallax))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    (format!("BACKGROUND_{}_LAYER_1", theme_idx), 0.125),
                    (format!("BACKGROUND_{}_LAYER_2", theme_idx), 0.5),
                    (format!("BACKGROUND_{}_LAYER_3", theme_idx), 1.0),
                ]
            });
        for (lname, parallax) in &bg_layers {
            draw_ground_layer(
                assets,
                lname,
                *parallax,
                layer_scale,
                self.cam_center.x,
                vis_left,
                vis_right,
                WHITE,
            );
        }

        // 4. Smoke flight trails (previous shot faded, current shot crisp)
        for dot in &self.sim.prev_trail {
            assets.draw_sprite(
                &dot.sprite,
                dot.x,
                dot.y,
                dot.scale,
                dot.scale,
                0.0,
                Color::new(1.0, 1.0, 1.0, 0.38),
            );
        }
        for dot in &self.sim.cur_trail {
            assets.draw_sprite(
                &dot.sprite,
                dot.x,
                dot.y,
                dot.scale,
                dot.scale,
                0.0,
                Color::new(1.0, 1.0, 1.0, 0.88),
            );
        }

        // 5. Slingshot BACK fork + back elastic strap
        let sling_w = self.sim.slingshot_pos * PHYSICS_TO_WORLD;
        assets.draw_sprite(
            "SLING_SHOT_01_BACK",
            sling_w.x,
            sling_w.y,
            1.0,
            1.0,
            0.0,
            WHITE,
        );

        let pouch_pos_phys = if self.dragging_sling {
            self.sling_pull_pos
        } else {
            self.sim.slingshot_pos + vec2(-0.1, -0.1)
        };
        let pouch_w = pouch_pos_phys * PHYSICS_TO_WORLD;

        let back_fork_anchor = sling_w + vec2(12.0, -4.0);
        let front_fork_anchor = sling_w + vec2(-12.0, -2.0);
        let strap_col = Color::from_rgba(48, 25, 14, 255);

        if self.dragging_sling || self.sim.active_slingshot_bird.is_some() {
            draw_line(
                back_fork_anchor.x,
                back_fork_anchor.y,
                pouch_w.x,
                pouch_w.y,
                6.0,
                strap_col,
            );
        }

        // Aiming preview dots while pulling slingshot
        if self.dragging_sling {
            let pull_vec = self.sim.slingshot_pos - self.sling_pull_pos;
            let pull_len = pull_vec.length().min(SHOOT_MAX_LENGTH);
            if pull_len >= 0.35 {
                let dir = pull_vec / pull_len;
                let spd = LAUNCH_SPEED_SCALE * (pull_len / SHOOT_MAX_LENGTH);
                let v0 = dir * spd;
                let dot_count = if self.show_full_arc { 26 } else { 8 };
                for step in 1..=dot_count {
                    let t = (step as f32) * 0.065;
                    let px = (self.sling_pull_pos.x + v0.x * t) * PHYSICS_TO_WORLD;
                    let py = (self.sling_pull_pos.y + v0.y * t + 0.5 * GRAVITY_Y * t * t)
                        * PHYSICS_TO_WORLD;
                    if py > 4.0 {
                        break;
                    }
                    let alpha = (1.0 - (step as f32) / (dot_count as f32 + 2.0)) * 0.85;
                    draw_circle(px, py, 4.2, Color::new(1.0, 1.0, 1.0, alpha));
                    draw_circle_lines(px, py, 4.2, 1.2, Color::new(0.15, 0.15, 0.15, alpha * 0.7));
                }
            }
        }

        // 6. Queued waiting birds & bird hopping into slingshot
        for qb in &self.sim.bird_queue {
            assets.draw_sprite(
                &qb.sprite,
                qb.ground_x * PHYSICS_TO_WORLD,
                qb.ground_y * PHYSICS_TO_WORLD + qb.hop_offset,
                1.0,
                1.0,
                0.0,
                WHITE,
            );
        }
        if let Some((lb, start_p, t)) = &self.sim.load_bird_anim {
            let cur_p = start_p.lerp(self.sim.slingshot_pos, *t);
            let arc_y = -(t * std::f32::consts::PI).sin() * 95.0;
            let spin = *t * std::f32::consts::TAU;
            assets.draw_sprite(
                &lb.sprite,
                cur_p.x * PHYSICS_TO_WORLD,
                cur_p.y * PHYSICS_TO_WORLD + arc_y,
                1.0,
                1.0,
                spin,
                WHITE,
            );
        }
        if let Some(sb) = &self.sim.active_slingshot_bird {
            assets.draw_sprite(&sb.sprite, pouch_w.x, pouch_w.y, 1.0, 1.0, 0.0, WHITE);
        }

        // 7. Joint ropes / chains
        for &(h1, loc1, h2, loc2) in &self.sim.joint_visuals {
            if let (Some(b1), Some(b2)) =
                (self.sim.rigid_bodies.get(h1), self.sim.rigid_bodies.get(h2))
            {
                let p1 = b1
                    .position()
                    .transform_point(&rapier2d::na::Point2::new(loc1.x, loc1.y));
                let p2 = b2
                    .position()
                    .transform_point(&rapier2d::na::Point2::new(loc2.x, loc2.y));
                draw_line(
                    p1.x * PHYSICS_TO_WORLD,
                    p1.y * PHYSICS_TO_WORLD,
                    p2.x * PHYSICS_TO_WORLD,
                    p2.y * PHYSICS_TO_WORLD,
                    4.0,
                    Color::from_rgba(70, 50, 32, 255),
                );
            }
        }

        // 8. Static blocks first, then dynamic blocks/pigs/birds, then visual decorations
        for pass in 0..2 {
            for (&h, ent) in &self.sim.entities {
                if (pass == 0) != ent.is_static_prop {
                    continue;
                }
                let Some(rb) = self.sim.rigid_bodies.get(h) else {
                    continue;
                };
                let wx = rb.translation().x * PHYSICS_TO_WORLD;
                let wy = rb.translation().y * PHYSICS_TO_WORLD;
                let rot = rb.rotation().angle();

                let draw_sp = if ent.smiling && !ent.smile_sprite.is_empty() {
                    &ent.smile_sprite
                } else if ent.blinking && !ent.blink_sprite.is_empty() {
                    &ent.blink_sprite
                } else {
                    &ent.sprite
                };

                if !draw_sp.is_empty() {
                    assets.draw_sprite(draw_sp, wx, wy, 1.0, 1.0, rot, WHITE);
                }
            }
        }

        for vp in &self.sim.visual_props {
            assets.draw_sprite(
                &vp.sprite,
                vp.x * PHYSICS_TO_WORLD,
                vp.y * PHYSICS_TO_WORLD,
                1.0,
                1.0,
                vp.angle,
                WHITE,
            );
        }

        // 9. Slingshot pouch + FRONT elastic strap + FRONT fork (drawn in front of bird!)
        if self.dragging_sling || self.sim.active_slingshot_bird.is_some() {
            let pull_dir = (pouch_w - sling_w).normalize_or_zero();
            let holder_pos = pouch_w + pull_dir * 8.0;
            let holder_rot = pull_dir.y.atan2(pull_dir.x);
            assets.draw_sprite(
                "SLING_HOLDER",
                holder_pos.x,
                holder_pos.y,
                1.0,
                1.0,
                holder_rot,
                WHITE,
            );
            draw_line(
                front_fork_anchor.x,
                front_fork_anchor.y,
                holder_pos.x,
                holder_pos.y,
                6.5,
                strap_col,
            );
        }
        assets.draw_sprite(
            "SLING_SHOT_01_FRONT",
            sling_w.x - 4.0,
            sling_w.y + 4.0,
            1.0,
            1.0,
            0.0,
            WHITE,
        );

        // 10. Authentic 1.2.0 foreground layers (ground cross-section + grass fringe) drawn OVER the birds
        let fg_layers: Vec<String> = tdef
            .map(|t| {
                t.fg_layers
                    .iter()
                    .map(|l| l.sprite.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![format!("FOREGROUND_{}_LAYER_1", theme_idx)]);
        for (idx, fname) in fg_layers.iter().enumerate() {
            // fgLayers[0] is the ground cross-section, fgLayers[1] the front grass blades.
            // Both scroll at world speed (parallax 1.0) and keep their native size.
            if idx == 0 {
                // Continue the cross-section past the bottom of the artwork with a flat fill in
                // the artwork's own deepest ground tone (sampled from its last row). Repeating
                // or stretching art rows here produced visible stripes and duplicated the wavy
                // shadow line, which is exactly the "glitchy ground" this replaces.
                let (_fw, fh) = assets.sprite_size(fname);
                if fh > 2.0 {
                    let sp = assets.sprites.get(fname);
                    let pivot_y = sp.map(|s| s.pivot_y).unwrap_or(0.0);
                    let art_bottom = -pivot_y * layer_scale + fh * layer_scale;
                    let tail_bottom = bot_right.y + 120.0;
                    if tail_bottom > art_bottom {
                        // Fade the artwork's deepest tone into a darker "deep earth" shade instead
                        // of a flat slab. Only visible when the camera is zoomed far out, but a
                        // hard band of colour there looked like a black bar across the screen.
                        const STRIPS: usize = 28;
                        let h = tail_bottom - art_bottom + 2.0;
                        let base = self.deep_ground_col;
                        let dark = Color::new(base.r * 0.38, base.g * 0.38, base.b * 0.40, 1.0);
                        for i in 0..STRIPS {
                            let t = i as f32 / (STRIPS - 1) as f32;
                            let c = Color::new(
                                base.r + (dark.r - base.r) * t,
                                base.g + (dark.g - base.g) * t,
                                base.b + (dark.b - base.b) * t,
                                1.0,
                            );
                            draw_rectangle(
                                vis_left,
                                art_bottom - 1.0 + h * (i as f32 / STRIPS as f32),
                                vis_right - vis_left,
                                h / STRIPS as f32 + 1.0,
                                c,
                            );
                        }
                    }
                }
            }
            draw_ground_layer(
                assets,
                fname,
                1.0,
                layer_scale,
                self.cam_center.x,
                vis_left,
                vis_right,
                WHITE,
            );
        }

        // 11. Particles & Floating Score Popups
        for p in &self.sim.particles {
            let alpha = (1.0 - (p.age / p.lifetime).powi(2)).clamp(0.0, 1.0);
            assets.draw_sprite(
                &p.sprite,
                p.x,
                p.y,
                p.scale,
                p.scale,
                p.rot,
                Color::new(1.0, 1.0, 1.0, alpha),
            );
        }
        for fs in &self.sim.floating_scores {
            let alpha = (1.0 - (fs.age / fs.lifetime).powi(2)).clamp(0.0, 1.0);
            if let Some(sp) = &fs.sprite {
                let pop = (1.0 + (fs.age * 6.0).min(0.25)) * 0.9;
                assets.draw_sprite(
                    sp,
                    fs.x,
                    fs.y,
                    pop,
                    pop,
                    0.0,
                    Color::new(1.0, 1.0, 1.0, alpha),
                );
            } else {
                assets.draw_bitmap_text(
                    "FONT_SCORE",
                    &fs.text,
                    fs.x,
                    fs.y,
                    0.52,
                    0.5,
                    Color::new(fs.color.r, fs.color.g, fs.color.b, alpha),
                );
            }
        }

        // Switch to screen-space HUD
        set_default_camera();
        self.draw_hud(gdata, save, assets);
    }

    fn draw_hud(&self, gdata: &GameData, save: &SaveState, assets: &Assets) {
        let sw = screen_width();
        let sh = screen_height();
        let ui = save.gui_scale();
        // Compact top bar: the pause/restart/level/score widgets used to cover a lot of
        // the playfield. Popups and overlays below still use the full `ui` scale.
        let hud = ui * 0.65;

        // Top-left Pause & Quick-Restart buttons
        assets.draw_sprite_centered("MENU_BUTTON", 35.0 * hud, 34.0 * hud, 0.90 * hud, WHITE);
        assets.draw_sprite_centered("BUTTON_RESTART", 89.0 * hud, 34.0 * hud, 0.58 * hud, WHITE);

        // Level label & active bird specialty hint
        let stage_lbl = format!("{}-{}", self.chapter, self.stage);
        assets.draw_bitmap_text(
            "FONT_MENU",
            &stage_lbl,
            130.0 * hud,
            43.0 * hud,
            0.68 * hud,
            0.0,
            WHITE,
        );

        if self.sim.primary_flying_bird.is_some() && self.sim.specialty_available {
            if let Some(fb_h) = self.sim.primary_flying_bird {
                if let Some(ent) = self.sim.entities.get(&fb_h) {
                    let bdef = gdata.block_for(&ent.definition);
                    if !bdef.sprite_action.is_empty() {
                        assets.draw_sprite_centered(
                            &bdef.sprite_action,
                            sw * 0.5,
                            36.0 * hud,
                            1.05 * hud,
                            WHITE,
                        );
                        assets.draw_bitmap_text(
                            "FONT_BASIC",
                            "CLICK / SPACE",
                            sw * 0.5,
                            72.0 * ui,
                            0.92 * ui,
                            0.5,
                            Color::from_rgba(255, 238, 130, 255),
                        );
                    }
                }
            }
        }

        // Top-right Highscore & Current Score using original Rovio bitmap fonts
        let best = save
            .highscores
            .get(&self.sim.level_index)
            .copied()
            .unwrap_or(0)
            .max(self.sim.score);

        assets.draw_bitmap_text(
            "FONT_BASIC",
            "HIGHSCORE",
            sw - 18.0 * hud,
            22.0 * hud,
            0.88 * hud,
            1.0,
            Color::from_rgba(255, 240, 175, 255),
        );
        assets.draw_bitmap_text(
            "FONT_BASIC",
            &best.to_string(),
            sw - 18.0 * hud,
            42.0 * hud,
            0.96 * hud,
            1.0,
            Color::from_rgba(255, 240, 175, 255),
        );
        assets.draw_bitmap_text(
            "FONT_BIG_NUMBERS",
            &self.sim.score.to_string(),
            sw - 18.0 * hud,
            80.0 * hud,
            0.78 * hud,
            1.0,
            WHITE,
        );

        // Modal overlays
        match self.modal {
            ModalState::Tutorial => {
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.58));
                if let Some(sp) = &self.active_tutorial_sprite {
                    assets.draw_sprite_centered(sp, sw * 0.5, sh * 0.47, 1.35 * ui, WHITE);
                }
                assets.draw_sprite_centered(
                    "TUTORIAL_OK",
                    sw * 0.5 + 195.0 * ui,
                    sh * 0.47 + 95.0 * ui,
                    1.2 * ui,
                    WHITE,
                );
            }
            ModalState::Paused => {
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.65));
                assets.draw_bitmap_text(
                    "FONT_MENU",
                    "PAUSED",
                    sw * 0.5,
                    sh * 0.24,
                    1.15 * ui,
                    0.5,
                    WHITE,
                );
                assets.draw_bitmap_text(
                    "FONT_BASIC",
                    "REBUILD BY SHOHAIL MAHMUD",
                    sw * 0.5,
                    sh * 0.33,
                    0.96 * ui,
                    0.5,
                    Color::from_rgba(255, 236, 150, 255),
                );

                let cy = sh * 0.50;
                let spacing = 115.0 * ui;
                assets.draw_sprite_centered(
                    "BUTTON_RESUME",
                    sw * 0.5 - spacing * 1.5,
                    cy,
                    1.25 * ui,
                    WHITE,
                );
                assets.draw_sprite_centered(
                    "BUTTON_RESTART",
                    sw * 0.5 - spacing * 0.5,
                    cy,
                    1.0 * ui,
                    WHITE,
                );
                assets.draw_sprite_centered(
                    "BUTTON_MENU",
                    sw * 0.5 + spacing * 0.5,
                    cy,
                    1.0 * ui,
                    WHITE,
                );
                assets.draw_sprite_centered(
                    "BUTTON_SFX",
                    sw * 0.5 + spacing * 1.5,
                    cy,
                    1.2 * ui,
                    WHITE,
                );
                if save.muted {
                    assets.draw_sprite_centered(
                        "BUTTON_OFF",
                        sw * 0.5 + spacing * 1.5,
                        cy,
                        1.2 * ui,
                        WHITE,
                    );
                }

                assets.draw_bitmap_text(
                    "FONT_BASIC",
                    "DRAG: SHOOT   SPACE: ABILITY   WHEEL: ZOOM   [ / ]: UI SIZE",
                    sw * 0.5,
                    sh - 56.0 * ui,
                    0.85 * ui,
                    0.5,
                    Color::from_rgba(235, 245, 255, 240),
                );
            }
            ModalState::Victory => {
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.65));
                let cx = sw * 0.5;
                let cy = sh * 0.48;
                assets.draw_sprite_centered("LEVEL_FINISH_BG", cx, cy, 1.35 * ui, WHITE);
                assets.draw_sprite_centered(
                    "LEVEL_FINISH_TOP_BG",
                    cx,
                    cy - 125.0 * ui,
                    1.35 * ui,
                    WHITE,
                );
                assets.draw_bitmap_text(
                    "FONT_MENU",
                    "LEVEL CLEARED!",
                    cx,
                    cy - 114.0 * ui,
                    0.78 * ui,
                    0.5,
                    WHITE,
                );

                let stars = self.sim.earned_stars();
                let star_sp = match stars {
                    3 => "STARS_BIG_3",
                    2 => "STARS_BIG_2",
                    1 => "STARS_BIG_1",
                    _ => "STARS_BIG_0",
                };
                assets.draw_sprite_centered(star_sp, cx, cy - 52.0 * ui, 2.2 * ui, WHITE);

                assets.draw_bitmap_text(
                    "FONT_BIG_NUMBERS",
                    &self.sim.score.to_string(),
                    cx,
                    cy + 26.0 * ui,
                    0.95 * ui,
                    0.5,
                    WHITE,
                );

                if self.is_new_highscore {
                    assets.draw_sprite_centered(
                        "NEW_HIGHSCORE",
                        cx + 175.0 * ui,
                        cy - 28.0 * ui,
                        1.1 * ui,
                        WHITE,
                    );
                }

                let btn_y = cy + 112.0 * ui;
                assets.draw_sprite_centered(
                    "BUTTON_MENU",
                    cx - 125.0 * ui,
                    btn_y,
                    1.05 * ui,
                    WHITE,
                );
                assets.draw_sprite_centered("BUTTON_RESTART", cx, btn_y, 1.05 * ui, WHITE);
                assets.draw_sprite_centered(
                    "BUTTON_NEXTLEVEL",
                    cx + 125.0 * ui,
                    btn_y,
                    1.05 * ui,
                    WHITE,
                );
            }
            ModalState::Defeat => {
                draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.65));
                let cx = sw * 0.5;
                let cy = sh * 0.48;
                assets.draw_sprite_centered("LEVEL_FAILED_BG", cx, cy, 1.35 * ui, WHITE);
                assets.draw_bitmap_text(
                    "FONT_MENU",
                    "LEVEL FAILED!",
                    cx,
                    cy - 34.0 * ui,
                    0.82 * ui,
                    0.5,
                    WHITE,
                );
                let btn_y = cy + 78.0 * ui;
                assets.draw_sprite_centered("BUTTON_MENU", cx - 75.0 * ui, btn_y, 1.05 * ui, WHITE);
                assets.draw_sprite_centered(
                    "BUTTON_RESTART",
                    cx + 75.0 * ui,
                    btn_y,
                    1.05 * ui,
                    WHITE,
                );
            }
            ModalState::None => {}
        }
    }

    fn update_pause_modal(&mut self, save: &mut SaveState, assets: &mut Assets) -> LevelOutcome {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return LevelOutcome::None;
        }
        let mpos: Vec2 = mouse_position().into();
        let sw = screen_width();
        let sh = screen_height();
        let ui = save.gui_scale();
        let cy = sh * 0.50;
        let spacing = 115.0 * ui;
        let r = 42.0 * ui;

        if mpos.distance(vec2(sw * 0.5 - spacing * 1.5, cy)) <= r {
            assets.play_sfx("menu confirm", 0.8);
            self.modal = ModalState::None;
        } else if mpos.distance(vec2(sw * 0.5 - spacing * 0.5, cy)) <= r {
            assets.play_sfx("menu confirm", 0.8);
            return LevelOutcome::RestartLevel;
        } else if mpos.distance(vec2(sw * 0.5 + spacing * 0.5, cy)) <= r {
            assets.play_sfx("menu back", 0.8);
            return LevelOutcome::BackToLevelSelect;
        } else if mpos.distance(vec2(sw * 0.5 + spacing * 1.5, cy)) <= r {
            save.muted = !save.muted;
            save.save();
            let ambient = theme_ambient(&self.sim.theme);
            assets.set_muted(save.muted, Some((ambient, 0.55)));
        }
        LevelOutcome::None
    }

    fn update_end_modal(
        &mut self,
        victory: bool,
        save: &SaveState,
        assets: &Assets,
    ) -> LevelOutcome {
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Enter) {
            assets.play_sfx("menu confirm", 0.8);
            if victory {
                return LevelOutcome::NextLevel(self.sim.level_index + 1);
            } else {
                return LevelOutcome::RestartLevel;
            }
        }
        if !is_mouse_button_pressed(MouseButton::Left) {
            return LevelOutcome::None;
        }
        let mpos: Vec2 = mouse_position().into();
        let cx = screen_width() * 0.5;
        let cy = screen_height() * 0.48;
        let ui = save.gui_scale();
        let r = 46.0 * ui;

        if victory {
            let btn_y = cy + 112.0 * ui;
            if mpos.distance(vec2(cx - 125.0 * ui, btn_y)) <= r {
                assets.play_sfx("menu back", 0.8);
                return LevelOutcome::BackToLevelSelect;
            }
            if mpos.distance(vec2(cx, btn_y)) <= r {
                assets.play_sfx("menu confirm", 0.8);
                return LevelOutcome::RestartLevel;
            }
            if mpos.distance(vec2(cx + 125.0 * ui, btn_y)) <= r {
                assets.play_sfx("menu confirm", 0.8);
                return LevelOutcome::NextLevel(self.sim.level_index + 1);
            }
        } else {
            let btn_y = cy + 78.0 * ui;
            if mpos.distance(vec2(cx - 75.0 * ui, btn_y)) <= r {
                assets.play_sfx("menu back", 0.8);
                return LevelOutcome::BackToLevelSelect;
            }
            if mpos.distance(vec2(cx + 75.0 * ui, btn_y)) <= r {
                assets.play_sfx("menu confirm", 0.8);
                return LevelOutcome::RestartLevel;
            }
        }
        LevelOutcome::None
    }
}
