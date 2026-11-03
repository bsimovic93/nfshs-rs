//! PAUSEMENU.CPP: the race's pause menu classes, a smaller copy of the front end's (FEMENU.CPP):
//! - texts: a word of the in-game text set drawn where the menu is at (`gPause_CurrentY`),
//!   centred as a title on the title's line;
//! - list iterators (`tPListIterator`): an `int` value indexing a list of text ids a 0 ends, or
//!   one of an array of values chosen by a byte (`tPListIteratorIndexed`, the player's);
//! - menu items: text, interactive ones, left-right choices and sliders (indexed: one value a
//!   player), buttons that go to a menu or give a command;
//! - menus (`tPMenu`): a title (item 0) and items up and down move between, past those that are
//!   disabled or can't be gone to.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free. Objects are typed views (`crate::types`);
//! each `pub fn` is the original function, `abi_*` its entry from the game.

use crate::call::{call_addr, call_virtual};
use crate::gpu;
use crate::types::{
    PListIterator, PListIteratorIndexed, PMenu, PMenuCommand, PMenuItem, PMenuItemCommandButton, PMenuItemGoToMenuButton,
    PMenuItemLeftRightChoice, PMenuItemLeftRightSlider, PMenuItemLeftRightSliderIndexed,
};
use ps1rt::{Cpu, Mem};

/// The vtables.
const ITERATOR_VT: u32 = 0x8005_6AE8;
const INDEXED_VT: u32 = 0x8005_6AB8;
const ITEM_VT: u32 = 0x8005_6A80;
const TEXT_VT: u32 = 0x8005_6A48;
const INTERACTIVE_VT: u32 = 0x8005_6A10;
const CHOICE_VT: u32 = 0x8005_69D8;
const SLIDER_VT: u32 = 0x8005_69A0;
const SLIDER_INDEXED_VT: u32 = 0x8005_6968;
const GO_TO_MENU_VT: u32 = 0x8005_6930;
const COMMAND_VT: u32 = 0x8005_68F8;
const MENU_VT: u32 = 0x8005_68D0;

/// Where an iterator's, an item's and a menu's vtable pointer is.
pub(crate) const ITERATOR_VPTR: u32 = 8;
pub(crate) const ITEM_VPTR: u32 = 8;
pub(crate) const MENU_VPTR: u32 = 80;
/// The iterators' virtual functions (their vtable entries' offsets).
const VALUE: u32 = 16;
const TEXT_VALUE: u32 = 24;
const INCREMENT: u32 = 32;
const DECREMENT: u32 = 40;
/// The items': `NextMenu`, `Debounce`, `ProcessInput(key, command)`, `IsNavigable`, `Draw(selected)`.
const NEXT_MENU: u32 = 16;
const DEBOUNCE: u32 = 24;
const PROCESS_INPUT: u32 = 32;
const IS_NAVIGABLE: u32 = 40;
const DRAW: u32 = 48;
/// The menus': `Initialize`, `ProcessInput(key, command)`, `Draw`.
pub(crate) const MENU_INITIALIZE: u32 = 16;
pub(crate) const MENU_PROCESS_INPUT: u32 = 24;
pub(crate) const MENU_DRAW: u32 = 32;

/// The input keys (`tInputKeyType`): taken (none), accept, back, up, down, left, right, start.
pub(crate) const KEY_TAKEN: u32 = 1;
pub(crate) const KEY_ACCEPT: u32 = 2;
pub(crate) const KEY_BACK: u32 = 16;
pub(crate) const KEY_UP: u32 = 512;
pub(crate) const KEY_DOWN: u32 = 1024;
pub(crate) const KEY_LEFT: u32 = 2048;
pub(crate) const KEY_RIGHT: u32 = 4096;
pub(crate) const KEY_START: u32 = 8192;
/// The menu commands (`tPMenuCommandType`): carry on racing (start), go to `nextMenu`, back.
/// The buttons' others (restart, quit, forfeit) are `MPause_Logic`'s results.
pub(crate) const COMMAND_CONTINUE: u32 = 1;
pub(crate) const COMMAND_GO_TO: u32 = 6;
pub(crate) const COMMAND_BACK: u32 = 7;

/// A menu item's flags: disabled.
pub(crate) const ITEM_DISABLED: u32 = 1;

/// The pause menu's globals: the line being drawn on (`gPause_CurrentY`; the title's is 98),
/// something changed that wants the menu drawn again (`gMPauseUpdateNextTime`).
pub(crate) const CURRENT_Y: u32 = 0x8013_F0CC;
pub(crate) const TITLE_Y: u32 = 98;
pub(crate) const UPDATE_NEXT_TIME: u32 = 0x8013_E518;
/// The language (`GameSetup.userSetting.language`): the choices' arrows are only drawn in the
/// first.
pub(crate) const LANGUAGE: u32 = 0x8011_4584;

/// The text set's words (`TextSys_Word`, `TextSys_WordX`, `TextSys_WordFlags`: its
/// justification), a string's width (`textpixels`), the font (`Font_TextColor`, and
/// `Font_TextXY`, hooked by the host).
const WORD: u32 = 0x800B_9F18;
const WORD_FLAGS: u32 = 0x800B_9F38;
const WORD_X: u32 = 0x800B_9F58;
const TEXT_PIXELS: u32 = 0x800E_4A2C;
const FONT_TEXT_COLOR: u32 = 0x800C_BF6C;
const FONT_TEXT_XY: u32 = 0x800C_C7F0;
/// A justification: right, centred.
const RIGHT: i16 = 1;
const CENTRED: i16 = 2;
/// The text colours: a selected item's, the others', the title's.
const SELECTED_COLOUR: u32 = 3;
const TEXT_COLOUR: u32 = 4;
const TITLE_COLOUR: u32 = 6;
/// `AudioCmn_PlayPauseSound`: up and down, accept and back, a value changed.
const PLAY_PAUSE_SOUND: u32 = 0x8007_A16C;
const SOUND_MOVE: u32 = 3;
const SOUND_ACCEPT: u32 = 4;
const SOUND_CHANGE: u32 = 5;
/// The library's `__builtin_delete`.
const DELETE: u32 = 0x800A_4910;
/// `Hud_FBuildF4` (hooked by the host): a flat box.
pub(crate) const FBUILD_F4: u32 = 0x800D_3558;
/// The arrows either side of a selected choice or slider (HUD shapes), their colour, and a
/// gouraud quad's size.
const ARROW_LEFT: u32 = 0x8011_20C4;
const ARROW_RIGHT: u32 = 0x8011_20D8;
const ARROW_COLOUR: u32 = 0xBEBE;
const POLY_GT4_SIZE: u32 = 52;
/// A slider: 15 bars, 3 by 5 pixels 5 apart, from 66 right of its text, in a black box; lit
/// (`ARROW_COLOUR` when selected) up to its value, grey beyond.
const SLIDER_BARS: i32 = 15;
const BAR_LIT: u32 = 0x80_8080;
const BAR_OFF: u32 = 0x32_3232;

/// A destructor: the class's vtable put back, then the base class's destructor `base`.
fn destroy(c: &mut Cpu, this: u32, flags: u32, vtable: u32, base: fn(&mut Cpu, u32, u32)) {
    c.m.w32(this.wrapping_add(ITEM_VPTR), vtable);
    base(c, this, flags);
}

/// A root class's destructor: its vtable (its pointer at `vptr`) put back, the object freed
/// when `flags & 1`.
fn destroy_root(c: &mut Cpu, this: u32, flags: u32, vptr: u32, vtable: u32) {
    c.m.w32(this.wrapping_add(vptr), vtable);
    if flags & 1 != 0 {
        call_addr(c, DELETE, &[this, 1]);
    }
}

fn play_sound(c: &mut Cpu, sound: u32) {
    call_addr(c, PLAY_PAUSE_SOUND, &[sound]);
}

/// A half-word argument as the original takes it (sign-extended).
fn short(v: u32) -> u32 {
    v as i16 as i32 as u32
}

// ---- The texts.

/// `PauseMenu_FullText` (PAUSEMENU.CPP, 0x800a7af4): `text` at `x` on the current line in
/// `colour`, its right end or its middle at `x` by `justify`; on the title's line, centred on
/// the screen in the title's colour.
pub fn full_text(c: &mut Cpu, text: u32, x: u32, justify: u32, colour: u32) {
    let (mut x, mut justify, mut colour) = (x, justify, colour);
    if c.m.r32(CURRENT_Y) == TITLE_Y {
        (x, justify, colour) = (160, CENTRED as u32, TITLE_COLOUR);
    }
    match justify as i16 {
        RIGHT => x = x.wrapping_sub(call_addr(c, TEXT_PIXELS, &[text])),
        CENTRED => x = x.wrapping_sub((call_addr(c, TEXT_PIXELS, &[text]) as i32 / 2) as u32),
        _ => {}
    }
    call_addr(c, FONT_TEXT_COLOR, &[short(colour)]);
    let y = c.m.r32(CURRENT_Y);
    call_addr(c, FONT_TEXT_XY, &[text, short(x), y]);
}

pub fn abi_full_text(c: &mut Cpu) {
    full_text(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `PauseMenu_MenuTextPositioned` (PAUSEMENU.CPP, 0x800a7ba8): text `id` at `x` with its own
/// justification, in the selected colour or not (`disabled` goes unused).
pub fn menu_text_positioned(c: &mut Cpu, id: u32, selected: u32, x: u32) {
    let id = short(id);
    let justify = call_addr(c, WORD_FLAGS, &[id]);
    let word = call_addr(c, WORD, &[id]);
    let colour = if selected as u16 != 0 { SELECTED_COLOUR } else { TEXT_COLOUR };
    full_text(c, word, short(x), short(justify), colour);
}

pub fn abi_menu_text_positioned(c: &mut Cpu) {
    menu_text_positioned(c, c.r[4], c.r[5], c.r[7]);
}

/// `PauseMenu_MenuText` (PAUSEMENU.CPP, 0x800a7c20): text `id` at its own place.
pub fn menu_text(c: &mut Cpu, id: u32, selected: u32) {
    let id = short(id);
    let x = call_addr(c, WORD_X, &[id]);
    menu_text_positioned(c, id, short(selected), short(x));
}

pub fn abi_menu_text(c: &mut Cpu) {
    menu_text(c, c.r[4], c.r[5]);
}

// ---- tPListIterator: `*fValue` indexes `fSelectionList`, text ids a 0 ends.

/// The text id at `k` in the iterator's list.
fn list_entry(m: &mut Mem, this: PListIterator, k: u32) -> i16 {
    let list = this.selection_list(m);
    m.r16(list.wrapping_add(k.wrapping_mul(2))) as i16
}

/// After a value changed: the sound, and the menu drawn again.
fn changed(c: &mut Cpu) {
    play_sound(c, SOUND_CHANGE);
    c.m.w32(UPDATE_NEXT_TIME, 1);
}

/// `tPListIterator::tPListIterator` (PAUSEMENU.CPP, 0x800a7c84): over `list`, the value at `value`.
pub fn iterator(m: &mut Mem, this: PListIterator, list: u32, value: u32) -> u32 {
    this.set_vtable(m, ITERATOR_VT);
    this.set_selection_list(m, list);
    this.set_value(m, value);
    this.0
}

pub fn abi_iterator(c: &mut Cpu) {
    c.r[2] = iterator(&mut c.m, PListIterator(c.r[4]), c.r[5], c.r[6]);
}

/// `tPListIterator::~tPListIterator` (PAUSEMENU.CPP, 0x800a7ca0).
pub fn iterator_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITERATOR_VPTR, ITERATOR_VT);
}

pub fn abi_iterator_dtor(c: &mut Cpu) {
    iterator_dtor(c, c.r[4], c.r[5]);
}

/// `tPListIterator::Value` (PAUSEMENU.CPP, 0x800a7cd4).
pub fn value(m: &mut Mem, this: PListIterator) -> u32 {
    let at = this.value(m);
    m.r32(at)
}

pub fn abi_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, PListIterator(c.r[4]));
}

/// `TextValue` (PAUSEMENU.CPP, 0x800a7ce8 and the indexed one's, 0x800a7ed4): the text id the
/// value (`Value`, virtual: its low byte) picks.
pub fn text_value(c: &mut Cpu, this: PListIterator) -> u32 {
    let v = call_virtual(c, this.0, ITERATOR_VPTR, VALUE, &[u32::MAX]);
    list_entry(&mut c.m, this, v & 0xFF) as i32 as u32
}

pub fn abi_text_value(c: &mut Cpu) {
    c.r[2] = text_value(c, PListIterator(c.r[4]));
}

/// The value after `at` steps up: past the list's end, back to 0.
fn step_up(m: &mut Mem, this: PListIterator, at: impl Fn(&mut Mem) -> u32) {
    let p = at(m);
    let v = m.r32(p).wrapping_add(1);
    m.w32(p, v);
    let p = at(m);
    let v = m.r32(p);
    if list_entry(m, this, v) == 0 {
        m.w32(p, 0);
    }
}

/// `tPListIterator::Increment` (PAUSEMENU.CPP, 0x800a7d38): the next entry, round to the first.
pub fn increment(c: &mut Cpu, this: PListIterator) {
    step_up(&mut c.m, this, |m| this.value(m));
    changed(c);
}

pub fn abi_increment(c: &mut Cpu) {
    increment(c, PListIterator(c.r[4]));
}

/// `tPListIterator::Decrement` (PAUSEMENU.CPP, 0x800a7da4): the previous entry, round to the last.
pub fn decrement(c: &mut Cpu, this: PListIterator) {
    let m = &mut c.m;
    let mut at = this.value(m);
    let v = m.r32(at);
    if v != 0 {
        m.w32(at, v.wrapping_sub(1));
    } else if list_entry(m, this, 1) > 0 {
        loop {
            let v = m.r32(at).wrapping_add(1);
            m.w32(at, v);
            at = this.value(m);
            let v = m.r32(at);
            if list_entry(m, this, v.wrapping_add(1)) <= 0 {
                break;
            }
        }
    }
    changed(c);
}

pub fn abi_decrement(c: &mut Cpu) {
    decrement(c, PListIterator(c.r[4]));
}

// ---- tPListIteratorIndexed: the value `fValue[*fIndex]`.

/// Where the indexed iterator's value is.
fn indexed_at(m: &mut Mem, this: PListIteratorIndexed) -> u32 {
    let index = this.index(m);
    let k = m.r8(index) as u32;
    this.value(m).wrapping_add(4 * k)
}

/// `tPListIteratorIndexed::tPListIteratorIndexed` (PAUSEMENU.CPP, 0x800a7e44): over `list`, the
/// values at `values`, the one `*index` picks.
pub fn indexed(m: &mut Mem, this: PListIteratorIndexed, list: u32, values: u32, index: u32) -> u32 {
    iterator(m, this.base(), list, values);
    this.set_vtable(m, INDEXED_VT);
    this.set_index(m, index);
    this.0
}

pub fn abi_indexed(c: &mut Cpu) {
    c.r[2] = indexed(&mut c.m, PListIteratorIndexed(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tPListIteratorIndexed::~tPListIteratorIndexed` (PAUSEMENU.CPP, 0x800a7e88).
pub fn indexed_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, INDEXED_VT, iterator_dtor);
}

pub fn abi_indexed_dtor(c: &mut Cpu) {
    indexed_dtor(c, c.r[4], c.r[5]);
}

/// `tPListIteratorIndexed::Value` (PAUSEMENU.CPP, 0x800a7eb0).
pub fn indexed_value(m: &mut Mem, this: PListIteratorIndexed) -> u32 {
    let at = indexed_at(m, this);
    m.r32(at)
}

pub fn abi_indexed_value(c: &mut Cpu) {
    c.r[2] = indexed_value(&mut c.m, PListIteratorIndexed(c.r[4]));
}

/// `tPListIteratorIndexed::Increment` (PAUSEMENU.CPP, 0x800a7f24).
pub fn indexed_increment(c: &mut Cpu, this: PListIteratorIndexed) {
    step_up(&mut c.m, this.base(), |m| indexed_at(m, this));
    changed(c);
}

pub fn abi_indexed_increment(c: &mut Cpu) {
    indexed_increment(c, PListIteratorIndexed(c.r[4]));
}

/// `tPListIteratorIndexed::Decrement` (PAUSEMENU.CPP, 0x800a7fb0).
pub fn indexed_decrement(c: &mut Cpu, this: PListIteratorIndexed) {
    let m = &mut c.m;
    // (the index's pointer read before the values')
    let (mut index, mut values) = (this.index(m), this.value(m));
    let at = values.wrapping_add(4 * m.r8(index) as u32);
    let v = m.r32(at);
    if v != 0 {
        m.w32(at, v.wrapping_sub(1));
    } else if list_entry(m, this.base(), 1) > 0 {
        loop {
            let at = values.wrapping_add(4 * m.r8(index) as u32);
            let v = m.r32(at).wrapping_add(1);
            m.w32(at, v);
            index = this.index(m);
            let k = m.r8(index) as u32;
            values = this.value(m);
            let v = m.r32(values.wrapping_add(4 * k));
            if list_entry(m, this.base(), v.wrapping_add(1)) <= 0 {
                break;
            }
        }
    }
    changed(c);
}

pub fn abi_indexed_decrement(c: &mut Cpu) {
    indexed_decrement(c, PListIteratorIndexed(c.r[4]));
}

// ---- The menu items.

/// `tPMenuItem::tPMenuItem` (PAUSEMENU.CPP, 0x800a807c): an item showing text `text`, enabled.
pub fn item(m: &mut Mem, this: PMenuItem, text: u32) -> u32 {
    this.set_vtable(m, ITEM_VT);
    this.set_text_description(m, text);
    this.set_flags(m, 0);
    this.0
}

pub fn abi_item(c: &mut Cpu) {
    c.r[2] = item(&mut c.m, PMenuItem(c.r[4]), c.r[5]);
}

/// `tPMenuItem::~tPMenuItem` (PAUSEMENU.CPP, 0x800a8098).
pub fn item_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITEM_VPTR, ITEM_VT);
}

pub fn abi_item_dtor(c: &mut Cpu) {
    item_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItem::NextMenu` (PAUSEMENU.CPP, 0x800a80cc): none.
pub fn abi_item_next_menu(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tPMenuItem::Debounce` (PAUSEMENU.CPP, 0x800a80d4): left and right taken once a press; the
/// sliders' (0x800a8538) repeat while held.
pub fn abi_item_debounce(c: &mut Cpu) {
    c.r[2] = 1;
}

pub fn abi_slider_debounce(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tPMenuItem::ProcessInput` (PAUSEMENU.CPP, 0x800a80dc): nothing.
pub fn abi_item_process_input(_c: &mut Cpu) {}

/// `tPMenuItemInteractive::IsNavigable` (PAUSEMENU.CPP, 0x800a932c): the menu can go to it; a
/// text (0x800a9334) can't be.
pub fn abi_interactive_is_navigable(c: &mut Cpu) {
    c.r[2] = 1;
}

pub fn abi_text_is_navigable(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tPMenuItem::IsEnabled`, `IsDisabled` (PAUSEMENU.H, 0x800a933c, 0x800a9350).
pub fn is_enabled(m: &mut Mem, this: PMenuItem) -> bool {
    this.flags(m) & ITEM_DISABLED == 0
}

pub fn abi_is_enabled(c: &mut Cpu) {
    c.r[2] = is_enabled(&mut c.m, PMenuItem(c.r[4])) as u32;
}

pub fn is_disabled(m: &mut Mem, this: PMenuItem) -> bool {
    this.flags(m) & ITEM_DISABLED != 0
}

pub fn abi_is_disabled(c: &mut Cpu) {
    c.r[2] = is_disabled(&mut c.m, PMenuItem(c.r[4])) as u32;
}

/// An item's text id, as the original loads it (its low half, signed).
fn text_id(m: &mut Mem, this: PMenuItem) -> u32 {
    m.r16(this.0.wrapping_add(4)) as i16 as i32 as u32
}

/// `tPMenuItemNonInteractiveText::tPMenuItemNonInteractiveText` (PAUSEMENU.CPP, 0x800a80e4).
pub fn text(m: &mut Mem, this: PMenuItem, text: u32) -> u32 {
    item(m, this, text);
    this.set_vtable(m, TEXT_VT);
    this.0
}

pub fn abi_text(c: &mut Cpu) {
    c.r[2] = text(&mut c.m, PMenuItem(c.r[4]), c.r[5]);
}

/// `tPMenuItemNonInteractiveText::~tPMenuItemNonInteractiveText` (PAUSEMENU.CPP, 0x800a8118).
pub fn text_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, TEXT_VT, item_dtor);
}

pub fn abi_text_dtor(c: &mut Cpu) {
    text_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItemNonInteractiveText::Draw` (PAUSEMENU.CPP, 0x800a8140): its text, never selected.
pub fn text_draw(c: &mut Cpu, this: PMenuItem) {
    let id = text_id(&mut c.m, this);
    menu_text(c, id, 0);
}

pub fn abi_text_draw(c: &mut Cpu) {
    text_draw(c, PMenuItem(c.r[4]));
}

/// `tPMenuItemInteractive::tPMenuItemInteractive` (PAUSEMENU.CPP, 0x800a8168).
pub fn interactive(m: &mut Mem, this: PMenuItem, text: u32) -> u32 {
    item(m, this, text);
    this.set_vtable(m, INTERACTIVE_VT);
    this.0
}

pub fn abi_interactive(c: &mut Cpu) {
    c.r[2] = interactive(&mut c.m, PMenuItem(c.r[4]), c.r[5]);
}

/// `tPMenuItemInteractive::~tPMenuItemInteractive` (PAUSEMENU.CPP, 0x800a819c).
pub fn interactive_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, INTERACTIVE_VT, item_dtor);
}

pub fn abi_interactive_dtor(c: &mut Cpu) {
    interactive_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItemInteractive::Draw` (PAUSEMENU.CPP, 0x800a81c4): its text, selected or not.
pub fn interactive_draw(c: &mut Cpu, this: PMenuItem, selected: u32) {
    let id = text_id(&mut c.m, this);
    menu_text(c, id, selected);
}

pub fn abi_interactive_draw(c: &mut Cpu) {
    interactive_draw(c, PMenuItem(c.r[4]), c.r[5]);
}

/// `tPMenuItemLeftRightChoice::tPMenuItemLeftRightChoice` (PAUSEMENU.CPP, 0x800a81ec): an item
/// choosing `data`'s value.
pub fn choice(m: &mut Mem, this: PMenuItemLeftRightChoice, text: u32, data: u32) -> u32 {
    interactive(m, this.base().base(), text);
    this.set_vtable(m, CHOICE_VT);
    this.set_data(m, PListIterator(data));
    this.0
}

pub fn abi_choice(c: &mut Cpu) {
    c.r[2] = choice(&mut c.m, PMenuItemLeftRightChoice(c.r[4]), c.r[5], c.r[6]);
}

/// `tPMenuItemLeftRightChoice::~tPMenuItemLeftRightChoice` (PAUSEMENU.CPP, 0x800a8230).
pub fn choice_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, CHOICE_VT, interactive_dtor);
}

pub fn abi_choice_dtor(c: &mut Cpu) {
    choice_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItemLeftRightChoice::ProcessInput` (PAUSEMENU.CPP, 0x800a8258): left and right step
/// the value down and up (the key taken).
pub fn choice_process_input(c: &mut Cpu, this: PMenuItemLeftRightChoice, key: u32) {
    let slot = match c.m.r32(key) {
        KEY_LEFT => DECREMENT,
        KEY_RIGHT => INCREMENT,
        _ => return,
    };
    let data = this.data(&mut c.m).0;
    call_virtual(c, data, ITERATOR_VPTR, slot, &[u32::MAX]);
    c.m.w32(key, KEY_TAKEN);
}

pub fn abi_choice_process_input(c: &mut Cpu) {
    choice_process_input(c, PMenuItemLeftRightChoice(c.r[4]), c.r[5]);
}

/// An arrow either side of a selected value (`shape`), its left end at `x` and its top at `y`.
fn arrow(c: &mut Cpu, shape: u32, x: u32, y: u32) {
    let prim = gpu::add_prim(&mut c.m, POLY_GT4_SIZE);
    crate::hud::build_gt4(&mut c.m, prim, shape, x, y, ARROW_COLOUR);
}

/// `tPMenuItemLeftRightChoice::Draw` (PAUSEMENU.CPP, 0x800a82e4): its text, and its value's at
/// the value's own place; selected (in the first language), arrows either side of the value.
pub fn choice_draw(c: &mut Cpu, this: PMenuItemLeftRightChoice, selected: u32) {
    let item = this.base().base();
    let words = item.text_description(&mut c.m);
    let x = call_addr(c, WORD_X, &[words]);
    let id = text_id(&mut c.m, item);
    menu_text_positioned(c, id, short(selected), short(x));
    let data = this.data(&mut c.m).0;
    let value = short(call_virtual(c, data, ITERATOR_VPTR, TEXT_VALUE, &[u32::MAX]));
    let value_x = short(call_addr(c, WORD_X, &[value]));
    menu_text_positioned(c, value, short(selected), value_x);
    let y = c.m.r32(CURRENT_Y);
    if selected != 0 && c.m.r32(LANGUAGE) == 0 {
        let prim = gpu::add_prim(&mut c.m, POLY_GT4_SIZE);
        let word = call_addr(c, WORD, &[value]);
        let width = call_addr(c, TEXT_PIXELS, &[word]);
        crate::hud::build_gt4(&mut c.m, prim, ARROW_LEFT, value_x.wrapping_sub(width).wrapping_sub(8), y.wrapping_add(5), ARROW_COLOUR);
        arrow(c, ARROW_RIGHT, value_x.wrapping_add(4), y.wrapping_add(5));
    }
}

pub fn abi_choice_draw(c: &mut Cpu) {
    choice_draw(c, PMenuItemLeftRightChoice(c.r[4]), c.r[5]);
}

/// `tPMenuItemLeftRightSlider::tPMenuItemLeftRightSlider` (PAUSEMENU.CPP, 0x800a84bc): a slider
/// of the value at `data`, from 0 to `max`.
pub fn slider(m: &mut Mem, this: PMenuItemLeftRightSlider, text: u32, data: u32, max: u32) -> u32 {
    interactive(m, this.base().base(), text);
    this.set_vtable(m, SLIDER_VT);
    this.set_data(m, data);
    this.set_max_val(m, max as i8);
    this.0
}

pub fn abi_slider(c: &mut Cpu) {
    c.r[2] = slider(&mut c.m, PMenuItemLeftRightSlider(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tPMenuItemLeftRightSlider::~tPMenuItemLeftRightSlider` (PAUSEMENU.CPP, 0x800a8510).
pub fn slider_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, SLIDER_VT, interactive_dtor);
}

pub fn abi_slider_dtor(c: &mut Cpu) {
    slider_dtor(c, c.r[4], c.r[5]);
}

/// A slider's maximum (an unsigned byte).
fn max_of(m: &mut Mem, this: PMenuItemLeftRightSlider) -> i32 {
    this.max_val(m) as u8 as i32
}

/// `tPMenuItemLeftRightSlider::ProcessInput` (PAUSEMENU.CPP, 0x800a8540): left and right move
/// the value a thirtieth of the way (kept from 0 to the maximum; the key taken), the sound if
/// it moved.
pub fn slider_process_input(c: &mut Cpu, this: PMenuItemLeftRightSlider, key: u32) {
    let m = &mut c.m;
    let moved = match m.r32(key) {
        KEY_LEFT => {
            let at = this.data(m);
            if m.r32(at) as i32 > 0 {
                m.w32(UPDATE_NEXT_TIME, 1);
                let step = max_of(m, this) / 30;
                let at = this.data(m);
                let v = (m.r32(at) as i32).wrapping_sub(step).max(0);
                m.w32(at, v as u32);
                true
            } else {
                false
            }
        }
        KEY_RIGHT => {
            let at = this.data(m);
            if (m.r32(at) as i32) < max_of(m, this) {
                m.w32(UPDATE_NEXT_TIME, 1);
                let max = max_of(m, this);
                let at = this.data(m);
                let v = (m.r32(at) as i32).wrapping_add(max / 30).min(max);
                m.w32(at, v as u32);
                true
            } else {
                false
            }
        }
        _ => return,
    };
    m.w32(key, KEY_TAKEN);
    if moved {
        play_sound(c, SOUND_CHANGE);
    }
}

pub fn abi_slider_process_input(c: &mut Cpu) {
    slider_process_input(c, PMenuItemLeftRightSlider(c.r[4]), c.r[5]);
}

/// `tPMenuItemLeftRightSlider::Draw` (PAUSEMENU.CPP, 0x800a865c): its text, then its 15 bars
/// lit up to its value (the arrows either side drawn with each when it is selected, in the
/// first language), in a black box.
pub fn slider_draw(c: &mut Cpu, this: PMenuItemLeftRightSlider, selected: u32) {
    let item = this.base().base();
    let words = item.text_description(&mut c.m);
    let x = call_addr(c, WORD_X, &[words]) as i16 as i32;
    let line = c.m.r16(CURRENT_Y) as i32;
    let id = text_id(&mut c.m, item);
    menu_text_positioned(c, id, short(selected), x as u32);
    let y = line.wrapping_add(4) as i16 as i32;
    for k in 0..SLIDER_BARS {
        let m = &mut c.m;
        let at = this.data(m);
        let lit = crate::fixed::div((m.r32(at) as i32).wrapping_mul(15), max_of(m, this));
        let bar_x = x.wrapping_add(66 + 5 * k);
        let colour = match (k < lit, selected != 0) {
            (false, _) => BAR_OFF,
            (true, false) => BAR_LIT,
            (true, true) => ARROW_COLOUR,
        };
        if selected != 0 && c.m.r32(LANGUAGE) == 0 {
            arrow(c, ARROW_LEFT, x.wrapping_add(53) as u32, y.wrapping_add(2) as u32);
            arrow(c, ARROW_RIGHT, x.wrapping_add(144) as u32, y.wrapping_add(2) as u32);
        }
        call_addr(c, FBUILD_F4, &[0, bar_x as u32, y.wrapping_add(2) as u32, 3, 5, colour, 0, 0]);
    }
    call_addr(c, FBUILD_F4, &[0, x.wrapping_add(63) as u32, y.wrapping_add(1) as u32, 79, 7, 0, 0, 0]);
}

pub fn abi_slider_draw(c: &mut Cpu) {
    slider_draw(c, PMenuItemLeftRightSlider(c.r[4]), c.r[5]);
}

/// `tPMenuItemLeftRightSliderIndexed::tPMenuItemLeftRightSliderIndexed` (PAUSEMENU.CPP,
/// 0x800a8900): a slider of `data[*index]`.
pub fn slider_indexed(m: &mut Mem, this: PMenuItemLeftRightSliderIndexed, text: u32, data: u32, max: u32, index: u32) -> u32 {
    slider(m, this.base(), text, data, max & 0xFF);
    this.set_vtable(m, SLIDER_INDEXED_VT);
    this.set_index(m, index);
    this.0
}

pub fn abi_slider_indexed(c: &mut Cpu) {
    let index = crate::abi::arg(c, 4);
    c.r[2] = slider_indexed(&mut c.m, PMenuItemLeftRightSliderIndexed(c.r[4]), c.r[5], c.r[6], c.r[7], index);
}

/// `tPMenuItemLeftRightSliderIndexed::~tPMenuItemLeftRightSliderIndexed` (PAUSEMENU.CPP, 0x800a8948).
pub fn slider_indexed_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, SLIDER_INDEXED_VT, slider_dtor);
}

pub fn abi_slider_indexed_dtor(c: &mut Cpu) {
    slider_indexed_dtor(c, c.r[4], c.r[5]);
}

/// Run `f` on the indexed slider as a plain slider of the value `*index` picks: its data
/// pointer moved to that value meanwhile, put back after.
fn as_slider(c: &mut Cpu, this: PMenuItemLeftRightSliderIndexed, f: impl FnOnce(&mut Cpu, PMenuItemLeftRightSlider)) {
    let m = &mut c.m;
    let index = this.index(m);
    let k = m.r8(index) as u32;
    let data = this.data(m);
    this.set_data(m, data.wrapping_add(4 * k));
    f(c, this.base());
    this.set_data(&mut c.m, data);
}

/// `tPMenuItemLeftRightSliderIndexed::ProcessInput` (PAUSEMENU.CPP, 0x800a8970).
pub fn slider_indexed_process_input(c: &mut Cpu, this: PMenuItemLeftRightSliderIndexed, key: u32) {
    as_slider(c, this, |c, s| slider_process_input(c, s, key));
}

pub fn abi_slider_indexed_process_input(c: &mut Cpu) {
    slider_indexed_process_input(c, PMenuItemLeftRightSliderIndexed(c.r[4]), c.r[5]);
}

/// `tPMenuItemLeftRightSliderIndexed::Draw` (PAUSEMENU.CPP, 0x800a89bc).
pub fn slider_indexed_draw(c: &mut Cpu, this: PMenuItemLeftRightSliderIndexed, selected: u32) {
    as_slider(c, this, |c, s| slider_draw(c, s, selected));
}

pub fn abi_slider_indexed_draw(c: &mut Cpu) {
    slider_indexed_draw(c, PMenuItemLeftRightSliderIndexed(c.r[4]), c.r[5]);
}

/// `tPMenuItemGoToMenuButton::tPMenuItemGoToMenuButton` (PAUSEMENU.CPP, 0x800a8a08): a button
/// going to `menu`, and calling `on_press` (if any) with the command.
pub fn go_to_menu(m: &mut Mem, this: PMenuItemGoToMenuButton, text: u32, menu: u32, on_press: u32) -> u32 {
    interactive(m, this.base().base(), text);
    this.set_vtable(m, GO_TO_MENU_VT);
    this.set_new_menu(m, PMenu(menu));
    this.set_on_button_press(m, on_press);
    this.0
}

pub fn abi_go_to_menu(c: &mut Cpu) {
    c.r[2] = go_to_menu(&mut c.m, PMenuItemGoToMenuButton(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tPMenuItemGoToMenuButton::~tPMenuItemGoToMenuButton` (PAUSEMENU.CPP, 0x800a8a5c).
pub fn go_to_menu_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, GO_TO_MENU_VT, interactive_dtor);
}

pub fn abi_go_to_menu_dtor(c: &mut Cpu) {
    go_to_menu_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItemGoToMenuButton::NextMenu` (PAUSEMENU.CPP, 0x800a8a84).
pub fn abi_go_to_menu_next_menu(c: &mut Cpu) {
    c.r[2] = PMenuItemGoToMenuButton(c.r[4]).new_menu(&mut c.m).0;
}

/// `tPMenuItemGoToMenuButton::ProcessInput` (PAUSEMENU.CPP, 0x800a8a90): accept goes to its
/// menu (`NextMenu`, virtual, when it has one) and calls its function (the key taken).
pub fn go_to_menu_process_input(c: &mut Cpu, this: PMenuItemGoToMenuButton, key: u32, command: PMenuCommand) {
    if c.m.r32(key) != KEY_ACCEPT {
        return;
    }
    play_sound(c, SOUND_ACCEPT);
    if call_virtual(c, this.0, ITEM_VPTR, NEXT_MENU, &[]) != 0 {
        command.set_type_(&mut c.m, COMMAND_GO_TO as i32);
        let menu = call_virtual(c, this.0, ITEM_VPTR, NEXT_MENU, &[]);
        command.set_next_menu(&mut c.m, PMenu(menu));
    }
    let on_press = this.on_button_press(&mut c.m);
    if on_press != 0 {
        call_addr(c, on_press, &[command.0]);
    }
    c.m.w32(key, KEY_TAKEN);
}

pub fn abi_go_to_menu_process_input(c: &mut Cpu) {
    go_to_menu_process_input(c, PMenuItemGoToMenuButton(c.r[4]), c.r[5], PMenuCommand(c.r[6]));
}

/// `tPMenuItemCommandButton::tPMenuItemCommandButton` (PAUSEMENU.CPP, 0x800a8b44): a button
/// giving `command`.
pub fn command_button(m: &mut Mem, this: PMenuItemCommandButton, text: u32, command: u32) -> u32 {
    interactive(m, this.base().base(), text);
    this.set_vtable(m, COMMAND_VT);
    this.set_command(m, command as i32);
    this.0
}

pub fn abi_command_button(c: &mut Cpu) {
    c.r[2] = command_button(&mut c.m, PMenuItemCommandButton(c.r[4]), c.r[5], c.r[6]);
}

/// `tPMenuItemCommandButton::~tPMenuItemCommandButton` (PAUSEMENU.CPP, 0x800a8b88).
pub fn command_button_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, COMMAND_VT, interactive_dtor);
}

pub fn abi_command_button_dtor(c: &mut Cpu) {
    command_button_dtor(c, c.r[4], c.r[5]);
}

/// `tPMenuItemCommandButton::ProcessInput` (PAUSEMENU.CPP, 0x800a8bb0): accept gives its
/// command (the key taken).
pub fn command_button_process_input(c: &mut Cpu, this: PMenuItemCommandButton, key: u32, command: PMenuCommand) {
    if c.m.r32(key) != KEY_ACCEPT {
        return;
    }
    play_sound(c, SOUND_ACCEPT);
    let m = &mut c.m;
    let v = this.command(m);
    command.set_type_(m, v);
    m.w32(key, KEY_TAKEN);
}

pub fn abi_command_button_process_input(c: &mut Cpu) {
    command_button_process_input(c, PMenuItemCommandButton(c.r[4]), c.r[5], PMenuCommand(c.r[6]));
}

// ---- tPMenu: a title (item 0), then the items, a null after the last (`fNumItems` counts
// them but the title).

/// Item `k`'s address.
fn item_at(m: &mut Mem, this: PMenu, k: u32) -> PMenuItem {
    PMenuItem(m.r32(this.0.wrapping_add(8).wrapping_add(k.wrapping_mul(4))))
}

fn current(m: &mut Mem, this: PMenu) -> u32 {
    this.current_item(m) as u32
}

/// `tPMenu::tPMenuConstructor` (PAUSEMENU.CPP, 0x800a8c10): the items: `first`, then those in
/// the list at `rest` to (and with) a null, counted.
pub fn menu_constructor(m: &mut Mem, this: PMenu, first: u32, rest: u32) {
    let mut at = rest;
    fill(m, this, first, || {
        let it = at;
        at = at.wrapping_add(4);
        it
    });
}

/// The items `first` and those `next` gives the addresses of (read as they are stored).
fn fill(m: &mut Mem, this: PMenu, first: u32, mut next: impl FnMut() -> u32) {
    m.w32(this.0.wrapping_add(8), first);
    this.set_num_items(m, 0);
    let at = next();
    let mut it = m.r32(at);
    m.w32(this.0.wrapping_add(12), it);
    let mut k = 2u32;
    while it != 0 {
        this.update_num_items(m, |n| n.wrapping_add(1));
        let at = next();
        it = m.r32(at);
        m.w32(this.0.wrapping_add(8 + 4 * k), it);
        k = k.wrapping_add(1);
    }
}

pub fn abi_menu_constructor(c: &mut Cpu) {
    menu_constructor(&mut c.m, PMenu(c.r[4]), c.r[5], c.r[6]);
}

/// `tPMenu::tPMenu(first, ...)` (PAUSEMENU.CPP, 0x800a8c5c): a menu of the items given, a
/// null after the last.
pub fn menu(c: &mut Cpu, this: PMenu, items: &[u32]) -> u32 {
    let m = &mut c.m;
    this.set_vtable(m, MENU_VT);
    this.set_current_item(m, 0);
    crate::call::with_buffer(c, 4 * items.len() as u32, |c, list| {
        for (k, &it) in items.iter().enumerate() {
            c.m.w32(list + 4 * k as u32, it);
        }
        menu_constructor(&mut c.m, this, items[0], list + 4);
    });
    this.0
}

/// The variadic constructor from the game: the items in a1, a2, a3 and on the stack from
/// sp+16, homed into the caller's argument area as the original does, to the null.
pub fn abi_menu(c: &mut Cpu) {
    let sp = c.r[29];
    for k in 1..4 {
        c.m.w32(sp + 4 * k as u32, c.r[4 + k]);
    }
    let this = PMenu(c.r[4]);
    let m = &mut c.m;
    this.set_vtable(m, MENU_VT);
    this.set_current_item(m, 0);
    menu_constructor(m, this, c.r[5], sp + 8);
    c.r[2] = this.0;
}

/// `tPMenu::~tPMenu` (PAUSEMENU.CPP, 0x800a8ca8).
pub fn menu_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, MENU_VPTR, MENU_VT);
}

pub fn abi_menu_dtor(c: &mut Cpu) {
    menu_dtor(c, c.r[4], c.r[5]);
}

/// Whether the menu can go to `item`: enabled and navigable (`IsNavigable`, virtual).
fn can_go_to(c: &mut Cpu, item: PMenuItem) -> bool {
    !is_disabled(&mut c.m, item) && call_virtual(c, item.0, ITEM_VPTR, IS_NAVIGABLE, &[]) != 0
}

/// Whether the current item can't be gone to: not navigable (`IsNavigable`, virtual, asked
/// first) or disabled.
fn current_unusable(c: &mut Cpu, this: PMenu) -> bool {
    let k = current(&mut c.m, this);
    let it = item_at(&mut c.m, this, k);
    if call_virtual(c, it.0, ITEM_VPTR, IS_NAVIGABLE, &[]) == 0 {
        return true;
    }
    let k = current(&mut c.m, this);
    let it = item_at(&mut c.m, this, k);
    is_disabled(&mut c.m, it)
}

/// The current item one up: from the first, round to the last (if there is an item after it).
fn step_back(m: &mut Mem, this: PMenu) {
    let k = current(m, this);
    if k as i32 > 0 {
        this.set_current_item(m, k as i32 - 1);
    } else if item_at(m, this, k.wrapping_add(1)).0 != 0 {
        loop {
            let k = current(m, this);
            this.set_current_item(m, k.wrapping_add(1) as i32);
            if item_at(m, this, k.wrapping_add(2)).0 == 0 {
                break;
            }
        }
    }
}

/// `tPMenu::Initialize` (PAUSEMENU.CPP, 0x800a8cdc): the first item the menu can go to made
/// current, the highlight on.
pub fn initialize(c: &mut Cpu, this: PMenu) {
    let m = &mut c.m;
    this.set_current_item(m, 0);
    this.set_highlight(m, 1);
    let k = current(m, this);
    let first = item_at(m, this, k);
    if can_go_to(c, first) {
        return;
    }
    loop {
        let k = current(&mut c.m, this);
        let it = item_at(&mut c.m, this, k);
        if it.0 == 0 || can_go_to(c, it) {
            return;
        }
        this.update_current_item(&mut c.m, |k| k.wrapping_add(1));
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, PMenu(c.r[4]));
}

/// `tPMenu::Debounce` (PAUSEMENU.CPP, 0x800a8ddc): the current item's (`Debounce`, virtual).
pub fn debounce(c: &mut Cpu, this: PMenu) -> u32 {
    let k = current(&mut c.m, this);
    let it = item_at(&mut c.m, this, k);
    call_virtual(c, it.0, ITEM_VPTR, DEBOUNCE, &[])
}

pub fn abi_debounce(c: &mut Cpu) {
    c.r[2] = debounce(c, PMenu(c.r[4]));
}

/// `tPMenu::CheckForDisabled` (PAUSEMENU.CPP, 0x800a8e28): while the current item can't be gone
/// to, the one above it (round from the first to the last; for ever if none can be).
pub fn check_for_disabled(c: &mut Cpu, this: PMenu) {
    while current_unusable(c, this) {
        step_back(&mut c.m, this);
    }
}

pub fn abi_check_for_disabled(c: &mut Cpu) {
    check_for_disabled(c, PMenu(c.r[4]));
}

/// `tPMenu::ProcessInput` (PAUSEMENU.CPP, 0x800a8f30): the current item's first (`ProcessInput`,
/// virtual); then up and down move to the next item the menu can go to (round), start carries
/// on and back goes back; each takes the key.
pub fn process_input(c: &mut Cpu, this: PMenu, key: u32, command: PMenuCommand) {
    let k = current(&mut c.m, this);
    let it = item_at(&mut c.m, this, k);
    if it.0 != 0 {
        call_virtual(c, it.0, ITEM_VPTR, PROCESS_INPUT, &[key, command.0]);
    }
    match c.m.r32(key) {
        KEY_UP => {
            play_sound(c, SOUND_MOVE);
            loop {
                step_back(&mut c.m, this);
                if !current_unusable(c, this) {
                    break;
                }
            }
        }
        KEY_DOWN => {
            play_sound(c, SOUND_MOVE);
            loop {
                let m = &mut c.m;
                let k = current(m, this).wrapping_add(1);
                this.set_current_item(m, k as i32);
                if item_at(m, this, k).0 == 0 {
                    this.set_current_item(m, 0);
                }
                if !current_unusable(c, this) {
                    break;
                }
            }
        }
        KEY_START => {
            play_sound(c, SOUND_ACCEPT);
            command.set_type_(&mut c.m, COMMAND_CONTINUE as i32);
        }
        KEY_BACK => {
            play_sound(c, SOUND_ACCEPT);
            command.set_type_(&mut c.m, COMMAND_BACK as i32);
        }
        _ => return,
    }
    c.m.w32(key, KEY_TAKEN);
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, PMenu(c.r[4]), c.r[5], PMenuCommand(c.r[6]));
}

/// `tPMenu::Draw` (PAUSEMENU.CPP, 0x800a91a4): the current item moved off one it can't be on
/// (`CheckForDisabled`), the title on its line, then the enabled items a line (13) each below,
/// the current one selected while the highlight is on.
pub fn draw(c: &mut Cpu, this: PMenu) {
    check_for_disabled(c, this);
    let title = item_at(&mut c.m, this, 0);
    c.m.w32(CURRENT_Y, TITLE_Y);
    call_virtual(c, title.0, ITEM_VPTR, DRAW, &[0]);
    c.m.w32(CURRENT_Y, 117);
    let mut k: i16 = 1;
    loop {
        let it = item_at(&mut c.m, this, k as i32 as u32);
        if it.0 == 0 {
            return;
        }
        if !is_disabled(&mut c.m, it) {
            let m = &mut c.m;
            let selected = this.highlight(m) != 0 && k as i32 == this.current_item(m);
            call_virtual(c, it.0, ITEM_VPTR, DRAW, &[selected as u32]);
            let y = c.m.r32(CURRENT_Y);
            c.m.w32(CURRENT_Y, y.wrapping_add(13));
        }
        k = k.wrapping_add(1);
    }
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, PMenu(c.r[4]));
}

/// `tPMenu::NumEnabledItems` (PAUSEMENU.CPP, 0x800a9294): the items (but the title) not disabled.
pub fn num_enabled_items(m: &mut Mem, this: PMenu) -> i32 {
    let n = this.num_items(m);
    let mut enabled = n;
    for k in 1..=n.max(0) as u32 {
        let it = item_at(m, this, k);
        if is_disabled(m, it) {
            enabled -= 1;
        }
    }
    enabled
}

pub fn abi_num_enabled_items(c: &mut Cpu) {
    c.r[2] = num_enabled_items(&mut c.m, PMenu(c.r[4])) as u32;
}

/// `tPMenu::ItemEnabledNum` (PAUSEMENU.CPP, 0x800a92e4): item `k`'s line among the enabled ones
/// (the title's counted).
pub fn item_enabled_num(m: &mut Mem, this: PMenu, k: i32) -> i32 {
    let mut line = k;
    for j in 0..k.max(0) as u32 {
        let it = item_at(m, this, j);
        if is_disabled(m, it) {
            line -= 1;
        }
    }
    line
}

pub fn abi_item_enabled_num(c: &mut Cpu) {
    c.r[2] = item_enabled_num(&mut c.m, PMenu(c.r[4]), c.r[5] as i32) as u32;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// The test's structures, in RAM the race leaves zero: a list of text ids, the iterators'
    /// values, the sliders' values, an index byte; an iterator of each class; an item of each
    /// class; two menus, room for new objects, a key, a command, a list of items for the
    /// constructor, a string.
    pub(crate) const LIST: u32 = 0x8014_0000;
    pub(crate) const VALUES: u32 = 0x8014_0100;
    pub(crate) const SLIDER_VALUES: u32 = 0x8014_0180;
    pub(crate) const INDEX: u32 = 0x8014_0200;
    pub(crate) const IT: u32 = 0x8014_0400;
    pub(crate) const IT_INDEXED: u32 = 0x8014_0420;
    pub(crate) const ITEMS: u32 = 0x8014_0800;
    pub(crate) const ITEM_SIZE: u32 = 0x40;
    pub(crate) const MENU: u32 = 0x8014_1000;
    pub(crate) const MENU2: u32 = 0x8014_1100;
    pub(crate) const NEW: u32 = 0x8014_1800;
    pub(crate) const KEY: u32 = 0x8014_2000;
    pub(crate) const COMMAND: u32 = 0x8014_2010;
    pub(crate) const VA: u32 = 0x8014_2100;
    pub(crate) const TEXT: u32 = 0x8014_2200;
    /// A function a button may call: a routine of the test's (interpreted) putting 0x77 in the
    /// command's next menu.
    pub(crate) const ON_PRESS: u32 = 0x8014_2300;
    /// Free RAM in the race image (compared, below the stack) for the primitives: an ordering
    /// table entry and the buffer.
    pub(crate) const OT_ENTRY: u32 = 0x801E_2A00;
    pub(crate) const PRIMS: u32 = 0x801E_3000;
    const PRIM_END: u32 = 0x1F80_0008;
    const MAX_PRIM: u32 = 0x8013_EAA8;

    /// The items' classes: text, interactive, choice, slider, indexed slider, go-to button,
    /// command button.
    const ITEM_VTS: [u32; 7] = [TEXT_VT, INTERACTIVE_VT, CHOICE_VT, SLIDER_VT, SLIDER_INDEXED_VT, GO_TO_MENU_VT, COMMAND_VT];

    pub(crate) fn item_at_k(k: u32) -> u32 {
        ITEMS + ITEM_SIZE * k
    }

    /// The primitives' buffer and ordering table pointed at free RAM full of any bytes (as the
    /// HUD's tests), the line drawn on (now and then the title's), the language.
    pub(crate) fn shuffle_drawing(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(gpu::OT, OT_ENTRY);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        m.w32(PRIM_END, PRIMS + 0x8000);
        m.w32(MAX_PRIM, PRIMS + 0x8000);
        for k in (0..0x2000).step_by(4) {
            m.w32(PRIMS + k, rng.next() as u32);
        }
        m.w32(OT_ENTRY, rng.next() as u32);
        m.w32(CURRENT_Y, if pick(rng, 4) == 0 { TITLE_Y } else { 117 + 13 * pick(rng, 7) });
        m.w32(LANGUAGE, (pick(rng, 3) == 0) as u32);
        m.w32(UPDATE_NEXT_TIME, pick(rng, 2));
    }

    /// The race's RAM with the test's structures: a list of 1 to 6 text ids (now and then a
    /// negative one), values in and past it, the iterators; an item of each class (some
    /// disabled; a text id, an iterator, a slider's value and maximum, a menu and a function, a
    /// command); two menus of a title and a few items, one at least that can be gone to (any
    /// current item, the highlight on or off); a key, a command, a list of items, a string.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle_drawing(c, rng);
        let m = &mut c.m;
        // addiu v0, zero, 0x77; jr ra; sw v0, 4(a0)
        for (k, w) in [0x2402_0077u32, 0x03E0_0008, 0xAC82_0004].into_iter().enumerate() {
            m.w32(ON_PRESS + 4 * k as u32, w);
        }
        let n = 1 + pick(rng, 6);
        for k in 0..n {
            let v = if pick(rng, 8) == 0 { -(pick(rng, 5) as i32) } else { 1 + pick(rng, 40) as i32 };
            m.w16(LIST + 2 * k, v as u16);
        }
        m.w16(LIST + 2 * n, 0);
        for k in 0..8 {
            m.w32(VALUES + 4 * k, pick(rng, n + 2));
        }
        m.w8(INDEX, pick(rng, 4) as u8);
        iterator(m, PListIterator(IT), LIST, VALUES + 4 * pick(rng, 4));
        indexed(m, PListIteratorIndexed(IT_INDEXED), LIST, VALUES, INDEX);
        let max = [1, 2, 29, 30, 31, 127, 255][pick(rng, 7) as usize];
        for k in 0..8 {
            let v = [0, 1, max, max - 1, max + 1, -1i32 as u32, pick(rng, max + 1)][pick(rng, 7) as usize];
            m.w32(SLIDER_VALUES + 4 * k, v);
        }
        for (k, vt) in ITEM_VTS.into_iter().enumerate() {
            let it = item_at_k(k as u32);
            m.w32(it, (pick(rng, 4) == 0) as u32 | (pick(rng, 4) << 8));
            m.w32(it + 4, pick(rng, 40) | if pick(rng, 4) == 0 { 0x1_0000 } else { 0 });
            m.w32(it + 8, vt);
            let data = match vt {
                CHOICE_VT => [IT, IT_INDEXED][pick(rng, 2) as usize],
                SLIDER_VT => SLIDER_VALUES + 4 * pick(rng, 4),
                SLIDER_INDEXED_VT => SLIDER_VALUES,
                GO_TO_MENU_VT => [0, MENU2][pick(rng, 2) as usize],
                COMMAND_VT => pick(rng, 9) | if pick(rng, 2) == 0 { 0x100 } else { 0 },
                _ => rng.next() as u32,
            };
            m.w32(it + 12, data);
            // (a slider's maximum, a button's function; an indexed slider's index after)
            let more = match vt {
                SLIDER_VT | SLIDER_INDEXED_VT => max,
                GO_TO_MENU_VT => [0, ON_PRESS][pick(rng, 2) as usize],
                _ => rng.next() as u32,
            };
            m.w32(it + 16, more);
            m.w32(it + 20, if vt == SLIDER_INDEXED_VT { INDEX } else { rng.next() as u32 });
        }
        for menu in [MENU, MENU2] {
            let count = 1 + pick(rng, 6);
            // (a title first; now and then an item it can be on, for the ways round)
            m.w32(menu + 8, item_at_k(if pick(rng, 4) == 0 { 1 + pick(rng, 6) } else { 0 }));
            let usable = 1 + pick(rng, count);
            for k in 1..=count {
                let class = if k == usable { 1 + pick(rng, 6) } else { pick(rng, 7) };
                let it = item_at_k(class);
                m.w32(menu + 8 + 4 * k, it);
            }
            m.w32(menu + 8 + 4 * (count + 1), 0);
            m.w32(menu + 0x4C, count);
            m.w32(menu, pick(rng, count + 1));
            m.w32(menu + 4, pick(rng, 2));
            m.w32(menu + 0x50, MENU_VT);
        }
        // the one to go to enabled (else the menus can loop for ever looking for one)
        for menu in [MENU, MENU2] {
            let count = m.r32(menu + 0x4C);
            let mut ok = false;
            for k in 1..=count {
                let it = m.r32(menu + 8 + 4 * k);
                ok |= it != item_at_k(0) && m.r32(it) & ITEM_DISABLED == 0;
            }
            if !ok {
                let it = m.r32(menu + 12);
                let first = if it == item_at_k(0) { item_at_k(1) } else { it };
                m.w32(menu + 12, first);
                let flags = m.r32(first);
                m.w32(first, flags & !ITEM_DISABLED);
            }
        }
        m.w32(KEY, [0, KEY_TAKEN, KEY_ACCEPT, KEY_BACK, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_START, 8][pick(rng, 10) as usize]);
        m.w32(COMMAND, pick(rng, 8));
        m.w32(COMMAND + 4, rng.next() as u32);
        let n = pick(rng, 6);
        for k in 0..n {
            m.w32(VA + 4 * k, item_at_k(pick(rng, 7)));
        }
        m.w32(VA + 4 * n, 0);
        let len = pick(rng, 24);
        for k in 0..32 {
            let ch = if k < len { [b'a', b'z', b'A', b'0', b' ', b'.', b'?'][pick(rng, 7) as usize] } else { 0 };
            m.w8(TEXT + k, ch);
        }
    }

    fn x(r: &mut Rng) -> u32 {
        [0, 1, 80, 160, 319, -5i32 as u32, 0x1_0010, r.next() as u32 & 0x1FF][pick(r, 8) as usize]
    }

    fn selected(r: &mut Rng) -> u32 {
        [0, 1, 0x1_0000, 2][pick(r, 4) as usize]
    }

    fn item_any(r: &mut Rng) -> u32 {
        item_at_k(pick(r, 7))
    }

    fn flags(r: &mut Rng) -> u32 {
        [0, 2][pick(r, 2) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("PauseMenu_FullText", 0x800a_7af4, nfs4rc::f_800a7af4, |_, r| vec![TEXT, x(r), pick(r, 4) | (pick(r, 2) << 16), pick(r, 8)]),
            ("PauseMenu_MenuTextPositioned", 0x800a_7ba8, nfs4rc::f_800a7ba8, |_, r| vec![pick(r, 40), selected(r), pick(r, 2), x(r)]),
            ("PauseMenu_MenuText", 0x800a_7c20, nfs4rc::f_800a7c20, |_, r| vec![pick(r, 40) | (pick(r, 2) << 16), selected(r), pick(r, 2)]),
            ("tPListIterator::tPListIterator", 0x800a_7c84, nfs4rc::f_800a7c84, |_, r| vec![NEW, LIST, VALUES + 4 * pick(r, 4)]),
            ("tPListIterator::~tPListIterator", 0x800a_7ca0, nfs4rc::f_800a7ca0, |_, r| vec![IT, flags(r)]),
            ("tPListIterator::Value", 0x800a_7cd4, nfs4rc::f_800a7cd4, |_, _| vec![IT]),
            ("tPListIterator::TextValue", 0x800a_7ce8, nfs4rc::f_800a7ce8, |_, _| vec![IT, u32::MAX]),
            ("tPListIterator::Increment", 0x800a_7d38, nfs4rc::f_800a7d38, |_, _| vec![IT, u32::MAX]),
            ("tPListIterator::Decrement", 0x800a_7da4, nfs4rc::f_800a7da4, |_, _| vec![IT, u32::MAX]),
            ("tPListIteratorIndexed::tPListIteratorIndexed", 0x800a_7e44, nfs4rc::f_800a7e44, |_, _| vec![NEW, LIST, VALUES, INDEX]),
            ("tPListIteratorIndexed::~tPListIteratorIndexed", 0x800a_7e88, nfs4rc::f_800a7e88, |_, r| vec![IT_INDEXED, flags(r)]),
            ("tPListIteratorIndexed::Value", 0x800a_7eb0, nfs4rc::f_800a7eb0, |_, _| vec![IT_INDEXED]),
            ("tPListIteratorIndexed::TextValue", 0x800a_7ed4, nfs4rc::f_800a7ed4, |_, _| vec![IT_INDEXED, u32::MAX]),
            ("tPListIteratorIndexed::Increment", 0x800a_7f24, nfs4rc::f_800a7f24, |_, _| vec![IT_INDEXED, u32::MAX]),
            ("tPListIteratorIndexed::Decrement", 0x800a_7fb0, nfs4rc::f_800a7fb0, |_, _| vec![IT_INDEXED, u32::MAX]),
            ("tPMenuItem::tPMenuItem", 0x800a_807c, nfs4rc::f_800a807c, |_, r| vec![NEW, pick(r, 40)]),
            ("tPMenuItem::~tPMenuItem", 0x800a_8098, nfs4rc::f_800a8098, |_, r| vec![item_any(r), flags(r)]),
            ("tPMenuItem::NextMenu", 0x800a_80cc, nfs4rc::f_800a80cc, |_, r| vec![item_any(r)]),
            ("tPMenuItem::Debounce", 0x800a_80d4, nfs4rc::f_800a80d4, |_, r| vec![item_any(r)]),
            ("tPMenuItem::ProcessInput", 0x800a_80dc, nfs4rc::f_800a80dc, |_, r| vec![item_any(r), KEY, COMMAND]),
            ("tPMenuItemNonInteractiveText::tPMenuItemNonInteractiveText", 0x800a_80e4, nfs4rc::f_800a80e4, |_, r| vec![NEW, pick(r, 40)]),
            ("tPMenuItemNonInteractiveText::~tPMenuItemNonInteractiveText", 0x800a_8118, nfs4rc::f_800a8118, |_, r| vec![item_at_k(0), flags(r)]),
            ("tPMenuItemNonInteractiveText::Draw", 0x800a_8140, nfs4rc::f_800a8140, |_, r| vec![item_any(r), selected(r)]),
            ("tPMenuItemInteractive::tPMenuItemInteractive", 0x800a_8168, nfs4rc::f_800a8168, |_, r| vec![NEW, pick(r, 40)]),
            ("tPMenuItemInteractive::~tPMenuItemInteractive", 0x800a_819c, nfs4rc::f_800a819c, |_, r| vec![item_at_k(1), flags(r)]),
            ("tPMenuItemInteractive::Draw", 0x800a_81c4, nfs4rc::f_800a81c4, |_, r| vec![item_any(r), selected(r)]),
            ("tPMenuItemLeftRightChoice::tPMenuItemLeftRightChoice", 0x800a_81ec, nfs4rc::f_800a81ec, |_, r| vec![NEW, pick(r, 40), [IT, IT_INDEXED][pick(r, 2) as usize]]),
            ("tPMenuItemLeftRightChoice::~tPMenuItemLeftRightChoice", 0x800a_8230, nfs4rc::f_800a8230, |_, r| vec![item_at_k(2), flags(r)]),
            ("tPMenuItemLeftRightChoice::ProcessInput", 0x800a_8258, nfs4rc::f_800a8258, |_, _| vec![item_at_k(2), KEY, COMMAND]),
            ("tPMenuItemLeftRightChoice::Draw", 0x800a_82e4, nfs4rc::f_800a82e4, |_, r| vec![item_at_k(2), selected(r)]),
            ("tPMenuItemLeftRightSlider::tPMenuItemLeftRightSlider", 0x800a_84bc, nfs4rc::f_800a84bc, |_, r| vec![NEW, pick(r, 40), SLIDER_VALUES, pick(r, 0x200)]),
            ("tPMenuItemLeftRightSlider::~tPMenuItemLeftRightSlider", 0x800a_8510, nfs4rc::f_800a8510, |_, r| vec![item_at_k(3), flags(r)]),
            ("tPMenuItemLeftRightSlider::Debounce", 0x800a_8538, nfs4rc::f_800a8538, |_, _| vec![item_at_k(3)]),
            ("tPMenuItemLeftRightSlider::ProcessInput", 0x800a_8540, nfs4rc::f_800a8540, |_, _| vec![item_at_k(3), KEY, COMMAND]),
            ("tPMenuItemLeftRightSlider::Draw", 0x800a_865c, nfs4rc::f_800a865c, |_, r| vec![item_at_k(3), selected(r)]),
            ("tPMenuItemLeftRightSliderIndexed::tPMenuItemLeftRightSliderIndexed", 0x800a_8900, nfs4rc::f_800a8900, |_, r| vec![NEW, pick(r, 40), SLIDER_VALUES, pick(r, 0x200), INDEX]),
            ("tPMenuItemLeftRightSliderIndexed::~tPMenuItemLeftRightSliderIndexed", 0x800a_8948, nfs4rc::f_800a8948, |_, r| vec![item_at_k(4), flags(r)]),
            ("tPMenuItemLeftRightSliderIndexed::ProcessInput", 0x800a_8970, nfs4rc::f_800a8970, |_, _| vec![item_at_k(4), KEY, COMMAND]),
            ("tPMenuItemLeftRightSliderIndexed::Draw", 0x800a_89bc, nfs4rc::f_800a89bc, |_, r| vec![item_at_k(4), selected(r)]),
            ("tPMenuItemGoToMenuButton::tPMenuItemGoToMenuButton", 0x800a_8a08, nfs4rc::f_800a8a08, |_, r| vec![NEW, pick(r, 40), MENU2, [0, ON_PRESS][pick(r, 2) as usize]]),
            ("tPMenuItemGoToMenuButton::~tPMenuItemGoToMenuButton", 0x800a_8a5c, nfs4rc::f_800a8a5c, |_, r| vec![item_at_k(5), flags(r)]),
            ("tPMenuItemGoToMenuButton::NextMenu", 0x800a_8a84, nfs4rc::f_800a8a84, |_, _| vec![item_at_k(5)]),
            ("tPMenuItemGoToMenuButton::ProcessInput", 0x800a_8a90, nfs4rc::f_800a8a90, |_, _| vec![item_at_k(5), KEY, COMMAND]),
            ("tPMenuItemCommandButton::tPMenuItemCommandButton", 0x800a_8b44, nfs4rc::f_800a8b44, |_, r| vec![NEW, pick(r, 40), pick(r, 0x200)]),
            ("tPMenuItemCommandButton::~tPMenuItemCommandButton", 0x800a_8b88, nfs4rc::f_800a8b88, |_, r| vec![item_at_k(6), flags(r)]),
            ("tPMenuItemCommandButton::ProcessInput", 0x800a_8bb0, nfs4rc::f_800a8bb0, |_, _| vec![item_at_k(6), KEY, COMMAND]),
            ("tPMenu::tPMenuConstructor", 0x800a_8c10, nfs4rc::f_800a8c10, |_, r| vec![NEW, item_any(r), VA]),
            ("tPMenu::tPMenu", 0x800a_8c5c, nfs4rc::f_800a8c5c, |_, r| {
                let n = pick(r, 7);
                let mut a = vec![NEW];
                a.extend((0..=n).map(|_| item_any(r)));
                a.push(0);
                a
            }),
            ("tPMenu::~tPMenu", 0x800a_8ca8, nfs4rc::f_800a8ca8, |_, r| vec![MENU, flags(r)]),
            ("tPMenu::Initialize", 0x800a_8cdc, nfs4rc::f_800a8cdc, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize]]),
            ("tPMenu::Debounce", 0x800a_8ddc, nfs4rc::f_800a8ddc, |_, _| vec![MENU]),
            ("tPMenu::CheckForDisabled", 0x800a_8e28, nfs4rc::f_800a8e28, |_, _| vec![MENU]),
            ("tPMenu::ProcessInput", 0x800a_8f30, nfs4rc::f_800a8f30, |_, _| vec![MENU, KEY, COMMAND]),
            // up and down at either end (round the menu)
            ("tPMenu::ProcessInput", 0x800a_8f30, nfs4rc::f_800a8f30, |c, r| {
                let n = c.m.r32(MENU + 0x4C);
                c.m.w32(KEY, [KEY_UP, KEY_DOWN][pick(r, 2) as usize]);
                c.m.w32(MENU, [0, 1, n][pick(r, 3) as usize]);
                vec![MENU, KEY, COMMAND]
            }),
            ("tPMenu::Draw", 0x800a_91a4, nfs4rc::f_800a91a4, |_, _| vec![MENU]),
            ("tPMenu::NumEnabledItems", 0x800a_9294, nfs4rc::f_800a9294, |_, _| vec![MENU]),
            ("tPMenu::ItemEnabledNum", 0x800a_92e4, nfs4rc::f_800a92e4, |c, r| {
                let n = c.m.r32(MENU + 0x4C);
                vec![MENU, pick(r, n + 2)]
            }),
            ("tPMenuItemInteractive::IsNavigable", 0x800a_932c, nfs4rc::f_800a932c, |_, r| vec![item_any(r)]),
            ("tPMenuItemNonInteractiveText::IsNavigable", 0x800a_9334, nfs4rc::f_800a9334, |_, r| vec![item_any(r)]),
            ("tPMenuItem::IsEnabled", 0x800a_933c, nfs4rc::f_800a933c, |_, r| vec![item_any(r)]),
            ("tPMenuItem::IsDisabled", 0x800a_9350, nfs4rc::f_800a9350, |_, r| vec![item_any(r)]),
        ]
    }

    #[test]
    fn pausemenu_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x9A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 300, 0x9A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
