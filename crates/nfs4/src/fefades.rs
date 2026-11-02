//! FEFADES.CPP: the front end's fades: a colour between two by 128ths (each channel, carried
//! into the next when out of range, as the original's arithmetic does), faded to black, and the
//! menu texts' colours between their states (unselected, selected, highlighted; on and off) by
//! their text type's styles.
//!
//! Written as FEMENU.CPP is (see there): plain functions of the colours.

use crate::globals::{TEXT_COLORS, TEXT_STYLES};
use ps1rt::{Cpu, Mem};

/// A text type's styles (6 bytes each): +3 unselected, +4 selected, +5 highlighted.
const STYLE_SIZE: u32 = 6;
const UNSELECTED: u32 = 3;
const SELECTED: u32 = 4;
const HIGHLIGHTED: u32 = 5;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// `CalcFadeVal(int, int, int)` (FEFADES.CPP, 0x8004bcd8): `a` to `b` by `t` 128ths.
pub fn fade(a: u32, b: u32, t: u32) -> u32 {
    let (t, u) = (t as i32, 128i32.wrapping_sub(t as i32));
    let channel = |k: u32| {
        let (ca, cb) = (((a as i32) >> k) & 0xFF, ((b as i32) >> k) & 0xFF);
        u.wrapping_mul(ca).wrapping_add(t.wrapping_mul(cb)) >> 7
    };
    ((channel(16) << 16) | (channel(8) << 8) | channel(0)) as u32
}

pub fn abi_fade(c: &mut Cpu) {
    c.r[2] = fade(c.r[4], c.r[5], c.r[6]);
}

/// `CalcFadeVal(int, int)` (FEFADES.CPP, 0x8004bd68): `color` to black by `t` 128ths.
pub fn fade_out(color: u32, t: u32) -> u32 {
    fade(color, 0, t)
}

pub fn abi_fade_out(c: &mut Cpu) {
    c.r[2] = fade_out(c.r[4], c.r[5]);
}

/// `CalcFadeVal(int, int, int, int)` (FEFADES.CPP, 0x8004bd8c): `a` to `b` by `t`, then to black
/// by `f`.
pub fn fade_faded(a: u32, b: u32, t: u32, f: u32) -> u32 {
    fade(fade(a, b, t), 0, f)
}

pub fn abi_fade_faded(c: &mut Cpu) {
    c.r[2] = fade_faded(c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// Text type `ty`'s colour in style `k`.
fn style(m: &mut Mem, ty: u32, k: u32) -> u32 {
    let s = m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(STYLE_SIZE)).wrapping_add(k)) as u32;
    m.r32(TEXT_COLORS + 4 * s)
}

/// `CalcTextFadeUnselToSel` (FEFADES.CPP, 0x8004bdc0): text type `ty`'s colour unselected to
/// selected by `t`, faded by `f` (both `short`s).
pub fn text_fade_unsel_to_sel(m: &mut Mem, ty: u32, t: u32, f: u32) -> u32 {
    fade_faded(style(m, ty, UNSELECTED), style(m, ty, SELECTED), sx(t), sx(f))
}

pub fn abi_text_fade_unsel_to_sel(c: &mut Cpu) {
    c.r[2] = text_fade_unsel_to_sel(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `CalcTextFadeSelToHi` (FEFADES.CPP, 0x8004be30): text type `ty`'s colour selected to
/// highlighted by `t`, faded by `f` (both `short`s).
pub fn text_fade_sel_to_hi(m: &mut Mem, ty: u32, t: u32, f: u32) -> u32 {
    fade_faded(style(m, ty, SELECTED), style(m, ty, HIGHLIGHTED), sx(t), sx(f))
}

pub fn abi_text_fade_sel_to_hi(c: &mut Cpu) {
    c.r[2] = text_fade_sel_to_hi(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `CalcOnOffFade` (FEFADES.CPP, 0x8004beb4): an on-off choice's two colours for text type `ty`
/// as the choice moves by `t`: the one chosen (written at `on`) from unselected-or-selected
/// toward selected-or-highlighted, the other (at `off`) the other way; both between by `s`,
/// faded by `f` (`t`, `s`, `f` `short`s).
pub fn on_off_fade(m: &mut Mem, ty: u32, t: u32, s: u32, f: u32, on: u32, off: u32) {
    let t = sx(t);
    let (selected, high, unselected) = (style(m, ty, SELECTED), style(m, ty, HIGHLIGHTED), style(m, ty, UNSELECTED));
    let on_high = fade(selected, high, t);
    let off_high = fade(high, selected, t);
    let on_low = fade(unselected, selected, t);
    let off_low = fade(selected, unselected, t);
    let (s, f) = (sx(s), sx(f));
    m.w32(on, fade_faded(on_low, on_high, s, f));
    m.w32(off, fade_faded(off_low, off_high, s, f));
}

pub fn abi_on_off_fade(c: &mut Cpu) {
    let (on, off) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    on_off_fade(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], on, off);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The on-off choice's two colours' room.
    const OUT: u32 = 0x8014_7000;

    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
    }

    fn color(r: &mut Rng) -> u32 {
        [0, 0x00FF_FFFF, 0x0080_4020, r.next() as u32, r.next() as u32 & 0x00FF_FFFF][pick(r, 5) as usize]
    }

    fn by(r: &mut Rng) -> u32 {
        [0, 64, 128, pick(r, 129), pick(r, 400).wrapping_sub(100), 0x1_0040][pick(r, 6) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("CalcFadeVal3", 0x8004_bcd8, nfs4rc::f_8004bcd8, |_, r| vec![color(r), color(r), by(r)]),
            ("CalcFadeVal2", 0x8004_bd68, nfs4rc::f_8004bd68, |_, r| vec![color(r), by(r)]),
            ("CalcFadeVal4", 0x8004_bd8c, nfs4rc::f_8004bd8c, |_, r| vec![color(r), color(r), by(r), by(r)]),
            ("CalcTextFadeUnselToSel", 0x8004_bdc0, nfs4rc::f_8004bdc0, |_, r| vec![pick(r, 14), by(r), by(r)]),
            ("CalcTextFadeSelToHi", 0x8004_be30, nfs4rc::f_8004be30, |_, r| vec![pick(r, 14), by(r), by(r)]),
            ("CalcOnOffFade", 0x8004_beb4, nfs4rc::f_8004beb4, |_, r| vec![pick(r, 14), by(r), by(r), by(r), OUT, OUT + 4]),
        ]
    }

    #[test]
    fn fefades_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xFADE ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 500, 0xFAD1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
