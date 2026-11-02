//! SCREENPINKSLIPS.CPP: the pink slips' track screen (`tScreenPinkSlips`). It lists the tracks
//! of the pink slips' race in boxes, the menu's one haloed, and plays the chosen track's video
//! over eight TVs (two rows of four). Four more TVs turn on one by one with the track's pictures
//! (`TR%02dPS`, swapped in when the track changes) and off again, with lines between them.
//! Options asks for the race only on tracks that allow it.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The videos,
//! `ClearImage`, `DrawSync`, `DrawTV` and the squares are the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenu::KEY_OPTIONS;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TRACK_MANAGER};
use crate::types::{FEApplication, RECT, Screen, TVConfig, TrackInformation};
use crate::{fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen` (its swapped shapes the track's pictures), then +100 the menu, +104
/// the video, +108 its frames shown, +112 the track whose pictures are swapped in, +114 and
/// +116 brightnesses (cleared), +124 the tick the picture TVs last turned, +128 turning on (1),
/// off (-1) or not, +132 the video's 8 TVs and +516 the pictures' 4 (`tTVConfig`s, 48 bytes
/// each), +708 the pictures' TVs set.
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
/// A shape file's flags: uploaded; a TV's state on; the video TVs' flags.
const UPLOADED: u16 = 1;
const TV_ON: i32 = 1;
const VIDEO_TV_FLAGS: u16 = 32;

/// The pink slips' tracks (`frontEnd + 0x23`, ids), the one chosen (`+0x46`), how many
/// (`+0x47`); the race's players (`+3`), its kind (`+4`), the second player's setting
/// (`+0x31e`).
const TRACKS: u32 = FRONT_END + 0x23;
const CHOSEN: u32 = FRONT_END + 0x46;
const COUNT: u32 = FRONT_END + 0x47;
const PLAYERS: u32 = FRONT_END + 3;
const KIND: u32 = FRONT_END + 4;
const SECOND: u32 = FRONT_END + 0x31E;
/// A track's description (`tTrackInformation`, 48 bytes).
const TRACK_SIZE: u32 = 48;
/// In the menus' block (`SONGS_SCREEN`'s): the track iterator, two items' flags (bit 0:
/// disabled).
const ITERATOR: u32 = 3208;
const RACE_ITEM_FLAGS: u32 = 3916;
const OTHER_ITEM_FLAGS: u32 = 3952;
const ITEM_DISABLED: u32 = 1;
/// The halo's clock (0..60), the swapped file's name, the order the picture TVs turn in.
const HALO: u32 = 0x8005_3080;
const SWAP_NAME: u32 = 0x8005_3090;
const TV_ORDER: u32 = 0x8005_22F0;
/// "%szzzTR%02d.dct", "zPink" (57 shapes; a track's 4 pictures), "TR%02dPS"; the videos'
/// directory (`Paths_Paths + 0xa4`).
const VIDEO_FORMAT: u32 = 0x8001_18B4;
const PINK_NAME: u32 = 0x8001_18C4;
const SHAPES: u16 = 57;
const PICTURES: u32 = 4;
const SWAP_FORMAT: u32 = 0x8001_18CC;
const VIDEO_PATH: u32 = PATHS + 0xA4;
/// "LOADING"; the halo's shape.
const LOADING_TEXT: u32 = 638;
const HALO_SHAPE: u32 = 56;

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
/// `GetTPage`, `purgememadr`, `AudioMus_Buffered`, `AudioMus_Threshold`, `FeAudio_systemtask`;
/// `Flare_2DHalo`, hooked (the host captures its primitives): called by address.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const BUFFERED: fn(&mut Cpu) = nfs4rc::f_8007aa48;
const THRESHOLD: fn(&mut Cpu) = nfs4rc::f_8007a9c4;
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const HALO_2D: u32 = 0x800C_ECD0;
/// Ports called by address: FETRACKS' `GetTrack` and the track iterator's `TextValue`,
/// `Decrement`, `Increment`; DRAWSHP's `RoundRectangle`; FETV's `InitTV`, `TurnOnTV`,
/// `TurnOffTV`.
const GET_TRACK: u32 = 0x8002_1C04;
const TRACK_TEXT_VALUE: u32 = 0x8002_1F34;
const TRACK_DECREMENT: u32 = 0x8002_2020;
const TRACK_INCREMENT: u32 = 0x8002_1F74;
const ROUND_RECTANGLE: u32 = 0x8004_C490;
const INIT_TV: u32 = 0x8002_3168;
const TURN_ON_TV: u32 = 0x8002_3388;
const TURN_OFF_TV: u32 = 0x8002_3378;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// A track's id (`fTrackID`, a `char`).
fn track_id(m: &mut Mem, info: TrackInformation) -> u32 {
    info.track_id(m) as i32 as u32
}

/// The menus' block.
fn menus(m: &mut Mem) -> u32 {
    m.r32(SONGS_SCREEN)
}

/// The screen's fade.
fn fade(m: &mut Mem, this: u32) -> i32 {
    Screen(this).screen_fade_val(m) as i32
}

/// A buffer for a track's description.
fn with_track_buffer<R>(c: &mut Cpu, f: impl FnOnce(&mut Cpu, TrackInformation) -> R) -> R {
    with_buffer(c, TRACK_SIZE, |c, info| f(c, TrackInformation(info)))
}

/// Track `id`'s description into `info`.
fn get_track(c: &mut Cpu, id: u32, info: TrackInformation) {
    call_addr(c, GET_TRACK, &[TRACK_MANAGER, id, info.0]);
}

/// A TV of the video's and of the pictures'.
fn track_tv(this: u32, k: u32) -> TVConfig {
    TVConfig(this + TRACK_TVS).at(k)
}

fn image_tv(this: u32, k: u32) -> TVConfig {
    TVConfig(this + IMAGE_TVS).at(k)
}

/// A track's video spooled from its file and started.
fn start_video(c: &mut Cpu, this: u32, id: u32) {
    with_buffer(c, 128, |c, file| {
        let path = c.m.r32(VIDEO_PATH);
        call(c, SPRINTF, &[file, VIDEO_FORMAT, path, id]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    })
}

/// `tScreenPinkSlips::DrawBackground` (SCREENPINKSLIPS.CPP, 0x80038d60). It shows "LOADING"
/// while the screen comes up; the tracks in boxes, the menu's selected one haloed (the halo
/// breathing); the chosen track's pictures swapped in, uploaded and turned on; the video's next
/// frame over the eight TVs (or, a while after it ended, the video again); the lines and TVs.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    if fade(&mut c.m, this) >= 100 {
        fetextrender::menu_text_positioned_justify(c, LOADING_TEXT, 480, 220, 1, 1, 4);
        let w = call(c, TEXT_WORD, &[LOADING_TEXT]);
        let px = call(c, TEXT_PIXELS, &[w]);
        call_addr(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(px).wrapping_sub(5), 7]);
    }
    with_track_buffer(c, |c, info| {
        draw_tracks(c, this, info);
        // the chosen track's pictures: the menu's current item, less one (but the first)
        let m = &mut c.m;
        let menu = m.r32(this + MENU);
        let n = m.r16(menu + 8) as u32;
        m.w8(CHOSEN, n as u8);
        if n & 0xFF != 0 {
            m.w8(CHOSEN, n.wrapping_sub(1) as u8);
        }
        let chosen = m.r8(CHOSEN) as u32;
        let id = m.r8(TRACKS.wrapping_add(chosen)) as u32;
        get_track(c, id, info);
        update_video_wall(c, this, info);
        let screen = Screen(this);
        fescreen::is_shape_file_loaded(c, screen, screen.swap_shapes());
        if screen.swap_shapes().file(&mut c.m) != 0 && (c.m.r8(this + TURNING) as i8) >= 0 {
            fescreen::upload_swap_shapes(c, screen, PICTURES);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w8(this + TURNING, 1);
            c.m.w32(this + TURNED_AT, ticks);
        }
        draw_video_wall(c, this);
        // the video: its next frame over the TVs, or played again a while after it ended
        let video = c.m.r32(this + VIDEO);
        if call_addr(c, VIDEO_STATE, &[video]) != 0 {
            let mut page_y = if c.m.r32(this + FRAMES) & 1 == 0 { 128 } else { 0 };
            let video = c.m.r32(this + VIDEO);
            if call_addr(c, VIDEO_UPDATE, &[video, 512, page_y]) != 0 {
                let frames = c.m.r32(this + FRAMES).wrapping_add(1);
                page_y = if frames & 1 == 0 { 128 } else { 0 };
                c.m.w32(this + FRAMES, frames);
            }
            set_video_tvs(c, this, page_y);
        } else if c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + TURNED_AT)) >= 257 {
            let id = track_id(&mut c.m, info);
            start_video(c, this, id);
        }
    });
    // the lines and the TVs
    for x in (240..480).step_by(80) {
        call_addr(c, DRAW_TRANS_SQUARE, &[0x0030_3030, x, 15, 2, 129, 1]);
    }
    call_addr(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 160, 79, 321, 1, 1]);
    call(c, SET_ABR_MODE, &[2]);
    for k in 0..8 {
        call_addr(c, DRAW_TV, &[track_tv(this, k).0]);
    }
}

/// The tracks in their boxes, 14 apart from y 143: each one's name (the menu's selected one
/// lit, and haloed while the halo shows), the box. The chosen track is each in turn meanwhile.
fn draw_tracks(c: &mut Cpu, this: u32, info: TrackInformation) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 347);
        rect.set_y(m, 143);
        rect.set_w(m, 144);
        rect.set_h(m, 14);
        let mut k = 0u32;
        while (k as i32) < c.m.r8(COUNT) as i32 {
            let m = &mut c.m;
            let menu = m.r32(this + MENU);
            let n = lh(m, menu + 8);
            let selected = k as i32 == n - 1 || (k == 0 && n == 0);
            if selected {
                halo(c, this, rect);
            }
            let id = c.m.r8(TRACKS.wrapping_add(k)) as u32;
            get_track(c, id, info);
            c.m.w8(CHOSEN, k as u8);
            let it = menus(&mut c.m) + ITERATOR;
            let text = call_addr(c, TRACK_TEXT_VALUE, &[it, u32::MAX]);
            let m = &mut c.m;
            let (x, y) = (sx((rect.x(m) as u16 as u32).wrapping_add(10)), sx((rect.y(m) as u16 as u32).wrapping_add(3)));
            fetextrender::menu_text_positioned_justify(c, sx(text), x, y, 0, if selected { 2 } else { 1 }, 1);
            call_addr(c, ROUND_RECTANGLE, &[u32::MAX, at, 1]);
            k += 1;
            rect.update_y(&mut c.m, |y| y.wrapping_add(14));
        }
    })
}

/// The selected track's halo (breathing with its clock, gone as the screen fades), left of its
/// box at `rect`.
fn halo(c: &mut Cpu, this: u32, rect: RECT) {
    let m = &mut c.m;
    let clock = m.r32(HALO).wrapping_add(1) as i32;
    m.w32(HALO, clock as u32);
    if clock >= 61 {
        m.w32(HALO, 0);
    }
    let h = m.r32(HALO) as i32;
    let size = ((if h < 31 { h } else { 60i32.wrapping_sub(h) }) / 2 + 20).wrapping_mul(128 - fade(m, this));
    if size <= 0 {
        return;
    }
    let (x, y) = (rect.x(m) as i32 as u32, rect.y(m) as i32 as u32);
    call_addr(c, HALO_2D, &[x.wrapping_sub(15), y.wrapping_add(6), (size / 2) as u32, (size.wrapping_mul(2) / 3) as u32, 23]);
    let f = fade(&mut c.m, this) as u32;
    call(c, DRAW_SHAPE_EXTENDED, &[HALO_SHAPE, 0, x.wrapping_sub(18), y, f, 1, 0]);
}

/// The video's TVs: two rows of four, each a quarter of a half of its frame (the frame in VRAM
/// rows from `page_y`).
fn set_video_tvs(c: &mut Cpu, this: u32, page_y: u32) {
    let v = page_y as u8 as u32;
    for row in 0..2u32 {
        for col in 0..4u32 {
            let tv = track_tv(this, 4 * row + col);
            let m = &mut c.m;
            tv.set_x(m, (80 * col + 160) as i16);
            tv.set_y(m, (64 * row + 25) as i16);
            tv.set_w(m, 80);
            tv.set_h(m, 64);
            tv.set_u(m, (40 * col) as u8);
            tv.set_v(m, ((row << 6) + v) as u8);
            tv.set_uw(m, 40);
            tv.set_vh(m, if row == 1 { 63 } else { 64 });
            let page = call(c, GET_TPAGE, &[2, 0, 512, sx(page_y)]);
            let m = &mut c.m;
            tv.set_tpage(m, page as u16);
            tv.set_clut(m, 0);
            tv.set_flags(m, VIDEO_TV_FLAGS);
            tv.set_tint(m, 0x0080_8080);
            tv.set_state(m, TV_ON);
        }
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenPinkSlips::GetShapeInfo` (SCREENPINKSLIPS.CPP, 0x80039378): 57 shapes of "zPink",
/// 4 of the first track's pictures (`TR%02dPS`), that track's the ones swapped in.
pub fn get_shape_info(c: &mut Cpu, this: Screen, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    with_track_buffer(c, |c, info| {
        let id = c.m.r8(TRACKS) as u32;
        get_track(c, id, info);
        let m = &mut c.m;
        m.w16(permanent, SHAPES);
        m.w16(swap, PICTURES as u16);
        m.w32(name, PINK_NAME);
        let id = track_id(m, info);
        call(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, id]);
        let m = &mut c.m;
        m.w32(swap_name, SWAP_NAME);
        let id = track_id(m, info);
        m.w16(this.0 + PREVIOUS, id as u16);
    })
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, Screen(c.r[4]), c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenPinkSlips::Initialize` (SCREENPINKSLIPS.CPP, 0x80039438): the video's place
/// cleared; the first track chosen (the iterator walked back and on); the screen's files read;
/// the first track's video made and played; the picture TVs not set, turning on from now.
pub fn initialize(c: &mut Cpu, this: Screen) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 512);
        rect.set_y(m, 0);
        rect.set_w(m, 170);
        rect.set_h(m, 256);
        call_addr(c, CLEAR_IMAGE, &[at, 0, 0, 0]);
        call_addr(c, DRAW_SYNC, &[0]);
    });
    c.m.w8(CHOSEN, 0);
    let it = menus(&mut c.m) + ITERATOR;
    call_addr(c, TRACK_DECREMENT, &[it, u32::MAX]);
    let it = menus(&mut c.m) + ITERATOR;
    call_addr(c, TRACK_INCREMENT, &[it, u32::MAX]);
    let m = &mut c.m;
    let menu = FEApplication(m.r32(FE_APP)).current_menu(m, 0);
    m.w32(this.0 + MENU, menu.0);
    fescreen::initialize(c, this);
    let this = this.0;
    c.m.w32(this + TVS_SET, 0);
    let id = with_track_buffer(c, |c, info| {
        let id = c.m.r8(TRACKS) as u32;
        get_track(c, id, info);
        track_id(&mut c.m, info)
    });
    with_buffer(c, 128, |c, file| {
        let path = c.m.r32(VIDEO_PATH);
        call(c, SPRINTF, &[file, VIDEO_FORMAT, path, id]);
        let video = call_addr(c, VIDEO_CREATE, &[160, 128, 0xF_0000, 0x2_0000, 16]);
        c.m.w32(this + VIDEO, video);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    });
    let m = &mut c.m;
    let ticks = m.r32(GAME_TICKS);
    m.w32(this + FRAMES, 0);
    m.w16(this + BRIGHT, 0);
    m.w16(this + DEST_BRIGHT, 0);
    m.w32(this + TVS_SET, 0);
    m.w8(this + TURNING, 1);
    m.w32(this + TURNED_AT, ticks);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenPinkSlips::Cleanup` (SCREENPINKSLIPS.CPP, 0x80039580): the video destroyed and
/// freed, the first track chosen; the music's buffer filled to its threshold; the screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    let video = c.m.r32(this.0 + VIDEO);
    call_addr(c, VIDEO_DESTROY, &[video]);
    let video = c.m.r32(this.0 + VIDEO);
    call(c, PURGE, &[video]);
    c.m.w8(CHOSEN, 0);
    loop {
        let buffered = call(c, BUFFERED, &[]) as i32;
        if buffered >= call(c, THRESHOLD, &[]) as i32 {
            break;
        }
        call(c, SYSTEM_TASK, &[0]);
    }
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreenPinkSlips::UpdateVideoWall` (SCREENPINKSLIPS.CPP, 0x800395fc): a track other than
/// the one swapped in: its pictures read, the picture TVs to be set again and turned off (unless
/// they are), the video stopped.
pub fn update_video_wall(c: &mut Cpu, this: u32, info: TrackInformation) {
    let id = track_id(&mut c.m, info);
    if id as i32 == lh(&mut c.m, this + PREVIOUS) {
        return;
    }
    call(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, id]);
    fescreen::async_load_swap_shape_file(c, Screen(this), SWAP_NAME);
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
    call_addr(c, VIDEO_ABORT, &[video]);
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], TrackInformation(c.r[5]));
}

/// `tScreenPinkSlips::DrawVideoWall` (SCREENPINKSLIPS.CPP, 0x800396a8): 36 shapes drawn with
/// nothing (the original's); once the pictures are uploaded, the picture TVs set from them; the
/// TVs turned on in order, one more every 4 ticks (or off the same way, done after 8 ticks);
/// the picture TVs drawn.
pub fn draw_video_wall(c: &mut Cpu, this: u32) {
    for k in 0..36 {
        call(c, DRAW_SHAPE_EXTENDED, &[k, 0, 0, 0, 0, 0, 0]);
    }
    let screen = Screen(this);
    if screen.swap_shapes().flags(&mut c.m) & UPLOADED != 0 && c.m.r32(this + TVS_SET) == 0 {
        for k in 0..PICTURES {
            let table = screen.swap_shapes().shapes(&mut c.m);
            call_addr(c, INIT_TV, &[image_tv(this, k).0, table.0, k]);
        }
        c.m.w32(this + TVS_SET, 1);
    }
    // one more every 4 ticks (the count a short): the ones off turned on, or the ones on off
    let m = &mut c.m;
    let count = sx(m.r32(GAME_TICKS).wrapping_sub(m.r32(this + TURNED_AT)) >> 2) as i32;
    let turning_on = (m.r8(this + TURNING) as i8) > 0;
    let (from, turn) = if turning_on { (0, TURN_ON_TV) } else { (TV_ON, TURN_OFF_TV) };
    for k in 0..count.clamp(0, 4) as u32 {
        let tv = image_tv(this, c.m.r8(TV_ORDER + k) as u32);
        if tv.state(&mut c.m) == from {
            call_addr(c, turn, &[tv.0]);
        }
    }
    if !turning_on && count >= 8 {
        c.m.w8(this + TURNING, 0);
    }
    for k in 0..4 {
        call_addr(c, DRAW_TV, &[image_tv(this, k).0]);
    }
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, c.r[4]);
}

/// `tScreenPinkSlips::ProcessInput` (SCREENPINKSLIPS.CPP, 0x80039924): on options, the race's
/// item disabled on tracks of difficulty 3 or more, egg tracks, with two players and the second
/// on its own setting, and in a one-player race of kind 1 (enabled otherwise); the other item
/// disabled.
pub fn process_input(c: &mut Cpu, key: u32) {
    if c.m.r32(key) != KEY_OPTIONS {
        return;
    }
    let chosen = c.m.r8(CHOSEN) as u32;
    let id = c.m.r8(TRACKS.wrapping_add(chosen)) as u32;
    with_track_buffer(c, |c, info| {
        get_track(c, id, info);
        let m = &mut c.m;
        let disable = |m: &mut Mem, at: u32| {
            let flags = menus(m) + at;
            let v = m.r32(flags) | ITEM_DISABLED;
            m.w32(flags, v);
        };
        let flags = menus(m) + RACE_ITEM_FLAGS;
        let v = m.r32(flags) & !ITEM_DISABLED;
        m.w32(flags, v);
        if m.r8(PLAYERS) != 1 && m.r8(SECOND) == 2 {
            disable(m, RACE_ITEM_FLAGS);
        }
        if info.track_difficulty(m) >= 3 {
            disable(m, RACE_ITEM_FLAGS);
        }
        if info.is_egg(m) != 0 {
            disable(m, RACE_ITEM_FLAGS);
        }
        if m.r8(PLAYERS) == 1 && m.r8(KIND) == 1 {
            disable(m, RACE_ITEM_FLAGS);
        }
        disable(m, OTHER_ITEM_FLAGS);
    })
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, c.r[6]);
}

/// `tScreenPinkSlips::~tScreenPinkSlips` (SCREENPINKSLIPS.H, 0x80039a60): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, pick, player, via_abi};
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram};

    /// The screen's swapped shapes (their table, the file read, uploaded), its fade; a TV's size.
    const SWAP_TABLE: u32 = 40;
    const SWAP_DATA: u32 = 44;
    const SWAP_UPLOADED: u32 = 58;
    const FADE: u32 = 92;
    const TV_SIZE: u32 = 48;

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

}
