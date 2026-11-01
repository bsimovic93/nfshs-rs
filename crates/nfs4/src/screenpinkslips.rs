//! SCREENPINKSLIPS.CPP: the pink slips' track screen (`tScreenPinkSlips`): the tracks of the
//! pink slips' race listed in boxes (the menu's one haloed), the chosen track's video playing
//! over eight TVs (two rows of four), four TVs turned on one by one with the track's pictures
//! (`TR%02dPS`, swapped in when the track changes) and off again, the lines between them.
//! Options asks for the race only on tracks it allows.
//!
//! The videos, `ClearImage`, `DrawSync`, `DrawTV` and the squares are the host's (called by
//! address).

use crate::abi::{self, Saved};
use crate::femenu::KEY_OPTIONS;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TRACK_MANAGER};
use crate::{drawshp, fescreen, fetextrender, fetracks, fetv};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen` (+40 its swapped shapes: +40 their table, +44 the file read, +58
// uploaded (bit 0); +92 its fade), then +100 the menu, +104 the video, +108 its frames shown,
// +112 the track whose pictures are swapped in, +114 and +116 brightnesses (cleared), +124 the
// tick the picture TVs last turned, +128 turning on (1), off (-1) or not, +132 the video's 8 TVs
// and +516 the pictures' 4 (FETV's, 48 bytes each), +708 the pictures' TVs set.
const SWAP_TABLE: u32 = 40;
const SWAP_DATA: u32 = 44;
const SWAP_UPLOADED: u32 = 58;
const FADE: u32 = 92;
const MENU: u32 = 100;
const VIDEO: u32 = 104;
const FRAMES: u32 = 108;
const PREVIOUS: u32 = 112;
const BRIGHT: u32 = 114;
const DEST_BRIGHT: u32 = 116;
const TURNED_AT: u32 = 124;
const TURNING: u32 = 128;
const TRACK_TVS: u32 = 132;
const IMAGE_TVS: u32 = 516;
const TVS_SET: u32 = 708;

// A TV (FETV's): +0 its state (0 off, 1 on), +8 its flags, +16 its colour, +20 its place and
// size, +28 its texture's corner and size, +34 its CLUT, +36 its page.
const TV_SIZE: u32 = 48;
const TV_FLAGS: u32 = 8;
const TV_COLOR: u32 = 16;
const TV_PLACE: u32 = 20;
const TV_UV: u32 = 28;
const TV_CLUT: u32 = 34;
const TV_PAGE: u32 = 36;

/// The pink slips' tracks (`frontEnd + 0x23`, ids), the one chosen (`+0x46`), how many
/// (`+0x47`); the race's players (`+3`), its kind (`+4`), the second player's setting
/// (`+0x31e`).
const TRACKS: u32 = FRONT_END + 0x23;
const CHOSEN: u32 = FRONT_END + 0x46;
const COUNT: u32 = FRONT_END + 0x47;
const PLAYERS: u32 = FRONT_END + 3;
const KIND: u32 = FRONT_END + 4;
const SECOND: u32 = FRONT_END + 0x31E;
/// In the menus' block (`SONGS_SCREEN`'s): the track iterator, two items' flags (bit 0 off).
const ITERATOR: u32 = 3208;
const RACE_ITEM_FLAGS: u32 = 3916;
const OTHER_ITEM_FLAGS: u32 = 3952;
/// The halo's clock (0..60), the swapped file's name, the order the picture TVs turn in.
const HALO: u32 = 0x8005_3080;
const SWAP_NAME: u32 = 0x8005_3090;
const TV_ORDER: u32 = 0x8005_22F0;
/// "%szzzTR%02d.dct", "zPink", "TR%02dPS"; the videos' directory (`Paths_Paths + 0xa4`).
const VIDEO_FORMAT: u32 = 0x8001_18B4;
const PINK_NAME: u32 = 0x8001_18C4;
const SWAP_FORMAT: u32 = 0x8001_18CC;
const VIDEO_PATH: u32 = PATHS + 0xA4;
/// "LOADING".
const LOADING_TEXT: u32 = 638;

/// The host's: the videos, `ClearImage`, `DrawSync`, `DrawTV`, `PSXDrawSquare`,
/// `PSXDrawTransSquare`.
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
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `GetTPage`, `Flare_2DHalo`, `purgememadr`, `AudioMus_Buffered`, `AudioMus_Threshold`,
/// `FeAudio_systemtask`.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
/// `Flare_2DHalo`, hooked (the host captures its primitives): called by address.
const HALO_2D: u32 = 0x800C_ECD0;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const BUFFERED: fn(&mut Cpu) = nfs4rc::f_8007aa48;
const THRESHOLD: fn(&mut Cpu) = nfs4rc::f_8007a9c4;
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// A track's id (`tTrackInformation::fTrackID`, a `char`).
fn track_id(m: &mut Mem, info: u32) -> u32 {
    m.r8(info) as i8 as i32 as u32
}

/// The menus' block.
fn menus(m: &mut Mem) -> u32 {
    m.r32(SONGS_SCREEN)
}

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `tScreenPinkSlips::DrawBackground` (SCREENPINKSLIPS.CPP, 0x80038d60): "LOADING" while the
/// screen comes up; the tracks in boxes, the menu's selected one haloed (the halo breathing);
/// the chosen track's pictures swapped in, uploaded and turned on; the video's next frame over
/// the eight TVs (or, a while after it ended, the video again); the lines and the TVs.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 232, |c, sp| {
        let mut r = Saved::of(c);
        r.0[8] = this;
        if lh(&mut c.m, this + FADE) >= 100 {
            r.call_leaving(c, fetextrender::menu_text_positioned_justify_entry, &[LOADING_TEXT, 480, 220, 1, 1, 4], 0x8003_8dc8);
            let w = r.call_leaving(c, TEXT_WORD, &[LOADING_TEXT], 0x8003_8dd0);
            let px = r.call_leaving(c, TEXT_PIXELS, &[w], 0x8003_8dd8);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(px).wrapping_sub(5), 7], 0x8003_8df8);
        }
        r.0[2] = 0;
        // the tracks
        r.0[4] = FRONT_END;
        r.0[3] = 0x8005_0000;
        let rect = sp + 32;
        for (k, v) in [347u16, 143, 144, 14].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        let info = sp + 40;
        loop {
            let k = sx(r.0[2]);
            r.0[0] = 1;
            if k as i32 >= c.m.r8(COUNT) as i32 {
                break;
            }
            let m = &mut c.m;
            let menu = m.r32(this + MENU);
            let n = lh(m, menu + 8);
            if k as i32 == n - 1 || (k == 0 && n == 0) {
                let halo = m.r32(HALO).wrapping_add(1) as i32;
                m.w32(HALO, halo as u32);
                if halo >= 61 {
                    m.w32(HALO, 0);
                }
                let h = m.r32(HALO) as i32;
                let size = (if h < 31 { h } else { 60i32.wrapping_sub(h) }) / 2 + 20;
                let size = size.wrapping_mul(128 - lh(m, this + FADE));
                if size > 0 {
                    let (x, y) = (lh(m, rect) as u32, lh(m, rect + 2) as u32);
                    r.0[0] = x;
                    r.0[1] = y;
                    r.call_ptr_leaving(c, HALO_2D, &[x.wrapping_sub(15), y.wrapping_add(6), (size / 2) as u32, (size.wrapping_mul(2) / 3) as u32, 23], 0x8003_8f20);
                    let fade = lh(&mut c.m, this + FADE) as u32;
                    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[56, 0, x.wrapping_sub(18), y, fade, 1, 0], 0x8003_8f48);
                }
                r.0[0] = 2;
            }
            let id = c.m.r8(TRACKS.wrapping_add(k)) as u32;
            port(c, &r, 0x8002_1c04, 0x8003_8f6c, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
            c.m.w8(CHOSEN, r.0[2] as u8);
            let it = menus(&mut c.m) + ITERATOR;
            let text = port(c, &r, 0x8002_1f34, 0x8003_8f84, &[it, u32::MAX], fetracks::abi_list_iterator_track_text_value);
            let m = &mut c.m;
            let (x, y) = (sx((m.r16(rect) as u32).wrapping_add(10)), sx((m.r16(rect + 2) as u32).wrapping_add(3)));
            r.call_leaving(c, fetextrender::menu_text_positioned_justify_entry, &[sx(text), x, y, 0, r.0[0], 1], 0x8003_8fc0);
            port(c, &r, 0x8004_c490, 0x8003_8fd0, &[u32::MAX, rect, 1], drawshp::abi_round_rectangle);
            r.0[2] = r.0[2].wrapping_add(1);
            let y = c.m.r16(rect + 2).wrapping_add(14);
            c.m.w16(rect + 2, y);
        }
        // the chosen track's pictures
        let m = &mut c.m;
        let menu = m.r32(this + MENU);
        let n = m.r16(menu + 8) as u32;
        m.w8(CHOSEN, n as u8);
        if n & 0xFF != 0 {
            m.w8(CHOSEN, n.wrapping_sub(1) as u8);
        }
        r.0[0] = info;
        let chosen = m.r8(CHOSEN) as u32;
        let id = m.r8(TRACKS.wrapping_add(chosen)) as u32;
        port(c, &r, 0x8002_1c04, 0x8003_902c, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
        port(c, &r, 0x8003_95fc, 0x8003_9038, &[this, info], abi_update_video_wall);
        port(c, &r, 0x8002_5e2c, 0x8003_9044, &[this, this + SWAP_TABLE], fescreen::abi_is_shape_file_loaded);
        if c.m.r32(this + SWAP_DATA) != 0 && (c.m.r8(this + TURNING) as i8) >= 0 {
            port(c, &r, 0x8002_5ff0, 0x8003_906c, &[this, 4], fescreen::abi_upload_swap_shapes);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w8(this + TURNING, 1);
            c.m.w32(this + TURNED_AT, ticks);
        }
        port(c, &r, 0x8003_96a8, 0x8003_908c, &[this], abi_draw_video_wall);
        // the video
        let y = if c.m.r32(this + FRAMES) & 1 == 0 { 128 } else { 0 };
        r.0[0] = y;
        c.m.w16(sp + 168, y as u16);
        let video = c.m.r32(this + VIDEO);
        if r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8003_90a8) != 0 {
            let video = c.m.r32(this + VIDEO);
            if r.call_ptr_leaving(c, VIDEO_UPDATE, &[video, 512, y], 0x8003_90c0) != 0 {
                let f = c.m.r32(this + FRAMES).wrapping_add(1);
                c.m.w16(sp + 168, if f & 1 == 0 { 128 } else { 0 });
                c.m.w32(this + FRAMES, f);
            }
            let v = c.m.r8(sp + 168);
            c.m.w8(sp + 176, v);
            r.0[3] = 0;
            for row in 0..2u32 {
                r.0[2] = row;
                r.0[5] = row;
                r.0[4] = row << 6;
                r.0[7] = (row << 6).wrapping_add(v as u32);
                r.0[6] = 63;
                for col in 0..4u32 {
                    r.0[1] = col;
                    let at = r.0[3];
                    let t = this + TRACK_TVS + TV_SIZE * at;
                    r.0[0] = this + TV_SIZE * at;
                    let m = &mut c.m;
                    m.w16(t + TV_PLACE, (80 * col + 160) as u16);
                    m.w16(t + TV_PLACE + 2, (64 * row + 25) as u16);
                    m.w16(t + TV_PLACE + 4, 80);
                    m.w16(t + TV_PLACE + 6, 64);
                    m.w8(t + TV_UV, (40 * col) as u8);
                    m.w8(t + TV_UV + 1, r.0[7] as u8);
                    m.w8(t + TV_UV + 2, 40);
                    m.w8(t + TV_UV + 3, if row == 1 { 63 } else { 64 });
                    m.w32(sp + 184, 1);
                    m.w32(sp + 188, 64);
                    let page_y = sx(m.r16(sp + 168) as u32);
                    let page = r.call_leaving(c, GET_TPAGE, &[2, 0, 512, page_y], 0x8003_91b0);
                    let m = &mut c.m;
                    m.w16(t + TV_PAGE, page as u16);
                    m.w16(t + TV_CLUT, 0);
                    m.w16(t + TV_FLAGS, 32);
                    m.w32(t + TV_COLOR, 0x0080_8080);
                    m.w32(t, 1);
                    r.0[1] = col + 1;
                    r.0[3] = at + 1;
                }
            }
        } else if c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + TURNED_AT)) >= 257 {
            let file = sp + 88;
            r.0[0] = file;
            let (path, id) = (c.m.r32(VIDEO_PATH), track_id(&mut c.m, info));
            r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path, id], 0x8003_9258);
            let video = c.m.r32(this + VIDEO);
            r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8003_9264);
            r.0[2] = 240;
            let video = c.m.r32(this + VIDEO);
            r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8003_9270);
        }
        // the lines and the TVs
        r.0[1] = 129;
        r.0[0] = 1;
        for x in (240..480).step_by(80) {
            r.0[2] = x;
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0030_3030, x, 15, 2, 129, 1], 0x8003_929c);
        }
        r.0[2] = 79;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 160, 79, 321, 1, 1], 0x8003_92e4);
        r.0[2] = 143;
        r.call_leaving(c, SET_ABR_MODE, &[2], 0x8003_9308);
        for k in 0..8 {
            r.0[2] = k;
            r.call_ptr_leaving(c, DRAW_TV, &[this + TRACK_TVS + TV_SIZE * k], 0x8003_932c);
        }
    })
}

/// `tScreenPinkSlips::GetShapeInfo` (SCREENPINKSLIPS.CPP, 0x80039378): 57 shapes of "zPink",
/// 4 of the chosen track's pictures (`TR%02dPS`), that track's the ones swapped in.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, permanent, swap, name) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[5] = this;
        r.0[1] = permanent;
        r.0[2] = swap;
        r.0[4] = name;
        r.0[3] = swap_name;
        let info = sp + 16;
        let id = c.m.r8(TRACKS) as u32;
        port(c, &r, 0x8002_1c04, 0x8003_93c4, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
        r.0[0] = SWAP_NAME;
        let m = &mut c.m;
        m.w16(permanent, 57);
        m.w16(swap, 4);
        m.w32(name, PINK_NAME);
        let id = track_id(m, info);
        r.call_leaving(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, id], 0x8003_93fc);
        let m = &mut c.m;
        m.w32(swap_name, SWAP_NAME);
        let id = track_id(m, info);
        m.w16(this + PREVIOUS, id as u16);
    })
}

/// `tScreenPinkSlips::Initialize` (SCREENPINKSLIPS.CPP, 0x80039438): the video's place
/// cleared; the first track chosen (the iterator walked back and on); the screen's files read;
/// the first track's video made and played; the picture TVs not set, turning on from now.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 176, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let rect = sp + 24;
        for (k, v) in [512u16, 0, 170, 256].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        r.call_ptr_leaving(c, CLEAR_IMAGE, &[rect, 0, 0, 0], 0x8003_9480);
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8003_9488);
        r.0[2] = 0x8005_0000;
        r.0[1] = FRONT_END;
        c.m.w8(CHOSEN, 0);
        let it = menus(&mut c.m) + ITERATOR;
        port(c, &r, 0x8002_2020, 0x8003_94a8, &[it, u32::MAX], fetracks::abi_list_iterator_track_decrement);
        let it = menus(&mut c.m) + ITERATOR;
        port(c, &r, 0x8002_1f74, 0x8003_94b8, &[it, u32::MAX], fetracks::abi_list_iterator_track_increment);
        let menu = {
            let m = &mut c.m;
            let app = m.r32(FE_APP);
            m.r32(app + 4)
        };
        c.m.w32(this + MENU, menu);
        port(c, &r, 0x8002_60cc, 0x8003_94d4, &[this], fescreen::abi_initialize);
        c.m.w32(this + TVS_SET, 0);
        let info = sp + 112;
        let id = c.m.r8(TRACKS) as u32;
        port(c, &r, 0x8002_1c04, 0x8003_94ec, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
        let file = sp + 32;
        r.0[1] = file;
        let (path, id) = (c.m.r32(VIDEO_PATH), track_id(&mut c.m, info));
        r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path, id], 0x8003_950c);
        let video = r.call_ptr_leaving(c, VIDEO_CREATE, &[160, 128, 0xF_0000, 0x2_0000, 16], 0x8003_9528);
        c.m.w32(this + VIDEO, video);
        r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8003_9538);
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8003_9544);
        let m = &mut c.m;
        let ticks = m.r32(GAME_TICKS);
        m.w32(this + FRAMES, 0);
        m.w16(this + BRIGHT, 0);
        m.w16(this + DEST_BRIGHT, 0);
        m.w32(this + TVS_SET, 0);
        m.w8(this + TURNING, 1);
        m.w32(this + TURNED_AT, ticks);
    })
}

/// `tScreenPinkSlips::Cleanup` (SCREENPINKSLIPS.CPP, 0x80039580): the video destroyed and
/// freed, the first track chosen; the music's buffer filled to its threshold; the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_DESTROY, &[video], 0x8003_95a0);
        let video = c.m.r32(this + VIDEO);
        r.call_leaving(c, PURGE, &[video], 0x8003_95ac);
        c.m.w8(CHOSEN, 0);
        loop {
            let buffered = r.call_leaving(c, BUFFERED, &[], 0x8003_95bc);
            r.0[0] = buffered;
            let threshold = r.call_leaving(c, THRESHOLD, &[], 0x8003_95c4);
            r.0[0] = ((buffered as i32) < threshold as i32) as u32;
            if r.0[0] == 0 {
                break;
            }
            r.call_leaving(c, SYSTEM_TASK, &[0], 0x8003_95d8);
        }
        port(c, &r, 0x8002_61c0, 0x8003_95e8, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenPinkSlips::UpdateVideoWall` (SCREENPINKSLIPS.CPP, 0x800395fc): a track other than
/// the one swapped in: its pictures read, the picture TVs to be set again and turned off (unless
/// they are), the video stopped.
pub fn update_video_wall(c: &mut Cpu, this: u32, info: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[2] = info;
        let id = track_id(&mut c.m, info);
        if id as i32 == lh(&mut c.m, this + PREVIOUS) {
            return;
        }
        r.0[0] = SWAP_NAME;
        r.call_leaving(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, id], 0x8003_9648);
        port(c, &r, 0x8002_5dd8, 0x8003_9654, &[this, SWAP_NAME], fescreen::abi_async_load_swap_shape_file);
        let m = &mut c.m;
        m.w32(this + TVS_SET, 0);
        let id = track_id(m, info);
        m.w16(this + PREVIOUS, id as u16);
        if (m.r8(this + TURNING) as i8) >= 0 {
            let ticks = m.r32(GAME_TICKS);
            m.w8(this + TURNING, 0xFF);
            m.w32(this + TURNED_AT, ticks);
        }
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_ABORT, &[video], 0x8003_9690);
    })
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenPinkSlips::DrawVideoWall` (SCREENPINKSLIPS.CPP, 0x800396a8): 36 shapes drawn with
/// nothing (the original's); once the pictures are uploaded, the picture TVs set from them; the
/// TVs turned on in order, one more every 4 ticks (or off the same way, done after 8 ticks);
/// the picture TVs drawn.
pub fn draw_video_wall(c: &mut Cpu, this: u32) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        for k in 0..36 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, 0, 0, 0, 0, 0, 0], 0x8003_96f0);
        }
        r.0[0] = 36;
        if c.m.r16(this + SWAP_UPLOADED) & 1 != 0 && c.m.r32(this + TVS_SET) == 0 {
            for k in 0..4 {
                r.0[0] = k;
                let (t, table) = (this + IMAGE_TVS + TV_SIZE * k, c.m.r32(this + SWAP_TABLE));
                r.enter_leaving(c, 0x8002_3168, 0x8003_9758, |c| fetv::init_tv(c, t, table, k));
            }
            r.0[0] = 4;
            c.m.w32(this + TVS_SET, 1);
        }
        let n = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + TURNED_AT)) >> 2;
        r.0[0] = n;
        let tv = |m: &mut Mem, k: u32| this + IMAGE_TVS + TV_SIZE * m.r8(TV_ORDER + k) as u32;
        let count = sx(n) as i32;
        if (c.m.r8(this + TURNING) as i8) > 0 {
            r.0[1] = 0;
            if count > 0 {
                r.0[3] = TV_ORDER;
                r.0[0] = count as u32;
                loop {
                    let k = sx(r.0[1]);
                    if k as i32 >= 4 {
                        break;
                    }
                    let t = tv(&mut c.m, k);
                    if c.m.r32(t) == 0 {
                        port(c, &r, 0x8002_3388, 0x8003_9808, &[t], fetv::abi_turn_on_tv);
                    }
                    r.0[1] = r.0[1].wrapping_add(1);
                    if sx(r.0[1]) as i32 >= count {
                        break;
                    }
                }
            }
        } else {
            r.0[1] = 0;
            if count > 0 {
                r.0[4] = TV_ORDER;
                r.0[3] = count as u32;
                loop {
                    let k = sx(r.0[1]);
                    if k as i32 >= 4 {
                        break;
                    }
                    let t = tv(&mut c.m, k);
                    if c.m.r32(t) == 1 {
                        port(c, &r, 0x8002_3378, 0x8003_9890, &[t], fetv::abi_turn_off_tv);
                    }
                    r.0[1] = r.0[1].wrapping_add(1);
                    if sx(r.0[1]) as i32 >= count {
                        break;
                    }
                }
            }
            if count >= 8 {
                c.m.w8(this + TURNING, 0);
            }
        }
        for k in 0..4 {
            r.0[0] = k;
            r.call_ptr_leaving(c, DRAW_TV, &[this + IMAGE_TVS + TV_SIZE * k], 0x8003_98e8);
        }
    })
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, c.r[4]);
}

/// `tScreenPinkSlips::ProcessInput` (SCREENPINKSLIPS.CPP, 0x80039924): on options, the race's
/// item turned off on tracks too hard, bonus tracks, a second player set so and a one-player
/// race of kind 1 (on otherwise), the other item off.
pub fn abi_process_input(c: &mut Cpu) {
    let key = c.r[6];
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        if c.m.r32(key) != KEY_OPTIONS {
            return;
        }
        r.0[0] = FRONT_END;
        let info = sp + 16;
        let id = {
            let m = &mut c.m;
            let chosen = m.r8(CHOSEN) as u32;
            m.r8(TRACKS.wrapping_add(chosen)) as u32
        };
        port(c, &r, 0x8002_1c04, 0x8003_9960, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
        let m = &mut c.m;
        let off = |m: &mut Mem| {
            let flags = menus(m) + RACE_ITEM_FLAGS;
            let v = m.r32(flags) | 1;
            m.w32(flags, v);
        };
        let flags = menus(m) + RACE_ITEM_FLAGS;
        let v = m.r32(flags) & !1;
        m.w32(flags, v);
        if m.r8(PLAYERS) != 1 && m.r8(SECOND) == 2 {
            m.w32(flags, v | 1);
        }
        if m.r8(info + 2) >= 3 {
            off(m);
        }
        if m.r8(info + 4) != 0 {
            off(m);
        }
        if m.r8(PLAYERS) == 1 && m.r8(KIND) == 1 {
            off(m);
        }
        let flags = menus(m) + OTHER_ITEM_FLAGS;
        let v = m.r32(flags) | 1;
        m.w32(flags, v);
    })
}

/// `tScreenPinkSlips::~tScreenPinkSlips` (SCREENPINKSLIPS.H, 0x80039a60): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8003_9a70, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, TOP, pick, player, via_abi};
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram, same_stack_in};

    /// The game's pink slips screen (never set up in the front end's RAM, its vtable the
    /// real one); the outputs of `GetShapeInfo`; the menu it reads (FEMENU's test menu).
    const SCREEN: u32 = 0x8018_1090;
    const OUT: u32 = 0x8014_7000;
    const TEST_MENU: u32 = base::MENU;
    /// The music library's state (`AudioMus_g`, gp + 472): +136 playing.
    const MUSIC: u32 = 0x8013_D83C + 472;

    /// FEMENU's structures, then the screen in any state: the car select's swapped shapes and
    /// file (read or not, uploaded or not), its fade about the loading text's, the video's
    /// frames, the track swapped in the chosen one's or another, the picture TVs turning
    /// either way or not about each time a TV turns, set or not, each on or off; the eight
    /// video TVs on or off; the pink slips' tracks (none to six of the manager's ten) and the
    /// one chosen, the menu's item about them, the halo's clock about its turns; the race's
    /// players, kind and second player, the items' flags; the music playing or not; the video's
    /// answers; the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12) + SWAP_TABLE;
        let s = SCREEN;
        for k in (0..40).step_by(4) {
            let v = m.r32(from + k);
            m.w32(s + SWAP_TABLE + k, v);
        }
        let buffer = m.r32(from + 8);
        m.w32(s + SWAP_TABLE + 12, 0);
        m.w32(s + SWAP_DATA, [0, buffer][pick(rng, 2) as usize]);
        let up = (m.r16(s + SWAP_UPLOADED) & !1) | pick(rng, 2) as u16;
        m.w16(s + SWAP_UPLOADED, up);
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        m.w16(s + FADE, [0, 99, 100, 128, pick(rng, 129)][pick(rng, 5) as usize] as u16);
        m.w32(s + MENU, TEST_MENU);
        m.w32(s + VIDEO, 0x8018_7C08);
        m.w32(s + FRAMES, pick(rng, 100));
        let n = [0, 1, 3, 6, pick(rng, 7)][pick(rng, 5) as usize];
        for k in 0..8 {
            m.w8(TRACKS + k, pick(rng, 10) as u8);
        }
        m.w8(COUNT, n as u8);
        m.w8(CHOSEN, pick(rng, n.max(1)) as u8);
        let chosen = m.r8(CHOSEN) as u32;
        let chosen = m.r8(TRACKS + chosen) as u32;
        let prev = if pick(rng, 2) == 0 { chosen } else { pick(rng, 18).wrapping_sub(1) };
        m.w16(s + PREVIOUS, prev as u16);
        m.w32(TEST_MENU + 8, [0, 1, n, n + 1, 256, pick(rng, 8)][pick(rng, 6) as usize]);
        // (now and then the music stopped: its buffer and threshold both 0)
        let music = m.r32(MUSIC);
        if pick(rng, 3) == 0 {
            m.w32(music + 136, 0);
        }
        m.w32(HALO, [0, 29, 30, 31, 59, 60, pick(rng, 61)][pick(rng, 7) as usize]);
        m.w8(s + TURNING, [0xFF, 0, 1, pick(rng, 256)][pick(rng, 4) as usize] as u8);
        let ago = [0, 3, 4, 15, 16, 31, 32, 256, 257, pick(rng, 400)][pick(rng, 10) as usize];
        m.w32(s + TURNED_AT, ticks.wrapping_sub(ago));
        m.w32(s + TVS_SET, pick(rng, 2));
        // (the TVs set from the shapes, as the game's are: the screen was never set up in the
        // front end's RAM, and TVs of any size draw over everything)
        let table = m.r32(s + SWAP_TABLE);
        for k in 0..4 {
            crate::abi::call(c, nfs4rc::f_80023168, &[s + IMAGE_TVS + TV_SIZE * k, table, k]);
            c.m.w32(s + IMAGE_TVS + TV_SIZE * k, [0, 1, 1, 2, 3][pick(rng, 5) as usize]);
        }
        for k in 0..8 {
            crate::abi::call(c, nfs4rc::f_80023168, &[s + TRACK_TVS + TV_SIZE * k, table, k]);
            c.m.w32(s + TRACK_TVS + TV_SIZE * k, pick(rng, 2));
        }
        let m = &mut c.m;
        m.w8(PLAYERS, [0, 1, 2][pick(rng, 3) as usize]);
        m.w8(KIND, pick(rng, 2) as u8);
        m.w8(SECOND, pick(rng, 4) as u8);
        let ms = menus(m);
        m.w32(ms + RACE_ITEM_FLAGS, [0x400, 0x401, rng.next() as u32][pick(rng, 3) as usize]);
        m.w32(ms + OTHER_ITEM_FLAGS, [0x400, 0x401, rng.next() as u32][pick(rng, 3) as usize]);
        m.w32(VIDEO_ANSWERS, [0, 1, 2, 3][pick(rng, 4) as usize]);
        m.w32(VIDEO_ANSWERS + 4, pick(rng, 2));
        m.w32(VIDEO_ANSWERS + 8, 0x8018_7C08);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenPinkSlips::DrawBackground", 0x8003_8d60, nfs4rc::f_80038d60, |_, _| vec![SCREEN]),
            ("tScreenPinkSlips::GetShapeInfo", 0x8003_9378, nfs4rc::f_80039378, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenPinkSlips::Initialize", 0x8003_9438, nfs4rc::f_80039438, |_, _| vec![SCREEN]),
            ("tScreenPinkSlips::Cleanup", 0x8003_9580, nfs4rc::f_80039580, |_, _| vec![SCREEN]),
            ("UpdateVideoWall", 0x8003_95fc, nfs4rc::f_800395fc, |c, r| {
                let info = OUT + 16;
                let id = [c.m.r16(SCREEN + PREVIOUS) as u32, pick(r, 18).wrapping_sub(1)][pick(r, 2) as usize];
                c.m.w8(info, id as u8);
                vec![SCREEN, info]
            }),
            ("DrawVideoWall", 0x8003_96a8, nfs4rc::f_800396a8, |_, _| vec![SCREEN]),
            ("tScreenPinkSlips::ProcessInput", 0x8003_9924, nfs4rc::f_80039924, |c, r| {
                if pick(r, 2) == 0 {
                    c.m.w32(KEY, KEY_OPTIONS);
                }
                vec![SCREEN, player(r), KEY, COMMAND]
            }),
            ("~tScreenPinkSlips", 0x8003_9a60, nfs4rc::f_80039a60, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screenpinkslips_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x9157 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x9158 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screenpinkslips_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x9168 + k as u64, shuffle, &args);
        }
    }
}
