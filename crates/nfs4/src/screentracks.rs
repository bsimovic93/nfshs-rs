//! SCREENTRACKS.CPP: the track select screen (`tScreenTrackSelect`). It shows the chosen track's
//! name in a box and its video playing, in two halves, faded in and out with the screen's
//! brightness (the track's own picture is swapped in while none plays). It also shows the
//! settings' video wall and the track's country, and asks for options only where the track
//! allows them.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The video,
//! `ClearImage`, `DrawSync` and `PSXDrawTransSquare` are the host's (called by address).

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{KEY_BACK, KEY_OPTIONS};
use crate::femenuextended::{NEXT_PRIM, OT, SET_ABR_MODE, SPRINTF};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TRACK_MANAGER};
use crate::types::{FEApplication, RECT, Screen, TextureShapeInfo, TrackInformation};
use crate::{fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +100 the video, +104 its frames shown, +108 the track whose
/// picture is swapped in, +110 the track whose video plays, +112 the brightness (0..128), +114
/// the brightness it goes to, +116 the brightness it went from, +120 since when, +124 the video
/// waiting to start (none played yet), +128 the video wall's TVs' config, +608 the video wall
/// (`tVideoWall`, +20 its state), +664 its settings shown, +668 when the video was last asked
/// to start.
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
/// A shape file's flags: uploaded.
const UPLOADED: u16 = 1;

/// A track's description (`tTrackInformation`, 48 bytes): +8 its picture's file's name, +42 its
/// country (a text from 67).
const TRACK_SIZE: u32 = 48;
const TRACK_FILE: u32 = 8;
const COUNTRY: u32 = 42;
const COUNTRIES: u32 = 67;
/// The track list (`frontEnd + 0x23`, the chosen one at `+0x46`); the players (`+3`), the
/// race's kind (`+4`), the second player's setting (`+0x31e`).
const TRACKS: u32 = FRONT_END + 0x23;
const CHOSEN: u32 = FRONT_END + 0x46;
const PLAYERS: u32 = FRONT_END + 3;
const KIND: u32 = FRONT_END + 4;
const SECOND: u32 = FRONT_END + 0x31E;
/// In the menus' block: the track iterator, two items' flags (bit 0: disabled).
const ITERATOR: u32 = 3208;
const OPTIONS_FLAGS: u32 = 3916;
const RACE_FLAGS: u32 = 3952;
const ITEM_DISABLED: u32 = 1;
/// The videos' directory (`Paths_Paths + 0xa4`); the swapped file's name; the video wall's TV
/// layout.
const VIDEO_PATH: u32 = PATHS + 0xA4;
const SWAP_NAME: u32 = 0x8005_3130;
const WALL_LAYOUT: u32 = 0x8005_2530;
/// "%szzzTR%02d.dct", "ztrack" (68 shapes; a track's picture 10), the swapped file's name's
/// format.
const VIDEO_FORMAT: u32 = 0x8001_1E9C;
const TRACK_NAME: u32 = 0x8001_1EAC;
const SHAPES: u16 = 68;
const PICTURE_SHAPES: u32 = 10;
const SWAP_FORMAT: u32 = 0x8001_1EB4;
/// A menu's vptr and its virtual `IsSubMenu`.
const MENU_VPTR: u32 = 104;
const MENU_IS_SUB_MENU: u32 = 64;
/// The video's states (stopped, playing).
const VIDEO_STOPPED: u32 = 1;
const VIDEO_PLAYING: u32 = 3;

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
/// `GetTPage`, `purgememadr`, `Front_EnableLocalSpeech`; `DrawVideoWall`, hooked (the host
/// records the spinning track's quads), so called by address.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const LOCAL_SPEECH: fn(&mut Cpu) = crate::front::enable_local_speech_entry;
const DRAW_VIDEO_WALL: u32 = 0x8004_22DC;
/// Ports called by address: FETRACKS' `GetTrack`, the track iterator's `TextValue`,
/// `Decrement` and `Increment`; `RoundRectangle` (DRAWSHP); the video wall's (FEVIDEOWALL)
/// `Initialize`, `SetAvailableText`, `SetAvailableIcon`, `SetAvailable`, `UpdateImages`,
/// `TurnOn`, `TurnOff`, `TurnOffInstant`, `Update`, `Draw`.
const GET_TRACK: u32 = 0x8002_1C04;
const TRACK_TEXT_VALUE: u32 = 0x8002_1F34;
const TRACK_DECREMENT: u32 = 0x8002_2020;
const TRACK_INCREMENT: u32 = 0x8002_1F74;
const ROUND_RECTANGLE: u32 = 0x8004_C490;
const WALL_INITIALIZE: u32 = 0x8003_6988;
const WALL_SET_AVAILABLE_TEXT: u32 = 0x8003_6B84;
const WALL_SET_AVAILABLE_ICON: u32 = 0x8003_6BA0;
const WALL_SET_AVAILABLE: u32 = 0x8003_6BCC;
const WALL_UPDATE_IMAGES: u32 = 0x8003_6A38;
const WALL_TURN_ON: u32 = 0x8003_70F0;
const WALL_TURN_OFF: u32 = 0x8003_7050;
const WALL_TURN_OFF_INSTANT: u32 = 0x8003_7074;
const WALL_UPDATE: u32 = 0x8003_6BDC;
const WALL_DRAW: u32 = 0x8003_6DAC;

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

/// The track at `k` in the list, its description in a buffer for `f`.
fn with_track<R>(c: &mut Cpu, k: u32, f: impl FnOnce(&mut Cpu, TrackInformation) -> R) -> R {
    with_buffer(c, TRACK_SIZE, |c, info| {
        let id = c.m.r8(TRACKS.wrapping_add(k)) as u32;
        call_addr(c, GET_TRACK, &[TRACK_MANAGER, id, info]);
        f(c, TrackInformation(info))
    })
}

/// The video's frame buffer in VRAM (from x 512, 170 by 256) cleared and drawn.
fn clear_video(c: &mut Cpu) {
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 512);
        rect.set_y(m, 0);
        rect.set_w(m, 170);
        rect.set_h(m, 256);
        call_addr(c, CLEAR_IMAGE, &[at, 0, 0, 0]);
        call_addr(c, DRAW_SYNC, &[0]);
    })
}

/// A video frame's half (a semi-transparent `POLY_FT4`) linked into the ordering table: from x
/// `x[0]` to `x[1]` (y 105..232), its texture from `u` (80 wide) of the page at `page_x`, rows
/// `top` to `bottom`; the screen's brightness its colour.
fn video_half(c: &mut Cpu, this: u32, x: [u32; 2], u: u32, page_x: u32, (top, bottom): (u32, u32)) {
    let m = &mut c.m;
    let prim = m.r32(NEXT_PRIM);
    let ot = m.r32(OT);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    m.w32(NEXT_PRIM, prim.wrapping_add(40));
    let entry = m.r32(ot);
    m.w32(ot, (entry & 0xFF00_0000) | (prim & 0x00FF_FFFF));
    let b = lh(m, this + BRIGHT);
    m.w8(prim + 3, 9);
    for (k, (px, py)) in [(x[0], 105), (x[1], 105), (x[0], 232), (x[1], 232)].into_iter().enumerate() {
        let at = prim + 8 + 8 * k as u32;
        m.w16(at, px as u16);
        m.w16(at + 2, py);
        m.w8(at + 4, if k % 2 == 0 { u } else { u + 80 } as u8);
        m.w8(at + 5, if k < 2 { top } else { bottom } as u8);
    }
    m.w32(prim + 4, (b << 16) | (b << 8) | b);
    m.w8(prim + 7, 46);
    let page = call(c, GET_TPAGE, &[2, 1, page_x, 0]);
    c.m.w16(prim + 22, page as u16);
    c.m.w16(prim + 14, 0);
}

/// `tScreenTrackSelect::DrawBackground` (SCREENTRACKS.CPP, 0x800419e0). It draws the chosen
/// track's name in its box, and brings the brightness and the swapped picture up to date (the
/// picture is uploaded and the wall turned on once read, while dark). Then the video: its
/// buffer is cleared (dark) when stopped; it shows its next frame while playing; or it is
/// started again (bright enough to want it, or never played, and a while after it was last
/// asked), the brightness set to its picture's. Then the frame's two halves and the wall.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 320);
        rect.set_y(m, 200);
        rect.set_w(m, 170);
        rect.set_h(m, 12);
        let it = menus(m) + ITERATOR;
        let text = call_addr(c, TRACK_TEXT_VALUE, &[it, u32::MAX]);
        call_addr(c, ROUND_RECTANGLE, &[sx(text), at, 0]);
    });
    with_track(c, 0, |c, info| {
        update_brightness(c, this, info);
        update_video_wall(c, this, info);
        let screen = Screen(this);
        fescreen::is_shape_file_loaded(c, screen, screen.swap_shapes());
        let wall = this + WALL;
        let m = &mut c.m;
        if screen.swap_shapes().file(m) != 0 && lh(m, wall + WALL_STATE) != u32::MAX && lh(m, this + BRIGHT) == 0 {
            fescreen::upload_swap_shapes(c, screen, PICTURE_SHAPES);
            call_addr(c, WALL_TURN_ON, &[wall]);
        }
        // the video: the rows of its frame in the buffer's halves (the even frames' at 128)
        let mut top = (((c.m.r32(this + FRAMES) & 1) == 0) as u32) << 7;
        let video = c.m.r32(this + VIDEO);
        match call_addr(c, VIDEO_STATE, &[video]) {
            VIDEO_STOPPED => {
                clear_video(c);
                let m = &mut c.m;
                let now = m.r32(GAME_TICKS);
                m.w16(this + BRIGHT, 0);
                m.w32(this + BRIGHT_AT, now.wrapping_sub(20));
            }
            VIDEO_PLAYING => {
                let video = c.m.r32(this + VIDEO);
                if call_addr(c, VIDEO_UPDATE, &[video, 512, top]) != 0 {
                    let m = &mut c.m;
                    let frames = m.r32(this + FRAMES).wrapping_add(1);
                    top = (((frames & 1) == 0) as u32) << 7;
                    m.w32(this + FRAMES, frames);
                }
            }
            _ => {
                let m = &mut c.m;
                let wants = (lh(m, this + DEST_BRIGHT) as i32) < lh(m, this + BRIGHT) as i32;
                if (m.r32(this + WAITING) != 0 || wants) && m.r32(GAME_TICKS).wrapping_sub(m.r32(this + ASKED_AT)) >= 257 {
                    if !wants {
                        let b = if info.available(&mut c.m) != 0 { 128 } else { 32 };
                        set_brightness(&mut c.m, this, b);
                    }
                    start_video(c, this);
                }
            }
        }
        // the frame's halves
        if lh(&mut c.m, this + BRIGHT) as i32 > 0 {
            let rows = (top, top | 0x7F);
            video_half(c, this, [153, 313], 0, 512, rows);
            video_half(c, this, [313, 473], 16, 576, rows);
        }
        call_addr(c, DRAW_VIDEO_WALL, &[this]);
    })
}

/// The video of the track whose video plays spooled from its file and started.
fn start_video(c: &mut Cpu, this: u32) {
    with_buffer(c, 128, |c, file| {
        let (path, id) = (c.m.r32(VIDEO_PATH), lh(&mut c.m, this + PLAYING));
        call(c, SPRINTF, &[file, VIDEO_FORMAT, path, id]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    })
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTrackSelect::GetShapeInfo` (SCREENTRACKS.CPP, 0x80041e8c): 68 shapes of "ztrack",
/// 10 of the chosen track's picture's file.
pub fn get_shape_info(c: &mut Cpu, this: Screen, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    with_track(c, 0, |c, info| {
        let m = &mut c.m;
        m.w16(permanent, SHAPES);
        m.w16(swap, PICTURE_SHAPES as u16);
        m.w32(name, TRACK_NAME);
        call(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, info.0 + TRACK_FILE]);
        let m = &mut c.m;
        m.w32(swap_name, SWAP_NAME);
        let id = info.track_id(m) as i32 as u32;
        m.w16(this.0 + PICTURE, id as u16);
    })
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, Screen(c.r[4]), c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTrackSelect::Initialize` (SCREENTRACKS.CPP, 0x80041f4c). The list starts from the
/// first track (back and forth to settle it), the video's buffer is cleared, and the screen's
/// files are read. The track's video is created and started; the wall is set up (its texts and
/// icon) and turned on, dark, the video waiting and asked a while ago.
pub fn initialize(c: &mut Cpu, this: Screen) {
    c.m.w8(CHOSEN, 0);
    let it = menus(&mut c.m) + ITERATOR;
    call_addr(c, TRACK_DECREMENT, &[it, u32::MAX]);
    let it = menus(&mut c.m) + ITERATOR;
    call_addr(c, TRACK_INCREMENT, &[it, u32::MAX]);
    let id = with_track(c, 0, |c, info| info.track_id(&mut c.m) as i32 as u32);
    clear_video(c);
    fescreen::initialize(c, this);
    let this = this.0;
    with_buffer(c, 128, |c, file| {
        let path = c.m.r32(VIDEO_PATH);
        call(c, SPRINTF, &[file, VIDEO_FORMAT, path, id]);
        let video = call_addr(c, VIDEO_CREATE, &[160, 128, 0xF_0000, 0x2_C000, 16]);
        c.m.w32(this + VIDEO, video);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    });
    c.m.w32(this + FRAMES, 0);
    let wall = this + WALL;
    let shapes: TextureShapeInfo = Screen(this).swap_shapes().shapes(&mut c.m);
    call_addr(c, WALL_INITIALIZE, &[wall, this + TV_CONFIG, shapes.0, 0, PICTURE_SHAPES, WALL_LAYOUT, 150]);
    call_addr(c, WALL_SET_AVAILABLE_TEXT, &[wall, 249, 320, 80]);
    let icons = Screen(this).perm_shapes().shapes(&mut c.m);
    call_addr(c, WALL_SET_AVAILABLE_ICON, &[wall, 38, 10, 310, 60, icons.0]);
    let m = &mut c.m;
    m.w16(this + BRIGHT, 0);
    m.w16(this + DEST_BRIGHT, 0);
    m.w32(this + WALL_SET, 0);
    call_addr(c, WALL_TURN_ON, &[wall]);
    let m = &mut c.m;
    let now = m.r32(GAME_TICKS);
    m.w32(this + WAITING, 1);
    m.w32(this + ASKED_AT, now.wrapping_sub(256));
    m.w16(this + PLAYING, id as u16);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTrackSelect::Cleanup` (SCREENTRACKS.CPP, 0x80042108): the video destroyed and freed,
/// then the screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    let video = c.m.r32(this.0 + VIDEO);
    call_addr(c, VIDEO_DESTROY, &[video]);
    let video = c.m.r32(this.0 + VIDEO);
    call(c, PURGE, &[video]);
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreenTrackSelect::SetBrightness` (SCREENTRACKS.CPP, 0x80042148): a new brightness to go
/// to, from the one now, starting now.
pub fn set_brightness(m: &mut Mem, this: u32, b: u32) {
    if sx(b) != lh(m, this + DEST_BRIGHT) {
        let now = m.r16(this + BRIGHT);
        let t = m.r32(GAME_TICKS);
        m.w16(this + DEST_BRIGHT, b as u16);
        m.w16(this + FROM_BRIGHT, now);
        m.w32(this + BRIGHT_AT, t);
    }
}

pub fn abi_set_brightness(c: &mut Cpu) {
    set_brightness(&mut c.m, c.r[4], c.r[5]);
}

/// `tScreenTrackSelect::UpdateBrightness` (SCREENTRACKS.CPP, 0x80042178): the brightness on its
/// way (over 128 ticks); dark (the brightness and the one wanted), the video stopped and, the
/// first time, waiting for this track's.
pub fn update_brightness(c: &mut Cpu, this: u32, info: TrackInformation) {
    let m = &mut c.m;
    let d = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + BRIGHT_AT)) as i32;
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
    // (the brightness and the one wanted together, as a word)
    if m.r32(this + BRIGHT) != 0 {
        return;
    }
    let video = m.r32(this + VIDEO);
    call_addr(c, VIDEO_ABORT, &[video]);
    let m = &mut c.m;
    if m.r32(this + WAITING) == 0 {
        let t = m.r32(GAME_TICKS);
        m.w32(this + WAITING, 1);
        m.w32(this + ASKED_AT, t);
        let id = info.track_id(m) as i32 as u32;
        m.w16(this + PLAYING, id as u16);
    }
}

pub fn abi_update_brightness(c: &mut Cpu) {
    update_brightness(c, c.r[4], TrackInformation(c.r[5]));
}

/// `tScreenTrackSelect::UpdateVideoWall` (SCREENTRACKS.CPP, 0x80042268): a new track's picture
/// loaded, the wall turned off and the settings to show again, going dark, no video waiting.
pub fn update_video_wall(c: &mut Cpu, this: u32, info: TrackInformation) {
    let m = &mut c.m;
    if info.track_id(m) as i32 as u32 == lh(m, this + PICTURE) {
        return;
    }
    fescreen::async_load_swap_shape_file(c, Screen(this), info.0 + TRACK_FILE);
    let m = &mut c.m;
    m.w32(this + WALL_SET, 0);
    let id = info.track_id(m) as i32 as u32;
    m.w16(this + PICTURE, id as u16);
    call_addr(c, WALL_TURN_OFF, &[this + WALL]);
    set_brightness(&mut c.m, this, 0);
    c.m.w32(this + WAITING, 0);
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], TrackInformation(c.r[5]));
}

/// `tScreenTrackSelect::DrawVideoWall` (SCREENTRACKS.CPP, 0x800422dc): the track's country, the
/// background picture, a dark band, the wall (its settings shown once the picture is up).
pub fn draw_video_wall(c: &mut Cpu, this: Screen) {
    let k = c.m.r8(CHOSEN) as u32;
    with_track(c, k, |c, info| {
        let text = (c.m.r8(info.0 + COUNTRY) as u32).wrapping_add(COUNTRIES);
        fetextrender::menu_text_positioned_justify(c, text, 478, 33, 1, 0, 11);
        let image = this.perm_shapes().shapes(&mut c.m);
        fescreen::draw_background_image(c, this, 0, 28, image, 150);
        call_addr(c, DRAW_TRANS_SQUARE, &[0, 320, 30, 160, 10, 1]);
        call(c, SET_ABR_MODE, &[0]);
        let wall = this.0 + WALL;
        if this.swap_shapes().flags(&mut c.m) & UPLOADED != 0 && c.m.r32(this.0 + WALL_SET) == 0 {
            let available = info.available(&mut c.m) as u32;
            call_addr(c, WALL_SET_AVAILABLE, &[wall, available]);
            call_addr(c, WALL_UPDATE_IMAGES, &[wall]);
            c.m.w32(this.0 + WALL_SET, 1);
        }
        call_addr(c, WALL_UPDATE, &[wall]);
        call_addr(c, WALL_DRAW, &[wall]);
    })
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, Screen(c.r[4]));
}

/// `tScreenTrackSelect::ProcessInput` (SCREENTRACKS.CPP, 0x800423ec). Options: the options
/// item is disabled with two players and the second on its own setting, a track of difficulty
/// 3 or more, an egg track, or one player in a race of kind 1; the race item is enabled only in
/// a race of kind 1 with local speech. Back: the wall off at once unless the menu is a sub-menu.
pub fn process_input(c: &mut Cpu, this: Screen, key: u32) {
    let key = c.m.r32(key);
    if key == KEY_OPTIONS {
        let k = c.m.r8(CHOSEN) as u32;
        with_track(c, k, |c, info| {
            let m = &mut c.m;
            let menus = menus(m);
            let flags = m.r32(menus + OPTIONS_FLAGS) & !ITEM_DISABLED;
            m.w32(menus + OPTIONS_FLAGS, flags);
            let disable = |m: &mut Mem| {
                let menus = m.r32(SONGS_SCREEN);
                let v = m.r32(menus + OPTIONS_FLAGS) | ITEM_DISABLED;
                m.w32(menus + OPTIONS_FLAGS, v);
            };
            if m.r8(PLAYERS) != 1 && m.r8(SECOND) == 2 {
                disable(m);
            }
            if info.track_difficulty(m) >= 3 {
                disable(m);
            }
            if info.is_egg(m) != 0 {
                disable(m);
            }
            if m.r8(PLAYERS) == 1 && m.r8(KIND) == 1 {
                disable(m);
            }
            let race_on = m.r8(KIND) == 1 && call(c, LOCAL_SPEECH, &[]) != 0;
            let m = &mut c.m;
            let menus = m.r32(SONGS_SCREEN);
            let v = m.r32(menus + RACE_FLAGS);
            m.w32(menus + RACE_FLAGS, if race_on { v & !ITEM_DISABLED } else { v | ITEM_DISABLED });
        });
    } else if key == KEY_BACK {
        let menu = FEApplication(c.m.r32(FE_APP)).current_menu(&mut c.m, 0);
        if call_virtual(c, menu.0, MENU_VPTR, MENU_IS_SUB_MENU, &[]) != 1 {
            call_addr(c, WALL_TURN_OFF_INSTANT, &[this.0 + WALL]);
        }
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, Screen(c.r[4]), c.r[6]);
}

/// `tScreenTrackSelect::~tScreenTrackSelect` (SCREENTRACKS.H, 0x800425b4): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::SHAPES as HELP_SHAPES;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram};

    /// The screen's swapped shapes (their table, the file read, uploaded).
    const SWAP: u32 = 40;
    const SWAP_DATA: u32 = 44;
    const SWAP_UPLOADED: u32 = 58;

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
