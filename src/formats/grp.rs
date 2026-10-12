//! Track geometry (`ZTRxx.GRP`, plus `N`/`S`/`W` night/snow/wet variants).
//!
//! A tree of chunks, each with a 16-byte header `{u32 type, u32 size (incl. header),
//! u32 0xCDCDCDCD, u32 count}`. Top level (inside root type 0x1e):
//! ```text
//! 0x1f  track info (u32 x8; [7] = node count)
//! 0x20  track nodes: i32 16.16 xyz per node
//! 0x1d  one segment block per node:
//!         0x1c  block header (bounding radius + outline)
//!         0x17  geometry:
//!                 0x1b  counts
//!                 0x18  vertices  {i16 x,y,z, u16 shade}   (world = node + v/64)
//!                 0x19  quads     {u16 tex, u8 v[4]}        (count in 0x1b)
//!                 0x1a  strips    rows {u8 next_row, u8 row, u8 n, u8 2(n+2), u16 tex[n]}
//!                 0x25  strips    (same layout, extra surface)
//!                 0x27  vertices  (second pool)
//!                 0x28  quads into 0x27 pool
//!                 0x29  quads into 0x27 pool
//!         0x03  object placements, 0x0b light/sound emitter, others: visibility/AI
//! 0x21  global: 0x02 texture table, 0x08 prop models, 0x07 animated objects,
//!               0x0f virtual road: 32-byte samples {i32 pos[3] (16.16), i8 normal[3],
//!                     i8 forward[3], i8 right[3], u8 pad, u16 flags, u16 left_wall, u16 right_wall
//!                     (1/256 m), u8 block, u8 ?, u8 ?, u8 ?}
//!               0x24/0x26 misc
//! 0x23  colour table
//! ```

use super::{i16le, i32le, u16le, u32le};
use anyhow::{Result, bail};
use glam::Vec3;

const PAD: u32 = 0xCDCD_CDCD;

#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    pub kind: u32,
    pub count: u32,
    pub body: &'a [u8],
}

pub fn chunks(d: &[u8]) -> impl Iterator<Item = Chunk<'_>> {
    let mut o = 0usize;
    std::iter::from_fn(move || {
        if o + 16 > d.len() || u32le(d, o + 8) != PAD {
            return None;
        }
        let size = u32le(d, o + 4) as usize;
        if size < 16 || o + size > d.len() {
            return None;
        }
        let c = Chunk { kind: u32le(d, o), count: u32le(d, o + 12), body: &d[o + 16..o + size] };
        o += size;
        Some(c)
    })
}

fn find<'a>(d: &'a [u8], kind: u32) -> Option<Chunk<'a>> {
    chunks(d).find(|c| c.kind == kind)
}

#[derive(Clone, Copy, Debug)]
pub struct TexEntry {
    pub shape: u16,
    pub mode: u8,
    pub flags: u8,
    pub color: [u8; 3],
    pub extra: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub tex: u16,
    pub v: [u16; 4],
    /// Built from a strip row (0x1A/0x25): wound the other way round than stored quads.
    pub strip: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub verts: Vec<[i16; 4]>,
    pub quads: Vec<Quad>,
}

/// Object placement from a block's 0x03 chunk (length-prefixed records).
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// Record type: 1 = plain model (20 bytes), 5 = rotated/scaled model (36 bytes),
    /// 9 = scaled object / sprite (28 bytes).
    pub kind: u8,
    pub model: u16,
    pub pos: Vec3,
    /// Yaw as (cos, sin); type 5 stores them as 2.30 fixed point.
    pub rot: (f32, f32),
    /// Scale (x, y, z), 0x100 = 1.0.
    pub scale: Vec3,
    /// The whole record (up to 36 bytes).
    pub raw: [u8; 36],
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    pub origin: Vec3,
    pub placements: Vec<Placement>,
    /// Road + terrain surface (vertex pool 0x18).
    pub ground: Mesh,
    /// Roadside detail (vertex pool 0x27).
    pub detail: Mesh,
    /// Painted road lines (0x09, Chunk::lineBuf): `Trk_Line` records
    /// (first point in the ground vertex pool, slice in the chunk, type, quad index).
    pub lines: Vec<[u8; 4]>,
}

/// Virtual-road sample (0x21/0x0f): fine centre line with frame vectors and lane widths.
#[derive(Clone, Copy, Debug)]
pub struct VRoad {
    pub pos: Vec3,
    pub normal: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub flags: u16,
    /// Drivable distance from the centre line to the left / right wall (m).
    pub left_wall: f32,
    pub right_wall: f32,
    pub block: u8,
    pub raw: [u8; 20],
}

pub struct Track {
    pub nodes: Vec<Vec3>,
    pub blocks: Vec<Block>,
    pub textures: Vec<TexEntry>,
    pub vroad: Vec<VRoad>,
    /// Prop models (0x21/0x08), vertices in 1/64 m around the placement point.
    pub models: Vec<Mesh>,
    /// Header flags of each prop model.
    pub model_flags: Vec<u16>,
}

pub fn fx(v: i32) -> f32 {
    v as f32 / 65536.0
}

fn strips(body: &[u8], n_rows: u32, out: &mut Vec<Quad>) {
    let mut o = 0usize;
    for _ in 0..n_rows {
        if o + 4 > body.len() {
            break;
        }
        let next = body[o] as u16;
        let row = body[o + 1] as u16;
        let n = body[o + 2] as usize;
        let len = body[o + 3] as usize;
        for i in 0..n {
            let tex = u16le(body, o + 4 + i * 2);
            let i = i as u16;
            // corner order as DrawW_StripDraw_High builds its Trk_Quad
            out.push(Quad { tex, v: [next + i + 1, next + i, row + i, row + i + 1], strip: true });
        }
        o += len.max(4 + 2 * n);
    }
}

fn quads(body: &[u8], n: usize, out: &mut Vec<Quad>) {
    for i in 0..n.min(body.len() / 6) {
        let q = &body[i * 6..i * 6 + 6];
        out.push(Quad { tex: u16le(q, 0), v: [q[2] as u16, q[3] as u16, q[4] as u16, q[5] as u16], strip: false });
    }
}

fn verts(body: &[u8]) -> Vec<[i16; 4]> {
    (0..body.len() / 8)
        .map(|i| [i16le(body, i * 8), i16le(body, i * 8 + 2), i16le(body, i * 8 + 4), i16le(body, i * 8 + 6)])
        .collect()
}

impl Track {
    pub fn parse(d: &[u8]) -> Result<Self> {
        let Some(root) = chunks(d).next() else { bail!("not a GRP file") };
        let mut nodes = Vec::new();
        let mut blocks = Vec::new();
        let mut textures = Vec::new();
        let mut vroad = Vec::new();
        let mut models = Vec::new();
        let mut model_flags = Vec::new();
        for c in chunks(root.body) {
            match c.kind {
                0x20 => {
                    nodes = (0..c.body.len() / 12)
                        .map(|i| {
                            Vec3::new(fx(i32le(c.body, i * 12)), fx(i32le(c.body, i * 12 + 4)), fx(i32le(c.body, i * 12 + 8)))
                        })
                        .collect();
                }
                0x1d => {
                    let origin = nodes.get(blocks.len()).copied().unwrap_or(Vec3::ZERO);
                    let mut b = Block { origin, ..Default::default() };
                    if let Some(pl) = find(c.body, 0x03) {
                        let d = pl.body;
                        let mut o = 0;
                        for _ in 0..pl.count {
                            let len = d.get(o).copied().unwrap_or(0) as usize;
                            if len < 20 || o + len > d.len() {
                                break;
                            }
                            let r = &d[o..o + len];
                            let mut rot = (1.0, 0.0);
                            let mut scale = Vec3::ONE;
                            match (r[2], len) {
                                (5, 36) => {
                                    rot = (i32le(r, 20) as f32 / (1u32 << 30) as f32, i32le(r, 24) as f32 / (1u32 << 30) as f32);
                                    scale = Vec3::new(u16le(r, 28) as f32, u16le(r, 30) as f32, u16le(r, 32) as f32) / 256.0;
                                }
                                (9, 28) => {
                                    let (sy, sx) = (u16le(r, 22) as f32 / 256.0, u16le(r, 24) as f32 / 256.0);
                                    scale = Vec3::new(sx, sy, sx);
                                }
                                _ => {}
                            }
                            b.placements.push(Placement {
                                kind: r[2],
                                model: u16le(r, 6),
                                pos: Vec3::new(fx(i32le(r, 8)), fx(i32le(r, 12)), fx(i32le(r, 16))),
                                rot,
                                scale,
                                raw: std::array::from_fn(|i| r.get(i).copied().unwrap_or(0)),
                            });
                            o += len;
                        }
                    }
                    if let Some(l) = find(c.body, 0x09) {
                        b.lines = l.body.chunks_exact(4).take(l.count as usize).map(|r| [r[0], r[1], r[2], r[3]]).collect();
                    }
                    if let Some(g) = find(c.body, 0x17) {
                        let hdr = find(g.body, 0x1b).map(|h| h.body).unwrap_or(&[]);
                        let cnt = |i: usize| if hdr.len() >= i * 2 + 2 { u16le(hdr, i * 2) as usize } else { 0 };
                        for s in chunks(g.body) {
                            match s.kind {
                                0x18 => b.ground.verts = verts(s.body),
                                // 0x19 holds the chunk's four render quad lists back to back
                                // (Chunk::renderQuads, sized by 0x1B counts 6, 7, 10 and 11),
                                // followed by copies of the detail lists (counts 8, 9)
                                0x19 => quads(s.body, cnt(6) + cnt(7) + cnt(10) + cnt(11), &mut b.ground.quads),
                                // 0x1A = Chunk::stripBuf; 0x25 = lorezstripBuf, a low-detail
                                // version of the same surfaces for distant chunks (not drawn)
                                0x1a => strips(s.body, s.count, &mut b.ground.quads),
                                0x27 => b.detail.verts = verts(s.body),
                                0x28 => quads(s.body, cnt(8), &mut b.detail.quads),
                                0x29 => quads(s.body, cnt(9), &mut b.detail.quads),
                                _ => {}
                            }
                        }
                    }
                    blocks.push(b);
                }
                0x21 => {
                    for s in chunks(c.body) {
                        match s.kind {
                            0x02 => {
                                textures = s
                                    .body
                                    .chunks_exact(10)
                                    .map(|r| TexEntry {
                                        shape: u16le(r, 0),
                                        mode: r[2],
                                        flags: r[3],
                                        color: [r[4], r[5], r[6]],
                                        extra: u16le(r, 8),
                                    })
                                    .collect();
                            }
                            0x08 => {
                                let b = s.body;
                                let mut o = 0;
                                for _ in 0..s.count {
                                    if o + 4 > b.len() {
                                        break;
                                    }
                                    let nv = b[o + 2] as usize;
                                    let np = b[o + 3] as usize;
                                    let vo = o + 4;
                                    let po = vo + nv * 8;
                                    let mut m = Mesh { verts: verts(&b[vo..po.min(b.len())]), quads: Vec::new() };
                                    quads(&b[po.min(b.len())..], np, &mut m.quads);
                                    models.push(m);
                                    model_flags.push(u16le(b, o));
                                    o = (po + np * 6 + 3) & !3;
                                }
                            }
                            0x0f => {
                                let b = s.body;
                                let v = |o: usize| {
                                    Vec3::new(b[o] as i8 as f32, b[o + 1] as i8 as f32, b[o + 2] as i8 as f32) / 127.0
                                };
                                for i in 0..s.count as usize {
                                    let o = i * 32;
                                    if o + 32 > b.len() {
                                        break;
                                    }
                                    vroad.push(VRoad {
                                        pos: Vec3::new(fx(i32le(b, o)), fx(i32le(b, o + 4)), fx(i32le(b, o + 8))),
                                        normal: v(o + 12),
                                        forward: v(o + 15),
                                        right: v(o + 18),
                                        flags: u16le(b, o + 22),
                                        left_wall: u16le(b, o + 24) as f32 / 256.0,
                                        right_wall: u16le(b, o + 26) as f32 / 256.0,
                                        block: b[o + 28],
                                        raw: b[o + 12..o + 32].try_into().unwrap(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Self { nodes, blocks, textures, vroad, models, model_flags })
    }
}
