//! FECREDITS.CPP: the credits (`tCreditManager`), over the main menu's TVs: their pages read
//! from the main screen's `zcred*.dat` (324 bytes each: a title and a subtitle, at a place or
//! word-wrapped, and lines of text), each faded in, shown 12 seconds (33 for a page of names)
//! or until a key, faded out; every third page and any page asking for one swaps the main
//! screen's background first; a picture spinning in the corner, two lines drawn out under the
//! titles.

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, STRLEN, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, PATHS};
use crate::{fecheats, fefades, feinput, felines, fetextrender, screenmain};
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

// A page: +0 its title's text (from 1301 on), +2 its subtitle's, +4 the title's place, +8 its
// justification, +10 its width (0: on one line), +12..+18 the subtitle's, +20 the lines' x (0:
// beside the subtitle), +22 their y (0: under it), +24 their justification, +28 the background
// it asks for (-1: none), +32 its lines (`\n` between; a tab marks a cheat's line, `*` the
// thanks, `^` the names).
const PAGE_SIZE: u32 = 324;

/// The last key's tick, how long a page stays, when the background was asked for.
const KEY_AT: u32 = 0x8005_1F2C;
const STAY: u32 = 0x8005_1F28;
const ASKED_AT: u32 = 0x8005_1F30;
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
/// The cheat whose lines show.
const CHEAT: u32 = 12;
/// The key that turns a page (the start key's), its sound.
const KEY: u32 = 32;
const TURN_SFX: u32 = 6;
/// The TVs' flags and colour (the main screen's +124 on, 48 bytes each).
const TVS: u32 = 124;

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

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `tCreditManager::Setup` (FECREDITS.CPP, 0x80035ca0): nothing loaded, nothing shown.
pub fn abi_setup(c: &mut Cpu) {
    let this = c.r[4];
    c.m.w32(this + LOADED, 0);
    c.m.w32(this + TO_FREE, 0);
    c.m.w32(this + SHOWN, 0);
}

/// `tCreditManager::Init` (FECREDITS.CPP, 0x80035cb0): unless loaded, the main screen's pages
/// read, nothing shown yet (the page's fade full, no page).
pub fn abi_init(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        if c.m.r32(this + LOADED) != 0 {
            return;
        }
        if c.m.r32(this + TO_FREE) != 0 {
            c.m.w32(this + TO_FREE, 0);
        }
        let m = &mut c.m;
        m.w32(this + PAGE, u32::MAX);
        m.w32(this + FADE, 128);
        for f in [SHOWN_AT, ASKED, CAME, OUT, TO_FREE, NEXT] {
            m.w32(this + f, 0);
        }
        let (path, n) = (m.r32(CREDITS_PATH), m.r8(MAIN_NUMBER) as u32);
        let file = sp + 16;
        r.call_leaving(c, SPRINTF, &[file, FILE_FORMAT, path, n], 0x8003_5d2c);
        let size = r.call_leaving(c, FILE_SIZE, &[file], 0x8003_5d34);
        let pages = r.call_leaving(c, RESERVE, &[RECORDS, size, 0], 0x8003_5d48);
        c.m.w32(this + PAGES, pages);
        r.call_leaving(c, LOAD_FILE, &[file, pages], 0x8003_5d58);
        let size = r.call_leaving(c, FILE_SIZE, &[file], 0x8003_5d60);
        c.m.w32(this + LOADED, 1);
        c.m.w32(this + COUNT, size / PAGE_SIZE);
    })
}

/// `tCreditManager::DeInit` (FECREDITS.CPP, 0x80035d90): to be freed once faded out.
pub fn abi_de_init(c: &mut Cpu) {
    c.m.w32(c.r[4] + TO_FREE, 1);
}

/// `tCreditManager::RealDeInit` (FECREDITS.CPP, 0x80035d9c): the pages freed.
pub fn real_de_init(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w32(this + TO_FREE, 0);
        if c.m.r32(this + LOADED) != 0 {
            let pages = c.m.r32(this + PAGES);
            r.call_leaving(c, PURGE, &[pages], 0x8003_5dc8);
            c.m.w32(this + LOADED, 0);
        }
    })
}

pub fn abi_real_de_init(c: &mut Cpu) {
    real_de_init(c, c.r[4]);
}

/// `tCreditManager::Draw` (FECREDITS.CPP, 0x80035ddc): the credits faded in (`on`) or out, the
/// main screen's TVs darkened as much (the page's fade held up meanwhile); once faded out the
/// pages freed if asked; else the pages turned and drawn.
pub fn abi_draw(c: &mut Cpu) {
    let (this, on) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let step = if on != 0 { 4 } else { (-4i32) as u32 };
        let v = m.r32(this + SHOWN).wrapping_add(step);
        m.w32(this + SHOWN, v);
        let v = (m.r32(this + SHOWN) as i32).clamp(0, 92);
        m.w32(this + SHOWN, v as u32);
        if v < 92 {
            let fade = (128 - v).max(m.r32(this + FADE) as i32);
            m.w32(this + FADE, fade as u32);
        }
        let screen = m.r32(MAIN_SCREEN);
        for k in 0..16 {
            let tv = screen.wrapping_add(TVS + 48 * k);
            let flags = m.r16(tv + 8) | 2;
            m.w16(tv + 8, flags);
            let f = 128u32.wrapping_sub(m.r32(this + SHOWN));
            m.w32(tv + 16, (f << 16) | (f << 8) | f);
        }
        // (the loop's registers, left for the callees: the last colour, the TVs' end, the
        // count, 128)
        let f = 128u32.wrapping_sub(m.r32(this + SHOWN));
        c.r[4..8].copy_from_slice(&[f, screen.wrapping_add(TVS + 48 * 16), 16, 128]);
        if c.m.r32(this + SHOWN) == 0 {
            if c.m.r32(this + TO_FREE) != 0 {
                r.enter_leaving(c, 0x8003_5d9c, 0x8003_5ed8, |c| real_de_init(c, this));
            }
        } else if c.m.r32(this + LOADED) != 0 {
            r.enter_leaving(c, 0x8003_5f20, 0x8003_5ef8, |c| update(c, this));
            if c.m.r32(this + PAGE) != u32::MAX {
                r.enter_leaving(c, 0x8003_6278, 0x8003_5f10, |c| draw_page(c, this));
            }
        }
    })
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

/// The credits' turning (FECREDITS.CPP, 0x80035f20): a key on either pad (a fifth of a second
/// after the last) turns a page shown in full, or the page's time running out does; a new page
/// fades out the one shown, then in; once in, its background asked for (every third page's, or
/// the one it asks for); once the background is up, two seconds on, the page fades out.
pub fn update(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(KEY_AT)) as i32;
        // (s1 cleared on the way to the page's check)
        if since >= 13 && m.r32(this + FADE) == 0 {
            r.0[1] = 0;
        }
        if since >= 13 && m.r32(this + FADE) == 0 && m.r32(this + NEXT) == m.r32(this + PAGE) {
            let mut key = port(c, &r, 0x8002_3918, 0x8003_5f80, &[KEY, 0], feinput::abi_get_no_debounce_key) != 0;
            if !key {
                key = port(c, &r, 0x8002_3918, 0x8003_5f90, &[KEY, 1], feinput::abi_get_no_debounce_key) != 0;
            }
            if key {
                r.0[1] = 1;
                r.call_leaving(c, PLAY_SFX, &[TURN_SFX], 0x8003_5fac);
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
            m.w32(this + STEP, 8);
            for f in [ASKED, CAME, OUT] {
                m.w32(this + f, 0);
            }
        }
        let v = m.r32(this + FADE).wrapping_add(m.r32(this + STEP));
        m.w32(this + FADE, v);
        if v as i32 <= 0 {
            m.w32(this + FADE, 0);
        }
        if m.r32(this + FADE) as i32 >= 128 {
            m.w32(this + FADE, 128);
        }
        if m.r32(this + FADE) == 128 && m.r32(this + ASKED) == 0 {
            let page = m.r32(this + NEXT);
            m.w32(this + ASKED, 1);
            m.w32(this + PAGE, page);
            let mut next = page.wrapping_add(1);
            if next as i32 >= m.r32(this + COUNT) as i32 {
                next = 0;
            }
            let back = m.r32(this + PAGES).wrapping_add(next.wrapping_mul(PAGE_SIZE)) + 28;
            if (page as i32).wrapping_rem(3) != 0 && lh(m, back) == u32::MAX {
                m.w32(ASKED_AT, 0);
            } else {
                let (screen, which) = (m.r32(MAIN_SCREEN), lh(m, back));
                port(c, &r, 0x8003_7118, 0x8003_6190, &[screen, which], screenmain::abi_swap_background);
                let ticks = c.m.r32(GAME_TICKS);
                c.m.w32(ASKED_AT, ticks);
            }
        }
        let m = &mut c.m;
        let mut came = false;
        if m.r32(this + CAME) == 0 && m.r32(this + ASKED) != 0 && m.r32(GAME_TICKS).wrapping_sub(m.r32(ASKED_AT)) as i32 >= 129 {
            let screen = m.r32(MAIN_SCREEN);
            came = port(c, &r, 0x8003_72d4, 0x8003_61f4, &[screen], screenmain::abi_done_loading_background) != 0;
        }
        let m = &mut c.m;
        if came {
            let ticks = m.r32(GAME_TICKS);
            m.w32(this + CAME, 1);
            m.w32(this + CAME_AT, ticks);
        }
        if m.r32(this + OUT) == 0 && m.r32(this + CAME) != 0 && m.r32(GAME_TICKS).wrapping_sub(m.r32(this + CAME_AT)) as i32 >= 31 {
            let ticks = m.r32(GAME_TICKS);
            m.w32(this + OUT, 1);
            m.w32(this + STEP, (-8i32) as u32);
            m.w32(this + SHOWN_AT, ticks);
        }
    })
}

pub fn abi_update(c: &mut Cpu) {
    update(c, c.r[4]);
}

/// The credits' page (FECREDITS.CPP, 0x80036278): the spinning picture; the title and the
/// subtitle (on a line, or word-wrapped), the lines under or beside them (the thanks; the 25
/// names; a cheat's lines while it is on), the page's time set by them; once its background has
/// come, two lines drawn out under the titles.
pub fn draw_page(c: &mut Cpu, this: u32) {
    abi::frame(c, 416, |c, sp| {
        let mut r = Saved::of(c);
        c.m.w32(sp + 416, this);
        let ticks = c.m.r32(GAME_TICKS) as i32;
        let spin = sp + 40;
        c.m.w32(spin + 4, 0x00CE_C844);
        let shape = (230 + (ticks >> 4) % 10) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 1040, 16, 16, 0, 0, spin], 0x8003_6310);
        let page = {
            let m = &mut c.m;
            m.r32(this + PAGES).wrapping_add(m.r32(this + PAGE).wrapping_mul(PAGE_SIZE))
        };
        r.0[1] = page;
        r.enter_leaving(c, 0x8002_1b48, 0x8003_6340, |c| fetextrender::set_abr(c, 1, 1));
        r.0[3] = c.m.r16(page + 14) as u32;
        let fade = |c: &mut Cpu, r: &Saved, color: u32, t: u32, ra: u32| port(c, r, 0x8004_bd68, ra, &[color, t], fefades::abi_fade_out);
        let f = c.m.r32(this + FADE);
        let v = fade(c, &r, 0xBEBE, f, 0x8003_6358);
        let title = fade(c, &r, v, 40, 0x8003_6364);
        r.0[0] = title;
        let f = c.m.r32(this + FADE);
        let v = fade(c, &r, 0xBEBE, f, 0x8003_6378);
        let subtitle = fade(c, &r, v, 40, 0x8003_6384);
        r.0[2] = subtitle;
        let f = c.m.r32(this + FADE);
        let lines = fade(c, &r, 0x0078_7878, f, 0x8003_639c);
        r.0[8] = lines;
        let rect = sp + 64;
        let word = |c: &mut Cpu, r: &Saved, id: u32, ra: u32| r.call_leaving(c, TEXT_WORD, &[id], ra);
        // the title, the subtitle
        for (text, at, color, ras) in [(0, 4, title, [0x8003_63e0, 0x8003_63fc, 0x8003_6410, 0x8003_6430]), (2, 12, subtitle, [0x8003_6474, 0x8003_6490, 0x8003_64a8, 0x8003_64c8])] {
            let m = &mut c.m;
            if lh(m, page + at + 6) != 0 {
                for (k, f) in [0, 2, 6].into_iter().enumerate() {
                    let v = m.r16(page + at + f);
                    m.w16(rect + 2 * k as u32, v);
                }
                m.w16(rect + 6, 100);
                let id = lh(m, page + text).wrapping_add(PAGE_TEXT);
                let w = word(c, &r, id, ras[0]);
                let justify = lh(&mut c.m, page + at + 4);
                let h = r.call_leaving(c, fetextrender::word_wrap_text_rgb_justify_entry, &[w, rect, color, justify, 0, 0], ras[1]);
                if text == 2 {
                    r.0[3] = r.0[3].wrapping_add(h);
                }
            } else {
                if text == 2 {
                    r.0[3] = r.0[3].wrapping_add(8);
                }
                let id = lh(m, page + text).wrapping_add(PAGE_TEXT);
                let w = word(c, &r, id, ras[2]);
                let m = &mut c.m;
                let (x, y, justify) = (lh(m, page + at), lh(m, page + at + 2), lh(m, page + at + 4));
                r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, x, y, color, 0, justify], ras[3]);
            }
        }
        let m = &mut c.m;
        if lh(m, page + 22) != 0 {
            r.0[3] = m.r16(page + 22) as u32;
        }
        let x = m.r16(page + 20);
        r.0[0] = lh(m, page + 18);
        m.w16(sp + 368, x);
        if r.0[0] == 0 {
            port(c, &r, 0x8002_0e38, 0x8003_64f8, &[0], fetextrender::abi_set_font);
            let id = lh(&mut c.m, page + 2).wrapping_add(PAGE_TEXT);
            let w = word(c, &r, id, 0x8003_6504);
            let px = r.call_leaving(c, TEXT_PIXELS, &[w], 0x8003_650c);
            r.0[0] = px;
            let w = word(c, &r, id, 0x8003_651c);
            let n = r.call_leaving(c, STRLEN, &[w], 0x8003_6524);
            r.0[0] = px.wrapping_sub(n);
        }
        let m = &mut c.m;
        if m.r16(sp + 368) == 0 {
            let x = m.r16(page + 12) as u32;
            let x = if lh(m, page + 16) == 0 { x.wrapping_add(r.0[0]).wrapping_add(2) } else { x.wrapping_sub(r.0[0]).wrapping_sub(2) };
            m.w16(sp + 368, x as u16);
        }
        // the lines
        let text = sp + 72;
        r.0[0] = text;
        r.call_leaving(c, STRCPY, &[text, page + 32], 0x8003_6584);
        r.0[7] = (c.m.r16(sp + 368) as u32) << 16;
        let mut at = text;
        while at != 0 {
            r.0[6] = 0;
            r.0[5] = 0;
            r.0[2] = 0;
            r.0[0] = at;
            let end = r.call_leaving(c, STRCHR, &[at, 10], 0x8003_65ac);
            r.0[4] = end;
            if end != 0 {
                c.m.w8(end, 0);
            }
            let mut skip = |m: &mut Mem, ch: u8| {
                let found = m.r8(at) == ch;
                while m.r8(at) == ch {
                    at = at.wrapping_add(1);
                }
                found
            };
            let m = &mut c.m;
            skip(m, b'\n');
            if skip(m, b'\t') {
                r.0[6] = 1;
            }
            if skip(m, b'*') {
                r.0[5] = 1;
            }
            if skip(m, b'^') {
                r.0[2] = 1;
            }
            r.0[0] = at;
            m.w32(STAY, if r.0[2] != 0 { 2000 } else { 700 });
            let (x, y, justify) = (((r.0[7] as i32) >> 16) as u32, sx(r.0[3]), lh(m, page + 24));
            if r.0[5] != 0 {
                for (k, v) in [180u16, 85, 280, 100].into_iter().enumerate() {
                    m.w16(rect + 2 * k as u32, v);
                }
                let w = word(c, &r, THANKS, 0x8003_66b4);
                r.call_leaving(c, fetextrender::word_wrap_text_rgb_justify_entry, &[w, rect, lines, 0, 0, 0], 0x8003_66d0);
                r.0[3] = r.0[3].wrapping_add(8);
            } else if r.0[2] != 0 {
                for k in 0..25 {
                    r.0[0] = k;
                    let w = word(c, &r, NAMES + k, 0x8003_66ec);
                    let y = sx(r.0[3]);
                    r.0[3] = r.0[3].wrapping_add(8);
                    r.0[0] = k + 1;
                    let justify = lh(&mut c.m, page + 24);
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, x, y, lines, 0, justify], 0x8003_6718);
                }
                r.0[3] = r.0[3].wrapping_add(8);
            } else {
                if r.0[6] == 0 {
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[at, x, y, lines, 0, justify], 0x8003_67a0);
                } else if port(c, &r, 0x8002_373c, 0x8003_675c, &[CHEAT], fecheats::abi_is_cheat_enabled) != 0 {
                    let f = c.m.r32(this + FADE);
                    let color = fade(c, &r, 0x0050_5050, f, 0x8003_6778);
                    let justify = lh(&mut c.m, page + 24);
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[at, x, y, color, 0, justify], 0x8003_67a0);
                }
                r.0[3] = r.0[3].wrapping_add(8);
            }
            at = if end == 0 { 0 } else { end + 1 };
        }
        r.0[6] = 0;
        r.0[0] = 0;
        r.enter_leaving(c, 0x8002_1b48, 0x8003_67c0, |c| fetextrender::set_abr(c, 0, 0));
        if c.m.r32(this + CAME) == 0 {
            return;
        }
        // the lines drawn out
        let line = |c: &mut Cpu, r: &Saved, args: [u32; 9], ra: u32| {
            for (k, &v) in args.iter().enumerate().skip(4) {
                c.m.w32(sp + 4 * k as u32, v);
            }
            r.enter_leaving(c, 0x8004_c058, ra, |c| felines::draw(c, &args));
        };
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
        r.0[0] = x as u32;
        r.0[2] = w as u32;
        if w > 0 {
            let color = fade(c, &r, 0x0050_5050, 64, 0x8003_6858);
            let y = lh(&mut c.m, page + 14).wrapping_sub(2);
            line(c, &r, [color, (x - 37) as u32, y, w as u32, 4, 3, 1, 0, 1], 0x8003_6890);
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
        r.0[0] = y as u32;
        r.0[1] = h as u32;
        if h > 0 {
            let color = fade(c, &r, 0x0050_5050, 64, 0x8003_6920);
            let x = lh(&mut c.m, sp + 368);
            line(c, &r, [color, x, (y - 58) as u32, 2, h as u32, 1, 1, 0, 1], 0x8003_6954);
        }
    })
}

pub fn abi_draw_page(c: &mut Cpu) {
    draw_page(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn fecredits_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xC5F + k as u64, shuffle, &args);
        }
    }
}
