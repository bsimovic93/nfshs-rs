//! FETV.CPP: the front end's TVs (the static pictures on the menus' screens): a TV set from a
//! shape (its place off the shape's, its size, its texture page and CLUT, the static's two
//! random starting lines), turned off and on. `DrawTV` and `DrawTVLines` stay recompiled (the
//! host captures their primitives).
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::call;
use crate::types::{TVConfig, TextureShapeInfo};
use ps1rt::{Cpu, Mem};

/// `GetClut`; `rand` (the BIOS's, through the game's stub).
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;

/// A TV's states (`tTVState`): off, on, turning on, turning off.
pub const OFF: i32 = 0;
pub const TURNING_ON: i32 = 2;
pub const TURNING_OFF: i32 = 3;
/// Full brightness; the grey a TV is tinted.
const BRIGHT: u16 = 128;
const GREY: i32 = 0x0080_8080;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// `InitTV` (FETV.CPP, 0x80023168): `tv` from shape `k` of `shapes`: off, placed at the shape's
/// centre negated, its size; its texture's corner (the shape's x inside its 64-wide page,
/// scaled by the shape's depth) and size, the shape's texture and CLUT places, its colour mode,
/// CLUT and texture page; the static starting at two random lines (of its height times 48);
/// no transition, grey, bright.
pub fn init_tv(c: &mut Cpu, tv: TVConfig, shapes: TextureShapeInfo, k: u32) {
    let shape = TextureShapeInfo(sx(k).wrapping_mul(TextureShapeInfo::SIZE).wrapping_add(shapes.0));
    let m = &mut c.m;
    tv.set_state(m, OFF);
    tv.set_flags(m, 0);
    let v = shape.centerx(m).wrapping_neg();
    tv.set_x(m, v);
    let v = shape.centery(m).wrapping_neg();
    tv.set_y(m, v);
    let v = shape.width(m);
    tv.set_w(m, v);
    let v = shape.height(m);
    tv.set_h(m, v);
    let x = shape.shapex(m) as i32;
    let inside = x.wrapping_sub((shape.shapex(m) as u16 & 0xFFC0) as i16 as i32) << 4;
    let depth = shape.depth(m) as u8 as i32;
    assert!(depth != 0, "InitTV: a shape of depth 0 (the original's `break`)");
    tv.set_u(m, (inside / depth) as u8);
    let (h, w, v) = (tv.h(m) as u8, tv.w(m) as u8, shape.shapey(m) as u8);
    tv.set_vh(m, h);
    tv.set_uw(m, w);
    tv.set_v(m, v);
    let v = shape.shapex(m) as u16;
    tv.set_shapex(m, v);
    let v = shape.shapey(m) as u16;
    tv.set_shapey(m, v);
    let v = shape.clut_id(m) as u16;
    tv.set_clut_id(m, v);
    // (the colour mode is the byte after the depth)
    let mode = m.r8(shape.0 + 9) & 3;
    let clut = tv.clut_id(m) as u32;
    tv.set_shape_type(m, mode);
    let id = call(c, GET_CLUT, &[(clut & 0x3F) << 4, clut >> 6]);
    let m = &mut c.m;
    tv.set_clut(m, id as u16);
    let (bits, ty, tx) = (m.r8(shape.0 + 9) as u32, shape.shapey(m) as u16 as u32, shape.shapex(m) as u16 as u32);
    let page = ((bits & 3) << 7) | ((ty & 0x100) >> 4) | ((tx & 0x3C0) >> 6) | ((ty & 0x200) << 2);
    tv.set_tpage(m, page as u16);
    for set in [TVConfig::set_fx_wide, TVConfig::set_fx_thin] {
        let v = call(c, RAND, &[]) as i32;
        let lines = (tv.h(&mut c.m) as i32).wrapping_mul(48);
        assert!(lines != 0, "InitTV: a TV 0 high (the original's `break`)");
        set(tv, &mut c.m, v.wrapping_rem(lines) as i16);
    }
    let m = &mut c.m;
    tv.set_flip_axis(m, 0);
    tv.set_transition(m, 0);
    tv.set_tint(m, GREY);
    tv.set_dest_brightness(m, BRIGHT);
}

pub fn abi_init_tv(c: &mut Cpu) {
    init_tv(c, TVConfig(c.r[4]), TextureShapeInfo(c.r[5]), c.r[6]);
}

/// `TurnOffTV` (FETV.CPP, 0x80023378): turning off, toward dark.
pub fn turn_off_tv(m: &mut Mem, tv: TVConfig) {
    tv.set_state(m, TURNING_OFF);
    tv.set_dest_brightness(m, 0);
}

pub fn abi_turn_off_tv(c: &mut Cpu) {
    turn_off_tv(&mut c.m, TVConfig(c.r[4]));
}

/// `TurnOnTV` (FETV.CPP, 0x80023388): turning on, toward bright.
pub fn turn_on_tv(m: &mut Mem, tv: TVConfig) {
    tv.set_state(m, TURNING_ON);
    tv.set_dest_brightness(m, BRIGHT);
}

pub fn abi_turn_on_tv(c: &mut Cpu) {
    turn_on_tv(&mut c.m, TVConfig(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::SHAPES;
    use crate::test_util::{Rng, against_original_in, fe_ram};

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

}
