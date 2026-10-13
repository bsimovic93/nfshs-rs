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
/// window (the PS1 screen, `screen` pixels wide and high (the front end uses 512x240), shown
/// at 4:3 as a television does).
pub struct FeFrame {
    pub atlas: Atlas,
    pub vertices: Vec<Vertex>,
    /// Vertex ranges drawn subtracting (semi-transparency mode 2).
    pub subtract: Vec<std::ops::Range<u32>>,
    /// Vertices before this index are drawn before the showroom car (the rest over it).
    pub under: usize,
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
    pub fn build(cmds: &[OverlayCmd], vram: &[u16], screen: (u16, u16), w: f32, h: f32) -> Self {
        Self::build_in(Atlas::new(1024), cmds, vram, screen, w, h)
    }

    /// As [`build`](Self::build), adding the textures to `atlas` (which may hold others, such
    /// as the showroom car's).
    pub fn build_in(mut atlas: Atlas, cmds: &[OverlayCmd], vram: &[u16], screen: (u16, u16), w: f32, h: f32) -> Self {
        let mut under = 0;
        let mut car_seen = false;
        // untextured semi-transparent quads blend as a whole: a texel with the semi bit
        let white_semi = atlas.add(1, 1, &[255, 255, 255, 128]);
        let mut cache: HashMap<(u16, u16, u8, u8, u8, u8), Rect> = HashMap::new();
        let (sw, sh) = (screen.0.max(1) as f32, screen.1.max(1) as f32);
        let (aw, ah) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 0.75) };
        let (scale_x, scale_y) = (aw / sw, ah / sh);
        let (ox, oy) = ((w - aw) / 2.0, (h - ah) / 2.0);
        let size = atlas.size as f32;
        let mut vertices = Vec::new();
        let mut subtract: Vec<std::ops::Range<u32>> = Vec::new();
        let mut clip = [0.0, 0.0, sw, sh];
        // the game adds each primitive at the head of its ordering table: last asked, first drawn
        for cmd in cmds.iter().rev() {
            if matches!(cmd, OverlayCmd::ShowroomCar { .. }) && !car_seen {
                car_seen = true;
                under = vertices.len();
                continue;
            }
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
                None => (if *semi { white_semi } else { atlas.white }, None, 510.0, match (*semi, abr) {
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
            let place = |v: Vertex| Vertex { pos: [ox + (v.pos[0] + shift) * scale_x, oy + (v.pos[1] + shift) * scale_y, 0.0], ..v };
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
        if !car_seen {
            under = 0;
        }
        Self { atlas, vertices, subtract, under }
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

/// The movie the front end is playing: decoded frame by frame as they are shown.
pub struct MoviePlayer {
    pub file: String,
    movie: crate::formats::str::Movie,
    decoder: crate::formats::str::Decoder,
    shown: Option<(u32, Vec<u8>)>,
}

impl MoviePlayer {
    pub fn load(data: &crate::assets::GameData, file: &str) -> Option<Self> {
        let bytes = data.read(&format!("MOVIES/{file}")).ok()?;
        let movie = crate::formats::str::Movie::parse(&bytes);
        Some(Self { file: file.to_string(), movie, decoder: Default::default(), shown: None })
    }

    /// The soundtrack from `frame` on (frames at `fps`).
    pub fn audio_from(&self, frame: u32, fps: f32) -> crate::formats::audio::Sample {
        let ch = if self.movie.stereo { 2 } else { 1 };
        let start = ((frame as f32 / fps.max(1.0) * self.movie.rate as f32) as usize * ch).min(self.movie.audio.len());
        crate::formats::audio::Sample { rate: self.movie.rate, channels: ch as u32, pcm: self.movie.audio[start..].to_vec(), loop_start: None, loop_end: None }
    }

    /// Frame `frame` filling a `w` x `h` window at 4:3.
    pub fn frame(&mut self, frame: u32, w: f32, h: f32) -> FeFrame {
        let (mw, mh) = (self.movie.width, self.movie.height);
        if self.shown.as_ref().is_none_or(|s| s.0 != frame) {
            let rgba = self.movie.frames.get(frame as usize).map(|bs| self.decoder.frame(bs, mw, mh)).unwrap_or_else(|| vec![0; (mw * mh * 4) as usize]);
            self.shown = Some((frame, rgba));
        }
        let mut atlas = Atlas::new(512);
        let r = atlas.add(mw, mh, &self.shown.as_ref().unwrap().1);
        let size = atlas.size as f32;
        let (aw, ah) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 0.75) };
        let (ox, oy) = ((w - aw) / 2.0, (h - ah) / 2.0);
        let corner = |cx: f32, cy: f32| Vertex {
            pos: [ox + cx * aw, oy + cy * ah, 0.0],
            uv: [(r.x as f32 + cx * r.w as f32) / size, (r.y as f32 + cy * r.h as f32) / size],
            color: [0.5, 0.5, 0.5, 1.0],
        };
        let q = [corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)];
        FeFrame { atlas, vertices: vec![q[0], q[1], q[2], q[0], q[2], q[3]], subtract: Vec::new(), under: 0 }
    }
}

/// The showroom's car model (with its textures in an atlas the front end's are added to).
pub struct Showroom {
    /// The car's code and paint.
    pub car: (String, u8),
    model: crate::assets::CarModel,
    pub atlas: Atlas,
}

impl Showroom {
    pub fn load(data: &crate::assets::GameData, car: &(String, u8)) -> Option<Self> {
        let mut atlas = Atlas::new(1024);
        let model = crate::assets::load_car(data, &mut atlas, &car.0, true, false, None, car.1).ok()?;
        Some(Self { car: car.clone(), model, atlas })
    }

    /// The cars the front end asked for (the car and its reflection in the floor) as world
    /// geometry, and the projection that puts them where the GTE would on the front end's
    /// `screen`, fitted at 4:3 into a `w` x `h` window.
    pub fn geometry(&self, cmds: &[OverlayCmd], screen: (u16, u16), w: f32, h: f32) -> Option<(Vec<Vertex>, glam::Mat4)> {
        use crate::game::original::{pose, v3};
        use glam::{Mat4, Vec3, Vec4};
        let mut out = Vec::new();
        let mut proj = None;
        for cmd in cmds {
            let OverlayCmd::ShowroomCar { car, parts, cam_pos, cam_rot, h: dist, ofx, ofy, mirrored, level } = cmd else { continue };
            if proj.is_none() {
                let raw = |i: usize| v3([cam_rot[i * 3], cam_rot[i * 3 + 1], cam_rot[i * 3 + 2]]);
                let (up, fwd) = (raw(1).normalize_or_zero(), raw(2).normalize_or_zero());
                let ys = (raw(1).length() / raw(2).length().max(1e-6)).clamp(0.2, 2.0);
                let pos = v3(*cam_pos);
                let view = Mat4::look_at_rh(pos, pos + fwd, up);
                let (sw, sh) = (screen.0.max(1) as f32, screen.1.max(1) as f32);
                let (aw, ah) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 0.75) };
                let (ox, oy) = ((w - aw) / 2.0, (h - ah) / 2.0);
                let (kx, ky) = (aw / sw, ah / sh);
                let hd = *dist as f32;
                // screen x = OFX + H x / z, y = OFY - H ys y / z (camera space, z forward),
                // then into window NDC; depth reversed (near / distance)
                let (fx, fy) = (*ofx as f32 / 65536.0, *ofy as f32 / 65536.0);
                let cx = (ox + kx * fx) * 2.0 / w - 1.0;
                let cy = 1.0 - (oy + ky * fy) * 2.0 / h;
                let p = Mat4::from_cols(
                    Vec4::new(2.0 * kx * hd / w, 0.0, 0.0, 0.0),
                    Vec4::new(0.0, 2.0 * ky * hd * ys / h, 0.0, 0.0),
                    Vec4::new(-cx, -cy, 0.0, -1.0),
                    Vec4::new(0.0, 0.0, 0.1, 0.0),
                );
                proj = Some(p * view);
            }
            // the car as in a race (model origin on the ground); its reflection mirrored in
            // the floor (y = 0) and darker
            let mut c = (**car).clone();
            if *mirrored {
                c.position[1] = -c.position[1];
            }
            let ps = pose(&c);
            let mut model = Mat4::from_rotation_translation(ps.rot, ps.pos);
            // lit by the level the game gives it (128 = as textured; halved in the reflection)
            let tint = *level as f32 / 128.0;
            if *mirrored {
                model = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * model;
            }
            for (pi, (part, &on)) in self.model.parts.iter().zip(parts.iter()).enumerate() {
                if on == 0 {
                    continue;
                }
                let tm = if part.wheel.is_some() { model * Mat4::from_translation(part.hub) } else { model };
                let _ = pi;
                for v in &part.verts {
                    let p = tm.transform_point3(Vec3::from(v.pos));
                    let col = v.color;
                    out.push(Vertex { pos: p.into(), color: [col[0] * tint, col[1] * tint, col[2] * tint, col[3]], ..*v });
                }
            }
        }
        proj.map(|p| (out, p))
    }
}

/// A front-end frame ready for the renderer: its atlas, the 2D under and over the showroom
/// car (with the subtracting ranges of the part over it), and the car's geometry and
/// projection when there is one.
pub struct FeScene {
    pub atlas: Atlas,
    pub under: Vec<Vertex>,
    pub over: Vec<Vertex>,
    pub subtract: Vec<std::ops::Range<u32>>,
    pub car: Vec<Vertex>,
    pub view_proj: Option<glam::Mat4>,
}

impl FeScene {
    pub fn build(cmds: &[OverlayCmd], vram: &[u16], screen: (u16, u16), w: f32, h: f32, showroom: Option<&Showroom>) -> Self {
        let base = showroom.map(|s| s.atlas.clone()).unwrap_or_else(|| Atlas::new(1024));
        let f = FeFrame::build_in(base, cmds, vram, screen, w, h);
        let (car, view_proj) = match showroom.and_then(|s| s.geometry(cmds, screen, w, h)) {
            Some((v, p)) => (v, Some(p)),
            None => (Vec::new(), None),
        };
        let split = if view_proj.is_some() { f.under } else { 0 };
        let mut atlas = f.atlas;
        let mut car = car;
        if view_proj.is_some() {
            car.extend(floor(cmds, vram, &mut atlas));
        }
        let mut vertices = f.vertices;
        let over = vertices.split_off(split);
        let subtract = f
            .subtract
            .iter()
            .filter(|r| r.start as usize >= split)
            .map(|r| (r.start - split as u32)..(r.end - split as u32))
            .collect();
        Self { atlas, under: vertices, over, subtract, car, view_proj }
    }

    /// The showroom car's code and paint, when the front end shows one.
    pub fn car_code(cmds: &[OverlayCmd]) -> Option<(String, u8)> {
        cmds.iter().find_map(|c| match c {
            OverlayCmd::ShowroomCar { car, .. } => Some((car.code.clone(), car.colour)),
            _ => None,
        })
    }
}

/// The showroom's turntable ring as world geometry (after the cars: drawn additively over
/// them), its texture decoded from VRAM into `atlas`.
fn floor(cmds: &[OverlayCmd], vram: &[u16], atlas: &mut Atlas) -> Vec<Vertex> {
    let mut out = Vec::new();
    for cmd in cmds {
        let OverlayCmd::ShowroomFloor { ring, tex, base, lights, rim, spot } = cmd else { continue };
        let clut = (tex[0] >> 16) as u16;
        let tpage = (tex[1] >> 16) as u16;
        let uv: [[u8; 2]; 4] = std::array::from_fn(|i| [tex[i] as u8, (tex[i] >> 8) as u8]);
        let (umin, umax) = (uv.iter().map(|t| t[0]).min().unwrap(), uv.iter().map(|t| t[0]).max().unwrap());
        let (vmin, vmax) = (uv.iter().map(|t| t[1]).min().unwrap(), uv.iter().map(|t| t[1]).max().unwrap());
        let (tw, th) = ((umax - umin) as u32 + 1, (vmax - vmin) as u32 + 1);
        let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
        for v in 0..th {
            for u in 0..tw {
                rgba.extend(texel(vram, tpage, clut, umin as u32 + u, vmin as u32 + v));
            }
        }
        let r = atlas.add(tw, th, &rgba);
        let size = atlas.size as f32;
        let tc = |k: usize| [(r.x as f32 + (uv[k][0] - umin) as f32) / size, (r.y as f32 + (uv[k][1] - vmin) as f32) / size];
        // engine space: PS1 x mirrored, 1/256 m
        let p = |v: [i16; 3]| [-(v[0] as f32) / 256.0, v[1] as f32 / 256.0, v[2] as f32 / 256.0];
        let additive = (tpage >> 5) & 3 == 1;
        for t in 0..32usize {
            let (a, b) = (2 * t, (2 * t + 2) % 64);
            let corners = [p(ring[a]), p(ring[b]), p(ring[a + 1]), p(ring[b + 1])];
            for colour in std::iter::once(*base).chain(lights[t]) {
                let c = [(colour & 0xFF) as f32 / 256.0, ((colour >> 8) & 0xFF) as f32 / 256.0, ((colour >> 16) & 0xFF) as f32 / 256.0, if additive { 0.0 } else { 0.5 }];
                for k in [0, 1, 3, 0, 3, 2] {
                    out.push(Vertex { pos: corners[k], uv: tc(k), color: c });
                }
            }
        }
        // the spot's fan (POLY_G3, added; untextured, through a white semi-transparent texel),
        // after the ring, where its rim is black: both lie on the floor and the later one fails
        // the depth test
        let white = atlas.add(1, 1, &[255, 255, 255, 128]);
        let wuv = [(white.x as f32 + 0.5) / size, (white.y as f32 + 0.5) / size];
        let centre = [(*spot & 0xFF) as f32 / 510.0, ((*spot >> 8) & 0xFF) as f32 / 510.0, ((*spot >> 16) & 0xFF) as f32 / 510.0, 0.0];
        for t in 0..32usize {
            let (a, b) = (rim[t], rim[(t + 1) % 32]);
            out.push(Vertex { pos: [0.0; 3], uv: wuv, color: centre });
            out.push(Vertex { pos: p(a), uv: wuv, color: [0.0; 4] });
            out.push(Vertex { pos: p(b), uv: wuv, color: [0.0; 4] });
        }
    }
    out
}

