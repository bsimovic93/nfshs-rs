//! FETV.CPP: the front end's TVs (the static pictures on the menus' screens): a TV set from a
//! shape (its place off the shape's, its size, its texture page and CLUT, the static's two
//! random starting lines), turned off and on. `DrawTV` and `DrawTVLines` stay recompiled (the
//! host captures their primitives).

use crate::abi::{self, Saved};
use ps1rt::{Cpu, Mem};

/// `GetClut`; `rand` (the BIOS's, through the game's stub).
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// `InitTV` (FETV.CPP, 0x80023168): `tv` from shape `k` of `shapes` (32 bytes each): off, its
/// place the shape's offsets negated, its size, its texture's corner (the shape's x's inside its
/// page, scaled by the shape's depth) and size, its page, CLUT and colour mode; the static
/// starting at two random lines; unfaded, grey.
pub fn init_tv(c: &mut Cpu, tv: u32, shapes: u32, k: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let shape = (sx(k) << 5).wrapping_add(shapes);
        r.0[..2].copy_from_slice(&[shape, tv]);
        let m = &mut c.m;
        m.w32(tv, 0);
        m.w16(tv + 8, 0);
        let v = m.r16(shape + 20).wrapping_neg();
        m.w16(tv + 20, v);
        let v = m.r16(shape + 22).wrapping_neg();
        m.w16(tv + 22, v);
        let v = m.r16(shape + 16);
        m.w16(tv + 24, v);
        let v = m.r16(shape + 18);
        m.w16(tv + 26, v);
        let x = m.r16(shape + 24) as i16 as i32;
        let inside = x.wrapping_sub(((m.r16(shape + 24) & 0xFFC0) as i16) as i32) << 4;
        let depth = m.r8(shape + 8) as i32;
        assert!(depth != 0, "InitTV: a shape of depth 0 (the original's `break`)");
        m.w8(tv + 28, (inside / depth) as u8);
        let (h, w, y) = (m.r8(tv + 26), m.r8(tv + 24), m.r8(shape + 26));
        m.w8(tv + 31, h);
        m.w8(tv + 30, w);
        m.w8(tv + 29, y);
        let v = m.r16(shape + 24);
        m.w16(tv + 38, v);
        let v = m.r16(shape + 26);
        m.w16(tv + 40, v);
        let v = m.r16(shape + 4);
        m.w16(tv + 42, v);
        let mode = m.r8(shape + 9) & 3;
        let clut = m.r16(tv + 42) as u32;
        m.w8(tv + 32, mode);
        let id = r.call_leaving(c, GET_CLUT, &[(clut & 0x3F) << 4, clut >> 6], 0x8002_3274);
        let m = &mut c.m;
        m.w16(tv + 34, id as u16);
        let (bits, ty, tx) = (m.r8(shape + 9) as u32, m.r16(shape + 26) as u32, m.r16(shape + 24) as u32);
        let page = ((bits & 3) << 7) | ((ty & 0x100) >> 4) | ((tx & 0x3C0) >> 6) | ((ty & 0x200) << 2);
        m.w16(tv + 36, page as u16);
        for (at, ra) in [(10, 0x8002_32bc), (12, 0x8002_3304)] {
            let v = r.call_leaving(c, RAND, &[], ra) as i32;
            let lines = (c.m.r16(tv + 26) as i16 as i32).wrapping_mul(48);
            assert!(lines != 0, "InitTV: a TV 0 high (the original's `break`)");
            c.m.w16(tv + at, v.wrapping_rem(lines) as u16);
        }
        let m: &mut Mem = &mut c.m;
        m.w16(tv + 44, 0);
        m.w16(tv + 4, 0);
        m.w32(tv + 16, 0x0080_8080);
        m.w16(tv + 6, 128);
    })
}

pub fn abi_init_tv(c: &mut Cpu) {
    init_tv(c, c.r[4], c.r[5], c.r[6]);
}

/// `TurnOffTV` (FETV.CPP, 0x80023378): turning off.
pub fn abi_turn_off_tv(c: &mut Cpu) {
    c.m.w32(c.r[4], 3);
    c.m.w16(c.r[4] + 6, 0);
}

/// `TurnOnTV` (FETV.CPP, 0x80023388): turning on.
pub fn abi_turn_on_tv(c: &mut Cpu) {
    c.m.w32(c.r[4], 2);
    c.m.w16(c.r[4] + 6, 128);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::globals::SHAPES;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// A TV, a table of shapes made from the help's.
    const TV: u32 = 0x8014_7000;
    const TABLE: u32 = 0x8014_7100;

    /// FEMENU's structures, then a shapes table: the help's first 8 shapes, their places,
    /// sizes, depths and texture bits shuffled (none of depth 0 nor 0 high: the original's
    /// division would `break`).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let help = m.r32(SHAPES);
        for k in 0..8 * 32 {
            let b = m.r8(help + k);
            m.w8(TABLE + k, b);
        }
        for k in 0..8 {
            let s = TABLE + 32 * k;
            m.w8(s + 8, [4, 8, 16, 1 + pick(rng, 255)][pick(rng, 4) as usize] as u8);
            m.w8(s + 9, pick(rng, 256) as u8);
            for f in [16, 20, 22, 24, 26] {
                m.w16(s + f, pick(rng, 0x1_0000) as u16);
            }
            m.w16(s + 18, (1 + pick(rng, 200)) as u16);
            m.w16(s + 4, pick(rng, 0x1_0000) as u16);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("InitTV", 0x8002_3168, nfs4rc::f_80023168, |_, r| vec![TV, TABLE, pick(r, 8)]),
            ("TurnOffTV", 0x8002_3378, nfs4rc::f_80023378, |_, _| vec![TV]),
            ("TurnOnTV", 0x8002_3388, nfs4rc::f_80023388, |_, _| vec![TV]),
        ]
    }

    #[test]
    fn fetv_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7E0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7E1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn fetv_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7F1 + k as u64, shuffle, &args);
        }
    }
}
