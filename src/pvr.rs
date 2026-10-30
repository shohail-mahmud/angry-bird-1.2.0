//! Pure-Rust PowerVR (.pvr v2) texture decoder for Angry Birds 1.0.
//! Supports:
//! - OGL_RGBA_4444 (format 0x10 / 16): 16-bit uncompressed RGBA4444
//! - OGL_RGBA_8888 (format 0x12 / 18): 32-bit uncompressed RGBA8888
//! - OGL_RGBA_5551 (format 0x11 / 17): 16-bit uncompressed RGBA5551
//! - OGL_RGB_565   (format 0x13 / 19): 16-bit uncompressed RGB565
//! - OGL_PVRTC4    (format 0x19 / 25): 4bpp PowerVR PVRTC1 texture compression

pub struct DecodedTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const PVR_FLAG_ALPHA: u32 = 0x8000;

pub fn decode_pvr(data: &[u8]) -> Result<DecodedTexture, String> {
    if data.len() < 52 {
        return Err("PVR data too short (< 52 bytes)".into());
    }
    let hdr_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let height = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let width = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let flags = u32::from_le_bytes(data[16..20].try_into().unwrap());
    let bpp = u32::from_le_bytes(data[24..28].try_into().unwrap());
    let tag = &data[44..48];
    if tag != b"PVR!" {
        return Err(format!("Invalid PVR magic: {:?}", tag));
    }
    if data.len() < hdr_len {
        return Err("PVR payload truncated".into());
    }
    let payload = &data[hdr_len..];
    let pixel_fmt = flags & 0xff;
    let has_alpha = (flags & PVR_FLAG_ALPHA) != 0;

    match pixel_fmt {
        // OGL_RGBA_4444
        16 => {
            let count = (width * height) as usize;
            if payload.len() < count * 2 {
                return Err("RGBA4444 payload too short".into());
            }
            let mut rgba = Vec::with_capacity(count * 4);
            for i in 0..count {
                let px = u16::from_le_bytes([payload[i * 2], payload[i * 2 + 1]]);
                let r = (((px >> 12) & 0x0f) as u8) * 17;
                let g = (((px >> 8) & 0x0f) as u8) * 17;
                let b = (((px >> 4) & 0x0f) as u8) * 17;
                let a = ((px & 0x0f) as u8) * 17;
                rgba.extend_from_slice(&[r, g, b, a]);
            }
            Ok(DecodedTexture {
                width,
                height,
                rgba,
            })
        }
        // OGL_RGBA_5551
        17 => {
            let count = (width * height) as usize;
            let mut rgba = Vec::with_capacity(count * 4);
            for i in 0..count {
                let px = u16::from_le_bytes([payload[i * 2], payload[i * 2 + 1]]);
                let r = (((px >> 11) & 0x1f) as u32 * 255 / 31) as u8;
                let g = (((px >> 6) & 0x1f) as u32 * 255 / 31) as u8;
                let b = (((px >> 1) & 0x1f) as u32 * 255 / 31) as u8;
                let a = if (px & 1) != 0 { 255 } else { 0 };
                rgba.extend_from_slice(&[r, g, b, a]);
            }
            Ok(DecodedTexture {
                width,
                height,
                rgba,
            })
        }
        // OGL_RGBA_8888
        18 => {
            let count = (width * height) as usize;
            let rgba = payload[..count * 4].to_vec();
            Ok(DecodedTexture {
                width,
                height,
                rgba,
            })
        }
        // OGL_RGB_565
        19 => {
            let count = (width * height) as usize;
            let mut rgba = Vec::with_capacity(count * 4);
            for i in 0..count {
                let px = u16::from_le_bytes([payload[i * 2], payload[i * 2 + 1]]);
                let r = (((px >> 11) & 0x1f) as u32 * 255 / 31) as u8;
                let g = (((px >> 5) & 0x3f) as u32 * 255 / 63) as u8;
                let b = ((px & 0x1f) as u32 * 255 / 31) as u8;
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
            Ok(DecodedTexture {
                width,
                height,
                rgba,
            })
        }
        // OGL_PVRTC4 (4bpp) or PVRTC fmt 12/13/24/25
        13 | 25 if bpp == 4 => {
            let rgba = decode_pvrtc4(payload, width as usize, height as usize, has_alpha)?;
            Ok(DecodedTexture {
                width,
                height,
                rgba,
            })
        }
        other => Err(format!(
            "Unsupported PVR pixel format {} (bpp={})",
            other, bpp
        )),
    }
}

#[derive(Clone, Copy, Default)]
struct Color32 {
    r: i32,
    g: i32,
    b: i32,
    a: i32,
}

#[derive(Clone, Copy)]
struct PvrtcPacket {
    modulation: u32,
    use_punchthrough: bool,
    color_a: Color32,
    color_b: Color32,
}

impl PvrtcPacket {
    fn from_u64(raw: u64, has_alpha_flag: bool) -> Self {
        let modulation = (raw & 0xffff_ffff) as u32;
        let word = (raw >> 32) as u32;
        let use_punchthrough = (word & 1) != 0;

        // Color A: bits [1..15], opacity bit 15
        let a_opaque = (word & (1 << 15)) != 0;
        let color_a = if a_opaque {
            // 5-5-4 RGB, alpha=15->255
            let r = ((word >> 10) & 0x1f) as i32;
            let g = ((word >> 5) & 0x1f) as i32;
            let b = ((word >> 1) & 0x0f) as i32;
            Color32 {
                r: (r << 3) | (r >> 2),
                g: (g << 3) | (g >> 2),
                b: (b << 4) | b,
                a: 255,
            }
        } else {
            // 3-4-4-3 ARGB
            let a = ((word >> 12) & 0x07) as i32;
            let r = ((word >> 8) & 0x0f) as i32;
            let g = ((word >> 4) & 0x0f) as i32;
            let b = ((word >> 1) & 0x07) as i32;
            let b4 = (b << 1) | (b >> 2);
            Color32 {
                r: (r << 4) | r,
                g: (g << 4) | g,
                b: (b4 << 4) | b4,
                a: if has_alpha_flag {
                    (a << 5) | (a << 2) | (a >> 1)
                } else {
                    255
                },
            }
        };

        // Color B: bits [16..31], opacity bit 31
        let b_opaque = (word & (1 << 31)) != 0;
        let color_b = if b_opaque {
            // 5-5-5 RGB
            let r = ((word >> 26) & 0x1f) as i32;
            let g = ((word >> 21) & 0x1f) as i32;
            let b = ((word >> 16) & 0x1f) as i32;
            Color32 {
                r: (r << 3) | (r >> 2),
                g: (g << 3) | (g >> 2),
                b: (b << 3) | (b >> 2),
                a: 255,
            }
        } else {
            // 3-4-4-4 ARGB
            let a = ((word >> 28) & 0x07) as i32;
            let r = ((word >> 24) & 0x0f) as i32;
            let g = ((word >> 20) & 0x0f) as i32;
            let b = ((word >> 16) & 0x0f) as i32;
            Color32 {
                r: (r << 4) | r,
                g: (g << 4) | g,
                b: (b << 4) | b,
                a: if has_alpha_flag {
                    (a << 5) | (a << 2) | (a >> 1)
                } else {
                    255
                },
            }
        };

        Self {
            modulation,
            use_punchthrough,
            color_a,
            color_b,
        }
    }
}

#[inline]
fn morton_interleave(x: usize, y: usize, blocks_x: usize, blocks_y: usize) -> usize {
    // PowerVR twiddling: interleave bits up to min(blocks_x, blocks_y), then linearize remaining high bits
    let min_dim = blocks_x.min(blocks_y);
    let mut offset = 0usize;
    let mut shift = 0usize;
    let mut bit = 1usize;
    while bit < min_dim {
        if (y & bit) != 0 {
            offset |= 1 << shift;
        }
        shift += 1;
        if (x & bit) != 0 {
            offset |= 1 << shift;
        }
        shift += 1;
        bit <<= 1;
    }
    if blocks_x > blocks_y {
        offset |= (x / min_dim) << shift;
    } else if blocks_y > blocks_x {
        offset |= (y / min_dim) << shift;
    }
    offset
}

fn decode_pvrtc4(
    payload: &[u8],
    width: usize,
    height: usize,
    has_alpha_flag: bool,
) -> Result<Vec<u8>, String> {
    let blocks_x = (width / 4).max(2);
    let blocks_y = (height / 4).max(2);
    let num_blocks = blocks_x * blocks_y;
    if payload.len() < num_blocks * 8 {
        return Err(format!(
            "PVRTC4 payload too small: got {}, need {}",
            payload.len(),
            num_blocks * 8
        ));
    }

    let mut packets = Vec::with_capacity(num_blocks);
    for i in 0..num_blocks {
        let raw = u64::from_le_bytes(payload[i * 8..i * 8 + 8].try_into().unwrap());
        packets.push(PvrtcPacket::from_u64(raw, has_alpha_flag));
    }

    let get_pkt = |bx: isize, by: isize| -> PvrtcPacket {
        let wx = bx.rem_euclid(blocks_x as isize) as usize;
        let wy = by.rem_euclid(blocks_y as isize) as usize;
        packets[morton_interleave(wx, wy, blocks_x, blocks_y)]
    };

    let mut rgba = vec![0u8; width * height * 4];

    for py in 0..height {
        // Bilinear block centers are at (bx*4 + 2, by*4 + 2)
        let fy = py as isize - 2;
        let by0 = fy.div_euclid(4);
        let dy = (fy - by0 * 4) as i32; // 0..3
        let by1 = by0 + 1;

        let blk_y = (py / 4) as isize;
        let sub_y = py & 3;

        for px in 0..width {
            let fx = px as isize - 2;
            let bx0 = fx.div_euclid(4);
            let dx = (fx - bx0 * 4) as i32; // 0..3
            let bx1 = bx0 + 1;

            let p00 = get_pkt(bx0, by0);
            let p10 = get_pkt(bx1, by0);
            let p01 = get_pkt(bx0, by1);
            let p11 = get_pkt(bx1, by1);

            let w00 = (4 - dx) * (4 - dy);
            let w10 = dx * (4 - dy);
            let w01 = (4 - dx) * dy;
            let w11 = dx * dy;

            let interp = |c00: Color32, c10: Color32, c01: Color32, c11: Color32| Color32 {
                r: (c00.r * w00 + c10.r * w10 + c01.r * w01 + c11.r * w11 + 8) >> 4,
                g: (c00.g * w00 + c10.g * w10 + c01.g * w01 + c11.g * w11 + 8) >> 4,
                b: (c00.b * w00 + c10.b * w10 + c01.b * w01 + c11.b * w11 + 8) >> 4,
                a: (c00.a * w00 + c10.a * w10 + c01.a * w01 + c11.a * w11 + 8) >> 4,
            };

            let ca = interp(p00.color_a, p10.color_a, p01.color_a, p11.color_a);
            let cb = interp(p00.color_b, p10.color_b, p01.color_b, p11.color_b);

            let cur_pkt = get_pkt((px / 4) as isize, blk_y);
            let bit_idx = (sub_y * 4 + (px & 3)) * 2;
            let mod_val = (cur_pkt.modulation >> bit_idx) & 0x03;

            let (r, g, b, a) = if cur_pkt.use_punchthrough {
                match mod_val {
                    0 => (ca.r, ca.g, ca.b, ca.a),
                    1 => (
                        (ca.r + cb.r) >> 1,
                        (ca.g + cb.g) >> 1,
                        (ca.b + cb.b) >> 1,
                        (ca.a + cb.a) >> 1,
                    ),
                    2 => (
                        (ca.r + cb.r) >> 1,
                        (ca.g + cb.g) >> 1,
                        (ca.b + cb.b) >> 1,
                        0,
                    ),
                    _ => (cb.r, cb.g, cb.b, cb.a),
                }
            } else {
                let (wa, wb) = match mod_val {
                    0 => (8, 0),
                    1 => (5, 3),
                    2 => (3, 5),
                    _ => (0, 8),
                };
                (
                    (ca.r * wa + cb.r * wb + 4) >> 3,
                    (ca.g * wa + cb.g * wb + 4) >> 3,
                    (ca.b * wa + cb.b * wb + 4) >> 3,
                    (ca.a * wa + cb.a * wb + 4) >> 3,
                )
            };

            // Note: OpenGL PVRTC textures in iOS store row 0 at bottom OR top depending on exporter;
            // we verify orientation against INGAME_2.dat sprite rects right below!
            let idx = (py * width + px) * 4;
            rgba[idx] = r.clamp(0, 255) as u8;
            rgba[idx + 1] = g.clamp(0, 255) as u8;
            rgba[idx + 2] = b.clamp(0, 255) as u8;
            rgba[idx + 3] = a.clamp(0, 255) as u8;
        }
    }

    Ok(rgba)
}
