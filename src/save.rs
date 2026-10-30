//! Persistent save-game management (stars, highscores, unlocked stages, seen tutorials, audio settings)
//! for the 1.2.0 build: 105 levels across 5 chapters (21 stages each).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn default_ui_scale() -> f32 {
    1.0
}

/// 1.2.0 `levelOrder` (chapters 1-3) + `levelOrder_packOne` (chapters 4-5) = 105 levels.
pub const LEVELS_PER_CHAPTER: usize = 21;
pub const CHAPTER_COUNT: usize = 5;
pub const TOTAL_LEVELS: usize = LEVELS_PER_CHAPTER * CHAPTER_COUNT;

/// Chapter titles, taken verbatim from `TEXTS_BASIC.dat` ("TEXT_LP_NAME_1/2") and the
/// 1.2.0 episode artwork ("Poached Eggs", "Mighty Hoax" + 3 bonus chapters).
pub fn chapter_name(chapter: usize) -> &'static str {
    match chapter {
        1 => "POACHED EGGS",
        2 => "MIGHTY HOAX",
        3 => "DANGER ABOVE",
        4 => "THE BIG SETUP",
        5 => "HAM EM HIGH",
        _ => "POACHED EGGS",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveState {
    pub unlocked_level: usize,
    pub unlock_all: bool,
    pub highscores: HashMap<usize, u32>,
    pub stars: HashMap<usize, u8>,
    pub seen_tutorials: HashSet<String>,
    pub seen_intro: bool,
    pub muted: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
}

impl Default for SaveState {
    fn default() -> Self {
        Self {
            unlocked_level: 1,
            unlock_all: false,
            highscores: HashMap::new(),
            stars: HashMap::new(),
            seen_tutorials: HashSet::new(),
            seen_intro: false,
            muted: false,
            ui_scale: 1.0,
        }
    }
}

impl SaveState {
    fn save_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("AngryBirds1_2_Rust");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("save.json");
        }
        if let Ok(home) = std::env::var("HOME") {
            let dir = PathBuf::from(home)
                .join(".config")
                .join("angry_birds_1_2_rust");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("save.json");
        }
        PathBuf::from("angry_birds_1_2_save.json")
    }

    pub fn load() -> Self {
        let p = Self::save_path();
        if let Ok(bytes) = std::fs::read(&p) {
            if let Ok(s) = serde_json::from_slice::<Self>(&bytes) {
                return s;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let p = Self::save_path();
        if let Ok(txt) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, txt);
        }
    }

    pub fn is_level_unlocked(&self, level_idx: usize) -> bool {
        self.unlock_all || level_idx <= self.unlocked_level.max(1)
    }

    pub fn is_chapter_unlocked(&self, chapter: usize) -> bool {
        if self.unlock_all || chapter <= 1 {
            return true;
        }
        let first_idx = (chapter - 1) * LEVELS_PER_CHAPTER + 1;
        self.unlocked_level >= first_idx
    }

    /// Highest chapter that has been reached (used to open Level Select on the right page).
    pub fn current_chapter(&self) -> usize {
        ((self.unlocked_level.saturating_sub(1)) / LEVELS_PER_CHAPTER + 1).clamp(1, CHAPTER_COUNT)
    }

    /// Number of stars collected in a chapter, and the chapter's maximum (21 x 3).
    pub fn chapter_stars(&self, chapter: usize) -> (u32, u32) {
        let start = (chapter - 1) * LEVELS_PER_CHAPTER + 1;
        let end = chapter * LEVELS_PER_CHAPTER;
        let mut sum = 0u32;
        for idx in start..=end {
            sum += self.stars.get(&idx).copied().unwrap_or(0) as u32;
        }
        (sum, (LEVELS_PER_CHAPTER * 3) as u32)
    }

    /// Total stars across all 5 chapters (out of 315).
    pub fn total_stars(&self) -> u32 {
        self.stars.values().map(|v| *v as u32).sum()
    }

    pub fn record_win(&mut self, level_idx: usize, score: u32, earned_stars: u8) -> bool {
        let prev_high = self.highscores.get(&level_idx).copied().unwrap_or(0);
        let is_new_high = score > prev_high;
        if is_new_high {
            self.highscores.insert(level_idx, score);
        }
        let prev_stars = self.stars.get(&level_idx).copied().unwrap_or(0);
        if earned_stars > prev_stars {
            self.stars.insert(level_idx, earned_stars);
        }
        if level_idx + 1 > self.unlocked_level && level_idx < TOTAL_LEVELS {
            self.unlocked_level = level_idx + 1;
        }
        self.save();
        is_new_high
    }

    /// Computes unified screen-space GUI scale (~40% more compact than mobile 480p scale,
    /// plus live user adjustment via `[` and `]` keys).
    pub fn gui_scale(&self) -> f32 {
        let sh = macroquad::window::screen_height().max(120.0);
        let base = crate::assets::safe_clamp(sh / 760.0, 0.45, 1.45);
        base * crate::assets::safe_clamp(self.ui_scale, 0.50, 1.60)
    }
}
