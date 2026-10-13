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
    /// Vertex ranges drawn subtracting (semi-transparency mode 2).
    pub subtract: Vec<std::ops::Range<u32>>,
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
        let mut subtract: Vec<std::ops::Range<u32>> = Vec::new();
        let mut clip = [0.0, 0.0, 320.0, 240.0];
        // the game adds each primitive at the head of its ordering table: last asked, first drawn
        for cmd in cmds.iter().rev() {
            if let &OverlayCmd::Clip { x, y, w, h } = cmd {
                clip = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
                continue;
            }
            let &OverlayCmd::Quad { ref pts, ref colours, ref tex, semi, abr } = cmd else { continue };
            let semi = &semi;
            let sub = *semi && abr == 2;
            let start = vertices.len() as u32;
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
                    // semi-transparent texels: half or added (subtracted in its own pass)
                    let alpha = match (*semi, abr) {
                        (false, _) => 1.0,
                        (true, 1) | (true, 3) => 0.0,
                        _ => 0.5,
                    };
                    (r, Some((*uv, umin, vmin)), 256.0, alpha)
                }
                None => (atlas.white, None, 510.0, match (*semi, abr) {
                    (false, _) => 1.0,
                    (true, 1) | (true, 3) => 0.0,
                    _ => 0.5,
                }),
            };
            // the PS1 fills pixels whose top-left corner is inside, the GPU those whose centre
            // is: shift untextured quads by half a pixel
            let shift = if uv0.is_some() { 0.0 } else { 0.5 };
            let corner = |k: usize| {
                let p = pts[k];
                let t = match uv0 {
                    Some((uv, umin, vmin)) => {
                        let tx = rect.x as f32 + (uv[k][0] - umin) as f32;
                        let ty = rect.y as f32 + (uv[k][1] - vmin) as f32;
                        [tx / size, ty / size]
                    }
                    None => [(rect.x as f32 + 0.5) / size, (rect.y as f32 + 0.5) / size],
                };
                let c = colours[k];
                Vertex { pos: [p.x, p.y, 0.0], uv: t, color: [(c & 0xFF) as f32 / div, ((c >> 8) & 0xFF) as f32 / div, ((c >> 16) & 0xFF) as f32 / div, alpha] }
            };
            // clipped to the drawing area, in PS1 screen pixels, then placed in the window
            let poly = clip_polygon(vec![corner(0), corner(1), corner(3), corner(2)], clip);
            let place = |v: Vertex| Vertex { pos: [ox + (v.pos[0] + shift) * scale, oy + (v.pos[1] + shift) * scale, 0.0], ..v };
            for k in 1..poly.len().saturating_sub(1) {
                vertices.extend([place(poly[0]), place(poly[k]), place(poly[k + 1])]);
            }
            if sub {
                let end = vertices.len() as u32;
                match subtract.last_mut() {
                    Some(r) if r.end == start => r.end = end,
                    _ => subtract.push(start..end),
                }
            }
        }
        Self { atlas, vertices, subtract }
    }
}

/// Sutherland-Hodgman clipping of a convex polygon against the rectangle [x0, y0, x1, y1],
/// interpolating texture coordinates and colours.
fn clip_polygon(mut poly: Vec<Vertex>, r: [f32; 4]) -> Vec<Vertex> {
    let lerp = |a: &Vertex, b: &Vertex, t: f32| Vertex {
        pos: std::array::from_fn(|i| a.pos[i] + (b.pos[i] - a.pos[i]) * t),
        uv: std::array::from_fn(|i| a.uv[i] + (b.uv[i] - a.uv[i]) * t),
        color: std::array::from_fn(|i| a.color[i] + (b.color[i] - a.color[i]) * t),
    };
    // (axis, bound, keep the side >= bound)
    for (axis, bound, ge) in [(0, r[0], true), (0, r[2], false), (1, r[1], true), (1, r[3], false)] {
        if poly.is_empty() {
            break;
        }
        let inside = |v: &Vertex| if ge { v.pos[axis] >= bound } else { v.pos[axis] <= bound };
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (&poly[i], &poly[(i + 1) % poly.len()]);
            if inside(a) {
                out.push(*a);
            }
            if inside(a) != inside(b) {
                let t = (bound - a.pos[axis]) / (b.pos[axis] - a.pos[axis]);
                out.push(lerp(a, b, t));
            }
        }
        poly = out;
    }
    poly
}

