//! The original front end's frames: the quads its drawing routines asked for this frame
//! (recorded by the host from the shape emitter, the square routines and Font_TextXY), drawn
//! with their textures taken from the VRAM the front end loaded them into.

use std::collections::HashMap;

use glam::Vec2;

use crate::assets::{Atlas, Rect, Vertex};
use crate::formats::psh::ps1_color;
use crate::ps1::host::{OverlayCmd, QuadTex};

const VRAM_W: usize = 1024;

/// One front-end frame ready to draw: its texture atlas and its vertices for a `w` x `h`
/// window (the PS1's 320x240 screen fitted at 4:3).
pub struct FeFrame {
    pub atlas: Atlas,
    pub vertices: Vec<Vertex>,
}

/// Texel `(u, v)` of texture page `tpage` through CLUT `clut`, as RGBA (alpha 0 transparent,
/// 128 semi-transparent texel, 255 opaque).
fn texel(vram: &[u16], tpage: u16, clut: u16, u: u32, v: u32) -> [u8; 4] {
    let px = (tpage & 0xF) as usize * 64;
    let py = ((tpage >> 4) & 1) as usize * 256 + v as usize;
    let at = |x: usize, y: usize| vram.get((y % 512) * VRAM_W + (x % VRAM_W)).copied().unwrap_or(0);
    let lookup = |i: u16| at((clut & 0x3F) as usize * 16 + i as usize, (clut >> 6) as usize & 0x1FF);
    let c = match (tpage >> 7) & 3 {
        0 => lookup((at(px + u as usize / 4, py) >> ((u % 4) * 4)) & 0xF),
        1 => lookup((at(px + u as usize / 2, py) >> ((u % 2) * 8)) & 0xFF),
        _ => at(px + u as usize, py),
    };
    ps1_color(c)
}

impl FeFrame {
    pub fn build(cmds: &[OverlayCmd], vram: &[u16], w: f32, h: f32) -> Self {
        let mut atlas = Atlas::new(1024);
        let mut cache: HashMap<(u16, u16, u8, u8, u8, u8), Rect> = HashMap::new();
        let scale = (h / 240.0).min(w / 320.0);
        let (ox, oy) = ((w - 320.0 * scale) / 2.0, (h - 240.0 * scale) / 2.0);
        let size = atlas.size as f32;
        let mut vertices = Vec::new();
        // the game adds each primitive at the head of its ordering table: last asked, first drawn
        for cmd in cmds.iter().rev() {
            let OverlayCmd::Quad { pts, colours, tex, semi } = cmd else { continue };
            let pts: [Vec2; 4] = pts.map(|[x, y]| Vec2::new(x as f32, y as f32));
            let (rect, uv0, div, alpha) = match tex {
                Some(QuadTex { tpage, clut, uv }) => {
                    let (umin, umax) = (uv.iter().map(|t| t[0]).min().unwrap(), uv.iter().map(|t| t[0]).max().unwrap());
                    let (vmin, vmax) = (uv.iter().map(|t| t[1]).min().unwrap(), uv.iter().map(|t| t[1]).max().unwrap());
                    let key = (tpage & 0x19F, *clut, umin, vmin, umax, vmax);
                    let r = *cache.entry(key).or_insert_with(|| {
                        let (tw, th) = ((umax - umin) as u32 + 1, (vmax - vmin) as u32 + 1);
                        let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
                        for v in 0..th {
                            for u in 0..tw {
                                rgba.extend(texel(vram, *tpage, *clut, umin as u32 + u, vmin as u32 + v));
                            }
                        }
                        atlas.add(tw, th, &rgba)
                    });
                    // blend mode of semi-transparent texels: the page's (as the HUD: half or add)
                    let alpha = match (*semi, (tpage >> 5) & 3) {
                        (false, _) => 1.0,
                        (true, 1) => 0.0,
                        _ => 0.5,
                    };
                    (r, Some((*uv, umin, vmin)), 256.0, alpha)
                }
                None => (atlas.white, None, 510.0, if *semi { 0.5 } else { 1.0 }),
            };
            // the PS1 fills pixels whose top-left corner is inside, the GPU those whose centre
            // is: shift untextured quads by half a pixel
            let shift = if uv0.is_some() { 0.0 } else { 0.5 };
            let corner = |k: usize| {
                let p = pts[k] + Vec2::splat(shift);
                let t = match uv0 {
                    Some((uv, umin, vmin)) => {
                        let tx = rect.x as f32 + (uv[k][0] - umin) as f32;
                        let ty = rect.y as f32 + (uv[k][1] - vmin) as f32;
                        [tx / size, ty / size]
                    }
                    None => [(rect.x as f32 + 0.5) / size, (rect.y as f32 + 0.5) / size],
                };
                let c = colours[k];
                Vertex {
                    pos: [ox + p.x * scale, oy + p.y * scale, 0.0],
                    uv: t,
                    color: [(c & 0xFF) as f32 / div, ((c >> 8) & 0xFF) as f32 / div, ((c >> 16) & 0xFF) as f32 / div, alpha],
                }
            };
            for k in [0, 1, 3, 0, 3, 2] {
                vertices.push(corner(k));
            }
        }
        Self { atlas, vertices }
    }
}
