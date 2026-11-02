//! SCREENUSERNAME.CPP: the name entry's screen (`tScreenUserName`). It draws a grid of letters
//! behind the name menu: each row a text, `-` a gap, and `!#&@$^` the space, delete and done
//! keys with their dividers. Two subtractive boxes go over and under it, lines are drawn out
//! along the grid as the options menu fades in, and 32 pictures are shown.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.
//! `SubtractiveBox` and `PSXDrawSquare` are the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_WORD};
use crate::globals::FRONT_END;
use crate::types::Screen;
use crate::{fefades, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +100 the options menu that called it (+116 its fade), +104 the
/// letters' fade (0..128), +112 the rows (9 bytes each).
const MENU: u32 = 100;
const MENU_FADE: u32 = 116;
const TEXT_FADE: u32 = 104;
const ROWS: u32 = 112;
const ROW_SIZE: u32 = 9;

/// The grid's rows and its first row's y; its rows 15 apart, its letters 28 apart from x 258.
const ROW_COUNT: u32 = 0x8005_2E40;
const TOP: u32 = 0x8005_2E42;
const ROW_HEIGHT: i32 = 15;
const COLUMN_WIDTH: u32 = 28;
const LEFT: u32 = 258;
/// The permanent file's name; "zUser%d" (92 shapes); a letter's text (`" "`); the rows' texts
/// (508 on); the keys' texts (space 518, delete 519, done 520).
const FILE_NAME: u32 = 0x8005_2E48;
const FILE_FORMAT: u32 = 0x8001_2590;
const SHAPES: u16 = 92;
const LETTER: u32 = 0x8001_2598;
const ROW_TEXTS: u32 = 508;
const SPACE: u32 = 518;
const DELETE: u32 = 519;
const DONE: u32 = 520;
/// The main screen's number (`frontEnd + 0x340`).
const MAIN_NUMBER: u32 = FRONT_END + 0x340;
/// The colours: the letters' (faded in), the boxes' shade (dark, then grey), the lines'.
const LETTER_COLOR: u32 = 0x00B5_4200;
const DARK: u32 = 0x0008_0808;
const GREY: u32 = 0x0050_5050;
const LINE_COLOR: u32 = 0x0078_5A5A;

/// `SubtractiveBox`, `PSXDrawSquare`: the host's; `DrawBrightEndLine` (FELINES's port).
const SUBTRACTIVE_BOX: u32 = 0x8001_C25C;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_BRIGHT_END_LINE: u32 = 0x8004_C018;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// `tScreenUserName::Initialize` (SCREENUSERNAME.CPP, 0x8004b468): the letters full; the grid
/// centred about y 130, its rows' texts; the screen's files read.
pub fn initialize(c: &mut Cpu, this: Screen) {
    let m = &mut c.m;
    m.w16(this.0 + TEXT_FADE, 128);
    let n = lh(m, ROW_COUNT);
    m.w16(TOP, (130 - ((n * ROW_HEIGHT) >> 1)) as u16);
    let mut k = 0i32;
    while k < lh(&mut c.m, ROW_COUNT) {
        let w = call(c, TEXT_WORD, &[(k as u32).wrapping_add(ROW_TEXTS)]);
        let row = this.0.wrapping_add((k as u32).wrapping_mul(ROW_SIZE)) + ROWS;
        call(c, SPRINTF, &[row, w]);
        k += 1;
    }
    fescreen::initialize(c, this);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenUserName::GetShapeInfo` (SCREENUSERNAME.CPP, 0x8004b528): 92 shapes of the main
/// screen's "zUser%d", none swapped.
pub fn get_shape_info(c: &mut Cpu, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    let m = &mut c.m;
    m.w32(swap_name, 0);
    m.w16(swap, 0);
    m.w16(permanent, SHAPES);
    let n = m.r8(MAIN_NUMBER) as u32;
    call(c, SPRINTF, &[FILE_NAME, FILE_FORMAT, n]);
    c.m.w32(name, FILE_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, c.r[5], c.r[6], c.r[7], swap_name);
}

/// A line's length: 0 to 64.
fn length(len: u32) -> u32 {
    (len as i16).clamp(0, 64) as u32
}

/// `tScreenUserName::DrawVerticalLine` (SCREENUSERNAME.CPP, 0x8004b588): a line down from x, y,
/// drawn out `len` sixty-fourths of its 162.
pub fn draw_vertical_line(c: &mut Cpu, x: u32, y: u32, len: u32) {
    let len = length(len);
    call_addr(c, DRAW_BRIGHT_END_LINE, &[LINE_COLOR, sx(x), sx(y), 2, 162, 1, sx(len) << 1, 0]);
}

pub fn abi_draw_vertical_line(c: &mut Cpu) {
    draw_vertical_line(c, c.r[5], c.r[6], c.r[7]);
}

/// `tScreenUserName::DrawHorizontalLine` (SCREENUSERNAME.CPP, 0x8004b608): a line right from
/// x, y, drawn out `len` sixty-fourths of its 192.
pub fn draw_horizontal_line(c: &mut Cpu, x: u32, y: u32, len: u32) {
    let len = length(len);
    call_addr(c, DRAW_BRIGHT_END_LINE, &[LINE_COLOR, sx(x), sx(y), 192, 1, 2, sx(len) << 1, 12]);
}

pub fn abi_draw_horizontal_line(c: &mut Cpu) {
    draw_horizontal_line(c, c.r[5], c.r[6], c.r[7]);
}

/// A key's text (`id`) `dx` right of the cell at `x`, and its dividers: `dividers` of them, 28
/// apart (none: one, 22 right of the cell).
fn key(c: &mut Cpu, id: u32, x: u32, row_y: u32, color: u32, dx: u32, dividers: u32) {
    let w = call(c, TEXT_WORD, &[id]);
    fetextrender::full_text_rgb(c, w, sx(x.wrapping_add(dx)), sx(row_y.wrapping_sub(1)), color, 1, 2);
    let y = sx(row_y).wrapping_sub(2);
    if dividers == 0 {
        call_addr(c, DRAW_SQUARE, &[0, sx(x).wrapping_add(22), y, 2, 14]);
        return;
    }
    for k in 1..=dividers {
        call_addr(c, DRAW_SQUARE, &[0, sx(x).wrapping_add(k * COLUMN_WIDTH).wrapping_sub(6), y, 2, 14]);
    }
}

/// The two subtractive boxes, over and under the grid, in `shade`.
fn boxes(c: &mut Cpu, shade: u32) {
    call_addr(c, SUBTRACTIVE_BOX, &[240, 42, 194, 85, shade, shade, 0, 0]);
    call_addr(c, SUBTRACTIVE_BOX, &[240, 127, 194, 85, 0, 0, shade, shade]);
}

/// `tScreenUserName::DrawBackground` (SCREENUSERNAME.CPP, 0x8004b68c). The options menu's fade
/// gives the pictures' brightness (from half of it), the lines' length (a quarter) and the
/// letters' fade. It draws the boxes; the grid (each row's letters 28 apart, and the keys); the
/// boxes again in grey; the lines along the columns and under the rows; the pictures.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let m = &mut c.m;
    let menu = m.r32(this.0 + MENU);
    let fade = m.r32(menu + MENU_FADE) as i16 as i32;
    let bright = ((fade >> 1) - 128).clamp(0, 128) as u32;
    let lines = (fade >> 2).clamp(0, 128) as u32;
    m.w16(this.0 + TEXT_FADE, fade.clamp(0, 128) as u16);
    boxes(c, DARK);
    // the grid
    with_buffer(c, 8, |c, letter| {
        for k in 0..2 {
            let b = c.m.r8(LETTER + k);
            c.m.w8(letter + k, b);
        }
        let mut row = 0i32;
        let mut row_y = c.m.r16(TOP) as u32;
        while row < lh(&mut c.m, ROW_COUNT) {
            let mut col = 0u32;
            let mut x = LEFT;
            loop {
                let at = this.0.wrapping_add(col.wrapping_add((row as u32).wrapping_mul(ROW_SIZE))) + ROWS;
                let ch = c.m.r8(at);
                if ch == 0 {
                    break;
                }
                c.m.w8(letter, ch);
                let text_fade = lh(&mut c.m, this.0 + TEXT_FADE) as u32;
                let color = fefades::fade(LETTER_COLOR, 0, text_fade);
                match ch {
                    b'!' => key(c, SPACE, x, row_y, color, 22, 0),
                    b'#' => key(c, DELETE, x, row_y, color, 22, 0),
                    b'&' => key(c, DONE, x, row_y, color, 22, 0),
                    b'@' => key(c, SPACE, x, row_y, color, 36, 2),
                    b'$' => key(c, DELETE, x, row_y, color, 36, 2),
                    b'^' => key(c, DONE, x, row_y, color, 78, 5),
                    b'-' => {}
                    _ => fetextrender::full_text_rgb(c, letter, sx(x.wrapping_add(8)), sx(row_y.wrapping_sub(1)), color, 1, 2),
                }
                col = (col + 1) as u16 as i16 as i32 as u32;
                x = x.wrapping_add(COLUMN_WIDTH);
            }
            row += 1;
            row_y = row_y.wrapping_add(ROW_HEIGHT as u32);
        }
    });
    boxes(c, GREY);
    // the lines: down the columns, then under each row (the bottom one first out)
    for k in 0..7u32 {
        draw_vertical_line(c, 252 + k * COLUMN_WIDTH, 46, lines.wrapping_sub(k << 1));
    }
    let top = (c.m.r16(TOP) as u32).wrapping_sub(3);
    let mut k = 0i32;
    while k < lh(&mut c.m, ROW_COUNT) + 1 {
        let n = lh(&mut c.m, ROW_COUNT);
        let y = top.wrapping_add((ROW_HEIGHT * k) as u32);
        draw_horizontal_line(c, 240, y, lines.wrapping_sub(((n - k) << 1) as u32));
        k += 1;
    }
    // the pictures
    for k in 0..32 {
        call(c, DRAW_SHAPE_EXTENDED, &[k, 0, 0, 0, bright, 0, 0]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenUserName::~tScreenUserName` (SCREENUSERNAME.H, 0x8004bcb4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

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

}
