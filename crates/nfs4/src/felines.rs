//! FELINES.CPP: the front end's bright-ended lines: a bar filled by `128 - fade` 128ths along
//! it (from either end, vertically or horizontally), a glow (the colour halfway to white, up to
//! 8 wide) shading into its end, and glows at its edges; drawn solid or translucent.

use crate::abi::{self, Saved};
use ps1rt::Cpu;

/// `PSXDrawSquare`, `PSXDrawTransSquare`, `PSXDrawGouraudSquare` (the host captures their
/// primitives: called by address).
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
const DRAW_GOURAUD_SQUARE: u32 = 0x8004_F1C4;

/// `PSXDrawBrightEndLine` (FELINES.CPP, 0x8004c018): a solid bright-ended line.
pub fn abi_draw_bright_end_line(c: &mut Cpu) {
    let args: Vec<u32> = (0..8).map(|k| abi::arg(c, k)).collect();
    abi::frame(c, 48, |c, sp| {
        let mut a = args.clone();
        a.push(0);
        // (the stack's arguments where the callee reads them)
        for (k, &v) in a.iter().enumerate().skip(4) {
            c.m.w32(sp + 4 * k as u32, v);
        }
        Saved::of(c).enter_leaving(c, 0x8004_c058, 0x8004_c048, |c| draw(c, &a));
    })
}

/// `x / 128` rounded toward 0 (the original's shift), and the glow's width (`x >> 10` of the
/// same, at most 8).
fn part(x: i32) -> (u32, u32) {
    let v = if x < 0 { x.wrapping_add(127) } else { x };
    ((v >> 7) as u32, (v >> 10).min(8) as u32)
}

/// `PSXTransDrawBrightEndLine` (FELINES.CPP, 0x8004c058): `color` at `x`, `y`, `w` by `h`,
/// filled along `mode` (0 and 1 vertically from either end, 2 and 3 horizontally) but `fade`
/// 128ths (nothing at 128), its end glowing; `glow` wide glows at its ends; translucent when
/// `trans`.
pub fn draw(c: &mut Cpu, a: &[u32]) {
    let (color, x, y, w, h, mode, fade, glow, trans) = (a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8]);
    abi::frame(c, 72, |c, _| {
        let mut r = Saved::of(c);
        let s1 = r.0[1];
        r.0[..8].copy_from_slice(&[128, s1, x, color, y, w, h, fade]);
        r.0[8] = mode;
        if fade == 128 {
            return;
        }
        let bright = r.call_leaving(c, nfs4rc::f_8004bcd8, &[color, 0x00FF_FFFF, 64], 0x8004_c0b8);
        let gouraud = |c: &mut Cpu, r: &Saved, args: &[u32], ra: u32| {
            r.call_ptr_leaving(c, DRAW_GOURAUD_SQUARE, args, ra);
        };
        if fade == 0 {
            r.0[1] = w;
            r.0[0] = h;
        } else if mode < 2 {
            let (len, edge) = part((h as i32).wrapping_mul(128i32.wrapping_sub(fade as i32)));
            r.0[0] = len;
            r.0[1] = w;
            if mode == 1 {
                let top = y.wrapping_add(len).wrapping_sub(edge);
                gouraud(c, &r, &[x, top, w, edge, color, color, bright, bright], 0x8004_c1d4);
            } else {
                r.0[4] = y.wrapping_add(h).wrapping_sub(len);
                gouraud(c, &r, &[x, r.0[4], w, edge, bright, bright, color, color], 0x8004_c1a8);
            }
        } else {
            let (len, edge) = part((w as i32).wrapping_mul(128i32.wrapping_sub(fade as i32)));
            r.0[1] = len;
            r.0[0] = h;
            if mode == 2 {
                r.0[2] = x.wrapping_add(w).wrapping_sub(len);
                gouraud(c, &r, &[r.0[2], y, edge, h, bright, color, bright, color], 0x8004_c1a8);
            } else {
                let left = x.wrapping_add(len).wrapping_sub(edge);
                gouraud(c, &r, &[left, y, edge, h, color, bright, color, bright], 0x8004_c1d4);
            }
        }
        let (x, y, len, h) = (r.0[2], r.0[4], r.0[1], r.0[0]);
        if glow != 0 {
            if mode == 2 || fade == 0 {
                gouraud(c, &r, &[x.wrapping_add(len).wrapping_sub(glow), y, glow, h, color, 0, color, 0], 0x8004_c230);
            }
            if mode == 3 || fade == 0 {
                gouraud(c, &r, &[x, y, glow, h, 0, color, 0, color], 0x8004_c264);
            }
        }
        if trans == 0 {
            r.call_ptr_leaving(c, DRAW_SQUARE, &[color, x, y, len, h], 0x8004_c288);
        } else {
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[color, x, y, len, h, 1], 0x8004_c2ac);
        }
    })
}

pub fn abi_draw(c: &mut Cpu) {
    let a: Vec<u32> = (0..9).map(|k| abi::arg(c, k)).collect();
    draw(c, &a);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
    }

    fn color(r: &mut Rng) -> u32 {
        [0, 0x00FF_FFFF, 0x0080_4020, r.next() as u32 & 0x00FF_FFFF][pick(r, 4) as usize]
    }

    fn line(r: &mut Rng) -> Vec<u32> {
        let fade = [0, 128, 64, 1, 127, pick(r, 129), pick(r, 300).wrapping_sub(100)][pick(r, 7) as usize];
        vec![color(r), pick(r, 300), pick(r, 240), pick(r, 200), pick(r, 100), pick(r, 5), fade, [0, 0, 4, pick(r, 20)][pick(r, 4) as usize], pick(r, 2)]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("PSXDrawBrightEndLine", 0x8004_c018, nfs4rc::f_8004c018, |_, r| {
                let mut a = line(r);
                a.pop();
                a
            }),
            ("PSXTransDrawBrightEndLine", 0x8004_c058, nfs4rc::f_8004c058, |_, r| line(r)),
        ]
    }

    #[test]
    fn felines_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x11E5 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 500, 0x11E6 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
