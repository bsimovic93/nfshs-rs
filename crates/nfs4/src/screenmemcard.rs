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

use crate::abi::{self, Saved};
use crate::globals::{FE_APP, GAME_TICKS, TEXT_COLORS};
use ps1rt::Cpu;

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

/// A port called from a port with `args` in a0.. (stack ones written by the caller first).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len().min(4)].copy_from_slice(&args[..args.len().min(4)]);
        let sp = c.r[29];
        for (k, &v) in args.iter().enumerate().skip(4) {
            c.m.w32(sp + 4 * k as u32, v);
        }
        f(c);
        c.r[2]
    })
}

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

fn h(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

/// `tScreenMemcard::GetShapeInfo` (SCREENMEMCARD.CPP, 0x80046708): 60 shapes of `zmemcrd`,
/// none swapped.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (count, swap, file) = (c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    c.m.w16(swap, 0);
    c.m.w32(swap_file, 0);
    c.m.w16(count, 60);
    c.m.w32(file, SHAPE_FILE);
    c.r[2] = SHAPE_FILE;
}

/// `tScreenMemcard::DrawIcon` (SCREENMEMCARD.CPP, 0x8004672c): an icon's shape drawn as a
/// semi-transparent `POLY_FT4` at `x`, `y`, `w` by `h`, faded by `fade`.
pub fn abi_draw_icon(c: &mut Cpu) {
    let (shape, x, y) = (c.r[5], c.r[6], c.r[7]);
    let (w, hgt, fade) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6) & 0xFFFF);
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let place = m.r32(shape + 12);
        let mode = m.r8(shape) as u32;
        let width = m.r16(shape + 4) as i16 as i32 as u32;
        let height = m.r16(shape + 6) as i16 as i32 as u32;
        let ty = ((place << 4) as i32 >> 20) as u32;
        let tx = ((place << 20) as i32 >> 20) as u32;
        let u = (((tx & 0x3F) << 4) as i32 / 4) as u32;
        m.w32(sp + 16, height);
        let v = ty.wrapping_sub((((ty as i32) / 256) as u32) << 8);
        (r.0[1], r.0[4], r.0[3], r.0[2], r.0[5], r.0[7], r.0[6], r.0[8]) = (ty, tx, u, v, x, y, mode, width);
        let ot = m.r32(0x1F80_0000);
        let prim = crate::mmeffect::add_prim(m, 40);
        r.0[0] = prim;
        m.w8(prim + 7, 46);
        m.w8(prim + 3, 9);
        let grey = (-128i32 as u32).wrapping_sub(fade) as u8;
        for at in [4, 5, 6] {
            m.w8(prim + at, grey);
        }
        // (the OT's linking leaves a1..a3, which the callee keeps)
        (c.r[5], c.r[6], c.r[7]) = (ot, 0xFF00_0000, 0x1F80_0004);
        let clut = r.call_leaving(c, SHAPE_TO_CLUT, &[shape], 0x8004_6860);
        let m = &mut c.m;
        let page = ((mode & 3) << 7) | (((ty & 0x100) as i32 >> 4) as u32) | (((tx & 0x3FF) as i32 >> 6) as u32) | ((ty & 0x200) << 2);
        m.w16(prim + 14, clut as u16);
        m.w16(prim + 22, page as u16);
        let u1 = u.wrapping_add(width);
        let v1 = v.wrapping_add(m.r32(sp + 16));
        for (at, b) in [(12, u), (13, v), (20, u1), (21, v), (28, u), (29, v1), (36, u1), (37, v1)] {
            m.w8(prim + at, b as u8);
        }
        let (x1, y1) = (x.wrapping_add(w), y.wrapping_add(hgt));
        for (at, hw) in [(8, x), (10, y), (18, y), (24, x), (16, x1), (32, x1), (26, y1), (34, y1)] {
            m.w16(prim + at, hw as u16);
        }
    })
}

/// `tScreenMemcard::LoadIcon` (SCREENMEMCARD.CPP, 0x80046924): (with the music's buffer full
/// enough) save `k`'s header read from the card: its frames (at most 3) and blocks, its icon's
/// frames put in VRAM with a CLUT of their own, faded in from nothing; a card gone or a bad
/// read noted (the icons released).
pub fn abi_load_icon(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5]);
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[6], r.0[5]) = (this, k);
        let buffered = r.call_leaving(c, MUSIC_BUFFERED, &[], 0x8004_695c);
        r.0[0] = buffered;
        let threshold = r.call_leaving(c, MUSIC_THRESHOLD, &[], 0x8004_6964);
        r.0[0] = ((buffered as i32) < threshold as i32) as u32;
        if r.0[0] != 0 {
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
        let file = this.wrapping_add(k.wrapping_mul(44)).wrapping_add(FILES);
        r.0[0] = k.wrapping_mul(44);
        r.call_leaving(c, BLOCKCLEAR, &[file, 44], 0x8004_69c0);
        r.0[1] = 22;
        r.0[0] = this.wrapping_add(k.wrapping_mul(44));
        let m = &mut c.m;
        m.w32(file + 4, this.wrapping_add(k << 5).wrapping_add(NAMES));
        let record = m.r32(this + RECORD);
        m.w32(file, record.wrapping_add(k.wrapping_mul(40)).wrapping_add(16));
        let icons = m.r32(ICONS).wrapping_add(k.wrapping_mul(576));
        m.w32(file + 24, icons);
        m.w32(file + 28, icons + 192);
        m.w32(file + 32, icons + 384);
        loop {
            let card = c.m.r32(this + CARD);
            let e = port(c, &r, 0x8005_0368, 0x8004_6a2c, &[card], crate::memcard::abi_handlecardevents);
            r.0[0] = k << 1;
            if e == 22 {
                break;
            }
            c.tick();
        }
        r.0[7] = 0;
        let card = c.m.r32(this + CARD);
        port(c, &r, 0x8004_fba0, 0x8004_6a5c, &[card, file, 1], crate::memcard::abi_loadfile);
        let s3 = this.wrapping_add(k);
        let fp = this.wrapping_add(k << 1);
        c.m.w32(sp + 32, k << 1);
        c.m.w32(sp + 36, k << 3);
        (r.0[3], r.0[8]) = (s3, fp);
        while r.0[7] == 0 {
            let card = c.m.r32(this + CARD);
            let at = port(c, &r, 0x8005_072c, 0x8004_6a84, &[card], crate::memcard::abi_getcard);
            if c.m.r32(at) == u32::MAX {
                c.m.w32(this + RELEASE, 1);
            }
            let card = c.m.r32(this + CARD);
            let e = port(c, &r, 0x8005_0368, 0x8004_6aa4, &[card], crate::memcard::abi_handlecardevents);
            match e {
                15 => {
                    r.0[7] = 1;
                    if c.m.r32(this + RELEASE) != 0 {
                        port(c, &r, 0x8004_8010, 0x8004_6ae8, &[this], abi_release_icons);
                        break;
                    }
                    let m = &mut c.m;
                    let frames_at = m.r32(this.wrapping_add(k.wrapping_mul(44)) + 676);
                    let frames = (m.r8(frames_at) as u32).wrapping_sub(16);
                    m.w8(s3 + FRAMES, frames as u8);
                    let card = m.r32(this + CARD);
                    let blocks = port(c, &r, 0x8005_07a4, 0x8004_6b30, &[card, k], crate::memcard::abi_grab_blocks);
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
                        r.call_leaving(c, RELEASE_CLUT, &[old as i16 as i32 as u32], 0x8004_6b78);
                    }
                    r.call_leaving(c, GET_CLUT_ID, &[0, sp + 24, sp + 28], 0x8004_6b88);
                    let m = &mut c.m;
                    let v = (h(m, sp + 28) << 6) | ((m.r32(sp + 24) as i32 >> 4) as u32 & 0x3F);
                    m.w16(fp + CLUTS, v as u16);
                    r.0[1] = 0;
                    if m.r8(s3 + FRAMES) != 0 {
                        let s4 = (c.m.r32(sp + 36).wrapping_add(k)) << 6;
                        (r.0[2], r.0[0], r.0[4]) = (900, 0, s4);
                        let mut j = 0u32;
                        loop {
                            let shape = c.m.r32(ICONS).wrapping_add(s4).wrapping_add(j * 192);
                            if c.m.r8(shape) & 0xF7 == 64 {
                                let (cx, cy) = (c.m.r32(sp + 24), c.m.r32(sp + 28));
                                r.call_leaving(c, UPLOAD_SHAPE, &[shape, r.0[2], (k << 4).wrapping_add(k), cx, cy], 0x8004_6c08);
                            }
                            r.0[2] += 17;
                            j += 1;
                            r.0[1] = j;
                            r.0[0] = j * 192;
                            if (j as i32) >= c.m.r8(s3 + FRAMES) as i32 {
                                break;
                            }
                        }
                    }
                    c.m.w16(fp + FADES, 128);
                    c.m.w8(s3 + LOADED, 1);
                }
                2 | 3 | 7 | 10 | 11 | 16 | 19 | 23 => {
                    r.0[7] = 1;
                    c.m.w32(this + RELEASE, 1);
                    c.m.w8(s3 + LOADED, 0);
                }
                _ => c.tick(),
            }
        }
        c.m.w32(CARD_ACTIVE, 0);
    })
}


/// The lines' clamp of their fade: 0..64 (a halfword's).
fn line_fade(v: u32) -> u32 {
    let f = v as u16 as i16 as i32;
    if f <= 0 {
        if f >= 0 { v } else { 0 }
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

/// `tScreenMemcard::DrawVerticalLine` (SCREENMEMCARD.CPP, 0x80046c80): one of the grid's
/// vertical lines at `x`, `y`, faded by `fade` (0..64), its brighter end up or down.
pub fn abi_draw_vertical_line(c: &mut Cpu) {
    let (x, y, fade) = (c.r[5], c.r[6], c.r[7]);
    let up = abi::arg(c, 4) & 0xFFFF;
    abi::frame(c, 40, |c, _| {
        let r = Saved::of(c);
        let fade = line_fade(fade);
        let m = &mut c.m;
        let len = sx(h(m, LINE_T).wrapping_add(h(m, GRID_H).wrapping_add(h(m, LINE_H) << 1)));
        let args = [0x78_5A5A, sx(x), sx(y), 2, len, (up << 16 == 0) as u32, ((fade << 16) as i32 >> 15) as u32, 0];
        port(c, &r, 0x8004_c018, 0x8004_6d24, &args, crate::felines::abi_draw_bright_end_line);
    })
}

/// `tScreenMemcard::DrawHorizontalLine` (SCREENMEMCARD.CPP, 0x80046d34): one of the grid's
/// horizontal lines at `x`, `y`, faded by `fade` (0..64), its glow below or above.
pub fn abi_draw_horizontal_line(c: &mut Cpu) {
    let (x, y, fade) = (c.r[5], c.r[6], c.r[7]);
    let top = abi::arg(c, 4) & 0xFFFF;
    abi::frame(c, 40, |c, _| {
        let r = Saved::of(c);
        let fade = line_fade(fade);
        let m = &mut c.m;
        let len = sx(h(m, GRID_W).wrapping_add((h(m, LINE_W) << 1) + 2));
        let kind = if top << 16 != 0 { 3 } else { 2 };
        let line_w = m.r32(LINE_W);
        let args = [0x78_5A5A, sx(x), sx(y), len, 1, kind, ((fade << 16) as i32 >> 15) as u32, line_w];
        port(c, &r, 0x8004_c018, 0x8004_6de4, &args, crate::felines::abi_draw_bright_end_line);
    })
}

/// A halfword's third (toward zero), sign-extended.
fn third(v: u32) -> u32 {
    sx(((v as u16 as i16 as i32) / 3) as u32)
}

/// `tScreenMemcard::PlaceIcons` (SCREENMEMCARD.CPP, 0x80046df4): save `k`'s icon in each of
/// the grid's cells its blocks take (three a row from the bottom, the odd rows right to
/// left), its frame by the clock, faded by its own fade and `fade`; this game's save a
/// highlight instead, with a box round its cell when nothing fades.
pub fn abi_place_icons(c: &mut Cpu) {
    let (this, k, fade) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3], r.0[4], r.0[5], r.0[6], r.0[8]) = (this, k, 0, 0x8005_0000, 0x8005_0000, 0x8005_0000);
        r.0[7] = this.wrapping_add(k << 1);
        c.m.w32(sp + 104, fade);
        let mut n = 0u32;
        while (n as i32) < c.m.r8(this.wrapping_add(k) + BLOCKS) as i32 {
            let m = &mut c.m;
            let place = h(m, this + PLACE);
            let row = third(place);
            let col = sx(place.wrapping_sub(row.wrapping_mul(3)));
            let col = if row & 1 == 0 { col } else { 2u32.wrapping_sub(col) };
            let cx = h(m, ICON_X).wrapping_add(h(m, GRID_X)).wrapping_add(m.r32(CELL_W).wrapping_mul(col));
            let cy = h(m, GRID_Y).wrapping_add(h(m, ICON_Y)).wrapping_add(4u32.wrapping_sub(third(place)).wrapping_mul(m.r32(CELL_H)));
            (r.0[1], r.0[0]) = (cx, cy);
            let tick = (m.r32(GAME_TICKS) as i32 >> 4) as u32;
            let frames = m.r8(this.wrapping_add(k) + FRAMES) as i32;
            let frame = (tick as i32).wrapping_rem(frames) as u32;
            let my_fade = sx(h(m, r.0[7] + FADES)).wrapping_add(fade);
            let f = if (my_fade as i32) < 129 { my_fade } else { 128 };
            if k == m.r32(this + SAVE) {
                m.w32(sp + 36, 0xB5_5623);
                let flash = sx(h(m, this + FLASH));
                let args = [flash, 1040, sx(cx).wrapping_sub(242), sx(cy).wrapping_sub(112), f, 1, sp + 32];
                r.call_leaving(c, DRAW_SHAPE_EXTENDED, &args, 0x8004_6fe4);
            } else {
                let shape = m.r32(ICONS).wrapping_add(k.wrapping_mul(576)).wrapping_add(frame.wrapping_mul(192));
                port(c, &r, 0x8004_672c, 0x8004_7060, &[this, shape, sx(cx), sx(cy), 31, 16, sx(f)], abi_draw_icon);
            }
            let m = &mut c.m;
            if m.r32(this + SAVE) == k && m.r32(sp + 104) == 0 && m.r32(this + FADING_OUT) == 0 {
                let (x, y) = (sx(cx), sx(cy));
                (r.0[1], r.0[0]) = (x, y);
                let (ix, iy, w, hh) = (m.r32(ICON_X), m.r32(ICON_Y), m.r32(CELL_W), m.r32(CELL_H));
                let args = [0, x.wrapping_sub(ix).wrapping_add(2), y.wrapping_sub(iy).wrapping_add(1), w.wrapping_sub(2), hh.wrapping_sub(1)];
                r.call_ptr_leaving(c, DRAW_SQUARE, &args, 0x8004_70d4);
                let m = &mut c.m;
                let (ix, iy, w, hh) = (m.r32(ICON_X), m.r32(ICON_Y), m.r32(CELL_W), m.r32(CELL_H));
                let args = [0xBEBE, x.wrapping_sub(ix), y.wrapping_sub(iy), w.wrapping_add(2), hh.wrapping_add(1)];
                r.call_ptr_leaving(c, DRAW_SQUARE, &args, 0x8004_7104);
            }
            let p = c.m.r16(this + PLACE).wrapping_add(1);
            c.m.w16(this + PLACE, p);
            n += 1;
            r.0[4] = n;
        }
    })
}

/// An icon faded out a step (8): at nothing (128) it's gone, its CLUT released; whether it
/// went.
fn fade_out_icon(c: &mut Cpu, r: &Saved, this: u32, k: u32, ra: u32) -> bool {
    let at = this.wrapping_add(k << 1);
    let f = c.m.r16(at + FADES).wrapping_add(8);
    c.m.w16(at + FADES, f);
    if (f as i16) < 129 {
        return false;
    }
    c.m.w16(at + FADES, 128);
    let gone = |c: &mut Cpu| {
        let s = this.wrapping_add(k);
        for f in [LOADED, FRAMES, BLOCKS] {
            c.m.w8(s + f, 0);
        }
    };
    let first = ra == 0x8004_72c4;
    if first {
        gone(c);
    }
    let clut = c.m.r16(at + CLUTS);
    if clut != 0 {
        r.call_leaving(c, RELEASE_CLUT, &[clut as i16 as i32 as u32], ra);
        c.m.w16(at + CLUTS, 0);
    }
    if !first {
        gone(c);
    }
    true
}

fn count(c: &mut Cpu, this: u32) -> i32 {
    let rec = c.m.r32(this + RECORD);
    c.m.r32(rec + 8) as i32
}

/// `tScreenMemcard::DrawMemCardStuff` (SCREENMEMCARD.CPP, 0x80047148): the card's state:
/// gone (its icons faded out, the message the card's missing or a while later none in), not
/// formatted, empty, or its saves' icons loaded one at a time (none while a save goes on or
/// the icons fade out), faded in and placed, this game's save found (its name the message);
/// the highlight's frame stepped; the card layer's events run.
pub fn abi_draw_mem_card_stuff(c: &mut Cpu) {
    let (this, fade) = (c.r[4], c.r[5]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[5]) = (this, fade);
        let card = c.m.r32(this + CARD);
        let rec = port(c, &r, 0x8005_072c, 0x8004_7178, &[card], crate::memcard::abi_getcard);
        let m = &mut c.m;
        m.w32(this + RECORD, rec);
        m.w16(this + PLACE, 0);
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
        let rec = m.r32(this + RECORD);
        let state = m.r32(rec);
        let mut k = 0u32;
        let place = |c: &mut Cpu, r: &Saved, k: u32, ra: u32| {
            port(c, r, 0x8004_6df4, ra, &[this, sx(k), sx(fade)], abi_place_icons);
        };
        if state == u32::MAX {
            m.w32(this + RELEASE, 1);
            if m.r32(this + GONE_AT) == 0 {
                let t = m.r32(GAME_TICKS);
                m.w32(this + GONE_AT, t);
            }
            let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + GONE_AT));
            m.w32(this + WORD, if (since as i32) < 801 { 636 } else { 640 });
            r.0[3] = 0;
            while (k as u16 as i16 as i32) < count(c, this) {
                let j = sx(k);
                if c.m.r8(this.wrapping_add(j) + LOADED) != 0 {
                    r.0[0] = this.wrapping_add(j << 1);
                    if !fade_out_icon(c, &r, this, j, 0x8004_72c4) {
                        place(c, &r, k, 0x8004_72d8);
                    }
                }
                k = k.wrapping_add(1);
                r.0[3] = k;
            }
        } else if state == (-2i32) as u32 {
            m.w32(this + GONE_AT, 0);
            m.w32(this + WORD, 649);
        } else {
            m.w32(this + GONE_AT, 0);
            let n = m.r32(rec + 8);
            m.w32(this + WORD, if n != 0 { 638 } else { 645 });
            'icons: {
                if m.r32(this + FADING_OUT) != 0 {
                    m.w32(this + ALL_IN, 1);
                    if (m.r32(rec + 8) as i32) <= 0 {
                        break 'icons;
                    }
                    r.0[3] = 0;
                    while (k as u16 as i16 as i32) < m.r32(rec + 8) as i32 {
                        if (m.r16(this.wrapping_add(sx(k) << 1) + FADES) as i16) < 128 {
                            m.w32(this + ALL_IN, 0);
                        }
                        k = k.wrapping_add(1);
                        r.0[3] = k;
                    }
                } else {
                    m.w32(this + ALL_IN, 0);
                }
                if count(c, this) <= 0 {
                    break 'icons;
                }
                (k, r.0[3], r.0[4]) = (0, 0, u32::MAX);
                loop {
                    if c.m.r32(this + RELEASE) != 0 {
                        port(c, &r, 0x8004_8010, 0x8004_7358, &[this], abi_release_icons);
                        c.m.w32(this + RELEASE, 0);
                        c.m.w32(this + FADING_OUT, 0);
                        break 'icons;
                    }
                    let j = sx(k);
                    let at = this.wrapping_add(j);
                    if c.m.r8(at + LOADED) == 0 && c.m.r32(CARD_ACTIVE) == 0 && c.m.r32(this + FADING_OUT) == 0 {
                        port(c, &r, 0x8004_6924, 0x8004_742c, &[this, j], abi_load_icon);
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
                            let card = m.r32(this + CARD);
                            let save = port(c, &r, 0x8005_06b4, 0x8004_74a8, &[card, FILE_NAME], crate::memcard::abi_fileexists);
                            let m = &mut c.m;
                            let word = m.r32(this + WORD);
                            m.w32(this + SAVE, save);
                            if word != 640 && word != 636 {
                                m.w32(this + GONE_AT, 0);
                            }
                            let save = m.r32(this + SAVE);
                            if save != u32::MAX && m.r8(this.wrapping_add(save) + LOADED) != 0 {
                                m.w32(this + WORD, u32::MAX);
                                let name = this.wrapping_add((save << 5) + NAMES);
                                r.call_leaving(c, SPRINTF, &[this + MESSAGE, name], 0x8004_7500);
                            } else {
                                m.w32(this + WORD, 645);
                            }
                        }
                    }
                    if !next && c.m.r8(at + LOADED) != 0 {
                        r.0[2] = at;
                        if c.m.r32(this + FADING_OUT) != 0 && c.m.r32(this + ALL_IN) == 0 {
                            r.0[0] = this.wrapping_add(j << 1);
                            fade_out_icon(c, &r, this, j, 0x8004_7594);
                        } else {
                            let f = this.wrapping_add(j << 1) + FADES;
                            let v = c.m.r16(f).wrapping_sub(8);
                            c.m.w16(f, v);
                            if (v as i16) < 0 {
                                c.m.w16(f, 0);
                            }
                        }
                        place(c, &r, k, 0x8004_75e8);
                    } else if !next {
                        r.0[2] = at;
                    }
                    k = k.wrapping_add(1);
                    r.0[3] = k;
                    if (k as u16 as i16 as i32) >= count(c, this) {
                        break;
                    }
                }
            }
            let m = &mut c.m;
            if m.r32(this + FADING_OUT) != 0 && m.r32(this + ALL_IN) != 0 {
                m.w32(this + FADING_OUT, 0);
            }
        }
        let m = &mut c.m;
        let rec = m.r32(this + RECORD);
        if m.r32(rec) == (-3i32) as u32 && m.r32(this + SAVE) == u32::MAX {
            m.w32(this + WORD, 704);
        }
        if m.r32(CARD_ACTIVE) == 0 {
            let card = m.r32(this + CARD);
            port(c, &r, 0x8005_0368, 0x8004_7680, &[card], crate::memcard::abi_handlecardevents);
        }
    })
}

/// `tScreenMemcard::SetEnablings` (SCREENMEMCARD.CPP, 0x800476a4): with the card settled (no
/// icon loading or fading, no save going on): saving on with this game's save there or
/// room, loading on with a save there (for the first player only); loading off for a cheater.
pub fn abi_set_enablings(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let card = c.m.r32(this + CARD);
        let rec = port(c, &r, 0x8005_072c, 0x8004_76c0, &[card], crate::memcard::abi_getcard);
        let m = &mut c.m;
        m.w32(this + RECORD, rec);
        let n = m.r32(rec + 8) as i32;
        let mut busy = false;
        for k in 0..n.max(0) as u32 {
            if m.r8(this + k + LOADED) == 0 || m.r16(this + 2 * k + FADES) as i16 > 0 {
                let rec = m.r32(this + RECORD);
                let st = m.r32(rec);
                if st != u32::MAX && st != (-2i32) as u32 {
                    busy = true;
                }
            }
        }
        if m.r32(CARD_ACTIVE) != 0 {
            busy = true;
        }
        if !busy {
            let menus = m.r32(ALL_MENUS);
            let v = m.r32(menus + 12416);
            let none = m.r32(this + SAVE) == u32::MAX;
            m.w32(menus + 12416, if none { v | 1 } else { v & !1 });
            let rec = m.r32(this + RECORD);
            let st = m.r32(rec);
            let there = st == 0 || st == (-2i32) as u32 || (st == (-3i32) as u32 && m.r32(this + SAVE) != u32::MAX);
            let on = there && m.r16(this + PLAYER) == 0;
            let menus = m.r32(ALL_MENUS);
            let v = m.r32(menus + 12372);
            m.w32(menus + 12372, if on { v & !1 } else { v | 1 });
        }
        if port(c, &r, 0x8002_37f0, 0x8004_7828, &[], crate::fecheats::abi_is_the_user_a_cry_baby_cheater) != 0 {
            let menus = c.m.r32(ALL_MENUS);
            let v = c.m.r32(menus + 12372);
            c.m.w32(menus + 12372, v | 1);
        }
    })
}

/// `tScreenMemcard::DrawBackground` (SCREENMEMCARD.CPP, 0x80047858): (the card layer up a
/// couple of seconds into the screen, its icons cleared) the card's grid, its saves, the
/// message (the card's state or this game's save's name), the player's title, the grid's
/// shading, boxes and lines and the screen's shapes, faded in; the menu's enablings.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[7] = this;
        let m = &mut c.m;
        m.w32(this + WORD, u32::MAX);
        let save = m.r32(this + SAVE);
        if m.r8(this.wrapping_add(save) + LOADED) == 0 {
            m.w32(this + SAVE, u32::MAX);
        }
        r.call_ptr_leaving(c, SYSTEM_TASK, &[0], 0x8004_78b4);
        let m = &mut c.m;
        let twice = h(m, this + FADE) << 1;
        m.w16(sp + 40, fade_clamp(twice.wrapping_sub(128)) as u16);
        m.w16(sp + 48, fade_clamp(((twice << 16) as i32 >> 17) as u32) as u16);
        let mut s0 = ((twice << 16) as i32 >> 15) as u32;
        if (s0 as i32) >= 129 {
            s0 = 128;
        }
        if (s0 as i32) < 0 {
            s0 = 0;
        }
        r.0[0] = s0;
        if m.r32(this + UP) == 0 {
            let fading = m.r16(this + FADE);
            m.w32(this + WORD, 637);
            if fading == 0 && (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32) >= 129 {
                port(c, &r, 0x8003_47d4, 0x8004_79a0, &[1, 0], crate::fememcard::abi_init_memcard);
                let m = &mut c.m;
                for k in 0..15 {
                    for f in [LOADED, FRAMES, BLOCKS] {
                        m.w8(this + k + f, 0);
                    }
                    m.w16(this + 2 * k + FADES, 128);
                }
                m.w32(this + UP, 1);
            } else if m.r32(this + START) == 0 {
                let t = m.r32(GAME_TICKS);
                m.w32(this + START, t);
            }
        }
        if c.m.r32(this + UP) != 0 && c.m.r16(sp + 40) as i16 != 128 {
            port(c, &r, 0x8004_7148, 0x8004_7a2c, &[this, sx(s0)], abi_draw_mem_card_stuff);
        }
        let kind = c.m.r8(FADE_KIND) as u32;
        let colours = c.m.r32(TEXT_COLORS + 4 * kind);
        let s0 = port(c, &r, 0x8004_bd68, 0x8004_7a54, &[colours, sx(s0)], crate::fefades::abi_fade_out);
        r.0[0] = s0;
        let m = &mut c.m;
        let over = m.r32(this + WORD_OVER);
        if over != u32::MAX {
            m.w32(this + WORD, over);
        }
        let word = m.r32(this + WORD);
        if word != u32::MAX {
            let text = r.call_leaving(c, TEXT_WORD, &[word], 0x8004_7a84);
            r.call_leaving(c, SPRINTF, &[this + MESSAGE, text], 0x8004_7a90);
        }
        let s6 = 0x50_5050;
        r.0[6] = s6;
        let m = &mut c.m;
        let (bx, by) = (m.r16(BOX_X), m.r16(BOX_Y));
        m.w16(sp + 36, 190);
        m.w16(sp + 38, 0);
        m.w16(sp + 32, bx);
        m.w16(sp + 34, by);
        port(c, &r, 0x8002_1404, 0x8004_7ad8, &[this + MESSAGE, sp + 32, s0, 2, 0, 0], crate::fetextrender::abi_word_wrap_text_rgb_justify);
        let player = sx(h(&mut c.m, this + PLAYER));
        let title = r.call_leaving(c, TEXT_WORD, &[player.wrapping_add(660)], 0x8004_7ae8);
        let (tx, ty) = (sx(h(&mut c.m, TEXT_X)), sx(h(&mut c.m, TEXT_Y)));
        port(c, &r, 0x8002_0eac, 0x8004_7b10, &[title, tx, ty, s0, 0, 2], crate::fetextrender::abi_full_text_rgb);
        // the grid's shading: a box faded to its middle from above and below
        (r.0[5], r.0[4]) = (0x8005_0000, 0x8005_0000);
        let m = &mut c.m;
        let (lw, gx, gy, lh, lt, gw) = (h(m, LINE_W), h(m, GRID_X), h(m, GRID_Y), h(m, LINE_H), h(m, LINE_T), h(m, GRID_W));
        r.0[8] = 0;
        let s3 = sx(gx.wrapping_sub(lw));
        let s2 = sx(gy.wrapping_sub(lh).wrapping_sub(lt + 4));
        let s1 = sx(gw.wrapping_add(lw << 1).wrapping_add(2));
        let tall = sx(h(m, GRID_H).wrapping_add(lh << 1).wrapping_add(lt + 6));
        let s0 = ((tall as i32) / 2) as u32;
        (r.0[3], r.0[2], r.0[1], r.0[0]) = (s3, s2, s1, s0);
        r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[s3, s2, s1, s0, s6, s6, 0, 0], 0x8004_7bc8);
        r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[s3, s2.wrapping_add(s0), s1, s0, 0, 0, s6, s6], 0x8004_7bec);
        let m = &mut c.m;
        let (gx, ty, gw, mt) = (m.r32(GRID_X), m.r32(TEXT_Y), m.r32(GRID_W), m.r32(MARGIN_T));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, gx + 2, ty.wrapping_sub(2), gw.wrapping_sub(4), mt.wrapping_sub(1)], 0x8004_7c20);
        let m = &mut c.m;
        let (gx, ch, gy, gw, mb) = (m.r32(GRID_X), m.r32(CELL_H), m.r32(GRID_Y), m.r32(GRID_W), m.r32(MARGIN_B));
        let y = gy.wrapping_add(ch.wrapping_mul(5)).wrapping_add(1);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, gx + 2, y, gw.wrapping_sub(4), mb.wrapping_sub(1)], 0x8004_7c68);
        // the columns' lines
        let mut s0 = h(&mut c.m, GRID_X);
        let mut fp = 0u32;
        let mut s1;
        loop {
            (r.0[0], r.0[8], r.0[2]) = (s0, fp, 0x8005_0000);
            let m = &mut c.m;
            let t2 = h(m, sp + 48);
            s1 = t2 << 16;
            r.0[1] = s1;
            let y = sx(h(m, GRID_Y).wrapping_sub(h(m, LINE_H)).wrapping_sub(h(m, LINE_T)));
            port(c, &r, 0x8004_6c80, 0x8004_7cd8, &[this, sx(s0), y, sx(t2), parity(fp)], abi_draw_vertical_line);
            fp = fp.wrapping_add(1);
            s0 = s0.wrapping_add(h(&mut c.m, CELL_W));
            if (fp as u16 as i16) >= 4 {
                break;
            }
        }
        // the rows' lines
        let m = &mut c.m;
        let lt = m.r32(LINE_T);
        let mut s0 = h(m, GRID_Y);
        (r.0[0], r.0[8]) = (s0, fp);
        let fade = (s1 as i32 >> 16) as u32;
        if lt != 0 {
            let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
            let y = sx(s0.wrapping_sub(h(m, LINE_T)));
            port(c, &r, 0x8004_6d34, 0x8004_7d4c, &[this, x, y, fade, 1], abi_draw_horizontal_line);
        }
        let mut fp = 0u32;
        let (mut t0, mut t1);
        (r.0[3], r.0[2]) = (0x8005_0000, 0x8005_0000);
        loop {
            r.0[8] = fp;
            let m = &mut c.m;
            let t2 = h(m, sp + 48);
            r.0[1] = t2 << 16;
            let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
            port(c, &r, 0x8004_6d34, 0x8004_7db0, &[this, x, sx(s0), sx(t2), parity(fp)], abi_draw_horizontal_line);
            fp = fp.wrapping_add(1);
            t1 = h(&mut c.m, CELL_H);
            t0 = s0.wrapping_add(t1);
            s0 = t0;
            r.0[0] = s0;
            if (fp as u16 as i16) >= 6 {
                break;
            }
        }
        r.0[8] = fp;
        let m = &mut c.m;
        let fade = sx(h(m, sp + 48));
        let x = sx(h(m, GRID_X).wrapping_sub(h(m, LINE_W)));
        let y = sx(t0.wrapping_add(h(m, MARGIN_B).wrapping_sub(t1)));
        port(c, &r, 0x8004_6d34, 0x8004_7e38, &[this, x, y, fade, parity(fp)], abi_draw_horizontal_line);
        // the screen's shapes
        let fade = sx(h(&mut c.m, sp + 40));
        r.0[1] = fade;
        for k in 0..16 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k + 30, 0, 0, 0, fade, 0, 0], 0x8004_7e68);
        }
        r.0[0] = 16;
        if c.m.r32(this + UP) != 0 {
            port(c, &r, 0x8004_76a4, 0x8004_7e90, &[this], abi_set_enablings);
        }
    })
}

/// `tScreenMemcard::DrawForeground` (SCREENMEMCARD.CPP, 0x80047ec0): (once faded in far
/// enough) the screen's help line and its box; the screen's front shapes, faded in.
pub fn abi_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let twice = h(&mut c.m, this + FADE) << 1;
        let fade = fade_clamp(twice.wrapping_sub(128));
        r.0[1] = fade;
        if (twice as u16 as i16) >= 100 {
            port(c, &r, 0x8002_12e0, 0x8004_7f4c, &[638, 480, 220, 1, 1, 4], crate::fetextrender::abi_menu_text_positioned_justify);
            let text = r.call_leaving(c, TEXT_WORD, &[638], 0x8004_7f54);
            let width = r.call_leaving(c, TEXT_PIXELS, &[text], 0x8004_7f5c);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(width).wrapping_sub(5), 7], 0x8004_7f7c);
        }
        let fade = sx(fade);
        r.0[1] = fade;
        for k in 0..4 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k + 56, 0, 0, 0, fade, 0, 0], 0x8004_7fa8);
        }
    })
}

/// `tScreenMemcard::tScreenMemcard` (SCREENMEMCARD.CPP, 0x80047fcc): a screen of the first
/// card, no word over its message.
pub fn abi_screen_memcard(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8004_7fe0, &[this], crate::fescreen::abi_screen);
        c.m.w32(this + 96, VTABLE);
        c.m.w32(this + WORD_OVER, u32::MAX);
        c.m.w32(this + CARD, 1);
        c.r[2] = this;
    })
}

/// `tScreenMemcard::ReleaseIcons` (SCREENMEMCARD.CPP, 0x80048010): every icon gone, its CLUT
/// released.
pub fn abi_release_icons(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        for k in 0..15 {
            (r.0[1], r.0[0]) = (k, this + 2 * k);
            for f in [LOADED, FRAMES, BLOCKS] {
                c.m.w8(this + k + f, 0);
            }
            let clut = c.m.r16(this + 2 * k + CLUTS);
            if clut != 0 {
                r.call_leaving(c, RELEASE_CLUT, &[clut as i16 as i32 as u32], 0x8004_805c);
                c.m.w16(this + 2 * k + CLUTS, 0);
            }
        }
    })
}

/// `tScreenMemcard::Initialize` (SCREENMEMCARD.CPP, 0x80048088): the grid's layout, the
/// icons' buffer, the player's card (1 or 5) and title, the menu's load and save off (its
/// save item saying the player's), no icons, no save found.
pub fn abi_initialize(c: &mut Cpu) {
    let (this, ra) = (c.r[4], c.r[31]);
    abi::frame(c, 24, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        // (its return address saved past where the prologue's scan looks)
        c.m.w32(sp + 20, ra);
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
        let icons = r.call_leaving(c, RESERVE, &[RECORDS_NAME, 8640, 0], 0x8004_8168);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        m.w32(ICONS, icons);
        m.w32(this + GONE_AT, 0);
        m.w16(this + FLASH, 0);
        m.w16(this + FLASH_TIME, 0);
        let player = m.r8(app + 557) as u32;
        m.w16(this + PLAYER, player as u16);
        m.w32(this + CARD, (player << 2) + 1);
        let item = if m.r16(this + PLAYER) != 0 { 650 } else { 648 };
        let menus = m.r32(ALL_MENUS);
        let (load, save) = (m.r32(menus + 12372), m.r32(menus + 12416));
        m.w32(menus + 12420, item);
        m.w32(menus + 12372, load | 1);
        m.w32(menus + 12416, save | 1);
        for k in 0..15 {
            for f in [LOADED, FRAMES, BLOCKS] {
                m.w8(this + k + f, 0);
            }
            m.w16(this + 2 * k + FADES, 128);
            m.w16(this + 2 * k + CLUTS, 0);
        }
        for (at, v) in [(UP, 0), (FADING_OUT, 0), (ALL_IN, 0), (SAVE, u32::MAX), (RELEASE, 0)] {
            m.w32(this + at, v);
        }
        c.r[8] = 128;
        port(c, &r, 0x8002_60cc, 0x8004_8220, &[this, this + 30, 15, item], crate::fescreen::abi_initialize);
    })
}

/// `tScreenMemcard::Cleanup` (SCREENMEMCARD.CPP, 0x80048230): the icons released, the card
/// layer stopped, the menu's load and save back on, the icons' buffer freed; then a wait for
/// the music's buffer.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8004_8010, 0x8004_8244, &[this], abi_release_icons);
        port(c, &r, 0x8003_4938, 0x8004_824c, &[], crate::fememcard::abi_deinit_memcard);
        let m = &mut c.m;
        let icons = m.r32(ICONS);
        let menus = m.r32(ALL_MENUS);
        let (load, save) = (m.r32(menus + 12372), m.r32(menus + 12416));
        m.w32(menus + 12372, load & !1);
        m.w32(menus + 12416, save & !1);
        r.call_leaving(c, PURGE, &[icons], 0x8004_827c);
        port(c, &r, 0x8002_61c0, 0x8004_8284, &[this], crate::fescreen::abi_cleanup);
        loop {
            let buffered = r.call_leaving(c, MUSIC_BUFFERED, &[], 0x8004_828c);
            r.0[0] = buffered;
            let threshold = r.call_leaving(c, MUSIC_THRESHOLD, &[], 0x8004_8294);
            r.0[0] = ((buffered as i32) < threshold as i32) as u32;
            if r.0[0] == 0 {
                break;
            }
            port(c, &r, 0x8001_5a70, 0x8004_82a8, &[0], crate::feaudio::abi_systemtask);
            c.tick();
        }
    })
}

/// `tScreenMemcard::~tScreenMemcard` (SCREENMEMCARD.CPP, 0x800482c0): the screen's.
pub fn abi_screen_memcard_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        port(c, &r, 0x8002_6098, 0x8004_82d0, &[this, flags], crate::fescreen::abi_screen_destructor);
    })
}

/// `tScreenMemcard::tScreenMemcard` entered as the original's callers `jal` it.
pub(crate) fn screen_memcard_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_7fcc, None, &[]);
    abi_screen_memcard(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{CARD_ANSWERS, Rng, against_original_in, fe_ram, same_stack_in};

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
            m.w16(s + FADES + 2 * k, [0, 4, 8, 120, 121, 124, 128, 136, 0xFFF8, pick(rng, 200)][pick(rng, 10) as usize] as u16);
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
        if m.r32(s + UP) == 0 && m.r16(s + FADE) == 0 && (now.wrapping_sub(m.r32(s + START)) as i32) >= 129 {
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

    #[test]
    fn screenmemcard_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xB1E + k as u64, shuffle, &args);
        }
    }
}
