#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Angry Birds 1.2.0 (Feb 2010) — Native Rust Rewrite for Windows.
//! Directly loads the original `1.2.0.ipa` textures (PNG + PVR/RGBA4444), `.dat` sprite
//! sheets, bitmap fonts, WAV/MP3 audio, and all 105 levels across 5 chapters.

mod assets;
mod game;
mod gamedata;
mod physics;
mod pvr;
mod save;
mod ui;

use assets::{draw_ui_text, measure_ui_text, safe_clamp, Assets};
use game::{GameSession, LevelOutcome};
use gamedata::GameData;
use macroquad::prelude::*;
use save::SaveState;
use ui::{
    CutscenePlayer, LevelSelectAction, LevelSelectScreen, MainMenu, MenuAction, SplashScreen,
};

fn window_conf() -> Conf {
    Conf {
        window_title: "Angry Birds 1.2.0 - Rebuild by @shohailmahmud09".to_owned(),
        window_width: 1280,
        window_height: 720,
        window_resizable: true,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

enum AppScreen {
    Splash(SplashScreen),
    MainMenu(MainMenu),
    Cutscene(CutscenePlayer),
    LevelSelect(LevelSelectScreen),
    InGame(Box<GameSession>),
}

/// Writes a small log next to the save file so a machine that "doesn't open" the game
/// leaves a trace we can read (%APPDATA%\AngryBirds1_2_Rust\run.log).
fn log_line(msg: &str) {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = std::path::PathBuf::from(appdata).join("AngryBirds1_2_Rust");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("run.log");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{}", msg);
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    log_line(&format!(
        "=== start build r9 | {} ===\narg0={}",
        std::env::consts::OS,
        std::env::args().next().unwrap_or_default()
    ));
    std::panic::set_hook(Box::new(|info| {
        log_line(&format!("PANIC: {}", info));
    }));

    // Draw immediate loading frame while decoding 1.0.ipa textures & audio
    clear_background(Color::from_rgba(18, 22, 28, 255));
    let msg = "Loading Angry Birds 1.2.0 assets...";
    let mtw = measure_ui_text(msg, 28.0);
    draw_ui_text(
        msg,
        (screen_width().max(64.0) - mtw) * 0.5,
        screen_height().max(64.0) * 0.5,
        28.0,
        WHITE,
    );
    next_frame().await;

    let mut save = SaveState::load();
    let gdata = GameData::load_embedded();
    let mut assets = match Assets::load(save.muted).await {
        Ok(a) => a,
        Err(err) => loop {
            clear_background(DARKGRAY);
            draw_ui_text(&format!("Asset Load Error: {}", err), 40.0, 80.0, 24.0, RED);
            next_frame().await;
        },
    };

    let mut fullscreen = false;
    let mut ui_toast_timer = 0.0f32;
    let mut screen = AppScreen::Splash(SplashScreen::new());

    loop {
        // Safely idle when window is minimized or shrunk to near-zero client area
        if screen_width() < 24.0 || screen_height() < 24.0 {
            next_frame().await;
            continue;
        }

        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }

        let dt = safe_clamp(get_frame_time(), 0.0, 0.05);

        // Live GUI size adjustment via `[` (smaller) and `]` (larger)
        if is_key_pressed(KeyCode::LeftBracket) {
            save.ui_scale = safe_clamp(save.ui_scale - 0.08, 0.50, 1.60);
            save.save();
            ui_toast_timer = 1.4;
        }
        if is_key_pressed(KeyCode::RightBracket) {
            save.ui_scale = safe_clamp(save.ui_scale + 0.08, 0.50, 1.60);
            save.save();
            ui_toast_timer = 1.4;
        }
        if ui_toast_timer > 0.0 {
            ui_toast_timer = (ui_toast_timer - dt).max(0.0);
        }

        let mut next_screen: Option<AppScreen> = None;

        match &mut screen {
            AppScreen::Splash(sp) => {
                if sp.update(dt) {
                    next_screen = Some(AppScreen::MainMenu(MainMenu::new()));
                }
                sp.draw(&save, &assets);
            }
            AppScreen::MainMenu(menu) => {
                match menu.update(dt, &mut save, &mut assets) {
                    MenuAction::None => {}
                    MenuAction::PlayClicked => {
                        if !save.seen_intro {
                            save.seen_intro = true;
                            save.save();
                            next_screen = Some(AppScreen::Cutscene(CutscenePlayer::new(
                                "gameStart",
                                None,
                                &mut assets,
                            )));
                        } else {
                            let ch = save.current_chapter();
                            next_screen = Some(AppScreen::LevelSelect(LevelSelectScreen::new(ch)));
                        }
                    }
                    MenuAction::WatchCutscene => {
                        next_screen = Some(AppScreen::Cutscene(CutscenePlayer::new(
                            "gameStart",
                            None,
                            &mut assets,
                        )));
                    }
                }
                menu.draw(&save, &assets);
            }
            AppScreen::Cutscene(cs) => {
                if cs.update(dt, &save, &assets) {
                    if let Some(lvl_idx) = cs.next_level_after {
                        if lvl_idx <= save::TOTAL_LEVELS {
                            let lvl = &gdata.levels[lvl_idx - 1];
                            next_screen = Some(AppScreen::InGame(Box::new(GameSession::new(
                                lvl,
                                &gdata,
                                &mut save,
                                &mut assets,
                            ))));
                        } else {
                            next_screen = Some(AppScreen::LevelSelect(LevelSelectScreen::new(
                                save::CHAPTER_COUNT,
                            )));
                        }
                    } else {
                        let ch = save.current_chapter();
                        next_screen = Some(AppScreen::LevelSelect(LevelSelectScreen::new(ch)));
                    }
                }
                cs.draw(&save, &assets);
            }
            AppScreen::LevelSelect(ls) => {
                match ls.update(&mut save, &mut assets) {
                    LevelSelectAction::None => {}
                    LevelSelectAction::BackToMenu => {
                        next_screen = Some(AppScreen::MainMenu(MainMenu::new()));
                    }
                    LevelSelectAction::StartLevel(idx) => {
                        if let Some(lvl) = gdata.levels.get(idx - 1) {
                            next_screen = Some(AppScreen::InGame(Box::new(GameSession::new(
                                lvl,
                                &gdata,
                                &mut save,
                                &mut assets,
                            ))));
                        }
                    }
                }
                ls.draw(&save, &assets);
            }
            AppScreen::InGame(session) => {
                let cur_idx = session.sim.level_index;
                let cur_ch = session.chapter;
                match session.update(dt, &gdata, &mut save, &mut assets) {
                    LevelOutcome::None => {}
                    LevelOutcome::BackToLevelSelect => {
                        next_screen = Some(AppScreen::LevelSelect(LevelSelectScreen::new(cur_ch)));
                    }
                    LevelOutcome::RestartLevel => {
                        let lvl = &gdata.levels[cur_idx - 1];
                        next_screen = Some(AppScreen::InGame(Box::new(GameSession::new(
                            lvl,
                            &gdata,
                            &mut save,
                            &mut assets,
                        ))));
                    }
                    LevelOutcome::NextLevel(next_idx) => {
                        // 1.2.0 story boards after the 21st stage of every chapter
                        let story: Option<(&str, Option<usize>)> = match cur_idx {
                            21 => Some(("theme1Complete", Some(22))),
                            42 => Some(("theme2Complete", Some(43))),
                            63 => Some(("theme3Complete", Some(64))),
                            84 => Some(("theme4Complete", Some(85))),
                            105 => Some(("theme5Complete", None)),
                            _ => None,
                        };
                        if let Some((scene, after)) = story {
                            next_screen = Some(AppScreen::Cutscene(CutscenePlayer::new(
                                scene,
                                after,
                                &mut assets,
                            )));
                        } else if let Some(lvl) = gdata.levels.get(next_idx - 1) {
                            next_screen = Some(AppScreen::InGame(Box::new(GameSession::new(
                                lvl,
                                &gdata,
                                &mut save,
                                &mut assets,
                            ))));
                        } else {
                            next_screen =
                                Some(AppScreen::LevelSelect(LevelSelectScreen::new(cur_ch)));
                        }
                    }
                }
                session.draw(&gdata, &save, &assets);
            }
        }

        if let Some(ns) = next_screen {
            screen = ns;
        }

        // Visible build stamp (bottom centre) so the running build can be identified at a glance
        set_default_camera();
        let stamp = "BUILD 2026-10-29-r9";
        let ssize = 14.0 * save.gui_scale();
        let swid = measure_ui_text(stamp, ssize);
        draw_ui_text(
            stamp,
            (screen_width().max(64.0) - swid) * 0.5,
            screen_height().max(64.0) - 6.0,
            ssize,
            Color::new(1.0, 1.0, 1.0, 0.45),
        );

        if ui_toast_timer > 0.0 {
            set_default_camera();
            let alpha = safe_clamp(ui_toast_timer * 2.5, 0.0, 0.96);
            let sw = screen_width().max(64.0);
            let sh = screen_height().max(64.0);
            let ui = save.gui_scale();
            let msg = format!(
                "GUI SIZE: {}%   [ - / ] +",
                (save.ui_scale * 100.0).round() as i32
            );
            let pill_w = 290.0 * ui;
            let pill_h = 40.0 * ui;
            let py = sh - 66.0 * ui;
            assets.draw_sprite_rect(
                "LEVEL_FAILED_BG",
                sw * 0.5 - pill_w * 0.5,
                py - pill_h * 0.5,
                pill_w,
                pill_h,
                Color::new(1.0, 1.0, 1.0, alpha),
            );
            assets.draw_bitmap_text(
                "FONT_BASIC",
                &msg,
                sw * 0.5,
                py + 4.0 * ui,
                0.86 * ui,
                0.5,
                Color::new(1.0, 0.96, 0.72, alpha),
            );
        }

        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::gamedata::GameData;
    use super::physics::LevelSimulation;
    use macroquad::math::vec2;

    #[test]
    fn test_all_105_levels_load_and_level1_playable() {
        let gdata = GameData::load_embedded();
        assert_eq!(
            gdata.levels.len(),
            105,
            "Must contain all 105 levels of Angry Birds 1.2.0"
        );
        // 5 chapters x 21 stages, one distinct theme per chapter
        for lvl in &gdata.levels {
            assert_eq!(
                lvl.chapter,
                (lvl.index - 1) / 21 + 1,
                "Level {} has an unexpected chapter",
                lvl.id
            );
            let expected_theme = format!("theme{}", lvl.chapter);
            assert_eq!(
                lvl.theme, expected_theme,
                "Level {} ({}) should use {}",
                lvl.index, lvl.id, expected_theme
            );
        }

        // Verify all 105 levels load AND stand stable without premature pig deaths on idle settle
        for lvl in &gdata.levels {
            let mut sim = LevelSimulation::new(lvl, &gdata);
            let init_pigs = sim.remaining_pigs();
            assert!(
                init_pigs >= 1,
                "Level {} ({}) must have at least 1 pig",
                lvl.index,
                lvl.id
            );
            assert!(
                sim.unused_birds_count() >= 1,
                "Level {} ({}) must have at least 1 bird",
                lvl.index,
                lvl.id
            );
            for _ in 0..90 {
                sim.step(1.0 / 60.0, &gdata);
            }
            assert!(
                sim.remaining_pigs() >= 1,
                "Level {} ({}) lost all pigs while idle! (init={})",
                lvl.index,
                lvl.id,
                init_pigs
            );

            // Verify BOTH slingshot bird head top clearance AND downside ground space across ALL 105 levels!
            let r1 = sim
                .active_slingshot_bird
                .as_ref()
                .map(|b| b.radius)
                .unwrap_or(1.0);
            let bird_top_wy = sim.slingshot_pos.y * 20.0 - r1 * 20.0 - 30.0;
            let castle_min_wy = lvl
                .objects
                .iter()
                .map(|o| o.y * 20.0 - 60.0)
                .fold(-200.0f32, f32::min);
            let level_max_wy = lvl
                .objects
                .iter()
                .map(|o| o.y * 20.0 + 35.0)
                .fold(0.0f32, f32::max);
            let desired_top = (bird_top_wy - 130.0).min(castle_min_wy * 0.72).min(-400.0);
            let desired_bot_world = (level_max_wy + 115.0).max(165.0);
            let needed_vis_h = ((-desired_top) / 0.73)
                .max((desired_bot_world - desired_top) * 1.04)
                .max(580.0);
            let default_zoom = super::assets::safe_clamp(480.0 / needed_vis_h, 0.24, 0.84);
            let vis_h = 480.0 / default_zoom;
            let downside_bot_wy = (level_max_wy + 115.0).max(165.0).max(vis_h * 0.25);
            let focus_y = (downside_bot_wy - vis_h * 0.5).min((bird_top_wy - 95.0) + vis_h * 0.5);
            let screen_top_wy = focus_y - vis_h * 0.5;
            let screen_bot_wy = focus_y + vis_h * 0.5;
            assert!(
                bird_top_wy - screen_top_wy >= 94.0,
                "Level {} ({}) cut off bird head! bird_top={}, screen_top={}",
                lvl.index,
                lvl.id,
                bird_top_wy,
                screen_top_wy
            );
            assert!(
                screen_bot_wy >= vis_h * 0.22 && screen_bot_wy >= level_max_wy + 95.0,
                "Level {} ({}) lacked downside space! screen_bot={}, vis_h={}, level_max_wy={}",
                lvl.index,
                lvl.id,
                screen_bot_wy,
                vis_h,
                level_max_wy
            );

            // Verify the resting camera frames BOTH the slingshot and the castle horizontally
            {
                let sling_wx = sim.slingshot_pos.x * 20.0;
                let mut cmin = f32::INFINITY;
                let mut cmax = f32::NEG_INFINITY;
                for o in &lvl.objects {
                    if !o.controllable && o.start_number == 0 {
                        cmin = cmin.min(o.x * 20.0 - 40.0);
                        cmax = cmax.max(o.x * 20.0 + 40.0);
                    }
                }
                let span_left = sling_wx - 130.0;
                let span_right = if cmax.is_finite() {
                    cmax + 130.0
                } else {
                    sling_wx + 1100.0
                };
                let span_w = (span_right - span_left).max(620.0);
                let hz = super::assets::safe_clamp(480.0 * 1280.0 / (span_w * 720.0), 0.20, 0.84);
                let z = default_zoom.min(hz);
                // visible world width at 1280x720 window
                let vis_w = 1280.0 / ((720.0 / 480.0) * z);
                let rest_x = if cmax.is_finite() {
                    (span_left + span_right) * 0.5
                } else {
                    sling_wx + 240.0
                };
                let view_l = rest_x - vis_w * 0.5;
                let view_r = rest_x + vis_w * 0.5;
                assert!(
                    view_l <= span_left + 1.0 && view_r >= span_right - 1.0,
                    "Level {} ({}) does not frame slingshot->castle: view=[{}, {}] span=[{}, {}] zoom={}",
                    lvl.index, lvl.id, view_l, view_r, span_left, span_right, z
                );
            }

            // Stress-test extreme zoom-out / zoom-in (0.20 .. 1.50) & manual vertical offsets
            // on every single level to ensure zero clamp panics or NaNs!
            for z_step in 4..=30 {
                let z = (z_step as f32) * 0.05;
                let vh = 480.0 / z.max(0.24);
                let dbot = (level_max_wy + 115.0).max(165.0).max(vh * 0.25);
                let rest_y = (dbot - vh * 0.5).min((bird_top_wy - 95.0) + vh * 0.5);
                let max_y = (dbot + 60.0) - vh * 0.5;
                let min_y = bird_top_wy.min(castle_min_wy) - 450.0;
                let lo_y = min_y.min(rest_y - 350.0);
                let hi_y = max_y.max(rest_y + 220.0);
                for off in [-550.0f32, -200.0, 0.0, 200.0, 350.0] {
                    let cy = super::assets::safe_clamp(rest_y + off, lo_y, hi_y);
                    assert!(cy.is_finite());
                }
            }
        }

        // Full-play stress test: fire a bird in EVERY level and make sure the world stays sane
        // (no NaN, nothing falls through the floor, no panics) for 10 simulated seconds.
        for lvl in &gdata.levels {
            let mut sim = LevelSimulation::new(lvl, &gdata);
            let pull = sim.slingshot_pos + vec2(-4.2, 2.1);
            sim.launch_bird(pull, &gdata);
            for _ in 0..600 {
                sim.step(1.0 / 60.0, &gdata);
            }
            for (h, rb) in sim.rigid_bodies.iter() {
                let t = rb.translation();
                assert!(
                    t.x.is_finite() && t.y.is_finite(),
                    "Level {} ({}) produced a non-finite body position",
                    lvl.index,
                    lvl.id
                );
                assert!(
                    t.y < 400.0,
                    "Level {} ({}) let a body fall through the ground (y={})",
                    lvl.index,
                    lvl.id,
                    t.y
                );
                let _ = h;
            }
            assert!(
                sim.score < 10_000_000,
                "Level {} ({}) produced an absurd score",
                lvl.index,
                lvl.id
            );
        }

        // Verify Level 1 castle stands rock-solid when idle for 3 seconds (180 frames)
        let mut sim1 = LevelSimulation::new(&gdata.levels[0], &gdata);
        let initial_pigs = sim1.remaining_pigs();
        let initial_entities = sim1.entities.len();
        for _ in 0..180 {
            sim1.step(1.0 / 60.0, &gdata);
        }
        assert_eq!(
            sim1.remaining_pigs(),
            initial_pigs,
            "Level 1 pig must not die on idle castle settle!"
        );
        assert_eq!(
            sim1.entities.len(),
            initial_entities,
            "Level 1 castle blocks must stand stable without collapsing!"
        );

        // Launch Bird 1 directly into Level 1's two-story castle and verify 1-shot victory!
        let pull_pos = sim1.slingshot_pos + vec2(-4.75, 2.35);
        sim1.launch_bird(pull_pos, &gdata);
        for _ in 0..360 {
            sim1.step(1.0 / 60.0, &gdata);
        }
        assert!(
            sim1.score >= 5000 && sim1.remaining_pigs() == 0,
            "Direct castle shot in Level 1 must collapse the hut and pop the pig (score={}, remaining_pigs={})!",
            sim1.score,
            sim1.remaining_pigs()
        );
        println!(
            "Verified all 105 levels! Level 1 1-shot VICTORY score (with unused bird bonus) = {}, remaining pigs = {}",
            sim1.score,
            sim1.remaining_pigs()
        );
    }
}

#[cfg(test)]
mod diagnose {
    use super::gamedata::GameData;
    use super::physics::LevelSimulation;

    /// Settle every level with no bird fired and report any that collapse/explode on load.
    #[test]
    fn report_unstable_levels() {
        let gdata = GameData::load_embedded();
        let mut bad = Vec::new();
        let mut n2 = 0;
        let mut n4 = 0;
        for lvl in &gdata.levels {
            let mut sim = LevelSimulation::new(lvl, &gdata);
            let mut start: Vec<(String, f32, f32)> = Vec::new();
            for (h, rb) in sim.rigid_bodies.iter() {
                let t = rb.translation();
                let nm = sim
                    .entities
                    .get(&h)
                    .map(|e| format!("{}[{}]", e.name, e.definition))
                    .unwrap_or_default();
                start.push((nm, t.x, t.y));
            }
            start.sort_by(|a, b| a.0.cmp(&b.0));
            for _ in 0..900 {
                sim.step(1.0 / 60.0, &gdata);
            }
            let mut worst = 0.0f32;
            let mut worst_name = String::new();
            for (h, rb) in sim.rigid_bodies.iter() {
                let t = rb.translation();
                let nm = sim
                    .entities
                    .get(&h)
                    .map(|e| format!("{}[{}]", e.name, e.definition))
                    .unwrap_or_default();
                if let Some((_, x0, y0)) = start.iter().find(|(n, _, _)| *n == nm) {
                    let d = ((t.x - x0).powi(2) + (t.y - y0).powi(2)).sqrt();
                    if d > worst {
                        worst = d;
                        worst_name = nm;
                    }
                }
            }
            if worst > 2.0 {
                n2 += 1;
            }
            if worst > 4.0 {
                n4 += 1;
            }
            if worst > 4.0 {
                bad.push((lvl.index, lvl.id.clone(), worst, worst_name));
            }
        }
        println!("SETTLE: >2 = {} levels, >4 = {} levels", n2, n4);
        for (i, id, w, n) in &bad {
            println!("  COLLAPSE idx={} id={} drift={:.2} worst={}", i, id, w, n);
        }
    }
}

#[cfg(test)]
mod diag2 {
    use super::gamedata::GameData;
    use super::physics::LevelSimulation;

    #[test]
    fn detail_frames() {
        let gdata = GameData::load_embedded();
        let lvl = gdata.levels.iter().find(|l| l.id == "LevelP2_103").unwrap();
        println!(
            "level {} theme={} obj={} joints={}",
            lvl.id,
            lvl.theme,
            lvl.objects.len(),
            lvl.joints.len()
        );
        // Objects with suspicious fields
        for o in lvl.objects.iter().take(8) {
            println!(
                "  {} {} x={:.2} y={:.2} w={:.2} h={:.2} ang={:.2} shape={}",
                o.name, o.definition, o.x, o.y, o.width, o.height, o.angle, o.shape_type
            );
        }
        let mut sim = LevelSimulation::new(lvl, &gdata);
        let mut start = Vec::new();
        for (h, rb) in sim.rigid_bodies.iter() {
            let t = rb.translation();
            let nm = sim
                .entities
                .get(&h)
                .map(|e| format!("{}[{}]", e.name, e.definition))
                .unwrap_or_default();
            start.push((nm, t.x, t.y));
        }
        start.sort_by(|a, b| a.0.cmp(&b.0));
        let mut fr = 1;
        for frame in 0..16 {
            let n = if frame == 0 { 1 } else { 59 };
            for _ in 0..n {
                sim.step(1.0 / 60.0, &gdata);
            }
            fr += n;
            let mut mx = 0.0f32;
            let mut who = String::new();
            for (h, rb) in sim.rigid_bodies.iter() {
                let t = rb.translation();
                let nm = sim
                    .entities
                    .get(&h)
                    .map(|e| format!("{}[{}]", e.name, e.definition))
                    .unwrap_or_default();
                if let Some((_, x0, y0)) = start.iter().find(|(n, _, _)| *n == nm) {
                    let d = ((t.x - x0).powi(2) + (t.y - y0).powi(2)).sqrt();
                    if d > mx {
                        mx = d;
                        who = nm;
                    }
                }
            }
            println!("   frame~{:3} maxdrift={:6.2} ({})", fr, mx, who);
        }
        let mut rows = Vec::new();
        for (h, rb) in sim.rigid_bodies.iter() {
            let t = rb.translation();
            let nm = sim
                .entities
                .get(&h)
                .map(|e| format!("{}[{}]", e.name, e.definition))
                .unwrap_or_default();
            if let Some((_, x0, y0)) = start.iter().find(|(n, _, _)| *n == nm) {
                let d = ((t.x - x0).powi(2) + (t.y - y0).powi(2)).sqrt();
                rows.push((d, nm.clone(), *x0, *y0, t.x, t.y));
            }
        }
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for r in rows.iter().take(8) {
            println!(
                "  drift={:6.2} {} ({:.2},{:.2}) -> ({:.2},{:.2})",
                r.0, r.1, r.2, r.3, r.4, r.5
            );
        }
    }
}
