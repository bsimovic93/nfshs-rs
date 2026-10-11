//! Minimal HUD text using a built-in 5x7 font baked into the atlas.

use crate::assets::{Atlas, Rect, Vertex};

const GLYPHS: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ:./-% ";
// 7 rows of 5 bits per glyph, MSB = leftmost column
const FONT: [[u8; 7]; 42] = [
    [14, 17, 19, 21, 25, 17, 14], // 0
    [4, 12, 4, 4, 4, 4, 14],      // 1
    [14, 17, 1, 2, 4, 8, 31],     // 2
    [31, 2, 4, 2, 1, 17, 14],     // 3
    [2, 6, 10, 18, 31, 2, 2],     // 4
    [31, 16, 30, 1, 1, 17, 14],   // 5
    [6, 8, 16, 30, 17, 17, 14],   // 6
    [31, 1, 2, 4, 8, 8, 8],       // 7
    [14, 17, 17, 14, 17, 17, 14], // 8
    [14, 17, 17, 15, 1, 2, 12],   // 9
    [14, 17, 17, 31, 17, 17, 17], // A
    [30, 17, 17, 30, 17, 17, 30], // B
    [14, 17, 16, 16, 16, 17, 14], // C
    [28, 18, 17, 17, 17, 18, 28], // D
    [31, 16, 16, 30, 16, 16, 31], // E
    [31, 16, 16, 30, 16, 16, 16], // F
    [14, 17, 16, 23, 17, 17, 15], // G
    [17, 17, 17, 31, 17, 17, 17], // H
    [14, 4, 4, 4, 4, 4, 14],      // I
    [7, 2, 2, 2, 2, 18, 12],      // J
    [17, 18, 20, 24, 20, 18, 17], // K
    [16, 16, 16, 16, 16, 16, 31], // L
    [17, 27, 21, 21, 17, 17, 17], // M
    [17, 17, 25, 21, 19, 17, 17], // N
    [14, 17, 17, 17, 17, 17, 14], // O
    [30, 17, 17, 30, 16, 16, 16], // P
    [14, 17, 17, 17, 21, 18, 13], // Q
    [30, 17, 17, 30, 20, 18, 17], // R
    [15, 16, 16, 14, 1, 1, 30],   // S
    [31, 4, 4, 4, 4, 4, 4],       // T
    [17, 17, 17, 17, 17, 17, 14], // U
    [17, 17, 17, 17, 17, 10, 4],  // V
    [17, 17, 17, 21, 21, 21, 10], // W
    [17, 17, 10, 4, 10, 17, 17],  // X
    [17, 17, 10, 4, 4, 4, 4],     // Y
    [31, 1, 2, 4, 8, 16, 31],     // Z
    [0, 12, 12, 0, 12, 12, 0],    // :
    [0, 0, 0, 0, 0, 12, 12],      // .
    [1, 1, 2, 4, 8, 16, 16],      // /
    [0, 0, 0, 31, 0, 0, 0],       // -
    [24, 25, 2, 4, 8, 19, 3],     // %
    [0, 0, 0, 0, 0, 0, 0],        // space
];

pub struct Font {
    rect: Rect,
    atlas_size: f32,
}

impl Font {
    pub fn new(atlas: &mut Atlas) -> Self {
        let w = 6 * GLYPHS.len() as u32;
        let h = 8u32;
        let mut px = vec![0u8; (w * h * 4) as usize];
        for (g, rows) in FONT.iter().enumerate() {
            for (y, row) in rows.iter().enumerate() {
                for x in 0..5 {
                    if row & (16 >> x) != 0 {
                        let i = ((y as u32 * w + g as u32 * 6 + x) * 4) as usize;
                        px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
        let rect = atlas.add(w, h, &px);
        Self { rect, atlas_size: atlas.size as f32 }
    }

    /// Append quads for `text` at pixel position (x, y) with `scale` pixels per font texel.
    pub fn text(&self, out: &mut Vec<Vertex>, text: &str, x: f32, y: f32, scale: f32, color: [f32; 4]) {
        let c = [color[0] * 0.5, color[1] * 0.5, color[2] * 0.5, color[3]];
        for (i, ch) in text.to_ascii_uppercase().chars().enumerate() {
            let Some(g) = GLYPHS.find(ch) else { continue };
            let u0 = (self.rect.x + g as u32 * 6) as f32 / self.atlas_size;
            let u1 = (self.rect.x + g as u32 * 6 + 5) as f32 / self.atlas_size;
            let v0 = self.rect.y as f32 / self.atlas_size;
            let v1 = (self.rect.y + 7) as f32 / self.atlas_size;
            let x0 = x + i as f32 * 6.0 * scale;
            let (x1, y1) = (x0 + 5.0 * scale, y + 7.0 * scale);
            // drop shadow then glyph
            for (off, col) in [(scale * 0.6, [0.0, 0.0, 0.0, color[3]]), (0.0, c)] {
                let q = [
                    ([x0 + off, y + off], [u0, v0]),
                    ([x1 + off, y + off], [u1, v0]),
                    ([x1 + off, y1 + off], [u1, v1]),
                    ([x0 + off, y1 + off], [u0, v1]),
                ];
                for k in [0, 1, 2, 0, 2, 3] {
                    out.push(Vertex { pos: [q[k].0[0], q[k].0[1], 0.0], uv: q[k].1, color: col });
                }
            }
        }
    }
}
