//! `SHPP` / `GIMX` shape files (`.PSH`): PS1 VRAM images with CLUTs.
//!
//! Layout (little-endian):
//! ```text
//! "SHPP" u32 file_size u32 count "GIMX"
//! count × { char[4] name, u32 offset }
//! at offset: chain of blocks, each 16-byte header
//!   u8 type, u24 next (relative, 0 = last), u16 w, u16 h,
//!   i16 center_x, i16 center_y, u16 vram_x, u16 vram_y
//! types: 0x40 = 4bpp indexed, 0x41 = 8bpp indexed, 0x42 = 16bpp direct,
//!        0x23 = 16-bit CLUT (palette) for the preceding image
//! ```

use super::{i16le, u16le, u32le};
use anyhow::{Result, bail};

#[derive(Clone)]
pub struct Shape {
    pub name: String,
    pub kind: u8,
    pub width: u16,
    pub height: u16,
    pub center: (i16, i16),
    /// Placement hint inside the original texture page (in pixels of this image's depth).
    pub vram: (u16, u16),
    /// RGBA8 pixels.
    pub rgba: Vec<u8>,
    /// Indexed pixels (for 4/8bpp shapes), kept for palette-swapping.
    pub indices: Vec<u8>,
    pub palette: Vec<u16>,
}

pub struct Psh {
    pub shapes: Vec<Shape>,
}

/// Convert a PS1 15-bit colour to RGBA. Pure black with STP clear is the transparent colour.
pub fn ps1_color(c: u16) -> [u8; 4] {
    let r = (c & 31) as u32;
    let g = ((c >> 5) & 31) as u32;
    let b = ((c >> 10) & 31) as u32;
    // 0x0000 is transparent; texels with the STP bit (0x8000) blend in semi-transparent
    // primitives, kept as alpha 128
    let a = if c == 0 { 0 } else if c & 0x8000 != 0 { 128 } else { 255 };
    [(r * 255 / 31) as u8, (g * 255 / 31) as u8, (b * 255 / 31) as u8, a]
}

impl Psh {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 16 || &d[..4] != b"SHPP" {
            bail!("not a SHPP file");
        }
        let count = u32le(d, 8) as usize;
        let mut shapes = Vec::with_capacity(count);
        for i in 0..count {
            let name_raw = &d[16 + i * 8..20 + i * 8];
            let name = String::from_utf8_lossy(name_raw).into_owned();
            let off = u32le(d, 20 + i * 8) as usize;
            shapes.push(Self::parse_shape(d, off, name)?);
        }
        Ok(Self { shapes })
    }

    pub fn parse_shape(d: &[u8], off: usize, name: String) -> Result<Shape> {
        let kind = d[off];
        let next = (u32le(d, off) >> 8) as usize;
        let w = u16le(d, off + 4);
        let h = u16le(d, off + 6);
        let center = (i16le(d, off + 8), i16le(d, off + 10));
        let vram = (u16le(d, off + 12), u16le(d, off + 14));
        let px = off + 16;
        // Find palette block in the chain.
        let mut palette = Vec::new();
        let mut b = off;
        let mut nx = next;
        while nx != 0 {
            b += nx;
            if b + 16 > d.len() {
                break;
            }
            let t = d[b];
            nx = (u32le(d, b) >> 8) as usize;
            if t == 0x23 || t == 0x24 || t == 0x22 {
                let pw = u16le(d, b + 4) as usize;
                let ph = u16le(d, b + 6) as usize;
                let n = pw * ph;
                palette = (0..n).map(|k| u16le(d, b + 16 + k * 2)).collect();
                break;
            }
        }
        let (wu, hu) = (w as usize, h as usize);
        let mut rgba = vec![0u8; wu * hu * 4];
        let mut indices = Vec::new();
        match kind {
            0x40 => {
                // VRAM rows are whole 16-bit words: 4 pixels per word at 4bpp
                let stride = wu.div_ceil(4) * 2;
                indices = vec![0u8; wu * hu];
                for y in 0..hu {
                    for x in 0..wu {
                        let byte = d.get(px + y * stride + x / 2).copied().unwrap_or(0);
                        indices[y * wu + x] = if x & 1 == 0 { byte & 15 } else { byte >> 4 };
                    }
                }
            }
            0x41 => {
                let stride = wu.div_ceil(2) * 2;
                indices = vec![0u8; wu * hu];
                for y in 0..hu {
                    for x in 0..wu {
                        indices[y * wu + x] = d.get(px + y * stride + x).copied().unwrap_or(0);
                    }
                }
            }
            0x42 => {
                for i in 0..wu * hu {
                    let c = if px + i * 2 + 1 < d.len() { u16le(d, px + i * 2) } else { 0 };
                    rgba[i * 4..i * 4 + 4].copy_from_slice(&ps1_color(c));
                }
            }
            _ => {}
        }
        if !indices.is_empty() {
            for (i, &ix) in indices.iter().enumerate() {
                let c = palette.get(ix as usize).copied().unwrap_or(0);
                rgba[i * 4..i * 4 + 4].copy_from_slice(&ps1_color(c));
            }
        }
        Ok(Shape { name, kind, width: w, height: h, center, vram, rgba, indices, palette })
    }

    pub fn get(&self, name: &str) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.name == name)
    }
}

/// Pixels-per-VRAM-word for a shape kind (VRAM x coordinates are in 16-bit units).
pub fn pixels_per_word(kind: u8) -> u32 {
    match kind {
        0x40 => 4,
        0x41 => 2,
        _ => 1,
    }
}

/// A 256x256 texture page assembled from shapes using their VRAM placement.
pub struct Atlas {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub fn new(size: u32) -> Self {
        Self { size, rgba: vec![0; (size * size * 4) as usize] }
    }

    /// Blit shapes at their VRAM position, wrapped into the page. 1x1 dummy palette holders are skipped.
    pub fn place(&mut self, shapes: &[Shape], page_w_words: u32) {
        for s in shapes {
            if s.width <= 1 && s.height <= 1 {
                continue;
            }
            let ppw = pixels_per_word(s.kind);
            let x0 = (s.vram.0 as u32 % page_w_words) * ppw;
            let y0 = s.vram.1 as u32 % self.size;
            self.blit(s, x0, y0);
        }
    }

    pub fn blit(&mut self, s: &Shape, x0: u32, y0: u32) {
        for y in 0..s.height as u32 {
            for x in 0..s.width as u32 {
                let (dx, dy) = (x0 + x, y0 + y);
                if dx >= self.size || dy >= self.size {
                    continue;
                }
                let si = ((y * s.width as u32 + x) * 4) as usize;
                let di = ((dy * self.size + dx) * 4) as usize;
                self.rgba[di..di + 4].copy_from_slice(&s.rgba[si..si + 4]);
            }
        }
    }
}
