//! FEDIALOG.CPP: the front end's dialogs: a stack of up to eight shown at once over the
//! screen (the top one first), each a box that grows open over 50 ticks and fades its text in;
//! the help box (the keys the current menu takes, with their pads' buttons, typed out letter
//! by letter), message boxes (a text word-wrapped or on one line), those that take only Start
//! or no input, and the interactive ones (`Run`: the front end's frames go on while one waits
//! for its answer): yes or no, the memory card's (closed when the card is taken out) and the
//! one Start answers too.
//!
//! The dialogs are screens (`tScreen`, its vptr at +96); their `ProcessInput`,
//! `CalculateDimensions` and `Draw` are virtual, called with the registers the original has at
//! its `jalr`.

use crate::abi::{self, Saved};
use crate::femenu::{KEY_ACCEPT, KEY_BACK, KEY_HELP, KEY_LEFT, KEY_NONE, KEY_RIGHT, KEY_START, PLAY_FE_SFX, PROCESS_INPUT, method};
use crate::femenuextended::{DRAW_TRANS_SQUARE, MENU_VPTR, SET_ABR_MODE, STRLEN, TEXT_PIXELS, TEXT_WORD};
use crate::fetextrender;
use crate::globals::{FE_APP, GAME_TICKS, PAD_INFO, SCREEN_WIDTH, SHAPES, TEXT_COLORS, TEXT_STYLES};
use ps1rt::{Cpu, Mem};

/// The dialogs shown, the top one first (`tDialogBase::fDialogs[8]`).
const DIALOGS: u32 = 0x8005_2FC8;
const MAX_DIALOGS: u32 = 8;

/// The vtables the yes-or-no dialog's constructor goes through: the base's, a message's, an
/// interactive one's, its own.
const BASE_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const INTERACTIVE_VT: u32 = 0x8001_03C8;
const YES_NO_VT: u32 = 0x8001_0368;

/// A dialog's vptr (a screen's) and its virtual `ProcessInput`, `CalculateDimensions` and
/// `Draw`.
const VPTR: u32 = 96;
const PROCESS_INPUT_SLOT: u32 = 72;
const CALCULATE_DIMENSIONS: u32 = 80;
const DRAW: u32 = 88;

// A dialog: +100 the player it listens to (-1 either), +102 x, +104 y, +106 its width, +108
// its height, +110 the room under its text (the yes-or-no's answers), +112 shown, +116 the tick
// it was shown at, +120 its timeout (ticks; 0 none), +124 and +126 its place off the centre,
// +128 a message's width, +130 its height (0: as its text takes), +132 open (grown to its
// size), +136 the answer it starts on, +138 the answer, +140 the text's fade, +144 a message's
// text (the help's page), +148 a message on one line (the help's texts, seven), +152 answered,
// +156 running, +160 the yes-or-no's two texts; the help's +176 buttons (seven), +204 how many
// lines, +206 the pads' kinds (1 a pad, 2 a NeGcon), +208 the texts' place right of the
// buttons.
const PLAYER: u32 = 100;
const X: u32 = 102;
const Y: u32 = 104;
const W: u32 = 106;
const H: u32 = 108;
const SHOWN: u32 = 112;
const SHOWN_AT: u32 = 116;
const TIMEOUT: u32 = 120;
const OPEN: u32 = 132;
const ANSWER: u32 = 138;
const FADE: u32 = 140;
const TEXT: u32 = 144;
const ONE_LINE: u32 = 148;
const ANSWERED: u32 = 152;
const HELP_TEXTS: u32 = 148;
const HELP_BUTTONS: u32 = 176;
const HELP_LINES: u32 = 204;
const HELP_PADS: u32 = 206;
const HELP_TEXT_X: u32 = 208;
const HELP_MAX_LINES: u32 = 7;

/// How long a dialog takes to open (ticks).
const OPENING: i32 = 50;

/// `FeTools_DrawPSXButton`; `PAD_update`; `blockmove`; `CalcFadeVal`; the front end's key for a
/// player (FEINPUT.CPP); a frame of the front end (`tFEApplication`'s, 0x80013894); the
/// memory card's events and its state (`MCRD_handlecardevents`, `MCRD_getcard`); the screen's
/// constructor and destructor (`tScreen`).
const DRAW_PSX_BUTTON: fn(&mut Cpu) = nfs4rc::f_800269d0;
const PAD_UPDATE: fn(&mut Cpu) = nfs4rc::f_800e4e08;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const CALC_FADE_VAL: fn(&mut Cpu) = nfs4rc::f_8004bd68;
const GET_KEY: fn(&mut Cpu) = nfs4rc::f_80023ca8;
/// `tFEApplication::Redraw`, hooked (the host autosaves between front-end frames): called by
/// address; `Cpu::call` routes it to the port after the host.
const FRONT_END_FRAME: u32 = 0x8001_3894;
const CARD_EVENTS: fn(&mut Cpu) = crate::memcard::handlecardevents_entry;
const GET_CARD: fn(&mut Cpu) = crate::memcard::getcard_entry;
const SCREEN: fn(&mut Cpu) = nfs4rc::f_80026040;
const SCREEN_DESTRUCTOR: fn(&mut Cpu) = nfs4rc::f_80026098;

/// The help's pages (one, its flag the menu's keys: no texts of its own), copied to the stack.
const HELP_PAGES: u32 = 0x8001_023C;
const HELP_PAGE_SIZE: u32 = 18;
/// The pads' buttons drawn for the keys (the PlayStation's, the NeGcon's).
const PAD_BUTTONS: u32 = 65;
const NEGCON_BUTTONS: u32 = 35;
const PAD_NEGCON: u32 = 0x23;
/// The memory card in use (`gMemCardPort`); its events' jump table (those that close the
/// dialog jump to 0x80019f50).
const CARD_PORT: u32 = 0x8005_1EF0;
const CARD_EVENT_TABLE: u32 = 0x8001_0250;
const CARD_EVENT_CLOSES: u32 = 0x8001_9F50;
/// The help dialog the front end keeps (`gFEApp + 344`), its menus (per player, +4) and their
/// pages of options (+324); set while the front end takes no input (+456).
const APP_HELP: u32 = 344;
const APP_MENUS: u32 = 4;
const APP_OPTIONS: u32 = 324;
const APP_NO_INPUT: u32 = 456;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// `tDialogBase::ShouldTimeOut` (FEDIALOG.CPP, 0x80018808): whether a dialog with a timeout has
/// been shown that long.
pub fn should_time_out(m: &mut Mem, this: u32) -> u32 {
    let timeout = m.r32(this + TIMEOUT) as i32;
    if timeout <= 0 {
        return 0;
    }
    let shown = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWN_AT)) as i32;
    (shown >= timeout) as u32
}

pub fn abi_should_time_out(c: &mut Cpu) {
    c.r[2] = should_time_out(&mut c.m, c.r[4]);
}

/// `tDialogBase::InitializeClass` (FEDIALOG.CPP, 0x80018840): no dialogs shown.
pub fn abi_initialize_class(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| hide_all_dialogs(&mut c.m));
}

/// `tDialogBase::DrawAllDialogs` (FEDIALOG.CPP, 0x80018860): each dialog shown, the top one
/// first, drawn; one that has timed out hidden instead (the rest move up, and the one moved
/// into its place is drawn).
pub fn abi_draw_all_dialogs(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = 0;
        if c.m.r32(DIALOGS) == 0 {
            return;
        }
        r.0[2] = DIALOGS;
        loop {
            let k = sx(r.0[1]);
            if k as i32 >= MAX_DIALOGS as i32 {
                return;
            }
            let at = DIALOGS.wrapping_add(k << 2);
            r.0[0] = at;
            let dialog = c.m.r32(at);
            if should_time_out(&mut c.m, dialog) != 0 {
                r.call_ptr(c, 0x8001_8a44, &[dialog], 0x8001_88c4);
                if c.m.r32(at) == 0 {
                    return;
                }
            }
            let dialog = c.m.r32(at);
            let (obj, f) = method(&mut c.m, dialog, VPTR, DRAW);
            r.call_ptr_leaving(c, f, &[obj], 0x8001_88f8);
            let next = r.0[1].wrapping_add(1);
            let more = c.m.r32(DIALOGS.wrapping_add(sx(next) << 2)) != 0;
            r.0[1] = next;
            if !more {
                return;
            }
        }
    })
}

/// `tDialogBase::HideAllDialogs` (FEDIALOG.CPP, 0x80018930).
pub fn hide_all_dialogs(m: &mut Mem) {
    for k in 0..MAX_DIALOGS {
        m.w32(DIALOGS + 4 * k, 0);
    }
}

pub fn abi_hide_all_dialogs(c: &mut Cpu) {
    hide_all_dialogs(&mut c.m);
}

/// `tDialogBase::GetTopMostDialog` (FEDIALOG.CPP, 0x80018970).
pub fn abi_get_top_most_dialog(c: &mut Cpu) {
    c.r[2] = c.m.r32(DIALOGS);
}

/// `tDialogBase::Display` (FEDIALOG.CPP, 0x80018980): a dialog not shown yet put on top (the
/// others moved down, the last dropped), shown from now on its first answer, closed, its text
/// unfaded, with a sound.
pub fn display(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        if m.r32(this + SHOWN) != 0 {
            return;
        }
        let first = m.r16(this + 136);
        m.w32(this + SHOWN, 1);
        m.w32(this + OPEN, 0);
        m.w16(this + ANSWER, first);
        for k in (1..MAX_DIALOGS).rev() {
            let v = m.r32(DIALOGS + 4 * (k - 1));
            m.w32(DIALOGS + 4 * k, v);
        }
        r.0[0] = 0x8005_0000;
        m.w32(DIALOGS, this);
        should_time_out(m, this);
        let top = m.r32(DIALOGS);
        should_time_out(m, top);
        let ticks = m.r32(GAME_TICKS);
        m.w32(this + SHOWN_AT, ticks);
        r.call_leaving(c, PLAY_FE_SFX, &[15], 0x8001_8a24);
        let m = &mut c.m;
        m.w32(this + OPEN, 0);
        m.w32(this + FADE, 128);
    })
}

pub fn abi_display(c: &mut Cpu) {
    display(c, c.r[4]);
}

/// `tDialogBase::Hide` (FEDIALOG.CPP, 0x80018a44): a dialog shown taken off the stack (those
/// under it moved up), with a sound.
pub fn hide(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        if c.m.r32(this + SHOWN) == 0 {
            return;
        }
        r.0[0] = 0;
        c.m.w32(this + SHOWN, 0);
        r.0[1] = DIALOGS;
        loop {
            let k = sx(r.0[0]);
            let at = DIALOGS.wrapping_add(k << 2);
            if c.m.r32(at) == this {
                let n = 7u32.wrapping_sub(k) << 2;
                c.m.w32(this + SHOWN, 0);
                c.m.w32(at, 0);
                r.call_leaving(c, BLOCKMOVE, &[at.wrapping_add(4), at, n], 0x8001_8ab8);
                c.m.w32(DIALOGS + 4 * (MAX_DIALOGS - 1), 0);
                r.call_leaving(c, PLAY_FE_SFX, &[18], 0x8001_8ac4);
            }
            r.0[0] = r.0[0].wrapping_add(1);
            if sx(r.0[0]) as i32 >= MAX_DIALOGS as i32 {
                return;
            }
        }
    })
}

pub fn abi_hide(c: &mut Cpu) {
    hide(c, c.r[4]);
}

/// `tDialogBase::Draw` (FEDIALOG.CPP, 0x80018af8): the box's frame (four edges, the sides
/// 4 wide, the top and bottom 2 high) and its shaded inside.
pub fn draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[2] = 4;
        let m = &mut c.m;
        let (x, y, w, h) = (lh(m, this + X), lh(m, this + Y), lh(m, this + W), lh(m, this + H));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x, y, 4, h, 4], 0x8001_8b34);
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(w).wrapping_sub(4), y, 4, h, 4], 0x8001_8b60);
        r.0[1] = 2;
        let m = &mut c.m;
        let (x, y, w) = (lh(m, this + X), lh(m, this + Y), lh(m, this + W));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y, w.wrapping_sub(8), 2, 4], 0x8001_8b88);
        let m = &mut c.m;
        let (x, y, h, w) = (lh(m, this + X), lh(m, this + Y), lh(m, this + H), lh(m, this + W));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y.wrapping_add(h).wrapping_sub(2), w.wrapping_sub(8), 2, 4], 0x8001_8bb8);
        let m = &mut c.m;
        let (x, y, w, h) = (lh(m, this + X), lh(m, this + Y), lh(m, this + W), lh(m, this + H));
        let args = [0, x.wrapping_add(4), y.wrapping_add(2), w.wrapping_sub(8), h.wrapping_sub(4), 2];
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &args, 0x8001_8be8);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8001_8bf0);
    })
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, c.r[4]);
}

/// `tDialogBase::ProcessInput` (FEDIALOG.CPP, 0x80018c08): any key hides the dialog (and is
/// taken).
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = key;
        r.0[0] = 1;
        if c.m.r32(key) != KEY_NONE {
            r.call_ptr(c, 0x8001_8a44, &[this], 0x8001_8c34);
            c.m.w32(key, KEY_NONE);
        }
    })
}

/// `tDialogHelp::AddItem` (FEDIALOG.CPP, 0x80018c4c): a line (a text and the pads' button for
/// it), while there is room for one.
pub fn add_item(c: &mut Cpu, this: u32, text: u32, button: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = button;
        if lh(&mut c.m, this + HELP_LINES) as i32 >= HELP_MAX_LINES as i32 {
            return;
        }
        let word = r.call_leaving(c, TEXT_WORD, &[sx(text)], 0x8001_8c80);
        let m = &mut c.m;
        let k = lh(m, this + HELP_LINES);
        m.w32(this.wrapping_add(k << 2).wrapping_add(HELP_TEXTS), word);
        let k = lh(m, this + HELP_LINES);
        m.w32(this.wrapping_add(k << 2).wrapping_add(HELP_BUTTONS), sx(button));
        let n = m.r16(this + HELP_LINES).wrapping_add(1);
        m.w16(this + HELP_LINES, n);
    })
}

pub fn abi_add_item(c: &mut Cpu) {
    add_item(c, c.r[4], c.r[5], c.r[6]);
}

/// The ABI entry of [`add_item`] for its callers here (its prologue's stores left).
fn enter_add_item(c: &mut Cpu, r: &Saved, this: u32, text: u32, button: u32, ra: u32) {
    r.enter(c, 0x8001_8c4c, ra, |c| add_item(c, this, text, button));
}

/// Where the box opening over its first ticks is now: its size (at `at`) grown from twice the
/// starting size `from` (`elapsed` ticks of 50; `signed` the division the original does: the
/// help's unsigned, a message's signed).
fn opening(m: &mut Mem, this: u32, at: u32, from: u32, elapsed: u32, signed: bool) -> u32 {
    let from = from as i16 as i32;
    let start = ((from << 1) as u16 as i16) as i32;
    let grown = (lh(m, this + at) as i32).wrapping_sub(start).wrapping_mul(elapsed as i32);
    let step = if signed { grown / OPENING } else { ((grown as u32 as u64 * 0x51EB_851F) >> 36) as i32 };
    (from << 1).wrapping_add(step) as u32
}

/// `tDialogHelp::CalculateDimensions` (FEDIALOG.CPP, 0x80018cd0): the help's lines: its title,
/// then the keys the player's current menu takes (up and down past two items or more, left and
/// right on its choices, accept on its other items, the options, the pages, start, back), its
/// page's own; the pads' kinds; a box as wide as the widest text past the buttons (as wide as
/// the help's title shape at least), 15 high a line, at the top right, opening over its first
/// ticks.
pub fn calculate_dimensions_help(c: &mut Cpu, this: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        let pages = sp + 16;
        for k in 0..HELP_PAGE_SIZE {
            let b = c.m.r8(HELP_PAGES + k);
            c.m.w8(pages + k, b);
        }
        r.call_leaving(c, fetextrender::set_font_entry, &[0], 0x8001_8d4c);
        c.m.w16(this + HELP_LINES, 0);
        enter_add_item(c, &r, this, 90, 0, 0x8001_8d60);
        r.0[5] = 0;
        let player = lh(&mut c.m, this + PLAYER);
        r.0[0] = pages;
        if player != u32::MAX {
            r.0[5] = player;
        }
        let page = lh(&mut c.m, this + TEXT);
        let entry = pages.wrapping_add(page.wrapping_mul(HELP_PAGE_SIZE));
        r.0[2] = 0;
        if c.m.r8(entry) != 0 {
            r.0[4] = 0;
            let app = c.m.r32(FE_APP);
            let menu = c.m.r32(app.wrapping_add(r.0[5] << 2).wrapping_add(APP_MENUS));
            r.0[0] = menu;
            r.0[1] = 0;
            loop {
                let item = c.m.r32(menu.wrapping_add(sx(r.0[1]) << 2).wrapping_add(16));
                if item == 0 {
                    break;
                }
                let flags = c.m.r32(item);
                if flags & 1 == 0 {
                    if flags & 0x400 != 0 {
                        r.0[2] = 1;
                    } else {
                        r.0[4] = 1;
                    }
                }
                r.0[1] = r.0[1].wrapping_add(1);
            }
            let n = r.enter(c, 0x8002_58b4, 0x8001_8e18, |c| crate::femenu::get_number_enabled_items(&mut c.m, crate::types::Menu(menu)));
            if sx(n) as i32 >= 2 {
                enter_add_item(c, &r, this, 83, 80, 0x8001_8e38);
            }
            if r.0[2] != 0 {
                enter_add_item(c, &r, this, 84, 160, 0x8001_8e4c);
            }
            if r.0[4] != 0 {
                let flags = c.m.r32(menu);
                let text = if flags & 0x3_0000 != 0 { 87 } else { 86 };
                enter_add_item(c, &r, this, text, 0x4000, 0x8001_8e98);
            }
            let app = c.m.r32(FE_APP);
            if c.m.r32(app.wrapping_add(r.0[5] << 2).wrapping_add(APP_OPTIONS)) as i32 > 0 {
                enter_add_item(c, &r, this, 85, 0x1000, 0x8001_8ec4);
            }
            if c.m.r32(menu + 92) != 0 {
                enter_add_item(c, &r, this, 88, 0xFFFF_8000, 0x8001_8ee0);
            }
            let flags = c.m.r32(menu);
            if flags & 0x800 != 0 {
                enter_add_item(c, &r, this, 89, 8, 0x8001_8f54);
            } else {
                let m = &mut c.m;
                if m.r32(menu + 84) != 0 || flags & 0x400 != 0 || m.r32(menu + 96) != 0 || m.r32(menu) & 4 != 0 {
                    enter_add_item(c, &r, this, 87, 8, 0x8001_8f54);
                }
            }
        }
        // the page's own lines
        r.0[2] = pages;
        let page = lh(&mut c.m, this + TEXT);
        let first = lh(&mut c.m, pages.wrapping_add(page.wrapping_mul(HELP_PAGE_SIZE)).wrapping_add(2));
        r.0[1] = 0;
        if first != 0 {
            loop {
                let page = sx(c.m.r16(this + TEXT) as u32);
                let at = pages.wrapping_add(sx(r.0[1]) << 2).wrapping_add(page.wrapping_mul(HELP_PAGE_SIZE));
                let (text, button) = (lh(&mut c.m, at + 2), lh(&mut c.m, at + 4));
                r.0[0] = pages;
                enter_add_item(c, &r, this, text, button, 0x8001_8fb4);
                r.0[1] = r.0[1].wrapping_add(1);
                let page = lh(&mut c.m, this + TEXT);
                r.0[0] = pages.wrapping_add((sx(r.0[1]) << 2).wrapping_add(page.wrapping_mul(HELP_PAGE_SIZE)));
                if lh(&mut c.m, r.0[0] + 2) == 0 {
                    break;
                }
            }
        }
        // the pads' kinds
        c.m.w16(this + HELP_PADS, 0);
        r.call_leaving(c, PAD_UPDATE, &[], 0x8001_8ff4);
        let m = &mut c.m;
        for pad in [PAD_INFO, PAD_INFO + 32] {
            if m.r8(pad + 4) == 0 {
                let kind = if m.r8(pad + 5) as u32 == PAD_NEGCON { 2 } else { 1 };
                let v = m.r16(this + HELP_PADS) | kind;
                m.w16(this + HELP_PADS, v);
            }
        }
        // as wide as the widest text
        m.w16(this + W, 0);
        r.0[1] = 0;
        while (sx(r.0[1]) as i32) < lh(&mut c.m, this + HELP_LINES) as i32 {
            let text = c.m.r32(this.wrapping_add(sx(r.0[1]) << 2).wrapping_add(HELP_TEXTS));
            let w = r.call_leaving(c, TEXT_PIXELS, &[text], 0x8001_9080);
            if (lh(&mut c.m, this + W) as i32) < w as i32 {
                c.m.w16(this + W, w as u16);
            }
            r.0[1] = r.0[1].wrapping_add(1);
        }
        let m = &mut c.m;
        let both = lh(m, this + HELP_PADS) == 3;
        let past = if both { 70 } else { 40 };
        m.w16(this + HELP_TEXT_X, past);
        let w = m.r16(this + W).wrapping_add(past);
        m.w16(this + W, w);
        let lines = lh(m, this + HELP_LINES);
        let h = if (lines as i32) < 2 { 0 } else { lines.wrapping_mul(15) };
        m.w16(this + H, h as u16);
        let shapes = m.r32(SHAPES);
        let x = (m.r32(SCREEN_WIDTH).wrapping_sub(lh(m, this + W)) as i32) / 2;
        m.w16(this + X, x as u16);
        let y = (240u32.wrapping_sub(lh(m, this + H)) as i32) / 2;
        m.w16(this + Y, y as u16);
        let least = lh(m, shapes + 112).wrapping_add(20);
        if (lh(m, this + W) as i32) < least as i32 {
            m.w16(this + W, least as u16);
        }
        let ticks = m.r32(GAME_TICKS);
        let w = m.r16(this + W).wrapping_add(20);
        m.w16(this + W, w);
        let h = m.r16(this + H).wrapping_add(10);
        m.w16(this + H, h);
        let elapsed = ticks.wrapping_sub(m.r32(this + SHOWN_AT));
        if elapsed < OPENING as u32 {
            let shapes = m.r32(SHAPES);
            let (from_w, from_h) = (lh(m, shapes + 1360), lh(m, shapes + 1362));
            let w = opening(m, this, W, from_w, elapsed, false);
            let h = opening(m, this, H, from_h, elapsed, false);
            m.w16(this + W, w as u16);
            m.w16(this + H, h as u16);
        }
        m.w16(this + Y, 20);
        let x = 505u32.wrapping_sub(m.r16(this + W) as u32);
        m.w16(this + X, x as u16);
    })
}

pub fn abi_calculate_dimensions_help(c: &mut Cpu) {
    calculate_dimensions_help(c, c.r[4]);
}

/// `tDialogHelp::Draw` (FEDIALOG.CPP, 0x8001926c): once open, the title and the lines, each
/// key's button (the PlayStation pad's, the NeGcon's, or both) left of its text, the texts
/// typed out a letter every three ticks; the box.
pub fn draw_help(c: &mut Cpu, this: u32) {
    abi::frame(c, 136, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let (obj, f) = method(&mut c.m, this, VPTR, CALCULATE_DIMENSIONS);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_92b0);
        let ticks = c.m.r32(GAME_TICKS);
        let shown = c.m.r32(this + SHOWN_AT);
        if (ticks as i32) >= (shown.wrapping_add(OPENING as u32) as i32) {
            let typed = (ticks.wrapping_sub(shown).wrapping_sub(OPENING as u32) as i32) / 3;
            r.0[5] = typed as u32;
            r.0[3] = 0;
            let buf = sp + 24;
            r.0[6] = buf;
            loop {
                let k = sx(r.0[3]);
                if k as i32 >= lh(&mut c.m, this + HELP_LINES) as i32 {
                    break;
                }
                let top = c.m.r16(this + Y) as u32;
                r.0[4] = if k == 0 { top.wrapping_add(4) } else { top.wrapping_add(k.wrapping_sub(1).wrapping_mul(15)).wrapping_add(19) };
                if k as i32 > 0 {
                    let m = &mut c.m;
                    let pads = lh(m, this + HELP_PADS);
                    let button = m.r32(this.wrapping_add(k << 2).wrapping_add(HELP_BUTTONS));
                    let row = k.wrapping_sub(1).wrapping_mul(15);
                    let kind = if pads == 2 {
                        NEGCON_BUTTONS
                    } else if pads != 3 || matches!(button, 160 | 80 | 64) {
                        PAD_BUTTONS
                    } else {
                        // both pads: the PlayStation's button and the NeGcon's
                        let b = button & 0xFFFF;
                        r.0[1] = b;
                        r.0[0] = row.wrapping_add(15);
                        let (x, y) = (lh(m, this + X), lh(m, this + Y));
                        let y = y.wrapping_add(r.0[0]).wrapping_add(4);
                        r.call_leaving(c, DRAW_PSX_BUTTON, &[PAD_BUTTONS, b, x.wrapping_add(20), y], 0x8001_93d4);
                        let m = &mut c.m;
                        let (x, y) = (lh(m, this + X), lh(m, this + Y));
                        let y = y.wrapping_add(r.0[0]).wrapping_add(4);
                        r.call_leaving(c, DRAW_PSX_BUTTON, &[NEGCON_BUTTONS, b, x.wrapping_add(40), y], 0x8001_942c);
                        0
                    };
                    if kind != 0 {
                        let m = &mut c.m;
                        let (x, y) = (lh(m, this + X), lh(m, this + Y));
                        r.call_leaving(c, DRAW_PSX_BUTTON, &[kind, button & 0xFFFF, x.wrapping_add(20), row.wrapping_add(y).wrapping_add(19)], 0x8001_942c);
                    }
                }
                r.0[1] = k;
                let at = this.wrapping_add(k << 2);
                r.0[0] = at;
                let text = c.m.r32(at + HELP_TEXTS);
                let len = r.call_leaving(c, STRLEN, &[text], 0x8001_9448);
                let m = &mut c.m;
                let x = sx(m.r16(this + X).wrapping_add(m.r16(this + HELP_TEXT_X)) as u32);
                let y = sx(r.0[4]);
                let ty = if k == 0 { 9 } else { 8 };
                let shown_text = if typed < len as i32 {
                    // typed so far
                    for i in 0..typed.max(0) as u32 {
                        let text = m.r32(at + HELP_TEXTS);
                        let b = m.r8(text.wrapping_add(i));
                        m.w8(buf + i, b);
                    }
                    m.w8(buf.wrapping_add(typed as u32), 0);
                    buf
                } else {
                    m.r32(at + HELP_TEXTS)
                };
                r.0[3] = r.0[3].wrapping_add(1);
                r.call_leaving(c, fetextrender::full_text_entry, &[shown_text, x, y, ty, 1, 0], 0x8001_9520);
            }
        }
        r.enter(c, 0x8001_8af8, 0x8001_9530, |c| draw(c, this));
    })
}

pub fn abi_draw_help(c: &mut Cpu) {
    draw_help(c, c.r[4]);
}

/// `tDialogMessageString::CalculateDimensions` (FEDIALOG.CPP, 0x80019558): the text fading in
/// after the box opens; the box as wide as asked and as high as the text word-wrapped (one line:
/// shown on a line) or as asked, opening over its first ticks, centred (and moved off the
/// centre as asked).
pub fn calculate_dimensions_message(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let ticks = m.r32(GAME_TICKS);
        let since = ticks.wrapping_sub(OPENING as u32).wrapping_sub(m.r32(this + SHOWN_AT)) << 7;
        let fade = 128i32.wrapping_sub(since as i32 / 100);
        m.w32(this + FADE, fade as u32);
        let fade = fade.clamp(0, 128);
        m.w32(this + FADE, fade as u32);
        if fade != 0 {
            m.w32(this + OPEN, 0);
        }
        let width = m.r16(this + 128);
        m.w16(this + W, width);
        let h = if lh(m, this + 130) == 0 {
            let text = m.r32(this + TEXT);
            let h = r.call_leaving(c, fetextrender::word_wrap_height_entry, &[sx((width as u32).wrapping_sub(40)), text], 0x8001_9600);
            let m = &mut c.m;
            m.w16(this + H, h as u16);
            m.w32(this + ONE_LINE, (sx(h) == 8) as u32);
            m.r16(this + H).wrapping_add(16)
        } else {
            m.w32(this + ONE_LINE, 0);
            m.r16(this + 130)
        };
        let m = &mut c.m;
        m.w16(this + H, h);
        let elapsed = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWN_AT));
        if (elapsed as i32) < OPENING {
            let shapes = m.r32(SHAPES);
            let (from_w, from_h) = (lh(m, shapes + 1360), lh(m, shapes + 1362));
            let w = opening(m, this, W, from_w, elapsed, true);
            let h = opening(m, this, H, from_h, elapsed, true);
            m.w32(this + OPEN, 0);
            m.w16(this + W, w as u16);
            m.w16(this + H, h as u16);
        } else {
            m.w32(this + OPEN, 1);
        }
        let x = (m.r32(SCREEN_WIDTH).wrapping_sub(lh(m, this + W)) as i32 / 2) as u32;
        let x = (m.r16(this + 124) as u32).wrapping_add(x);
        m.w16(this + X, x as u16);
        let y = (240u32.wrapping_sub(lh(m, this + H)) as i32 / 2) as u32;
        let y = (m.r16(this + 126) as u32).wrapping_add(y);
        m.w16(this + Y, y as u16);
    })
}

pub fn abi_calculate_dimensions_message(c: &mut Cpu) {
    calculate_dimensions_message(c, c.r[4]);
}

/// `tDialogMessageString::Draw` (FEDIALOG.CPP, 0x8001976c): once open, the text faded in,
/// centred on its line or word-wrapped inside the box (above the room left under it); the box.
pub fn draw_message(c: &mut Cpu, this: u32) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let (obj, f) = method(&mut c.m, this, VPTR, CALCULATE_DIMENSIONS);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_979c);
        let m = &mut c.m;
        if (m.r32(GAME_TICKS) as i32) < (m.r32(this + SHOWN_AT).wrapping_add(OPENING as u32) as i32) {
            m.w32(this + OPEN, 0);
        } else {
            if m.r32(this + FADE) != 0 {
                m.w32(this + OPEN, 0);
            }
            let style = m.r8(TEXT_STYLES + 52) as u32;
            let base = m.r32(TEXT_COLORS + 4 * style);
            let fade = m.r32(this + FADE);
            let color = r.call_leaving(c, CALC_FADE_VAL, &[base, fade], 0x8001_97f8);
            let m = &mut c.m;
            let (x, y, w, h) = (m.r16(this + X), m.r16(this + Y), m.r16(this + W), m.r16(this + H));
            let under = m.r16(this + 110).wrapping_add(8);
            m.w16(sp + 24, x.wrapping_add(17));
            m.w16(sp + 26, y.wrapping_add(10));
            m.w16(sp + 28, w.wrapping_sub(20));
            r.0[1] = color;
            m.w16(sp + 30, h.wrapping_sub(under));
            r.call_leaving(c, fetextrender::set_abr_entry, &[1, 1], 0x8001_9840);
            let m = &mut c.m;
            let text = m.r32(this + TEXT);
            if m.r32(this + ONE_LINE) != 0 {
                let (x, y, w) = (m.r16(this + X) as u32, m.r16(this + Y) as u32, m.r16(this + W));
                let x = sx(x.wrapping_add(((w as i16) >> 1) as i32 as u32));
                let y = sx(y.wrapping_add(8));
                r.call_leaving(c, fetextrender::full_text_rgb_entry, &[text, x, y, color, 0, 2], 0x8001_9890);
            } else {
                r.call_leaving(c, fetextrender::word_wrap_text_rgb_entry, &[text, sp + 24, color], 0x8001_98a8);
            }
            r.call_leaving(c, fetextrender::set_abr_entry, &[0, 0], 0x8001_98b4);
        }
        r.enter(c, 0x8001_8af8, 0x8001_98bc, |c| draw(c, this));
    })
}

pub fn abi_draw_message(c: &mut Cpu) {
    draw_message(c, c.r[4]);
}

/// `tDialogBackUpOnly::ProcessInput` (FEDIALOG.CPP, 0x800198d0): for its player, Start hides the
/// dialog; any key but help is taken.
pub fn abi_process_input_back_up_only(c: &mut Cpu) {
    let (this, player, key) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 24, |c, _| {
        let only = lh(&mut c.m, this + PLAYER);
        if only != u32::MAX && player != only {
            return;
        }
        match c.m.r32(key) {
            KEY_START => {
                Saved::of(c).call_ptr(c, 0x8001_8a44, &[this], 0x8001_9918);
            }
            KEY_HELP => {}
            _ => c.m.w32(key, KEY_NONE),
        }
    })
}

/// `tDialogInteractive::Run` (FEDIALOG.CPP, 0x80019928): the dialog shown and the front end run
/// a frame at a time until it is answered: each player's key (help to the current menu while
/// the front end takes input, hiding the help dialog when it is up), then to the dialog; hidden
/// with a sound after; the answer.
pub fn run(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        c.m.w32(this + 156, 1);
        r.call_ptr(c, 0x8001_8980, &[this], 0x8001_9960);
        let keys = sp + 16;
        let command = sp + 24;
        r.0[7] = keys;
        r.0[4] = 1;
        r.0[5] = 0x8005_0000;
        let (obj, f) = method(&mut c.m, this, VPTR, CALCULATE_DIMENSIONS);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_9984);
        c.m.w32(this + ANSWERED, 0);
        loop {
            r.0[6] = u32::MAX;
            if c.m.r32(this + ANSWERED) != 0 {
                break;
            }
            c.m.w32(command, 0);
            r.0[1] = 0;
            r.0[0] = keys;
            r.0[2] = 0;
            while (r.0[1] as i32) < 2 {
                let (player, key) = (r.0[1], r.0[0]);
                let k = r.call_leaving(c, GET_KEY, &[player, u32::MAX], 0x8001_99bc);
                c.m.w32(key, if k == 0 { KEY_NONE } else { k });
                if c.m.r32(key) == KEY_HELP {
                    let app = c.m.r32(FE_APP);
                    let menu = c.m.r32(app.wrapping_add(r.0[2]).wrapping_add(APP_MENUS));
                    if c.m.r32(app + APP_NO_INPUT) == 0 && menu != 0 {
                        let (obj, f) = method(&mut c.m, menu, MENU_VPTR, PROCESS_INPUT);
                        r.call_ptr_leaving(c, f, &[obj, player, key, command], 0x8001_9a30);
                        c.m.w32(key, KEY_NONE);
                    }
                }
                if c.m.r32(key) != KEY_NONE {
                    let help = c.m.r32(FE_APP).wrapping_add(APP_HELP);
                    if c.m.r32(help + SHOWN) != 0 {
                        r.call_ptr(c, 0x8001_8a44, &[help], 0x8001_9a68);
                        c.m.w32(key, KEY_NONE);
                    }
                }
                if c.m.r32(key) != 0 {
                    let (obj, f) = method(&mut c.m, this, VPTR, PROCESS_INPUT_SLOT);
                    r.call_ptr_leaving(c, f, &[obj, player, key, command], 0x8001_9a9c);
                }
                r.0[0] = key.wrapping_add(4);
                r.0[2] = r.0[2].wrapping_add(4);
                r.0[1] = player.wrapping_add(1);
            }
            let app = c.m.r32(FE_APP);
            r.call_ptr_leaving(c, FRONT_END_FRAME, &[app], 0x8001_9ab8);
        }
        r.call_leaving(c, PLAY_FE_SFX, &[0], 0x8001_9ac8);
        r.call_ptr(c, 0x8001_8a44, &[this], 0x8001_9ad0);
        let app = c.m.r32(FE_APP);
        r.call_ptr_leaving(c, FRONT_END_FRAME, &[app], 0x8001_9ae0);
        let answer = lh(&mut c.m, this + ANSWER);
        c.m.w32(this + 156, 0);
        answer
    })
}

pub fn abi_run(c: &mut Cpu) {
    c.r[2] = run(c, c.r[4]);
}

/// `tDialogYesNo::CalculateDimensions` (FEDIALOG.CPP, 0x80019b14): a message's, with room for
/// the answers under the text (growing in as the box opens), unless its height is given.
pub fn calculate_dimensions_yes_no(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter(c, 0x8001_9558, 0x8001_9b28, |c| calculate_dimensions_message(c, this));
        let m = &mut c.m;
        if m.r16(this + 130) != 0 {
            return;
        }
        let elapsed = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWN_AT)) as i32;
        let room = if elapsed < OPENING { elapsed.wrapping_mul(15) / OPENING } else { 15 };
        let h = m.r16(this + H).wrapping_add(room as u16);
        m.w16(this + H, h);
        m.w16(this + 110, 15);
        let y = (240u32.wrapping_sub(lh(m, this + H)) as i32 / 2) as u32;
        let y = (m.r16(this + 126) as u32).wrapping_add(y);
        m.w16(this + Y, y as u16);
    })
}

pub fn abi_calculate_dimensions_yes_no(c: &mut Cpu) {
    calculate_dimensions_yes_no(c, c.r[4]);
}

/// `tDialogYesNo::tDialogYesNo` (FEDIALOG.CPP, 0x80019bcc): a screen made a dialog for either
/// player, 288 wide, hidden, its text unfaded, answering yes.
pub fn yes_no(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_leaving(c, SCREEN, &[this], 0x8001_9be0);
        let m = &mut c.m;
        m.w32(this + VPTR, BASE_VT);
        m.w16(this + 128, 288);
        m.w16(this + PLAYER, u16::MAX);
        m.w32(this + VPTR, MESSAGE_VT);
        m.w32(this + FADE, 128);
        m.w32(this + VPTR, INTERACTIVE_VT);
        m.w32(this + SHOWN, 0);
        for f in [110, 130, 126, 124, H, W, Y, X, 136] {
            m.w16(this + f, 0);
        }
        for f in [TIMEOUT, ONE_LINE, OPEN, 156] {
            m.w32(this + f, 0);
        }
        m.w32(this + VPTR, YES_NO_VT);
        m.w16(this + ANSWER, 0);
        m.w32(this + ANSWERED, 0);
        this
    })
}

pub fn abi_yes_no(c: &mut Cpu) {
    c.r[2] = yes_no(c, c.r[4]);
}

/// `tDialogYesNo::Draw` (FEDIALOG.CPP, 0x80019c84): once open, the two answers (their texts,
/// the chosen one highlighted) faded in along the bottom, a quarter and three quarters across;
/// the message.
pub fn draw_yes_no(c: &mut Cpu, this: u32) {
    abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let (obj, f) = method(&mut c.m, this, VPTR, CALCULATE_DIMENSIONS);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_9ccc);
        let m = &mut c.m;
        if m.r32(this + FADE) != 0 {
            m.w32(this + OPEN, 0);
        }
        let elapsed = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWN_AT)) as i32;
        r.0[3] = 0;
        if elapsed >= OPENING {
            r.0[7] = TEXT_COLORS;
            r.0[6] = TEXT_STYLES;
            r.0[5] = this;
            r.0[4] = lh(m, this + X).wrapping_add(((m.r16(this + W) as i16) >> 2) as i32 as u32);
            while (r.0[3] as i32) < 2 {
                let m = &mut c.m;
                let state = if r.0[3] == lh(m, this + ANSWER) { 1 } else { 2 };
                let style = m.r8(TEXT_STYLES + 51 + state) as u32;
                let (base, fade) = (m.r32(TEXT_COLORS + 4 * style), m.r32(this + FADE));
                r.0[3] = r.0[3].wrapping_add(1);
                let color = r.call_leaving(c, CALC_FADE_VAL, &[base, fade], 0x8001_9d64);
                let m = &mut c.m;
                r.0[0] = lh(m, this + Y).wrapping_add(lh(m, this + H)).wrapping_sub(11);
                r.0[1] = color;
                r.call_leaving(c, fetextrender::set_abr_entry, &[1, 1], 0x8001_9d84);
                r.0[0] <<= 16;
                let id = c.m.r32(r.0[5] + 160);
                let word = r.call_leaving(c, TEXT_WORD, &[id], 0x8001_9d90);
                let args = [word, sx(r.0[4]), ((r.0[0] as i32) >> 16) as u32, color, 0, 2];
                r.call_leaving(c, fetextrender::full_text_rgb_entry, &args, 0x8001_9db4);
                r.call_leaving(c, fetextrender::set_abr_entry, &[0, 0], 0x8001_9dc0);
                r.0[5] = r.0[5].wrapping_add(4);
                let w = c.m.r16(this + W) as i16;
                r.0[4] = r.0[4].wrapping_add((w >> 1) as i32 as u32);
            }
        }
        // `tDialogMessageString::Draw` is hooked (the host hides the save message when autosaving):
        // called by address, `Cpu::call` routing it to the port after the host
        r.call_ptr(c, 0x8001_976C, &[this], 0x8001_9de0);
    })
}

pub fn abi_draw_yes_no(c: &mut Cpu) {
    draw_yes_no(c, c.r[4]);
}

/// `tDialogYesNo::ProcessInput` (FEDIALOG.CPP, 0x80019e0c): for its player, once open: left
/// yes, right no (with their sounds); accept or back answers.
pub fn process_input_yes_no(c: &mut Cpu, this: u32, player: u32, key: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let only = lh(&mut c.m, this + PLAYER);
        if (only != u32::MAX && player != only) || c.m.r32(this + OPEN) != 1 {
            return;
        }
        match c.m.r32(key) {
            KEY_LEFT => {
                r.call_leaving(c, PLAY_FE_SFX, &[5], 0x8001_9e94);
                c.m.w16(this + ANSWER, 1);
            }
            KEY_RIGHT => {
                r.call_leaving(c, PLAY_FE_SFX, &[6], 0x8001_9ea8);
                c.m.w16(this + ANSWER, 0);
            }
            KEY_ACCEPT | KEY_BACK => {
                r.call_leaving(c, PLAY_FE_SFX, &[0], 0x8001_9eb8);
                c.m.w32(this + ANSWERED, 1);
                r.call_ptr(c, 0x8001_8a44, &[this], 0x8001_9ec8);
            }
            _ => {}
        }
    })
}

pub fn abi_process_input_yes_no(c: &mut Cpu) {
    process_input_yes_no(c, c.r[4], c.r[5], c.r[6]);
}

/// `tDialogYesNoMem::ProcessInput` (FEDIALOG.CPP, 0x80019ed8): answered -1 when the memory card
/// in use is taken out (its events, or no card), else a yes-or-no's.
pub fn abi_process_input_yes_no_mem(c: &mut Cpu) {
    let (this, player, key, command) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let port = (c.m.r32(CARD_PORT) << 2).wrapping_add(1);
        r.0[..6].copy_from_slice(&[0, port, this, player, key, command]);
        let event = r.call_leaving(c, CARD_EVENTS, &[port], 0x8001_9f24).wrapping_sub(2);
        if event < 22 && c.m.r32(CARD_EVENT_TABLE + 4 * event) == CARD_EVENT_CLOSES {
            r.0[0] = 1;
        }
        let card = r.call_leaving(c, GET_CARD, &[port], 0x8001_9f5c);
        if c.m.r32(card) == u32::MAX {
            r.0[0] = 1;
        }
        if r.0[0] != 0 {
            c.m.w32(this + ANSWERED, 1);
            c.m.w16(this + ANSWER, u16::MAX);
        } else {
            r.enter(c, 0x8001_9e0c, 0x8001_9f98, |c| process_input_yes_no(c, this, player, key));
        }
    })
}

/// `tDialogYesNoTri::ProcessInput` (FEDIALOG.CPP, 0x80019fbc): Start answers -1, else a
/// yes-or-no's.
pub fn abi_process_input_yes_no_tri(c: &mut Cpu) {
    let (this, player, key) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 24, |c, _| {
        if c.m.r32(key) == KEY_START {
            c.m.w32(this + ANSWERED, 1);
            c.m.w16(this + ANSWER, u16::MAX);
        } else {
            Saved::of(c).enter(c, 0x8001_9e0c, 0x8001_9fec, |c| process_input_yes_no(c, this, player, key));
        }
    })
}

/// `tDialogNoInputMessage::ProcessInput` (FEDIALOG.CPP, 0x80019ffc): no input taken.
pub fn abi_process_input_no_input(_: &mut Cpu) {}

/// A dialog's destructor (FEDIALOG.H; the one at `addr`): the screen's.
fn destructor(c: &mut Cpu, addr: u32) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        Saved::of(c).call_leaving(c, SCREEN_DESTRUCTOR, &[this, flags], addr + 0x10);
    })
}

/// `tDialogMessageStringWithTimeout::~tDialogMessageStringWithTimeout` (FEDIALOG.H, 0x80015884).
pub fn abi_message_with_timeout_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_5884);
}

/// `tDialogYesNoTri::~tDialogYesNoTri` (FEDIALOG.H, 0x8001a004).
pub fn abi_yes_no_tri_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a004);
}

/// `tDialogYesNoMem::~tDialogYesNoMem` (FEDIALOG.H, 0x8001a024).
pub fn abi_yes_no_mem_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a024);
}

/// `tDialogYesNo::~tDialogYesNo` (FEDIALOG.H, 0x8001a044).
pub fn abi_yes_no_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a044);
}

/// `tDialogInteractive::~tDialogInteractive` (FEDIALOG.H, 0x8001a064).
pub fn abi_interactive_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a064);
}

/// `tDialogNoInputMessage::~tDialogNoInputMessage` (FEDIALOG.H, 0x8001a084).
pub fn abi_no_input_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a084);
}

/// `tDialogBackUpOnly::~tDialogBackUpOnly` (FEDIALOG.H, 0x8001a0a4).
pub fn abi_back_up_only_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a0a4);
}

/// `tDialogMessageString::~tDialogMessageString` (FEDIALOG.H, 0x8001a0c4).
pub fn abi_message_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a0c4);
}

/// `tDialogHelp::~tDialogHelp` (FEDIALOG.H, 0x8001a0e4).
pub fn abi_help_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a0e4);
}

/// `tDialogBase::~tDialogBase` (FEDIALOG.H, 0x8001a104).
pub fn abi_base_destructor(c: &mut Cpu) {
    destructor(c, 0x8001_a104);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, MENU2, pick, player, via_abi};
    use crate::femenu::{KEY_DOWN, KEY_OPTIONS, KEY_UP};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A dialog of each class (vtables), 0x100 apart, and their texts.
    const DIALOG_AT: u32 = 0x8014_D000;
    const HELP_VT: u32 = 0x8001_0548;
    const BACK_UP_ONLY_VT: u32 = 0x8001_0488;
    const NO_INPUT_VT: u32 = 0x8001_0428;
    const YES_NO_MEM_VT: u32 = 0x8001_0308;
    const YES_NO_TRI_VT: u32 = 0x8001_02A8;
    const VTS: [u32; 8] = [HELP_VT, MESSAGE_VT, BACK_UP_ONLY_VT, NO_INPUT_VT, INTERACTIVE_VT, YES_NO_VT, YES_NO_MEM_VT, YES_NO_TRI_VT];
    /// (in the RAM the image leaves free: the CD's reads are at 0x8014e268)
    const STRINGS: u32 = 0x8014_7000;
    /// The pads' library state (`Padglobal`, 34 bytes a pad); the memory cards' (the one in
    /// use's first word -1 when it is out; `MCRD_getcard`'s).
    const PAD_GLOBAL: u32 = 0x8013_FBF8;
    const CARD_INFO: u32 = 0x8005_3694;
    /// The help's vtable copied, its `CalculateDimensions` doing nothing: its `Draw` then
    /// draws the shuffled lines (the real one makes its own, whose buttons are never 64).
    const FAKE_HELP_VT: u32 = 0x8014_DF00;

    fn dialog(k: u32) -> u32 {
        DIALOG_AT + 0x100 * k
    }

    const HELP: u32 = DIALOG_AT;

    fn message(rng: &mut Rng) -> u32 {
        dialog(1 + pick(rng, VTS.len() as u32 - 1))
    }

    fn yes_no(rng: &mut Rng) -> u32 {
        dialog(5 + pick(rng, 3))
    }

    fn any(rng: &mut Rng) -> u32 {
        dialog(pick(rng, VTS.len() as u32))
    }

    /// Texts short enough for `FullTextRGB`'s 128-byte buffer.
    fn short_texts() -> &'static [u32] {
        static TEXTS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
        TEXTS.get_or_init(|| {
            let ram = fe_ram().unwrap_or_default();
            if ram.is_empty() {
                return vec![0];
            }
            let mut c = crate::test_util::cpu_with(&ram);
            (0..400).filter(|&id| base::text_len(&mut c.m, id as i16) < 56).collect()
        })
    }

    /// Words of capitals and spaces, `len` letters at most, at `at`.
    fn string(m: &mut Mem, rng: &mut Rng, at: u32, len: u32) -> u32 {
        let n = pick(rng, len + 1);
        for k in 0..n {
            let b = if pick(rng, 6) == 0 { 32 } else { 65 + pick(rng, 26) as u8 };
            m.w8(at + k, b);
        }
        m.w8(at + n, 0);
        at
    }

    /// FEMENU's structures (the players' menus the help reads), then: a dialog of each class,
    /// shown or not, open or not, for either player or one, shown a while ago or just now,
    /// timing out or not, its place, size, texts and answers; the help's lines and the pads'
    /// kinds; the box's starting size and the help's title width; the dialogs shown (none to
    /// all eight); the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for menu in [MENU, MENU2] {
            let mut flags = m.r32(menu);
            for bit in [0x400, 0x800, 0x2_0000] {
                if pick(rng, 4) == 0 {
                    flags |= bit;
                }
            }
            m.w32(menu, flags);
        }
        let app = m.r32(FE_APP);
        let real = m.r32(app + APP_MENUS);
        for p in 0..2 {
            m.w32(app + APP_MENUS + 4 * p, [real, MENU, MENU2][pick(rng, 3) as usize]);
            m.w32(app + APP_OPTIONS + 4 * p, [0, 1, 3, u32::MAX][pick(rng, 4) as usize]);
        }
        let ticks = m.r32(GAME_TICKS).wrapping_add(rng.next() as u32 & 0xFFFF);
        m.w32(GAME_TICKS, ticks);
        let texts = short_texts();
        for (k, vt) in VTS.into_iter().enumerate() {
            let d = dialog(k as u32);
            m.w32(d + VPTR, vt);
            m.w16(d + PLAYER, [u32::MAX, 0, 1][pick(rng, 3) as usize] as u16);
            m.w16(d + X, pick(rng, 400) as u16);
            m.w16(d + Y, pick(rng, 240) as u16);
            m.w16(d + W, pick(rng, 400) as u16);
            m.w16(d + H, pick(rng, 240) as u16);
            m.w16(d + 110, [0, 15, pick(rng, 40)][pick(rng, 3) as usize] as u16);
            m.w32(d + SHOWN, pick(rng, 2));
            let ago = [0, 1, 49, 50, 51, pick(rng, 60), pick(rng, 400), pick(rng, 2000)][pick(rng, 8) as usize];
            m.w32(d + SHOWN_AT, ticks.wrapping_sub(ago));
            m.w32(d + TIMEOUT, [0, 0, 50, pick(rng, 300), u32::MAX][pick(rng, 5) as usize]);
            m.w16(d + 124, (pick(rng, 80) as i32 - 40) as u16);
            m.w16(d + 126, (pick(rng, 80) as i32 - 40) as u16);
            m.w16(d + 128, (100 + pick(rng, 220)) as u16);
            m.w16(d + 130, [0, 0, 40, pick(rng, 200)][pick(rng, 4) as usize] as u16);
            m.w32(d + OPEN, [0, 1, 1, 2][pick(rng, 4) as usize]);
            m.w16(d + 136, pick(rng, 2) as u16);
            m.w16(d + ANSWER, [0, 1, 1, u32::MAX][pick(rng, 4) as usize] as u16);
            m.w32(d + FADE, [0, 128, pick(rng, 129)][pick(rng, 3) as usize]);
            let len = [20, 50, 200][pick(rng, 3) as usize];
            let s = string(m, rng, STRINGS + 0x100 * k as u32, len);
            m.w32(d + TEXT, s);
            m.w32(d + ONE_LINE, pick(rng, 2));
            m.w32(d + ANSWERED, pick(rng, 2));
            m.w32(d + 156, pick(rng, 2));
            for f in [160, 164] {
                m.w32(d + f, texts[pick(rng, texts.len() as u32) as usize]);
            }
        }
        // the help: its first page, its lines (the title, keys with their buttons)
        m.w16(HELP + TEXT, 0);
        let lines = pick(rng, HELP_MAX_LINES + 1);
        m.w16(HELP + HELP_LINES, lines as u16);
        for k in 0..HELP_MAX_LINES {
            let len = [8, 30, 55][pick(rng, 3) as usize];
            let s = string(m, rng, STRINGS + 0x800 + 0x40 * k, len);
            m.w32(HELP + HELP_TEXTS + 4 * k, s);
            let button = [0, 8, 64, 80, 160, 0x1000, 0x4000, 0xFFFF_8000, pick(rng, 0x1_0000)][pick(rng, 9) as usize];
            m.w32(HELP + HELP_BUTTONS + 4 * k, button);
        }
        m.w16(HELP + HELP_PADS, [0, 1, 2, 3, pick(rng, 8)][pick(rng, 5) as usize] as u16);
        for k in 0..24 {
            let v = m.r32(HELP_VT + 4 * k);
            m.w32(FAKE_HELP_VT + 4 * k, v);
        }
        m.w32(FAKE_HELP_VT + CALCULATE_DIMENSIONS + 4, base::NOTHING);
        if pick(rng, 2) == 0 {
            m.w32(HELP + VPTR, FAKE_HELP_VT);
        }
        m.w16(HELP + HELP_TEXT_X, [40, 70][pick(rng, 2) as usize] as u16);
        // the pads (`PAD_update` copies them from the library's state: connected or not, their
        // kinds)
        for p in [PAD_GLOBAL, PAD_GLOBAL + 34] {
            m.w8(p, (pick(rng, 3) == 0) as u8);
            m.w8(p + 1, [PAD_NEGCON, 0x41, 0x73][pick(rng, 3) as usize] as u8);
        }
        // the box's starting size (now and then wider than the box: it shrinks open), the
        // help's title
        let shapes = m.r32(SHAPES);
        m.w16(shapes + 112, [2, pick(rng, 300)][pick(rng, 2) as usize] as u16);
        for f in [1360, 1362] {
            m.w16(shapes + f, [0, 7, 18, pick(rng, 80), pick(rng, 250), (pick(rng, 100) as i32 - 50) as u32][pick(rng, 6) as usize] as u16);
        }
        // the memory card in use, taken out or not
        let port = pick(rng, 2);
        m.w32(CARD_PORT, port);
        m.w32(CARD_INFO + 616 * 4 * port, [u32::MAX, 0, 1][pick(rng, 3) as usize]);
        // the dialogs shown
        let n = pick(rng, MAX_DIALOGS + 1);
        for k in 0..MAX_DIALOGS {
            let d = if k < n { dialog(pick(rng, VTS.len() as u32)) } else { 0 };
            m.w32(DIALOGS + 4 * k, d);
        }
        let keys = [0, KEY_NONE, KEY_ACCEPT, KEY_HELP, KEY_OPTIONS, KEY_START, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_BACK];
        m.w32(KEY, keys[pick(rng, keys.len() as u32) as usize]);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("~tDialogMessageStringWithTimeout", 0x8001_5884, nfs4rc::f_80015884, |_, r| vec![message(r), 2 * pick(r, 2)]),
            ("ShouldTimeOut", 0x8001_8808, nfs4rc::f_80018808, |_, r| vec![any(r)]),
            ("InitializeClass", 0x8001_8840, nfs4rc::f_80018840, |_, _| vec![]),
            ("DrawAllDialogs", 0x8001_8860, nfs4rc::f_80018860, |_, _| vec![]),
            ("HideAllDialogs", 0x8001_8930, nfs4rc::f_80018930, |_, _| vec![]),
            ("GetTopMostDialog", 0x8001_8970, nfs4rc::f_80018970, |_, _| vec![]),
            ("Display", 0x8001_8980, nfs4rc::f_80018980, |_, r| vec![any(r)]),
            ("Hide", 0x8001_8a44, nfs4rc::f_80018a44, |c, r| {
                // (now and then one of those shown)
                let d = c.m.r32(DIALOGS + 4 * pick(r, MAX_DIALOGS));
                vec![if d != 0 && pick(r, 2) == 0 { d } else { any(r) }]
            }),
            ("tDialogBase::Draw", 0x8001_8af8, nfs4rc::f_80018af8, |_, r| vec![any(r)]),
            ("tDialogBase::ProcessInput", 0x8001_8c08, nfs4rc::f_80018c08, |_, r| vec![any(r), player(r), KEY, COMMAND]),
            ("AddItem", 0x8001_8c4c, nfs4rc::f_80018c4c, |_, r| vec![HELP, pick(r, 400), [8, 80, 0x8000, pick(r, 0x1_0000)][pick(r, 4) as usize]]),
            ("tDialogHelp::CalculateDimensions", 0x8001_8cd0, nfs4rc::f_80018cd0, |_, _| vec![HELP]),
            ("tDialogHelp::Draw", 0x8001_926c, nfs4rc::f_8001926c, |_, _| vec![HELP]),
            ("tDialogMessageString::CalculateDimensions", 0x8001_9558, nfs4rc::f_80019558, |_, r| vec![message(r)]),
            ("tDialogMessageString::Draw", 0x8001_976c, nfs4rc::f_8001976c, |_, r| vec![message(r)]),
            ("tDialogBackUpOnly::ProcessInput", 0x8001_98d0, nfs4rc::f_800198d0, |_, r| vec![dialog(2), player(r), KEY, COMMAND]),
            ("tDialogYesNo::CalculateDimensions", 0x8001_9b14, nfs4rc::f_80019b14, |_, r| vec![yes_no(r)]),
            ("tDialogYesNo", 0x8001_9bcc, nfs4rc::f_80019bcc, |_, _| vec![base::NEW]),
            ("tDialogYesNo::Draw", 0x8001_9c84, nfs4rc::f_80019c84, |_, r| vec![yes_no(r)]),
            ("tDialogYesNo::ProcessInput", 0x8001_9e0c, nfs4rc::f_80019e0c, |_, r| vec![yes_no(r), player(r), KEY, COMMAND]),
            ("tDialogYesNoMem::ProcessInput", 0x8001_9ed8, nfs4rc::f_80019ed8, |_, r| vec![dialog(6), player(r), KEY, COMMAND]),
            ("tDialogYesNoTri::ProcessInput", 0x8001_9fbc, nfs4rc::f_80019fbc, |_, r| vec![dialog(7), player(r), KEY, COMMAND]),
            ("tDialogNoInputMessage::ProcessInput", 0x8001_9ffc, nfs4rc::f_80019ffc, |_, r| vec![dialog(3), player(r), KEY, COMMAND]),
            ("~tDialogYesNoTri", 0x8001_a004, nfs4rc::f_8001a004, |_, r| vec![dialog(7), 2 * pick(r, 2)]),
            ("~tDialogYesNoMem", 0x8001_a024, nfs4rc::f_8001a024, |_, r| vec![dialog(6), 2 * pick(r, 2)]),
            ("~tDialogYesNo", 0x8001_a044, nfs4rc::f_8001a044, |_, r| vec![dialog(5), 2 * pick(r, 2)]),
            ("~tDialogInteractive", 0x8001_a064, nfs4rc::f_8001a064, |_, r| vec![dialog(4), 2 * pick(r, 2)]),
            ("~tDialogNoInputMessage", 0x8001_a084, nfs4rc::f_8001a084, |_, r| vec![dialog(3), 2 * pick(r, 2)]),
            ("~tDialogBackUpOnly", 0x8001_a0a4, nfs4rc::f_8001a0a4, |_, r| vec![dialog(2), 2 * pick(r, 2)]),
            ("~tDialogMessageString", 0x8001_a0c4, nfs4rc::f_8001a0c4, |_, r| vec![dialog(1), 2 * pick(r, 2)]),
            ("~tDialogHelp", 0x8001_a0e4, nfs4rc::f_8001a0e4, |_, r| vec![HELP, 2 * pick(r, 2)]),
            ("~tDialogBase", 0x8001_a104, nfs4rc::f_8001a104, |_, r| vec![any(r), 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn fedialog_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xD1A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xD1A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
