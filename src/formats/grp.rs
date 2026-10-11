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
//!               0x0f virtual road (fine path), 0x24/0x26 misc
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
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub verts: Vec<[i16; 4]>,
    pub quads: Vec<Quad>,
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    pub origin: Vec3,
    /// Road + terrain surface (vertex pool 0x18).
    pub ground: Mesh,
    /// Roadside detail (vertex pool 0x27).
    pub detail: Mesh,
}

/// Virtual-road sample (0x21/0x0f): fine centre line with frame vectors and lane widths.
#[derive(Clone, Copy, Debug)]
pub struct VRoad {
    pub pos: Vec3,
    pub normal: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub raw: [u8; 20],
}

pub struct Track {
    pub nodes: Vec<Vec3>,
    pub blocks: Vec<Block>,
    pub textures: Vec<TexEntry>,
    pub vroad: Vec<VRoad>,
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
            out.push(Quad { tex, v: [row + i, row + i + 1, next + i + 1, next + i] });
        }
        o += len.max(4 + 2 * n);
    }
}

fn quads(body: &[u8], n: usize, out: &mut Vec<Quad>) {
    for i in 0..n.min(body.len() / 6) {
        let q = &body[i * 6..i * 6 + 6];
        out.push(Quad { tex: u16le(q, 0), v: [q[2] as u16, q[3] as u16, q[4] as u16, q[5] as u16] });
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
                    if let Some(g) = find(c.body, 0x17) {
                        let hdr = find(g.body, 0x1b).map(|h| h.body).unwrap_or(&[]);
                        let cnt = |i: usize| if hdr.len() >= i * 2 + 2 { u16le(hdr, i * 2) as usize } else { 0 };
                        for s in chunks(g.body) {
                            match s.kind {
                                0x18 => b.ground.verts = verts(s.body),
                                0x19 => quads(s.body, cnt(6), &mut b.ground.quads),
                                0x1a | 0x25 => strips(s.body, s.count, &mut b.ground.quads),
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
        Ok(Self { nodes, blocks, textures, vroad })
    }
}
