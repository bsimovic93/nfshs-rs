//! FEFADES.CPP: the front end's fades: a colour between two by 128ths (each channel, carried
//! into the next when out of range, as the original's arithmetic does), faded to black, and the
//! menu texts' colours between their states (unselected, selected, highlighted; on and off) by
//! their text type's styles.

use crate::abi::{self, Saved};
use crate::globals::{TEXT_COLORS, TEXT_STYLES};
use ps1rt::{Cpu, Mem};

/// A text type's styles (6 bytes each): +3 unselected, +4 selected, +5 highlighted.
const STYLE_SIZE: u32 = 6;

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
pub fn abi_fade_out(c: &mut Cpu) {
    let (color, t) = (c.r[4], c.r[5]);
    c.r[2] = abi::frame(c, 24, |c, _| Saved::of(c).enter_leaving(c, 0x8004_bcd8, 0x8004_bd7c, |_| fade(color, 0, t)));
}

/// `CalcFadeVal(int, int, int, int)` (FEFADES.CPP, 0x8004bd8c): `a` to `b` by `t`, then to black
/// by `f`.
pub fn fade_faded(c: &mut Cpu, a: u32, b: u32, t: u32, f: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = f;
        let v = r.enter_leaving(c, 0x8004_bcd8, 0x8004_bda0, |_| fade(a, b, t));
        r.enter_leaving(c, 0x8004_bcd8, 0x8004_bdb0, |_| fade(v, 0, f))
    })
}

pub fn abi_fade_faded(c: &mut Cpu) {
    c.r[2] = fade_faded(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// Text type `ty`'s colour in style `k` (3..5).
fn style(m: &mut Mem, ty: u32, k: u32) -> u32 {
    let s = m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(STYLE_SIZE)).wrapping_add(k)) as u32;
    m.r32(TEXT_COLORS + 4 * s)
}

/// `CalcTextFadeUnselToSel` (FEFADES.CPP, 0x8004bdc0): text type `ty`'s colour unselected to
/// selected by `t`, faded by `f`.
pub fn abi_text_fade_unsel_to_sel(c: &mut Cpu) {
    let (ty, t, f) = (c.r[4], c.r[5], c.r[6]);
    c.r[2] = abi::frame(c, 24, |c, _| {
        let (a, b) = (style(&mut c.m, ty, 3), style(&mut c.m, ty, 4));
        Saved::of(c).enter_leaving(c, 0x8004_bd8c, 0x8004_be20, |c| fade_faded(c, a, b, sx(t), sx(f)))
    });
}

/// `CalcTextFadeSelToHi` (FEFADES.CPP, 0x8004be30): text type `ty`'s colour selected to
/// highlighted by `t`, faded by `f`.
pub fn abi_text_fade_sel_to_hi(c: &mut Cpu) {
    let (ty, t, f) = (c.r[4], c.r[5], c.r[6]);
    c.r[2] = abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = f;
        let (a, b) = (style(&mut c.m, ty, 4), style(&mut c.m, ty, 5));
        let v = r.enter_leaving(c, 0x8004_bcd8, 0x8004_be90, |_| fade(a, b, sx(t)));
        r.0[0] = f << 16;
        r.enter_leaving(c, 0x8004_bcd8, 0x8004_bea4, |_| fade(v, 0, sx(f)))
    });
}

/// `CalcOnOffFade` (FEFADES.CPP, 0x8004beb4): an on-off choice's two colours (the one chosen,
/// the other) for text type `ty` as the choice moves by `t` (selected to highlighted for the
/// one, the other way for the other; unselected between), selected by `s`, faded by `f`.
pub fn abi_on_off_fade(c: &mut Cpu) {
    let (ty, t, s, f) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (on, off) = (abi::arg(c, 4), abi::arg(c, 5));
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let t = sx(t);
        let m = &mut c.m;
        let (selected, high, unselected) = (style(m, ty, 4), style(m, ty, 5), style(m, ty, 3));
        let s6 = r.0[6];
        r.0[..8].copy_from_slice(&[selected, t, high, s, f, unselected, s6, on]);
        r.0[8] = off;
        let v1 = r.enter_leaving(c, 0x8004_bcd8, 0x8004_bf64, |_| fade(selected, high, t));
        r.0[2] = v1;
        let v2 = r.enter_leaving(c, 0x8004_bcd8, 0x8004_bf78, |_| fade(high, selected, t));
        r.0[6] = v2;
        let v3 = r.enter_leaving(c, 0x8004_bcd8, 0x8004_bf8c, |_| fade(unselected, selected, t));
        r.0[0] = v3;
        let v4 = r.enter_leaving(c, 0x8004_bcd8, 0x8004_bfa0, |_| fade(selected, unselected, t));
        let (s, f) = (sx(s), sx(f));
        r.0[3] = s;
        r.0[4] = f;
        r.0[0] = v4;
        let v5 = r.enter_leaving(c, 0x8004_bd8c, 0x8004_bfc8, |c| fade_faded(c, v3, v1, s, f));
        c.m.w32(on, v5);
        let v6 = r.enter_leaving(c, 0x8004_bd8c, 0x8004_bfe0, |c| fade_faded(c, v4, v2, s, f));
        c.m.w32(off, v6);
    })
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
