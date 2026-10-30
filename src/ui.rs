//! Authentic Angry Birds 1.2.0 UI screens: Splash sequence, Parallax Main Menu with flying
//! background birds, interactive Credits (`"i"`) modal with clickable Instagram & Email
//! buttons, the original comic-story cutscenes, and the 5-chapter Level Select screen.

use crate::assets::{open_external_url, safe_clamp, Assets};
use crate::save::{chapter_name, SaveState, CHAPTER_COUNT, LEVELS_PER_CHAPTER};
use macroquad::prelude::*;

const INSTAGRAM_URL: &str = "https://www.instagram.com/shohailmahmud09/";
const EMAIL_ADDRESS: &str = "nirjonpc@gmail.com";
const MAILTO_URL: &str = "mailto:nirjonpc@gmail.com";

#[derive(Clone, Debug)]
struct MenuFlyingBird {
    sprite: String,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    scale: f32,
}

pub struct SplashScreen {
    pub index: usize,
    pub timer: f32,
}

impl SplashScreen {
    pub fn new() -> Self {
        Self {
            index: 0,
            timer: 0.0,
        }
    }

    fn card_duration(&self) -> f32 {
        if self.index == 2 {
            1.75
        } else {
            1.35
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        self.timer += dt;
        let dur = self.card_duration();
        if is_mouse_button_pressed(MouseButton::Left)
            || is_key_pressed(KeyCode::Space)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
        {
            self.index += 1;
            self.timer = 0.0;
        } else if self.timer >= dur {
            self.index += 1;
            self.timer = 0.0;
        }
        self.index >= 3
    }

    pub fn draw(&self, save: &SaveState, assets: &Assets) {
        clear_background(BLACK);
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        let scale = (sh / 320.0).min(sw / 480.0).max(0.1);
        let ui = save.gui_scale();
        let dur = self.card_duration();

        let alpha = if self.timer < 0.25 {
            self.timer / 0.25
        } else if self.timer > dur - 0.25 {
            ((dur - self.timer) / 0.25).max(0.0)
        } else {
            1.0
        };
        let col = Color::new(1.0, 1.0, 1.0, alpha);

        let sp = match self.index {
            0 => "SPLASH_CLICKGAMER",
            1 => "SPLASH_ROVIO",
            _ => "SPLASH_ANGRY_BIRDS",
        };
        if self.index == 1 {
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(1.0, 1.0, 1.0, alpha));
        }
        assets.draw_sprite_centered(sp, sw * 0.5, sh * 0.48, scale * 0.94, col);

        // Authentic cartoon Angry Birds bitmap credit on the start sequence (with generous downside margin)
        let txt_col = if self.index == 1 {
            Color::new(1.0, 0.94, 0.68, alpha)
        } else {
            Color::new(1.0, 0.95, 0.72, alpha)
        };
        assets.draw_bitmap_text(
            "FONT_BASIC",
            "REBUILD BY SHOHAIL MAHMUD",
            sw * 0.5,
            sh - (48.0 * ui).max(34.0),
            1.08 * ui,
            0.5,
            txt_col,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    PlayClicked,
    WatchCutscene,
}

pub struct MainMenu {
    scroll: f32,
    sun_angle: f32,
    birds: Vec<MenuFlyingBird>,
    spawn_timer: f32,
    show_credits: bool,
    link_feedback: Option<(String, f32)>,
}

impl MainMenu {
    pub fn new() -> Self {
        Self {
            scroll: 0.0,
            sun_angle: 0.0,
            birds: Vec::new(),
            spawn_timer: 0.25,
            show_credits: false,
            link_feedback: None,
        }
    }

    /// Play button centre + hit box, shared by `update()` and `draw()`.
    fn play_geometry(sw: f32, sh: f32, ui: f32) -> (Vec2, Rect) {
        let (ms, _, moy) = Self::menu_metrics(sw, sh);
        let cx = sw * 0.5;
        let cy = moy + 168.0 * ms;
        let half_w = (90.0 * 1.08 * ms * 0.62).max(74.0 * ui);
        let half_h = (42.0 * 1.08 * ms * 0.62).max(34.0 * ui);
        (
            vec2(cx, cy),
            Rect::new(cx - half_w, cy - half_h, half_w * 2.0, half_h * 2.0),
        )
    }

    /// Single scale for the credits panel. Derived from the window only, so the panel is as
    /// readable as the window allows (deliberately does NOT inherit the menu's small UI scale).
    fn credits_scale(sw: f32, sh: f32) -> f32 {
        ((sw * 0.92) / 476.0)
            .min((sh * 0.88) / 318.0)
            .clamp(0.40, 1.30)
    }

    pub fn update(&mut self, dt: f32, save: &mut SaveState, assets: &mut Assets) -> MenuAction {
        assets.play_music("title_theme", true, 0.72);
        self.scroll += dt * 42.0;
        self.sun_angle += dt * 0.18;

        if let Some((_, ref mut t)) = self.link_feedback {
            *t -= dt;
            if *t <= 0.0 {
                self.link_feedback = None;
            }
        }

        // Credits panel shortcut (also reachable via the "i" icon)
        if is_key_pressed(KeyCode::I) {
            self.show_credits = !self.show_credits;
            self.link_feedback = None;
            assets.play_sfx("menu select", 0.8);
        }

        // Toggle unlock-all shortcut
        if is_key_pressed(KeyCode::U) {
            save.unlock_all = !save.unlock_all;
            save.save();
            assets.play_sfx("menu confirm", 0.8);
        }
        if is_key_pressed(KeyCode::M) {
            save.muted = !save.muted;
            save.save();
            assets.set_muted(save.muted, Some(("title_theme", 0.72)));
        }

        // Spawn flying birds across the title sky
        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 && self.birds.len() < 8 {
            self.spawn_timer = 0.65 + (macroquad::rand::rand() % 100) as f32 * 0.012;
            let choices = [
                "BIRD_RED_FLYING",
                "BIRD_BLUE",
                "BIRD_YELLOW_FLYING",
                "BIRD_GREY_FLYING",
                "BIRD_GREEN_FLYING",
            ];
            let sp = choices[(macroquad::rand::rand() as usize) % choices.len()].to_string();
            let depth = 0.55 + (macroquad::rand::rand() % 65) as f32 * 0.01;
            let sh = screen_height().max(64.0);
            self.birds.push(MenuFlyingBird {
                sprite: sp,
                x: -60.0,
                y: sh * (0.48 + (macroquad::rand::rand() % 30) as f32 * 0.01),
                vx: (240.0 + (macroquad::rand::rand() % 180) as f32) * depth,
                vy: -(210.0 + (macroquad::rand::rand() % 140) as f32) * depth,
                scale: depth * (sh / 420.0),
            });
        }

        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        for b in &mut self.birds {
            b.vy += 260.0 * dt;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
        }
        self.birds.retain(|b| b.x < sw + 120.0 && b.y < sh + 120.0);

        if self.show_credits {
            if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Space) {
                assets.play_sfx("menu back", 0.8);
                self.show_credits = false;
                self.link_feedback = None;
                return MenuAction::None;
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                let mpos: Vec2 = mouse_position().into();
                // identical geometry to the draw pass (single window-derived scale)
                let u = Self::credits_scale(sw, sh);
                let card_rect = Rect::new(
                    sw * 0.5 - 476.0 * u * 0.5,
                    sh * 0.5 - 318.0 * u * 0.5,
                    476.0 * u,
                    318.0 * u,
                );
                let btn_w = card_rect.w * 0.80;
                let btn_h = card_rect.h * 0.125;
                let insta_rect = Rect::new(
                    card_rect.x + card_rect.w * 0.5 - btn_w * 0.5,
                    card_rect.y + 0.355 * card_rect.h,
                    btn_w,
                    btn_h,
                );
                let mail_rect = Rect::new(
                    card_rect.x + card_rect.w * 0.5 - btn_w * 0.5,
                    card_rect.y + 0.515 * card_rect.h,
                    btn_w,
                    btn_h,
                );
                let close_pos = vec2(card_rect.right() - 14.0 * u, card_rect.y + 14.0 * u);

                if insta_rect.contains(mpos) {
                    assets.play_sfx("menu confirm", 0.9);
                    macroquad::miniquad::window::clipboard_set(INSTAGRAM_URL);
                    open_external_url(INSTAGRAM_URL);
                    self.link_feedback = Some(("OPENING INSTAGRAM...".to_string(), 2.8));
                } else if mail_rect.contains(mpos) {
                    assets.play_sfx("menu confirm", 0.9);
                    macroquad::miniquad::window::clipboard_set(EMAIL_ADDRESS);
                    open_external_url(MAILTO_URL);
                    self.link_feedback = Some(("MAIL OPENED + COPIED!".to_string(), 2.8));
                } else if mpos.distance(close_pos) <= 30.0 * u || !card_rect.contains(mpos) {
                    assets.play_sfx("menu back", 0.8);
                    self.show_credits = false;
                    self.link_feedback = None;
                }
            }
            return MenuAction::None;
        }

        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            assets.play_sfx("menu confirm", 0.85);
            return MenuAction::PlayClicked;
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            let mpos: Vec2 = mouse_position().into();
            let ui = save.gui_scale();
            let bot_y = sh - 62.0 * ui;

            // Play button center
            let (_, play_rect) = Self::play_geometry(sw, sh, ui);
            if play_rect.contains(mpos) {
                assets.play_sfx("menu confirm", 0.9);
                return MenuAction::PlayClicked;
            }

            // Bottom-left Audio toggle
            if mpos.distance(vec2(58.0 * ui, bot_y)) <= 36.0 * ui {
                save.muted = !save.muted;
                save.save();
                assets.set_muted(save.muted, Some(("title_theme", 0.72)));
                return MenuAction::None;
            }

            // Bottom-left Unlock-All circular Angry Birds icon button (next to Audio)
            if mpos.distance(vec2(138.0 * ui, bot_y)) <= 36.0 * ui {
                save.unlock_all = !save.unlock_all;
                save.save();
                assets.play_sfx("menu confirm", 0.85);
                return MenuAction::None;
            }

            // Bottom-right "i" Credits/Info button
            if mpos.distance(vec2(sw - 58.0 * ui, bot_y)) <= 36.0 * ui {
                assets.play_sfx("menu select", 0.85);
                self.show_credits = true;
                self.link_feedback = None;
                return MenuAction::None;
            }

            // Cutscene button next to "i"
            if mpos.distance(vec2(sw - 138.0 * ui, bot_y)) <= 36.0 * ui {
                assets.play_sfx("menu select", 0.85);
                return MenuAction::WatchCutscene;
            }
        }

        MenuAction::None
    }

    /// Design-space scale + letterboxing for the 480x320 main menu composition.
    fn menu_metrics(sw: f32, sh: f32) -> (f32, f32, f32) {
        // Normal windows: cover-fit, which fills the frame with no bars. But for extreme
        // window shapes (very wide/short, e.g. 1133x275) cover-fit cropped most of the menu
        // away, so fall back to contain-fit there and keep every button on screen.
        let cover = (sw / 480.0).max(sh / 320.0);
        let contain = (sw / 480.0).min(sh / 320.0);
        let aspect = sw / sh.max(1.0);
        let s = if aspect > 2.6 || aspect < 1.1 {
            contain
        } else {
            cover
        }
        .max(0.2);
        let ox = (sw - 480.0 * s) * 0.5;
        let oy = (sh - 320.0 * s) * 0.5;
        (s, ox, oy)
    }

    pub fn draw(&self, save: &SaveState, assets: &Assets) {
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        // Compact menu: widgets, logo and title are drawn at 78% so the artwork stays visible
        let ui = save.gui_scale() * 0.62;
        let (ms, mox, moy) = Self::menu_metrics(sw, sh);
        let cs = ms * 0.62;

        // Sky backdrop sampled from the original 1.2.0 menu art (no flat fill gaps)
        clear_background(Color::from_rgba(31, 143, 168, 255));

        // 1) Radial sky + clouds sheet, stretched to cover the full width
        assets.draw_sprite_rect(
            "MENU_BACKGROUND_TOP",
            mox,
            moy,
            480.0 * ms,
            320.0 * ms,
            WHITE,
        );

        // 2) Setting sun, bottom-anchored behind the hills, drifting with parallax
        let sun_w = 254.0 * ms;
        let sun_h = 106.0 * ms;
        let sun_drift = (self.scroll * 0.12).sin() * 14.0 * ms;
        assets.draw_sprite_rect(
            "MENU_SUN",
            mox + (480.0 - 254.0) * 0.5 * ms + sun_drift,
            moy + (320.0 - 106.0) * ms,
            sun_w,
            sun_h,
            WHITE,
        );

        // 3) Flying background birds silhouetted against the sky
        for b in &self.birds {
            let rot = b.vy.atan2(b.vx) * 0.45;
            assets.draw_sprite(&b.sprite, b.x, b.y, b.scale, b.scale, rot, WHITE);
        }

        // 4) Five original parallax hill layers, each bottom-anchored and tiled seamlessly
        let hills = [
            ("MENU_LAYER_5", 0.22f32),
            ("MENU_LAYER_4", 0.38),
            ("MENU_LAYER_3", 0.58),
            ("MENU_LAYER_2", 0.82),
            ("MENU_LAYER_1", 1.15),
        ];
        for (lname, speed) in hills {
            let (lw, lh) = assets.sprite_size(lname);
            let dw = (lw * ms).max(32.0);
            let dh = (lh * ms).max(16.0);
            let y = moy + 320.0 * ms - dh;
            let shift = (self.scroll * speed * ms).rem_euclid(dw);
            let mut x = mox - shift;
            let mut count = 0;
            while x < sw + dw && count < 40 {
                assets.draw_sprite_rect(lname, x, y, dw + 1.0, dh, WHITE);
                x += dw;
                count += 1;
            }
        }

        // Title Logo + Builder Name right below in genuine Angry Birds cartoon bitmap font.
        // MENU_LOGO is authored 389x74 design px; it is drawn at ~340 design px wide.
        let bob = (self.scroll * 0.06).sin() * 5.0 * ui;
        let logo_y = moy + 74.0 * ms + bob;
        let logo_scale = (340.0 / 389.0) * cs;
        assets.draw_sprite_centered("MENU_LOGO", sw * 0.5, logo_y, logo_scale, WHITE);
        assets.draw_bitmap_text(
            "FONT_BASIC",
            "VERSION 1.2.0",
            sw * 0.5,
            logo_y + 34.0 * cs,
            0.82 * cs,
            0.5,
            Color::from_rgba(255, 248, 190, 255),
        );

        let mpos: Vec2 = mouse_position().into();

        // Play Button (with subtle hover pulse) — MENU_PLAY is 90x42 design px
        let (play_center, play_rect) = Self::play_geometry(sw, sh, ui);
        let hovering_play = !self.show_credits && play_rect.contains(mpos);
        let play_scale = (if hovering_play { 1.14 } else { 1.08 }) * cs;
        assets.draw_sprite_centered("MENU_PLAY", play_center.x, play_center.y, play_scale, WHITE);

        // Bottom Circular Angry Birds Controls (lifted well clear of the window bottom edge)
        let bot_y = sh - 78.0 * ui;

        // 1) Sound Toggle (Bottom-Left)
        let sfx_pos = vec2(58.0 * ui, bot_y);
        let sfx_sc = if !self.show_credits && mpos.distance(sfx_pos) <= 34.0 * ui {
            1.10 * ui
        } else {
            1.02 * ui
        };
        assets.draw_sprite_centered("MENU_SFX", sfx_pos.x, sfx_pos.y, sfx_sc, WHITE);
        if save.muted {
            assets.draw_sprite_centered("BUTTON_OFF_SMALL", sfx_pos.x, sfx_pos.y, 1.06 * ui, WHITE);
        }

        // 2) Authentic Angry Birds Unlock-All Icon Button (next to Sound, replacing rectangular pill)
        let unl_pos = vec2(138.0 * ui, bot_y);
        let unl_hover = !self.show_credits && mpos.distance(unl_pos) <= 34.0 * ui;
        let unl_mul = if unl_hover { 1.09 } else { 1.0 };
        if save.unlock_all {
            assets.draw_sprite_centered(
                "MENU_YES",
                unl_pos.x,
                unl_pos.y,
                0.80 * ui * unl_mul,
                WHITE,
            );
            assets.draw_bitmap_text(
                "FONT_BASIC",
                "ALL 105",
                unl_pos.x,
                unl_pos.y - 32.0 * ui,
                0.72 * ui,
                0.5,
                Color::from_rgba(255, 242, 145, 255),
            );
        } else {
            assets.draw_sprite_centered(
                "LS_WORLD_LOCK",
                unl_pos.x,
                unl_pos.y - 2.0 * ui,
                0.92 * ui * unl_mul,
                WHITE,
            );
            assets.draw_bitmap_text(
                "FONT_BASIC",
                "UNLOCK",
                unl_pos.x,
                unl_pos.y - 38.0 * ui,
                0.70 * ui,
                0.5,
                WHITE,
            );
        }

        // 3) Cutscene Button (Bottom-Right inner)
        let cut_pos = vec2(sw - 138.0 * ui, bot_y);
        let cut_sc = if !self.show_credits && mpos.distance(cut_pos) <= 34.0 * ui {
            0.86 * ui
        } else {
            0.78 * ui
        };
        assets.draw_sprite_centered("MENU_CUTSCENE", cut_pos.x, cut_pos.y, cut_sc, WHITE);

        // 4) "i" Credits & Links Button (Bottom-Right corner)
        let info_pos = vec2(sw - 58.0 * ui, bot_y);
        let info_sc = if !self.show_credits && mpos.distance(info_pos) <= 34.0 * ui {
            1.12 * ui
        } else {
            1.02 * ui
        };
        assets.draw_sprite_centered("MENU_CREDITS", info_pos.x, info_pos.y, info_sc, WHITE);

        // Authentic Angry Birds 1.0 "i" Credits Dialog with Clickable Instagram & Email Buttons
        if self.show_credits {
            // ---- Credits panel ----------------------------------------------------------
            // One scale drives the card AND its contents, derived from the window so the
            // panel is as large as it can be (the menu's own 62% shrink is NOT applied here,
            // otherwise the panel became unreadably small).
            let u = Self::credits_scale(sw, sh);
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.68));

            let card_w = 476.0 * u;
            let card_h = 318.0 * u;
            let card = Rect::new(
                sw * 0.5 - card_w * 0.5,
                sh * 0.5 - card_h * 0.5,
                card_w,
                card_h,
            );
            let cx = card.x + card.w * 0.5;

            // Text scale that always fits inside the frame's inner width.
            let inner_w = card.w * 0.78;
            let fit = |txt: &str, base: f32| -> f32 {
                let w = assets.measure_bitmap_text("FONT_BASIC", txt, 1.0);
                if w > 0.5 {
                    base.min(inner_w / w)
                } else {
                    base
                }
            };

            // Perched trio of birds above the frame
            assets.draw_sprite_centered("ABOUT_BIRDS_1", cx, card.y - 20.0 * u, 0.92 * u, WHITE);
            assets.draw_sprite_rect("LEVEL_FINISH_BG", card.x, card.y, card.w, card.h, WHITE);

            // Close button (top-right, on the frame's corner)
            let close_pos = vec2(card.right() - 14.0 * u, card.y + 14.0 * u);
            let close_hover = mpos.distance(close_pos) <= 30.0 * u;
            assets.draw_sprite_centered(
                "MENU_NO",
                close_pos.x,
                close_pos.y,
                if close_hover { 0.68 * u } else { 0.60 * u },
                WHITE,
            );

            // Header
            assets.draw_bitmap_text(
                "FONT_MENU",
                "ANGRY BIRDS 1.2.0",
                cx,
                card.y + 0.185 * card.h,
                0.62 * u,
                0.5,
                WHITE,
            );
            let rebuild = "REBUILD BY SHOHAIL MAHMUD";
            assets.draw_bitmap_text(
                "FONT_BASIC",
                rebuild,
                cx,
                card.y + 0.275 * card.h,
                fit(rebuild, 0.92 * u),
                0.5,
                Color::from_rgba(255, 240, 150, 255),
            );

            // Buttons
            let btn_w = card.w * 0.80;
            let btn_h = card.h * 0.125;
            let insta_rect = Rect::new(cx - btn_w * 0.5, card.y + 0.355 * card.h, btn_w, btn_h);
            let mail_rect = Rect::new(cx - btn_w * 0.5, card.y + 0.515 * card.h, btn_w, btn_h);

            let insta_hover = insta_rect.contains(mpos);
            let mail_hover = mail_rect.contains(mpos);

            for (rect, label, hovered) in [
                (insta_rect, "INSTAGRAM: @shohailmahmud09", insta_hover),
                (mail_rect, "MAIL: nirjonpc@gmail.com", mail_hover),
            ] {
                let grow = if hovered { 5.0 * u } else { 0.0 };
                assets.draw_sprite_rect(
                    "LEVEL_FAILED_BG",
                    rect.x - grow,
                    rect.y - grow * 0.4,
                    rect.w + grow * 2.0,
                    rect.h + grow * 0.8,
                    if hovered {
                        Color::from_rgba(255, 250, 210, 255)
                    } else {
                        Color::from_rgba(235, 248, 255, 255)
                    },
                );
                assets.draw_sprite_centered(
                    "PERFECT_STAR",
                    rect.x + rect.h * 0.42,
                    rect.y + rect.h * 0.5,
                    0.34 * u,
                    WHITE,
                );
                assets.draw_bitmap_text(
                    "FONT_BASIC",
                    label,
                    cx + rect.h * 0.15,
                    rect.y + rect.h * 0.72,
                    fit(label, 0.86 * u),
                    0.5,
                    if hovered {
                        Color::from_rgba(255, 242, 140, 255)
                    } else {
                        WHITE
                    },
                );
            }

            // Hint / feedback line
            let status_txt = if let Some((ref msg, _)) = self.link_feedback {
                msg.as_str()
            } else if insta_hover {
                "CLICK TO OPEN INSTAGRAM IN BROWSER!"
            } else if mail_hover {
                "CLICK TO SEND EMAIL (ALSO COPIES ADDRESS)!"
            } else {
                "CLICK BUTTONS ABOVE TO OPEN INSTAGRAM OR MAIL"
            };
            let status_col = if self.link_feedback.is_some() || insta_hover || mail_hover {
                Color::from_rgba(255, 242, 140, 255)
            } else {
                Color::from_rgba(225, 248, 255, 245)
            };
            assets.draw_bitmap_text(
                "FONT_BASIC",
                status_txt,
                cx,
                card.y + 0.700 * card.h,
                fit(status_txt, 0.74 * u),
                0.5,
                status_col,
            );

            let controls = "DRAG: AIM   SPACE: ABILITY   WHEEL: ZOOM   [ / ]: UI";
            assets.draw_bitmap_text(
                "FONT_BASIC",
                controls,
                cx,
                card.y + 0.800 * card.h,
                fit(controls, 0.68 * u),
                0.5,
                WHITE,
            );

            let credit = "ORIGINAL 2010 GAME BY ROVIO";
            assets.draw_bitmap_text(
                "FONT_BASIC",
                credit,
                cx,
                card.y + 0.900 * card.h,
                fit(credit, 0.68 * u),
                0.5,
                Color::from_rgba(255, 240, 150, 235),
            );
        }
    }
}

/// A single 1.2.0 comic story board: a wide background strip plus the original
/// element overlays that Rovio composites on top of it (`gamelogic.lua` story tables).
pub struct CutscenePlayer {
    pub scene: String,
    pub background: String,
    pub elements: Vec<&'static str>,
    pub progress: f32,
    pub duration: f32,
    pub next_level_after: Option<usize>,
}

impl CutscenePlayer {
    pub fn new(scene: &str, next_level_after: Option<usize>, assets: &mut Assets) -> Self {
        let (background, elements, track, duration): (
            &'static str,
            Vec<&'static str>,
            &'static str,
            f32,
        ) = match scene {
            // Intro: "gameStart" story board
            "gameStart" | "STORY_BEGINNING" => (
                "STORY_BEGIN_BG",
                vec!["STORY_BEGIN_PIG_GROUP_1"],
                "birds_intro",
                7.5,
            ),
            "theme1Complete" | "STORY_BOSS_1" => (
                "STORY_BOSS_BG",
                vec!["STORY_BOSS_THEME_1", "STORY_BOSS_KING_ESCAPING"],
                "birds_boss",
                6.0,
            ),
            "theme2Complete" | "STORY_BOSS_2" => (
                "STORY_BOSS_BG",
                vec!["STORY_BOSS_THEME_2", "STORY_BOSS_KING_ESCAPING"],
                "birds_boss",
                6.0,
            ),
            "theme3Complete" | "STORY_END_1" => (
                "STORY_END_1",
                vec![
                    "STORY_END_KING_EYE_NORMAL",
                    "STORY_END_KING_EYE_PEEK",
                    "STORY_END_KING_EYE_WINK",
                ],
                "birds_outro",
                7.0,
            ),
            "theme4Complete" => (
                "STORY_BOSS_BG",
                vec!["STORY_BOSS_THEME_4", "STORY_BOSS_KING_ESCAPING"],
                "birds_boss",
                6.0,
            ),
            "theme5Complete" => (
                "STORY_BEGIN_BG",
                vec!["STORY_BEGIN_FAKE_EGGS", "STORY_BEGIN_PIG_GROUP_2"],
                "birds_outro",
                8.0,
            ),
            _ => (
                "STORY_BEGIN_BG",
                vec!["STORY_BEGIN_PIG_GROUP_1"],
                "birds_intro",
                6.0,
            ),
        };
        assets.play_music(track, false, 0.80);
        assets.play_sfx("piglette oink story", 0.75);
        Self {
            scene: scene.to_string(),
            background: background.to_string(),
            elements,
            progress: 0.0,
            duration,
            next_level_after,
        }
    }

    pub fn update(&mut self, dt: f32, save: &SaveState, assets: &Assets) -> bool {
        self.progress += dt;
        if is_key_pressed(KeyCode::Space)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Escape)
        {
            assets.play_sfx("menu confirm", 0.8);
            return true;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let mpos: Vec2 = mouse_position().into();
            let sw = screen_width().max(64.0);
            let sh = screen_height().max(64.0);
            let ui = save.gui_scale();
            if mpos.distance(vec2(sw - 60.0 * ui, sh - 60.0 * ui)) <= 44.0 * ui
                || self.progress > 1.2
            {
                assets.play_sfx("menu confirm", 0.8);
                return true;
            }
        }
        self.progress >= self.duration + 1.0
    }

    pub fn draw(&self, save: &SaveState, assets: &Assets) {
        clear_background(Color::from_rgba(22, 18, 16, 255));
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        let ui = save.gui_scale();

        // 1.2.0 story boards are wide 996x320 comic strips that scroll horizontally
        let (cw, ch) = assets.sprite_size(&self.background);
        let scale = ((sh * 0.86) / ch.max(1.0)).max(0.2);
        let draw_h = ch * scale;
        let strip_w = cw * scale;
        let y = (sh - draw_h) * 0.5;

        let t = safe_clamp(self.progress / self.duration.max(0.1), 0.0, 1.0);
        let travel = (strip_w - sw).max(0.0);
        let x = -(travel * t);

        assets.draw_sprite_rect(&self.background, x, y, strip_w, draw_h, WHITE);
        // Story elements are authored at (0,0) with negative pivots -> they land on the strip
        for name in &self.elements {
            assets.draw_sprite(name, x, y, scale, scale, 0.0, WHITE);
        }

        // Chapter label for this story board (bottom-left, authentic bitmap font)
        let story_label = match self.scene.as_str() {
            "gameStart" | "STORY_BEGINNING" => "THE PIGS STOLE THE EGGS...",
            "theme5Complete" => "EPISODE 5 COMPLETE!",
            "theme4Complete" => "EPISODE 4 COMPLETE!",
            "theme3Complete" | "STORY_END_1" => "EPISODE 3 COMPLETE!",
            "theme2Complete" | "STORY_BOSS_2" => "EPISODE 2 COMPLETE!",
            "theme1Complete" | "STORY_BOSS_1" => "EPISODE 1 COMPLETE!",
            _ => "",
        };
        if !story_label.is_empty() {
            assets.draw_bitmap_text(
                "FONT_BASIC",
                story_label,
                18.0 * ui,
                sh - 22.0 * ui,
                0.78 * ui,
                0.0,
                Color::from_rgba(255, 248, 190, 255),
            );
        }

        // Tap-to-continue prompt (authentic cartoon "OK" badge)
        let prompt = if self.progress < 1.0 {
            "CLICK TO SKIP"
        } else {
            "CLICK TO CONTINUE"
        };
        assets.draw_sprite_centered(
            "TUTORIAL_OK",
            sw - 60.0 * ui,
            sh - 60.0 * ui,
            1.02 * ui,
            WHITE,
        );
        assets.draw_bitmap_text(
            "FONT_BASIC",
            prompt,
            sw - 60.0 * ui,
            sh - 100.0 * ui,
            0.72 * ui,
            0.5,
            WHITE,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LevelSelectAction {
    None,
    BackToMenu,
    StartLevel(usize),
}

/// Authentic 1.2.0 Level Select: 5 episode tabs across the top (LS_THEME_1..5),
/// the original 7x3 stage grid underneath, and a lock badge on unfinished episodes.
pub struct LevelSelectScreen {
    pub selected_chapter: usize, // 1..=5
}

impl LevelSelectScreen {
    pub fn new(initial_chapter: usize) -> Self {
        Self {
            selected_chapter: initial_chapter.clamp(1, CHAPTER_COUNT),
        }
    }

    fn tab_x(sw: f32, ui: f32, chapter: usize) -> f32 {
        let spacing = (sw * 0.155).clamp(64.0 * ui, 128.0 * ui);
        sw * 0.5 + ((chapter as f32) - 3.0) * spacing
    }

    fn tab_y(sh: f32, ui: f32) -> f32 {
        (sh * 0.20).max(96.0 * ui)
    }

    fn grid_geometry(sw: f32, sh: f32, ui: f32) -> (f32, f32, f32, f32) {
        let grid_w = (sw * 0.82).min(820.0 * ui);
        let start_x = (sw - grid_w) * 0.5 + grid_w / 14.0;
        let col_step = grid_w / 7.0;
        let center_y = sh * 0.60;
        let row_step = 92.0 * ui;
        (start_x, col_step, center_y, row_step)
    }

    pub fn update(&mut self, save: &mut SaveState, assets: &mut Assets) -> LevelSelectAction {
        assets.play_music("title_theme", true, 0.72);

        if is_key_pressed(KeyCode::Escape) {
            assets.play_sfx("menu back", 0.8);
            return LevelSelectAction::BackToMenu;
        }
        if is_key_pressed(KeyCode::U) {
            save.unlock_all = !save.unlock_all;
            save.save();
            assets.play_sfx("menu confirm", 0.8);
        }
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
            if self.selected_chapter > 1 && save.is_chapter_unlocked(self.selected_chapter - 1) {
                self.selected_chapter -= 1;
                assets.play_sfx("menu select", 0.7);
            }
        }
        if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
            if self.selected_chapter < CHAPTER_COUNT
                && save.is_chapter_unlocked(self.selected_chapter + 1)
            {
                self.selected_chapter += 1;
                assets.play_sfx("menu select", 0.7);
            }
        }

        if !is_mouse_button_pressed(MouseButton::Left) {
            return LevelSelectAction::None;
        }

        let mpos: Vec2 = mouse_position().into();
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        let ui = save.gui_scale();

        // Back button bottom-left (with generous downside margin)
        if mpos.distance(vec2(58.0 * ui, sh - 60.0 * ui)) <= 40.0 * ui {
            assets.play_sfx("menu back", 0.8);
            return LevelSelectAction::BackToMenu;
        }

        // Unlock-All icon button bottom-right
        if mpos.distance(vec2(sw - 58.0 * ui, sh - 60.0 * ui)) <= 40.0 * ui {
            save.unlock_all = !save.unlock_all;
            save.save();
            assets.play_sfx("menu confirm", 0.85);
            return LevelSelectAction::None;
        }

        // Episode tabs across the top
        let tab_y = Self::tab_y(sh, ui);
        for ch in 1..=CHAPTER_COUNT {
            let tx = Self::tab_x(sw, ui, ch);
            let rect = Rect::new(tx - 52.0 * ui, tab_y - 40.0 * ui, 104.0 * ui, 80.0 * ui);
            if rect.contains(mpos) {
                if save.is_chapter_unlocked(ch) {
                    assets.play_sfx("menu select", 0.8);
                    self.selected_chapter = ch;
                } else {
                    assets.play_sfx("menu back", 0.7);
                }
                return LevelSelectAction::None;
            }
        }

        // 7x3 stage grid
        let (start_x, col_step, center_y, row_step) = Self::grid_geometry(sw, sh, ui);
        for stage in 1..=LEVELS_PER_CHAPTER {
            let row = (stage - 1) / 7;
            let col = (stage - 1) % 7;
            let bx = start_x + (col as f32) * col_step;
            let by = center_y + ((row as f32) - 1.0) * row_step;
            let global_idx = (self.selected_chapter - 1) * LEVELS_PER_CHAPTER + stage;

            if mpos.distance(vec2(bx, by)) <= 32.0 * ui {
                if save.is_level_unlocked(global_idx) {
                    assets.play_sfx("menu confirm", 0.9);
                    return LevelSelectAction::StartLevel(global_idx);
                } else {
                    assets.play_sfx("menu back", 0.7);
                }
                return LevelSelectAction::None;
            }
        }

        LevelSelectAction::None
    }

    pub fn draw(&self, save: &SaveState, assets: &Assets) {
        let sw = screen_width().max(64.0);
        let sh = screen_height().max(64.0);
        let bg = safe_clamp(sh / 320.0, 0.55, 3.0);
        let ui = save.gui_scale();

        clear_background(Color::from_rgba(95, 165, 215, 255));
        assets.draw_sprite_rect("LS_BACKGROUND", 0.0, 0.0, sw, sh, WHITE);

        // Decorative episode scenery on the left and right (original 1.2.0 artwork)
        let left_sp = format!("LS_THEME_{}_LEFT", self.selected_chapter.min(3));
        let right_sp = format!("LS_THEME_{}_RIGHT", self.selected_chapter);
        let (lw, lh) = assets.sprite_size(&left_sp);
        assets.draw_sprite_rect(&left_sp, 0.0, sh - lh * bg, lw * bg, lh * bg, WHITE);
        let (rw, rh) = assets.sprite_size(&right_sp);
        assets.draw_sprite_rect(
            &right_sp,
            sw - rw * bg,
            sh - rh * bg,
            rw * bg,
            rh * bg,
            WHITE,
        );

        // "SELECT EPISODE" banner, safely below the top edge
        let (ew, eh) = assets.sprite_size("LS_SELECT_EPISODE");
        let eb = 1.05 * ui;
        let banner_y = (Self::tab_y(sh, ui) - 96.0 * ui).max(6.0 * ui);
        assets.draw_sprite_rect(
            "LS_SELECT_EPISODE",
            sw * 0.5 - ew * eb * 0.5,
            banner_y,
            ew * eb,
            eh * eb,
            WHITE,
        );

        // Episode tabs 1..5 with original selected/deselected artwork + lock badges
        let tab_y = Self::tab_y(sh, ui);
        for ch in 1..=CHAPTER_COUNT {
            let tx = Self::tab_x(sw, ui, ch);
            let sp = if self.selected_chapter == ch {
                format!("LS_THEME_{}_SELECTED", ch)
            } else {
                format!("LS_THEME_{}_DESELECTED", ch)
            };
            let sc = if self.selected_chapter == ch {
                1.02 * ui
            } else {
                0.90 * ui
            };
            assets.draw_sprite_centered(&sp, tx, tab_y, sc, WHITE);

            if !save.is_chapter_unlocked(ch) {
                assets.draw_sprite_centered(
                    "LS_WORLD_LOCK",
                    tx,
                    tab_y + 2.0 * ui,
                    0.78 * ui,
                    WHITE,
                );
            }
        }

        let mpos: Vec2 = mouse_position().into();
        let (start_x, col_step, center_y, row_step) = Self::grid_geometry(sw, sh, ui);

        for stage in 1..=LEVELS_PER_CHAPTER {
            let row = (stage - 1) / 7;
            let col = (stage - 1) % 7;
            let bx = start_x + (col as f32) * col_step;
            let by = center_y + ((row as f32) - 1.0) * row_step;
            let global_idx = (self.selected_chapter - 1) * LEVELS_PER_CHAPTER + stage;
            let unlocked = save.is_level_unlocked(global_idx);
            let is_boss = stage == LEVELS_PER_CHAPTER;

            let hover = unlocked && mpos.distance(vec2(bx, by)) <= 32.0 * ui;
            let tile_scale = if hover { 1.16 * ui } else { 1.04 * ui };

            if unlocked {
                let open_sp = if is_boss {
                    format!("LS_LEVEL_BG_BOSS_OPEN_{}", self.selected_chapter)
                } else {
                    format!("LS_LEVEL_BG_NORMAL_OPEN_{}", self.selected_chapter)
                };
                assets.draw_sprite_centered(&open_sp, bx, by, tile_scale, WHITE);
                assets.draw_bitmap_text(
                    "FONT_MENU",
                    &stage.to_string(),
                    bx,
                    by + 7.0 * ui,
                    0.52 * ui,
                    0.5,
                    WHITE,
                );

                let st = save.stars.get(&global_idx).copied().unwrap_or(0);
                let star_sp = match st {
                    3 => "STARS_3",
                    2 => "STARS_2",
                    1 => "STARS_1",
                    _ => "STARS_0",
                };
                assets.draw_sprite_centered(star_sp, bx, by + 24.0 * ui, 1.0 * ui, WHITE);
            } else {
                let closed_sp = if is_boss {
                    "LS_LEVEL_BG_BOSS_CLOSED"
                } else {
                    "LS_LEVEL_BG_NORMAL_CLOSED"
                };
                assets.draw_sprite_centered(closed_sp, bx, by, tile_scale, WHITE);
            }
        }

        // Episode name + star tally under the *selected* tab (authentic cartoon bitmap font)
        let label_x = Self::tab_x(sw, ui, self.selected_chapter).clamp(120.0 * ui, sw - 120.0 * ui);
        assets.draw_bitmap_text(
            "FONT_BASIC",
            chapter_name(self.selected_chapter),
            label_x,
            Self::tab_y(sh, ui) + 46.0 * ui,
            0.92 * ui,
            0.5,
            Color::from_rgba(255, 244, 170, 255),
        );
        // Star tally for the current episode, tucked under the tabs on the left
        let (earned, total) = save.chapter_stars(self.selected_chapter);
        assets.draw_sprite_centered(
            "LS_STAR_GOLD",
            26.0 * ui,
            tab_y + 62.0 * ui,
            1.15 * ui,
            WHITE,
        );
        assets.draw_bitmap_text(
            "FONT_BASIC",
            &format!("{}/{}", earned, total),
            44.0 * ui,
            tab_y + 66.0 * ui,
            0.78 * ui,
            0.0,
            WHITE,
        );

        // Bottom-left Back Button & Bottom-right Unlock Icon (with generous downside space)
        let bot_y = sh - 60.0 * ui;
        assets.draw_sprite_centered("LS_BACK_BUTTON", 58.0 * ui, bot_y, 0.92 * ui, WHITE);

        let unl_pos = vec2(sw - 58.0 * ui, bot_y);
        let unl_hover = mpos.distance(unl_pos) <= 34.0 * ui;
        let unl_mul = if unl_hover { 1.09 } else { 1.0 };
        if save.unlock_all {
            assets.draw_sprite_centered(
                "MENU_YES",
                unl_pos.x,
                unl_pos.y,
                0.70 * ui * unl_mul,
                WHITE,
            );
        } else {
            assets.draw_sprite_centered(
                "LS_WORLD_LOCK",
                unl_pos.x,
                unl_pos.y - 2.0 * ui,
                0.84 * ui * unl_mul,
                WHITE,
            );
        }
    }
}
