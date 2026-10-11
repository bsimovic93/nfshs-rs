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

pub struct TrackAssets {
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
pub fn load_track(data: &GameData, atlas: &mut Atlas, track_no: u32, variant: &str) -> Result<TrackAssets> {
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
    for b in &track.blocks {
        for mesh in [&b.ground, &b.detail] {
            for q in &mesh.quads {
                let te = track.textures.get(q.tex as usize);
                let rect = te.and_then(|t| rects.get(t.shape as usize)).copied().unwrap_or(atlas.white);
                let p: Vec<Vec3> = q
                    .v
                    .iter()
                    .map(|&i| {
                        let v = mesh.verts.get(i as usize).copied().unwrap_or([0; 4]);
                        ps1_to_world(b.origin + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0)
                    })
                    .collect();
                let (w, h) = (rect.w as f32, rect.h as f32);
                let corners = [Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)];
                let shade = |i: u16| {
                    let s = mesh.verts.get(i as usize).map(|v| v[3]).unwrap_or(0);
                    let k = 1.0 - (s as f32 / 32.0).clamp(0.0, 0.6);
                    [k * 0.5, k * 0.5, k * 0.5, 1.0]
                };
                let base = verts.len() as u32;
                for k in 0..4 {
                    verts.push(Vertex { pos: p[k].into(), uv: atlas.uv(rect, corners[k]), color: shade(q.v[k]) });
                }
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                for (a, b2, c) in [(0, 1, 2), (0, 2, 3)] {
                    let n = (p[b2] - p[a]).cross(p[c] - p[a]);
                    if n.length_squared() > 1e-8 {
                        tris.push(CollTri { p: [p[a], p[b2], p[c]], n: n.normalize() });
                    }
                }
            }
        }
    }
    let nodes = track.nodes.iter().map(|&n| ps1_to_world(n)).collect();
    log::info!("track {grp_name}: {} quads, {} textures", indices.len() / 6, psh.shapes.len());
    Ok(TrackAssets { verts, indices, tris, track, nodes })
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
