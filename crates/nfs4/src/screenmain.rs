//! SCREENMAIN.CPP: the main menu's screen (`tScreenMain`). A wall of 16 TVs over the background
//! shows one of:
//! - slices of one of two background pictures (one shown, the other loading);
//! - a video played across them (`zzzVD*.dct`, now and then, once the wall has stood a while);
//! - the credits.
//!
//! On the menu's last item, a picture with a text goes over the middle TVs. A little screen is
//! swapped every 25 seconds, a column of bits rolls by, and the drop shadow and the lines
//! between the TVs are drawn. Back, past the first menu, swaps the background.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The videos,
//! `ClearImage`, `DrawSync`, `DrawTV`, the drop shadow and the squares are the host's (called by
//! address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenu::KEY_BACK;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE, SPRINTF, TEXT_WORD};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN};
use crate::types::{DrawShapeExtended, FEApplication, RECT, Screen, ShapeInformation, TVConfig, TextureShapeInfo};
use crate::{fefades, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// A main screen: a `tScreen`, then +100 its video, +104 the video's frames shown, +108 the
/// tick its state began, +112 the tick the swapped shapes came, +116 where they are (0..3),
/// +120 its state, +124 the 16 TVs (`tTVConfig`s), +892 what each TV shows (an image's kind),
/// +956 the 16 images to show (24 bytes each), +1344 started, +1348 the swapped shapes up, +1352
/// the swapped file's number, +1354 the last item's picture's fade (0..96), +1356 the video's
/// number and +1358 the next's, +1360 a video stopped and +1364 when, +1368 the two
/// backgrounds' shapes (`tShapeInformation`s), +1448 the one shown, +1452 the two backgrounds'
/// numbers, +1460 the TVs left to turn on.
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
/// The states (`tScreenMainState`): the background, the video, the last item's picture, the
/// credits. They are also what an image shows.
const BACKGROUND: u32 = 1;
const VIDEO_STATE_PLAYING: u32 = 2;
const PICTURE: u32 = 3;
const CREDITS_STATE: u32 = 4;
/// A TV's states (off, on).
const TV_OFF: i32 = 0;
const TV_ON: i32 = 1;

/// An image: +0 its kind (a state's), +4 the TV's flags, +6 its texture's corner and size (u,
/// v, w, h), +10 its CLUT, +12 its page, +16 its colour, +20 its brightness.
const IMAGE_SIZE: u32 = 24;
const KIND: u32 = 0;
const FLAGS: u32 = 4;
const UV: u32 = 6;
const CLUT: u32 = 10;
const PAGE: u32 = 12;
const COLOR: u32 = 16;
const BRIGHT: u32 = 20;

/// The credits (`CreditManager`; +16 on) and FECREDITS' `Init`, `DeInit`, `Draw` (ported,
/// called by address).
const CREDITS: u32 = 0x8011_6014;
const CREDITS_ON: u32 = 16;
const CREDITS_INIT: u32 = 0x8003_5CB0;
const CREDITS_DE_INIT: u32 = 0x8003_5D90;
const CREDITS_DRAW: u32 = 0x8003_5DDC;
/// The menus' block (`SONGS_SCREEN`'s): +2592 the main menu, +14832 the credits', +2292 the
/// main menu's last item.
const MENUS: u32 = SONGS_SCREEN;
const MAIN_MENU: u32 = 2592;
const CREDITS_MENU: u32 = 14832;
const LAST_ITEM: u32 = 2292;
/// The front end's mode that shows the last item's picture (`frontEnd + 4`, 6), its main
/// screen's number (`frontEnd + 0x340`).
const MODE: u32 = FRONT_END + 4;
const PICTURE_MODE: u8 = 6;
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
const BIT_ROWS: u32 = 25;
/// The names `GetShapeInfo` gives: the swapped file's, the permanent one's; the backgrounds
/// (28, 16 shapes each), the videos (20), the swapped files (25, 10 shapes each).
const ROLL_NAME: u32 = 0x8005_3040;
const MAIN_NAME: u32 = 0x8005_3060;
const SHAPES: u16 = 267;
const BACK_COUNT: i32 = 28;
const BACK_SHAPES: u32 = 16;
const VIDEO_COUNT: i32 = 20;
const ROLL_COUNT: i32 = 25;
const ROLL_SHAPES: u32 = 10;
/// "zyVid%02d", "%szzzVD%02d.dct", "yVda%02d", "zMain%d".
const BACK_FORMAT: u32 = 0x8001_1828;
const CLIP_FORMAT: u32 = 0x8001_1834;
const ROLL_FORMAT: u32 = 0x8001_1844;
const MAIN_FORMAT: u32 = 0x8001_1850;
/// The last item's text.
const PICTURE_TEXT: u32 = 627;

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
const VIDEO_STOPPED: u32 = 1;
const VIDEO_PLAYING: u32 = 3;
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const DRAW_TV: u32 = 0x8002_24AC;
const DRAW_DROP_SHADOW: u32 = 0x8003_79FC;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `rand`, `GetClut`, `GetTPage`, `gettick`, `ScaleShapeExtended`, `FeAudio_systemtask`,
/// `purgememadr`; FETV's `InitTV`, `TurnOnTV`, `TurnOffTV` (ported, called by address).
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const GETTICK: fn(&mut Cpu) = nfs4rc::f_800e8e0c;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const INIT_TV: u32 = 0x8002_3168;
const TURN_ON_TV: u32 = 0x8002_3388;
const TURN_OFF_TV: u32 = 0x8002_3378;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// Background `k`'s shapes, TV `k`, image `k`, background `k`'s number.
fn back(this: u32, k: u32) -> ShapeInformation {
    ShapeInformation(this.wrapping_add(BACKS).wrapping_add(k.wrapping_mul(40)))
}

fn tv(this: u32, k: u32) -> TVConfig {
    TVConfig(this.wrapping_add(TVS)).at(k)
}

fn image(this: u32, k: u32) -> u32 {
    this.wrapping_add(IMAGES).wrapping_add(k.wrapping_mul(IMAGE_SIZE))
}

fn back_num(this: u32, k: u32) -> u32 {
    this.wrapping_add(BACK_NUMS).wrapping_add(k << 2)
}

/// The background shown, and the other.
fn cur(m: &mut Mem, this: u32) -> u32 {
    m.r32(this + CUR)
}

/// The background shown's colour.
fn back_color(m: &mut Mem, this: u32) -> u32 {
    let cur = cur(m, this);
    let n = m.r32(back_num(this, cur));
    m.r32(BACK_COLORS.wrapping_add(n << 2))
}

/// Whether a shape file is still reading or read and not uploaded.
fn busy(m: &mut Mem, info: ShapeInformation) -> bool {
    info.async_handle(m) != 0 || info.file(m) != 0
}

/// Either background still loading.
fn loading(m: &mut Mem, this: u32) -> bool {
    let cur = cur(m, this);
    busy(m, back(this, cur)) || busy(m, back(this, 1u32.wrapping_sub(cur)))
}

/// A random number below `n`.
fn rand_below(c: &mut Cpu, n: i32) -> u32 {
    (call(c, RAND, &[]) as i32).wrapping_rem(n) as u32
}

/// `tScreenMain::SwapBackground` (SCREENMAIN.CPP, 0x80037118): the video stopped. Unless either
/// background is still loading, background `which` (-1: a random one, not the other's) is
/// loaded in place of the one shown, and the other is shown, its TVs turned on one by one.
pub fn swap_background(c: &mut Cpu, this: u32, which: u32) {
    let video = c.m.r32(this + VIDEO);
    call_addr(c, VIDEO_ABORT, &[video]);
    c.m.w32(this + ABORTED, 1);
    if loading(&mut c.m, this) {
        return;
    }
    if which == u32::MAX {
        loop {
            let n = rand_below(c, BACK_COUNT);
            let m = &mut c.m;
            let cur = cur(m, this);
            m.w32(back_num(this, cur), n);
            let cur = self::cur(m, this);
            if m.r32(back_num(this, cur)) != m.r32(back_num(this, 1u32.wrapping_sub(cur))) {
                break;
            }
        }
    } else {
        let cur = cur(&mut c.m, this);
        c.m.w32(back_num(this, cur), which);
    }
    with_buffer(c, 64, |c, file| {
        let cur = cur(&mut c.m, this);
        let n = c.m.r32(back_num(this, cur));
        call(c, SPRINTF, &[file, BACK_FORMAT, n]);
        let cur = self::cur(&mut c.m, this);
        fescreen::async_load_shape_file(c, Screen(this), file, back(this, cur));
    });
    let m = &mut c.m;
    let (state, cur) = (m.r32(this + STATE), cur(m, this));
    m.w32(this + STATE, 0);
    m.w32(this + CUR, 1u32.wrapping_sub(cur));
    set_state(c, this, if state == PICTURE { PICTURE } else { BACKGROUND });
    c.m.w32(this + TO_TURN, 16);
}

pub fn abi_swap_background(c: &mut Cpu) {
    swap_background(c, c.r[4], c.r[5]);
}

/// `tScreenMain::DoneLoadingBackground` (SCREENMAIN.CPP, 0x800372d4): neither background still
/// loading.
pub fn done_loading_background(m: &mut Mem, this: u32) -> bool {
    !loading(m, this)
}

pub fn abi_done_loading_background(c: &mut Cpu) {
    c.r[2] = done_loading_background(&mut c.m, c.r[4]) as u32;
}

/// `tScreenMain::SetState` (SCREENMAIN.CPP, 0x80037350): a new state from now (the video
/// stopped, the credits too unless they are it). Each TV is to show its slice of the
/// background shown; then the last item's picture goes on the middle eight, the video's layout,
/// or the credits on all.
pub fn set_state(c: &mut Cpu, this: u32, state: u32) {
    if state == c.m.r32(this + STATE) {
        return;
    }
    let video = c.m.r32(this + VIDEO);
    call_addr(c, VIDEO_ABORT, &[video]);
    let ticks = c.m.r32(GAME_TICKS);
    c.m.w32(this + ABORTED, 1);
    c.m.w32(this + ABORTED_AT, ticks);
    if state != CREDITS_STATE && c.m.r32(CREDITS + CREDITS_ON) == 1 {
        call_addr(c, CREDITS_DE_INIT, &[CREDITS]);
    }
    let ticks = c.m.r32(GAME_TICKS);
    c.m.w32(this + STATE, state);
    c.m.w32(this + STATE_AT, ticks);
    for k in 0..16 {
        let e = image(this, k);
        let m = &mut c.m;
        let cur = cur(m, this);
        let shape = back(this, cur).shapes(m).at(k).0;
        m.w32(e + KIND, BACKGROUND);
        // the slice's texture: its corner within its page (in its depth's pixels), its size,
        // its page and CLUT
        let tx = m.r16(shape + 24);
        let inside = ((tx as i16 as i32).wrapping_sub((tx & 0xFFC0) as i16 as i32)) << 4;
        let depth = m.r8(shape + 8) as i32;
        assert!(depth != 0, "SetState: a shape of depth 0 (the original's `break`)");
        m.w8(e + UV, (inside / depth) as u8);
        for (f, at) in [(1, 26), (2, 16), (3, 18)] {
            let v = m.r8(shape + at);
            m.w8(e + UV + f, v);
        }
        let (bits, ty, tx) = (m.r8(shape + 9) as u32, m.r16(shape + 26) as u32, tx as u32);
        let page = ((bits & 3) << 7) | ((ty & 0x100) >> 4) | ((tx & 0x3C0) >> 6) | ((ty & 0x200) << 2);
        m.w16(e + PAGE, page as u16);
        let clut = m.r32(shape + 4);
        let id = call(c, GET_CLUT, &[(clut & 0x3F) << 4, ((clut as i32) >> 6) as u32]);
        let m = &mut c.m;
        m.w16(e + CLUT, id as u16);
        m.w16(e + FLAGS, 0);
        m.w32(e + COLOR, 0x0080_8080);
        m.w16(e + BRIGHT, 128);
    }
    match c.m.r32(this + STATE) {
        PICTURE => {
            let m = &mut c.m;
            for k in 4..12 {
                m.w16(image(this, k) + BRIGHT, 128);
                m.w32(image(this, k) + KIND, PICTURE);
            }
            m.w16(image(this, 6) + BRIGHT, 128);
            m.w16(image(this, 5) + BRIGHT, 128);
        }
        VIDEO_STATE_PLAYING => init_dynamic_images(c, this),
        CREDITS_STATE => {
            let m = &mut c.m;
            for k in 0..16 {
                m.w16(image(this, k) + BRIGHT, 128);
                m.w32(image(this, k) + KIND, CREDITS_STATE);
            }
            let at = m.r32(this + STATE_AT);
            call_addr(c, CREDITS_INIT, &[CREDITS, at]);
        }
        _ => {}
    }
}

pub fn abi_set_state(c: &mut Cpu) {
    set_state(c, c.r[4], c.r[5]);
}

/// `tScreenMain::InitDynamicImages` (SCREENMAIN.CPP, 0x80037624): the video across the TVs in
/// one of 24 layouts: pieces of tiles, each tile's TVs showing their part of an 80 by 80 frame,
/// in the background's colour.
pub fn init_dynamic_images(c: &mut Cpu, this: u32) {
    let layout = LAYOUTS.wrapping_add(rand_below(c, 24).wrapping_mul(20));
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
                        let e = image(this, 4 * (row + first_row + y * rows) + col + first + x * cols);
                        let m = &mut c.m;
                        m.w8(e + UV, (col as i32).wrapping_mul(w) as u8);
                        m.w8(e + UV + 1, (row as i32).wrapping_mul(h) as u8);
                        m.w8(e + UV + 2, (w - 1) as u8);
                        m.w8(e + UV + 3, h as u8);
                        let page = call(c, GET_TPAGE, &[2, 0, 512, 0]);
                        let m = &mut c.m;
                        m.w16(e + PAGE, page as u16);
                        m.w16(e + CLUT, 0);
                        m.w16(e + FLAGS, 34);
                        let color = back_color(m, this);
                        m.w32(e + KIND, VIDEO_STATE_PLAYING);
                        m.w16(e + BRIGHT, 128);
                        m.w32(e + COLOR, color);
                    }
                }
            }
        }
        j += 1;
    }
}

pub fn abi_init_dynamic_images(c: &mut Cpu) {
    init_dynamic_images(c, c.r[4]);
}

/// `tScreenMain::ProcessInput` (SCREENMAIN.CPP, 0x80037990): back, past the first menu, swaps
/// the background (on the main menu, the front end's mode back to 0).
pub fn process_input(c: &mut Cpu, this: Screen, key: u32) {
    if c.m.r32(key) != KEY_BACK {
        return;
    }
    let app = FEApplication(c.m.r32(FE_APP));
    if app.back_depth(&mut c.m, 0) <= 0 {
        return;
    }
    swap_background(c, this.0, u32::MAX);
    let m = &mut c.m;
    let app = FEApplication(m.r32(FE_APP));
    if app.current_menu(m, 0).0 == m.r32(MENUS).wrapping_add(MAIN_MENU) {
        m.w8(MODE, 0);
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, Screen(c.r[4]), c.r[6]);
}

/// `tScreenMain::DrawVideoLines` (SCREENMAIN.CPP, 0x80037b10): the lines between the TVs,
/// darkening.
pub fn draw_video_lines(c: &mut Cpu) {
    for x in (243..483).step_by(80) {
        call_addr(c, DRAW_TRANS_SQUARE, &[0x0030_3030, x, 42, 2, 169, 1]);
    }
    for y in (84..210).step_by(42) {
        call_addr(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 163, y, 321, 1, 1]);
    }
    call(c, SET_ABR_MODE, &[2]);
}

pub fn abi_draw_video_lines(c: &mut Cpu) {
    draw_video_lines(c);
}

/// The main menu's selected item is its last.
fn on_last_item(m: &mut Mem) -> bool {
    let menu = FEApplication(m.r32(FE_APP)).current_menu(m, 0);
    let k = menu.current_item(m) as u32;
    m.r32(menu.0.wrapping_add(k << 2) + 16) == m.r32(MENUS).wrapping_add(LAST_ITEM)
}

/// `tScreenMain::DrawBackground` (SCREENMAIN.CPP, 0x80037bb8). In order:
/// - a background read is uploaded once every TV shows the one before;
/// - on their menu, the credits (drawn);
/// - the last item's picture, faded in or out;
/// - the video's next frame, or a new video once the wall has stood ten seconds (the background
///   back when it ends);
/// - the background, the drop shadow and the lines;
/// - a new swapped file every 25 seconds, shown for 13; the rolling bits;
/// - the TVs whose image changed turned off one by one, the ones off turned on with their new
///   one; the TVs and their frames.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    upload_backgrounds(c, this);
    // the credits on their menu
    let m = &mut c.m;
    let credits = FEApplication(m.r32(FE_APP)).current_menu(m, 0).0 == m.r32(MENUS).wrapping_add(CREDITS_MENU);
    let state = m.r32(this + STATE);
    if state == CREDITS_STATE && !credits {
        set_state(c, this, BACKGROUND);
    } else if credits {
        set_state(c, this, CREDITS_STATE);
    }
    let on = (c.m.r32(this + STATE) == CREDITS_STATE) as u32;
    call_addr(c, CREDITS_DRAW, &[CREDITS, on]);
    // the last item's picture (or the front end's mode for it), fading in, or out
    let last = c.m.r8(MODE) == PICTURE_MODE || on_last_item(&mut c.m);
    if c.m.r32(this + STATE) == PICTURE {
        if !last {
            set_state(c, this, BACKGROUND);
        }
    } else if last {
        set_state(c, this, PICTURE);
    }
    let fade = lh(&mut c.m, this + FADE);
    let picture = if c.m.r32(this + STATE) != PICTURE {
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
        draw_picture(c, this);
    }
    // the video (its frame's halves in VRAM rows 0 and 80 by turns)
    let y = if c.m.r32(this + FRAMES) & 1 != 0 { 0 } else { 80 };
    let video = c.m.r32(this + VIDEO);
    if call_addr(c, VIDEO_STATE, &[video]) == VIDEO_PLAYING {
        let (video, ticks) = (c.m.r32(this + VIDEO), c.m.r32(GAME_TICKS));
        c.m.w32(this + ABORTED, 0);
        c.m.w32(this + ABORTED_AT, ticks);
        if call_addr(c, VIDEO_UPDATE, &[video, 512, y]) != 0 {
            let n = c.m.r32(this + FRAMES).wrapping_add(1);
            c.m.w32(this + FRAMES, n);
        }
    } else {
        let video = c.m.r32(this + VIDEO);
        let state = call_addr(c, VIDEO_STATE, &[video]);
        let due = state != VIDEO_STOPPED && c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + STATE_AT)) >= 641;
        if due && c.m.r32(this + STATE) == BACKGROUND {
            start_video(c, this);
        }
    }
    if c.m.r32(this + STATE) == VIDEO_STATE_PLAYING {
        let video = c.m.r32(this + VIDEO);
        if call_addr(c, VIDEO_STATE, &[video]) == 0 {
            set_state(c, this, BACKGROUND);
        }
    }
    let screen = Screen(this);
    let table = screen.perm_shapes().shapes(&mut c.m);
    fescreen::draw_background_image(c, screen, 2, 28, table, 0);
    call_addr(c, DRAW_DROP_SHADOW, &[this]);
    draw_video_lines(c);
    let since = swap_shapes(c, this);
    draw_swapped(c, this, since);
    draw_bits(c);
    turn_tvs(c, this);
    for k in 0..16 {
        call_addr(c, DRAW_TV, &[tv(this, k).0]);
    }
    // the TVs' frames
    for x in (243..483).step_by(80) {
        call_addr(c, DRAW_SQUARE, &[0x0040_4040, x, 42, 2, 169]);
    }
    for y in (84..210).step_by(42) {
        call_addr(c, DRAW_SQUARE, &[0x0030_3030, 163, y, 321, 1]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// Each background read uploaded (the second 166 across in VRAM) once every TV shows the one
/// before (a background's slice or the credits).
fn upload_backgrounds(c: &mut Cpu, this: u32) {
    for k in 0..2 {
        let info = back(this, k);
        fescreen::is_shape_file_loaded(c, Screen(this), info);
        if info.file(&mut c.m) == 0 {
            continue;
        }
        let m = &mut c.m;
        let shown = (0..16).all(|j| {
            let (kind, e) = (m.r32(this + SHOWN + 4 * j), image(this, j));
            (kind == BACKGROUND || kind == CREDITS_STATE) && m.r32(e + KIND) == kind && tv(this, j).clut(m) == m.r16(e + CLUT)
        });
        if shown {
            fescreen::upload_shapes(c, Screen(this), info, sx(166 * k), 0, BACK_SHAPES, 0);
        }
    }
}

/// The swapped shapes: every 25 seconds (but on the last item's picture) a new random file
/// read, uploaded once read and shown in one of four places. The time since they came (0 when
/// a new one is asked for).
fn swap_shapes(c: &mut Cpu, this: u32) -> u32 {
    let screen = Screen(this);
    let mut since = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + SWAP_AT));
    if since >= 1501 && c.m.r32(this + STATE) != PICTURE {
        since = 0;
        let reading = screen.swap_shapes().async_handle(&mut c.m) != 0;
        c.m.w32(this + ROLLING, 0);
        if !reading {
            let n = loop {
                let n = rand_below(c, ROLL_COUNT);
                if sx(n) as i32 != lh(&mut c.m, this + ROLL) {
                    break n;
                }
            };
            c.m.w16(this + ROLL, n as u16);
            with_buffer(c, 64, |c, name| {
                call(c, SPRINTF, &[name, ROLL_FORMAT, sx(n)]);
                fescreen::async_load_swap_shape_file(c, screen, name);
            });
        }
    }
    fescreen::is_shape_file_loaded(c, screen, screen.swap_shapes());
    if screen.swap_shapes().file(&mut c.m) != 0 {
        screen.swap_shapes().set_num_shapes(&mut c.m, ROLL_SHAPES as u16);
        fescreen::upload_swap_shapes(c, screen, ROLL_SHAPES);
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(this + SWAP_AT, ticks);
        c.m.w32(this + ROLLING, 1);
        let spot = c.m.r16(this + SWAP_AT) & 3;
        c.m.w16(this + SPOT, spot);
    }
    since
}

/// The swapped shapes up for 800 ticks after they came: fading in over 128, out over the last
/// 128 (brighter with the credits on), in the background's colour.
fn draw_swapped(c: &mut Cpu, this: u32, since: u32) {
    if since >= 800 || c.m.r32(this + ROLLING) == 0 {
        return;
    }
    let mut bright = if since < 128 {
        128 - since as i32
    } else if since.wrapping_sub(672) < 128 {
        since as i32 - 672
    } else {
        0
    };
    with_buffer(c, DrawShapeExtended::SIZE, |c, at| {
        let (options, m) = (DrawShapeExtended(at), &mut c.m);
        let color = back_color(m, this);
        options.set_tint(m, 0, color as i32);
        let table = Screen(this).swap_shapes().shapes(m);
        options.set_custom_shapes(m, table);
        if m.r32(CREDITS + CREDITS_ON) != 0 {
            bright = (bright + 64).min(128);
        }
        let spot = SPOTS.wrapping_add((lh(m, this + SPOT) as u32) << 2);
        let (x, y) = (lh(m, spot) as u32, lh(m, spot + 2) as u32);
        let shape = since / 15 % 10;
        call(c, DRAW_SHAPE_EXTENDED, &[shape, 0x611, x, y, bright as u32, 1, at]);
    });
}

/// The rolling bits: rows 8 apart from y 50 (down to 210) of five bits 13 apart from x 418,
/// each row's byte one on every 20 ticks.
fn draw_bits(c: &mut Cpu) {
    let mut y = 50u32;
    for row in 0..BIT_ROWS {
        if y as i32 >= 210 {
            break;
        }
        let mut x = 418u32;
        for bit in (0..=4).rev() {
            let ticks = c.m.r32(GAME_TICKS) as i32;
            let k = (row as i32).wrapping_add(ticks / 20).wrapping_rem(BIT_ROWS as i32);
            let byte = c.m.r8(BITS.wrapping_add(k as u32)) as i32;
            call(c, DRAW_SHAPE_EXTENDED, &[((byte >> bit) & 1) as u32, 1, sx(x), sx(y), 64, 3, 0]);
            x = x.wrapping_add(13);
        }
        y = y.wrapping_add(8);
    }
}

/// The TVs whose image changed turned off in order, one more every 16 ticks since the state
/// began (the count a short); the TVs off turned on with their new image.
fn turn_tvs(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let since = ((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + STATE_AT)) << 12) as i32) >> 16;
    if since >= 0 {
        for k in 0..=since.min(15) as u32 {
            let at = c.m.r8(TV_ORDER.wrapping_add(k)) as u32;
            let m = &mut c.m;
            let (e, t) = (image(this, at), tv(this, at));
            let same = m.r32(this + SHOWN + 4 * at) == m.r32(e + KIND) && t.clut(m) == m.r16(e + CLUT);
            if !same && t.state(m) == TV_ON {
                call_addr(c, TURN_OFF_TV, &[t.0]);
            }
        }
    }
    for k in 0..16 {
        let t = tv(this, k);
        if t.state(&mut c.m) != TV_OFF {
            continue;
        }
        let e = image(this, k);
        let m = &mut c.m;
        for (f, set) in [(0, TVConfig::set_u as fn(TVConfig, &mut Mem, u8)), (1, TVConfig::set_v), (2, TVConfig::set_uw), (3, TVConfig::set_vh)] {
            let v = m.r8(e + UV + f);
            set(t, m, v);
        }
        let v = m.r16(e + PAGE);
        t.set_tpage(m, v);
        let v = m.r16(e + CLUT);
        t.set_clut(m, v);
        let v = m.r16(e + FLAGS);
        t.set_flags(m, v);
        let v = m.r32(e + COLOR);
        t.set_tint(m, v as i32);
        let v = m.r32(e + KIND);
        m.w32(this + SHOWN + 4 * k, v);
        call_addr(c, TURN_ON_TV, &[t.0]);
        let m = &mut c.m;
        let v = m.r16(e + BRIGHT);
        t.set_dest_brightness(m, v);
        let n = m.r32(this + TO_TURN).wrapping_sub(1);
        m.w32(this + TO_TURN, n);
    }
}

/// [`draw_background`]'s last item's picture: the middle TVs dimmed by its fade (two of them
/// less), two flickering shapes and a bar, a turning one, and its text.
fn draw_picture(c: &mut Cpu, this: u32) {
    let grey = |v: u32| (v << 16) | (v << 8) | v;
    let m = &mut c.m;
    for k in 4..12 {
        let t = tv(this, k);
        t.update_flags(m, |f| f | 2);
        let f = 128i32.wrapping_sub(lh(m, this + FADE)) as u32;
        t.set_tint(m, grey(f) as i32);
    }
    let g = (128 - (lh(m, this + FADE) << 6) / 96) as u32;
    tv(this, 6).set_tint(m, grey(g) as i32);
    tv(this, 5).set_tint(m, grey(g) as i32);
    let fade = |c: &mut Cpu| 96i32.wrapping_sub(lh(&mut c.m, this + FADE)) as u32;
    with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
        DrawShapeExtended(options).set_tint(&mut c.m, 0, 0xBEBE);
        for (flags, x) in [(0x411, 163), (0x415, 403)] {
            let t = call(c, GETTICK, &[]) as i32;
            let f = fade(c);
            call(c, DRAW_SHAPE_EXTENDED, &[(257 + t / 15 % 10) as u32, flags, x, (-15i32) as u32, f, 1, options]);
        }
        let f = fade(c);
        call(c, DRAW_SHAPE_EXTENDED, &[256, 0x11, 0, 0, f, 1, options]);
    });
    let t = call(c, GETTICK, &[]) as i32;
    let f = fade(c);
    call(c, SCALE_SHAPE_EXTENDED, &[(250 + t / 21 % 6) as u32, 1, 0, 0, f, 3, 0]);
    fetextrender::set_abr(c, 1, 1);
    let color = fefades::fade(0xBEBE, 0, fade(c));
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 181);
        rect.set_y(m, 129);
        rect.set_w(m, 284);
        rect.set_h(m, 42);
        let word = call(c, TEXT_WORD, &[PICTURE_TEXT]);
        fetextrender::word_wrap_text_rgb(c, word, at, color);
    });
    fetextrender::set_abr(c, 0, 0);
    call(c, SET_ABR_MODE, &[0]);
}

/// [`draw_background`]'s new video: its frame's place cleared, one other than the last
/// spooled and played.
fn start_video(c: &mut Cpu, this: u32) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 512);
        rect.set_y(m, 0);
        rect.set_w(m, 80);
        rect.set_h(m, 160);
        call_addr(c, CLEAR_IMAGE, &[at, 0, 0, 0]);
        call_addr(c, DRAW_SYNC, &[0]);
    });
    let n = loop {
        let n = rand_below(c, VIDEO_COUNT);
        c.m.w16(this + NEXT_CLIP, n as u16);
        if sx(n) as i32 != lh(&mut c.m, this + CLIP) {
            break n;
        }
    };
    c.m.w16(this + CLIP, n as u16);
    set_state(c, this, VIDEO_STATE_PLAYING);
    with_buffer(c, 128, |c, file| {
        let (path, n) = (c.m.r32(VIDEO_PATH), lh(&mut c.m, this + NEXT_CLIP) as u32);
        call(c, SPRINTF, &[file, CLIP_FORMAT, path, n]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    });
}

/// `tScreenMain::GetShapeInfo` (SCREENMAIN.CPP, 0x80038890): 267 shapes of the main screen's
/// file, 10 of the swapped one's.
pub fn get_shape_info(m: &mut Mem, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w16(permanent, SHAPES);
    m.w32(name, MAIN_NAME);
    m.w16(swap, ROLL_SHAPES as u16);
    m.w32(swap_name, ROLL_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenMain::PreLoad` (SCREENMAIN.CPP, 0x800388c0): the main screen's file and a random
/// swapped one named, then read.
pub fn pre_load(c: &mut Cpu, this: Screen) {
    let n = c.m.r8(MAIN_NUMBER) as u32;
    call(c, SPRINTF, &[MAIN_NAME, MAIN_FORMAT, n]);
    let n = rand_below(c, ROLL_COUNT);
    c.m.w16(this.0 + ROLL, n as u16);
    call(c, SPRINTF, &[ROLL_NAME, ROLL_FORMAT, sx(n)]);
    fescreen::pre_load(c, this);
}

pub fn abi_pre_load(c: &mut Cpu) {
    pre_load(c, Screen(c.r[4]));
}

/// `tScreenMain::Initialize` (SCREENMAIN.CPP, 0x8003895c): the screen's files read; two
/// different random backgrounds read and uploaded (the sound kept going); the video made; the
/// TVs set from the first background, showing nothing yet; the bits rolled.
pub fn initialize(c: &mut Cpu, this: Screen) {
    fescreen::initialize(c, this);
    let (screen, this) = (this, this.0);
    for k in 0..2 {
        let info = back(this, k);
        info.set_shapes(&mut c.m, TextureShapeInfo(0));
        fescreen::initialize_shapes(c, info, BACK_SHAPES);
    }
    let first = rand_below(c, BACK_COUNT);
    c.m.w32(back_num(this, 0), first);
    let other = (call(c, RAND, &[]) as i32).wrapping_rem(BACK_COUNT - 1);
    let n = (c.m.r32(back_num(this, 0)) as i32).wrapping_add(other).wrapping_add(1);
    c.m.w32(back_num(this, 1), n.wrapping_rem(BACK_COUNT) as u32);
    with_buffer(c, 64, |c, file| {
        for k in 0..2 {
            let n = c.m.r32(back_num(this, k));
            call(c, SPRINTF, &[file, BACK_FORMAT, n]);
            fescreen::async_load_shape_file(c, screen, file, back(this, k));
        }
    });
    // both read and uploaded (the second 166 across in VRAM)
    let (first, second) = (back(this, 0), back(this, 1));
    loop {
        call(c, SYSTEM_TASK, &[0]);
        let loaded = fescreen::is_shape_file_loaded(c, screen, first);
        if first.file(&mut c.m) != 0 {
            fescreen::upload_shapes(c, screen, first, 0, 0, BACK_SHAPES, 0);
        }
        let both = loaded && fescreen::is_shape_file_loaded(c, screen, second);
        if second.file(&mut c.m) != 0 {
            fescreen::upload_shapes(c, screen, second, 166, 0, BACK_SHAPES, 0);
        }
        if both {
            break;
        }
    }
    c.m.w16(this + CLIP, 0xFFFF);
    c.m.w32(this + FRAMES, 0);
    let video = call_addr(c, VIDEO_CREATE, &[80, 80, 0xF_0000, 0x2_0000, 16]);
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
        let m = &mut c.m;
        let cur = cur(m, this);
        let table = back(this, cur).shapes(m);
        call_addr(c, INIT_TV, &[tv(this, k).0, table.0, k]);
    }
    c.m.w32(this + STATE, 0);
    set_state(c, this, BACKGROUND);
    for k in 0..16 {
        c.m.w32(this + SHOWN + 4 * k, 0);
    }
    for k in 0..BIT_ROWS {
        let v = call(c, RAND, &[]);
        c.m.w8(BITS + k, v as u8);
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenMain::Cleanup` (SCREENMAIN.CPP, 0x80038cd0): the video destroyed and freed, the
/// backgrounds' shapes freed, then the screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    let video = c.m.r32(this.0 + VIDEO);
    call_addr(c, VIDEO_DESTROY, &[video]);
    let video = c.m.r32(this.0 + VIDEO);
    call(c, PURGE, &[video]);
    for k in 0..2 {
        fescreen::free_shapes(c, this, back(this.0, k));
    }
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreenMain::~tScreenMain` (SCREENMAIN.CPP, 0x80038d3c): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, pick, player, via_abi};
    use crate::globals::SHAPES;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram};

    /// The screen's swapped shapes; a shape file's table and file read; a TV's state and CLUT.
    const SWAP: u32 = 40;
    const TABLE: u32 = 0;
    const DATA: u32 = 4;
    const TV_STATE: u32 = 0;
    const TV_CLUT: u32 = 34;

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
            let info = back(s, k as u32).0;
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
            let (t, e) = (tv(s, k).0, image(s, k));
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
                let (t, e) = (tv(s, k).0, image(s, k));
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
                    c.m.w32(KEY, KEY_BACK);
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

}
