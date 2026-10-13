//! The original front end's frames: the quads its drawing routines asked for this frame
//! (recorded by the host from the shape emitter, the square routines and Font_TextXY), drawn
//! with their textures taken from the VRAM the front end loaded them into.

use std::collections::HashMap;

use glam::Vec2;

use crate::assets::{Atlas, Rect, Vertex};
use crate::formats::psh::ps1_color;
use crate::ps1::host::{CarEnv, OverlayCmd, QuadTex};

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
    /// The showroom floor's vertices (just before `under`) start here.
    pub floor: usize,
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
        // the game adds each primitive at the head of its ordering table: last asked, first
        // drawn; the showroom's floor goes right before its cars (the car cache's first slot)
        let floor: Vec<&OverlayCmd> = cmds.iter().filter_map(|c| if let OverlayCmd::ShowroomFloor(p) = c { Some(p.iter().rev()) } else { None }).flatten().collect();
        let mut order: Vec<&OverlayCmd> = Vec::with_capacity(cmds.len() + floor.len());
        let has_car = cmds.iter().any(|c| matches!(c, OverlayCmd::ShowroomCar { .. }));
        let mut floor_placed = false;
        let mut floor_at = usize::MAX;
        for cmd in cmds.iter().rev() {
            match cmd {
                OverlayCmd::ShowroomFloor(p) if !has_car => order.extend(p.iter().rev()),
                OverlayCmd::ShowroomFloor(_) => {}
                OverlayCmd::ShowroomCar { .. } if !floor_placed => {
                    floor_placed = true;
                    floor_at = order.len();
                    order.extend(floor.iter().copied());
                    order.push(cmd);
                }
                _ => order.push(cmd),
            }
        }
        let mut floor_start = 0;
        for (i, cmd) in order.into_iter().enumerate() {
            if i == floor_at {
                floor_start = vertices.len();
            }
            if matches!(cmd, OverlayCmd::ShowroomCar { .. }) && !car_seen {
                car_seen = true;
                under = vertices.len();
                continue;
            }
            if let &OverlayCmd::Clip { x, y, w, h } = cmd {
                clip = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
                continue;
            }
            if let OverlayCmd::Bitmap { x, y, w: bw, h: bh, rgba } = cmd {
                // a picture put on the screen as it is
                let r = atlas.add(*bw as u32, *bh as u32, rgba);
                let (x0, y0, x1, y1) = (*x as f32, *y as f32, (*x as f32 + *bw as f32), (*y as f32 + *bh as f32));
                let v = |px: f32, py: f32, u: f32, t: f32| Vertex {
                    pos: [ox + px * scale_x, oy + py * scale_y, 0.0],
                    uv: [(r.x as f32 + u) / size, (r.y as f32 + t) / size],
                    color: [0.5, 0.5, 0.5, 1.0],
                };
                let (bw, bh) = (*bw as f32, *bh as f32);
                let q = [v(x0, y0, 0.0, 0.0), v(x1, y0, bw, 0.0), v(x1, y1, bw, bh), v(x0, y1, 0.0, bh)];
                vertices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
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
            floor_start = 0;
        }
        Self { atlas, vertices, subtract, under, floor: floor_start.min(under) }
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
        FeFrame { atlas, vertices: vec![q[0], q[1], q[2], q[0], q[2], q[3]], subtract: Vec::new(), under: 0, floor: 0 }
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
    /// The reflections come first: their vertices end at the returned split.
    /// `env_map`: where reflection map A (DrawC_PrimStart's, from VRAM) is in the atlas.
    pub fn geometry(&self, cmds: &[OverlayCmd], screen: (u16, u16), w: f32, h: f32, env_map: Option<(Rect, f32)>) -> Option<(Vec<Vertex>, u32, glam::Mat4)> {
        use crate::game::original::{pose, v3};
        use glam::{Mat4, Vec3, Vec4};
        let mut out = Vec::new();
        let mut proj = None;
        let mut split = 0;
        let reflections = cmds.iter().filter(|c| matches!(c, OverlayCmd::ShowroomCar { mirrored: true, .. }));
        let cars = cmds.iter().filter(|c| matches!(c, OverlayCmd::ShowroomCar { mirrored: false, .. }));
        for (k, cmd) in reflections.map(|c| (0, c)).chain(cars.map(|c| (1, c))) {
            if k == 1 && split == 0 {
                split = out.len() as u32;
            }
            let OverlayCmd::ShowroomCar { car, parts, cam_pos, cam_rot, h: dist, ofx, ofy, mirrored, level, env } = cmd else { continue };
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
                // the wheel arches' subtracted shadow: dark (no subtract pass here)
                let dark = if part.subtract { 0.0 } else { 1.0 };
                for v in &part.verts {
                    let p = tm.transform_point3(Vec3::from(v.pos));
                    let col = [v.color[0] * dark, v.color[1] * dark, v.color[2] * dark, if part.subtract { 1.0 } else { v.color[3] }];
                    out.push(Vertex { pos: p.into(), color: [col[0] * tint, col[1] * tint, col[2] * tint, col[3]], ..*v });
                }
            }
            // the reflection map over the reflection-mapped parts (DrawC_Prim's environment
            // path, as in races): map coordinates from the vertex normals turned by the
            // camera's yaw around the car, u = 64 + x, v = |z| into the map, added
            let (Some(env), Some((rect, size))) = (env, env_map) else { continue };
            if env.a.is_none() {
                continue;
            }
            let a = env.yaw as f32 / 4096.0 * std::f32::consts::TAU;
            let (sn, cs) = a.sin_cos();
            let rgb = [(env.colour & 0xFF) as f32 / 256.0, ((env.colour >> 8) & 0xFF) as f32 / 256.0, ((env.colour >> 16) & 0xFF) as f32 / 256.0];
            let towards = |p: Vec3| (v3(*cam_pos) - p).normalize_or_zero() * 0.01;
            for (part, &on) in self.model.parts.iter().zip(parts.iter()) {
                let (Some((normals, flags)), true) = (&part.env, on != 0) else { continue };
                let tm = if part.wheel.is_some() { model * Mat4::from_translation(part.hub) } else { model };
                for t in 0..flags.len() {
                    for k in 0..3 {
                        let n = normals[t * 3 + k];
                        let (nx, nz) = (n[0] as f32, n[2] as f32);
                        let x = (cs * nx + sn * nz) as i32;
                        let z = (-sn * nx + cs * nz) as i32;
                        let (u, v) = ((64 + x).clamp(0, rect.w as i32 - 1) as f32, z.unsigned_abs().min(rect.h - 1) as f32);
                        let p = tm.transform_point3(Vec3::from(part.verts[t * 3 + k].pos));
                        out.push(Vertex {
                            pos: (p + towards(p)).into(),
                            uv: [(rect.x as f32 + u + 0.5) / size, (rect.y as f32 + v + 0.5) / size],
                            color: [rgb[0], rgb[1], rgb[2], 0.0],
                        });
                    }
                }
            }
        }
        if split == 0 {
            split = out.len() as u32;
        }
        proj.map(|p| (out, split, p))
    }
}

/// A front-end frame ready for the renderer: its atlas, the 2D under and over the showroom
/// car (with the subtracting ranges of the part over it), and the car's geometry and
/// projection when there is one.
pub struct FeScene {
    pub atlas: Atlas,
    pub under: Vec<Vertex>,
    /// Vertex ranges of `under` drawn subtracting.
    pub under_subtract: Vec<std::ops::Range<u32>>,
    pub over: Vec<Vertex>,
    pub subtract: Vec<std::ops::Range<u32>>,
    pub car: Vec<Vertex>,
    /// The car's vertices from here on are the car itself, before them its reflection; the
    /// floor (`mid`, 2D) is drawn in between.
    pub car_split: u32,
    pub mid: Vec<Vertex>,
    pub mid_subtract: Vec<std::ops::Range<u32>>,
    pub view_proj: Option<glam::Mat4>,
}

impl FeScene {
    pub fn build(cmds: &[OverlayCmd], vram: &[u16], screen: (u16, u16), w: f32, h: f32, showroom: Option<&Showroom>) -> Self {
        let base = showroom.map(|s| s.atlas.clone()).unwrap_or_else(|| Atlas::new(1024));
        let f = FeFrame::build_in(base, cmds, vram, screen, w, h);
        let mut atlas = f.atlas;
        // the showroom car's reflection map, from VRAM (128 x 65 texels from its corner)
        let env_map = cmds.iter().find_map(|c| match c {
            OverlayCmd::ShowroomCar { env: Some(CarEnv { a: Some(m), .. }), .. } => Some(*m),
            _ => None,
        });
        let env_map = env_map.map(|m| {
            let (clut, tpage) = ((m[0] >> 16) as u16, (m[1] >> 16) as u16);
            let (u0, v0) = (m[0] as u8 as u32, (m[0] >> 8) as u8 as u32);
            let (tw, th) = (128u32, 65u32);
            let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
            for v in 0..th {
                for u in 0..tw {
                    rgba.extend(texel(vram, tpage, clut, (u0 + u) & 0xFF, (v0 + v) & 0xFF));
                }
            }
            (atlas.add(tw, th, &rgba), atlas.size as f32)
        });
        let (car, car_split, view_proj) = match showroom.and_then(|s| s.geometry(cmds, screen, w, h, env_map)) {
            Some((v, k, p)) => (v, k, Some(p)),
            None => (Vec::new(), 0, None),
        };
        let split = if view_proj.is_some() { f.under } else { 0 };
        let mut vertices = f.vertices;
        let over = vertices.split_off(split);
        let mid_at = f.floor.min(vertices.len());
        let mid = if view_proj.is_some() { vertices.split_off(mid_at) } else { Vec::new() };
        let mid_subtract = if view_proj.is_some() {
            f.subtract.iter().filter(|r| r.start as usize >= mid_at && (r.end as usize) <= split).map(|r| (r.start - mid_at as u32)..(r.end - mid_at as u32)).collect()
        } else {
            Vec::new()
        };
        let subtract = f
            .subtract
            .iter()
            .filter(|r| r.start as usize >= split)
            .map(|r| (r.start - split as u32)..(r.end - split as u32))
            .collect();
        let under_subtract = f.subtract.iter().filter(|r| (r.end as usize) <= vertices.len()).cloned().collect();
        Self { atlas, under: vertices, under_subtract, over, subtract, car, car_split, mid, mid_subtract, view_proj }
    }

    /// The showroom car's code and paint, when the front end shows one.
    pub fn car_code(cmds: &[OverlayCmd]) -> Option<(String, u8)> {
        cmds.iter().find_map(|c| match c {
            OverlayCmd::ShowroomCar { car, .. } => Some((car.code.clone(), car.colour)),
            _ => None,
        })
    }
}

