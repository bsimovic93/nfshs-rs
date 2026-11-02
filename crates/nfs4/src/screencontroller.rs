//! SCREENCONTROLLER.CPP: the controller screen (`tScreenControllerConfig`): the player's pad
//! found out (a digital pad, a Dual Shock with its analog mode on or off, a NeGcon, none), its
//! picture swapped in and animated in or out (turning over between a pad and an analog one), its
//! buttons' labels and the arrows to them drawn letter by letter, the sticks' and the NeGcon's
//! twist shown on the picture, the rumble tried; the menu set for the kind (the configs, the
//! vibration's sliders).
//!
//! The screen: +40 its shape information, +100 shaking, +104 the shake's time, +106 the
//! actuators' bytes, +108 the pad kind last seen, +109 the kind and +110 the state the labels
//! show, +111 the state last checked, +112 animate out (+116 from state), +114 animate in (+118
//! to state), +120 the animation's start, +124 animating, +126 turning over (+128 its frame,
//! +130 from, +132 to, +134 its direction), +136 the state turned to, +138 swap at the end,
//! +140 the slide's direction (+142 from, +144 to, +146 the frame, +148 the state), +150 the
//! picture loaded (-1 none), +152 the dialog's answer, +156 the menu in, +160 slid in, +164
//! sliding out, +168 the labels' fade (+170 its step), +172 the letters shown, +176 the kind
//! changed, +180 a line's brightness, +184 the dialog asking whether the pad is a Dual Shock,
//! +352 the time an analog pad went quiet, +356 out fast, +360 the sound played, +364 the
//! rumble's slider (-1 none), +368 the rumble restarted, +372 the state (0 none, 1 digital, 2
//! analog off, 3 unknown, 4 Dual Shock, 5 NeGcon, 6 analog NeGcon-like), +376 the player.
//!
//! Written as FEMENU.CPP is (see there). libpad, the squares and `Flare_2DHalo` are the host's
//! (called by address); FRONT.CPP's functions too until it is rewritten.

use crate::call::{call, call64, call_addr, with_buffer};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PAD_INFO, TEXT_COLORS};
use crate::types::{DialogInteractive, DialogYesNo, DrawShapeExtended, MenuItemSlidingMenu, OptionsMenu, Screen, ShapeInformation, TextureShapeInfo};
use crate::{fescreen, fetextrender, psxfront};
use ps1rt::{Cpu, Mem};

const SHAKING: u32 = 100;
const SHAKE_TIME: u32 = 104;
const ACT: u32 = 106;
const KIND: u32 = 108;
const SHOWN_KIND: u32 = 109;
const SHOWN: u32 = 110;
const CHECKED: u32 = 111;
const OUT: u32 = 112;
const IN: u32 = 114;
const OUT_FROM: u32 = 116;
const IN_TO: u32 = 118;
const START: u32 = 120;
const ANIMATING: u32 = 124;
const TURNING: u32 = 126;
const TURN_FRAME: u32 = 128;
const TURN_FROM: u32 = 130;
const TURN_TO: u32 = 132;
const TURN_DIR: u32 = 134;
const TURNED: u32 = 136;
const SWAP: u32 = 138;
const DIR: u32 = 140;
const FROM: u32 = 142;
const TO: u32 = 144;
const FRAME: u32 = 146;
const SLID: u32 = 148;
const PICTURE: u32 = 150;
const ANSWER: u32 = 152;
const MENU_IN: u32 = 156;
const SLID_IN: u32 = 160;
const SLIDING_OUT: u32 = 164;
const FADE: u32 = 168;
const FADE_STEP: u32 = 170;
const LETTERS: u32 = 172;
const CHANGED: u32 = 176;
const BRIGHT: u32 = 180;
const DIALOG: u32 = 184;
const QUIET_AT: u32 = 352;
const FAST: u32 = 356;
const SOUNDED: u32 = 360;
const SLIDER: u32 = 364;
const RESTART: u32 = 368;
const STATE: u32 = 372;
const PLAYER: u32 = 376;
const SHAPES_INFO: u32 = 40;
const SCREEN_FLAGS: u32 = 58;
const VPTR: u32 = 96;
/// The states: none, digital, analog off (a Dual Shock in its digital mode), unknown, a Dual
/// Shock, a NeGcon, an analog pad like a NeGcon.
const NONE: u32 = 0;
const DIGITAL: u32 = 1;
const ANALOG_OFF: u32 = 2;
const UNKNOWN: u32 = 3;
const DUAL_SHOCK: u32 = 4;
const NEGCON: u32 = 5;
const ANALOG_NEGCON: u32 = 6;
/// libpad's stable pad; the front end's pads' kinds (+866) and rumble strengths (the big motor
/// +857, the small one +859), by player; the application's player (+557).
const PAD_STABLE: u32 = 6;
const PAD_KINDS: u32 = 866;
const BIG_MOTOR: u32 = 857;
const SMALL_MOTOR: u32 = 859;
const APP_PLAYER: u32 = 557;
/// In the menus: the options menu, the configs' sliding item and the menus it slides between
/// (digital, Dual Shock, NeGcon, analog NeGcon-like), the rumble's sliders shown.
const OPTIONS_MENU: u32 = 11116;
const SLIDING_ITEM: u32 = 11048;
const DIGITAL_MENU: u32 = 12256;
const DUAL_SHOCK_MENU: u32 = 11908;
const NEGCON_MENU: u32 = 12024;
const ANALOG_NEGCON_MENU: u32 = 12140;
const RUMBLE_SLIDERS: u32 = 11216;
/// The picture's swapped shapes; the slide's sound; "is it a Dual Shock?".
const PICTURE_SHAPES: u32 = 66;
const SLIDE_SOUND: u32 = 15;
const DUAL_SHOCK_QUESTION: u32 = 524;
/// FRONT.CPP's `Front_GetPSXPadValue`, `Front_ResetPSXController`.
const GET_PSX_PAD_VALUE: u32 = 0x8002_71B4;
const RESET_CONTROLLER: u32 = 0x8002_7104;

/// The menus (the options menu at +11116, the config menus' sliding item at +11048), the
/// pads' pictures' file names, their places, the rumble's wave, the actuators' alignment,
/// the pads' configurations (39 words a kind), the halo's size, target and step, the labels'
/// counts (by state and kind), indexes and lines, their texts, the arrows' lines.
const ALL_MENUS: u32 = 0x8005_1EE0;
const NAMES: u32 = 0x8005_2578;
const SPOTS: u32 = 0x8005_2568;
const WAVE: u32 = 0x8012_0F50;
const ALIGN: u32 = 0x8013_EB28;
const CONFIGS: u32 = 0x8012_20EC;
const HALO_SIZE: u32 = 0x8005_2D54;
const HALO_TARGET: u32 = 0x8005_2D58;
const HALO_STEP: u32 = 0x8005_2D5C;
const LABEL_COUNTS: u32 = 0x8005_2B48;
const LABEL_IDS: u32 = 0x8005_2B74;
const LABELS: u32 = 0x8005_2828;
const LINES: u32 = 0x8005_2594;
const TEXT_KIND: u32 = 0x8005_1AE5;
/// `"zpCtrl"`, `"aCtrl"`; the vtable.
const SHAPE_FILE: u32 = 0x8001_201C;
const SWAP_FILE: u32 = 0x8001_2014;
const VTABLE: u32 = 0x8001_2028;

const STRCMP: fn(&mut Cpu) = nfs4rc::f_800e6968;
const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const TEXT_PIXELS: fn(&mut Cpu) = nfs4rc::f_800e4a2c;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const GET_DEVICE: fn(&mut Cpu) = nfs4rc::f_800dd940;
const SETTRANS: fn(&mut Cpu) = nfs4rc::f_800f1440;
/// `Flare_2DHalo`, hooked (the host captures its primitives): called by address.
const HALO: u32 = 0x800C_ECD0;
const PLAY_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;
/// The float library's: int to float, float division, float to double, double product,
/// double to int.
const I2F: fn(&mut Cpu) = nfs4rc::f_800f6b80;
const FDIV: fn(&mut Cpu) = nfs4rc::f_800f7040;
const F2D: fn(&mut Cpu) = nfs4rc::f_800f6120;
const DMUL: fn(&mut Cpu) = nfs4rc::f_800f6c40;
const D2I: fn(&mut Cpu) = nfs4rc::f_800f7190;
/// The host's: libpad's `PadGetState`, `PadSetAct`, `PadSetActAlign`, `PadStartCom`;
/// `PSXDrawSquare`, `PSXDrawTransSquare`.
const PAD_GET_STATE: u32 = 0x800F_07FC;
const PAD_SET_ACT: u32 = 0x800F_0B08;
const PAD_SET_ACT_ALIGN: u32 = 0x800F_0A88;
const PAD_START_COM: u32 = 0x800F_07BC;
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

/// A halfword, sign-extended.
fn hs(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn hu(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

fn b(m: &mut Mem, at: u32) -> u32 {
    m.r8(at) as u32
}

fn menus(m: &mut Mem) -> u32 {
    m.r32(ALL_MENUS)
}

/// The player's pad's kind (the front end's, `+866` by player).
fn pad_kind(m: &mut Mem, this: u32) -> u32 {
    let p = m.r32(this + PLAYER);
    b(m, p.wrapping_add(FRONT_END) + PAD_KINDS)
}

/// The player's pad's state from libpad (the first or the second port).
fn pad_state(c: &mut Cpu, this: u32) -> u32 {
    let p = (0 < c.m.r32(this + PLAYER)) as u32;
    call_addr(c, PAD_GET_STATE, &[p << 4])
}

/// `x / 6` as the original's multiply makes it.
fn div6(x: u32) -> u32 {
    let hi = ((x as i32 as i64 * 0x2AAA_AAAB) >> 32) as i32;
    hi.wrapping_sub(x as i32 >> 31) as u32
}

/// `x / 127` (`x >> 3` of the multiply by 0x0FE03F81's high word).
fn div127(x: u32) -> u32 {
    let hi = ((x as i32 as i64 * 0x0FE0_3F81) >> 32) as i32;
    (hi >> 3).wrapping_sub(x as i32 >> 31) as u32
}

/// `x / 13` of a non-negative halfword.
fn div13(x: u32) -> u32 {
    let hi = ((x as i32 as i64 * 0x4EC4_EC4F) >> 32) as i32;
    (hi >> 2).wrapping_sub(x as i32 >> 31) as u32
}

/// `tScreenControllerConfig::ClearActuators` (SCREENCONTROLLER.CPP, 0x80043500).
pub fn clear_actuators(m: &mut Mem, this: u32) {
    m.w8(this + ACT, 0);
    m.w8(this + ACT + 1, 0);
}

pub fn abi_clear_actuators(c: &mut Cpu) {
    clear_actuators(&mut c.m, c.r[4]);
}

/// `tScreenControllerConfig::SetActuators` (SCREENCONTROLLER.CPP, 0x8004350c): the rumble at
/// `strength` for a while (64 frames from a restart): the small motor on by the wave, the big
/// one at the strength; off when the time's out.
pub fn set_actuators(m: &mut Mem, this: u32, strength: u32) {
    let time = if m.r32(this + RESTART) != 0 {
        m.w32(this + RESTART, 0);
        64
    } else {
        let t = b(m, this + SHAKE_TIME);
        if t == 0 {
            clear_actuators(m, this);
            return;
        }
        t - 1
    };
    m.w8(this + SHAKE_TIME, time as u8);
    if m.r8(this + SHAKE_TIME) == 0 {
        clear_actuators(m, this);
        return;
    }
    let k = (m.r32(GAME_TICKS) as i32 >> 2) as u32 & 0xFF;
    let wave = b(m, WAVE + k);
    m.w8(this + ACT + 1, strength as u8);
    m.w8(this + ACT, ((wave as i32) < strength as i32) as u8);
}

pub fn abi_set_actuators(c: &mut Cpu) {
    set_actuators(&mut c.m, c.r[4], c.r[5]);
}

/// `tScreenControllerConfig::TurnOffShakers` (SCREENCONTROLLER.CPP, 0x80043594).
pub fn turn_off_shakers(m: &mut Mem, this: u32) {
    m.w8(this + SHAKING, 0);
}

pub fn abi_turn_off_shakers(c: &mut Cpu) {
    turn_off_shakers(&mut c.m, c.r[4]);
}

/// `tScreenControllerConfig::ShakeIt` (SCREENCONTROLLER.CPP, 0x8004359c): with the pad
/// stable, its actuators set up once and given the bytes; else (gone) off.
pub fn shake_it(c: &mut Cpu, this: u32) {
    let port = c.m.r32(this + PLAYER) << 4;
    let st = call_addr(c, PAD_GET_STATE, &[port]);
    if st == PAD_STABLE {
        if c.m.r8(this + SHAKING) == 0 {
            c.m.w8(this + SHAKING, 1);
            call_addr(c, PAD_SET_ACT, &[port, this + ACT, 2]);
            call_addr(c, PAD_SET_ACT_ALIGN, &[port, ALIGN]);
        }
    } else {
        if (st as i32) < 4 {
            c.m.w8(this + SHAKING, 0);
        }
        clear_actuators(&mut c.m, this);
    }
}

pub fn abi_shake_it(c: &mut Cpu) {
    shake_it(c, c.r[4]);
}

/// `Controller_SetRamp` (SCREENCONTROLLER.CPP, 0x80043638): each player's three ramps on, but
/// those whose config's buttons are on the sticks (accelerate, brake, the third).
pub fn controller_set_ramp(c: &mut Cpu) {
    for k in 0..2u32 {
        let m = &mut c.m;
        let kind = hs(m, FRONT_END + 2 * k + 862);
        let config = match kind {
            35 => 0,
            83 | 115 => 1,
            _ => 2,
        };
        let fe = k.wrapping_add(FRONT_END);
        let pad = b(m, fe + PAD_KINDS);
        for at in [793, 795, 791] {
            m.w8(fe + at, 1);
        }
        let buttons = (pad.wrapping_mul(39).wrapping_add(config) << 2).wrapping_add(CONFIGS);
        for (off, flag) in [(0, 791), (24, 793), (36, 795)] {
            let button = c.m.r32(buttons + off);
            let v = call_addr(c, GET_PSX_PAD_VALUE, &[button, 0]);
            if call(c, GET_DEVICE, &[v]) == 1 {
                c.m.w8(fe + flag, 0);
            }
        }
    }
}

pub fn abi_controller_set_ramp(c: &mut Cpu) {
    controller_set_ramp(c);
}

/// `tScreenControllerConfig::AnimKeyPoints` (SCREENCONTROLLER.CPP, 0x80043784): the
/// animation's frame at its start or end (`end`), sliding in or out (`slide_in`): an analog
/// pad's 0, 20 or 40, the others' 0 or 24.
pub fn anim_key_points(m: &mut Mem, this: u32, slide_in: u32, end: u32) -> u32 {
    if b(m, this + STATE) == ANALOG_OFF {
        match (slide_in != 0, end != 0) {
            (_, false) => 20,
            (true, true) => 0,
            (false, true) => 40,
        }
    } else if end & slide_in != 0 {
        0
    } else if end != 0 {
        24
    } else if slide_in == 0 {
        0
    } else {
        24
    }
}

pub fn abi_anim_key_points(c: &mut Cpu) {
    c.r[2] = anim_key_points(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// The state's picture's file the same as the one loaded.
fn same_picture(c: &mut Cpu, this: u32) -> bool {
    let m = &mut c.m;
    let picture = hs(m, this + PICTURE);
    if picture == u32::MAX {
        return false;
    }
    let theirs = m.r32(NAMES + 4 * picture);
    let state = b(m, this + STATE);
    let ours = m.r32(NAMES + 4 * state);
    call(c, STRCMP, &[ours, theirs]) == 0
}

/// `tScreenControllerConfig::CheckConfigs` (SCREENCONTROLLER.CPP, 0x800437e8): a new state
/// (the pad reset for its kind): the picture animated out (none now), in (none before), or
/// out and in with a swap (another picture), the labels' state set; the same picture: the
/// kind changed (a Dual Shock's or the analog NeGcon's swapped in for the NeGcon's or the
/// unknown's). The same state but another pad kind (reset): turning the picture over, the way
/// the kinds go.
pub fn check_configs(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    if m.r8(this + STATE) != m.r8(this + CHECKED) {
        turn_off_shakers(m, this);
        let p = m.r32(this + PLAYER);
        let kind = pad_kind(m, this);
        call_addr(c, RESET_CONTROLLER, &[p, kind]);
        let m = &mut c.m;
        if hs(m, this + TURNING) != 0 {
            return;
        }
        let labels;
        if b(m, this + STATE) == NONE {
            // the pad gone: the picture out
            if hs(m, this + DIR) != 0 {
                return;
            }
            let (from, state) = (b(m, this + CHECKED), b(m, this + STATE));
            m.w16(this + OUT, 1);
            m.w16(this + OUT_FROM, from as u16);
            m.w8(this + CHECKED, state as u8);
            return;
        } else if b(m, this + CHECKED) == NONE {
            // a pad come: its picture in
            if hs(m, this + DIR) != 0 {
                return;
            }
            let state = b(m, this + STATE);
            labels = (hs(m, this + FADE) as i32) < 128;
            m.w16(this + SWAP, 1);
            m.w16(this + IN, 1);
            m.w16(this + IN_TO, state as u16);
        } else {
            if same_picture(c, this) {
                let m = &mut c.m;
                let checked = b(m, this + CHECKED);
                m.w32(this + CHANGED, 1);
                if checked == NEGCON || checked == UNKNOWN {
                    let state = b(m, this + STATE);
                    if state == DUAL_SHOCK || state == ANALOG_NEGCON {
                        swap_in_controller(c, this);
                    }
                }
                let m = &mut c.m;
                let state = b(m, this + STATE);
                m.w8(this + CHECKED, state as u8);
                return;
            }
            // another picture: out, swapped, in
            let m = &mut c.m;
            if hs(m, this + DIR) != 0 {
                return;
            }
            let (from, state) = (b(m, this + CHECKED), b(m, this + STATE));
            labels = (hs(m, this + FADE) as i32) < 128;
            for f in [OUT, SWAP, IN] {
                m.w16(this + f, 1);
            }
            m.w16(this + OUT_FROM, from as u16);
            m.w16(this + IN_TO, state as u16);
        }
        let m = &mut c.m;
        if !labels {
            let state = b(m, this + STATE);
            m.w8(this + SHOWN, state as u8);
        }
        if m.r8(this + SHOWN) == ANALOG_NEGCON as u8 {
            m.w8(this + SHOWN, NEGCON as u8);
        }
        let state = b(m, this + STATE);
        m.w8(this + CHECKED, state as u8);
        return;
    }
    // the same state, another pad kind: the picture turned over
    let p = m.r32(this + PLAYER);
    let kind = pad_kind(m, this);
    if kind == b(m, this + KIND) {
        return;
    }
    call_addr(c, RESET_CONTROLLER, &[p, kind]);
    let m = &mut c.m;
    if m.r32(this + OUT) == 0 && hs(m, this + TURNING) == 0 {
        let state = b(m, this + STATE);
        m.w16(this + TURNING, 1);
        m.w16(this + TURNED, state as u16);
        let (now, was) = (pad_kind(m, this), b(m, this + KIND));
        let up = (was < now && !(now == 2 && was == 0)) || (now == 0 && was == 2);
        let (a, dir) = if up { (1, 1u32) } else { (0, u32::MAX) };
        let from = anim_key_points(m, this, a, 1);
        m.w16(this + TURN_FROM, from as u16);
        let to = anim_key_points(m, this, a, 0);
        m.w16(this + TURN_TO, to as u16);
        m.w16(this + TURN_DIR, dir as u16);
        let f = hu(m, this + TURN_FROM);
        m.w16(this + TURN_FRAME, f as u16);
    }
    let m = &mut c.m;
    let kind = pad_kind(m, this);
    m.w8(this + KIND, kind as u8);
}

pub fn abi_check_configs(c: &mut Cpu) {
    check_configs(c, c.r[4]);
}

/// `tScreenControllerConfig::SwapInController` (SCREENCONTROLLER.CPP, 0x80043ad4): the
/// state's picture loaded (unless the one loaded has its file).
pub fn swap_in_controller(c: &mut Cpu, this: u32) {
    if same_picture(c, this) {
        return;
    }
    let m = &mut c.m;
    let state = b(m, this + STATE);
    let name = m.r32(NAMES + 4 * state);
    fescreen::async_load_swap_shape_file(c, Screen(this), name);
    let state = b(&mut c.m, this + STATE);
    c.m.w16(this + PICTURE, state as u16);
}

pub fn abi_swap_in_controller(c: &mut Cpu) {
    swap_in_controller(c, c.r[4]);
}

/// `tScreenControllerConfig::SetCurrentController` (SCREENCONTROLLER.CPP, 0x80043b60): the
/// player's pad's state from libpad: none, a digital pad (asking, the first time on the
/// screen, whether it's a Dual Shock with its analog mode off), a Dual Shock (stable: its
/// rumble's menu), analog (quiet a while or in the digital mode: unknown), a NeGcon (stable:
/// an analog NeGcon-like one; else the NeGcon's menu), others unknown; the menu set for it, the
/// rumble's sliders shown for a NeGcon kind on a Dual Shock.
pub fn set_current_controller(c: &mut Cpu, this: u32, first: u32) {
    let dialog = this + DIALOG;
    let title = call(c, TEXT_WORD, &[DUAL_SHOCK_QUESTION]);
    let m = &mut c.m;
    m.w32(dialog + 144, title);
    m.w32(dialog + 160, 525);
    m.w32(dialog + 164, 526);
    m.w16(dialog + 136, 1);
    let second = (0 < m.r32(this + PLAYER)) as u32;
    let info = (0u32.wrapping_sub(second) & 0x20).wrapping_add(PAD_INFO);
    let mut first = first;
    let mut menu = 0u32;
    let mut set = false;
    let mut unknown = false;
    if m.r8(info + 4) != 0 {
        // no pad
        let labels = (hs(m, this + FADE) as i32) < 128;
        m.w8(this + STATE, NONE as u8);
        if !labels {
            m.w16(this + PICTURE, 0);
        }
        set = true;
    } else {
        match b(m, info + 5) {
            // a digital pad (maybe a Dual Shock in its digital mode: asked once)
            0x23 => {
                m.w32(this + QUIET_AT, 0);
                let state = b(m, this + STATE);
                if first != 0 || state == ANALOG_OFF || state == DIGITAL {
                    menu = menus(m).wrapping_add(DIGITAL_MENU);
                } else {
                    let unasked = m.r32(dialog + 112) == 0 && m.r32(dialog + 156) == 0;
                    let answer = hs(m, this + ANSWER);
                    if unasked && answer == u32::MAX {
                        m.w8(this + STATE, NONE as u8);
                        m.w16(this + FADE, 128);
                        let v = crate::fedialog::run(c, DialogInteractive(dialog));
                        let m = &mut c.m;
                        m.w16(this + ANSWER, v as u16);
                        m.w8(this + STATE, if v << 16 == 0 { DIGITAL } else { ANALOG_OFF } as u8);
                        m.w16(this + ANSWER, answer as u16);
                        menu = menus(m).wrapping_add(DIGITAL_MENU);
                    } else {
                        set = true;
                        first = 1;
                        m.w16(this + FADE, 128);
                        m.w8(this + STATE, NONE as u8);
                    }
                }
            }
            // a Dual Shock: stable, its menu; quiet a while or gone, unknown
            0x41 => {
                if pad_state(c, this) == PAD_STABLE {
                    let m = &mut c.m;
                    m.w8(this + STATE, DUAL_SHOCK as u8);
                    m.w32(this + QUIET_AT, 0);
                    menu = menus(m).wrapping_add(DUAL_SHOCK_MENU);
                } else {
                    let mut lost = pad_state(c, this) == 2;
                    if !lost && c.m.r32(this + QUIET_AT) != 0 && pad_state(c, this) == 1 {
                        let m = &mut c.m;
                        lost = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + QUIET_AT)) as i32) >= 97;
                    }
                    let m = &mut c.m;
                    if lost || m.r8(this + STATE) == UNKNOWN as u8 {
                        unknown = true;
                    } else if m.r32(this + QUIET_AT) == 0 {
                        let t = m.r32(GAME_TICKS);
                        m.w32(this + QUIET_AT, t);
                    }
                }
            }
            // a NeGcon, or an analog pad like one
            0x53 | 0x73 => {
                m.w32(this + QUIET_AT, 0);
                if pad_state(c, this) == PAD_STABLE {
                    c.m.w8(this + STATE, ANALOG_NEGCON as u8);
                    menu = menus(&mut c.m).wrapping_add(ANALOG_NEGCON_MENU);
                } else if pad_state(c, this) == 2 {
                    c.m.w8(this + STATE, NEGCON as u8);
                    menu = menus(&mut c.m).wrapping_add(NEGCON_MENU);
                }
            }
            _ => unknown = true,
        }
    }
    if unknown {
        set = true;
        c.m.w32(this + QUIET_AT, 0);
        c.m.w8(this + STATE, UNKNOWN as u8);
    }
    if menu != 0 || set {
        let item = menus(&mut c.m).wrapping_add(SLIDING_ITEM);
        crate::femenuoptions::sliding_menu_set_menu(&mut c.m, MenuItemSlidingMenu(item), first, menu);
    }
    let m = &mut c.m;
    let state = b(m, this + STATE);
    let shown = state.wrapping_sub(NEGCON) < 2 && b(m, this + SHOWN_KIND) < 2;
    let all = menus(m);
    m.w16(all + RUMBLE_SLIDERS, shown as u16);
}

pub fn abi_set_current_controller(c: &mut Cpu) {
    set_current_controller(c, c.r[4], c.r[5]);
}

/// `tScreenControllerConfig::CalcAnimFrame` (SCREENCONTROLLER.CPP, 0x80043ed8): an analog
/// pad's picture's frame for an animation's frame (its turning over between 20 and 40 and
/// back); the others' the frame.
pub fn calc_anim_frame(m: &mut Mem, this: u32, f: u32) -> u32 {
    let fi = f as i32;
    if m.r8(this + STATE) != ANALOG_OFF as u8 {
        f
    } else if f == 0 || f == 20 || f == 40 {
        1
    } else if fi < 11 {
        (if fi < 10 { f } else { 9 }) + 2
    } else if fi < 20 {
        22u32.wrapping_sub(f)
    } else if fi >= 31 {
        56u32.wrapping_sub(f)
    } else {
        let g = f - 20;
        (if (g as i32) < 10 { g } else { 9 }) + 16
    }
}

pub fn abi_calc_anim_frame(c: &mut Cpu) {
    c.r[2] = calc_anim_frame(&mut c.m, c.r[4], c.r[5]);
}

/// The picture's drawing flags: the swapped shapes' table (`OWN_TABLE`), by its corner,
/// semi-transparent when fading.
fn picture_flags(fade: u32) -> u32 {
    if fade != 0 { 0x601 } else { 0x600 }
}

/// `tScreenControllerConfig::ActualDrawController` (SCREENCONTROLLER.CPP, 0x80043f64): the
/// pad's picture at `frame` (scaled, faded by `fade`, moved by `dx`, `dy`) over its base
/// (faded by `base`); an analog pad's buttons' plates and the dots where the labels' lines
/// meet them.
#[allow(clippy::too_many_arguments)]
pub fn actual_draw_controller(c: &mut Cpu, this: u32, frame: u32, fade: u32, base: u32, dx: u32, dy: u32) {
    if c.m.r16(this + PICTURE) == 0 {
        return;
    }
    with_buffer(c, 2 * DrawShapeExtended::SIZE, |c, buffer| {
        let (options, plates) = (DrawShapeExtended(buffer), DrawShapeExtended(buffer + DrawShapeExtended::SIZE));
        let m = &mut c.m;
        let info = m.r32(this + SHAPES_INFO);
        options.set_custom_shapes(m, TextureShapeInfo(info));
        let picture = hs(m, this + PICTURE);
        // the picture (scaled while it moves) and the analog pad's still frame over it
        let shape;
        let (mut x, mut y) = (0, 0);
        if frame != 0 || picture == 1 {
            let at = SPOTS.wrapping_add(2 * picture);
            let (x0, y0) = (b(m, at), b(m, at.wrapping_add(1)));
            psxfront::scale_shape_extended(c, frame + 1, picture_flags(fade), x0, y0, fade, 0, options.0);
            let m = &mut c.m;
            if hs(m, this + PICTURE) != 2 {
                shape = None;
            } else {
                shape = Some(0);
                (x, y) = (b(m, SPOTS + 4), b(m, SPOTS + 5));
            }
        } else {
            let at = SPOTS + (picture << 1);
            shape = Some(1);
            (x, y) = (b(m, at), b(m, at + 1));
        }
        if let Some(k) = shape {
            psxfront::draw_shape_extended(c, k, picture_flags(fade), x.wrapping_add(dx), y.wrapping_add(dy), fade, 0, options.0);
        }
        if shape.is_none() || hs(&mut c.m, this + PICTURE) != 2 {
            let flags = if base != 0 { 0x201 } else { 0x200 };
            psxfront::draw_shape_extended(c, 0, flags, 0, 0, base, 0, options.0);
            return;
        }
        // the analog pad's buttons' plates, and the dots where the lines meet them
        let f = hs(&mut c.m, this + FADE);
        let colour = crate::fefades::fade_out(0xC8_C8C8, f);
        plates.set_tint(&mut c.m, 0, colour as i32);
        for (shape, y) in [(26, 146), (27, 161), (28, 172), (29, 187)] {
            psxfront::draw_shape_extended(c, shape, 24, 158, y, 0, 1, plates.0);
        }
        for k in 48..54u32 {
            let line = LINES + 12 * k;
            let m = &mut c.m;
            let (x, y) = (hs(m, line + 4), hs(m, line + 6));
            call_addr(c, DRAW_SQUARE, &[colour, x.wrapping_sub(2), y.wrapping_sub(2), 5, 3]);
        }
    })
}

pub fn abi_actual_draw_controller(c: &mut Cpu) {
    let (dx, dy) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    actual_draw_controller(c, c.r[4], c.r[5], c.r[6], c.r[7], dx, dy);
}

/// The picture's slide's key frames (`AnimKeyPoints` of `slide_in` at its start and end): the
/// start noted.
fn key_points(m: &mut Mem, this: u32, slide_in: u32) -> u32 {
    let from = anim_key_points(m, this, slide_in, 1);
    m.w16(this + FROM, from as u16);
    anim_key_points(m, this, slide_in, 0)
}

/// A stick's lean as one of 10 frames: the larger of its two axes' (`x`, `y` bytes about 128),
/// by 13 (`pos`'s frames one way, `neg`'s the other).
fn lean(m: &mut Mem, at: u32, pos: u32, neg: u32) -> u32 {
    let dx = 128u32.wrapping_sub(b(m, at + 1));
    let dy = 128u32.wrapping_sub(b(m, at));
    let a1 = if ((dx as i32).abs()) < (dy as i32).abs() { dy } else { dx };
    let v = sx(a1);
    if (v as i32) >= 0 { div13(v).wrapping_add(pos) } else { div13(0u32.wrapping_sub(v)).wrapping_add(neg) }
}

/// The rumble menu's motor for the state (a Dual Shock's or the analog NeGcon's), when the
/// rumble's on (`menus + 11124`): 0 the big one, 1 the small one.
fn rumble_motor(m: &mut Mem, this: u32, motor: u32) -> bool {
    let state = b(m, this + STATE);
    let all = menus(m);
    ((state == DUAL_SHOCK && hs(m, all + 11916) == motor) || (state == ANALOG_NEGCON && hs(m, all + 12148) == motor)) && hs(m, all + 11124) == 1
}

/// `tScreenControllerConfig::DrawController` (SCREENCONTROLLER.CPP, 0x800441cc): the rumble
/// tried with its menu's slider on (the big motor's or the small one's strength, the picture
/// shaking with it), else off; the picture's shapes uploaded once loaded; then: a NeGcon's
/// halo pulsing while settled; the picture sliding out, in (swapping between), or turning over
/// (with their sound) at its frame and fade; a NeGcon's or a stick's lean on the picture; an
/// analog pad's twist; the picture still; nothing (the labels reset).
pub fn draw_controller(c: &mut Cpu, this: u32) {
    // the rumble: the big motor's strength, else the small one's; the shake's sizes by it
    let (mut shake_x, mut shake_y) = (0u32, 0u32);
    let mut on = false;
    let m = &mut c.m;
    if rumble_motor(m, this, 0) {
        let p = m.r32(this + PLAYER);
        if m.r8(p.wrapping_add(FRONT_END) + BIG_MOTOR) != 0 {
            if m.r16(this + SLIDER) != 0 {
                m.w16(this + SLIDER, 0);
                m.w32(this + RESTART, 1);
            }
            let s = b(m, p.wrapping_add(FRONT_END) + BIG_MOTOR);
            set_actuators(m, this, s << 1);
            let v = b(m, p.wrapping_add(FRONT_END) + BIG_MOTOR);
            (shake_x, shake_y) = ((v >> 5) + 2, (v >> 6) + 2);
            on = true;
        }
    }
    if !on && rumble_motor(m, this, 1) {
        let p = m.r32(this + PLAYER);
        let s = b(m, p.wrapping_add(FRONT_END) + SMALL_MOTOR);
        if s != 0 {
            set_actuators(m, this, s << 1);
            let p = m.r32(this + PLAYER);
            let v = b(m, p.wrapping_add(FRONT_END) + SMALL_MOTOR);
            (shake_x, shake_y) = ((v >> 5) + 2, (v >> 6) + 2);
            if hs(m, this + SLIDER) != 1 {
                m.w16(this + SLIDER, 1);
                m.w32(this + RESTART, 1);
            }
            on = true;
        }
    }
    if !on {
        m.w16(this + SLIDER, 0xFFFF);
        clear_actuators(m, this);
    }
    shake_it(c, this);
    let (mut dx, mut dy) = (0u32, 0u32);
    let sx16 = shake_x << 16;
    let size_x = (sx16 as i32 >> 16) as u32;
    if size_x != 0 || shake_y != 0 {
        let v = call(c, RAND, &[]);
        let rem = if size_x == 0 { 0 } else { (v as i32).wrapping_rem(size_x as i32) };
        dx = (rem as u32).wrapping_sub((sx16 as i32 >> 17) as u32);
        let v = call(c, RAND, &[]);
        let sy16 = shake_y << 16;
        let d = sy16 as i32 >> 16;
        let rem = if d == 0 { 0 } else { (v as i32).wrapping_rem(d) };
        dy = (rem as u32).wrapping_sub((sy16 as i32 >> 17) as u32);
    }
    // the picture's shapes uploaded once its file's in
    fescreen::is_shape_file_loaded(c, Screen(this), ShapeInformation(this + SHAPES_INFO));
    if c.m.r32(this + SHAPES_INFO + 4) != 0 {
        fescreen::upload_swap_shapes(c, Screen(this), PICTURE_SHAPES);
        return;
    }
    let m = &mut c.m;
    if m.r16(this + SCREEN_FLAGS) & 1 == 0 {
        m.w16(this + ANIMATING, 0);
        return;
    }
    halo(c, this, dx, dy);
    let swap = slides(c, this);
    if swap {
        swap_in_controller(c, this);
        c.m.w16(this + SWAP, 0);
    }
    let m = &mut c.m;
    if m.r16(this + DIR) != 0 {
        slide(c, this);
        return;
    }
    if m.r16(this + TURNING) != 0 {
        turn_over(c, this);
        return;
    }
    // a NeGcon's or a stick's lean on the picture
    if b(m, this + STATE).wrapping_sub(NEGCON) < 2 {
        let all = menus(m);
        let mut shape = 0u32;
        if m.r32(all + 11564) != 0 || m.r32(all + 11632) != 0 {
            let p = m.r32(this + PLAYER);
            shape = lean(m, PAD_INFO.wrapping_add(p << 5) + 10, 26, 36);
        }
        let all = menus(m);
        if m.r32(all + 11428) != 0 || m.r32(all + 11496) != 0 {
            let p = m.r32(this + PLAYER);
            shape = lean(m, PAD_INFO.wrapping_add(p << 5) + 8, 46, 56);
        }
        let shape = shape & 0xFF;
        if shape != 0 {
            let at = SPOTS.wrapping_add(2 * hs(m, this + PICTURE));
            let (x, y) = (b(m, at), b(m, at + 1));
            let info = m.r32(this + SHAPES_INFO);
            with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
                DrawShapeExtended(options).set_custom_shapes(&mut c.m, TextureShapeInfo(info));
                psxfront::draw_shape_extended(c, shape, 0x600, x, y, 0, 0, options);
            });
        }
    }
    // an analog pad's twist
    let m = &mut c.m;
    let state = b(m, this + STATE);
    if state.wrapping_sub(DIGITAL) < 2 {
        let all = menus(m);
        if m.r32(all + 11700) != 0 || m.r32(all + 11768) != 0 {
            let p = m.r32(this + PLAYER);
            let a0 = b(m, PAD_INFO.wrapping_add(p << 5) + 8).wrapping_sub(128);
            let frame = if (a0 as i32) >= 11 {
                if state == ANALOG_OFF { 2u32.wrapping_add(div127(a0.wrapping_mul(13))) } else { 26u32.wrapping_add(div127(a0 << 3)) }
            } else if (a0 as i32) < -10 {
                let a0 = 0u32.wrapping_sub(a0);
                if state == ANALOG_OFF { 16u32.wrapping_add(div127(a0.wrapping_mul(13))) } else { 35u32.wrapping_add(div127(a0 << 3)) }
            } else {
                (state == ANALOG_OFF) as u32
            };
            actual_draw_controller(c, this, frame, 0, 0, 0, 0);
            return;
        }
    }
    let m = &mut c.m;
    if m.r8(this + STATE) != NONE as u8 {
        actual_draw_controller(c, this, 0, 0, 0, sx(dx), sx(dy));
    } else {
        m.w8(this + SHOWN, 0);
        m.w16(this + FADE, 128);
        m.w16(this + LETTERS, 0);
    }
}

pub fn abi_draw_controller(c: &mut Cpu) {
    draw_controller(c, c.r[4]);
}

/// A NeGcon's halo round its twist, pulsing (between 3750 and 4750, 55 a step) while all's
/// settled, else shrinking away; two of them, the picture's shake with them.
fn halo(c: &mut Cpu, this: u32, dx: u32, dy: u32) {
    let m = &mut c.m;
    let settled = b(m, this + STATE).wrapping_sub(NEGCON) < 2 && m.r16(this + TURNING) == 0 && m.r32(this + OUT) == 0 && m.r16(this + DIR) == 0;
    if settled {
        let size = m.r32(HALO_SIZE).wrapping_add(250);
        m.w32(HALO_SIZE, size);
        let target = m.r32(HALO_TARGET);
        if !((size as i32) < target as i32) {
            let t = target.wrapping_add(m.r32(HALO_STEP));
            m.w32(HALO_TARGET, t);
            if !((t as i32) < 4751) {
                m.w32(HALO_TARGET, 4750);
                m.w32(HALO_STEP, (-55i32) as u32);
            }
            if (m.r32(HALO_TARGET) as i32) < 3750 {
                m.w32(HALO_TARGET, 3750);
                m.w32(HALO_STEP, 55);
            }
            let t = m.r32(HALO_TARGET);
            m.w32(HALO_SIZE, t);
        }
    } else {
        let size = m.r32(HALO_SIZE);
        m.w32(HALO_TARGET, 3750);
        let s = size.wrapping_sub(250);
        m.w32(HALO_SIZE, s);
        if (s as i32) < 0 {
            m.w32(HALO_SIZE, 0);
        }
    }
    if (c.m.r32(HALO_SIZE) as i32) > 0 {
        let (ox, oy) = (sx(dx).wrapping_add(126), sx(dy));
        for k in 0..2 {
            let m = &mut c.m;
            let at = SPOTS + 2 * b(m, this + STATE);
            let size = m.r32(HALO_SIZE);
            let x = b(m, at).wrapping_add(ox);
            let y = b(m, at + 1).wrapping_add(k + 63).wrapping_add(oy);
            let q = (if (size as i32) < 0 { size.wrapping_add(3) } else { size } as i32 >> 2) as u32;
            call_addr(c, HALO, &[x, y, size, q, 21]);
        }
    }
}

/// The picture's slides started: out (asked, none going), in (asked, none going or after the
/// out: swapping the picture between when asked); otherwise asks dropped. Whether the picture
/// is to be swapped now.
fn slides(c: &mut Cpu, this: u32) -> bool {
    let m = &mut c.m;
    if m.r16(this + OUT) != 0 && m.r16(this + DIR) == 0 {
        let from = hu(m, this + OUT_FROM);
        m.w16(this + OUT, 0);
        m.w16(this + DIR, 0xFFFF);
        m.w16(this + SLID, from as u16);
        let to = key_points(m, this, 0);
        let from = hu(m, this + FROM);
        m.w16(this + TO, to as u16);
        m.w16(this + FRAME, from as u16);
        return false;
    }
    if m.r16(this + IN) != 0 {
        let dir = hs(m, this + DIR);
        if dir == 0 || (dir == 1 && m.r16(this + OUT) == 0) {
            let to_state = hu(m, this + IN_TO);
            m.w16(this + IN, 0);
            m.w16(this + DIR, 1);
            m.w16(this + SLID, to_state as u16);
            let to = key_points(m, this, 1);
            let from = hu(m, this + FROM);
            let swapping = m.r16(this + SWAP) != 0;
            m.w16(this + TO, to as u16);
            m.w16(this + FRAME, from as u16);
            if swapping {
                if dir == 0 {
                    return true;
                }
                swap_in_controller(c, this);
                c.m.w16(this + ANIMATING, 0);
                c.m.w16(this + SWAP, 0);
            }
            return false;
        }
    }
    let mut pending = None;
    if m.r16(this + OUT) != 0 {
        pending = Some(m.r16(this + SWAP));
        m.w16(this + OUT, 0);
    } else if m.r16(this + IN) != 0 && hs(m, this + DIR) == 1 {
        pending = Some(m.r16(this + SWAP));
        m.w16(this + IN, 0);
    }
    pending.is_some_and(|v| v != 0)
}

/// The picture sliding (a frame every 6 ticks, its sound 11 ticks in going in), faded by how
/// far it has gone (twice as fast going out quickly); the labels reset once in.
fn slide(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    if m.r16(this + ANIMATING) == 0 {
        let t = m.r32(GAME_TICKS);
        m.w16(this + ANIMATING, 1);
        m.w32(this + SOUNDED, 0);
        m.w32(this + START, t);
    }
    if (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32) >= 11 && m.r32(this + SOUNDED) == 0 {
        let dir = hs(m, this + DIR);
        if dir == 1 {
            call(c, PLAY_SFX, &[SLIDE_SOUND]);
            c.m.w32(this + SOUNDED, dir);
        }
    }
    let m = &mut c.m;
    let t = div6(m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)));
    let dir = hs(m, this + DIR);
    let frame = hu(m, this + FROM).wrapping_add(t.wrapping_mul(dir));
    m.w16(this + FRAME, frame as u16);
    let (to, cur) = (hs(m, this + TO) as i32, hs(m, this + FRAME) as i32);
    if (to < cur && dir == 1) || (cur < to && hs(m, this + DIR) == u32::MAX) {
        let to = hu(m, this + TO);
        m.w16(this + ANIMATING, 0);
        m.w16(this + FRAME, to as u16);
        if hs(m, this + DIR) == 1 {
            m.w16(this + LETTERS, 0);
            m.w16(this + FADE, 128);
        }
    }
    let from = hs(m, this + FROM);
    let num = (hs(m, this + FRAME).wrapping_sub(from) << 7) as i32;
    let den = hs(m, this + TO).wrapping_sub(from) as i32;
    let q = if den == 0 { 0 } else { num.wrapping_div(den) };
    let mut fade = (q.wrapping_abs() as u32) << 1;
    if m.r32(this + FAST) != 0 {
        fade <<= 1;
    }
    if !((fade as i32) < 129) {
        fade = 128;
    }
    if hs(m, this + DIR) == 1 {
        fade = 128u32.wrapping_sub(fade);
    }
    if m.r16(this + ANIMATING) == 0 {
        m.w16(this + DIR, 0);
    }
    let f = hs(m, this + FRAME);
    let frame = calc_anim_frame(m, this, f);
    actual_draw_controller(c, this, frame, fade, fade, 0, 0);
}

/// The picture turning over (a frame every 6 ticks, with its sound), its base fading with the
/// turn's progress (the game's floats: out and back over the turn).
fn turn_over(c: &mut Cpu, this: u32) {
    if c.m.r16(this + ANIMATING) == 0 {
        call(c, PLAY_SFX, &[SLIDE_SOUND]);
        let m = &mut c.m;
        let t = m.r32(GAME_TICKS);
        m.w16(this + ANIMATING, 1);
        m.w32(this + START, t);
    }
    let m = &mut c.m;
    let t = div6(m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)));
    let dir = hs(m, this + TURN_DIR);
    let frame = hu(m, this + TURN_FROM).wrapping_add(t.wrapping_mul(dir));
    m.w16(this + TURN_FRAME, frame as u16);
    let (to, cur) = (hs(m, this + TURN_TO) as i32, hs(m, this + TURN_FRAME) as i32);
    if (to < cur && dir == 1) || (cur < to && hs(m, this + TURN_DIR) == u32::MAX) {
        let to = hu(m, this + TURN_TO);
        m.w16(this + TURNING, 0);
        m.w16(this + ANIMATING, 0);
        m.w16(this + TURN_FRAME, to as u16);
    }
    // the progress: (now - start) / (span * 6) * 256, as floats
    let dt = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START));
    let a = call(c, I2F, &[dt]);
    let m = &mut c.m;
    let dir = hs(m, this + TURN_DIR);
    let span = hs(m, this + TURN_TO).wrapping_sub(hs(m, this + TURN_FROM)).wrapping_mul(dir.wrapping_mul(6));
    let bf = call(c, I2F, &[span]);
    let q = call(c, FDIV, &[a, bf]);
    let (lo, hi) = call64(c, F2D, &[q]);
    let (lo, hi) = call64(c, DMUL, &[lo, hi, 0, 0x4070_0000]);
    let mut fade = call(c, D2I, &[lo, hi]) as i32;
    if fade >= 257 {
        fade = 256;
    }
    if fade >= 129 {
        fade = 256 - fade;
    }
    fade += 10;
    let fade = fade.clamp(0, 128) as u32;
    let m = &mut c.m;
    let f = hs(m, this + TURN_FRAME);
    let frame = calc_anim_frame(m, this, f);
    actual_draw_controller(c, this, frame, 0, fade, 0, 0);
}

/// `tScreenControllerConfig::HorzVertLine` (SCREENCONTROLLER.CPP, 0x80044edc): one of an
/// arrow's two lines (its first or second leg, `second`), two pixels wide, drawn translucent
/// as far as its brightness says, a white tip at its end.
pub fn horz_vert_line(c: &mut Cpu, this: u32, line: u32, second: u32) {
    let m = &mut c.m;
    // the leg: from (x0, y0) to (x1, y1)
    let (mut x1, mut x0, mut y1, mut y0) = (hs(m, line + 8), sx(hu(m, line + 4)), hs(m, line + 10), sx(hu(m, line + 6)));
    if second != 0 {
        (x1, y1) = (x0, y0);
        (x0, y0) = (hs(m, line), hs(m, line + 2));
    }
    let mut colour = 0x1E_1E1E;
    if (hs(m, this + FADE_STEP) as i32) > 0 {
        let f = hs(m, this + FADE);
        colour = crate::fefades::fade_out(0x1E_1E1E, f);
    }
    let m = &mut c.m;
    let lt = |a: u32, b: u32| (a as i32) < b as i32;
    let mul6 = |a: u32, b: u32| ((sx(a) as i32).wrapping_mul(b as i32) >> 6) as u32;
    if x0 == x1 {
        // vertical
        let s4 = if second == 0 { (!lt(y1, y0)) as u32 } else { 0 };
        let tip = if second == 1 { (!lt(y1, y0)) as u32 } else { 0 };
        let mut len = y1.wrapping_sub(y0);
        let bright = hs(m, this + BRIGHT);
        if bright != 64 {
            len = mul6(len, bright);
            let t = sx(len);
            let e = (t as i32 >> 3) as u32;
            let y = y1.wrapping_sub(t.wrapping_sub(e).wrapping_sub(s4).wrapping_add(tip));
            call_addr(c, DRAW_TRANS_SQUARE, &[0xFF_FFFF, x1, y, 2, 0u32.wrapping_sub(e), 2]);
        }
        let h = 0u32.wrapping_sub(sx(len).wrapping_sub(s4).wrapping_add(tip));
        call_addr(c, DRAW_TRANS_SQUARE, &[colour, x1, y1.wrapping_add(tip), 2, h, 2]);
    } else {
        // horizontal
        let tip = ((!lt(x1, x0)) as u32) << 1;
        let mut len = x1.wrapping_sub(x0);
        let bright = hs(m, this + BRIGHT);
        if bright != 64 {
            len = mul6(len, bright);
            let a1 = sx(len);
            if !lt(tip, (a1 as i32).unsigned_abs()) {
                return;
            }
            let e = (a1 as i32 >> 3) as u32;
            let x = x1.wrapping_sub(a1.wrapping_sub(e)).wrapping_add(tip);
            call_addr(c, DRAW_TRANS_SQUARE, &[0xFF_FFFF, x, y0, 0u32.wrapping_sub(e.wrapping_add(tip)), 1, 2]);
        }
        let a3 = sx(len);
        if lt(tip, (a3 as i32).unsigned_abs()) {
            call_addr(c, DRAW_TRANS_SQUARE, &[colour, x1.wrapping_add(tip), y1, 0u32.wrapping_sub(a3.wrapping_add(tip)), 1, 2]);
        }
    }
}

pub fn abi_horz_vert_line(c: &mut Cpu) {
    horz_vert_line(c, c.r[4], c.r[5], c.r[6]);
}

/// `tScreenControllerConfig::DrawArrow` (SCREENCONTROLLER.CPP, 0x80045128): an arrow from a
/// label to its button: its first leg (if it has one) and its second, as bright as the
/// labels' fade lets them, translucent.
pub fn draw_arrow(c: &mut Cpu, this: u32, line: u32) {
    c.m.w16(this + BRIGHT, 0);
    call(c, SETTRANS, &[1]);
    crate::mmeffect::set_abr_mode(c, 0);
    if c.m.r16(line) != 0 {
        let m = &mut c.m;
        if (hs(m, this + FADE_STEP) as i32) >= 0 {
            m.w16(this + BRIGHT, 64);
        } else {
            let v = 64u32.wrapping_sub(hu(m, this + FADE));
            m.w16(this + BRIGHT, v as u16);
            if (v as u16 as i16) < 0 {
                m.w16(this + BRIGHT, 0);
            }
        }
        horz_vert_line(c, this, line, 1);
    }
    let m = &mut c.m;
    let v = if (hs(m, this + FADE_STEP) as i32) >= 0 {
        64
    } else {
        let v1 = 128u32.wrapping_sub(hu(m, this + FADE));
        m.w16(this + BRIGHT, v1 as u16);
        let a0 = sx(v1) as i32;
        if a0 >= 64 {
            64
        } else if a0 >= 0 {
            v1
        } else {
            0
        }
    };
    m.w16(this + BRIGHT, v as u16);
    horz_vert_line(c, this, line, 0);
    crate::mmeffect::set_abr_mode(c, 1);
    call(c, SETTRANS, &[0]);
}

pub fn abi_draw_arrow(c: &mut Cpu) {
    draw_arrow(c, c.r[4], c.r[5]);
}

/// The options menu's transition finished.
fn transition_is_finished(c: &mut Cpu) -> bool {
    let opts = menus(&mut c.m).wrapping_add(OPTIONS_MENU);
    crate::femenuoptions::options_menu_transition_is_finished(c, OptionsMenu(opts)) != 0
}

/// `tScreenControllerConfig::DrawBackground` (SCREENCONTROLLER.CPP, 0x80045228): the picture
/// slid in once the menu's in (or out as it goes), the pad checked while not sliding out, the
/// picture drawn; the screen's shapes faded with the menu.
pub fn draw_background(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let opts = menus(m).wrapping_add(OPTIONS_MENU);
    let v = (m.r32(opts + 116) as i32 >> 1) as u32;
    let fade = if (sx(v) as i32) < 129 { v } else { 128 };
    if m.r32(this + MENU_IN) == 0 && transition_is_finished(c) {
        c.m.w32(this + MENU_IN, 1);
    }
    let m = &mut c.m;
    if (sx(fade) as i32) < 129 && m.r32(this + SLID_IN) == 0 && m.r8(this + STATE) != NONE as u8 {
        m.w16(this + DIR, 1);
        let to = key_points(m, this, 1);
        let (from, state) = (hu(m, this + FROM), b(m, this + STATE));
        m.w16(this + TO, to as u16);
        m.w32(this + SLID_IN, 1);
        m.w16(this + FRAME, from as u16);
        m.w16(this + SLID, state as u16);
    }
    let finished = transition_is_finished(c);
    let m = &mut c.m;
    if !finished && m.r32(this + MENU_IN) != 0 && m.r32(this + SLIDING_OUT) == 0 && m.r8(this + STATE) != NONE as u8 {
        m.w16(this + DIR, 0xFFFF);
        let to = key_points(m, this, 0);
        let (from, state) = (hu(m, this + FROM), b(m, this + STATE));
        m.w16(this + TO, to as u16);
        m.w32(this + SLIDING_OUT, 1);
        m.w32(this + FAST, 1);
        m.w16(this + FRAME, from as u16);
        m.w16(this + SLID, state as u16);
    }
    if c.m.r32(this + SLIDING_OUT) == 0 {
        set_current_controller(c, this, 0);
        check_configs(c, this);
    }
    let m = &mut c.m;
    if (m.r16(this + DIR) != 0 || m.r32(this + SLIDING_OUT) == 0) && m.r32(this + SLID_IN) != 0 {
        draw_controller(c, this);
    }
    let fade = sx(fade);
    for k in 0..12 {
        psxfront::scale_shape_extended(c, k + 10, 0, 0, 0, fade, 0, 0);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, c.r[4]);
}

/// The labels' fade: out while anything changes, in once all's settled; at all out, their
/// state and kind taken (`false`: nothing more to draw); typing on once in.
fn label_fade(c: &mut Cpu, this: u32) -> bool {
    let mut step = 8u32;
    let m = &mut c.m;
    if m.r32(this + CHANGED) == 0 && m.r16(this + TURNING) == 0 && m.r16(this + DIR) == 0 && m.r32(this + OUT) == 0 && transition_is_finished(c) {
        let m = &mut c.m;
        if m.r32(this + SLIDING_OUT) == 0 {
            step = (-8i32) as u32;
            if (hs(m, this + FADE_STEP) as i32) > 0 {
                m.w16(this + LETTERS, 0);
            }
        }
    }
    let m = &mut c.m;
    m.w16(this + FADE_STEP, step as u16);
    let f = m.r16(this + FADE).wrapping_add(m.r16(this + FADE_STEP));
    m.w16(this + FADE, f);
    if (f as i16) < 0 {
        m.w16(this + FADE, 0);
    }
    let fade = hs(m, this + FADE) as i32;
    if fade >= 128 {
        let state = b(m, this + STATE);
        m.w16(this + FADE, 128);
        m.w8(this + SHOWN, state as u8);
        if state == ANALOG_NEGCON {
            m.w8(this + SHOWN, NEGCON as u8);
        }
        let kind = pad_kind(m, this);
        m.w32(this + CHANGED, 0);
        m.w8(this + SHOWN_KIND, kind as u8);
        return false;
    }
    if fade == 0 {
        let l = m.r16(this + LETTERS).wrapping_add(1);
        m.w16(this + LETTERS, l);
    }
    if (hs(m, this + LETTERS) as i32) >= 101 {
        m.w16(this + LETTERS, 100);
    }
    true
}

/// The labels' table for the shown state and kind: how many, and label `k`'s two bytes (its
/// text's entry, its arrow's line; -1 none).
fn labels(m: &mut Mem, this: u32) -> (u32, u32, i32) {
    let shown = b(m, this + SHOWN).wrapping_sub(1);
    let kind = b(m, this + SHOWN_KIND);
    let row = shown.wrapping_mul(3);
    let count = hs(m, LABEL_COUNTS.wrapping_add(row.wrapping_add(kind) << 1)) as i32;
    (row, kind, count)
}

fn label_entry(m: &mut Mem, row: u32, kind: u32, k: u32, at: u32) -> u32 {
    m.r8(LABEL_IDS.wrapping_add(k << 1).wrapping_add(kind << 5).wrapping_add(row << 5) + at) as i8 as i32 as u32
}

/// `tScreenControllerConfig::DrawForeground` (SCREENCONTROLLER.CPP, 0x8004544c): the pad
/// checked; the labels faded out while anything changes, in once all's settled (their state
/// and kind taken when out); each label typed out letter by letter (the last letter brighter)
/// over a box, its arrow to its button (but under the dialog); the screen's front shapes.
pub fn draw_foreground(c: &mut Cpu, this: u32) {
    set_current_controller(c, this, 0);
    check_configs(c, this);
    if !label_fade(c, this) {
        return;
    }
    if c.m.r16(this + FADE) != 0 {
        fetextrender::set_abr(c, 1, 1);
    }
    if c.m.r8(this + SHOWN) != NONE as u8 {
        let m = &mut c.m;
        let kind = b(m, TEXT_KIND);
        let colours = m.r32(TEXT_COLORS + 4 * kind);
        let f = hs(m, this + FADE);
        let bright = crate::fefades::fade_faded(colours, 0xFF_FFFF, 100, f);
        let colour = crate::fefades::fade_out(colours, hs(m, this + FADE));
        let mut k = 0u32;
        loop {
            let m = &mut c.m;
            let (row, kind, count) = labels(m, this);
            if !((sx(k) as i32) < count) {
                break;
            }
            let id = label_entry(m, row, kind, sx(k), 0);
            if id != u32::MAX {
                draw_label(c, this, LABELS.wrapping_add(id.wrapping_mul(10)), colour, bright);
            }
            k = k.wrapping_add(1);
        }
    }
    // the arrows (but under the dialog)
    let mut k = 0u32;
    loop {
        let m = &mut c.m;
        let (row, kind, count) = labels(m, this);
        if !((sx(k) as i32) < count) {
            break;
        }
        let dialog = this + DIALOG;
        let free = m.r32(dialog + 112) == 0 && m.r32(dialog + 156) == 0;
        if free && (b(m, this + SHOWN).wrapping_sub(1) as i32) >= 0 {
            let v = label_entry(m, row, kind, sx(k), 1);
            if v != u32::MAX {
                draw_arrow(c, this, v.wrapping_mul(12).wrapping_add(LINES));
            }
        }
        k = k.wrapping_add(1);
    }
    fetextrender::set_abr(c, 0, 0);
    let m = &mut c.m;
    let all = menus(m);
    let v = (m.r32(all + 11232) as i32 >> 1) as u32;
    let fade = sx(if (sx(v) as i32) < 129 { v } else { 128 });
    for k in 0..2 {
        psxfront::scale_shape_extended(c, k + 22, 0, 0, 0, fade, 0, 0);
    }
}

pub fn abi_draw_foreground(c: &mut Cpu) {
    draw_foreground(c, c.r[4]);
}

/// A label (`at`: x, y, its word, its justification (0 left, 1 right), 10 bytes) typed out as
/// far as the letters shown: the letters so far (from its start, or the right one's from its
/// end), the last one again brighter beside them; the whole text once typed; a box under it.
fn draw_label(c: &mut Cpu, this: u32, at: u32, colour: u32, bright: u32) {
    let m = &mut c.m;
    let (x0, y, word, just) = (hs(m, at), hs(m, at + 2), hs(m, at + 4), m.r16(at + 6));
    let text = call(c, TEXT_WORD, &[word]);
    let n = call(c, STRLEN, &[text]);
    let len = sx(n);
    let letters = hs(&mut c.m, this + LETTERS);
    let typed = sx(if (len as i32) < letters as i32 { n } else { letters });
    // (the letters in a buffer, the byte before it the original's 0x80: read as the last
    // letter before any is typed; the full text, the last letter)
    with_buffer(c, 40 + 256 + 8, |c, buffer| {
        let (part, full, last) = (buffer + 1, buffer + 40, buffer + 40 + 256);
        c.m.w8(buffer, 0x80);
        let shown = if !((hs(&mut c.m, this + LETTERS) as i32) < len as i32) {
            call(c, TEXT_WORD, &[word])
        } else {
            type_letters(c, this, x0, y, word, just, (n, len, typed), (part, full, last), bright);
            part
        };
        fetextrender::full_text_rgb(c, shown, x0, y, colour, 0, just as i16 as i32 as u32);
        let px = call(c, TEXT_PIXELS, &[shown]);
        let n3 = call(c, STRLEN, &[shown]);
        let mut w = px.wrapping_sub(n3);
        if just != 0 {
            w = 0u32.wrapping_sub(w);
        }
        call_addr(c, DRAW_SQUARE, &[0, x0, y, w, 7]);
    });
}

/// The letters typed so far into `part`: from the start, or the right-justified label's from
/// its end; the last one again beside them, brighter.
#[allow(clippy::too_many_arguments)]
fn type_letters(c: &mut Cpu, this: u32, x0: u32, y: u32, word: u32, just: u16, (n, len, typed): (u32, u32, u32), (part, full, last): (u32, u32, u32), bright: u32) {
    let t = call(c, TEXT_WORD, &[word]);
    call(c, SPRINTF, &[full, t]);
    let m = &mut c.m;
    if just == 0 {
        // from the start
        let mut s = 0u32;
        if 0 < hs(m, this + LETTERS) as i32 {
            loop {
                let n = call(c, STRLEN, &[full]);
                let k = sx(s);
                if !((k as i32) < sx(n) as i32) {
                    break;
                }
                s = s.wrapping_add(1);
                let m = &mut c.m;
                let ch = m.r8(full + k);
                m.w8(part.wrapping_add(k), ch);
                if !((sx(s) as i32) < hs(m, this + LETTERS) as i32) {
                    break;
                }
            }
        }
        let m = &mut c.m;
        let k = sx(s);
        m.w8(part.wrapping_add(k), 0);
        let ch = m.r8(part.wrapping_add(k).wrapping_sub(1));
        m.w8(last, ch);
    } else {
        // the right-justified one's from its end
        let mut s = n.wrapping_sub(typed);
        let skip = len.wrapping_sub(typed);
        while (sx(s) as i32) < len as i32 {
            let k = sx(s);
            s = s.wrapping_add(1);
            let ch = m.r8(full.wrapping_add(k));
            m.w8(part.wrapping_add(k.wrapping_sub(skip)), ch);
        }
        let k = sx(s);
        let gone = sx(n).wrapping_sub(typed);
        m.w8(part.wrapping_add(k.wrapping_sub(gone)), 0);
        let ch = m.r8(part);
        m.w8(last, ch);
    }
    c.m.w8(last + 1, 0);
    // the last letter beside the letters, brighter
    fetextrender::set_font(c, 0);
    let px = call(c, TEXT_PIXELS, &[part]);
    let n2 = call(c, STRLEN, &[part]);
    let w = px.wrapping_sub(n2);
    let x = if just == 1 { x0.wrapping_sub(w) } else { x0.wrapping_add(w) };
    fetextrender::full_text_rgb(c, last, sx(x), y, bright, 0, (just == 0) as u32);
}

/// `tScreenControllerConfig::GetShapeInfo` (SCREENCONTROLLER.CPP, 0x80045ab4): 24 shapes of
/// `zpCtrl`, 66 swapped from `aCtrl`.
pub fn get_shape_info(m: &mut Mem, count: u32, swap: u32, file: u32, swap_file: u32) {
    m.w16(count, 24);
    m.w32(file, SHAPE_FILE);
    m.w16(swap, PICTURE_SHAPES as u16);
    m.w32(swap_file, SWAP_FILE);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_file);
}

/// `tScreenControllerConfig::Initialize` (SCREENCONTROLLER.CPP, 0x80045ae4): the rumble off,
/// the player's pad, no picture, the screen's; no menu, the pad's state found and its picture
/// loaded, the labels' state and kind, the configs checked; nothing animating, the labels out.
pub fn initialize(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    m.w8(this + SHAKING, 0);
    turn_off_shakers(m, this);
    clear_actuators(m, this);
    m.w32(this + RESTART, 1);
    let app = m.r32(FE_APP);
    m.w16(this + SLIDER, 0xFFFF);
    let player = b(m, app + APP_PLAYER);
    m.w32(this + QUIET_AT, 0);
    m.w16(this + PICTURE, 0xFFFF);
    m.w16(this + ANSWER, 0xFFFF);
    m.w32(this + PLAYER, player);
    fescreen::initialize(c, Screen(this));
    c.m.w8(this + STATE, NONE as u8);
    let item = menus(&mut c.m).wrapping_add(SLIDING_ITEM);
    crate::femenuoptions::sliding_menu_set_menu(&mut c.m, MenuItemSlidingMenu(item), 1, 0);
    set_current_controller(c, this, 1);
    swap_in_controller(c, this);
    let m = &mut c.m;
    let labels = (hs(m, this + FADE) as i32) < 128;
    let state = b(m, this + STATE);
    m.w8(this + CHECKED, state as u8);
    if !labels {
        m.w8(this + SHOWN, state as u8);
    }
    if m.r8(this + SHOWN) == ANALOG_NEGCON as u8 {
        m.w8(this + SHOWN, NEGCON as u8);
    }
    let kind = pad_kind(m, this);
    m.w8(this + KIND, kind as u8);
    m.w8(this + SHOWN_KIND, kind as u8);
    check_configs(c, this);
    let m = &mut c.m;
    for f in [ANIMATING, TURNING, DIR, OUT, IN] {
        m.w16(this + f, 0);
    }
    m.w16(this + FADE, 128);
    m.w16(this + FADE_STEP, 0);
    m.w16(this + LETTERS, 0);
    for f in [SOUNDED, MENU_IN, SLID_IN, SLIDING_OUT, FAST] {
        m.w32(this + f, 0);
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, c.r[4]);
}

/// `tScreenControllerConfig::Cleanup` (SCREENCONTROLLER.CPP, 0x80045c08): the rumble off, the
/// pads' communication restarted, the screen's.
pub fn cleanup(c: &mut Cpu, this: u32) {
    clear_actuators(&mut c.m, this);
    turn_off_shakers(&mut c.m, this);
    call_addr(c, PAD_START_COM, &[]);
    fescreen::cleanup(c, Screen(this));
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, c.r[4]);
}

/// `tScreenControllerConfig::tScreenControllerConfig` (SCREENCONTROLLER.CPP, 0x80045c44): a
/// screen with its dialog, nothing animating, the first player.
pub fn screen_controller(c: &mut Cpu, this: u32) -> u32 {
    fescreen::screen(c, Screen(this));
    c.m.w32(this + VPTR, VTABLE);
    crate::fedialog::yes_no(c, DialogYesNo(this + DIALOG));
    let m = &mut c.m;
    for f in [ANIMATING, TURNING, IN, OUT] {
        m.w16(this + f, 0);
    }
    m.w32(this + PLAYER, 0);
    this
}

pub fn abi_screen_controller(c: &mut Cpu) {
    c.r[2] = screen_controller(c, c.r[4]);
}

/// `tScreenControllerConfig::GetHelpText` (SCREENCONTROLLER.CPP, 0x80045c94): the help's word
/// for the state (a NeGcon's, a digital pad's, an analog one's) or none.
pub fn get_help_text(m: &mut Mem, this: u32) -> u32 {
    match m.r8(this + STATE) as u32 {
        ANALOG_OFF => 539,
        DIGITAL => 538,
        NEGCON | ANALOG_NEGCON => 537,
        _ => u32::MAX,
    }
}

pub fn abi_get_help_text(c: &mut Cpu) {
    c.r[2] = get_help_text(&mut c.m, c.r[4]);
}

/// `tScreenControllerConfig::~tScreenControllerConfig` (SCREENCONTROLLER.CPP, 0x80045cf4):
/// the dialog's and the screen's.
pub fn abi_screen_controller_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    fescreen::screen_destructor(c, Screen(this + DIALOG), 2);
    fescreen::screen_destructor(c, Screen(this), flags);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's controller screen (constructed, never shown in the RAM image); the outputs of
    /// `GetShapeInfo`; an arrow's lines.
    const SCREEN: u32 = 0x8018_0514;
    const OUTS: u32 = 0x8014_7000;
    const LINE: u32 = 0x8014_7100;

    /// FEMENU's structures, then: the screen's shape information without a read or data (its
    /// flags any); the player, both pads' libpad state (connected or not, digital, analog,
    /// NeGcon or another type, the sticks about their dead zones and ends) and the front end's
    /// kinds, rumble strengths and configs; the state any, and the state last checked the one
    /// the pad gives (else `CheckConfigs` resets the pad, which waits with `timedwait`); the
    /// dialog asked or answered (its `Run` waits on the front end's frames); the labels' state
    /// and kind, the animations (their key frames apart, or the original divides by 0), the
    /// clocks about their limits; the menus' sliders, items, choice and fades; the halo.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let s = SCREEN;
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12) + 40;
        for k in (0..40).step_by(4) {
            let v = m.r32(from + k);
            m.w32(s + 40 + k, v);
        }
        let buffer = m.r32(from + 8);
        m.w32(s + 52, 0);
        m.w32(s + 44, [0, buffer][pick(rng, 2) as usize]);
        let flags = m.r16(s + 58) & !1;
        m.w16(s + 58, flags | pick(rng, 2) as u16);
        let player = pick(rng, 2);
        m.w32(s + PLAYER, player);
        for p in 0..2 {
            let info = PAD_INFO + 32 * p;
            m.w8(info + 4, [0, 0, 0, 0xFF][pick(rng, 4) as usize]);
            m.w8(info + 5, [35, 65, 83, 115, 0x12][pick(rng, 5) as usize]);
            for k in 8..12 {
                m.w8(info + k, [0, 117, 118, 127, 128, 138, 139, 255, pick(rng, 256)][pick(rng, 9) as usize] as u8);
            }
            m.w8(FRONT_END + p + 866, pick(rng, 3) as u8);
            m.w8(FRONT_END + p + 857, [0, 1, 64, 255][pick(rng, 4) as usize]);
            m.w8(FRONT_END + p + 859, [0, 1, 64, 255][pick(rng, 4) as usize]);
            m.w16(FRONT_END + 2 * p + 862, [35, 83, 115, 65][pick(rng, 4) as usize]);
        }
        let info = PAD_INFO + if player != 0 { 32 } else { 0 };
        let state = pick(rng, 7);
        let (status, ty) = (m.r8(info + 4), m.r8(info + 5));
        let now = if status != 0 {
            0
        } else {
            match ty {
                35 if state == 1 || state == 2 => state,
                35 => 0,
                65 => 4,
                83 | 115 => 6,
                _ => 3,
            }
        };
        m.w8(s + STATE, state as u8);
        m.w8(s + CHECKED, now as u8);
        // the kind shown: now and then another than the pad's (the picture turned over)
        let kind = m.r8(FRONT_END + player + 866);
        m.w8(s + KIND, if pick(rng, 3) == 0 { pick(rng, 3) as u8 } else { kind });
        if pick(rng, 2) == 0 {
            m.w32(s + DIALOG + 112, 0);
            m.w32(s + DIALOG + 156, 0);
            m.w16(s + ANSWER, pick(rng, 2) as u16);
        } else {
            m.w32(s + DIALOG + 112, pick(rng, 2));
            m.w32(s + DIALOG + 156, 1);
            m.w16(s + ANSWER, [0xFFFF, 0, 1][pick(rng, 3) as usize]);
        }
        m.w8(s + SHAKING, pick(rng, 2) as u8);
        m.w8(s + SHAKE_TIME, [0, 1, 2, 64][pick(rng, 4) as usize]);
        m.w8(s + SHOWN, pick(rng, 7) as u8);
        m.w8(s + SHOWN_KIND, pick(rng, 3) as u8);
        for f in [OUT, IN, SWAP, TURNING, ANIMATING] {
            m.w16(s + f, (pick(rng, 3) == 0) as u16);
        }
        m.w16(s + OUT_FROM, pick(rng, 7) as u16);
        m.w16(s + IN_TO, pick(rng, 7) as u16);
        let ticks = m.r32(GAME_TICKS);
        m.w32(s + START, ticks.wrapping_sub([0, 5, 10, 11, 60, 200, 300][pick(rng, 7) as usize]));
        let from = [0, 20, 24, 40][pick(rng, 4) as usize];
        let to = [0, 20, 24, 40].into_iter().filter(|&v| v != from).nth(pick(rng, 3) as usize).unwrap();
        m.w16(s + TURN_FROM, from as u16);
        m.w16(s + TURN_TO, to as u16);
        m.w16(s + TURN_FRAME, pick(rng, 45) as u16);
        m.w16(s + TURN_DIR, [1, 0xFFFF][pick(rng, 2) as usize]);
        let from = [0, 20, 24, 40][pick(rng, 4) as usize];
        let to = [0, 20, 24, 40].into_iter().filter(|&v| v != from).nth(pick(rng, 3) as usize).unwrap();
        m.w16(s + FROM, from as u16);
        m.w16(s + TO, to as u16);
        m.w16(s + FRAME, pick(rng, 45) as u16);
        m.w16(s + DIR, [0, 0, 1, 0xFFFF][pick(rng, 4) as usize]);
        m.w16(s + SLID, pick(rng, 7) as u16);
        m.w16(s + PICTURE, [0xFFFF, 0, 1, 2, pick(rng, 7)][pick(rng, 5) as usize] as u16);
        for f in [MENU_IN, SLID_IN, SLIDING_OUT, CHANGED, FAST, SOUNDED, RESTART] {
            m.w32(s + f, (pick(rng, 2) == 0) as u32);
        }
        m.w16(s + FADE, [0, 1, 8, 64, 100, 120, 127, 128, 136][pick(rng, 9) as usize]);
        m.w16(s + FADE_STEP, [0, 8, 0xFFF8][pick(rng, 3) as usize]);
        m.w16(s + LETTERS, [0, 1, 3, 10, 99, 100, 101][pick(rng, 7) as usize]);
        m.w16(s + BRIGHT, pick(rng, 70) as u16);
        m.w32(s + QUIET_AT, [0, ticks, ticks.wrapping_sub(96), ticks.wrapping_sub(97)][pick(rng, 4) as usize]);
        m.w16(s + SLIDER, [0xFFFF, 0, 1][pick(rng, 3) as usize]);
        let all = m.r32(ALL_MENUS);
        m.w16(all + 11124, pick(rng, 3) as u16);
        m.w16(all + 11916, pick(rng, 2) as u16);
        m.w16(all + 12148, pick(rng, 2) as u16);
        // (the sticks' items off: their lean is drawn with a struct `DrawController` leaves
        // on the stack, which holds the fuzz's random leftovers; the tour draws it)
        for at in [11428, 11496, 11564, 11632] {
            m.w32(all + at, 0);
        }
        for at in [11700, 11768] {
            m.w32(all + at, (pick(rng, 3) == 0) as u32);
        }
        for at in [11116 + 116, 11232] {
            m.w32(all + at, [0, 100, 256, 257, 258, 300][pick(rng, 6) as usize]);
        }
        m.w32(HALO_SIZE, [0, 100, 250, 3800, 4000][pick(rng, 5) as usize]);
        m.w32(HALO_TARGET, [3700, 3750, 4000, 4700, 4751][pick(rng, 5) as usize]);
        m.w32(HALO_STEP, [55, (-55i32) as u32][pick(rng, 2) as usize]);
        for k in 0..6 {
            m.w16(LINE + 2 * k, [0, pick(rng, 320), pick(rng, 240)][pick(rng, 3) as usize] as u16);
        }
        if pick(rng, 3) == 0 {
            let x = m.r16(LINE + 4);
            m.w16(LINE + 8, x);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenControllerConfig::ClearActuators", 0x8004_3500, nfs4rc::f_80043500, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::SetActuators", 0x8004_350c, nfs4rc::f_8004350c, |_, r| vec![SCREEN, [0, 2, 128, 254, 255][pick(r, 5) as usize]]),
            ("tScreenControllerConfig::TurnOffShakers", 0x8004_3594, nfs4rc::f_80043594, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::ShakeIt", 0x8004_359c, nfs4rc::f_8004359c, |_, _| vec![SCREEN]),
            ("Controller_SetRamp", 0x8004_3638, nfs4rc::f_80043638, |_, _| vec![]),
            ("tScreenControllerConfig::AnimKeyPoints", 0x8004_3784, nfs4rc::f_80043784, |_, r| vec![SCREEN, pick(r, 3), pick(r, 3)]),
            ("tScreenControllerConfig::CheckConfigs", 0x8004_37e8, nfs4rc::f_800437e8, |c, r| {
                // (half the time the state the one checked: the picture turned over)
                if pick(r, 2) == 0 {
                    let state = c.m.r8(SCREEN + STATE);
                    c.m.w8(SCREEN + CHECKED, state);
                }
                vec![SCREEN]
            }),
            ("tScreenControllerConfig::SwapInController", 0x8004_3ad4, nfs4rc::f_80043ad4, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::SetCurrentController", 0x8004_3b60, nfs4rc::f_80043b60, |_, r| vec![SCREEN, pick(r, 2)]),
            ("tScreenControllerConfig::CalcAnimFrame", 0x8004_3ed8, nfs4rc::f_80043ed8, |_, r| vec![SCREEN, pick(r, 60)]),
            ("tScreenControllerConfig::ActualDrawController", 0x8004_3f64, nfs4rc::f_80043f64, |_, r| {
                vec![SCREEN, pick(r, 60), [0, 64, 128][pick(r, 3) as usize], [0, 64, 128][pick(r, 3) as usize], pick(r, 9).wrapping_sub(4), pick(r, 9).wrapping_sub(4)]
            }),
            ("tScreenControllerConfig::DrawController", 0x8004_41cc, nfs4rc::f_800441cc, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::HorzVertLine", 0x8004_4edc, nfs4rc::f_80044edc, |_, r| vec![SCREEN, LINE, pick(r, 3)]),
            ("tScreenControllerConfig::DrawArrow", 0x8004_5128, nfs4rc::f_80045128, |_, _| vec![SCREEN, LINE]),
            ("tScreenControllerConfig::DrawBackground", 0x8004_5228, nfs4rc::f_80045228, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::DrawForeground", 0x8004_544c, nfs4rc::f_8004544c, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::GetShapeInfo", 0x8004_5ab4, nfs4rc::f_80045ab4, |_, _| vec![SCREEN, OUTS, OUTS + 4, OUTS + 8, OUTS + 12]),
            ("tScreenControllerConfig::Initialize", 0x8004_5ae4, nfs4rc::f_80045ae4, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig::Cleanup", 0x8004_5c08, nfs4rc::f_80045c08, |_, _| vec![SCREEN]),
            ("tScreenControllerConfig", 0x8004_5c44, nfs4rc::f_80045c44, |_, _| vec![OUTS + 0x100]),
            ("tScreenControllerConfig::GetHelpText", 0x8004_5c94, nfs4rc::f_80045c94, |_, _| vec![SCREEN]),
            ("~tScreenControllerConfig", 0x8004_5cf4, nfs4rc::f_80045cf4, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screencontroller_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xC0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xC0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
