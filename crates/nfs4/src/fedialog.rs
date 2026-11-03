//! FEDIALOG.CPP: the front end's dialogs. Up to eight are shown at once over the screen, the
//! top one first; each is a box that grows open over 50 ticks and fades its text in.
//! - The help box: the keys the current menu takes with the pads' buttons, typed out letter
//!   by letter.
//! - Message boxes: a text word-wrapped or on one line; those that take only Start, or no
//!   input.
//! - The interactive ones (`Run`: the front end's frames go on while one waits for its
//!   answer): yes or no, the memory card's (closed when the card is taken out), and the one
//!   Start answers too.
//!
//! The dialogs are screens (`tScreen`, their vtable pointer at +96); their `ProcessInput`,
//! `CalculateDimensions` and `Draw` are virtual. Written as FEMENU.CPP is (see there).

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{ITEM_DISABLED, KEY_ACCEPT, KEY_START, KEY_HELP, KEY_LEFT, KEY_NONE, KEY_RIGHT, KEY_BACK, PLAY_FE_SFX, PROCESS_INPUT};
use crate::femenuextended::{DRAW_TRANS_SQUARE, MENU_VPTR, SET_ABR_MODE, STRLEN, TEXT_PIXELS, TEXT_WORD};
use crate::fetextrender;
use crate::globals::{FE_APP, GAME_TICKS, PAD_INFO, SCREEN_WIDTH, SHAPES, TEXT_COLORS, TEXT_STYLES};
use crate::types::{DialogBase, DialogHelp, DialogInteractive, DialogMessageString, DialogYesNo, FEApplication, Menu, MenuItem, RECT};
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

/// A dialog's vtable pointer (a screen's) and its virtual `ProcessInput`,
/// `CalculateDimensions` and `Draw`.
const VPTR: u32 = 96;
const PROCESS_INPUT_SLOT: u32 = 72;
const CALCULATE_DIMENSIONS: u32 = 80;
const DRAW: u32 = 88;

/// How long a dialog takes to open (ticks).
const OPENING: i32 = 50;
/// The help's lines (its title and six keys at most).
const HELP_MAX_LINES: i16 = 7;

/// `FeTools_DrawPSXButton`; `PAD_update`; `blockmove`; `CalcFadeVal`; the front end's key for a
/// player (FEINPUT.CPP); the memory card's events and its state (`MCRD_handlecardevents`,
/// `MCRD_getcard`); the screen's constructor and destructor (`tScreen`).
const DRAW_PSX_BUTTON: u32 = 0x8002_69d0;
const PAD_UPDATE: fn(&mut Cpu) = nfs4rc::f_800e4e08;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const CALC_FADE_VAL: u32 = 0x8004_bd68;
const GET_KEY: u32 = 0x8002_3ca8;
const CARD_EVENTS: fn(&mut Cpu) = crate::memcard::handlecardevents_entry;
const GET_CARD: fn(&mut Cpu) = crate::memcard::getcard_entry;
const SCREEN: u32 = 0x8002_6040;
const SCREEN_DESTRUCTOR: u32 = 0x8002_6098;
/// Hooked by the host, so called by address: `tDialog::Display` and `Hide` (it notes the
/// dialog for its autosave's sounds); `tFEApplication::Redraw` (it autosaves between frames);
/// `tDialogMessageString::Draw` (it hides the save's message while autosaving).
const DISPLAY: u32 = 0x8001_8980;
const HIDE: u32 = 0x8001_8A44;
const FRONT_END_FRAME: u32 = 0x8001_3894;
const MESSAGE_DRAW: u32 = 0x8001_976C;

/// The help's pages (one), 18 bytes each: a byte whether it shows the menu's keys, then up to
/// four lines of its own (a text, a button: shorts) to a 0 text.
const HELP_PAGES: u32 = 0x8001_023C;
const HELP_PAGE_SIZE: u32 = 18;
/// The help's title, and its keys' texts and the pads' buttons for them.
const HELP_TITLE: u32 = 90;
const HELP_MOVE: (u32, u32) = (83, 80);
const HELP_CHOOSE: (u32, u32) = (84, 160);
const HELP_SELECT: u32 = 86;
const HELP_SELECT_OR_PAGE: u32 = 87;
const HELP_ACCEPT_BUTTON: u32 = 0x4000;
const HELP_OPTIONS: (u32, u32) = (85, 0x1000);
const HELP_MENU_OPTIONS: (u32, u32) = (88, 0xFFFF_8000);
const HELP_BACK: u32 = 89;
const HELP_BACK_BUTTON: u32 = 8;
/// The buttons' shapes: the PlayStation pad's, the NeGcon's.
const PAD_BUTTONS: u32 = 65;
const NEGCON_BUTTONS: u32 = 35;
const PAD_NEGCON: u32 = 0x23;
/// The memory card in use (`gMemCardPort`); its events' jump table (those that close the
/// dialog jump to 0x80019f50).
const CARD_PORT: u32 = 0x8005_1EF0;
const CARD_EVENT_TABLE: u32 = 0x8001_0250;
const CARD_EVENT_CLOSES: u32 = 0x8001_9F50;
/// The application's help dialog (`gFEApp + 344`), each player's pages of options (+324),
/// whether the front end takes no input (+456).
const APP_HELP: u32 = 344;
const APP_OPTIONS: u32 = 324;
const APP_NO_INPUT: u32 = 456;
/// A menu's flags the help reads: back goes to the parent; start goes on; its items' kinds.
const MENU_BACK_PARENT: u32 = 0x800;
const MENU_START_CONTINUES: u32 = 0x3_0000;
const MENU_START_ACCEPTS: u32 = 4;
const ITEM_LEFT_RIGHT: u32 = 0x400;
/// The front end's sounds: a dialog opening, closing; left, right, accept.
const SOUND_OPEN: u32 = 15;
const SOUND_CLOSE: u32 = 18;
/// A message's text style (type 8, state 1); the yes-or-no answers' (type 8, states 1 and 2).
const MESSAGE_STYLE: u32 = 52;
const ANSWER_STYLES: u32 = 51;
/// The dialogs' opening shape (42nd: its width and height, the size they open from).
const OPENING_SHAPE: u32 = 1360;
/// The help box's narrowest (the 4th shape's width, +20) and where it opens.
const HELP_TITLE_SHAPE: u32 = 112;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn ticks(m: &mut Mem) -> u32 {
    m.r32(GAME_TICKS)
}

/// The dialog's `k`th place on the stack.
fn slot(k: u32) -> u32 {
    DIALOGS.wrapping_add(k << 2)
}

/// `tDialogBase::ShouldTimeOut` (FEDIALOG.CPP, 0x80018808): whether a dialog with a timeout has
/// been shown that long.
pub fn should_time_out(m: &mut Mem, this: DialogBase) -> u32 {
    let timeout = this.time_out_ticks(m);
    if timeout <= 0 {
        return 0;
    }
    let shown = ticks(m).wrapping_sub(this.start_ticks(m) as u32) as i32;
    (shown >= timeout) as u32
}

pub fn abi_should_time_out(c: &mut Cpu) {
    c.r[2] = should_time_out(&mut c.m, DialogBase(c.r[4]));
}

/// `tDialogBase::InitializeClass` (FEDIALOG.CPP, 0x80018840): no dialogs shown.
pub fn abi_initialize_class(c: &mut Cpu) {
    hide_all_dialogs(&mut c.m);
}

/// `tDialogBase::DrawAllDialogs` (FEDIALOG.CPP, 0x80018860): each dialog shown, the top one
/// first, drawn; one that has timed out hidden instead (the rest move up, and the one moved
/// into its place is drawn).
pub fn abi_draw_all_dialogs(c: &mut Cpu) {
    if c.m.r32(DIALOGS) == 0 {
        return;
    }
    for k in 0..MAX_DIALOGS {
        let dialog = c.m.r32(slot(k));
        if should_time_out(&mut c.m, DialogBase(dialog)) != 0 {
            call_addr(c, HIDE, &[dialog]);
            if c.m.r32(slot(k)) == 0 {
                return;
            }
        }
        let dialog = c.m.r32(slot(k));
        call_virtual(c, dialog, VPTR, DRAW, &[]);
        if c.m.r32(slot(k + 1)) == 0 {
            return;
        }
    }
}

/// `tDialogBase::HideAllDialogs` (FEDIALOG.CPP, 0x80018930).
pub fn hide_all_dialogs(m: &mut Mem) {
    for k in 0..MAX_DIALOGS {
        m.w32(slot(k), 0);
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
pub fn display(c: &mut Cpu, this: DialogBase) {
    let m = &mut c.m;
    if this.currently_on(m) != 0 {
        return;
    }
    let first = this.default(m);
    this.set_currently_on(m, 1);
    this.set_fully_open(m, 0);
    this.set_return_value(m, first);
    for k in (1..MAX_DIALOGS).rev() {
        let v = m.r32(slot(k - 1));
        m.w32(slot(k), v);
    }
    m.w32(DIALOGS, this.0);
    let now = ticks(m);
    this.set_start_ticks(m, now as i32);
    call(c, PLAY_FE_SFX, &[SOUND_OPEN]);
    let m = &mut c.m;
    this.set_fully_open(m, 0);
    this.set_fade_text(m, 128);
}

pub fn abi_display(c: &mut Cpu) {
    display(c, DialogBase(c.r[4]));
}

/// `tDialogBase::Hide` (FEDIALOG.CPP, 0x80018a44): a dialog shown taken off the stack (those
/// under it moved up), with a sound.
pub fn hide(c: &mut Cpu, this: DialogBase) {
    if this.currently_on(&mut c.m) == 0 {
        return;
    }
    this.set_currently_on(&mut c.m, 0);
    for k in 0..MAX_DIALOGS {
        let at = slot(k);
        if c.m.r32(at) == this.0 {
            this.set_currently_on(&mut c.m, 0);
            c.m.w32(at, 0);
            call(c, BLOCKMOVE, &[at + 4, at, (7 - k) << 2]);
            c.m.w32(slot(MAX_DIALOGS - 1), 0);
            call(c, PLAY_FE_SFX, &[SOUND_CLOSE]);
        }
    }
}

pub fn abi_hide(c: &mut Cpu) {
    hide(c, DialogBase(c.r[4]));
}

/// `tDialogBase::Draw` (FEDIALOG.CPP, 0x80018af8): the box's frame (four edges, the sides 4
/// wide, the top and bottom 2 high) and its shaded inside.
pub fn draw(c: &mut Cpu, this: DialogBase) {
    let m = &mut c.m;
    let (x, y, w, h) = (this.left(m) as i32 as u32, this.top(m) as i32 as u32, this.width(m) as i32 as u32, this.height(m) as i32 as u32);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, x, y, 4, h, 4]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(w).wrapping_sub(4), y, 4, h, 4]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y, w.wrapping_sub(8), 2, 4]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y.wrapping_add(h).wrapping_sub(2), w.wrapping_sub(8), 2, 4]);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y.wrapping_add(2), w.wrapping_sub(8), h.wrapping_sub(4), 2]);
    call(c, SET_ABR_MODE, &[0]);
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, DialogBase(c.r[4]));
}

/// `tDialogBase::ProcessInput` (FEDIALOG.CPP, 0x80018c08): any key hides the dialog (and is
/// taken).
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    if c.m.r32(key) != KEY_NONE {
        call_addr(c, HIDE, &[this]);
        c.m.w32(key, KEY_NONE);
    }
}

/// `tDialogHelp::AddItem` (FEDIALOG.CPP, 0x80018c4c): a line (a text and the pads' button for
/// it), while there is room for one.
pub fn add_item(c: &mut Cpu, this: DialogHelp, text: u32, button: u32) {
    if this.num_items(&mut c.m) >= HELP_MAX_LINES {
        return;
    }
    let word = call(c, TEXT_WORD, &[sx(text)]);
    let m = &mut c.m;
    let k = this.num_items(m) as i32 as u32;
    this.set_text(m, k, word);
    let k = this.num_items(m) as i32 as u32;
    this.set_cont(m, k, sx(button) as i32);
    this.update_num_items(m, |n| n.wrapping_add(1));
}

pub fn abi_add_item(c: &mut Cpu) {
    add_item(c, DialogHelp(c.r[4]), c.r[5], c.r[6]);
}

/// The size of an opening box: grown from twice the opening shape's size `from` to the full
/// `size` over the first 50 ticks (`elapsed` of them; `signed` the division the original does:
/// the help's unsigned, a message's signed).
fn opening(size: i16, from: i16, elapsed: u32, signed: bool) -> i16 {
    let from = from as i32;
    let start = ((from << 1) as u16 as i16) as i32;
    let grown = (size as i32).wrapping_sub(start).wrapping_mul(elapsed as i32);
    let step = if signed { grown / OPENING } else { ((grown as u32 as u64 * 0x51EB_851F) >> 36) as i32 };
    (from << 1).wrapping_add(step) as i16
}

/// The opening shape's width and height.
fn opening_size(m: &mut Mem) -> (i16, i16) {
    let shapes = m.r32(SHAPES);
    (m.r16(shapes + OPENING_SHAPE) as i16, m.r16(shapes + OPENING_SHAPE + 2) as i16)
}

/// `tDialogHelp::CalculateDimensions` (FEDIALOG.CPP, 0x80018cd0): the help's lines: its title,
/// then the keys the player's current menu takes (up and down past two items or more, left and
/// right on its choices, accept on its other items, the options, the pages, start, back), its
/// page's own; the pads' kinds; a box as wide as the widest text past the buttons (as wide as
/// the help's title shape at least), 15 high a line, at the top right, opening over its first
/// ticks.
pub fn calculate_dimensions_help(c: &mut Cpu, this: DialogHelp) {
    call(c, fetextrender::set_font_entry, &[0]);
    this.set_num_items(&mut c.m, 0);
    add_item(c, this, HELP_TITLE, 0);
    let player = match this.specific_player(&mut c.m) {
        -1 => 0,
        p => p as i32 as u32,
    };
    let page = HELP_PAGES.wrapping_add((this.variant(&mut c.m) as i32 as u32).wrapping_mul(HELP_PAGE_SIZE));
    if c.m.r8(page) != 0 {
        // the keys the player's current menu takes
        let app = FEApplication(c.m.r32(FE_APP));
        // (the player's current menu, read as the original does for any player)
        let menu = Menu(c.m.r32(app.0.wrapping_add(player << 2).wrapping_add(4)));
        let (mut choices, mut others) = (false, false);
        let mut k = 0u32;
        loop {
            let it = MenuItem(c.m.r32(menu.0.wrapping_add(sx(k) << 2).wrapping_add(16)));
            if it.0 == 0 {
                break;
            }
            let flags = it.flags(&mut c.m);
            if flags & ITEM_DISABLED == 0 {
                if flags & ITEM_LEFT_RIGHT != 0 {
                    choices = true;
                } else {
                    others = true;
                }
            }
            k = k.wrapping_add(1);
        }
        let line = |c: &mut Cpu, (text, button): (u32, u32)| add_item(c, this, text, button);
        if sx(crate::femenu::get_number_enabled_items(&mut c.m, menu)) as i32 >= 2 {
            line(c, HELP_MOVE);
        }
        if choices {
            line(c, HELP_CHOOSE);
        }
        if others {
            let text = if menu.flags(&mut c.m) & MENU_START_CONTINUES != 0 { HELP_SELECT_OR_PAGE } else { HELP_SELECT };
            line(c, (text, HELP_ACCEPT_BUTTON));
        }
        if c.m.r32(app.0.wrapping_add(player << 2).wrapping_add(APP_OPTIONS)) as i32 > 0 {
            line(c, HELP_OPTIONS);
        }
        if menu.options_menu(&mut c.m).0 != 0 {
            line(c, HELP_MENU_OPTIONS);
        }
        let m = &mut c.m;
        let flags = menu.flags(m);
        if flags & MENU_BACK_PARENT != 0 {
            line(c, (HELP_BACK, HELP_BACK_BUTTON));
        } else if menu.next_menu(m).0 != 0 || flags & ITEM_LEFT_RIGHT != 0 || menu.on_button_press(m) != 0 || menu.flags(m) & MENU_START_ACCEPTS != 0 {
            line(c, (HELP_SELECT_OR_PAGE, HELP_BACK_BUTTON));
        }
    }
    // the page's own lines
    let own = |m: &mut Mem, k: u32| {
        let at = HELP_PAGES.wrapping_add(sx(k) << 2).wrapping_add((this.variant(m) as i32 as u32).wrapping_mul(HELP_PAGE_SIZE));
        (m.r16(at + 2) as i16 as i32 as u32, m.r16(at + 4) as i16 as i32 as u32)
    };
    if own(&mut c.m, 0).0 != 0 {
        let mut k = 0u32;
        loop {
            let (text, button) = own(&mut c.m, k);
            add_item(c, this, text, button);
            k = k.wrapping_add(1);
            if own(&mut c.m, k).0 == 0 {
                break;
            }
        }
    }
    // the pads' kinds: 1 a pad, 2 a NeGcon
    this.set_helpcontrollers(&mut c.m, 0);
    call(c, PAD_UPDATE, &[]);
    let m = &mut c.m;
    for pad in [PAD_INFO, PAD_INFO + 32] {
        if m.r8(pad + 4) == 0 {
            let kind = if m.r8(pad + 5) as u32 == PAD_NEGCON { 2 } else { 1 };
            this.update_helpcontrollers(m, |v| v | kind);
        }
    }
    // as wide as the widest text
    this.set_width(m, 0);
    let mut k = 0u32;
    while (sx(k) as i32) < this.num_items(&mut c.m) as i32 {
        let text = this.text(&mut c.m, sx(k));
        let w = call(c, TEXT_PIXELS, &[text]);
        if (this.width(&mut c.m) as i32) < w as i32 {
            this.set_width(&mut c.m, w as i16);
        }
        k = k.wrapping_add(1);
    }
    let m = &mut c.m;
    let past = if this.helpcontrollers(m) == 3 { 70 } else { 40 };
    this.set_lefttext(m, past);
    this.update_width(m, |w| w.wrapping_add(past));
    let lines = this.num_items(m);
    this.set_height(m, if lines < 2 { 0 } else { lines.wrapping_mul(15) });
    let x = (m.r32(SCREEN_WIDTH).wrapping_sub(this.width(m) as i32 as u32) as i32) / 2;
    this.set_left(m, x as i16);
    let y = (240u32.wrapping_sub(this.height(m) as i32 as u32) as i32) / 2;
    this.set_top(m, y as i16);
    let shapes = m.r32(SHAPES);
    let least = (m.r16(shapes + HELP_TITLE_SHAPE) as i16).wrapping_add(20);
    if this.width(m) < least {
        this.set_width(m, least);
    }
    let now = ticks(m);
    this.update_width(m, |w| w.wrapping_add(20));
    this.update_height(m, |h| h.wrapping_add(10));
    let elapsed = now.wrapping_sub(this.start_ticks(m) as u32);
    if elapsed < OPENING as u32 {
        let (from_w, from_h) = opening_size(m);
        let (w, h) = (opening(this.width(m), from_w, elapsed, false), opening(this.height(m), from_h, elapsed, false));
        this.set_width(m, w);
        this.set_height(m, h);
    }
    this.set_top(m, 20);
    let x = 505u32.wrapping_sub(this.width(m) as u16 as u32);
    this.set_left(m, x as i16);
}

pub fn abi_calculate_dimensions_help(c: &mut Cpu) {
    calculate_dimensions_help(c, DialogHelp(c.r[4]));
}

/// `tDialogHelp::Draw` (FEDIALOG.CPP, 0x8001926c): once open, the title and the lines, each
/// key's button (the PlayStation pad's, the NeGcon's, or both) left of its text, the texts
/// typed out a letter every three ticks; the box.
pub fn draw_help(c: &mut Cpu, this: DialogHelp) {
    call_virtual(c, this.0, VPTR, CALCULATE_DIMENSIONS, &[]);
    let now = ticks(&mut c.m);
    let shown = this.start_ticks(&mut c.m) as u32;
    if (now as i32) >= (shown.wrapping_add(OPENING as u32) as i32) {
        let typed = (now.wrapping_sub(shown).wrapping_sub(OPENING as u32) as i32) / 3;
        with_buffer(c, 112, |c, buf| {
            let mut k = 0u32;
            while (sx(k) as i32) < this.num_items(&mut c.m) as i32 {
                let k_ = sx(k);
                let top = this.top(&mut c.m) as u16 as u32;
                let y = if k_ == 0 { top.wrapping_add(4) } else { top.wrapping_add(k_.wrapping_sub(1).wrapping_mul(15)).wrapping_add(19) };
                if k_ as i32 > 0 {
                    // the key's button
                    let m = &mut c.m;
                    let pads = this.helpcontrollers(m);
                    let button = this.cont(m, k_) as u32;
                    let row = k_.wrapping_sub(1).wrapping_mul(15);
                    let (x, by) = (this.left(m) as i32 as u32, (this.top(m) as i32 as u32).wrapping_add(row).wrapping_add(19));
                    if pads == 2 {
                        call_addr(c, DRAW_PSX_BUTTON, &[NEGCON_BUTTONS, button & 0xFFFF, x.wrapping_add(20), by]);
                    } else if pads != 3 || matches!(button, 160 | 80 | 64) {
                        call_addr(c, DRAW_PSX_BUTTON, &[PAD_BUTTONS, button & 0xFFFF, x.wrapping_add(20), by]);
                    } else {
                        // both pads: the PlayStation's button and the NeGcon's
                        call_addr(c, DRAW_PSX_BUTTON, &[PAD_BUTTONS, button & 0xFFFF, x.wrapping_add(20), by]);
                        let m = &mut c.m;
                        let (x, by) = (this.left(m) as i32 as u32, (this.top(m) as i32 as u32).wrapping_add(row).wrapping_add(19));
                        call_addr(c, DRAW_PSX_BUTTON, &[NEGCON_BUTTONS, button & 0xFFFF, x.wrapping_add(40), by]);
                    }
                }
                // its text, typed so far
                let text = this.text(&mut c.m, k_);
                let len = call(c, STRLEN, &[text]);
                let m = &mut c.m;
                let x = sx((this.left(m) as u16).wrapping_add(this.lefttext(m) as u16) as u32);
                let shown_text = if typed < len as i32 {
                    for i in 0..typed.max(0) as u32 {
                        let text = this.text(m, k_);
                        let b = m.r8(text.wrapping_add(i));
                        m.w8(buf + i, b);
                    }
                    m.w8(buf.wrapping_add(typed as u32), 0);
                    buf
                } else {
                    this.text(m, k_)
                };
                k = k.wrapping_add(1);
                let ty = if k_ == 0 { 9 } else { 8 };
                call(c, fetextrender::full_text_entry, &[shown_text, x, sx(y), ty, 1, 0]);
            }
        });
    }
    draw(c, this.base());
}

pub fn abi_draw_help(c: &mut Cpu) {
    draw_help(c, DialogHelp(c.r[4]));
}

/// `tDialogMessageString::CalculateDimensions` (FEDIALOG.CPP, 0x80019558): the text fading in
/// after the box opens; the box as wide as asked and as high as the text word-wrapped (one line:
/// shown on a line) or as asked, opening over its first ticks, centred (and moved off the
/// centre as asked).
pub fn calculate_dimensions_message(c: &mut Cpu, this: DialogMessageString) {
    let m = &mut c.m;
    let since = ticks(m).wrapping_sub(OPENING as u32).wrapping_sub(this.start_ticks(m) as u32) << 7;
    let fade = 128i32.wrapping_sub(since as i32 / 100).clamp(0, 128);
    this.set_fade_text(m, fade);
    if fade != 0 {
        this.set_fully_open(m, 0);
    }
    let width = this.max_w(m);
    this.set_width(m, width);
    let height = if this.max_h(m) == 0 {
        let text = this.string(m);
        let h = call(c, fetextrender::word_wrap_height_entry, &[sx((width as u32).wrapping_sub(40)), text]);
        let m = &mut c.m;
        this.set_height(m, h as i16);
        this.set_centerit(m, (sx(h) == 8) as u32);
        this.height(m).wrapping_add(16)
    } else {
        this.set_centerit(m, 0);
        this.max_h(m)
    };
    let m = &mut c.m;
    this.set_height(m, height);
    let elapsed = ticks(m).wrapping_sub(this.start_ticks(m) as u32);
    if (elapsed as i32) < OPENING {
        let (from_w, from_h) = opening_size(m);
        let (w, h) = (opening(this.width(m), from_w, elapsed, true), opening(this.height(m), from_h, elapsed, true));
        this.set_fully_open(m, 0);
        this.set_width(m, w);
        this.set_height(m, h);
    } else {
        this.set_fully_open(m, 1);
    }
    let x = (m.r32(SCREEN_WIDTH).wrapping_sub(this.width(m) as i32 as u32) as i32 / 2) as u32;
    let left = (this.offset_x(m) as u16 as u32).wrapping_add(x);
    this.set_left(m, left as i16);
    centre_vertically(m, this.base());
}

/// The box centred on the screen's height, moved off the centre as asked.
fn centre_vertically(m: &mut Mem, this: DialogBase) {
    let y = (240u32.wrapping_sub(this.height(m) as i32 as u32) as i32 / 2) as u32;
    let top = (this.offset_y(m) as u16 as u32).wrapping_add(y);
    this.set_top(m, top as i16);
}

pub fn abi_calculate_dimensions_message(c: &mut Cpu) {
    calculate_dimensions_message(c, DialogMessageString(c.r[4]));
}

/// `tDialogMessageString::Draw` (FEDIALOG.CPP, 0x8001976c): once open, the text faded in,
/// centred on its line or word-wrapped inside the box (above the room left under it); the box.
pub fn draw_message(c: &mut Cpu, this: DialogMessageString) {
    call_virtual(c, this.0, VPTR, CALCULATE_DIMENSIONS, &[]);
    let m = &mut c.m;
    if (ticks(m) as i32) < ((this.start_ticks(m) as u32).wrapping_add(OPENING as u32) as i32) {
        this.set_fully_open(m, 0);
    } else {
        if this.fade_text(m) != 0 {
            this.set_fully_open(m, 0);
        }
        let style = m.r8(TEXT_STYLES + MESSAGE_STYLE) as u32;
        let base = m.r32(TEXT_COLORS + 4 * style);
        let fade = this.fade_text(m) as u32;
        let colour = call_addr(c, CALC_FADE_VAL, &[base, fade]);
        with_buffer(c, 8, |c, at| {
            let (m, rect) = (&mut c.m, RECT(at));
            let (x, y, w, h) = (this.left(m), this.top(m), this.width(m), this.height(m));
            let under = this.reservedheight(m).wrapping_add(8);
            rect.set_x(m, x.wrapping_add(17));
            rect.set_y(m, y.wrapping_add(10));
            rect.set_w(m, w.wrapping_sub(20));
            rect.set_h(m, h.wrapping_sub(under));
            call(c, fetextrender::set_abr_entry, &[1, 1]);
            let m = &mut c.m;
            let text = this.string(m);
            if this.centerit(m) != 0 {
                let (x, y, w) = (this.left(m) as u16 as u32, this.top(m) as u16 as u32, this.width(m));
                let x = sx(x.wrapping_add((w >> 1) as i32 as u32));
                call(c, fetextrender::full_text_rgb_entry, &[text, x, sx(y.wrapping_add(8)), colour, 0, 2]);
            } else {
                call(c, fetextrender::word_wrap_text_rgb_entry, &[text, rect.0, colour]);
            }
            call(c, fetextrender::set_abr_entry, &[0, 0]);
        });
    }
    draw(c, this.base());
}

pub fn abi_draw_message(c: &mut Cpu) {
    draw_message(c, DialogMessageString(c.r[4]));
}

/// Whether a dialog for `only` (-1: either player) listens to `player`.
fn listens_to(m: &mut Mem, this: DialogBase, player: u32) -> bool {
    let only = this.specific_player(m) as i32 as u32;
    only == u32::MAX || player == only
}

/// `tDialogBackUpOnly::ProcessInput` (FEDIALOG.CPP, 0x800198d0): for its player, Start hides the
/// dialog; any key but help is taken.
pub fn abi_process_input_back_up_only(c: &mut Cpu) {
    let (this, player, key) = (DialogBase(c.r[4]), c.r[5], c.r[6]);
    if !listens_to(&mut c.m, this, player) {
        return;
    }
    match c.m.r32(key) {
        KEY_BACK => {
            call_addr(c, HIDE, &[this.0]);
        }
        KEY_HELP => {}
        _ => c.m.w32(key, KEY_NONE),
    }
}

/// `tDialogInteractive::Run` (FEDIALOG.CPP, 0x80019928): the dialog shown and the front end run
/// a frame at a time until it is answered: each player's key (help goes to the current menu
/// while the front end takes input; any key hides the help dialog when it is up), then to the
/// dialog; hidden with a sound after; the answer.
pub fn run(c: &mut Cpu, this: DialogInteractive) -> u32 {
    this.set_currently_running(&mut c.m, 1);
    call_addr(c, DISPLAY, &[this.0]);
    call_virtual(c, this.0, VPTR, CALCULATE_DIMENSIONS, &[]);
    this.set_ready_to_return_value(&mut c.m, 0);
    // (the two players' keys at +0, the menu command at +8)
    with_buffer(c, 16, |c, keys| {
        let command = keys + 8;
        while this.ready_to_return_value(&mut c.m) == 0 {
            c.m.w32(command, 0);
            for player in 0..2u32 {
                let key = keys + 4 * player;
                let k = call_addr(c, GET_KEY, &[player, u32::MAX]);
                c.m.w32(key, if k == 0 { KEY_NONE } else { k });
                let app = FEApplication(c.m.r32(FE_APP));
                if c.m.r32(key) == KEY_HELP {
                    let menu = app.current_menu(&mut c.m, player);
                    if c.m.r32(app.0 + APP_NO_INPUT) == 0 && menu.0 != 0 {
                        call_virtual(c, menu.0, MENU_VPTR, PROCESS_INPUT, &[player, key, command]);
                        c.m.w32(key, KEY_NONE);
                    }
                }
                if c.m.r32(key) != KEY_NONE {
                    let help = DialogBase(c.m.r32(FE_APP).wrapping_add(APP_HELP));
                    if help.currently_on(&mut c.m) != 0 {
                        call_addr(c, HIDE, &[help.0]);
                        c.m.w32(key, KEY_NONE);
                    }
                }
                if c.m.r32(key) != 0 {
                    call_virtual(c, this.0, VPTR, PROCESS_INPUT_SLOT, &[player, key, command]);
                }
            }
            let app = c.m.r32(FE_APP);
            call_addr(c, FRONT_END_FRAME, &[app]);
        }
    });
    call(c, PLAY_FE_SFX, &[0]);
    call_addr(c, HIDE, &[this.0]);
    let app = c.m.r32(FE_APP);
    call_addr(c, FRONT_END_FRAME, &[app]);
    let answer = this.return_value(&mut c.m) as i32 as u32;
    this.set_currently_running(&mut c.m, 0);
    answer
}

pub fn abi_run(c: &mut Cpu) {
    c.r[2] = run(c, DialogInteractive(c.r[4]));
}

/// The room under a yes-or-no's text for its answers.
const ANSWERS_ROOM: i32 = 15;

/// `tDialogYesNo::CalculateDimensions` (FEDIALOG.CPP, 0x80019b14): a message's, with room for
/// the answers under the text (growing in as the box opens), unless its height is given.
pub fn calculate_dimensions_yes_no(c: &mut Cpu, this: DialogMessageString) {
    calculate_dimensions_message(c, this);
    let m = &mut c.m;
    if this.max_h(m) != 0 {
        return;
    }
    let elapsed = ticks(m).wrapping_sub(this.start_ticks(m) as u32) as i32;
    let room = if elapsed < OPENING { elapsed.wrapping_mul(ANSWERS_ROOM) / OPENING } else { ANSWERS_ROOM };
    this.update_height(m, |h| h.wrapping_add(room as i16));
    this.set_reservedheight(m, ANSWERS_ROOM as i16);
    centre_vertically(m, this.base());
}

pub fn abi_calculate_dimensions_yes_no(c: &mut Cpu) {
    calculate_dimensions_yes_no(c, DialogMessageString(c.r[4]));
}

/// `tDialogYesNo::tDialogYesNo` (FEDIALOG.CPP, 0x80019bcc): a screen made a dialog for either
/// player, 288 wide, hidden, its text unfaded, answering yes (through each base class's
/// constructor: the vtable written as each does).
pub fn yes_no(c: &mut Cpu, this: DialogYesNo) -> u32 {
    call_addr(c, SCREEN, &[this.0]);
    let m = &mut c.m;
    this.set_vtable(m, BASE_VT);
    this.set_max_w(m, 288);
    this.set_specific_player(m, -1);
    this.set_vtable(m, MESSAGE_VT);
    this.set_fade_text(m, 128);
    this.set_vtable(m, INTERACTIVE_VT);
    this.set_currently_on(m, 0);
    this.set_reservedheight(m, 0);
    this.set_max_h(m, 0);
    this.set_offset_y(m, 0);
    this.set_offset_x(m, 0);
    this.set_height(m, 0);
    this.set_width(m, 0);
    this.set_top(m, 0);
    this.set_left(m, 0);
    this.set_default(m, 0);
    this.set_time_out_ticks(m, 0);
    this.set_centerit(m, 0);
    this.set_fully_open(m, 0);
    this.set_currently_running(m, 0);
    this.set_vtable(m, YES_NO_VT);
    this.set_return_value(m, 0);
    this.set_ready_to_return_value(m, 0);
    this.0
}

pub fn abi_yes_no(c: &mut Cpu) {
    c.r[2] = yes_no(c, DialogYesNo(c.r[4]));
}

/// `tDialogYesNo::Draw` (FEDIALOG.CPP, 0x80019c84): once open, the two answers (their texts,
/// the chosen one highlighted) faded in along the bottom, a quarter and three quarters across;
/// the message (by address: the host hides the save's message).
pub fn draw_yes_no(c: &mut Cpu, this: DialogYesNo) {
    call_virtual(c, this.0, VPTR, CALCULATE_DIMENSIONS, &[]);
    let m = &mut c.m;
    if this.fade_text(m) != 0 {
        this.set_fully_open(m, 0);
    }
    let elapsed = ticks(m).wrapping_sub(this.start_ticks(m) as u32) as i32;
    if elapsed >= OPENING {
        let mut x = (this.left(m) as i32 as u32).wrapping_add((this.width(m) >> 2) as i32 as u32);
        for k in 0..2u32 {
            let m = &mut c.m;
            let state = if k == this.return_value(m) as i32 as u32 { 1 } else { 2 };
            let style = m.r8(TEXT_STYLES + ANSWER_STYLES + state) as u32;
            let (base, fade) = (m.r32(TEXT_COLORS + 4 * style), this.fade_text(m) as u32);
            let colour = call_addr(c, CALC_FADE_VAL, &[base, fade]);
            let m = &mut c.m;
            let y = sx((this.top(m) as i32 as u32).wrapping_add(this.height(m) as i32 as u32).wrapping_sub(11));
            call(c, fetextrender::set_abr_entry, &[1, 1]);
            let id = this.yesnowords(&mut c.m, k) as u32;
            let word = call(c, TEXT_WORD, &[id]);
            call(c, fetextrender::full_text_rgb_entry, &[word, sx(x), y, colour, 0, 2]);
            call(c, fetextrender::set_abr_entry, &[0, 0]);
            x = x.wrapping_add((this.width(&mut c.m) >> 1) as i32 as u32);
        }
    }
    call_addr(c, MESSAGE_DRAW, &[this.0]);
}

pub fn abi_draw_yes_no(c: &mut Cpu) {
    draw_yes_no(c, DialogYesNo(c.r[4]));
}

/// `tDialogYesNo::ProcessInput` (FEDIALOG.CPP, 0x80019e0c): for its player, once open: left
/// yes, right no (with their sounds); accept or back answers.
pub fn process_input_yes_no(c: &mut Cpu, this: DialogInteractive, player: u32, key: u32) {
    if !listens_to(&mut c.m, this.base().base(), player) || this.fully_open(&mut c.m) != 1 {
        return;
    }
    match c.m.r32(key) {
        KEY_LEFT => {
            call(c, PLAY_FE_SFX, &[5]);
            this.set_return_value(&mut c.m, 1);
        }
        KEY_RIGHT => {
            call(c, PLAY_FE_SFX, &[6]);
            this.set_return_value(&mut c.m, 0);
        }
        KEY_ACCEPT | KEY_START => {
            call(c, PLAY_FE_SFX, &[0]);
            this.set_ready_to_return_value(&mut c.m, 1);
            call_addr(c, HIDE, &[this.0]);
        }
        _ => {}
    }
}

pub fn abi_process_input_yes_no(c: &mut Cpu) {
    process_input_yes_no(c, DialogInteractive(c.r[4]), c.r[5], c.r[6]);
}

/// A dialog answered -1 (cancelled).
fn cancel(m: &mut Mem, this: DialogInteractive) {
    this.set_ready_to_return_value(m, 1);
    this.set_return_value(m, -1);
}

/// `tDialogYesNoMem::ProcessInput` (FEDIALOG.CPP, 0x80019ed8): answered -1 when the memory card
/// in use is taken out (its events, or no card), else a yes-or-no's.
pub fn abi_process_input_yes_no_mem(c: &mut Cpu) {
    let (this, player, key) = (DialogInteractive(c.r[4]), c.r[5], c.r[6]);
    let port = (c.m.r32(CARD_PORT) << 2).wrapping_add(1);
    let event = call(c, CARD_EVENTS, &[port]).wrapping_sub(2);
    let mut gone = event < 22 && c.m.r32(CARD_EVENT_TABLE + 4 * event) == CARD_EVENT_CLOSES;
    let card = call(c, GET_CARD, &[port]);
    if c.m.r32(card) == u32::MAX {
        gone = true;
    }
    if gone {
        cancel(&mut c.m, this);
    } else {
        process_input_yes_no(c, this, player, key);
    }
}

/// `tDialogYesNoTri::ProcessInput` (FEDIALOG.CPP, 0x80019fbc): Start answers -1, else a
/// yes-or-no's.
pub fn abi_process_input_yes_no_tri(c: &mut Cpu) {
    let (this, player, key) = (DialogInteractive(c.r[4]), c.r[5], c.r[6]);
    if c.m.r32(key) == KEY_BACK {
        cancel(&mut c.m, this);
    } else {
        process_input_yes_no(c, this, player, key);
    }
}

/// `tDialogNoInputMessage::ProcessInput` (FEDIALOG.CPP, 0x80019ffc): no input taken.
pub fn abi_process_input_no_input(_: &mut Cpu) {}

/// The dialogs' destructors (FEDIALOG.H): the screen's.
fn destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    call_addr(c, SCREEN_DESTRUCTOR, &[this, flags]);
}

/// `tDialogMessageStringWithTimeout::~tDialogMessageStringWithTimeout` (FEDIALOG.H, 0x80015884).
pub fn abi_message_with_timeout_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogYesNoTri::~tDialogYesNoTri` (FEDIALOG.H, 0x8001a004).
pub fn abi_yes_no_tri_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogYesNoMem::~tDialogYesNoMem` (FEDIALOG.H, 0x8001a024).
pub fn abi_yes_no_mem_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogYesNo::~tDialogYesNo` (FEDIALOG.H, 0x8001a044).
pub fn abi_yes_no_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogInteractive::~tDialogInteractive` (FEDIALOG.H, 0x8001a064).
pub fn abi_interactive_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogNoInputMessage::~tDialogNoInputMessage` (FEDIALOG.H, 0x8001a084).
pub fn abi_no_input_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogBackUpOnly::~tDialogBackUpOnly` (FEDIALOG.H, 0x8001a0a4).
pub fn abi_back_up_only_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogMessageString::~tDialogMessageString` (FEDIALOG.H, 0x8001a0c4).
pub fn abi_message_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogHelp::~tDialogHelp` (FEDIALOG.H, 0x8001a0e4).
pub fn abi_help_destructor(c: &mut Cpu) {
    destructor(c);
}

/// `tDialogBase::~tDialogBase` (FEDIALOG.H, 0x8001a104).
pub fn abi_base_destructor(c: &mut Cpu) {
    destructor(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU, MENU2, pick, player, via_abi};
    use crate::femenu::{KEY_DOWN, KEY_OPTIONS, KEY_UP};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    // The dialogs' fields the shuffle sets (`tDialogBase`'s, a message's, the help's, the
    // yes-or-no's), and the application's menus.
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
    const APP_MENUS: u32 = 4;

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
        let keys = [0, KEY_NONE, KEY_ACCEPT, KEY_HELP, KEY_OPTIONS, KEY_BACK, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_START];
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
