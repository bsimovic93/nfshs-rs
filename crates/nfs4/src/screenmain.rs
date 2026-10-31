//! SCREENMAIN.CPP: the main menu's screen (`tScreenMain`): a wall of 16 TVs over the background,
//! showing slices of one of two background pictures (one shown, the other loading), or a video
//! played across them (`zzzVD*.dct`, now and then once the wall has stood a while), or the
//! credits; on the menu's last item a picture over the middle TVs with a text; a little screen
//! swapped every 25 seconds, a column of bits rolling by, the drop shadow and the lines between
//! the TVs. Start swaps the background.
//!
//! The videos, `ClearImage`, `DrawSync`, `DrawTV`, the drop shadow and the squares are the
//! host's (called by address).

use crate::abi::{self, Saved};
use crate::femenu::KEY_START;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE, SPRINTF, TEXT_WORD};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN};
use crate::{fecredits, fefades, fescreen, fetextrender, fetv};
use ps1rt::{Cpu, Mem};

// A main screen: a `tScreen` (+0 its permanent shapes, +40 its swapped ones), then +100 its
// video, +104 the video's frames shown, +108 the tick its state began, +112 the tick the
// swapped shapes came, +116 where they are (0..3), +120 its state (`tScreenMainState`: 1 the
// background, 2 the video, 3 the last item's picture, 4 the credits), +124 the 16 TVs (48 bytes
// each), +892 what each TV shows (an image's kind), +956 the 16 images to show (24 bytes each),
// +1344 started, +1348 the swapped shapes up, +1352 the swapped file's number, +1354 the last
// item's picture's fade (0..96), +1356 the video's number and +1358 the next's, +1360 a video
// stopped and +1364 when, +1368 the two backgrounds' shapes (`tShapeInformation`, 40 bytes each),
// +1448 the one shown, +1452 the two backgrounds' numbers, +1460 the TVs left to turn on.
const SWAP: u32 = 40;
const VIDEO: u32 = 100;
const FRAMES: u32 = 104;
const STATE_AT: u32 = 108;
const SWAP_AT: u32 = 112;
const SPOT: u32 = 116;
const STATE: u32 = 120;
const TVS: u32 = 124;
const SHOWN: u32 = 892;
const IMAGES: u32 = 956;
const STARTED: u32 = 1344;
const ROLLING: u32 = 1348;
const ROLL: u32 = 1352;
const FADE: u32 = 1354;
const CLIP: u32 = 1356;
const NEXT_CLIP: u32 = 1358;
const ABORTED: u32 = 1360;
const ABORTED_AT: u32 = 1364;
const BACKS: u32 = 1368;
const CUR: u32 = 1448;
const BACK_NUMS: u32 = 1452;
const TO_TURN: u32 = 1460;

// A TV (FETV's): +0 its state (0 off, 1 on), +6 its brightness, +8 its flags, +16 its colour,
// +28 its texture's corner and size, +34 its CLUT, +36 its page.
const TV_SIZE: u32 = 48;
const TV_STATE: u32 = 0;
const TV_BRIGHT: u32 = 6;
const TV_FLAGS: u32 = 8;
const TV_COLOR: u32 = 16;
const TV_UV: u32 = 28;
const TV_CLUT: u32 = 34;
const TV_PAGE: u32 = 36;

// An image: +0 its kind (1 a background's slice, 2 the video's, 3 the last item's picture, 4
// the credits), +4 the TV's flags, +6 its texture's corner and size (u, v, w, h), +10 its CLUT,
// +12 its page, +16 its colour, +20 its brightness.
const IMAGE_SIZE: u32 = 24;
const KIND: u32 = 0;
const FLAGS: u32 = 4;
const UV: u32 = 6;
const CLUT: u32 = 10;
const PAGE: u32 = 12;
const COLOR: u32 = 16;
const BRIGHT: u32 = 20;

// A `tShapeInformation`'s table, what it read, its read.
const TABLE: u32 = 0;
const DATA: u32 = 4;
const HANDLE: u32 = 12;
const COUNT: u32 = 16;

/// The credits (`CreditManager`; +16 on).
const CREDITS: u32 = 0x8011_6014;
const CREDITS_ON: u32 = 16;
/// The menus' block (`SONGS_SCREEN`'s): +2592 the main menu, +14832 the credits', +2292 the
/// main menu's last item.
const MENUS: u32 = SONGS_SCREEN;
const MAIN_MENU: u32 = 2592;
const CREDITS_MENU: u32 = 14832;
const LAST_ITEM: u32 = 2292;
/// The front end's mode that shows the last item's picture (`frontEnd + 4`), its main screen's
/// number (`frontEnd + 0x340`).
const MODE: u32 = FRONT_END + 4;
const MAIN_NUMBER: u32 = FRONT_END + 0x340;
/// The video's directory (`Paths_Paths + 0xa4`).
const VIDEO_PATH: u32 = PATHS + 0xA4;

/// The video's wall: its 24 layouts (20 bytes each: a count, then that many pieces: the first
/// TV, the first row, the columns and rows of a tile, the tiles across and down), the
/// backgrounds' colours, the order the TVs turn off in, the swapped shapes' four places, the
/// rolling bits (25 bytes).
const LAYOUTS: u32 = 0x8005_2108;
const BACK_COLORS: u32 = 0x8005_1F48;
const TV_ORDER: u32 = 0x8005_1F38;
const SPOTS: u32 = 0x8005_1FF8;
const BITS: u32 = 0x8005_3020;
/// The names `GetShapeInfo` gives: the swapped file's, the permanent one's.
const ROLL_NAME: u32 = 0x8005_3040;
const MAIN_NAME: u32 = 0x8005_3060;
/// "zyVid%02d", "%szzzVD%02d.dct", "yVda%02d", "zMain%d".
const BACK_FORMAT: u32 = 0x8001_1828;
const CLIP_FORMAT: u32 = 0x8001_1834;
const ROLL_FORMAT: u32 = 0x8001_1844;
const MAIN_FORMAT: u32 = 0x8001_1850;

/// The videos (`VIDEO_create`, `_destroy`, `_spoolfile`, `_startplayback`, `_abortplayback`,
/// `_state`, `_updateframexy`), `ClearImage`, `DrawSync`, `DrawTV`,
/// `tScreenMain::DrawDropShadow`, `PSXDrawSquare`, `PSXDrawTransSquare`: the host's.
const VIDEO_CREATE: u32 = 0x8005_11B0;
const VIDEO_DESTROY: u32 = 0x8005_1328;
const VIDEO_SPOOL: u32 = 0x8005_1380;
const VIDEO_START: u32 = 0x8005_13DC;
const VIDEO_ABORT: u32 = 0x8005_143C;
const VIDEO_STATE: u32 = 0x8005_1480;
const VIDEO_UPDATE: u32 = 0x8005_1528;
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const DRAW_TV: u32 = 0x8002_24AC;
const DRAW_DROP_SHADOW: u32 = 0x8003_79FC;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `rand`, `GetClut`, `GetTPage`, `gettick`, `ScaleShapeExtended`, `FeAudio_systemtask`,
/// `purgememadr`.
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const GETTICK: fn(&mut Cpu) = nfs4rc::f_800e8e0c;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = nfs4rc::f_8004ef70;
const SYSTEM_TASK: fn(&mut Cpu) = nfs4rc::f_80015a70;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// Background `k`'s shapes, TV `k`, image `k`, background `k`'s number.
fn back(this: u32, k: u32) -> u32 {
    this.wrapping_add(BACKS).wrapping_add(k.wrapping_mul(40))
}

fn tv(this: u32, k: u32) -> u32 {
    this.wrapping_add(TVS).wrapping_add(k.wrapping_mul(TV_SIZE))
}

fn image(this: u32, k: u32) -> u32 {
    this.wrapping_add(IMAGES).wrapping_add(k.wrapping_mul(IMAGE_SIZE))
}

fn back_num(this: u32, k: u32) -> u32 {
    this.wrapping_add(BACK_NUMS).wrapping_add(k << 2)
}

/// Whether a shape file is still reading or read and not uploaded.
fn busy(m: &mut Mem, info: u32) -> bool {
    m.r32(info + HANDLE) != 0 || m.r32(info + DATA) != 0
}

/// The high word of `v * k` (the original's division by a constant leaves it in a register a
/// variadic `sprintf` homes).
fn hi(v: i32, k: u32) -> u32 {
    ((v as i64 * k as i32 as i64) >> 32) as u32
}

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `tScreenMain::SwapBackground` (SCREENMAIN.CPP, 0x80037118): the video stopped; unless either
/// background is still loading, background `which` (-1: a random one, not the other's) loaded
/// in place of the one shown, the other shown: its TVs turned on one by one.
pub fn swap_background(c: &mut Cpu, this: u32, which: u32) {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = which;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_ABORT, &[video], 0x8003_713c);
        let cur = c.m.r32(this + CUR);
        c.m.w32(this + ABORTED, 1);
        if busy(&mut c.m, back(this, cur)) || busy(&mut c.m, back(this, 1u32.wrapping_sub(cur))) {
            return;
        }
        let mut a3 = None;
        if which == u32::MAX {
            r.0[1] = 0x9249_2493;
            r.0[2] = 1;
            loop {
                let v = r.call_leaving(c, RAND, &[], 0x8003_71c4) as i32;
                a3 = Some(hi(v, 0x9249_2493));
                let cur = c.m.r32(this + CUR);
                c.m.w32(back_num(this, cur), v.wrapping_rem(28) as u32);
                let cur = c.m.r32(this + CUR);
                if c.m.r32(back_num(this, cur)) != c.m.r32(back_num(this, 1u32.wrapping_sub(cur))) {
                    break;
                }
            }
        } else {
            c.m.w32(back_num(this, cur), which);
        }
        let file = sp + 16;
        let cur = c.m.r32(this + CUR);
        let n = c.m.r32(back_num(this, cur));
        match a3 {
            Some(a3) => r.call_leaving(c, SPRINTF, &[file, BACK_FORMAT, n, a3], 0x8003_7258),
            None => r.call_leaving(c, SPRINTF, &[file, BACK_FORMAT, n], 0x8003_7258),
        };
        let cur = c.m.r32(this + CUR);
        let info = back(this, cur);
        r.enter_leaving(c, 0x8002_6260, 0x8003_727c, |c| fescreen::async_load_shape_file(c, this, file, info));
        let (state, cur) = (c.m.r32(this + STATE), c.m.r32(this + CUR));
        c.m.w32(this + STATE, 0);
        c.m.w32(this + CUR, 1u32.wrapping_sub(cur));
        let to = if state == 3 { 3 } else { 1 };
        r.enter_leaving(c, 0x8003_7350, 0x8003_72b4, |c| set_state(c, this, to));
        c.m.w32(this + TO_TURN, 16);
    })
}

pub fn abi_swap_background(c: &mut Cpu) {
    swap_background(c, c.r[4], c.r[5]);
}

/// `tScreenMain::DoneLoadingBackground` (SCREENMAIN.CPP, 0x800372d4): neither background still
/// loading.
pub fn abi_done_loading_background(c: &mut Cpu) {
    let this = c.r[4];
    let cur = c.m.r32(this + CUR);
    c.r[2] = !(busy(&mut c.m, back(this, cur)) || busy(&mut c.m, back(this, 1u32.wrapping_sub(cur)))) as u32;
}

/// `tScreenMain::SetState` (SCREENMAIN.CPP, 0x80037350): a new state from now (the video
/// stopped, the credits too unless they are it): each TV to show its slice of the background
/// shown; the last item's picture on the middle eight, the video's layout, or the credits on all.
pub fn set_state(c: &mut Cpu, this: u32, state: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[0] = state;
        if state == c.m.r32(this + STATE) {
            return;
        }
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_ABORT, &[video], 0x8003_7384);
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(this + ABORTED, 1);
        c.m.w32(this + ABORTED_AT, ticks);
        if state != 4 && c.m.r32(CREDITS + CREDITS_ON) == 1 {
            port(c, &r, 0x8003_5d90, 0x8003_73c0, &[CREDITS], fecredits::abi_de_init);
        }
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(this + STATE, state);
        c.m.w32(this + STATE_AT, ticks);
        for k in 0..16 {
            r.0[1] = k;
            let e = image(this, k);
            r.0[0] = this + IMAGE_SIZE * k;
            let m = &mut c.m;
            let cur = m.r32(this + CUR);
            let shape = m.r32(back(this, cur) + TABLE).wrapping_add(k << 5);
            m.w32(e + KIND, 1);
            let inside = (m.r16(shape + 24) as i16 as i32).wrapping_sub((m.r16(shape + 24) & 0xFFC0) as i16 as i32) << 4;
            let depth = m.r8(shape + 8) as i32;
            assert!(depth != 0, "SetState: a shape of depth 0 (the original's `break`)");
            m.w8(e + UV, (inside / depth) as u8);
            let v = m.r8(shape + 26);
            m.w8(e + UV + 1, v);
            let v = m.r8(shape + 16);
            m.w8(e + UV + 2, v);
            let v = m.r8(shape + 18);
            m.w8(e + UV + 3, v);
            let (bits, ty, tx) = (m.r8(shape + 9) as u32, m.r16(shape + 26) as u32, m.r16(shape + 24) as u32);
            let page = ((bits & 3) << 7) | ((ty & 0x100) >> 4) | ((tx & 0x3C0) >> 6) | ((ty & 0x200) << 2);
            m.w16(e + PAGE, page as u16);
            let clut = m.r32(shape + 4);
            let id = r.call_leaving(c, GET_CLUT, &[(clut & 0x3F) << 4, ((clut as i32) >> 6) as u32], 0x8003_74e8);
            let m = &mut c.m;
            m.w16(e + CLUT, id as u16);
            m.w16(e + FLAGS, 0);
            m.w32(e + COLOR, 0x0080_8080);
            m.w16(e + BRIGHT, 128);
        }
        r.0[1] = 16;
        match c.m.r32(this + STATE) {
            3 => {
                for k in 4..12 {
                    c.m.w16(image(this, k) + BRIGHT, 128);
                    c.m.w32(image(this, k) + KIND, 3);
                }
                c.m.w16(image(this, 6) + BRIGHT, 128);
                c.m.w16(image(this, 5) + BRIGHT, 128);
            }
            2 => {
                r.enter_leaving(c, 0x8003_7624, 0x8003_7560, |c| init_dynamic_images(c, this));
            }
            4 => {
                for k in 0..16 {
                    c.m.w16(image(this, k) + BRIGHT, 128);
                    c.m.w32(image(this, k) + KIND, 4);
                }
                let at = c.m.r32(this + STATE_AT);
                port(c, &r, 0x8003_5cb0, 0x8003_760c, &[CREDITS, at], fecredits::abi_init);
            }
            _ => {}
        }
    })
}

pub fn abi_set_state(c: &mut Cpu) {
    set_state(c, c.r[4], c.r[5]);
}

/// `tScreenMain::InitDynamicImages` (SCREENMAIN.CPP, 0x80037624): the video across the TVs in
/// one of 24 layouts: pieces of tiles, each tile's TVs showing their part of an 80 by 80 frame,
/// in the background's colour.
pub fn init_dynamic_images(c: &mut Cpu, this: u32) {
    abi::frame(c, 80, |c, sp| {
        let r = Saved::of(c);
        c.m.w32(sp + 16, this);
        let v = r.call_leaving(c, RAND, &[], 0x8003_765c) as i32;
        let layout = LAYOUTS.wrapping_add((v.wrapping_rem(24) as u32).wrapping_mul(20));
        let mut j = 0;
        while (j as i32) < lh(&mut c.m, layout) {
            let piece = c.m.r32(layout + 4 + 4 * j);
            let m = &mut c.m;
            let (first, first_row) = (m.r8(piece) as u32, m.r8(piece + 1) as u32);
            let (cols, rows) = (m.r8(piece + 2) as u32, m.r8(piece + 3) as u32);
            let (across, down) = (m.r8(piece + 4) as u32, m.r8(piece + 5) as u32);
            for x in 0..across {
                for y in 0..down {
                    for row in 0..rows {
                        for col in 0..cols {
                            let (w, h) = (80 / cols as i32, 80 / rows as i32);
                            let at = 4 * (row + first_row + y * rows) + col + first + x * cols;
                            let e = image(this, at);
                            let m = &mut c.m;
                            m.w8(e + UV, (col as i32).wrapping_mul(w) as u8);
                            m.w8(e + UV + 1, (row as i32).wrapping_mul(h) as u8);
                            m.w8(e + UV + 2, (w - 1) as u8);
                            m.w8(e + UV + 3, h as u8);
                            for (k, v) in [this, first, j, first_row, BACK_COLORS, LAYOUTS].into_iter().enumerate() {
                                m.w32(sp + 16 + 4 * k as u32, v);
                            }
                            let page = r.call_leaving(c, GET_TPAGE, &[2, 0, 512, 0], 0x8003_7890);
                            let m = &mut c.m;
                            m.w16(e + PAGE, page as u16);
                            m.w16(e + CLUT, 0);
                            m.w16(e + FLAGS, 34);
                            let cur = m.r32(this + CUR);
                            let n = m.r32(back_num(this, cur));
                            let color = m.r32(BACK_COLORS.wrapping_add(n << 2));
                            m.w32(e + KIND, 2);
                            m.w16(e + BRIGHT, 128);
                            m.w32(e + COLOR, color);
                        }
                    }
                }
            }
            j += 1;
        }
    })
}

pub fn abi_init_dynamic_images(c: &mut Cpu) {
    init_dynamic_images(c, c.r[4]);
}

/// `tScreenMain::ProcessInput` (SCREENMAIN.CPP, 0x80037990): start, past the first menu,
/// swaps the background (on the main menu, the front end's mode back to 0).
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8005_0000;
        if c.m.r32(key) != KEY_START {
            return;
        }
        let app = c.m.r32(FE_APP);
        if c.m.r32(app + 324) as i32 <= 0 {
            return;
        }
        r.enter_leaving(c, 0x8003_7118, 0x8003_79cc, |c| swap_background(c, this, u32::MAX));
        let app = c.m.r32(FE_APP);
        if c.m.r32(app + 4) == c.m.r32(MENUS).wrapping_add(MAIN_MENU) {
            c.m.w8(MODE, 0);
        }
    })
}

/// `tScreenMain::DrawVideoLines` (SCREENMAIN.CPP, 0x80037b10): the lines between the TVs,
/// darkening.
pub fn abi_draw_video_lines(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = 169;
        r.0[1] = 1;
        for x in (243..483).step_by(80) {
            r.0[0] = x;
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0030_3030, x, 42, 2, 169, 1], 0x8003_7b50);
        }
        for y in (84..210).step_by(42) {
            r.0[0] = y;
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 163, y, 321, 1, 1], 0x8003_7b88);
        }
        r.0[0] = 210;
        r.call_leaving(c, SET_ABR_MODE, &[2], 0x8003_7ba0);
    })
}

/// The main menu's selected item is its last.
fn on_last_item(m: &mut Mem) -> bool {
    let app = m.r32(FE_APP);
    let menu = m.r32(app + 4);
    let at = m.r32(menu + 8);
    m.r32(menu.wrapping_add(at << 2) + 16) == m.r32(MENUS).wrapping_add(LAST_ITEM)
}

/// `tScreenMain::DrawBackground` (SCREENMAIN.CPP, 0x80037bb8): a background read uploaded once
/// every TV shows the one before; the credits on their menu (and drawn); the last item's
/// picture faded in or out; the video's next frame, or a new video once the wall has stood ten
/// seconds, the background back when it ends; the background, the drop shadow and the lines; a
/// new swapped file every 25 seconds, shown for 13; the rolling bits; the TVs whose image changed
/// turned off one by one, the ones off turned on with their new one; the TVs and their frames.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 216, |c, sp| {
        let mut r = Saved::of(c);
        r.0[6] = this;
        r.0[7] = 0;
        r.0[3] = 1;
        r.0[1] = 4;
        for k in 0..2 {
            r.0[2] = k;
            r.0[0] = 40 * k;
            let info = back(this, k);
            r.enter_leaving(c, 0x8002_5e2c, 0x8003_7c20, |c| fescreen::is_shape_file_loaded(c, this, info));
            r.0[0] = this + 40 * k;
            if c.m.r32(info + DATA) == 0 {
                continue;
            }
            let mut shown = true;
            for j in 0..16 {
                let m = &mut c.m;
                let (kind, e) = (m.r32(this + SHOWN + 4 * j), image(this, j));
                if !((kind == 1 || kind == 4) && m.r32(e + KIND) == kind && m.r16(tv(this, j) + TV_CLUT) == m.r16(e + CLUT)) {
                    shown = false;
                }
            }
            r.0[0] = 16;
            if shown {
                c.m.w32(sp + 16, 16);
                c.m.w32(sp + 20, 0);
                r.enter_leaving(c, 0x8002_6614, 0x8003_7d00, |c| fescreen::upload_shapes(c, this, info, sx(166 * k), 0, 16, 0));
            }
        }
        r.0[2] = 2;
        // the credits on their menu
        let credits = {
            let m = &mut c.m;
            let app = m.r32(FE_APP);
            m.r32(app + 4) == m.r32(MENUS).wrapping_add(CREDITS_MENU)
        };
        let state = c.m.r32(this + STATE);
        let to = if state == 4 && !credits { Some(1) } else if credits { Some(4) } else { None };
        if let Some(to) = to {
            r.enter_leaving(c, 0x8003_7350, 0x8003_7d6c, |c| set_state(c, this, to));
        }
        let on = (c.m.r32(this + STATE) == 4) as u32;
        port(c, &r, 0x8003_5ddc, 0x8003_7d84, &[CREDITS, on], fecredits::abi_draw);
        // the last item's picture
        let last = c.m.r8(MODE) == 6 || on_last_item(&mut c.m);
        let to = if c.m.r32(this + STATE) == 3 { (!last).then_some(1) } else { last.then_some(3) };
        if let Some(to) = to {
            r.enter_leaving(c, 0x8003_7350, 0x8003_7e3c, |c| set_state(c, this, to));
        }
        let fade = lh(&mut c.m, this + FADE);
        let picture = if c.m.r32(this + STATE) != 3 {
            if fade > 0 {
                c.m.w16(this + FADE, (fade - 4) as u16);
            }
            fade > 0
        } else {
            if fade < 96 {
                c.m.w16(this + FADE, (fade + 4) as u16);
            }
            true
        };
        if picture {
            draw_picture(c, &mut r, this, sp);
        }
        // the video
        let y = if c.m.r32(this + FRAMES) & 1 != 0 { 0 } else { 80 };
        r.0[0] = y;
        let video = c.m.r32(this + VIDEO);
        if r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8003_8140) == 3 {
            let (video, ticks) = (c.m.r32(this + VIDEO), c.m.r32(GAME_TICKS));
            c.m.w32(this + ABORTED, 0);
            c.m.w32(this + ABORTED_AT, ticks);
            if r.call_ptr_leaving(c, VIDEO_UPDATE, &[video, 512, y], 0x8003_8168) != 0 {
                let n = c.m.r32(this + FRAMES).wrapping_add(1);
                c.m.w32(this + FRAMES, n);
            }
        } else {
            let video = c.m.r32(this + VIDEO);
            r.0[0] = 0;
            let state = r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8003_8190);
            let due = state != 1 && c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + STATE_AT)) >= 641;
            r.0[0] = due as u32;
            if due && c.m.r32(this + STATE) == 1 {
                start_video(c, &mut r, this, sp);
            }
        }
        if c.m.r32(this + STATE) == 2 {
            let video = c.m.r32(this + VIDEO);
            if r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8003_82ac) == 0 {
                r.enter_leaving(c, 0x8003_7350, 0x8003_82c0, |c| set_state(c, this, 1));
            }
        }
        c.m.w32(sp + 16, 0);
        let table = c.m.r32(this + TABLE);
        port(c, &r, 0x8002_5ce8, 0x8003_82d8, &[this, 2, 28, table], fescreen::abi_draw_background_image);
        r.call_ptr_leaving(c, DRAW_DROP_SHADOW, &[this], 0x8003_82e0);
        port(c, &r, 0x8003_7b10, 0x8003_82e8, &[this], abi_draw_video_lines);
        // the swapped shapes
        let since = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + SWAP_AT));
        r.0[1] = since;
        if since >= 1501 && c.m.r32(this + STATE) != 3 {
            r.0[1] = 0;
            let reading = c.m.r32(this + SWAP + HANDLE) != 0;
            c.m.w32(this + ROLLING, 0);
            if !reading {
                r.0[0] = 0x51EB_851F;
                let n = loop {
                    let v = r.call_leaving(c, RAND, &[], 0x8003_8338) as i32;
                    let n = v.wrapping_rem(25) as u32;
                    if sx(n) as i32 != lh(&mut c.m, this + ROLL) {
                        break n;
                    }
                };
                c.m.w16(this + ROLL, n as u16);
                let name = sp + 56;
                r.0[0] = name;
                r.call_leaving(c, SPRINTF, &[name, ROLL_FORMAT, sx(n)], 0x8003_8388);
                port(c, &r, 0x8002_5dd8, 0x8003_8394, &[this, name], fescreen::abi_async_load_swap_shape_file);
            }
        }
        r.enter_leaving(c, 0x8002_5e2c, 0x8003_83a0, |c| fescreen::is_shape_file_loaded(c, this, this + SWAP));
        if c.m.r32(this + SWAP + DATA) != 0 {
            c.m.w16(this + SWAP + COUNT, 10);
            port(c, &r, 0x8002_5ff0, 0x8003_83c0, &[this, 10], fescreen::abi_upload_swap_shapes);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w32(this + SWAP_AT, ticks);
            c.m.w32(this + ROLLING, 1);
            let spot = c.m.r16(this + SWAP_AT) & 3;
            c.m.w16(this + SPOT, spot);
        }
        let since = r.0[1];
        if since < 128 {
            r.0[7] = (since << 25).wrapping_sub(since).wrapping_add(128);
        } else if since.wrapping_sub(672) < 128 {
            r.0[7] = since - 672;
        }
        r.0[3] = 50;
        if since < 800 && c.m.r32(this + ROLLING) != 0 {
            let m = &mut c.m;
            let cur = m.r32(this + CUR);
            let n = m.r32(back_num(this, cur));
            let color = m.r32(BACK_COLORS.wrapping_add(n << 2));
            m.w32(sp + 36, color);
            let mut bright = sx(r.0[7]) as i32;
            let table = m.r32(this + SWAP + TABLE);
            m.w32(sp + 52, table);
            if m.r32(CREDITS + CREDITS_ON) != 0 {
                bright = (bright + 64).min(128);
            }
            let spot = SPOTS.wrapping_add((lh(m, this + SPOT) as u32) << 2);
            let (x, y) = (lh(m, spot) as u32, lh(m, spot + 2) as u32);
            let shape = since / 15 % 10;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 1553, x, y, bright as u32, 1, sp + 32], 0x8003_84f4);
        }
        // the rolling bits
        r.0[2] = 0;
        r.0[5] = BITS;
        loop {
            let row = sx(r.0[2]);
            if row as i32 >= 25 {
                break;
            }
            r.0[1] = 418;
            if sx(r.0[3]) as i32 >= 210 {
                break;
            }
            r.0[0] = 4;
            r.0[4] = row;
            loop {
                let bit = sx(r.0[0]) as i32;
                if bit < 0 {
                    break;
                }
                let ticks = c.m.r32(GAME_TICKS) as i32;
                let k = (row as i32).wrapping_add(ticks / 20).wrapping_rem(25);
                let (x, y) = (sx(r.0[1]), sx(r.0[3]));
                r.0[0] = r.0[0].wrapping_sub(1);
                r.0[1] = r.0[1].wrapping_add(13);
                let byte = c.m.r8(BITS.wrapping_add(k as u32)) as i32;
                r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[((byte >> bit) & 1) as u32, 1, x, y, 64, 3, 0], 0x8003_85d8);
            }
            r.0[3] = r.0[3].wrapping_add(8);
            r.0[2] = r.0[2].wrapping_add(1);
        }
        // the TVs whose image changed turned off, one more every 16 ticks
        let since = ((c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + STATE_AT)) << 12) as i32) >> 16;
        r.0[0] = 0;
        if since >= 0 {
            r.0[2] = TV_ORDER;
            r.0[1] = since as u32;
            loop {
                let k = sx(r.0[0]);
                if k as i32 >= 16 {
                    break;
                }
                let at = c.m.r8(TV_ORDER.wrapping_add(k)) as u32;
                let m = &mut c.m;
                let (e, t) = (image(this, at), tv(this, at));
                let same = m.r32(this + SHOWN + 4 * at) == m.r32(e + KIND) && m.r16(t + TV_CLUT) == m.r16(e + CLUT);
                if !same && m.r32(t + TV_STATE) == 1 {
                    port(c, &r, 0x8002_3378, 0x8003_86a0, &[t], fetv::abi_turn_off_tv);
                }
                r.0[0] = r.0[0].wrapping_add(1);
                if since < sx(r.0[0]) as i32 {
                    break;
                }
            }
        }
        // the TVs off turned on with their image
        for k in 0..16 {
            r.0[2] = k;
            let t = tv(this, k);
            r.0[0] = 3 * k;
            r.0[1] = this + 48 * k;
            if c.m.r32(t + TV_STATE) != 0 {
                continue;
            }
            let e = image(this, k);
            r.0[0] = this + IMAGE_SIZE * k;
            let m = &mut c.m;
            for f in 0..4 {
                let v = m.r8(e + UV + f);
                m.w8(t + TV_UV + f, v);
            }
            let v = m.r16(e + PAGE);
            m.w16(t + TV_PAGE, v);
            let v = m.r16(e + CLUT);
            m.w16(t + TV_CLUT, v);
            let v = m.r16(e + FLAGS);
            m.w16(t + TV_FLAGS, v);
            let v = m.r32(e + COLOR);
            m.w32(t + TV_COLOR, v);
            let v = m.r32(e + KIND);
            m.w32(this + SHOWN + 4 * k, v);
            port(c, &r, 0x8002_3388, 0x8003_8764, &[t], fetv::abi_turn_on_tv);
            let m = &mut c.m;
            let v = m.r16(e + BRIGHT);
            m.w16(t + TV_BRIGHT, v);
            let n = m.r32(this + TO_TURN).wrapping_sub(1);
            m.w32(this + TO_TURN, n);
        }
        for k in 0..16 {
            r.0[2] = k;
            r.call_ptr_leaving(c, DRAW_TV, &[tv(this, k)], 0x8003_87c0);
        }
        // the TVs' frames
        r.0[0] = 169;
        for x in (243..483).step_by(80) {
            r.0[2] = x;
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0x0040_4040, x, 42, 2, 169], 0x8003_8804);
        }
        r.0[0] = 1;
        for y in (84..210).step_by(42) {
            r.0[2] = y;
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0x0030_3030, 163, y, 321, 1], 0x8003_8848);
        }
    })
}

/// [`abi_draw_background`]'s last item's picture: the middle TVs dimmed by its fade (two of them
/// less), two flickering shapes and a bar, a turning one, and its text.
fn draw_picture(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) {
    r.0[2] = 4;
    for k in 4..12 {
        let m = &mut c.m;
        let t = tv(this, k);
        let flags = m.r16(t + TV_FLAGS) | 2;
        m.w16(t + TV_FLAGS, flags);
        let f = 128i32.wrapping_sub(lh(m, this + FADE)) as u32;
        m.w32(t + TV_COLOR, (f << 16) | (f << 8) | f);
    }
    r.0[2] = 12;
    let g = (128 - (lh(&mut c.m, this + FADE) << 6) / 96) as u32;
    let gray = (g << 16) | (g << 8) | g;
    c.m.w32(tv(this, 6) + TV_COLOR, gray);
    c.m.w32(tv(this, 5) + TV_COLOR, gray);
    r.0[5] = 0x2AAA_AAAB;
    r.0[4] = 0x8888_8889;
    c.m.w32(sp + 36, 0xBEBE);
    let fade = |c: &mut Cpu| 96i32.wrapping_sub(lh(&mut c.m, this + FADE)) as u32;
    let t = r.call_leaving(c, GETTICK, &[], 0x8003_7f34) as i32;
    r.0[3] = 0x6666_6667;
    r.0[1] = 1;
    r.0[0] = sp + 32;
    r.0[2] = 96;
    let f = fade(c);
    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[(257 + t / 15 % 10) as u32, 1041, 163, (-15i32) as u32, f, 1, sp + 32], 0x8003_7fac);
    let t = r.call_leaving(c, GETTICK, &[], 0x8003_7fb4) as i32;
    let f = fade(c);
    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[(257 + t / 15 % 10) as u32, 1045, 403, (-15i32) as u32, f, 1, sp + 32], 0x8003_8018);
    let f = fade(c);
    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[256, 17, 0, 0, f, 1, sp + 32], 0x8003_8040);
    let t = r.call_leaving(c, GETTICK, &[], 0x8003_8048) as i32;
    let f = fade(c);
    r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[(250 + t / 21 % 6) as u32, 1, 0, 0, f, 3, 0], 0x8003_80b0);
    r.enter_leaving(c, 0x8002_1b48, 0x8003_80bc, |c| fetextrender::set_abr(c, 1, 1));
    let f = fade(c);
    let color = port(c, r, 0x8004_bd68, 0x8003_80cc, &[0xBEBE, f], fefades::abi_fade_out);
    r.0[0] = color;
    let rect = sp + 88;
    for (k, v) in [181u16, 129, 284, 42].into_iter().enumerate() {
        c.m.w16(rect + 2 * k as u32, v);
    }
    let word = r.call_leaving(c, TEXT_WORD, &[627], 0x8003_80f8);
    r.enter_leaving(c, 0x8002_1878, 0x8003_8108, |c| fetextrender::word_wrap_text_rgb(c, word, rect, color));
    r.enter_leaving(c, 0x8002_1b48, 0x8003_8114, |c| fetextrender::set_abr(c, 0, 0));
    r.call_leaving(c, SET_ABR_MODE, &[0], 0x8003_811c);
}

/// [`abi_draw_background`]'s new video: its frame's place cleared, one other than the last
/// spooled and played.
fn start_video(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) {
    let rect = sp + 88;
    for (k, v) in [512u16, 0, 80, 160].into_iter().enumerate() {
        c.m.w16(rect + 2 * k as u32, v);
    }
    r.call_ptr_leaving(c, CLEAR_IMAGE, &[rect, 0, 0, 0], 0x8003_81f8);
    r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8003_8200);
    r.0[0] = 0x6666_6667;
    let n = loop {
        let v = r.call_leaving(c, RAND, &[], 0x8003_8210) as i32;
        let n = v.wrapping_rem(20) as u32;
        c.m.w16(this + NEXT_CLIP, n as u16);
        if sx(n) as i32 != lh(&mut c.m, this + CLIP) {
            break n;
        }
    };
    c.m.w16(this + CLIP, n as u16);
    r.enter_leaving(c, 0x8003_7350, 0x8003_8258, |c| set_state(c, this, 2));
    let file = sp + 96;
    r.0[0] = file;
    let (path, n) = (c.m.r32(VIDEO_PATH), lh(&mut c.m, this + NEXT_CLIP) as u32);
    r.call_leaving(c, SPRINTF, &[file, CLIP_FORMAT, path, n], 0x8003_8278);
    let video = c.m.r32(this + VIDEO);
    r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8003_8284);
    let video = c.m.r32(this + VIDEO);
    r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8003_828c);
}

/// `tScreenMain::GetShapeInfo` (SCREENMAIN.CPP, 0x80038890): 267 shapes of the main screen's
/// file, 10 of the swapped one's.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(permanent, 267);
    m.w32(name, MAIN_NAME);
    m.w16(swap, 10);
    m.w32(swap_name, ROLL_NAME);
}

/// `tScreenMain::PreLoad` (SCREENMAIN.CPP, 0x800388c0): the main screen's file and a random
/// swapped one named, then read.
pub fn abi_pre_load(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let n = c.m.r8(MAIN_NUMBER) as u32;
        r.call_leaving(c, SPRINTF, &[MAIN_NAME, MAIN_FORMAT, n], 0x8003_88ec);
        let v = r.call_leaving(c, RAND, &[], 0x8003_88f4) as i32;
        let n = v.wrapping_rem(25) as u32;
        c.m.w16(this + ROLL, n as u16);
        r.call_leaving(c, SPRINTF, &[ROLL_NAME, ROLL_FORMAT, sx(n), hi(v, 0x51EB_851F)], 0x8003_8944);
        port(c, &r, 0x8002_67a0, 0x8003_894c, &[this], fescreen::abi_pre_load);
    })
}

/// `tScreenMain::Initialize` (SCREENMAIN.CPP, 0x8003895c): the screen's files read; two
/// different random backgrounds read and uploaded (the sound kept going); the video made; the
/// TVs set from the first background, showing nothing yet; the bits rolled.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        r.0[4] = 0;
        port(c, &r, 0x8002_60cc, 0x8003_8988, &[this], fescreen::abi_initialize);
        for k in 0..2 {
            r.0[2] = k;
            let info = back(this, k);
            c.m.w32(info + TABLE, 0);
            r.enter_leaving(c, 0x8002_640c, 0x8003_89bc, |c| fescreen::initialize_shapes(c, info, 16));
        }
        r.0[2] = 2;
        r.0[0] = 0x9249_0000;
        let v = r.call_leaving(c, RAND, &[], 0x8003_89e0) as i32;
        r.0[0] = 0x9249_2493;
        c.m.w32(back_num(this, 0), v.wrapping_rem(28) as u32);
        let w = r.call_leaving(c, RAND, &[], 0x8003_8a14) as i32;
        let n = (c.m.r32(back_num(this, 0)) as i32).wrapping_add(w.wrapping_rem(27)).wrapping_add(1);
        c.m.w32(back_num(this, 1), n.wrapping_rem(28) as u32);
        r.0[5] = 0x8001_0000;
        let file = sp + 24;
        for k in 0..2 {
            r.0[2] = k;
            r.0[1] = k;
            r.0[0] = 4 * k;
            let n = c.m.r32(back_num(this, k));
            r.call_leaving(c, SPRINTF, &[file, BACK_FORMAT, n], 0x8003_8aa4);
            r.0[0] = BACKS + 40 * k;
            let info = back(this, k);
            r.enter_leaving(c, 0x8002_6260, 0x8003_8ac0, |c| fescreen::async_load_shape_file(c, this, file, info));
        }
        r.0[2] = 2;
        let (first, second) = (back(this, 0), back(this, 1));
        r.0[0] = first;
        r.0[1] = 16;
        loop {
            r.call_leaving(c, SYSTEM_TASK, &[0], 0x8003_8af4);
            let loaded = r.enter_leaving(c, 0x8002_5e2c, 0x8003_8b00, |c| fescreen::is_shape_file_loaded(c, this, first));
            r.0[4] = loaded;
            if c.m.r32(first + DATA) != 0 {
                c.m.w32(sp + 16, 16);
                c.m.w32(sp + 20, 0);
                r.enter_leaving(c, 0x8002_6614, 0x8003_8b2c, |c| fescreen::upload_shapes(c, this, first, 0, 0, 16, 0));
            }
            let mut both = 0;
            if loaded != 0 {
                both = (r.enter_leaving(c, 0x8002_5e2c, 0x8003_8b40, |c| fescreen::is_shape_file_loaded(c, this, second)) != 0) as u32;
            }
            r.0[4] = both;
            if c.m.r32(second + DATA) != 0 {
                c.m.w32(sp + 16, 16);
                c.m.w32(sp + 20, 0);
                r.enter_leaving(c, 0x8002_6614, 0x8003_8b70, |c| fescreen::upload_shapes(c, this, second, 166, 0, 16, 0));
            }
            if both != 0 {
                break;
            }
        }
        c.m.w16(this + CLIP, 0xFFFF);
        c.m.w32(this + FRAMES, 0);
        let video = r.call_ptr_leaving(c, VIDEO_CREATE, &[80, 80, 0xF_0000, 0x2_0000, 16], 0x8003_8b9c);
        r.0[2] = 0;
        let m = &mut c.m;
        m.w32(this + VIDEO, video);
        let ticks = m.r32(GAME_TICKS);
        m.w32(this + ABORTED, 0);
        m.w8(this + STARTED, 1);
        m.w32(this + ROLLING, 0);
        m.w16(this + FADE, 0);
        m.w32(this + TO_TURN, 0);
        m.w32(this + CUR, 0);
        m.w32(this + STATE_AT, ticks);
        m.w32(this + SWAP_AT, ticks.wrapping_sub(800));
        for k in 0..16 {
            r.0[2] = k;
            let cur = c.m.r32(this + CUR);
            let table = c.m.r32(back(this, cur) + TABLE);
            r.enter_leaving(c, 0x8002_3168, 0x8003_8c10, |c| fetv::init_tv(c, tv(this, k), table, k));
        }
        r.0[2] = 16;
        c.m.w32(this + STATE, 0);
        r.enter_leaving(c, 0x8003_7350, 0x8003_8c3c, |c| set_state(c, this, 1));
        for k in 0..16 {
            c.m.w32(this + SHOWN + 4 * k, 0);
        }
        r.0[0] = BITS;
        for k in 0..25 {
            r.0[2] = k;
            let v = r.call_leaving(c, RAND, &[], 0x8003_8c80);
            c.m.w8(BITS + k, v as u8);
        }
    })
}

/// `tScreenMain::Cleanup` (SCREENMAIN.CPP, 0x80038cd0): the video destroyed and freed, the
/// backgrounds' shapes freed, then the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[1] = 0;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_DESTROY, &[video], 0x8003_8cf4);
        r.0[0] = BACKS;
        let video = c.m.r32(this + VIDEO);
        r.call_leaving(c, PURGE, &[video], 0x8003_8d00);
        for k in 0..2 {
            r.0[1] = k;
            r.0[0] = BACKS + 40 * k;
            r.enter_leaving(c, 0x8002_64b4, 0x8003_8d0c, |c| fescreen::free_shapes(c, this, back(this, k)));
        }
        r.0[1] = 2;
        r.0[0] = BACKS + 80;
        port(c, &r, 0x8002_61c0, 0x8003_8d24, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenMain::~tScreenMain` (SCREENMAIN.CPP, 0x80038d3c): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8003_8d4c, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, TOP, pick, player, via_abi};
    use crate::globals::SHAPES;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram, same_stack_in};

    /// The main screen (the front end's own, left as its `Cleanup` left it: no shapes); the
    /// outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8017_DA88;
    const OUT: u32 = 0x8014_7000;
    const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
    const TABLE_NAME: u32 = 0x8001_13B4;
    const PERMANENT_SHAPES: u32 = 0x8005_1C54;

    /// The car select's screen (`gFEApp + 12`): its swapped file's shapes (11, the file still
    /// in its buffer).
    fn real(m: &mut Mem) -> u32 {
        let app = m.r32(FE_APP);
        m.r32(app + 12)
    }

    /// A background's shapes: 16 made from the help's, their places, sizes, depths and texture
    /// bits shuffled (none of depth 0: the original's division would `break`), in a table of
    /// the heap's.
    fn background(c: &mut Cpu, rng: &mut Rng) -> u32 {
        let table = crate::abi::call(c, RESERVE, &[TABLE_NAME, 16 * 32, 0]);
        let m = &mut c.m;
        let help = m.r32(SHAPES);
        for k in 0..16 * 32 {
            let b = m.r8(help + k % (8 * 32));
            m.w8(table + k, b);
        }
        for k in 0..16 {
            let s = table + 32 * k;
            m.w8(s + 8, [4, 8, 16, 1 + pick(rng, 255)][pick(rng, 4) as usize] as u8);
            m.w8(s + 9, pick(rng, 256) as u8);
            for f in [16, 18, 20, 22, 24, 26] {
                m.w16(s + f, pick(rng, 0x1_0000) as u16);
            }
            m.w32(s + 4, rng.next() as u32);
        }
        table
    }

    /// The main screen's own shapes (267, the help's over and over) in a table of the heap's,
    /// the permanent shapes' now (as its `Initialize` leaves them): the shapes it draws past
    /// the car select's 142 would be any size, their primitives over everything.
    fn permanent(c: &mut Cpu) -> u32 {
        let table = crate::abi::call(c, RESERVE, &[TABLE_NAME, 267 * 32, 0]);
        let m = &mut c.m;
        let help = m.r32(SHAPES);
        for k in 0..267 * 32 {
            let b = m.r8(help + k % (8 * 32));
            m.w8(table + k, b);
        }
        m.w32(PERMANENT_SHAPES, table);
        table
    }

    /// FEMENU's structures, then the main screen in any state: its backgrounds' shapes (one
    /// now and then read, to upload), the swapped shapes (read or not), the times its state
    /// and the swapped shapes came about each limit, its fade, video and numbers; its TVs on or
    /// off, showing their images or others; the video's answers; the credits on or not; the
    /// player's menu the main, the credits' or the test's (on its last item or not); the clock.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let tables = [background(c, rng), background(c, rng)];
        let permanent = permanent(c);
        let m = &mut c.m;
        let from = real(m);
        let (swap, buffer) = (m.r32(from + SWAP + TABLE), m.r32(from + SWAP + 8));
        let s = SCREEN;
        m.w32(s + TABLE, permanent);
        m.w32(s + SWAP + TABLE, swap);
        m.w32(s + SWAP + 8, buffer);
        m.w32(s + SWAP + DATA, [0, buffer][pick(rng, 2) as usize]);
        for (k, table) in tables.into_iter().enumerate() {
            let info = back(s, k as u32);
            m.w32(info + TABLE, table);
            let read = pick(rng, 3) == 0;
            m.w32(info + 8, if read { buffer } else { 0 });
            m.w32(info + DATA, if read { buffer } else { 0 });
        }
        m.w32(s + CUR, pick(rng, 2));
        for k in 0..2 {
            m.w32(back_num(s, k), pick(rng, 28));
        }
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        m.w32(s + STATE, [0, 1, 1, 1, 2, 3, 4, 5][pick(rng, 8) as usize]);
        let ago = [0, 100, 640, 641, 700, 4000, pick(rng, 5000)][pick(rng, 7) as usize];
        m.w32(s + STATE_AT, ticks.wrapping_sub(ago));
        let ago = [0, 127, 128, 672, 700, 799, 800, 1500, 1501, 3000, pick(rng, 2000)][pick(rng, 11) as usize];
        m.w32(s + SWAP_AT, ticks.wrapping_sub(ago));
        m.w16(s + SPOT, pick(rng, 4) as u16);
        m.w32(s + ROLLING, pick(rng, 2));
        m.w16(s + ROLL, pick(rng, 25) as u16);
        m.w16(s + FADE, [0, 4, 92, 95, 96, pick(rng, 97)][pick(rng, 6) as usize] as u16);
        m.w16(s + CLIP, [0xFFFF, pick(rng, 20)][pick(rng, 2) as usize] as u16);
        m.w32(s + FRAMES, pick(rng, 100));
        m.w32(s + TO_TURN, pick(rng, 17));
        for k in 0..16 {
            let (t, e) = (tv(s, k), image(s, k));
            m.w32(t + TV_STATE, [0, 1, 1, 2, 3][pick(rng, 5) as usize]);
            m.w32(e + KIND, 1 + pick(rng, 4));
            let shown = if pick(rng, 3) == 0 { pick(rng, 5) } else { m.r32(e + KIND) };
            m.w32(s + SHOWN + 4 * k, shown);
            m.w16(e + CLUT, pick(rng, 0x1_0000) as u16);
            let clut = if pick(rng, 3) == 0 { pick(rng, 0x1_0000) as u16 } else { m.r16(e + CLUT) };
            m.w16(t + TV_CLUT, clut);
        }
        // (now and then every TV showing its image of the background or the credits, so a
        // background read is uploaded)
        if pick(rng, 3) == 0 {
            let kind = [1, 4][pick(rng, 2) as usize];
            for k in 0..16 {
                let (t, e) = (tv(s, k), image(s, k));
                m.w32(e + KIND, kind);
                m.w32(s + SHOWN + 4 * k, kind);
                let clut = m.r16(e + CLUT);
                m.w16(t + TV_CLUT, clut);
            }
        }
        m.w32(VIDEO_ANSWERS, [0, 1, 2, 3, 3][pick(rng, 5) as usize]);
        m.w32(VIDEO_ANSWERS + 4, pick(rng, 2));
        m.w32(VIDEO_ANSWERS + 8, 0x8018_7C08);
        m.w32(CREDITS + CREDITS_ON, (pick(rng, 4) == 0) as u32);
        let app = m.r32(FE_APP);
        let menus = m.r32(MENUS);
        m.w32(app + 4, [MENU, menus + MAIN_MENU, menus + CREDITS_MENU][pick(rng, 3) as usize]);
        m.w32(app + 324, pick(rng, 3));
        if pick(rng, 2) == 0 {
            let at = m.r32(MENU + 8);
            m.w32(MENU + 16 + 4 * at, menus + LAST_ITEM);
        }
        m.w8(MODE, [0, 6, pick(rng, 8)][pick(rng, 3) as usize] as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("SwapBackground", 0x8003_7118, nfs4rc::f_80037118, |_, r| vec![SCREEN, [u32::MAX, u32::MAX, pick(r, 28)][pick(r, 3) as usize]]),
            ("DoneLoadingBackground", 0x8003_72d4, nfs4rc::f_800372d4, |_, _| vec![SCREEN]),
            ("SetState", 0x8003_7350, nfs4rc::f_80037350, |_, r| vec![SCREEN, pick(r, 6)]),
            ("InitDynamicImages", 0x8003_7624, nfs4rc::f_80037624, |_, _| vec![SCREEN]),
            ("tScreenMain::ProcessInput", 0x8003_7990, nfs4rc::f_80037990, |c, r| {
                if pick(r, 2) == 0 {
                    c.m.w32(KEY, KEY_START);
                }
                vec![SCREEN, player(r), KEY, COMMAND]
            }),
            ("DrawVideoLines", 0x8003_7b10, nfs4rc::f_80037b10, |_, _| vec![SCREEN]),
            ("tScreenMain::DrawBackground", 0x8003_7bb8, nfs4rc::f_80037bb8, |_, _| vec![SCREEN]),
            ("tScreenMain::GetShapeInfo", 0x8003_8890, nfs4rc::f_80038890, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenMain::PreLoad", 0x8003_88c0, nfs4rc::f_800388c0, |_, _| vec![SCREEN]),
            ("tScreenMain::Initialize", 0x8003_895c, nfs4rc::f_8003895c, |c, r| {
                // (the backgrounds' new tables where two of shapes were: the CD reads nothing)
                let tables = [background(c, r), background(c, r)];
                for table in tables.into_iter().rev() {
                    crate::abi::call(c, PURGE, &[table]);
                }
                vec![SCREEN]
            }),
            ("tScreenMain::Cleanup", 0x8003_8cd0, nfs4rc::f_80038cd0, |_, _| vec![SCREEN]),
            ("~tScreenMain", 0x8003_8d3c, nfs4rc::f_80038d3c, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screenmain_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x5A1 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x5A2 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screenmain_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x5B2 + k as u64, shuffle, &args);
        }
    }
}
