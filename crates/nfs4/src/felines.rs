//! FELINES.CPP: the front end's bright-ended lines: a bar filled by `128 - fade` 128ths along
//! it (from either end, vertically or horizontally), a glow (the colour halfway to white, up to
//! 8 wide) shading into its end, and glows at its edges; drawn solid or translucent.
//!
//! Written as FEMENU.CPP is (see there). The squares are the host's (called by address: it
//! captures their primitives).

use crate::call::call_addr;
use crate::fefades;
use ps1rt::Cpu;

/// `PSXDrawSquare`, `PSXDrawTransSquare`, `PSXDrawGouraudSquare`.
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
const DRAW_GOURAUD_SQUARE: u32 = 0x8004_F1C4;

/// The fill's directions (`mode`): 0 up from the bottom, 1 down from the top (vertical), 2
/// left from the right, 3 right from the left (horizontal).
const FROM_TOP: u32 = 1;
const FROM_RIGHT: u32 = 2;
const FROM_LEFT: u32 = 3;

/// A line: its colour, place, size, direction (see `FROM_*`), how far faded (128ths, 128 none of
/// it), its end glows' width, translucent or not.
#[derive(Clone, Copy)]
pub struct Line {
    pub color: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub mode: u32,
    pub fade: u32,
    pub glow: u32,
    pub trans: bool,
}

impl Line {
    /// The arguments as the original takes them (`PSXTransDrawBrightEndLine`'s nine, the last
    /// `trans`).
    pub fn from_args(a: &[u32]) -> Line {
        Line { color: a[0], x: a[1], y: a[2], w: a[3], h: a[4], mode: a[5], fade: a[6], glow: a[7], trans: a.get(8).is_some_and(|&t| t != 0) }
    }
}

/// `x / 128` rounded toward 0 (the original's shift), and the glow's width (`x >> 10` of the
/// same, at most 8).
fn part(x: i32) -> (u32, u32) {
    let v = if x < 0 { x.wrapping_add(127) } else { x };
    ((v >> 7) as u32, (v >> 10).min(8) as u32)
}

fn gouraud(c: &mut Cpu, x: u32, y: u32, w: u32, h: u32, colors: [u32; 4]) {
    let [a, b, cc, d] = colors;
    call_addr(c, DRAW_GOURAUD_SQUARE, &[x, y, w, h, a, b, cc, d]);
}

/// `PSXTransDrawBrightEndLine` (FELINES.CPP, 0x8004c058): the bar filled along its direction
/// but for its faded part, the colour halfway to white glowing into its moving end; its edges'
/// glows (`glow` wide: the right one when filled from the right, the left one from the left,
/// both unfaded); then the bar itself, solid or translucent.
pub fn draw(c: &mut Cpu, l: Line) {
    if l.fade == 128 {
        return;
    }
    let bright = fefades::fade(l.color, 0x00FF_FFFF, 64);
    let (color, (mut x, mut y), (mut w, mut h)) = (l.color, (l.x, l.y), (l.w, l.h));
    // the part left (the whole bar unfaded) and its glowing end
    if l.fade != 0 && l.mode <= FROM_TOP {
        let (len, edge) = part((l.h as i32).wrapping_mul(128i32.wrapping_sub(l.fade as i32)));
        h = len;
        if l.mode == FROM_TOP {
            let top = l.y.wrapping_add(len).wrapping_sub(edge);
            gouraud(c, x, top, w, edge, [color, color, bright, bright]);
        } else {
            y = l.y.wrapping_add(l.h).wrapping_sub(len);
            gouraud(c, x, y, w, edge, [bright, bright, color, color]);
        }
    } else if l.fade != 0 {
        let (len, edge) = part((l.w as i32).wrapping_mul(128i32.wrapping_sub(l.fade as i32)));
        w = len;
        if l.mode == FROM_RIGHT {
            x = l.x.wrapping_add(l.w).wrapping_sub(len);
            gouraud(c, x, y, edge, h, [bright, color, bright, color]);
        } else {
            let left = l.x.wrapping_add(len).wrapping_sub(edge);
            gouraud(c, left, y, edge, h, [color, bright, color, bright]);
        }
    }
    if l.glow != 0 {
        if l.mode == FROM_RIGHT || l.fade == 0 {
            gouraud(c, x.wrapping_add(w).wrapping_sub(l.glow), y, l.glow, h, [color, 0, color, 0]);
        }
        if l.mode == FROM_LEFT || l.fade == 0 {
            gouraud(c, x, y, l.glow, h, [0, color, 0, color]);
        }
    }
    if l.trans {
        call_addr(c, DRAW_TRANS_SQUARE, &[color, x, y, w, h, 1]);
    } else {
        call_addr(c, DRAW_SQUARE, &[color, x, y, w, h]);
    }
}

pub fn abi_draw(c: &mut Cpu) {
    let a: Vec<u32> = (0..9).map(|k| crate::abi::arg(c, k)).collect();
    draw(c, Line::from_args(&a));
}

/// `PSXDrawBrightEndLine` (FELINES.CPP, 0x8004c018): the line drawn solid.
pub fn abi_draw_bright_end_line(c: &mut Cpu) {
    let a: Vec<u32> = (0..8).map(|k| crate::abi::arg(c, k)).collect();
    draw(c, Line::from_args(&a));
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
