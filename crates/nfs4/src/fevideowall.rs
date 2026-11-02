//! FEVIDEOWALL.CPP: a wall of TVs (`tVideoWall`, the car select's and the track select's): each
//! set from a shape of a table, moved by an offset; turned on one by one in its order (one more
//! every 8 ticks) while it is valid and available, off the same way; when it isn't available,
//! its TVs dimmed and a sign over them (a text and a turning icon) faded in.
//!
//! Written as FEMENU.CPP is (see there). `DrawTV` and `DrawTVLines` are the host's (called by
//! address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, TEXT_WORD};
use crate::globals::GAME_TICKS;
use crate::types::{DrawShapeExtended, TVConfig, TextureShapeInfo};
use crate::{fefades, fetextrender, fetv};
use ps1rt::{Cpu, Mem};

// A wall: +0 its TVs, +4 its first shape, +6 how many TVs, +8 the shapes' table, +12 the order
// they turn on in (`short[]`), +16 the tick it last turned, +20 turning on (1) or off (-1),
// +22 the TVs' static (none: 0), +24 their offset, +28 the sign's text (-1: none), +30
// available, +32 the sign's fade (0..64), +34 valid, +36 the text's place, +40 the icon's
// shapes' table, +44 its first shape, +46 its frames, +48 its place, +52 the images set.
const TVS: u32 = 0;
const FIRST: u32 = 4;
const COUNT: u32 = 6;
const SHAPES: u32 = 8;
const ORDER: u32 = 12;
const TURNED_AT: u32 = 16;
const TURNING: u32 = 20;
const STATIC: u32 = 22;
const OFFSET: u32 = 24;
const TEXT: u32 = 28;
const AVAILABLE: u32 = 30;
const FADE: u32 = 32;
const VALID: u32 = 34;
const TEXT_AT: u32 = 36;
const ICONS: u32 = 40;
const ICON: u32 = 44;
const FRAMES: u32 = 46;
const ICON_AT: u32 = 48;
const SET: u32 = 52;
/// Turning on, off.
const ON: u16 = 1;
const OFF: u16 = 0xFFFF;
/// One more TV every 8 ticks; the sign's fade's step and full (64).
const TICKS_PER_TV: u32 = 3;
const FADE_STEP: u16 = 4;
const FADE_FULL: i32 = 64;

/// A TV's flags: static, dimmed.
const TV_STATIC: u16 = 4;
const TV_DIMMED: u16 = 0x28;
/// A TV's state: on (FETV's).
const TV_ON: i32 = 1;

/// `DrawTVLines`, `DrawTV`.
const DRAW_TV_LINES: u32 = 0x8002_2158;
const DRAW_TV: u32 = 0x8002_24AC;
/// The sign's colour; its icon's flags (semi-transparent, coloured, centred, its own table).
const SIGN_COLOR: u32 = 0xBEBE;
const ICON_FLAGS: u32 = 0x611;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// TV `k` of the wall.
fn tv(m: &mut Mem, this: u32, k: u32) -> TVConfig {
    TVConfig(m.r32(this + TVS).wrapping_add(sx(k).wrapping_mul(TVConfig::SIZE)))
}

fn count(m: &mut Mem, this: u32) -> i32 {
    lh(m, this + COUNT)
}

fn showing(m: &mut Mem, this: u32) -> bool {
    lh(m, this + VALID) != 0 && lh(m, this + AVAILABLE) != 0
}

/// `tVideoWall::Initialize` (FEVIDEOWALL.CPP, 0x80036988): `count` TVs at `tvs` from shapes
/// `first` on of `shapes`, turning on in `order`, with static `static_`; valid and available, no
/// sign; all off, their images not set yet.
#[allow(clippy::too_many_arguments)]
pub fn initialize(m: &mut Mem, this: u32, tvs: u32, shapes: u32, first: u32, n: u32, order: u32, static_: u32) {
    m.w32(this + TVS, tvs);
    m.w16(this + FIRST, first as u16);
    m.w32(this + SHAPES, shapes);
    for f in [OFFSET + 2, OFFSET, TURNING] {
        m.w16(this + f, 0);
    }
    m.w16(this + TEXT, 0xFFFF);
    m.w16(this + COUNT, n as u16);
    let ticks = m.r32(GAME_TICKS);
    m.w16(this + AVAILABLE, 1);
    m.w16(this + FADE, 0);
    m.w16(this + VALID, 1);
    m.w32(this + ICONS, 0);
    m.w32(this + ORDER, order);
    m.w32(this + TURNED_AT, ticks);
    m.w16(this + STATIC, static_ as u16);
    let mut k = 0;
    while (k as i32) < count(m, this) {
        let t = tv(m, this, k);
        t.set_state(m, 0);
        k += 1;
    }
    m.w32(this + SET, 0);
}

pub fn abi_initialize(c: &mut Cpu) {
    let (n, order, static_) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    initialize(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], n, order, static_);
}

/// `tVideoWall::UpdateImages` (FEVIDEOWALL.CPP, 0x80036a38): each TV set from its shape, moved by
/// the offset, with the static; dimmed unless valid and available.
pub fn update_images(c: &mut Cpu, this: u32) {
    let mut k = 0;
    while (k as i32) < count(&mut c.m, this) {
        let m = &mut c.m;
        let (t, shapes, shape) = (tv(m, this, k), m.r32(this + SHAPES), sx((m.r16(this + FIRST) as u32).wrapping_add(k)));
        fetv::init_tv(c, t, TextureShapeInfo(shapes), shape);
        let m = &mut c.m;
        let t = tv(m, this, k);
        let v = (t.x(m) as u16).wrapping_add(m.r16(this + OFFSET));
        t.set_x(m, v as i16);
        let t = tv(m, this, k);
        let v = (t.y(m) as u16).wrapping_add(m.r16(this + OFFSET + 2));
        t.set_y(m, v as i16);
        if lh(m, this + STATIC) > 0 {
            let t = tv(m, this, k);
            t.update_flags(m, |f| f | TV_STATIC);
            let v = m.r16(this + STATIC);
            tv(m, this, k).set_flip_axis(m, v);
        }
        if !showing(m, this) {
            tv(m, this, k).update_flags(m, |f| f | TV_DIMMED);
        }
        k = k.wrapping_add(1);
    }
    c.m.w32(this + SET, 1);
}

pub fn abi_update_images(c: &mut Cpu) {
    update_images(c, c.r[4]);
}

/// `tVideoWall::SetAvailableText` (FEVIDEOWALL.CPP, 0x80036b84): the sign's text and place;
/// available, the sign gone.
pub fn set_available_text(m: &mut Mem, this: u32, text: u32, x: u32, y: u32) {
    m.w16(this + TEXT, text as u16);
    m.w16(this + TEXT_AT, x as u16);
    m.w16(this + TEXT_AT + 2, y as u16);
    m.w16(this + FADE, 0);
    m.w16(this + AVAILABLE, 1);
}

pub fn abi_set_available_text(c: &mut Cpu) {
    set_available_text(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tVideoWall::SetAvailableIcon` (FEVIDEOWALL.CPP, 0x80036ba0): the sign's icon: its first
/// shape, frames, place and shapes.
#[allow(clippy::too_many_arguments)]
pub fn set_available_icon(m: &mut Mem, this: u32, shape: u32, frames: u32, x: u32, y: u32, shapes: u32) {
    m.w16(this + ICON, shape as u16);
    m.w16(this + FRAMES, frames as u16);
    m.w16(this + ICON_AT, x as u16);
    m.w32(this + ICONS, shapes);
    m.w16(this + ICON_AT + 2, y as u16);
}

pub fn abi_set_available_icon(c: &mut Cpu) {
    let (y, shapes) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    set_available_icon(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], y, shapes);
}

/// `tVideoWall::SetOffset` (FEVIDEOWALL.CPP, 0x80036bc0).
pub fn set_offset(m: &mut Mem, this: u32, x: u32, y: u32) {
    m.w16(this + OFFSET, x as u16);
    m.w16(this + OFFSET + 2, y as u16);
}

pub fn abi_set_offset(c: &mut Cpu) {
    set_offset(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tVideoWall::SetAvailable` (FEVIDEOWALL.CPP, 0x80036bcc).
pub fn set_available(m: &mut Mem, this: u32, on: u32) {
    m.w16(this + AVAILABLE, on as u16);
}

pub fn abi_set_available(c: &mut Cpu) {
    set_available(&mut c.m, c.r[4], c.r[5]);
}

/// `tVideoWall::SetValid` (FEVIDEOWALL.CPP, 0x80036bd4).
pub fn set_valid(m: &mut Mem, this: u32, on: u32) {
    m.w16(this + VALID, on as u16);
}

pub fn abi_set_valid(c: &mut Cpu) {
    set_valid(&mut c.m, c.r[4], c.r[5]);
}

/// `tVideoWall::Update` (FEVIDEOWALL.CPP, 0x80036bdc): turning on, while valid, its TVs on in
/// order, one more every 8 ticks (the time held while it isn't valid; none while it isn't
/// available); turning off, its TVs off the same way, done once all are.
pub fn update(m: &mut Mem, this: u32) {
    let n = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + TURNED_AT)) >> TICKS_PER_TV;
    let on = lh(m, this + TURNING) > 0;
    if on && lh(m, this + VALID) == 0 {
        let ticks = m.r32(GAME_TICKS);
        m.w32(this + TURNED_AT, ticks);
        return;
    }
    let mut k = 0u32;
    while (sx(k) as i32) < sx(n) as i32 && (sx(k) as i32) < count(m, this) {
        let order = m.r32(this + ORDER).wrapping_add(sx(k) << 1);
        let at = lh(m, order) as u32;
        let t = tv(m, this, at);
        if on {
            if t.state(m) == 0 && showing(m, this) {
                fetv::turn_on_tv(m, t);
            }
        } else if t.state(m) == TV_ON {
            fetv::turn_off_tv(m, t);
        }
        k = k.wrapping_add(1);
    }
    if !on && sx(n) as i32 >= count(m, this) {
        m.w16(this + TURNING, 0);
    }
}

pub fn abi_update(c: &mut Cpu) {
    update(&mut c.m, c.r[4]);
}

/// `tVideoWall::Draw` (FEVIDEOWALL.CPP, 0x80036dac): the sign faded in while the wall is valid
/// and not available (else out); with the sign, the TVs' lines, the icon turning and the text;
/// the TVs, but while the wall is valid and available before its images are set.
pub fn draw(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let fading_in = lh(m, this + AVAILABLE) == 0 && lh(m, this + VALID) != 0;
    let v = m.r16(this + FADE).wrapping_add(if fading_in { FADE_STEP } else { FADE_STEP.wrapping_neg() });
    m.w16(this + FADE, v);
    let v = lh(m, this + FADE);
    if v > FADE_FULL {
        m.w16(this + FADE, FADE_FULL as u16);
    } else if v < 0 {
        m.w16(this + FADE, 0);
    }
    if !(showing(m, this) && lh(m, this + FADE) == 0) {
        let mut k = 0u32;
        while (sx(k) as i32) < count(&mut c.m, this) {
            let t = tv(&mut c.m, this, k);
            call_addr(c, DRAW_TV_LINES, &[t.0]);
            k = k.wrapping_add(1);
        }
        let fade = lh(&mut c.m, this + FADE);
        if fade > 0 {
            draw_sign(c, this, fefades::fade_out(SIGN_COLOR, (128 - fade) as u32));
        }
    }
    let m = &mut c.m;
    if m.r32(this + SET) != 0 || !showing(m, this) {
        let mut k = 0u32;
        while (sx(k) as i32) < count(&mut c.m, this) {
            let t = tv(&mut c.m, this, k);
            call_addr(c, DRAW_TV, &[t.0]);
            k = k.wrapping_add(1);
        }
    }
}

/// The sign: its icon turning (a frame every 16 ticks), its text (translucent) in `color`.
fn draw_sign(c: &mut Cpu, this: u32, color: u32) {
    let m = &mut c.m;
    if m.r32(this + ICONS) != 0 && lh(m, this + FRAMES) > 0 {
        with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
            let m = &mut c.m;
            let o = DrawShapeExtended(options);
            o.set_tint(m, 0, SIGN_COLOR as i32);
            let icons = m.r32(this + ICONS);
            o.set_custom_shapes(m, TextureShapeInfo(icons));
            let frame = ((m.r32(GAME_TICKS) as i32) >> 4) % lh(m, this + FRAMES);
            let shape = lh(m, this + ICON).wrapping_add(frame) as u32;
            let (x, y, fade) = (lh(m, this + ICON_AT) as u32, lh(m, this + ICON_AT + 2) as u32, (128 - lh(m, this + FADE)) as u32);
            call(c, DRAW_SHAPE_EXTENDED, &[shape, ICON_FLAGS, x, y, fade, 1, options]);
        });
    }
    if lh(&mut c.m, this + TEXT) >= 0 {
        fetextrender::set_abr(c, 1, 1);
        let text = lh(&mut c.m, this + TEXT) as u32;
        let w = call(c, TEXT_WORD, &[text]);
        let m = &mut c.m;
        let (x, y) = (lh(m, this + TEXT_AT) as u32, lh(m, this + TEXT_AT + 2) as u32);
        fetextrender::full_text_rgb(c, w, x, y, color, 3, 2);
        fetextrender::set_abr(c, 0, 0);
    }
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, c.r[4]);
}

/// Turning `way` from now (unless it is).
fn turn(m: &mut Mem, this: u32, way: u16) {
    if m.r16(this + TURNING) != way {
        let ticks = m.r32(GAME_TICKS);
        m.w16(this + TURNING, way);
        m.w32(this + TURNED_AT, ticks);
    }
}

/// `tVideoWall::TurnOff` (FEVIDEOWALL.CPP, 0x80037050): turning off from now (unless it is).
pub fn turn_off(m: &mut Mem, this: u32) {
    turn(m, this, OFF);
}

pub fn abi_turn_off(c: &mut Cpu) {
    turn_off(&mut c.m, c.r[4]);
}

/// `tVideoWall::TurnOffInstant` (FEVIDEOWALL.CPP, 0x80037074): turning off, every TV off now.
pub fn turn_off_instant(m: &mut Mem, this: u32) {
    turn_off(m, this);
    let mut k = 0u32;
    while (sx(k) as i32) < count(m, this) {
        let t = tv(m, this, k);
        fetv::turn_off_tv(m, t);
        k = k.wrapping_add(1);
    }
}

pub fn abi_turn_off_instant(c: &mut Cpu) {
    turn_off_instant(&mut c.m, c.r[4]);
}

/// `tVideoWall::TurnOn` (FEVIDEOWALL.CPP, 0x800370f0): turning on from now (unless it is).
pub fn turn_on(m: &mut Mem, this: u32) {
    turn(m, this, ON);
}

pub fn abi_turn_on(c: &mut Cpu) {
    turn_on(&mut c.m, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::SHAPES as HELP_SHAPES;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A wall, its TVs (up to 16), its order, a table of 24 shapes made from the help's.
    const WALL: u32 = 0x8014_5000;
    const TV_AT: u32 = 0x8014_5100;
    const ORDER_AT: u32 = 0x8014_5500;
    const TABLE: u32 = 0x8014_5600;

    /// FEMENU's structures, then the wall: its shapes (the help's, their places, sizes, depths
    /// and texture bits shuffled; none of depth 0 nor 0 high: `InitTV`'s division would
    /// `break`), 0 to 16 TVs on or off, their order any of theirs, turning either way or not
    /// about each time a TV turns, the static, the offset, the sign (text, icon) any or none,
    /// available and valid or not, the sign's fade about its ends, the images set or not.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let help = m.r32(HELP_SHAPES);
        for k in 0..24 * 32 {
            let b = m.r8(help + k % (8 * 32));
            m.w8(TABLE + k, b);
        }
        for k in 0..24 {
            let s = TABLE + 32 * k;
            m.w8(s + 8, [4, 8, 16, 1 + pick(rng, 255)][pick(rng, 4) as usize] as u8);
            m.w8(s + 9, pick(rng, 256) as u8);
            for f in [16, 20, 22, 24, 26] {
                m.w16(s + f, pick(rng, 0x1_0000) as u16);
            }
            m.w16(s + 18, (1 + pick(rng, 200)) as u16);
        }
        let n = [0, 1, 6, 12, 16][pick(rng, 5) as usize];
        let mut order: Vec<u32> = (0..n).collect();
        for k in (1..order.len()).rev() {
            order.swap(k, pick(rng, k as u32 + 1) as usize);
        }
        for (k, &v) in order.iter().enumerate() {
            m.w16(ORDER_AT + 2 * k as u32, v as u16);
        }
        for k in 0..16 {
            m.w32(TV_AT + TVConfig::SIZE * k, [0, 1, 2, 3][pick(rng, 4) as usize]);
        }
        let ticks = m.r32(GAME_TICKS);
        let w = WALL;
        m.w32(w + TVS, TV_AT);
        m.w16(w + FIRST, pick(rng, 8) as u16);
        m.w16(w + COUNT, n as u16);
        m.w32(w + SHAPES, TABLE);
        m.w32(w + ORDER, ORDER_AT);
        m.w32(w + TURNED_AT, ticks.wrapping_sub([0, 7, 8, 40, 200, pick(rng, 300)][pick(rng, 6) as usize]));
        m.w16(w + TURNING, [0xFFFF, 0, 1][pick(rng, 3) as usize]);
        m.w16(w + STATIC, [0, 5, 0xFFFF][pick(rng, 3) as usize]);
        m.w16(w + OFFSET, pick(rng, 64) as u16);
        m.w16(w + OFFSET + 2, pick(rng, 64) as u16);
        m.w16(w + TEXT, [0xFFFF, pick(rng, 600) as u16][pick(rng, 2) as usize]);
        m.w16(w + AVAILABLE, pick(rng, 2) as u16);
        m.w16(w + FADE, [0, 4, 60, 61, 64, pick(rng, 65)][pick(rng, 6) as usize] as u16);
        m.w16(w + VALID, pick(rng, 2) as u16);
        m.w16(w + TEXT_AT, pick(rng, 400) as u16);
        m.w16(w + TEXT_AT + 2, pick(rng, 200) as u16);
        m.w32(w + ICONS, [0, help][pick(rng, 2) as usize]);
        m.w16(w + ICON, 0);
        m.w16(w + FRAMES, [0, 1, 4, 8][pick(rng, 4) as usize]);
        m.w16(w + ICON_AT, pick(rng, 400) as u16);
        m.w16(w + ICON_AT + 2, pick(rng, 200) as u16);
        m.w32(w + SET, pick(rng, 2));
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tVideoWall::Initialize", 0x8003_6988, nfs4rc::f_80036988, |_, r| vec![WALL, TV_AT, TABLE, pick(r, 8), [0, 1, 6, 16][pick(r, 4) as usize], ORDER_AT, [0, 5][pick(r, 2) as usize]]),
            ("tVideoWall::UpdateImages", 0x8003_6a38, nfs4rc::f_80036a38, |_, _| vec![WALL]),
            ("tVideoWall::SetAvailableText", 0x8003_6b84, nfs4rc::f_80036b84, |_, r| vec![WALL, pick(r, 600), pick(r, 400), pick(r, 200)]),
            ("tVideoWall::SetAvailableIcon", 0x8003_6ba0, nfs4rc::f_80036ba0, |_, r| vec![WALL, pick(r, 8), pick(r, 8), pick(r, 400), pick(r, 200), TABLE]),
            ("tVideoWall::SetOffset", 0x8003_6bc0, nfs4rc::f_80036bc0, |_, r| vec![WALL, pick(r, 64), pick(r, 64)]),
            ("tVideoWall::SetAvailable", 0x8003_6bcc, nfs4rc::f_80036bcc, |_, r| vec![WALL, pick(r, 2)]),
            ("tVideoWall::SetValid", 0x8003_6bd4, nfs4rc::f_80036bd4, |_, r| vec![WALL, pick(r, 2)]),
            ("tVideoWall::Update", 0x8003_6bdc, nfs4rc::f_80036bdc, |_, _| vec![WALL]),
            ("tVideoWall::Draw", 0x8003_6dac, nfs4rc::f_80036dac, |_, _| vec![WALL]),
            ("tVideoWall::TurnOff", 0x8003_7050, nfs4rc::f_80037050, |_, _| vec![WALL]),
            ("tVideoWall::TurnOffInstant", 0x8003_7074, nfs4rc::f_80037074, |_, _| vec![WALL]),
            ("tVideoWall::TurnOn", 0x8003_70f0, nfs4rc::f_800370f0, |_, _| vec![WALL]),
        ]
    }

    #[test]
    fn videowall_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7A11 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 400, 0x7A12 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
