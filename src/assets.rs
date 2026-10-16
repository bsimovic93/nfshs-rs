//! Converts decoded PS1 assets into engine-ready meshes and a single texture atlas.
//!
//! Coordinate conversion: the PS1 world is left-handed (X mirrored relative to our
//! right-handed Y-up space), so every X coordinate is negated on load.

use crate::formats::{geo::Geo, grp, psh::Psh, viv::Viv};
use anyhow::{Context, Result};
use glam::{Quat, Vec2, Vec3};
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
#[derive(Clone)]
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
        // (a placeholder atlas too small for it keeps the default rectangle)
        if size >= 8 {
            a.white = a.add(4, 4, &[255; 64]);
        }
        a
    }

    pub fn add(&mut self, w: u32, h: u32, rgba: &[u8]) -> Rect {
        let (pw, ph) = (w + 2, h + 2); // 1px gutter
        if self.cursor.0 + pw > self.size {
            self.cursor = (0, self.cursor.1 + self.shelf_h);
            self.shelf_h = 0;
        }
        if self.cursor.1 + ph > self.size {
            log::warn!("atlas full: no room for {w}x{h} (size {}, shelf at y {})", self.size, self.cursor.1);
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

/// A camera-facing object (trees): local-space triangles rotated about Y each frame.
pub struct Sprite {
    pub pos: Vec3,
    pub verts: Vec<Vertex>,
    /// The track block (chunk) it belongs to.
    pub block: usize,
}

/// Keyframed scenery (the windmills' sails): the model's triangles in its own space
/// (world-handed), placed each frame from its keyframes.
pub struct AnimatedObject {
    pub verts: Vec<Vertex>,
    pub ticks: f32,
    /// Position (world) and rotation (world-handed) per keyframe.
    pub frames: Vec<(Vec3, Quat)>,
}

/// A track texture Track_AnimateTextures scrolls (material flag 0x80): the material's own atlas
/// rectangle (with its 1-texel gutter) and the shape's texels, which are written into it each
/// frame turned up by the original's V offset, as its texture window wraps them.
pub struct ScrollTexture {
    pub material: u16,
    pub rect: Rect,
    pub rgba: Vec<u8>,
}

impl ScrollTexture {
    /// The rectangle's texels (gutter included) with the shape scrolled by `dv` rows.
    pub fn scrolled(&self, dv: u8) -> crate::render::AtlasWrite {
        let (w, h) = (self.rect.w, self.rect.h);
        let mut rgba = Vec::with_capacity(((w + 2) * (h + 2) * 4) as usize);
        for y in 0..h + 2 {
            let sy = (y.clamp(1, h) - 1 + dv as u32) % h;
            for x in 0..w + 2 {
                let o = ((sy * w + x.clamp(1, w) - 1) * 4) as usize;
                rgba.extend_from_slice(&self.rgba[o..o + 4]);
            }
        }
        crate::render::AtlasWrite { x: self.rect.x - 1, y: self.rect.y - 1, w: w + 2, h: h + 2, rgba }
    }
}

pub struct TrackAssets {
    pub animated: Vec<AnimatedObject>,
    pub scrolling: Vec<ScrollTexture>,
    /// Camera-relative sky cylinder (horizon panorama).
    pub sky: Vec<Vertex>,
    /// How many of `sky`'s vertices are the dome (the horizon band follows).
    pub sky_dome: usize,
    /// Sky colour sampled from the cloud texture (zenith) and panorama top row (horizon).
    pub sky_colors: (Vec3, Vec3),
    pub sprites: Vec<Sprite>,
    /// Per block (track chunk): index ranges of its ground (full detail, low detail), the
    /// detail surfaces drawn at both, its objects and its road lines.
    pub block_ranges: Vec<[std::ops::Range<u32>; 5]>,
    /// The track-wide scenery (0x21/0x24: islands, headlands), drawn whatever the chunks.
    pub global_range: std::ops::Range<u32>,
    /// Each track model's triangles in its own PS1 space (metres, not mirrored), its
    /// two-sided polygons both ways round: the objects the original draws are placed from
    /// these each frame.
    pub models: Vec<Vec<Vertex>>,
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
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
        // (a 1x1 shape is one texel all over its polygons: "TRAN" is colour 0, which the PS1
        // does not draw, so those polygons are invisible)
        .map(|s| if s.rgba.len() < 4 { atlas.white } else { atlas.add(s.width.max(1) as u32, s.height.max(1) as u32, &s.rgba) })
        .collect();

    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let line_rects: Vec<Option<Rect>> = match data.read("ZSFX.PSH").and_then(|d| Psh::parse(&d)) {
        Ok(sfx) => (0..10)
            .map(|i| sfx.shapes.iter().find(|s| s.name == format!("LIN{i}")).map(|s| atlas.add(s.width as u32, s.height as u32, &s.rgba)))
            .collect(),
        Err(e) => {
            log::warn!("no road line textures: {e:#}");
            Vec::new()
        }
    };
    // the scrolling materials (Track_AnimateTextures' type 0x80 controllers are made for flag
    // 0x80): each its own copy of its shape, rewritten per frame by its scroll
    let scrolling: Vec<ScrollTexture> = track
        .textures
        .iter()
        .enumerate()
        .filter(|(_, t)| t.mode & 0x80 != 0)
        .filter_map(|(i, t)| {
            let sh = psh.shapes.get(t.shape as usize).filter(|s| s.width > 1 && s.height > 1)?;
            Some(ScrollTexture { material: i as u16, rect: atlas.add(sh.width as u32, sh.height as u32, &sh.rgba), rgba: sh.rgba.clone() })
        })
        .collect();
    let quad_uvs = |tex: u16| -> (Rect, [Vec2; 4]) {
        let te = track.textures.get(tex as usize);
        let own = scrolling.iter().find(|s| s.material == tex).map(|s| s.rect);
        let rect = own.or_else(|| te.and_then(|t| rects.get(t.shape as usize)).copied()).unwrap_or(atlas.white);
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
    // the vertex's light-table index rides on its alpha (+ 2 (s + 1)) for the shader's night
    // headlight lookup
    let tag = |s: i16| if !light.is_empty() && (0..256).contains(&s) { 2.0 * (s as f32 + 1.0) } else { 0.0 };
    let shade = |s: i16| {
        // vertex colour from the light table: 0x80 per channel is neutral (texture x 1)
        if let Some(&c) = light.get(s as usize) {
            return [(c & 0xFF) as f32 / 256.0, ((c >> 8) & 0xFF) as f32 / 256.0, ((c >> 16) & 0xFF) as f32 / 256.0, 1.0];
        }
        let k = 1.0 - (s as f32 / 32.0).clamp(0.0, 0.6);
        [k * 0.5, k * 0.5, k * 0.5, 1.0]
    };
    // Emit one mesh, positioned by `place` (local vertex -> world), into verts/indices.
    let emit = |mesh: &grp::Mesh, place: &dyn Fn(Vec3) -> Vec3, verts: &mut Vec<Vertex>, indices: &mut Vec<u32>| {
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
            // semi-transparent textures blend their STP texels by their shape's translucency
            // mode: added (smoke, clouds) or halved. The PS1 makes a polygon semi-transparent
            // when its texture's CLUT has an STP colour (Texture_LoadPmx: Draw_tPixMap +14,
            // which DrawW_DrawQuad ORs into the GPU code), whatever the material says: Snowy
            // Ridge's #CLD mist has no material flag; a material's bit 0 stands for it when
            // the shape has no CLUT
            let te = track.textures.get(q.tex as usize);
            let semi = |t: &&grp::TexEntry| psh.shapes.get(t.shape as usize).is_some_and(|s| if s.palette.is_empty() { t.flags & 1 != 0 } else { s.palette.iter().any(|&c| c > 0x8000) });
            let alpha = match te.filter(semi).and_then(|t| psh.shapes.get(t.shape as usize)).map(|s| s.translucency) {
                Some(1) => 0.0,
                Some(0) => 0.5,
                _ => 1.0,
            };
            for k in 0..4 {
                let s = mesh.verts.get(q.v[k] as usize).map(|v| v[3]).unwrap_or(0);
                let mut color = shade(s);
                color[3] = alpha + tag(s);
                verts.push(Vertex { pos: p[k].into(), uv: atlas.uv(rect, corners[k]), color });
            }
            let front = [base, base + 1, base + 2, base, base + 2, base + 3];
            let back = [base, base + 2, base + 1, base, base + 3, base + 2];
            indices.extend_from_slice(&front);
            // Trk_Material flag 0x10: drawn without the back-face test (DrawW_DrawQuad)
            if track.textures.get(q.tex as usize).is_some_and(|t| t.mode & 0x10 != 0) {
                indices.extend_from_slice(&back);
            }
        }
    };
    if std::env::var_os("NFSHS_DUMP_SHADES").is_some() {
        let all = track.blocks.iter().flat_map(|b| b.ground.verts.iter().chain(&b.detail.verts)).chain(track.models.iter().flat_map(|m| m.verts.iter()));
        let (mn, mx) = all.fold((i16::MAX, i16::MIN), |(a, b), v| (a.min(v[3]), b.max(v[3])));
        println!("vertex shade index range {mn}..={mx}");
    }
    // NFSHS_DUMP_MATERIALS=1 (debug): the animated materials (Trk_Material flag 0x80 or a
    // texture count) and their shapes
    if std::env::var_os("NFSHS_DUMP_MATERIALS").is_some() {
        // (where the first ground or detail quad with material i is, in world space)
        let first_at = |i: usize| {
            track.blocks.iter().find_map(|b| {
                let m = [&b.ground, &b.detail].into_iter().find(|m| m.quads.iter().any(|q| q.tex as usize == i))?;
                let q = m.quads.iter().find(|q| q.tex as usize == i)?;
                let v = m.verts.get(q.v[0] as usize)?;
                Some(ps1_to_world(b.origin + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0))
            })
        };
        for (i, t) in track.textures.iter().enumerate() {
            if let Some(sh) = psh.shapes.get(t.shape as usize).filter(|s| s.width <= 1 && s.height <= 1) {
                println!("material {i}: 1x1 shape {} {:?} rgba {:?} flag {:#x} uv {:#x}, at {:?}", t.shape, sh.name, sh.rgba, t.mode, t.flags, first_at(i));
            }
        }
        for (i, t) in track.textures.iter().enumerate().filter(|(_, t)| t.mode & 0x80 != 0 || t.count > 1) {
            let sh = psh.shapes.get(t.shape as usize);
            let at = first_at(i);
            println!("material {i}: shape {} {:?} {:?}, flag {:#x} uv {:#x} count {} interval {}, at {at:?}", t.shape, sh.map(|s| &s.name), sh.map(|s| (s.width, s.height, s.vram)), t.mode, t.flags, t.count, t.interval);
        }
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
    // painted road lines (DrawW_BuildChunkCenterLineFacets / DrawW_OnyxLinePrim): each record
    // is a point on the line, widened left/right by the road slice's right vector / 8, and
    // consecutive points form a quad textured with LIN<type> from ZSFX.PSH (0xFF = gap)
    // per block: index ranges of its ground (trough), objects and road lines, so the chunks
    // can be drawn as the original's build list says
    let mut block_ranges = vec![[0u32..0, 0..0, 0..0, 0..0, 0..0]; track.blocks.len()];
    for (bi, b) in track.blocks.iter().enumerate() {
        let lines_start = indices.len() as u32;
        let point = |r: &[u8; 4]| -> Option<(Vec3, Vec3, i16)> {
            let v = b.ground.verts.get(r[0] as usize)?;
            let p = b.origin + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0;
            // the road slice the point lies on
            let slice = track.vroad.iter().min_by(|a, c| a.pos.distance_squared(p).total_cmp(&c.pos.distance_squared(p)))?;
            let right = Vec3::new((slice.raw[6] as i8 >> 3) as f32, (slice.raw[7] as i8 >> 3) as f32, (slice.raw[8] as i8 >> 3) as f32) / 64.0;
            Some((p, right, v[3]))
        };
        for w in b.lines.windows(2) {
            let (r0, r1) = (&w[0], &w[1]);
            if r0[2] == 0xFF {
                continue;
            }
            let Some(Some(rect)) = line_rects.get(r0[2] as usize).copied() else { continue };
            let (Some((p0, d0, s0)), Some((p1, d1, s1))) = (point(r0), point(r1)) else { continue };
            let lift = Vec3::Y * 0.02;
            let (tw, th) = (rect.w as f32, rect.h as f32);
            let corners = [(p0 - d0, s0, Vec2::new(0.0, 0.0)), (p0 + d0, s0, Vec2::new(0.0, th)), (p1 + d1, s1, Vec2::new(tw, th)), (p1 - d1, s1, Vec2::new(tw, 0.0))];
            let base = verts.len() as u32;
            for (p, s, uv) in corners {
                let mut color = shade(s);
                color[3] += tag(s);
                verts.push(Vertex { pos: (ps1_to_world(p) + lift).into(), uv: atlas.uv(rect, uv), color });
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3, base, base + 2, base + 1, base, base + 3, base + 2]);
        }
        block_ranges[bi][4] = lines_start..indices.len() as u32;
    }
    let mut sprites = Vec::new();
    for (bi, b) in track.blocks.iter().enumerate() {
        let origin = b.origin;
        let place = |v| ps1_to_world(origin + v);
        let start = indices.len() as u32;
        emit(&b.ground, &place, &mut verts, &mut indices);
        block_ranges[bi][0] = start..indices.len() as u32;
        let start = indices.len() as u32;
        let far = grp::Mesh { verts: b.ground.verts.clone(), quads: b.ground.far.clone(), far: Vec::new() };
        emit(&far, &place, &mut verts, &mut indices);
        block_ranges[bi][1] = start..indices.len() as u32;
        let start = indices.len() as u32;
        emit(&b.detail, &place, &mut verts, &mut indices);
        block_ranges[bi][2] = start..indices.len() as u32;
        let objects_start = indices.len() as u32;
        for pl in &b.placements {
            let Some(model) = track.models.get(pl.model as usize) else { continue };
            let flat_quad = model.quads.len() == 1 && model.verts.iter().all(|v| v[2] == 0);
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
                    emit(model, &|v| Vec3::new(-v.x, v.y, v.z) * scale, &mut local, &mut idx);
                    let tri_verts: Vec<Vertex> = idx.iter().map(|&i| local[i as usize]).collect();
                    sprites.push(Sprite { pos: ps1_to_world(pl.pos), verts: tri_verts, block: bi });
                }
                Placement::Fixed(m) => {
                    let pos = pl.pos;
                    let place = move |v: Vec3| ps1_to_world(pos + m * v);
                    emit(model, &place, &mut verts, &mut indices);
                }
            }
        }
        block_ranges[bi][3] = objects_start..indices.len() as u32;
    }
    // the track-wide scenery, placed in absolute coordinates
    let global_start = indices.len() as u32;
    for pl in &track.globals {
        let Some(model) = track.models.get(pl.model as usize) else { continue };
        if let Placement::Fixed(m) = placement_transform(pl) {
            let pos = pl.pos;
            let place = move |v: Vec3| ps1_to_world(pos + m * v);
            emit(model, &place, &mut verts, &mut indices);
        }
    }
    let global_range = global_start..indices.len() as u32;
    // NFSHS_DUMP_MODEL=N (debug): track model N's quads (placed or drawn by the original)
    if let Some(n) = std::env::var("NFSHS_DUMP_MODEL").ok().and_then(|v| v.parse::<usize>().ok()) {
        if let Some(model) = track.models.get(n) {
            for q in &model.quads {
                let te = track.textures.get(q.tex as usize);
                let shape = te.and_then(|t| psh.shapes.get(t.shape as usize));
                let name = shape.map(|s| s.name.as_str()).unwrap_or("?");
                let p: Vec<Vec3> = q.v.iter().map(|&i| model.verts.get(i as usize).map(|v| Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 64.0).unwrap_or(Vec3::ZERO)).collect();
                let shades: Vec<i16> = q.v.iter().map(|&i| model.verts.get(i as usize).map(|v| v[3]).unwrap_or(0)).collect();
                let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                println!(
                    "  quad tex {} {name} {:?} translucency {:?} stp {:?} uv {:02x} mode {:02x} shades {shades:?} n(ps1) {n:.2} verts {p:.2?}",
                    q.tex,
                    shape.map(|s| (s.width, s.height)),
                    shape.map(|s| s.translucency),
                    shape.map(|s| s.palette.iter().any(|&c| c > 0x8000)),
                    te.map(|t| t.flags).unwrap_or(0),
                    te.map(|t| t.mode).unwrap_or(0)
                );
            }
        }
    }
    let models = track
        .models
        .iter()
        .map(|model| {
            let (mut local, mut idx) = (Vec::new(), Vec::new());
            emit(model, &|v| v, &mut local, &mut idx);
            idx.iter().map(|&i| local[i as usize]).collect()
        })
        .collect();
    let animated: Vec<AnimatedObject> = track
        .keyframed
        .iter()
        .filter_map(|k| {
            let model = track.models.get(k.model as usize)?;
            let mut local = Vec::new();
            let mut idx = Vec::new();
            emit(model, &|v| Vec3::new(-v.x, v.y, v.z), &mut local, &mut idx);
            // the PS1's rotation seen in the mirrored world: axis (x, -y, -z)
            let frames = k.frames.iter().map(|&(p, q)| (ps1_to_world(p), Quat::from_xyzw(q[0], -q[1], -q[2], q[3]).normalize())).collect();
            Some(AnimatedObject { verts: idx.iter().map(|&i| local[i as usize]).collect(), ticks: k.ticks as f32, frames })
        })
        .collect();
    log::info!("{} keyframed scenery objects", animated.len());
    log::info!("{} track-wide scenery objects", track.globals.len());
    let nodes = track.nodes.iter().map(|&n| ps1_to_world(n)).collect();
    log::info!("{} sprites", sprites.len());
    let (sky, sky_dome, sky_colors) = match sky {
        Some(s) => build_original_sky(s, &psh, &rects, atlas),
        None => {
            let (v, c) = build_sky(&psh, &rects, atlas);
            (v, 0, c)
        }
    };
    Ok(TrackAssets { animated, scrolling, verts, indices, track, nodes, sprites, sky, sky_dome, sky_colors, block_ranges, global_range, models })
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
fn build_original_sky(s: &crate::ps1::race::SkyData, psh: &Psh, rects: &[Rect], atlas: &Atlas) -> (Vec<Vertex>, usize, (Vec3, Vec3)) {
    let mut verts = Vec::new();
    log::debug!("sky dome rings {:?}", (0..5).map(|r| s.dome_colors[r * 17 + 8]).collect::<Vec<_>>());
    // untextured: the PS1 draws the vertex colour itself (x2 cancels the shader's modulation)
    let flat = |c: [u8; 3]| [c[0] as f32 / 510.0, c[1] as f32 / 510.0, c[2] as f32 / 510.0, 1.0];
    let white = atlas.uv(atlas.white, Vec2::new(0.5, 0.5));
    let dome = |i: usize| {
        let v = s.dome[i];
        Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) * 0.1
    };
    let modulate = |c: [u8; 3]| [c[0] as f32 / 256.0, c[1] as f32 / 256.0, c[2] as f32 / 256.0, 1.0];
    for ring in 0..4 {
        for seg in 0..16 {
            let q = [ring * 17 + seg, ring * 17 + seg + 1, (ring + 1) * 17 + seg + 1, (ring + 1) * 17 + seg];
            // a textured sky (Hrz_BuildSky's GT4s): the quad's texture's top edge on the upper
            // ring, its colours modulating it
            let tex = s.dome_tex.as_ref().and_then(|t| {
                let k = t[ring * 16 + seg] as usize;
                let name = format!("HZ{:02}", if k < 12 { k + 1 } else { k - 3 });
                psh.shapes.iter().position(|sh| sh.name == name).map(|i| (rects[i], k >= 12))
            });
            for k in [0, 1, 2, 0, 2, 3] {
                let (uv, color) = match tex {
                    Some((rect, mirrored)) => {
                        let (w, h) = (rect.w as f32, rect.h as f32);
                        let (left, right) = if mirrored { (w, 0.0) } else { (0.0, w) };
                        let corner = [Vec2::new(left, h), Vec2::new(right, h), Vec2::new(right, 0.0), Vec2::new(left, 0.0)][k];
                        (atlas.uv(rect, corner), modulate(s.dome_colors[q[k]]))
                    }
                    None => (white, flat(s.dome_colors[q[k]])),
                };
                verts.push(Vertex { pos: dome(q[k]).into(), uv, color });
            }
        }
    }
    let dome = verts.len();
    // horizon band: each segment textured with HZ01..HZ08 (`index & 7`) and as tall as its
    // texture against the tallest one (Hrz_Init2DRing's ratios, which
    // Horizon_InterpolateLineSCoords applies between the band's bottom and top), so the 1x1
    // placeholders some tracks use draw nothing
    let names: Vec<String> = (1..=8).map(|i| format!("HZ{i:02}")).collect();
    let shapes: Vec<Option<usize>> = names.iter().map(|n| if !s.band || std::env::var_os("NFSHS_NO_HORIZON").is_some() { None } else { psh.shapes.iter().position(|sh| sh.name == *n) }).collect();
    let extent = |t: usize| shapes[t & 7].map_or(0, |i| psh.shapes[i].height.saturating_sub(1) as i32);
    let tallest = (0..16).map(|seg| extent(s.segment_tex[seg] as usize)).fold(1, i32::max);
    let k = 100.0 / 32000.0;
    for seg in 0..16 {
        let t = s.segment_tex[seg] as usize;
        let Some(shape) = shapes[t & 7] else { continue };
        let ratio = extent(t) as f32 / tallest as f32;
        if ratio <= 0.0 {
            continue;
        }
        // from `bottom` (the texture's bottom edge) up as far as the segment's ratio of
        // `height` (HrzSetPsxMatrix negates y: world heights, up positive)
        let (y0, y1) = (s.bottom as f32 * k, (s.bottom as f32 + s.height as f32 * ratio) * k);
        let rect = rects[shape];
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
    (verts, dome, (c(s.dome_colors[4 * 17]), c(s.dome_colors[0])))
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

/// One part of a car model (the GEO's 57 parts), in the car's local space.
#[derive(Default)]
pub struct CarPart {
    pub verts: Vec<Vertex>,
    /// Reflection-mapped parts (R3DCar_ObjectInfo flags 9: DrawC_Prim's environment path):
    /// per vertex the model-space normal / 64 (PS1 axes), per triangle the polygon flags.
    pub env: Option<(Vec<[i16; 3]>, Vec<u16>)>,
    /// Reflection-mapped parts: per triangle corner its GEO vertex (for normals from elsewhere),
    /// and how many vertices the GEO part has.
    pub env_vertices: Vec<u16>,
    pub geo_vertices: usize,
    /// Per triangle (as `verts`): its GEO texture, flags and UVs, for the texture
    /// substitutes ([`CarModel::substitute`]).
    pub tris: Vec<CarTri>,
    /// Wheel parts turn around their hub: which wheel (FL, FR, RL, RR) and where.
    pub wheel: Option<usize>,
    pub hub: Vec3,
    /// The part darkens what is behind it (the wheel arches' "shdw" texture, subtracted).
    pub subtract: bool,
}

/// A GEO polygon as the texture substitutes need it.
#[derive(Clone, Copy)]
pub struct CarTri {
    pub texture: u8,
    /// flags | flags2 << 8: the zone bits (0x3F0) and, as i16 below 0, the mirrored twin.
    pub flags: u16,
    pub uv: [[u8; 2]; 3],
}

/// A car model in local space (metres, origin at ground contact centre, +Z forward).
pub struct CarModel {
    /// Indexed like the GEO (and `R3DCar_ObjectVisible`).
    pub parts: Vec<CarPart>,
    pub half_extents: Vec3,
    /// Which of the model's two texture pages each GEO texture is on (`CAR_TEXTURES`
    /// order; none when the car has no such texture), the pages in the atlas and its size.
    pub texture_page: Vec<Option<u8>>,
    pub page_rects: [Rect; 2],
    pub atlas_size: f32,
    /// The original's texture substitutes for this model and its parts' drawing modes (from
    /// the running game, see `read_texture_subs`): empty until set.
    pub texture_subs: Vec<crate::ps1::race::TextureSubs>,
    pub part_modes: Vec<u8>,
}

/// `R3DCar_ObjectVisible` as the game leaves it during a race (used until the simulation
/// provides its own): the parts drawn, by GEO part number.
pub const CAR_PARTS_VISIBLE: [u8; 57] = [
    0, 0, 1, 1, 0, 10, 0, 0, 11, 11, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 15, 0, 0, 15, 0, 0, 1, 0, 0, 1, 13, 0, 0, 0, 3, 3, 4, 4, 4, 1, 1, 5, 6,
    0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0,
];

/// R3DCar_ObjectInfo byte 0 per part: the ordering-table bias the part is sorted with
/// (negative = drawn over the parts around it: plates, lights, decals).
pub const CAR_PART_BIAS: [i8; 57] = [
    0, 0, 32, 48, -8, -16, -32, -32, -20, -16, -20, -16, -24, -24, -44, -44, -31, -31, -44, -30, -30, -44, -30, -30, -44, -30, -30, -44, -24, -24,
    -44, -40, -12, -12, -44, 48, 40, 38, 38, 36, 36, 0, 0, -16, -16, -24, -24, 31, 31, 32, 32, 32, 32, 32, 32, 32, 32,
];

/// The number plate's two halves: the car textures (indices into [`CAR_TEXTURES`]) they
/// replace, per car type (CarIO_UpdateCarTextureData's table at 0x8011FDD0).
const PLATE_TEXTURES: [(u8, u8); 21] = [
    (27, 33), (33, 26), (27, 33), (33, 18), (27, 33), (27, 33), (18, 33), (27, 33), (27, 33), (26, 33), (27, 33),
    (27, 33), (33, 17), (33, 18), (27, 33), (33, 26), (27, 33), (27, 33), (26, 33), (27, 33), (33, 26),
];

/// The car's texture names by a GEO polygon's texture number (CarIO's table): each is a
/// shape of the car's PSH files, placed at its own VRAM position.
const CAR_TEXTURES: [&str; 51] = [
    "frnt", "rear", "side", "top1", "top2", "dHiF", "dHiR", "dHiS", "dTp1", "dTop", "dLoF", "dLoR", "dLoS", "wndw", "ftdm", "splr", "ext2", "ext3",
    "ext4", "whl ", "ext1", "botm", "whlI", "tred", "bROf", "bSOf", "rvOf", "sFOf", "sROf", "ltOf", "bROn", "bSOn", "shdw", "sFOn", "sROn",
    "ltOn", "dcl1", "dcl2", "dcl3", "dcl4", "dcl5", "dcl6", "ins0", "ins1", "ins2", "ins3", "ins4", "ins5", "ins6", "ins7", "ins8",
];

enum Placement {
    /// Faces the camera (yaw only); scale (width, height, width).
    Billboard(Vec3),
    /// Fixed orientation and scale, in PS1 space (applied as M * v): the transpose of the
    /// game's matrix, as DrawObjectTransform hands the GTE (matrix * view)ᵀ.
    Fixed(glam::Mat3),
}

/// How DrawW_BuildChunkObjectFacets orients a placed object:
/// type 1 as is; types 2 and 5 by the quaternion at +20 (Quatern_QuatToMat) with column
/// scales at +28/+30/+32; type 9 facing the camera if byte +5 bit 0 is set, otherwise by the Y angle
/// at +20 (1024 = 360 degrees), scaled by +24 (x, z) and +22 (y). Scales are 8.8.
fn placement_transform(pl: &grp::Placement) -> Placement {
    let r = &pl.raw;
    let s16 = |o: usize| i16::from_le_bytes([r[o], r[o + 1]]) as f32;
    let scale = |v: f32| if v == 0.0 { 1.0 } else { v / 256.0 };
    match pl.kind {
        2 | 5 => {
            let (x, y, z, w) = (s16(20) / 16384.0, s16(22) / 16384.0, s16(24) / 16384.0, s16(26) / 16384.0);
            // the game's matrix is the transpose of the textbook quaternion rotation
            let m = glam::Mat3::from_cols(
                Vec3::new(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)),
                Vec3::new(2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)),
                Vec3::new(2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)),
            );
            let sc = Vec3::new(scale(s16(28)), scale(s16(30)), scale(s16(32)));
            Placement::Fixed(glam::Mat3::from_cols(m.x_axis * sc.x, m.y_axis * sc.y, m.z_axis * sc.z).transpose())
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
                Placement::Fixed(m.transpose())
            }
        }
        _ => Placement::Fixed(glam::Mat3::IDENTITY),
    }
}

/// Load a car as the race does (R3DCar_ReadInCarData / CarIO_ReadInCarTextureData): the
/// player's car from `ZZZ<code>H.VIV`, the others from `ZZZ<code>S.VIV`. The PSH files hold
/// the texture pages: the interior and underside ("i") on a page of their own, the body's
/// light ("l", colours 0..7) or dark ("d", colours 8..15) set, then "g". `colour` is the
/// paint as [`crate::ps1::race::CarState::colour`] packs it.
pub fn load_car(data: &GameData, atlas: &mut Atlas, code: &str, player: bool, night: bool, plate: Option<&str>, colour: u8) -> Result<CarModel> {
    let name = format!("ZZZ{code}{}.VIV", if player { "H" } else { "S" });
    // (police cars: the livery the game loaded for this track, e.g. CBMW -> ZZZGBMWS.VIV)
    let viv = Viv::parse(
        &data
            .read(&name)
            .or_else(|_| data.read(&format!("ZZZ{code}.VIV")))
            .or_else(|e| crate::ps1::host::opened_variant(code, player).map_or(Err(e), |f| data.read(&f)))?,
    )?;
    let (_, geo) = viv.find_suffix(".geo").context("car viv has no .geo")?;
    let geo = Geo::parse(geo)?;
    // two 256x256 pages; which page and shape each texture name is on
    let mut pages = [crate::formats::psh::Atlas::new(256), crate::formats::psh::Atlas::new(256)];
    let mut page_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // R3DCar_Instantiate3DCar: the body's second texture file is "d" for colours 8..15 and
    // "l" for 0..7, then "g"; each painted with palette row (colour & 7) + 8 when the car's
    // flag for it is set (Car_tObj+0x8B3: bit 0 for the "l"/"d" sheet, bit 1 for the
    // others); night driving changes nothing here
    let _ = night;
    let skip = if colour & 8 != 0 { ['l'] } else { ['d'] };
    let row = |flag: u8| (colour & 7) as usize + if colour >> 4 & flag != 0 { 8 } else { 0 };
    let mut files: Vec<(char, Psh)> = Vec::new();
    for (n, d) in &viv.entries {
        let n = n.to_ascii_lowercase();
        let Some(stem) = n.strip_suffix(".psh") else { continue };
        // the variant letter follows the base name ("zz" and the car's four letters):
        // zzbmwmd.psh is the "d" sheet, zzsand.psh the only one (not a "d" variant)
        let kind = stem.chars().nth(6).unwrap_or(' ');
        if skip.contains(&kind) {
            continue;
        }
        let mut psh = Psh::parse(d)?;
        psh.paint(row(if matches!(kind, 'l' | 'd') { 1 } else { 2 }));
        files.push((kind, psh));
    }
    // the dents have no paint holder of their own: CarIO_ReadInCarTextureData draws each with
    // the (painted) palette of the body texture it belongs to (the texture table at
    // 0x8011FAF4: dHiF/dLoF the front, dHiR/dLoR the rear, dHiS/dLoS the side, dTp1 and dTop
    // the tops), not with the holder that follows it in the file
    let body_palette = |name: &str| files.iter().flat_map(|(_, p)| &p.shapes).find(|s| s.name == name && s.kind == 0x40).map(|s| s.palette.clone());
    let dents: Vec<(&str, Option<Vec<u16>>)> = [("dHiF", "frnt"), ("dLoF", "frnt"), ("dHiR", "rear"), ("dLoR", "rear"), ("dHiS", "side"), ("dLoS", "side"), ("dTp1", "top1"), ("dTop", "top2")]
        .into_iter()
        .map(|(dent, body)| (dent, body_palette(body)))
        .collect();
    for (_, psh) in &mut files {
        for s in psh.shapes.iter_mut().filter(|s| s.kind == 0x40) {
            if let Some((_, Some(pal))) = dents.iter().find(|(d, _)| *d == s.name) {
                if pal.len() >= 16 {
                    for (k, &ix) in s.indices.iter().enumerate() {
                        s.rgba[k * 4..k * 4 + 4].copy_from_slice(&crate::formats::psh::ps1_color(pal[ix as usize & 15]));
                    }
                    s.palette = pal.clone();
                }
            }
        }
    }
    // the later files ("d", "g") last, so they cover the shapes before them
    files.sort_by_key(|(k, _)| matches!(k, 'd' | 'g'));
    for (kind, psh) in &files {
        let page = usize::from(*kind == 'i');
        pages[page].place(&psh.shapes, 64);
        for s in &psh.shapes {
            page_of.insert(s.name.clone(), page);
        }
    }
    // NFSHS_CAR_PAGES=DIR: save the assembled texture pages (for checking the layout)
    if let Ok(dir) = std::env::var("NFSHS_CAR_PAGES") {
        for (k, p) in pages.iter().enumerate() {
            if let Some(img) = image::RgbaImage::from_raw(256, 256, p.rgba.clone()) {
                let _ = img.save(format!("{dir}/{code}_{k}.png"));
            }
        }
    }
    // the player's number plate (CarIO_CreateLicense): the name in ZLICENSE.PSH letters on
    // "blnk", split into the two halves the plate polygons show ("sFOf" and "sFOn" slots)
    // (only the race cars have one: CarIO_ReadInCarTextureData makes it for car types below
    // 22, so the police cars keep their light bar's "sFOf"/"sFOn")
    let plate_slots = crate::ps1::race::car_type(code).and_then(|t| PLATE_TEXTURES.get(t as usize)).copied();
    if let (Some(name), Some((lt, rt)), Ok(lic)) = (plate, plate_slots, data.read("ZLICENSE.PSH").and_then(|d| Psh::parse(&d))) {
        let shape_pos = |n: &str| files.iter().flat_map(|(_, p)| p.shapes.iter()).find(|s| s.name == n).map(|s| ((s.vram.0 as usize % 64) * 4, s.vram.1 as usize % 256));
        // which textures the plate's halves replace, per car type (the game's table at
        // 0x8011FDD0: texture index for the left half, then the right half)
        let (lname, rname) = (CAR_TEXTURES[lt as usize], CAR_TEXTURES[rt as usize]);
        if let (Some(blnk), Some(left), Some(right)) = (lic.shapes.iter().find(|s| s.name == "blnk"), shape_pos(lname), shape_pos(rname)) {
            let (w, h) = (blnk.width as usize, blnk.height as usize);
            let mut idx = blnk.indices.clone();
            let name: Vec<char> = name.chars().map(|c| c.to_ascii_lowercase()).collect();
            let mut x = 24 - 3 * name.len() as isize;
            for &c in &name {
                if let Some(l) = lic.shapes.iter().find(|s| s.name.trim_end() == c.to_string() && c != ' ') {
                    for ly in 0..l.height as usize {
                        for lx in 0..l.width as usize {
                            let (px, py) = (x + lx as isize, 5 + ly);
                            if px >= 0 && (px as usize) < w && py < h {
                                idx[py * w + px as usize] = l.indices[ly * l.width as usize + lx];
                            }
                        }
                    }
                }
                x += 6;
            }
            for (half, (ox, oy)) in [(0, left), (1, right)] {
                for y in 0..h {
                    for xx in 0..24 {
                        let (dx, dy) = (ox + xx, oy + y);
                        if dx < 256 && dy < 256 {
                            let c = crate::formats::psh::ps1_color(blnk.palette.get(idx[y * w + half * 24 + xx] as usize).copied().unwrap_or(0));
                            let o = (dy * 256 + dx) * 4;
                            pages[0].rgba[o..o + 4].copy_from_slice(&c);
                        }
                    }
                }
            }
        }
    }
    let rects = [atlas.add(256, 256, &pages[0].rgba), atlas.add(256, 256, &pages[1].rgba)];
    let mut parts = Vec::with_capacity(geo.parts.len());
    for (pi, part) in geo.parts.iter().enumerate() {
        let off = Vec3::new(part.position[0] as f32, part.position[1] as f32, part.position[2] as f32) / 65536.0;
        // wheels: discs 47/48, front tyres 49..52, rear wheels 53..56 (left, right)
        let wheel = match pi {
            47 | 49 | 51 => Some(0),
            48 | 50 | 52 => Some(1),
            53 | 55 => Some(2),
            54 | 56 => Some(3),
            _ => None,
        };
        let hub = if wheel.is_some() { ps1_to_world(off) } else { Vec3::ZERO };
        let mut verts = Vec::new();
        let mut env = matches!(pi, 0 | 8 | 10 | 41 | 42 | 45 | 46).then(|| (Vec::new(), Vec::new()));
        let mut env_vertices = Vec::new();
        let tris: Vec<CarTri> = part.polys.iter().map(|p| CarTri { texture: p.texture, flags: p.flags as u16 | (p.flags2 as u16) << 8, uv: p.uv }).collect();
        for poly in &part.polys {
            if let Some((normals, flags)) = &mut env {
                flags.push((poly.flags as u16 | (poly.flags2 as u16) << 8) & 0xFFF);
                env_vertices.extend(poly.v.map(|v| v as u16));
                for k in 0..3 {
                    let n = part.normals.get(poly.v[k] as usize).copied().unwrap_or([0; 3]);
                    normals.push(n.map(|c| c >> 6));
                }
            }
            let tex = CAR_TEXTURES.get(poly.texture as usize).copied().unwrap_or("");
            let rect = rects[page_of.get(tex).copied().unwrap_or(0)];
            for k in 0..3 {
                let v = part.verts.get(poly.v[k] as usize).copied().unwrap_or([0; 3]);
                let p = ps1_to_world(off + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 256.0) - hub;
                let t = Vec2::new(poly.uv[k][0] as f32 + 0.5, poly.uv[k][1] as f32 + 0.5);
                // DrawC_Prim: flags bit 0 makes the polygon semi-transparent (its STP texels
                // blend: the windows)
                let a = if poly.flags & 1 != 0 { 0.5 } else { 1.0 };
                verts.push(Vertex { pos: p.into(), uv: atlas.uv(rect, t), color: [0.5, 0.5, 0.5, a] });
            }
        }
        let subtract = part.polys.iter().all(|p| CAR_TEXTURES.get(p.texture as usize) == Some(&"shdw")) && !part.polys.is_empty();
        parts.push(CarPart { verts, env, env_vertices, geo_vertices: part.verts.len(), tris, wheel, hub, subtract });
    }
    let mut max = Vec3::ZERO;
    for p in parts.iter().filter(|p| p.wheel.is_none()) {
        for v in &p.verts {
            max = max.max(Vec3::from(v.pos).abs());
        }
    }
    let texture_page = CAR_TEXTURES.iter().map(|n| page_of.get(*n).map(|&p| p as u8)).collect();
    Ok(CarModel { parts, half_extents: max, texture_page, page_rects: rects, atlas_size: atlas.size as f32, texture_subs: Vec::new(), part_modes: Vec::new() })
}

impl CarModel {
    /// DrawC_Prim's texture substitute for triangle `t` of part `pi` (drawing modes 8 and 9)
    /// under the car's light and damage `codes`: the triangle's UVs on the substitute texture
    /// (the lit brake light, the dented panel) and whether it is drawn at full brightness
    /// (mode 8, code bit 0x80: a lit lamp), or None to draw it as it is.
    pub fn substitute(&self, pi: usize, t: usize, codes: &[u16]) -> Option<([[f32; 2]; 3], bool)> {
        let mode = *self.part_modes.get(pi)?;
        if mode != 8 && mode != 9 {
            return None;
        }
        let tri = self.parts.get(pi)?.tris.get(t)?;
        let code = *codes.get(tri.texture as usize)?;
        if code == 0 {
            return None;
        }
        // the mirrored twin takes the high byte
        let mut a = if (tri.flags as i16) < 0 { code >> 8 } else { code & 0xFF } as u32;
        // the polygon's damage zones pick the code's 2-bit field
        let zones = (tri.flags & 0xFFF) as u32;
        if zones & 0x3F0 != 0 {
            a &= zones >> 4;
            while a != 0 && a & 3 == 0 {
                a >>= 2;
            }
        }
        let k = (a & 3) as usize;
        if k == 0 {
            return None;
        }
        let (offset, du, dv) = self.texture_subs.get(tri.texture as usize)?[k - 1];
        let texture = (tri.texture as i16 + offset) as usize;
        let page = self.texture_page.get(texture).copied().flatten().or_else(|| self.texture_page.get(tri.texture as usize).copied().flatten())?;
        let r = self.page_rects[page as usize];
        let uv = tri.uv.map(|[u, v]| {
            let (u, v) = (u.wrapping_add(du), v.wrapping_add(dv));
            [(r.x as f32 + u as f32 + 0.5) / self.atlas_size, (r.y as f32 + v as f32 + 0.5) / self.atlas_size]
        });
        Some((uv, mode == 8 && a & 0x80 != 0))
    }

    /// The reflection normals of part `pi` from per-vertex normals as the original keeps them
    /// in memory (R3DCar's loaded part objects: about 64 long, not the GEO's).
    /// (Left as they are when the model is another one than the game's: vertex counts differ.)
    pub fn set_env_normals(&mut self, pi: usize, normals: &[[i16; 3]]) {
        let Some(part) = self.parts.get_mut(pi) else { return };
        if part.geo_vertices != normals.len() {
            return;
        }
        let Some((env, _)) = &mut part.env else { return };
        for (n, &v) in env.iter_mut().zip(&part.env_vertices) {
            if let Some(&m) = normals.get(v as usize) {
                *n = m;
            }
        }
    }
}

/// The track's reflection maps (ZTR<nn>R.PSH: "test", "tree", "tunn", "buil"), laid out as
/// the game loads them: a 256x256 page with each 128x64 map at u 128, v 64 * n
/// (Track_gReflectionMaps). Returns the page's atlas rectangle.
pub fn load_reflection_maps(data: &GameData, atlas: &mut Atlas, track_no: u32) -> Option<Rect> {
    let psh = Psh::parse(&data.read(&format!("ZTR{track_no:02}R.PSH")).ok()?).ok()?;
    let mut page = crate::formats::psh::Atlas::new(256);
    for s in psh.shapes.iter().filter(|s| s.width > 1) {
        page.blit(s, 128, s.vram.1 as u32 % 256);
    }
    Some(atlas.add(256, 256, &page.rgba))
}

