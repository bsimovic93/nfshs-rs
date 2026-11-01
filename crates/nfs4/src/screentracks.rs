//! SCREENTRACKS.CPP: the track select screen (`tScreenTrackSelect`): the chosen track's name in
//! a box, its video playing (in two halves, faded in and out with the screen's brightness, the
//! track's own picture swapped in while none plays), its settings' video wall, the track's
//! length text, and the options asked for only where the track allows them.
//!
//! The video, `ClearImage`, `DrawSync` and `PSXDrawTransSquare` are the host's (called by
//! address).

use crate::abi::{self, Saved};
use crate::femenu::method;
use crate::femenuextended::{NEXT_PRIM, OT, SET_ABR_MODE, SPRINTF};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TRACK_MANAGER};
use crate::{drawshp, fescreen, fetextrender, fetracks, fevideowall};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen` (+0 its permanent shapes' table, +40 its swapped shapes: +44 the file
// read, +58 uploaded (bit 0)), then +100 the video, +104 its frames shown, +108 the track whose
// picture is swapped in, +110 the track whose video plays, +112 the brightness (0..128), +114
// the brightness it goes to, +116 the brightness it went from, +120 since when, +124 the video
// waiting to start (none played yet), +128 the video wall's TVs' config, +608 the video wall
// (`tVideoWall`, +20 its state), +664 its settings shown, +668 when the video was last asked
// to start.
const SWAP: u32 = 40;
const SWAP_DATA: u32 = 44;
const SWAP_UPLOADED: u32 = 58;
const VIDEO: u32 = 100;
const FRAMES: u32 = 104;
const PICTURE: u32 = 108;
const PLAYING: u32 = 110;
const BRIGHT: u32 = 112;
const DEST_BRIGHT: u32 = 114;
const FROM_BRIGHT: u32 = 116;
const BRIGHT_AT: u32 = 120;
const WAITING: u32 = 124;
const TV_CONFIG: u32 = 128;
const WALL: u32 = 608;
const WALL_STATE: u32 = 20;
const WALL_SET: u32 = 664;
const ASKED_AT: u32 = 668;

/// The track list (`frontEnd + 0x23`, the chosen one at `+0x46`); the players (`+3`), the
/// race's kind (`+4`), the second player's setting (`+0x31e`).
const TRACKS: u32 = FRONT_END + 0x23;
const CHOSEN: u32 = FRONT_END + 0x46;
const PLAYERS: u32 = FRONT_END + 3;
const KIND: u32 = FRONT_END + 4;
const SECOND: u32 = FRONT_END + 0x31E;
/// In the menus' block: the track iterator, two items' flags (bit 0 off).
const ITERATOR: u32 = 3208;
const OPTIONS_FLAGS: u32 = 3916;
const RACE_FLAGS: u32 = 3952;
/// The videos' directory (`Paths_Paths + 0xa4`); the swapped file's name; the video wall's TV
/// layout.
const VIDEO_PATH: u32 = PATHS + 0xA4;
const SWAP_NAME: u32 = 0x8005_3130;
const WALL_LAYOUT: u32 = 0x8005_2530;
/// "%szzzTR%02d.dct", "ztrack", the swapped file's name's format.
const VIDEO_FORMAT: u32 = 0x8001_1E9C;
const TRACK_NAME: u32 = 0x8001_1EAC;
const SWAP_FORMAT: u32 = 0x8001_1EB4;
/// A menu's vptr and its virtual function asked on back.
const MENU_VPTR: u32 = 104;
const MENU_ON_BACK: u32 = 64;
/// The keys: options, back.
const KEY_OPTIONS: u32 = 8;
const KEY_BACK: u32 = 16;

/// The host's: the videos, `ClearImage`, `DrawSync`, `PSXDrawTransSquare`.
const VIDEO_CREATE: u32 = 0x8005_11B0;
const VIDEO_DESTROY: u32 = 0x8005_1328;
const VIDEO_SPOOL: u32 = 0x8005_1380;
const VIDEO_START: u32 = 0x8005_13DC;
const VIDEO_ABORT: u32 = 0x8005_143C;
const VIDEO_STATE: u32 = 0x8005_1480;
const VIDEO_UPDATE: u32 = 0x8005_1528;
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `GetTPage`, `purgememadr`, `Front_EnableLocalSpeech`.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
/// `DrawVideoWall`: hooked (the host records the spinning track's quads), so called by address.
const DRAW_VIDEO_WALL: u32 = 0x8004_22DC;
const LOCAL_SPEECH: fn(&mut Cpu) = crate::front::enable_local_speech_entry;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// The menus' block.
fn menus(m: &mut Mem) -> u32 {
    m.r32(SONGS_SCREEN)
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// The track at `k` in the list into `info`.
fn get_track(c: &mut Cpu, r: &Saved, k: u32, info: u32, ra: u32) {
    let id = c.m.r8(TRACKS.wrapping_add(k)) as u32;
    port(c, r, 0x8002_1c04, ra, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
}

/// A video frame's half (`POLY_FT4`, semi-transparent) linked into the ordering table: at `x`
/// (160 wide, at y 105..232), its texture from `u` of the page at `page_x`, rows from `v`; the
/// screen's brightness its colour. The original's `s0` after (its mask with the primitive's
/// address the second time).
fn video_half(c: &mut Cpu, r: &mut Saved, this: u32, x: [u32; 2], u: u32, page_x: u32, ra: u32) {
    let m = &mut c.m;
    let prim = m.r32(NEXT_PRIM);
    r.0[8] = prim;
    let ot = m.r32(OT);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    m.w32(NEXT_PRIM, prim.wrapping_add(40));
    let entry = m.r32(ot);
    m.w32(ot, (entry & 0xFF00_0000) | (prim & r.0[0]));
    let b = lh(m, this + BRIGHT);
    m.w8(prim + 3, 9);
    for (k, (px, py)) in [(x[0], 105), (x[1], 105), (x[0], 232), (x[1], 232)].into_iter().enumerate() {
        let at = prim + 8 + 8 * k as u32;
        m.w16(at, px as u16);
        m.w16(at + 2, py);
        m.w8(at + 4, if k % 2 == 0 { u } else { u + 80 } as u8);
        m.w8(at + 5, if k < 2 { r.0[6] } else { r.0[2] } as u8);
    }
    m.w32(prim + 4, (b << 16) | (b << 8) | b);
    m.w8(prim + 7, 46);
    let page = r.call_leaving(c, GET_TPAGE, &[2, 1, page_x, r.0[5]], ra);
    c.m.w16(prim + 22, page as u16);
    c.m.w16(prim + 14, 0);
}

/// `tScreenTrackSelect::DrawBackground` (SCREENTRACKS.CPP, 0x800419e0): the chosen track's name
/// in its box; the brightness and the swapped picture brought up to date (the picture uploaded
/// and the wall turned on once read, while dark); the video: its buffer cleared (dark) when
/// stopped, its next frame, or (bright enough to want it, or never played, and a while after
/// it was last asked) started again with the brightness to its picture's; the frame's two
/// halves; the wall.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        r.0[7] = this;
        let rect = sp + 16;
        for (k, v) in [320u16, 200, 170, 12].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        let it = menus(&mut c.m) + ITERATOR;
        let text = port(c, &r, 0x8002_1f34, 0x8004_1a44, &[it, u32::MAX], fetracks::abi_list_iterator_track_text_value);
        port(c, &r, 0x8004_c490, 0x8004_1a58, &[sx(text), rect, 0], drawshp::abi_round_rectangle);
        let info = sp + 24;
        r.0[0] = info;
        get_track(c, &r, 0, info, 0x8004_1a74);
        port(c, &r, 0x8004_2178, 0x8004_1a80, &[this, info], abi_update_brightness);
        port(c, &r, 0x8004_2268, 0x8004_1a8c, &[this, info], abi_update_video_wall);
        port(c, &r, 0x8002_5e2c, 0x8004_1a98, &[this, this + SWAP], fescreen::abi_is_shape_file_loaded);
        let wall = this + WALL;
        r.0[0] = wall;
        let m = &mut c.m;
        if m.r32(this + SWAP_DATA) != 0 && lh(m, wall + WALL_STATE) != u32::MAX && lh(m, this + BRIGHT) == 0 {
            port(c, &r, 0x8002_5ff0, 0x8004_1ad0, &[this, 10], fescreen::abi_upload_swap_shapes);
            port(c, &r, 0x8003_70f0, 0x8004_1ad8, &[wall], fevideowall::abi_turn_on);
        }
        // the video
        let even = ((c.m.r32(this + FRAMES) & 1) == 0) as u32;
        r.0[0] = even << 7;
        r.0[6] = r.0[0];
        let video = c.m.r32(this + VIDEO);
        let state = r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8004_1af4);
        if state == 1 {
            let clear = sp + 72;
            for (k, v) in [512u16, 0, 170, 256].into_iter().enumerate() {
                c.m.w16(clear + 2 * k as u32, v);
            }
            r.call_ptr_leaving(c, CLEAR_IMAGE, &[clear, 0, 0, 0], 0x8004_1b34);
            r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8004_1b3c);
            let m = &mut c.m;
            let now = m.r32(GAME_TICKS);
            m.w16(this + BRIGHT, 0);
            m.w32(this + BRIGHT_AT, now.wrapping_sub(20));
        } else if state == 3 {
            let video = c.m.r32(this + VIDEO);
            if r.call_ptr_leaving(c, VIDEO_UPDATE, &[video, 512, r.0[0] & 0xFFFF], 0x8004_1b6c) != 0 {
                let m = &mut c.m;
                let frames = m.r32(this + FRAMES).wrapping_add(1);
                r.0[6] = (((frames & 1) == 0) as u32) << 7;
                m.w32(this + FRAMES, frames);
            }
        } else {
            let m = &mut c.m;
            let wants = (lh(m, this + DEST_BRIGHT) as i32) < lh(m, this + BRIGHT) as i32;
            if (m.r32(this + WAITING) != 0 || wants) && m.r32(GAME_TICKS).wrapping_sub(m.r32(this + ASKED_AT)) >= 257 {
                let file = sp + 80;
                r.0[0] = file;
                if !wants {
                    let b = if c.m.r8(info + 3) != 0 { 128 } else { 32 };
                    port(c, &r, 0x8004_2148, 0x8004_1c0c, &[this, b], abi_set_brightness);
                }
                let (path, id) = (c.m.r32(VIDEO_PATH), lh(&mut c.m, this + PLAYING));
                r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path, id], 0x8004_1c2c);
                let video = c.m.r32(this + VIDEO);
                r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8004_1c38);
                let video = c.m.r32(this + VIDEO);
                r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8004_1c44);
            }
        }
        // the frame's halves
        if lh(&mut c.m, this + BRIGHT) as i32 > 0 {
            c.m.w32(sp + 160, NEXT_PRIM);
            r.0[0] = 0x00FF_FFFF;
            r.0[1] = (-64i32) as u32;
            r.0[5] = 0;
            r.0[4] = 313;
            r.0[3] = 232;
            r.0[2] = r.0[6] | 0x7F;
            video_half(c, &mut r, this, [153, 313], 0, 512, 0x8004_1d5c);
            r.0[1] = 576 << 16;
            let prim = c.m.r32(NEXT_PRIM);
            r.0[0] &= prim;
            video_half(c, &mut r, this, [313, 473], 16, 576, 0x8004_1e4c);
        }
        r.call_ptr_leaving(c, DRAW_VIDEO_WALL, &[this], 0x8004_1e5c);
    })
}

/// `tScreenTrackSelect::GetShapeInfo` (SCREENTRACKS.CPP, 0x80041e8c): 68 shapes of "ztrack",
/// 10 of the chosen track's picture's file.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, permanent, swap, name) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[5] = this;
        r.0[2] = permanent;
        r.0[3] = swap;
        r.0[4] = name;
        r.0[1] = swap_name;
        let info = sp + 16;
        get_track(c, &r, 0, info, 0x8004_1ed8);
        r.0[0] = SWAP_NAME;
        let m = &mut c.m;
        m.w16(permanent, 68);
        m.w16(swap, 10);
        m.w32(name, TRACK_NAME);
        r.call_leaving(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, info + 8], 0x8004_1f10);
        let m = &mut c.m;
        m.w32(swap_name, SWAP_NAME);
        let id = m.r8(info) as i8 as i32 as u32;
        m.w16(this + PICTURE, id as u16);
    })
}

/// `tScreenTrackSelect::Initialize` (SCREENTRACKS.CPP, 0x80041f4c): the list from the first
/// track (back and forth to settle it), the video's buffer cleared, the screen's; the track's
/// video created and started; the wall set up (its texts and icon) and turned on, dark, the
/// video waiting and asked a while ago.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 184, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[0] = FRONT_END;
        r.0[1] = 0x8005_0000;
        c.m.w8(CHOSEN, 0);
        let it = menus(&mut c.m) + ITERATOR;
        port(c, &r, 0x8002_2020, 0x8004_1f84, &[it, u32::MAX], fetracks::abi_list_iterator_track_decrement);
        let it = menus(&mut c.m) + ITERATOR;
        port(c, &r, 0x8002_1f74, 0x8004_1f94, &[it, u32::MAX], fetracks::abi_list_iterator_track_increment);
        let info = sp + 32;
        get_track(c, &r, 0, info, 0x8004_1fa8);
        let clear = sp + 80;
        for (k, v) in [512u16, 0, 170, 256].into_iter().enumerate() {
            c.m.w16(clear + 2 * k as u32, v);
        }
        r.call_ptr_leaving(c, CLEAR_IMAGE, &[clear, 0, 0, 0], 0x8004_1fd8);
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8004_1fe0);
        port(c, &r, 0x8002_60cc, 0x8004_1fe8, &[this], fescreen::abi_initialize);
        let file = sp + 88;
        r.0[0] = file;
        let (path, id) = (c.m.r32(VIDEO_PATH), c.m.r8(info) as i8 as i32 as u32);
        r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path, id], 0x8004_2008);
        let video = r.call_ptr_leaving(c, VIDEO_CREATE, &[160, 128, 0xF_0000, 0x2_C000, 16], 0x8004_2028);
        c.m.w32(this + VIDEO, video);
        r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8004_2038);
        let video = c.m.r32(this + VIDEO);
        let wall = this + WALL;
        r.0[0] = wall;
        r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8004_2044);
        c.m.w32(this + FRAMES, 0);
        let shapes = c.m.r32(this + SWAP);
        for (k, v) in [10, WALL_LAYOUT, 150].into_iter().enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, v);
        }
        port(c, &r, 0x8003_6988, 0x8004_2078, &[wall, this + TV_CONFIG, shapes, 0], fevideowall::abi_initialize);
        port(c, &r, 0x8003_6b84, 0x8004_208c, &[wall, 249, 320, 80], fevideowall::abi_set_available_text);
        let icons = c.m.r32(this);
        c.m.w32(sp + 16, 60);
        c.m.w32(sp + 20, icons);
        port(c, &r, 0x8003_6ba0, 0x8004_20b0, &[wall, 38, 10, 310], fevideowall::abi_set_available_icon);
        let m = &mut c.m;
        m.w16(this + BRIGHT, 0);
        m.w16(this + DEST_BRIGHT, 0);
        m.w32(this + WALL_SET, 0);
        port(c, &r, 0x8003_70f0, 0x8004_20c4, &[wall], fevideowall::abi_turn_on);
        let m = &mut c.m;
        let now = m.r32(GAME_TICKS);
        m.w32(this + WAITING, 1);
        m.w32(this + ASKED_AT, now.wrapping_sub(256));
        let id = m.r8(info) as i8 as i32 as u32;
        m.w16(this + PLAYING, id as u16);
    })
}

/// `tScreenTrackSelect::Cleanup` (SCREENTRACKS.CPP, 0x80042108): the video destroyed and freed,
/// then the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_DESTROY, &[video], 0x8004_2124);
        let video = c.m.r32(this + VIDEO);
        r.call_leaving(c, PURGE, &[video], 0x8004_2130);
        port(c, &r, 0x8002_61c0, 0x8004_2138, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenTrackSelect::SetBrightness` (SCREENTRACKS.CPP, 0x80042148): a new brightness to go
/// to, from the one now, starting now.
pub fn abi_set_brightness(c: &mut Cpu) {
    let (this, b) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    if sx(b) != lh(m, this + DEST_BRIGHT) {
        let now = m.r16(this + BRIGHT);
        let t = m.r32(GAME_TICKS);
        m.w16(this + DEST_BRIGHT, b as u16);
        m.w16(this + FROM_BRIGHT, now);
        m.w32(this + BRIGHT_AT, t);
    }
}

/// `tScreenTrackSelect::UpdateBrightness` (SCREENTRACKS.CPP, 0x80042178): the brightness on its
/// way (over 128 ticks); dark (and the wanted one too), the video stopped and, the first time,
/// waiting for this track's.
pub fn abi_update_brightness(c: &mut Cpu) {
    let (this, info) = (c.r[4], c.r[5]);
    let now = c.m.r32(GAME_TICKS);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = info;
        let m = &mut c.m;
        let d = now.wrapping_sub(m.r32(this + BRIGHT_AT)) as i32;
        let to = lh(m, this + DEST_BRIGHT);
        if to != lh(m, this + BRIGHT) {
            let v = if d >= 128 {
                to
            } else if d < 0 {
                0
            } else {
                let from = lh(m, this + FROM_BRIGHT);
                let step = (to.wrapping_sub(from) as i32).wrapping_mul(d) / 128;
                (m.r16(this + FROM_BRIGHT) as u32).wrapping_add(step as u32)
            };
            m.w16(this + BRIGHT, v as u16);
        }
        if m.r32(this + BRIGHT) != 0 {
            return;
        }
        let video = m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_ABORT, &[video], 0x8004_2220);
        let m = &mut c.m;
        if m.r32(this + WAITING) == 0 {
            let t = m.r32(GAME_TICKS);
            m.w32(this + WAITING, 1);
            m.w32(this + ASKED_AT, t);
            let id = m.r8(info) as i8 as i32 as u32;
            m.w16(this + PLAYING, id as u16);
        }
    })
}

/// `tScreenTrackSelect::UpdateVideoWall` (SCREENTRACKS.CPP, 0x80042268): a new track's picture
/// loaded, the wall turned off and the settings to show again, going dark, no video waiting.
pub fn abi_update_video_wall(c: &mut Cpu) {
    let (this, info) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = info;
        let m = &mut c.m;
        let id = m.r8(info) as i8 as i32 as u32;
        if id == lh(m, this + PICTURE) {
            return;
        }
        port(c, &r, 0x8002_5dd8, 0x8004_229c, &[this, info + 8], fescreen::abi_async_load_swap_shape_file);
        let m = &mut c.m;
        m.w32(this + WALL_SET, 0);
        let id = m.r8(info) as i8 as i32 as u32;
        m.w16(this + PICTURE, id as u16);
        port(c, &r, 0x8003_7050, 0x8004_22b8, &[this + WALL], fevideowall::abi_turn_off);
        port(c, &r, 0x8004_2148, 0x8004_22c4, &[this, 0], abi_set_brightness);
        c.m.w32(this + WAITING, 0);
    })
}

/// `tScreenTrackSelect::DrawVideoWall` (SCREENTRACKS.CPP, 0x800422dc): the track's length text,
/// the background picture, a dark band, the wall (its settings shown once the picture is up).
pub fn abi_draw_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let info = sp + 24;
        let k = c.m.r8(CHOSEN) as u32;
        get_track(c, &r, k, info, 0x8004_2318);
        let text = (c.m.r8(info + 42) as u32).wrapping_add(67);
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_entry, &[text, 478, 33, 1, 0, 11], 0x8004_233c);
        let image = c.m.r32(this);
        c.m.w32(sp + 16, 150);
        port(c, &r, 0x8002_5ce8, 0x8004_2358, &[this, 0, 28, image], fescreen::abi_draw_background_image);
        r.0[2] = 1;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, 320, 30, 160, 10, 1], 0x8004_237c);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8004_2384);
        let wall = this + WALL;
        r.0[0] = wall;
        if c.m.r16(this + SWAP_UPLOADED) & 1 != 0 && c.m.r32(this + WALL_SET) == 0 {
            let available = c.m.r8(info + 3) as u32;
            port(c, &r, 0x8003_6bcc, 0x8004_23b4, &[wall, available], fevideowall::abi_set_available);
            port(c, &r, 0x8003_6a38, 0x8004_23bc, &[wall], fevideowall::abi_update_images);
            c.m.w32(this + WALL_SET, 1);
        }
        port(c, &r, 0x8003_6bdc, 0x8004_23cc, &[wall], fevideowall::abi_update);
        port(c, &r, 0x8003_6dac, 0x8004_23d4, &[wall], fevideowall::abi_draw);
    })
}

/// `tScreenTrackSelect::ProcessInput` (SCREENTRACKS.CPP, 0x800423ec): options: the options item
/// disabled (bit 0) with two players and the second on its own setting, a track of class 3 or
/// more, a track flagged (+4), or one player in a race of kind 1; the race item enabled but in a
/// race of kind 1 with local speech. Back: the wall off at once unless the menu's virtual
/// answers 1.
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let key = c.m.r32(key);
        if key == KEY_OPTIONS {
            r.0[0] = FRONT_END;
            let info = sp + 16;
            let k = c.m.r8(CHOSEN) as u32;
            get_track(c, &r, k, info, 0x8004_242c);
            let m = &mut c.m;
            let menus = menus(m);
            let flags = m.r32(menus + OPTIONS_FLAGS) & !1;
            m.w32(menus + OPTIONS_FLAGS, flags);
            if m.r8(PLAYERS) != 1 && m.r8(SECOND) == 2 {
                m.w32(menus + OPTIONS_FLAGS, flags | 1);
            }
            let on = |m: &mut Mem| {
                let menus = m.r32(SONGS_SCREEN);
                let v = m.r32(menus + OPTIONS_FLAGS) | 1;
                m.w32(menus + OPTIONS_FLAGS, v);
            };
            if m.r8(info + 2) >= 3 {
                on(m);
            }
            if m.r8(info + 4) != 0 {
                on(m);
            }
            let alone = m.r8(PLAYERS) == 1;
            if alone && m.r8(KIND) == 1 {
                on(m);
            }
            let race_off = m.r8(KIND) == 1 && r.call_leaving(c, LOCAL_SPEECH, &[], 0x8004_2518) != 0;
            let m = &mut c.m;
            let menus = m.r32(SONGS_SCREEN);
            let v = m.r32(menus + RACE_FLAGS);
            m.w32(menus + RACE_FLAGS, if race_off { v & !1 } else { v | 1 });
        } else if key == KEY_BACK {
            let app = c.m.r32(FE_APP);
            let menu = c.m.r32(app + 4);
            let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_ON_BACK);
            if r.call_ptr_leaving(c, f, &[obj], 0x8004_2590) ^ 1 != 0 {
                port(c, &r, 0x8003_7074, 0x8004_25a4, &[this + WALL], fevideowall::abi_turn_off_instant);
            }
        }
    })
}

/// `tScreenTrackSelect::~tScreenTrackSelect` (SCREENTRACKS.H, 0x800425b4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_25c4, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::SHAPES as HELP_SHAPES;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram};

    /// The game's track select screen (its vtable the real one); a table of 64 shapes made from
    /// the help's; outputs.
    const SCREEN: u32 = 0x8017_FBD0;
    const TABLE: u32 = 0x8014_5600;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen (never shown in the RAM image): its shapes (the
    /// help's, with sane sizes and depths), its wall set up as `Initialize` does and turned on or
    /// off (now and then turning off, dark); the tracks available or not; the swapped shapes the
    /// shown screen's (its file read or not, no read going) and uploaded or not, the video's state and frames, the
    /// pictures' and video's tracks, the brightness on its way (from before its start to after
    /// its end), the video waiting or not and asked about 256 ticks ago, the settings shown; the
    /// track list, the chosen one, the players, the race's kind, the second player's setting, the
    /// items' flags.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let help = m.r32(HELP_SHAPES);
        for k in 0..64 * 32 {
            let b = m.r8(help + k % (8 * 32));
            m.w8(TABLE + k, b);
        }
        for k in 0..64 {
            let s = TABLE + 32 * k;
            m.w8(s + 8, [4, 8, 16][pick(rng, 3) as usize]);
            m.w16(s + 18, (1 + pick(rng, 100)) as u16);
        }
        let s = SCREEN;
        m.w32(s, TABLE);
        m.w32(s + SWAP, TABLE);
        let wall = s + WALL;
        crate::abi::call(c, nfs4rc::f_80036988, &[wall, s + TV_CONFIG, TABLE, 0, 10, WALL_LAYOUT, 150]);
        crate::abi::call(c, nfs4rc::f_80036b84, &[wall, 249, 320, 80]);
        crate::abi::call(c, nfs4rc::f_80036ba0, &[wall, 38, 10, 310, 60, TABLE]);
        if pick(rng, 2) == 0 {
            crate::abi::call(c, nfs4rc::f_800370f0, &[wall]);
        }
        let m = &mut c.m;
        for k in 0..16 {
            m.w32(TRACK_MANAGER + 8 + 4 * k, pick(rng, 2));
        }
        let now = m.r32(GAME_TICKS);
        let app = m.r32(FE_APP);
        let shown = m.r32(app + 12);
        for k in (0..40).step_by(4) {
            let v = m.r32(shown + SWAP + k);
            m.w32(s + SWAP + k, v);
        }
        let buffer = m.r32(shown + SWAP + 8);
        m.w32(s + SWAP, TABLE);
        m.w32(s + SWAP + 12, 0);
        m.w32(s + SWAP_DATA, [0, buffer][pick(rng, 2) as usize]);
        m.w16(s + SWAP_UPLOADED, pick(rng, 2) as u16);
        m.w32(s + VIDEO, 0x8018_7C08);
        m.w32(s + FRAMES, pick(rng, 100));
        let tracks = m.r32(TRACK_MANAGER);
        m.w16(s + PICTURE, [0xFFFF, pick(rng, tracks.max(1)) as u16][pick(rng, 2) as usize]);
        m.w16(s + PLAYING, pick(rng, tracks.max(1)) as u16);
        m.w16(s + BRIGHT, [0, 1, 32, 64, 128][pick(rng, 5) as usize]);
        m.w16(s + DEST_BRIGHT, [0, 32, 128][pick(rng, 3) as usize]);
        m.w16(s + FROM_BRIGHT, [0, 32, 128, pick(rng, 129) as u16][pick(rng, 4) as usize]);
        m.w32(s + BRIGHT_AT, now.wrapping_sub([0, 1, 64, 127, 128, 500, (-5i32) as u32][pick(rng, 7) as usize]));
        if pick(rng, 3) == 0 {
            // turning off, dark
            m.w16(wall + WALL_STATE, 0xFFFF);
            m.w16(s + BRIGHT, 0);
            m.w16(s + DEST_BRIGHT, 0);
        }
        m.w32(s + WAITING, pick(rng, 2));
        m.w32(s + WALL_SET, pick(rng, 2));
        m.w32(s + ASKED_AT, now.wrapping_sub([0, 256, 257, 1000][pick(rng, 4) as usize]));
        for k in 0..4 {
            m.w8(TRACKS + k, pick(rng, tracks.max(1)) as u8);
        }
        m.w8(CHOSEN, pick(rng, 4) as u8);
        m.w8(PLAYERS, pick(rng, 3) as u8);
        m.w8(KIND, pick(rng, 3) as u8);
        m.w8(SECOND, pick(rng, 3) as u8);
        let menus = menus(m);
        m.w32(menus + OPTIONS_FLAGS, pick(rng, 4));
        m.w32(menus + RACE_FLAGS, pick(rng, 4));
        m.w32(VIDEO_ANSWERS, [0, 1, 2, 3][pick(rng, 4) as usize]);
        m.w32(VIDEO_ANSWERS + 4, pick(rng, 2));
        m.w32(VIDEO_ANSWERS + 8, 0x8018_7C08);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTrackSelect::DrawBackground", 0x8004_19e0, nfs4rc::f_800419e0, |_, _| vec![SCREEN]),
            ("tScreenTrackSelect::GetShapeInfo", 0x8004_1e8c, nfs4rc::f_80041e8c, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTrackSelect::Initialize", 0x8004_1f4c, nfs4rc::f_80041f4c, |_, _| vec![SCREEN]),
            ("tScreenTrackSelect::Cleanup", 0x8004_2108, nfs4rc::f_80042108, |_, _| vec![SCREEN]),
            ("tScreenTrackSelect::SetBrightness", 0x8004_2148, nfs4rc::f_80042148, |_, r| vec![SCREEN, [0, 32, 128][pick(r, 3) as usize]]),
            ("tScreenTrackSelect::UpdateBrightness", 0x8004_2178, nfs4rc::f_80042178, |c, r| {
                c.m.w8(OUT, pick(r, 8) as u8);
                vec![SCREEN, OUT]
            }),
            ("tScreenTrackSelect::UpdateVideoWall", 0x8004_2268, nfs4rc::f_80042268, |c, r| {
                let id = [c.m.r8(SCREEN + PICTURE), pick(r, 8) as u8][pick(r, 2) as usize];
                c.m.w8(OUT, id);
                vec![SCREEN, OUT]
            }),
            ("tScreenTrackSelect::DrawVideoWall", 0x8004_22dc, nfs4rc::f_800422dc, |_, _| vec![SCREEN]),
            ("tScreenTrackSelect::ProcessInput", 0x8004_23ec, nfs4rc::f_800423ec, |c, r| {
                c.m.w32(OUT, [KEY_OPTIONS, KEY_BACK, 1][pick(r, 3) as usize]);
                vec![SCREEN, 0, OUT, OUT + 4]
            }),
            ("~tScreenTrackSelect", 0x8004_25b4, nfs4rc::f_800425b4, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screentracks_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x830 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x831 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
