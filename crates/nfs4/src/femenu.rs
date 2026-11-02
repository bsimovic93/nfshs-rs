//! FEMENU.CPP: the front end's menus. Three families of classes:
//! - list iterators: the value a menu item steps through, a byte into a list of text ids
//!   (`tListIterator`), indexed by one or two other bytes, one per player, or a plain range;
//! - menu items: the base (`tMenuItem`), interactive ones, left-right choices and sliders, and
//!   buttons that go to a menu;
//! - menus (`tMenu`): a list of items with a current one that up and down move past disabled
//!   items, and what accept, back, options, help and start do.
//!
//! Each object is a typed view over PS1 RAM (`crate::types`): `item.flags(m)`,
//! `menu.set_current_item(m, 0)`; a derived class's view reaches its base class's members, and
//! `base()` is the base class's view. Each `pub fn` is the original function; `abi_*` is its
//! entry from the game (arguments from the registers, the result in v0). Calls into the game go
//! through `crate::call`, virtual ones through the object's vtable.

use crate::call::{call, call_addr, call_virtual};
use crate::globals::{FE_APP, SUB_MENU_PLAYER};
use crate::types::{
    ListIterator, ListIteratorDoubleIndexed, ListIteratorIndexed, ListIteratorRange, ListIteratorRangeIndexed, Menu, MenuCommand,
    MenuItem, MenuItemGoToMenuButton, MenuItemInteractive, MenuItemLeftRightChoice, MenuItemLeftRightSlider, Screen,
};
use ps1rt::{Cpu, Mem};

/// The vtables.
const LIST_ITERATOR_VT: u32 = 0x8001_1370;
const INDEXED_VT: u32 = 0x8001_1340;
const DOUBLE_INDEXED_VT: u32 = 0x8001_1310;
const MULTI_PLAYER_VT: u32 = 0x8001_12E0;
const RANGE_VT: u32 = 0x8001_12B0;
const RANGE_INDEXED_VT: u32 = 0x8001_1280;
const MENU_ITEM_VT: u32 = 0x8001_1228;
const INTERACTIVE_VT: u32 = 0x8001_11D0;
const LEFT_RIGHT_CHOICE_VT: u32 = 0x8001_1178;
const LEFT_RIGHT_SLIDER_VT: u32 = 0x8001_1120;
const GO_TO_MENU_BUTTON_VT: u32 = 0x8001_10C8;
const MENU_VT: u32 = 0x8001_1070;

/// Where an iterator's, an item's and a menu's vtable pointer is.
pub(crate) const ITERATOR_VPTR: u32 = 12;
pub(crate) const ITEM_VPTR: u32 = 24;
const MENU_VPTR: u32 = 104;
/// The iterators' virtual functions (their vtable entries' offsets): `Value`, `TextValue`,
/// `Increment`, `Decrement`.
pub(crate) const VALUE: u32 = 16;
pub(crate) const TEXT_VALUE: u32 = 24;
pub(crate) const INCREMENT: u32 = 32;
pub(crate) const DECREMENT: u32 = 40;
/// The menu items': `DebounceKeys`, `ProcessInput`, `Draw(bool)`, `Draw` at a place,
/// `UpdateTransition`.
pub(crate) const DEBOUNCE_KEYS: u32 = 16;
pub(crate) const PROCESS_INPUT: u32 = 24;
pub(crate) const DRAW: u32 = 32;
pub(crate) const DRAW_AT: u32 = 48;
pub(crate) const UPDATE_TRANSITION: u32 = 80;

/// The input keys a menu reacts to (`tInputKeyType`): back is Triangle, start is Start.
pub(crate) const KEY_NONE: u32 = 1;
pub(crate) const KEY_ACCEPT: u32 = 2;
pub(crate) const KEY_HELP: u32 = 4;
pub(crate) const KEY_OPTIONS: u32 = 8;
pub(crate) const KEY_BACK: u32 = 16;
pub(crate) const KEY_UP: u32 = 512;
pub(crate) const KEY_DOWN: u32 = 1024;
pub(crate) const KEY_LEFT: u32 = 2048;
pub(crate) const KEY_RIGHT: u32 = 4096;
pub(crate) const KEY_START: u32 = 8192;
/// A menu command's types (`tMenuCommandType`, FEAPP's `MainLoop` runs them): go to a menu
/// (pushed on the back stack); go to it with the stack emptied; go to it with the second
/// player's menu (its child) up beside it; back; race; race once both players are ready; race
/// (a second command doing the same); quit the front end.
pub(crate) const COMMAND_GO_TO: u32 = 1;
pub(crate) const COMMAND_GO_TO_RESET: u32 = 2;
pub(crate) const COMMAND_GO_TO_WITH_SECOND: u32 = 3;
pub(crate) const COMMAND_BACK: u32 = 4;
pub(crate) const COMMAND_RACE: u32 = 5;
pub(crate) const COMMAND_RACE_WHEN_BOTH_READY: u32 = 6;
pub(crate) const COMMAND_RACE_TOO: u32 = 7;
pub(crate) const COMMAND_QUIT: u32 = 8;

/// A menu item's flags: disabled; a button whose menu is gone to with the back stack emptied;
/// a slider; a left-right choice.
pub(crate) const ITEM_DISABLED: u32 = 1;
const ITEM_GO_RESETS: u32 = 0x40;
const ITEM_SLIDER: u32 = 0x80;
const ITEM_LEFT_RIGHT: u32 = 0x400;
/// A menu's flags: start races, or races once both players are ready; start is taken as accept;
/// accept as start.
const MENU_START_RACES: u32 = 1;
const MENU_START_RACES_WHEN_BOTH_READY: u32 = 2;
const MENU_START_ACCEPTS: u32 = 4;
const MENU_ACCEPT_STARTS: u32 = 0x1_0000;

/// The front end's sounds: up, down, left, right (`AudioCmn_PlayFESFX`); the slider's tick
/// (`AudioCmn_PlayFESFXVol`, hooked by the host: called by address).
pub(crate) const PLAY_FE_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;
const PLAY_FE_SFX_VOL: u32 = 0x8007_A0AC;
const SOUND_UP: u32 = 3;
const SOUND_DOWN: u32 = 4;
const SOUND_LEFT: u32 = 5;
const SOUND_RIGHT: u32 = 6;
const SOUND_TICK: u32 = 21;
/// The library's `__builtin_delete`.
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;
/// `TextSys_WordX`, `TextSys_WordY`: a text id's place on the screen.
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// `FETextRender_MenuTextPositioned(text, x, y, selected, type)`, `FETextRender_Title`.
const MENU_TEXT_POSITIONED: fn(&mut Cpu) = crate::fetextrender::menu_text_positioned_entry;
pub(crate) const TITLE: fn(&mut Cpu) = crate::fetextrender::title_entry;
/// The text type of a menu item's text and value.
const ITEM_TEXT: u32 = 6;
/// `tFEApplication::DisplayHelp(app, page)`.
const DISPLAY_HELP: fn(&mut Cpu) = nfs4rc::f_80014048;
/// `DrawSlider(value, min, max, x, y, w, h, ...)`, hooked by the host: called by address.
const DRAW_SLIDER: u32 = 0x8002_4C4C;

/// A destructor: the class's vtable put back (its pointer at `vptr`), then the base class's
/// destructor `base` with the caller's `flags`.
fn destroy(c: &mut Cpu, this: u32, flags: u32, vptr: u32, vtable: u32, base: fn(&mut Cpu, u32, u32)) {
    c.m.w32(this.wrapping_add(vptr), vtable);
    base(c, this, flags);
}

/// A root class's destructor: its vtable put back, the object freed when `flags & 1`.
fn destroy_root(c: &mut Cpu, this: u32, flags: u32, vptr: u32, vtable: u32) {
    c.m.w32(this.wrapping_add(vptr), vtable);
    if flags & 1 != 0 {
        call(c, DELETE, &[this, 1]);
    }
}

// ---- tListIterator: the value is a byte `*fValue`, an index into `fSelectionList`, a list of
// text ids that a 0 ends.

/// The text id at `k` in the iterator's list.
fn list_entry(m: &mut Mem, this: ListIterator, k: u32) -> i32 {
    let list = this.selection_list(m);
    m.r16(list.wrapping_add(k.wrapping_mul(2))) as i16 as i32
}

/// `tListIterator::tListIterator` (FEMENU.CPP, 0x80023df0): over `list`, the value at `value`.
pub fn list_iterator(m: &mut Mem, this: ListIterator, list: u32, value: u32) -> u32 {
    this.set_vtable(m, LIST_ITERATOR_VT);
    this.set_selection_list(m, list);
    this.set_value(m, value);
    this.set_max_value(m, 0);
    this.set_min_value(m, 0);
    this.0
}

pub fn abi_list_iterator(c: &mut Cpu) {
    c.r[2] = list_iterator(&mut c.m, ListIterator(c.r[4]), c.r[5], c.r[6]);
}

/// `tListIterator::~tListIterator` (FEMENU.CPP, 0x80023e14).
pub fn list_iterator_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITERATOR_VPTR, LIST_ITERATOR_VT);
}

pub fn abi_list_iterator_dtor(c: &mut Cpu) {
    list_iterator_dtor(c, c.r[4], c.r[5]);
}

/// `tListIterator::Value` (FEMENU.CPP, 0x80023e48): the value.
pub fn value(m: &mut Mem, this: ListIterator) -> u32 {
    let at = this.value(m);
    m.r8(at) as u32
}

pub fn abi_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, ListIterator(c.r[4]));
}

/// Every iterator's `TextValue`: the text id at its value (its virtual `Value` for `player`).
fn text_value_of(c: &mut Cpu, this: ListIterator, player: u32) -> u32 {
    let v = call_virtual(c, this.0, ITERATOR_VPTR, VALUE, &[player]);
    list_entry(&mut c.m, this, v & 0xFF) as u32
}

/// `tListIterator::TextValue` (FEMENU.CPP, 0x80023e5c): the value's text id.
pub fn text_value(c: &mut Cpu, this: ListIterator) -> u32 {
    text_value_of(c, this, u32::MAX)
}

pub fn abi_text_value(c: &mut Cpu) {
    c.r[2] = text_value(c, ListIterator(c.r[4]));
}

/// The next value in the list, round to the first after the last: every `Increment`. `at`
/// gives the value's place (read again after the step, as the original does).
fn step_up(m: &mut Mem, this: ListIterator, at: impl Fn(&mut Mem) -> u32) {
    let a = at(m);
    let v = m.r8(a).wrapping_add(1);
    m.w8(a, v);
    let a = at(m);
    let k = m.r8(a) as u32;
    if list_entry(m, this, k) == 0 {
        m.w8(a, 0);
    }
}

/// The previous value, round to the last: every `Decrement`. `first` is the value's place as
/// first read, `at` gives it again.
fn step_down(m: &mut Mem, this: ListIterator, first: u32, at: impl Fn(&mut Mem) -> u32) {
    let v = m.r8(first);
    if v != 0 {
        m.w8(first, v - 1);
        return;
    }
    if list_entry(m, this, 1) <= 0 {
        return;
    }
    // from the first, up to the last
    let mut a = first;
    loop {
        let v = m.r8(a).wrapping_add(1);
        m.w8(a, v);
        a = at(m);
        let k = m.r8(a) as u32;
        if list_entry(m, this, k + 1) <= 0 {
            break;
        }
    }
}

/// `tListIterator::Increment` (FEMENU.CPP, 0x80023eac): the next text.
pub fn increment(m: &mut Mem, this: ListIterator) {
    step_up(m, this, |m| this.value(m));
}

pub fn abi_increment(c: &mut Cpu) {
    increment(&mut c.m, ListIterator(c.r[4]));
}

/// `tListIterator::Decrement` (FEMENU.CPP, 0x80023ef8): the previous text.
pub fn decrement(m: &mut Mem, this: ListIterator) {
    let first = this.value(m);
    step_down(m, this, first, |m| this.value(m));
}

pub fn abi_decrement(c: &mut Cpu) {
    decrement(&mut c.m, ListIterator(c.r[4]));
}

// ---- tListIteratorIndexed: the value is `fValue[*fIndex]`.

/// The value's place `fValue[*index]`, for an iterator indexed by the byte at `index`.
fn at_index(m: &mut Mem, this: ListIterator, index: u32) -> u32 {
    let k = m.r8(index) as u32;
    this.value(m).wrapping_add(k)
}

fn indexed_at(m: &mut Mem, this: ListIteratorIndexed) -> u32 {
    let index = this.index(m);
    at_index(m, this.base(), index)
}

/// `tListIteratorIndexed::tListIteratorIndexed` (FEMENU.CPP, 0x80023f80): over `list`, the
/// values at `value`, the one at `*index`.
pub fn indexed(c: &mut Cpu, this: ListIteratorIndexed, list: u32, value: u32, index: u32) -> u32 {
    let m = &mut c.m;
    list_iterator(m, this.base(), list, value);
    this.set_vtable(m, INDEXED_VT);
    this.set_index(m, index);
    this.0
}

pub fn abi_indexed(c: &mut Cpu) {
    c.r[2] = indexed(c, ListIteratorIndexed(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tListIteratorIndexed::~tListIteratorIndexed` (FEMENU.CPP, 0x80023fc4).
pub fn indexed_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, ITERATOR_VPTR, INDEXED_VT, list_iterator_dtor);
}

pub fn abi_indexed_dtor(c: &mut Cpu) {
    indexed_dtor(c, c.r[4], c.r[5]);
}

/// `tListIteratorIndexed::Value` (FEMENU.CPP, 0x80023fec).
pub fn indexed_value(m: &mut Mem, this: ListIteratorIndexed) -> u32 {
    let at = indexed_at(m, this);
    m.r8(at) as u32
}

pub fn abi_indexed_value(c: &mut Cpu) {
    c.r[2] = indexed_value(&mut c.m, ListIteratorIndexed(c.r[4]));
}

/// `tListIteratorIndexed::TextValue` (FEMENU.CPP, 0x80024010).
pub fn abi_indexed_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, ListIterator(c.r[4]), u32::MAX);
}

/// `tListIteratorIndexed::Increment` (FEMENU.CPP, 0x80024060).
pub fn indexed_increment(m: &mut Mem, this: ListIteratorIndexed) {
    step_up(m, this.base(), |m| indexed_at(m, this));
}

pub fn abi_indexed_increment(c: &mut Cpu) {
    indexed_increment(&mut c.m, ListIteratorIndexed(c.r[4]));
}

/// `tListIteratorIndexed::Decrement` (FEMENU.CPP, 0x800240c8).
pub fn indexed_decrement(m: &mut Mem, this: ListIteratorIndexed) {
    let first = indexed_at(m, this);
    step_down(m, this.base(), first, |m| indexed_at(m, this));
}

pub fn abi_indexed_decrement(c: &mut Cpu) {
    indexed_decrement(&mut c.m, ListIteratorIndexed(c.r[4]));
}

// ---- tListIteratorDoubleIndexed: the value is `fValue[*fIndex1 * index1multiplier + *fIndex2]`.

fn double_indexed_at(m: &mut Mem, this: ListIteratorDoubleIndexed) -> u32 {
    let index1 = this.index1(m);
    let row = (m.r8(index1) as u32).wrapping_mul(this.index1multiplier(m) as u32);
    let index2 = this.index2(m);
    let column = m.r8(index2) as u32;
    this.value(m).wrapping_add(row.wrapping_add(column))
}

/// `tListIteratorDoubleIndexed::~tListIteratorDoubleIndexed` (FEMENU.CPP, 0x80024174).
pub fn abi_double_indexed_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITERATOR_VPTR, DOUBLE_INDEXED_VT, list_iterator_dtor);
}

/// `tListIteratorDoubleIndexed::Value` (FEMENU.CPP, 0x8002419c).
pub fn double_indexed_value(m: &mut Mem, this: ListIteratorDoubleIndexed) -> u32 {
    let at = double_indexed_at(m, this);
    m.r8(at) as u32
}

pub fn abi_double_indexed_value(c: &mut Cpu) {
    c.r[2] = double_indexed_value(&mut c.m, ListIteratorDoubleIndexed(c.r[4]));
}

/// `tListIteratorDoubleIndexed::TextValue` (FEMENU.CPP, 0x800241d8).
pub fn abi_double_indexed_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, ListIterator(c.r[4]), u32::MAX);
}

/// `tListIteratorDoubleIndexed::Increment` (FEMENU.CPP, 0x80024228).
pub fn double_indexed_increment(m: &mut Mem, this: ListIteratorDoubleIndexed) {
    step_up(m, this.base(), |m| double_indexed_at(m, this));
}

pub fn abi_double_indexed_increment(c: &mut Cpu) {
    double_indexed_increment(&mut c.m, ListIteratorDoubleIndexed(c.r[4]));
}

/// `tListIteratorDoubleIndexed::Decrement` (FEMENU.CPP, 0x800242c4).
pub fn double_indexed_decrement(m: &mut Mem, this: ListIteratorDoubleIndexed) {
    let first = double_indexed_at(m, this);
    step_down(m, this.base(), first, |m| double_indexed_at(m, this));
}

pub fn abi_double_indexed_decrement(c: &mut Cpu) {
    double_indexed_decrement(&mut c.m, ListIteratorDoubleIndexed(c.r[4]));
}

// ---- tListIteratorMultiPlayer: a value per player, `fValue[player]` (player -1, any of them:
// the first's).

fn multi_player_at(m: &mut Mem, this: ListIterator, player: u32) -> u32 {
    let k = if player == u32::MAX { 0 } else { player };
    this.value(m).wrapping_add(k)
}

/// `tListIteratorMultiPlayer::~tListIteratorMultiPlayer` (FEMENU.CPP, 0x800243b0).
pub fn abi_multi_player_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITERATOR_VPTR, MULTI_PLAYER_VT, list_iterator_dtor);
}

/// `tListIteratorMultiPlayer::Value` (FEMENU.CPP, 0x800243d8): `player`'s value.
pub fn multi_player_value(m: &mut Mem, this: ListIterator, player: u32) -> u32 {
    let at = multi_player_at(m, this, player);
    m.r8(at) as u32
}

pub fn abi_multi_player_value(c: &mut Cpu) {
    c.r[2] = multi_player_value(&mut c.m, ListIterator(c.r[4]), c.r[5]);
}

/// `tListIteratorMultiPlayer::TextValue` (FEMENU.CPP, 0x80024400): `player`'s text id.
pub fn abi_multi_player_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, ListIterator(c.r[4]), c.r[5]);
}

/// `tListIteratorMultiPlayer::Increment` (FEMENU.CPP, 0x80024450).
pub fn multi_player_increment(m: &mut Mem, this: ListIterator, player: u32) {
    step_up(m, this, |m| multi_player_at(m, this, player));
}

pub fn abi_multi_player_increment(c: &mut Cpu) {
    multi_player_increment(&mut c.m, ListIterator(c.r[4]), c.r[5]);
}

/// `tListIteratorMultiPlayer::Decrement` (FEMENU.CPP, 0x800244b4).
pub fn multi_player_decrement(m: &mut Mem, this: ListIterator, player: u32) {
    let first = multi_player_at(m, this, player);
    step_down(m, this, first, |m| multi_player_at(m, this, player));
}

pub fn abi_multi_player_decrement(c: &mut Cpu) {
    multi_player_decrement(&mut c.m, ListIterator(c.r[4]), c.r[5]);
}

// ---- tListIteratorRange: a value from `fMinValue` to `fMaxValue` (no list, no text).

/// `tListIteratorRange::tListIteratorRange` (FEMENU.CPP, 0x80024554).
pub fn range(c: &mut Cpu, this: ListIteratorRange, min: u32, max: u32, value: u32) -> u32 {
    let m = &mut c.m;
    list_iterator(m, this.base(), 0, value);
    this.set_vtable(m, RANGE_VT);
    this.set_min_value(m, min as u8);
    this.set_max_value(m, max as u8);
    this.0
}

pub fn abi_range(c: &mut Cpu) {
    c.r[2] = range(c, ListIteratorRange(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tListIteratorRange::~tListIteratorRange` (FEMENU.CPP, 0x800245b0).
pub fn range_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, ITERATOR_VPTR, RANGE_VT, list_iterator_dtor);
}

pub fn abi_range_dtor(c: &mut Cpu) {
    range_dtor(c, c.r[4], c.r[5]);
}

/// `tListIteratorRange::Value` (FEMENU.CPP, 0x800245d8).
pub fn abi_range_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, ListIterator(c.r[4]));
}

/// `tListIteratorRange::TextValue` (FEMENU.CPP, 0x800245ec): none.
pub fn abi_range_text_value(c: &mut Cpu) {
    c.r[2] = 0;
}

/// One up, to the maximum.
fn range_up(m: &mut Mem, this: ListIteratorRange, at: u32) {
    let v = m.r8(at);
    if v < this.max_value(m) {
        m.w8(at, v + 1);
    }
}

/// One down, to the minimum.
fn range_down(m: &mut Mem, this: ListIteratorRange, at: u32) {
    let v = m.r8(at);
    if this.min_value(m) < v {
        m.w8(at, v - 1);
    }
}

/// `tListIteratorRange::Increment` (FEMENU.CPP, 0x800245f4).
pub fn range_increment(m: &mut Mem, this: ListIteratorRange) {
    let at = this.value(m);
    range_up(m, this, at);
}

pub fn abi_range_increment(c: &mut Cpu) {
    range_increment(&mut c.m, ListIteratorRange(c.r[4]));
}

/// `tListIteratorRange::Decrement` (FEMENU.CPP, 0x8002461c).
pub fn range_decrement(m: &mut Mem, this: ListIteratorRange) {
    let at = this.value(m);
    range_down(m, this, at);
}

pub fn abi_range_decrement(c: &mut Cpu) {
    range_decrement(&mut c.m, ListIteratorRange(c.r[4]));
}

// ---- tListIteratorRangeIndexed: a range, the value `fValue[*fIndex]`.

fn range_indexed_at(m: &mut Mem, this: ListIteratorRangeIndexed) -> u32 {
    let index = this.index(m);
    at_index(m, this.base().base(), index)
}

/// `tListIteratorRangeIndexed::tListIteratorRangeIndexed` (FEMENU.CPP, 0x80024644).
pub fn range_indexed(c: &mut Cpu, this: ListIteratorRangeIndexed, min: u32, max: u32, value: u32, index: u32) -> u32 {
    range(c, this.base(), min & 0xFF, max & 0xFF, value);
    this.set_vtable(&mut c.m, RANGE_INDEXED_VT);
    this.set_index(&mut c.m, index);
    this.0
}

pub fn abi_range_indexed(c: &mut Cpu) {
    let index = crate::abi::arg(c, 4);
    c.r[2] = range_indexed(c, ListIteratorRangeIndexed(c.r[4]), c.r[5], c.r[6], c.r[7], index);
}

/// `tListIteratorRangeIndexed::~tListIteratorRangeIndexed` (FEMENU.CPP, 0x80024690).
pub fn abi_range_indexed_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITERATOR_VPTR, RANGE_INDEXED_VT, range_dtor);
}

/// `tListIteratorRangeIndexed::Value` (FEMENU.CPP, 0x800246b8).
pub fn range_indexed_value(m: &mut Mem, this: ListIteratorRangeIndexed) -> u32 {
    let at = range_indexed_at(m, this);
    m.r8(at) as u32
}

pub fn abi_range_indexed_value(c: &mut Cpu) {
    c.r[2] = range_indexed_value(&mut c.m, ListIteratorRangeIndexed(c.r[4]));
}

/// `tListIteratorRangeIndexed::Increment` (FEMENU.CPP, 0x800246dc).
pub fn range_indexed_increment(m: &mut Mem, this: ListIteratorRangeIndexed) {
    let at = range_indexed_at(m, this);
    range_up(m, this.base(), at);
}

pub fn abi_range_indexed_increment(c: &mut Cpu) {
    range_indexed_increment(&mut c.m, ListIteratorRangeIndexed(c.r[4]));
}

/// `tListIteratorRangeIndexed::Decrement` (FEMENU.CPP, 0x80024718).
pub fn range_indexed_decrement(m: &mut Mem, this: ListIteratorRangeIndexed) {
    let at = range_indexed_at(m, this);
    range_down(m, this.base(), at);
}

pub fn abi_range_indexed_decrement(c: &mut Cpu) {
    range_indexed_decrement(&mut c.m, ListIteratorRangeIndexed(c.r[4]));
}

// ---- tMenuItem: its flags (`ITEM_*`), its text id, the selection's fade (0..128).

/// `tMenuItem::tMenuItem` (FEMENU.CPP, 0x80024754): an item showing `text`.
pub fn menu_item(m: &mut Mem, this: MenuItem, text: u32) -> u32 {
    this.set_vtable(m, MENU_ITEM_VT);
    this.set_text_description(m, text);
    this.set_flags(m, 0);
    this.set_sel_fade(m, 0);
    this.set_button_image(m, -1);
    this.set_num_frames(m, 0);
    this.set_new_menu(m, Menu(0));
    this.0
}

pub fn abi_menu_item(c: &mut Cpu) {
    c.r[2] = menu_item(&mut c.m, MenuItem(c.r[4]), c.r[5]);
}

/// `tMenuItem::~tMenuItem` (FEMENU.CPP, 0x80024784).
pub fn menu_item_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITEM_VPTR, MENU_ITEM_VT);
}

pub fn abi_menu_item_dtor(c: &mut Cpu) {
    menu_item_dtor(c, c.r[4], c.r[5]);
}

/// `tMenuItem::DebounceKeys` (FEMENU.CPP, 0x800247b8): every key debounced (-1).
pub fn abi_menu_item_debounce_keys(c: &mut Cpu) {
    c.r[2] = u32::MAX;
}

/// `tMenuItem::ProcessInput`, `TransitionOn`, `TransitionOff` (FEMENU.CPP, 0x800247c0,
/// 0x80025ac8, 0x80025ad0), `tMenu::TransitionOff`, `TransitionOn` (0x80025a5c, 0x80025a64):
/// nothing.
pub fn abi_nothing(_: &mut Cpu) {}

/// `tMenuItem::UpdateTransition` (FEMENU.CPP, 0x800247c8): the selection's fade.
pub fn menu_item_update_transition(c: &mut Cpu, this: MenuItem, selected: u32) {
    update_sel_fade(&mut c.m, this, selected);
}

pub fn abi_menu_item_update_transition(c: &mut Cpu) {
    menu_item_update_transition(c, MenuItem(c.r[4]), c.r[5]);
}

/// `tMenuItem::TransitionIsFinished`, `tMenu::TransitionIsFinished` (FEMENU.CPP, 0x800247e8,
/// 0x80025a6c): always.
pub fn abi_finished(c: &mut Cpu) {
    c.r[2] = 1;
}

/// `tMenuItem::UpdateSelFade` (FEMENU.CPP, 0x800247f0): the selection fades in (64 a frame)
/// while selected, out (10) when not, within 0..128.
pub fn update_sel_fade(m: &mut Mem, this: MenuItem, selected: u32) {
    let fade = this.sel_fade(m);
    this.set_sel_fade(m, if selected != 0 { fade.wrapping_add(64) } else { fade.wrapping_sub(10) });
    if this.sel_fade(m) > 128 {
        this.set_sel_fade(m, 128);
    }
    if this.sel_fade(m) < 0 {
        this.set_sel_fade(m, 0);
    }
}

pub fn abi_update_sel_fade(c: &mut Cpu) {
    update_sel_fade(&mut c.m, MenuItem(c.r[4]), c.r[5]);
}

/// `tMenuItem::Draw(int, int, bool)` (FEMENU.CPP, 0x80024848): the plain `Draw(bool)`; the
/// place given isn't used.
pub fn abi_menu_item_draw_at(c: &mut Cpu) {
    let (this, selected) = (c.r[4], c.r[7]);
    call_virtual(c, this, ITEM_VPTR, DRAW, &[selected]);
}

/// `tMenuItem::Draw(int, int, int, bool)` (FEMENU.CPP, 0x80024884): the virtual `Draw` at a
/// place, given the bool first and a 0 last.
pub fn abi_menu_item_draw_at3(c: &mut Cpu) {
    let (this, a, b) = (c.r[4], c.r[5], c.r[6]);
    let selected = crate::abi::arg(c, 4);
    call_virtual(c, this, ITEM_VPTR, DRAW_AT, &[selected, a, b, 0]);
}

/// `tMenuItemInteractive::tMenuItemInteractive` (FEMENU.CPP, 0x800248c8).
pub fn interactive(c: &mut Cpu, this: MenuItemInteractive, text: u32) -> u32 {
    menu_item(&mut c.m, this.base(), text);
    this.set_vtable(&mut c.m, INTERACTIVE_VT);
    this.0
}

pub fn abi_interactive(c: &mut Cpu) {
    c.r[2] = interactive(c, MenuItemInteractive(c.r[4]), c.r[5]);
}

/// `tMenuItemInteractive::~tMenuItemInteractive` (FEMENU.CPP, 0x800248fc).
pub fn interactive_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy(c, this, flags, ITEM_VPTR, INTERACTIVE_VT, menu_item_dtor);
}

pub fn abi_interactive_dtor(c: &mut Cpu) {
    interactive_dtor(c, c.r[4], c.r[5]);
}

/// `tMenuItemLeftRightChoice::tMenuItemLeftRightChoice` (FEMENU.CPP, 0x80024924): an item
/// choosing through `data` with left and right.
pub fn left_right_choice(c: &mut Cpu, this: MenuItemLeftRightChoice, text: u32, data: ListIterator) -> u32 {
    interactive(c, this.base(), text);
    let m = &mut c.m;
    let flags = this.flags(m);
    this.set_vtable(m, LEFT_RIGHT_CHOICE_VT);
    this.set_data(m, data);
    this.set_flags(m, flags | ITEM_LEFT_RIGHT);
    this.0
}

pub fn abi_left_right_choice(c: &mut Cpu) {
    c.r[2] = left_right_choice(c, MenuItemLeftRightChoice(c.r[4]), c.r[5], ListIterator(c.r[6]));
}

/// `tMenuItemLeftRightChoice::~tMenuItemLeftRightChoice` (FEMENU.CPP, 0x80024974).
pub fn abi_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITEM_VPTR, LEFT_RIGHT_CHOICE_VT, interactive_dtor);
}

/// Left or right on an enabled item steps its iterator (`data`, read after the key) down or up
/// for `player`. Whether it did.
fn left_right(c: &mut Cpu, this: MenuItemInteractive, data: impl Fn(&mut Mem) -> ListIterator, player: u32, key: u32) -> bool {
    let m = &mut c.m;
    if this.flags(m) & ITEM_DISABLED != 0 {
        return false;
    }
    let step = match m.r32(key) {
        KEY_LEFT => DECREMENT,
        KEY_RIGHT => INCREMENT,
        _ => return false,
    };
    let data = data(m);
    call_virtual(c, data.0, ITERATOR_VPTR, step, &[player]);
    true
}

/// `tMenuItemLeftRightChoice::ProcessInput` (FEMENU.CPP, 0x8002499c): left and right step the
/// choice (with a sound) and are used up.
pub fn left_right_choice_process_input(c: &mut Cpu, this: MenuItemLeftRightChoice, player: u32, key: u32) {
    let left = c.m.r32(key) == KEY_LEFT;
    if left_right(c, this.base(), |m| this.data(m), player, key) {
        c.m.w32(key, KEY_NONE);
        call(c, PLAY_FE_SFX, &[if left { SOUND_LEFT } else { SOUND_RIGHT }]);
    }
}

pub fn abi_left_right_choice_process_input(c: &mut Cpu) {
    left_right_choice_process_input(c, MenuItemLeftRightChoice(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightChoice::Draw` (FEMENU.CPP, 0x80024a4c): the item's text where its text
/// id places it, and the choice's text (for the sub-menu's player) 180 to its right.
pub fn left_right_choice_draw(c: &mut Cpu, this: MenuItemLeftRightChoice, selected: u32) {
    let text = this.text_description(&mut c.m);
    let x = call(c, WORD_X, &[text]) as i16;
    let y = call(c, WORD_Y, &[text]) as i16 as i32 as u32;
    let selected = (selected != 0) as u32;
    let id = text as i16 as i32 as u32;
    call(c, MENU_TEXT_POSITIONED, &[id, x as i32 as u32, y, selected, ITEM_TEXT]);
    let data = this.data(&mut c.m);
    let player = c.m.r32(SUB_MENU_PLAYER);
    let choice = call_virtual(c, data.0, ITERATOR_VPTR, TEXT_VALUE, &[player]) as i16 as i32 as u32;
    let x = x.wrapping_add(180) as i32 as u32;
    call(c, MENU_TEXT_POSITIONED, &[choice, x, y, selected, ITEM_TEXT]);
}

pub fn abi_left_right_choice_draw(c: &mut Cpu) {
    left_right_choice_draw(c, MenuItemLeftRightChoice(c.r[4]), c.r[5]);
}

/// `tMenuItemLeftRightSlider::tMenuItemLeftRightSlider` (FEMENU.CPP, 0x80024b18): a slider
/// through `data`.
pub fn left_right_slider(c: &mut Cpu, this: MenuItemLeftRightSlider, text: u32, data: ListIterator) -> u32 {
    interactive(c, this.base(), text);
    let m = &mut c.m;
    this.set_data(m, data);
    this.set_vtable(m, LEFT_RIGHT_SLIDER_VT);
    let flags = this.flags(m);
    this.set_flags(m, flags | ITEM_SLIDER);
    this.0
}

pub fn abi_left_right_slider(c: &mut Cpu) {
    c.r[2] = left_right_slider(c, MenuItemLeftRightSlider(c.r[4]), c.r[5], ListIterator(c.r[6]));
}

/// `tMenuItemLeftRightSlider::~tMenuItemLeftRightSlider` (FEMENU.CPP, 0x80024b74).
pub fn abi_left_right_slider_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITEM_VPTR, LEFT_RIGHT_SLIDER_VT, interactive_dtor);
}

/// `tMenuItemLeftRightSlider::DebounceKeys` (FEMENU.CPP, 0x80024b9c): left and right repeat.
pub fn abi_left_right_slider_debounce_keys(c: &mut Cpu) {
    c.r[2] = 1536;
}

/// `tMenuItemLeftRightSlider::ProcessInput` (FEMENU.CPP, 0x80024ba4): left and right step the
/// slider (with a tick) and are used up.
pub fn left_right_slider_process_input(c: &mut Cpu, this: MenuItemLeftRightSlider, player: u32, key: u32) {
    if left_right(c, this.base(), |m| this.data(m), player, key) {
        c.m.w32(key, KEY_NONE);
        call_addr(c, PLAY_FE_SFX_VOL, &[SOUND_TICK, 64]);
    }
}

pub fn abi_left_right_slider_process_input(c: &mut Cpu) {
    left_right_slider_process_input(c, MenuItemLeftRightSlider(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightSlider::Draw` (FEMENU.CPP, 0x80025204): the item's text (its place taken
/// from the text id the first time), and the slider 115 to its right at the iterator's value
/// between its bounds, bright when selected.
pub fn left_right_slider_draw(c: &mut Cpu, this: MenuItemLeftRightSlider, selected: u32) {
    if this.x(&mut c.m) == 0 && this.y(&mut c.m) == 0 {
        let text = this.text_description(&mut c.m);
        let x = call(c, WORD_X, &[text]);
        this.set_x(&mut c.m, x as i16);
        let y = call(c, WORD_Y, &[text]);
        this.set_y(&mut c.m, y as i16);
    }
    let selected = (selected != 0) as u32;
    let m = &mut c.m;
    let id = this.text_description(m) as i16 as i32 as u32;
    let (x, y) = (this.x(m), this.y(m));
    let text_at = [x.wrapping_add(8) as i32 as u32, y.wrapping_add(3) as i32 as u32];
    call(c, MENU_TEXT_POSITIONED, &[id, text_at[0], text_at[1], selected, ITEM_TEXT]);
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, ITERATOR_VPTR, VALUE, &[u32::MAX]) & 0xFF;
    let m = &mut c.m;
    let data = this.data(m);
    let (min, max) = (data.min_value(m) as u32, data.max_value(m) as u32);
    let (x, y) = (this.x(m).wrapping_add(115) as i32 as u32, this.y(m).wrapping_add(4) as i32 as u32);
    let (w, h) = (this.width(m) as i32 as u32, this.height(m) as i32 as u32);
    let brightness = if selected != 0 { 128 } else { 0 };
    call_addr(c, DRAW_SLIDER, &[value, min, max, x, y, w, h, 6, 4, 0, 0, brightness, 0]);
}

pub fn abi_left_right_slider_draw(c: &mut Cpu) {
    left_right_slider_draw(c, MenuItemLeftRightSlider(c.r[4]), c.r[5]);
}

/// `tMenuItemLeftRightSlider::SetDimensions` (FEMENU.CPP, 0x80025330): the slider's place and
/// size.
pub fn set_dimensions(m: &mut Mem, this: MenuItemLeftRightSlider, x: u32, y: u32, w: u32, h: u32) {
    this.set_x(m, x as i16);
    this.set_y(m, y as i16);
    this.set_width(m, w as i16);
    this.set_height(m, h as i16);
}

pub fn abi_set_dimensions(c: &mut Cpu) {
    let h = crate::abi::arg(c, 4);
    set_dimensions(&mut c.m, MenuItemLeftRightSlider(c.r[4]), c.r[5], c.r[6], c.r[7], h);
}

/// `tMenuItemGoToMenuButton::tMenuItemGoToMenuButton` (FEMENU.CPP, 0x80025348): a button
/// going to `menu` and calling `on_press`.
pub fn go_to_menu_button(c: &mut Cpu, this: MenuItemGoToMenuButton, text: u32, menu: Menu, on_press: u32) -> u32 {
    interactive(c, this.base(), text);
    let m = &mut c.m;
    this.set_vtable(m, GO_TO_MENU_BUTTON_VT);
    this.set_new_menu(m, menu);
    this.set_on_button_press(m, on_press);
    this.0
}

pub fn abi_go_to_menu_button(c: &mut Cpu) {
    c.r[2] = go_to_menu_button(c, MenuItemGoToMenuButton(c.r[4]), c.r[5], Menu(c.r[6]), c.r[7]);
}

/// `tMenuItemGoToMenuButton::~tMenuItemGoToMenuButton` (FEMENU.CPP, 0x8002539c).
pub fn abi_go_to_menu_button_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], c.r[5], ITEM_VPTR, GO_TO_MENU_BUTTON_VT, interactive_dtor);
}

/// `tMenuItemGoToMenuButton::ProcessInput` (FEMENU.CPP, 0x800253c4): accept on an enabled
/// button goes to its menu (with the back stack emptied or not, by its flags) and calls its
/// function; the key is used up.
pub fn go_to_menu_button_process_input(c: &mut Cpu, this: MenuItemGoToMenuButton, key: u32, command: MenuCommand) {
    let m = &mut c.m;
    let flags = this.flags(m);
    if flags & ITEM_DISABLED != 0 || m.r32(key) != KEY_ACCEPT {
        return;
    }
    if this.new_menu(m).0 != 0 {
        let kind = if flags & ITEM_GO_RESETS != 0 { COMMAND_GO_TO_RESET } else { COMMAND_GO_TO };
        command.set_type_(m, kind as i32);
        let menu = this.new_menu(m);
        command.set_next_menu(m, menu);
    }
    let on_press = this.on_button_press(m);
    if on_press != 0 {
        call_addr(c, on_press, &[command.0]);
    }
    c.m.w32(key, KEY_NONE);
}

pub fn abi_go_to_menu_button_process_input(c: &mut Cpu) {
    go_to_menu_button_process_input(c, MenuItemGoToMenuButton(c.r[4]), c.r[6], MenuCommand(c.r[7]));
}

// ---- tMenu: its flags (`MENU_*`), its title, its items (to a null), the current one, the
// menus after it and for options, the function for back.

/// The menu's `k`th item. (Not through `item_list`: a list of 16 has no null after it, and the
/// original reads on past its end.)
fn item(m: &mut Mem, this: Menu, k: u32) -> MenuItem {
    MenuItem(m.r32(this.0.wrapping_add(0x10).wrapping_add(k.wrapping_mul(4))))
}

fn set_item(m: &mut Mem, this: Menu, k: u32, item: u32) {
    m.w32(this.0.wrapping_add(0x10).wrapping_add(k.wrapping_mul(4)), item);
}

fn is_disabled(m: &mut Mem, item: MenuItem) -> bool {
    item.flags(m) & ITEM_DISABLED != 0
}

fn current(m: &mut Mem, this: Menu) -> u32 {
    this.current_item(m) as u32
}

fn set_current(m: &mut Mem, this: Menu, k: u32) {
    this.set_current_item(m, k as i32)
}

fn current_is_disabled(m: &mut Mem, this: Menu) -> bool {
    let k = current(m, this);
    let it = item(m, this, k);
    is_disabled(m, it)
}

/// `tMenu::tMenuConstructor` (FEMENU.CPP, 0x80025454): the items: `first`, then those in the
/// list at `rest`, to (and with) a null.
pub fn menu_constructor(m: &mut Mem, this: Menu, first: u32, rest: u32) {
    this.set_vert_help(m, 0);
    set_item(m, this, 0, first);
    let (mut k, mut at) = (1u32, rest);
    loop {
        let it = m.r32(at);
        at = at.wrapping_add(4);
        set_item(m, this, k, it);
        if it == 0 {
            return;
        }
        k = k.wrapping_add(1);
    }
}

pub fn abi_menu_constructor(c: &mut Cpu) {
    menu_constructor(&mut c.m, Menu(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenu::tMenu` (FEMENU.CPP, 0x8002548c): a menu of `screen`, its flags, the menus after it
/// and for options, the function for back, its title.
#[allow(clippy::too_many_arguments)]
pub fn menu(m: &mut Mem, this: Menu, flags: u32, screen: u32, next: u32, options: u32, on_start: u32, title: u32) -> u32 {
    this.set_vtable(m, MENU_VT);
    this.set_flags(m, flags);
    this.set_current_item(m, 0);
    this.set_screen(m, Screen(screen));
    this.set_next_menu(m, Menu(next));
    this.set_never_any_enabled(m, 0);
    this.set_child_menu(m, Menu(0));
    this.set_options_menu(m, Menu(options));
    this.set_on_button_press(m, on_start);
    this.set_title(m, title as i16);
    this.0
}

pub fn abi_menu(c: &mut Cpu) {
    let (options, on_start, title) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    c.r[2] = menu(&mut c.m, Menu(c.r[4]), c.r[5], c.r[6], c.r[7], options, on_start, title);
}

/// `tMenu::~tMenu` (FEMENU.CPP, 0x800254d0).
pub fn abi_menu_dtor(c: &mut Cpu) {
    destroy_root(c, c.r[4], c.r[5], MENU_VPTR, MENU_VT);
}

/// `tMenu::Initialize` (FEMENU.CPP, 0x80025504): the current item moved on past disabled ones,
/// round to the first, at most once round.
pub fn initialize(m: &mut Mem, this: Menu) {
    let start = current(m, this);
    while current_is_disabled(m, this) {
        let k = current(m, this) + 1;
        set_current(m, this, k);
        if item(m, this, k).0 == 0 {
            set_current(m, this, 0);
        }
        if current(m, this) == start {
            return;
        }
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, Menu(c.r[4]));
}

/// Up: the current item moves to the one before, round to the last, past disabled ones.
fn move_up(m: &mut Mem, this: Menu) {
    loop {
        let k = current(m, this);
        if (k as i32) > 0 {
            set_current(m, this, k - 1);
        } else if item(m, this, k.wrapping_add(1)).0 != 0 {
            // round to the last
            let mut k = k;
            loop {
                set_current(m, this, k.wrapping_add(1));
                if item(m, this, k.wrapping_add(2)).0 == 0 {
                    break;
                }
                k = current(m, this);
            }
        }
        if !current_is_disabled(m, this) {
            return;
        }
    }
}

/// Down: the current item moves to the one after, round to the first, past disabled ones.
fn move_down(m: &mut Mem, this: Menu) {
    loop {
        let k = current(m, this).wrapping_add(1);
        set_current(m, this, k);
        if item(m, this, k).0 == 0 {
            set_current(m, this, 0);
        }
        if !current_is_disabled(m, this) {
            return;
        }
    }
}

/// `tMenu::ProcessInput` (FEMENU.CPP, 0x80025598): the current item's input first; then up and
/// down move the current item (with a sound when it moves), accept goes to the menu after,
/// back goes back, start calls the menu's function (or races, or races once both players are
/// ready), options goes to the options menu, help shows the help. A key the menu takes is used up.
pub fn process_input(c: &mut Cpu, this: Menu, player: u32, key: u32, command: MenuCommand) {
    let m = &mut c.m;
    if this.flags(m) & MENU_START_ACCEPTS != 0 && m.r32(key) == KEY_START {
        m.w32(key, KEY_ACCEPT);
    }
    if this.flags(m) & MENU_ACCEPT_STARTS != 0 && m.r32(key) == KEY_ACCEPT {
        m.w32(key, KEY_START);
    }
    let k = current(m, this);
    let it = item(m, this, k);
    if it.0 != 0 {
        call_virtual(c, it.0, ITEM_VPTR, PROCESS_INPUT, &[player, key, command.0]);
    }
    let m = &mut c.m;
    // (the command's type is written before the menu is read again, as the original does)
    let go_to = |m: &mut Mem, menu: fn(Menu, &mut Mem) -> Menu| {
        command.set_type_(m, COMMAND_GO_TO as i32);
        let menu = menu(this, m);
        command.set_next_menu(m, menu);
        m.w32(key, KEY_NONE);
    };
    let take = |m: &mut Mem, kind: u32| {
        command.set_type_(m, kind as i32);
        m.w32(key, KEY_NONE);
    };
    match m.r32(key) {
        KEY_BACK => take(m, COMMAND_BACK),
        KEY_UP | KEY_DOWN => {
            let (was, up) = (current(m, this), m.r32(key) == KEY_UP);
            if up { move_up(m, this) } else { move_down(m, this) }
            if current(m, this) != was {
                call(c, PLAY_FE_SFX, &[if up { SOUND_UP } else { SOUND_DOWN }]);
            }
            c.m.w32(key, KEY_NONE);
        }
        KEY_ACCEPT if this.next_menu(m).0 != 0 => go_to(m, Menu::next_menu),
        KEY_START => {
            let on_start = this.on_button_press(m);
            if on_start != 0 {
                call_addr(c, on_start, &[command.0]);
                c.m.w32(key, KEY_NONE);
            } else if this.flags(m) & MENU_START_RACES != 0 {
                take(m, COMMAND_RACE);
            } else if this.flags(m) & MENU_START_RACES_WHEN_BOTH_READY != 0 {
                take(m, COMMAND_RACE_WHEN_BOTH_READY);
            }
        }
        KEY_HELP => {
            let app = m.r32(FE_APP);
            call(c, DISPLAY_HELP, &[app, 0]);
        }
        KEY_OPTIONS if this.options_menu(m).0 != 0 => go_to(m, Menu::options_menu),
        _ => {}
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, Menu(c.r[4]), c.r[5], c.r[6], MenuCommand(c.r[7]));
}

/// `tMenu::GetNumberEnabledItems` (FEMENU.CPP, 0x800258b4): the items not disabled (none when
/// `fNeverAnyEnabled`).
pub fn get_number_enabled_items(m: &mut Mem, this: Menu) -> u32 {
    if this.never_any_enabled(m) != 0 {
        return 0;
    }
    // (both counters are shorts)
    let (mut n, mut k) = (0i16, 0i16);
    loop {
        let it = item(m, this, k as i32 as u32);
        if it.0 == 0 {
            return n as i32 as u32;
        }
        if !is_disabled(m, it) {
            n = n.wrapping_add(1);
        }
        k = k.wrapping_add(1);
    }
}

pub fn abi_get_number_enabled_items(c: &mut Cpu) {
    c.r[2] = get_number_enabled_items(&mut c.m, Menu(c.r[4]));
}

/// Each item's virtual `slot` (`Draw(bool)`, `UpdateTransition(bool)`), told whether it is
/// the current one.
fn each_item(c: &mut Cpu, this: Menu, slot: u32) {
    // (the counter is a short)
    let mut k = 0i16;
    loop {
        let m = &mut c.m;
        let it = item(m, this, k as i32 as u32);
        if it.0 == 0 {
            return;
        }
        let selected = (k as i32 as u32 == current(m, this)) as u32;
        call_virtual(c, it.0, ITEM_VPTR, slot, &[selected]);
        k = k.wrapping_add(1);
    }
}

/// `tMenu::Draw` (FEMENU.CPP, 0x8002591c): its title (if it has one) and its items.
pub fn draw(c: &mut Cpu, this: Menu) {
    let title = this.title(&mut c.m);
    if title >= 0 {
        call(c, TITLE, &[title as i32 as u32]);
    }
    each_item(c, this, DRAW);
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, Menu(c.r[4]));
}

/// `tMenu::UpdateTransition` (FEMENU.CPP, 0x800259c8): each item's.
pub fn update_transition(c: &mut Cpu, this: Menu) {
    each_item(c, this, UPDATE_TRANSITION);
}

pub fn abi_update_transition(c: &mut Cpu) {
    update_transition(c, Menu(c.r[4]));
}

/// `tMenu::IsSubMenu` (FEMENU.CPP, 0x80025a74): not.
pub fn abi_is_sub_menu(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tMenu::DebounceKeys` (FEMENU.CPP, 0x80025a7c): the current item's.
pub fn debounce_keys(c: &mut Cpu, this: Menu) -> u32 {
    let k = current(&mut c.m, this);
    let it = item(&mut c.m, this, k);
    call_virtual(c, it.0, ITEM_VPTR, DEBOUNCE_KEYS, &[])
}

pub fn abi_debounce_keys(c: &mut Cpu) {
    c.r[2] = debounce_keys(c, Menu(c.r[4]));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original_in, fe_ram};
    use std::cell::Cell;

    /// The test's structures, in RAM the front end leaves zero: a list of text ids, values,
    /// two index bytes; an iterator of each class; a menu item of each class; two menus, their
    /// item lists, a key and a command.
    pub(crate) const LIST: u32 = 0x8014_0000;
    pub(crate) const VALUES: u32 = 0x8014_0100;
    pub(crate) const INDEX1: u32 = 0x8014_0200;
    pub(crate) const INDEX2: u32 = 0x8014_0201;
    pub(crate) const IT_BASE: u32 = 0x8014_0400;
    pub(crate) const IT_INDEXED: u32 = 0x8014_0420;
    pub(crate) const IT_DOUBLE: u32 = 0x8014_0440;
    pub(crate) const IT_MULTI: u32 = 0x8014_0460;
    pub(crate) const IT_RANGE: u32 = 0x8014_0480;
    pub(crate) const IT_RANGE_INDEXED: u32 = 0x8014_04A0;
    pub(crate) const ITEMS: u32 = 0x8014_0800;
    pub(crate) const ITEM_SIZE: u32 = 0x40;
    pub(crate) const MENU: u32 = 0x8014_1000;
    pub(crate) const MENU2: u32 = 0x8014_1100;
    pub(crate) const KEY: u32 = 0x8014_2000;
    pub(crate) const COMMAND: u32 = 0x8014_2010;
    pub(crate) const NEW: u32 = 0x8014_3000;
    /// A copy of the base item's vtable with the four-argument `Draw` doing nothing (no class
    /// here has one: the base's calls itself), for the item after the five classes'.
    pub(crate) const FAKE_VT: u32 = 0x8014_0700;
    pub(crate) const FAKE_ITEM: u32 = ITEMS + 5 * ITEM_SIZE;
    /// A function the menus may call (does nothing): `tMenuItem::ProcessInput`.
    pub(crate) const NOTHING: u32 = 0x8002_47C0;

    pub(crate) fn pick(rng: &mut Rng, n: u32) -> u32 {
        (rng.next() % n as u64) as u32
    }

    /// The iterators' classes (vtables) and the items' (vtables, data at +28).
    const ITERATORS: [(u32, u32); 6] = [(IT_BASE, LIST_ITERATOR_VT), (IT_INDEXED, INDEXED_VT), (IT_DOUBLE, DOUBLE_INDEXED_VT), (IT_MULTI, MULTI_PLAYER_VT), (IT_RANGE, RANGE_VT), (IT_RANGE_INDEXED, RANGE_INDEXED_VT)];
    const ITEM_VTS: [u32; 5] = [MENU_ITEM_VT, INTERACTIVE_VT, LEFT_RIGHT_CHOICE_VT, LEFT_RIGHT_SLIDER_VT, GO_TO_MENU_BUTTON_VT];

    /// The car select's RAM with the test's structures: a list of 1 to 10 text ids (now and
    /// then a negative one), values and indexes in and past it, the iterators over them; an
    /// item of each class (some disabled, a text id, a fade, an iterator, a slider's place, a
    /// menu to go to and a function), two menus of a few of them (any current item, flags,
    /// title, menus after and for options, a function for back); a key and a command.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let n = 1 + pick(rng, 10);
        for k in 0..n {
            let v = if pick(rng, 8) == 0 { -(pick(rng, 50) as i32) } else { 1 + pick(rng, 400) as i32 };
            m.w16(LIST + 2 * k, v as u16);
        }
        m.w16(LIST + 2 * n, 0);
        for k in 0..32 {
            m.w8(VALUES + k, pick(rng, n + 2) as u8);
        }
        m.w8(INDEX1, pick(rng, 4) as u8);
        m.w8(INDEX2, pick(rng, 8) as u8);
        for (it, vt) in ITERATORS {
            m.w32(it, LIST);
            m.w32(it + 4, VALUES);
            let min = pick(rng, 4);
            m.w8(it + 8, min as u8);
            m.w8(it + 9, (min + pick(rng, 10)) as u8);
            m.w32(it + 12, vt);
        }
        m.w32(IT_INDEXED + 16, INDEX1);
        m.w32(IT_DOUBLE + 16, INDEX1);
        m.w32(IT_DOUBLE + 20, pick(rng, 5));
        m.w32(IT_DOUBLE + 24, INDEX2);
        m.w32(IT_RANGE_INDEXED + 16, INDEX2);
        for (k, vt) in ITEM_VTS.into_iter().enumerate() {
            let it = ITEMS + ITEM_SIZE * k as u32;
            let flags = (pick(rng, 4) == 0) as u32 | [0, 0x40, 0x80, 0x400][pick(rng, 4) as usize];
            m.w32(it, flags);
            m.w32(it + 4, pick(rng, 300));
            m.w16(it + 8, pick(rng, 140) as u16);
            m.w32(it + 20, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(it + 24, vt);
            let data = if vt == GO_TO_MENU_BUTTON_VT { [0, NOTHING][pick(rng, 2) as usize] } else { ITERATORS[pick(rng, 6) as usize].0 };
            m.w32(it + 28, data);
            let v = if pick(rng, 2) == 0 { 0 } else { rng.next() as u32 & 0x00FF_00FF };
            m.w32(it + 32, v);
            m.w32(it + 36, rng.next() as u32 & 0x003F_007F);
        }
        for k in 0..13 {
            let v = m.r32(MENU_ITEM_VT + 4 * k);
            m.w32(FAKE_VT + 4 * k, v);
        }
        m.w32(FAKE_VT + DRAW_AT + 4, NOTHING);
        for k in 0..10 {
            let v = m.r32(ITEMS + 4 * k);
            m.w32(FAKE_ITEM + 4 * k, v);
        }
        m.w32(FAKE_ITEM + 24, FAKE_VT);
        // (the second menu's items draw: a choice or a slider, the others' Draw is pure)
        for (menu, items) in [(MENU, 1 + pick(rng, 5)), (MENU2, 1 + pick(rng, 3))] {
            m.w32(menu + 104, MENU_VT);
            let mut flags = 0;
            for bit in [1, 2, 4, 0x1_0000] {
                if pick(rng, 3) == 0 {
                    flags |= bit;
                }
            }
            m.w32(menu, flags);
            m.w16(menu + 4, if pick(rng, 3) == 0 { u16::MAX } else { pick(rng, 300) as u16 });
            // (a title the title's buffer holds: FETextRender_Title copies it into 88 bytes on
            // its stack, and a longer one overwrites its saved registers)
            loop {
                let title = m.r16(menu + 4) as i16;
                if title < 0 || text_len(m, title) < 80 {
                    break;
                }
                m.w16(menu + 4, pick(rng, 300) as u16);
            }
            m.w32(menu + 8, pick(rng, items));
            m.w32(menu + 12, (pick(rng, 6) == 0) as u32);
            for k in 0..items {
                let class = if menu == MENU2 { 2 + pick(rng, 2) } else { pick(rng, 5) };
                m.w32(menu + 16 + 4 * k, ITEMS + ITEM_SIZE * class);
            }
            m.w32(menu + 16 + 4 * items, 0);
            m.w32(menu + 84, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(menu + 92, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(menu + 96, [0, NOTHING][pick(rng, 2) as usize]);
        }
        // (at least one item enabled, or the original's up and down go round for ever)
        let first = m.r32(MENU + 16);
        let flags = m.r32(first);
        m.w32(first, flags & !ITEM_DISABLED);
        let first = m.r32(MENU2 + 16);
        let flags = m.r32(first);
        m.w32(first, flags & !ITEM_DISABLED);
        let keys = [KEY_NONE, KEY_ACCEPT, KEY_HELP, KEY_OPTIONS, KEY_BACK, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_START, 32];
        m.w32(KEY, keys[pick(rng, keys.len() as u32) as usize]);
        m.w32(COMMAND, 0);
        m.w32(COMMAND + 4, 0);
        m.w32(SUB_MENU_PLAYER, [u32::MAX, 0, 1][pick(rng, 3) as usize]);
    }

    /// The length of text `id`'s string (`TextSys_Word`'s, from the loaded texts).
    pub(crate) fn text_len(m: &mut Mem, id: i16) -> u32 {
        let mut c = crate::test_util::cpu_with(&m.ram);
        let mut at = crate::abi::call(&mut c, nfs4rc::f_800b9f18, &[id as i32 as u32]);
        let mut n = 0;
        while c.m.r8(at) != 0 && n < 1000 {
            at += 1;
            n += 1;
        }
        n
    }

    thread_local! {
        /// The function [`via_abi`] runs.
        pub(crate) static ADDR: Cell<u32> = const { Cell::new(0) };
    }

    /// The port at `ADDR` entered through its ABI entry with `a` (from the fifth on at sp+16,
    /// where the original's caller has them); its v0 when it returns one.
    pub(crate) fn via_abi(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        let i = crate::find(ADDR.get()).expect("routed");
        let sp = c.r[29];
        for (k, &v) in a.iter().enumerate() {
            if k < 4 {
                c.r[4 + k] = v;
            } else {
                c.m.w32(sp + 4 * k as u32, v);
            }
        }
        (crate::FUNCS[i].run)(c);
        crate::FUNCS[i].returns.then_some(c.r[2])
    }

    pub(crate) type Args = fn(&mut Cpu, &mut Rng) -> Vec<u32>;
    /// A function's name, address, original and arguments.
    pub(crate) type Case = (&'static str, u32, fn(&mut Cpu), Args);

    pub(crate) fn iterator(rng: &mut Rng) -> u32 {
        ITERATORS[pick(rng, 6) as usize].0
    }

    fn item_arg(rng: &mut Rng) -> u32 {
        ITEMS + ITEM_SIZE * pick(rng, 5)
    }

    pub(crate) fn player(rng: &mut Rng) -> u32 {
        [u32::MAX, 0, 1, 2][pick(rng, 4) as usize]
    }

    /// Every function: its name, address, original and arguments.
    fn cases() -> Vec<Case> {
        vec![
            ("tListIterator", 0x8002_3df0, nfs4rc::f_80023df0, |_, _| vec![NEW, LIST, VALUES]),
            ("~tListIterator", 0x8002_3e14, nfs4rc::f_80023e14, |_, r| vec![IT_BASE, 2 * pick(r, 2)]),
            ("Value", 0x8002_3e48, nfs4rc::f_80023e48, |_, r| vec![IT_BASE, player(r)]),
            ("TextValue", 0x8002_3e5c, nfs4rc::f_80023e5c, |_, r| vec![iterator(r), player(r)]),
            ("Increment", 0x8002_3eac, nfs4rc::f_80023eac, |_, r| vec![IT_BASE, player(r)]),
            ("Decrement", 0x8002_3ef8, nfs4rc::f_80023ef8, |_, r| vec![IT_BASE, player(r)]),
            ("Indexed", 0x8002_3f80, nfs4rc::f_80023f80, |_, _| vec![NEW, LIST, VALUES, INDEX1]),
            ("~Indexed", 0x8002_3fc4, nfs4rc::f_80023fc4, |_, r| vec![IT_INDEXED, 2 * pick(r, 2)]),
            ("Indexed::Value", 0x8002_3fec, nfs4rc::f_80023fec, |_, _| vec![IT_INDEXED]),
            ("Indexed::TextValue", 0x8002_4010, nfs4rc::f_80024010, |_, r| vec![[IT_INDEXED, IT_DOUBLE][pick(r, 2) as usize], player(r)]),
            ("Indexed::Increment", 0x8002_4060, nfs4rc::f_80024060, |_, _| vec![IT_INDEXED]),
            ("Indexed::Decrement", 0x8002_40c8, nfs4rc::f_800240c8, |_, _| vec![IT_INDEXED]),
            ("~DoubleIndexed", 0x8002_4174, nfs4rc::f_80024174, |_, r| vec![IT_DOUBLE, 2 * pick(r, 2)]),
            ("DoubleIndexed::Value", 0x8002_419c, nfs4rc::f_8002419c, |_, _| vec![IT_DOUBLE]),
            ("DoubleIndexed::TextValue", 0x8002_41d8, nfs4rc::f_800241d8, |_, r| vec![iterator(r), player(r)]),
            ("DoubleIndexed::Increment", 0x8002_4228, nfs4rc::f_80024228, |_, _| vec![IT_DOUBLE]),
            ("DoubleIndexed::Decrement", 0x8002_42c4, nfs4rc::f_800242c4, |_, _| vec![IT_DOUBLE]),
            ("~MultiPlayer", 0x8002_43b0, nfs4rc::f_800243b0, |_, r| vec![IT_MULTI, 2 * pick(r, 2)]),
            ("MultiPlayer::Value", 0x8002_43d8, nfs4rc::f_800243d8, |_, r| vec![IT_MULTI, player(r)]),
            ("MultiPlayer::TextValue", 0x8002_4400, nfs4rc::f_80024400, |_, r| vec![[IT_MULTI, IT_INDEXED][pick(r, 2) as usize], player(r)]),
            ("MultiPlayer::Increment", 0x8002_4450, nfs4rc::f_80024450, |_, r| vec![IT_MULTI, player(r)]),
            ("MultiPlayer::Decrement", 0x8002_44b4, nfs4rc::f_800244b4, |_, r| vec![IT_MULTI, player(r)]),
            ("Range", 0x8002_4554, nfs4rc::f_80024554, |_, r| vec![NEW, pick(r, 300), pick(r, 300), VALUES]),
            ("~Range", 0x8002_45b0, nfs4rc::f_800245b0, |_, r| vec![IT_RANGE, 2 * pick(r, 2)]),
            ("Range::Value", 0x8002_45d8, nfs4rc::f_800245d8, |_, _| vec![IT_RANGE]),
            ("Range::TextValue", 0x8002_45ec, nfs4rc::f_800245ec, |_, _| vec![IT_RANGE]),
            ("Range::Increment", 0x8002_45f4, nfs4rc::f_800245f4, |_, _| vec![IT_RANGE]),
            ("Range::Decrement", 0x8002_461c, nfs4rc::f_8002461c, |_, _| vec![IT_RANGE]),
            ("RangeIndexed", 0x8002_4644, nfs4rc::f_80024644, |_, r| vec![NEW, pick(r, 300), pick(r, 300), VALUES, INDEX2]),
            ("~RangeIndexed", 0x8002_4690, nfs4rc::f_80024690, |_, r| vec![IT_RANGE_INDEXED, 2 * pick(r, 2)]),
            ("RangeIndexed::Value", 0x8002_46b8, nfs4rc::f_800246b8, |_, _| vec![IT_RANGE_INDEXED]),
            ("RangeIndexed::Increment", 0x8002_46dc, nfs4rc::f_800246dc, |_, _| vec![IT_RANGE_INDEXED]),
            ("RangeIndexed::Decrement", 0x8002_4718, nfs4rc::f_80024718, |_, _| vec![IT_RANGE_INDEXED]),
            ("tMenuItem", 0x8002_4754, nfs4rc::f_80024754, |_, r| vec![NEW, pick(r, 300)]),
            ("~tMenuItem", 0x8002_4784, nfs4rc::f_80024784, |_, r| vec![item_arg(r), 2 * pick(r, 2)]),
            ("tMenuItem::DebounceKeys", 0x8002_47b8, nfs4rc::f_800247b8, |_, r| vec![item_arg(r)]),
            ("tMenuItem::UpdateTransition", 0x8002_47c8, nfs4rc::f_800247c8, |_, r| vec![item_arg(r), pick(r, 2)]),
            ("tMenuItem::TransitionIsFinished", 0x8002_47e8, nfs4rc::f_800247e8, |_, r| vec![item_arg(r)]),
            ("tMenuItem::UpdateSelFade", 0x8002_47f0, nfs4rc::f_800247f0, |_, r| vec![item_arg(r), pick(r, 3)]),
            ("tMenuItem::Draw", 0x8002_4848, nfs4rc::f_80024848, |_, r| vec![ITEMS + ITEM_SIZE * (2 + pick(r, 2)), pick(r, 300), pick(r, 300), pick(r, 2)]),
            ("tMenuItem::Draw(int,int,int,bool)", 0x8002_4884, nfs4rc::f_80024884, |_, r| vec![FAKE_ITEM, pick(r, 300), pick(r, 300), pick(r, 300), pick(r, 2)]),
            ("Interactive", 0x8002_48c8, nfs4rc::f_800248c8, |_, r| vec![NEW, pick(r, 300)]),
            ("~Interactive", 0x8002_48fc, nfs4rc::f_800248fc, |_, r| vec![item_arg(r), 2 * pick(r, 2)]),
            ("LeftRightChoice", 0x8002_4924, nfs4rc::f_80024924, |_, r| vec![NEW, pick(r, 300), iterator(r)]),
            ("~LeftRightChoice", 0x8002_4974, nfs4rc::f_80024974, |_, r| vec![item_arg(r), 2 * pick(r, 2)]),
            ("LeftRightChoice::ProcessInput", 0x8002_499c, nfs4rc::f_8002499c, |_, r| vec![ITEMS + 2 * ITEM_SIZE, player(r), KEY, COMMAND]),
            ("LeftRightChoice::Draw", 0x8002_4a4c, nfs4rc::f_80024a4c, |_, r| vec![ITEMS + 2 * ITEM_SIZE, pick(r, 3)]),
            ("LeftRightSlider", 0x8002_4b18, nfs4rc::f_80024b18, |_, r| vec![NEW, pick(r, 300), iterator(r)]),
            ("~LeftRightSlider", 0x8002_4b74, nfs4rc::f_80024b74, |_, r| vec![item_arg(r), 2 * pick(r, 2)]),
            ("LeftRightSlider::DebounceKeys", 0x8002_4b9c, nfs4rc::f_80024b9c, |_, r| vec![item_arg(r)]),
            ("LeftRightSlider::ProcessInput", 0x8002_4ba4, nfs4rc::f_80024ba4, |_, r| vec![ITEMS + 3 * ITEM_SIZE, player(r), KEY, COMMAND]),
            ("LeftRightSlider::Draw", 0x8002_5204, nfs4rc::f_80025204, |_, r| vec![ITEMS + 3 * ITEM_SIZE, pick(r, 3)]),
            ("LeftRightSlider::SetDimensions", 0x8002_5330, nfs4rc::f_80025330, |_, r| vec![item_arg(r), pick(r, 400), pick(r, 300), pick(r, 200), pick(r, 100)]),
            ("GoToMenuButton", 0x8002_5348, nfs4rc::f_80025348, |_, r| vec![NEW, pick(r, 300), MENU2, NOTHING]),
            ("~GoToMenuButton", 0x8002_539c, nfs4rc::f_8002539c, |_, r| vec![item_arg(r), 2 * pick(r, 2)]),
            ("GoToMenuButton::ProcessInput", 0x8002_53c4, nfs4rc::f_800253c4, |_, r| vec![ITEMS + 4 * ITEM_SIZE, player(r), KEY, COMMAND]),
            ("tMenuConstructor", 0x8002_5454, nfs4rc::f_80025454, |c, r| {
                for k in 0..4 {
                    let v = if k == 3 || pick(r, 3) == 0 { 0 } else { item_arg(r) };
                    c.m.w32(NEW + 0x100 + 4 * k, v);
                }
                vec![NEW, item_arg(r), NEW + 0x100]
            }),
            ("tMenu", 0x8002_548c, nfs4rc::f_8002548c, |_, r| vec![NEW, pick(r, 8), 0x8014_5000, MENU2, MENU2, NOTHING, pick(r, 70000)]),
            ("~tMenu", 0x8002_54d0, nfs4rc::f_800254d0, |_, r| vec![MENU, 2 * pick(r, 2)]),
            ("tMenu::Initialize", 0x8002_5504, nfs4rc::f_80025504, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize]]),
            ("tMenu::ProcessInput", 0x8002_5598, nfs4rc::f_80025598, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize], player(r), KEY, COMMAND]),
            ("tMenu::GetNumberEnabledItems", 0x8002_58b4, nfs4rc::f_800258b4, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize]]),
            ("tMenu::Draw", 0x8002_591c, nfs4rc::f_8002591c, |_, _| vec![MENU2]),
            ("tMenu::UpdateTransition", 0x8002_59c8, nfs4rc::f_800259c8, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize]]),
            ("tMenu::TransitionIsFinished", 0x8002_5a6c, nfs4rc::f_80025a6c, |_, _| vec![MENU]),
            ("tMenu::IsSubMenu", 0x8002_5a74, nfs4rc::f_80025a74, |_, _| vec![MENU]),
            ("tMenu::DebounceKeys", 0x8002_5a7c, nfs4rc::f_80025a7c, |_, r| vec![[MENU, MENU2][pick(r, 2) as usize], player(r)]),
        ]
    }

    #[test]
    fn femenu_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            // (FEMENU_ONLY=name: that function's cases alone)
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xFE30 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 400, 0xFE31 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
