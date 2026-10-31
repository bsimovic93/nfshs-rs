//! SCREENUSERNAME.CPP: the name entry's screen (`tScreenUserName`): a grid of letters behind the
//! name menu (each row a text, `-` a gap, `!#&@$^` the space, delete and done keys with their
//! dividers), two subtractive boxes over and under it, lines drawn out along the grid as the
//! options menu fades in, and 32 pictures.
//!
//! `SubtractiveBox`, `PSXDrawSquare`: the host's (called by address).

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_WORD};
use crate::globals::FRONT_END;
use crate::{fefades, felines, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen`, then +100 the options menu that called it (+116 its fade), +104 the
// letters' fade (0..128), +112 the rows (9 bytes each).
const MENU: u32 = 100;
const MENU_FADE: u32 = 116;
const TEXT_FADE: u32 = 104;
const ROWS: u32 = 112;
const ROW_SIZE: u32 = 9;

/// The grid's rows and its first row's y.
const ROW_COUNT: u32 = 0x8005_2E40;
const TOP: u32 = 0x8005_2E42;
/// The permanent file's name; "zUser%d"; a letter's text (`" "`); the rows' texts (508 on); the
/// keys' texts (space 518, delete 519, done 520).
const FILE_NAME: u32 = 0x8005_2E48;
const FILE_FORMAT: u32 = 0x8001_2590;
const LETTER: u32 = 0x8001_2598;
const ROW_TEXTS: u32 = 508;
/// The main screen's number (`frontEnd + 0x340`).
const MAIN_NUMBER: u32 = FRONT_END + 0x340;

/// `SubtractiveBox`, `PSXDrawSquare`: the host's.
const SUBTRACTIVE_BOX: u32 = 0x8001_C25C;
const DRAW_SQUARE: u32 = 0x8004_F12C;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// A port with stack arguments: `args` 5 on written to the caller's outgoing slots first.
fn port_stack(c: &mut Cpu, r: &Saved, sp: u32, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    for (k, &v) in args.iter().enumerate().skip(4) {
        c.m.w32(sp + 4 * k as u32, v);
    }
    port(c, r, addr, ra, &args[..4], f)
}

/// `tScreenUserName::Initialize` (SCREENUSERNAME.CPP, 0x8004b468): the letters full; the grid
/// centred about y 130, its rows' texts; the screen's files read.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[1] = 0;
        let m = &mut c.m;
        m.w16(this + TEXT_FADE, 128);
        let n = lh(m, ROW_COUNT);
        m.w16(TOP, (130 - ((n * 15) >> 1)) as u16);
        // (a1 and a2 still the row globals' half address, which `sprintf` homes)
        c.r[5] = 0x8005_0000;
        c.r[6] = 0x8005_0000;
        if n > 0 {
            r.0[3] = 0x8005_0000;
            loop {
                let k = sx(r.0[1]);
                r.0[0] = k;
                let w = r.call_leaving(c, TEXT_WORD, &[k.wrapping_add(ROW_TEXTS)], 0x8004_b4cc);
                let row = this.wrapping_add(k.wrapping_mul(ROW_SIZE)) + ROWS;
                r.call_leaving(c, SPRINTF, &[row, w], 0x8004_b4e4);
                r.0[1] = r.0[1].wrapping_add(1);
                // (the loop's delay slot shifts s0 before its head sign-extends it)
                r.0[0] = r.0[1] << 16;
                if sx(r.0[1]) as i32 >= lh(&mut c.m, ROW_COUNT) {
                    break;
                }
            }
        }
        port(c, &r, 0x8002_60cc, 0x8004_b50c, &[this], fescreen::abi_initialize);
    })
}

/// `tScreenUserName::GetShapeInfo` (SCREENUSERNAME.CPP, 0x8004b528): 92 shapes of the main
/// screen's "zUser%d", none swapped.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = name;
        r.0[0] = FILE_NAME;
        let m = &mut c.m;
        m.w32(swap_name, 0);
        m.w16(swap, 0);
        m.w16(permanent, 92);
        let n = m.r8(MAIN_NUMBER) as u32;
        r.call_leaving(c, SPRINTF, &[FILE_NAME, FILE_FORMAT, n], 0x8004_b570);
        c.m.w32(name, FILE_NAME);
    })
}

/// A line's length: 0 to 64.
fn length(len: u32) -> u32 {
    (len as i16).clamp(0, 64) as u32
}

/// `tScreenUserName::DrawVerticalLine` (SCREENUSERNAME.CPP, 0x8004b588): a line down from x, y,
/// drawn out `len` sixty-fourths of its 162.
pub fn abi_draw_vertical_line(c: &mut Cpu) {
    let (x, y, len) = (c.r[5], c.r[6], length(c.r[7]));
    abi::frame(c, 40, |c, sp| {
        let r = Saved::of(c);
        port_stack(c, &r, sp, 0x8004_c018, 0x8004_b5f8, &[0x0078_5A5A, sx(x), sx(y), 2, 162, 1, sx(len) << 1, 0], felines::abi_draw_bright_end_line);
    })
}

/// `tScreenUserName::DrawHorizontalLine` (SCREENUSERNAME.CPP, 0x8004b608): a line right from
/// x, y, drawn out `len` sixty-fourths of its 192.
pub fn abi_draw_horizontal_line(c: &mut Cpu) {
    let (x, y, len) = (c.r[5], c.r[6], length(c.r[7]));
    abi::frame(c, 40, |c, sp| {
        let r = Saved::of(c);
        port_stack(c, &r, sp, 0x8004_c018, 0x8004_b67c, &[0x0078_5A5A, sx(x), sx(y), 192, 1, 2, sx(len) << 1, 12], felines::abi_draw_bright_end_line);
    })
}

/// A key's text (`id`) at `x` and its dividers: the 2 or 5 after the text 28 apart (a short
/// key: the one at `x`).
fn key(c: &mut Cpu, r: &mut Saved, id: u32, dx: u32, dividers: u32, ras: [u32; 3]) {
    let w = r.call_leaving(c, TEXT_WORD, &[id], ras[0]);
    let (x, y, color) = (r.0[1], ((r.0[4] as i32) >> 16) as u32, r.0[0]);
    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, sx(x.wrapping_add(dx)), y, color, 1, 2], ras[1]);
    if dividers == 0 {
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, sx(x).wrapping_add(22), r.0[3].wrapping_sub(2), 2, 14], ras[2]);
        return;
    }
    r.0[0] = 1;
    r.0[2] = sx(x);
    loop {
        let k = sx(r.0[0]);
        let at = r.0[2].wrapping_add(k.wrapping_mul(28)).wrapping_sub(6);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, at, r.0[3].wrapping_sub(2), 2, 14], ras[2]);
        r.0[0] = r.0[0].wrapping_add(1);
        if sx(r.0[0]) as i32 >= dividers as i32 + 1 {
            break;
        }
    }
}

/// `tScreenUserName::DrawBackground` (SCREENUSERNAME.CPP, 0x8004b68c): the options menu's fade
/// gives the pictures' brightness (from half of it), the lines' length (a quarter) and the
/// letters' fade; the boxes; the grid (each row's letters 28 apart, the keys); the boxes again
/// in grey; the lines along the columns and under the rows; the pictures.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        m.w32(sp + 112, this);
        let menu = m.r32(this + MENU);
        let v = m.r32(menu + MENU_FADE);
        let half = ((v as i16 as i32) >> 1) - 128;
        m.w16(sp + 40, half.clamp(0, 128) as u16);
        let quarter = (v as i16 as i32) >> 2;
        m.w16(sp + 48, quarter.clamp(0, 128) as u16);
        m.w16(this + TEXT_FADE, (v as i16 as i32).clamp(0, 128) as u16);
        r.0[0] = 0x0008_0808;
        let boxes = |c: &mut Cpu, r: &mut Saved, shade: u32, ras: [u32; 2]| {
            r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[240, 42, 194, 85, shade, shade, 0, 0], ras[0]);
            r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[240, 127, 194, 85, 0, 0, shade, shade], ras[1]);
        };
        boxes(c, &mut r, 0x0008_0808, [0x8004_b7a8, 0x8004_b7cc]);
        // the grid
        r.0[8] = 1;
        r.0[7] = 2;
        r.0[6] = 14;
        r.0[5] = c.m.r16(TOP) as u32;
        c.m.w16(sp + 56, 0);
        for k in 0..2 {
            let b = c.m.r8(LETTER + k);
            c.m.w8(sp + 32 + k, b);
        }
        loop {
            let row = lh(&mut c.m, sp + 56);
            if row >= lh(&mut c.m, ROW_COUNT) {
                break;
            }
            r.0[1] = 258;
            c.m.w16(sp + 64, 0);
            r.0[4] = r.0[5].wrapping_sub(1) << 16;
            r.0[3] = sx(r.0[5]);
            loop {
                let col = lh(&mut c.m, sp + 64) as u32;
                let at = this.wrapping_add(col.wrapping_add((row as u32).wrapping_mul(ROW_SIZE))) + ROWS;
                let ch = c.m.r8(at);
                if ch == 0 {
                    break;
                }
                c.m.w8(sp + 32, ch);
                let fade = lh(&mut c.m, this + TEXT_FADE) as u32;
                let color = port(c, &r, 0x8004_bd68, 0x8004_b880, &[0x00B5_4200, fade], fefades::abi_fade_out);
                r.0[0] = color;
                match ch {
                    b'!' => key(c, &mut r, 518, 22, 0, [0x8004_b9e8, 0x8004_ba0c, 0x8004_ba2c]),
                    b'#' => key(c, &mut r, 519, 22, 0, [0x8004_b9e8, 0x8004_ba0c, 0x8004_ba2c]),
                    b'&' => key(c, &mut r, 520, 22, 0, [0x8004_b9e8, 0x8004_ba0c, 0x8004_ba2c]),
                    b'@' => key(c, &mut r, 518, 36, 2, [0x8004_b8c4, 0x8004_b8e8, 0x8004_b924]),
                    b'$' => key(c, &mut r, 519, 36, 2, [0x8004_b958, 0x8004_b97c, 0x8004_b9b8]),
                    b'^' => key(c, &mut r, 520, 78, 5, [0x8004_ba3c, 0x8004_ba60, 0x8004_ba9c]),
                    b'-' => {}
                    _ => {
                        let (x, y) = (sx(r.0[1].wrapping_add(8)), ((r.0[4] as i32) >> 16) as u32);
                        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[sp + 32, x, y, color, 1, 2], 0x8004_bae4);
                    }
                }
                let v = c.m.r16(sp + 64).wrapping_add(1);
                r.0[1] = r.0[1].wrapping_add(28);
                c.m.w16(sp + 64, v);
            }
            let v = c.m.r16(sp + 56).wrapping_add(1);
            r.0[5] = r.0[5].wrapping_add(15);
            c.m.w16(sp + 56, v);
        }
        r.0[0] = 0x0050_5050;
        boxes(c, &mut r, 0x0050_5050, [0x8004_bb38, 0x8004_bb5c]);
        // the lines
        let length_now = |m: &mut Mem| m.r16(sp + 48) as u32;
        r.0[1] = 252;
        r.0[0] = 0;
        for k in 0..7u32 {
            let len = sx(length_now(&mut c.m).wrapping_sub(k << 1));
            port(c, &r, 0x8004_b588, 0x8004_bb90, &[this, sx(r.0[1]), 46, len], abi_draw_vertical_line);
            r.0[0] = k + 1;
            r.0[1] = r.0[1].wrapping_add(28);
        }
        r.0[0] = 0;
        r.0[5] = (c.m.r16(TOP) as u32).wrapping_sub(3);
        if lh(&mut c.m, ROW_COUNT) + 1 > 0 {
            r.0[1] = 0x8005_0000;
            loop {
                let y = sx(r.0[5]);
                r.0[5] = r.0[5].wrapping_add(15);
                let k = sx(r.0[0]);
                let n = lh(&mut c.m, ROW_COUNT) as u32;
                let len = sx(length_now(&mut c.m).wrapping_sub(n.wrapping_sub(k) << 1));
                port(c, &r, 0x8004_b608, 0x8004_bc10, &[this, 240, y, len], abi_draw_horizontal_line);
                r.0[0] = r.0[0].wrapping_add(1);
                if sx(r.0[0]) as i32 >= lh(&mut c.m, ROW_COUNT) + 1 {
                    break;
                }
            }
        }
        // the pictures
        let bright = lh(&mut c.m, sp + 40) as u32;
        r.0[1] = bright;
        for k in 0..32 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, 0, 0, 0, bright, 0, 0], 0x8004_bc68);
        }
    })
}

/// `tScreenUserName::~tScreenUserName` (SCREENUSERNAME.H, 0x8004bcb4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_bcc4, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP as STACK_TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The game's name entry screen; a fake options menu (its fade); the outputs of
    /// `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_0D14;
    const FAKE_MENU: u32 = 0x8014_7200;
    const OUT: u32 = 0x8014_7000;

    /// Letters a row may have: the grid's, the keys, a gap, a space and others.
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.!#&@$^-  az~";

    /// FEMENU's structures, then the screen: the calling menu's fade about each limit (the
    /// pictures', the lines' and the letters'), the letters' fade, 0 to 10 rows of up to eight
    /// letters (of every kind), the grid's top.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let s = SCREEN;
        m.w32(s + MENU, FAKE_MENU);
        let v = [0, 1, 127, 128, 129, 255, 256, 257, 383, 512, 513, 0xFFFF, 0x8000, rng.next() as u32][pick(rng, 14) as usize];
        m.w32(FAKE_MENU + MENU_FADE, v);
        m.w16(s + TEXT_FADE, pick(rng, 129) as u16);
        let n = [0, 1, 4, 6, 10, pick(rng, 11)][pick(rng, 6) as usize];
        m.w16(ROW_COUNT, n as u16);
        m.w16(TOP, [55, 85, 130, pick(rng, 200)][pick(rng, 4) as usize] as u16);
        for k in 0..10 {
            let len = pick(rng, 9);
            for j in 0..ROW_SIZE {
                let ch = if j < len { LETTERS[pick(rng, LETTERS.len() as u32) as usize] } else { 0 };
                m.w8(s + ROWS + ROW_SIZE * k + j, ch);
            }
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenUserName::Initialize", 0x8004_b468, nfs4rc::f_8004b468, |c, r| {
                c.m.w16(ROW_COUNT, pick(r, 8) as u16);
                vec![SCREEN]
            }),
            ("tScreenUserName::GetShapeInfo", 0x8004_b528, nfs4rc::f_8004b528, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("DrawVerticalLine", 0x8004_b588, nfs4rc::f_8004b588, |_, r| vec![SCREEN, pick(r, 400), pick(r, 240), [0, 1, 63, 64, 65, 0xFFFF, 0x1_0010, rng_any(r)][pick(r, 8) as usize]]),
            ("DrawHorizontalLine", 0x8004_b608, nfs4rc::f_8004b608, |_, r| vec![SCREEN, pick(r, 400), pick(r, 240), [0, 1, 63, 64, 65, 0xFFFF, 0x1_0010, rng_any(r)][pick(r, 8) as usize]]),
            ("tScreenUserName::DrawBackground", 0x8004_b68c, nfs4rc::f_8004b68c, |_, _| vec![SCREEN]),
            ("~tScreenUserName", 0x8004_bcb4, nfs4rc::f_8004bcb4, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    fn rng_any(r: &mut Rng) -> u32 {
        r.next() as u32
    }

    #[test]
    fn screenusername_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x05E ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x05F + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screenusername_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, STACK_TOP, 150, 0x06F + k as u64, shuffle, &args);
        }
    }
}
