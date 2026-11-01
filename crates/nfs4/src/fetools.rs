//! FETOOLS.CPP: the front end's tools: strings made upper or lower case, a pad's button drawn
//! (the PlayStation pad's or the NeGcon's, the d-pad's directions as two arrows), the fonts
//! loaded and freed, an amount of money formatted with its thousands.

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, STRLEN, TEXT_WORD};
use crate::globals::{CURRENT_FONT, FONTS, PATHS};
use ps1rt::{Cpu, Mem};

/// The NeGcon (as `FeTools_DrawPSXButton`'s kind).
const NEGCON: u32 = 35;
/// The buttons' shapes (24: the help's shapes): the d-pad's up and down and its left and right
/// (two arrows each), and each button's on the PlayStation pad and the NeGcon, and how far up
/// it is drawn.
const BUTTON_SHAPES: u32 = 24;
const UP_DOWN: u32 = 80;
const LEFT_RIGHT: u32 = 160;
const PAD_BUTTONS: [(u32, u32, u32); 4] = [(0x1000, 15, 3), (0x4000, 12, 3), (0x8000, 14, 3), (8, 16, 1)];
const NEGCON_BUTTONS: [(u32, u32, u32); 4] = [(0x1000, 18, 3), (0x4000, 19, 4), (0x8000, 20, 2), (8, 21, 1)];

/// The fonts' files ("%stiny.pfn", "%ssmall.pfn", "%stitle.pfn" in the front end's path),
/// where in VRAM each goes; the font system's start and end (0x800cc608, 0x800cc5b8),
/// `Font_LoadFont`, `loadfileadrz`, `purgememadr`.
const FONT_FILES: [(u32, u32, u32, u32); 3] = [(0x8001_141C, 960, 385, 0x8002_6bf4), (0x8001_1428, 896, 256, 0x8002_6c30), (0x8001_1434, 960, 256, 0x8002_6c6c)];
const FONT_PATH: u32 = PATHS + 0x84;
const FONT_START: fn(&mut Cpu) = nfs4rc::f_800cc608;
const FONT_END: fn(&mut Cpu) = nfs4rc::f_800cc5b8;
const LOAD_FONT: fn(&mut Cpu) = nfs4rc::f_800cc61c;
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e6394;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

/// The money's formats (up to a thousand, a million, more).
const MONEY_TEXTS: [u32; 3] = [132, 133, 134];

/// A string's letters from `from` (`'a'` or `'A'`) moved by `by`, in place.
fn change_case(c: &mut Cpu, s: u32, from: u8, by: u8, ra: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = s;
        let n = r.call_leaving(c, STRLEN, &[s], ra) as i32;
        let m: &mut Mem = &mut c.m;
        for k in 0..n.max(0) as u32 {
            let b = m.r8(s + k);
            if b.wrapping_sub(from) < 26 {
                m.w8(s + k, b.wrapping_add(by));
            }
        }
    })
}

/// `s_upper` (FETOOLS.CPP, 0x80026910): a string in upper case.
pub fn abi_s_upper(c: &mut Cpu) {
    change_case(c, c.r[4], b'a', 224, 0x8002_6924);
}

/// `s_lower` (FETOOLS.CPP, 0x80026970): a string in lower case.
pub fn abi_s_lower(c: &mut Cpu) {
    change_case(c, c.r[4], b'A', 32, 0x8002_6984);
}

/// `FeTools_DrawPSXButton` (FETOOLS.CPP, 0x800269d0): `button` drawn at `x`, `y` for a pad of
/// `kind`: the d-pad's directions as two arrows, the buttons as the pad's or the NeGcon's
/// (none known: shape 0).
pub fn draw_psx_button(c: &mut Cpu, kind: u32, button: u32, x: u32, y: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = x;
        r.0[1] = y.wrapping_add(1);
        c.m.w32(sp + 36, 0xBEBE);
        let draw = sp + 32;
        let shape = |c: &mut Cpu, r: &Saved, k: u32, x: u32, y: u32, ra: u32| {
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, BUTTON_SHAPES, x, y, 0, 0, draw], ra);
        };
        match button & 0xFFFF {
            UP_DOWN => {
                r.0[0] = draw;
                let y = r.0[1];
                shape(c, &r, 26, x.wrapping_add(3), y.wrapping_sub(2), 0x8002_6a3c);
                shape(c, &r, 27, x.wrapping_add(4), y.wrapping_add(4), 0x8002_6bb0);
            }
            LEFT_RIGHT => {
                r.0[1] = r.0[1].wrapping_sub(1);
                r.0[0] = draw;
                let y = r.0[1];
                shape(c, &r, 28, x, y, 0x8002_6a84);
                shape(c, &r, 29, x.wrapping_add(10), y, 0x8002_6bb0);
            }
            b => {
                let table = if kind & 0xFF == NEGCON { NEGCON_BUTTONS } else { PAD_BUTTONS };
                let mut k = 0;
                if let Some(&(_, s, up)) = table.iter().find(|e| e.0 == b) {
                    k = s;
                    r.0[1] = r.0[1].wrapping_sub(up);
                    if s == 16 {
                        r.0[2] = r.0[2].wrapping_add(2);
                    }
                }
                let (x, y) = (r.0[2], r.0[1]);
                shape(c, &r, k, x, y, 0x8002_6bb0);
            }
        }
    })
}

pub fn abi_draw_psx_button(c: &mut Cpu) {
    draw_psx_button(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FeTools_init` (FETOOLS.CPP, 0x80026bc8): the font system started, the three fonts loaded
/// (each file kept, the font put in VRAM), font 3 the current one.
pub fn abi_init(c: &mut Cpu) {
    abi::frame(c, 280, |c, sp| {
        let mut r = Saved::of(c);
        r.call_leaving(c, FONT_START, &[], 0x8002_6bd8);
        r.0[0] = PATHS;
        let name = sp + 16;
        for (k, (format, vx, vy, ra)) in FONT_FILES.into_iter().enumerate() {
            let path = c.m.r32(FONT_PATH);
            r.call_leaving(c, SPRINTF, &[name, format, path], ra);
            let file = r.call_leaving(c, LOAD_FILE, &[name, 0], ra + 12);
            c.m.w32(FONTS + 4 * k as u32, file);
            r.call_leaving(c, LOAD_FONT, &[file, vx, vy, 0], ra + 40);
        }
        c.m.w16(CURRENT_FONT, 3);
    })
}

/// `FeTools_deinit` (FETOOLS.CPP, 0x80026cb0): the fonts' files freed, the font system ended.
pub fn abi_deinit(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        for (k, ra) in [(1, 0x8002_6cc8), (2, 0x8002_6cd8), (0, 0x8002_6ce8)] {
            let file = c.m.r32(FONTS + 4 * k);
            r.call_leaving(c, PURGE, &[file], ra);
        }
        r.call_leaving(c, FONT_END, &[], 0x8002_6cf0);
    })
}

/// `FeTools_FormatMoney` (FETOOLS.CPP, 0x80026d00): `amount` into `out`, its sign and its
/// thousands (and millions) apart.
pub fn format_money(c: &mut Cpu, out: u32, amount: u32) {
    abi::frame(c, 40, |c, sp| {
        // (the return address saved after the first branch, where the prologue's replay
        // doesn't look)
        let ra = c.r[31];
        c.m.w32(sp + 36, ra);
        let mut r = Saved::of(c);
        let v = (amount as i32).wrapping_abs();
        let sign = if (amount as i32) < 0 { b'-' } else { b' ' } as u32;
        r.0[..3].copy_from_slice(&[v as u32, sign, out]);
        if v < 1000 {
            let format = r.call_leaving(c, TEXT_WORD, &[MONEY_TEXTS[0]], 0x8002_6d44);
            r.call_leaving(c, SPRINTF, &[out, format, sign, v as u32], 0x8002_6d58);
        } else if v <= 999_999 {
            let format = r.call_leaving(c, TEXT_WORD, &[MONEY_TEXTS[1]], 0x8002_6d78);
            r.call_leaving(c, SPRINTF, &[out, format, sign, (v / 1000) as u32, (v % 1000) as u32], 0x8002_6e5c);
        } else {
            let format = r.call_leaving(c, TEXT_WORD, &[MONEY_TEXTS[2]], 0x8002_6dc4);
            let args = [out, format, sign, (v / 1_000_000) as u32, (v % 1_000_000 / 1000) as u32, (v % 1000) as u32];
            r.call_leaving(c, SPRINTF, &args, 0x8002_6e5c);
        }
    })
}

pub fn abi_format_money(c: &mut Cpu) {
    format_money(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A string (letters of both cases, digits, accented letters), the money's room.
    const TEXT: u32 = 0x8014_7000;
    const OUT: u32 = 0x8014_7100;

    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let n = pick(rng, 40);
        for k in 0..n {
            let b = [b'a' + pick(rng, 26) as u8, b'A' + pick(rng, 26) as u8, b'0', 0x60, 0x7B, 0x40, 0x5B, 0x80 + pick(rng, 128) as u8][pick(rng, 8) as usize];
            m.w8(TEXT + k, b);
        }
        m.w8(TEXT + n, 0);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("s_upper", 0x8002_6910, nfs4rc::f_80026910, |_, _| vec![TEXT]),
            ("s_lower", 0x8002_6970, nfs4rc::f_80026970, |_, _| vec![TEXT]),
            ("DrawPSXButton", 0x8002_69d0, nfs4rc::f_800269d0, |_, r| {
                let button = [UP_DOWN, LEFT_RIGHT, 0x1000, 0x4000, 0x8000, 8, 0x2000, 0x1_0050, pick(r, 0x1_0000)][pick(r, 9) as usize];
                vec![[0, NEGCON, 0x123, 65][pick(r, 4) as usize], button, pick(r, 300), pick(r, 240)]
            }),
            ("FeTools_deinit", 0x8002_6cb0, nfs4rc::f_80026cb0, |_, _| vec![]),
            ("FormatMoney", 0x8002_6d00, nfs4rc::f_80026d00, |_, r| {
                let v = [0, 999, 1000, 999_999, 1_000_000, pick(r, 1000), pick(r, 1_000_000), r.next() as u32 & 0x7FFF_FFFF, 0x8000_0000][pick(r, 9) as usize];
                vec![OUT, if pick(r, 3) == 0 { v.wrapping_neg() } else { v }]
            }),
        ]
    }

    #[test]
    fn fetools_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7001 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7002 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
