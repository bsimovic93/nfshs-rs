//! FEMENUOPTIONS.CPP: the options screens' menus and items.
//! - The pulsing yellow and the flare left of the current item.
//! - Items that fade in and out: buttons, left-right choices (the display's, the controller's,
//!   on-off ones), audio sliders.
//! - The options menu: its items fade in one after the other and out the other way, the
//!   screen fades, its picture animates.
//! - The sliding boxes that open into an inside menu: the songs', sliders, the analog sticks'
//!   calibration.
//! - The user name's letter grid, and the memory card's button.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{self, ITEM_DISABLED, ITEM_VPTR, KEY_DOWN, KEY_HELP, KEY_NONE, KEY_BACK, KEY_UP};
use crate::femenuextended::{
    self, CALC_ON_OFF_FADE, CALC_TEXT_FADE_SEL_TO_HI, DRAW_SHAPE_EXTENDED, DRAW_SQUARE, GET_DRAWENV, ITEM_DRAW_AT, ITEM_TRANSITION_IS_FINISHED, ITEM_TRANSITION_OFF, ITEM_TRANSITION_ON,
    ITEM_UPDATE_TRANSITION, MENU_TRANSITION_IS_FINISHED, MENU_VPTR, SET_DRAW_AREA, SPRINTF, STRLEN, TEXT_WORD,
};
use crate::fetextrender;
use crate::globals::{
    BOX_HELP_TEXT, FE_APP, FLIP, FRONT_END, GAME_TICKS, MUSIC_STATE, PAD_INFO, PLAYER1_VIEW, PULSATE_YELLOW, SCREEN_HEIGHT, SHAPES, SLIDING_FLARE_PHASE, SONGS_SCREEN, SUB_MENU_PLAYER, TEXT_COLORS,
    TEXT_STYLES,
};
use crate::types::{
    DRAWENV, FEApplication, InsideBoxMenu, InsideBoxSongMenu, ListIterator, Menu, MenuCommand, MenuItem, MenuItemGoToMenuButtonFade, MenuItemLeftRightAudioSlider,
    MenuItemLeftRightFade, MenuItemLeftRightSlider, MenuItemOnOffLeftRightChoice, MenuItemSlidingActivated, MenuItemSlidingMenu, MemoryCardMenuItem, InsideBoxTwoWaySlider,
    OptionsMenu, RECT, UserNameMenuItem,
};
use ps1rt::{Cpu, Mem};

/// The vtables.
const LEFT_RIGHT_FADE_VT: u32 = 0x8001_0F58;
const OPTIONS_MENU_VT: u32 = 0x8001_0F00;
const INSIDE_BOX_MENU_VT: u32 = 0x8001_0EA0;
const SLIDING_MENU_VT: u32 = 0x8001_0E40;
const AUDIO_SLIDER_VT: u32 = 0x8001_0CD8;
const SONG_MENU_VT: u32 = 0x8001_0C70;
const INSIDE_SLIDER_VT: u32 = 0x8001_0BC0;
const TWO_WAY_SLIDER_VT: u32 = 0x8001_0B68;
const USER_NAME_VT: u32 = 0x8001_0B10;

/// `CalcFadeVal` (a colour and a fade); `TextSys_WordX`; `Flare_2DHalo` (the host captures its
/// primitives: called by address).
const CALC_FADE_VAL: fn(&mut Cpu) = nfs4rc::f_8004bd68;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const FLARE_2D_HALO: u32 = 0x800C_ECD0;
/// `CalcFadeVal` (a colour between two by the selection, faded), `TextSys_WordY`;
/// `SubtractiveBox` (the host captures its primitives: called by address).
const CALC_FADE_VAL_FADED: fn(&mut Cpu) = nfs4rc::f_8004bd8c;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// `TextSys_WordFlags` (a text's text type); `DrawSlider` (the host captures its primitives:
/// called by address).
const WORD_FLAGS: fn(&mut Cpu) = nfs4rc::f_800b9f38;
const DRAW_SLIDER: u32 = 0x8002_4C4C;
/// `CalcFadeVal` (a colour between two by the selection), `CalcTextFadeUnselToSel`.
const CALC_FADE_VAL_SEL: fn(&mut Cpu) = nfs4rc::f_8004bcd8;
const CALC_TEXT_FADE_UNSEL_TO_SEL: fn(&mut Cpu) = nfs4rc::f_8004bdc0;
/// The calibration's help text (0x80045c94, of the word at 0x80052d60).
const CALIBRATE_HELP: fn(&mut Cpu) = nfs4rc::f_80045c94;
const CALIBRATE_HELP_ARG: u32 = 0x8005_2D60;
/// The key that ends a calibration.
const KEY_STOP: u32 = 128;
/// The pads' kinds (their state's +5): the NeGcon, the analog joystick, the DualShock.
const PAD_NEGCON: u32 = 0x23;
const PAD_ANALOG: u32 = 0x53;
const PAD_DUAL_SHOCK: u32 = 0x73;
/// `FECheat_ActivateCheat`, `FECheat_ActivateBonusByCode`; the records' case for a name
/// (`Stattool_SamNelsonsUpperLowerStringConverterForRecords`); `FeAudio_AsyncPlaySpeech`.
const ACTIVATE_CHEAT: fn(&mut Cpu) = nfs4rc::f_80023634;
const ACTIVATE_BONUS: fn(&mut Cpu) = nfs4rc::f_80023800;
const CASE_FOR_RECORDS: fn(&mut Cpu) = nfs4rc::f_8004b0ac;
const PLAY_SPEECH: fn(&mut Cpu) = crate::feaudio::async_play_speech_entry;
/// The name grid: its rows' texts (from 508), how many rows each language has (6 shorts), the
/// rows now (`gNameRows`) and the grid's top; the accented letters' jump table.
const GRID_TEXTS: u32 = 508;
const NAME_ROWS_BY_LANGUAGE: u32 = 0x8001_09F8;
const NAME_ROWS: u32 = 0x8005_2E40;
const NAME_GRID_TOP: u32 = 0x8005_2E42;
const SPECIAL_CHARACTERS: u32 = 0x8001_0988;
/// Set while the memory card is busy (the name's fade waits for it).
const CARD_BUSY: u32 = 0x8005_19E0;
/// ON, OFF.
const TEXT_ON: u32 = 103;
const TEXT_OFF: u32 = 104;
const SUBTRACTIVE_BOX: u32 = 0x8001_C25C;
/// The music: `AudioMus_StopSong`, `AudioMus_SysCleanUp`, `AudioMus_SysStartUp`, the song list's
/// loading (0x8007b214), `purgememadr`; `AudioMus_PlaySong` (the host logs it: called by
/// address). The files and songs: the menu's ("amus", "zmenu*"), the game's ("ymus", "game*")
/// and its song list.
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
const MUSIC_CLEAN_UP: fn(&mut Cpu) = nfs4rc::f_8007b720;
const MUSIC_START_UP: fn(&mut Cpu) = nfs4rc::f_8007b5ac;
const LOAD_SONG_LIST: fn(&mut Cpu) = nfs4rc::f_8007b214;
const PURGE_MEM: fn(&mut Cpu) = nfs4rc::f_800e612c;
const PLAY_SONG: u32 = 0x8007_B9C4;
const MENU_MUSIC_FILE: u32 = 0x8001_0964;
const MENU_SONGS: u32 = 0x8001_096C;
const GAME_MUSIC_FILE: u32 = 0x8001_0974;
const GAME_SONGS: u32 = 0x8001_097C;
const SONG_LIST_FILE: u32 = 0x8001_0984;

/// The text whose place the flare is drawn left of.
const FLARE_TEXT: u32 = 479;
/// An options menu's fade over the screen at its full (its transition's end).
const SCREEN_FADE_FULL: i32 = 552;
/// A screen's vptr and its `TransitionIsFinished`.
const SCREEN_VPTR: u32 = 96;
const SCREEN_TRANSITION_IS_FINISHED: u32 = 64;
/// The items' `DebounceKeys`, `ProcessInput`, `Draw(int, int, int, bool)` (their vtable's).
const ITEM_DEBOUNCE_KEYS: u32 = 16;
const ITEM_PROCESS_INPUT: u32 = 24;
const ITEM_DRAW_AT3: u32 = 48;
/// The menus' `DebounceKeys`, `ProcessInput`; a sliding box's `UpdatefOpenHeight`; the inside
/// menus' `Draw(short, short, short, short, short)` and the songs' `DrawOneSong`.
const MENU_DEBOUNCE_KEYS: u32 = 32;
const MENU_PROCESS_INPUT: u32 = 24;
const SLIDING_UPDATE_OPEN_HEIGHT: u32 = 88;
const MENU_DRAW_BOX: u32 = 88;
const MENU_DRAW_ONE_SONG: u32 = 96;
/// An item's flag: it is a slider (FEMENU's), set on the sliding boxes too.
const ITEM_SLIDER: u32 = 0x80;
/// `DrawShapeExtended`'s flags for the menu's animated picture; its tint.
const PICTURE_FLAGS: u32 = 1040;
const PICTURE_COLOUR: u32 = 0x00CE_C844;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// A menu's `k`th item (`k` taken as a short, as the original's loops do; read past the 16 of
/// `item_list` as the original does).
fn item(m: &mut Mem, this: Menu, k: u32) -> MenuItem {
    MenuItem(m.r32(this.0.wrapping_add(16).wrapping_add(sx(k) << 2)))
}

/// Each item in order, from the first to the null (a short counter).
fn items(c: &mut Cpu, this: Menu, mut each: impl FnMut(&mut Cpu, u32, MenuItem)) {
    let mut k = 0u32;
    loop {
        let it = item(&mut c.m, this, k);
        if it.0 == 0 {
            return;
        }
        each(c, sx(k), it);
        k = k.wrapping_add(1);
    }
}

fn is_disabled(m: &mut Mem, it: MenuItem) -> bool {
    it.flags(m) & ITEM_DISABLED != 0
}

/// The front end's screen now (the first player's).
fn current_screen(m: &mut Mem) -> u32 {
    FEApplication(m.r32(FE_APP)).current_screen(m, 0).0
}

/// The address of a caller's stack argument `n` (from 0): the variadic constructors' item
/// lists.
fn stack_arg(c: &Cpu, n: u32) -> u32 {
    c.r[29].wrapping_add(4 * n)
}

/// `CalcPulsateYellow` (FEMENUOPTIONS.CPP, 0x8001c0a8): the yellow pulsing with the clock
/// (up for 64 ticks of 128, down for the rest).
pub fn calc_pulsate_yellow(c: &mut Cpu) {
    let t = (c.m.r32(GAME_TICKS) as i32) % 128;
    let f = if t >= 65 { 128 - t } else { t };
    let v = call(c, CALC_FADE_VAL, &[0xBEBE, f as u32]);
    c.m.w32(PULSATE_YELLOW, v);
}

pub fn abi_calc_pulsate_yellow(c: &mut Cpu) {
    calc_pulsate_yellow(c);
}

/// `DrawLeftFlare` (FEMENUOPTIONS.CPP, 0x8001c100): the flare left of the current item at `y`.
/// Its phase (`*phase`, 0..60) counts on while the item is fully selected (`sel` 128) and winds
/// back out after; a halo grows and shrinks with it and a shape is drawn, both by the selection
/// and the item's `fade`.
pub fn draw_left_flare(c: &mut Cpu, y: u32, sel: u32, fade: u32, phase: u32) {
    let m = &mut c.m;
    if sel == 128 {
        let v = m.r32(phase).wrapping_add(1);
        m.w32(phase, v);
    } else if m.r32(phase) != 0 {
        let v = m.r32(phase).wrapping_add(5);
        m.w32(phase, v);
    }
    if m.r32(phase) as i32 >= 61 {
        m.w32(phase, 0);
    }
    let p = m.r32(phase) as i32;
    let half = if p < 31 { p / 2 } else { (60 - p) / 2 };
    let size = (half + 20).wrapping_mul(sel as i32).wrapping_mul(128i32.wrapping_sub(fade as i32)) / 128;
    let shade = (128i32.wrapping_sub(sel as i32)).max(fade as i32);
    if size <= 0 {
        return;
    }
    let x = call(c, WORD_X, &[FLARE_TEXT]);
    let twice = size.wrapping_mul(2);
    call_addr(c, FLARE_2D_HALO, &[x, y.wrapping_add(5), (size / 2) as u32, (twice / 3) as u32, 23]);
    call(c, DRAW_SHAPE_EXTENDED, &[0, 0, x.wrapping_sub(3), y.wrapping_sub(1), shade as u32, (shade != 0) as u32, 0]);
}

pub fn abi_draw_left_flare(c: &mut Cpu) {
    draw_left_flare(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

// ---- The fading items.

/// A fading item's fade: its value (0 shown .. 128 gone), its step a frame, and whether it is
/// still fading. Each class keeps it at its own place.
#[derive(Clone, Copy)]
struct Fade(u32);

impl Fade {
    fn of_button(b: MenuItemGoToMenuButtonFade) -> Self {
        Fade(b.0 + 0x20)
    }
    fn of_choice(c: MenuItemLeftRightFade) -> Self {
        Fade(c.0 + 0x20)
    }
    fn of_sliding(s: MenuItemSlidingMenu) -> Self {
        Fade(s.0 + 0x2c)
    }
    fn of_audio(a: MenuItemLeftRightAudioSlider) -> Self {
        Fade(a.0 + 0x28)
    }
    fn value(self, m: &mut Mem) -> i16 {
        m.r16(self.0) as i16
    }
    fn set_value(self, m: &mut Mem, v: i16) {
        m.w16(self.0, v as u16)
    }
    fn step(self, m: &mut Mem) -> i16 {
        m.r16(self.0 + 2) as i16
    }
    fn set_step(self, m: &mut Mem, v: i16) {
        m.w16(self.0 + 2, v as u16)
    }
    fn fading(self, m: &mut Mem) -> u32 {
        m.r32(self.0 + 4)
    }
    fn set_fading(self, m: &mut Mem, v: u32) {
        m.w32(self.0 + 4, v)
    }
}

/// A fade's step, either way.
const FADE_STEP: i16 = 30;

/// A fading item's `TransitionIsFinished`: whether the fade has reached its end (noting that
/// it still fades while it hasn't).
fn fade_is_finished(m: &mut Mem, f: Fade) -> u32 {
    let (value, step) = (f.value(m), f.step(m));
    let fading = (step < 0 && value > 0) || (step > 0 && value < 128);
    f.set_fading(m, fading as u32);
    f.fading(m) ^ 1
}

/// A fading item's `UpdateTransition`: the fade stepped within 0..128, its virtual
/// `TransitionIsFinished` (which notes whether it still fades), then `tMenuItem`'s.
fn fade_update_transition(c: &mut Cpu, item: MenuItem, f: Fade, selected: u32) {
    let m = &mut c.m;
    let v = (f.value(m) as i32 + f.step(m) as i32).clamp(0, 128);
    f.set_value(m, v as i16);
    call_virtual(c, item.0, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED, &[]);
    femenu::menu_item_update_transition(c, item, selected);
}

/// `tMenuItemGoToMenuButtonFade::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c3bc): fading out.
pub fn abi_button_fade_transition_off(c: &mut Cpu) {
    Fade::of_button(MenuItemGoToMenuButtonFade(c.r[4])).set_step(&mut c.m, FADE_STEP);
}

/// `tMenuItemGoToMenuButtonFade::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c3c8): fading in from
/// 128, unselected, the enabled fade from 0.
pub fn abi_button_fade_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemGoToMenuButtonFade(c.r[4]));
    let f = Fade::of_button(this);
    f.set_value(m, 128);
    f.set_step(m, -FADE_STEP);
    this.set_sel_fade(m, 0);
    this.set_enable_val(m, 0);
}

/// `tMenuItemGoToMenuButtonFade::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c3e4).
pub fn abi_button_fade_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, Fade::of_button(MenuItemGoToMenuButtonFade(c.r[4])));
}

/// `tMenuItemGoToMenuButtonFade::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c43c).
pub fn abi_button_fade_update_transition(c: &mut Cpu) {
    let this = MenuItemGoToMenuButtonFade(c.r[4]);
    fade_update_transition(c, MenuItem(this.0), Fade::of_button(this), c.r[5]);
}

/// `tMenuItemLeftRightFade::tMenuItemLeftRightFade` (FEMENUOPTIONS.CPP, 0x8001c4b8): a
/// left-right choice that fades, its flare's phase 0.
pub fn left_right_fade(c: &mut Cpu, this: MenuItemLeftRightFade, text: u32, data: ListIterator) -> u32 {
    femenu::left_right_choice(c, this.base(), text, data);
    let m = &mut c.m;
    this.set_vtable(m, LEFT_RIGHT_FADE_VT);
    this.set_flareextra(m, 0);
    this.set_sel_fade(m, 0);
    this.0
}

pub fn abi_left_right_fade(c: &mut Cpu) {
    c.r[2] = left_right_fade(c, MenuItemLeftRightFade(c.r[4]), c.r[5], ListIterator(c.r[6]));
}

/// `tMenuItemLeftRightFade::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c4f4): fading out from 0.
pub fn abi_left_right_fade_transition_off(c: &mut Cpu) {
    let (m, f) = (&mut c.m, Fade::of_choice(MenuItemLeftRightFade(c.r[4])));
    f.set_fading(m, 1);
    f.set_step(m, FADE_STEP);
    f.set_value(m, 0);
}

/// `tMenuItemLeftRightFade::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c50c): fading in from 128,
/// unselected, the flare from its start.
pub fn abi_left_right_fade_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemLeftRightFade(c.r[4]));
    let f = Fade::of_choice(this);
    f.set_fading(m, 1);
    f.set_step(m, -FADE_STEP);
    this.set_flareextra(m, 0);
    this.set_sel_fade(m, 0);
    f.set_value(m, 128);
}

/// `tMenuItemLeftRightFade::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c530).
pub fn abi_left_right_fade_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, Fade::of_choice(MenuItemLeftRightFade(c.r[4])));
}

/// `tMenuItemLeftRightFade::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c598).
pub fn abi_left_right_fade_update_transition(c: &mut Cpu) {
    let this = MenuItemLeftRightFade(c.r[4]);
    fade_update_transition(c, MenuItem(this.0), Fade::of_choice(this), c.r[5]);
}

// ---- tOptionsMenu: its items fade in one after the other and out the other way; the screen
// fades over them; its picture animates.

/// A screen's fade's step a frame (out: up; in: down).
const SCREEN_FADE_STEP: i8 = 40;

/// `tOptionsMenu::tOptionsMenu` (FEMENUOPTIONS.CPP, 0x8001c614): `tMenu`'s with a picture of
/// `frames` frames from `first_frame`, its items from the caller's list.
pub fn abi_options_menu(c: &mut Cpu) {
    let this = OptionsMenu(c.r[4]);
    let (flags, screen, next) = (c.r[5], c.r[6], c.r[7]);
    let arg = |c: &mut Cpu, n| crate::abi::arg(c, n);
    let (options, on_start, title) = (arg(c, 4), arg(c, 5), sx(arg(c, 6)));
    let (first_frame, frames, first, rest) = (arg(c, 7), arg(c, 8), arg(c, 9), stack_arg(c, 10));
    let menu = this.base();
    femenu::menu(&mut c.m, menu, flags, screen, next, options, on_start, title);
    let m = &mut c.m;
    this.set_vtable(m, OPTIONS_MENU_VT);
    femenu::menu_constructor(m, menu, first, rest);
    this.set_screen_fade(m, 0);
    this.set_prev_item(m, 0);
    this.set_first_frame(m, first_frame as i32);
    this.set_num_frames(m, frames as i32);
    c.r[2] = this.0;
}

/// `tOptionsMenu::~tOptionsMenu` (FEMENUOPTIONS.CPP, 0x8001c6a4).
pub fn abi_options_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, OPTIONS_MENU_VT)), femenu::abi_menu_dtor);
}

/// `tOptionsMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001c6cc): the current item's (none when
/// it is missing or disabled).
pub fn options_menu_debounce_keys(c: &mut Cpu, this: OptionsMenu) -> u32 {
    let m = &mut c.m;
    let current = this.current_item(m) as u32;
    let it = MenuItem(m.r32(this.0.wrapping_add(current << 2).wrapping_add(16)));
    if it.0 == 0 || is_disabled(m, it) {
        return 0;
    }
    call_virtual(c, it.0, ITEM_VPTR, ITEM_DEBOUNCE_KEYS, &[])
}

pub fn abi_options_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = options_menu_debounce_keys(c, OptionsMenu(c.r[4]));
}

/// `tOptionsMenu::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c738): the items fading out, the
/// screen's fade going up from 0.
pub fn abi_options_menu_transition_off(c: &mut Cpu) {
    let this = OptionsMenu(c.r[4]);
    this.set_transition_direction(&mut c.m, SCREEN_FADE_STEP);
    this.set_screen_fade(&mut c.m, 0);
    items(c, this.base(), |c, _, it| {
        call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_OFF, &[]);
    });
    this.set_in_menu_transition(&mut c.m, 1);
}

/// `tOptionsMenu::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c7d0): the items fading in, the
/// screen's fade going down from its full.
pub fn abi_options_menu_transition_on(c: &mut Cpu) {
    let this = OptionsMenu(c.r[4]);
    let m = &mut c.m;
    this.set_transition_direction(m, -SCREEN_FADE_STEP);
    this.set_in_menu_transition(m, 1);
    this.set_screen_fade(m, SCREEN_FADE_FULL);
    items(c, this.base(), |c, _, it| {
        call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_ON, &[]);
    });
}

/// `tOptionsMenu::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c86c): when the screen's fade
/// is at its end, every item's transition and the screen's are.
pub fn options_menu_transition_is_finished(c: &mut Cpu, this: OptionsMenu) -> u32 {
    let m = &mut c.m;
    let (fade, direction) = (this.screen_fade(m), this.transition_direction(m));
    let moving = if fade >= SCREEN_FADE_FULL && direction > 0 { false } else { fade > 0 || direction >= 0 };
    this.set_in_menu_transition(m, moving as u32);
    if this.in_menu_transition(m) == 0 {
        items(c, this.base(), |c, _, it| {
            if call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED, &[]) != 1 {
                this.set_in_menu_transition(&mut c.m, 1);
            }
        });
    }
    let screen = current_screen(&mut c.m);
    if call_virtual(c, screen, SCREEN_VPTR, SCREEN_TRANSITION_IS_FINISHED, &[]) != 1 {
        this.set_in_menu_transition(&mut c.m, 1);
    }
    this.in_menu_transition(&mut c.m) ^ 1
}

pub fn abi_options_menu_transition_is_finished(c: &mut Cpu) {
    c.r[2] = options_menu_transition_is_finished(c, OptionsMenu(c.r[4]));
}

/// An item's virtual `UpdateTransition`, told it isn't the current one.
fn update_item_unselected(c: &mut Cpu, it: MenuItem) {
    call_virtual(c, it.0, ITEM_VPTR, ITEM_UPDATE_TRANSITION, &[0]);
}

/// Whether an item's virtual `TransitionIsFinished` says it is.
fn transition_is_finished(c: &mut Cpu, it: MenuItem) -> bool {
    call_virtual(c, it.0, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED, &[]) != 0
}

/// `tOptionsMenu::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c9b4): out of a transition, each
/// item's, told whether it is the current one. Fading in: the screen's fade first, then the
/// items one at a time from the first; fading out: the items from the last, then the screen.
/// Then its virtual `TransitionIsFinished`.
pub fn options_menu_update_transition(c: &mut Cpu, this: OptionsMenu) {
    let menu = this.base();
    let m = &mut c.m;
    if this.in_menu_transition(m) == 0 {
        items(c, menu, |c, k, it| {
            let m = &mut c.m;
            let selected = (this.in_menu_transition(m) == 0 && k == menu.current_item(m) as u32) as u32;
            call_virtual(c, it.0, ITEM_VPTR, ITEM_UPDATE_TRANSITION, &[selected]);
        });
    } else if this.transition_direction(m) < 0 {
        let fade = this.screen_fade(m);
        if fade > 0 {
            let v = (fade + this.transition_direction(m) as i32).max(0);
            this.set_screen_fade(m, v);
        } else if item(m, menu, 0).0 != 0 {
            // the first item still fading
            let mut k = 0u32;
            loop {
                let it = item(&mut c.m, menu, k);
                if !transition_is_finished(c, it) {
                    break;
                }
                k = k.wrapping_add(1);
                if item(&mut c.m, menu, k).0 == 0 {
                    break;
                }
            }
            let it = item(&mut c.m, menu, k);
            if it.0 != 0 {
                update_item_unselected(c, it);
            }
        }
    } else {
        // the last item still fading, else the screen's fade
        let mut n = 0u32;
        if item(m, menu, 0).0 != 0 {
            loop {
                n = n.wrapping_add(1);
                if item(m, menu, n).0 == 0 {
                    break;
                }
            }
        }
        let mut k = n.wrapping_sub(1);
        let mut fading = None;
        while sx(k) != u32::MAX {
            let it = item(&mut c.m, menu, k);
            if !transition_is_finished(c, it) {
                fading = Some(it);
                break;
            }
            k = k.wrapping_sub(1);
        }
        match fading {
            Some(it) => update_item_unselected(c, it),
            None => {
                let m = &mut c.m;
                let fade = this.screen_fade(m);
                if fade < SCREEN_FADE_FULL {
                    let v = fade.wrapping_add(this.transition_direction(m) as i32);
                    this.set_screen_fade(m, v.min(SCREEN_FADE_FULL));
                }
            }
        }
    }
    call_virtual(c, this.0, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[]);
}

pub fn abi_options_menu_update_transition(c: &mut Cpu) {
    options_menu_update_transition(c, OptionsMenu(c.r[4]));
}

/// `tOptionsMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001cc64): the pulsing yellow worked out; the
/// current item moved off disabled ones (forward, then back); the picture's frame by the clock;
/// each item at its own place, told whether it is the current one (none in a transition).
pub fn options_menu_draw(c: &mut Cpu, this: OptionsMenu) {
    calc_pulsate_yellow(c);
    let menu = this.base();
    let m = &mut c.m;
    let at = |m: &mut Mem, k: u32| MenuItem(m.r32(this.0.wrapping_add(k << 2).wrapping_add(16)));
    let current = |m: &mut Mem| menu.current_item(m) as u32;
    let k = current(m);
    let it = at(m, k);
    if it.0 == 0 || is_disabled(m, it) {
        menu.set_current_item(m, 0);
    }
    loop {
        let k = current(m);
        let it = at(m, k);
        if !is_disabled(m, it) || at(m, k.wrapping_add(1)).0 == 0 {
            break;
        }
        menu.set_current_item(m, k.wrapping_add(1) as i32);
    }
    loop {
        let k = current(m);
        let it = at(m, k);
        if !is_disabled(m, it) || k as i32 <= 0 {
            break;
        }
        menu.set_current_item(m, k.wrapping_sub(1) as i32);
    }
    let (first, frames) = (this.first_frame(m), this.num_frames(m));
    if first >= 0 && frames > 0 {
        let frame = first.wrapping_add(((m.r32(GAME_TICKS) as i32) >> 4) % frames) as u32;
        with_buffer(c, 8, |c, options| {
            c.m.w32(options + 4, PICTURE_COLOUR);
            call(c, DRAW_SHAPE_EXTENDED, &[frame, PICTURE_FLAGS, 16, 16, 0, 0, options]);
        });
    }
    items(c, menu, |c, k, it| {
        let m = &mut c.m;
        let selected = (this.in_menu_transition(m) == 0 && k == menu.current_item(m) as u32) as u32;
        call_virtual(c, it.0, ITEM_VPTR, ITEM_DRAW_AT, &[0, 0, selected]);
    });
}

pub fn abi_options_menu_draw(c: &mut Cpu) {
    options_menu_draw(c, OptionsMenu(c.r[4]));
}

/// `tOptionsMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001ce68): with no item enabled, the
/// first current and every key but start and help used up; then `tMenu`'s.
pub fn options_menu_process_input(c: &mut Cpu, this: Menu, player: u32, key: u32, command: u32) {
    let m = &mut c.m;
    if femenu::get_number_enabled_items(m, this) & 0xFFFF == 0 {
        this.set_current_item(m, 0);
        let k = m.r32(key);
        if k != KEY_BACK && k != KEY_HELP {
            m.w32(key, KEY_NONE);
        }
    }
    femenu::process_input(c, this, player, key, MenuCommand(command));
}

pub fn abi_options_menu_process_input(c: &mut Cpu) {
    options_menu_process_input(c, Menu(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

// ---- tInsideBoxMenu: a menu inside a sliding box, its items scrolling (24 a row) when the
// current one changes.

/// `tInsideBoxMenu::tInsideBoxMenu` (FEMENUOPTIONS.CPP, 0x8001cef0): `tMenu`'s, its items from
/// the caller's list, not scrolling.
pub fn abi_inside_box_menu(c: &mut Cpu) {
    let this = InsideBoxMenu(c.r[4]);
    let (flags, screen, next) = (c.r[5], c.r[6], c.r[7]);
    let arg = |c: &mut Cpu, n| crate::abi::arg(c, n);
    let (options, on_start, title, first, rest) = (arg(c, 4), arg(c, 5), sx(arg(c, 6)), arg(c, 7), stack_arg(c, 8));
    let menu = this.base();
    femenu::menu(&mut c.m, menu, flags, screen, next, options, on_start, title);
    let m = &mut c.m;
    this.set_vtable(m, INSIDE_BOX_MENU_VT);
    femenu::menu_constructor(m, menu, first, rest);
    this.set_prev_item(m, 0);
    this.set_moving(m, 0);
    this.set_moving_dir(m, 0);
    c.r[2] = this.0;
}

/// `tInsideBoxMenu::~tInsideBoxMenu` (FEMENUOPTIONS.CPP, 0x8001cf64).
pub fn abi_inside_box_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, INSIDE_BOX_MENU_VT)), femenu::abi_menu_dtor);
}

/// `tInsideBoxMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001cf8c): up and down used up while
/// the items scroll; up at the first item goes to that item (the box closing), down at the last
/// does nothing; the rest is `tMenu`'s.
pub fn inside_box_menu_process_input(c: &mut Cpu, this: InsideBoxMenu, player: u32, key: u32, command: u32) {
    let m = &mut c.m;
    let k = m.r32(key);
    if this.moving(m) != 0 && (k == KEY_UP || k == KEY_DOWN) {
        m.w32(key, KEY_NONE);
    }
    let k = m.r32(key);
    let current = this.current_item(m) as u32;
    if k == KEY_UP && current == 0 {
        let first = this.item_list(m, 0);
        call_virtual(c, first.0, ITEM_VPTR, ITEM_PROCESS_INPUT, &[player, key, command]);
        return;
    }
    if k == KEY_DOWN && m.r32(this.0.wrapping_add(current.wrapping_add(1) << 2).wrapping_add(16)) == 0 {
        return;
    }
    femenu::process_input(c, this.base(), player, key, MenuCommand(command));
}

pub fn abi_inside_box_menu_process_input(c: &mut Cpu) {
    inside_box_menu_process_input(c, InsideBoxMenu(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// The scroll a row's change sets going (toward 0), and its step.
const SCROLL: i16 = 24;
const SCROLL_STEP: i16 = 4;

/// `tInsideBoxMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001d05c): the items scrolling toward the
/// current one; the two before it, it and the two after (the last only when the one before it
/// is there), 24 apart from `y + top`, each told whether it is the current one (none while
/// scrolling): first its `UpdateTransition`, then its four-argument `Draw` at `x` with `w`.
pub fn inside_box_menu_draw(c: &mut Cpu, this: InsideBoxMenu, x: u32, y: u32, w: u32, top: u32) {
    let m = &mut c.m;
    let (prev, current) = (this.prev_item(m) as i32, this.current_item(m));
    if current != prev {
        let (scroll, step) = if prev < current { (SCROLL, -SCROLL_STEP) } else { (-SCROLL, SCROLL_STEP) };
        this.set_moving(m, scroll);
        this.set_moving_dir(m, step);
        this.set_prev_item(m, current as i16);
    }
    if this.moving(m) != 0 {
        let v = this.moving(m).wrapping_add(this.moving_dir(m));
        this.set_moving(m, v);
        if this.moving_dir(m) < 0 && v < 0 {
            this.set_moving(m, 0);
        }
        if this.moving_dir(m) > 0 && this.moving(m) > 0 {
            this.set_moving(m, 0);
        }
    }
    let (x, y, w) = (sx(x), sx(y), sx(w));
    let selected = |m: &mut Mem, k: u32| (this.moving(m) == 0 && k == this.current_item(m) as u32) as u32;
    for slot in 0..5u32 {
        let m = &mut c.m;
        let k = (this.current_item(m) as u16 as u32).wrapping_add(slot.wrapping_sub(2));
        let ki = sx(k);
        // (the last row only below an item)
        if slot >= 4 && m.r32(this.0.wrapping_add(ki << 2).wrapping_add(12)) == 0 {
            return;
        }
        if (ki as i32) < 0 {
            continue;
        }
        let it = MenuItem(m.r32(this.0.wrapping_add(ki << 2).wrapping_add(16)));
        if it.0 == 0 {
            continue;
        }
        let sel = selected(m, ki);
        call_virtual(c, it.0, ITEM_VPTR, ITEM_UPDATE_TRANSITION, &[sel]);
        let m = &mut c.m;
        let it = item(m, this.base(), k);
        let row = ((slot as i32 - 1) * 24) as u32;
        let at = (this.moving(m) as i32 as u32).wrapping_add(y.wrapping_add(sx(top)).wrapping_add(row)).wrapping_add(5);
        let sel = selected(m, ki);
        call_virtual(c, it.0, ITEM_VPTR, ITEM_DRAW_AT3, &[x, at, w, sel]);
    }
}

pub fn abi_inside_box_menu_draw(c: &mut Cpu) {
    let top = crate::abi::arg(c, 4);
    inside_box_menu_draw(c, InsideBoxMenu(c.r[4]), c.r[5], c.r[6], c.r[7], top);
}

// ---- tMenuItemSlidingMenu: an item that opens into a box holding an inside menu.

/// `tMenuItemSlidingMenu::tMenuItemSlidingMenu` (FEMENUOPTIONS.CPP, 0x8001d2c0): a box `w` by
/// `h` (a slider's flag), no menu in it.
#[allow(clippy::too_many_arguments)]
pub fn sliding_menu(c: &mut Cpu, this: MenuItemSlidingMenu, text: u32, w: u32, h: u32, dx: u32, dy: u32, fill: u32) -> u32 {
    femenu::menu_item(&mut c.m, this.base(), text);
    let m = &mut c.m;
    this.set_curr_menu(m, InsideBoxMenu(0));
    this.set_next_menu(m, InsideBoxMenu(0));
    this.set_width(m, w as i16);
    this.set_height(m, h as i16);
    this.set_sel_fade(m, 0);
    this.set_vtable(m, SLIDING_MENU_VT);
    this.set_diff_x(m, dx as i16);
    this.set_diff_y(m, dy as i16);
    this.set_fillback(m, fill);
    let flags = this.flags(m);
    this.set_flags(m, flags | ITEM_SLIDER);
    this.0
}

pub fn abi_sliding_menu(c: &mut Cpu) {
    let (dx, dy, fill) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    c.r[2] = sliding_menu(c, MenuItemSlidingMenu(c.r[4]), c.r[5], c.r[6], c.r[7], dx, dy, fill);
}

/// `tMenuItemSlidingMenu::~tMenuItemSlidingMenu` (FEMENUOPTIONS.CPP, 0x8001d368).
pub fn abi_sliding_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, SLIDING_MENU_VT)), femenu::abi_menu_item_dtor);
}

/// A box's slide when it opens: half its height again.
fn opening_slide(m: &mut Mem, this: MenuItemSlidingMenu) -> i16 {
    let h = this.height(m);
    h.wrapping_add(h >> 1)
}

/// `tMenuItemSlidingMenu::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001d390): transitioning out;
/// fading out from 0 when a menu is open in it.
pub fn abi_sliding_menu_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemSlidingMenu(c.r[4]));
    let f = Fade::of_sliding(this);
    let open = this.curr_menu(m).0;
    f.set_step(m, 0);
    this.set_transitioning_out(m, 1);
    if open != 0 {
        f.set_fading(m, 1);
        f.set_step(m, FADE_STEP);
        f.set_value(m, 0);
    }
}

/// `tMenuItemSlidingMenu::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001d3bc): fading in from 128,
/// closed, its slide at the start again.
pub fn abi_sliding_menu_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemSlidingMenu(c.r[4]));
    let f = Fade::of_sliding(this);
    let slide = opening_slide(m, this);
    f.set_step(m, -FADE_STEP);
    f.set_fading(m, 1);
    this.set_open_height(m, 0);
    this.set_transitioning_out(m, 0);
    f.set_value(m, 128);
    this.set_slide_offset(m, slide);
}

/// `tMenuItemSlidingMenu::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001d3f4): finished
/// unless a menu is open and one to open next, and the fade isn't at its end.
pub fn abi_sliding_menu_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemSlidingMenu(c.r[4]));
    let f = Fade::of_sliding(this);
    if this.curr_menu(m).0 == 0 || this.next_menu(m).0 == 0 {
        f.set_step(m, 0);
        f.set_fading(m, 0);
        c.r[2] = 1;
        return;
    }
    c.r[2] = fade_is_finished(m, f);
}

/// `tMenuItemSlidingMenu::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001d47c), and
/// `tMenuItemSlidingActivated`'s (0x8001e028).
pub fn abi_sliding_menu_update_transition(c: &mut Cpu) {
    let this = MenuItemSlidingMenu(c.r[4]);
    fade_update_transition(c, this.base(), Fade::of_sliding(this), c.r[5]);
}

/// `tMenuItemSlidingMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001d4f8): up and down, and the
/// open menu's keys.
pub fn sliding_menu_debounce_keys(c: &mut Cpu, this: MenuItemSlidingMenu) -> u32 {
    let open = this.curr_menu(&mut c.m).0;
    if open == 0 {
        return KEY_UP | KEY_DOWN;
    }
    call_virtual(c, open, MENU_VPTR, MENU_DEBOUNCE_KEYS, &[]) | KEY_UP | KEY_DOWN
}

pub fn abi_sliding_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = sliding_menu_debounce_keys(c, MenuItemSlidingMenu(c.r[4]));
}

/// `tMenuItemSlidingMenu::Draw(bool)` (FEMENUOPTIONS.CPP, 0x8001d540): its `Draw` at (0, 0).
pub fn sliding_menu_draw_bool(c: &mut Cpu, this: u32, selected: u32) {
    call_virtual(c, this, ITEM_VPTR, ITEM_DRAW_AT, &[0, 0, selected]);
}

pub fn abi_sliding_menu_draw_bool(c: &mut Cpu) {
    sliding_menu_draw_bool(c, c.r[4], c.r[5]);
}

/// The fade 40 nearer gone (`out`: shown), within 0..128.
fn step_fade(m: &mut Mem, f: Fade, out: bool) {
    let v = f.value(m) as i32;
    let v = if out { (v - 40).max(0) } else { (v + 40).min(128) };
    f.set_value(m, v as i16);
}

/// The open height's and the slide's steps a frame.
const OPEN_STEP: i32 = 4;
const SLIDE_STEP: i32 = 6;

/// `tMenuItemSlidingMenu::UpdatefOpenHeight` (FEMENUOPTIONS.CPP, 0x8001d57c): a box changing
/// menus closes first (then the next menu fades in, the old one out); open, the selected box
/// opens to its height, then slides; an unselected one closes.
pub fn sliding_menu_update_open_height(m: &mut Mem, this: MenuItemSlidingMenu, selected: u32) {
    let f = Fade::of_sliding(this);
    this.set_closing(m, 0);
    if this.open_height(m) == 0 {
        let slide = opening_slide(m, this);
        this.set_slide_offset(m, slide);
    }
    let (next, open) = (this.next_menu(m).0, this.curr_menu(m).0);
    if next != open && this.open_height(m) > 0 {
        // closing to change menus
        let v = (this.open_height(m) as i32 - OPEN_STEP).max(0);
        this.set_open_height(m, v as i16);
        this.set_closing(m, 1);
    } else if next == 0 && f.value(m) < 128 {
        if f.fading(m) == 0 {
            step_fade(m, f, false);
        }
    } else if f.value(m) > 0 && this.curr_menu(m).0 == 0 && this.next_menu(m).0 != 0 {
        if f.fading(m) == 0 {
            step_fade(m, f, true);
        }
        if this.transitioning_out(m) == 0 {
            let next = this.next_menu(m);
            this.set_curr_menu(m, next);
        }
    } else {
        let next = this.next_menu(m);
        this.set_curr_menu(m, next);
        if this.transitioning_out(m) == 0 && f.fading(m) == 0 {
            step_fade(m, f, next.0 != 0);
        }
    }
    if this.curr_menu(m).0 == 0 {
        return;
    }
    if selected != 0 {
        if this.closing(m) == 0 {
            let (open, h) = (this.open_height(m) as i32, this.height(m) as i32);
            if open < h {
                this.set_open_height(m, (open + OPEN_STEP).min(h) as i16);
            } else {
                let v = (this.slide_offset(m) as i32 - SLIDE_STEP).max(0);
                this.set_slide_offset(m, v as i16);
            }
        }
        return;
    }
    let open = this.open_height(m) as i32;
    if open > 0 {
        this.set_open_height(m, (open - OPEN_STEP).max(0) as i16);
    }
}

pub fn abi_sliding_menu_update_open_height(c: &mut Cpu) {
    sliding_menu_update_open_height(&mut c.m, MenuItemSlidingMenu(c.r[4]), c.r[5]);
}

/// The drawing clipped to `rect` from here on (a draw area primitive in the ordering table;
/// `SetDrawArea` called by address: the host records the front end's clipping).
fn set_draw_area(c: &mut Cpu, rect: RECT) {
    let prim = femenuextended::add_prim(&mut c.m);
    call_addr(c, SET_DRAW_AREA, &[prim, rect.0]);
}

fn set_rect(m: &mut Mem, rect: RECT, x: i16, y: i16, w: i16, h: i16) {
    rect.set_x(m, x);
    rect.set_y(m, y);
    rect.set_w(m, w);
    rect.set_h(m, h);
}

/// A front-end shape's width and height (PSXFRONT.CPP's shape entries: +16, +18).
fn shape_w(m: &mut Mem, shape: u32) -> u32 {
    m.r16(shape + 16) as i16 as i32 as u32
}

fn shape_h(m: &mut Mem, shape: u32) -> u32 {
    m.r16(shape + 18) as i16 as i32 as u32
}

/// The shapes a sliding box draws with: its bar (30), its filled back's top and bottom corners
/// (31, 32: entries of 32 bytes in the table at `SHAPES`), its arrows (57, 58).
const BAR: u32 = 30;
const TOP_CORNER: u32 = 31;
const BOTTOM_CORNER: u32 = 32;
const UP_ARROW: u32 = 57;
const DOWN_ARROW: u32 = 58;
/// The box's shaded edges' grey.
const BOX_GREY: u32 = 0x0050_5050;
/// The songs' screen's box (its offset in the songs' screen).
const SONGS_BOX: u32 = 10072;

/// `tMenuItemSlidingMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001d79c): the box's open height worked
/// out; its text at its own place (offset by `x`, `y`) with the flare (the songs' box, not
/// fully faded) and the arrows (an open box or the songs'); an open box: dark at its edges
/// (two shaded halves), the menu in it (scrolled by the slide), its back filled (corners and
/// squares) or not, the help text of its items, the text again over it and the bar under it.
pub fn sliding_menu_draw(c: &mut Cpu, this: MenuItemSlidingMenu, x: u32, y: u32, selected: u32) {
    call_virtual(c, this.0, ITEM_VPTR, SLIDING_UPDATE_OPEN_HEIGHT, &[selected]);
    let f = Fade::of_sliding(this);
    let (sel, fade) = (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade]);
    let text = this.text_description(&mut c.m);
    let tx = call(c, WORD_X, &[text]).wrapping_add(x);
    let ty = call(c, WORD_Y, &[text]).wrapping_add(y);
    let m = &mut c.m;
    m.w32(BOX_HELP_TEXT, u32::MAX);
    let bar = m.r32(SHAPES).wrapping_add(BAR * 32);
    let songs = this.curr_menu(m).0 == m.r32(SONGS_SCREEN).wrapping_add(SONGS_BOX);
    // (the shape options and the full screen's rectangle at +0, the box's at +8)
    with_buffer(c, 16, |c, buf| {
        let fade = f.value(&mut c.m) as i32 as u32;
        if fade != 128 && songs {
            let sel = this.sel_fade(&mut c.m) as i32 as u32;
            draw_left_flare(c, ty, sel, fade, SLIDING_FLARE_PHASE);
        }
        if this.curr_menu(&mut c.m).0 != 0 || songs {
            // the arrows at its right end
            let right = if songs { 220 } else { this.width(&mut c.m) as i32 as u32 };
            let (sel, fade) = (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
            let arrow = call(c, CALC_FADE_VAL_FADED, &[0, 0xBEBE, sel, fade]);
            c.m.w32(buf + 4, arrow);
            let end = tx.wrapping_add(right);
            let ax = end.wrapping_sub(shape_w(&mut c.m, bar)).wrapping_sub(10);
            let ay = if songs { ty.wrapping_sub(3) } else { ty.wrapping_sub(2) };
            call(c, DRAW_SHAPE_EXTENDED, &[UP_ARROW, 24, ax, ay, 0, 1, buf]);
            let ax = end.wrapping_sub(shape_w(&mut c.m, bar)).wrapping_sub(10);
            let ay = if songs { ty.wrapping_add(3) } else { ty.wrapping_add(4) };
            call(c, DRAW_SHAPE_EXTENDED, &[DOWN_ARROW, 24, ax, ay, 0, 1, buf]);
        }
        let menu = this.curr_menu(&mut c.m);
        if menu.0 == 0 {
            return;
        }
        // the box's two shaded halves, clipped to the screen
        let m = &mut c.m;
        let (bx, by) = (tx.wrapping_add(this.diff_x(m) as i32 as u32), ty.wrapping_add(this.diff_y(m) as i32 as u32));
        let w = this.width(m) as i32 as u32;
        let open = this.open_height(m) as i32;
        let (view, flip) = (m.r32(PLAYER1_VIEW), m.r32(FLIP));
        let env = DRAWENV(call(c, GET_DRAWENV, &[view, flip]));
        let m = &mut c.m;
        let screen = RECT(buf);
        let (top, height) = (env.clip().y(m), m.r16(SCREEN_HEIGHT) as i16);
        set_rect(m, screen, 0, top, 512, height);
        set_draw_area(c, screen);
        let half = (open >> 1) as u32;
        call_addr(c, SUBTRACTIVE_BOX, &[bx, by, w, half, BOX_GREY, BOX_GREY, 0, 0]);
        call_addr(c, SUBTRACTIVE_BOX, &[bx, by.wrapping_add(half), w, half, 0, 0, BOX_GREY, BOX_GREY]);
        // the menu in it
        let m = &mut c.m;
        let slide = (this.slide_offset(m).wrapping_mul(2)) as i32 as u32;
        let shapes = m.r32(SHAPES);
        let box_h = this.height(m) as i32 as u32;
        call_virtual(c, menu.0, MENU_VPTR, MENU_DRAW_BOX, &[sx(bx), sx(by), w, slide, box_h]);
        let open = open as u32;
        let (top_corner, bottom_corner) = (shapes.wrapping_add(TOP_CORNER * 32), shapes.wrapping_add(BOTTOM_CORNER * 32));
        if this.fillback(&mut c.m) != 0 && (shape_h(&mut c.m, top_corner) as i32) < open as i32 {
            // the filled back's corners
            call(c, DRAW_SHAPE_EXTENDED, &[TOP_CORNER, 12, bx, by, 0, 0, 0]);
            let right = bx.wrapping_add(w);
            let x2 = right.wrapping_sub(shape_w(&mut c.m, top_corner));
            call(c, DRAW_SHAPE_EXTENDED, &[TOP_CORNER, 8, x2, by, 0, 0, 0]);
            let bottom = by.wrapping_add(open);
            let y2 = bottom.wrapping_sub(shape_h(&mut c.m, bottom_corner));
            call(c, DRAW_SHAPE_EXTENDED, &[BOTTOM_CORNER, 12, bx, y2, 0, 0, 0]);
            let m = &mut c.m;
            let (x2, y2) = (right.wrapping_sub(shape_w(m, bottom_corner)), bottom.wrapping_sub(shape_h(m, bottom_corner)));
            call(c, DRAW_SHAPE_EXTENDED, &[BOTTOM_CORNER, 8, x2, y2, 0, 0, 0]);
        }
        let m = &mut c.m;
        let (cw, cw2) = (shape_w(m, top_corner), shape_w(m, bottom_corner));
        let middle_w = w.wrapping_sub(cw).wrapping_sub(cw2).wrapping_sub(10);
        call_addr(c, DRAW_SQUARE, &[0, bx.wrapping_add(cw).wrapping_add(5), by, middle_w, open]);
        let m = &mut c.m;
        let (ch, ch2) = (shape_h(m, top_corner), shape_h(m, bottom_corner));
        if (ch.wrapping_add(ch2) as i32) < open as i32 {
            call_addr(c, DRAW_SQUARE, &[0, bx, by.wrapping_add(ch), w, open.wrapping_sub(ch).wrapping_sub(ch2)]);
        }
        // clipped to the box: its items' help text
        let m = &mut c.m;
        let inside = RECT(buf + 8);
        let top = env.clip().y(m);
        set_rect(m, inside, bx as i16, top.wrapping_add(by as i16), w as i16, open as i16);
        set_draw_area(c, inside);
        if c.m.r32(BOX_HELP_TEXT) != u32::MAX {
            let help = c.m.r32(BOX_HELP_TEXT);
            let word = call(c, TEXT_WORD, &[help]);
            let help = c.m.r32(BOX_HELP_TEXT);
            let hx = sx(call(c, WORD_X, &[help]));
            let help = c.m.r32(BOX_HELP_TEXT);
            let hy = sx(call(c, WORD_Y, &[help]));
            let yellow = c.m.r32(PULSATE_YELLOW);
            call(c, fetextrender::full_text_rgb_entry, &[word, hx, hy, yellow, 0, 2]);
        }
        // its text again over it, and the bar under the text
        let text = this.text_description(&mut c.m);
        let word = call(c, TEXT_WORD, &[text]);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), colour, 0, songs as u32]);
        let m = &mut c.m;
        let bar_y = ty.wrapping_sub(2);
        let bar_x = tx.wrapping_add(this.width(m) as i32 as u32).wrapping_sub(shape_w(m, bar));
        let fade = f.value(m) as i32 as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[BAR, 8, bar_x, bar_y, fade, 0, 0]);
        let m = &mut c.m;
        if this.fillback(m) != 0 {
            let len = (this.width(m) as i32 as u32).wrapping_sub(shape_w(m, bar));
            let h = shape_h(m, bar);
            call_addr(c, DRAW_SQUARE, &[0, tx, bar_y, len, h]);
        }
    });
}

pub fn abi_sliding_menu_draw(c: &mut Cpu) {
    sliding_menu_draw(c, MenuItemSlidingMenu(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemSlidingMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001dde0): a fully open box's
/// menu takes the input first; down used up (then `tMenuItem`'s, which does nothing).
pub fn sliding_menu_process_input(c: &mut Cpu, this: MenuItemSlidingMenu, player: u32, key: u32, command: u32) {
    let m = &mut c.m;
    let menu = this.curr_menu(m).0;
    if this.open_height(m) == this.height(m) && menu != 0 {
        call_virtual(c, menu, MENU_VPTR, MENU_PROCESS_INPUT, &[player, key, command]);
    }
    if c.m.r32(key) == KEY_DOWN {
        c.m.w32(key, KEY_NONE);
    }
}

pub fn abi_sliding_menu_process_input(c: &mut Cpu) {
    sliding_menu_process_input(c, MenuItemSlidingMenu(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemSlidingMenu::SetMenu` (FEMENUOPTIONS.CPP, 0x8001de88): the menu to open next (and
/// now, when `now`); the box disabled without one.
pub fn sliding_menu_set_menu(m: &mut Mem, this: MenuItemSlidingMenu, now: u32, menu: u32) {
    this.set_next_menu(m, InsideBoxMenu(menu));
    if now != 0 {
        this.set_curr_menu(m, InsideBoxMenu(menu));
    }
    let flags = this.flags(m);
    let none = this.next_menu(m).0 == 0;
    this.set_flags(m, if none { flags | ITEM_DISABLED } else { flags & !ITEM_DISABLED });
}

pub fn abi_sliding_menu_set_menu(c: &mut Cpu) {
    sliding_menu_set_menu(&mut c.m, MenuItemSlidingMenu(c.r[4]), c.r[5], c.r[6]);
}

// ---- tMenuItemSlidingActivated: the songs' box, active while the game's music plays in it.

/// The slide's step a frame while the box is active (up) or not (down).
const ACTIVE_SLIDE_STEP: i16 = 3;

/// `tMenuItemSlidingActivated::UpdatefOpenHeight` (FEMENUOPTIONS.CPP, 0x8001decc): the slide
/// up while active, else down, within 0 and its start.
pub fn sliding_activated_update_open_height(m: &mut Mem, this: MenuItemSlidingActivated) {
    let slide = this.slide_offset(m);
    let slide = if this.active(m) != 0 { slide.wrapping_sub(ACTIVE_SLIDE_STEP) } else { slide.wrapping_add(ACTIVE_SLIDE_STEP) };
    let start = opening_slide(m, this.base()) as i32;
    this.set_slide_offset(m, (slide as i32).min(start).max(0) as i16);
}

pub fn abi_sliding_activated_update_open_height(c: &mut Cpu) {
    sliding_activated_update_open_height(&mut c.m, MenuItemSlidingActivated(c.r[4]));
}

/// `tMenuItemSlidingActivated::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001df60): fading in from
/// 128, inactive, fully open, its slide at the start.
pub fn abi_sliding_activated_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemSlidingActivated(c.r[4]));
    let f = Fade::of_sliding(this.base());
    let (slide, h) = (opening_slide(m, this.base()), this.height(m));
    f.set_step(m, -FADE_STEP);
    f.set_fading(m, 1);
    this.set_active(m, 0);
    this.set_transitioning_out(m, 0);
    f.set_value(m, 128);
    this.set_slide_offset(m, slide);
    this.set_open_height(m, h);
}

/// `tMenuItemSlidingActivated::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001e028).
pub fn abi_sliding_activated_update_transition(c: &mut Cpu) {
    abi_sliding_menu_update_transition(c);
}

/// The front end's sounds for the songs' box: activated, back.
const SOUND_ACTIVATE: u32 = 0;
const SOUND_BACK: u32 = 1;

/// `tMenuItemSlidingActivated::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001e0a4): accept makes it
/// active (the menu music stopped, the game's songs loaded and one playing), start makes it
/// inactive again (the menu music back); active and slid in, its menu takes the input and up
/// and down are used up (then `tMenuItem`'s, which does nothing).
pub fn sliding_activated_process_input(c: &mut Cpu, this: MenuItemSlidingActivated, player: u32, key: u32, command: u32) {
    let k = c.m.r32(key);
    if k == femenu::KEY_ACCEPT && this.active(&mut c.m) == 0 {
        call(c, femenu::PLAY_FE_SFX, &[SOUND_ACTIVATE]);
        this.set_active(&mut c.m, 1);
        c.m.w32(key, KEY_NONE);
        call(c, STOP_SONG, &[20]);
        call(c, MUSIC_CLEAN_UP, &[]);
        call(c, MUSIC_START_UP, &[0xC000, 0x18000, GAME_MUSIC_FILE]);
        call_addr(c, PLAY_SONG, &[GAME_SONGS]);
        let list = call(c, LOAD_SONG_LIST, &[SONG_LIST_FILE, 0]);
        let state = c.m.r32(MUSIC_STATE);
        c.m.w32(state + 120, list);
    } else if k == KEY_BACK && this.active(&mut c.m) != 0 {
        call(c, femenu::PLAY_FE_SFX, &[SOUND_BACK]);
        this.set_active(&mut c.m, 0);
        c.m.w32(key, KEY_NONE);
        call(c, STOP_SONG, &[20]);
        let list = song_list(&mut c.m);
        if list != 0 {
            call(c, PURGE_MEM, &[list]);
        }
        let state = c.m.r32(MUSIC_STATE);
        c.m.w32(state + 120, 0);
        call(c, MUSIC_CLEAN_UP, &[]);
        call(c, MUSIC_START_UP, &[0xD800, 0x18000, MENU_MUSIC_FILE]);
        call_addr(c, PLAY_SONG, &[MENU_SONGS]);
    }
    let m = &mut c.m;
    if this.slide_offset(m) == 0 {
        if this.active(m) == 0 {
            return;
        }
        let menu = this.curr_menu(m).0;
        if menu != 0 {
            call_virtual(c, menu, MENU_VPTR, MENU_PROCESS_INPUT, &[player, key, command]);
        }
    }
    let m = &mut c.m;
    if this.active(m) != 0 {
        let k = m.r32(key);
        if k == KEY_UP || k == KEY_DOWN {
            m.w32(key, KEY_NONE);
        }
    }
}

pub fn abi_sliding_activated_process_input(c: &mut Cpu) {
    sliding_activated_process_input(c, MenuItemSlidingActivated(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

// ---- The fading left-right choices: the display's, the controller's, on-off ones.

/// The arrows' and the speaker's colours (`CalcFadeVal`'s from).
const ARROWS_COLOUR: u32 = 0x00C8_3C1E;
const SPEAKER_COLOUR: u32 = 0x0055_1E00;

/// `tMenuItemLeftRightFade::MyLeftRightDraw` (FEMENUOPTIONS.CPP, 0x8001e29c): a fading
/// left-right choice's flare, its text at (`x`, `y`) and the arrows either side of its value.
pub fn my_left_right_draw(c: &mut Cpu, this: MenuItemLeftRightFade, x: u32, y: u32) {
    let f = Fade::of_choice(this);
    let faded = |c: &mut Cpu| (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    with_buffer(c, 8, |c, options| {
        let (sel, fade) = faded(c);
        let arrows = call(c, CALC_FADE_VAL_FADED, &[ARROWS_COLOUR, 0xBEBE, sel, fade]);
        c.m.w32(options + 4, arrows);
        let (sel, fade) = faded(c);
        draw_left_flare(c, y, sel, fade, this.0 + 0x28);
        let (sel, fade) = faded(c);
        let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade]);
        let text = this.text_description(&mut c.m);
        let word = call(c, TEXT_WORD, &[text]);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(x), sx(y), colour, 0, 1]);
        let ay = y.wrapping_add(1);
        call(c, DRAW_SHAPE_EXTENDED, &[46, 24, x.wrapping_add(26), ay, 0, 0, options]);
        call(c, DRAW_SHAPE_EXTENDED, &[47, 24, x.wrapping_add(200), ay, 0, 0, options]);
    });
}

pub fn abi_my_left_right_draw(c: &mut Cpu) {
    my_left_right_draw(c, MenuItemLeftRightFade(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenuItemDisplayLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001e390): unless faded out,
/// the choice at its text's place (offset by `x`, `y`), its value (the sub-menu player's)
/// right of it.
pub fn display_choice_draw(c: &mut Cpu, this: MenuItemLeftRightFade, x: u32, y: u32) {
    let f = Fade::of_choice(this);
    if f.value(&mut c.m) == 128 {
        return;
    }
    let text = this.text_description(&mut c.m);
    let tx = call(c, WORD_X, &[text]).wrapping_add(x);
    let ty = call(c, WORD_Y, &[text]).wrapping_add(y);
    let (sel, fade) = (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade]);
    let ty = sx(ty);
    my_left_right_draw(c, this, sx(tx), ty);
    let (data, player) = (this.data(&mut c.m), c.m.r32(SUB_MENU_PLAYER));
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::TEXT_VALUE, &[player]);
    let word = call(c, TEXT_WORD, &[sx(value)]);
    call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(115)), ty, colour, 0, 2]);
}

pub fn abi_display_choice_draw(c: &mut Cpu) {
    display_choice_draw(c, MenuItemLeftRightFade(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenuItemOnOffLeftRightChoice::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001e480): the on-off
/// fade at its value's end, then the left-right fade's.
pub fn on_off_transition_on(c: &mut Cpu, this: MenuItemOnOffLeftRightChoice) {
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]);
    this.set_on_fade(&mut c.m, if value & 0xFF != 0 { 128 } else { 0 });
    call(c, abi_left_right_fade_transition_on, &[this.0]);
}

pub fn abi_on_off_transition_on(c: &mut Cpu) {
    on_off_transition_on(c, MenuItemOnOffLeftRightChoice(c.r[4]));
}

/// `tMenuItemOnOffLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001e4e4): unless faded out, the
/// on-off fade moving toward the value (32 a frame), the choice at its text's place (offset by
/// `x`, `y`), ON and OFF right of it in the fade's colours.
pub fn on_off_draw(c: &mut Cpu, this: MenuItemOnOffLeftRightChoice, x: u32, y: u32) {
    let f = Fade::of_choice(this.base());
    if f.value(&mut c.m) == 128 {
        return;
    }
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]);
    let m = &mut c.m;
    let step = if value & 0xFF != 0 { 32 } else { -32 };
    let v = this.on_fade(m).wrapping_add(step).clamp(0, 128);
    this.set_on_fade(m, v);
    let text = this.text_description(m);
    let tx = call(c, WORD_X, &[text]).wrapping_add(x);
    let ty = call(c, WORD_Y, &[text]).wrapping_add(y);
    with_buffer(c, 8, |c, colours| {
        let m = &mut c.m;
        let (on, sel, fade) = (this.on_fade(m) as i32 as u32, this.sel_fade(m) as i32 as u32, f.value(m) as i32 as u32);
        call(c, CALC_ON_OFF_FADE, &[6, on, sel, fade, colours, colours + 4]);
        let ty = sx(ty);
        my_left_right_draw(c, this.base(), sx(tx), ty);
        let word = call(c, TEXT_WORD, &[TEXT_ON]);
        let colour = c.m.r32(colours);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(55)), ty, colour, 0, 0]);
        let word = call(c, TEXT_WORD, &[TEXT_OFF]);
        let colour = c.m.r32(colours + 4);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(158)), ty, colour, 0, 0]);
    });
}

pub fn abi_on_off_draw(c: &mut Cpu) {
    on_off_draw(c, MenuItemOnOffLeftRightChoice(c.r[4]), c.r[5], c.r[6]);
}

// ---- tMenuItemLeftRightAudioSlider: a slider with a speaker picture, fading.

/// `tMenuItemLeftRightAudioSlider::tMenuItemLeftRightAudioSlider` (FEMENUOPTIONS.CPP,
/// 0x8001e65c): a slider with picture `art`.
pub fn audio_slider(c: &mut Cpu, this: MenuItemLeftRightAudioSlider, text: u32, data: ListIterator, art: u32) -> u32 {
    femenu::left_right_slider(c, this.base(), text, data);
    let m = &mut c.m;
    this.set_vtable(m, AUDIO_SLIDER_VT);
    this.set_audio_art(m, art as i16);
    this.set_flareextra(m, 0);
    this.set_sel_fade(m, 0);
    this.0
}

pub fn abi_audio_slider(c: &mut Cpu) {
    c.r[2] = audio_slider(c, MenuItemLeftRightAudioSlider(c.r[4]), c.r[5], ListIterator(c.r[6]), c.r[7]);
}

/// `tMenuItemLeftRightAudioSlider::~tMenuItemLeftRightAudioSlider` (FEMENUOPTIONS.CPP,
/// 0x8001e6a8).
pub fn abi_audio_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, AUDIO_SLIDER_VT)), femenu::abi_left_right_slider_dtor);
}

/// The colour of text `id`'s text type's style `k` (4 unselected, 5 selected).
fn style_colour(c: &mut Cpu, id: u32, k: u32) -> u32 {
    let ty = call(c, WORD_FLAGS, &[id]);
    let m = &mut c.m;
    let colour = m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(6)).wrapping_add(k)) as u32;
    m.r32(TEXT_COLORS + 4 * colour)
}

/// `tMenuItemLeftRightAudioSlider::Draw` (FEMENUOPTIONS.CPP, 0x8001e6d0): the slider placed at
/// its text's place (offset by `x`, `y`); the flare; unless faded out its text (in its text
/// type's colours, blended by the selection), the speaker picture when selected and the
/// slider at the value.
pub fn audio_slider_draw(c: &mut Cpu, this: MenuItemLeftRightAudioSlider, x: u32, y: u32) {
    let f = Fade::of_audio(this);
    let text = this.text_description(&mut c.m);
    let wx = call(c, WORD_X, &[text]);
    this.set_x(&mut c.m, wx.wrapping_add(x) as i16);
    let wy = call(c, WORD_Y, &[text]);
    this.set_y(&mut c.m, wy.wrapping_add(y) as i16);
    let id = text as i16 as i32 as u32;
    let selected_colour = style_colour(c, id, 5);
    let unselected_colour = style_colour(c, id, 4);
    let faded = |c: &mut Cpu| (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    let (sel, fade) = faded(c);
    let fy = this.y(&mut c.m) as i32 as u32;
    draw_left_flare(c, fy, sel, fade, this.0 + 0x34);
    let (sel, fade) = faded(c);
    let colour = call(c, CALC_FADE_VAL_FADED, &[unselected_colour, selected_colour, sel, fade]);
    if f.value(&mut c.m) == 128 {
        return;
    }
    let word = call(c, TEXT_WORD, &[text]);
    let (px, py) = (this.x(&mut c.m) as i32 as u32, this.y(&mut c.m) as i32 as u32);
    call(c, fetextrender::full_text_rgb_entry, &[word, px, py, colour, 0, 1]);
    let (sel, fade) = faded(c);
    let speaker = call(c, CALC_FADE_VAL_FADED, &[SPEAKER_COLOUR, 0xBEBE, sel, fade]);
    with_buffer(c, 8, |c, options| {
        c.m.w32(options + 4, speaker);
        if this.sel_fade(&mut c.m) != 0 {
            let art = (this.audio_art(&mut c.m) as i32 as u32).wrapping_add(1);
            call(c, DRAW_SHAPE_EXTENDED, &[art, 16, 0, 0, 0, 0, options]);
        }
    });
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]) & 0xFF;
    let m = &mut c.m;
    let data = this.data(m);
    let (min, max) = (data.min_value(m) as u32, data.max_value(m) as u32);
    let (sx_, sy) = (sx((this.x(m) as u16 as u32).wrapping_add(20)), sx((this.y(m) as u16 as u32).wrapping_add(1)));
    let (w, h) = (this.width(m) as i32 as u32, this.height(m) as i32 as u32);
    let (sel, fade) = (this.sel_fade(m) as i32 as u32, f.value(m) as i32 as u32);
    call_addr(c, DRAW_SLIDER, &[value, min, max, sx_, sy, w, h, 4, 4, 0, 0, sel, fade]);
}

pub fn abi_audio_slider_draw(c: &mut Cpu) {
    audio_slider_draw(c, MenuItemLeftRightAudioSlider(c.r[4]), c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightAudioSlider::Percentage` (FEMENUOPTIONS.CPP, 0x8001e8dc): the value's
/// place in the iterator's range in percent (one up when it isn't 0, up to 100).
pub fn audio_slider_percentage(c: &mut Cpu, this: MenuItemLeftRightAudioSlider) -> u32 {
    let data = this.data(&mut c.m);
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]);
    let m = &mut c.m;
    let data = this.data(m);
    let min = data.min_value(m) as u32;
    let range = (data.max_value(m) as u32).wrapping_sub(min);
    let percent = crate::fixed::div(((value & 0xFF).wrapping_sub(min)).wrapping_mul(100) as i32, range as i32) as u32;
    let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]);
    if value & 0xFF != 0 && (percent as i32) < 100 { percent.wrapping_add(1) } else { percent }
}

pub fn abi_audio_slider_percentage(c: &mut Cpu) {
    c.r[2] = audio_slider_percentage(c, MenuItemLeftRightAudioSlider(c.r[4]));
}

/// The audio sliders' fade step, either way.
const AUDIO_FADE_STEP: i16 = 60;

/// `tMenuItemLeftRightAudioSlider::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001e9b0): fading out
/// from 0.
pub fn abi_audio_slider_transition_off(c: &mut Cpu) {
    let (m, f) = (&mut c.m, Fade::of_audio(MenuItemLeftRightAudioSlider(c.r[4])));
    f.set_value(m, 0);
    f.set_step(m, AUDIO_FADE_STEP);
}

/// `tMenuItemLeftRightAudioSlider::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001e9c0): fading in
/// from 128, unselected.
pub fn abi_audio_slider_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, MenuItemLeftRightAudioSlider(c.r[4]));
    let f = Fade::of_audio(this);
    f.set_value(m, 128);
    f.set_step(m, -AUDIO_FADE_STEP);
    this.set_sel_fade(m, 0);
}

/// `tMenuItemLeftRightAudioSlider::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001e9d8).
pub fn abi_audio_slider_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, Fade::of_audio(MenuItemLeftRightAudioSlider(c.r[4])));
}

/// `tMenuItemLeftRightAudioSlider::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001ea34): the fade
/// stepped within 0..128, then `tMenuItem`'s.
pub fn audio_slider_update_transition(c: &mut Cpu, this: MenuItemLeftRightAudioSlider, selected: u32) {
    let (m, f) = (&mut c.m, Fade::of_audio(this));
    let v = (f.value(m) as i32 + f.step(m) as i32).clamp(0, 128);
    f.set_value(m, v as i16);
    femenu::menu_item_update_transition(c, MenuItem(this.0), selected);
}

pub fn abi_audio_slider_update_transition(c: &mut Cpu) {
    audio_slider_update_transition(c, MenuItemLeftRightAudioSlider(c.r[4]), c.r[5]);
}

// ---- tInsideBoxSongMenu: the songs (the music library's song list), five rows about the
// current song, each with an on-off and a selection fade.

/// The music's song list (0: none loaded: its count at +0, 64 bytes a song from +12) and the
/// current song.
fn song_list(m: &mut Mem) -> u32 {
    let state = m.r32(MUSIC_STATE);
    m.r32(state + 120)
}

fn current_song(m: &mut Mem) -> u32 {
    let state = m.r32(MUSIC_STATE);
    m.r16(state + 116) as i16 as i32 as u32
}

/// Where the front end keeps whether each song is on (a word each).
const SONGS_ON: u32 = 924;

fn song_on(song: u32) -> u32 {
    FRONT_END.wrapping_add(song << 2).wrapping_add(SONGS_ON)
}

/// The rows' spacing; the middle row's index.
const SONG_ROW: i32 = 21;
const MIDDLE: u32 = 2;

/// `tInsideBoxSongMenu::tInsideBoxSongMenu` (FEMENUOPTIONS.CPP, 0x8001ea80): `tMenu`'s, the
/// fades 0, its items from the caller's list, not scrolling.
pub fn abi_song_menu(c: &mut Cpu) {
    let this = InsideBoxSongMenu(c.r[4]);
    let (flags, screen, next) = (c.r[5], c.r[6], c.r[7]);
    let arg = |c: &mut Cpu, n| crate::abi::arg(c, n);
    let (options, on_start, title, first, rest) = (arg(c, 4), arg(c, 5), sx(arg(c, 6)), arg(c, 7), stack_arg(c, 8));
    let menu = Menu(this.0);
    femenu::menu(&mut c.m, menu, flags, screen, next, options, on_start, title);
    let m = &mut c.m;
    this.set_vtable(m, SONG_MENU_VT);
    for k in 0..5 {
        this.set_sel_fade(m, k, 0);
        this.set_on_off_fade(m, k, 0);
    }
    femenu::menu_constructor(m, menu, first, rest);
    this.set_moving(m, 0);
    this.set_moving_dir(m, 0);
    c.r[2] = this.0;
}

/// `tInsideBoxSongMenu::~tInsideBoxSongMenu` (FEMENUOPTIONS.CPP, 0x8001eb10).
pub fn abi_song_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((MENU_VPTR, SONG_MENU_VT)), abi_inside_box_menu_dtor);
}

/// `tInsideBoxSongMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001eb38): with songs loaded, the
/// selection fades toward the middle row, the list scrolling (twice the step), and the two
/// songs before the current one, it and the two after (those there are), 21 apart about the
/// middle of the box (`h` high, from `y + top`), each's on-off fade moving toward whether it is
/// on (32 a frame): its virtual `DrawOneSong`.
#[allow(clippy::too_many_arguments)]
pub fn song_menu_draw(c: &mut Cpu, this: InsideBoxSongMenu, x: u32, y: u32, w: u32, top: u32, h: u32) {
    if song_list(&mut c.m) == 0 {
        return;
    }
    let mid = sx(y).wrapping_add(((sx(h) as i32 - SONG_ROW) >> 1) as u32);
    let m = &mut c.m;
    for k in 0..5 {
        let v = this.sel_fade(m, k) as i32;
        this.set_sel_fade(m, k, if k == MIDDLE { (v + 8).min(128) } else { (v - 8).max(0) } as i16);
    }
    if this.moving(m) != 0 {
        let step = this.moving_dir(m);
        let v = this.moving(m).wrapping_add(step.wrapping_mul(2));
        this.set_moving(m, v);
        if step < 0 && v < 0 {
            this.set_moving(m, 0);
        }
        if this.moving_dir(m) > 0 && this.moving(m) > 0 {
            this.set_moving(m, 0);
        }
    }
    for k in 0..5u32 {
        let m = &mut c.m;
        let row = (-40 + SONG_ROW * k as i32) as u32;
        let song = current_song(m).wrapping_add(k).wrapping_sub(MIDDLE);
        let list = song_list(m);
        let count = m.r32(list) as i32;
        if (song as i32) < 0 || (song as i32) >= count {
            continue;
        }
        let on = m.r32(song_on(song)) != 0;
        let v = this.on_off_fade(m, k).wrapping_add(if on { 32 } else { -32 }).clamp(0, 128);
        this.set_on_off_fade(m, k, v);
        let at = (top & 0xFFFF).wrapping_add((this.moving(m) as u16 as u32).wrapping_add(mid.wrapping_add(row)));
        let (on_fade, sel_fade) = (this.on_off_fade(m, k) as i32 as u32, this.sel_fade(m, k) as i32 as u32);
        call_virtual(c, this.0, MENU_VPTR, MENU_DRAW_ONE_SONG, &[sx(song), sx(x), sx(at), sx(w), on_fade, sel_fade]);
    }
}

pub fn abi_song_menu_draw(c: &mut Cpu) {
    let (top, h) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    song_menu_draw(c, InsideBoxSongMenu(c.r[4]), c.r[5], c.r[6], c.r[7], top, h);
}

/// `tInsideBoxSongMenu::DrawOneSong` (FEMENUOPTIONS.CPP, 0x8001eda8): a song's two lines of
/// text (in its selection's colour), ON and OFF in its on-off fade's, and its frame.
#[allow(clippy::too_many_arguments)]
pub fn song_menu_draw_one_song(c: &mut Cpu, song: u32, x: u32, y: u32, w: u32, on_fade: u32, sel_fade: u32) {
    let frame = call(c, CALC_FADE_VAL, &[SPEAKER_COLOUR, 40]);
    with_buffer(c, 8, |c, colours| {
        call(c, CALC_ON_OFF_FADE, &[6, sx(on_fade), sx(sel_fade), 0, colours, colours + 4]);
        let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sx(sel_fade), 0]);
        // the song's entry: 64 bytes, its two lines' texts at +12 and +16
        let entry = (sx(song) << 6).wrapping_add(12);
        let list = song_list(&mut c.m);
        let line = c.m.r32(list.wrapping_add(entry));
        call(c, fetextrender::full_text_rgb_entry, &[line, sx(x.wrapping_add(3)), sx(y.wrapping_add(2)), colour, 0, 0]);
        let list = song_list(&mut c.m);
        let line = c.m.r32(list.wrapping_add(entry + 4));
        call(c, fetextrender::full_text_rgb_entry, &[line, sx(x.wrapping_add(2)), sx(y.wrapping_add(10)), colour, 0, 0]);
        let right = x.wrapping_add(w);
        let ty = sx(y.wrapping_add(6));
        let word = call(c, TEXT_WORD, &[TEXT_ON]);
        let on = c.m.r32(colours);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(right.wrapping_sub(90)), ty, on, 0, 2]);
        let word = call(c, TEXT_WORD, &[TEXT_OFF]);
        let off = c.m.r32(colours + 4);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(right.wrapping_sub(30)), ty, off, 0, 2]);
    });
    // the frame: two dividers, the top and the bottom
    let (x, w, y) = (sx(x), sx(w), sx(y));
    let right = x.wrapping_add(w);
    call_addr(c, DRAW_SQUARE, &[frame, right.wrapping_sub(120), y, 2, 19]);
    call_addr(c, DRAW_SQUARE, &[frame, right.wrapping_sub(60), y, 2, 19]);
    call_addr(c, DRAW_SQUARE, &[frame, x, y, w, 1]);
    call_addr(c, DRAW_SQUARE, &[frame, x, y.wrapping_add(19), w, u32::MAX]);
}

pub fn abi_song_menu_draw_one_song(c: &mut Cpu) {
    let (w, on_fade, sel_fade) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    song_menu_draw_one_song(c, c.r[5], c.r[6], c.r[7], w, on_fade, sel_fade);
}

/// `tInsideBoxSongMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001efd4): up, down, left and right.
pub fn abi_song_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = KEY_UP | KEY_DOWN | femenu::KEY_LEFT | femenu::KEY_RIGHT;
}

/// `tInsideBoxSongMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001efdc): with songs loaded, left
/// and right turn the current song on or off (with a sound); up and down move to the song
/// before or after, the rows' fades shifting and the list scrolling (none while it scrolls).
/// The keys it takes are used up.
pub fn song_menu_process_input(c: &mut Cpu, this: InsideBoxSongMenu, key: u32) {
    if song_list(&mut c.m) == 0 {
        return;
    }
    match c.m.r32(key) {
        femenu::KEY_LEFT => {
            call(c, femenu::PLAY_FE_SFX, &[5]);
        }
        femenu::KEY_RIGHT => {
            call(c, femenu::PLAY_FE_SFX, &[6]);
        }
        _ => {}
    }
    let m = &mut c.m;
    match m.r32(key) {
        femenu::KEY_LEFT | femenu::KEY_RIGHT => {
            let at = song_on(current_song(m));
            let toggled = (m.r32(at) == 0) as u32;
            m.w32(at, toggled);
        }
        KEY_UP => {
            if (current_song(m) as i32) > 0 && this.moving(m) == 0 {
                this.set_moving(m, -(SONG_ROW as i16));
                this.set_moving_dir(m, SCROLL_STEP);
                for k in (1..5).rev() {
                    let v = this.on_off_fade(m, k - 1);
                    this.set_on_off_fade(m, k, v);
                    let v = this.sel_fade(m, k - 1);
                    this.set_sel_fade(m, k, v);
                }
                this.set_on_off_fade(m, 0, 0);
                this.set_sel_fade(m, 0, 0);
                let state = m.r32(MUSIC_STATE);
                let v = m.r16(state + 116).wrapping_sub(1);
                m.w16(state + 116, v);
                call(c, femenu::PLAY_FE_SFX, &[3]);
            }
        }
        KEY_DOWN => {
            let list = song_list(m);
            let last = m.r32(list).wrapping_sub(1);
            if (current_song(m) as i32) < last as i32 && this.moving(m) == 0 {
                this.set_moving(m, SONG_ROW as i16);
                this.set_moving_dir(m, -SCROLL_STEP);
                for k in 0..4 {
                    let v = this.on_off_fade(m, k + 1);
                    this.set_on_off_fade(m, k, v);
                    let v = this.sel_fade(m, k + 1);
                    this.set_sel_fade(m, k, v);
                }
                this.set_on_off_fade(m, 4, 0);
                this.set_sel_fade(m, 4, 0);
                let state = m.r32(MUSIC_STATE);
                let v = m.r16(state + 116).wrapping_add(1);
                m.w16(state + 116, v);
                call(c, femenu::PLAY_FE_SFX, &[4]);
            }
        }
        _ => return,
    }
    c.m.w32(key, KEY_NONE);
}

pub fn abi_song_menu_process_input(c: &mut Cpu) {
    song_menu_process_input(c, InsideBoxSongMenu(c.r[4]), c.r[6]);
}

/// `tMenuItemControllerLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001f1e4): unless faded
/// out, the choice's text at its own place (offset by `x`, `y`) and its value (the sub-menu
/// player's) right of it; the arrows, the bar under it and its square.
pub fn controller_choice_draw(c: &mut Cpu, this: MenuItemLeftRightFade, x: u32, y: u32) {
    let f = Fade::of_choice(this);
    let text = this.text_description(&mut c.m);
    let tx = call(c, WORD_X, &[text]).wrapping_add(x);
    let ty = call(c, WORD_Y, &[text]).wrapping_add(y);
    let bar = c.m.r32(SHAPES).wrapping_add(BAR * 32);
    let faded = |c: &mut Cpu| (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    let (sel, fade) = faded(c);
    let arrows = call(c, CALC_FADE_VAL_FADED, &[ARROWS_COLOUR, 0xBEBE, sel, fade]);
    let (sel, fade) = faded(c);
    let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade]);
    with_buffer(c, 8, |c, options| {
        c.m.w32(options + 4, arrows);
        if f.value(&mut c.m) != 128 {
            let word = call(c, TEXT_WORD, &[text]);
            call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), colour, 0, 0]);
            let (data, player) = (this.data(&mut c.m), c.m.r32(SUB_MENU_PLAYER));
            let value = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::TEXT_VALUE, &[player]);
            let word = call(c, TEXT_WORD, &[sx(value)]);
            call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(151)), sx(ty), colour, 0, 2]);
        }
        let (sel, fade) = faded(c);
        let v = call(c, CALC_FADE_VAL_FADED, &[0, 0xBEBE, sel, fade]);
        let (x, y) = (sx(tx), sx(ty));
        c.m.w32(options + 4, v);
        call(c, DRAW_SHAPE_EXTENDED, &[10, 24, x.wrapping_add(131), y, 0, 1, options]);
        call(c, DRAW_SHAPE_EXTENDED, &[11, 24, x.wrapping_add(161), y, 0, 1, options]);
        let bar_y = y.wrapping_sub(3);
        let m = &mut c.m;
        let bar_x = x.wrapping_sub(shape_w(m, bar).wrapping_sub(176));
        let fade = f.value(m) as i32 as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[BAR, 8, bar_x, bar_y, fade, 0, 0]);
        let m = &mut c.m;
        let (w, h) = (176u32.wrapping_sub(shape_w(m, bar)), shape_h(m, bar));
        call_addr(c, DRAW_SQUARE, &[0, x, bar_y, w, h]);
    });
}

pub fn abi_controller_choice_draw(c: &mut Cpu) {
    controller_choice_draw(c, MenuItemLeftRightFade(c.r[4]), c.r[5], c.r[6]);
}

// ---- The sliders in a box: a plain one, and a two-way one that calibrates a pad's axis.

/// An inside slider's height.
const INSIDE_SLIDER_HEIGHT: i16 = 5;
/// The inside sliders' lines' colours: unselected, selected; the two-way slider's middle mark.
const LINE_DIM: u32 = 0x0055_1E00;
const LINE_LIT: u32 = 0x00C8_3C1E;
const MARK_LIT: u32 = 0xBEBE;

/// `tInsideBoxLeftRightSlider::tInsideBoxLeftRightSlider` (FEMENUOPTIONS.CPP, 0x8001f3e8): a
/// slider 5 high.
pub fn inside_slider(c: &mut Cpu, this: MenuItemLeftRightSlider, text: u32, data: ListIterator) -> u32 {
    femenu::left_right_slider(c, this, text, data);
    let m = &mut c.m;
    this.set_vtable(m, INSIDE_SLIDER_VT);
    this.set_sel_fade(m, 0);
    this.set_height(m, INSIDE_SLIDER_HEIGHT);
    this.0
}

pub fn abi_inside_slider(c: &mut Cpu) {
    c.r[2] = inside_slider(c, MenuItemLeftRightSlider(c.r[4]), c.r[5], ListIterator(c.r[6]));
}

/// `tInsideBoxLeftRightSlider::~tInsideBoxLeftRightSlider` (FEMENUOPTIONS.CPP, 0x8001f428).
pub fn abi_inside_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, INSIDE_SLIDER_VT)), femenu::abi_left_right_slider_dtor);
}

/// A slider's value (its iterator's virtual `Value`) and the iterator's range.
fn slider_value(c: &mut Cpu, this: MenuItemLeftRightSlider) -> (u32, u32, u32) {
    let data = this.data(&mut c.m);
    let v = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]) & 0xFF;
    let m = &mut c.m;
    let data = this.data(m);
    (v, data.min_value(m) as u32, data.max_value(m) as u32)
}

/// A slider's place, plus (`dx`, `dy`), sign-extended.
fn slider_at(m: &mut Mem, this: MenuItemLeftRightSlider, dx: u32, dy: u32) -> (u32, u32) {
    (sx((this.x(m) as u16 as u32).wrapping_add(dx)), sx((this.y(m) as u16 as u32).wrapping_add(dy)))
}

/// `tInsideBoxLeftRightSlider::Draw` (FEMENUOPTIONS.CPP, 0x8001f450): the slider at (`x`, `y`)
/// `w` wide: two lines, its text and the slider at the value.
pub fn inside_slider_draw(c: &mut Cpu, this: MenuItemLeftRightSlider, x: u32, y: u32, w: u32) {
    this.set_x(&mut c.m, x as i16);
    this.set_y(&mut c.m, y as i16);
    let sel = this.sel_fade(&mut c.m) as i32 as u32;
    let line = call(c, CALC_FADE_VAL_SEL, &[LINE_DIM, LINE_LIT, sel]);
    let sel = this.sel_fade(&mut c.m) as i32 as u32;
    let colour = call(c, CALC_TEXT_FADE_UNSEL_TO_SEL, &[6, sel, 0]);
    for dy in [0, 8] {
        let (px, py) = (this.x(&mut c.m) as i32 as u32, this.y(&mut c.m) as i32 as u32);
        call_addr(c, DRAW_SQUARE, &[line, px, py.wrapping_add(dy), w, 1]);
    }
    let text = this.text_description(&mut c.m);
    let word = call(c, TEXT_WORD, &[text]);
    let (tx, ty) = slider_at(&mut c.m, this, 4, 10);
    call(c, fetextrender::full_text_rgb_entry, &[word, tx, ty, colour, 0, 0]);
    let (v, min, max) = slider_value(c, this);
    let m = &mut c.m;
    let (sx_, sy) = slider_at(m, this, 4, 2);
    let (h, sel) = (this.height(m) as i32 as u32, this.sel_fade(m) as i32 as u32);
    call_addr(c, DRAW_SLIDER, &[v, min, max, sx_, sy, sx(w.wrapping_sub(8)), h, 4, 4, 0, 0, sel, 0]);
}

pub fn abi_inside_slider_draw(c: &mut Cpu) {
    inside_slider_draw(c, MenuItemLeftRightSlider(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// `tInsideBoxTwoWaySlider::tInsideBoxTwoWaySlider` (FEMENUOPTIONS.CPP, 0x8001f5d4): a slider 5
/// high calibrating `ty` (0..3: the NeGcon's twist or a stick each way).
pub fn two_way_slider(c: &mut Cpu, this: InsideBoxTwoWaySlider, text: u32, data: ListIterator, ty: u32) -> u32 {
    femenu::left_right_slider(c, this.base(), text, data);
    let m = &mut c.m;
    this.set_vtable(m, TWO_WAY_SLIDER_VT);
    this.set_type_(m, ty as i16);
    this.set_sel_fade(m, 0);
    this.set_height(m, INSIDE_SLIDER_HEIGHT);
    this.set_active(m, 0);
    this.0
}

pub fn abi_two_way_slider(c: &mut Cpu) {
    c.r[2] = two_way_slider(c, InsideBoxTwoWaySlider(c.r[4]), c.r[5], ListIterator(c.r[6]), c.r[7]);
}

/// `tInsideBoxTwoWaySlider::~tInsideBoxTwoWaySlider` (FEMENUOPTIONS.CPP, 0x8001f628).
pub fn abi_two_way_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], c.r[5], Some((ITEM_VPTR, TWO_WAY_SLIDER_VT)), femenu::abi_left_right_slider_dtor);
}

/// `tInsideBoxTwoWaySlider::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001f650): accept starts the
/// calibration, the stop key ends it (with sounds); calibrating it takes every key, otherwise
/// left and right.
pub fn two_way_slider_process_input(c: &mut Cpu, this: InsideBoxTwoWaySlider, key: u32) {
    if c.m.r32(key) == femenu::KEY_ACCEPT {
        call(c, femenu::PLAY_FE_SFX, &[SOUND_ACTIVATE]);
        this.set_active(&mut c.m, 1);
        c.m.w32(key, KEY_NONE);
    }
    if c.m.r32(key) == KEY_STOP {
        if this.active(&mut c.m) == 0 {
            return;
        }
        call(c, femenu::PLAY_FE_SFX, &[SOUND_BACK]);
        this.set_active(&mut c.m, 0);
        c.m.w32(key, KEY_NONE);
    }
    let m = &mut c.m;
    let k = m.r32(key);
    if this.active(m) != 0 || k == femenu::KEY_LEFT || k == femenu::KEY_RIGHT {
        m.w32(key, KEY_NONE);
    }
}

pub fn abi_two_way_slider_process_input(c: &mut Cpu) {
    two_way_slider_process_input(c, InsideBoxTwoWaySlider(c.r[4]), c.r[6]);
}

/// `tInsideBoxTwoWaySlider::Draw` (FEMENUOPTIONS.CPP, 0x8001f700): calibrating, the pad
/// measured first; then the slider at (`x`, `y`) `w` wide, in two halves either side of a mark:
/// their lines, its text and both halves' sliders at the value.
pub fn two_way_slider_draw(c: &mut Cpu, this: InsideBoxTwoWaySlider, x: u32, y: u32, w: u32) {
    if this.active(&mut c.m) != 0 {
        calibrate(c, this);
    }
    let slider = this.base();
    let half = ((w as i32) >> 1) as u32;
    slider.set_x(&mut c.m, x as i16);
    slider.set_y(&mut c.m, y as i16);
    let sel = |c: &mut Cpu| this.sel_fade(&mut c.m) as i32 as u32;
    let s = sel(c);
    let line = call(c, CALC_FADE_VAL_SEL, &[LINE_DIM, LINE_LIT, s]);
    let s = sel(c);
    let mark = call(c, CALC_FADE_VAL_SEL, &[LINE_DIM, MARK_LIT, s]);
    let s = sel(c);
    let colour = call(c, CALC_TEXT_FADE_UNSEL_TO_SEL, &[6, s, 0]);
    let len = half.wrapping_sub(4);
    // the halves' lines, above and below
    for (dx, dy) in [(0, 0), (8, 0), (0, 8), (9, 8)] {
        let (px, py) = (slider.x(&mut c.m) as i32 as u32, slider.y(&mut c.m) as i32 as u32);
        let px = if dx != 0 { px.wrapping_add(len).wrapping_add(dx) } else { px };
        call_addr(c, DRAW_SQUARE, &[line, px, py.wrapping_add(dy), len, 1]);
    }
    let (px, py) = (slider.x(&mut c.m) as i32 as u32, slider.y(&mut c.m) as i32 as u32);
    call_addr(c, DRAW_SQUARE, &[mark, px.wrapping_add(len).wrapping_add(3), py, 2, 9]);
    let text = this.text_description(&mut c.m);
    let word = call(c, TEXT_WORD, &[text]);
    let (tx, ty) = slider_at(&mut c.m, slider, 4, 10);
    call(c, fetextrender::full_text_rgb_entry, &[word, tx, ty, colour, 0, 0]);
    // the left half's slider (reversed), the right half's
    let w = sx(half.wrapping_sub(8));
    let (v, min, max) = slider_value(c, slider);
    let m = &mut c.m;
    let (sx_, sy) = slider_at(m, slider, 1, 2);
    let (h, s) = (slider.height(m) as i32 as u32, slider.sel_fade(m) as i32 as u32);
    call_addr(c, DRAW_SLIDER, &[v, min, max, sx_, sy, w, h, 4, 4, 1, 0, s, 0]);
    let (v, min, max) = slider_value(c, slider);
    let m = &mut c.m;
    let (sx_, sy) = slider_at(m, slider, len.wrapping_add(10), 2);
    let (h, s) = (slider.height(m) as i32 as u32, slider.sel_fade(m) as i32 as u32);
    call_addr(c, DRAW_SLIDER, &[v, min, max, sx_, sy, w, h, 4, 4, 0, 0, s, 0]);
}

pub fn abi_two_way_slider_draw(c: &mut Cpu) {
    two_way_slider_draw(c, InsideBoxTwoWaySlider(c.r[4]), c.r[5], c.r[6], c.r[7]);
}

/// A pad's state (32 bytes each from `PAD_INFO`): its kind (+5), the NeGcon's twist and
/// buttons (+8, +9, +10), the left and right sticks' axes (+8, +10).
fn pad_info(pad: u32) -> u32 {
    PAD_INFO.wrapping_add(pad << 5)
}

/// How far from the middle (128) a byte of the pad's state is.
fn from_middle(v: u32) -> u32 {
    v.abs_diff(128)
}

/// The furthest from the middle of pad `pad`'s two axes at +`at` and +`at + 1`.
fn stick_range(m: &mut Mem, pad: u32, at: u32) -> u32 {
    let p = pad_info(pad);
    let (x, y) = (m.r8(p + at) as u32, m.r8(p + at + 1) as u32);
    from_middle(y).max(from_middle(x))
}

/// `GetCurrentStickRange` (FEMENUOPTIONS.CPP, 0x8001f9c0): how far pad `pad`'s left stick is
/// from the middle.
pub fn abi_get_current_stick_range(c: &mut Cpu) {
    c.r[2] = stick_range(&mut c.m, c.r[4], 8);
}

/// `GetCurrentStickRange2` (FEMENUOPTIONS.CPP, 0x8001fa1c): the right stick's.
pub fn abi_get_current_stick_range2(c: &mut Cpu) {
    c.r[2] = stick_range(&mut c.m, c.r[4], 10);
}

/// The front end's calibration, a byte a pad from each place: the NeGcon's twist each way, its
/// two buttons' ranges; the left stick's range each way, the right's.
const TWIST_MAX: u32 = 841;
const TWIST_MIN: u32 = 843;
const NEGCON_I: u32 = 845;
const NEGCON_II: u32 = 847;
const LEFT_STICK_MAX: u32 = 849;
const LEFT_STICK_MIN: u32 = 851;
const RIGHT_STICK_MAX: u32 = 853;
const RIGHT_STICK_MIN: u32 = 855;
/// How far apart a range's two ends are kept.
const CALIBRATION_GAP: i32 = 10;

/// `tInsideBoxTwoWaySlider::Calibrate` (FEMENUOPTIONS.CPP, 0x8001fa78): the help text for it;
/// the front end input player's pad measured into the front end's calibration, each end kept
/// 10 from the other.
pub fn calibrate(c: &mut Cpu, this: InsideBoxTwoWaySlider) {
    let m = &mut c.m;
    let arg = m.r32(CALIBRATE_HELP_ARG);
    let pad = FEApplication(m.r32(FE_APP)).input_player(m) as u8 as u32;
    let info = pad_info(pad);
    let help = call(c, CALIBRATE_HELP, &[arg]);
    let m = &mut c.m;
    m.w32(BOX_HELP_TEXT, help);
    let cal = pad.wrapping_add(FRONT_END);
    let kind = m.r8(info + 5) as u32;
    let analog = kind == PAD_ANALOG || kind == PAD_DUAL_SHOCK;
    let at_least = |m: &mut Mem, at: u32, v: i32, other: u32| {
        let floor = m.r8(cal + other) as i32 + CALIBRATION_GAP;
        m.w8(cal + at, v.max(floor) as u8);
    };
    let at_most = |m: &mut Mem, at: u32, v: i32, other: u32| {
        let ceil = m.r8(cal + other) as i32 - CALIBRATION_GAP;
        m.w8(cal + at, v.min(ceil).max(0) as u8);
    };
    match this.type_(m) {
        0 if kind == PAD_NEGCON => {
            let v = from_middle(m.r8(info + 8) as u32) as i32;
            at_least(m, TWIST_MAX, v, TWIST_MIN);
        }
        0 if analog => {
            let v = stick_range(m, pad, 8) as i32;
            at_least(m, LEFT_STICK_MAX, v, LEFT_STICK_MIN);
        }
        1 if kind == PAD_NEGCON => {
            let v = from_middle(m.r8(info + 8) as u32) as i32;
            at_most(m, TWIST_MIN, v, TWIST_MAX);
        }
        1 if analog => {
            let v = stick_range(m, pad, 8) as i32;
            at_most(m, LEFT_STICK_MIN, v, LEFT_STICK_MAX);
        }
        2 if kind == PAD_NEGCON => {
            let v = m.r8(info + 9);
            m.w8(cal + NEGCON_I, v);
        }
        2 if analog => {
            let v = stick_range(m, pad, 10) as i32;
            at_least(m, RIGHT_STICK_MAX, v, RIGHT_STICK_MIN);
        }
        3 if kind == PAD_NEGCON => {
            let v = m.r8(info + 10).max(10);
            m.w8(cal + NEGCON_II, v);
        }
        3 if analog => {
            let v = stick_range(m, pad, 10) as i32;
            at_most(m, RIGHT_STICK_MIN, v, RIGHT_STICK_MAX);
        }
        _ => {}
    }
}

pub fn abi_calibrate(c: &mut Cpu) {
    calibrate(c, InsideBoxTwoWaySlider(c.r[4]));
}

// ---- tUserNameMenuItem: the name typed with a grid of letters (6 columns; `gNameRows` rows
// of 9 bytes: '!' and '@' end the name, '#' and '$' delete a letter, '&' and '^' add a space,
// '-' is an empty cell).

const COLUMNS: i16 = 6;
const EMPTY_CELL: u8 = b'-';

/// The grid's cell (`row`, `col`): its letter's place in the item (its rows from +0x26).
fn cell(this: u32, row: u32, col: u32) -> u32 {
    this.wrapping_add(col).wrapping_add(row.wrapping_mul(9)).wrapping_add(0x26)
}

/// The current cell's letter.
fn current_letter(m: &mut Mem, this: UserNameMenuItem) -> u8 {
    let (row, col) = (this.current_row(m) as i32 as u32, this.current_column(m) as i32 as u32);
    m.r8(cell(this.0, row, col))
}

/// The cursor one column left or right, round.
fn column_left(m: &mut Mem, this: UserNameMenuItem) {
    let col = this.current_column(m).wrapping_sub(1);
    this.set_current_column(m, if col < 0 { COLUMNS - 1 } else { col });
}

fn column_right(m: &mut Mem, this: UserNameMenuItem) {
    let col = this.current_column(m).wrapping_add(1);
    this.set_current_column(m, if col >= COLUMNS { 0 } else { col });
}

/// `tUserNameMenuItem::tUserNameMenuItem` (FEMENUOPTIONS.CPP, 0x8001fd98): the grid at its
/// top left, its rows empty, no name.
pub fn user_name(c: &mut Cpu, this: UserNameMenuItem, text: u32) -> u32 {
    femenu::menu_item(&mut c.m, this.base(), text);
    let m = &mut c.m;
    this.set_vtable(m, USER_NAME_VT);
    this.set_current_row(m, 0);
    this.set_current_column(m, 0);
    for row in 0..10 {
        m.w8(cell(this.0, row, 0), 0);
    }
    this.set_data(m, 0);
    this.0
}

pub fn abi_user_name(c: &mut Cpu) {
    c.r[2] = user_name(c, UserNameMenuItem(c.r[4]), c.r[5]);
}

/// `CheckForCheats` (FEMENUOPTIONS.CPP, 0x8001fe0c): `name` (padded with zeros to 8) entered
/// as a cheat or a bonus code; whether it was one.
pub fn check_for_cheats(c: &mut Cpu, name: u32) -> u32 {
    let len = call(c, STRLEN, &[name]) as i32;
    for k in len.max(0)..8 {
        c.m.w8(name.wrapping_add(k as u32), 0);
    }
    if call(c, ACTIVATE_CHEAT, &[name]) != 0 {
        return 1;
    }
    (call(c, ACTIVATE_BONUS, &[name]) != 0) as u32
}

pub fn abi_check_for_cheats(c: &mut Cpu) {
    c.r[2] = check_for_cheats(c, c.r[4]);
}

/// The menu flag that ends the name with back (else start).
const MENU_NAME_ENDS_WITH_BACK: u32 = 0x100;
/// The user name's sounds: a letter typed or deleted, refused or done.
const SOUND_LETTER: u32 = 21;
const SOUND_REFUSED: u32 = 1;

/// `tUserNameMenuItem::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001fe8c): up and down change rows
/// (round), left and right columns (round; right past empty cells), with sounds; accept types
/// the letter (up to the longest), deletes one, adds a space or ends the name (a cheat, or
/// start or back by the menu's flag); the cursor kept off empty cells; the name's case set as
/// the records have it.
pub fn user_name_process_input(c: &mut Cpu, this: UserNameMenuItem, key: u32) {
    let k = c.m.r32(key);
    let mut used = true;
    match k {
        KEY_UP | KEY_DOWN => {
            let m = &mut c.m;
            let rows = m.r16(NAME_ROWS) as i16;
            let row = this.current_row(m);
            let sound = if k == KEY_UP {
                this.set_current_row(m, if row.wrapping_sub(1) < 0 { rows.wrapping_sub(1) } else { row.wrapping_sub(1) });
                3
            } else {
                this.set_current_row(m, if row.wrapping_add(1) >= rows { 0 } else { row.wrapping_add(1) });
                4
            };
            call(c, femenu::PLAY_FE_SFX, &[sound]);
            if this.current_column(&mut c.m) < 0 {
                this.set_current_column(&mut c.m, 0);
            }
        }
        femenu::KEY_LEFT => {
            column_left(&mut c.m, this);
            call(c, femenu::PLAY_FE_SFX, &[3]);
        }
        femenu::KEY_RIGHT => {
            column_right(&mut c.m, this);
            call(c, femenu::PLAY_FE_SFX, &[4]);
            let m = &mut c.m;
            m.w32(key, KEY_NONE);
            while current_letter(m, this) == EMPTY_CELL {
                column_right(m, this);
            }
            let name = this.data(m);
            call(c, CASE_FOR_RECORDS, &[name]);
            return;
        }
        femenu::KEY_ACCEPT => {
            let name = this.data(&mut c.m);
            let len = sx(call(c, STRLEN, &[name]));
            let m = &mut c.m;
            let letter = current_letter(m, this);
            let room = (len as i32) < this.max_string_length(m) as i32;
            // a letter or a space typed at the end of the name
            let type_at_end = |c: &mut Cpu, ch: u8| {
                let m = &mut c.m;
                let name = this.data(m);
                m.w8(name.wrapping_add(len), ch);
                let name = this.data(m);
                m.w8(len.wrapping_add(name).wrapping_add(1), 0);
                call(c, femenu::PLAY_FE_SFX, &[SOUND_LETTER]);
            };
            match letter {
                b'#' | b'$' => {
                    if (len as i32) > 0 {
                        let name = this.data(m);
                        m.w8(len.wrapping_add(name).wrapping_sub(1), 0);
                        call(c, femenu::PLAY_FE_SFX, &[SOUND_LETTER]);
                    }
                }
                b'!' | b'@' => {
                    used = false;
                    let name = this.data(m);
                    if check_for_cheats(c, name) != 0 {
                        let name = this.data(&mut c.m);
                        c.m.w8(name, 0);
                    } else {
                        let m = &mut c.m;
                        let menu = FEApplication(m.r32(FE_APP)).current_menu(m, 0);
                        let k = if menu.flags(m) & MENU_NAME_ENDS_WITH_BACK != 0 { KEY_BACK } else { femenu::KEY_START };
                        m.w32(key, k);
                        call(c, femenu::PLAY_FE_SFX, &[SOUND_REFUSED]);
                    }
                }
                b'&' | b'^' => {
                    used = false;
                    if room {
                        type_at_end(c, b' ');
                    } else {
                        call(c, femenu::PLAY_FE_SFX, &[SOUND_REFUSED]);
                    }
                }
                _ => {
                    if room {
                        type_at_end(c, letter);
                    } else {
                        used = false;
                        call(c, femenu::PLAY_FE_SFX, &[SOUND_REFUSED]);
                    }
                }
            }
        }
        _ => used = false,
    }
    let m = &mut c.m;
    if used {
        m.w32(key, KEY_NONE);
    }
    // off the empty cells, to the left
    while current_letter(m, this) == EMPTY_CELL {
        column_left(m, this);
    }
    let name = this.data(m);
    call(c, CASE_FOR_RECORDS, &[name]);
}

pub fn abi_user_name_process_input(c: &mut Cpu) {
    user_name_process_input(c, UserNameMenuItem(c.r[4]), c.r[6]);
}

/// `SpecialCharacter` (FEMENUOPTIONS.CPP, 0x8002024c): the shape of an accented letter (225..252,
/// by the original's jump table at 0x80010988), 0 for others.
pub fn special_character(m: &mut Mem, ch: u32) -> u32 {
    let k = (ch & 0xFF).wrapping_sub(225);
    if k >= 28 {
        return 0;
    }
    match m.r32(SPECIAL_CHARACTERS + 4 * k) {
        t @ 0x8002_0280..=0x8002_02c8 if (t - 0x8002_0280) % 8 == 0 => 81 + (t - 0x8002_0280) / 8,
        0x8002_02d0 => 91,
        _ => 0,
    }
}

pub fn abi_special_character(c: &mut Cpu) {
    c.r[2] = special_character(&mut c.m, c.r[4]);
}

/// The user name's cursor's shape for a grid letter: the end key, delete, space, a letter or a
/// digit, an accented letter.
fn cursor_shape(m: &mut Mem, letter: u8) -> u32 {
    match letter {
        b'!' | b'@' => 78,
        b'#' | b'$' => 79,
        b'&' | b'^' => 80,
        b'a'..=b'z' => letter as u32 - 55,
        b'0'..=b'9' => letter as u32 + 20,
        _ => special_character(m, letter as u32),
    }
}

/// The name bar's shape (33), under the name.
const NAME_BAR: u32 = 33;

/// `tUserNameMenuItem::Draw(bool)` (FEMENUOPTIONS.CPP, 0x800202dc): unless faded out, its text,
/// the name typed so far a letter at a time (the longest name's cells right-aligned from its
/// text's place, 20 apart), lines under the cells left, the cursor's letter (in the pulsing
/// yellow) on the grid; the two bars at its place.
pub fn user_name_draw(c: &mut Cpu, this: UserNameMenuItem) {
    let text = this.text_description(&mut c.m);
    let tx = call(c, WORD_X, &[text]);
    let ty = call(c, WORD_Y, &[text]);
    let bar = c.m.r32(SHAPES).wrapping_add(BAR * 32);
    // (a one-letter string at +0; the cursor's shape options at +8)
    with_buffer(c, 16, |c, buf| {
        if this.fade_val(&mut c.m) == 128 {
            return;
        }
        c.m.w8(buf + 1, 0);
        let fade = this.fade_val(&mut c.m) as i32 as u32;
        let colour = call(c, CALC_FADE_VAL, &[0xBEBE, fade]);
        let fade = this.fade_val(&mut c.m) as i32 as u32;
        let line = call(c, CALC_FADE_VAL, &[0xBEBE, fade]);
        let word = call(c, TEXT_WORD, &[text]);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), colour, 0, 0]);
        let name = this.data(&mut c.m);
        let len = sx(call(c, STRLEN, &[name]));
        let longest = this.max_string_length(&mut c.m) as i32;
        let left = tx.wrapping_add(78).wrapping_sub((longest as u32).wrapping_mul(10));
        // the letters typed
        for k in 0..(len as i32).max(0) as u32 {
            let m = &mut c.m;
            let name = this.data(m);
            let ch = m.r8(name.wrapping_add(k));
            m.w8(buf, ch);
            call(c, fetextrender::full_text_rgb_entry, &[buf, sx(left.wrapping_add(k.wrapping_mul(20))), sx(ty.wrapping_add(17)), colour, 1, 0]);
        }
        // lines under the cells left
        let mut k = len as u16 as u32;
        while (sx(k) as i32) < this.max_string_length(&mut c.m) as i32 {
            call_addr(c, DRAW_SQUARE, &[line, sx(left).wrapping_add(sx(k).wrapping_mul(20)), ty.wrapping_add(25), 17, 1]);
            k = k.wrapping_add(1);
        }
        // the cursor, once faded in
        let m = &mut c.m;
        let (col, row) = (this.current_column(m) as i32 as u32, this.current_row(m) as i32 as u32);
        let cx = col.wrapping_mul(28);
        let cy = (m.r16(NAME_GRID_TOP) as i16 as i32 as u32).wrapping_add(row.wrapping_mul(15));
        if this.fade_val(m) == 0 {
            let yellow = m.r32(PULSATE_YELLOW);
            m.w32(buf + 12, yellow);
            let letter = current_letter(m, this);
            let shape = cursor_shape(m, letter);
            call(c, DRAW_SHAPE_EXTENDED, &[shape, 16, cx.wrapping_add(253), cy.wrapping_sub(3), 0, 0, buf + 8]);
        }
    });
    // the bars under its text and the name
    let m = &mut c.m;
    let (y1, end) = (ty.wrapping_sub(3), tx.wrapping_add(156));
    let fade = this.fade_val(m) as i32 as u32;
    let x = end.wrapping_sub(shape_w(m, bar));
    call(c, DRAW_SHAPE_EXTENDED, &[BAR, 8, x, y1, fade, 0, 0]);
    let m = &mut c.m;
    let (w, h) = (shape_w(m, bar), shape_h(m, bar));
    call_addr(c, DRAW_SQUARE, &[0, tx, y1, 156u32.wrapping_sub(w), h]);
    let m = &mut c.m;
    let y2 = ty.wrapping_add(12);
    let name_bar = m.r32(SHAPES).wrapping_add(NAME_BAR * 32);
    let fade = this.fade_val(m) as i32 as u32;
    let x = end.wrapping_sub(shape_w(m, name_bar));
    call(c, DRAW_SHAPE_EXTENDED, &[NAME_BAR, 8, x, y2, fade, 0, 0]);
    let m = &mut c.m;
    let (w, h) = (shape_w(m, name_bar), shape_h(m, name_bar));
    call_addr(c, DRAW_SQUARE, &[0, tx, y2, 156u32.wrapping_sub(w), h]);
}

pub fn abi_user_name_draw(c: &mut Cpu) {
    user_name_draw(c, UserNameMenuItem(c.r[4]));
}

/// `tUserNameMenuItem::TransitionOff` (FEMENUOPTIONS.CPP, 0x800206d4): fading out.
pub fn abi_user_name_transition_off(c: &mut Cpu) {
    UserNameMenuItem(c.r[4]).set_fade_dir(&mut c.m, FADE_STEP);
}

/// The front end's language (a byte).
const LANGUAGE: u32 = FRONT_END + 0x340;
/// Where the application keeps each player's pending speech (-1: none).
const PENDING_SPEECH: u32 = 888;

fn is_end_key(ch: u8) -> bool {
    matches!(ch, b'!' | b'@')
}

/// `tUserNameMenuItem::TransitionOn` (FEMENUOPTIONS.CPP, 0x800206e0): the grid's rows from
/// texts 508.., as many as the language has; the cursor on the end key (searched row by row
/// along each row, from the current column on the first); fading in from 128, unselected; the
/// player's pending speech played.
pub fn user_name_transition_on(c: &mut Cpu, this: UserNameMenuItem) {
    let row_at = |row: u32| cell(this.0, row, 0);
    for k in 0..10u32 {
        let word = call(c, TEXT_WORD, &[GRID_TEXTS + k]);
        call(c, SPRINTF, &[row_at(k), word]);
    }
    let m = &mut c.m;
    let language = m.r8(LANGUAGE) as u32;
    let rows = m.r16(NAME_ROWS_BY_LANGUAGE + 2 * language);
    m.w16(NAME_ROWS, rows);
    this.set_current_row(m, 0);
    let mut found = is_end_key(current_letter(m, this));
    while !found && (this.current_row(&mut c.m) as i32) < c.m.r16(NAME_ROWS) as i16 as i32 {
        let m = &mut c.m;
        this.set_current_column(m, 0);
        let row = row_at(this.current_row(m) as i32 as u32);
        if !is_end_key(m.r8(row)) {
            loop {
                let row = row_at(this.current_row(&mut c.m) as i32 as u32);
                let len = call(c, STRLEN, &[row]) as i32;
                let m = &mut c.m;
                if this.current_column(m) as i32 >= len {
                    break;
                }
                let col = this.current_column(m).wrapping_add(1);
                this.set_current_column(m, col);
                if is_end_key(m.r8(row.wrapping_add(col as i32 as u32))) {
                    break;
                }
            }
        }
        let m = &mut c.m;
        let next = this.current_row(m).wrapping_add(1);
        this.set_current_row(m, next);
        found = is_end_key(current_letter(m, this));
    }
    let m = &mut c.m;
    let row = this.current_row(m);
    this.set_fade_val(m, 128);
    this.set_fade_dir(m, -FADE_STEP);
    let player = this.player(m) as i32 as u32;
    this.set_sel_fade(m, 0);
    this.set_current_row(m, row.wrapping_sub(1));
    let pending = m.r32(FE_APP).wrapping_add(player << 2).wrapping_add(PENDING_SPEECH);
    let speech = m.r32(pending);
    if speech != u32::MAX {
        call(c, PLAY_SPEECH, &[2, speech]);
        let m = &mut c.m;
        let player = this.player(m) as i32 as u32;
        let pending = m.r32(FE_APP).wrapping_add(player << 2).wrapping_add(PENDING_SPEECH);
        m.w32(pending, u32::MAX);
    }
}

pub fn abi_user_name_transition_on(c: &mut Cpu) {
    user_name_transition_on(c, UserNameMenuItem(c.r[4]));
}

/// `tUserNameMenuItem::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x80020914): the fade at its
/// end (fading in, also while the memory card is busy).
pub fn abi_user_name_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, UserNameMenuItem(c.r[4]));
    let (fade, step) = (this.fade_val(m), this.fade_dir(m));
    let fading = (step < 0 && fade > 0) || (step > 0 && (fade < 128 || m.r16(CARD_BUSY) != 0));
    this.set_in_transition(m, fading as u32);
    c.r[2] = this.in_transition(m) ^ 1;
}

/// `tUserNameMenuItem::UpdateTransition` (FEMENUOPTIONS.CPP, 0x80020980).
pub fn abi_user_name_update_transition(c: &mut Cpu) {
    let this = UserNameMenuItem(c.r[4]);
    // (its fade's value and step are where the other items keep theirs, from +0x82)
    fade_update_transition(c, this.base(), Fade(this.0 + 0x82), c.r[5]);
}

// ---- tMemoryCardMenuItem: a button that opens (0..256) while enabled, closes while not.

/// `tMemoryCardMenuItem::Draw(bool)` (FEMENUOPTIONS.CPP, 0x800209fc): opening or closing (8 a
/// frame); unless faded out or closed, its text fading in over the second half, and its bar
/// growing over the first half to 150 (by the fade).
pub fn memory_card_draw(c: &mut Cpu, this: MemoryCardMenuItem) {
    let m = &mut c.m;
    let step = if this.flags(m) & ITEM_DISABLED != 0 { -8 } else { 8 };
    let open = this.enable_val(m).wrapping_add(step).clamp(0, 256);
    this.set_enable_val(m, open);
    let (open, f) = (open as i32, Fade::of_button(this.base()));
    let (late, early) = ((open - 128).clamp(0, 128) as u32, open.clamp(0, 128) as u32);
    if f.value(m) == 128 || this.enable_val(m) == 0 {
        return;
    }
    let text = this.text_description(m);
    let fade_in = 128 - late;
    let tx = call(c, WORD_X, &[text]);
    let ty = call(c, WORD_Y, &[text]);
    let faded = |c: &mut Cpu| (this.sel_fade(&mut c.m) as i32 as u32, f.value(&mut c.m) as i32 as u32);
    let (sel, fade) = faded(c);
    let colour = call(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade]);
    let (sel, fade) = faded(c);
    let bar_colour = call(c, CALC_FADE_VAL_FADED, &[ARROWS_COLOUR, 0xBEBE, sel, fade]);
    call(c, CALC_FADE_VAL, &[bar_colour, fade_in]);
    let colour = call(c, CALC_FADE_VAL, &[colour, fade_in]);
    if colour != 0 {
        let word = call(c, TEXT_WORD, &[text]);
        call(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), colour, 0, 0]);
    }
    let m = &mut c.m;
    let grown = (early.wrapping_mul(150) as i32) / 128;
    let len = (grown.wrapping_mul(128 - f.value(m) as i32) / 128) as u32;
    let by = ty.wrapping_sub(3);
    let bar = m.r32(SHAPES).wrapping_add(BAR * 32);
    let bx = tx.wrapping_add(len).wrapping_sub(shape_w(m, bar));
    call(c, DRAW_SHAPE_EXTENDED, &[BAR, 8, bx, by, 0, 0, 0]);
    let m = &mut c.m;
    let w = shape_w(m, bar);
    if (w as i32) < len as i32 {
        let h = shape_h(m, bar);
        call_addr(c, DRAW_SQUARE, &[0, tx, by, len.wrapping_sub(w), h]);
    }
}

pub fn abi_memory_card_draw(c: &mut Cpu) {
    memory_card_draw(c, MemoryCardMenuItem(c.r[4]));
}

/// Where the controller screen notes that its settings changed (the help text's word's state
/// +368).
const CONTROLLER_CHANGED: u32 = 368;

/// `tInsideBoxControllerLeftRightSlider::ProcessInput` (FEMENUOPTIONS.CPP, 0x80020c54): left,
/// or right below the top, notes the controller changed; then the slider's.
pub fn controller_slider_process_input(c: &mut Cpu, this: MenuItemLeftRightSlider, player: u32, key: u32) {
    let k = c.m.r32(key);
    if k == femenu::KEY_LEFT || k == femenu::KEY_RIGHT {
        let changed = k == femenu::KEY_LEFT || {
            let data = this.data(&mut c.m);
            let v = call_virtual(c, data.0, femenu::ITERATOR_VPTR, femenu::VALUE, &[u32::MAX]);
            let m = &mut c.m;
            v & 0xFF != this.data(m).max_value(m) as u32
        };
        if changed {
            let state = c.m.r32(CALIBRATE_HELP_ARG);
            c.m.w32(state + CONTROLLER_CHANGED, 1);
        }
    }
    femenu::left_right_slider_process_input(c, this, player, key);
}

pub fn abi_controller_slider_process_input(c: &mut Cpu) {
    controller_slider_process_input(c, MenuItemLeftRightSlider(c.r[4]), c.r[5], c.r[6]);
}

/// The destructors of the classes without one of their own: the base's, the vtable left.
fn destroy_as_base(c: &mut Cpu, base: fn(&mut Cpu)) {
    femenuextended::destroy(c, c.r[4], c.r[5], None, base);
}

/// `tInsideBoxControllerLeftRightSlider::~...` (FEMENUOPTIONS.CPP, 0x80020d18).
pub fn abi_controller_slider_dtor(c: &mut Cpu) {
    destroy_as_base(c, abi_inside_slider_dtor);
}

/// `tMemoryCardMenuItem::~...` (FEMENUOPTIONS.CPP, 0x80020d38).
pub fn abi_memory_card_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_go_to_menu_button_dtor);
}

/// `tMenuItemGoToMenuButtonFade::~...` (FEMENUOPTIONS.CPP, 0x80020d58).
pub fn abi_button_fade_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_go_to_menu_button_dtor);
}

/// `tUserNameMenuItem::~...` (FEMENUOPTIONS.CPP, 0x80020d78).
pub fn abi_user_name_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_menu_item_dtor);
}

/// `tMenuItemControllerLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020d98).
pub fn abi_controller_choice_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemOnOffLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020db8).
pub fn abi_on_off_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemDisplayLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020dd8).
pub fn abi_display_choice_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemSlidingActivated::~...` (FEMENUOPTIONS.CPP, 0x80020df8).
pub fn abi_sliding_activated_dtor(c: &mut Cpu) {
    destroy_as_base(c, abi_sliding_menu_dtor);
}

/// `tMenuItemLeftRightFade::~...` (FEMENUOPTIONS.CPP, 0x80020e18).
pub fn abi_left_right_fade_dtor(c: &mut Cpu) {
    destroy_as_base(c, femenu::abi_left_right_choice_dtor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU2, NEW, NOTHING, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The test's structures after FEMENU's: an item of each class here, the menus, a word for
    /// the flare's phase.
    const ITEMS: u32 = 0x8014_9000;
    const MENUS: u32 = 0x8014_A000;
    const PHASE: u32 = 0x8014_B000;
    /// The fading items' classes (vtables): left-right fades, the memory card's button, the
    /// display's, on-off and the controller's choices.
    const MEMORY_CARD_VT: u32 = 0x8001_0A60;
    const DISPLAY_CHOICE_VT: u32 = 0x8001_0D88;
    const ON_OFF_CHOICE_VT: u32 = 0x8001_0D30;
    const CONTROLLER_CHOICE_VT: u32 = 0x8001_0C18;
    const ITEM_VTS: [u32; 5] = [LEFT_RIGHT_FADE_VT, MEMORY_CARD_VT, DISPLAY_CHOICE_VT, ON_OFF_CHOICE_VT, CONTROLLER_CHOICE_VT];
    /// The sliders that go in a box (their four-argument `Draw` draws them): left-right, two-way
    /// (the analog sticks'), the controller's.
    const CONTROLLER_SLIDER_VT: u32 = 0x8001_0A08;
    const SLIDER_VTS: [u32; 3] = [INSIDE_SLIDER_VT, TWO_WAY_SLIDER_VT, CONTROLLER_SLIDER_VT];
    /// The sliding boxes: a menu's, an activated one's.
    const SLIDING_ACTIVATED_VT: u32 = 0x8001_0DE0;
    const SLIDING_VTS: [u32; 2] = [SLIDING_MENU_VT, SLIDING_ACTIVATED_VT];
    const ITEM_STRIDE: u32 = 0x80;
    const OPTIONS: u32 = MENUS;
    /// Two menus that go in the boxes.
    const INSIDE: u32 = MENUS + 0x100;
    const INSIDE2: u32 = MENUS + 0x200;
    const SONGS: u32 = MENUS + 0x300;
    /// The music's state (its current song, its song list) and the songs' lines.
    const MUSIC: u32 = 0x8014_C000;
    const SONG_LIST: u32 = 0x8014_C100;
    const SONG_TEXT: u32 = 0x8014_C800;

    fn item_of(k: u32) -> u32 {
        ITEMS + ITEM_STRIDE * k
    }

    fn slider_of(k: u32) -> u32 {
        item_of(ITEM_VTS.len() as u32 + k)
    }

    fn sliding_of(k: u32) -> u32 {
        item_of((ITEM_VTS.len() + SLIDER_VTS.len()) as u32 + k)
    }

    /// The audio slider, the user name's item and its name.
    fn audio() -> u32 {
        item_of((ITEM_VTS.len() + SLIDER_VTS.len() + SLIDING_VTS.len()) as u32)
    }

    fn user_name() -> u32 {
        audio() + ITEM_STRIDE
    }

    const NAME: u32 = 0x8014_B100;

    /// Texts short enough for the buffers the items' text goes through (`FullTextRGB`'s 128
    /// bytes: a longer one overwrites its saved registers, which the game's never do).
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

    fn text(rng: &mut Rng) -> u32 {
        let t = short_texts();
        t[pick(rng, t.len() as u32) as usize]
    }

    /// FEMENU's structures, then: an item of each class (disabled now and then; a text, the
    /// selection's fade, an iterator or a function; fades and steps about their ranges, still
    /// fading or not, the flare's phase or the enabled fade), an options menu of a few of them
    /// (the current item, in a transition or not, either way, the screen's fade about its
    /// range, a picture or none), the flare's phase, the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        // (the list's texts short too: the choices draw them)
        for k in 0..12 {
            let at = base::LIST + 2 * k;
            if (m.r16(at) as i16) != 0 {
                m.w16(at, text(rng) as u16);
            }
        }
        for (k, vt) in ITEM_VTS.into_iter().enumerate() {
            let it = item_of(k as u32);
            m.w32(it, (pick(rng, 4) == 0) as u32);
            m.w32(it + 4, text(rng));
            m.w16(it + 8, pick(rng, 140) as u16);
            m.w32(it + 20, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(it + ITEM_VPTR, vt);
            m.w32(it + 28, if vt == MEMORY_CARD_VT { [0, NOTHING][pick(rng, 2) as usize] } else { base::iterator(rng) });
            m.w16(it + 32, [0, 128, pick(rng, 129), pick(rng, 200).wrapping_sub(30)][pick(rng, 4) as usize] as u16);
            m.w16(it + 34, [0, 30, -30i32 as u32, pick(rng, 80).wrapping_sub(40)][pick(rng, 4) as usize] as u16);
            m.w32(it + 36, pick(rng, 2));
            m.w32(it + 40, pick(rng, 70) - 5);
            m.w16(it + 44, pick(rng, 129) as u16);
        }
        for (k, vt) in SLIDER_VTS.into_iter().enumerate() {
            let it = slider_of(k as u32);
            m.w32(it, (k != 0 && pick(rng, 4) == 0) as u32);
            m.w32(it + 4, text(rng));
            m.w16(it + 8, pick(rng, 140) as u16);
            m.w32(it + ITEM_VPTR, vt);
            m.w32(it + 28, base::iterator(rng));
            for (f, v) in [pick(rng, 300), pick(rng, 240), 20 + pick(rng, 200), 4 + pick(rng, 20)].into_iter().enumerate() {
                m.w16(it + 32 + 2 * f as u32, v as u16);
            }
            m.w16(it + 40, [0, 1, 2, 3, pick(rng, 6) - 1][pick(rng, 5) as usize] as u16);
            m.w32(it + 44, pick(rng, 2));
        }
        // the pads (a NeGcon, analog ones, a digital one; the sticks anywhere) and their
        // calibration
        let app = m.r32(FE_APP);
        m.w8(app + 557, pick(rng, 2) as u8);
        for pad in 0..2 {
            let p = PAD_INFO + 32 * pad;
            m.w8(p + 5, [PAD_NEGCON, PAD_ANALOG, PAD_DUAL_SHOCK, 0x41][pick(rng, 4) as usize] as u8);
            for k in 8..12 {
                m.w8(p + k, [0, 128, 255, pick(rng, 256)][pick(rng, 4) as usize] as u8);
            }
        }
        for k in 841..857 {
            m.w8(FRONT_END + k, pick(rng, 256) as u8);
        }
        let it = audio();
        m.w32(it, (pick(rng, 4) == 0) as u32);
        m.w32(it + 4, text(rng));
        m.w16(it + 8, pick(rng, 140) as u16);
        m.w32(it + ITEM_VPTR, AUDIO_SLIDER_VT);
        // (a range to slide over: `Percentage` divides by it)
        let data = base::iterator(rng);
        m.w32(it + 28, data);
        let min = m.r8(data + 8);
        m.w8(data + 9, min + 1 + pick(rng, 9) as u8);
        for (f, v) in [pick(rng, 300), pick(rng, 240), 20 + pick(rng, 200), 4 + pick(rng, 20)].into_iter().enumerate() {
            m.w16(it + 32 + 2 * f as u32, v as u16);
        }
        m.w16(it + 40, [0, 128, pick(rng, 129)][pick(rng, 3) as usize] as u16);
        m.w16(it + 42, [0, 60, -60i32 as u32, pick(rng, 100) - 50][pick(rng, 4) as usize] as u16);
        m.w32(it + 44, pick(rng, 2));
        m.w16(it + 48, pick(rng, 10) as u16);
        m.w32(it + 52, [0, 30, pick(rng, 70)][pick(rng, 3) as usize]);
        // the user name's item: a grid of letters, digits, accented letters, empty cells and
        // the end, delete and space keys; a name of up to its longest
        let it = user_name();
        m.w32(it, (pick(rng, 4) == 0) as u32);
        m.w32(it + 4, text(rng));
        m.w16(it + 8, pick(rng, 140) as u16);
        m.w32(it + ITEM_VPTR, USER_NAME_VT);
        m.w32(it + 28, NAME);
        let longest = 1 + pick(rng, 8);
        m.w16(it + 32, longest as u16);
        let rows = 1 + pick(rng, 9);
        m.w16(NAME_ROWS, rows as u16);
        m.w16(NAME_GRID_TOP, pick(rng, 100) as u16);
        let cells: &[u8] = b"ABCZaz09-!@#$&^";
        for row in 0..10 {
            let n = 1 + pick(rng, 6);
            for col in 0..6 {
                let ch = if col >= n {
                    0
                } else if pick(rng, 8) == 0 {
                    225 + pick(rng, 28) as u8
                } else {
                    cells[pick(rng, cells.len() as u32) as usize]
                };
                m.w8(cell(it, row, col), ch);
            }
        }
        // (at least one cell not empty in the current row: the cursor skips empty ones round it)
        let (row, col) = (pick(rng, rows), pick(rng, 6));
        m.w16(it + 36, row as u16);
        m.w16(it + 34, col as u16);
        m.w8(cell(it, row, col), b'A' + pick(rng, 26) as u8);
        let len = pick(rng, longest + 1);
        for k in 0..len {
            m.w8(NAME + k, b'A' + pick(rng, 26) as u8);
        }
        m.w8(NAME + len, 0);
        m.w16(it + 128, pick(rng, 2) as u16);
        m.w16(it + 130, [0, 128, pick(rng, 129)][pick(rng, 3) as usize] as u16);
        m.w16(it + 132, [0, 30, -30i32 as u32][pick(rng, 3) as usize] as u16);
        m.w32(it + 136, pick(rng, 2));
        m.w8(FRONT_END + 0x340, pick(rng, 6) as u8);
        m.w16(CARD_BUSY, pick(rng, 2) as u16);
        m.w16(item_of(1) + 40, pick(rng, 257) as u16);
        for (k, vt) in SLIDING_VTS.into_iter().enumerate() {
            let it = sliding_of(k as u32);
            m.w32(it, (pick(rng, 4) == 0) as u32 | 0x80);
            m.w32(it + 4, text(rng));
            m.w16(it + 8, pick(rng, 140) as u16);
            m.w32(it + ITEM_VPTR, vt);
            m.w32(it + 28, [0, INSIDE, INSIDE2][pick(rng, 3) as usize]);
            m.w32(it + 32, [0, INSIDE, INSIDE2][pick(rng, 3) as usize]);
            let h = 20 + pick(rng, 130);
            m.w16(it + 36, (100 + pick(rng, 220)) as u16);
            m.w16(it + 38, h as u16);
            m.w16(it + 40, [0, h, pick(rng, h + 8)][pick(rng, 3) as usize] as u16);
            m.w16(it + 42, [0, h + h / 2, pick(rng, 2 * h)][pick(rng, 3) as usize] as u16);
            m.w16(it + 44, [0, 128, pick(rng, 129)][pick(rng, 3) as usize] as u16);
            m.w16(it + 46, [0, 30, -30i32 as u32][pick(rng, 3) as usize] as u16);
            for f in [48, 52, 56, 64, 68] {
                m.w32(it + f, pick(rng, 2));
            }
            m.w16(it + 60, pick(rng, 60) as u16);
            m.w16(it + 62, pick(rng, 60) as u16);
        }
        for (menu, vt) in [(INSIDE, INSIDE_BOX_MENU_VT), (INSIDE2, INSIDE_BOX_MENU_VT)] {
            m.w32(menu, [0, 1, 2, 4][pick(rng, 4) as usize]);
            m.w16(menu + 4, text(rng) as u16);
            let n = 1 + pick(rng, 6);
            m.w32(menu + 8, pick(rng, n));
            m.w32(menu + 12, 0);
            // (one item always enabled: with none, tMenu's input looks for one for ever)
            for i in 0..n {
                m.w32(menu + 16 + 4 * i, slider_of(pick(rng, SLIDER_VTS.len() as u32)));
            }
            let k = pick(rng, n);
            m.w32(menu + 16 + 4 * k, slider_of(0));
            m.w32(menu + 16 + 4 * n, 0);
            m.w32(menu + 84, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(menu + 96, [0, NOTHING][pick(rng, 2) as usize]);
            m.w32(menu + MENU_VPTR, vt);
            m.w16(menu + 108, pick(rng, n + 1) as u16);
            m.w16(menu + 110, [0, 24, -24i32 as u32, pick(rng, 50) - 25][pick(rng, 4) as usize] as u16);
            m.w16(menu + 112, [0, 4, -4i32 as u32][pick(rng, 3) as usize] as u16);
        }
        // the songs' menu, a song list of a few (or none) and which play
        m.w32(SONGS, 0);
        m.w32(SONGS + 8, 0);
        m.w32(SONGS + 16, slider_of(0));
        m.w32(SONGS + 20, 0);
        m.w32(SONGS + MENU_VPTR, SONG_MENU_VT);
        m.w16(SONGS + 110, [0, 21, -21i32 as u32, pick(rng, 50) - 25][pick(rng, 4) as usize] as u16);
        m.w16(SONGS + 112, [0, 4, -4i32 as u32][pick(rng, 3) as usize] as u16);
        for k in 0..10 {
            m.w16(SONGS + 116 + 2 * k, pick(rng, 140) as u16);
        }
        m.w32(MUSIC_STATE, MUSIC);
        let n = 1 + pick(rng, 9);
        m.w32(MUSIC + 120, if pick(rng, 6) == 0 { 0 } else { SONG_LIST });
        m.w16(MUSIC + 116, pick(rng, n) as u16);
        m.w32(SONG_LIST, n);
        for k in 0..n {
            for (j, b) in format!("SONG {k} LINE").bytes().chain([0]).enumerate() {
                m.w8(SONG_TEXT + 32 * k + j as u32, b);
            }
            m.w32(SONG_LIST + 12 + 64 * k, SONG_TEXT + 32 * k);
            m.w32(SONG_LIST + 16 + 64 * k, SONG_TEXT + 32 * k + 5);
            m.w32(FRONT_END + 924 + 4 * k, pick(rng, 2));
        }
        // (now and then the songs' menu is the first box's)
        if pick(rng, 3) == 0 {
            m.w32(SONGS_SCREEN, INSIDE.wrapping_sub(10072));
        }
        m.w32(BOX_HELP_TEXT, [u32::MAX, text(rng)][pick(rng, 2) as usize]);
        let menu = OPTIONS;
        m.w32(menu, [0, 1, 2, 4][pick(rng, 4) as usize]);
        m.w16(menu + 4, pick(rng, 300) as u16);
        let n = pick(rng, 5);
        m.w32(menu + 8, pick(rng, n + 1));
        m.w32(menu + 12, (pick(rng, 6) == 0) as u32);
        for i in 0..n {
            m.w32(menu + 16 + 4 * i, item_of(pick(rng, ITEM_VTS.len() as u32)));
        }
        m.w32(menu + 16 + 4 * n, 0);
        m.w32(menu + 84, [0, MENU2][pick(rng, 2) as usize]);
        m.w32(menu + 92, [0, MENU2][pick(rng, 2) as usize]);
        m.w32(menu + 96, [0, NOTHING][pick(rng, 2) as usize]);
        m.w32(menu + MENU_VPTR, OPTIONS_MENU_VT);
        m.w32(menu + 108, pick(rng, 2));
        m.w8(menu + 112, [40, -40i32 as u32, pick(rng, 256)][pick(rng, 3) as usize] as u8);
        // (513 meets 552 a step of 40 past: the fade's clamp)
        m.w32(menu + 116, [0, SCREEN_FADE_FULL as u32, 513, pick(rng, 600) - 20][pick(rng, 4) as usize]);
        m.w32(menu + 120, [u32::MAX, pick(rng, 40)][pick(rng, 2) as usize]);
        m.w32(menu + 124, pick(rng, 6));
        m.w32(PHASE, [0, 30, 60, pick(rng, 80) - 10][pick(rng, 4) as usize]);
        // (a clock the game reaches: a negative one would pick a picture before the first)
        let ticks = m.r32(GAME_TICKS).wrapping_add(rng.next() as u32 & 0x00FF_FFFF);
        m.w32(GAME_TICKS, ticks);
    }

    fn sliding(rng: &mut Rng) -> u32 {
        sliding_of(pick(rng, SLIDING_VTS.len() as u32))
    }

    fn fade_item(rng: &mut Rng) -> u32 {
        item_of(pick(rng, ITEM_VTS.len() as u32))
    }

    fn cases() -> Vec<Case> {
        vec![
            ("CalcPulsateYellow", 0x8001_c0a8, nfs4rc::f_8001c0a8, |_, _| vec![]),
            ("DrawLeftFlare", 0x8001_c100, nfs4rc::f_8001c100, |_, r| {
                let sel = if pick(r, 2) == 0 { 128 } else { pick(r, 129) };
                vec![pick(r, 240), sel, pick(r, 129), PHASE]
            }),
            ("ButtonFade::TransitionOff", 0x8001_c3bc, nfs4rc::f_8001c3bc, |_, r| vec![fade_item(r)]),
            ("ButtonFade::TransitionOn", 0x8001_c3c8, nfs4rc::f_8001c3c8, |_, r| vec![fade_item(r)]),
            ("ButtonFade::TransitionIsFinished", 0x8001_c3e4, nfs4rc::f_8001c3e4, |_, r| vec![fade_item(r)]),
            ("ButtonFade::UpdateTransition", 0x8001_c43c, nfs4rc::f_8001c43c, |_, r| vec![fade_item(r), pick(r, 2)]),
            ("LeftRightFade", 0x8001_c4b8, nfs4rc::f_8001c4b8, |_, r| vec![NEW, text(r), base::iterator(r)]),
            ("LeftRightFade::TransitionOff", 0x8001_c4f4, nfs4rc::f_8001c4f4, |_, r| vec![fade_item(r)]),
            ("LeftRightFade::TransitionOn", 0x8001_c50c, nfs4rc::f_8001c50c, |_, r| vec![fade_item(r)]),
            ("LeftRightFade::TransitionIsFinished", 0x8001_c530, nfs4rc::f_8001c530, |_, r| vec![fade_item(r)]),
            ("LeftRightFade::UpdateTransition", 0x8001_c598, nfs4rc::f_8001c598, |_, r| vec![fade_item(r), pick(r, 2)]),
            ("tOptionsMenu", 0x8001_c614, nfs4rc::f_8001c614, |c, r| {
                for k in 0..4 {
                    let v = if k == 3 || pick(r, 3) == 0 { 0 } else { fade_item(r) };
                    c.m.w32(NEW + 0x200 + 4 * k, v);
                }
                let mut a = vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 70000), pick(r, 40), pick(r, 6), fade_item(r)];
                a.extend((0..4).map(|k| c.m.r32(NEW + 0x200 + 4 * k)));
                a
            }),
            ("~tOptionsMenu", 0x8001_c6a4, nfs4rc::f_8001c6a4, |_, r| vec![OPTIONS, 2 * pick(r, 2)]),
            ("tOptionsMenu::DebounceKeys", 0x8001_c6cc, nfs4rc::f_8001c6cc, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::TransitionOff", 0x8001_c738, nfs4rc::f_8001c738, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::TransitionOn", 0x8001_c7d0, nfs4rc::f_8001c7d0, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::TransitionIsFinished", 0x8001_c86c, nfs4rc::f_8001c86c, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::UpdateTransition", 0x8001_c9b4, nfs4rc::f_8001c9b4, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::Draw", 0x8001_cc64, nfs4rc::f_8001cc64, |_, _| vec![OPTIONS]),
            ("tOptionsMenu::ProcessInput", 0x8001_ce68, nfs4rc::f_8001ce68, |_, r| vec![OPTIONS, player(r), KEY, COMMAND]),
            ("tInsideBoxMenu", 0x8001_cef0, nfs4rc::f_8001cef0, |c, r| {
                for k in 0..4 {
                    let v = if k == 3 || pick(r, 3) == 0 { 0 } else { slider_of(pick(r, 3)) };
                    c.m.w32(NEW + 0x200 + 4 * k, v);
                }
                let mut a = vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 70000), slider_of(pick(r, 3))];
                a.extend((0..4).map(|k| c.m.r32(NEW + 0x200 + 4 * k)));
                a
            }),
            ("~tInsideBoxMenu", 0x8001_cf64, nfs4rc::f_8001cf64, |_, r| vec![INSIDE, 2 * pick(r, 2)]),
            ("tInsideBoxMenu::ProcessInput", 0x8001_cf8c, nfs4rc::f_8001cf8c, |_, r| vec![[INSIDE, INSIDE2][pick(r, 2) as usize], player(r), KEY, COMMAND]),
            ("tInsideBoxMenu::Draw", 0x8001_d05c, nfs4rc::f_8001d05c, |_, r| vec![[INSIDE, INSIDE2][pick(r, 2) as usize], pick(r, 300), pick(r, 240), 100 + pick(r, 200), pick(r, 100)]),
            ("SlidingMenu", 0x8001_d2c0, nfs4rc::f_8001d2c0, |_, r| vec![NEW, text(r), 100 + pick(r, 200), 20 + pick(r, 130), pick(r, 60), pick(r, 60), pick(r, 2)]),
            ("~SlidingMenu", 0x8001_d368, nfs4rc::f_8001d368, |_, r| vec![sliding(r), 2 * pick(r, 2)]),
            ("SlidingMenu::TransitionOff", 0x8001_d390, nfs4rc::f_8001d390, |_, r| vec![sliding(r)]),
            ("SlidingMenu::TransitionOn", 0x8001_d3bc, nfs4rc::f_8001d3bc, |_, r| vec![sliding(r)]),
            ("SlidingMenu::TransitionIsFinished", 0x8001_d3f4, nfs4rc::f_8001d3f4, |_, r| vec![sliding(r)]),
            ("SlidingMenu::UpdateTransition", 0x8001_d47c, nfs4rc::f_8001d47c, |_, r| vec![sliding_of(0), pick(r, 2)]),
            ("SlidingMenu::DebounceKeys", 0x8001_d4f8, nfs4rc::f_8001d4f8, |_, r| vec![sliding(r), player(r)]),
            ("SlidingMenu::UpdatefOpenHeight", 0x8001_d57c, nfs4rc::f_8001d57c, |_, r| vec![sliding_of(0), pick(r, 2)]),
            ("SlidingMenu::Draw", 0x8001_d79c, nfs4rc::f_8001d79c, |_, r| vec![sliding(r), pick(r, 100), pick(r, 100), pick(r, 2)]),
            ("SlidingMenu::Draw(bool)", 0x8001_d540, nfs4rc::f_8001d540, |_, r| vec![sliding(r), pick(r, 2)]),
            ("SlidingMenu::ProcessInput", 0x8001_dde0, nfs4rc::f_8001dde0, |_, r| vec![sliding_of(0), player(r), KEY, COMMAND]),
            ("SlidingMenu::SetMenu", 0x8001_de88, nfs4rc::f_8001de88, |_, r| vec![sliding(r), pick(r, 2), [0, INSIDE, INSIDE2][pick(r, 3) as usize]]),
            ("SlidingActivated::UpdatefOpenHeight", 0x8001_decc, nfs4rc::f_8001decc, |_, r| vec![sliding_of(1), pick(r, 2)]),
            ("SlidingActivated::TransitionOn", 0x8001_df60, nfs4rc::f_8001df60, |_, _| vec![sliding_of(1)]),
            ("SlidingActivated::UpdateTransition", 0x8001_e028, nfs4rc::f_8001e028, |_, r| vec![sliding_of(1), pick(r, 2)]),
            ("MyLeftRightDraw", 0x8001_e29c, nfs4rc::f_8001e29c, |_, r| vec![fade_item(r), pick(r, 300), pick(r, 240)]),
            ("DisplayChoice::Draw", 0x8001_e390, nfs4rc::f_8001e390, |_, r| vec![item_of(2), pick(r, 100), pick(r, 100), pick(r, 2)]),
            ("OnOff::TransitionOn", 0x8001_e480, nfs4rc::f_8001e480, |_, _| vec![item_of(3)]),
            ("OnOff::Draw", 0x8001_e4e4, nfs4rc::f_8001e4e4, |_, r| vec![item_of(3), pick(r, 100), pick(r, 100), pick(r, 2)]),
            ("AudioSlider", 0x8001_e65c, nfs4rc::f_8001e65c, |_, r| vec![NEW, text(r), base::iterator(r), pick(r, 10)]),
            ("~AudioSlider", 0x8001_e6a8, nfs4rc::f_8001e6a8, |_, r| vec![audio(), 2 * pick(r, 2)]),
            ("AudioSlider::Draw", 0x8001_e6d0, nfs4rc::f_8001e6d0, |_, r| vec![audio(), pick(r, 100), pick(r, 100), pick(r, 2)]),
            ("AudioSlider::Percentage", 0x8001_e8dc, nfs4rc::f_8001e8dc, |_, _| vec![audio()]),
            ("AudioSlider::TransitionOff", 0x8001_e9b0, nfs4rc::f_8001e9b0, |_, _| vec![audio()]),
            ("AudioSlider::TransitionOn", 0x8001_e9c0, nfs4rc::f_8001e9c0, |_, _| vec![audio()]),
            ("AudioSlider::TransitionIsFinished", 0x8001_e9d8, nfs4rc::f_8001e9d8, |_, _| vec![audio()]),
            ("AudioSlider::UpdateTransition", 0x8001_ea34, nfs4rc::f_8001ea34, |_, r| vec![audio(), pick(r, 2)]),
            ("tInsideBoxSongMenu", 0x8001_ea80, nfs4rc::f_8001ea80, |c, r| {
                for k in 0..4 {
                    let v = if k == 3 || pick(r, 3) == 0 { 0 } else { slider_of(pick(r, 3)) };
                    c.m.w32(NEW + 0x200 + 4 * k, v);
                }
                let mut a = vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 70000), slider_of(pick(r, 3))];
                a.extend((0..4).map(|k| c.m.r32(NEW + 0x200 + 4 * k)));
                a
            }),
            ("~tInsideBoxSongMenu", 0x8001_eb10, nfs4rc::f_8001eb10, |_, r| vec![SONGS, 2 * pick(r, 2)]),
            ("tInsideBoxSongMenu::Draw", 0x8001_eb38, nfs4rc::f_8001eb38, |_, r| vec![SONGS, pick(r, 300), pick(r, 240), 100 + pick(r, 200), pick(r, 100), 20 + pick(r, 150)]),
            ("tInsideBoxSongMenu::DrawOneSong", 0x8001_eda8, nfs4rc::f_8001eda8, |c, r| {
                let n = c.m.r32(SONG_LIST);
                vec![SONGS, pick(r, n), pick(r, 300), pick(r, 240), 100 + pick(r, 200), pick(r, 129), pick(r, 129)]
            }),
            ("tInsideBoxSongMenu::DebounceKeys", 0x8001_efd4, nfs4rc::f_8001efd4, |_, _| vec![SONGS]),
            ("tInsideBoxSongMenu::ProcessInput", 0x8001_efdc, nfs4rc::f_8001efdc, |_, r| vec![SONGS, player(r), KEY, COMMAND]),
            ("ControllerChoice::Draw", 0x8001_f1e4, nfs4rc::f_8001f1e4, |_, r| vec![item_of(4), pick(r, 100), pick(r, 100), pick(r, 2)]),
            ("InsideSlider", 0x8001_f3e8, nfs4rc::f_8001f3e8, |_, r| vec![NEW, text(r), base::iterator(r)]),
            ("~InsideSlider", 0x8001_f428, nfs4rc::f_8001f428, |_, r| vec![slider_of(0), 2 * pick(r, 2)]),
            ("InsideSlider::Draw", 0x8001_f450, nfs4rc::f_8001f450, |_, r| vec![slider_of(0), pick(r, 300), pick(r, 240), 40 + pick(r, 200), pick(r, 2)]),
            ("TwoWay", 0x8001_f5d4, nfs4rc::f_8001f5d4, |_, r| vec![NEW, text(r), base::iterator(r), pick(r, 4)]),
            ("~TwoWay", 0x8001_f628, nfs4rc::f_8001f628, |_, r| vec![slider_of(1), 2 * pick(r, 2)]),
            ("TwoWay::ProcessInput", 0x8001_f650, nfs4rc::f_8001f650, |c, r| {
                c.m.w32(KEY, [femenu::KEY_ACCEPT, KEY_STOP, femenu::KEY_LEFT, femenu::KEY_RIGHT, femenu::KEY_UP, femenu::KEY_NONE][pick(r, 6) as usize]);
                vec![slider_of(1), player(r), KEY, COMMAND]
            }),
            ("TwoWay::Draw", 0x8001_f700, nfs4rc::f_8001f700, |_, r| vec![slider_of(1), pick(r, 300), pick(r, 240), 40 + pick(r, 200), pick(r, 2)]),
            ("GetCurrentStickRange", 0x8001_f9c0, nfs4rc::f_8001f9c0, |_, r| vec![pick(r, 2)]),
            ("GetCurrentStickRange2", 0x8001_fa1c, nfs4rc::f_8001fa1c, |_, r| vec![pick(r, 2)]),
            ("Calibrate", 0x8001_fa78, nfs4rc::f_8001fa78, |_, _| vec![slider_of(1)]),
            ("UserName", 0x8001_fd98, nfs4rc::f_8001fd98, |_, r| vec![NEW, text(r)]),
            ("CheckForCheats", 0x8001_fe0c, nfs4rc::f_8001fe0c, |_, _| vec![NAME]),
            ("UserName::ProcessInput", 0x8001_fe8c, nfs4rc::f_8001fe8c, |c, r| {
                c.m.w32(KEY, [femenu::KEY_ACCEPT, femenu::KEY_UP, femenu::KEY_DOWN, femenu::KEY_LEFT, femenu::KEY_RIGHT, femenu::KEY_NONE][pick(r, 6) as usize]);
                vec![user_name(), player(r), KEY, COMMAND]
            }),
            ("SpecialCharacter", 0x8002_024c, nfs4rc::f_8002024c, |_, r| vec![[200 + pick(r, 60), pick(r, 256), 0x100 + pick(r, 0x100)][pick(r, 3) as usize]]),
            ("UserName::Draw", 0x8002_02dc, nfs4rc::f_800202dc, |_, r| vec![user_name(), pick(r, 2)]),
            ("UserName::TransitionOff", 0x8002_06d4, nfs4rc::f_800206d4, |_, _| vec![user_name()]),
            ("UserName::TransitionOn", 0x8002_06e0, nfs4rc::f_800206e0, |_, _| vec![user_name()]),
            ("UserName::TransitionIsFinished", 0x8002_0914, nfs4rc::f_80020914, |_, _| vec![user_name()]),
            ("UserName::UpdateTransition", 0x8002_0980, nfs4rc::f_80020980, |_, r| vec![user_name(), pick(r, 2)]),
            ("MemoryCard::Draw", 0x8002_09fc, nfs4rc::f_800209fc, |_, r| vec![item_of(1), pick(r, 2)]),
            ("ControllerSlider::ProcessInput", 0x8002_0c54, nfs4rc::f_80020c54, |c, r| {
                c.m.w32(KEY, [femenu::KEY_LEFT, femenu::KEY_RIGHT, femenu::KEY_UP, femenu::KEY_ACCEPT][pick(r, 4) as usize]);
                vec![slider_of(2), player(r), KEY, COMMAND]
            }),
            ("~ControllerSlider", 0x8002_0d18, nfs4rc::f_80020d18, |_, r| vec![slider_of(2), 2 * pick(r, 2)]),
            ("~MemoryCard", 0x8002_0d38, nfs4rc::f_80020d38, |_, r| vec![item_of(1), 2 * pick(r, 2)]),
            ("~ButtonFade", 0x8002_0d58, nfs4rc::f_80020d58, |_, r| vec![item_of(1), 2 * pick(r, 2)]),
            ("~UserName", 0x8002_0d78, nfs4rc::f_80020d78, |_, r| vec![user_name(), 2 * pick(r, 2)]),
            ("~ControllerChoice", 0x8002_0d98, nfs4rc::f_80020d98, |_, r| vec![item_of(4), 2 * pick(r, 2)]),
            ("~OnOff", 0x8002_0db8, nfs4rc::f_80020db8, |_, r| vec![item_of(3), 2 * pick(r, 2)]),
            ("~DisplayChoice", 0x8002_0dd8, nfs4rc::f_80020dd8, |_, r| vec![item_of(2), 2 * pick(r, 2)]),
            ("~SlidingActivated", 0x8002_0df8, nfs4rc::f_80020df8, |_, r| vec![sliding_of(1), 2 * pick(r, 2)]),
            ("~LeftRightFade", 0x8002_0e18, nfs4rc::f_80020e18, |_, r| vec![item_of(0), 2 * pick(r, 2)]),
            ("SlidingActivated::ProcessInput", 0x8001_e0a4, nfs4rc::f_8001e0a4, |c, r| {
                // (not the keys that switch the music: its library waits on the sound hardware)
                let active = c.m.r32(sliding_of(1) + 68) != 0;
                let k = c.m.r32(KEY);
                if (k == femenu::KEY_ACCEPT && !active) || (k == femenu::KEY_BACK && active) {
                    c.m.w32(KEY, femenu::KEY_UP);
                }
                vec![sliding_of(1), player(r), KEY, COMMAND]
            }),
        ]
    }

    #[test]
    fn femenuoptions_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x0F70 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x0F71 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
