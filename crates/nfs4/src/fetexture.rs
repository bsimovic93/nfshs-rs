//! FETEXTURE.CPP: a shape of a shape file put in VRAM and described as a front-end shape
//! (`tTexture_ShapeInfo`, see `psxfront`): unpacked first when packed, its CLUT found (but for
//! 16-bit shapes), uploaded at its place in VRAM (moved by an offset), its size, origin, texture
//! place, depth, texture page and CLUT noted.

use crate::abi::{self, Saved};
use ps1rt::Cpu;

/// `"unpacked"`.
const UNPACKED: u32 = 0x8001_27F8;

const SHAPE_POINTER: fn(&mut Cpu) = nfs4rc::f_800f17ec;
const UNPACK_SIZE: fn(&mut Cpu) = nfs4rc::f_800e4c10;
const UNPACK: fn(&mut Cpu) = nfs4rc::f_800e4b28;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const GET_CLUT_ID: fn(&mut Cpu) = nfs4rc::f_800e0544;
/// The shape uploaded to VRAM at a place, with its CLUT.
const UPLOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f7304;
const SHAPE_DEPTH: fn(&mut Cpu) = nfs4rc::f_800f4d40;
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;

/// A 12-bit signed field at `shift` (from the top) of a shape's place word.
fn field12(v: u32, shift: u32) -> u32 {
    ((v << shift) as i32 >> 20) as u32
}

/// `FETexture_LoadPmxAtOffset` (FETEXTURE.CPP, 0x8004f5e8): shape `k` of the shape file `file`
/// into `info`, uploaded `dy` lines down and `dx` across from its place (its header put back
/// after when moved); the unpacked copy freed.
pub fn abi_load_pmx_at_offset(c: &mut Cpu) {
    let (file, k, info, dy) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let dx = abi::arg(c, 4);
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[6], r.0[3], r.0[5]) = (dx, info, dy);
        c.m.w32(sp + 48, 0);
        c.m.w32(sp + 52, 0);
        let shape = r.call_leaving(c, SHAPE_POINTER, &[file, k], 0x8004_f624);
        r.0[2] = shape;
        if shape == 0 {
            c.r[2] = 0;
            return;
        }
        let packed = c.m.r8(shape) as u32 & 0x80;
        r.0[4] = packed;
        r.0[0] = shape + 16;
        let s = if packed != 0 {
            let size = r.call_leaving(c, UNPACK_SIZE, &[shape + 16], 0x8004_f64c);
            let s = r.call_leaving(c, RESERVE, &[UNPACKED, size.wrapping_add(16), 0], 0x8004_f660);
            r.0[1] = s;
            r.call_leaving(c, BLOCKMOVE, &[shape, s, 16], 0x8004_f674);
            r.call_leaving(c, UNPACK, &[shape + 16, s + 16], 0x8004_f680);
            let m = &mut c.m;
            let v = m.r8(s) & 0x7F;
            m.w8(s, v);
            let next = (m.r32(shape) as i32 >> 8) as u32;
            let low = m.r8(s) as u32;
            let v = low | (shape.wrapping_add(next).wrapping_sub(s) << 8);
            m.w32(s, v);
            s
        } else {
            r.0[1] = shape;
            shape
        };
        let kind = c.m.r8(s) as u32 & 3;
        if kind != 2 {
            r.call_leaving(c, GET_CLUT_ID, &[kind, sp + 48, sp + 52], 0x8004_f6d8);
            let m = &mut c.m;
            let v = (m.r32(sp + 52) << 6) | ((m.r32(sp + 48) as i32 >> 4) as u32 & 0x3F);
            m.w32(info + 4, v);
        } else {
            c.r[5] = sp + 48;
        }
        let m = &mut c.m;
        for k in 0..5 {
            let v = m.r32(s + 4 * k);
            m.w32(sp + 24 + 4 * k, v);
        }
        let place = m.r32(s + 12);
        let (clut_x, clut_y) = (m.r32(sp + 48), m.r32(sp + 52));
        let x = field12(place, 20).wrapping_add(dx);
        let y = field12(place, 4).wrapping_add(dy);
        r.call_leaving(c, UPLOAD_SHAPE, &[s, x, y, clut_x, clut_y], 0x8004_f750);
        let m = &mut c.m;
        m.w32(info, shape);
        let b = m.r8(s);
        let v = m.r32(info + 12);
        m.w8(info + 9, b);
        let v = (v & 0xFF00_0000) | (m.r32(s) >> 8);
        m.w32(info + 12, v);
        for at in [4, 6, 8, 10] {
            let v = m.r16(s + at);
            m.w16(info + 12 + at, v);
        }
        let place = m.r32(s + 12);
        m.w16(info + 24, field12(place, 20) as u16);
        let place = m.r32(s + 12);
        m.w16(info + 26, field12(place, 4) as u16);
        let depth = r.call_leaving(c, SHAPE_DEPTH, &[s], 0x8004_f7d4);
        let m = &mut c.m;
        let clut = m.r32(info + 4);
        let blend = m.r8(info + 9) as u32;
        let ty = m.r16(info + 26) as u32;
        let tx = m.r16(info + 24) as u32;
        m.w8(info + 8, depth as u8);
        let page = ((blend & 3) << 7)
            | ((((ty & 0x100) << 16) as i32 >> 20) as u32)
            | ((tx & 0x3FF) >> 6)
            | ((ty & 0x200) << 2);
        m.w16(info + 28, page as u16);
        let v = r.call_leaving(c, GET_CLUT, &[(clut & 0x3F) << 4, (clut as i32 >> 6) as u32, page, (ty & 0x200) << 2], 0x8004_f82c);
        c.m.w16(info + 30, v as u16);
        if dx != 0 || dy != 0 {
            let m = &mut c.m;
            for k in 0..5 {
                let v = m.r32(sp + 24 + 4 * k);
                m.w32(s + 4 * k, v);
            }
        }
        if packed != 0 {
            r.call_leaving(c, PURGE, &[s], 0x8004_f878);
        }
    })
}

/// `FETexture_LoadPmx` (FETEXTURE.CPP, 0x8004f8a0): `FETexture_LoadPmxAtOffset` at its place.
pub fn abi_load_pmx(c: &mut Cpu) {
    let (file, k, info) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 32, |c, sp| {
        let r = Saved::of(c);
        c.m.w32(sp + 16, 0);
        r.enter_leaving(c, 0x8004_f5e8, 0x8004_f8b4, |c| {
            (c.r[4], c.r[5], c.r[6], c.r[7]) = (file, k, info, 0);
            abi_load_pmx_at_offset(c)
        });
    })
}

/// `FETexture_LoadPmx` entered as the original's callers `jal` it.
pub(crate) fn load_pmx_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_f8a0, None, &[]);
    abi_load_pmx(c);
}

/// `FETexture_LoadPmxAtOffset` entered as the original's callers `jal` it.
pub(crate) fn load_pmx_at_offset_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_f5e8, None, &[]);
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
