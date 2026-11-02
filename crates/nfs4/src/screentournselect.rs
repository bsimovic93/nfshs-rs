//! SCREENTOURNSELECT.CPP: the tournament select's screen (`tScreenTournSelect`). It shows the
//! money, the entry fee and the three prizes; the chosen tournament's name and description; its
//! trophy over four TVs (swapped in when the tournament changes, the TVs turned off and on again
//! one by one); the tournaments' video across eight more; and the lines and the turning trophy.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The videos,
//! `ClearImage`, `DrawSync`, `DrawTV`, `DrawTVLines` and the squares are the host's (called by
//! address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenuextended::{SET_ABR_MODE, SPRINTF, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, PATHS, SONGS_SCREEN, TOURNAMENT_MANAGER};
use crate::types::{DrawShapeExtended, RECT, Screen, TVConfig};
use crate::{fefades, fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen` (its swapped shapes the trophy's), then +100 the video, +104 its
/// frames shown, +108 the video's 8 TVs and +492 the trophy's 4 (`tTVConfig`s, 48 bytes each),
/// +684 and +686 the movies (cleared), +692 the tick the trophy's TVs last turned, +696 turning
/// on (1), off (-1) or not, +698 the trophy swapped in, +700 cleared when a trophy is asked
/// for, +704 the description's top (-1: to measure), +708 its text.
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
/// A TV's state on; the trophy's TVs' flags (added), the video's.
const TV_ON: i32 = 1;
const TROPHY_TV_FLAGS: u16 = 0x30;
const VIDEO_TV_FLAGS: u16 = 34;

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
/// The screen's vtable; the trophy's file's name; "ztourn" (64 shapes; the trophy's 32),
/// "%szzzTRN.dct"; the videos' directory; the order the trophy's TVs turn in.
const TOURN_SELECT_VT: u32 = 0x8001_1D60;
const TROPHY_NAME: u32 = 0x8005_3120;
const TOURN_NAME: u32 = 0x8001_1D44;
const SHAPES: u16 = 64;
const TROPHY_SHAPES: u32 = 32;
const VIDEO_FORMAT: u32 = 0x8001_1D4C;
const VIDEO_PATH: u32 = PATHS + 0xA4;
const TV_ORDER: u32 = 0x8005_24E8;
/// The texts: the money's, the entry fee's and the prizes' headings (124, 154, 155), the
/// places (725 on); a tournament's description (its name's id + 38), its first paragraph (from
/// 891, as the descriptions from 872) and that paragraph's heading (988).
const MONEY_TEXT: u32 = 124;
const FEE_TEXT: u32 = 154;
const PRIZES_TEXT: u32 = 155;
const PLACES: u32 = 725;
const DESCRIPTIONS: u32 = 38;
const FIRST_DESCRIPTION: u32 = 872;
const FIRST_PARAGRAPH: u32 = 891;
const PARAGRAPH_HEADING: u32 = 988;
/// The money's kind (`DrawMoney`'s), the colours (gold, shade, grey) faded by the screen.
const MONEY_KIND: u32 = 6;
const GOLD: u32 = 0xBEBE;
const SHADE: u32 = 0x0023_2323;
const GREY: u32 = 0x0050_5050;
/// The trophy's shapes' flags (its own table, from its top left).
const TROPHY_FLAGS: u32 = 0x600;

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
/// `GetTrophyName`, `tListIteratorTournament::TextValue` (FETOURN.CPP's ports); `GetTPage`,
/// `ScaleShapeExtended`, `purgememadr`, `TextSys_WordX`, `TextSys_WordY`.
const GET_TROPHY_NAME: fn(&mut Cpu) = fetourn::get_trophy_name_entry;
const TOURNAMENT_TEXT: fn(&mut Cpu) = fetourn::text_value_entry;
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// Ports called by address: FETV's `InitTV`, `TurnOnTV`, `TurnOffTV`; FELINES'
/// `DrawBrightEndLine`; SCREENPOST's `DrawMoney`.
const INIT_TV: u32 = 0x8002_3168;
const TURN_ON_TV: u32 = 0x8002_3388;
const TURN_OFF_TV: u32 = 0x8002_3378;
const DRAW_BRIGHT_END_LINE: u32 = 0x8004_C018;
const DRAW_MONEY: u32 = 0x8003_9A80;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn word_x(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_X, &[k])
}

fn word_y(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_Y, &[k])
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

/// The video's TV `k` and the trophy's.
fn video_tv(this: u32, k: u32) -> TVConfig {
    TVConfig(this + VIDEO_TVS).at(k)
}

fn trophy_tv(this: u32, k: u32) -> TVConfig {
    TVConfig(this + TROPHY_TVS).at(k)
}

/// The screen's fade.
fn fade(m: &mut Mem, this: u32) -> u32 {
    Screen(this).screen_fade_val(m) as i32 as u32
}

/// The video's file name for `f`.
fn with_video_file<R>(c: &mut Cpu, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    with_buffer(c, 128, |c, file| {
        let path = c.m.r32(VIDEO_PATH);
        call(c, SPRINTF, &[file, VIDEO_FORMAT, path]);
        f(c, file)
    })
}

/// `tScreenTournSelect::tScreenTournSelect` (SCREENTOURNSELECT.CPP, 0x8003fb48).
pub fn screen_tourn_select(c: &mut Cpu, this: Screen) -> Screen {
    fescreen::screen(c, this);
    this.set_vtable(&mut c.m, TOURN_SELECT_VT);
    this
}

pub fn abi_screen_tourn_select(c: &mut Cpu) {
    c.r[2] = screen_tourn_select(c, Screen(c.r[4])).0;
}

/// `tScreenTournSelect::~tScreenTournSelect` (SCREENTOURNSELECT.CPP, 0x8003fb7c).
pub fn abi_destructor(c: &mut Cpu) {
    let this = Screen(c.r[4]);
    this.set_vtable(&mut c.m, TOURN_SELECT_VT);
    fescreen::screen_destructor(c, this, c.r[5]);
}

/// `tScreenTournSelect::GetShapeInfo` (SCREENTOURNSELECT.CPP, 0x8003fba4): 64 shapes of
/// "ztourn", 32 of the chosen tournament's trophy.
pub fn get_shape_info(c: &mut Cpu, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    let m = &mut c.m;
    let t = tournament(m);
    m.w16(permanent, SHAPES);
    m.w16(swap, TROPHY_SHAPES as u16);
    m.w32(name, TOURN_NAME);
    call(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 1, TROPHY_NAME, u32::MAX]);
    c.m.w32(swap_name, TROPHY_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTournSelect::Initialize` (SCREENTOURNSELECT.CPP, 0x8003fc70): the description to
/// measure; the video's place cleared; the screen's files; the tournaments' video made and
/// played; the trophy's four TVs set (two by two); the chosen tournament's trophy swapped in,
/// its TVs turning on from now.
pub fn initialize(c: &mut Cpu, this: Screen) {
    let this = this.0;
    c.m.w32(this + TEXT_TOP, u32::MAX);
    c.m.w32(this + TEXT_ID, u32::MAX);
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 512);
        rect.set_y(m, 0);
        rect.set_w(m, 80);
        rect.set_h(m, 160);
        call_addr(c, CLEAR_IMAGE, &[at, 0, 0, 0]);
        call_addr(c, DRAW_SYNC, &[0]);
    });
    fescreen::initialize(c, Screen(this));
    c.m.w16(this + CURRENT_MOVIE, 0);
    c.m.w16(this + PREVIOUS_MOVIE, 0);
    with_video_file(c, |c, file| {
        let video = call_addr(c, VIDEO_CREATE, &[80, 80, 0xF_0000, 0x2_5800, 16]);
        c.m.w32(this + VIDEO, video);
        call_addr(c, VIDEO_SPOOL, &[video, file]);
        let video = c.m.r32(this + VIDEO);
        call_addr(c, VIDEO_START, &[video]);
    });
    c.m.w32(this + FRAMES, 0);
    // the trophy's TVs: two columns of two
    for row in 0..2u32 {
        for col in 0..2u32 {
            let tv = trophy_tv(this, 2 * row + col);
            let table = Screen(this).perm_shapes().shapes(&mut c.m);
            call_addr(c, INIT_TV, &[tv.0, table.0, 0]);
            let m = &mut c.m;
            tv.set_x(m, (76 * row + 165) as i16);
            tv.set_y(m, (37 * col + 142) as i16);
            tv.set_w(m, 76);
            tv.set_h(m, 37);
            tv.update_flags(m, |f| f | TROPHY_TV_FLAGS);
        }
    }
    let m = &mut c.m;
    let t = tournament(m);
    let trophy = m.r8(t + TROPHY_ID);
    m.w8(this + TROPHY, trophy);
    let ticks = m.r32(GAME_TICKS);
    m.w16(this + TURNING, 1);
    m.w32(this + TURNED_AT, ticks);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTournSelect::Cleanup` (SCREENTOURNSELECT.CPP, 0x8003feb8): the video destroyed and
/// freed; the screen's.
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

/// `tScreenTournSelect::UpdateVideoWall` (SCREENTOURNSELECT.CPP, 0x8003fef8): a tournament whose
/// trophy isn't the one swapped in: its trophy read, its TVs turning off from now (unless they
/// are).
pub fn update_video_wall(c: &mut Cpu, this: u32, t: u32) {
    if c.m.r8(t + TROPHY_ID) as u32 == c.m.r8(this + TROPHY) as i8 as i32 as u32 {
        return;
    }
    call(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 1, TROPHY_NAME, u32::MAX]);
    fescreen::async_load_swap_shape_file(c, Screen(this), TROPHY_NAME);
    let m = &mut c.m;
    m.w32(this + TVS_SET, 0);
    let trophy = m.r8(t + TROPHY_ID);
    m.w8(this + TROPHY, trophy);
    if lh(m, this + TURNING) >= 0 {
        let ticks = m.r32(GAME_TICKS);
        m.w16(this + TURNING, 0xFFFF);
        m.w32(this + TURNED_AT, ticks);
    }
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenTournSelect::DrawVideoWall` (SCREENTOURNSELECT.CPP, 0x8003ff9c): the background; the
/// lines between the video's TVs; the trophy's TVs turned on in order, one more every 4 ticks (or
/// off, done after 16 ticks); their frame; the TVs; the trophy turning.
pub fn draw_video_wall(c: &mut Cpu, this: u32) {
    let screen = Screen(this);
    let table = screen.perm_shapes().shapes(&mut c.m);
    fescreen::draw_background_image(c, screen, 0, 34, table, 0);
    for x in (244..485).step_by(80) {
        call_addr(c, DRAW_TRANS_SQUARE, &[0x0020_2020, x, 41, 2, 97, 1]);
    }
    call_addr(c, DRAW_TRANS_SQUARE, &[0x0014_1414, 165, 89, 321, 1, 1]);
    call(c, SET_ABR_MODE, &[2]);
    // the trophy's TVs turning, one more every 4 ticks: on (while turning on), else off
    let m = &mut c.m;
    let n = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + TURNED_AT)) >> 2) as i32;
    let turning = lh(m, this + TURNING);
    // (each in turn: the ones off turned on, or the ones on turned off)
    let (from, turn) = if turning > 0 { (0, TURN_ON_TV) } else { (TV_ON, TURN_OFF_TV) };
    for k in 0..n.clamp(0, 4) as u32 {
        let tv = trophy_tv(this, c.m.r8(TV_ORDER + k) as u32);
        if tv.state(&mut c.m) == from {
            call_addr(c, turn, &[tv.0]);
        }
    }
    if turning <= 0 && n >= 4 {
        c.m.w16(this + TURNING, 0);
    }
    // the frame, the TVs, the trophy
    call_addr(c, DRAW_TRANS_SQUARE, &[0x0030_3030, 241, 142, 2, 74, 1]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0x0020_2020, 165, 179, 152, 1, 1]);
    call(c, SET_ABR_MODE, &[2]);
    for k in 0..4 {
        call_addr(c, DRAW_TV, &[trophy_tv(this, k).0]);
    }
    let frame = ((c.m.r32(GAME_TICKS) as i32 >> 4) % 32) as u32;
    with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
        let table = screen.swap_shapes().shapes(&mut c.m);
        DrawShapeExtended(options).set_custom_shapes(&mut c.m, table);
        call(c, SCALE_SHAPE_EXTENDED, &[frame, TROPHY_FLAGS, 182, 147, 0, 0, options]);
    });
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, c.r[4]);
}

/// An amount of money at x, y in gold with its shade.
fn money(c: &mut Cpu, (gold, shade): (u32, u32), x: u32, y: u32, amount: u32) {
    call_addr(c, DRAW_MONEY, &[x, y, MONEY_KIND, amount, gold, shade]);
}

/// `tScreenTournSelect::DrawBackground` (SCREENTOURNSELECT.CPP, 0x8004022c). It draws the
/// money, the entry fee and the three prizes (their headings and amounts); the trophy swapped
/// in and its TVs; the video's next frame over its eight TVs (or the video played again); the
/// tournament's name and description (its top measured when it changes), the headings; the TVs.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    let m = &mut c.m;
    let t = tournament(m);
    let f = fade(m, this);
    let colours = (fefades::fade(GOLD, 0, f), fefades::fade(SHADE, 0, f));
    let grey = fefades::fade(GREY, 0, f);
    // the money, the entry fee
    let amount = m.r32(TOURNAMENT_MANAGER + MONEY);
    fetextrender::menu_text_fade(c, f, MONEY_TEXT, 1, 4);
    let x = word_x(c, MONEY_TEXT).wrapping_add(140);
    let y = word_y(c, MONEY_TEXT).wrapping_add(9);
    money(c, colours, x, y, amount);
    let f = fade(&mut c.m, this);
    fetextrender::menu_text_fade(c, f, FEE_TEXT, 1, 14);
    let x = word_x(c, FEE_TEXT).wrapping_add(140);
    let y = word_y(c, FEE_TEXT).wrapping_add(9);
    let fee = c.m.r32(t + ENTRY_FEE);
    money(c, colours, x, y, fee);
    // the prizes: each place and its amount
    let f = fade(&mut c.m, this);
    fetextrender::menu_text_fade(c, f, PRIZES_TEXT, 1, 14);
    let mut y = word_y(c, PRIZES_TEXT).wrapping_add(9);
    for k in 0..3u32 {
        let w = call(c, TEXT_WORD, &[k + PLACES]);
        let x = word_x(c, PRIZES_TEXT);
        fetextrender::full_text_rgb(c, w, sx(x), sx(y), grey, 0, 0);
        let x = word_x(c, FEE_TEXT).wrapping_add(140);
        let prize = c.m.r32(t.wrapping_add(4 * k) + PRIZES);
        money(c, colours, x, sx(y), prize);
        y = y.wrapping_add(9);
    }
    // the trophy
    update_video_wall(c, this, t);
    let screen = Screen(this);
    fescreen::is_shape_file_loaded(c, screen, screen.swap_shapes());
    if screen.swap_shapes().file(&mut c.m) != 0 && lh(&mut c.m, this + TURNING) >= 0 {
        fescreen::upload_swap_shapes(c, screen, TROPHY_SHAPES);
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w16(this + TURNING, 1);
        c.m.w32(this + TURNED_AT, ticks);
    }
    draw_video_wall(c, this);
    // the video's TVs: two rows of four, each a fifth of the frame (the odd frames' at 0, the
    // even at 80)
    let page_y = if c.m.r32(this + FRAMES) & 1 != 0 { 0 } else { 80 };
    for row in 0..2u32 {
        for col in 0..4u32 {
            let tv = video_tv(this, col + 4 * row);
            let m = &mut c.m;
            tv.set_x(m, (80 * col + 165) as i16);
            tv.set_w(m, 80);
            tv.set_h(m, 48);
            tv.set_uw(m, 20);
            tv.set_y(m, (48 * row + 41) as i16);
            tv.set_u(m, (20 * col) as u8);
            tv.set_v(m, (40 * row) as u8);
            tv.set_vh(m, 40);
            let page = call(c, GET_TPAGE, &[2, 0, 512, page_y]);
            let m = &mut c.m;
            tv.set_tpage(m, page as u16);
            tv.set_state(m, TV_ON);
            tv.set_clut(m, 0);
            tv.set_flags(m, VIDEO_TV_FLAGS);
            tv.set_tint(m, 0x002C_1E1E);
            tv.set_dest_brightness(m, 128);
            tv.set_transition(m, 128);
        }
    }
    let video = c.m.r32(this + VIDEO);
    if call_addr(c, VIDEO_STATE, &[video]) != 0 {
        let video = c.m.r32(this + VIDEO);
        if call_addr(c, VIDEO_UPDATE, &[video, 512, page_y]) != 0 {
            let n = c.m.r32(this + FRAMES).wrapping_add(1);
            c.m.w32(this + FRAMES, n);
        }
    } else {
        c.m.w16(this + CURRENT_MOVIE, 0);
        with_video_file(c, |c, file| {
            let video = c.m.r32(this + VIDEO);
            call_addr(c, VIDEO_SPOOL, &[video, file]);
            let video = c.m.r32(this + VIDEO);
            call_addr(c, VIDEO_START, &[video]);
        });
    }
    for k in 0..8 {
        call_addr(c, DRAW_TV_LINES, &[video_tv(this, k).0]);
    }
    // the tournament's name and description (its top measured when it changes), the heading
    with_buffer(c, 8, |c, at| {
        let (rect, m) = (RECT(at), &mut c.m);
        rect.set_x(m, 325);
        rect.set_y(m, 43);
        rect.set_w(m, 314);
        rect.set_h(m, 10);
        let it = iterator(m);
        let name = call(c, TOURNAMENT_TEXT, &[it, u32::MAX]);
        let f = fade(&mut c.m, this);
        fetextrender::word_wrap_fade(c, f, sx(name), at, 2, 13);
        let m = &mut c.m;
        rect.set_x(m, 170);
        rect.update_w(m, |w| w.wrapping_sub(10));
        let it = iterator(m);
        let description = call(c, TOURNAMENT_TEXT, &[it, u32::MAX]).wrapping_add(DESCRIPTIONS);
        let id = sx(description);
        if id != c.m.r32(this + TEXT_ID) || c.m.r32(this + TEXT_TOP) == u32::MAX {
            c.m.w32(this + TEXT_ID, id);
            let w = call(c, TEXT_WORD, &[id]);
            let width = rect.w(&mut c.m) as i32 as u32;
            let h = fetextrender::word_wrap_height(c, width, w);
            c.m.w32(this + TEXT_TOP, 117u32.wrapping_sub(h));
        }
        let top = c.m.r16(this + TEXT_TOP) as i16;
        rect.set_y(&mut c.m, top);
        let w = call(c, TEXT_WORD, &[id]);
        let grey = fefades::fade(GREY, 0, fade(&mut c.m, this));
        fetextrender::word_wrap_text_rgb(c, w, at, grey);
        let f = fade(&mut c.m, this);
        fetextrender::menu_text_positioned_justify_fade(c, f, PARAGRAPH_HEADING, 170, 117, 0, 1, 4);
        let paragraph = sx(description.wrapping_sub(FIRST_DESCRIPTION).wrapping_add(FIRST_PARAGRAPH));
        let f = fade(&mut c.m, this);
        fetextrender::menu_text_positioned_justify_fade(c, f, paragraph, 170, 125, 0, 2, 4);
    });
    for k in 0..8 {
        call_addr(c, DRAW_TV, &[video_tv(this, k).0]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTournSelect::DrawForeground` (SCREENTOURNSELECT.CPP, 0x800408a8): two lines along the
/// money's box (not on the fifth main screen).
pub fn draw_foreground(c: &mut Cpu, this: Screen) {
    if c.m.r8(MAIN_NUMBER) == 5 {
        return;
    }
    for (y, glow) in [(41, 3), (74, 2)] {
        let f = fade(&mut c.m, this.0);
        call_addr(c, DRAW_BRIGHT_END_LINE, &[SHADE, 167, y, 316, 1, glow, f, 20]);
    }
}

pub fn abi_draw_foreground(c: &mut Cpu) {
    draw_foreground(c, Screen(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, VIDEO_ANSWERS, against_original_in, fe_ram};

    /// The screen's swapped shapes (their table, the file read), its fade; a TV's size.
    const SWAP_TABLE: u32 = 40;
    const SWAP_DATA: u32 = 44;
    const FADE: u32 = 92;
    const TV_SIZE: u32 = 48;

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

}
