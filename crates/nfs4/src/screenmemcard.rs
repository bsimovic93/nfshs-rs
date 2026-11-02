//! SCREENMEMCARD.CPP: the memory card screen (`tScreenMemcard`): the card's saves shown as
//! their icons (loaded from the card one at a time, faded in and out) in a grid, the save of
//! this game's highlighted, the card's state and a message, the menu's enablings (load and
//! save on or off by the card).
//!
//! The screen: +92 the fade, +100 the save of this game's (its directory entry, -1 none), +104
//! the card (1 or 5), +108 the card's record (MEMCARD.C's), +112 the saves' names (32 each),
//! +592 an icon loaded, +607 its frames, +622 its blocks (15 each), +640 the icons' files
//! (MEMCARD.C's loadfile infos, 44 each), +1300 their CLUTs and +1330 their fades (halves),
//! +1360 all faded in, +1364 the card layer up, +1368 the message, +1408 its word (-1 none),
//! +1412 a word to show instead, +1416 the highlight's frame and +1418 its time, +1420 the
//! icon's place in the grid, +1424 the time a card went missing, +1428 the icons to release,
//! +1432 the screen's start, +1436 the player, +1440 the icons fading out.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, call_addr, with_buffer};
use crate::felines::{self, Line};
use crate::globals::{FE_APP, GAME_TICKS, TEXT_COLORS};
use crate::memcard;
use crate::types::{DrawShapeExtended, RECT, Screen};
use ps1rt::{Cpu, Mem};

const FADE: u32 = 92;
const SAVE: u32 = 100;
const CARD: u32 = 104;
const RECORD: u32 = 108;
const NAMES: u32 = 112;
const LOADED: u32 = 592;
const FRAMES: u32 = 607;
const BLOCKS: u32 = 622;
const FILES: u32 = 640;
const CLUTS: u32 = 1300;
const FADES: u32 = 1330;
const ALL_IN: u32 = 1360;
const UP: u32 = 1364;
const MESSAGE: u32 = 1368;
const WORD: u32 = 1408;
const WORD_OVER: u32 = 1412;
const FLASH: u32 = 1416;
const FLASH_TIME: u32 = 1418;
const PLACE: u32 = 1420;
const GONE_AT: u32 = 1424;
const RELEASE: u32 = 1428;
const START: u32 = 1432;
const PLAYER: u32 = 1436;
const FADING_OUT: u32 = 1440;
const VPTR: u32 = 96;
/// The saves (15 at most), a loadfile info's size, an icon's frames' room (3 of 192 bytes); an
/// icon's fade when gone; a `POLY_FT4`'s size; the screen's shapes.
const SAVES: u32 = 15;
const FILE_INFO_SIZE: u32 = 44;
const ICON_SIZE: u32 = 192;
const ICON_SET: u32 = 576;
const FADED_OUT: u16 = 128;
const POLY_FT4_SIZE: u32 = 40;
const SHAPE_COUNT: u16 = 60;
/// The words: the card's saves (also the screen's help line), checking the card, no card,
/// insert a card, no save, unformatted, the card full, the player's title (660 on).
const SAVES_WORD: u32 = 638;
const CHECKING_CARD: u32 = 637;
const NO_CARD: u32 = 636;
const INSERT_CARD: u32 = 640;
const NO_SAVE: u32 = 645;
const UNFORMATTED: u32 = 649;
const CARD_FULL: u32 = 704;
const TITLE: u32 = 660;
const CHECKING: u32 = SAVES_WORD;
/// The colours: the grid's lines, the highlight, the shading; the highlight's flags (coloured,
/// by its top-left corner).
const LINE_COLOR: u32 = 0x78_5A5A;
const HIGHLIGHT: i32 = 0xB5_5623;
const SHADE: u32 = 0x50_5050;
const COLOURED_TOP_LEFT: u32 = 0x410;
/// The menus' load and save items' flags (bit 0 off) and the save item's word; the
/// application's player (+557).
const LOAD_ITEM: u32 = 12372;
const SAVE_ITEM: u32 = 12416;
const SAVE_ITEM_WORD: u32 = 12420;
const APP_PLAYER: u32 = 557;

/// A card's states (MEMCARD.C's).
mod memcard_state {
    pub const NONE: u32 = u32::MAX;
    pub const UNFORMATTED: u32 = (-2i32) as u32;
    pub const FULL: u32 = (-3i32) as u32;
}

/// The grid's layout (Initialize's): its corner, a cell's size, the lines' and margins'
/// sizes, the message's box; the icons' buffer (15 saves of 3 frames of 192 bytes).
const GRID_X: u32 = 0x8005_2D84;
const GRID_Y: u32 = 0x8005_2D88;
const CELL_W: u32 = 0x8005_2D8C;
const CELL_H: u32 = 0x8005_2D90;
const MARGIN_B: u32 = 0x8005_2D94;
const MARGIN_T: u32 = 0x8005_2D98;
const LINE_T: u32 = 0x8005_2D9C;
const GRID_W: u32 = 0x8005_2DA0;
const GRID_H: u32 = 0x8005_2DA4;
const LINE_W: u32 = 0x8005_2DA8;
const LINE_H: u32 = 0x8005_2DAC;
const ICON_X: u32 = 0x8005_2DB0;
const ICON_Y: u32 = 0x8005_2DB4;
const TEXT_X: u32 = 0x8005_2DB8;
const TEXT_Y: u32 = 0x8005_2DBC;
const BOX_X: u32 = 0x8005_2DC0;
const BOX_Y: u32 = 0x8005_2DC4;
const ICONS: u32 = 0x8005_2DC8;
/// A save going on (FEMEMCARD's); the menus (+12372, +12416 the load and save items' flags,
/// +12420 the save item's word); the fades' table and the screen's fade kind.
const CARD_ACTIVE: u32 = 0x8005_1F04;
const ALL_MENUS: u32 = 0x8005_1EE0;
const FADE_KIND: u32 = 0x8005_1AC9;
/// `"zmemcrd"`, `"NFS4"`, `"records"`.
const SHAPE_FILE: u32 = 0x8001_2164;
const FILE_NAME: u32 = 0x8001_21C8;
const RECORDS_NAME: u32 = 0x8001_21D0;
const VTABLE: u32 = 0x8001_21D8;

const SHAPE_TO_CLUT: fn(&mut Cpu) = nfs4rc::f_800f75f0;
const RELEASE_CLUT: fn(&mut Cpu) = nfs4rc::f_800e05b0;
const GET_CLUT_ID: fn(&mut Cpu) = nfs4rc::f_800e0544;
const UPLOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f7304;
const BLOCKCLEAR: fn(&mut Cpu) = nfs4rc::f_800f24d4;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const TEXT_PIXELS: fn(&mut Cpu) = nfs4rc::f_800e4a2c;
const MUSIC_BUFFERED: fn(&mut Cpu) = nfs4rc::f_8007aa48;
const MUSIC_THRESHOLD: fn(&mut Cpu) = nfs4rc::f_8007a9c4;
const DRAW_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::draw_shape_extended_entry;
/// The host's: `systemtask`, `PSXDrawSquare`, `SubtractiveBox` (hooked).
const SYSTEM_TASK: u32 = 0x800E_77F0;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const SUBTRACTIVE_BOX: u32 = 0x8001_C25C;

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

fn h(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

/// `tScreenMemcard::GetShapeInfo` (SCREENMEMCARD.CPP, 0x80046708): 60 shapes of `zmemcrd`,
/// none swapped.
pub fn get_shape_info(m: &mut Mem, count: u32, swap: u32, file: u32, swap_file: u32) {
    m.w16(swap, 0);
    m.w32(swap_file, 0);
    m.w16(count, SHAPE_COUNT);
    m.w32(file, SHAPE_FILE);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_file);
}

/// `tScreenMemcard::DrawIcon` (SCREENMEMCARD.CPP, 0x8004672c): an icon's shape drawn as a
/// semi-transparent `POLY_FT4` (40 bytes) at `x`, `y`, `w` by `h`, darkened by `fade`.
#[allow(clippy::too_many_arguments)]
pub fn draw_icon(c: &mut Cpu, shape: u32, x: u32, y: u32, w: u32, hgt: u32, fade: u32) {
    let fade = fade & 0xFFFF;
    let m = &mut c.m;
    let place = m.r32(shape + 12);
    let mode = m.r8(shape) as u32;
    let width = m.r16(shape + 4) as i16 as i32 as u32;
    let height = m.r16(shape + 6) as i16 as i32 as u32;
    // its texture's place: y and x (12-bit fields), u by its 4-bit depth
    let ty = ((place << 4) as i32 >> 20) as u32;
    let tx = ((place << 20) as i32 >> 20) as u32;
    let u = (((tx & 0x3F) << 4) as i32 / 4) as u32;
    let v = ty.wrapping_sub((((ty as i32) / 256) as u32) << 8);
    let prim = crate::mmeffect::add_prim(m, POLY_FT4_SIZE);
    m.w8(prim.wrapping_add(7), 46);
    m.w8(prim.wrapping_add(3), 9);
    let grey = (-128i32 as u32).wrapping_sub(fade) as u8;
    for at in [4, 5, 6] {
        m.w8(prim.wrapping_add(at), grey);
    }
    let clut = call(c, SHAPE_TO_CLUT, &[shape]);
    let m = &mut c.m;
    let page = ((mode & 3) << 7) | (((ty & 0x100) as i32 >> 4) as u32) | (((tx & 0x3FF) as i32 >> 6) as u32) | ((ty & 0x200) << 2);
    m.w16(prim.wrapping_add(14), clut as u16);
    m.w16(prim.wrapping_add(22), page as u16);
    let u1 = u.wrapping_add(width);
    let v1 = v.wrapping_add(height);
    for (at, b) in [(12, u), (13, v), (20, u1), (21, v), (28, u), (29, v1), (36, u1), (37, v1)] {
        m.w8(prim.wrapping_add(at), b as u8);
    }
    let (x1, y1) = (x.wrapping_add(w), y.wrapping_add(hgt));
    for (at, hw) in [(8, x), (10, y), (18, y), (24, x), (16, x1), (32, x1), (26, y1), (34, y1)] {
        m.w16(prim.wrapping_add(at), hw as u16);
    }
}

pub fn abi_draw_icon(c: &mut Cpu) {
    let (w, hgt, fade) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    draw_icon(c, c.r[5], c.r[6], c.r[7], w, hgt, fade);
}

/// Whether the music's buffer is full enough for the CD to read something else.
fn music_buffered(c: &mut Cpu) -> bool {
    let buffered = call(c, MUSIC_BUFFERED, &[]);
    let threshold = call(c, MUSIC_THRESHOLD, &[]);
    (buffered as i32) >= threshold as i32
}

/// `tScreenMemcard::LoadIcon` (SCREENMEMCARD.CPP, 0x80046924): (with the music's buffer full
/// enough) save `k`'s header read from the card: its frames (at most 3) and blocks, its icon's
/// frames put in VRAM with a CLUT of their own, faded in from nothing; a card gone or a bad
/// read noted (the icons released).
pub fn load_icon(c: &mut Cpu, this: u32, k: u32) {
    if !music_buffered(c) {
        return;
    }
    c.m.w32(CARD_ACTIVE, 1);
    c.m.w8(this + k + FRAMES, 0);
    c.m.w8(this + k + BLOCKS, 0);
    let record = c.m.r32(this + RECORD);
    if (k as i32) >= c.m.r32(record + 8) as i32 {
        c.m.w32(CARD_ACTIVE, 0);
        return;
    }
    // the save's file: its name in the card's directory, its title, its icons' places
    let file = this.wrapping_add(k.wrapping_mul(FILE_INFO_SIZE)).wrapping_add(FILES);
    call(c, BLOCKCLEAR, &[file, FILE_INFO_SIZE]);
    let m = &mut c.m;
    m.w32(file + 4, this.wrapping_add(k << 5).wrapping_add(NAMES));
    let record = m.r32(this + RECORD);
    m.w32(file, record.wrapping_add(k.wrapping_mul(40)).wrapping_add(16));
    let icons = m.r32(ICONS).wrapping_add(k.wrapping_mul(ICON_SET));
    for j in 0..3 {
        m.w32(file + 24 + 4 * j, icons + ICON_SIZE * j);
    }
    // the card idle, then the header read
    loop {
        let card = c.m.r32(this + CARD);
        if memcard::handlecardevents(c, card) == 22 {
            break;
        }
        c.tick();
    }
    let card = c.m.r32(this + CARD);
    memcard::loadfile(c, card, file, true);
    let s3 = this.wrapping_add(k);
    let fp = this.wrapping_add(k << 1);
    loop {
        let card = c.m.r32(this + CARD);
        let at = memcard::getcard(&mut c.m, card);
        if c.m.r32(at) == u32::MAX {
            c.m.w32(this + RELEASE, 1);
        }
        let card = c.m.r32(this + CARD);
        match memcard::handlecardevents(c, card) {
            15 => {
                if c.m.r32(this + RELEASE) != 0 {
                    release_icons(c, this);
                    break;
                }
                let m = &mut c.m;
                let frames_at = m.r32(file + 36);
                let frames = (m.r8(frames_at) as u32).wrapping_sub(16);
                m.w8(s3 + FRAMES, frames as u8);
                let card = m.r32(this + CARD);
                let blocks = memcard::grab_blocks(c, card, k);
                let m = &mut c.m;
                m.w8(s3 + BLOCKS, blocks as u8);
                if m.r8(s3 + FRAMES) >= 4 {
                    m.w8(s3 + FRAMES, 3);
                }
                if m.r8(s3 + FRAMES) == 0 {
                    m.w8(s3 + FRAMES, 1);
                }
                let old = m.r16(fp + CLUTS);
                if old != 0 {
                    call(c, RELEASE_CLUT, &[old as i16 as i32 as u32]);
                }
                // a CLUT of its own, each frame put in VRAM (17 apart, by the save)
                with_buffer(c, 8, |c, clut| {
                    call(c, GET_CLUT_ID, &[0, clut, clut + 4]);
                    let m = &mut c.m;
                    let v = (h(m, clut + 4) << 6) | ((m.r32(clut) as i32 >> 4) as u32 & 0x3F);
                    m.w16(fp + CLUTS, v as u16);
                    if m.r8(s3 + FRAMES) != 0 {
                        let mut j = 0u32;
                        loop {
                            let shape = c.m.r32(ICONS).wrapping_add(k.wrapping_mul(ICON_SET)).wrapping_add(j * ICON_SIZE);
                            if c.m.r8(shape) & 0xF7 == 64 {
                                let (cx, cy) = (c.m.r32(clut), c.m.r32(clut + 4));
                                call(c, UPLOAD_SHAPE, &[shape, 900 + 17 * j, (k << 4).wrapping_add(k), cx, cy]);
                            }
                            j += 1;
                            if (j as i32) >= c.m.r8(s3 + FRAMES) as i32 {
                                break;
                            }
                        }
                    }
                });
                c.m.w16(fp + FADES, FADED_OUT);
                c.m.w8(s3 + LOADED, 1);
                break;
            }
            2 | 3 | 7 | 10 | 11 | 16 | 19 | 23 => {
                c.m.w32(this + RELEASE, 1);
                c.m.w8(s3 + LOADED, 0);
                break;
            }
            _ => c.tick(),
        }
    }
    c.m.w32(CARD_ACTIVE, 0);
}

pub fn abi_load_icon(c: &mut Cpu) {
    load_icon(c, c.r[4], c.r[5]);
}

/// The lines' clamp of their fade: 0..64 (a halfword's).
fn line_fade(v: u32) -> u32 {
    let f = v as u16 as i16 as i32;
    if f < 0 {
        0
    } else if f < 64 {
        v
    } else {
        64
    }
}

/// The screen's clamp of a fade: 0..128 (a halfword's).
fn fade_clamp(v: u32) -> u32 {
    let f = v as u16 as i16 as i32;
    if f <= 0 {
        0
    } else if f > 128 {
        128
    } else {
        v
    }
}

/// `x % 2` of a halfword (toward zero), sign-extended.
fn parity(x: u32) -> u32 {
    sx(((x as u16 as i16 as i32) % 2) as u32)
}

/// A grid line: solid, its fade doubled.
fn grid_line(c: &mut Cpu, x: u32, y: u32, w: u32, h: u32, mode: u32, fade: u32, glow: u32) {
    let fade = ((fade << 16) as i32 >> 15) as u32;
    felines::draw(c, Line { color: LINE_COLOR, x: sx(x), y: sx(y), w, h, mode, fade, glow, trans: false });
}

/// `tScreenMemcard::DrawVerticalLine` (SCREENMEMCARD.CPP, 0x80046c80): one of the grid's
/// vertical lines at `x`, `y`, faded by `fade` (0..64), filled from its top or bottom.
pub fn draw_vertical_line(c: &mut Cpu, x: u32, y: u32, fade: u32, up: u32) {
    let fade = line_fade(fade);
    let m = &mut c.m;
    let len = sx(h(m, LINE_T).wrapping_add(h(m, GRID_H).wrapping_add(h(m, LINE_H) << 1)));
    grid_line(c, x, y, 2, len, ((up & 0xFFFF) << 16 == 0) as u32, fade, 0);
}

pub fn abi_draw_vertical_line(c: &mut Cpu) {
    let up = crate::abi::arg(c, 4);
    draw_vertical_line(c, c.r[5], c.r[6], c.r[7], up);
}

/// `tScreenMemcard::DrawHorizontalLine` (SCREENMEMCARD.CPP, 0x80046d34): one of the grid's
/// horizontal lines at `x`, `y`, faded by `fade` (0..64), filled from its left or right, its
/// ends glowing.
pub fn draw_horizontal_line(c: &mut Cpu, x: u32, y: u32, fade: u32, top: u32) {
    let fade = line_fade(fade);
    let m = &mut c.m;
    let len = sx(h(m, GRID_W).wrapping_add((h(m, LINE_W) << 1) + 2));
    let mode = if (top & 0xFFFF) << 16 != 0 { 3 } else { 2 };
    let glow = m.r32(LINE_W);
    grid_line(c, x, y, len, 1, mode, fade, glow);
}

pub fn abi_draw_horizontal_line(c: &mut Cpu) {
    let top = crate::abi::arg(c, 4);
    draw_horizontal_line(c, c.r[5], c.r[6], c.r[7], top);
}

/// A halfword's third (toward zero), sign-extended.
fn third(v: u32) -> u32 {
    sx(((v as u16 as i16 as i32) / 3) as u32)
}

/// `tScreenMemcard::PlaceIcons` (SCREENMEMCARD.CPP, 0x80046df4): save `k`'s icon in each of
/// the grid's cells its blocks take (three a row from the bottom, the odd rows right to
/// left), its frame by the clock, faded by its own fade and `fade`; this game's save a
/// highlight instead, with a box round its cell when nothing fades.
pub fn place_icons(c: &mut Cpu, this: u32, k: u32, fade: u32) {
    let mut n = 0u32;
    while (n as i32) < c.m.r8(this.wrapping_add(k) + BLOCKS) as i32 {
        let m = &mut c.m;
        let place = h(m, this + PLACE);
        let row = third(place);
        let col = sx(place.wrapping_sub(row.wrapping_mul(3)));
        let col = if row & 1 == 0 { col } else { 2u32.wrapping_sub(col) };
        let cx = h(m, ICON_X).wrapping_add(h(m, GRID_X)).wrapping_add(m.r32(CELL_W).wrapping_mul(col));
        let cy = h(m, GRID_Y).wrapping_add(h(m, ICON_Y)).wrapping_add(4u32.wrapping_sub(third(place)).wrapping_mul(m.r32(CELL_H)));
        let tick = (m.r32(GAME_TICKS) as i32 >> 4) as u32;
        let frames = m.r8(this.wrapping_add(k) + FRAMES) as i32;
        let frame = (tick as i32).wrapping_rem(frames) as u32;
        let own = sx(h(m, this.wrapping_add(k << 1) + FADES)).wrapping_add(fade);
        let f = if (own as i32) < 129 { own } else { 128 };
        if k == m.r32(this + SAVE) {
            let flash = sx(h(m, this + FLASH));
            with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
                DrawShapeExtended(options).set_tint(&mut c.m, 0, HIGHLIGHT);
                call(c, DRAW_SHAPE_EXTENDED, &[flash, COLOURED_TOP_LEFT, sx(cx).wrapping_sub(242), sx(cy).wrapping_sub(112), f, 1, options]);
            });
        } else {
            let shape = m.r32(ICONS).wrapping_add(k.wrapping_mul(ICON_SET)).wrapping_add(frame.wrapping_mul(ICON_SIZE));
            draw_icon(c, shape, sx(cx), sx(cy), 31, 16, sx(f));
        }
        let m = &mut c.m;
        if m.r32(this + SAVE) == k && fade == 0 && m.r32(this + FADING_OUT) == 0 {
            // a box round the cell: black inside white
            let (x, y) = (sx(cx), sx(cy));
            let (ix, iy, w, hh) = (m.r32(ICON_X), m.r32(ICON_Y), m.r32(CELL_W), m.r32(CELL_H));
            call_addr(c, DRAW_SQUARE, &[0, x.wrapping_sub(ix).wrapping_add(2), y.wrapping_sub(iy).wrapping_add(1), w.wrapping_sub(2), hh.wrapping_sub(1)]);
            let m = &mut c.m;
            let (ix, iy, w, hh) = (m.r32(ICON_X), m.r32(ICON_Y), m.r32(CELL_W), m.r32(CELL_H));
            call_addr(c, DRAW_SQUARE, &[0xBEBE, x.wrapping_sub(ix), y.wrapping_sub(iy), w.wrapping_add(2), hh.wrapping_add(1)]);
        }
        let p = c.m.r16(this + PLACE).wrapping_add(1);
        c.m.w16(this + PLACE, p);
        n += 1;
    }
}

pub fn abi_place_icons(c: &mut Cpu) {
    place_icons(c, c.r[4], c.r[5], c.r[6]);
}

/// Save `k`'s icon gone: not loaded, no frames or blocks.
fn icon_gone(m: &mut Mem, this: u32, k: u32) {
    let s = this.wrapping_add(k);
    for f in [LOADED, FRAMES, BLOCKS] {
        m.w8(s + f, 0);
    }
}

/// Icon `k` faded out a step (8): at nothing (128) it's gone (before or after its CLUT is
/// released, as the two callers have it); whether it went.
fn fade_out_icon(c: &mut Cpu, this: u32, k: u32, gone_first: bool) -> bool {
    let at = this.wrapping_add(k << 1);
    let f = c.m.r16(at + FADES).wrapping_add(8);
    c.m.w16(at + FADES, f);
    if (f as i16) <= FADED_OUT as i16 {
        return false;
    }
    c.m.w16(at + FADES, FADED_OUT);
    if gone_first {
        icon_gone(&mut c.m, this, k);
    }
    let clut = c.m.r16(at + CLUTS);
    if clut != 0 {
        call(c, RELEASE_CLUT, &[clut as i16 as i32 as u32]);
        c.m.w16(at + CLUTS, 0);
    }
    if !gone_first {
        icon_gone(&mut c.m, this, k);
    }
    true
}

/// How many saves the card has.
fn count(c: &mut Cpu, this: u32) -> i32 {
    let rec = c.m.r32(this + RECORD);
    c.m.r32(rec + 8) as i32
}

/// `tScreenMemcard::DrawMemCardStuff` (SCREENMEMCARD.CPP, 0x80047148): the card's state:
/// gone (its icons faded out, the message the card's missing or a while later none in), not
/// formatted, empty, or its saves' icons loaded one at a time (none while a save goes on or
/// the icons fade out), faded in and placed, this game's save found (its name the message);
/// the highlight's frame stepped; the card layer's events run.
pub fn draw_mem_card_stuff(c: &mut Cpu, this: u32, fade: u32) {
    let card = c.m.r32(this + CARD);
    let rec = memcard::getcard(&mut c.m, card);
    let m = &mut c.m;
    m.w32(this + RECORD, rec);
    m.w16(this + PLACE, 0);
    // the highlight's frame: one more every 4 frames, of 30
    if m.r32(this + SAVE) != u32::MAX {
        let t = m.r16(this + FLASH_TIME).wrapping_add(1);
        m.w16(this + FLASH_TIME, t);
    } else {
        m.w16(this + FLASH, 0);
        m.w16(this + FLASH_TIME, 0);
    }
    if m.r16(this + FLASH_TIME) as i16 == 4 {
        m.w16(this + FLASH_TIME, 0);
        let f = m.r16(this + FLASH).wrapping_add(1);
        m.w16(this + FLASH, f);
        if f as i16 >= 30 {
            m.w16(this + FLASH, 0);
        }
    }
    let state = m.r32(rec);
    let place = |c: &mut Cpu, k: u32| place_icons(c, this, sx(k), sx(fade));
    if state == memcard_state::NONE {
        // the card gone: its icons faded out; "no card", later "insert a card"
        m.w32(this + RELEASE, 1);
        if m.r32(this + GONE_AT) == 0 {
            let t = m.r32(GAME_TICKS);
            m.w32(this + GONE_AT, t);
        }
        let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + GONE_AT));
        m.w32(this + WORD, if (since as i32) < 801 { NO_CARD } else { INSERT_CARD });
        let mut k = 0u32;
        while (k as u16 as i16 as i32) < count(c, this) {
            let j = sx(k);
            if c.m.r8(this.wrapping_add(j) + LOADED) != 0 && !fade_out_icon(c, this, j, true) {
                place(c, k);
            }
            k = k.wrapping_add(1);
        }
    } else if state == memcard_state::UNFORMATTED {
        m.w32(this + GONE_AT, 0);
        m.w32(this + WORD, UNFORMATTED);
    } else {
        m.w32(this + GONE_AT, 0);
        let n = m.r32(rec + 8);
        m.w32(this + WORD, if n != 0 { SAVES_WORD } else { NO_SAVE });
        icons(c, this, fade);
        let m = &mut c.m;
        if m.r32(this + FADING_OUT) != 0 && m.r32(this + ALL_IN) != 0 {
            m.w32(this + FADING_OUT, 0);
        }
    }
    let m = &mut c.m;
    let rec = m.r32(this + RECORD);
    if m.r32(rec) == memcard_state::FULL && m.r32(this + SAVE) == u32::MAX {
        m.w32(this + WORD, CARD_FULL);
    }
    if m.r32(CARD_ACTIVE) == 0 {
        let card = m.r32(this + CARD);
        memcard::handlecardevents(c, card);
    }
}

/// The saves' icons (a card there): when fading out, whether all are out; each loaded (one a
/// frame, none during a save or a fade-out), faded in (or out) and placed; this game's save
/// looked for once an icon is in.
fn icons(c: &mut Cpu, this: u32, fade: u32) {
    let m = &mut c.m;
    let rec = m.r32(this + RECORD);
    if m.r32(this + FADING_OUT) != 0 {
        m.w32(this + ALL_IN, 1);
        if (m.r32(rec + 8) as i32) <= 0 {
            return;
        }
        let mut k = 0u32;
        while (k as u16 as i16 as i32) < m.r32(rec + 8) as i32 {
            if (m.r16(this.wrapping_add(sx(k) << 1) + FADES) as i16) < FADED_OUT as i16 {
                m.w32(this + ALL_IN, 0);
            }
            k = k.wrapping_add(1);
        }
    } else {
        m.w32(this + ALL_IN, 0);
    }
    if count(c, this) <= 0 {
        return;
    }
    let mut k = 0u32;
    loop {
        if c.m.r32(this + RELEASE) != 0 {
            release_icons(c, this);
            c.m.w32(this + RELEASE, 0);
            c.m.w32(this + FADING_OUT, 0);
            return;
        }
        let j = sx(k);
        let at = this.wrapping_add(j);
        if c.m.r8(at + LOADED) == 0 && c.m.r32(CARD_ACTIVE) == 0 && c.m.r32(this + FADING_OUT) == 0 {
            load_icon(c, this, j);
        }
        let m = &mut c.m;
        if m.r32(this + FADING_OUT) != 0 && m.r32(this + ALL_IN) != 0 {
            m.w8(at + LOADED, 0);
        }
        let mut next = false;
        if m.r32(CARD_ACTIVE) == 0 {
            if m.r8(at + LOADED) == 0 {
                next = true;
            } else if m.r16(this.wrapping_add(j << 1) + FADES) == 0 {
                // this game's save: its name the message
                let card = m.r32(this + CARD);
                let save = memcard::fileexists(c, card, FILE_NAME);
                let m = &mut c.m;
                let word = m.r32(this + WORD);
                m.w32(this + SAVE, save);
                if word != INSERT_CARD && word != NO_CARD {
                    m.w32(this + GONE_AT, 0);
                }
                let save = m.r32(this + SAVE);
                if save != u32::MAX && m.r8(this.wrapping_add(save) + LOADED) != 0 {
                    m.w32(this + WORD, u32::MAX);
                    let name = this.wrapping_add((save << 5) + NAMES);
                    call(c, SPRINTF, &[this + MESSAGE, name]);
                } else {
                    m.w32(this + WORD, NO_SAVE);
                }
            }
        }
        if !next && c.m.r8(at + LOADED) != 0 {
            if c.m.r32(this + FADING_OUT) != 0 && c.m.r32(this + ALL_IN) == 0 {
                fade_out_icon(c, this, j, false);
            } else {
                let f = this.wrapping_add(j << 1) + FADES;
                let v = c.m.r16(f).wrapping_sub(8);
                c.m.w16(f, v);
                if (v as i16) < 0 {
                    c.m.w16(f, 0);
                }
            }
            place_icons(c, this, sx(k), sx(fade));
        }
        k = k.wrapping_add(1);
        if (k as u16 as i16 as i32) >= count(c, this) {
            return;
        }
    }
}

pub fn abi_draw_mem_card_stuff(c: &mut Cpu) {
    draw_mem_card_stuff(c, c.r[4], c.r[5]);
}

/// A menu item's flags (bit 0: off) in the menus.
fn set_off(m: &mut Mem, item: u32, off: bool) {
    let menus = m.r32(ALL_MENUS);
    let v = m.r32(menus + item);
    m.w32(menus + item, if off { v | 1 } else { v & !1 });
}

/// `tScreenMemcard::SetEnablings` (SCREENMEMCARD.CPP, 0x800476a4): with the card settled (no
/// icon loading or fading, no save going on): saving on with this game's save there or
/// room, loading on with a save there (for the first player only); loading off for a cheater.
pub fn set_enablings(c: &mut Cpu, this: u32) {
    let card = c.m.r32(this + CARD);
    let rec = memcard::getcard(&mut c.m, card);
    let m = &mut c.m;
    m.w32(this + RECORD, rec);
    let n = m.r32(rec + 8) as i32;
    let mut busy = false;
    for k in 0..n.max(0) as u32 {
        if m.r8(this + k + LOADED) == 0 || m.r16(this + 2 * k + FADES) as i16 > 0 {
            let rec = m.r32(this + RECORD);
            let st = m.r32(rec);
            if st != memcard_state::NONE && st != memcard_state::UNFORMATTED {
                busy = true;
            }
        }
    }
    if m.r32(CARD_ACTIVE) != 0 {
        busy = true;
    }
    if !busy {
        let none = m.r32(this + SAVE) == u32::MAX;
        set_off(m, SAVE_ITEM, none);
        let rec = m.r32(this + RECORD);
        let st = m.r32(rec);
        let there = st == 0 || st == memcard_state::UNFORMATTED || (st == memcard_state::FULL && m.r32(this + SAVE) != u32::MAX);
        let on = there && m.r16(this + PLAYER) == 0;
        set_off(m, LOAD_ITEM, !on);
    }
    if crate::fecheats::is_the_user_a_cry_baby_cheater(&mut c.m) {
        set_off(&mut c.m, LOAD_ITEM, true);
    }
}

pub fn abi_set_enablings(c: &mut Cpu) {
    set_enablings(c, c.r[4]);
}

/// `tScreenMemcard::DrawBackground` (SCREENMEMCARD.CPP, 0x80047858): (the card layer up a
/// couple of seconds into the screen, its icons cleared) the card's grid, its saves, the
/// message (the card's state or this game's save's name), the player's title, the grid's
/// shading, boxes and lines and the screen's shapes, faded in; the menu's enablings.
pub fn draw_background(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    m.w32(this + WORD, u32::MAX);
    let save = m.r32(this + SAVE);
    if m.r8(this.wrapping_add(save) + LOADED) == 0 {
        m.w32(this + SAVE, u32::MAX);
    }
    call_addr(c, SYSTEM_TASK, &[0]);
    // the fades: the shapes' (the second half of the screen's), the lines' (half), the
    // texts' (double)
    let m = &mut c.m;
    let twice = h(m, this + FADE) << 1;
    let shapes_fade = fade_clamp(twice.wrapping_sub(128)) as u16;
    let lines_fade = fade_clamp(((twice << 16) as i32 >> 17) as u32) as u16;
    let mut texts_fade = ((twice << 16) as i32 >> 15) as u32;
    if (texts_fade as i32) >= 129 {
        texts_fade = 128;
    }
    if (texts_fade as i32) < 0 {
        texts_fade = 0;
    }
    if m.r32(this + UP) == 0 {
        let fading = m.r16(this + FADE);
        m.w32(this + WORD, CHECKING_CARD);
        if fading == 0 && (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32) >= 129 {
            crate::fememcard::init_memcard(c, true, false);
            let m = &mut c.m;
            for k in 0..SAVES {
                icon_gone(m, this, k);
                m.w16(this + 2 * k + FADES, FADED_OUT);
            }
            m.w32(this + UP, 1);
        } else if m.r32(this + START) == 0 {
            let t = m.r32(GAME_TICKS);
            m.w32(this + START, t);
        }
    }
    if c.m.r32(this + UP) != 0 && shapes_fade as i16 != 128 {
        draw_mem_card_stuff(c, this, sx(texts_fade));
    }
    let kind = c.m.r8(FADE_KIND) as u32;
    let colours = c.m.r32(TEXT_COLORS + 4 * kind);
    let colour = crate::fefades::fade_out(colours, sx(texts_fade));
    // the message
    let m = &mut c.m;
    let over = m.r32(this + WORD_OVER);
    if over != u32::MAX {
        m.w32(this + WORD, over);
    }
    let word = m.r32(this + WORD);
    if word != u32::MAX {
        let text = call(c, TEXT_WORD, &[word]);
        call(c, SPRINTF, &[this + MESSAGE, text]);
    }
    with_buffer(c, 8, |c, rect| {
        let m = &mut c.m;
        let (bx, by) = (m.r16(BOX_X), m.r16(BOX_Y));
        m.w16(rect + 4, 190);
        m.w16(rect + 6, 0);
        m.w16(rect, bx);
        m.w16(rect + 2, by);
        crate::fetextrender::word_wrap_text_rgb_justify(c, this + MESSAGE, RECT(rect), colour, 2, 0, 0);
    });
    let player = sx(h(&mut c.m, this + PLAYER));
    let title = call(c, TEXT_WORD, &[player.wrapping_add(TITLE)]);
    let (tx, ty) = (sx(h(&mut c.m, TEXT_X)), sx(h(&mut c.m, TEXT_Y)));
    crate::fetextrender::full_text_rgb(c, title, tx, ty, colour, 0, 2);
    // the grid's shading: a box darkened to its middle from above and below; the margins black
    let m = &mut c.m;
    let (lw, gx, gy, lh, lt, gw) = (h(m, LINE_W), h(m, GRID_X), h(m, GRID_Y), h(m, LINE_H), h(m, LINE_T), h(m, GRID_W));
    let x = sx(gx.wrapping_sub(lw));
    let y = sx(gy.wrapping_sub(lh).wrapping_sub(lt + 4));
    let w = sx(gw.wrapping_add(lw << 1).wrapping_add(2));
    let tall = sx(h(m, GRID_H).wrapping_add(lh << 1).wrapping_add(lt + 6));
    let half = ((tall as i32) / 2) as u32;
    call_addr(c, SUBTRACTIVE_BOX, &[x, y, w, half, SHADE, SHADE, 0, 0]);
    call_addr(c, SUBTRACTIVE_BOX, &[x, y.wrapping_add(half), w, half, 0, 0, SHADE, SHADE]);
    let m = &mut c.m;
    let (gx, ty, gw, mt) = (m.r32(GRID_X), m.r32(TEXT_Y), m.r32(GRID_W), m.r32(MARGIN_T));
    call_addr(c, DRAW_SQUARE, &[0, gx + 2, ty.wrapping_sub(2), gw.wrapping_sub(4), mt.wrapping_sub(1)]);
    let m = &mut c.m;
    let (gx, ch, gy, gw, mb) = (m.r32(GRID_X), m.r32(CELL_H), m.r32(GRID_Y), m.r32(GRID_W), m.r32(MARGIN_B));
    let y = gy.wrapping_add(ch.wrapping_mul(5)).wrapping_add(1);
    call_addr(c, DRAW_SQUARE, &[0, gx + 2, y, gw.wrapping_sub(4), mb.wrapping_sub(1)]);
    // the columns' lines, the rows' (one over the top when there's room)
    let lines_fade = sx(lines_fade as u32);
    let mut x = h(&mut c.m, GRID_X);
    for k in 0..4u32 {
        let m = &mut c.m;
        let y = sx(h(m, GRID_Y).wrapping_sub(h(m, LINE_H)).wrapping_sub(h(m, LINE_T)));
        draw_vertical_line(c, sx(x), y, lines_fade, parity(k));
        x = x.wrapping_add(h(&mut c.m, CELL_W));
    }
    let m = &mut c.m;
    let mut y = h(m, GRID_Y);
    if m.r32(LINE_T) != 0 {
        let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
        let top = sx(y.wrapping_sub(h(m, LINE_T)));
        draw_horizontal_line(c, x, top, lines_fade, 1);
    }
    let mut cell = 0;
    for k in 0..6u32 {
        let m = &mut c.m;
        let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
        draw_horizontal_line(c, x, sx(y), lines_fade, parity(k));
        cell = h(&mut c.m, CELL_H);
        y = y.wrapping_add(cell);
    }
    let m = &mut c.m;
    let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
    let bottom = sx(y.wrapping_add(h(m, MARGIN_B).wrapping_sub(cell)));
    draw_horizontal_line(c, x, bottom, lines_fade, parity(6));
    // the screen's shapes
    let fade = sx(shapes_fade as u32);
    for k in 0..16 {
        call(c, DRAW_SHAPE_EXTENDED, &[k + 30, 0, 0, 0, fade, 0, 0]);
    }
    if c.m.r32(this + UP) != 0 {
        set_enablings(c, this);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, c.r[4]);
}

/// `tScreenMemcard::DrawForeground` (SCREENMEMCARD.CPP, 0x80047ec0): (once faded in far
/// enough) the screen's help line and its box; the screen's front shapes, faded in.
pub fn draw_foreground(c: &mut Cpu, this: u32) {
    let twice = h(&mut c.m, this + FADE) << 1;
    let fade = fade_clamp(twice.wrapping_sub(128));
    if (twice as u16 as i16) >= 100 {
        crate::fetextrender::menu_text_positioned_justify(c, CHECKING, 480, 220, 1, 1, 4);
        let text = call(c, TEXT_WORD, &[CHECKING]);
        let width = call(c, TEXT_PIXELS, &[text]);
        call_addr(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(width).wrapping_sub(5), 7]);
    }
    let fade = sx(fade);
    for k in 0..4 {
        call(c, DRAW_SHAPE_EXTENDED, &[k + 56, 0, 0, 0, fade, 0, 0]);
    }
}

pub fn abi_draw_foreground(c: &mut Cpu) {
    draw_foreground(c, c.r[4]);
}

/// `tScreenMemcard::tScreenMemcard` (SCREENMEMCARD.CPP, 0x80047fcc): a screen of the first
/// card, no word over its message.
pub fn screen_memcard(c: &mut Cpu, this: u32) -> u32 {
    crate::fescreen::screen(c, Screen(this));
    c.m.w32(this + VPTR, VTABLE);
    c.m.w32(this + WORD_OVER, u32::MAX);
    c.m.w32(this + CARD, 1);
    this
}

pub fn abi_screen_memcard(c: &mut Cpu) {
    c.r[2] = screen_memcard(c, c.r[4]);
}

/// `tScreenMemcard::ReleaseIcons` (SCREENMEMCARD.CPP, 0x80048010): every icon gone, its CLUT
/// released.
pub fn release_icons(c: &mut Cpu, this: u32) {
    for k in 0..SAVES {
        icon_gone(&mut c.m, this, k);
        let clut = c.m.r16(this + 2 * k + CLUTS);
        if clut != 0 {
            call(c, RELEASE_CLUT, &[clut as i16 as i32 as u32]);
            c.m.w16(this + 2 * k + CLUTS, 0);
        }
    }
}

pub fn abi_release_icons(c: &mut Cpu) {
    release_icons(c, c.r[4]);
}

/// `tScreenMemcard::Initialize` (SCREENMEMCARD.CPP, 0x80048088): the grid's layout, the
/// icons' buffer, the player's card (1 or 5) and title, the menu's load and save off (its
/// save item saying the player's), no icons, no save found.
pub fn initialize(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    for (at, v) in [
        (GRID_X, 246), (GRID_Y, 60), (CELL_W, 59), (CELL_H, 27), (MARGIN_T, 12), (MARGIN_B, 12), (LINE_T, 12),
        (GRID_W, 177), (GRID_H, 147), (LINE_W, 12), (LINE_H, 6), (ICON_X, 15), (ICON_Y, 6), (TEXT_Y, 51),
        (BOX_X, 336), (TEXT_X, 336),
    ] {
        m.w32(at, v);
    }
    m.w32(this + START, 0);
    m.w32(BOX_Y, 198);
    let icons = call(c, RESERVE, &[RECORDS_NAME, SAVES * ICON_SET, 0]);
    let m = &mut c.m;
    let app = m.r32(FE_APP);
    m.w32(ICONS, icons);
    m.w32(this + GONE_AT, 0);
    m.w16(this + FLASH, 0);
    m.w16(this + FLASH_TIME, 0);
    let player = m.r8(app + APP_PLAYER) as u32;
    m.w16(this + PLAYER, player as u16);
    m.w32(this + CARD, (player << 2) + 1);
    let item = if m.r16(this + PLAYER) != 0 { 650 } else { 648 };
    let menus = m.r32(ALL_MENUS);
    let (load, save) = (m.r32(menus + LOAD_ITEM), m.r32(menus + SAVE_ITEM));
    m.w32(menus + SAVE_ITEM_WORD, item);
    m.w32(menus + LOAD_ITEM, load | 1);
    m.w32(menus + SAVE_ITEM, save | 1);
    for k in 0..SAVES {
        icon_gone(m, this, k);
        m.w16(this + 2 * k + FADES, FADED_OUT);
        m.w16(this + 2 * k + CLUTS, 0);
    }
    for (at, v) in [(UP, 0), (FADING_OUT, 0), (ALL_IN, 0), (SAVE, u32::MAX), (RELEASE, 0)] {
        m.w32(this + at, v);
    }
    crate::fescreen::initialize(c, Screen(this));
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, c.r[4]);
}

/// `tScreenMemcard::Cleanup` (SCREENMEMCARD.CPP, 0x80048230): the icons released, the card
/// layer stopped, the menu's load and save back on, the icons' buffer freed; then a wait for
/// the music's buffer.
pub fn cleanup(c: &mut Cpu, this: u32) {
    release_icons(c, this);
    crate::fememcard::deinit_memcard(c);
    let m = &mut c.m;
    let icons = m.r32(ICONS);
    set_off(m, LOAD_ITEM, false);
    set_off(m, SAVE_ITEM, false);
    call(c, PURGE, &[icons]);
    crate::fescreen::cleanup(c, Screen(this));
    while !music_buffered(c) {
        crate::feaudio::systemtask(c);
        c.tick();
    }
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, c.r[4]);
}

/// `tScreenMemcard::~tScreenMemcard` (SCREENMEMCARD.CPP, 0x800482c0): the screen's.
pub fn abi_screen_memcard_destructor(c: &mut Cpu) {
    crate::fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{CARD_ANSWERS, Rng, against_original_in, fe_ram};

    /// The game's memory card screen; the icons' buffer (15 saves of 3 icon shapes); a music
    /// stream; the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_0770;
    const BUFFER: u32 = 0x8014_0000;
    const MUSIC: u32 = 0x8014_2400;
    const OUT: u32 = 0x8014_2600;
    /// The music's stream (gp-relative), MEMCARD.C's state and cards, FEMEMCARD's card layer up.
    const STREAM: u32 = 0x8013_D83C + 0x1D8;
    const S: u32 = crate::memcard::STATE;
    const CARDS: u32 = S + 1180;
    const INITED: u32 = 0x8005_1F08;

    fn put(m: &mut ps1rt::Mem, at: u32, s: &[u8]) {
        for (k, &b) in s.iter().enumerate() {
            m.w8(at + k as u32, b);
        }
        m.w8(at + s.len() as u32, 0);
    }

    /// FEMENU's structures, then: the grid's layout (Initialize's, its top line now and then
    /// none); the icons (4-bit and others, a palette after them); the music's buffer below its
    /// threshold or not; the card layer idle (its sync answering nothing, nothing pending, the
    /// timers out), the cards' states (gone, unformatted, new, there), saves and their sizes
    /// (this game's among others); the screen: its fade about its steps, card, this game's
    /// save, the saves' names, each icon loaded or not (its frames 1..3, blocks 0..3, CLUT, fade
    /// about its ends), the flags, the message's words, the highlight's frame and time about
    /// their wraps, the grid's place, the card's time gone and the screen's start about their
    /// limits (never a start of the card layer: it waits on the pads), the player.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for (at, v) in [
            (GRID_X, 246), (GRID_Y, 60), (CELL_W, 59), (CELL_H, 27), (MARGIN_T, 12), (MARGIN_B, 12), (LINE_T, 12),
            (GRID_W, 177), (GRID_H, 147), (LINE_W, 12), (LINE_H, 6), (ICON_X, 15), (ICON_Y, 6), (TEXT_Y, 51),
            (BOX_X, 336), (TEXT_X, 336), (BOX_Y, 198),
        ] {
            m.w32(at, if pick(rng, 8) == 0 { pick(rng, 300) } else { v });
        }
        if pick(rng, 4) == 0 {
            m.w32(LINE_T, 0);
        }
        if pick(rng, 8) == 0 {
            m.w32(GRID_H, 0xFFF0 + pick(rng, 16));
        }
        let app = m.r32(FE_APP);
        m.w8(app + 557, pick(rng, 2) as u8);
        m.w32(ICONS, BUFFER);
        for k in 0..45 {
            let s = BUFFER + 192 * k;
            for j in 0..192 {
                m.w8(s + j, rng.next() as u8);
            }
            m.w8(s, [0x40, 0x48, 0x41, 0x09][pick(rng, 4) as usize]);
            m.w8(s + 1, 0x90);
            m.w16(s + 2, 0);
            m.w16(s + 4, [16, pick(rng, 64)][pick(rng, 2) as usize] as u16);
            m.w16(s + 6, [16, pick(rng, 64)][pick(rng, 2) as usize] as u16);
            m.w8(s + 0x90, [0x22, 0x23, 0x24][pick(rng, 3) as usize]);
            m.w8(s + 0x91, 0);
            m.w16(s + 0x92, 0);
            m.w16(s + 0x94, pick(rng, 17) as u16);
            m.w16(s + 0x96, 1);
        }
        if pick(rng, 3) == 0 {
            m.w32(STREAM, MUSIC);
            m.w32(MUSIC + 0x88, 1);
            m.w32(MUSIC + 0x8C, 1);
            m.w32(MUSIC + 0x1C, 0);
            m.w32(MUSIC + 0x14, 2);
            m.w32(MUSIC + 0xA4, pick(rng, 4));
            m.w32(MUSIC + 0x84, pick(rng, 4));
        } else {
            m.w32(STREAM, 0);
        }
        m.w32(CARD_ANSWERS, u32::MAX);
        // (an icon's read's sync: its result fine mostly, so the reads end well too)
        m.w32(CARD_ANSWERS + 4, pick(rng, 4));
        m.w32(CARD_ANSWERS + 8, [0, 0, 0, 1, 3][pick(rng, 5) as usize]);
        for k in 0..32 {
            let v = [0x4F82u16, 0x6082, 0x8182, 0x4081, 0][pick(rng, 5) as usize];
            m.w16(S + 608 + 48 + 2 * k, if k == 31 { 0 } else { v });
        }
        for k in 3..11 {
            m.w32(CARD_ANSWERS + 4 * k, [0, 1, 2, 3, 4, 5, pick(rng, 10)][pick(rng, 7) as usize]);
        }
        m.w32(S + 48, 0);
        for k in 0..8 {
            m.w32(S + 64 + 4 * k, [0, 1, 100][pick(rng, 3) as usize]);
        }
        m.w32(CARD_ACTIVE, (pick(rng, 4) == 0) as u32);
        let saves = [0, 1, 2, 5, 14, 15][pick(rng, 6) as usize];
        for card in [0, 4] {
            let at = CARDS + 616 * card;
            m.w32(at, [0, u32::MAX, (-2i32) as u32, (-3i32) as u32, 1][pick(rng, 5) as usize]);
            m.w32(at + 8, saves);
            m.w32(at + 12, 15 - saves.min(15));
            for e in 0..15 {
                let entry = at + 16 + 40 * e;
                let name: &[u8] = [&b"BASLUS-00826NFS4"[..], b"BASLUS-00826NFS4P", b"BESLES-00001X", b""][pick(rng, 4) as usize];
                put(m, entry, name);
                m.w32(entry + 24, [8192, 16384, 0x6000, pick(rng, 0x10000)][pick(rng, 4) as usize]);
            }
        }
        let s = SCREEN;
        m.w16(s + FADE, [0, 1, 32, 49, 50, 63, 64, 65, 100, 128, pick(rng, 0x10000)][pick(rng, 11) as usize] as u16);
        let card = [1, 5][pick(rng, 2) as usize];
        m.w32(s + CARD, card);
        m.w32(s + RECORD, CARDS + 616 * (card - 1));
        m.w32(s + SAVE, if pick(rng, 3) == 0 { u32::MAX } else { pick(rng, 15) });
        for k in 0..15 {
            let len = pick(rng, 20);
            for j in 0..32 {
                m.w8(s + NAMES + 32 * k + j, if j < len { b'A' + pick(rng, 26) as u8 } else { 0 });
            }
            m.w8(s + LOADED + k, (pick(rng, 3) != 0) as u8);
            m.w8(s + FRAMES + k, 1 + pick(rng, 3) as u8);
            m.w8(s + BLOCKS + k, pick(rng, 4) as u8);
            m.w16(s + CLUTS + 2 * k, [0, 0x7C00 + pick(rng, 64), 0x7800][pick(rng, 3) as usize] as u16);
            m.w16(s + FADES + 2 * k, [0, 1, 4, 8, 120, 121, 124, 127, 128, 136, 0xFFF8, pick(rng, 200)][pick(rng, 12) as usize] as u16);
        }
        for f in [ALL_IN, UP, FADING_OUT, RELEASE] {
            m.w32(s + f, (pick(rng, 3) == 0) as u32);
        }
        m.w32(s + WORD, [u32::MAX, 636, 640, 645][pick(rng, 4) as usize]);
        m.w32(s + WORD_OVER, [u32::MAX, u32::MAX, 704, 637][pick(rng, 4) as usize]);
        m.w16(s + FLASH, [0, 1, 28, 29, 30][pick(rng, 5) as usize]);
        m.w16(s + FLASH_TIME, [0, 2, 3, 4, 5][pick(rng, 5) as usize]);
        m.w16(s + PLACE, pick(rng, 20) as u16);
        let now = m.r32(GAME_TICKS);
        m.w32(s + GONE_AT, [0, now, now.wrapping_sub(800), now.wrapping_sub(801), now.wrapping_sub(5000)][pick(rng, 5) as usize]);
        m.w32(s + START, [0, now, now.wrapping_sub(128), now.wrapping_sub(129)][pick(rng, 4) as usize]);
        // (never the card layer's start, which waits on a dialog the test CPUs don't draw:
        // `DrawBackground`'s `systemtask` lets a tick go by first)
        if m.r32(s + UP) == 0 && m.r16(s + FADE) == 0 && (now.wrapping_sub(m.r32(s + START)) as i32) >= 128 {
            m.w32(s + START, now);
        }
        m.w16(s + PLAYER, (pick(rng, 3) == 0) as u16);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenMemcard::GetShapeInfo", 0x8004_6708, nfs4rc::f_80046708, |_, _| vec![SCREEN, OUT, OUT + 4, OUT + 8, OUT + 12]),
            ("tScreenMemcard::DrawIcon", 0x8004_672c, nfs4rc::f_8004672c, |_, r| {
                let shape = BUFFER + 192 * pick(r, 45);
                vec![SCREEN, shape, pick(r, 640), pick(r, 480), 31, 16, [0, 64, 128, pick(r, 0x10000)][pick(r, 4) as usize]]
            }),
            ("tScreenMemcard::LoadIcon", 0x8004_6924, nfs4rc::f_80046924, |_, r| vec![SCREEN, pick(r, 16)]),
            ("tScreenMemcard::DrawVerticalLine", 0x8004_6c80, nfs4rc::f_80046c80, |_, r| line(r)),
            ("tScreenMemcard::DrawHorizontalLine", 0x8004_6d34, nfs4rc::f_80046d34, |_, r| line(r)),
            ("tScreenMemcard::PlaceIcons", 0x8004_6df4, nfs4rc::f_80046df4, |_, r| {
                vec![SCREEN, pick(r, 15), [0, 0, 8, 64, 128, (-8i32) as u32][pick(r, 6) as usize]]
            }),
            ("tScreenMemcard::DrawMemCardStuff", 0x8004_7148, nfs4rc::f_80047148, |_, r| vec![SCREEN, [0, 8, 64, 128][pick(r, 4) as usize]]),
            ("tScreenMemcard::SetEnablings", 0x8004_76a4, nfs4rc::f_800476a4, |_, _| vec![SCREEN]),
            ("tScreenMemcard::DrawBackground", 0x8004_7858, nfs4rc::f_80047858, |_, _| vec![SCREEN]),
            ("tScreenMemcard::DrawForeground", 0x8004_7ec0, nfs4rc::f_80047ec0, |_, _| vec![SCREEN]),
            ("tScreenMemcard", 0x8004_7fcc, nfs4rc::f_80047fcc, |_, _| vec![OUT + 0x100]),
            ("tScreenMemcard::ReleaseIcons", 0x8004_8010, nfs4rc::f_80048010, |_, _| vec![SCREEN]),
            ("tScreenMemcard::Initialize", 0x8004_8088, nfs4rc::f_80048088, |_, _| vec![SCREEN]),
            ("tScreenMemcard::Cleanup", 0x8004_8230, nfs4rc::f_80048230, |c, _| {
                // (the card layer down and the music's buffer full: the test CPUs neither wait
                // for the pads nor fill it)
                c.m.w32(INITED, 0);
                c.m.w32(STREAM, 0);
                vec![SCREEN]
            }),
            ("~tScreenMemcard", 0x8004_82c0, nfs4rc::f_800482c0, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    fn line(r: &mut Rng) -> Vec<u32> {
        let fade = [0, 1, 63, 64, 65, 0xFFFF, 0x1_0020, pick(r, 0x10000)][pick(r, 8) as usize];
        vec![SCREEN, pick(r, 640), pick(r, 480), fade, [0, 1, 0x1_0000][pick(r, 3) as usize]]
    }

    #[test]
    fn screenmemcard_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xB0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xB0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
