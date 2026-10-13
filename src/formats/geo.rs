//! Car models (`.GEO` inside car `.VIV` archives).
//!
//! The file is a memory image saved with absolute PS1 pointers:
//! ```text
//! u32 part_ptr[57]
//! ... misc table ...
//! part: u16 nverts, u16 npolys, i32 pos[3], u32 verts_ptr, u32 normals_ptr, u32 polys_ptr
//! vertex: i16 x, y, z      normal: i16 x, y, z
//! poly (12 bytes): u8 flags, u8 flags2, u8 texture, u8 v[3], u8 uv[3][2]
//! ```
//! The load base is recovered from the first part's vertex pointer, which always points
//! directly behind the 0x1c-byte part header.

use super::{i16le, i32le, u16le, u32le};
use anyhow::{Result, bail};

pub const PART_COUNT: usize = 57;
const PART_HEADER: usize = 0x1c;

#[derive(Clone, Debug)]
pub struct GeoPoly {
    pub flags: u8,
    pub flags2: u8,
    pub texture: u8,
    pub v: [u8; 3],
    pub uv: [[u8; 2]; 3],
}

#[derive(Clone, Debug, Default)]
pub struct GeoPart {
    pub position: [i32; 3],
    pub verts: Vec<[i16; 3]>,
    pub normals: Vec<[i16; 3]>,
    pub polys: Vec<GeoPoly>,
}

pub struct Geo {
    pub parts: Vec<GeoPart>,
}

impl Geo {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < PART_COUNT * 4 {
            bail!("geo too small");
        }
        let ptrs: Vec<u32> = (0..PART_COUNT).map(|i| u32le(d, i * 4)).collect();
        if ptrs.iter().all(|&p| p == 0) {
            return Ok(Self::parse_packed(d));
        }
        // Locate the first non-empty part and derive the base address from its vertex pointer.
        let mut base = None;
        let lo = ptrs.iter().copied().min().unwrap();
        for &p in &ptrs {
            // try every candidate offset such that header->verts_ptr == header + 0x1c
            let _ = p;
        }
        // Search for a header whose vertex pointer points to itself + 0x1c.
        for off in (PART_COUNT * 4..d.len().saturating_sub(PART_HEADER)).step_by(4) {
            let vp = u32le(d, off + 16);
            let candidate = vp.wrapping_sub((off + PART_HEADER) as u32);
            if vp != 0 && candidate.wrapping_add(off as u32) == lo && u16le(d, off) > 0 {
                base = Some(candidate);
                break;
            }
        }
        let Some(base) = base else { bail!("could not determine geo load base") };
        let to_off = |p: u32| -> Option<usize> {
            if p == 0 {
                return None;
            }
            let o = p.wrapping_sub(base) as usize;
            (o < d.len()).then_some(o)
        };
        let mut parts = Vec::with_capacity(PART_COUNT);
        for &p in &ptrs {
            let Some(o) = to_off(p) else {
                parts.push(GeoPart::default());
                continue;
            };
            let nv = u16le(d, o) as usize;
            let np = u16le(d, o + 2) as usize;
            let position = [i32le(d, o + 4), i32le(d, o + 8), i32le(d, o + 12)];
            let mut part = GeoPart { position, ..Default::default() };
            if let Some(vo) = to_off(u32le(d, o + 16)) {
                part.verts = (0..nv)
                    .map(|i| [i16le(d, vo + i * 6), i16le(d, vo + i * 6 + 2), i16le(d, vo + i * 6 + 4)])
                    .collect();
            }
            if let Some(no) = to_off(u32le(d, o + 20)) {
                part.normals = (0..nv)
                    .map(|i| [i16le(d, no + i * 6), i16le(d, no + i * 6 + 2), i16le(d, no + i * 6 + 4)])
                    .collect();
            }
            if let Some(po) = to_off(u32le(d, o + 24)) {
                for i in 0..np {
                    let q = &d[po + i * 12..po + i * 12 + 12];
                    part.polys.push(GeoPoly {
                        flags: q[0],
                        flags2: q[1],
                        texture: q[2],
                        v: [q[3], q[4], q[5]],
                        uv: [[q[6], q[7]], [q[8], q[9]], [q[10], q[11]]],
                    });
                }
            }
            parts.push(part);
        }
        Ok(Self { parts })
    }

    /// The traffic cars' layout: no pointers, the parts one after another from 0x24C, each a
    /// part header, its vertices and its polygons (no normals); parts 0..47 (the last one
    /// the wheel disc).
    fn parse_packed(d: &[u8]) -> Self {
        let mut parts = Vec::with_capacity(PART_COUNT);
        let mut o = 0x24C;
        while parts.len() < PART_COUNT && o + PART_HEADER <= d.len() {
            let (nv, np) = (u16le(d, o) as usize, u16le(d, o + 2) as usize);
            let end = o + PART_HEADER + nv * 6 + np * 12;
            if nv > 1000 || end > d.len() {
                break;
            }
            let position = [i32le(d, o + 4), i32le(d, o + 8), i32le(d, o + 12)];
            let vo = o + PART_HEADER;
            let po = vo + nv * 6;
            let verts = (0..nv).map(|i| [i16le(d, vo + i * 6), i16le(d, vo + i * 6 + 2), i16le(d, vo + i * 6 + 4)]).collect();
            let polys = (0..np)
                .map(|i| {
                    let q = &d[po + i * 12..po + i * 12 + 12];
                    GeoPoly { flags: q[0], flags2: q[1], texture: q[2], v: [q[3], q[4], q[5]], uv: [[q[6], q[7]], [q[8], q[9]], [q[10], q[11]]] }
                })
                .collect();
            parts.push(GeoPart { position, verts, normals: Vec::new(), polys });
            o = end;
        }
        parts.resize(PART_COUNT, GeoPart::default());
        Self { parts }
    }
}
