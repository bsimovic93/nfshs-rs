//! Converts decoded PS1 assets into engine-ready meshes and a single texture atlas.
//!
//! Coordinate conversion: the PS1 world is left-handed (X mirrored relative to our
//! right-handed Y-up space), so every X coordinate is negated on load.

use crate::formats::{geo::Geo, grp, psh::Psh, viv::Viv};
use anyhow::{Context, Result};
use glam::{Vec2, Vec3};
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

pub fn ps1_to_world(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, v.z)
}

/// Rectangle inside the atlas, in pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Shelf-packed RGBA atlas shared by the whole scene.
pub struct Atlas {
    pub size: u32,
    pub rgba: Vec<u8>,
    cursor: (u32, u32),
    shelf_h: u32,
    pub white: Rect,
}

impl Atlas {
    pub fn new(size: u32) -> Self {
        let mut a = Self { size, rgba: vec![0; (size * size * 4) as usize], cursor: (0, 0), shelf_h: 0, white: Rect::default() };
        a.white = a.add(4, 4, &[255; 64]);
        a
    }

    pub fn add(&mut self, w: u32, h: u32, rgba: &[u8]) -> Rect {
        let (pw, ph) = (w + 2, h + 2); // 1px gutter
        if self.cursor.0 + pw > self.size {
            self.cursor = (0, self.cursor.1 + self.shelf_h);
            self.shelf_h = 0;
        }
        if self.cursor.1 + ph > self.size {
            log::warn!("atlas full");
            return self.white;
        }
        let r = Rect { x: self.cursor.0 + 1, y: self.cursor.1 + 1, w, h };
        // copy with clamped gutter so nearest/linear sampling at edges never bleeds
        for yy in 0..ph {
            for xx in 0..pw {
                let sx = (xx as i32 - 1).clamp(0, w as i32 - 1) as u32;
                let sy = (yy as i32 - 1).clamp(0, h as i32 - 1) as u32;
                let si = ((sy * w + sx) * 4) as usize;
                let di = (((self.cursor.1 + yy) * self.size + self.cursor.0 + xx) * 4) as usize;
                self.rgba[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
            }
        }
        self.cursor.0 += pw;
        self.shelf_h = self.shelf_h.max(ph);
        r
    }

    /// Texel coordinate (in rect-local pixels) to normalized atlas UV.
    pub fn uv(&self, r: Rect, t: Vec2) -> [f32; 2] {
        let x = r.x as f32 + t.x.clamp(0.0, r.w as f32);
        let y = r.y as f32 + t.y.clamp(0.0, r.h as f32);
        [x / self.size as f32, y / self.size as f32]
    }
}

/// Triangle soup with world-space positions, used for physics queries.
#[derive(Clone, Copy, Debug)]
pub struct CollTri {
    pub p: [Vec3; 3],
    pub n: Vec3,
}

/// A camera-facing object (trees): local-space triangles rotated about Y each frame.
pub struct Sprite {
    pub pos: Vec3,
    pub verts: Vec<Vertex>,
}

pub struct TrackAssets {
    /// Camera-relative sky cylinder (horizon panorama).
    pub sky: Vec<Vertex>,
    /// Sky colour sampled from the cloud texture (zenith) and panorama top row (horizon).
    pub sky_colors: (Vec3, Vec3),
    pub sprites: Vec<Sprite>,
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub tris: Vec<CollTri>,
    pub track: grp::Track,
    pub nodes: Vec<Vec3>,
}

/// Locate game data files case-insensitively (CD file names are upper case).
pub struct GameData {
    pub root: PathBuf,
}

impl GameData {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let p = self.root.join(name.to_ascii_uppercase());
        std::fs::read(&p).with_context(|| format!("reading {}", p.display()))
    }
}

/// Variant suffix for the GRP/PSH files: "" day, "N" night, "S" snow, "W" wet.
/// `light`: the game's vertex colour table (`Chunk_lightTable`); without it vertices are grey.
pub fn load_track(data: &GameData, atlas: &mut Atlas, track_no: u32, variant: &str, light: &[u32], sky: Option<&crate::ps1::race::SkyData>) -> Result<TrackAssets> {
    let grp_name = format!("ZTR{track_no:02}{variant}.GRP");
    let psh_name = format!("ZTR{track_no:02}{variant}0.PSH");
    let track = grp::Track::parse(&data.read(&grp_name)?)?;
    let psh = Psh::parse(&data.read(&psh_name)?)?;
    let rects: Vec<Rect> = psh
        .shapes
        .iter()
        .map(|s| if s.width <= 1 && s.height <= 1 { atlas.white } else { atlas.add(s.width as u32, s.height as u32, &s.rgba) })
        .collect();

    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let mut tris = Vec::new();
    let quad_uvs = |tex: u16| -> (Rect, [Vec2; 4]) {
        let te = track.textures.get(tex as usize);
        let rect = te.and_then(|t| rects.get(t.shape as usize)).copied().unwrap_or(atlas.white);
        let (w, h) = (rect.w as f32, rect.h as f32);
        // UV slots as the PS1 pixmap stores them: top-left, top-right, bottom-left, bottom-right
        let mut pmx = [Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(0.0, h), Vec2::new(w, h)];
        // Trk_Material::uvFlag, applied in the order of Track_ProcessFlipAndUVFlags:
        // 0x10 flip U, 0x40 flip V, then one of 0x02 / 0x04 / 0x08 = rotate 90 / 180 / 270
        let fl = te.map(|t| t.flags).unwrap_or(0);
        let remap = |p: [Vec2; 4], o: [usize; 4]| o.map(|i| p[i]);
        if fl & 0x10 != 0 {
            pmx = remap(pmx, [1, 0, 3, 2]);
        }
        if fl & 0x40 != 0 {
            pmx = remap(pmx, [2, 3, 0, 1]);
        }
        if fl & 0x02 != 0 {
            pmx = remap(pmx, [2, 0, 3, 1]);
        } else if fl & 0x04 != 0 {
            pmx = remap(pmx, [3, 2, 1, 0]);
        } else if fl & 0x08 != 0 {
            pmx = remap(pmx, [1, 3, 0, 2]);
        }
        // DrawW_DrawQuad sends Trk_Quad points 1, 0, 2, 3 as the POLY_GT4's vertices 0..3,
        // which take the pixmap's UV slots in order: point 0 = top-right, 1 = top-left,
        // 2 = bottom-left, 3 = bottom-right
        (rect, [pmx[1], pmx[0], pmx[2], pmx[3]])
    };
    let shade = |s: i16| {
        // vertex colour from the light table: 0x80 per channel is neutral (texture x 1)
        if let Some(&c) = light.get(s as usize) {
            return [(c & 0xFF) as f32 / 256.0, ((c >> 8) & 0xFF) as f32 / 256.0, ((c >> 16) & 0xFF) as f32 / 256.0, 1.0];
        }
        let k = 1.0 - (s as f32 / 32.0).clamp(0.0, 0.6);
        [k * 0.5, k * 0.5, k * 0.5, 1.0]
    };
    // Emit one mesh, positioned by `place` (local vertex -> world), into verts/indices (+collision).
    let emit = |mesh: &grp::Mesh, place: &dyn Fn(Vec3) -> Vec3, collide: bool, verts: &mut Vec<Vertex>, indices: &mut Vec<u32>, tris: &mut Vec<CollTri>| {
        for q in &mesh.quads {
            let (rect, mut corners) = quad_uvs(q.tex);
            if q.strip {
                // NFSHS_STRIP_ORIENT=o (debug): rotate the strip quad's corners by o%4, mirror if o>=4
                if let Some(o) = std::env::var("NFSHS_STRIP_ORIENT").ok().and_then(|v| v.parse::<usize>().ok()) {
                    corners.rotate_left(o % 4);
                    if o >= 4 {
                        corners.swap(0, 1);
                        corners.swap(2, 3);
                    }
                }
            }
            let p: Vec<Vec3> = q
                .v
                .iter()
                .map(|&i| {
                    let v = mesh.verts.get(i as usize).copied().unwrap_or([0; 4]);
                    place(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0)
                })
                .collect();
            let base = verts.len() as u32;
            for k in 0..4 {
                let s = mesh.verts.get(q.v[k] as usize).map(|v| v[3]).unwrap_or(0);
                verts.push(Vertex { pos: p[k].into(), uv: atlas.uv(rect, corners[k]), color: shade(s) });
            }
            let front = [base, base + 1, base + 2, base, base + 2, base + 3];
            let back = [base, base + 2, base + 1, base, base + 3, base + 2];
            indices.extend_from_slice(&front);
            // Trk_Material flag 0x10: drawn without the back-face test (DrawW_DrawQuad)
            if track.textures.get(q.tex as usize).is_some_and(|t| t.mode & 0x10 != 0) {
                indices.extend_from_slice(&back);
            }
            if collide {
                for (a, b2, c) in [(0, 1, 2), (0, 2, 3)] {
                    let n = (p[b2] - p[a]).cross(p[c] - p[a]);
                    if n.length_squared() > 1e-8 {
                        tris.push(CollTri { p: [p[a], p[b2], p[c]], n: n.normalize() });
                    }
                }
            }
        }
    };
    if std::env::var_os("NFSHS_DUMP_SHADES").is_some() {
        let all = track.blocks.iter().flat_map(|b| b.ground.verts.iter().chain(&b.detail.verts)).chain(track.models.iter().flat_map(|m| m.verts.iter()));
        let (mn, mx) = all.fold((i16::MAX, i16::MIN), |(a, b), v| (a.min(v[3]), b.max(v[3])));
        println!("vertex shade index range {mn}..={mx}");
    }
    if std::env::var_os("NFSHS_DUMP_PROPS").is_some() {
        let mut hist = std::collections::BTreeMap::new();
        for b in &track.blocks {
            for q in b.ground.quads.iter().chain(&b.detail.quads) {
                let m = track.textures.get(q.tex as usize).map(|t| t.mode).unwrap_or(0);
                *hist.entry(m).or_insert(0) += 1;
            }
        }
        println!("track quads by material flag: {hist:02x?}");
    }
    let mut sprites = Vec::new();
    for b in &track.blocks {
        let origin = b.origin;
        for mesh in [&b.ground, &b.detail] {
            emit(mesh, &|v| ps1_to_world(origin + v), true, &mut verts, &mut indices, &mut tris);
        }
        for pl in &b.placements {
            let Some(model) = track.models.get(pl.model as usize) else { continue };
            let flat_quad = model.quads.len() == 1 && model.verts.iter().all(|v| v[2] == 0);
            if std::env::var("NFSHS_DUMP_MODEL").ok().and_then(|v| v.parse::<u16>().ok()) == Some(pl.model) {
                for q in &model.quads {
                    let te = track.textures.get(q.tex as usize);
                    let name = te.and_then(|t| psh.shapes.get(t.shape as usize)).map(|s| s.name.as_str()).unwrap_or("?");
                    let p: Vec<Vec3> = q.v.iter().map(|&i| model.verts.get(i as usize).map(|v| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0).unwrap_or(Vec3::ZERO)).collect();
                    let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                    println!("  quad tex {} {name} uv {:02x} mode {:02x} n(ps1) {n:.2} verts {p:.2?}", q.tex, te.map(|t| t.flags).unwrap_or(0), te.map(|t| t.mode).unwrap_or(0));
                }
            }
            if std::env::var_os("NFSHS_DUMP_PROPS").is_some() {
                let tex = model.quads.first().map(|q| q.tex).unwrap_or(0);
                let shape = track.textures.get(tex as usize).map(|t| t.shape as usize).unwrap_or(0);
                let name = psh.shapes.get(shape).map(|s| s.name.as_str()).unwrap_or("?");
                let te = track.textures.get(tex as usize);
                println!(
                    "prop model {:3} at {:?} flags {:04x} quads {:2} flat {} kind {} tex {} {} texflags {:02x} mode {:02x} raw {:02x?}",
                    pl.model,
                    ps1_to_world(pl.pos),
                    track.model_flags.get(pl.model as usize).copied().unwrap_or(0),
                    model.quads.len(),
                    flat_quad,
                    pl.kind,
                    tex,
                    name,
                    te.map(|t| t.flags).unwrap_or(0),
                    te.map(|t| t.mode).unwrap_or(0),
                    pl.raw
                );
            }
            match placement_transform(pl) {
                Placement::Billboard(scale) => {
                    // cut-out tree: rotated to face the camera every frame
                    let mut local = Vec::new();
                    let mut idx = Vec::new();
                    emit(model, &|v| Vec3::new(-v.x, v.y, v.z) * scale, false, &mut local, &mut idx, &mut Vec::new());
                    let tri_verts: Vec<Vertex> = idx.iter().map(|&i| local[i as usize]).collect();
                    sprites.push(Sprite { pos: ps1_to_world(pl.pos), verts: tri_verts });
                }
                Placement::Fixed(m) => {
                    let pos = pl.pos;
                    let place = move |v: Vec3| ps1_to_world(pos + m * v);
                    emit(model, &place, false, &mut verts, &mut indices, &mut tris);
                }
            }
        }
    }
    let nodes = track.nodes.iter().map(|&n| ps1_to_world(n)).collect();
    log::info!("{} sprites", sprites.len());
    let (sky, sky_colors) = match sky {
        Some(s) => build_original_sky(s, &psh, &rects, atlas),
        None => build_sky(&psh, &rects, atlas),
    };
    Ok(TrackAssets { verts, indices, tris, track, nodes, sprites, sky, sky_colors })
}

fn avg_color(rgba: &[u8], rows: std::ops::Range<usize>, w: usize) -> Vec3 {
    let mut acc = Vec3::ZERO;
    let mut n = 0.0;
    for y in rows {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if i + 3 < rgba.len() && rgba[i + 3] > 0 {
                acc += Vec3::new(rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32) / 255.0;
                n += 1.0;
            }
        }
    }
    if n > 0.0 { acc / n } else { Vec3::new(0.5, 0.6, 0.8) }
}

/// Horizon cylinder from the HZ01..HZ04 panorama strips (8 segments, ~22 degrees tall).
/// The original's sky: a gouraud-shaded dome (`Hrz_BuildSky`) and the textured horizon band
/// (`Hrz_BuildHorizon`), camera-centred and scaled to radius 100.
fn build_original_sky(s: &crate::ps1::race::SkyData, psh: &Psh, rects: &[Rect], atlas: &Atlas) -> (Vec<Vertex>, (Vec3, Vec3)) {
    let mut verts = Vec::new();
    // untextured: the PS1 draws the vertex colour itself (x2 cancels the shader's modulation)
    let flat = |c: [u8; 3]| [c[0] as f32 / 510.0, c[1] as f32 / 510.0, c[2] as f32 / 510.0, 1.0];
    let white = atlas.uv(atlas.white, Vec2::new(0.5, 0.5));
    let dome = |i: usize| {
        let v = s.dome[i];
        Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) * 0.1
    };
    for ring in 0..4 {
        for seg in 0..16 {
            let q = [ring * 17 + seg, ring * 17 + seg + 1, (ring + 1) * 17 + seg + 1, (ring + 1) * 17 + seg];
            for k in [0, 1, 2, 0, 2, 3] {
                verts.push(Vertex { pos: dome(q[k]).into(), uv: white, color: flat(s.dome_colors[q[k]]) });
            }
        }
    }
    // horizon band
    let names = if std::env::var_os("NFSHS_NO_HORIZON").is_some() { [""; 4] } else { ["HZ01", "HZ02", "HZ03", "HZ04"] };
    let shapes: Vec<Option<usize>> = names.iter().map(|n| psh.shapes.iter().position(|sh| sh.name == *n)).collect();
    let k = 100.0 / 32000.0;
    let (y0, y1) = (s.bottom as f32 * k, (s.bottom as f32 + s.height as f32) * k);
    let modulate = |c: [u8; 3]| [c[0] as f32 / 256.0, c[1] as f32 / 256.0, c[2] as f32 / 256.0, 1.0];
    for seg in 0..16 {
        let t = s.segment_tex[seg] as usize;
        let Some(Some(shape)) = shapes.get(t) else { continue };
        let rect = rects[*shape];
        let (w, h) = (rect.w as f32, rect.h as f32);
        let p = |i: usize, y: f32| [-(s.ring[i][0] as f32) * k, y, s.ring[i][1] as f32 * k];
        let quad = [
            (p(seg, y1), Vec2::new(0.0, 0.0), modulate(s.top_colors[seg])),
            (p(seg + 1, y1), Vec2::new(w, 0.0), modulate(s.top_colors[seg + 1])),
            (p(seg + 1, y0), Vec2::new(w, h), modulate(s.bottom_colors[seg + 1])),
            (p(seg, y0), Vec2::new(0.0, h), modulate(s.bottom_colors[seg])),
        ];
        for i in [0, 1, 2, 0, 2, 3] {
            verts.push(Vertex { pos: quad[i].0, uv: atlas.uv(rect, quad[i].1), color: quad[i].2 });
        }
    }
    let c = |c: [u8; 3]| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
    (verts, (c(s.dome_colors[4 * 17]), c(s.dome_colors[0])))
}

fn build_sky(psh: &Psh, rects: &[Rect], atlas: &Atlas) -> (Vec<Vertex>, (Vec3, Vec3)) {
    let panels: Vec<usize> = ["HZ01", "HZ02", "HZ03", "HZ04"]
        .iter()
        .filter_map(|n| psh.shapes.iter().position(|s| s.name == *n))
        .collect();
    let mut verts = Vec::new();
    let mut colors = (Vec3::new(0.3, 0.45, 0.8), Vec3::new(0.7, 0.75, 0.85));
    if let Some(&p0) = panels.first() {
        let s = &psh.shapes[p0];
        colors.1 = avg_color(&s.rgba, 0..2, s.width as usize);
    }
    // zenith: a deeper, bluer version of the panorama's sky colour
    colors.0 = (colors.1 * Vec3::new(0.55, 0.7, 0.95)).min(Vec3::ONE);
    if panels.is_empty() {
        return (verts, colors);
    }
    let segs = 8;
    let r = 100.0;
    let (y0, y1) = (-r * 0.04, r * 0.24);
    for k in 0..segs {
        let rect = rects[panels[k % panels.len()]];
        let a0 = k as f32 / segs as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / segs as f32 * std::f32::consts::TAU;
        let p = |a: f32, y: f32| [a.sin() * r, y, a.cos() * r];
        let (w, h) = (rect.w as f32, rect.h as f32);
        let quad = [
            (p(a0, y1), Vec2::new(0.0, 0.0)),
            (p(a1, y1), Vec2::new(w, 0.0)),
            (p(a1, y0), Vec2::new(w, h)),
            (p(a0, y0), Vec2::new(0.0, h)),
        ];
        for i in [0, 1, 2, 0, 2, 3] {
            verts.push(Vertex { pos: quad[i].0, uv: atlas.uv(rect, quad[i].1), color: [0.5, 0.5, 0.5, 1.0] });
        }
    }
    (verts, colors)
}

/// A car model in local space (metres, origin at ground contact centre, +Z forward).
pub struct CarModel {
    pub body: Vec<Vertex>,
    /// Wheels: local vertex lists around the hub, and hub position. Order FL, FR, RL, RR.
    pub wheels: Vec<(Vec<Vertex>, Vec3)>,
    pub wheel_radius: f32,
    pub half_extents: Vec3,
}

/// Body parts drawn for the high-detail model (from inspection of the 57-part layout).
const BODY_PARTS: &[usize] = &[0, 43, 44];

enum Placement {
    /// Faces the camera (yaw only); scale (width, height, width).
    Billboard(Vec3),
    /// Fixed orientation and scale, in PS1 space (applied as M * v).
    Fixed(glam::Mat3),
}

/// How DrawW_BuildChunkObjectFacets orients a placed object:
/// type 1 as is; type 5 by the quaternion at +20 (Quatern_QuatToMat) with column scales at
/// +28/+30/+32; type 9 facing the camera if byte +5 bit 0 is set, otherwise by the Y angle
/// at +20 (1024 = 360 degrees), scaled by +24 (x, z) and +22 (y). Scales are 8.8.
fn placement_transform(pl: &grp::Placement) -> Placement {
    let r = &pl.raw;
    let s16 = |o: usize| i16::from_le_bytes([r[o], r[o + 1]]) as f32;
    let scale = |v: f32| if v == 0.0 { 1.0 } else { v / 256.0 };
    match pl.kind {
        5 => {
            let (x, y, z, w) = (s16(20) / 16384.0, s16(22) / 16384.0, s16(24) / 16384.0, s16(26) / 16384.0);
            // the game's matrix is the transpose of the textbook quaternion rotation
            let m = glam::Mat3::from_cols(
                Vec3::new(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)),
                Vec3::new(2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)),
                Vec3::new(2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)),
            );
            let sc = Vec3::new(scale(s16(28)), scale(s16(30)), scale(s16(32)));
            Placement::Fixed(glam::Mat3::from_cols(m.x_axis * sc.x, m.y_axis * sc.y, m.z_axis * sc.z))
        }
        9 => {
            let (w, h) = (scale(s16(24)), scale(s16(22)));
            if r[5] & 1 != 0 {
                Placement::Billboard(Vec3::new(w, h, w))
            } else {
                let a = s16(20) / 1024.0 * std::f32::consts::TAU;
                let (sn, c) = a.sin_cos();
                // xformy: rows (c, 0, -s), (0, 1, 0), (s, 0, c)
                let m = glam::Mat3::from_cols(Vec3::new(c * w, 0.0, sn * w), Vec3::new(0.0, h, 0.0), Vec3::new(-sn * w, 0.0, c * w));
                Placement::Fixed(m)
            }
        }
        _ => Placement::Fixed(glam::Mat3::IDENTITY),
    }
}

pub fn load_car(data: &GameData, atlas: &mut Atlas, viv_name: &str) -> Result<CarModel> {
    let viv = Viv::parse(&data.read(viv_name)?)?;
    let (_, geo) = viv.find_suffix(".geo").context("car viv has no .geo")?;
    let geo = Geo::parse(geo)?;
    let mut page = crate::formats::psh::Atlas::new(256);
    for (name, d) in &viv.entries {
        let n = name.to_ascii_lowercase();
        if n.ends_with(".psh") && !n.ends_with("d.psh") && !n.ends_with("g.psh") && !n.ends_with("i.psh") {
            page.place(&Psh::parse(d)?.shapes, 64);
        }
    }
    let rect = atlas.add(256, 256, &page.rgba);
    let part_tris = |pi: usize, center: Vec3| -> Vec<Vertex> {
        let part = &geo.parts[pi];
        let off = Vec3::new(part.position[0] as f32, part.position[1] as f32, part.position[2] as f32) / 65536.0;
        let mut out = Vec::new();
        for poly in &part.polys {
            for k in 0..3 {
                let v = part.verts.get(poly.v[k] as usize).copied().unwrap_or([0; 3]);
                let p = ps1_to_world(off + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 256.0) - center;
                let t = Vec2::new(poly.uv[k][0] as f32 + 0.5, poly.uv[k][1] as f32 + 0.5);
                out.push(Vertex { pos: p.into(), uv: atlas.uv(rect, t), color: [0.5, 0.5, 0.5, 1.0] });
            }
        }
        out
    };
    let mut body = Vec::new();
    for &pi in BODY_PARTS {
        body.extend(part_tris(pi, Vec3::ZERO));
    }
    let mut wheels = Vec::new();
    // front wheels are split into hub (47/48) + tyre (51/52); rear wheels are whole (55/56)
    for group in [[47usize, 51], [48, 52], [55, 55], [56, 56]] {
        let p = &geo.parts[group[0]];
        let hub = ps1_to_world(Vec3::new(p.position[0] as f32, p.position[1] as f32, p.position[2] as f32) / 65536.0);
        let mut v = part_tris(group[0], hub);
        if group[1] != group[0] {
            v.extend(part_tris(group[1], hub));
        }
        wheels.push((v, hub));
    }
    let wheel_radius = wheels.first().map(|w| w.1.y).unwrap_or(0.33).max(0.2);
    let mut max = Vec3::ZERO;
    for v in &body {
        max = max.max(Vec3::from(v.pos).abs());
    }
    Ok(CarModel { body, wheels, wheel_radius, half_extents: max })
}
