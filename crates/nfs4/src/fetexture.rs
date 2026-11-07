//! FETEXTURE.CPP: a shape of a shape file put in VRAM and described as a front-end shape
//! (`tTexture_ShapeInfo`, see `psxfront`): unpacked first when packed, its CLUT found (but for
//! 16-bit shapes), uploaded at its place in VRAM (moved by an offset), its size, origin, texture
//! place, depth, texture page and CLUT noted.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::types::TextureShapeInfo;
use ps1rt::{Cpu, Mem};

/// `"unpacked"`.
const UNPACKED: u32 = 0x8001_27F8;

/// The shape library's: a shape's header in a file, the size it unpacks to, unpacking it;
/// `reservememadr`, `purgememadr`, `blockmove`; a shape kind's CLUT place; the shape and its
/// CLUT uploaded to VRAM; a shape's depth; `GetClut`.
const SHAPE_POINTER: fn(&mut Cpu) = nfs4rc::f_800f17ec;
const UNPACK_SIZE: fn(&mut Cpu) = nfs4rc::f_800e4c10;
const UNPACK: fn(&mut Cpu) = nfs4rc::f_800e4b28;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const GET_CLUT_ID: u32 = 0x800e_0544;
const UPLOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f7304;
const SHAPE_DEPTH: fn(&mut Cpu) = nfs4rc::f_800f4d40;
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;

/// A shape's header (16 bytes): +0 its kind (low 2 bits; 2 = 16-bit, no CLUT; bit 7 packed)
/// and the offset to the next block (24 bits), +4 its size, +8 its origin, +12 its place in VRAM
/// (x and y in 12-bit signed fields); 20 bytes are kept to put back.
const HEADER: u32 = 16;
const KEPT: u32 = 20;
const PACKED: u8 = 0x80;
const KIND_16BIT: u32 = 2;
/// The front-end shape's bytes that have no field of their own: +9 the shape's kind byte, +12
/// (24 bits) the offset to its next block.
const INFO_KIND: u32 = 9;
const INFO_NEXT: u32 = 12;

/// A 12-bit signed field at `shift` (from the top) of a shape's place word.
fn field12(v: u32, shift: u32) -> u32 {
    ((v << shift) as i32 >> 20) as u32
}

/// The place word's x, y.
fn place(m: &mut Mem, s: u32) -> (u32, u32) {
    let p = m.r32(s + 12);
    (field12(p, 20), field12(p, 4))
}

/// `FETexture_LoadPmxAtOffset` (FETEXTURE.CPP, 0x8004f5e8): shape `k` of the shape file `file`
/// into `info`, uploaded `dy` lines down and `dx` across from its place (its header put back
/// after when moved); the unpacked copy freed. Nothing when the file has no such shape.
pub fn load_pmx_at_offset(c: &mut Cpu, file: u32, k: u32, info: TextureShapeInfo, dy: u32, dx: u32) {
    let shape = call(c, SHAPE_POINTER, &[file, k]);
    if shape == 0 {
        return;
    }
    let packed = c.m.r8(shape) & PACKED != 0;
    // the shape, unpacked into memory of its own when packed (its header copied, the packed
    // bit cleared, the next block's offset made relative to the copy)
    let s = if packed {
        let size = call(c, UNPACK_SIZE, &[shape + HEADER]);
        let s = call(c, RESERVE, &[UNPACKED, size.wrapping_add(HEADER), 0]);
        call(c, BLOCKMOVE, &[shape, s, HEADER]);
        call(c, UNPACK, &[shape + HEADER, s + HEADER]);
        let m = &mut c.m;
        let v = m.r8(s) & !PACKED;
        m.w8(s, v);
        let next = (m.r32(shape) as i32 >> 8) as u32;
        let v = m.r8(s) as u32 | (shape.wrapping_add(next).wrapping_sub(s) << 8);
        m.w32(s, v);
        s
    } else {
        shape
    };
    with_buffer(c, 8 + KEPT, |c, buffer| {
        let (clut_at, kept) = (buffer, buffer + 8);
        c.m.w32(clut_at, 0);
        c.m.w32(clut_at + 4, 0);
        let kind = c.m.r8(s) as u32 & 3;
        if kind != KIND_16BIT {
            crate::call::call_jal(c, GET_CLUT_ID, &[kind, clut_at, clut_at + 4]);
            let m = &mut c.m;
            let v = (m.r32(clut_at + 4) << 6) | ((m.r32(clut_at) as i32 >> 4) as u32 & 0x3F);
            info.set_clut_id(m, v as i32);
        }
        // the header kept, the shape uploaded at its place moved
        let m = &mut c.m;
        for k in 0..KEPT / 4 {
            let v = m.r32(s + 4 * k);
            m.w32(kept + 4 * k, v);
        }
        let (x, y) = place(m, s);
        let (clut_x, clut_y) = (m.r32(clut_at), m.r32(clut_at + 4));
        call(c, UPLOAD_SHAPE, &[s, x.wrapping_add(dx), y.wrapping_add(dy), clut_x, clut_y]);
        // the description
        let m = &mut c.m;
        info.set_shpptr(m, shape);
        let b = m.r8(s);
        let v = m.r32(info.0 + INFO_NEXT);
        m.w8(info.0 + INFO_KIND, b);
        let v = (v & 0xFF00_0000) | (m.r32(s) >> 8);
        m.w32(info.0 + INFO_NEXT, v);
        let size = (m.r16(s + 4), m.r16(s + 6), m.r16(s + 8), m.r16(s + 10));
        info.set_width(m, size.0 as i16);
        info.set_height(m, size.1 as i16);
        info.set_centerx(m, size.2 as i16);
        info.set_centery(m, size.3 as i16);
        let (x, _) = place(m, s);
        info.set_shapex(m, x as i16);
        let (_, y) = place(m, s);
        info.set_shapey(m, y as i16);
        let depth = call(c, SHAPE_DEPTH, &[s]);
        let m = &mut c.m;
        let clut = info.clut_id(m) as u32;
        let blend = m.r8(info.0 + INFO_KIND) as u32;
        let (ty, tx) = (info.shapey(m) as u16 as u32, info.shapex(m) as u16 as u32);
        info.set_depth(m, depth as i8);
        let page = ((blend & 3) << 7) | ((((ty & 0x100) << 16) as i32 >> 20) as u32) | ((tx & 0x3FF) >> 6) | ((ty & 0x200) << 2);
        info.set_tpage(m, page as u16);
        let v = call(c, GET_CLUT, &[(clut & 0x3F) << 4, (clut as i32 >> 6) as u32, page, (ty & 0x200) << 2]);
        info.set_clut(&mut c.m, v as u16);
        if dx != 0 || dy != 0 {
            let m = &mut c.m;
            for k in 0..KEPT / 4 {
                let v = m.r32(kept + 4 * k);
                m.w32(s + 4 * k, v);
            }
        }
    });
    if packed {
        call(c, PURGE, &[s]);
    }
}

pub fn abi_load_pmx_at_offset(c: &mut Cpu) {
    let dx = crate::abi::arg(c, 4);
    load_pmx_at_offset(c, c.r[4], c.r[5], TextureShapeInfo(c.r[6]), c.r[7], dx);
}

/// `FETexture_LoadPmx` (FETEXTURE.CPP, 0x8004f8a0): the shape at its place.
pub fn load_pmx(c: &mut Cpu, file: u32, k: u32, info: TextureShapeInfo) {
    load_pmx_at_offset(c, file, k, info, 0, 0);
}

pub fn abi_load_pmx(c: &mut Cpu) {
    load_pmx(c, c.r[4], c.r[5], TextureShapeInfo(c.r[6]));
}

/// For callers not rewritten yet: the functions with the C convention's arguments.
pub(crate) fn load_pmx_at_offset_entry(c: &mut Cpu) {
    abi_load_pmx_at_offset(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A shape file of four shapes (a count at +8, entries of a name and an offset from +16),
    /// each a 16-byte header and 112 bytes of data; the description.
    const FILE: u32 = 0x8014_5000;
    const SHAPES: u32 = FILE + 0x100;
    const INFO: u32 = 0x8014_5400;

    /// Each shape an image of any depth (0x40..0x43), its size, origin and place (the place's
    /// high texture rows too) any, a palette attached or not (`0x800f7304` uploads the chain of
    /// blocks after it, converting palettes in a 512-byte buffer on its stack: random links and
    /// counts overrun it into its saved registers, which real files never do); packed now and
    /// then: its data a tiny RefPack stream of literals, or a stream the unpacker doesn't know
    /// (nothing unpacked).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(FILE + 8, 4);
        for k in 0..4u32 {
            m.w32(FILE + 16 + 8 * k, 0x4142_4300 | k);
            m.w32(FILE + 20 + 8 * k, 0x100 + 0x80 * k);
            let s = SHAPES + 0x80 * k;
            for j in 0..0x80 {
                m.w8(s + j, rng.next() as u8);
            }
            let packed = pick(rng, 3) == 0;
            m.w8(s, 0x40 | (pick(rng, 4) as u8) | if packed { 0x80 } else { 0 });
            m.w16(s + 4, pick(rng, 64) as u16);
            m.w16(s + 6, pick(rng, 64) as u16);
            // no attached block, or a palette 0x40 on (its count in the upload's 512-byte
            // buffer, nothing after it)
            let next = [0, 0x40][pick(rng, 2) as usize];
            m.w8(s + 1, next as u8);
            m.w8(s + 2, 0);
            m.w8(s + 3, 0);
            m.w8(s + 0x40, [0x22, 0x23, 0x24, 0x2A, 0x2D, 0x29, 34 + pick(rng, 34)][pick(rng, 7) as usize] as u8);
            m.w8(s + 0x41, 0);
            m.w16(s + 0x42, 0);
            m.w16(s + 0x44, pick(rng, 65) as u16);
            m.w16(s + 0x46, pick(rng, 2) as u16);
            if packed && pick(rng, 2) == 0 {
                let n = 1 + pick(rng, 3);
                for (j, b) in [0x10, 0xFB, 0, 0, n, 0xFC | n].into_iter().enumerate() {
                    m.w8(s + 16 + j as u32, b as u8);
                }
            }
        }
        for k in 0..32 {
            m.w8(INFO + k, rng.next() as u8);
        }
    }

    fn offset(r: &mut Rng) -> u32 {
        [0, 0, 1, 64, (-16i32) as u32, pick(r, 1024)][pick(r, 6) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("FETexture_LoadPmxAtOffset", 0x8004_f5e8, nfs4rc::f_8004f5e8, |_, r| vec![FILE, pick(r, 6), INFO, offset(r), offset(r)]),
            ("FETexture_LoadPmx", 0x8004_f8a0, nfs4rc::f_8004f8a0, |_, r| vec![FILE, pick(r, 6), INFO]),
        ]
    }

    #[test]
    fn fetexture_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x8C0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x8C1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
