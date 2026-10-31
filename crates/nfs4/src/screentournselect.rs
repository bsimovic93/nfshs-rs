//! SCREENTOURNSELECT.CPP: the tournament select's screen (`tScreenTournSelect`): the money, the
//! entry fee and the three prizes; the chosen tournament's name and description; its trophy over
//! four TVs (swapped in when the tournament changes, the TVs turned off and on again one by one);
//! the tournaments' video across eight more; the lines and the turning trophy.
//!
//! The videos, `ClearImage`, `DrawSync`, `DrawTV`, `DrawTVLines` and the squares are the
//! host's (called by address).

use crate::abi::{self, Saved};
use crate::femenuextended::{SET_ABR_MODE, SPRINTF, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TOURNAMENT_MANAGER};
use crate::{fefades, felines, fescreen, fetextrender, fetv};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen` (+0 its permanent shapes' table, +40 its swapped ones (the trophy's):
// +40 their table, +44 the file read; +92 its fade), then +100 the video, +104 its frames shown,
// +108 the video's 8 TVs and +492 the trophy's 4 (FETV's, 48 bytes each), +684 and +686 the
// movies (cleared), +692 the tick the trophy's TVs last turned, +696 turning on (1), off (-1) or
// not, +698 the trophy swapped in, +700 cleared when a trophy is asked for, +704 the description's
// top (-1: to measure), +708 its text.
const SWAP_TABLE: u32 = 40;
const SWAP_DATA: u32 = 44;
const FADE: u32 = 92;
const VIDEO: u32 = 100;
const FRAMES: u32 = 104;
const VIDEO_TVS: u32 = 108;
const TROPHY_TVS: u32 = 492;
const PREVIOUS_MOVIE: u32 = 684;
const CURRENT_MOVIE: u32 = 686;
const TURNED_AT: u32 = 692;
const TURNING: u32 = 696;
const TROPHY: u32 = 698;
const TVS_SET: u32 = 700;
const TEXT_TOP: u32 = 704;
const TEXT_ID: u32 = 708;

// A TV (FETV's): +0 its state (0 off, 1 on), +4 and +6 its brightnesses, +8 its flags, +16 its
// colour, +20 its place and size, +28 its texture's corner and size, +34 its CLUT, +36 its page.
const TV_SIZE: u32 = 48;

/// The front end's tier and its kind's tournament for a tier or none (`frontEnd + 0x122`,
/// `+0x121`, `+0x120`); the main screen's number (`+0x340`).
const FE_TIER: u32 = FRONT_END + 0x122;
const TIER_TOURNAMENT: u32 = FRONT_END + 0x121;
const TOURNAMENT_ONLY: u32 = FRONT_END + 0x120;
const MAIN_NUMBER: u32 = FRONT_END + 0x340;
/// The tournament manager's money (+20) and definition (+24: each tier's first tournament at
/// +2 of its 12 bytes; tournaments from +36, 84 bytes each: +22 the trophy, +24 the prizes,
/// +48 the entry fee).
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const TROPHY_ID: u32 = 22;
const PRIZES: u32 = 24;
const ENTRY_FEE: u32 = 48;
/// In the menus' block: the tournaments' iterators (a tier's, the others').
const TIER_ITERATOR: u32 = 1932;
const ITERATOR: u32 = 1704;
/// The screen's vtable; the trophy's file's name; "ztourn", "%szzzTRN.dct"; the videos'
/// directory; the order the trophy's TVs turn in.
const TOURN_SELECT_VT: u32 = 0x8001_1D60;
const TROPHY_NAME: u32 = 0x8005_3120;
const TOURN_NAME: u32 = 0x8001_1D44;
const VIDEO_FORMAT: u32 = 0x8001_1D4C;
const VIDEO_PATH: u32 = PATHS + 0xA4;
const TV_ORDER: u32 = 0x8005_24E8;

/// The host's.
const VIDEO_CREATE: u32 = 0x8005_11B0;
const VIDEO_DESTROY: u32 = 0x8005_1328;
const VIDEO_SPOOL: u32 = 0x8005_1380;
const VIDEO_START: u32 = 0x8005_13DC;
const VIDEO_STATE: u32 = 0x8005_1480;
const VIDEO_UPDATE: u32 = 0x8005_1528;
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const DRAW_TV: u32 = 0x8002_24AC;
const DRAW_TV_LINES: u32 = 0x8002_2158;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `GetTrophyName`, `tListIteratorTournament::TextValue` (FETOURN.CPP), `DrawMoney`
/// (SCREENPOST.CPP), all still recompiled; `GetTPage`, `ScaleShapeExtended`, `purgememadr`,
/// `TextSys_WordX`, `TextSys_WordY`.
const GET_TROPHY_NAME: fn(&mut Cpu) = nfs4rc::f_80033ca0;
const TOURNAMENT_TEXT: fn(&mut Cpu) = nfs4rc::f_80033f64;
const DRAW_MONEY: fn(&mut Cpu) = nfs4rc::f_80039a80;
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = nfs4rc::f_8004ef70;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// The chosen tournament (`tTourneyInfo *`): the tier's, or the others'.
fn tournament(m: &mut Mem) -> u32 {
    let tier = m.r8(FE_TIER) as u32;
    let k = m.r8(if tier != 0 { TIER_TOURNAMENT } else { TOURNAMENT_ONLY }) as u32;
    let definition = m.r32(TOURNAMENT_MANAGER + DEFINITION);
    let first = m.r8(definition.wrapping_add(tier * 12) + 2) as u32;
    definition.wrapping_add(k.wrapping_add(first).wrapping_mul(84)) + 36
}

/// The tournaments' iterator of the front end's tier.
fn iterator(m: &mut Mem) -> u32 {
    let menus = m.r32(SONGS_SCREEN);
    menus + if m.r8(FE_TIER) != 0 { TIER_ITERATOR } else { ITERATOR }
}

/// `tScreenTournSelect::tScreenTournSelect` (SCREENTOURNSELECT.CPP, 0x8003fb48).
pub fn abi_screen_tourn_select(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8003_fb5c, &[this], fescreen::abi_screen);
        c.m.w32(this + 96, TOURN_SELECT_VT);
        c.r[2] = this;
    })
}

/// `tScreenTournSelect::~tScreenTournSelect` (SCREENTOURNSELECT.CPP, 0x8003fb7c).
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + 96, TOURN_SELECT_VT);
        port(c, &Saved::of(c), 0x8002_6098, 0x8003_fb94, &[this, flags], fescreen::abi_screen_destructor);
    })
}

/// `tScreenTournSelect::GetShapeInfo` (SCREENTOURNSELECT.CPP, 0x8003fba4): 64 shapes of
/// "ztourn", 32 of the chosen tournament's trophy.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = TROPHY_NAME;
        let m = &mut c.m;
        let t = tournament(m);
        m.w16(permanent, 64);
        m.w16(swap, 32);
        m.w32(name, TOURN_NAME);
        r.call_leaving(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 1, TROPHY_NAME, u32::MAX], 0x8003_fc54);
        c.m.w32(swap_name, TROPHY_NAME);
    })
}

/// `tScreenTournSelect::Initialize` (SCREENTOURNSELECT.CPP, 0x8003fc70): the description to
/// measure; the video's place cleared; the screen's files; the tournaments' video made and
/// played; the trophy's four TVs set (two by two); the chosen tournament's trophy swapped in,
/// its TVs turning on from now.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 144, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        c.m.w32(this + TEXT_TOP, u32::MAX);
        c.m.w32(this + TEXT_ID, u32::MAX);
        let rect = sp + 24;
        for (k, v) in [512u16, 0, 80, 160].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        r.call_ptr_leaving(c, CLEAR_IMAGE, &[rect, 0, 0, 0], 0x8003_fcd4);
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8003_fcdc);
        port(c, &r, 0x8002_60cc, 0x8003_fce4, &[this], fescreen::abi_initialize);
        let file = sp + 32;
        r.0[0] = file;
        c.m.w16(this + CURRENT_MOVIE, 0);
        c.m.w16(this + PREVIOUS_MOVIE, 0);
        let path = c.m.r32(VIDEO_PATH);
        r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path], 0x8003_fd08);
        let video = r.call_ptr_leaving(c, VIDEO_CREATE, &[80, 80, 0xF_0000, 0x2_5800, 16], 0x8003_fd28);
        c.m.w32(this + VIDEO, video);
        r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8003_fd38);
        r.0[5] = 0;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8003_fd44);
        c.m.w32(this + FRAMES, 0);
        r.0[2] = 0;
        // the trophy's TVs
        for row in 0..2u32 {
            r.0[5] = row;
            r.0[6] = row << 1;
            r.0[4] = 76 * row + 165;
            for col in 0..2u32 {
                r.0[1] = col;
                let k = 2 * row + col;
                r.0[0] = TV_SIZE * k;
                let t = this + TROPHY_TVS + TV_SIZE * k;
                let table = c.m.r32(this);
                port(c, &r, 0x8002_3168, 0x8003_fd9c, &[t, table, 0], fetv::abi_init_tv);
                r.0[2] = col + 1;
                r.0[0] = this + TV_SIZE * k;
                let m = &mut c.m;
                m.w16(t + 22, (37 * col + 142) as u16);
                m.w16(t + 24, 76);
                let flags = m.r16(t + 8);
                m.w16(t + 20, r.0[4] as u16);
                m.w16(t + 26, 37);
                m.w16(t + 8, flags | 0x30);
            }
            r.0[2] = 0;
        }
        let m = &mut c.m;
        let t = tournament(m);
        let trophy = m.r8(t + TROPHY_ID);
        m.w8(this + TROPHY, trophy);
        let ticks = m.r32(GAME_TICKS);
        m.w16(this + TURNING, 1);
        m.w32(this + TURNED_AT, ticks);
    })
}

/// `tScreenTournSelect::Cleanup` (SCREENTOURNSELECT.CPP, 0x8003feb8): the video destroyed and
/// freed; the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let video = c.m.r32(this + VIDEO);
        r.call_ptr_leaving(c, VIDEO_DESTROY, &[video], 0x8003_fed4);
        let video = c.m.r32(this + VIDEO);
        r.call_leaving(c, PURGE, &[video], 0x8003_fee0);
        port(c, &r, 0x8002_61c0, 0x8003_fee8, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenTournSelect::UpdateVideoWall` (SCREENTOURNSELECT.CPP, 0x8003fef8): a tournament whose
/// trophy isn't the one swapped in: its trophy read, its TVs turning off from now (unless they
/// are).
pub fn update_video_wall(c: &mut Cpu, this: u32, t: u32) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[2] = t;
        if c.m.r8(t + TROPHY_ID) as u32 == c.m.r8(this + TROPHY) as i8 as i32 as u32 {
            return;
        }
        r.0[3] = u32::MAX;
        r.0[0] = TROPHY_NAME;
        r.call_leaving(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 1, TROPHY_NAME, u32::MAX], 0x8003_ff4c);
        port(c, &r, 0x8002_5dd8, 0x8003_ff58, &[this, TROPHY_NAME], fescreen::abi_async_load_swap_shape_file);
        let m = &mut c.m;
        m.w32(this + TVS_SET, 0);
        let trophy = m.r8(t + TROPHY_ID);
        let turning = lh(m, this + TURNING);
        m.w8(this + TROPHY, trophy);
        if turning >= 0 {
            let ticks = m.r32(GAME_TICKS);
            m.w16(this + TURNING, 0xFFFF);
            m.w32(this + TURNED_AT, ticks);
        }
    })
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenTournSelect::DrawVideoWall` (SCREENTOURNSELECT.CPP, 0x8003ff9c): the background; the
/// lines between the video's TVs; the trophy's TVs turned on in order, one more every 4 ticks (or
/// off, done after 16 ticks); their frame; the TVs; the trophy turning.
pub fn draw_video_wall(c: &mut Cpu, this: u32) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[1] = 244;
        r.0[3] = 97;
        r.0[0] = 1;
        let m = &mut c.m;
        let swap = m.r32(this + SWAP_TABLE);
        m.w32(sp + 52, swap);
        m.w32(sp + 16, 0);
        let table = m.r32(this);
        port(c, &r, 0x8002_5ce8, 0x8003_ffe4, &[this, 0, 34, table], fescreen::abi_draw_background_image);
        for x in (244..485).step_by(80) {
            r.0[1] = x;
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0020_2020, x, 41, 2, 97, 1], 0x8004_0004);
        }
        r.0[1] = 89;
        r.0[0] = 1;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0014_1414, 165, 89, 321, 1, 1], 0x8004_003c);
        r.0[1] = 137;
        r.call_leaving(c, SET_ABR_MODE, &[2], 0x8004_0054);
        // the trophy's TVs turning
        let m = &mut c.m;
        let n = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + TURNED_AT)) >> 2;
        r.0[1] = n;
        let tv = |m: &mut Mem, k: u32| this + TROPHY_TVS + TV_SIZE * m.r8(TV_ORDER + k) as u32;
        r.0[0] = 0;
        if lh(m, this + TURNING) > 0 {
            if n != 0 {
                r.0[3] = TV_ORDER;
                while (r.0[0] as i32) < 4 {
                    let t = tv(&mut c.m, r.0[0]);
                    if c.m.r32(t) == 0 {
                        port(c, &r, 0x8002_3388, 0x8004_00c8, &[t], fetv::abi_turn_on_tv);
                    }
                    r.0[0] += 1;
                    if (r.0[0] as i32) >= n as i32 {
                        break;
                    }
                }
            }
        } else {
            if (n as i32) > 0 {
                r.0[4] = TV_ORDER;
                r.0[3] = 1;
                while (r.0[0] as i32) < 4 {
                    let t = tv(&mut c.m, r.0[0]);
                    if c.m.r32(t) == 1 {
                        port(c, &r, 0x8002_3378, 0x8004_0134, &[t], fetv::abi_turn_off_tv);
                    }
                    r.0[0] += 1;
                    if (r.0[0] as i32) >= n as i32 {
                        break;
                    }
                }
            }
            if (n as i32) >= 4 {
                c.m.w16(this + TURNING, 0);
            }
        }
        // the frame, the TVs, the trophy
        r.0[0] = 1;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0030_3030, 241, 142, 2, 74, 1], 0x8004_017c);
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 165, 179, 152, 1, 1], 0x8004_019c);
        r.call_leaving(c, SET_ABR_MODE, &[2], 0x8004_01a4);
        for k in 0..4 {
            r.0[1] = k;
            r.0[0] = TROPHY_TVS + TV_SIZE * k;
            r.call_ptr_leaving(c, DRAW_TV, &[this + TROPHY_TVS + TV_SIZE * k], 0x8004_01b4);
        }
        r.0[1] = 4;
        r.0[0] = TROPHY_TVS + TV_SIZE * 4;
        let frame = ((c.m.r32(GAME_TICKS) as i32 >> 4) % 32) as u32;
        r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[frame, 1536, 182, 147, 0, 0, sp + 32], 0x8004_020c);
    })
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, c.r[4]);
}

/// `tScreenTournSelect::DrawBackground` (SCREENTOURNSELECT.CPP, 0x8004022c): the money, the
/// entry fee and the three prizes (their headings and amounts); the trophy swapped in and its
/// TVs; the video's next frame over its eight TVs (or played again); the tournament's name and
/// description (its top measured when it changes), the headings; the TVs.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = this;
        r.0[2] = 0;
        r.0[0] = TOURNAMENT_MANAGER;
        let t = tournament(&mut c.m);
        r.0[7] = t;
        let fade = |c: &mut Cpu, r: &Saved, color: u32, ra: u32| {
            let f = lh(&mut c.m, this + FADE) as u32;
            port(c, r, 0x8004_bd68, ra, &[color, f], fefades::abi_fade_out)
        };
        let gold = fade(c, &r, 0xBEBE, 0x8004_02d4);
        r.0[6] = gold;
        let shade = fade(c, &r, 0x0023_2323, 0x8004_02e8);
        r.0[5] = shade;
        let grey = fade(c, &r, 0x0050_5050, 0x8004_02fc);
        r.0[8] = grey;
        r.0[1] = c.m.r32(TOURNAMENT_MANAGER + MONEY);
        let money = |c: &mut Cpu, r: &Saved, x: u32, y: u32, amount: u32, ra: u32| {
            c.m.w32(sp + 16, r.0[6]);
            c.m.w32(sp + 20, r.0[5]);
            r.call_leaving(c, DRAW_MONEY, &[x, y, 6, amount], ra);
        };
        // the money, the entry fee
        let f = lh(&mut c.m, this + FADE) as u32;
        port(c, &r, 0x8002_1214, 0x8004_0318, &[f, 124, 1, 4], fetextrender::abi_menu_text_fade);
        let x = r.call_leaving(c, WORD_X, &[124], 0x8004_0320);
        r.0[0] = x.wrapping_add(140);
        let y = r.call_leaving(c, WORD_Y, &[124], 0x8004_032c);
        money(c, &r, r.0[0], y.wrapping_add(9), r.0[1], 0x8004_0348);
        let f = lh(&mut c.m, this + FADE) as u32;
        port(c, &r, 0x8002_1214, 0x8004_035c, &[f, 154, 1, 14], fetextrender::abi_menu_text_fade);
        let x = r.call_leaving(c, WORD_X, &[154], 0x8004_0364);
        r.0[0] = x.wrapping_add(140);
        let y = r.call_leaving(c, WORD_Y, &[154], 0x8004_0370);
        let fee = c.m.r32(t + ENTRY_FEE);
        money(c, &r, r.0[0], y.wrapping_add(9), fee, 0x8004_038c);
        // the prizes
        let f = lh(&mut c.m, this + FADE) as u32;
        port(c, &r, 0x8002_1214, 0x8004_03a0, &[f, 155, 1, 14], fetextrender::abi_menu_text_fade);
        let y = r.call_leaving(c, WORD_Y, &[155], 0x8004_03a8);
        r.0[3] = y.wrapping_add(9);
        for k in 0..3u32 {
            r.0[1] = k;
            let w = r.call_leaving(c, TEXT_WORD, &[k + 725], 0x8004_03bc);
            r.0[0] = w;
            let x = r.call_leaving(c, WORD_X, &[155], 0x8004_03c8);
            let y = sx(r.0[3]);
            r.0[0] = y;
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, sx(x), y, r.0[8], 0, 0], 0x8004_03f0);
            let x = r.call_leaving(c, WORD_X, &[154], 0x8004_03f8);
            r.0[1] = t.wrapping_add(k << 2);
            let prize = c.m.r32(r.0[1] + PRIZES);
            r.0[3] = r.0[3].wrapping_add(9);
            money(c, &r, x.wrapping_add(140), y, prize, 0x8004_0420);
            r.0[2] = k + 1;
        }
        // the trophy
        port(c, &r, 0x8003_fef8, 0x8004_0448, &[this, t], abi_update_video_wall);
        port(c, &r, 0x8002_5e2c, 0x8004_0454, &[this, this + SWAP_TABLE], fescreen::abi_is_shape_file_loaded);
        if c.m.r32(this + SWAP_DATA) != 0 && lh(&mut c.m, this + TURNING) >= 0 {
            port(c, &r, 0x8002_5ff0, 0x8004_047c, &[this, 32], fescreen::abi_upload_swap_shapes);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w16(this + TURNING, 1);
            c.m.w32(this + TURNED_AT, ticks);
        }
        port(c, &r, 0x8003_ff9c, 0x8004_0498, &[this], abi_draw_video_wall);
        // the video's TVs
        let y = if c.m.r32(this + FRAMES) & 1 != 0 { 0 } else { 80 };
        c.m.w32(sp + 184, 0x002C_1E1E);
        r.0[2] = 0;
        for row in 0..2u32 {
            r.0[2] = row;
            r.0[1] = 0;
            r.0[8] = 48 * row + 41;
            r.0[3] = row << 2;
            r.0[7] = 40 * row;
            for col in 0..4u32 {
                r.0[6] = 512 << 16;
                r.0[5] = y << 16;
                let k = col + 4 * row;
                let t = this + VIDEO_TVS + TV_SIZE * k;
                r.0[0] = this + TV_SIZE * k;
                let m = &mut c.m;
                m.w16(t + 20, (80 * col + 165) as u16);
                m.w16(t + 24, 80);
                m.w16(t + 26, 48);
                m.w8(t + 30, 20);
                m.w16(t + 22, r.0[8] as u16);
                m.w8(t + 28, (20 * col) as u8);
                m.w8(t + 29, r.0[7] as u8);
                m.w8(t + 31, 40);
                m.w32(sp + 192, y);
                m.w32(sp + 196, 128);
                let page = r.call_leaving(c, GET_TPAGE, &[2, 0, 512, y], 0x8004_057c);
                r.0[1] = col + 1;
                let m = &mut c.m;
                m.w16(t + 36, page as u16);
                m.w32(t, 1);
                m.w16(t + 34, 0);
                m.w16(t + 8, 34);
                m.w32(t + 16, 0x002C_1E1E);
                m.w16(t + 6, 128);
                m.w16(t + 4, 128);
            }
            r.0[2] = row + 1;
        }
        let video = c.m.r32(this + VIDEO);
        if r.call_ptr_leaving(c, VIDEO_STATE, &[video], 0x8004_05f0) != 0 {
            let video = c.m.r32(this + VIDEO);
            let next = r.call_ptr_leaving(c, VIDEO_UPDATE, &[video, 512, y], 0x8004_0608);
            r.0[2] = 0;
            if next != 0 {
                let n = c.m.r32(this + FRAMES).wrapping_add(1);
                c.m.w32(this + FRAMES, n);
            }
        } else {
            let file = sp + 104;
            r.0[0] = file;
            c.m.w16(this + CURRENT_MOVIE, 0);
            let path = c.m.r32(VIDEO_PATH);
            r.call_leaving(c, SPRINTF, &[file, VIDEO_FORMAT, path], 0x8004_0640);
            let video = c.m.r32(this + VIDEO);
            r.call_ptr_leaving(c, VIDEO_SPOOL, &[video, file], 0x8004_064c);
            r.0[2] = 0;
            let video = c.m.r32(this + VIDEO);
            r.call_ptr_leaving(c, VIDEO_START, &[video], 0x8004_0658);
        }
        for k in 0..8 {
            r.0[2] = k;
            r.call_ptr_leaving(c, DRAW_TV_LINES, &[this + VIDEO_TVS + TV_SIZE * k], 0x8004_0678);
        }
        r.0[2] = 8;
        // the tournament's name and description
        let rect = sp + 96;
        for (k, v) in [325u16, 43, 314, 10].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        let it = iterator(&mut c.m);
        let name = r.call_leaving(c, TOURNAMENT_TEXT, &[it, u32::MAX], 0x8004_06ec);
        let f = lh(&mut c.m, this + FADE) as u32;
        c.m.w32(sp + 16, 13);
        port(c, &r, 0x8002_1984, 0x8004_070c, &[f, sx(name), rect, 2], fetextrender::abi_word_wrap_fade);
        c.m.w16(rect, 170);
        let w = c.m.r16(rect + 4).wrapping_sub(10);
        c.m.w16(rect + 4, w);
        let it = iterator(&mut c.m);
        let v = r.call_leaving(c, TOURNAMENT_TEXT, &[it, u32::MAX], 0x8004_0758);
        r.0[2] = v.wrapping_add(38);
        let id = sx(r.0[2]);
        if id != c.m.r32(this + TEXT_ID) || c.m.r32(this + TEXT_TOP) == u32::MAX {
            c.m.w32(this + TEXT_ID, id);
            let w = r.call_leaving(c, TEXT_WORD, &[id], 0x8004_078c);
            let width = lh(&mut c.m, rect + 4) as u32;
            let h = r.call_leaving(c, fetextrender::word_wrap_height_entry, &[width, w], 0x8004_079c);
            c.m.w32(this + TEXT_TOP, 117u32.wrapping_sub(h));
        }
        r.0[1] = r.0[2].wrapping_sub(872);
        let top = c.m.r16(this + TEXT_TOP);
        r.0[2] = 0;
        c.m.w16(rect + 2, top);
        let w = r.call_leaving(c, TEXT_WORD, &[id], 0x8004_07c4);
        r.0[0] = w;
        let color = fade(c, &r, 0x0050_5050, 0x8004_07d8);
        r.call_leaving(c, fetextrender::word_wrap_text_rgb_entry, &[w, rect, color], 0x8004_07e8);
        let f = lh(&mut c.m, this + FADE) as u32;
        r.0[0] = 4;
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[f, 988, 170, 117, 0, 1, 4], 0x8004_0810);
        r.0[1] = sx(r.0[1].wrapping_add(891)) << 16;
        let f = lh(&mut c.m, this + FADE) as u32;
        let heading = ((r.0[1] as i32) >> 16) as u32;
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[f, heading, 170, 125, 0, 2, 4], 0x8004_083c);
        for k in 0..8 {
            r.0[2] = k;
            r.call_ptr_leaving(c, DRAW_TV, &[this + VIDEO_TVS + TV_SIZE * k], 0x8004_085c);
        }
    })
}

/// `tScreenTournSelect::DrawForeground` (SCREENTOURNSELECT.CPP, 0x800408a8): two lines along the
/// money's box (not on the fifth main screen).
pub fn abi_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        if c.m.r8(MAIN_NUMBER) == 5 {
            return;
        }
        r.0[0] = 1;
        r.0[1] = 20;
        for (y, glow, ra) in [(41, 3, 0x8004_0908), (74, 2, 0x8004_0938)] {
            let fade = lh(&mut c.m, this + FADE) as u32;
            for (k, v) in [1, glow, fade, 20].into_iter().enumerate() {
                c.m.w32(sp + 16 + 4 * k as u32, v);
            }
            port(c, &r, 0x8004_c018, ra, &[0x0023_2323, 167, y, 316], felines::abi_draw_bright_end_line);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram, same_stack_in};

    /// The game's tournament select screen; the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8017_FE70;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen: the car select's shapes and swapped ones (read or
    /// not), its twelve TVs set from them (the screen was never set up in the front end's RAM),
    /// each on or off; the trophy's TVs turning either way or not about each step, the trophy
    /// swapped in the tournament's or another; the description measured or not; the video's
    /// frames and answers; the tier and tournaments, the money; the main screen's number.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12);
        let s = SCREEN;
        for k in (0..80).step_by(4) {
            let v = m.r32(from + k);
            m.w32(s + k, v);
        }
        m.w32(s + 12, 0);
        m.w32(s + SWAP_TABLE + 12, 0);
        m.w32(s + 36, 0);
        m.w32(s + SWAP_TABLE + 36, 0);
        let buffer = m.r32(from + SWAP_TABLE + 8);
        m.w32(s + SWAP_DATA, [0, buffer][pick(rng, 2) as usize]);
        m.w16(s + FADE, [0, 64, 128, pick(rng, 129)][pick(rng, 4) as usize] as u16);
        let table = m.r32(s + SWAP_TABLE);
        for k in 0..12 {
            crate::abi::call(c, nfs4rc::f_80023168, &[s + VIDEO_TVS + TV_SIZE * k, table, k]);
            c.m.w32(s + VIDEO_TVS + TV_SIZE * k, [0, 1, 1, 2, 3][pick(rng, 5) as usize]);
        }
        let m = &mut c.m;
        m.w8(FE_TIER, pick(rng, 3) as u8);
        m.w8(TIER_TOURNAMENT, pick(rng, 3) as u8);
        m.w8(TOURNAMENT_ONLY, pick(rng, 3) as u8);
        m.w32(TOURNAMENT_MANAGER + MONEY, [0, 1500, 999_999, rng.next() as u32 & 0xF_FFFF][pick(rng, 4) as usize]);
        let t = tournament(m);
        // (now and then a trophy past 127: the original compares it unsigned with the one
        // swapped in signed)
        if pick(rng, 4) == 0 {
            m.w8(t + TROPHY_ID, 128 + pick(rng, 128) as u8);
        }
        let trophy = if pick(rng, 2) == 0 { m.r8(t + TROPHY_ID) } else { pick(rng, 256) as u8 };
        m.w8(s + TROPHY, trophy);
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        m.w32(s + TURNED_AT, ticks.wrapping_sub([0, 3, 4, 7, 8, 15, 16, 17, pick(rng, 200)][pick(rng, 9) as usize]));
        m.w16(s + TURNING, [0xFFFF, 0, 1, 2][pick(rng, 4) as usize]);
        // (the description's text the tournament's now and then, measured or not)
        let it = iterator(m);
        let text = sx(crate::abi::call(c, TOURNAMENT_TEXT, &[it, u32::MAX]).wrapping_add(38));
        let m = &mut c.m;
        m.w32(s + TEXT_ID, [u32::MAX, text, text, rng.next() as u32][pick(rng, 4) as usize]);
        m.w32(s + TEXT_TOP, [u32::MAX, 60, pick(rng, 120)][pick(rng, 3) as usize]);
        m.w32(s + VIDEO, 0x8018_7C08);
        m.w32(s + FRAMES, pick(rng, 100));
        m.w32(VIDEO_ANSWERS, pick(rng, 4));
        m.w32(VIDEO_ANSWERS + 4, pick(rng, 2));
        m.w32(VIDEO_ANSWERS + 8, 0x8018_7C08);
        m.w8(MAIN_NUMBER, [0, 5, pick(rng, 8)][pick(rng, 3) as usize] as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTournSelect", 0x8003_fb48, nfs4rc::f_8003fb48, |_, _| vec![OUT + 0x100]),
            ("~tScreenTournSelect", 0x8003_fb7c, nfs4rc::f_8003fb7c, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
            ("tScreenTournSelect::GetShapeInfo", 0x8003_fba4, nfs4rc::f_8003fba4, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTournSelect::Initialize", 0x8003_fc70, nfs4rc::f_8003fc70, |_, _| vec![SCREEN]),
            ("tScreenTournSelect::Cleanup", 0x8003_feb8, nfs4rc::f_8003feb8, |_, _| vec![SCREEN]),
            ("tScreenTournSelect::UpdateVideoWall", 0x8003_fef8, nfs4rc::f_8003fef8, |c, _| {
                let t = tournament(&mut c.m);
                vec![SCREEN, t]
            }),
            ("tScreenTournSelect::DrawVideoWall", 0x8003_ff9c, nfs4rc::f_8003ff9c, |_, _| vec![SCREEN]),
            ("tScreenTournSelect::DrawBackground", 0x8004_022c, nfs4rc::f_8004022c, |_, _| vec![SCREEN]),
            ("tScreenTournSelect::DrawForeground", 0x8004_08a8, nfs4rc::f_800408a8, |_, _| vec![SCREEN]),
        ]
    }

    #[test]
    fn screentournselect_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x75E ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x75F + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screentournselect_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x76F + k as u64, shuffle, &args);
        }
    }
}
