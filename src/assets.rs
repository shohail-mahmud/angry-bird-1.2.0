//! Direct pure-Rust loader for `1.2.0.ipa` textures (PNG + PVR v2), `.dat` sprite sheets,
//! `.dat` bitmap fonts, `.wav` sound effects, and `.mp3` music tracks.
//!
//! The 1.2.0 payload lives under `Payload/AngryBirds.app/data_iphone/` with the sheets,
//! fonts and audio split into `images/`, `fonts/`, `audio/sfx/` and `audio/music/`.

use crate::pvr::decode_pvr;
use macroquad::audio::{load_sound_from_bytes, play_sound, stop_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use zip::ZipArchive;

#[cfg(feature = "embed-assets")]
static EMBEDDED_IPA: &[u8] = include_bytes!("../../1.2.0.ipa");

#[derive(Clone, Debug)]
pub struct SpriteEntry {
    pub tex_name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FontGlyph {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub baseline_y: f32,
}

#[derive(Clone, Debug)]
pub struct BitmapFont {
    pub tex_name: String,
    pub line_height: f32,
    pub tracking: f32,
    pub glyphs: HashMap<char, FontGlyph>,
}

/// Opens an external `https://` or `mailto:` URL in the user's default OS browser / mail app
/// without spawning a console window on Windows.
#[cfg(target_os = "windows")]
pub fn open_external_url(url: &str) {
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut core::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }
    let op: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(
            core::ptr::null_mut(),
            op.as_ptr(),
            target.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            1, // SW_SHOWNORMAL
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open_external_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Panic-free float clamp that handles `a > b` or non-finite inputs safely
/// (unlike `f32::clamp`, which panics if `min > max`).
#[inline]
pub fn safe_clamp(v: f32, a: f32, b: f32) -> f32 {
    let lo = a.min(b);
    let hi = a.max(b);
    if !v.is_finite() {
        (lo + hi) * 0.5
    } else {
        v.max(lo).min(hi)
    }
}

/// Measures UI text using a single fixed glyph raster size (`24`) + smooth float scaling
/// so live window resizing never bloats/overflows macroquad's glyph atlas or hits size 0.
#[inline]
pub fn measure_ui_text(text: &str, px_size: f32) -> f32 {
    let scale = px_size.max(6.0) / 24.0;
    measure_text(text, None, 24, scale).width
}

/// Draws UI text using fixed glyph raster size (`24`) + smooth float scaling.
#[inline]
pub fn draw_ui_text(text: &str, x: f32, y: f32, px_size: f32, color: Color) {
    let scale = px_size.max(6.0) / 24.0;
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: None,
            font_size: 24,
            font_scale: scale,
            font_scale_aspect: 1.0,
            rotation: 0.0,
            color,
        },
    );
}

pub struct Assets {
    pub textures: HashMap<String, Texture2D>,
    pub sprites: HashMap<String, SpriteEntry>,
    pub fonts: HashMap<String, BitmapFont>,
    pub sounds: HashMap<String, Sound>,
    pub sound_groups: HashMap<String, Vec<String>>,
    pub current_music: Option<String>,
    pub muted: bool,
}

impl Assets {
    pub async fn load(muted: bool) -> Result<Self, String> {
        let ipa_bytes: Vec<u8> = if let Ok(ext) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("1.2.0.ipa")))
            .ok_or(())
            .and_then(|p| std::fs::read(p).map_err(|_| ()))
        {
            ext
        } else if let Ok(local) = std::fs::read("1.2.0.ipa") {
            local
        } else {
            #[cfg(feature = "embed-assets")]
            {
                EMBEDDED_IPA.to_vec()
            }
            #[cfg(not(feature = "embed-assets"))]
            {
                return Err(
                    "1.2.0.ipa not found. Put the original 1.2.0.ipa next to the executable."
                        .to_string(),
                );
            }
        };

        let mut archive = ZipArchive::new(Cursor::new(ipa_bytes))
            .map_err(|e| format!("Failed to open 1.2.0.ipa archive: {}", e))?;

        let prefix = "Payload/AngryBirds.app/data_iphone/";
        let mut raw_files: HashMap<String, Vec<u8>> = HashMap::new();
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().to_string();
            if name.starts_with(prefix) && !name.ends_with('/') {
                let rel = name[prefix.len()..].to_string();
                let mut buf = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                raw_files.insert(rel, buf);
            }
        }

        let mut textures = HashMap::new();
        let mut sprites = HashMap::new();
        let mut fonts = HashMap::new();

        // 1. Decode all PNG and PVR textures (keyed by file name, e.g. "MENU_ELEMENTS_1.png")
        for (rel, bytes) in &raw_files {
            let base = rel.rsplit('/').next().unwrap_or(rel.as_str()).to_string();
            if base.ends_with(".png") {
                let dyn_img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
                    .map_err(|e| format!("PNG decode error {}: {}", rel, e))?;
                let rgba = dyn_img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let tex = Texture2D::from_rgba8(w as u16, h as u16, rgba.as_raw());
                tex.set_filter(FilterMode::Linear);
                textures.insert(base, tex);
            } else if base.ends_with(".pvr") {
                let dec = decode_pvr(bytes)?;
                let tex = Texture2D::from_rgba8(dec.width as u16, dec.height as u16, &dec.rgba);
                tex.set_filter(FilterMode::Linear);
                textures.insert(base, tex);
            }
        }

        // 2. Parse sprite sheets & bitmap fonts
        for (rel, bytes) in &raw_files {
            let base = rel.rsplit('/').next().unwrap_or(rel.as_str()).to_string();
            if !base.ends_with(".dat") || base.starts_with("TEXTS_") {
                continue;
            }
            if base.starts_with("FONT_") {
                let font = parse_font_dat(bytes)?;
                let key = base.trim_end_matches(".dat").to_string();
                fonts.insert(key, font);
            } else {
                let sheet_sprites = parse_sprite_dat(bytes)?;
                for (sname, entry) in sheet_sprites {
                    sprites.insert(sname, entry);
                }
            }
        }

        // 3. Load WAV SFX & MP3 Music
        let mut sounds = HashMap::new();
        for (rel, bytes) in &raw_files {
            if !(rel.starts_with("audio/sfx/") || rel.starts_with("audio/music/")) {
                continue;
            }
            let base = rel.rsplit('/').next().unwrap_or(rel.as_str());
            let stem = match base.rsplit_once('.') {
                Some((s, _ext)) => s,
                None => continue,
            };
            if base.ends_with(".wav") {
                if let Ok(snd) = load_sound_from_bytes(bytes).await {
                    sounds.insert(stem.to_string(), snd);
                }
            } else if base.ends_with(".mp3") {
                if let Ok(wav_bytes) = mp3_to_wav_bytes(bytes) {
                    if let Ok(snd) = load_sound_from_bytes(&wav_bytes).await {
                        sounds.insert(stem.to_string(), snd);
                    }
                }
            }
        }

        let sound_groups = build_sound_groups(&sounds);

        Ok(Self {
            textures,
            sprites,
            fonts,
            sounds,
            sound_groups,
            current_music: None,
            muted,
        })
    }

    /// Draws a named Rovio sprite anchored at its exact `(pivot_x, pivot_y)` in screen/world coords.
    pub fn draw_sprite(
        &self,
        name: &str,
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        rotation: f32,
        color: Color,
    ) {
        let Some(sp) = self.sprites.get(name) else {
            return;
        };
        let Some(tex) = self.textures.get(&sp.tex_name) else {
            return;
        };
        let dw = sp.w * scale_x;
        let dh = sp.h * scale_y;
        let dest_x = x - sp.pivot_x * scale_x;
        let dest_y = y - sp.pivot_y * scale_y;

        draw_texture_ex(
            tex,
            dest_x,
            dest_y,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(dw, dh)),
                source: Some(Rect::new(sp.x, sp.y, sp.w, sp.h)),
                rotation,
                pivot: Some(vec2(x, y)),
                ..Default::default()
            },
        );
    }

    /// Draws a sprite centered inside `(cx, cy)` regardless of original pivot (convenient for UI buttons).
    pub fn draw_sprite_centered(&self, name: &str, cx: f32, cy: f32, scale: f32, color: Color) {
        let Some(sp) = self.sprites.get(name) else {
            return;
        };
        let Some(tex) = self.textures.get(&sp.tex_name) else {
            return;
        };
        let dw = sp.w * scale;
        let dh = sp.h * scale;
        draw_texture_ex(
            tex,
            cx - dw * 0.5,
            cy - dh * 0.5,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(dw, dh)),
                source: Some(Rect::new(sp.x, sp.y, sp.w, sp.h)),
                ..Default::default()
            },
        );
    }

    /// Draws a sprite from its top-left `(x, y)` with custom width/height.
    pub fn draw_sprite_rect(&self, name: &str, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let Some(sp) = self.sprites.get(name) else {
            return;
        };
        let Some(tex) = self.textures.get(&sp.tex_name) else {
            return;
        };
        draw_texture_ex(
            tex,
            x,
            y,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                source: Some(Rect::new(sp.x, sp.y, sp.w, sp.h)),
                ..Default::default()
            },
        );
    }

    /// 1.2.0 draws `StaticBlockTheme*` ground blocks as *texture-filled* silhouettes: the
    /// sprite is only an alpha mask and the block's `texture` field (`INGAME_THEME_GROUND_n`
    /// == `THEME_GROUND_TEXTURE_n`, a tileable 256x256 dirt sheet) supplies the pattern.
    /// Baking them once at level load turns the flat brown blobs into proper dirt terrain.
    pub fn bake_texture_fill(&mut self, mask: &str, fill: &str) -> Option<String> {
        if fill.is_empty() {
            return None;
        }
        let key = format!("{}#fill#{}", mask, fill);
        if self.sprites.contains_key(&key) {
            return Some(key);
        }
        let me = self.sprites.get(mask)?.clone();
        // blocks.lua names the *texture* (`INGAME_THEME_GROUND_1`); the sprite in the atlas is
        // `THEME_GROUND_TEXTURE_1`. Accept either.
        let fill_sprite = if self.sprites.contains_key(fill) {
            fill.to_string()
        } else if let Some(n) = fill.strip_prefix("INGAME_THEME_GROUND_") {
            format!("THEME_GROUND_TEXTURE_{}", n)
        } else {
            fill.to_string()
        };
        let fe = self.sprites.get(&fill_sprite)?.clone();
        let mask_img = self.textures.get(&me.tex_name)?.get_texture_data();
        let fill_img = self.textures.get(&fe.tex_name)?.get_texture_data();
        let mw = me.w.max(1.0) as u32;
        let mh = me.h.max(1.0) as u32;
        let fw = fill_img.width.max(1) as u32;
        let fh = fill_img.height.max(1) as u32;
        let mut out = Vec::with_capacity((mw * mh * 4) as usize);
        for y in 0..mh {
            for x in 0..mw {
                let mwmax = (mask_img.width as u32).saturating_sub(1);
                let mhmax = (mask_img.height as u32).saturating_sub(1);
                let sx = (me.x as u32 + x).min(mwmax);
                let sy = (me.y as u32 + y).min(mhmax);
                let m = mask_img.get_pixel(sx, sy);
                if m.a <= 0.01 {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                // Tile the pattern in sprite-local space so it lines up with the art
                let fx = (fe.x as u32 + x % fw) % fw;
                let fy = (fe.y as u32 + y % fh) % fh;
                let c = fill_img.get_pixel(fx, fy);
                // The mask sprite is only a silhouette (its RGB is a flat dark brown); the
                // dirt pattern supplies the colour, exactly like the original's pattern fill.
                let cr = c.r.clamp(0.0, 1.0);
                let cg = c.g.clamp(0.0, 1.0);
                let cb = c.b.clamp(0.0, 1.0);
                out.extend_from_slice(&[
                    (cr * 255.0) as u8,
                    (cg * 255.0) as u8,
                    (cb * 255.0) as u8,
                    ((c.a * m.a) * 255.0) as u8,
                ]);
            }
        }
        let tex = Texture2D::from_rgba8(mw as u16, mh as u16, &out);
        tex.set_filter(FilterMode::Linear);
        self.textures.insert(key.clone(), tex);
        self.sprites.insert(
            key.clone(),
            SpriteEntry {
                tex_name: key.clone(),
                x: 0.0,
                y: 0.0,
                w: me.w,
                h: me.h,
                pivot_x: me.pivot_x,
                pivot_y: me.pivot_y,
            },
        );
        Some(key)
    }

    /// Average colour of the bottom row of a sprite. The 1.2.0 ground cross-sections
    /// (`FOREGROUND_x_LAYER_1`) are authored for a 480x320 screen, so on a taller window the
    /// area below the artwork has to be continued with the artwork's own deepest ground tone
    /// instead of stretching the sprite (which used to turn the rocks into tall blobs).
    pub fn sprite_bottom_color(&self, name: &str) -> Option<Color> {
        let sp = self.sprites.get(name)?;
        let tex = self.textures.get(&sp.tex_name)?;
        let img = tex.get_texture_data();
        let iw = img.width as f32;
        let ih = img.height as f32;
        // Sample a band just above the artwork's bottom edge instead of the very last
        // row: the final row is the artwork's dark shadow line, and filling the area
        // below the ground with that tone left a hard black band under the scene.
        let ya = (sp.y + sp.h * 0.80).clamp(0.0, (ih - 1.0).max(0.0));
        let yb = (sp.y + sp.h * 0.97).clamp(0.0, (ih - 1.0).max(0.0));
        let x0 = sp.x.clamp(0.0, (iw - 1.0).max(0.0));
        let x1 = (sp.x + sp.w - 1.0).clamp(x0, (iw - 1.0).max(0.0));
        let step = (((x1 - x0) / 32.0).floor() as i64).max(1) as f32;
        let (mut r, mut g, mut b, mut a, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut y = ya;
        while y <= yb {
            let mut x = x0;
            while x <= x1 {
                let c = img.get_pixel(x as u32, y as u32);
                r += c.r;
                g += c.g;
                b += c.b;
                a += c.a;
                n += 1.0;
                x += step;
            }
            y += 2.0;
        }
        if n < 1.0 {
            return None;
        }
        Some(Color::new(r / n, g / n, b / n, a / n))
    }

    /// True when the sprite exists in one of the decoded 1.2.0 atlases.
    pub fn has_sprite(&self, name: &str) -> bool {
        self.sprites.contains_key(name)
    }

    pub fn sprite_size(&self, name: &str) -> (f32, f32) {
        self.sprites
            .get(name)
            .map(|s| (s.w, s.h))
            .unwrap_or((32.0, 32.0))
    }

    pub fn measure_bitmap_text(&self, font_name: &str, text: &str, scale: f32) -> f32 {
        let Some(font) = self.fonts.get(font_name) else {
            return text.len() as f32 * 12.0 * scale;
        };
        let mut width = 0.0f32;
        for ch in text.chars() {
            if let Some(g) = font.glyphs.get(&ch) {
                width += (g.w + font.tracking) * scale;
            } else {
                width += font.line_height * 0.35 * scale;
            }
        }
        width.max(0.0)
    }

    /// Renders text using Rovio's original `.dat` bitmap font (`align`: 0.0=left, 0.5=center, 1.0=right).
    pub fn draw_bitmap_text(
        &self,
        font_name: &str,
        text: &str,
        x: f32,
        baseline_y: f32,
        scale: f32,
        align: f32,
        color: Color,
    ) {
        let Some(font) = self.fonts.get(font_name) else {
            draw_ui_text(text, x, baseline_y, 24.0 * scale, color);
            return;
        };
        let Some(tex) = self.textures.get(&font.tex_name) else {
            return;
        };
        let total_w = self.measure_bitmap_text(font_name, text, scale);
        let mut cur_x = x - total_w * align;

        for ch in text.chars() {
            if let Some(g) = font.glyphs.get(&ch) {
                let dw = g.w * scale;
                let dh = g.h * scale;
                let dy = baseline_y - g.baseline_y * scale;
                draw_texture_ex(
                    tex,
                    cur_x,
                    dy,
                    color,
                    DrawTextureParams {
                        dest_size: Some(vec2(dw, dh)),
                        source: Some(Rect::new(g.x, g.y, g.w, g.h)),
                        ..Default::default()
                    },
                );
                cur_x += (g.w + font.tracking) * scale;
            } else {
                cur_x += font.line_height * 0.35 * scale;
            }
        }
    }

    pub fn play_sfx(&self, name: &str, volume: f32) {
        if self.muted || name.is_empty() {
            return;
        }
        if let Some(group) = self.sound_groups.get(name) {
            if !group.is_empty() {
                let idx = (macroquad::rand::rand() as usize) % group.len();
                if let Some(snd) = self.sounds.get(&group[idx]) {
                    play_sound(
                        snd,
                        PlaySoundParams {
                            looped: false,
                            volume: volume.clamp(0.0, 1.0),
                        },
                    );
                }
                return;
            }
        }
        // Try exact or space/underscore normalized match
        let alt = name.replace('_', " ");
        if let Some(snd) = self.sounds.get(name).or_else(|| self.sounds.get(&alt)) {
            play_sound(
                snd,
                PlaySoundParams {
                    looped: false,
                    volume: volume.clamp(0.0, 1.0),
                },
            );
        }
    }

    pub fn play_music(&mut self, track: &str, looped: bool, volume: f32) {
        if self.current_music.as_deref() == Some(track) {
            return;
        }
        self.stop_music();
        self.current_music = Some(track.to_string());
        if self.muted {
            return;
        }
        if let Some(snd) = self.sounds.get(track) {
            play_sound(
                snd,
                PlaySoundParams {
                    looped,
                    volume: volume.clamp(0.0, 1.0),
                },
            );
        }
    }

    pub fn stop_music(&mut self) {
        if let Some(prev) = self.current_music.take() {
            if let Some(snd) = self.sounds.get(&prev) {
                stop_sound(snd);
            }
        }
    }

    pub fn set_muted(&mut self, muted: bool, resume_track: Option<(&str, f32)>) {
        self.muted = muted;
        if muted {
            if let Some(cur) = &self.current_music {
                if let Some(snd) = self.sounds.get(cur) {
                    stop_sound(snd);
                }
            }
        } else if let Some((track, vol)) = resume_track {
            self.current_music = None;
            self.play_music(track, true, vol);
        }
    }
}

fn parse_sprite_dat(raw: &[u8]) -> Result<Vec<(String, SpriteEntry)>, String> {
    if raw.len() < 4 {
        return Err("Sprite .dat too short".into());
    }
    let mut pos = 0usize;
    let tlen = u16::from_be_bytes([raw[pos], raw[pos + 1]]) as usize;
    pos += 2;
    let tex_name = String::from_utf8_lossy(&raw[pos..pos + tlen]).to_string();
    pos += tlen;
    let count = u16::from_be_bytes([raw[pos], raw[pos + 1]]) as usize;
    pos += 2;

    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let nlen = u16::from_be_bytes([raw[pos], raw[pos + 1]]) as usize;
        pos += 2;
        let sname = String::from_utf8_lossy(&raw[pos..pos + nlen]).to_string();
        pos += nlen;
        let x = i16::from_be_bytes([raw[pos], raw[pos + 1]]) as f32;
        let y = i16::from_be_bytes([raw[pos + 2], raw[pos + 3]]) as f32;
        let w = i16::from_be_bytes([raw[pos + 4], raw[pos + 5]]) as f32;
        let h = i16::from_be_bytes([raw[pos + 6], raw[pos + 7]]) as f32;
        let px = i16::from_be_bytes([raw[pos + 8], raw[pos + 9]]) as f32;
        let py = i16::from_be_bytes([raw[pos + 10], raw[pos + 11]]) as f32;
        pos += 12;
        out.push((
            sname,
            SpriteEntry {
                tex_name: tex_name.clone(),
                x,
                y,
                w,
                h,
                pivot_x: px,
                pivot_y: py,
            },
        ));
    }
    Ok(out)
}

fn parse_font_dat(raw: &[u8]) -> Result<BitmapFont, String> {
    if raw.len() < 8 {
        return Err("Font .dat too short".into());
    }
    let mut pos = 0usize;
    let tlen = u16::from_be_bytes([raw[pos], raw[pos + 1]]) as usize;
    pos += 2;
    let tex_name = String::from_utf8_lossy(&raw[pos..pos + tlen]).to_string();
    pos += tlen;

    let line_height = i16::from_be_bytes([raw[pos], raw[pos + 1]]) as f32;
    let tracking = i16::from_be_bytes([raw[pos + 2], raw[pos + 3]]) as f32;
    let count = i16::from_be_bytes([raw[pos + 4], raw[pos + 5]]) as usize;
    pos += 6;

    let mut glyphs = HashMap::with_capacity(count);
    for _ in 0..count {
        let ch_code = u16::from_be_bytes([raw[pos], raw[pos + 1]]) as u32;
        let x = i16::from_be_bytes([raw[pos + 2], raw[pos + 3]]) as f32;
        let y = i16::from_be_bytes([raw[pos + 4], raw[pos + 5]]) as f32;
        let w = i16::from_be_bytes([raw[pos + 6], raw[pos + 7]]) as f32;
        let h = i16::from_be_bytes([raw[pos + 8], raw[pos + 9]]) as f32;
        let oy = i16::from_be_bytes([raw[pos + 10], raw[pos + 11]]) as f32;
        pos += 12;
        if let Some(ch) = char::from_u32(ch_code) {
            glyphs.insert(
                ch,
                FontGlyph {
                    x,
                    y,
                    w,
                    h,
                    baseline_y: oy,
                },
            );
        }
    }

    Ok(BitmapFont {
        tex_name,
        line_height,
        tracking,
        glyphs,
    })
}

fn mp3_to_wav_bytes(mp3_data: &[u8]) -> Result<Vec<u8>, String> {
    let mss = MediaSourceStream::new(Box::new(Cursor::new(mp3_data.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("No MP3 audio track")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(16000);
    let mut channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;

    let mut pcm: Vec<i16> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        if let Ok(decoded) = decoder.decode(&packet) {
            channels = decoded.spec().channels.count() as u16;
            let mut sbuf = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
            sbuf.copy_interleaved_ref(decoded);
            pcm.extend_from_slice(sbuf.samples());
        }
    }

    let data_bytes = (pcm.len() * 2) as u32;
    let byte_rate = sample_rate * (channels as u32) * 2;
    let block_align = channels * 2;

    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for s in pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    Ok(wav)
}

fn build_sound_groups(sounds: &HashMap<String, Sound>) -> HashMap<String, Vec<String>> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    let prefixes = [
        ("bird_01_collision", "bird 01 collision"),
        ("bird_02_collision", "bird 02 collision"),
        ("bird_03_collision", "bird 03 collision"),
        ("bird_04_collision", "bird 04 collision"),
        ("bird_05_collision", "bird 05 collision"),
        ("bird_misc", "bird misc"),
        ("bird_next", "bird next military"),
        ("bird_shot", "bird shot-"),
        ("red_special", "redbird_yell"),
        ("light_collision", "ice light collision"),
        ("light_damage", "light damage"),
        ("light_destroyed", "light destroyed"),
        ("wood_collision", "wood collision"),
        ("wood_damage", "wood damage"),
        ("wood_destroyed", "wood destroyed"),
        ("rock_collision", "rock collision"),
        ("rock_damage", "rock damage"),
        ("rock_destroyed", "rock destroyed"),
        ("piglette_collision", "piglette collision"),
        ("piglette_damage", "piglette damage"),
        ("piglette_oink", "piglette oink a"),
        ("level_start_military", "level start military"),
        ("level_clear_military", "level clear military"),
        ("level_failed_piglets", "level failed piglets"),
    ];
    for (group_key, prefix) in prefixes {
        let mut list: Vec<String> = sounds
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        list.sort();
        if !list.is_empty() {
            groups.insert(group_key.to_string(), list);
        }
    }
    // Single-file aliases matching blocks.lua / gamelogic.lua
    let singles = [
        ("piglette_destroyed", "piglette destroyed"),
        ("special_boost", "special boost"),
        ("special_group", "special group"),
        ("special_egg", "special egg"),
        ("special_egg_explosion", "special egg explosion"),
        ("special_explosion", "tnt box explodes"),
        ("slingshot_stretched", "slingshot streched"),
        ("bird_destroyed", "bird destroyed"),
    ];
    for (alias, target) in singles {
        if sounds.contains_key(target) {
            groups.insert(alias.to_string(), vec![target.to_string()]);
        }
    }
    groups
}
