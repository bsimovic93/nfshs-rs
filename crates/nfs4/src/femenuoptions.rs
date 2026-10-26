//! FEMENUOPTIONS.CPP: the options screens' menus and items: the pulsing yellow and the flare
//! left of the current item, items that fade in and out (buttons, left-right choices: the
//! display's, the controller's, on-off ones; audio sliders), the options menu (its items fading
//! in one after the other and out the other way, the screen fading, its animated picture),
//! the sliding boxes that open into an inside menu (the songs', sliders, the analog sticks'
//! calibration), the user name's letter grid and the memory card's button.
//!
//! Like FEMENU.CPP's, the classes call each other through their vtables (`method`), the
//! registers each call gets as the original has them at its `jal` or `jalr`.

use crate::abi::{self, Saved};
use crate::femenu::{self, ITEM_DISABLED, ITEM_VPTR, method};
use crate::femenuextended::{
    self, CALC_ON_OFF_FADE, CALC_TEXT_FADE_SEL_TO_HI, DRAW_SHAPE_EXTENDED, DRAW_SQUARE, GET_DRAWENV, ITEM_DRAW_AT, ITEM_TRANSITION_IS_FINISHED, ITEM_TRANSITION_OFF, ITEM_TRANSITION_ON, ITEM_UPDATE_TRANSITION, MENU_TRANSITION_IS_FINISHED, MENU_VPTR,
    SET_DRAW_AREA, SPRINTF, STRLEN, TEXT_WORD,
};
use crate::fetextrender;
use crate::globals::{
    BOX_HELP_TEXT, FE_APP, FLIP, FRONT_END, GAME_TICKS, MUSIC_STATE, PAD_INFO, PLAYER1_VIEW, PULSATE_YELLOW, SCREEN_HEIGHT, SHAPES, SLIDING_FLARE_PHASE, SONGS_SCREEN, SUB_MENU_PLAYER, TEXT_COLORS,
    TEXT_STYLES,
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
/// `tMenuItem::ProcessInput` (nothing); `SubtractiveBox` (the host captures its primitives:
/// called by address).
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
const PLAY_SPEECH: fn(&mut Cpu) = nfs4rc::f_80015de0;
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
const MENU_ITEM_PROCESS_INPUT: fn(&mut Cpu) = nfs4rc::f_800247c0;
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
const SCREEN_FADE_FULL: u32 = 552;
/// A screen's vptr and its `TransitionIsFinished` (the front end's current screen, at
/// `gFEApp + 12`).
const SCREEN_VPTR: u32 = 96;
const SCREEN_TRANSITION_IS_FINISHED: u32 = 64;
/// The items' `DebounceKeys`, `ProcessInput`, `Draw(int, int, bool)` and `Draw(int, int, int,
/// bool)` (their vtable's); the menus' `DebounceKeys`.
const ITEM_DEBOUNCE_KEYS: u32 = 16;
const ITEM_PROCESS_INPUT: u32 = 24;
const ITEM_DRAW_AT3: u32 = 48;
const MENU_DEBOUNCE_KEYS: u32 = 32;
/// A sliding box's `UpdatefOpenHeight`; the menus' `ProcessInput` and the inside menus'
/// `Draw(short, short, short, short, short)`.
const SLIDING_UPDATE_OPEN_HEIGHT: u32 = 88;
const MENU_PROCESS_INPUT: u32 = 24;
const MENU_DRAW_BOX: u32 = 88;
const MENU_DRAW_ONE_SONG: u32 = 96;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// A menu's `k`th item (`k` taken as a short, as the original's loops do).
fn item(m: &mut Mem, this: u32, k: u32) -> u32 {
    m.r32(this.wrapping_add(16).wrapping_add(sx(k) << 2))
}

/// `CalcPulsateYellow` (FEMENUOPTIONS.CPP, 0x8001c0a8): the yellow pulsing with the clock
/// (up for 64 ticks of 128, down for the rest).
pub fn calc_pulsate_yellow(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let t = c.m.r32(GAME_TICKS) as i32;
        let mut f = t % 128;
        if f >= 65 {
            f = 128 - f;
        }
        let v = Saved::of(c).call_leaving(c, CALC_FADE_VAL, &[0xBEBE, f as u32], 0x8001_c0ec);
        c.m.w32(PULSATE_YELLOW, v);
    })
}

pub fn abi_calc_pulsate_yellow(c: &mut Cpu) {
    calc_pulsate_yellow(c);
}

/// `DrawLeftFlare` (FEMENUOPTIONS.CPP, 0x8001c100): the flare left of the current item at `y`:
/// its phase (`*phase`, 0..60) counting on while the item is fully selected (`sel` 128) and
/// winding back out after; a halo growing and shrinking with it and a shape, both by the
/// selection and the item's `fade`.
pub fn draw_left_flare(c: &mut Cpu, y: u32, sel: u32, fade: u32, phase: u32) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[4] = y;
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
        let size = (half + 20).wrapping_mul(sel as i32);
        let scaled = size.wrapping_mul(128i32.wrapping_sub(fade as i32));
        let size = scaled / 128;
        let shade = (128i32.wrapping_sub(sel as i32)).max(fade as i32);
        // (s2 keeps the product rounded toward zero, as the original's shift leaves it)
        r.0[1] = size as u32;
        r.0[2] = if scaled < 0 { scaled.wrapping_add(127) } else { scaled } as u32;
        r.0[3] = shade as u32;
        if size <= 0 {
            return;
        }
        let x = r.call_leaving(c, WORD_X, &[FLARE_TEXT], 0x8001_c1d8);
        r.0[0] = x;
        // (a third of twice the size by the original's multiply)
        let twice = size.wrapping_mul(2);
        let third = ((twice as i64 * 0x5555_5556) >> 32) as i32 - (twice >> 31);
        let args = [x, y.wrapping_add(5), (size / 2) as u32, third as u32, 23];
        r.call_ptr_leaving(c, FLARE_2D_HALO, &args, 0x8001_c218);
        let args = [0, 0, x.wrapping_sub(3), y.wrapping_sub(1), shade as u32, (shade != 0) as u32, 0];
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &args, 0x8001_c23c);
    })
}

pub fn abi_draw_left_flare(c: &mut Cpu) {
    draw_left_flare(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

// The fading items: +32 the fade, +34 its step, +36 whether it is still fading (+40: a
// button's enabled fade, a left-right choice's flare phase); a sliding menu's at +44.
const FADE: u32 = 32;
const SLIDING_FADE: u32 = 44;
const AUDIO_FADE: u32 = 40;
const USER_NAME_FADE: u32 = 130;

/// Whether a fading item's fade (at `at`: then its step, the flag) has reached its end (the
/// flag left set while it hasn't).
fn fade_is_finished(m: &mut Mem, this: u32, at: u32) -> u32 {
    let (fade, step) = (m.r16(this + at) as i16, m.r16(this + at + 2) as i16);
    let fading = (step < 0 && fade > 0) || (step > 0 && fade < 128);
    m.w32(this + at + 4, fading as u32);
    m.r32(this + at + 4) ^ 1
}

/// A fading item's `UpdateTransition`: the fade (at `at`) stepped within 0..128, its
/// `TransitionIsFinished` (which notes whether it still fades), `tMenuItem`'s; `ras` the two
/// calls' returns.
fn fade_update_transition(c: &mut Cpu, this: u32, at: u32, selected: u32, ras: [u32; 2]) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = selected;
        let m = &mut c.m;
        let v = (m.r16(this + at) as i16 as i32 + m.r16(this + at + 2) as i16 as i32).clamp(0, 128);
        m.w16(this + at, v as u16);
        let (obj, f) = method(m, this, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED);
        r.call_ptr_leaving(c, f, &[obj], ras[0]);
        r.enter(c, 0x8002_47c8, ras[1], |c| femenu::menu_item_update_transition(c, this, selected));
    })
}

/// `tMenuItemGoToMenuButtonFade::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c3bc): fading out.
pub fn abi_button_fade_transition_off(c: &mut Cpu) {
    c.m.w16(c.r[4] + 34, 30);
}

/// `tMenuItemGoToMenuButtonFade::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c3c8): fading in from
/// 128, unselected, the enabled fade from 0.
pub fn abi_button_fade_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w16(this + 32, 128);
    m.w16(this + 34, (-30i16) as u16);
    m.w16(this + 8, 0);
    m.w16(this + 40, 0);
}

/// `tMenuItemGoToMenuButtonFade::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c3e4).
pub fn abi_button_fade_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, c.r[4], FADE);
}

/// `tMenuItemGoToMenuButtonFade::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c43c).
pub fn abi_button_fade_update_transition(c: &mut Cpu) {
    fade_update_transition(c, c.r[4], FADE, c.r[5], [0x8001_c498, 0x8001_c4a4]);
}

/// `tMenuItemLeftRightFade::tMenuItemLeftRightFade` (FEMENUOPTIONS.CPP, 0x8001c4b8): a
/// left-right choice that fades, its flare's phase 0.
pub fn left_right_fade(c: &mut Cpu, this: u32, text: u32, data: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter(c, 0x8002_4924, 0x8001_c4cc, |c| femenu::left_right_choice(c, this, text, data));
        let m = &mut c.m;
        m.w32(this + ITEM_VPTR, LEFT_RIGHT_FADE_VT);
        m.w32(this + 40, 0);
        m.w16(this + 8, 0);
        this
    })
}

pub fn abi_left_right_fade(c: &mut Cpu) {
    c.r[2] = left_right_fade(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightFade::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c4f4): fading out from 0.
pub fn abi_left_right_fade_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w32(this + 36, 1);
    m.w16(this + 34, 30);
    m.w16(this + 32, 0);
}

/// `tMenuItemLeftRightFade::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c50c): fading in from 128,
/// unselected, the flare from its start.
pub fn abi_left_right_fade_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w32(this + 36, 1);
    m.w16(this + 34, (-30i16) as u16);
    m.w32(this + 40, 0);
    m.w16(this + 8, 0);
    m.w16(this + 32, 128);
}

/// `tMenuItemLeftRightFade::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c530).
pub fn abi_left_right_fade_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, c.r[4], FADE);
}

/// `tMenuItemLeftRightFade::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c598).
pub fn abi_left_right_fade_update_transition(c: &mut Cpu) {
    fade_update_transition(c, c.r[4], FADE, c.r[5], [0x8001_c5f4, 0x8001_c600]);
}

// tOptionsMenu: +108 in its transition, +112 the transition's direction (a byte: 40 out, -40
// in), +114 the item before, +116 the screen's fade, +120 its picture's first frame, +124 how
// many.

/// `tOptionsMenu::tOptionsMenu` (FEMENUOPTIONS.CPP, 0x8001c614): `tMenu`'s with a picture of
/// `frames` frames from `first_frame`, its items from the caller's list.
pub fn abi_options_menu(c: &mut Cpu) {
    let (this, flags, screen, next) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let args = c.r[29] + 16;
    c.r[2] = abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let (options, on_back, title) = (m.r32(args), m.r32(args + 4), sx(m.r32(args + 8)));
        let (first_frame, frames, first) = (m.r32(args + 12), m.r32(args + 16), m.r32(args + 20));
        r.0[..3].copy_from_slice(&[this, first_frame, frames]);
        m.w32(sp + 16, options);
        m.w32(sp + 20, on_back);
        m.w32(sp + 24, title);
        r.enter(c, 0x8002_548c, 0x8001_c65c, |c| femenu::menu(&mut c.m, this, flags, screen, next, options, on_back, title));
        c.m.w32(this + MENU_VPTR, OPTIONS_MENU_VT);
        r.enter(c, 0x8002_5454, 0x8001_c678, |c| femenu::menu_constructor(&mut c.m, this, first, args + 24));
        let m = &mut c.m;
        m.w32(this + 116, 0);
        m.w16(this + 114, 0);
        m.w32(this + 120, first_frame);
        m.w32(this + 124, frames);
        this
    });
}

/// `tOptionsMenu::~tOptionsMenu` (FEMENUOPTIONS.CPP, 0x8001c6a4).
pub fn abi_options_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((MENU_VPTR, OPTIONS_MENU_VT)), 0x8002_54d0, 0x8001_c6bc, femenu::abi_menu_dtor);
}

/// `tOptionsMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001c6cc): the current item's (none when
/// it is missing or disabled).
pub fn options_menu_debounce_keys(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let cur = m.r32(this + 8);
        let it = m.r32(this.wrapping_add(cur << 2).wrapping_add(16));
        if it == 0 || m.r32(it) & ITEM_DISABLED != 0 {
            return 0;
        }
        let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DEBOUNCE_KEYS);
        Saved::of(c).call_ptr_leaving(c, f, &[obj], 0x8001_c728)
    })
}

pub fn abi_options_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = options_menu_debounce_keys(c, c.r[4]);
}

/// Each of a menu's items' virtual `slot` in order, `r.0[k_reg]` the index at each call (and
/// one past the last after); `ra` the calls' return.
fn each_item(c: &mut Cpu, r: &mut Saved, this: u32, slot: u32, ra: u32, k_reg: usize) {
    let mut k = 0u32;
    r.0[k_reg] = 0;
    if item(&mut c.m, this, 0) == 0 {
        return;
    }
    loop {
        let it = item(&mut c.m, this, k);
        let (obj, f) = method(&mut c.m, it, ITEM_VPTR, slot);
        r.call_ptr_leaving(c, f, &[obj], ra);
        k = k.wrapping_add(1);
        r.0[k_reg] = k;
        if item(&mut c.m, this, k) == 0 {
            return;
        }
    }
}

/// `tOptionsMenu::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001c738): the items fading out, the
/// screen's fade going up from 0.
pub fn abi_options_menu_transition_off(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        m.w8(this + 112, 40);
        m.w32(this + 116, 0);
        each_item(c, &mut r, this, ITEM_TRANSITION_OFF, 0x8001_c794, 0);
        c.m.w32(this + 108, 1);
    })
}

/// `tOptionsMenu::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001c7d0): the items fading in, the
/// screen's fade going down from its full.
pub fn abi_options_menu_transition_on(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        m.w8(this + 112, (-40i8) as u8);
        m.w32(this + 108, 1);
        m.w32(this + 116, SCREEN_FADE_FULL);
        each_item(c, &mut r, this, ITEM_TRANSITION_ON, 0x8001_c838, 0);
    })
}

/// `tOptionsMenu::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001c86c): when the screen's fade
/// is at its end, every item's transition and the screen's are.
pub fn options_menu_transition_is_finished(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let fade = m.r32(this + 116) as i32;
        let dir = m.r8(this + 112) as i8;
        let moving = if fade >= SCREEN_FADE_FULL as i32 && dir > 0 { 0 } else { (fade > 0 || dir >= 0) as u32 };
        m.w32(this + 108, moving);
        if m.r32(this + 108) == 0 {
            r.0[0] = 0;
            if item(m, this, 0) != 0 {
                r.0[2] = 1;
                let mut k = 0u32;
                loop {
                    let it = item(&mut c.m, this, k);
                    let (obj, f) = method(&mut c.m, it, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED);
                    let done = r.call_ptr_leaving(c, f, &[obj], 0x8001_c928);
                    if done != 1 {
                        c.m.w32(this + 108, 1);
                    }
                    k = k.wrapping_add(1);
                    r.0[0] = k;
                    if item(&mut c.m, this, k) == 0 {
                        break;
                    }
                }
            }
        }
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let screen = m.r32(app + 12);
        let (obj, f) = method(m, screen, SCREEN_VPTR, SCREEN_TRANSITION_IS_FINISHED);
        let done = r.call_ptr_leaving(c, f, &[obj], 0x8001_c984);
        if done != 1 {
            c.m.w32(this + 108, 1);
        }
        c.m.r32(this + 108) ^ 1
    })
}

pub fn abi_options_menu_transition_is_finished(c: &mut Cpu) {
    c.r[2] = options_menu_transition_is_finished(c, c.r[4]);
}

/// An item's virtual `UpdateTransition` told it isn't the current one (`r` the registers at
/// the call).
fn update_item_unselected(c: &mut Cpu, r: &Saved, it: u32) {
    let (obj, f) = method(&mut c.m, it, ITEM_VPTR, ITEM_UPDATE_TRANSITION);
    r.call_ptr_leaving(c, f, &[obj, 0], 0x8001_cb68);
}

/// `tOptionsMenu::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001c9b4): out of a transition, each
/// item's, told whether it is the current one. Fading in: the screen's fade first, then the
/// items one at a time from the first; fading out: the items from the last, then the screen.
/// Then its `TransitionIsFinished`.
pub fn options_menu_update_transition(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        if m.r32(this + 108) == 0 {
            r.0[0] = 0;
            if item(m, this, 0) != 0 {
                let mut k = 0u32;
                loop {
                    let m = &mut c.m;
                    let i = sx(k);
                    let it = item(m, this, i);
                    let selected = if m.r32(this + 108) == 0 { (i == m.r32(this + 8)) as u32 } else { 0 };
                    let vt = m.r32(it + ITEM_VPTR);
                    let (obj, f) = method(m, it, ITEM_VPTR, ITEM_UPDATE_TRANSITION);
                    r.call_ptr_leaving(c, f, &[obj, selected, i, vt + ITEM_UPDATE_TRANSITION], 0x8001_cc10);
                    k = k.wrapping_add(1);
                    r.0[0] = k;
                    if item(&mut c.m, this, k) == 0 {
                        break;
                    }
                }
            }
        } else if (m.r8(this + 112) as i8) < 0 {
            let dir = m.r8(this + 112) as i8 as i32;
            let fade = m.r32(this + 116) as i32;
            if fade > 0 {
                m.w32(this + 116, (fade + dir).max(0) as u32);
            } else {
                // the first item still fading
                r.0[0] = 0;
                if item(m, this, 0) != 0 {
                    let mut k = 0u32;
                    loop {
                        let it = item(&mut c.m, this, k);
                        let (obj, f) = method(&mut c.m, it, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED);
                        if r.call_ptr_leaving(c, f, &[obj], 0x8001_ca4c) == 0 {
                            break;
                        }
                        k = k.wrapping_add(1);
                        r.0[0] = k;
                        if item(&mut c.m, this, k) == 0 {
                            break;
                        }
                    }
                    let it = item(&mut c.m, this, k);
                    if it != 0 {
                        update_item_unselected(c, &r, it);
                    }
                }
            }
        } else {
            // the last item still fading, else the screen's fade
            let mut n = 0u32;
            if item(m, this, 0) != 0 {
                loop {
                    n = n.wrapping_add(1);
                    if item(m, this, n) == 0 {
                        break;
                    }
                }
            }
            let mut k = n.wrapping_sub(1);
            r.0[0] = k;
            r.0[2] = u32::MAX;
            let mut found = false;
            while sx(k) != u32::MAX {
                let it = item(&mut c.m, this, k);
                let (obj, f) = method(&mut c.m, it, ITEM_VPTR, ITEM_TRANSITION_IS_FINISHED);
                if r.call_ptr_leaving(c, f, &[obj], 0x8001_cb10) == 0 {
                    found = true;
                    break;
                }
                k = k.wrapping_sub(1);
                r.0[0] = k;
            }
            if found {
                let it = item(&mut c.m, this, k);
                update_item_unselected(c, &r, it);
            } else {
                let m = &mut c.m;
                let fade = m.r32(this + 116) as i32;
                if fade < SCREEN_FADE_FULL as i32 {
                    let v = fade.wrapping_add(m.r8(this + 112) as i8 as i32);
                    m.w32(this + 116, v as u32);
                    if v > SCREEN_FADE_FULL as i32 {
                        m.w32(this + 116, SCREEN_FADE_FULL);
                    }
                }
            }
        }
        let (obj, f) = method(&mut c.m, this, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_cc4c);
    })
}

pub fn abi_options_menu_update_transition(c: &mut Cpu) {
    options_menu_update_transition(c, c.r[4]);
}

/// `tOptionsMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001cc64): the pulsing yellow worked out; the
/// current item moved off disabled ones (forward, then back); the picture's frame by the clock;
/// each item at its own place, told whether it is the current one (none in a transition).
pub fn options_menu_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter(c, 0x8001_c0a8, 0x8001_cc7c, calc_pulsate_yellow);
        let m = &mut c.m;
        let at = |m: &mut Mem, k: u32| m.r32(this.wrapping_add(k << 2).wrapping_add(16));
        let cur = m.r32(this + 8);
        let it = at(m, cur);
        if it == 0 || m.r32(it) & ITEM_DISABLED != 0 {
            m.w32(this + 8, 0);
        }
        loop {
            let cur = m.r32(this + 8);
            let it = at(m, cur);
            if m.r32(it) & ITEM_DISABLED == 0 || at(m, cur.wrapping_add(1)) == 0 {
                break;
            }
            m.w32(this + 8, cur.wrapping_add(1));
        }
        loop {
            let cur = m.r32(this + 8);
            let it = at(m, cur);
            if m.r32(it) & ITEM_DISABLED == 0 || cur as i32 <= 0 {
                break;
            }
            m.w32(this + 8, cur.wrapping_sub(1));
        }
        let (first, frames) = (m.r32(this + 120) as i32, m.r32(this + 124) as i32);
        if first >= 0 && frames > 0 {
            m.w32(sp + 36, 0x00CE_C844);
            let t = (m.r32(GAME_TICKS) as i32) >> 4;
            let frame = first.wrapping_add(t % frames) as u32;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[frame, 1040, 16, 16, 0, 0, sp + 32], 0x8001_cdc8);
        }
        r.0[1] = 0;
        if item(&mut c.m, this, 0) == 0 {
            return;
        }
        let mut k = 0u32;
        loop {
            let m = &mut c.m;
            let i = sx(k);
            let it = item(m, this, i);
            let selected = if m.r32(this + 108) == 0 { (i == m.r32(this + 8)) as u32 } else { 0 };
            let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DRAW_AT);
            r.call_ptr_leaving(c, f, &[obj, 0, 0, selected], 0x8001_ce34);
            k = k.wrapping_add(1);
            r.0[1] = k;
            if item(&mut c.m, this, k) == 0 {
                return;
            }
        }
    })
}

pub fn abi_options_menu_draw(c: &mut Cpu) {
    options_menu_draw(c, c.r[4]);
}

/// `tOptionsMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001ce68): with no item enabled, the
/// first current and every key but start and help used up; then `tMenu`'s.
pub fn options_menu_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[this, key, player, command]);
        let n = r.enter(c, 0x8002_58b4, 0x8001_ce94, |c| femenu::get_number_enabled_items(&mut c.m, this));
        if n & 0xFFFF == 0 {
            let m = &mut c.m;
            m.w32(this + 8, 0);
            let k = m.r32(key);
            if k != femenu::KEY_START && k != femenu::KEY_HELP {
                m.w32(key, femenu::KEY_NONE);
            }
        }
        r.enter(c, 0x8002_5598, 0x8001_ced4, |c| femenu::process_input(c, this, player, key, command));
    })
}

pub fn abi_options_menu_process_input(c: &mut Cpu) {
    options_menu_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

// tInsideBoxMenu: +108 the item before (a short), +110 the items' scroll (a short: 24 or -24
// toward 0 when the current one changes), +112 its step.

/// `tInsideBoxMenu::tInsideBoxMenu` (FEMENUOPTIONS.CPP, 0x8001cef0): `tMenu`'s, its items from
/// the caller's list, not scrolling.
pub fn abi_inside_box_menu(c: &mut Cpu) {
    let (this, flags, screen, next) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let args = c.r[29] + 16;
    c.r[2] = abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let (options, on_back, title, first) = (m.r32(args), m.r32(args + 4), sx(m.r32(args + 8)), m.r32(args + 12));
        m.w32(sp + 16, options);
        m.w32(sp + 20, on_back);
        m.w32(sp + 24, title);
        r.enter(c, 0x8002_548c, 0x8001_cf28, |c| femenu::menu(&mut c.m, this, flags, screen, next, options, on_back, title));
        c.m.w32(this + MENU_VPTR, INSIDE_BOX_MENU_VT);
        r.enter(c, 0x8002_5454, 0x8001_cf44, |c| femenu::menu_constructor(&mut c.m, this, first, args + 16));
        let m = &mut c.m;
        m.w16(this + 108, 0);
        m.w16(this + 110, 0);
        m.w16(this + 112, 0);
        this
    });
}

/// `tInsideBoxMenu::~tInsideBoxMenu` (FEMENUOPTIONS.CPP, 0x8001cf64).
pub fn abi_inside_box_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((MENU_VPTR, INSIDE_BOX_MENU_VT)), 0x8002_54d0, 0x8001_cf7c, femenu::abi_menu_dtor);
}

/// `tInsideBoxMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001cf8c): up and down used up while
/// the items scroll; up at the first item goes to it (the box closing), down at the last does
/// nothing; the rest `tMenu`'s.
pub fn inside_box_menu_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let k = m.r32(key);
        if m.r16(this + 110) != 0 && (k == femenu::KEY_UP || k == femenu::KEY_DOWN) {
            m.w32(key, femenu::KEY_NONE);
        }
        let k = m.r32(key);
        let cur = m.r32(this + 8);
        if k == femenu::KEY_UP && cur == 0 {
            let first = m.r32(this + 16);
            let (obj, f) = method(m, first, ITEM_VPTR, ITEM_PROCESS_INPUT);
            Saved::of(c).call_ptr_leaving(c, f, &[obj], 0x8001_d008);
            return;
        }
        if k == femenu::KEY_DOWN && m.r32(this.wrapping_add(cur.wrapping_add(1) << 2).wrapping_add(16)) == 0 {
            return;
        }
        Saved::of(c).enter(c, 0x8002_5598, 0x8001_d04c, |c| femenu::process_input(c, this, player, key, command));
    })
}

pub fn abi_inside_box_menu_process_input(c: &mut Cpu) {
    inside_box_menu_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tInsideBoxMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001d05c): the items scrolling toward the
/// current one; the two before it, it and the two after (the last only when the one before it
/// is there), 24 apart from `y + top`, each told whether it is the current one (none while
/// scrolling): first its `UpdateTransition`, then its four-argument `Draw` at `x` with `w`.
pub fn inside_box_menu_draw(c: &mut Cpu, this: u32, x: u32, y: u32, w: u32, top: u32) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[6] = top & 0xFFFF;
        let m = &mut c.m;
        let (prev, cur) = (m.r16(this + 108) as i16 as i32, m.r32(this + 8) as i32);
        if cur != prev {
            let (scroll, step) = if prev < cur { (24i16, -4i16) } else { (-24, 4) };
            m.w16(this + 110, scroll as u16);
            m.w16(this + 112, step as u16);
            let v = m.r16(this + 8);
            m.w16(this + 108, v);
        }
        if m.r16(this + 110) != 0 {
            let v = m.r16(this + 110).wrapping_add(m.r16(this + 112));
            m.w16(this + 110, v);
            if (m.r16(this + 112) as i16) < 0 && (v as i16) < 0 {
                m.w16(this + 110, 0);
            }
            if (m.r16(this + 112) as i16) > 0 && (m.r16(this + 110) as i16) > 0 {
                m.w16(this + 110, 0);
            }
        }
        let (x, y, w) = (sx(x), sx(y), sx(w));
        r.0[3] = w;
        r.0[4] = y;
        r.0[5] = x;
        let mut slot = 0u32;
        loop {
            r.0[1] = slot;
            let m = &mut c.m;
            let k = (m.r16(this + 8) as u32).wrapping_add(slot.wrapping_sub(2));
            r.0[2] = k;
            let ki = sx(k);
            if sx(slot) as i32 >= 4 && m.r32(this.wrapping_add(ki << 2).wrapping_add(12)) == 0 {
                return;
            }
            if (ki as i32) >= 0 {
                let it = m.r32(this.wrapping_add(ki << 2).wrapping_add(16));
                if it != 0 {
                    let selected = if m.r16(this + 110) == 0 { (ki == m.r32(this + 8)) as u32 } else { 0 };
                    let vt = m.r32(it + ITEM_VPTR);
                    let (obj, f) = method(m, it, ITEM_VPTR, ITEM_UPDATE_TRANSITION);
                    r.call_ptr_leaving(c, f, &[obj, selected, vt + ITEM_UPDATE_TRANSITION], 0x8001_d1e4);
                    let m = &mut c.m;
                    let it = item(m, this, k);
                    let row = (sx(slot) as i32 - 1).wrapping_mul(24) as u32;
                    let at = (lh(m, this + 110)).wrapping_add(y.wrapping_add(sx(top)).wrapping_add(row)).wrapping_add(5);
                    let selected = if m.r16(this + 110) == 0 { (ki == m.r32(this + 8)) as u32 } else { 0 };
                    let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DRAW_AT3);
                    r.call_ptr_leaving(c, f, &[obj, x, at, w, selected], 0x8001_d27c);
                }
            }
            slot = slot.wrapping_add(1);
            if sx(slot) as i32 >= 5 {
                return;
            }
        }
    })
}

pub fn abi_inside_box_menu_draw(c: &mut Cpu) {
    let top = abi::arg(c, 4);
    inside_box_menu_draw(c, c.r[4], c.r[5], c.r[6], c.r[7], top);
}

// tMenuItemSlidingMenu: +28 the menu open in the box, +32 the one to open next, +36 the box's
// width, +38 its height, +40 how far it is open, +42 the slide, +44 the fade (+46 its step, +48
// whether it fades), +52 transitioning out, +56 closing, +60 and +62 where the box goes, +64
// whether its back is filled.

/// `tMenuItemSlidingMenu::tMenuItemSlidingMenu` (FEMENUOPTIONS.CPP, 0x8001d2c0): a box `w` by
/// `h` (flag 0x80), no menu in it.
#[allow(clippy::too_many_arguments)]
pub fn sliding_menu(c: &mut Cpu, this: u32, text: u32, w: u32, h: u32, dx: u32, dy: u32, fill: u32) -> u32 {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[..6].copy_from_slice(&[this, dx, dy, fill, w, h]);
        r.enter(c, 0x8002_4754, 0x8001_d2fc, |c| femenu::menu_item(&mut c.m, this, text));
        let m = &mut c.m;
        let flags = m.r32(this);
        m.w32(this + 28, 0);
        m.w32(this + 32, 0);
        m.w16(this + 36, w as u16);
        m.w16(this + 38, h as u16);
        m.w16(this + 8, 0);
        m.w32(this, flags | 0x80);
        let flags = m.r32(this);
        m.w32(this + ITEM_VPTR, SLIDING_MENU_VT);
        m.w16(this + 60, dx as u16);
        m.w16(this + 62, dy as u16);
        m.w32(this + 64, fill);
        m.w32(this, flags | 0x80);
        this
    })
}

pub fn abi_sliding_menu(c: &mut Cpu) {
    let (dx, dy, fill) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    c.r[2] = sliding_menu(c, c.r[4], c.r[5], c.r[6], c.r[7], dx, dy, fill);
}

/// `tMenuItemSlidingMenu::~tMenuItemSlidingMenu` (FEMENUOPTIONS.CPP, 0x8001d368).
pub fn abi_sliding_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((ITEM_VPTR, SLIDING_MENU_VT)), 0x8002_4784, 0x8001_d380, femenu::abi_menu_item_dtor);
}

/// `tMenuItemSlidingMenu::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001d390): transitioning out;
/// fading out from 0 with a menu open.
pub fn abi_sliding_menu_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let open = m.r32(this + 28);
    m.w16(this + 46, 0);
    m.w32(this + 52, 1);
    if open != 0 {
        m.w32(this + 48, 1);
        m.w16(this + 46, 30);
        m.w16(this + 44, 0);
    }
}

/// `tMenuItemSlidingMenu::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001d3bc): fading in from 128,
/// closed, the slide at half the height again.
pub fn abi_sliding_menu_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let h = m.r16(this + 38);
    m.w16(this + 46, (-30i16) as u16);
    m.w32(this + 48, 1);
    m.w16(this + 40, 0);
    m.w32(this + 52, 0);
    m.w16(this + 44, 128);
    m.w16(this + 42, h.wrapping_add((h as i16 >> 1) as u16));
}

/// `tMenuItemSlidingMenu::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001d3f4): finished
/// unless a menu is open and one to open next, and the fade isn't at its end.
pub fn abi_sliding_menu_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    if m.r32(this + 28) == 0 || m.r32(this + 32) == 0 {
        m.w16(this + 46, 0);
        m.w32(this + 48, 0);
        c.r[2] = 1;
        return;
    }
    c.r[2] = fade_is_finished(m, this, SLIDING_FADE);
}

/// `tMenuItemSlidingMenu::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001d47c).
pub fn abi_sliding_menu_update_transition(c: &mut Cpu) {
    fade_update_transition(c, c.r[4], SLIDING_FADE, c.r[5], [0x8001_d4d8, 0x8001_d4e4]);
}

/// `tMenuItemSlidingMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001d4f8): up and down, and the
/// open menu's.
pub fn sliding_menu_debounce_keys(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let open = c.m.r32(this + 28);
        if open == 0 {
            return femenu::KEY_UP | femenu::KEY_DOWN;
        }
        let (obj, f) = method(&mut c.m, open, MENU_VPTR, MENU_DEBOUNCE_KEYS);
        Saved::of(c).call_ptr_leaving(c, f, &[obj], 0x8001_d52c) | femenu::KEY_UP | femenu::KEY_DOWN
    })
}

pub fn abi_sliding_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = sliding_menu_debounce_keys(c, c.r[4]);
}

/// `tMenuItemSlidingMenu::Draw(bool)` (FEMENUOPTIONS.CPP, 0x8001d540): its `Draw` at (0, 0).
pub fn sliding_menu_draw_bool(c: &mut Cpu, this: u32, selected: u32) {
    abi::frame(c, 24, |c, _| {
        let (obj, f) = method(&mut c.m, this, ITEM_VPTR, ITEM_DRAW_AT);
        Saved::of(c).call_ptr_leaving(c, f, &[obj, 0, 0, selected], 0x8001_d56c);
    })
}

pub fn abi_sliding_menu_draw_bool(c: &mut Cpu) {
    sliding_menu_draw_bool(c, c.r[4], c.r[5]);
}

/// The fade 40 nearer `128` (`out`: 0), within 0..128.
fn step_fade(m: &mut Mem, this: u32, out: bool) {
    let fade = m.r16(this + 44) as i16 as i32;
    let v = if out { (fade - 40).max(0) } else { (fade + 40).min(128) };
    m.w16(this + 44, v as u16);
}

/// `tMenuItemSlidingMenu::UpdatefOpenHeight` (FEMENUOPTIONS.CPP, 0x8001d57c): a box changing
/// menus closes first (4 a frame; then the next menu fading in, the old one out); open, the
/// selected box opens to its height (4 a frame), then slides (6 a frame); an unselected one
/// closes.
pub fn sliding_menu_update_open_height(m: &mut Mem, this: u32, selected: u32) {
    m.w32(this + 56, 0);
    if m.r16(this + 40) == 0 {
        let h = m.r16(this + 38);
        m.w16(this + 42, h.wrapping_add((h as i16 >> 1) as u16));
    }
    let (next, open) = (m.r32(this + 32), m.r32(this + 28));
    if next != open && (m.r16(this + 40) as i16) > 0 {
        let v = (m.r16(this + 40) as i16 as i32 - 4).max(0);
        m.w16(this + 40, v as u16);
        m.w32(this + 56, 1);
    } else if next == 0 && (m.r16(this + 44) as i16) < 128 {
        if m.r32(this + 48) == 0 {
            step_fade(m, this, false);
        }
    } else {
        let fade = m.r16(this + 44) as i16;
        if fade > 0 && m.r32(this + 28) == 0 && m.r32(this + 32) != 0 {
            if m.r32(this + 48) == 0 {
                step_fade(m, this, true);
            }
            if m.r32(this + 52) == 0 {
                let v = m.r32(this + 32);
                m.w32(this + 28, v);
            }
        } else {
            let next = m.r32(this + 32);
            m.w32(this + 28, next);
            if m.r32(this + 52) == 0 && m.r32(this + 48) == 0 {
                step_fade(m, this, next != 0);
            }
        }
    }
    if m.r32(this + 28) == 0 {
        return;
    }
    if selected != 0 {
        if m.r32(this + 56) == 0 {
            let (open, h) = (m.r16(this + 40) as i16 as i32, m.r16(this + 38) as i16 as i32);
            if open < h {
                m.w16(this + 40, (open + 4).min(h) as u16);
            } else {
                let v = (m.r16(this + 42) as i16 as i32 - 6).max(0);
                m.w16(this + 42, v as u16);
            }
        }
        return;
    }
    let open = m.r16(this + 40) as i16 as i32;
    if open > 0 {
        m.w16(this + 40, (open - 4).max(0) as u16);
    }
}

pub fn abi_sliding_menu_update_open_height(c: &mut Cpu) {
    sliding_menu_update_open_height(&mut c.m, c.r[4], c.r[5]);
}

/// A draw-area primitive set to `rect` (`SetDrawArea`, called by address: the host records the
/// front end's clipping), linked into the ordering table; `r` the registers at the call.
fn set_draw_area(c: &mut Cpu, r: &Saved, rect: u32, ra: u32) {
    let prim = femenuextended::add_prim(&mut c.m);
    r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, rect], ra);
}

/// `tMenuItemSlidingMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001d79c): the box's open height worked
/// out; its text at its own place (offset by `x`, `y`) with the flare (the songs' box, not
/// fully faded) and the arrows (an open box or the songs'); an open box: dark at its edges
/// (two shaded halves), the menu in it (scrolled by the slide), its back filled (corners and
/// squares) or not, the help text of its items, the text again over it and the bar under it.
pub fn sliding_menu_draw(c: &mut Cpu, this: u32, x: u32, y: u32, selected: u32) {
    abi::frame(c, 120, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = x;
        r.0[1] = y;
        r.0[6] = this;
        let (obj, f) = method(&mut c.m, this, ITEM_VPTR, SLIDING_UPDATE_OPEN_HEIGHT);
        r.call_ptr_leaving(c, f, &[obj, selected], 0x8001_d7f0);
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + SLIDING_FADE));
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade], 0x8001_d800);
        c.m.w32(sp + 56, color);
        let text = c.m.r32(this + 4);
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8001_d80c).wrapping_add(x);
        c.m.w32(sp + 60, tx);
        let text = c.m.r32(this + 4);
        let ty = r.call_leaving(c, WORD_Y, &[text], 0x8001_d81c).wrapping_add(y);
        let m = &mut c.m;
        m.w32(BOX_HELP_TEXT, u32::MAX);
        m.w32(sp + 64, ty);
        let shapes = m.r32(SHAPES).wrapping_add(960);
        m.w32(sp + 68, shapes);
        let songs = (m.r32(this + 28) == m.r32(SONGS_SCREEN).wrapping_add(10072)) as u32;
        m.w32(sp + 72, songs);
        let fade = lh(m, this + SLIDING_FADE);
        if fade != 128 && songs != 0 {
            let sel = lh(m, this + 8);
            r.enter(c, 0x8001_c100, 0x8001_d888, |c| draw_left_flare(c, ty, sel, fade, SLIDING_FLARE_PHASE));
        }
        if c.m.r32(this + 28) != 0 || songs != 0 {
            // the arrows either side
            let right = if songs != 0 { 220 } else { lh(&mut c.m, this + 36) };
            r.0[0] = right;
            let m = &mut c.m;
            let (sel, fade) = (lh(m, this + 8), lh(m, this + SLIDING_FADE));
            let arrow = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0, 0xBEBE, sel, fade], 0x8001_d8d0);
            let m = &mut c.m;
            m.w32(sp + 36, arrow);
            let end = tx.wrapping_add(right);
            r.0[1] = end;
            r.0[2] = 1;
            r.0[0] = sp + 32;
            let ax = end.wrapping_sub(lh(m, shapes + 16)).wrapping_sub(10);
            let ay = if songs != 0 { ty.wrapping_sub(3) } else { ty.wrapping_sub(2) };
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[57, 24, ax, ay, 0, 1, sp + 32], 0x8001_d934);
            let ax = end.wrapping_sub(lh(&mut c.m, shapes + 16)).wrapping_sub(10);
            let ay = if songs != 0 { ty.wrapping_add(3) } else { ty.wrapping_add(4) };
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[58, 24, ax, ay, 0, 1, sp + 32], 0x8001_d984);
        }
        if c.m.r32(this + 28) == 0 {
            return;
        }
        // the box: its two shaded halves, clipped to the screen
        let m = &mut c.m;
        let grey = 0x0050_5050;
        let (bx, by) = (tx.wrapping_add(lh(m, this + 60)), ty.wrapping_add(lh(m, this + 62)));
        let w = lh(m, this + 36);
        let open16 = (m.r16(this + 40) as u32) << 16;
        r.0[1] = grey;
        r.0[2] = open16;
        r.0[4] = bx;
        r.0[5] = by;
        r.0[8] = w;
        let (view, flip) = (m.r32(PLAYER1_VIEW), m.r32(FLIP));
        let env = r.call_leaving(c, GET_DRAWENV, &[view, flip], 0x8001_d9d0);
        let m = &mut c.m;
        m.w16(sp + 32, 0);
        m.w32(sp + 76, env);
        let top = m.r16(env + 2);
        m.w16(sp + 36, 512);
        let h = m.r16(SCREEN_HEIGHT);
        m.w16(sp + 34, top);
        m.w16(sp + 38, h);
        set_draw_area(c, &r, sp + 32, 0x8001_da50);
        let half = ((open16 as i32) >> 17) as u32;
        r.0[0] = half;
        r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[bx, by, w, half, grey, grey, 0, 0], 0x8001_da78);
        r.call_ptr_leaving(c, SUBTRACTIVE_BOX, &[bx, by.wrapping_add(half), w, half, 0, 0, grey, grey], 0x8001_da9c);
        // the menu in it
        let m = &mut c.m;
        let menu = m.r32(this + 28);
        let slide = ((((m.r16(this + 42) as u32) << 17) as i32) >> 16) as u32;
        let shapes = m.r32(SHAPES);
        let bh = lh(m, this + 38);
        r.0[7] = shapes.wrapping_add(992);
        r.0[3] = shapes.wrapping_add(1024);
        let (obj, f) = method(m, menu, MENU_VPTR, MENU_DRAW_BOX);
        r.call_ptr_leaving(c, f, &[obj, sx(bx), sx(by), w, slide, bh], 0x8001_daf4);
        let open = sx(open16 >> 16);
        r.0[2] = open;
        let (corner, corner2) = (shapes.wrapping_add(992), shapes.wrapping_add(1024));
        if c.m.r32(this + 64) != 0 && (lh(&mut c.m, corner + 18) as i32) < open as i32 {
            // the filled back's corners
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[31, 12, bx, by, 0, 0, 0], 0x8001_db34);
            let right = bx.wrapping_add(w);
            r.0[1] = right;
            let x2 = right.wrapping_sub(lh(&mut c.m, corner + 16));
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[31, 8, x2, by, 0, 0, 0], 0x8001_db5c);
            let bottom = by.wrapping_add(open);
            r.0[0] = bottom;
            let y2 = bottom.wrapping_sub(lh(&mut c.m, corner2 + 18));
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[32, 12, bx, y2, 0, 0, 0], 0x8001_db84);
            let m = &mut c.m;
            let (x2, y2) = (right.wrapping_sub(lh(m, corner2 + 16)), bottom.wrapping_sub(lh(m, corner2 + 18)));
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[32, 8, x2, y2, 0, 0, 0], 0x8001_dbac);
        }
        let m = &mut c.m;
        let (cw, cw2) = (lh(m, corner + 16), lh(m, corner2 + 16));
        let sw = w.wrapping_sub(cw).wrapping_sub(cw2).wrapping_sub(10);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, bx.wrapping_add(cw).wrapping_add(5), by, sw, open], 0x8001_dbd8);
        let m = &mut c.m;
        let (ch, ch2) = (lh(m, corner + 18), lh(m, corner2 + 18));
        if (ch.wrapping_add(ch2) as i32) < open as i32 {
            let sh = open.wrapping_sub(ch).wrapping_sub(ch2);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, bx, by.wrapping_add(ch), w, sh], 0x8001_dc10);
        }
        // clipped to the box
        let m = &mut c.m;
        m.w16(sp + 40, bx as u16);
        let env = m.r32(sp + 76);
        let top = m.r16(env + 2);
        m.w16(sp + 44, w as u16);
        m.w16(sp + 46, open as u16);
        m.w16(sp + 42, top.wrapping_add(by as u16));
        set_draw_area(c, &r, sp + 40, 0x8001_dc88);
        r.0[0] = 0x8005_0000;
        let help = c.m.r32(BOX_HELP_TEXT);
        if help != u32::MAX {
            let word = r.call_leaving(c, TEXT_WORD, &[help], 0x8001_dca4);
            r.0[1] = word;
            let help = c.m.r32(BOX_HELP_TEXT);
            let hx = sx(r.call_leaving(c, WORD_X, &[help], 0x8001_dcb0));
            r.0[0] = hx;
            let help = c.m.r32(BOX_HELP_TEXT);
            let hy = sx(r.call_leaving(c, WORD_Y, &[help], 0x8001_dcc0));
            let yellow = c.m.r32(PULSATE_YELLOW);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, hx, hy, yellow, 0, 2], 0x8001_dce8);
        }
        let m = &mut c.m;
        let text = m.r32(this + 4);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_dcf4);
        let m = &mut c.m;
        let (tx, ty, color, songs) = (m.r32(sp + 60), m.r32(sp + 64), m.r32(sp + 56), m.r32(sp + 72));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), color, 0, (songs != 0) as u32], 0x8001_dd3c);
        // the bar under the text
        let m = &mut c.m;
        let bar_y = ty.wrapping_sub(2);
        r.0[0] = bar_y;
        let shapes = m.r32(sp + 68);
        let bar_x = tx.wrapping_add(lh(m, this + 36)).wrapping_sub(lh(m, shapes + 16));
        let fade = lh(m, this + SLIDING_FADE);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[30, 8, bar_x, bar_y, fade, 0, 0], 0x8001_dd7c);
        let m = &mut c.m;
        if m.r32(this + 64) != 0 {
            let shapes = m.r32(sp + 68);
            let len = lh(m, this + 36).wrapping_sub(lh(m, shapes + 16));
            let (h, tx) = (lh(m, shapes + 18), m.r32(sp + 60));
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, tx, bar_y, len, h], 0x8001_ddb0);
        }
    })
}

pub fn abi_sliding_menu_draw(c: &mut Cpu) {
    sliding_menu_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemSlidingMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001dde0): a fully open box's
/// menu takes the input first; down used up; then `tMenuItem`'s.
pub fn sliding_menu_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[this, key, player, command]);
        let m = &mut c.m;
        let menu = m.r32(this + 28);
        if m.r16(this + 40) == m.r16(this + 38) && menu != 0 {
            let (obj, f) = method(m, menu, MENU_VPTR, MENU_PROCESS_INPUT);
            r.call_ptr_leaving(c, f, &[obj], 0x8001_de44);
        }
        if c.m.r32(key) == femenu::KEY_DOWN {
            c.m.w32(key, femenu::KEY_NONE);
        }
        r.call_leaving(c, MENU_ITEM_PROCESS_INPUT, &[this, player, key, command], 0x8001_de6c);
    })
}

pub fn abi_sliding_menu_process_input(c: &mut Cpu) {
    sliding_menu_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemSlidingMenu::SetMenu` (FEMENUOPTIONS.CPP, 0x8001de88): the menu to open next (and
/// now, when `now`); the box disabled without one.
pub fn sliding_menu_set_menu(m: &mut Mem, this: u32, now: u32, menu: u32) {
    m.w32(this + 32, menu);
    if now != 0 {
        m.w32(this + 28, menu);
    }
    let flags = m.r32(this);
    let none = m.r32(this + 32) == 0;
    m.w32(this, if none { flags | ITEM_DISABLED } else { flags & !ITEM_DISABLED });
}

pub fn abi_sliding_menu_set_menu(c: &mut Cpu) {
    sliding_menu_set_menu(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

// tMenuItemSlidingActivated: +68 whether it is active (the music playing in the songs' box).

/// `tMenuItemSlidingActivated::UpdatefOpenHeight` (FEMENUOPTIONS.CPP, 0x8001decc): the slide
/// 3 up a frame while active, else 3 down, within 0 and half the height again.
pub fn sliding_activated_update_open_height(m: &mut Mem, this: u32) {
    let slide = m.r16(this + 42);
    let slide = if m.r32(this + 68) != 0 { slide.wrapping_sub(3) } else { slide.wrapping_add(3) };
    m.w16(this + 42, slide);
    let h = m.r16(this + 38) as i16 as i32;
    let v = (slide as i16 as i32).min(h + (h >> 1)).max(0);
    m.w16(this + 42, v as u16);
}

pub fn abi_sliding_activated_update_open_height(c: &mut Cpu) {
    sliding_activated_update_open_height(&mut c.m, c.r[4]);
}

/// `tMenuItemSlidingActivated::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001df60): fading in from
/// 128, inactive, fully open, the slide at half the height again.
pub fn abi_sliding_activated_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let h = m.r16(this + 38);
    m.w16(this + 46, (-30i16) as u16);
    m.w32(this + 48, 1);
    m.w32(this + 68, 0);
    m.w32(this + 52, 0);
    m.w16(this + 44, 128);
    m.w16(this + 42, h.wrapping_add((h as i16 >> 1) as u16));
    m.w16(this + 40, h);
}

/// `tMenuItemSlidingActivated::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001e028).
pub fn abi_sliding_activated_update_transition(c: &mut Cpu) {
    fade_update_transition(c, c.r[4], SLIDING_FADE, c.r[5], [0x8001_e084, 0x8001_e090]);
}

/// `tMenuItemSlidingActivated::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001e0a4): accept makes it
/// active (the menu music stopped, the game's songs loaded and one playing), start makes it
/// inactive again (the menu music back); active and slid in, its menu takes the input and up
/// and down are used up; then `tMenuItem`'s.
pub fn sliding_activated_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1..5].copy_from_slice(&[this, key, player, command]);
        let k = c.m.r32(key);
        if k == femenu::KEY_ACCEPT && c.m.r32(this + 68) == 0 {
            r.call_leaving(c, femenu::PLAY_FE_SFX, &[0], 0x8001_e188);
            c.m.w32(this + 68, 1);
            c.m.w32(key, femenu::KEY_NONE);
            r.call_leaving(c, STOP_SONG, &[20], 0x8001_e19c);
            r.call_leaving(c, MUSIC_CLEAN_UP, &[], 0x8001_e1a4);
            r.call_leaving(c, MUSIC_START_UP, &[0xC000, 0x18000, GAME_MUSIC_FILE], 0x8001_e1bc);
            r.call_ptr_leaving(c, PLAY_SONG, &[GAME_SONGS], 0x8001_e1c8);
            let list = r.call_leaving(c, LOAD_SONG_LIST, &[SONG_LIST_FILE, 0], 0x8001_e1d8);
            let state = c.m.r32(MUSIC_STATE);
            c.m.w32(state + 120, list);
        } else if k == femenu::KEY_START && c.m.r32(this + 68) != 0 {
            r.call_leaving(c, femenu::PLAY_FE_SFX, &[1], 0x8001_e100);
            c.m.w32(this + 68, 0);
            c.m.w32(key, femenu::KEY_NONE);
            r.call_leaving(c, STOP_SONG, &[20], 0x8001_e114);
            r.0[0] = 0x8005_0000;
            let state = c.m.r32(MUSIC_STATE);
            let list = c.m.r32(state + 120);
            if list != 0 {
                r.call_leaving(c, PURGE_MEM, &[list], 0x8001_e138);
            }
            let state = c.m.r32(MUSIC_STATE);
            c.m.w32(state + 120, 0);
            r.call_leaving(c, MUSIC_CLEAN_UP, &[], 0x8001_e144);
            r.call_leaving(c, MUSIC_START_UP, &[0xD800, 0x18000, MENU_MUSIC_FILE], 0x8001_e15c);
            r.call_ptr_leaving(c, PLAY_SONG, &[MENU_SONGS], 0x8001_e168);
        }
        let m = &mut c.m;
        if m.r16(this + 42) == 0 {
            if m.r32(this + 68) == 0 {
                r.call_leaving(c, MENU_ITEM_PROCESS_INPUT, &[this, player, key, command], 0x8001_e27c);
                return;
            }
            let menu = m.r32(this + 28);
            if menu != 0 {
                let (obj, f) = method(m, menu, MENU_VPTR, MENU_PROCESS_INPUT);
                r.call_ptr_leaving(c, f, &[obj, player, key, command], 0x8001_e238);
            }
        }
        let m = &mut c.m;
        if m.r32(this + 68) != 0 {
            let k = m.r32(key);
            if k == femenu::KEY_UP || k == femenu::KEY_DOWN {
                m.w32(key, femenu::KEY_NONE);
            }
        }
        r.call_leaving(c, MENU_ITEM_PROCESS_INPUT, &[this, player, key, command], 0x8001_e27c);
    })
}

pub fn abi_sliding_activated_process_input(c: &mut Cpu) {
    sliding_activated_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemLeftRightFade::MyLeftRightDraw` (FEMENUOPTIONS.CPP, 0x8001e29c): a fading
/// left-right choice's flare, its text at (`x`, `y`) and the arrows either side of its value.
pub fn my_left_right_draw(c: &mut Cpu, this: u32, x: u32, y: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, y, x]);
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let arrows = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0x00C8_3C1E, 0xBEBE, sel, fade], 0x8001_e2d4);
        c.m.w32(sp + 36, arrows);
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        r.enter(c, 0x8001_c100, 0x8001_e2ec, |c| draw_left_flare(c, y, sel, fade, this + 40));
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade], 0x8001_e2fc);
        r.0[0] = color;
        let text = c.m.r32(this + 4);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_e308);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(x), sx(y), color, 0, 1], 0x8001_e330);
        let ay = y.wrapping_add(1);
        r.0[1] = ay;
        r.0[0] = sp + 32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[46, 24, x.wrapping_add(26), ay, 0, 0, sp + 32], 0x8001_e358);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[47, 24, x.wrapping_add(200), ay, 0, 0, sp + 32], 0x8001_e378);
    })
}

pub fn abi_my_left_right_draw(c: &mut Cpu) {
    my_left_right_draw(c, c.r[4], c.r[5], c.r[6]);
}

/// The iterator's virtual function `slot` (`Value` or `TextValue`) for `player`; `r` the
/// registers at the call.
fn iterator_call(c: &mut Cpu, r: &Saved, data: u32, slot: u32, player: u32, ra: u32) -> u32 {
    let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, slot);
    r.call_ptr_leaving(c, f, &[obj, player, data], ra)
}

/// `tMenuItemDisplayLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001e390): unless faded out,
/// the choice at its text's place (offset by `x`, `y`), its value (the sub-menu player's)
/// right of it.
pub fn display_choice_draw(c: &mut Cpu, this: u32, x: u32, y: u32) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = x;
        r.0[2] = y;
        r.0[3] = this;
        if c.m.r16(this + FADE) == 128 {
            return;
        }
        let text = c.m.r32(this + 4);
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8001_e3cc).wrapping_add(x);
        r.0[1] = tx;
        let text = c.m.r32(this + 4);
        let ty = r.call_leaving(c, WORD_Y, &[text], 0x8001_e3d8).wrapping_add(y);
        r.0[0] = ty;
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade], 0x8001_e3ec);
        let ty = sx(ty);
        r.0[0] = ty;
        r.0[2] = color;
        r.enter(c, 0x8001_e29c, 0x8001_e40c, |c| my_left_right_draw(c, this, sx(tx), ty));
        let m = &mut c.m;
        let (data, player) = (m.r32(this + 28), m.r32(SUB_MENU_PLAYER));
        let id = iterator_call(c, &r, data, femenu::TEXT_VALUE, player, 0x8001_e430);
        let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8001_e43c);
        // (s1 keeps the sum shifted up, the argument shifted back)
        let vx = sx(tx.wrapping_add(115));
        r.0[1] = tx.wrapping_add(115) << 16;
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, vx, ty, color, 0, 2], 0x8001_e464);
    })
}

pub fn abi_display_choice_draw(c: &mut Cpu) {
    display_choice_draw(c, c.r[4], c.r[5], c.r[6]);
}

// tMenuItemOnOffLeftRightChoice: +44 the on-off fade (128 on).

/// `tMenuItemOnOffLeftRightChoice::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001e480): the on-off
/// fade at its value's end, then the left-right fade's.
pub fn on_off_transition_on(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_e4bc);
        c.m.w16(this + 44, (((v & 0xFF) != 0) as u16) << 7);
        r.enter(c, 0x8001_c50c, 0x8001_e4d4, |c| {
            c.r[4] = this;
            abi_left_right_fade_transition_on(c);
        });
    })
}

pub fn abi_on_off_transition_on(c: &mut Cpu) {
    on_off_transition_on(c, c.r[4]);
}

/// `tMenuItemOnOffLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001e4e4): unless faded out, the
/// on-off fade moving toward the value's (32 a frame), the choice at its text's place (offset
/// by `x`, `y`), ON and OFF right of it in the fade's colours.
pub fn on_off_draw(c: &mut Cpu, this: u32, x: u32, y: u32) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = x;
        r.0[2] = this;
        r.0[3] = y;
        if c.m.r16(this + FADE) == 128 {
            return;
        }
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_e538);
        let m = &mut c.m;
        let on = m.r16(this + 44);
        m.w16(this + 44, if v & 0xFF != 0 { on.wrapping_add(32) } else { on.wrapping_sub(32) });
        if (m.r16(this + 44) as i16) > 128 {
            m.w16(this + 44, 128);
        }
        if (m.r16(this + 44) as i16) < 0 {
            m.w16(this + 44, 0);
        }
        let text = m.r32(this + 4);
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8001_e598).wrapping_add(x);
        r.0[1] = tx;
        let text = c.m.r32(this + 4);
        let wy = r.call_leaving(c, WORD_Y, &[text], 0x8001_e5a4);
        let ty = wy.wrapping_add(y);
        r.0[0] = ty;
        let m = &mut c.m;
        let (on, sel, fade) = (lh(m, this + 44), lh(m, this + 8), lh(m, this + FADE));
        r.call_leaving(c, CALC_ON_OFF_FADE, &[6, on, sel, fade, sp + 24, sp + 28], 0x8001_e5cc);
        let ty = sx(ty);
        r.0[0] = ty;
        r.enter(c, 0x8001_e29c, 0x8001_e5e8, |c| my_left_right_draw(c, this, sx(tx), ty));
        let word = r.call_leaving(c, TEXT_WORD, &[TEXT_ON], 0x8001_e5f0);
        let color = c.m.r32(sp + 24);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(55)), ty, color, 0, 0], 0x8001_e614);
        let word = r.call_leaving(c, TEXT_WORD, &[TEXT_OFF], 0x8001_e61c);
        let ox = sx(tx.wrapping_add(158));
        r.0[1] = tx.wrapping_add(158) << 16;
        let color = c.m.r32(sp + 28);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, ox, ty, color, 0, 0], 0x8001_e640);
    })
}

pub fn abi_on_off_draw(c: &mut Cpu) {
    on_off_draw(c, c.r[4], c.r[5], c.r[6]);
}

// tMenuItemLeftRightAudioSlider: the slider's place at +32 (its text's, +34 the y), +40 the
// fade (+42 its step, +44 whether it fades), +48 the speaker picture, +52 the flare's phase.

/// `tMenuItemLeftRightAudioSlider::tMenuItemLeftRightAudioSlider` (FEMENUOPTIONS.CPP,
/// 0x8001e65c): a slider with picture `art`.
pub fn audio_slider(c: &mut Cpu, this: u32, text: u32, data: u32, art: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = art;
        r.enter(c, 0x8002_4b18, 0x8001_e678, |c| femenu::left_right_slider(c, this, text, data));
        let m = &mut c.m;
        m.w32(this + ITEM_VPTR, AUDIO_SLIDER_VT);
        m.w16(this + 48, art as u16);
        m.w32(this + 52, 0);
        m.w16(this + 8, 0);
        this
    })
}

pub fn abi_audio_slider(c: &mut Cpu) {
    c.r[2] = audio_slider(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemLeftRightAudioSlider::~tMenuItemLeftRightAudioSlider` (FEMENUOPTIONS.CPP,
/// 0x8001e6a8).
pub fn abi_audio_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((ITEM_VPTR, AUDIO_SLIDER_VT)), 0x8002_4b74, 0x8001_e6c0, femenu::abi_left_right_slider_dtor);
}

/// `tMenuItemLeftRightAudioSlider::Draw` (FEMENUOPTIONS.CPP, 0x8001e6d0): the slider placed at
/// its text's place (offset by `x`, `y`); the flare; unless faded out its text (in its text
/// type's colours, blended by the selection), the speaker picture when selected and the
/// slider at the value.
pub fn audio_slider_draw(c: &mut Cpu, this: u32, x: u32, y: u32) {
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = x;
        r.0[1] = y;
        r.0[3] = this;
        let text = c.m.r32(this + 4);
        let wx = r.call_leaving(c, WORD_X, &[text], 0x8001_e6fc);
        c.m.w16(this + 32, wx.wrapping_add(x) as u16);
        let text = c.m.r32(this + 4);
        let wy = r.call_leaving(c, WORD_Y, &[text], 0x8001_e70c);
        c.m.w16(this + 34, wy.wrapping_add(y) as u16);
        r.0[1] = TEXT_COLORS;
        let id = lh(&mut c.m, this + 4);
        let ty = r.call_leaving(c, WORD_FLAGS, &[id], 0x8001_e724);
        r.0[0] = TEXT_STYLES;
        let style = |m: &mut Mem, ty: u32, k: u32| m.r8(TEXT_STYLES.wrapping_add(ty.wrapping_mul(6)).wrapping_add(k)) as u32;
        let k = style(&mut c.m, ty, 5);
        let hi = c.m.r32(TEXT_COLORS + 4 * k);
        r.0[2] = hi;
        let id = lh(&mut c.m, this + 4);
        let ty = r.call_leaving(c, WORD_FLAGS, &[id], 0x8001_e758);
        let k = style(&mut c.m, ty, 4);
        let lo = c.m.r32(TEXT_COLORS + 4 * k);
        r.0[0] = lo;
        let m = &mut c.m;
        let (fy, sel, fade) = (lh(m, this + 34), lh(m, this + 8), lh(m, this + 40));
        r.enter(c, 0x8001_c100, 0x8001_e78c, |c| draw_left_flare(c, fy, sel, fade, this + 52));
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + 40));
        let color = r.call_leaving(c, CALC_FADE_VAL_FADED, &[lo, hi, sel, fade], 0x8001_e7a0);
        r.0[0] = color;
        if c.m.r16(this + 40) == 128 {
            return;
        }
        let text = c.m.r32(this + 4);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_e7c0);
        let m = &mut c.m;
        let (px, py) = (lh(m, this + 32), lh(m, this + 34));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, px, py, color, 0, 1], 0x8001_e7e0);
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + 40));
        let speaker = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0x0055_1E00, 0xBEBE, sel, fade], 0x8001_e7f8);
        c.m.w32(sp + 60, speaker);
        if c.m.r16(this + 8) != 0 {
            let art = lh(&mut c.m, this + 48).wrapping_add(1);
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[art, 16, 0, 0, 0, 0, sp + 56], 0x8001_e830);
        }
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_e854) & 0xFF;
        let m = &mut c.m;
        let data = m.r32(this + 28);
        let (min, max) = (m.r8(data + 8) as u32, m.r8(data + 9) as u32);
        let sx_ = sx((m.r16(this + 32) as u32).wrapping_add(20));
        let sy = sx((m.r16(this + 34) as u32).wrapping_add(1));
        let (w, h, sel, fade) = (lh(m, this + 36), lh(m, this + 38), lh(m, this + 8), lh(m, this + 40));
        r.call_ptr_leaving(c, DRAW_SLIDER, &[v, min, max, sx_, sy, w, h, 4, 4, 0, 0, sel, fade], 0x8001_e8c0);
    })
}

pub fn abi_audio_slider_draw(c: &mut Cpu) {
    audio_slider_draw(c, c.r[4], c.r[5], c.r[6]);
}

/// `tMenuItemLeftRightAudioSlider::Percentage` (FEMENUOPTIONS.CPP, 0x8001e8dc): the value's
/// place in the iterator's range in percent (one up when it isn't 0, up to 100).
pub fn audio_slider_percentage(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_e910);
        let m = &mut c.m;
        let data = m.r32(this + 28);
        let min = m.r8(data + 8) as u32;
        let range = (m.r8(data + 9) as u32).wrapping_sub(min);
        let pc = crate::fixed::div(((v & 0xFF).wrapping_sub(min)).wrapping_mul(100) as i32, range as i32) as u32;
        r.0[0] = pc;
        let (obj, f) = method(m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX, data], 0x8001_e984);
        if v & 0xFF != 0 && (pc as i32) < 100 { pc.wrapping_add(1) } else { pc }
    })
}

pub fn abi_audio_slider_percentage(c: &mut Cpu) {
    c.r[2] = audio_slider_percentage(c, c.r[4]);
}

/// `tMenuItemLeftRightAudioSlider::TransitionOff` (FEMENUOPTIONS.CPP, 0x8001e9b0): fading out
/// from 0.
pub fn abi_audio_slider_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w16(this + 40, 0);
    m.w16(this + 42, 60);
}

/// `tMenuItemLeftRightAudioSlider::TransitionOn` (FEMENUOPTIONS.CPP, 0x8001e9c0): fading in
/// from 128, unselected.
pub fn abi_audio_slider_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w16(this + 40, 128);
    m.w16(this + 42, (-60i16) as u16);
    m.w16(this + 8, 0);
}

/// `tMenuItemLeftRightAudioSlider::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x8001e9d8).
pub fn abi_audio_slider_transition_is_finished(c: &mut Cpu) {
    c.r[2] = fade_is_finished(&mut c.m, c.r[4], AUDIO_FADE);
}

/// `tMenuItemLeftRightAudioSlider::UpdateTransition` (FEMENUOPTIONS.CPP, 0x8001ea34): the fade
/// stepped within 0..128, then `tMenuItem`'s.
pub fn audio_slider_update_transition(c: &mut Cpu, this: u32, selected: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let v = (m.r16(this + AUDIO_FADE) as i16 as i32 + m.r16(this + AUDIO_FADE + 2) as i16 as i32).clamp(0, 128);
        m.w16(this + AUDIO_FADE, v as u16);
        Saved::of(c).enter(c, 0x8002_47c8, 0x8001_ea70, |c| femenu::menu_item_update_transition(c, this, selected));
    })
}

pub fn abi_audio_slider_update_transition(c: &mut Cpu) {
    audio_slider_update_transition(c, c.r[4], c.r[5]);
}

// tInsideBoxSongMenu: the songs' on-off fades at +116 and selection fades at +126 (5 each: the
// two songs before the current one, it and the two after); the music's state has the song list
// (+120: its count at +0, 64 bytes a song from +12) and the current song (+116); the front
// end's +924 whether each song is on.

/// The music's song list (0: none loaded) and current song.
fn song_list(m: &mut Mem) -> u32 {
    let state = m.r32(MUSIC_STATE);
    m.r32(state + 120)
}

fn current_song(m: &mut Mem) -> u32 {
    let state = m.r32(MUSIC_STATE);
    lh(m, state + 116)
}

/// `tInsideBoxSongMenu::tInsideBoxSongMenu` (FEMENUOPTIONS.CPP, 0x8001ea80): `tMenu`'s, the
/// fades 0, its items from the caller's list, not scrolling.
pub fn abi_song_menu(c: &mut Cpu) {
    let (this, flags, screen, next) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let args = c.r[29] + 16;
    c.r[2] = abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let (options, on_back, title, first) = (m.r32(args), m.r32(args + 4), sx(m.r32(args + 8)), m.r32(args + 12));
        m.w32(sp + 16, options);
        m.w32(sp + 20, on_back);
        m.w32(sp + 24, title);
        r.enter(c, 0x8002_548c, 0x8001_eab8, |c| femenu::menu(&mut c.m, this, flags, screen, next, options, on_back, title));
        let m = &mut c.m;
        m.w32(this + MENU_VPTR, SONG_MENU_VT);
        for k in 0..5 {
            m.w16(this + 126 + 2 * k, 0);
            m.w16(this + 116 + 2 * k, 0);
        }
        r.enter(c, 0x8002_5454, 0x8001_eaf4, |c| femenu::menu_constructor(&mut c.m, this, first, args + 16));
        let m = &mut c.m;
        m.w16(this + 110, 0);
        m.w16(this + 112, 0);
        this
    });
}

/// `tInsideBoxSongMenu::~tInsideBoxSongMenu` (FEMENUOPTIONS.CPP, 0x8001eb10).
pub fn abi_song_menu_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((MENU_VPTR, SONG_MENU_VT)), 0x8001_cf64, 0x8001_eb28, abi_inside_box_menu_dtor);
}

/// `tInsideBoxSongMenu::Draw` (FEMENUOPTIONS.CPP, 0x8001eb38): with songs loaded, the songs'
/// selection fades toward the middle one, the list scrolling (twice the step), and the two
/// songs before the current one, it and the two after (those there are), 21 apart about the
/// middle of the box (`h` high, from `y + top`), each's on-off fade moving toward whether it is
/// on (32 a frame): its `DrawOneSong`.
#[allow(clippy::too_many_arguments)]
pub fn song_menu_draw(c: &mut Cpu, this: u32, x: u32, y: u32, w: u32, top: u32, h: u32) {
    abi::frame(c, 72, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[6] = top & 0xFFFF;
        r.0[7] = x;
        if song_list(&mut c.m) == 0 {
            return;
        }
        let mid = sx(y).wrapping_add(((sx(h) as i32 - 21) >> 1) as u32);
        r.0[5] = mid;
        let m = &mut c.m;
        for k in 0..5 {
            if k != 2 {
                let v = (m.r16(this + 126 + 2 * k) as i16 as i32 - 8).max(0);
                m.w16(this + 126 + 2 * k, v as u16);
            }
        }
        let v = (m.r16(this + 130) as i16 as i32 + 8).min(128);
        m.w16(this + 130, v as u16);
        if m.r16(this + 110) != 0 {
            let v = m.r16(this + 110).wrapping_add(m.r16(this + 112) << 1);
            let step = m.r16(this + 112) as i16;
            m.w16(this + 110, v);
            if step < 0 && (v as i16) < 0 {
                m.w16(this + 110, 0);
            }
            if (m.r16(this + 112) as i16) > 0 && (m.r16(this + 110) as i16) > 0 {
                m.w16(this + 110, 0);
            }
        }
        r.0[8] = FRONT_END;
        r.0[4] = sx(w);
        let mut row = -40i32 as u32;
        for k in 0..5u32 {
            r.0[0] = this + 2 * k;
            r.0[1] = k;
            r.0[3] = row;
            let m = &mut c.m;
            let song = current_song(m).wrapping_add(k).wrapping_sub(2);
            let list = song_list(m);
            if (song as i32) >= 0 && (song as i32) < m.r32(list) as i32 {
                let fade = this + 116 + 2 * k;
                let on = m.r32(FRONT_END.wrapping_add(song << 2).wrapping_add(924)) != 0;
                let v = m.r16(fade);
                m.w16(fade, if on { v.wrapping_add(32) } else { v.wrapping_sub(32) });
                if (m.r16(fade) as i16) > 128 {
                    m.w16(fade, 128);
                }
                if (m.r16(fade) as i16) < 0 {
                    m.w16(fade, 0);
                }
                let at = (top & 0xFFFF).wrapping_add((m.r16(this + 110) as u32).wrapping_add(mid.wrapping_add(row)));
                let (on_fade, sel_fade) = (lh(m, fade), lh(m, this + 126 + 2 * k));
                let (obj, f) = method(m, this, MENU_VPTR, MENU_DRAW_ONE_SONG);
                r.call_ptr_leaving(c, f, &[obj, sx(song), sx(x), sx(at), sx(w), on_fade, sel_fade], 0x8001_ed64);
            }
            row = row.wrapping_add(21);
        }
    })
}

pub fn abi_song_menu_draw(c: &mut Cpu) {
    let (top, h) = (abi::arg(c, 4), abi::arg(c, 5));
    song_menu_draw(c, c.r[4], c.r[5], c.r[6], c.r[7], top, h);
}

/// `tInsideBoxSongMenu::DrawOneSong` (FEMENUOPTIONS.CPP, 0x8001eda8): a song's two lines of
/// text (in its selection's colour), ON and OFF in its on-off fade's, and its frame.
#[allow(clippy::too_many_arguments)]
pub fn song_menu_draw_one_song(c: &mut Cpu, song: u32, x: u32, y: u32, w: u32, on_fade: u32, sel_fade: u32) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[..6].copy_from_slice(&[sel_fade, on_fade, y, x, song, w]);
        let frame = r.call_leaving(c, CALC_FADE_VAL, &[0x0055_1E00, 40], 0x8001_edf4);
        r.0[6] = frame;
        r.0[1] = on_fade << 16;
        r.0[0] = sx(sel_fade);
        r.call_leaving(c, CALC_ON_OFF_FADE, &[6, sx(on_fade), sx(sel_fade), 0, sp + 24, sp + 28], 0x8001_ee28);
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sx(sel_fade), 0], 0x8001_ee38);
        r.0[1] = MUSIC_STATE & 0xFFFF_0000;
        let entry = ((song << 16) as i32 >> 10) as u32;
        r.0[4] = entry;
        r.0[0] = color;
        let list = song_list(&mut c.m);
        let line = c.m.r32(list.wrapping_add(entry).wrapping_add(12));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[line, sx(x.wrapping_add(3)), sx(y.wrapping_add(2)), color, 0, 0], 0x8001_ee80);
        let list = song_list(&mut c.m);
        let line = c.m.r32(list.wrapping_add(entry).wrapping_add(16));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[line, sx(x.wrapping_add(2)), sx(y.wrapping_add(10)), color, 0, 0], 0x8001_eeb8);
        let word = r.call_leaving(c, TEXT_WORD, &[TEXT_ON], 0x8001_eec0);
        let right = x.wrapping_add(w);
        let ty = sx(y.wrapping_add(6));
        r.0[1] = right;
        r.0[0] = ty;
        r.0[4] = 2;
        let on_color = c.m.r32(sp + 24);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(right.wrapping_sub(90)), ty, on_color, 0, 2], 0x8001_eef8);
        let word = r.call_leaving(c, TEXT_WORD, &[TEXT_OFF], 0x8001_ef00);
        r.0[1] = right.wrapping_sub(30) << 16;
        let off_color = c.m.r32(sp + 28);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(right.wrapping_sub(30)), ty, off_color, 0, 2], 0x8001_ef24);
        // the frame: two dividers, the top and the bottom
        let (x, w, y) = (sx(x), sx(w), sx(y));
        let right = x.wrapping_add(w);
        r.0[3] = x;
        r.0[5] = w;
        r.0[1] = right;
        r.0[2] = y;
        r.0[0] = 19;
        r.call_ptr_leaving(c, DRAW_SQUARE, &[frame, right.wrapping_sub(120), y, 2, 19], 0x8001_ef5c);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[frame, right.wrapping_sub(60), y, 2, 19], 0x8001_ef74);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[frame, x, y, w, 1], 0x8001_ef90);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[frame, x, y.wrapping_add(19), w, u32::MAX], 0x8001_efac);
    })
}

pub fn abi_song_menu_draw_one_song(c: &mut Cpu) {
    let (w, on_fade, sel_fade) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    song_menu_draw_one_song(c, c.r[5], c.r[6], c.r[7], w, on_fade, sel_fade);
}

/// `tInsideBoxSongMenu::DebounceKeys` (FEMENUOPTIONS.CPP, 0x8001efd4): up, down, left and right.
pub fn abi_song_menu_debounce_keys(c: &mut Cpu) {
    c.r[2] = femenu::KEY_UP | femenu::KEY_DOWN | femenu::KEY_LEFT | femenu::KEY_RIGHT;
}

/// `tInsideBoxSongMenu::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001efdc): with songs loaded, left
/// and right turn the current song on or off (with a sound); up and down move to the song
/// before or after, the list and its fades shifting and scrolling (none while it scrolls).
/// The keys it takes are used up.
pub fn song_menu_process_input(c: &mut Cpu, this: u32, key: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = key;
        if song_list(&mut c.m) == 0 {
            return;
        }
        match c.m.r32(key) {
            femenu::KEY_LEFT => r.call_leaving(c, femenu::PLAY_FE_SFX, &[5], 0x8001_f030),
            femenu::KEY_RIGHT => r.call_leaving(c, femenu::PLAY_FE_SFX, &[6], 0x8001_f030),
            _ => 0,
        };
        let m = &mut c.m;
        match m.r32(key) {
            femenu::KEY_LEFT | femenu::KEY_RIGHT => {
                let at = FRONT_END.wrapping_add(current_song(m) << 2).wrapping_add(924);
                let off = (m.r32(at) == 0) as u32;
                m.w32(at, off);
            }
            femenu::KEY_UP => {
                if (current_song(m) as i32) > 0 && m.r16(this + 110) == 0 {
                    m.w16(this + 110, (-21i16) as u16);
                    m.w16(this + 112, 4);
                    for k in (1..5).rev() {
                        let v = m.r16(this + 116 + 2 * (k - 1));
                        m.w16(this + 116 + 2 * k, v);
                        let v = m.r16(this + 126 + 2 * (k - 1));
                        m.w16(this + 126 + 2 * k, v);
                    }
                    m.w16(this + 116, 0);
                    m.w16(this + 126, 0);
                    let state = m.r32(MUSIC_STATE);
                    let v = m.r16(state + 116).wrapping_sub(1);
                    m.w16(state + 116, v);
                    r.call_leaving(c, femenu::PLAY_FE_SFX, &[3], 0x8001_f1c8);
                }
            }
            femenu::KEY_DOWN => {
                let list = song_list(m);
                let last = m.r32(list).wrapping_sub(1);
                if (current_song(m) as i32) < last as i32 && m.r16(this + 110) == 0 {
                    m.w16(this + 110, 21);
                    m.w16(this + 112, (-4i16) as u16);
                    for k in 0..4 {
                        let v = m.r16(this + 116 + 2 * (k + 1));
                        m.w16(this + 116 + 2 * k, v);
                        let v = m.r16(this + 126 + 2 * (k + 1));
                        m.w16(this + 126 + 2 * k, v);
                    }
                    m.w16(this + 124, 0);
                    m.w16(this + 134, 0);
                    let state = m.r32(MUSIC_STATE);
                    let v = m.r16(state + 116).wrapping_add(1);
                    m.w16(state + 116, v);
                    r.call_leaving(c, femenu::PLAY_FE_SFX, &[4], 0x8001_f1c8);
                }
            }
            _ => return,
        }
        c.m.w32(key, femenu::KEY_NONE);
    })
}

pub fn abi_song_menu_process_input(c: &mut Cpu) {
    song_menu_process_input(c, c.r[4], c.r[6]);
}

/// `tMenuItemControllerLeftRightChoice::Draw` (FEMENUOPTIONS.CPP, 0x8001f1e4): unless faded
/// out, the choice's text at its own place (offset by `x`, `y`) and its value (the sub-menu
/// player's) right of it; the arrows, the bar under it and its square.
pub fn controller_choice_draw(c: &mut Cpu, this: u32, x: u32, y: u32) {
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = x;
        r.0[1] = y;
        r.0[4] = this;
        let text = c.m.r32(this + 4);
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8001_f218).wrapping_add(x);
        r.0[2] = tx;
        let text = c.m.r32(this + 4);
        let ty = r.call_leaving(c, WORD_Y, &[text], 0x8001_f224).wrapping_add(y);
        r.0[3] = ty;
        let m = &mut c.m;
        let shapes = m.r32(SHAPES).wrapping_add(960);
        r.0[5] = shapes;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let arrows = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0x00C8_3C1E, 0xBEBE, sel, fade], 0x8001_f24c);
        r.0[0] = arrows;
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade], 0x8001_f260);
        r.0[1] = color;
        c.m.w32(sp + 36, arrows);
        if c.m.r16(this + FADE) != 128 {
            let text = c.m.r32(this + 4);
            r.0[0] = ty << 16;
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_f284);
            r.0[0] = sx(ty);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), color, 0, 0], 0x8001_f2a8);
            let m = &mut c.m;
            let (data, player) = (m.r32(this + 28), m.r32(SUB_MENU_PLAYER));
            let id = iterator_call(c, &r, data, femenu::TEXT_VALUE, player, 0x8001_f2cc);
            let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8001_f2d8);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx.wrapping_add(151)), sx(ty), color, 0, 2], 0x8001_f300);
        }
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let v = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0, 0xBEBE, sel, fade], 0x8001_f314);
        let (x, y) = (sx(tx), sx(ty));
        r.0[1] = x;
        r.0[0] = y;
        r.0[3] = 1;
        r.0[2] = sp + 32;
        c.m.w32(sp + 36, v);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[10, 24, x.wrapping_add(131), y, 0, 1, sp + 32], 0x8001_f350);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[11, 24, x.wrapping_add(161), y, 0, 1, sp + 32], 0x8001_f370);
        let by = y.wrapping_sub(3);
        r.0[0] = by;
        let m = &mut c.m;
        let bx = x.wrapping_sub(lh(m, shapes + 16).wrapping_sub(176));
        let fade = lh(m, this + FADE);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[30, 8, bx, by, fade, 0, 0], 0x8001_f3a0);
        let m = &mut c.m;
        let (sw, sh) = (176u32.wrapping_sub(lh(m, shapes + 16)), lh(m, shapes + 18));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, x, by, sw, sh], 0x8001_f3c4);
    })
}

pub fn abi_controller_choice_draw(c: &mut Cpu) {
    controller_choice_draw(c, c.r[4], c.r[5], c.r[6]);
}

// The sliders in a box: tMenuItemLeftRightSlider's (+32 and +34 where it is drawn, +36 and +38
// its size); a two-way slider's +40 what it calibrates (0..3: the NeGcon's twist or the
// sticks' ranges, each way), +44 whether it is calibrating.

/// `tInsideBoxLeftRightSlider::tInsideBoxLeftRightSlider` (FEMENUOPTIONS.CPP, 0x8001f3e8): a
/// slider 5 high.
pub fn inside_slider(c: &mut Cpu, this: u32, text: u32, data: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter(c, 0x8002_4b18, 0x8001_f3fc, |c| femenu::left_right_slider(c, this, text, data));
        let m = &mut c.m;
        m.w32(this + ITEM_VPTR, INSIDE_SLIDER_VT);
        m.w16(this + 8, 0);
        m.w16(this + 38, 5);
        this
    })
}

pub fn abi_inside_slider(c: &mut Cpu) {
    c.r[2] = inside_slider(c, c.r[4], c.r[5], c.r[6]);
}

/// `tInsideBoxLeftRightSlider::~tInsideBoxLeftRightSlider` (FEMENUOPTIONS.CPP, 0x8001f428).
pub fn abi_inside_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((ITEM_VPTR, INSIDE_SLIDER_VT)), 0x8002_4b74, 0x8001_f440, femenu::abi_left_right_slider_dtor);
}

/// A slider's value and its iterator's range; `r` the registers at the call.
fn slider_value(c: &mut Cpu, r: &Saved, this: u32, ra: u32) -> (u32, u32, u32) {
    let data = c.m.r32(this + 28);
    let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
    let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], ra) & 0xFF;
    let data = c.m.r32(this + 28);
    (v, c.m.r8(data + 8) as u32, c.m.r8(data + 9) as u32)
}

/// `tInsideBoxLeftRightSlider::Draw` (FEMENUOPTIONS.CPP, 0x8001f450): the slider at (`x`, `y`)
/// `w` wide: two lines, its text and the slider at the value.
pub fn inside_slider_draw(c: &mut Cpu, this: u32, x: u32, y: u32, w: u32) {
    abi::frame(c, 80, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = w;
        let m = &mut c.m;
        m.w16(this + 32, x as u16);
        m.w16(this + 34, y as u16);
        let sel = lh(m, this + 8);
        let line = r.call_leaving(c, CALC_FADE_VAL_SEL, &[0x0055_1E00, 0x00C8_3C1E, sel], 0x8001_f494);
        r.0[3] = line;
        let sel = lh(&mut c.m, this + 8);
        let color = r.call_leaving(c, CALC_TEXT_FADE_UNSEL_TO_SEL, &[6, sel, 0], 0x8001_f4a8);
        r.0[2] = 1;
        r.0[4] = color;
        let m = &mut c.m;
        let (px, py) = (lh(m, this + 32), lh(m, this + 34));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[line, px, py, w, 1], 0x8001_f4c8);
        let m = &mut c.m;
        let (px, py) = (lh(m, this + 32), lh(m, this + 34));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[line, px, py.wrapping_add(8), w, 1], 0x8001_f4e4);
        let text = c.m.r32(this + 4);
        r.0[1] = w.wrapping_sub(8);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_f4f0);
        let m = &mut c.m;
        let (tx, ty) = (sx((m.r16(this + 32) as u32).wrapping_add(4)), sx((m.r16(this + 34) as u32).wrapping_add(10)));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, tx, ty, color, 0, 0], 0x8001_f524);
        let (v, min, max) = slider_value(c, &r, this, 0x8001_f548);
        let sw = sx(w.wrapping_sub(8));
        r.0[1] = sw;
        let m = &mut c.m;
        let (sx_, sy) = (sx((m.r16(this + 32) as u32).wrapping_add(4)), sx((m.r16(this + 34) as u32).wrapping_add(2)));
        let (h, sel) = (lh(m, this + 38), lh(m, this + 8));
        r.call_ptr_leaving(c, DRAW_SLIDER, &[v, min, max, sx_, sy, sw, h, 4, 4, 0, 0, sel, 0], 0x8001_f5b4);
    })
}

pub fn abi_inside_slider_draw(c: &mut Cpu) {
    inside_slider_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tInsideBoxTwoWaySlider::tInsideBoxTwoWaySlider` (FEMENUOPTIONS.CPP, 0x8001f5d4): a slider 5
/// high calibrating `ty`.
pub fn two_way_slider(c: &mut Cpu, this: u32, text: u32, data: u32, ty: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = ty;
        r.enter(c, 0x8002_4b18, 0x8001_f5f0, |c| femenu::left_right_slider(c, this, text, data));
        let m = &mut c.m;
        m.w32(this + ITEM_VPTR, TWO_WAY_SLIDER_VT);
        m.w16(this + 40, ty as u16);
        m.w16(this + 8, 0);
        m.w16(this + 38, 5);
        m.w32(this + 44, 0);
        this
    })
}

pub fn abi_two_way_slider(c: &mut Cpu) {
    c.r[2] = two_way_slider(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tInsideBoxTwoWaySlider::~tInsideBoxTwoWaySlider` (FEMENUOPTIONS.CPP, 0x8001f628).
pub fn abi_two_way_slider_dtor(c: &mut Cpu) {
    femenuextended::destroy(c, c.r[4], Some((ITEM_VPTR, TWO_WAY_SLIDER_VT)), 0x8002_4b74, 0x8001_f640, femenu::abi_left_right_slider_dtor);
}

/// `tInsideBoxTwoWaySlider::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001f650): accept starts the
/// calibration, the stop key (128) ends it (with sounds); calibrating it takes every key,
/// otherwise left and right.
pub fn two_way_slider_process_input(c: &mut Cpu, this: u32, key: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = key;
        r.0[1] = this;
        if c.m.r32(key) == femenu::KEY_ACCEPT {
            r.call_leaving(c, femenu::PLAY_FE_SFX, &[0], 0x8001_f67c);
            c.m.w32(this + 44, 1);
            c.m.w32(key, femenu::KEY_NONE);
        }
        if c.m.r32(key) == KEY_STOP {
            if c.m.r32(this + 44) == 0 {
                return;
            }
            r.call_leaving(c, femenu::PLAY_FE_SFX, &[1], 0x8001_f6b0);
            c.m.w32(this + 44, 0);
            c.m.w32(key, femenu::KEY_NONE);
        }
        let m = &mut c.m;
        let k = m.r32(key);
        if m.r32(this + 44) != 0 || k == femenu::KEY_LEFT || k == femenu::KEY_RIGHT {
            m.w32(key, femenu::KEY_NONE);
        }
    })
}

pub fn abi_two_way_slider_process_input(c: &mut Cpu) {
    two_way_slider_process_input(c, c.r[4], c.r[6]);
}

/// `tInsideBoxTwoWaySlider::Draw` (FEMENUOPTIONS.CPP, 0x8001f700): calibrating, the pad
/// measured first; then the slider at (`x`, `y`) `w` wide, two halves either side of a mark:
/// their lines, its text and both halves' sliders at the value.
pub fn two_way_slider_draw(c: &mut Cpu, this: u32, x: u32, y: u32, w: u32) {
    abi::frame(c, 88, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[x, y, w]);
        r.0[6] = this;
        if c.m.r32(this + 44) != 0 {
            r.enter_leaving(c, 0x8001_fa78, 0x8001_f748, |c| calibrate(c, this));
        }
        let half = ((w as i32) >> 1) as u32;
        r.0[2] = half;
        let m = &mut c.m;
        m.w16(this + 32, x as u16);
        m.w16(this + 34, y as u16);
        let sel = lh(m, this + 8);
        let line = r.call_leaving(c, CALC_FADE_VAL_SEL, &[0x0055_1E00, 0x00C8_3C1E, sel], 0x8001_f76c);
        r.0[0] = line;
        let sel = lh(&mut c.m, this + 8);
        let mark = r.call_leaving(c, CALC_FADE_VAL_SEL, &[0x0055_1E00, 0xBEBE, sel], 0x8001_f784);
        r.0[4] = mark;
        let sel = lh(&mut c.m, this + 8);
        let color = r.call_leaving(c, CALC_TEXT_FADE_UNSEL_TO_SEL, &[6, sel, 0], 0x8001_f798);
        let len = half.wrapping_sub(4);
        r.0[1] = len;
        r.0[3] = 1;
        r.0[5] = color;
        for (dx, dy, ra) in [(0, 0, 0x8001_f7bc), (8, 0, 0x8001_f7dc), (0, 8, 0x8001_f7f8), (9, 8, 0x8001_f81c)] {
            let m = &mut c.m;
            let (px, py) = (lh(m, this + 32), lh(m, this + 34));
            let px = if dx != 0 { px.wrapping_add(len).wrapping_add(dx) } else { px };
            r.call_ptr_leaving(c, DRAW_SQUARE, &[line, px, py.wrapping_add(dy), len, 1], ra);
        }
        let m = &mut c.m;
        let (px, py) = (lh(m, this + 32), lh(m, this + 34));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[mark, px.wrapping_add(len).wrapping_add(3), py, 2, 9], 0x8001_f840);
        let text = c.m.r32(this + 4);
        let sw = half.wrapping_sub(8);
        r.0[2] = sw;
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_f84c);
        let m = &mut c.m;
        let (tx, ty) = (sx((m.r16(this + 32) as u32).wrapping_add(4)), sx((m.r16(this + 34) as u32).wrapping_add(10)));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, tx, ty, color, 0, 0], 0x8001_f880);
        let (v, min, max) = slider_value(c, &r, this, 0x8001_f8a4);
        let sw = sx(sw);
        r.0[2] = sw;
        r.0[0] = 4;
        let m = &mut c.m;
        let (sx_, sy) = (sx((m.r16(this + 32) as u32).wrapping_add(1)), sx((m.r16(this + 34) as u32).wrapping_add(2)));
        let (h, sel) = (lh(m, this + 38), lh(m, this + 8));
        r.call_ptr_leaving(c, DRAW_SLIDER, &[v, min, max, sx_, sy, sw, h, 4, 4, 1, 0, sel, 0], 0x8001_f910);
        let (v, min, max) = slider_value(c, &r, this, 0x8001_f934);
        let m = &mut c.m;
        let sx_ = sx((m.r16(this + 32) as u32).wrapping_add(len).wrapping_add(10));
        let sy = sx((m.r16(this + 34) as u32).wrapping_add(2));
        let (h, sel) = (lh(m, this + 38), lh(m, this + 8));
        r.call_ptr_leaving(c, DRAW_SLIDER, &[v, min, max, sx_, sy, sw, h, 4, 4, 0, 0, sel, 0], 0x8001_f998);
    })
}

pub fn abi_two_way_slider_draw(c: &mut Cpu) {
    two_way_slider_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// How far from the middle (128) a byte of the pad's state is.
fn from_middle(v: u32) -> u32 {
    v.abs_diff(128)
}

/// The furthest from the middle of pad `pad`'s two axes at +`at` and +`at + 1` (its state, 32
/// bytes a pad).
fn stick_range(m: &mut Mem, pad: u32, at: u32) -> u32 {
    let p = PAD_INFO.wrapping_add(pad << 5);
    let x = m.r8(p + at) as i32;
    let y = m.r8(p + at + 1) as i32;
    let a = if 128 - x > 0 { 128 - x } else { x - 128 };
    let b = if 128 - y > 0 { 128 - y } else { y - 128 };
    b.max(a) as u32
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

/// `tInsideBoxTwoWaySlider::Calibrate` (FEMENUOPTIONS.CPP, 0x8001fa78): the help text for it;
/// the front end player's pad measured into the front end's calibration (per pad: +841 and
/// +843 the NeGcon's twist each way, +845 and +847 its buttons; +849..+855 the sticks'
/// ranges each way), each kept 10 from the other way's.
pub fn calibrate(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let arg = m.r32(CALIBRATE_HELP_ARG);
        let app = m.r32(FE_APP);
        let pad = m.r8(app + 557) as u32;
        let info = PAD_INFO.wrapping_add(pad << 5);
        r.0[1] = info;
        r.0[2] = pad;
        let help = r.call_leaving(c, CALIBRATE_HELP, &[arg], 0x8001_fab8);
        let m = &mut c.m;
        m.w32(BOX_HELP_TEXT, help);
        let cal = pad.wrapping_add(FRONT_END);
        let kind = m.r8(info + 5) as u32;
        let analog = kind == PAD_ANALOG || kind == PAD_DUAL_SHOCK;
        match lh(m, this + 40) as i32 {
            0 => {
                if kind == PAD_NEGCON {
                    let v = from_middle(m.r8(info + 8) as u32) as i32;
                    let floor = m.r8(cal + 843) as i32 + 10;
                    m.w8(cal + 841, v.max(floor) as u8);
                } else if analog {
                    let v = r.enter(c, 0x8001_f9c0, 0x8001_fb78, |c| stick_range(&mut c.m, pad, 8)) & 0xFF;
                    let m = &mut c.m;
                    let floor = m.r8(cal + 851) as i32 + 10;
                    m.w8(cal + 849, (v as i32).max(floor) as u8);
                }
            }
            1 => {
                if kind == PAD_NEGCON {
                    let v = from_middle(m.r8(info + 8) as u32) as i32;
                    let ceil = m.r8(cal + 841) as i32 - 10;
                    m.w8(cal + 843, v.min(ceil).max(0) as u8);
                } else if analog {
                    let v = r.enter(c, 0x8001_f9c0, 0x8001_fc2c, |c| stick_range(&mut c.m, pad, 8)) & 0xFF;
                    let m = &mut c.m;
                    let ceil = m.r8(cal + 849) as i32 - 10;
                    m.w8(cal + 851, (v as i32).min(ceil).max(0) as u8);
                }
            }
            2 => {
                if kind == PAD_NEGCON {
                    let v = m.r8(info + 9);
                    m.w8(cal + 845, v);
                } else if analog {
                    let v = r.enter(c, 0x8001_fa1c, 0x8001_fcb8, |c| stick_range(&mut c.m, pad, 10)) & 0xFF;
                    let m = &mut c.m;
                    let floor = m.r8(cal + 855) as i32 + 10;
                    m.w8(cal + 853, (v as i32).max(floor) as u8);
                }
            }
            3 => {
                if kind == PAD_NEGCON {
                    let v = m.r8(info + 10).max(10);
                    m.w8(cal + 847, v);
                } else if analog {
                    let v = r.enter(c, 0x8001_fa1c, 0x8001_fd48, |c| stick_range(&mut c.m, pad, 10)) & 0xFF;
                    let m = &mut c.m;
                    let ceil = m.r8(cal + 853) as i32 - 10;
                    m.w8(cal + 855, (v as i32).min(ceil).max(0) as u8);
                }
            }
            _ => {}
        }
    })
}

pub fn abi_calibrate(c: &mut Cpu) {
    calibrate(c, c.r[4]);
}

// tUserNameMenuItem: +28 the name being typed, +32 its longest, +34 the grid's column (0..5),
// +36 its row, from +38 the rows (9 bytes each: their letters; '!' and '@' end the name, '#'
// and '$' delete a letter, '&' and '^' a space, '-' an empty cell), +128 the player, +130 the
// fade (+132 its step, +136 whether it fades). The grid has `gNameRows` rows, drawn from
// `gNameGridTop`.

/// The grid's cell (`row`, `col`): its letter's place in the item.
fn cell(this: u32, row: u32, col: u32) -> u32 {
    this.wrapping_add(col).wrapping_add(row.wrapping_mul(9)).wrapping_add(38)
}

/// The current cell's letter.
fn current_letter(m: &mut Mem, this: u32) -> u8 {
    let (row, col) = (lh(m, this + 36), lh(m, this + 34));
    m.r8(cell(this, row, col))
}

/// `tUserNameMenuItem::tUserNameMenuItem` (FEMENUOPTIONS.CPP, 0x8001fd98): the grid at its
/// top left, its rows empty, no name.
pub fn user_name(c: &mut Cpu, this: u32, text: u32) -> u32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.enter(c, 0x8002_4754, 0x8001_fdac, |c| femenu::menu_item(&mut c.m, this, text));
        let m = &mut c.m;
        m.w32(this + ITEM_VPTR, USER_NAME_VT);
        m.w16(this + 36, 0);
        m.w16(this + 34, 0);
        for row in 0..10 {
            m.w8(cell(this, row, 0), 0);
        }
        m.w32(this + 28, 0);
        this
    })
}

pub fn abi_user_name(c: &mut Cpu) {
    c.r[2] = user_name(c, c.r[4], c.r[5]);
}

/// `CheckForCheats` (FEMENUOPTIONS.CPP, 0x8001fe0c): `name` (padded with zeros to 8) entered
/// as a cheat or a bonus code; whether it was one.
pub fn check_for_cheats(c: &mut Cpu, name: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = name;
        let len = r.call_leaving(c, STRLEN, &[name], 0x8001_fe24) as i32;
        for k in len.max(0)..8 {
            c.m.w8(name.wrapping_add(k as u32), 0);
        }
        r.0[1] = 0;
        if r.call_leaving(c, ACTIVATE_CHEAT, &[name], 0x8001_fe58) != 0 {
            return 1;
        }
        (r.call_leaving(c, ACTIVATE_BONUS, &[name], 0x8001_fe68) != 0) as u32
    })
}

pub fn abi_check_for_cheats(c: &mut Cpu) {
    c.r[2] = check_for_cheats(c, c.r[4]);
}

/// `tUserNameMenuItem::ProcessInput` (FEMENUOPTIONS.CPP, 0x8001fe8c): up and down change rows
/// (round), left and right columns (round; right past empty cells), with sounds; accept types
/// the letter (up to the longest), deletes one, adds a space or ends the name (a cheat, or
/// start or back by the menu's flag 0x100); the cursor kept off empty cells; the name's case
/// set as the records have it.
pub fn user_name_process_input(c: &mut Cpu, this: u32, key: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = key;
        let k = c.m.r32(key);
        let mut used = true;
        match k {
            femenu::KEY_UP | femenu::KEY_DOWN => {
                let m = &mut c.m;
                let rows = m.r16(NAME_ROWS);
                let sfx = if k == femenu::KEY_UP {
                    let row = m.r16(this + 36).wrapping_sub(1);
                    m.w16(this + 36, row);
                    if (row as i16) < 0 {
                        m.w16(this + 36, rows.wrapping_sub(1));
                    }
                    3
                } else {
                    let row = m.r16(this + 36).wrapping_add(1);
                    m.w16(this + 36, row);
                    if row as i16 >= rows as i16 {
                        m.w16(this + 36, 0);
                    }
                    4
                };
                r.call_leaving(c, femenu::PLAY_FE_SFX, &[sfx], 0x8001_ff60);
                if (c.m.r16(this + 34) as i16) < 0 {
                    c.m.w16(this + 34, 0);
                }
            }
            femenu::KEY_LEFT => {
                let m = &mut c.m;
                let col = m.r16(this + 34).wrapping_sub(1);
                m.w16(this + 34, col);
                if (col as i16) < 0 {
                    m.w16(this + 34, 5);
                }
                r.call_leaving(c, femenu::PLAY_FE_SFX, &[3], 0x8002_01b8);
            }
            femenu::KEY_RIGHT => {
                let m = &mut c.m;
                let col = m.r16(this + 34).wrapping_add(1);
                m.w16(this + 34, col);
                if col as i16 >= 6 {
                    m.w16(this + 34, 0);
                }
                r.call_leaving(c, femenu::PLAY_FE_SFX, &[4], 0x8001_ffd0);
                let m = &mut c.m;
                m.w32(key, femenu::KEY_NONE);
                // past the empty cells to the right
                while current_letter(m, this) == b'-' {
                    let col = m.r16(this + 34).wrapping_add(1);
                    m.w16(this + 34, col);
                    if col as i16 >= 6 {
                        m.w16(this + 34, 0);
                    }
                }
                let name = m.r32(this + 28);
                r.call_leaving(c, CASE_FOR_RECORDS, &[name], 0x8002_0238);
                return;
            }
            femenu::KEY_ACCEPT => {
                let name = c.m.r32(this + 28);
                let len = r.call_leaving(c, STRLEN, &[name], 0x8002_005c);
                let m = &mut c.m;
                let letter = current_letter(m, this);
                let len16 = sx(len);
                match letter {
                    b'#' | b'$' => {
                        if (len16 as i32) > 0 {
                            let name = m.r32(this + 28);
                            m.w8(len16.wrapping_add(name).wrapping_sub(1), 0);
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[21], 0x8002_01b8);
                        }
                    }
                    b'!' | b'@' => {
                        used = false;
                        let name = m.r32(this + 28);
                        if r.enter(c, 0x8001_fe0c, 0x8002_00d0, |c| check_for_cheats(c, name)) != 0 {
                            let name = c.m.r32(this + 28);
                            c.m.w8(name, 0);
                        } else {
                            let m = &mut c.m;
                            let app = m.r32(FE_APP);
                            let menu = m.r32(app + 4);
                            let k = if m.r32(menu) & 0x100 != 0 { femenu::KEY_START } else { femenu::KEY_BACK };
                            m.w32(key, k);
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[1], 0x8002_0188);
                        }
                    }
                    b'&' | b'^' => {
                        used = false;
                        if (len16 as i32) < lh(m, this + 32) as i32 {
                            let name = m.r32(this + 28);
                            m.w8(name.wrapping_add(len16), b' ');
                            let name = m.r32(this + 28);
                            m.w8(len16.wrapping_add(name).wrapping_add(1), 0);
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[21], 0x8002_0160);
                        } else {
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[1], 0x8002_0188);
                        }
                    }
                    _ => {
                        if (len16 as i32) < lh(m, this + 32) as i32 {
                            let name = m.r32(this + 28);
                            m.w8(name.wrapping_add(len16), letter);
                            let name = m.r32(this + 28);
                            m.w8(len16.wrapping_add(name).wrapping_add(1), 0);
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[21], 0x8002_01b8);
                        } else {
                            used = false;
                            r.call_leaving(c, femenu::PLAY_FE_SFX, &[1], 0x8002_0188);
                        }
                    }
                }
            }
            _ => used = false,
        }
        let m = &mut c.m;
        if used {
            m.w32(key, femenu::KEY_NONE);
        }
        // off the empty cells to the left
        while current_letter(m, this) == b'-' {
            let col = m.r16(this + 34).wrapping_sub(1);
            m.w16(this + 34, col);
            if (col as i16) < 0 {
                m.w16(this + 34, 5);
            }
        }
        let name = m.r32(this + 28);
        r.call_leaving(c, CASE_FOR_RECORDS, &[name], 0x8002_0238);
    })
}

pub fn abi_user_name_process_input(c: &mut Cpu) {
    user_name_process_input(c, c.r[4], c.r[6]);
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

/// `tUserNameMenuItem::Draw(bool)` (FEMENUOPTIONS.CPP, 0x800202dc): unless faded out, its text,
/// the name typed so far a letter at a time (the longest name's cells right-aligned from its
/// text's place, 20 apart), lines under the cells left, the cursor's letter (in the pulsing
/// yellow) on the grid; the two bars at its place.
pub fn user_name_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 136, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let text = c.m.r32(this + 4);
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8002_0318);
        c.m.w32(sp + 88, tx);
        let text = c.m.r32(this + 4);
        let ty = r.call_leaving(c, WORD_Y, &[text], 0x8002_0324);
        r.0[8] = ty;
        let shapes = c.m.r32(SHAPES).wrapping_add(960);
        r.0[6] = shapes;
        if c.m.r16(this + 130) != 128 {
            c.m.w8(sp + 33, 0);
            let fade = lh(&mut c.m, this + 130);
            let color = r.call_leaving(c, CALC_FADE_VAL, &[0xBEBE, fade], 0x8002_0354);
            r.0[3] = color;
            let fade = lh(&mut c.m, this + 130);
            let line = r.call_leaving(c, CALC_FADE_VAL, &[0xBEBE, fade], 0x8002_0364);
            r.0[7] = line;
            c.m.w32(sp + 44, line);
            let text = c.m.r32(this + 4);
            r.0[0] = 0;
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8002_0378);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), color, 0, 0], 0x8002_03a0);
            let name = c.m.r32(this + 28);
            let len = r.call_leaving(c, STRLEN, &[name], 0x8002_03ac);
            let m = &mut c.m;
            m.w16(sp + 92, len as u16);
            let longest = lh(m, this + 32);
            let left = tx.wrapping_add(78).wrapping_sub(longest.wrapping_mul(10));
            r.0[1] = left;
            let n = sx(len);
            if (n as i32) > 0 {
                let ly = (ty.wrapping_add(17)) << 16;
                r.0[5] = ly;
                r.0[4] = n;
                let mut k = 0u32;
                loop {
                    r.0[0] = k;
                    let m = &mut c.m;
                    let i = sx(k);
                    let lx = sx(left.wrapping_add(i.wrapping_mul(20)));
                    let name = m.r32(this + 28);
                    let ch = m.r8(name.wrapping_add(i));
                    m.w8(sp + 32, ch);
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[sp + 32, lx, ((ly as i32) >> 16) as u32, color, 1, 0], 0x8002_0430);
                    k = k.wrapping_add(1);
                    if (sx(k) as i32) >= n as i32 {
                        break;
                    }
                }
            }
            let mut k = c.m.r16(sp + 92) as u32;
            r.0[0] = k;
            if (sx(k) as i32) < lh(&mut c.m, this + 32) as i32 {
                let left = sx(left);
                r.0[1] = left;
                loop {
                    let lx = left.wrapping_add(sx(k).wrapping_mul(20));
                    r.call_ptr_leaving(c, DRAW_SQUARE, &[line, lx, ty.wrapping_add(25), 17, 1], 0x8002_049c);
                    k = k.wrapping_add(1);
                    r.0[0] = k;
                    if (sx(k) as i32) >= lh(&mut c.m, this + 32) as i32 {
                        break;
                    }
                }
            }
            // the cursor
            let m = &mut c.m;
            let (col, row) = (lh(m, this + 34), lh(m, this + 36));
            let cx = col.wrapping_mul(28);
            r.0[1] = cx.wrapping_add(258);
            let cy = lh(m, NAME_GRID_TOP).wrapping_add(row.wrapping_mul(15));
            r.0[0] = cy;
            if m.r16(this + 130) == 0 {
                let yellow = m.r32(PULSATE_YELLOW);
                m.w32(sp + 68, yellow);
                let letter = current_letter(m, this) as u32;
                let shape = match letter as u8 {
                    b'!' | b'@' => 78,
                    b'#' | b'$' => 79,
                    b'&' | b'^' => 80,
                    b'a'..=b'z' => letter - 55,
                    b'0'..=b'9' => letter + 20,
                    _ => r.enter(c, 0x8002_024c, 0x8002_05cc, |c| special_character(&mut c.m, letter)),
                };
                r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 16, cx.wrapping_add(253), cy.wrapping_sub(3), 0, 0, sp + 64], 0x8002_05f0);
            }
        }
        // the bars under its text and the name
        let m = &mut c.m;
        let tx = m.r32(sp + 88);
        let y1 = ty.wrapping_sub(3);
        let end = tx.wrapping_add(156);
        r.0[0] = y1;
        r.0[1] = end;
        let (w, fade) = (lh(m, shapes + 16), lh(m, this + 130));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[30, 8, end.wrapping_sub(w), y1, fade, 0, 0], 0x8002_0624);
        let m = &mut c.m;
        let (w, h) = (lh(m, shapes + 16), lh(m, shapes + 18));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, tx, y1, 156u32.wrapping_sub(w), h], 0x8002_0648);
        let m = &mut c.m;
        let y2 = ty.wrapping_add(12);
        let shapes = m.r32(SHAPES).wrapping_add(1056);
        r.0[0] = y2;
        r.0[6] = shapes;
        let (w, fade) = (lh(m, shapes + 16), lh(m, this + 130));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[33, 8, end.wrapping_sub(w), y2, fade, 0, 0], 0x8002_0680);
        let m = &mut c.m;
        let (w, h) = (lh(m, shapes + 16), lh(m, shapes + 18));
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, tx, y2, 156u32.wrapping_sub(w), h], 0x8002_06a4);
    })
}

pub fn abi_user_name_draw(c: &mut Cpu) {
    user_name_draw(c, c.r[4]);
}

/// `tUserNameMenuItem::TransitionOff` (FEMENUOPTIONS.CPP, 0x800206d4): fading out.
pub fn abi_user_name_transition_off(c: &mut Cpu) {
    c.m.w16(c.r[4] + 132, 30);
}

/// `tUserNameMenuItem::TransitionOn` (FEMENUOPTIONS.CPP, 0x800206e0): the grid's rows from
/// texts 508.., as many as the language has; the cursor on the end key (searched row by row
/// along each row); fading in from 128, unselected; the player's pending speech played.
pub fn user_name_transition_on(c: &mut Cpu, this: u32) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        for k in 0..10u32 {
            r.0[2] = k;
            r.0[0] = k;
            let word = r.call_leaving(c, TEXT_WORD, &[GRID_TEXTS + k], 0x8002_0710);
            r.call_leaving(c, SPRINTF, &[cell(this, k, 0), word], 0x8002_0728);
        }
        r.0[2] = 10;
        let m = &mut c.m;
        let lang = m.r8(FRONT_END + 0x340) as u32;
        for k in 0..12 {
            let b = m.r8(NAME_ROWS_BY_LANGUAGE + k);
            m.w8(sp + 16 + k, b);
        }
        let rows = m.r16(sp + 16 + 2 * lang);
        m.w16(NAME_ROWS, rows);
        r.0[3] = b'!' as u32;
        m.w16(this + 36, 0);
        let mut found = matches!(current_letter_at(m, this, 0), b'!' | b'@');
        r.0[2] = b'@' as u32;
        let mut row = 0u32;
        while !found && (row as i32) < lh(&mut c.m, NAME_ROWS) as i32 {
            let m = &mut c.m;
            m.w16(this + 34, 0);
            let r9 = lh(m, this + 36).wrapping_mul(9);
            r.0[0] = 0;
            if !matches!(m.r8(this.wrapping_add(r9).wrapping_add(38)), b'!' | b'@') {
                loop {
                    let r9 = lh(&mut c.m, this + 36).wrapping_mul(9);
                    let len = r.call_leaving(c, STRLEN, &[this.wrapping_add(r9).wrapping_add(38)], 0x8002_0808);
                    if (r.0[0] as i32) >= len as i32 {
                        break;
                    }
                    let m = &mut c.m;
                    let col = m.r16(this + 34).wrapping_add(1);
                    let s0 = sx(col as u32);
                    r.0[0] = s0;
                    let r9 = lh(m, this + 36).wrapping_mul(9);
                    m.w16(this + 34, col);
                    if matches!(m.r8(this.wrapping_add(s0).wrapping_add(r9).wrapping_add(38)), b'!' | b'@') {
                        break;
                    }
                }
            }
            let m = &mut c.m;
            let next = m.r16(this + 36).wrapping_add(1);
            m.w16(this + 36, next);
            row = sx(next as u32);
            found = matches!(current_letter(m, this), b'!' | b'@');
        }
        let m = &mut c.m;
        let row = m.r16(this + 36);
        m.w16(this + 130, 128);
        m.w16(this + 132, (-30i16) as u16);
        let player = lh(m, this + 128);
        r.0[2] = 0x8005_0000;
        m.w16(this + 8, 0);
        m.w16(this + 36, row.wrapping_sub(1));
        let app = m.r32(FE_APP);
        let speech = m.r32(app.wrapping_add(player << 2).wrapping_add(888));
        r.0[0] = u32::MAX;
        if speech != u32::MAX {
            r.call_leaving(c, PLAY_SPEECH, &[2, speech], 0x8002_08e4);
            let m = &mut c.m;
            let player = lh(m, this + 128);
            let app = m.r32(FE_APP);
            m.w32(app.wrapping_add(player << 2).wrapping_add(888), u32::MAX);
        }
    })
}

/// Row `row`'s letter at the current column.
fn current_letter_at(m: &mut Mem, this: u32, row: u32) -> u8 {
    let col = lh(m, this + 34);
    m.r8(this.wrapping_add(col).wrapping_add(row.wrapping_mul(9)).wrapping_add(38))
}

pub fn abi_user_name_transition_on(c: &mut Cpu) {
    user_name_transition_on(c, c.r[4]);
}

/// `tUserNameMenuItem::TransitionIsFinished` (FEMENUOPTIONS.CPP, 0x80020914): the fade at its
/// end (fading in, also while the memory card is busy).
pub fn abi_user_name_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let (fade, step) = (m.r16(this + 130) as i16, m.r16(this + 132) as i16);
    let fading = (step < 0 && fade > 0) || (step > 0 && (fade < 128 || m.r16(CARD_BUSY) != 0));
    m.w32(this + 136, fading as u32);
    c.r[2] = m.r32(this + 136) ^ 1;
}

/// `tUserNameMenuItem::UpdateTransition` (FEMENUOPTIONS.CPP, 0x80020980).
pub fn abi_user_name_update_transition(c: &mut Cpu) {
    fade_update_transition(c, c.r[4], USER_NAME_FADE, c.r[5], [0x8002_09dc, 0x8002_09e8]);
}

// tMemoryCardMenuItem: +40 its open state (0..256: opening while enabled, closing while not).

/// `tMemoryCardMenuItem::Draw(bool)` (FEMENUOPTIONS.CPP, 0x800209fc): opening or closing (8 a
/// frame); unless faded out or closed, its text fading in over the second half, and its bar
/// growing over the first half to 150 (by the fade).
pub fn memory_card_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let m = &mut c.m;
        let open = m.r16(this + 40);
        let closing = m.r32(this) & ITEM_DISABLED != 0;
        m.w16(this + 40, if closing { open.wrapping_sub(8) } else { open.wrapping_add(8) });
        let v = (m.r16(this + 40) as i16).clamp(0, 256);
        m.w16(this + 40, v as u16);
        let v = v as i32;
        let late = (v - 128).clamp(0, 128) as u32;
        let early = v.clamp(0, 128) as u32;
        r.0[0] = late;
        r.0[3] = early;
        if m.r16(this + FADE) == 128 {
            return;
        }
        r.0[5] = 128;
        if m.r16(this + 40) == 0 {
            return;
        }
        let text = m.r32(this + 4);
        let fade_in = 128 - late;
        r.0[0] = fade_in;
        let tx = r.call_leaving(c, WORD_X, &[text], 0x8002_0af8);
        r.0[4] = tx;
        let text = c.m.r32(this + 4);
        let ty = r.call_leaving(c, WORD_Y, &[text], 0x8002_0b04);
        r.0[6] = ty;
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let color = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, sel, fade], 0x8002_0b18);
        r.0[1] = color;
        let m = &mut c.m;
        let (sel, fade) = (lh(m, this + 8), lh(m, this + FADE));
        let bar = r.call_leaving(c, CALC_FADE_VAL_FADED, &[0x00C8_3C1E, 0xBEBE, sel, fade], 0x8002_0b34);
        let bar = r.call_leaving(c, CALC_FADE_VAL, &[bar, fade_in], 0x8002_0b40);
        r.0[0] = bar;
        let color = r.call_leaving(c, CALC_FADE_VAL, &[color, fade_in], 0x8002_0b50);
        r.0[1] = color;
        c.m.w32(sp + 36, bar);
        if color != 0 {
            let text = c.m.r32(this + 4);
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8002_0b68);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(tx), sx(ty), color, 0, 0], 0x8002_0b8c);
        }
        let m = &mut c.m;
        let grown = (early.wrapping_mul(150) as i32) / 128;
        let len = (grown.wrapping_mul(128 - lh(m, this + FADE) as i32)) / 128;
        let len = len as u32;
        r.0[0] = len;
        let by = ty.wrapping_sub(3);
        r.0[2] = by;
        let shapes = m.r32(SHAPES).wrapping_add(960);
        r.0[1] = shapes;
        let bx = tx.wrapping_add(len).wrapping_sub(lh(m, shapes + 16));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[30, 8, bx, by, 0, 0, 0], 0x8002_0c00);
        let m = &mut c.m;
        let w = lh(m, shapes + 16);
        if (w as i32) < len as i32 {
            let h = lh(m, shapes + 18);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, tx, by, len.wrapping_sub(w), h], 0x8002_0c2c);
        }
    })
}

pub fn abi_memory_card_draw(c: &mut Cpu) {
    memory_card_draw(c, c.r[4]);
}

/// `tInsideBoxControllerLeftRightSlider::ProcessInput` (FEMENUOPTIONS.CPP, 0x80020c54): left,
/// or right below the top, notes the controller changed; then the slider's.
pub fn controller_slider_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[this, key, player, command]);
        let k = c.m.r32(key);
        if k == femenu::KEY_LEFT || k == femenu::KEY_RIGHT {
            let mut changed = true;
            if k == femenu::KEY_RIGHT {
                let data = c.m.r32(this + 28);
                let (obj, f) = method(&mut c.m, data, femenu::ITERATOR_VPTR, femenu::VALUE);
                let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8002_0cc4);
                let data = c.m.r32(this + 28);
                changed = v & 0xFF != c.m.r8(data + 9) as u32;
            }
            if changed {
                let state = c.m.r32(CALIBRATE_HELP_ARG);
                c.m.w32(state + 368, 1);
            }
        }
        r.enter(c, 0x8002_4ba4, 0x8002_0cfc, |c| femenu::left_right_slider_process_input(c, this, player, key));
    })
}

pub fn abi_controller_slider_process_input(c: &mut Cpu) {
    controller_slider_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The destructors of the classes without one of their own: the base's (at `base_addr`),
/// the vtable left; `ra` its return.
fn destroy_as_base(c: &mut Cpu, base_addr: u32, ra: u32, base: fn(&mut Cpu)) {
    femenuextended::destroy(c, c.r[4], None, base_addr, ra, base);
}

/// `tInsideBoxControllerLeftRightSlider::~...` (FEMENUOPTIONS.CPP, 0x80020d18).
pub fn abi_controller_slider_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8001_f428, 0x8002_0d28, abi_inside_slider_dtor);
}

/// `tMemoryCardMenuItem::~...` (FEMENUOPTIONS.CPP, 0x80020d38).
pub fn abi_memory_card_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_539c, 0x8002_0d48, femenu::abi_go_to_menu_button_dtor);
}

/// `tMenuItemGoToMenuButtonFade::~...` (FEMENUOPTIONS.CPP, 0x80020d58).
pub fn abi_button_fade_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_539c, 0x8002_0d68, femenu::abi_go_to_menu_button_dtor);
}

/// `tUserNameMenuItem::~...` (FEMENUOPTIONS.CPP, 0x80020d78).
pub fn abi_user_name_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_4784, 0x8002_0d88, femenu::abi_menu_item_dtor);
}

/// `tMenuItemControllerLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020d98).
pub fn abi_controller_choice_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_4974, 0x8002_0da8, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemOnOffLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020db8).
pub fn abi_on_off_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_4974, 0x8002_0dc8, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemDisplayLeftRightChoice::~...` (FEMENUOPTIONS.CPP, 0x80020dd8).
pub fn abi_display_choice_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_4974, 0x8002_0de8, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemSlidingActivated::~...` (FEMENUOPTIONS.CPP, 0x80020df8).
pub fn abi_sliding_activated_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8001_d368, 0x8002_0e08, abi_sliding_menu_dtor);
}

/// `tMenuItemLeftRightFade::~...` (FEMENUOPTIONS.CPP, 0x80020e18).
pub fn abi_left_right_fade_dtor(c: &mut Cpu) {
    destroy_as_base(c, 0x8002_4974, 0x8002_0e28, femenu::abi_left_right_choice_dtor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, MENU2, NEW, NOTHING, TOP, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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
            m.w16(it + 32, [0, 128, pick(rng, 129), pick(rng, 200) - 30][pick(rng, 4) as usize] as u16);
            m.w16(it + 34, [0, 30, -30i32 as u32, pick(rng, 80) - 40][pick(rng, 4) as usize] as u16);
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
        m.w32(menu + 116, [0, SCREEN_FADE_FULL, 513, pick(rng, 600) - 20][pick(rng, 4) as usize]);
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
                if (k == femenu::KEY_ACCEPT && !active) || (k == femenu::KEY_START && active) {
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

    #[test]
    fn femenuoptions_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x0F91 + k as u64, shuffle, &args);
        }
    }
}
