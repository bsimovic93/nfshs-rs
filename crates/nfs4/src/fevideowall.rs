//! FEVIDEOWALL.CPP: a wall of TVs (`tVideoWall`, the car select's and the track select's): each
//! set from a shape of a table, moved by an offset; turned on one by one in its order (one more
//! every 8 ticks) while it is valid and available, off the same way; when it isn't available,
//! its TVs dimmed and a sign over them (a text and a turning icon) faded in.

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, TEXT_WORD};
use crate::globals::GAME_TICKS;
use crate::{fefades, fetextrender, fetv};
use ps1rt::{Cpu, Mem};

// A wall: +0 its TVs (FETV's, 48 bytes each), +4 its first shape, +6 how many TVs, +8 the
// shapes' table, +12 the order they turn on in (`short[]`), +16 the tick it last turned, +20
// turning on (1) or off (-1), +22 the TVs' static (none: 0), +24 their offset, +28 the sign's text
// (-1: none), +30 available, +32 the sign's fade (0..64), +34 valid, +36 the text's place, +40
// the icon's shapes' table, +44 its first shape, +46 its frames, +48 its place, +52 the images
// set.
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

// A TV's state (on: 1), place, flags, static.
const TV_SIZE: u32 = 48;
const TV_PLACE: u32 = 20;
const TV_FLAGS: u32 = 8;
const TV_STATIC: u32 = 44;

/// `DrawTVLines`, `DrawTV`: the host's (called by address).
const DRAW_TV_LINES: u32 = 0x8002_2158;
const DRAW_TV: u32 = 0x8002_24AC;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// TV `k` of the wall.
fn tv(m: &mut Mem, this: u32, k: u32) -> u32 {
    m.r32(this + TVS).wrapping_add(sx(k).wrapping_mul(TV_SIZE))
}

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `tVideoWall::Initialize` (FEVIDEOWALL.CPP, 0x80036988): `count` TVs at `tvs` from shapes
/// `first` on of `shapes`, turning on in `order`, with static `static_`; valid and available, no
/// sign; all off, their images not set yet.
pub fn abi_initialize(c: &mut Cpu) {
    let (this, tvs, shapes, first) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (count, order, static_) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    let m = &mut c.m;
    m.w32(this + TVS, tvs);
    m.w16(this + FIRST, first as u16);
    m.w32(this + SHAPES, shapes);
    for f in [OFFSET + 2, OFFSET, TURNING] {
        m.w16(this + f, 0);
    }
    m.w16(this + TEXT, 0xFFFF);
    m.w16(this + COUNT, count as u16);
    let ticks = m.r32(GAME_TICKS);
    m.w16(this + AVAILABLE, 1);
    m.w16(this + FADE, 0);
    m.w16(this + VALID, 1);
    m.w32(this + ICONS, 0);
    m.w32(this + ORDER, order);
    m.w32(this + TURNED_AT, ticks);
    m.w16(this + STATIC, static_ as u16);
    let mut k = 0;
    while (k as i32) < lh(m, this + COUNT) {
        let t = tv(m, this, k);
        m.w32(t, 0);
        k += 1;
    }
    m.w32(this + SET, 0);
}

/// `tVideoWall::UpdateImages` (FEVIDEOWALL.CPP, 0x80036a38): each TV set from its shape, moved by
/// the offset, with the static; dimmed unless valid and available.
pub fn abi_update_images(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[2] = 0;
        let mut k = 0;
        while (k as i32) < lh(&mut c.m, this + COUNT) {
            r.0[2] = k;
            r.0[1] = TV_SIZE.wrapping_mul(sx(k));
            let m = &mut c.m;
            let (t, shapes, shape) = (tv(m, this, k), m.r32(this + SHAPES), sx((m.r16(this + FIRST) as u32).wrapping_add(k)));
            r.enter_leaving(c, 0x8002_3168, 0x8003_6a94, |c| fetv::init_tv(c, t, shapes, shape));
            let m = &mut c.m;
            for f in [0, 2] {
                let t = tv(m, this, k);
                let v = m.r16(t + TV_PLACE + f).wrapping_add(m.r16(this + OFFSET + f));
                m.w16(t + TV_PLACE + f, v);
            }
            if lh(m, this + STATIC) > 0 {
                let t = tv(m, this, k);
                let flags = m.r16(t + TV_FLAGS) | 4;
                m.w16(t + TV_FLAGS, flags);
                let v = m.r16(this + STATIC);
                let t = tv(m, this, k);
                m.w16(t + TV_STATIC, v);
            }
            if lh(m, this + VALID) == 0 || lh(m, this + AVAILABLE) == 0 {
                let t = tv(m, this, k);
                let flags = m.r16(t + TV_FLAGS) | 0x28;
                m.w16(t + TV_FLAGS, flags);
            }
            k = k.wrapping_add(1);
        }
        c.m.w32(this + SET, 1);
    })
}

/// `tVideoWall::SetAvailableText` (FEVIDEOWALL.CPP, 0x80036b84): the sign's text and place;
/// available, the sign gone.
pub fn abi_set_available_text(c: &mut Cpu) {
    let (this, text, x, y) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(this + TEXT, text as u16);
    m.w16(this + TEXT_AT, x as u16);
    m.w16(this + TEXT_AT + 2, y as u16);
    m.w16(this + FADE, 0);
    m.w16(this + AVAILABLE, 1);
}

/// `tVideoWall::SetAvailableIcon` (FEVIDEOWALL.CPP, 0x80036ba0): the sign's icon: its first
/// shape, frames, place and shapes.
pub fn abi_set_available_icon(c: &mut Cpu) {
    let (this, shape, frames, x) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (y, shapes) = (abi::arg(c, 4), abi::arg(c, 5));
    let m = &mut c.m;
    m.w16(this + ICON, shape as u16);
    m.w16(this + FRAMES, frames as u16);
    m.w16(this + ICON_AT, x as u16);
    m.w32(this + ICONS, shapes);
    m.w16(this + ICON_AT + 2, y as u16);
}

/// `tVideoWall::SetOffset` (FEVIDEOWALL.CPP, 0x80036bc0).
pub fn abi_set_offset(c: &mut Cpu) {
    let this = c.r[4];
    c.m.w16(this + OFFSET, c.r[5] as u16);
    c.m.w16(this + OFFSET + 2, c.r[6] as u16);
}

/// `tVideoWall::SetAvailable` (FEVIDEOWALL.CPP, 0x80036bcc).
pub fn abi_set_available(c: &mut Cpu) {
    c.m.w16(c.r[4] + AVAILABLE, c.r[5] as u16);
}

/// `tVideoWall::SetValid` (FEVIDEOWALL.CPP, 0x80036bd4).
pub fn abi_set_valid(c: &mut Cpu) {
    c.m.w16(c.r[4] + VALID, c.r[5] as u16);
}

/// The wall's turning (FEVIDEOWALL.CPP, 0x80036bdc): turning on, while valid, its TVs on in
/// order, one more every 8 ticks (the time held while it isn't valid; none while it isn't
/// available); turning off, its TVs off the same way, done once all are.
pub fn abi_update(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let n = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + TURNED_AT)) >> 3;
        r.0[3] = n;
        let on = lh(m, this + TURNING) > 0;
        if on && lh(m, this + VALID) == 0 {
            let ticks = m.r32(GAME_TICKS);
            m.w32(this + TURNED_AT, ticks);
            return;
        }
        let n = sx(n) as i32;
        r.0[0] = 0;
        if n > 0 {
            r.0[2] = n as u32;
            loop {
                let k = sx(r.0[0]);
                if k as i32 >= lh(&mut c.m, this + COUNT) {
                    break;
                }
                let m = &mut c.m;
                let order = m.r32(this + ORDER).wrapping_add(k << 1);
                let at = lh(m, order) as u32;
                let t = tv(m, this, at);
                if on {
                    if m.r32(t) == 0 && lh(m, this + VALID) != 0 && lh(m, this + AVAILABLE) != 0 {
                        port(c, &r, 0x8002_3388, 0x8003_6cb8, &[t], fetv::abi_turn_on_tv);
                    }
                } else if m.r32(t) == 1 {
                    port(c, &r, 0x8002_3378, 0x8003_6d58, &[t], fetv::abi_turn_off_tv);
                }
                r.0[0] = r.0[0].wrapping_add(1);
                if sx(r.0[0]) as i32 >= n {
                    break;
                }
            }
        }
        if !on && sx(r.0[3]) as i32 >= lh(&mut c.m, this + COUNT) {
            c.m.w16(this + TURNING, 0);
        }
    })
}

/// `tVideoWall::Draw` (FEVIDEOWALL.CPP, 0x80036dac): the sign faded in while the wall is valid
/// and not available (else out); with the sign, the TVs' lines, the icon turning and the text;
/// the TVs, but while the wall is valid and available before its images are set.
pub fn abi_draw(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let fading_in = lh(m, this + AVAILABLE) == 0 && lh(m, this + VALID) != 0;
        let v = m.r16(this + FADE).wrapping_add(if fading_in { 4 } else { 0xFFFC });
        m.w16(this + FADE, v);
        let v = lh(m, this + FADE);
        if v >= 65 {
            m.w16(this + FADE, 64);
        } else if v < 0 {
            m.w16(this + FADE, 0);
        }
        let shown = lh(m, this + VALID) != 0 && lh(m, this + AVAILABLE) != 0 && lh(m, this + FADE) == 0;
        if !shown {
            r.0[0] = 0;
            while (sx(r.0[0]) as i32) < lh(&mut c.m, this + COUNT) {
                let t = tv(&mut c.m, this, r.0[0]);
                r.call_ptr_leaving(c, DRAW_TV_LINES, &[t], 0x8003_6e88);
                r.0[0] = r.0[0].wrapping_add(1);
            }
            let fade = lh(&mut c.m, this + FADE);
            if fade > 0 {
                r.0[2] = 128;
                let color = port(c, &r, 0x8004_bd68, 0x8003_6ec4, &[0xBEBE, (128 - fade) as u32], fefades::abi_fade_out);
                r.0[0] = color;
                let m = &mut c.m;
                if m.r32(this + ICONS) != 0 && lh(m, this + FRAMES) > 0 {
                    m.w32(sp + 36, 0xBEBE);
                    let icons = m.r32(this + ICONS);
                    m.w32(sp + 52, icons);
                    let frame = ((m.r32(GAME_TICKS) as i32) >> 4) % lh(m, this + FRAMES);
                    let shape = lh(m, this + ICON).wrapping_add(frame) as u32;
                    let (x, y, fade) = (lh(m, this + ICON_AT) as u32, lh(m, this + ICON_AT + 2) as u32, (128 - lh(m, this + FADE)) as u32);
                    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 1553, x, y, fade, 1, sp + 32], 0x8003_6f64);
                }
                if lh(&mut c.m, this + TEXT) >= 0 {
                    r.enter_leaving(c, 0x8002_1b48, 0x8003_6f7c, |c| fetextrender::set_abr(c, 1, 1));
                    let text = lh(&mut c.m, this + TEXT) as u32;
                    let w = r.call_leaving(c, TEXT_WORD, &[text], 0x8003_6f88);
                    let m = &mut c.m;
                    let (x, y) = (lh(m, this + TEXT_AT) as u32, lh(m, this + TEXT_AT + 2) as u32);
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, x, y, color, 3, 2], 0x8003_6fac);
                    r.enter_leaving(c, 0x8002_1b48, 0x8003_6fb8, |c| fetextrender::set_abr(c, 0, 0));
                }
            }
        }
        let m = &mut c.m;
        if m.r32(this + SET) != 0 || lh(m, this + VALID) == 0 || lh(m, this + AVAILABLE) == 0 {
            r.0[0] = 0;
            while (sx(r.0[0]) as i32) < lh(&mut c.m, this + COUNT) {
                let t = tv(&mut c.m, this, r.0[0]);
                r.call_ptr_leaving(c, DRAW_TV, &[t], 0x8003_7018);
                r.0[0] = r.0[0].wrapping_add(1);
            }
        }
    })
}

/// `tVideoWall::TurnOff` (FEVIDEOWALL.CPP, 0x80037050): turning off from now (unless it is).
pub fn abi_turn_off(c: &mut Cpu) {
    turn(&mut c.m, c.r[4], 0xFFFF);
}

fn turn(m: &mut Mem, this: u32, way: u16) {
    if m.r16(this + TURNING) != way {
        let ticks = m.r32(GAME_TICKS);
        m.w16(this + TURNING, way);
        m.w32(this + TURNED_AT, ticks);
    }
}

/// `tVideoWall::TurnOffInstant` (FEVIDEOWALL.CPP, 0x80037074): turning off, every TV off now.
pub fn abi_turn_off_instant(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.enter_leaving(c, 0x8003_7050, 0x8003_708c, |c| turn(&mut c.m, this, 0xFFFF));
        r.0[0] = 0;
        while (sx(r.0[0]) as i32) < lh(&mut c.m, this + COUNT) {
            let t = tv(&mut c.m, this, r.0[0]);
            port(c, &r, 0x8002_3378, 0x8003_70bc, &[t], fetv::abi_turn_off_tv);
            r.0[0] = r.0[0].wrapping_add(1);
        }
    })
}

/// `tVideoWall::TurnOn` (FEVIDEOWALL.CPP, 0x800370f0): turning on from now (unless it is).
pub fn abi_turn_on(c: &mut Cpu) {
    turn(&mut c.m, c.r[4], 1);
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
            m.w32(TV_AT + TV_SIZE * k, [0, 1, 2, 3][pick(rng, 4) as usize]);
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
