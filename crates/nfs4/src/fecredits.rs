//! FECREDITS.CPP: the credits (`tCreditManager`), over the main menu's TVs: their pages read
//! from the main screen's `zcred*.dat` (324 bytes each: a title and a subtitle, at a place or
//! word-wrapped, and lines of text), each faded in, shown 12 seconds (33 for a page of names)
//! or until a key, faded out; every third page and any page asking for one swaps the main
//! screen's background first; a picture spinning in the corner, two lines drawn out under the
//! titles.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, STRLEN, TEXT_PIXELS, TEXT_WORD};
use crate::felines::Line;
use crate::fefades::fade_out;
use crate::globals::{FRONT_END, GAME_TICKS, PATHS};
use crate::types::{DrawShapeExtended, RECT, TVConfig};
use crate::{fecheats, feinput, felines, fetextrender, screenmain};
use ps1rt::{Cpu, Mem};

// The credits: +0 their pages, +4 their fade in (0..92, 4 a frame), +8 the page's fade (0..128)
// and +12 its step (8 in, -8 out), +16 loaded, +20 to be freed once faded out, +24 how many
// pages, +28 the page shown (-1: none yet), +32 the next, +36 the page's background asked for,
// +40 come (+48 when), +44 fading out, +52 the tick the page was shown from (0: not yet).
const PAGES: u32 = 0;
const SHOWN: u32 = 4;
const FADE: u32 = 8;
const STEP: u32 = 12;
const LOADED: u32 = 16;
const TO_FREE: u32 = 20;
const COUNT: u32 = 24;
const PAGE: u32 = 28;
const NEXT: u32 = 32;
const ASKED: u32 = 36;
const CAME: u32 = 40;
const OUT: u32 = 44;
const CAME_AT: u32 = 48;
const SHOWN_AT: u32 = 52;
/// Shown in full (the credits' fade in), the page's fade full and its steps.
const SHOWN_FULL: i32 = 92;
const SHOWN_STEP: u32 = 4;
const FADE_FULL: u32 = 128;
const FADE_IN: u32 = 8;
const FADE_OUT: u32 = (-8i32) as u32;

// A page: +0 its title's text (from 1301 on), +2 its subtitle's, +4 the title's place, +8 its
// justification, +10 its width (0: on one line), +12..+18 the subtitle's, +20 the lines' x (0:
// beside the subtitle), +22 their y (0: under it), +24 their justification, +28 the background
// it asks for (-1: none), +32 its lines (`\n` between; a tab marks a cheat's line, `*` the
// thanks, `^` the names).
const PAGE_SIZE: u32 = 324;
const TITLE_TEXT: u32 = 0;
const SUBTITLE_TEXT: u32 = 2;
const TITLE: u32 = 4;
const SUBTITLE: u32 = 12;
const LINES_X: u32 = 20;
const LINES_Y: u32 = 22;
const LINES_JUSTIFY: u32 = 24;
const BACKGROUND: u32 = 28;
const LINES: u32 = 32;
/// A title's place: x, y, justification, width.
const X: u32 = 0;
const Y: u32 = 2;
const JUSTIFY: u32 = 4;
const WIDTH: u32 = 6;

/// The last key's tick, how long a page stays (700 ticks, 2000 for the names), when the
/// background was asked for.
const KEY_AT: u32 = 0x8005_1F2C;
const STAY: u32 = 0x8005_1F28;
const ASKED_AT: u32 = 0x8005_1F30;
const STAY_PAGE: u32 = 700;
const STAY_NAMES: u32 = 2000;
/// A key turns a page 13 ticks after the last; the background comes 129 ticks after it is
/// asked for; the page goes 31 ticks after it came.
const KEY_REPEAT: i32 = 13;
const BACKGROUND_WAIT: i32 = 129;
const PAGE_WAIT: i32 = 31;
/// The main screen (`tScreenMain *`).
const MAIN_SCREEN: u32 = 0x8005_22E8;
/// "%szcred%d.dat", "records"; the credits' directory (`Paths_Paths + 0x94`), the main
/// screen's number (`frontEnd + 0x340`).
const FILE_FORMAT: u32 = 0x8001_180C;
const RECORDS: u32 = 0x8001_181C;
const CREDITS_PATH: u32 = PATHS + 0x94;
const MAIN_NUMBER: u32 = FRONT_END + 0x340;
/// The texts: the pages' (from 1301), the thanks', the names' (25).
const PAGE_TEXT: u32 = 1301;
const THANKS: u32 = 1431;
const NAMES: u32 = 1432;
const NAME_COUNT: u32 = 25;
/// The cheat whose lines show.
const CHEAT: u32 = 12;
/// The key that turns a page (the pad's Start), its sound.
const KEY: u32 = 32;
const TURN_SFX: u32 = 6;
/// The main screen's 16 TVs (from +124); their flag dimming them.
const TVS: u32 = 124;
const TV_COUNT: u32 = 16;
const TV_TINTED: u16 = 2;
/// The colours: the titles', the lines', a cheat's line and the drawn lines'; the spinning
/// picture's (shapes 230..239, a frame every 16 ticks, at 16, 16 by its corner, coloured).
const TITLE_COLOR: u32 = 0xBEBE;
const LINES_COLOR: u32 = 0x0078_7878;
const GREY: u32 = 0x0050_5050;
const SPIN_COLOR: i32 = 0x00CE_C844;
const SPIN_SHAPE: i32 = 230;
const SPIN_FLAGS: u32 = 0x410;

/// `filesize`, `reservememadr`, `loadfileatadrz`, `purgememadr`, `strcpy`, `strchr`,
/// `AudioCmn_PlayFESFX`.
const FILE_SIZE: fn(&mut Cpu) = nfs4rc::f_800e6258;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e649c;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const STRCPY: fn(&mut Cpu) = nfs4rc::f_800e6714;
const STRCHR: fn(&mut Cpu) = nfs4rc::f_800f6b70;
const PLAY_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// `tCreditManager::Setup` (FECREDITS.CPP, 0x80035ca0): nothing loaded, nothing shown.
pub fn setup(m: &mut Mem, this: u32) {
    m.w32(this + LOADED, 0);
    m.w32(this + TO_FREE, 0);
    m.w32(this + SHOWN, 0);
}

pub fn abi_setup(c: &mut Cpu) {
    setup(&mut c.m, c.r[4]);
}

/// `tCreditManager::Init` (FECREDITS.CPP, 0x80035cb0): unless loaded, the main screen's pages
/// read, nothing shown yet (the page's fade full, no page).
pub fn init(c: &mut Cpu, this: u32) {
    if c.m.r32(this + LOADED) != 0 {
        return;
    }
    let m = &mut c.m;
    if m.r32(this + TO_FREE) != 0 {
        m.w32(this + TO_FREE, 0);
    }
    m.w32(this + PAGE, u32::MAX);
    m.w32(this + FADE, FADE_FULL);
    for f in [SHOWN_AT, ASKED, CAME, OUT, TO_FREE, NEXT] {
        m.w32(this + f, 0);
    }
    let (path, n) = (m.r32(CREDITS_PATH), m.r8(MAIN_NUMBER) as u32);
    with_buffer(c, 88, |c, file| {
        call(c, SPRINTF, &[file, FILE_FORMAT, path, n]);
        let size = call(c, FILE_SIZE, &[file]);
        let pages = call(c, RESERVE, &[RECORDS, size, 0]);
        c.m.w32(this + PAGES, pages);
        call(c, LOAD_FILE, &[file, pages]);
        let size = call(c, FILE_SIZE, &[file]);
        c.m.w32(this + LOADED, 1);
        c.m.w32(this + COUNT, size / PAGE_SIZE);
    });
}

pub fn abi_init(c: &mut Cpu) {
    init(c, c.r[4]);
}

/// `tCreditManager::DeInit` (FECREDITS.CPP, 0x80035d90): to be freed once faded out.
pub fn de_init(m: &mut Mem, this: u32) {
    m.w32(this + TO_FREE, 1);
}

pub fn abi_de_init(c: &mut Cpu) {
    de_init(&mut c.m, c.r[4]);
}

/// `tCreditManager::RealDeInit` (FECREDITS.CPP, 0x80035d9c): the pages freed.
pub fn real_de_init(c: &mut Cpu, this: u32) {
    c.m.w32(this + TO_FREE, 0);
    if c.m.r32(this + LOADED) != 0 {
        let pages = c.m.r32(this + PAGES);
        call(c, PURGE, &[pages]);
        c.m.w32(this + LOADED, 0);
    }
}

pub fn abi_real_de_init(c: &mut Cpu) {
    real_de_init(c, c.r[4]);
}

/// `tCreditManager::Draw` (FECREDITS.CPP, 0x80035ddc): the credits faded in (`on`) or out, the
/// main screen's TVs darkened as much (the page's fade held up meanwhile); once faded out the
/// pages freed if asked; else the pages turned and drawn.
pub fn draw(c: &mut Cpu, this: u32, on: bool) {
    let m = &mut c.m;
    let step = if on { SHOWN_STEP } else { SHOWN_STEP.wrapping_neg() };
    let v = m.r32(this + SHOWN).wrapping_add(step);
    m.w32(this + SHOWN, v);
    let v = (m.r32(this + SHOWN) as i32).clamp(0, SHOWN_FULL);
    m.w32(this + SHOWN, v as u32);
    if v < SHOWN_FULL {
        let fade = (FADE_FULL as i32 - v).max(m.r32(this + FADE) as i32);
        m.w32(this + FADE, fade as u32);
    }
    let screen = m.r32(MAIN_SCREEN);
    for k in 0..TV_COUNT {
        let tv = TVConfig(screen.wrapping_add(TVS)).at(k);
        tv.update_flags(m, |f| f | TV_TINTED);
        let f = 128u32.wrapping_sub(m.r32(this + SHOWN));
        tv.set_tint(m, ((f << 16) | (f << 8) | f) as i32);
    }
    if c.m.r32(this + SHOWN) == 0 {
        if c.m.r32(this + TO_FREE) != 0 {
            real_de_init(c, this);
        }
    } else if c.m.r32(this + LOADED) != 0 {
        update(c, this);
        if c.m.r32(this + PAGE) != u32::MAX {
            draw_page(c, this);
        }
    }
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, c.r[4], c.r[5] != 0);
}

/// The next page from `at` on, round to the first; past the first back to the last.
fn wrap(m: &mut Mem, this: u32) {
    if m.r32(this + NEXT) as i32 >= m.r32(this + COUNT) as i32 {
        m.w32(this + NEXT, 0);
    }
    if (m.r32(this + NEXT) as i32) < 0 {
        let v = m.r32(this + COUNT).wrapping_sub(1);
        m.w32(this + NEXT, v);
    }
}

/// `tCreditManager::Update` (FECREDITS.CPP, 0x80035f20): Start on either pad (13 ticks after
/// the last) turns a page shown in full, or the page's time running out does; a new page fades
/// out the one shown, then in; once in, its background asked for (every third page's, or the
/// one it asks for); once the background is up, half a second on, the page fades out.
pub fn update(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(KEY_AT)) as i32;
    if since >= KEY_REPEAT && m.r32(this + FADE) == 0 && m.r32(this + NEXT) == m.r32(this + PAGE) {
        let key = feinput::get_no_debounce_key(c, KEY, 0) || feinput::get_no_debounce_key(c, KEY, 1);
        if key {
            call(c, PLAY_SFX, &[TURN_SFX]);
            let m = &mut c.m;
            let page = m.r32(this + PAGE);
            m.w32(this + SHOWN_AT, 0);
            m.w32(this + NEXT, page.wrapping_add(1));
            let ticks = m.r32(GAME_TICKS);
            m.w32(KEY_AT, ticks);
        }
        wrap(&mut c.m, this);
    }
    let m = &mut c.m;
    let at = m.r32(this + SHOWN_AT);
    if at != 0 && (m.r32(STAY) as i32) < m.r32(GAME_TICKS).wrapping_sub(at) as i32 {
        let v = m.r32(this + NEXT).wrapping_add(1);
        m.w32(this + NEXT, v);
        wrap(m, this);
        m.w32(this + SHOWN_AT, 0);
    }
    if m.r32(this + PAGE) != m.r32(this + NEXT) {
        m.w32(this + STEP, FADE_IN);
        for f in [ASKED, CAME, OUT] {
            m.w32(this + f, 0);
        }
    }
    let v = m.r32(this + FADE).wrapping_add(m.r32(this + STEP));
    m.w32(this + FADE, v);
    if v as i32 <= 0 {
        m.w32(this + FADE, 0);
    }
    if m.r32(this + FADE) as i32 >= FADE_FULL as i32 {
        m.w32(this + FADE, FADE_FULL);
    }
    // faded out: the next page, its background asked for
    if m.r32(this + FADE) == FADE_FULL && m.r32(this + ASKED) == 0 {
        let page = m.r32(this + NEXT);
        m.w32(this + ASKED, 1);
        m.w32(this + PAGE, page);
        let mut next = page.wrapping_add(1);
        if next as i32 >= m.r32(this + COUNT) as i32 {
            next = 0;
        }
        let back = m.r32(this + PAGES).wrapping_add(next.wrapping_mul(PAGE_SIZE)) + BACKGROUND;
        if (page as i32).wrapping_rem(3) != 0 && lh(m, back) == u32::MAX {
            m.w32(ASKED_AT, 0);
        } else {
            let (screen, which) = (m.r32(MAIN_SCREEN), lh(m, back));
            screenmain::swap_background(c, screen, which);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w32(ASKED_AT, ticks);
        }
    }
    let m = &mut c.m;
    if m.r32(this + CAME) == 0 && m.r32(this + ASKED) != 0 && m.r32(GAME_TICKS).wrapping_sub(m.r32(ASKED_AT)) as i32 >= BACKGROUND_WAIT {
        let screen = m.r32(MAIN_SCREEN);
        if screenmain::done_loading_background(m, screen) {
            let ticks = m.r32(GAME_TICKS);
            m.w32(this + CAME, 1);
            m.w32(this + CAME_AT, ticks);
        }
    }
    if m.r32(this + OUT) == 0 && m.r32(this + CAME) != 0 && m.r32(GAME_TICKS).wrapping_sub(m.r32(this + CAME_AT)) as i32 >= PAGE_WAIT {
        let ticks = m.r32(GAME_TICKS);
        m.w32(this + OUT, 1);
        m.w32(this + STEP, FADE_OUT);
        m.w32(this + SHOWN_AT, ticks);
    }
}

pub fn abi_update(c: &mut Cpu) {
    update(c, c.r[4]);
}

/// `tCreditManager::DrawPage` (FECREDITS.CPP, 0x80036278): the spinning picture; the title and
/// the subtitle (on a line, or word-wrapped), the lines under or beside them (the thanks; the 25
/// names; a cheat's lines while it is on), the page's time set by them; once its background has
/// come, two lines drawn out under the titles.
pub fn draw_page(c: &mut Cpu, this: u32) {
    let ticks = c.m.r32(GAME_TICKS) as i32;
    with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
        DrawShapeExtended(options).set_tint(&mut c.m, 0, SPIN_COLOR);
        let shape = (SPIN_SHAPE + (ticks >> 4) % 10) as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[shape, SPIN_FLAGS, 16, 16, 0, 0, options]);
    });
    let page = {
        let m = &mut c.m;
        m.r32(this + PAGES).wrapping_add(m.r32(this + PAGE).wrapping_mul(PAGE_SIZE))
    };
    fetextrender::set_abr(c, 1, 1);
    // the lines' y: under the subtitle, past it when it wraps
    let mut y = c.m.r16(page + SUBTITLE + Y) as u32;
    let f = c.m.r32(this + FADE);
    let title_color = fade_out(fade_out(TITLE_COLOR, f), 40);
    let subtitle_color = fade_out(fade_out(TITLE_COLOR, f), 40);
    let lines_color = fade_out(LINES_COLOR, f);
    let x = with_buffer(c, 8 + 296, |c, buffer| {
        let (rect, text) = (buffer, buffer + 8);
        for (id_at, place, color) in [(TITLE_TEXT, page + TITLE, title_color), (SUBTITLE_TEXT, page + SUBTITLE, subtitle_color)] {
            let subtitle = id_at == SUBTITLE_TEXT;
            let m = &mut c.m;
            let id = lh(m, page + id_at).wrapping_add(PAGE_TEXT);
            if lh(m, place + WIDTH) != 0 {
                for (k, f) in [X, Y, WIDTH].into_iter().enumerate() {
                    let v = m.r16(place + f);
                    m.w16(rect + 2 * k as u32, v);
                }
                m.w16(rect + 6, 100);
                let w = call(c, TEXT_WORD, &[id]);
                let justify = lh(&mut c.m, place + JUSTIFY);
                let h = fetextrender::word_wrap_text_rgb_justify(c, w, RECT(rect), color, justify, 0, 0);
                if subtitle {
                    y = y.wrapping_add(h);
                }
            } else {
                if subtitle {
                    y = y.wrapping_add(8);
                }
                let w = call(c, TEXT_WORD, &[id]);
                let m = &mut c.m;
                let (x, ty, justify) = (lh(m, place + X), lh(m, place + Y), lh(m, place + JUSTIFY));
                fetextrender::full_text_rgb(c, w, x, ty, color, 0, justify);
            }
        }
        let m = &mut c.m;
        if lh(m, page + LINES_Y) != 0 {
            y = m.r16(page + LINES_Y) as u32;
        }
        // the lines' x: given, or beside the subtitle (its width, or its text's)
        let mut x = m.r16(page + LINES_X);
        let mut beside = lh(m, page + SUBTITLE + WIDTH);
        if beside == 0 {
            fetextrender::set_font(c, 0);
            let id = lh(&mut c.m, page + SUBTITLE_TEXT).wrapping_add(PAGE_TEXT);
            let w = call(c, TEXT_WORD, &[id]);
            let px = call(c, TEXT_PIXELS, &[w]);
            let w = call(c, TEXT_WORD, &[id]);
            beside = px.wrapping_sub(call(c, STRLEN, &[w]));
        }
        let m = &mut c.m;
        if x == 0 {
            let sx_ = m.r16(page + SUBTITLE + X) as u32;
            x = if lh(m, page + SUBTITLE + JUSTIFY) == 0 { sx_.wrapping_add(beside).wrapping_add(2) } else { sx_.wrapping_sub(beside).wrapping_sub(2) } as u16;
        }
        let x = x as i16 as i32 as u32;
        // the lines
        call(c, STRCPY, &[text, page + LINES]);
        let mut at = text;
        while at != 0 {
            let end = call(c, STRCHR, &[at, b'\n' as u32]);
            if end != 0 {
                c.m.w8(end, 0);
            }
            let m = &mut c.m;
            let mut skip = |m: &mut Mem, ch: u8| {
                let found = m.r8(at) == ch;
                while m.r8(at) == ch {
                    at = at.wrapping_add(1);
                }
                found
            };
            skip(m, b'\n');
            let cheat = skip(m, b'\t');
            let thanks = skip(m, b'*');
            let names = skip(m, b'^');
            m.w32(STAY, if names { STAY_NAMES } else { STAY_PAGE });
            let (line_y, justify) = (sx(y), lh(m, page + LINES_JUSTIFY));
            if thanks {
                for (k, v) in [180u16, 85, 280, 100].into_iter().enumerate() {
                    m.w16(rect + 2 * k as u32, v);
                }
                let w = call(c, TEXT_WORD, &[THANKS]);
                fetextrender::word_wrap_text_rgb_justify(c, w, RECT(rect), lines_color, 0, 0, 0);
            } else if names {
                for k in 0..NAME_COUNT {
                    let w = call(c, TEXT_WORD, &[NAMES + k]);
                    let name_y = sx(y);
                    y = y.wrapping_add(8);
                    let justify = lh(&mut c.m, page + LINES_JUSTIFY);
                    fetextrender::full_text_rgb(c, w, x, name_y, lines_color, 0, justify);
                }
            } else if !cheat {
                fetextrender::full_text_rgb(c, at, x, line_y, lines_color, 0, justify);
            } else if fecheats::is_cheat_enabled(&mut c.m, CHEAT) {
                let color = fade_out(GREY, c.m.r32(this + FADE));
                let justify = lh(&mut c.m, page + LINES_JUSTIFY);
                fetextrender::full_text_rgb(c, at, x, line_y, color, 0, justify);
            }
            y = y.wrapping_add(8);
            at = if end == 0 { 0 } else { end + 1 };
        }
        x
    });
    fetextrender::set_abr(c, 0, 0);
    if c.m.r32(this + CAME) != 0 {
        draw_lines(c, this, page, x);
    }
}

/// Two lines drawn out from where the background came: across under the title (200 wide at
/// most, 520 pixels in 80 ticks, leaving at the right), down beside the lines (100 high, 268
/// pixels in 80 ticks).
fn draw_lines(c: &mut Cpu, this: u32, page: u32, lines_x: u32) {
    let d = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + CAME_AT)) as i32;
    let mut x = d.wrapping_mul(520) / 80;
    let mut w = 200;
    if x < w {
        w = x;
        x = 200;
    }
    if x >= 321 {
        w = (320 - x).max(0);
        x = 520 - w;
    }
    let line = |c: &mut Cpu, x: u32, y: u32, w: u32, h: u32, mode: u32| {
        let color = fade_out(GREY, 64);
        felines::draw(c, Line { color, x, y, w, h, mode, fade: 1, glow: 0, trans: true });
    };
    if w > 0 {
        let y = lh(&mut c.m, page + SUBTITLE + Y).wrapping_sub(2);
        line(c, (x - 37) as u32, y, w as u32, 4, 3);
    }
    let d = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(this + CAME_AT)) as i32;
    let mut y = d.wrapping_mul(268) / 80;
    let mut h = 100;
    if y < h {
        h = y;
        y = 100;
    }
    if y >= 169 {
        h = (168 - y).max(0);
        // (the original's: less the horizontal line's width, not this one's height)
        y = 268 - w;
    }
    if h > 0 {
        line(c, lines_x, (y - 58) as u32, 2, h as u32, 1);
    }
}

pub fn abi_draw_page(c: &mut Cpu) {
    draw_page(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The credits (`CreditManager`).
    const CREDITS: u32 = 0x8011_6014;
    const PAGES_NAME: u32 = 0x8001_181C;

    /// Lines a page may have: plain, a cheat's (a tab), the thanks (`*`), the names (`^`),
    /// blank lines between; now and then one of each last (the last line sets the page's time).
    const LINES: [&[u8]; 9] = [b"Producer\nSomeone Else\n", b"\tA cheat's line\nplain\n", b"*\n", b"^\n", b"\n\nAfter blanks\n\t\tTwo tabs", b"One line", b"^", b"*", b"\tLast cheat"];
    /// The cheats on (FECHEATS's).
    const CHEATS_ON: u32 = 0x8013_E550;

    /// The main screen's (SCREENMAIN's), the pads' (FEINPUT's), the cheats on (the credits'
    /// cheat, another, any), then the credits: 1 to 5 pages
    /// in a block of the heap's (titles and subtitles on a line or wrapped, the lines' place
    /// given or not, any background or none, lines of each kind), loaded or not, any page shown
    /// and next, in any fade, asked, come and out or not, about each time limit.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::screenmain::tests::shuffle(c, rng);
        crate::feinput::tests::pads(c, rng);
        let n = 1 + pick(rng, 5);
        let pages = crate::abi::call(c, nfs4rc::f_800e5f28, &[PAGES_NAME, n * PAGE_SIZE, 0]);
        let m = &mut c.m;
        for k in 0..n {
            let p = pages + k * PAGE_SIZE;
            for f in (0..32).step_by(2) {
                m.w16(p + f, 0);
            }
            m.w16(p, pick(rng, 120) as u16);
            m.w16(p + 2, pick(rng, 120) as u16);
            for at in [4, 12] {
                m.w16(p + at, pick(rng, 400) as u16);
                m.w16(p + at + 2, pick(rng, 200) as u16);
                m.w16(p + at + 4, pick(rng, 3) as u16);
                m.w16(p + at + 6, [0, 0, 150, 280][pick(rng, 4) as usize] as u16);
            }
            m.w16(p + 20, [0, pick(rng, 400)][pick(rng, 2) as usize] as u16);
            m.w16(p + 22, [0, pick(rng, 200)][pick(rng, 2) as usize] as u16);
            m.w16(p + 24, pick(rng, 3) as u16);
            m.w16(p + 28, [0xFFFF, 0xFFFF, pick(rng, 28) as u16][pick(rng, 3) as usize]);
            let mut text = Vec::new();
            for _ in 0..1 + pick(rng, 3) {
                text.extend_from_slice(LINES[pick(rng, LINES.len() as u32) as usize]);
            }
            text.truncate(280);
            text.push(0);
            for (j, &b) in text.iter().enumerate() {
                m.w8(p + 32 + j as u32, b);
            }
        }
        let ticks = m.r32(GAME_TICKS);
        let ago = |rng: &mut Rng, near: &[u32]| ticks.wrapping_sub([near, &[0, 5000]].concat()[pick(rng, near.len() as u32 + 2) as usize]);
        let s = CREDITS;
        m.w32(s + PAGES, pages);
        m.w32(s + COUNT, n);
        m.w32(s + SHOWN, [0, 4, 88, 92, pick(rng, 93)][pick(rng, 5) as usize]);
        m.w32(s + FADE, [0, 8, 120, 128, pick(rng, 129)][pick(rng, 5) as usize]);
        m.w32(s + STEP, [8, (-8i32) as u32][pick(rng, 2) as usize]);
        m.w32(s + LOADED, (pick(rng, 4) != 0) as u32);
        m.w32(s + TO_FREE, pick(rng, 2));
        let page = if pick(rng, 4) == 0 { u32::MAX } else { pick(rng, n) };
        m.w32(s + PAGE, page);
        m.w32(s + NEXT, if pick(rng, 2) == 0 { page } else { pick(rng, n) });
        for f in [ASKED, CAME, OUT] {
            m.w32(s + f, pick(rng, 2));
        }
        let at = ago(rng, &[30, 31, 100]);
        m.w32(s + CAME_AT, at);
        let at = if pick(rng, 3) == 0 { 0 } else { ago(rng, &[700, 701, 2000, 2001]) };
        m.w32(s + SHOWN_AT, at);
        let at = ago(rng, &[12, 13]);
        m.w32(KEY_AT, at);
        m.w32(STAY, [700, 2000][pick(rng, 2) as usize]);
        m.w32(CHEATS_ON, [0, 1 << CHEAT, 1 << (CHEAT + 1), rng.next() as u32][pick(rng, 4) as usize]);
        let at = ago(rng, &[128, 129]);
        m.w32(ASKED_AT, at);
    }

    /// A page shown (`DrawPage`'s).
    fn shown(c: &mut Cpu, r: &mut Rng) {
        let n = c.m.r32(CREDITS + COUNT);
        c.m.w32(CREDITS + PAGE, pick(r, n));
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tCreditManager::Setup", 0x8003_5ca0, nfs4rc::f_80035ca0, |_, _| vec![CREDITS]),
            ("tCreditManager::Init", 0x8003_5cb0, nfs4rc::f_80035cb0, |_, r| vec![CREDITS, pick(r, 5000)]),
            ("tCreditManager::DeInit", 0x8003_5d90, nfs4rc::f_80035d90, |_, _| vec![CREDITS]),
            ("tCreditManager::RealDeInit", 0x8003_5d9c, nfs4rc::f_80035d9c, |_, _| vec![CREDITS]),
            ("tCreditManager::Draw", 0x8003_5ddc, nfs4rc::f_80035ddc, |_, r| vec![CREDITS, pick(r, 2)]),
            ("tCreditManager::Update", 0x8003_5f20, nfs4rc::f_80035f20, |_, _| vec![CREDITS]),
            ("tCreditManager::DrawPage", 0x8003_6278, nfs4rc::f_80036278, |c, r| {
                shown(c, r);
                vec![CREDITS]
            }),
        ]
    }

    #[test]
    fn fecredits_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xC4E ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xC4F + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
