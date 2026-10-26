//! FEMENU.CPP: the front end's menus: the list iterators a menu item steps through (a value
//! in a list of text ids, indexed by a byte or two, per player, or a range), the menu items
//! (the base, the interactive ones, left-right choices and sliders, buttons that go to a menu)
//! and the menus that hold them (the current item, moving up and down past disabled ones,
//! accept, back, options and help).
//!
//! Most of it is C++ virtual calls: an iterator's vtable pointer is at +12, an item's at +24
//! and a menu's at +104; each entry is the `this` offset (a short) and the function (`method`).
//! A virtual call gets the registers the original's caller has at its `jalr`, those it doesn't
//! set from the calls before (`Saved::call_ptr_leaving`).

use crate::abi::{self, Saved};
use crate::globals::{FE_APP, SUB_MENU_PLAYER};
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

/// The input keys a menu reacts to (`tInputKeyType`).
pub(crate) const KEY_NONE: u32 = 1;
pub(crate) const KEY_ACCEPT: u32 = 2;
pub(crate) const KEY_HELP: u32 = 4;
pub(crate) const KEY_OPTIONS: u32 = 8;
pub(crate) const KEY_START: u32 = 16;
pub(crate) const KEY_UP: u32 = 512;
pub(crate) const KEY_DOWN: u32 = 1024;
pub(crate) const KEY_LEFT: u32 = 2048;
pub(crate) const KEY_RIGHT: u32 = 4096;
pub(crate) const KEY_BACK: u32 = 8192;
/// A menu command's types (`tMenuCommandType`): go to a menu, the start, back, back out.
pub(crate) const COMMAND_GO_TO: u32 = 1;
pub(crate) const COMMAND_START: u32 = 4;
pub(crate) const COMMAND_BACK: u32 = 5;
pub(crate) const COMMAND_EXIT: u32 = 6;
/// A menu item is disabled (flag bit 0).
pub(crate) const ITEM_DISABLED: u32 = 1;

/// The library's `__builtin_delete` (0x800a4910), and the front end's sounds.
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;
pub(crate) const PLAY_FE_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;
/// `AudioCmn_PlayFESFXVol` (the host watches it: called by address).
const PLAY_FE_SFX_VOL: u32 = 0x8007_A0AC;
/// `TextSys_WordX`, `TextSys_WordY`: a text id's place on the screen.
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// `FETextRender_MenuTextPositioned` (FETEXTRENDER.CPP), `FETextRender_Title`.
const MENU_TEXT_POSITIONED: fn(&mut Cpu) = crate::fetextrender::menu_text_positioned_entry;
pub(crate) const TITLE: fn(&mut Cpu) = crate::fetextrender::title_entry;
/// `tFEApplication::DisplayHelp` (FEAPP.CPP).
const DISPLAY_HELP: fn(&mut Cpu) = nfs4rc::f_80014048;
/// `DrawSlider` (the host captures its primitives: called by address).
const DRAW_SLIDER: u32 = 0x8002_4C4C;

/// The virtual function at `slot` of the object `this` whose vtable pointer is at
/// `this + vptr`: `this` adjusted by the entry's offset, and the function.
pub(crate) fn method(m: &mut Mem, this: u32, vptr: u32, slot: u32) -> (u32, u32) {
    let vt = m.r32(this.wrapping_add(vptr));
    let delta = m.r16(vt.wrapping_add(slot)) as i16 as i32 as u32;
    (this.wrapping_add(delta), m.r32(vt.wrapping_add(slot + 4)))
}

/// The text id at `k` in an iterator's list (`fSelectionList`, `short[]`).
fn list_entry(m: &mut Mem, this: u32, k: u32) -> i32 {
    let list = m.r32(this);
    m.r16(list.wrapping_add(k.wrapping_mul(2))) as i16 as i32
}

/// A destructor: the vtable set back to the class's (`vptr` its place), then the base class's
/// destructor (`base`, at `base_addr`), given the caller's flags; the return address `ra`.
fn destroy(c: &mut Cpu, this: u32, vptr: u32, vtable: u32, base_addr: u32, ra: u32, base: fn(&mut Cpu, u32, u32)) {
    abi::frame(c, 24, |c, _| {
        c.m.w32(this.wrapping_add(vptr), vtable);
        let flags = c.r[5];
        Saved::of(c).enter(c, base_addr, ra, |c| base(c, this, flags));
    })
}

/// A root class's destructor: the vtable set back to the class's, the object deleted when
/// `flags & 1`.
fn destroy_root(c: &mut Cpu, this: u32, flags: u32, vptr: u32, vtable: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        c.m.w32(this.wrapping_add(vptr), vtable);
        if flags & 1 != 0 {
            Saved::of(c).call(c, DELETE, &[this, 1], ra);
        }
    })
}

// tListIterator: a byte `*fValue`, an index into `fSelectionList`, the list of text ids (0
// ends it).

/// `tListIterator::tListIterator` (FEMENU.CPP, 0x80023df0): over `list`, the value at `value`.
pub fn list_iterator(m: &mut Mem, this: u32, list: u32, value: u32) -> u32 {
    m.w32(this + 12, LIST_ITERATOR_VT);
    m.w32(this, list);
    m.w32(this + 4, value);
    m.w8(this + 9, 0);
    m.w8(this + 8, 0);
    this
}

pub fn abi_list_iterator(c: &mut Cpu) {
    c.r[2] = list_iterator(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tListIterator::~tListIterator` (FEMENU.CPP, 0x80023e14).
pub fn list_iterator_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITERATOR_VPTR, LIST_ITERATOR_VT, 0x8002_3e38);
}

pub fn abi_list_iterator_dtor(c: &mut Cpu) {
    list_iterator_dtor(c, c.r[4], c.r[5]);
}

/// `tListIterator::Value` (FEMENU.CPP, 0x80023e48): the value.
pub fn value(m: &mut Mem, this: u32) -> u32 {
    let at = m.r32(this + 4);
    m.r8(at) as u32
}

pub fn abi_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, c.r[4]);
}

/// A `TextValue`: the text id at the value (`Value` for `player`, through the vtable) in the
/// list; `ra` the virtual call's return.
fn text_value_of(c: &mut Cpu, this: u32, player: u32, ra: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let (obj, f) = method(&mut c.m, this, ITERATOR_VPTR, VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, player], ra);
        list_entry(&mut c.m, this, v & 0xFF) as u32
    })
}

/// `tListIterator::TextValue` (FEMENU.CPP, 0x80023e5c): the value's text id.
pub fn text_value(c: &mut Cpu, this: u32) -> u32 {
    text_value_of(c, this, u32::MAX, 0x8002_3e88)
}

pub fn abi_text_value(c: &mut Cpu) {
    c.r[2] = text_value(c, c.r[4]);
}

/// The next value in the list at `at` (round to the first after the last): `Increment`'s.
/// `at` gives the value's place, read again after the step as the original does.
fn step_up(m: &mut Mem, this: u32, at: impl Fn(&mut Mem) -> u32) {
    let a = at(m);
    let v = m.r8(a).wrapping_add(1);
    m.w8(a, v);
    let a = at(m);
    let k = m.r8(a) as u32;
    if list_entry(m, this, k) == 0 {
        m.w8(a, 0);
    }
}

/// The previous value (round to the last: on while the list goes on): `Decrement`'s. `at`
/// gives the value's place; `first` is it as the original first read it.
fn step_down(m: &mut Mem, this: u32, first: u32, at: impl Fn(&mut Mem) -> u32) {
    let v = m.r8(first);
    if v != 0 {
        m.w8(first, v - 1);
        return;
    }
    if list_entry(m, this, 1) <= 0 {
        return;
    }
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
pub fn increment(m: &mut Mem, this: u32) {
    step_up(m, this, |m| m.r32(this + 4));
}

pub fn abi_increment(c: &mut Cpu) {
    increment(&mut c.m, c.r[4]);
}

/// `tListIterator::Decrement` (FEMENU.CPP, 0x80023ef8): the previous text.
pub fn decrement(m: &mut Mem, this: u32) {
    let first = m.r32(this + 4);
    step_down(m, this, first, |m| m.r32(this + 4));
}

pub fn abi_decrement(c: &mut Cpu) {
    decrement(&mut c.m, c.r[4]);
}

// tListIteratorIndexed: the value at `fValue[*fIndex]`.

/// `tListIteratorIndexed`'s value's place.
fn indexed_at(m: &mut Mem, this: u32) -> u32 {
    let index = m.r32(this + 16);
    let k = m.r8(index) as u32;
    m.r32(this + 4).wrapping_add(k)
}

/// `tListIteratorIndexed::tListIteratorIndexed` (FEMENU.CPP, 0x80023f80): over `list`, the
/// values at `value`, the one at `*index`.
pub fn indexed(c: &mut Cpu, this: u32, list: u32, value: u32, index: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[this, index]);
        r.enter(c, 0x8002_3df0, 0x8002_3f9c, |c| list_iterator(&mut c.m, this, list, value));
        c.m.w32(this + 12, INDEXED_VT);
        c.m.w32(this + 16, index);
        this
    })
}

pub fn abi_indexed(c: &mut Cpu) {
    c.r[2] = indexed(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tListIteratorIndexed::~tListIteratorIndexed` (FEMENU.CPP, 0x80023fc4).
pub fn abi_indexed_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITERATOR_VPTR, INDEXED_VT, 0x8002_3e14, 0x8002_3fdc, list_iterator_dtor);
}

/// `tListIteratorIndexed::Value` (FEMENU.CPP, 0x80023fec).
pub fn indexed_value(m: &mut Mem, this: u32) -> u32 {
    let at = indexed_at(m, this);
    m.r8(at) as u32
}

pub fn abi_indexed_value(c: &mut Cpu) {
    c.r[2] = indexed_value(&mut c.m, c.r[4]);
}

/// `tListIteratorIndexed::TextValue` (FEMENU.CPP, 0x80024010).
pub fn abi_indexed_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, c.r[4], u32::MAX, 0x8002_403c);
}

/// `tListIteratorIndexed::Increment` (FEMENU.CPP, 0x80024060).
pub fn indexed_increment(m: &mut Mem, this: u32) {
    step_up(m, this, |m| indexed_at(m, this));
}

pub fn abi_indexed_increment(c: &mut Cpu) {
    indexed_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorIndexed::Decrement` (FEMENU.CPP, 0x800240c8).
pub fn indexed_decrement(m: &mut Mem, this: u32) {
    let first = indexed_at(m, this);
    step_down(m, this, first, |m| indexed_at(m, this));
}

pub fn abi_indexed_decrement(c: &mut Cpu) {
    indexed_decrement(&mut c.m, c.r[4]);
}

// tListIteratorDoubleIndexed: the value at `fValue[*fIndex1 * index1multiplier + *fIndex2]`.

fn double_indexed_at(m: &mut Mem, this: u32) -> u32 {
    let index1 = m.r32(this + 16);
    let row = (m.r8(index1) as u32).wrapping_mul(m.r32(this + 20));
    let index2 = m.r32(this + 24);
    let k = m.r8(index2) as u32;
    m.r32(this + 4).wrapping_add(row.wrapping_add(k))
}

/// `tListIteratorDoubleIndexed::~tListIteratorDoubleIndexed` (FEMENU.CPP, 0x80024174).
pub fn abi_double_indexed_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITERATOR_VPTR, DOUBLE_INDEXED_VT, 0x8002_3e14, 0x8002_418c, list_iterator_dtor);
}

/// `tListIteratorDoubleIndexed::Value` (FEMENU.CPP, 0x8002419c).
pub fn double_indexed_value(m: &mut Mem, this: u32) -> u32 {
    let at = double_indexed_at(m, this);
    m.r8(at) as u32
}

pub fn abi_double_indexed_value(c: &mut Cpu) {
    c.r[2] = double_indexed_value(&mut c.m, c.r[4]);
}

/// `tListIteratorDoubleIndexed::TextValue` (FEMENU.CPP, 0x800241d8).
pub fn abi_double_indexed_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, c.r[4], u32::MAX, 0x8002_4204);
}

/// `tListIteratorDoubleIndexed::Increment` (FEMENU.CPP, 0x80024228).
pub fn double_indexed_increment(m: &mut Mem, this: u32) {
    step_up(m, this, |m| double_indexed_at(m, this));
}

pub fn abi_double_indexed_increment(c: &mut Cpu) {
    double_indexed_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorDoubleIndexed::Decrement` (FEMENU.CPP, 0x800242c4).
pub fn double_indexed_decrement(m: &mut Mem, this: u32) {
    let first = double_indexed_at(m, this);
    step_down(m, this, first, |m| double_indexed_at(m, this));
}

pub fn abi_double_indexed_decrement(c: &mut Cpu) {
    double_indexed_decrement(&mut c.m, c.r[4]);
}

// tListIteratorMultiPlayer: a value per player, `fValue[player]` (-1, all of them, the first).

fn player_offset(player: u32) -> u32 {
    if player == u32::MAX { 0 } else { player }
}

fn multi_player_at(m: &mut Mem, this: u32, player: u32) -> u32 {
    m.r32(this + 4).wrapping_add(player_offset(player))
}

/// `tListIteratorMultiPlayer::~tListIteratorMultiPlayer` (FEMENU.CPP, 0x800243b0).
pub fn abi_multi_player_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITERATOR_VPTR, MULTI_PLAYER_VT, 0x8002_3e14, 0x8002_43c8, list_iterator_dtor);
}

/// `tListIteratorMultiPlayer::Value` (FEMENU.CPP, 0x800243d8): `player`'s value.
pub fn multi_player_value(m: &mut Mem, this: u32, player: u32) -> u32 {
    let at = multi_player_at(m, this, player);
    m.r8(at) as u32
}

pub fn abi_multi_player_value(c: &mut Cpu) {
    c.r[2] = multi_player_value(&mut c.m, c.r[4], c.r[5]);
}

/// `tListIteratorMultiPlayer::TextValue` (FEMENU.CPP, 0x80024400): `player`'s text id.
pub fn abi_multi_player_text_value(c: &mut Cpu) {
    c.r[2] = text_value_of(c, c.r[4], c.r[5], 0x8002_442c);
}

/// `tListIteratorMultiPlayer::Increment` (FEMENU.CPP, 0x80024450).
pub fn multi_player_increment(m: &mut Mem, this: u32, player: u32) {
    step_up(m, this, |m| multi_player_at(m, this, player));
}

pub fn abi_multi_player_increment(c: &mut Cpu) {
    multi_player_increment(&mut c.m, c.r[4], c.r[5]);
}

/// `tListIteratorMultiPlayer::Decrement` (FEMENU.CPP, 0x800244b4).
pub fn multi_player_decrement(m: &mut Mem, this: u32, player: u32) {
    let first = multi_player_at(m, this, player);
    step_down(m, this, first, |m| multi_player_at(m, this, player));
}

pub fn abi_multi_player_decrement(c: &mut Cpu) {
    multi_player_decrement(&mut c.m, c.r[4], c.r[5]);
}

// tListIteratorRange: a value from `fMinValue` to `fMaxValue` (no list, no text).

/// `tListIteratorRange::tListIteratorRange` (FEMENU.CPP, 0x80024554).
pub fn range(c: &mut Cpu, this: u32, min: u32, max: u32, value: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, min, max]);
        r.enter(c, 0x8002_3df0, 0x8002_4580, |c| list_iterator(&mut c.m, this, 0, value));
        let m = &mut c.m;
        m.w32(this + 12, RANGE_VT);
        m.w8(this + 8, min as u8);
        m.w8(this + 9, max as u8);
        this
    })
}

pub fn abi_range(c: &mut Cpu) {
    c.r[2] = range(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tListIteratorRange::~tListIteratorRange` (FEMENU.CPP, 0x800245b0).
pub fn range_dtor(c: &mut Cpu, this: u32, _flags: u32) {
    destroy(c, this, ITERATOR_VPTR, RANGE_VT, 0x8002_3e14, 0x8002_45c8, list_iterator_dtor);
}

pub fn abi_range_dtor(c: &mut Cpu) {
    range_dtor(c, c.r[4], c.r[5]);
}

/// `tListIteratorRange::Value` (FEMENU.CPP, 0x800245d8).
pub fn abi_range_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, c.r[4]);
}

/// `tListIteratorRange::TextValue` (FEMENU.CPP, 0x800245ec): none.
pub fn abi_range_text_value(c: &mut Cpu) {
    c.r[2] = 0;
}

/// One up, to the maximum.
fn range_up(m: &mut Mem, this: u32, at: u32) {
    let v = m.r8(at);
    if v < m.r8(this + 9) {
        m.w8(at, v + 1);
    }
}

/// One down, to the minimum.
fn range_down(m: &mut Mem, this: u32, at: u32) {
    let v = m.r8(at);
    if m.r8(this + 8) < v {
        m.w8(at, v - 1);
    }
}

/// `tListIteratorRange::Increment` (FEMENU.CPP, 0x800245f4).
pub fn range_increment(m: &mut Mem, this: u32) {
    let at = m.r32(this + 4);
    range_up(m, this, at);
}

pub fn abi_range_increment(c: &mut Cpu) {
    range_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorRange::Decrement` (FEMENU.CPP, 0x8002461c).
pub fn range_decrement(m: &mut Mem, this: u32) {
    let at = m.r32(this + 4);
    range_down(m, this, at);
}

pub fn abi_range_decrement(c: &mut Cpu) {
    range_decrement(&mut c.m, c.r[4]);
}

// tListIteratorRangeIndexed: a range, the value at `fValue[*fIndex]`.

/// `tListIteratorRangeIndexed::tListIteratorRangeIndexed` (FEMENU.CPP, 0x80024644).
pub fn range_indexed(c: &mut Cpu, this: u32, min: u32, max: u32, value: u32, index: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[this, index]);
        r.enter(c, 0x8002_4554, 0x8002_4668, |c| range(c, this, min & 0xFF, max & 0xFF, value));
        c.m.w32(this + 12, RANGE_INDEXED_VT);
        c.m.w32(this + 16, index);
        this
    })
}

pub fn abi_range_indexed(c: &mut Cpu) {
    let index = abi::arg(c, 4);
    c.r[2] = range_indexed(c, c.r[4], c.r[5], c.r[6], c.r[7], index);
}

/// `tListIteratorRangeIndexed::~tListIteratorRangeIndexed` (FEMENU.CPP, 0x80024690).
pub fn abi_range_indexed_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITERATOR_VPTR, RANGE_INDEXED_VT, 0x8002_45b0, 0x8002_46a8, range_dtor);
}

/// `tListIteratorRangeIndexed::Value` (FEMENU.CPP, 0x800246b8).
pub fn abi_range_indexed_value(c: &mut Cpu) {
    c.r[2] = indexed_value(&mut c.m, c.r[4]);
}

/// `tListIteratorRangeIndexed::Increment` (FEMENU.CPP, 0x800246dc).
pub fn range_indexed_increment(m: &mut Mem, this: u32) {
    let at = indexed_at(m, this);
    range_up(m, this, at);
}

pub fn abi_range_indexed_increment(c: &mut Cpu) {
    range_indexed_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorRangeIndexed::Decrement` (FEMENU.CPP, 0x80024718).
pub fn range_indexed_decrement(m: &mut Mem, this: u32) {
    let at = indexed_at(m, this);
    range_down(m, this, at);
}

pub fn abi_range_indexed_decrement(c: &mut Cpu) {
    range_indexed_decrement(&mut c.m, c.r[4]);
}

// tMenuItem: flags (+0: bit 0 disabled), its text id (+4), the selection's fade (+8, 0..128).

/// `tMenuItem::tMenuItem` (FEMENU.CPP, 0x80024754): an item showing `text`.
pub fn menu_item(m: &mut Mem, this: u32, text: u32) -> u32 {
    m.w32(this + 24, MENU_ITEM_VT);
    m.w32(this + 4, text);
    m.w32(this, 0);
    m.w16(this + 8, 0);
    m.w32(this + 12, u32::MAX);
    m.w32(this + 16, 0);
    m.w32(this + 20, 0);
    this
}

pub fn abi_menu_item(c: &mut Cpu) {
    c.r[2] = menu_item(&mut c.m, c.r[4], c.r[5]);
}

/// `tMenuItem::~tMenuItem` (FEMENU.CPP, 0x80024784).
pub fn menu_item_dtor(c: &mut Cpu, this: u32, flags: u32) {
    destroy_root(c, this, flags, ITEM_VPTR, MENU_ITEM_VT, 0x8002_47a8);
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
pub fn menu_item_update_transition(c: &mut Cpu, this: u32, selected: u32) {
    abi::frame(c, 24, |c, _| update_sel_fade(&mut c.m, this, selected));
}

pub fn abi_menu_item_update_transition(c: &mut Cpu) {
    menu_item_update_transition(c, c.r[4], c.r[5]);
}

/// `tMenuItem::TransitionIsFinished`, `tMenu::TransitionIsFinished` (FEMENU.CPP, 0x800247e8,
/// 0x80025a6c): always.
pub fn abi_finished(c: &mut Cpu) {
    c.r[2] = 1;
}

/// `tMenuItem::UpdateSelFade` (FEMENU.CPP, 0x800247f0): the selection fades in (64 a frame)
/// while selected, out (10) when not, within 0..128.
pub fn update_sel_fade(m: &mut Mem, this: u32, selected: u32) {
    let v = m.r16(this + 8);
    m.w16(this + 8, if selected != 0 { v.wrapping_add(64) } else { v.wrapping_sub(10) });
    if m.r16(this + 8) as i16 > 128 {
        m.w16(this + 8, 128);
    }
    if (m.r16(this + 8) as i16) < 0 {
        m.w16(this + 8, 0);
    }
}

pub fn abi_update_sel_fade(c: &mut Cpu) {
    update_sel_fade(&mut c.m, c.r[4], c.r[5]);
}

/// `tMenuItem::Draw(int, int, bool)` (FEMENU.CPP, 0x80024848): `Draw(bool)` (the place given
/// is not used).
pub fn abi_menu_item_draw_at(c: &mut Cpu) {
    let (this, selected) = (c.r[4], c.r[7]);
    abi::frame(c, 24, |c, _| {
        let (obj, f) = method(&mut c.m, this, ITEM_VPTR, DRAW);
        Saved::of(c).call_ptr_leaving(c, f, &[obj, selected], 0x8002_4874);
    });
}

/// `tMenuItem::Draw(int, int, int, bool)` (FEMENU.CPP, 0x80024884): the three-argument
/// `Draw` (its first argument the bool, its fifth 0).
pub fn abi_menu_item_draw_at3(c: &mut Cpu) {
    let (this, a, b) = (c.r[4], c.r[5], c.r[6]);
    let selected = abi::arg(c, 4);
    abi::frame(c, 32, |c, sp| {
        c.m.w32(sp + 16, 0);
        let (obj, f) = method(&mut c.m, this, ITEM_VPTR, DRAW_AT);
        Saved::of(c).call_ptr_leaving(c, f, &[obj, selected, a, b], 0x8002_48b8);
    });
}

/// `tMenuItemInteractive::tMenuItemInteractive` (FEMENU.CPP, 0x800248c8).
pub fn interactive(c: &mut Cpu, this: u32, text: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        menu_item(&mut c.m, this, text);
        c.m.w32(this + 24, INTERACTIVE_VT);
        this
    })
}

pub fn abi_interactive(c: &mut Cpu) {
    c.r[2] = interactive(c, c.r[4], c.r[5]);
}

/// `tMenuItemInteractive::~tMenuItemInteractive` (FEMENU.CPP, 0x800248fc).
pub fn interactive_dtor(c: &mut Cpu, this: u32, _flags: u32) {
    destroy(c, this, ITEM_VPTR, INTERACTIVE_VT, 0x8002_4784, 0x8002_4914, menu_item_dtor);
}

pub fn abi_interactive_dtor(c: &mut Cpu) {
    interactive_dtor(c, c.r[4], c.r[5]);
}

/// An interactive item's constructor (the left-right choice's and slider's): the item, then
/// `data` (the iterator) at +28; `ra` the base constructor's return.
fn interactive_with(c: &mut Cpu, this: u32, text: u32, data: u32, ra: u32) {
    let mut r = Saved::of(c);
    r.0[..2].copy_from_slice(&[this, data]);
    r.enter(c, 0x8002_48c8, ra, |c| interactive(c, this, text));
}

/// `tMenuItemLeftRightChoice::tMenuItemLeftRightChoice` (FEMENU.CPP, 0x80024924): an item
/// choosing through `data` with left and right.
pub fn left_right_choice(c: &mut Cpu, this: u32, text: u32, data: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        interactive_with(c, this, text, data, 0x8002_4940);
        let m = &mut c.m;
        let flags = m.r32(this);
        m.w32(this + 24, LEFT_RIGHT_CHOICE_VT);
        m.w32(this + 28, data);
        m.w32(this, flags | 0x400);
        this
    })
}

pub fn abi_left_right_choice(c: &mut Cpu) {
    c.r[2] = left_right_choice(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightChoice::~tMenuItemLeftRightChoice` (FEMENU.CPP, 0x80024974).
pub fn abi_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITEM_VPTR, LEFT_RIGHT_CHOICE_VT, 0x8002_48fc, 0x8002_498c, interactive_dtor);
}

/// The iterator's step for left (`Decrement`) or right (`Increment`), when the item is
/// enabled; `rest(c, r)` after it. The `player` and registers as given.
fn left_right(c: &mut Cpu, r: &Saved, this: u32, player: u32, key: u32, ras: [u32; 2]) -> bool {
    let m = &mut c.m;
    if m.r32(this) & ITEM_DISABLED != 0 {
        return false;
    }
    let slot = match m.r32(key) {
        KEY_LEFT => DECREMENT,
        KEY_RIGHT => INCREMENT,
        _ => return false,
    };
    let data = m.r32(this + 28);
    let (obj, f) = method(m, data, ITERATOR_VPTR, slot);
    r.call_ptr_leaving(c, f, &[obj, player], if slot == DECREMENT { ras[0] } else { ras[1] });
    true
}

/// `tMenuItemLeftRightChoice::ProcessInput` (FEMENU.CPP, 0x8002499c): left and right step the
/// choice (with a sound) and are used up.
pub fn left_right_choice_process_input(c: &mut Cpu, this: u32, player: u32, key: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = key;
        let left = c.m.r32(key) == KEY_LEFT;
        if left_right(c, &r, this, player, key, [0x8002_4a00, 0x8002_4a2c]) {
            c.m.w32(key, KEY_NONE);
            r.call_leaving(c, PLAY_FE_SFX, &[if left { 5 } else { 6 }], 0x8002_4a3c);
        }
    })
}

pub fn abi_left_right_choice_process_input(c: &mut Cpu) {
    left_right_choice_process_input(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightChoice::Draw` (FEMENU.CPP, 0x80024a4c): the item's text, and the
/// choice's 180 to its right (for the sub-menu's player).
pub fn left_right_choice_draw(c: &mut Cpu, this: u32, selected: u32) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[3] = selected;
        let text = c.m.r32(this + 4);
        let x = r.call_leaving(c, WORD_X, &[text], 0x8002_4a78);
        r.0[0] = x;
        let text = c.m.r32(this + 4);
        let y = r.call_leaving(c, WORD_Y, &[text], 0x8002_4a84);
        let (x16, y16) = (x as i16 as i32 as u32, y as i16 as i32 as u32);
        let on = (selected != 0) as u32;
        r.0[2] = y16;
        r.0[3] = on;
        r.0[4] = 6;
        c.m.w32(sp + 16, 6);
        let id = c.m.r16(this + 4) as i16 as i32 as u32;
        r.call_leaving(c, MENU_TEXT_POSITIONED, &[id, x16, y16, on], 0x8002_4ab0);
        let data = c.m.r32(this + 28);
        let player = c.m.r32(SUB_MENU_PLAYER);
        let (obj, f) = method(&mut c.m, data, ITERATOR_VPTR, TEXT_VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, player, data], 0x8002_4ad4);
        let x = x.wrapping_add(180);
        // (s0 holds it shifted up for the sign extension)
        r.0[0] = x << 16;
        c.m.w32(sp + 16, 6);
        r.call_leaving(c, MENU_TEXT_POSITIONED, &[v as i16 as i32 as u32, x as i16 as i32 as u32, y16, on], 0x8002_4af8);
    })
}

pub fn abi_left_right_choice_draw(c: &mut Cpu) {
    left_right_choice_draw(c, c.r[4], c.r[5]);
}

/// `tMenuItemLeftRightSlider::tMenuItemLeftRightSlider` (FEMENU.CPP, 0x80024b18): a slider
/// through `data`.
pub fn left_right_slider(c: &mut Cpu, this: u32, text: u32, data: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        interactive_with(c, this, text, data, 0x8002_4b34);
        let m = &mut c.m;
        let flags = m.r32(this);
        m.w32(this + 28, data);
        m.w32(this, flags | 0x80);
        let flags = m.r32(this);
        m.w32(this + 24, LEFT_RIGHT_SLIDER_VT);
        m.w32(this, flags | 0x80);
        this
    })
}

pub fn abi_left_right_slider(c: &mut Cpu) {
    c.r[2] = left_right_slider(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightSlider::~tMenuItemLeftRightSlider` (FEMENU.CPP, 0x80024b74).
pub fn abi_left_right_slider_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITEM_VPTR, LEFT_RIGHT_SLIDER_VT, 0x8002_48fc, 0x8002_4b8c, interactive_dtor);
}

/// `tMenuItemLeftRightSlider::DebounceKeys` (FEMENU.CPP, 0x80024b9c): left and right repeat.
pub fn abi_left_right_slider_debounce_keys(c: &mut Cpu) {
    c.r[2] = 1536;
}

/// `tMenuItemLeftRightSlider::ProcessInput` (FEMENU.CPP, 0x80024ba4): left and right step the
/// slider (with a tick) and are used up.
pub fn left_right_slider_process_input(c: &mut Cpu, this: u32, player: u32, key: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = key;
        if left_right(c, &r, this, player, key, [0x8002_4c28, 0x8002_4c28]) {
            c.m.w32(key, KEY_NONE);
            r.call_ptr_leaving(c, PLAY_FE_SFX_VOL, &[21, 64], 0x8002_4c3c);
        }
    })
}

pub fn abi_left_right_slider_process_input(c: &mut Cpu) {
    left_right_slider_process_input(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightSlider::Draw` (FEMENU.CPP, 0x80025204): the item's text (its place kept
/// from the text the first time), and the slider at the value 115 to its right.
pub fn left_right_slider_draw(c: &mut Cpu, this: u32, selected: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[2] = selected;
        if c.m.r32(this + 32) == 0 {
            let text = c.m.r32(this + 4);
            let x = r.call_leaving(c, WORD_X, &[text], 0x8002_5238);
            c.m.w16(this + 32, x as u16);
            let text = c.m.r32(this + 4);
            let y = r.call_leaving(c, WORD_Y, &[text], 0x8002_5244);
            c.m.w16(this + 34, y as u16);
        }
        let on = (selected != 0) as u32;
        let m = &mut c.m;
        let id = m.r16(this + 4) as i16 as i32 as u32;
        let x = m.r16(this + 32).wrapping_add(8) as i16 as i32 as u32;
        let y = m.r16(this + 34).wrapping_add(3) as i16 as i32 as u32;
        r.0[0] = 6;
        m.w32(sp + 16, 6);
        r.call_leaving(c, MENU_TEXT_POSITIONED, &[id, x, y, on], 0x8002_527c);
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, ITERATOR_VPTR, VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8002_52a0) & 0xFF;
        let m = &mut c.m;
        let data = m.r32(this + 28);
        let (min, max) = (m.r8(data + 8) as u32, m.r8(data + 9) as u32);
        let x = m.r16(this + 32).wrapping_add(115) as i16 as i32 as u32;
        let y = m.r16(this + 34).wrapping_add(4) as i16 as i32 as u32;
        let (w, h) = (m.r16(this + 36) as i16 as i32 as u32, m.r16(this + 38) as i16 as i32 as u32);
        let fade = if selected != 0 { 128 } else { 0 };
        let args = [v, min, max, x, y, w, h, 6, 4, 0, 0, fade, 0];
        r.call_ptr_leaving(c, DRAW_SLIDER, &args, 0x8002_5318);
    })
}

pub fn abi_left_right_slider_draw(c: &mut Cpu) {
    left_right_slider_draw(c, c.r[4], c.r[5]);
}

/// `tMenuItemLeftRightSlider::SetDimensions` (FEMENU.CPP, 0x80025330): the slider's place and
/// size.
pub fn set_dimensions(m: &mut Mem, this: u32, x: u32, y: u32, w: u32, h: u32) {
    m.w16(this + 32, x as u16);
    m.w16(this + 34, y as u16);
    m.w16(this + 36, w as u16);
    m.w16(this + 38, h as u16);
}

pub fn abi_set_dimensions(c: &mut Cpu) {
    let h = abi::arg(c, 4);
    set_dimensions(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], h);
}

/// `tMenuItemGoToMenuButton::tMenuItemGoToMenuButton` (FEMENU.CPP, 0x80025348): a button
/// going to `menu` and calling `on_press`.
pub fn go_to_menu_button(c: &mut Cpu, this: u32, text: u32, menu: u32, on_press: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, menu, on_press]);
        r.enter(c, 0x8002_48c8, 0x8002_536c, |c| interactive(c, this, text));
        let m = &mut c.m;
        m.w32(this + 24, GO_TO_MENU_BUTTON_VT);
        m.w32(this + 20, menu);
        m.w32(this + 28, on_press);
        this
    })
}

pub fn abi_go_to_menu_button(c: &mut Cpu) {
    c.r[2] = go_to_menu_button(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemGoToMenuButton::~tMenuItemGoToMenuButton` (FEMENU.CPP, 0x8002539c).
pub fn abi_go_to_menu_button_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], ITEM_VPTR, GO_TO_MENU_BUTTON_VT, 0x8002_48fc, 0x8002_53b4, interactive_dtor);
}

/// `tMenuItemGoToMenuButton::ProcessInput` (FEMENU.CPP, 0x800253c4): accept goes to the menu
/// (flag 0x40: as an accept, else a go-to) and calls the button's function; used up.
pub fn go_to_menu_button_process_input(c: &mut Cpu, this: u32, key: u32, command: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = key;
        let m = &mut c.m;
        let flags = m.r32(this);
        if flags & ITEM_DISABLED != 0 || m.r32(key) != KEY_ACCEPT {
            return;
        }
        if m.r32(this + 20) != 0 {
            m.w32(command, if flags & 0x40 != 0 { KEY_ACCEPT } else { COMMAND_GO_TO });
            let menu = m.r32(this + 20);
            m.w32(command + 4, menu);
        }
        let on_press = m.r32(this + 28);
        if on_press != 0 {
            r.call_ptr_leaving(c, on_press, &[command, flags, key, command], 0x8002_543c);
        }
        c.m.w32(key, KEY_NONE);
    })
}

pub fn abi_go_to_menu_button_process_input(c: &mut Cpu) {
    go_to_menu_button_process_input(c, c.r[4], c.r[6], c.r[7]);
}

// tMenu: flags (+0: 4 back accepts, 0x10000 accept backs, 1 and 2 back out), its title (+4),
// the current item (+8), its items (+16, to a null), the menus after (+84), for options (+92)
// and the function for back (+96).

/// The menu's `k`th item.
fn item(m: &mut Mem, this: u32, k: u32) -> u32 {
    m.r32(this.wrapping_add(16).wrapping_add(k.wrapping_mul(4)))
}

fn current(m: &mut Mem, this: u32) -> u32 {
    m.r32(this + 8)
}

fn set_current(m: &mut Mem, this: u32, k: u32) {
    m.w32(this + 8, k)
}

/// `tMenu::tMenuConstructor` (FEMENU.CPP, 0x80025454): the items: `first`, then those in the
/// list at `rest` to a null (stored too).
pub fn menu_constructor(m: &mut Mem, this: u32, first: u32, rest: u32) {
    m.w16(this + 100, 0);
    m.w32(this + 16, first);
    let (mut k, mut at) = (1u32, rest);
    loop {
        let v = m.r32(at);
        at = at.wrapping_add(4);
        m.w32(this.wrapping_add(16).wrapping_add(k.wrapping_mul(4)), v);
        if v == 0 {
            return;
        }
        k = k.wrapping_add(1);
    }
}

pub fn abi_menu_constructor(c: &mut Cpu) {
    menu_constructor(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tMenu::tMenu` (FEMENU.CPP, 0x8002548c): a menu of `screen`, its flags, the menus after and
/// for options, the function for back, its title.
#[allow(clippy::too_many_arguments)]
pub fn menu(m: &mut Mem, this: u32, flags: u32, screen: u32, next: u32, options: u32, on_back: u32, title: u32) -> u32 {
    m.w32(this + 104, MENU_VT);
    m.w32(this, flags);
    m.w32(this + 8, 0);
    m.w32(this + 80, screen);
    m.w32(this + 84, next);
    m.w32(this + 12, 0);
    m.w32(this + 88, 0);
    m.w32(this + 92, options);
    m.w32(this + 96, on_back);
    m.w16(this + 4, title as u16);
    this
}

pub fn abi_menu(c: &mut Cpu) {
    let (options, on_back, title) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    c.r[2] = menu(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], options, on_back, title);
    // (a0 left holding the fifth argument, as the original leaves it)
    c.r[4] = options;
}

/// `tMenu::~tMenu` (FEMENU.CPP, 0x800254d0).
pub fn abi_menu_dtor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    destroy_root(c, this, flags, 104, MENU_VT, 0x8002_54f4);
}

/// `tMenu::Initialize` (FEMENU.CPP, 0x80025504): the current item moved on past disabled ones
/// (round to the first, at most once round).
pub fn initialize(m: &mut Mem, this: u32) {
    let start = current(m, this);
    let disabled = |m: &mut Mem, k: u32| {
        let i = item(m, this, k);
        m.r32(i) & ITEM_DISABLED != 0
    };
    if !disabled(m, start) {
        return;
    }
    loop {
        let k = current(m, this);
        if !disabled(m, k) {
            return;
        }
        set_current(m, this, k + 1);
        if item(m, this, k + 1) == 0 {
            set_current(m, this, 0);
        }
        if current(m, this) == start {
            return;
        }
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, c.r[4]);
}

/// `tMenu::ProcessInput` (FEMENU.CPP, 0x80025598): the current item's input first; then up and
/// down move the current item (past disabled ones, round, with a sound when it moves), accept
/// goes to the menu after, back calls the menu's function (or backs out), options goes to the
/// options menu, help shows the help, start starts. A key the menu takes is used up.
pub fn process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, command, key]);
        let m = &mut c.m;
        let flags = m.r32(this);
        if flags & 4 != 0 && m.r32(key) == KEY_BACK {
            m.w32(key, KEY_ACCEPT);
        }
        if m.r32(this) & 0x1_0000 != 0 && m.r32(key) == KEY_ACCEPT {
            m.w32(key, KEY_BACK);
        }
        let cur = current(m, this);
        let it = item(m, this, cur);
        if it != 0 {
            let (obj, f) = method(m, it, ITEM_VPTR, PROCESS_INPUT);
            r.call_ptr_leaving(c, f, &[obj, player, key, command], 0x8002_5640);
        }
        let m = &mut c.m;
        let disabled = |m: &mut Mem, k: u32| {
            let i = item(m, this, k);
            m.r32(i) & ITEM_DISABLED != 0
        };
        match m.r32(key) {
            KEY_START => {
                m.w32(command, COMMAND_START);
                m.w32(key, KEY_NONE);
            }
            KEY_UP => {
                let was = current(m, this);
                loop {
                    let k = current(m, this);
                    if (k as i32) > 0 {
                        set_current(m, this, k - 1);
                    } else if item(m, this, k.wrapping_add(1)) != 0 {
                        // round to the last
                        let mut k = k;
                        loop {
                            set_current(m, this, k.wrapping_add(1));
                            if item(m, this, k.wrapping_add(2)) == 0 {
                                break;
                            }
                            k = current(m, this);
                        }
                    }
                    let k = current(m, this);
                    if !disabled(m, k) {
                        break;
                    }
                }
                if current(m, this) != was {
                    r.call_leaving(c, PLAY_FE_SFX, &[3], 0x8002_5768);
                }
                c.m.w32(key, KEY_NONE);
            }
            KEY_DOWN => {
                let was = current(m, this);
                loop {
                    let k = current(m, this).wrapping_add(1);
                    set_current(m, this, k);
                    if item(m, this, k) == 0 {
                        set_current(m, this, 0);
                    }
                    let k = current(m, this);
                    if !disabled(m, k) {
                        break;
                    }
                }
                if current(m, this) != was {
                    r.call_leaving(c, PLAY_FE_SFX, &[4], 0x8002_57dc);
                }
                c.m.w32(key, KEY_NONE);
            }
            KEY_ACCEPT if m.r32(this + 84) != 0 => {
                m.w32(command, COMMAND_GO_TO);
                let next = m.r32(this + 84);
                m.w32(command + 4, next);
                m.w32(key, KEY_NONE);
            }
            KEY_BACK => {
                let on_back = m.r32(this + 96);
                if on_back != 0 {
                    r.call_ptr_leaving(c, on_back, &[command], 0x8002_581c);
                    c.m.w32(key, KEY_NONE);
                } else {
                    let flags = m.r32(this);
                    if flags & 1 != 0 {
                        m.w32(command, COMMAND_BACK);
                        m.w32(key, KEY_NONE);
                    } else if flags & 2 != 0 {
                        m.w32(command, COMMAND_EXIT);
                        m.w32(key, KEY_NONE);
                    }
                }
            }
            KEY_HELP => {
                let app = m.r32(FE_APP);
                r.call_leaving(c, DISPLAY_HELP, &[app, 0], 0x8002_585c);
            }
            KEY_OPTIONS if m.r32(this + 92) != 0 => {
                m.w32(command, COMMAND_GO_TO);
                let options = m.r32(this + 92);
                m.w32(command + 4, options);
                m.w32(key, KEY_NONE);
            }
            _ => {}
        }
    })
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenu::GetNumberEnabledItems` (FEMENU.CPP, 0x800258b4): the items not disabled (none when
/// `fNeverAnyEnabled`).
pub fn get_number_enabled_items(m: &mut Mem, this: u32) -> u32 {
    if m.r32(this + 12) != 0 {
        return 0;
    }
    let mut n = 0u32;
    let mut k = 0u32;
    loop {
        let i = item(m, this, k as i16 as i32 as u32);
        if i == 0 {
            return n as i16 as i32 as u32;
        }
        if m.r32(i) & ITEM_DISABLED == 0 {
            n = n.wrapping_add(1);
        }
        k = k.wrapping_add(1);
    }
}

pub fn abi_get_number_enabled_items(c: &mut Cpu) {
    c.r[2] = get_number_enabled_items(&mut c.m, c.r[4]);
}

/// Each item's virtual `slot` (`Draw(bool)`, `UpdateTransition(bool)`), told whether it is
/// the current one (`Draw`'s and `UpdateTransition`'s loop); `ra` the call's return.
fn each_item(c: &mut Cpu, r: &mut Saved, this: u32, slot: u32, ra: u32) {
    if item(&mut c.m, this, 0) == 0 {
        return;
    }
    let mut k = 0u32;
    loop {
        r.0[0] = k;
        let m = &mut c.m;
        let i = (k as i16) as i32;
        let it = item(m, this, i as u32);
        let selected = (i as u32 == current(m, this)) as u32;
        let vt = m.r32(it + ITEM_VPTR);
        let (obj, f) = method(m, it, ITEM_VPTR, slot);
        r.call_ptr_leaving(c, f, &[obj, selected, vt], ra);
        k = k.wrapping_add(1);
        if item(&mut c.m, this, k as i16 as i32 as u32) == 0 {
            return;
        }
    }
}

/// `tMenu::Draw` (FEMENU.CPP, 0x8002591c): its title (if it has one) and its items.
pub fn draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let title = c.m.r16(this + 4) as i16 as i32;
        if title >= 0 {
            r.call_leaving(c, TITLE, &[title as u32], 0x8002_5948);
        }
        r.0[0] = 0;
        each_item(c, &mut r, this, DRAW, 0x8002_5994);
    })
}

pub fn abi_draw(c: &mut Cpu) {
    draw(c, c.r[4]);
}

/// `tMenu::UpdateTransition` (FEMENU.CPP, 0x800259c8): each item's.
pub fn update_transition(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = 0;
        each_item(c, &mut r, this, UPDATE_TRANSITION, 0x8002_5a28);
    })
}

pub fn abi_update_transition(c: &mut Cpu) {
    update_transition(c, c.r[4]);
}

/// `tMenu::IsSubMenu` (FEMENU.CPP, 0x80025a74): not.
pub fn abi_is_sub_menu(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `tMenu::DebounceKeys` (FEMENU.CPP, 0x80025a7c): the current item's.
pub fn debounce_keys(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let cur = current(&mut c.m, this);
        let it = item(&mut c.m, this, cur);
        let (obj, f) = method(&mut c.m, it, ITEM_VPTR, DEBOUNCE_KEYS);
        Saved::of(c).call_ptr_leaving(c, f, &[obj], 0x8002_5ab8)
    })
}

pub fn abi_debounce_keys(c: &mut Cpu) {
    c.r[2] = debounce_keys(c, c.r[4]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};
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
    /// The stack's top in the front end's tests (main RAM).
    pub(crate) const TOP: u32 = 0x801F_FE00;

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
        let keys = [KEY_NONE, KEY_ACCEPT, KEY_HELP, KEY_OPTIONS, KEY_START, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_BACK, 32];
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

    /// The functions with frames leave the stack as the original does (their callees' frames
    /// and saved registers, the registers the virtual calls get).
    #[test]
    fn femenu_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            let frame = (0..8).map(|i| ram[(addr & 0x1F_FFFF) as usize + 4 * i..][..4].to_vec()).any(|w| w[2..4] == [0xBD, 0x27]);
            if !frame || std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 200, 0xFE51 + k as u64, shuffle, &args);
        }
    }
}
