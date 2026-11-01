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

use crate::abi::{self, Saved};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PAD_INFO, TEXT_COLORS};
use ps1rt::Cpu;

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
const HALO: fn(&mut Cpu) = nfs4rc::f_800cecd0;
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

/// A port called from a port with `args` in a0.. (stack ones written by the caller first).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        let n = args.len().min(4);
        c.r[4..4 + n].copy_from_slice(&args[..n]);
        let sp = c.r[29];
        for (k, &v) in args.iter().enumerate().skip(4) {
            c.m.w32(sp + 4 * k as u32, v);
        }
        f(c);
        c.r[2]
    })
}

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

/// A halfword, sign-extended.
fn hs(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn hu(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

fn b(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r8(at) as u32
}

fn menus(m: &mut ps1rt::Mem) -> u32 {
    m.r32(ALL_MENUS)
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
pub fn abi_clear_actuators(c: &mut Cpu) {
    let this = c.r[4];
    c.m.w8(this + ACT, 0);
    c.m.w8(this + ACT + 1, 0);
}

/// `tScreenControllerConfig::SetActuators` (SCREENCONTROLLER.CPP, 0x8004350c): the rumble at
/// `strength` for a while (64 frames from a restart): the small motor on by the wave, the big
/// one at the strength; off when the time's out.
pub fn abi_set_actuators(c: &mut Cpu) {
    let (this, strength) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let time = if c.m.r32(this + RESTART) != 0 {
            c.m.w32(this + RESTART, 0);
            64
        } else {
            let t = b(&mut c.m, this + SHAKE_TIME);
            if t == 0 {
                port(c, &r, 0x8004_3500, 0x8004_3584, &[this], abi_clear_actuators);
                return;
            }
            t - 1
        };
        let m = &mut c.m;
        m.w8(this + SHAKE_TIME, time as u8);
        if m.r8(this + SHAKE_TIME) == 0 {
            port(c, &r, 0x8004_3500, 0x8004_3584, &[this], abi_clear_actuators);
            return;
        }
        let k = (m.r32(GAME_TICKS) as i32 >> 2) as u32 & 0xFF;
        let wave = b(m, WAVE + k);
        m.w8(this + ACT + 1, strength as u8);
        m.w8(this + ACT, ((wave as i32) < strength as i32) as u8);
    })
}

/// `tScreenControllerConfig::TurnOffShakers` (SCREENCONTROLLER.CPP, 0x80043594).
pub fn abi_turn_off_shakers(c: &mut Cpu) {
    let this = c.r[4];
    c.m.w8(this + SHAKING, 0);
}

/// `tScreenControllerConfig::ShakeIt` (SCREENCONTROLLER.CPP, 0x8004359c): with the pad
/// stable, its actuators set up once and given the bytes; else (gone) off.
pub fn abi_shake_it(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let port_id = c.m.r32(this + PLAYER) << 4;
        (r.0[1], r.0[0]) = (this, port_id);
        let st = r.call_ptr_leaving(c, PAD_GET_STATE, &[port_id], 0x8004_35c4);
        if st == 6 {
            if c.m.r8(this + SHAKING) == 0 {
                c.m.w8(this + SHAKING, 1);
                r.call_ptr_leaving(c, PAD_SET_ACT, &[port_id, this + ACT, 2], 0x8004_3614);
                r.call_ptr_leaving(c, PAD_SET_ACT_ALIGN, &[port_id, ALIGN], 0x8004_3624);
            }
        } else {
            if (st as i32) < 4 {
                c.m.w8(this + SHAKING, 0);
            }
            port(c, &r, 0x8004_3500, 0x8004_35e8, &[this], abi_clear_actuators);
        }
    })
}

/// `Controller_SetRamp` (SCREENCONTROLLER.CPP, 0x80043638): each player's three ramps on, but
/// those whose config's buttons are on the sticks (accelerate, brake, the third).
pub fn abi_controller_set_ramp(c: &mut Cpu) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[4], r.0[5], r.0[3]) = (0, FRONT_END, CONFIGS, 1);
        loop {
            let k = sx(r.0[2]);
            if (k as i32) >= 2 {
                break;
            }
            let m = &mut c.m;
            let kind = hs(m, FRONT_END + 2 * k + 862);
            let config = match kind {
                35 => 0,
                83 | 115 => 1,
                _ => 2,
            };
            let s0 = k.wrapping_add(FRONT_END);
            r.0[0] = s0;
            let pad = b(m, s0 + 866);
            for at in [793, 795, 791] {
                m.w8(s0 + at, 1);
            }
            let s1 = (pad.wrapping_mul(39).wrapping_add(config) << 2).wrapping_add(CONFIGS);
            r.0[1] = s1;
            for (off, flag, ra) in [(0, 791, 0x8004_3704), (24, 793, 0x8004_3724), (36, 795, 0x8004_3744)] {
                let button = c.m.r32(s1 + off);
                let v = port(c, &r, 0x8002_71b4, ra, &[button, 0], crate::front::abi_get_psx_pad_value);
                if r.call_leaving(c, GET_DEVICE, &[v], ra + 8) == 1 {
                    c.m.w8(s0 + flag, 0);
                }
            }
            r.0[2] = r.0[2].wrapping_add(1);
        }
    })
}

/// `tScreenControllerConfig::AnimKeyPoints` (SCREENCONTROLLER.CPP, 0x80043784): the
/// animation's frame at its start or end (`end`), sliding in or out (`slide_in`): an analog
/// pad's 0, 20 or 40, the others' 0 or 24.
pub fn abi_anim_key_points(c: &mut Cpu) {
    let (this, slide_in, end) = (c.r[4], c.r[5], c.r[6]);
    c.r[2] = if c.m.r8(this + STATE) == 2 {
        match (slide_in != 0, end != 0) {
            (true, false) | (false, false) => 20,
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
    };
}

/// `tScreenControllerConfig::CheckConfigs` (SCREENCONTROLLER.CPP, 0x800437e8): a new state
/// (the pad reset for its kind): the picture animated out (none now), in (none before), or
/// out and in with a swap (another picture), the labels' state set; the same picture: the
/// kind changed (a Dual Shock's or the analog NeGcon's swapped in for the NeGcon's or the
/// unknown's). The same state but another pad kind (reset): turning the picture over, the way
/// the kinds go.
pub fn abi_check_configs(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        if m.r8(this + STATE) != m.r8(this + CHECKED) {
            port(c, &r, 0x8004_3594, 0x8004_3818, &[this], abi_turn_off_shakers);
            let p = c.m.r32(this + PLAYER);
            let kind = b(&mut c.m, p.wrapping_add(FRONT_END) + 866);
            port(c, &r, 0x8002_7104, 0x8004_3834, &[p, kind], crate::front::abi_reset_psx_controller);
            let m = &mut c.m;
            if hs(m, this + TURNING) != 0 {
                return;
            }
            let labels;
            if m.r8(this + STATE) == 0 {
                if hs(m, this + DIR) != 0 {
                    return;
                }
                let (from, state) = (b(m, this + CHECKED), b(m, this + STATE));
                m.w16(this + OUT, 1);
                m.w16(this + OUT_FROM, from as u16);
                m.w8(this + CHECKED, state as u8);
                return;
            } else if m.r8(this + CHECKED) == 0 {
                if hs(m, this + DIR) != 0 {
                    return;
                }
                let state = b(m, this + STATE);
                labels = (hs(m, this + FADE) as i32) < 128;
                m.w16(this + SWAP, 1);
                m.w16(this + IN, 1);
                m.w16(this + IN_TO, state as u16);
            } else {
                let picture = hs(m, this + PICTURE);
                let mut same = false;
                if picture != u32::MAX {
                    let theirs = m.r32(NAMES + 4 * picture);
                    let state = b(m, this + STATE);
                    let ours = m.r32(NAMES + 4 * state);
                    same = r.call_leaving(c, STRCMP, &[ours, theirs], 0x8004_38f0) == 0;
                }
                let m = &mut c.m;
                if same {
                    let checked = b(m, this + CHECKED);
                    m.w32(this + CHANGED, 1);
                    if checked == 5 || checked == 3 {
                        let state = b(m, this + STATE);
                        if state == 4 || state == 6 {
                            port(c, &r, 0x8004_3ad4, 0x8004_3994, &[this], abi_swap_in_controller);
                        }
                    }
                    let m = &mut c.m;
                    let state = b(m, this + STATE);
                    m.w8(this + CHECKED, state as u8);
                    return;
                }
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
            if m.r8(this + SHOWN) == 6 {
                m.w8(this + SHOWN, 5);
            }
            let state = b(m, this + STATE);
            m.w8(this + CHECKED, state as u8);
        } else {
            r.0[1] = FRONT_END;
            let p = m.r32(this + PLAYER);
            let kind = b(m, p.wrapping_add(FRONT_END) + 866);
            if kind == b(m, this + KIND) {
                return;
            }
            port(c, &r, 0x8002_7104, 0x8004_39c8, &[p, kind], crate::front::abi_reset_psx_controller);
            let m = &mut c.m;
            if m.r32(this + OUT) == 0 && hs(m, this + TURNING) == 0 {
                let state = b(m, this + STATE);
                m.w16(this + TURNING, 1);
                m.w16(this + TURNED, state as u16);
                let p = m.r32(this + PLAYER);
                let now = b(m, p.wrapping_add(FRONT_END) + 866);
                let was = b(m, this + KIND);
                let up = (was < now && !(now == 2 && was == 0)) || (now == 0 && was == 2);
                let (a, z, ras, dir) = if up {
                    (1, 1, (0x8004_3a48, 0x8004_3a5c), 1u32)
                } else {
                    (0, 1, (0x8004_3a78, 0x8004_3a8c), u32::MAX)
                };
                let from = port(c, &r, 0x8004_3784, ras.0, &[this, a, z], abi_anim_key_points);
                c.m.w16(this + TURN_FROM, from as u16);
                let to = port(c, &r, 0x8004_3784, ras.1, &[this, a, 0], abi_anim_key_points);
                let m = &mut c.m;
                m.w16(this + TURN_TO, to as u16);
                m.w16(this + TURN_DIR, dir as u16);
                let f = hu(m, this + TURN_FROM);
                m.w16(this + TURN_FRAME, f as u16);
            }
            let m = &mut c.m;
            let p = m.r32(this + PLAYER);
            let kind = b(m, p.wrapping_add(FRONT_END) + 866);
            m.w8(this + KIND, kind as u8);
        }
    })
}

/// `tScreenControllerConfig::SwapInController` (SCREENCONTROLLER.CPP, 0x80043ad4): the
/// state's picture loaded (unless the one loaded has its file).
pub fn abi_swap_in_controller(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let picture = hs(m, this + PICTURE);
        if picture != u32::MAX {
            let theirs = m.r32(NAMES + 4 * picture);
            let state = b(m, this + STATE);
            let ours = m.r32(NAMES + 4 * state);
            if r.call_leaving(c, STRCMP, &[ours, theirs], 0x8004_3b1c) == 0 {
                return;
            }
        }
        let m = &mut c.m;
        let state = b(m, this + STATE);
        let name = m.r32(NAMES + 4 * state);
        port(c, &r, 0x8002_5dd8, 0x8004_3b44, &[this, name], crate::fescreen::abi_async_load_swap_shape_file);
        let state = b(&mut c.m, this + STATE);
        c.m.w16(this + PICTURE, state as u16);
    })
}

/// `tScreenControllerConfig::SetCurrentController` (SCREENCONTROLLER.CPP, 0x80043b60): the
/// player's pad's state from libpad: none, a digital pad (asking, the first time on the
/// screen, whether it's a Dual Shock with its analog mode off), a Dual Shock (stable: its
/// rumble's menu), analog (quiet a while or in the digital mode: unknown), a NeGcon (stable:
/// an analog NeGcon-like one; else the NeGcon's menu), others unknown; the menu set for it, the
/// rumble's sliders shown for a NeGcon kind on a Dual Shock.
pub fn abi_set_current_controller(c: &mut Cpu) {
    let (this, first) = (c.r[4], c.r[5]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let dialog = this + DIALOG;
        (r.0[1], r.0[5], r.0[2], r.0[0]) = (this, first, 0, dialog);
        let title = r.call_leaving(c, TEXT_WORD, &[524], 0x8004_3b98);
        let m = &mut c.m;
        m.w32(dialog + 144, title);
        m.w32(dialog + 160, 525);
        m.w32(dialog + 164, 526);
        m.w16(dialog + 136, 1);
        let second = (0 < m.r32(this + PLAYER)) as u32;
        let a0 = 0u32.wrapping_sub(second);
        let info = (a0 & 0x20).wrapping_add(PAD_INFO);
        r.0[4] = 0;
        let mut menu = 0u32;
        let mut set = false;
        enum Next {
            Menu,
            Unknown { s4: bool },
        }
        let next = if m.r8(info + 4) != 0 {
            let labels = (hs(m, this + FADE) as i32) < 128;
            m.w8(this + STATE, 0);
            if !labels {
                m.w16(this + PICTURE, 0);
            }
            set = true;
            Next::Menu
        } else {
            match b(m, info + 5) {
                35 => {
                    m.w32(this + QUIET_AT, 0);
                    let state = b(m, this + STATE);
                    if r.0[5] != 0 || state == 2 || state == 1 {
                        menu = menus(m).wrapping_add(12256);
                    } else {
                        let unasked = if m.r32(dialog + 112) != 0 { false } else { m.r32(dialog + 156) == 0 };
                        let answer = if unasked { hs(m, this + ANSWER) } else { 0 };
                        if unasked {
                            r.0[3] = answer;
                        }
                        if unasked && answer == u32::MAX {
                            m.w8(this + STATE, 0);
                            m.w16(this + FADE, 128);
                            let v = port(c, &r, 0x8001_9928, 0x8004_3ca8, &[dialog], crate::fedialog::abi_run);
                            let m = &mut c.m;
                            m.w16(this + ANSWER, v as u16);
                            m.w8(this + STATE, if v << 16 == 0 { 1 } else { 2 });
                            m.w16(this + ANSWER, r.0[3] as u16);
                            menu = menus(m).wrapping_add(12256);
                        } else {
                            set = true;
                            r.0[5] = 1;
                            m.w16(this + FADE, 128);
                            m.w8(this + STATE, 0);
                        }
                    }
                    Next::Menu
                }
                65 => {
                    let st = r.call_ptr_leaving(c, PAD_GET_STATE, &[a0 & 0x10], 0x8004_3d04);
                    r.0[0] = 0;
                    if st == 6 {
                        let m = &mut c.m;
                        m.w8(this + STATE, 4);
                        m.w32(this + QUIET_AT, 0);
                        menu = menus(m).wrapping_add(11908);
                        Next::Menu
                    } else {
                        let p = (0 < c.m.r32(this + PLAYER)) as u32;
                        let mut lost = r.call_ptr_leaving(c, PAD_GET_STATE, &[p << 4], 0x8004_3d40) == 2;
                        if !lost && c.m.r32(this + QUIET_AT) != 0 {
                            let p = (0 < c.m.r32(this + PLAYER)) as u32;
                            if r.call_ptr_leaving(c, PAD_GET_STATE, &[p << 4], 0x8004_3d70) == 1 {
                                let m = &mut c.m;
                                lost = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + QUIET_AT)) as i32) >= 97;
                            }
                        }
                        if lost {
                            r.0[0] = 1;
                        }
                        let m = &mut c.m;
                        if lost || m.r8(this + STATE) == 3 {
                            Next::Unknown { s4: true }
                        } else {
                            if m.r32(this + QUIET_AT) == 0 {
                                let t = m.r32(GAME_TICKS);
                                m.w32(this + QUIET_AT, t);
                            }
                            Next::Menu
                        }
                    }
                }
                83 | 115 => {
                    let p = m.r32(this + PLAYER);
                    m.w32(this + QUIET_AT, 0);
                    let st = r.call_ptr_leaving(c, PAD_GET_STATE, &[((0 < p) as u32) << 4], 0x8004_3de4);
                    if st == 6 {
                        c.m.w8(this + STATE, 6);
                        menu = menus(&mut c.m).wrapping_add(12140);
                    } else {
                        let p = c.m.r32(this + PLAYER);
                        if r.call_ptr_leaving(c, PAD_GET_STATE, &[((0 < p) as u32) << 4], 0x8004_3e18) == 2 {
                            c.m.w8(this + STATE, 5);
                            menu = menus(&mut c.m).wrapping_add(12024);
                        }
                    }
                    Next::Menu
                }
                _ => Next::Unknown { s4: true },
            }
        };
        if let Next::Unknown { s4 } = next {
            set |= s4;
            c.m.w32(this + QUIET_AT, 0);
            c.m.w8(this + STATE, 3);
        }
        r.0[2] = menu;
        r.0[4] = set as u32;
        if menu != 0 || set {
            let item = menus(&mut c.m).wrapping_add(11048);
            port(c, &r, 0x8001_de88, 0x8004_3e6c, &[item, r.0[5], menu], crate::femenuoptions::abi_sliding_menu_set_menu);
        }
        let m = &mut c.m;
        let state = b(m, this + STATE);
        let shown = state.wrapping_sub(5) < 2 && b(m, this + SHOWN_KIND) < 2;
        let all = menus(m);
        m.w16(all + 11216, shown as u16);
    })
}

/// `tScreenControllerConfig::CalcAnimFrame` (SCREENCONTROLLER.CPP, 0x80043ed8): an analog
/// pad's picture's frame for an animation's frame (its turning over between 20 and 40 and
/// back); the others' the frame.
pub fn abi_calc_anim_frame(c: &mut Cpu) {
    let (this, f) = (c.r[4], c.r[5]);
    let fi = f as i32;
    c.r[2] = if c.m.r8(this + STATE) != 2 {
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
    };
}

/// `tScreenControllerConfig::ActualDrawController` (SCREENCONTROLLER.CPP, 0x80043f64): the
/// pad's picture at `frame` (scaled, faded by `fade`, moved by `dx`, `dy`) over its base
/// (faded by `base`); an analog pad's buttons' plates and the dots where the labels' lines
/// meet them.
pub fn abi_actual_draw_controller(c: &mut Cpu) {
    let (this, frame, fade, base) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (dx, dy) = (abi::arg(c, 4), abi::arg(c, 5));
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[3], r.0[6], r.0[4], r.0[5]) = (this, fade, base, dx, dy);
        let m = &mut c.m;
        if m.r16(this + PICTURE) == 0 {
            return;
        }
        let info = m.r32(this + 40);
        m.w32(sp + 52, info);
        let mode = |fade: u32| if fade != 0 { 1537 } else { 1536 };
        let picture = hs(m, this + PICTURE);
        let (shape, x, y, scaled);
        if frame != 0 || picture == 1 {
            r.0[2] = SPOTS;
            let at = SPOTS.wrapping_add(2 * hs(m, this + PICTURE));
            let (x0, y0) = (b(m, at), b(m, at.wrapping_add(1)));
            r.0[0] = sp + 32;
            port(c, &r, 0x8004_ef70, 0x8004_4004, &[frame + 1, mode(fade), x0, y0, fade, 0, sp + 32], crate::psxfront::abi_scale_shape_extended);
            scaled = true;
            if hs(&mut c.m, this + PICTURE) != 2 {
                (shape, x, y) = (u32::MAX, 0, 0);
            } else {
                let m = &mut c.m;
                (shape, x, y) = (0, b(m, SPOTS + 4), b(m, SPOTS + 5));
            }
        } else {
            scaled = false;
            let at = SPOTS + (picture << 1);
            (shape, x, y) = (1, b(m, at), b(m, at + 1));
        }
        if shape != u32::MAX {
            let _ = scaled;
            port(c, &r, 0x8004_eb60, 0x8004_4080, &[shape, mode(fade), x.wrapping_add(dx), y.wrapping_add(dy), fade, 0, sp + 32], crate::psxfront::abi_draw_shape_extended);
        }
        if shape == u32::MAX || hs(&mut c.m, this + PICTURE) != 2 {
            let a1 = if base != 0 { 513 } else { 512 };
            port(c, &r, 0x8004_eb60, 0x8004_40bc, &[0, a1, 0, 0, base, 0, sp + 32], crate::psxfront::abi_draw_shape_extended);
            return;
        }
        let f = hs(&mut c.m, this + FADE);
        r.0[2] = 48;
        let colour = port(c, &r, 0x8004_bd68, 0x8004_40d4, &[0xC8_C8C8, f], crate::fefades::abi_fade_out);
        (r.0[1], r.0[0]) = (1, sp + 56);
        c.m.w32(sp + 60, colour);
        for (shape, y, ra) in [(26, 146, 0x8004_4100), (27, 161, 0x8004_4120), (28, 172, 0x8004_4140), (29, 187, 0x8004_4160)] {
            port(c, &r, 0x8004_eb60, ra, &[shape, 24, 158, y, 0, 1, sp + 56], crate::psxfront::abi_draw_shape_extended);
        }
        let mut at = LINES + 576;
        loop {
            let m = &mut c.m;
            let (x, y) = (hs(m, at + 4), hs(m, at + 6));
            at += 12;
            r.0[0] = at;
            r.0[2] += 1;
            let colour = m.r32(sp + 60);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[colour, x.wrapping_sub(2), y.wrapping_sub(2), 5, 3], 0x8004_4198);
            if r.0[2] as i32 >= 54 {
                break;
            }
        }
    })
}

/// The picture's slide's key frames (`AnimKeyPoints` of `slide_in` at its start and end).
fn key_points(c: &mut Cpu, r: &Saved, this: u32, slide_in: u32, ras: (u32, u32)) -> (u32, u32) {
    let from = port(c, r, 0x8004_3784, ras.0, &[this, slide_in, 1], abi_anim_key_points);
    c.m.w16(this + FROM, from as u16);
    let to = port(c, r, 0x8004_3784, ras.1, &[this, slide_in, 0], abi_anim_key_points);
    (from, to)
}

/// A stick's lean as one of 10 frames: the larger of its two axes' (`x`, `y` bytes about 128),
/// by 13 (`pos`'s frames one way, `neg`'s the other).
fn lean(m: &mut ps1rt::Mem, at: u32, pos: u32, neg: u32) -> u32 {
    let dx = 128u32.wrapping_sub(b(m, at + 1));
    let dy = 128u32.wrapping_sub(b(m, at));
    let a1 = if ((dx as i32).abs()) < (dy as i32).abs() { dy } else { dx };
    let v = sx(a1);
    if (v as i32) >= 0 {
        div13(v).wrapping_add(pos)
    } else {
        div13(0u32.wrapping_sub(v)).wrapping_add(neg)
    }
}

/// `tScreenControllerConfig::DrawController` (SCREENCONTROLLER.CPP, 0x800441cc): the rumble
/// tried with its menu's slider on (the big motor's or the small one's strength, the picture
/// shaking with it), else off; the picture's shapes uploaded once loaded; then: a NeGcon's
/// halo pulsing while settled; the picture sliding out, in (swapping between), or turning over
/// (with their sound) at its frame and fade; a NeGcon's or a stick's lean on the picture; an
/// analog pad's twist; the picture still; nothing (the labels reset).
pub fn abi_draw_controller(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[6], r.0[5], r.0[2], r.0[3]) = (this, 0, 0, 0, 0);
        let m = &mut c.m;
        let info = m.r32(this + 40);
        m.w32(sp + 52, info);
        let state = b(m, this + STATE);
        let all = menus(m);
        let big = ((state == 4 && hs(m, all + 11916) == 0) || (state == 6 && hs(m, all + 12148) == 0)) && hs(m, all + 11124) == 1;
        let mut on = false;
        if big {
            r.0[0] = FRONT_END;
            let p = m.r32(this + PLAYER);
            if m.r8(p.wrapping_add(FRONT_END) + 857) != 0 {
                if m.r16(this + SLIDER) != 0 {
                    m.w16(this + SLIDER, 0);
                    m.w32(this + RESTART, 1);
                }
                let p = m.r32(this + PLAYER);
                let s = b(m, p.wrapping_add(FRONT_END) + 857);
                port(c, &r, 0x8004_350c, 0x8004_42d4, &[this, s << 1], abi_set_actuators);
                let m = &mut c.m;
                let p = m.r32(this + PLAYER);
                let v = b(m, p.wrapping_add(FRONT_END) + 857);
                (r.0[2], r.0[3]) = ((v >> 5) + 2, (v >> 6) + 2);
                on = true;
            }
        }
        if !on {
            let m = &mut c.m;
            let state = b(m, this + STATE);
            let all = menus(m);
            let small = ((state == 4 && hs(m, all + 11916) == 1) || (state == 6 && hs(m, all + 12148) == 1)) && hs(m, all + 11124) == 1;
            if small {
                r.0[0] = FRONT_END;
                let p = m.r32(this + PLAYER);
                let s = b(m, p.wrapping_add(FRONT_END) + 859);
                if s != 0 {
                    port(c, &r, 0x8004_350c, 0x8004_4398, &[this, s << 1], abi_set_actuators);
                    let m = &mut c.m;
                    let p = m.r32(this + PLAYER);
                    let v = b(m, p.wrapping_add(FRONT_END) + 859);
                    (r.0[2], r.0[3]) = ((v >> 5) + 2, (v >> 6) + 2);
                    if hs(m, this + SLIDER) != 1 {
                        m.w16(this + SLIDER, 1);
                        m.w32(this + RESTART, 1);
                    }
                    on = true;
                }
            }
            if !on {
                c.m.w16(this + SLIDER, 0xFFFF);
                port(c, &r, 0x8004_3500, 0x8004_43e8, &[this], abi_clear_actuators);
            }
        }
        port(c, &r, 0x8004_359c, 0x8004_43f0, &[this], abi_shake_it);
        let s2 = r.0[2] << 16;
        let s0 = (s2 as i32 >> 16) as u32;
        (r.0[2], r.0[0]) = (s2, s0);
        if s0 != 0 || r.0[3] != 0 {
            let v = r.call_leaving(c, RAND, &[], 0x8004_4410);
            let rem = if s0 == 0 { 0 } else { (v as i32).wrapping_rem(s0 as i32) };
            r.0[6] = (rem as u32).wrapping_sub((s2 as i32 >> 17) as u32);
            let v = r.call_leaving(c, RAND, &[], 0x8004_4448);
            let a0 = r.0[3] << 16;
            let d = a0 as i32 >> 16;
            let rem = if d == 0 { 0 } else { (v as i32).wrapping_rem(d) };
            r.0[5] = (rem as u32).wrapping_sub((a0 as i32 >> 17) as u32);
        }
        port(c, &r, 0x8002_5e2c, 0x8004_4490, &[this, this + 40], crate::fescreen::abi_is_shape_file_loaded);
        if c.m.r32(this + 44) != 0 {
            port(c, &r, 0x8002_5ff0, 0x8004_44a8, &[this, 66], crate::fescreen::abi_upload_swap_shapes);
            return;
        }
        let m = &mut c.m;
        if m.r16(this + 58) & 1 == 0 {
            m.w16(this + ANIMATING, 0);
            return;
        }
        // a NeGcon's halo, pulsing while all's settled
        let settled = b(m, this + STATE).wrapping_sub(5) < 2 && m.r16(this + TURNING) == 0 && m.r32(this + OUT) == 0 && m.r16(this + DIR) == 0;
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
            (r.0[4], r.0[3], r.0[2]) = (SPOTS, sx(r.0[6]).wrapping_add(126), sx(r.0[5]));
            for k in 0..2 {
                r.0[0] = k;
                let m = &mut c.m;
                let at = SPOTS + 2 * b(m, this + STATE);
                let size = m.r32(HALO_SIZE);
                let x = b(m, at).wrapping_add(r.0[3]);
                let y = b(m, at + 1).wrapping_add(k + 63).wrapping_add(r.0[2]);
                let q = (if (size as i32) < 0 { size.wrapping_add(3) } else { size } as i32 >> 2) as u32;
                r.call_leaving(c, HALO, &[x, y, size, q, 21], 0x8004_4628);
            }
            r.0[0] = 2;
        }
        // the picture's slide out and in
        let m = &mut c.m;
        let mut swap = false;
        if m.r16(this + OUT) != 0 && m.r16(this + DIR) == 0 {
            let from = hu(m, this + OUT_FROM);
            m.w16(this + OUT, 0);
            m.w16(this + DIR, 0xFFFF);
            m.w16(this + SLID, from as u16);
            let (_, to) = key_points(c, &r, this, 0, (0x8004_4678, 0x8004_468c));
            let m = &mut c.m;
            let from = hu(m, this + FROM);
            m.w16(this + TO, to as u16);
            m.w16(this + FRAME, from as u16);
        } else {
            let mut tail = true;
            if m.r16(this + IN) != 0 {
                let dir = hs(m, this + DIR);
                if dir == 0 || (dir == 1 && m.r16(this + OUT) == 0) {
                    tail = false;
                    let to_state = hu(m, this + IN_TO);
                    m.w16(this + IN, 0);
                    m.w16(this + DIR, 1);
                    m.w16(this + SLID, to_state as u16);
                    let ras = if dir == 0 { (0x8004_46e0, 0x8004_46f4) } else { (0x8004_4744, 0x8004_4758) };
                    let (_, to) = key_points(c, &r, this, 1, ras);
                    let m = &mut c.m;
                    let from = hu(m, this + FROM);
                    let swapping = m.r16(this + SWAP) != 0;
                    m.w16(this + TO, to as u16);
                    m.w16(this + FRAME, from as u16);
                    if swapping {
                        if dir == 0 {
                            swap = true;
                        } else {
                            port(c, &r, 0x8004_3ad4, 0x8004_4774, &[this], abi_swap_in_controller);
                            c.m.w16(this + ANIMATING, 0);
                            c.m.w16(this + SWAP, 0);
                        }
                    }
                }
            }
            if tail {
                let m = &mut c.m;
                let mut pending = None;
                if m.r16(this + OUT) != 0 {
                    pending = Some(m.r16(this + SWAP));
                    m.w16(this + OUT, 0);
                } else if m.r16(this + IN) != 0 && hs(m, this + DIR) == 1 {
                    pending = Some(m.r16(this + SWAP));
                    m.w16(this + IN, 0);
                }
                swap = pending.is_some_and(|v| v != 0);
            }
        }
        if swap {
            port(c, &r, 0x8004_3ad4, 0x8004_47d0, &[this], abi_swap_in_controller);
            c.m.w16(this + SWAP, 0);
        }
        let m = &mut c.m;
        if m.r16(this + DIR) != 0 {
            if m.r16(this + ANIMATING) == 0 {
                let t = m.r32(GAME_TICKS);
                m.w16(this + ANIMATING, 1);
                m.w32(this + SOUNDED, 0);
                m.w32(this + START, t);
            }
            if (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32) >= 11 && m.r32(this + SOUNDED) == 0 {
                let s0 = hs(m, this + DIR);
                r.0[0] = s0;
                if s0 == 1 {
                    r.call_leaving(c, PLAY_SFX, &[15], 0x8004_4850);
                    c.m.w32(this + SOUNDED, s0);
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
            let mut s0 = (q.wrapping_abs() as u32) << 1;
            if m.r32(this + FAST) != 0 {
                s0 <<= 1;
            }
            if !((s0 as i32) < 129) {
                s0 = 128;
            }
            if hs(m, this + DIR) == 1 {
                s0 = 128u32.wrapping_sub(s0);
            }
            if m.r16(this + ANIMATING) == 0 {
                m.w16(this + DIR, 0);
            }
            r.0[0] = s0;
            let f = hs(m, this + FRAME);
            let frame = port(c, &r, 0x8004_3ed8, 0x8004_49a0, &[this, f], abi_calc_anim_frame);
            port(c, &r, 0x8004_3f64, 0x8004_4e5c, &[this, frame, s0, s0, 0, 0], abi_actual_draw_controller);
            return;
        }
        // turning over
        if m.r16(this + TURNING) != 0 {
            if m.r16(this + ANIMATING) == 0 {
                r.call_leaving(c, PLAY_SFX, &[15], 0x8004_49d8);
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
            let dt = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START));
            let a = r.call_leaving(c, I2F, &[dt], 0x8004_4a8c);
            r.0[0] = a;
            let m = &mut c.m;
            let dir = hs(m, this + TURN_DIR);
            let span = hs(m, this + TURN_TO).wrapping_sub(hs(m, this + TURN_FROM)).wrapping_mul(dir.wrapping_mul(6));
            let bf = r.call_leaving(c, I2F, &[span], 0x8004_4ab8);
            let q = r.call_leaving(c, FDIV, &[a, bf], 0x8004_4ac4);
            r.call_leaving(c, F2D, &[q], 0x8004_4acc);
            let (lo, hi) = (c.r[2], c.r[3]);
            r.call_leaving(c, DMUL, &[lo, hi, 0, 0x4070_0000], 0x8004_4ae0);
            let (lo, hi) = (c.r[2], c.r[3]);
            let mut s0 = r.call_leaving(c, D2I, &[lo, hi], 0x8004_4aec) as i32;
            if s0 >= 257 {
                s0 = 256;
            }
            if s0 >= 129 {
                s0 = 256 - s0;
            }
            s0 += 10;
            if s0 < 0 {
                s0 = 0;
            }
            if s0 >= 129 {
                s0 = 128;
            }
            r.0[0] = s0 as u32;
            let f = hs(&mut c.m, this + TURN_FRAME);
            let frame = port(c, &r, 0x8004_3ed8, 0x8004_4b3c, &[this, f], abi_calc_anim_frame);
            port(c, &r, 0x8004_3f64, 0x8004_4e5c, &[this, frame, 0, s0 as u32, 0, 0], abi_actual_draw_controller);
            return;
        }
        // a NeGcon's or a stick's lean
        if b(m, this + STATE).wrapping_sub(5) < 2 {
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
                port(c, &r, 0x8004_eb60, 0x8004_4d3c, &[shape, 1536, x, y, 0, 0, sp + 32], crate::psxfront::abi_draw_shape_extended);
            }
        }
        // an analog pad's twist
        let m = &mut c.m;
        let state = b(m, this + STATE);
        if state.wrapping_sub(1) < 2 {
            let all = menus(m);
            if m.r32(all + 11700) != 0 || m.r32(all + 11768) != 0 {
                let p = m.r32(this + PLAYER);
                let a0 = b(m, PAD_INFO.wrapping_add(p << 5) + 8).wrapping_sub(128);
                let frame = if (a0 as i32) >= 11 {
                    if state == 2 { 2u32.wrapping_add(div127(a0.wrapping_mul(13))) } else { 26u32.wrapping_add(div127(a0 << 3)) }
                } else if (a0 as i32) < -10 {
                    let a0 = 0u32.wrapping_sub(a0);
                    if state == 2 { 16u32.wrapping_add(div127(a0.wrapping_mul(13))) } else { 35u32.wrapping_add(div127(a0 << 3)) }
                } else {
                    (state == 2) as u32
                };
                port(c, &r, 0x8004_3f64, 0x8004_4e5c, &[this, frame, 0, 0, 0, 0], abi_actual_draw_controller);
                return;
            }
        }
        let m = &mut c.m;
        if m.r8(this + STATE) != 0 {
            let (dx, dy) = (sx(r.0[6]), sx(r.0[5]));
            port(c, &r, 0x8004_3f64, 0x8004_4e9c, &[this, 0, 0, 0, dx, dy], abi_actual_draw_controller);
        } else {
            m.w8(this + SHOWN, 0);
            m.w16(this + FADE, 128);
            m.w16(this + LETTERS, 0);
        }
    })
}

/// `tScreenControllerConfig::HorzVertLine` (SCREENCONTROLLER.CPP, 0x80044edc): one of an
/// arrow's two lines (its first or second leg, `second`), two pixels wide, drawn translucent
/// as far as its brightness says, a white tip at its end.
pub fn abi_horz_vert_line(c: &mut Cpu) {
    let (this, line, second) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        (r.0[6], r.0[5]) = (this, second);
        let m = &mut c.m;
        let (mut s1, mut s0, mut s3, mut s2) = (hs(m, line + 8), sx(hu(m, line + 4)), hs(m, line + 10), sx(hu(m, line + 6)));
        if second != 0 {
            (s1, s3) = (s0, s2);
            (s0, s2) = (hs(m, line), hs(m, line + 2));
        }
        (r.0[1], r.0[0], r.0[3], r.0[2], r.0[7]) = (s1, s0, s3, s2, 0x1E_1E1E);
        if (hs(m, this + FADE_STEP) as i32) > 0 {
            let f = hs(m, this + FADE);
            r.0[7] = port(c, &r, 0x8004_bd68, 0x8004_4f64, &[0x1E_1E1E, f], crate::fefades::abi_fade_out);
        }
        let s7 = r.0[7];
        let d = s1.wrapping_sub(s0);
        let m = &mut c.m;
        let lt = |a: u32, b: u32| (a as i32) < b as i32;
        let mul6 = |a: u32, b: u32| ((sx(a) as i32).wrapping_mul(b as i32) >> 6) as u32;
        if s0 == s1 {
            let s4 = if second == 0 { (!lt(s3, s2)) as u32 } else { 0 };
            let tip = if second == 1 { (!lt(s3, s2)) as u32 } else { 0 };
            let mut len = s3.wrapping_sub(s2);
            (r.0[4], r.0[0], r.0[2]) = (s4, tip, len);
            let bright = hs(m, this + BRIGHT);
            if bright != 64 {
                len = mul6(len, bright);
                r.0[2] = len;
                let t = sx(len);
                let e = (t as i32 >> 3) as u32;
                let y = s3.wrapping_sub(t.wrapping_sub(e).wrapping_sub(s4).wrapping_add(tip));
                r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0xFF_FFFF, s1, y, 2, 0u32.wrapping_sub(e), 2], 0x8004_5000);
            }
            let h = 0u32.wrapping_sub(sx(len).wrapping_sub(s4).wrapping_add(tip));
            r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[s7, s1, s3.wrapping_add(tip), 2, h, 2], 0x8004_50fc);
        } else {
            let tip = ((!lt(s1, s0)) as u32) << 1;
            let mut len = d;
            (r.0[0], r.0[4]) = (tip, len);
            let bright = hs(m, this + BRIGHT);
            if bright != 64 {
                len = mul6(d, bright);
                r.0[4] = len;
                let a1 = sx(len);
                if !lt(tip, (a1 as i32).unsigned_abs()) {
                    return;
                }
                let e = (a1 as i32 >> 3) as u32;
                let x = s1.wrapping_sub(a1.wrapping_sub(e)).wrapping_add(tip);
                r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0xFF_FFFF, x, s2, 0u32.wrapping_sub(e.wrapping_add(tip)), 1, 2], 0x8004_50b8);
            }
            let a3 = sx(len);
            if lt(tip, (a3 as i32).unsigned_abs()) {
                r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[s7, s1.wrapping_add(tip), s3, 0u32.wrapping_sub(a3.wrapping_add(tip)), 1, 2], 0x8004_50fc);
            }
        }
    })
}

/// `tScreenControllerConfig::DrawArrow` (SCREENCONTROLLER.CPP, 0x80045128): an arrow from a
/// label to its button: its first leg (if it has one) and its second, as bright as the
/// labels' fade lets them, translucent.
pub fn abi_draw_arrow(c: &mut Cpu) {
    let (this, line) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this, line);
        c.m.w16(this + BRIGHT, 0);
        r.call_leaving(c, SETTRANS, &[1], 0x8004_514c);
        port(c, &r, 0x8004_dbc0, 0x8004_5154, &[0], crate::mmeffect::abi_set_abr_mode);
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
            port(c, &r, 0x8004_4edc, 0x8004_51ac, &[this, line, 1], abi_horz_vert_line);
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
        port(c, &r, 0x8004_4edc, 0x8004_5204, &[this, line, 0], abi_horz_vert_line);
        port(c, &r, 0x8004_dbc0, 0x8004_520c, &[1], crate::mmeffect::abi_set_abr_mode);
        r.call_leaving(c, SETTRANS, &[0], 0x8004_5214);
    })
}

fn transition_is_finished(c: &mut Cpu, r: &Saved, ra: u32) -> u32 {
    let opts = menus(&mut c.m).wrapping_add(11116);
    port(c, r, 0x8001_c86c, ra, &[opts], crate::femenuoptions::abi_options_menu_transition_is_finished)
}

/// `tScreenControllerConfig::DrawBackground` (SCREENCONTROLLER.CPP, 0x80045228): the picture
/// slid in once the menu's in (or out as it goes), the pad checked while not sliding out, the
/// picture drawn; the screen's shapes faded with the menu.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let opts = menus(m).wrapping_add(11116);
        let v = (m.r32(opts + 116) as i32 >> 1) as u32;
        let s1 = if (sx(v) as i32) < 129 { v } else { 128 };
        r.0[1] = s1;
        if m.r32(this + MENU_IN) == 0
            && port(c, &r, 0x8001_c86c, 0x8004_5288, &[opts], crate::femenuoptions::abi_options_menu_transition_is_finished) != 0
        {
            c.m.w32(this + MENU_IN, 1);
        }
        let m = &mut c.m;
        if (sx(s1) as i32) < 129 && m.r32(this + SLID_IN) == 0 && m.r8(this + STATE) != 0 {
            m.w16(this + DIR, 1);
            let (_, to) = key_points(c, &r, this, 1, (0x8004_52dc, 0x8004_52f0));
            let m = &mut c.m;
            let (from, state) = (hu(m, this + FROM), b(m, this + STATE));
            m.w16(this + TO, to as u16);
            m.w32(this + SLID_IN, 1);
            m.w16(this + FRAME, from as u16);
            m.w16(this + SLID, state as u16);
        }
        let finished = transition_is_finished(c, &r, 0x8004_531c);
        let m = &mut c.m;
        if finished ^ 1 != 0 && m.r32(this + MENU_IN) != 0 && m.r32(this + SLIDING_OUT) == 0 && m.r8(this + STATE) != 0 {
            m.w16(this + DIR, 0xFFFF);
            let (_, to) = key_points(c, &r, this, 0, (0x8004_536c, 0x8004_5380));
            let m = &mut c.m;
            let (from, state) = (hu(m, this + FROM), b(m, this + STATE));
            m.w16(this + TO, to as u16);
            m.w32(this + SLIDING_OUT, 1);
            m.w32(this + FAST, 1);
            m.w16(this + FRAME, from as u16);
            m.w16(this + SLID, state as u16);
        }
        if c.m.r32(this + SLIDING_OUT) == 0 {
            port(c, &r, 0x8004_3b60, 0x8004_53bc, &[this, 0], abi_set_current_controller);
            port(c, &r, 0x8004_37e8, 0x8004_53c4, &[this], abi_check_configs);
        }
        let m = &mut c.m;
        if (m.r16(this + DIR) != 0 || m.r32(this + SLIDING_OUT) == 0) && m.r32(this + SLID_IN) != 0 {
            port(c, &r, 0x8004_41cc, 0x8004_53fc, &[this], abi_draw_controller);
        }
        let fade = sx(s1);
        r.0[1] = fade;
        for k in 0..12 {
            r.0[0] = k;
            port(c, &r, 0x8004_ef70, 0x8004_5428, &[k + 10, 0, 0, 0, fade, 0, 0], crate::psxfront::abi_scale_shape_extended);
        }
    })
}

/// `tScreenControllerConfig::DrawForeground` (SCREENCONTROLLER.CPP, 0x8004544c): the pad
/// checked; the labels faded out while anything changes, in once all's settled (their state
/// and kind taken when out); each label typed out letter by letter (the last letter brighter)
/// over a box, its arrow to its button (but under the dialog); the screen's front shapes.
pub fn abi_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 184, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        port(c, &r, 0x8004_3b60, 0x8004_5484, &[this, 0], abi_set_current_controller);
        port(c, &r, 0x8004_37e8, 0x8004_548c, &[this], abi_check_configs);
        c.m.w8(sp + 129, 0);
        let mut step = 8u32;
        let m = &mut c.m;
        if m.r32(this + CHANGED) == 0 && m.r16(this + TURNING) == 0 && m.r16(this + DIR) == 0 && m.r32(this + OUT) == 0
            && transition_is_finished(c, &r, 0x8004_54e0) != 0
        {
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
            if state == 6 {
                m.w8(this + SHOWN, 5);
            }
            let p = m.r32(this + PLAYER);
            let kind = b(m, p.wrapping_add(FRONT_END) + 866);
            m.w32(this + CHANGED, 0);
            m.w8(this + SHOWN_KIND, kind as u8);
            return;
        }
        if fade == 0 {
            let l = m.r16(this + LETTERS).wrapping_add(1);
            m.w16(this + LETTERS, l);
        }
        if (hs(m, this + LETTERS) as i32) >= 101 {
            m.w16(this + LETTERS, 100);
        }
        if m.r16(this + FADE) != 0 {
            port(c, &r, 0x8002_1b48, 0x8004_55d8, &[1, 1], crate::fetextrender::abi_set_abr);
        }
        if c.m.r8(this + SHOWN) != 0 {
            (r.0[5], r.0[7], r.0[4]) = (0, sp + 88, sp + 48);
            let m = &mut c.m;
            let kind = b(m, TEXT_KIND);
            let colours = m.r32(TEXT_COLORS + 4 * kind);
            r.0[8] = colours;
            let f = hs(m, this + FADE);
            let bright = port(c, &r, 0x8004_bd8c, 0x8004_5624, &[colours, 0xFF_FFFF, 100, f], crate::fefades::abi_fade_faded);
            c.m.w32(sp + 136, bright);
            let f = hs(&mut c.m, this + FADE);
            r.0[8] = port(c, &r, 0x8004_bd68, 0x8004_5634, &[colours, f], crate::fefades::abi_fade_out);
            let colour = r.0[8];
            loop {
                let m = &mut c.m;
                let row = b(m, this + SHOWN).wrapping_sub(1).wrapping_mul(3);
                let kind = b(m, this + SHOWN_KIND);
                let count = hs(m, LABEL_COUNTS.wrapping_add(row.wrapping_add(kind) << 1)) as i32;
                let k = sx(r.0[5]);
                if !((k as i32) < count) {
                    break;
                }
                let id = m.r8(LABEL_IDS.wrapping_add(k << 1).wrapping_add(kind << 5).wrapping_add(row << 5)) as i8 as i32 as u32;
                if id != u32::MAX {
                    let label = LABELS.wrapping_add(id.wrapping_mul(10));
                    for j in 0..5 {
                        let v = m.r16(label + 2 * j);
                        m.w16(sp + 32 + 2 * j, v);
                    }
                    let word = hs(m, sp + 36);
                    let text = r.call_leaving(c, TEXT_WORD, &[word], 0x8004_56e8);
                    let n = r.call_leaving(c, STRLEN, &[text], 0x8004_56f0);
                    r.0[0] = n;
                    let s1 = sx(n);
                    r.0[1] = s1;
                    let m = &mut c.m;
                    let letters = hs(m, this + LETTERS);
                    let a0 = if (s1 as i32) < letters as i32 { n } else { letters };
                    r.0[6] = n;
                    let s2 = sx(a0);
                    r.0[2] = s2;
                    let shown_text;
                    if (hs(m, this + LETTERS) as i32) < s1 as i32 {
                        let word = hs(m, sp + 36);
                        let t = r.call_leaving(c, TEXT_WORD, &[word], 0x8004_5738);
                        r.call_leaving(c, SPRINTF, &[sp + 88, t], 0x8004_5744);
                        let m = &mut c.m;
                        if m.r16(sp + 38) == 0 {
                            let mut s0 = 0u32;
                            r.0[0] = 0;
                            if 0 < hs(m, this + LETTERS) as i32 {
                                loop {
                                    let n = r.call_leaving(c, STRLEN, &[sp + 88], 0x8004_5770);
                                    let k = sx(s0);
                                    if !((k as i32) < sx(n) as i32) {
                                        break;
                                    }
                                    s0 = s0.wrapping_add(1);
                                    r.0[0] = s0;
                                    let m = &mut c.m;
                                    let ch = m.r8(sp + 88 + k);
                                    m.w8((sp + 48).wrapping_add(k), ch);
                                    if !((sx(s0) as i32) < hs(m, this + LETTERS) as i32) {
                                        break;
                                    }
                                }
                            }
                            let m = &mut c.m;
                            let k = sx(s0);
                            m.w8((sp + 48).wrapping_add(k), 0);
                            let ch = m.r8((sp + 47).wrapping_add(k));
                            m.w8(sp + 128, ch);
                        } else {
                            let mut s0 = n.wrapping_sub(s2);
                            r.0[0] = s0;
                            if (sx(s0) as i32) < s1 as i32 {
                                let skip = s1.wrapping_sub(s2);
                                loop {
                                    let k = sx(s0);
                                    s0 = s0.wrapping_add(1);
                                    let ch = m.r8((sp + 88).wrapping_add(k));
                                    m.w8((sp + 48).wrapping_add(k.wrapping_sub(skip)), ch);
                                    if !((sx(s0) as i32) < s1 as i32) {
                                        break;
                                    }
                                }
                                r.0[0] = s0;
                            }
                            let k = sx(s0);
                            let gone = sx(r.0[6]).wrapping_sub(s2);
                            m.w8((sp + 48).wrapping_add(k.wrapping_sub(gone)), 0);
                            let ch = m.r8(sp + 48);
                            m.w8(sp + 128, ch);
                        }
                        port(c, &r, 0x8002_0e38, 0x8004_5858, &[0], crate::fetextrender::abi_set_font);
                        let mut x = hs(&mut c.m, sp + 32);
                        r.0[1] = x;
                        let px = r.call_leaving(c, TEXT_PIXELS, &[sp + 48], 0x8004_5864);
                        r.0[0] = px;
                        let n2 = r.call_leaving(c, STRLEN, &[sp + 48], 0x8004_5870);
                        let w = px.wrapping_sub(n2);
                        r.0[0] = w;
                        let m = &mut c.m;
                        x = if hs(m, sp + 38) == 1 { x.wrapping_sub(w) } else { x.wrapping_add(w) };
                        r.0[1] = x;
                        r.0[0] = sp + 48;
                        let (y, col, left) = (hs(m, sp + 34), m.r32(sp + 136), (m.r16(sp + 38) == 0) as u32);
                        port(c, &r, 0x8002_0eac, 0x8004_58bc, &[sp + 128, sx(x), y, col, 0, left], crate::fetextrender::abi_full_text_rgb);
                        shown_text = sp + 48;
                    } else {
                        let word = hs(m, sp + 36);
                        shown_text = r.call_leaving(c, TEXT_WORD, &[word], 0x8004_58d0);
                        r.0[0] = shown_text;
                    }
                    let m = &mut c.m;
                    let (x, y, just) = (hs(m, sp + 32), hs(m, sp + 34), hs(m, sp + 38));
                    port(c, &r, 0x8002_0eac, 0x8004_58f4, &[shown_text, x, y, colour, 0, just], crate::fetextrender::abi_full_text_rgb);
                    let px = r.call_leaving(c, TEXT_PIXELS, &[shown_text], 0x8004_58fc);
                    r.0[0] = px;
                    let n3 = r.call_leaving(c, STRLEN, &[shown_text], 0x8004_5908);
                    let m = &mut c.m;
                    let mut w = px.wrapping_sub(n3);
                    let (x, just, y) = (hs(m, sp + 32), hs(m, sp + 38), hs(m, sp + 34));
                    if just != 0 {
                        w = 0u32.wrapping_sub(w);
                    }
                    r.call_ptr_leaving(c, DRAW_SQUARE, &[0, x, y, w, 7], 0x8004_5930);
                }
                r.0[5] = r.0[5].wrapping_add(1);
            }
        }
        // the arrows
        (r.0[5], r.0[2], r.0[1], r.0[0]) = (0, LABEL_COUNTS, LABEL_IDS, LINES);
        loop {
            let m = &mut c.m;
            let shown = b(m, this + SHOWN).wrapping_sub(1);
            let kind = b(m, this + SHOWN_KIND);
            let row = shown.wrapping_mul(3);
            let count = hs(m, LABEL_COUNTS.wrapping_add(row.wrapping_add(kind) << 1)) as i32;
            let k = sx(r.0[5]);
            if !((k as i32) < count) {
                break;
            }
            let dialog = this + DIALOG;
            let free = if m.r32(dialog + 112) != 0 { false } else { m.r32(dialog + 156) == 0 };
            if free && (shown as i32) >= 0 {
                let v = m.r8(LABEL_IDS.wrapping_add(k << 1).wrapping_add(kind << 5).wrapping_add(row << 5) + 1) as i8 as i32 as u32;
                if v != u32::MAX {
                    port(c, &r, 0x8004_5128, 0x8004_59f0, &[this, v.wrapping_mul(12).wrapping_add(LINES)], abi_draw_arrow);
                }
            }
            r.0[5] = r.0[5].wrapping_add(1);
        }
        port(c, &r, 0x8002_1b48, 0x8004_5a04, &[0, 0], crate::fetextrender::abi_set_abr);
        let m = &mut c.m;
        let all = menus(m);
        let v = (m.r32(all + 11232) as i32 >> 1) as u32;
        let v = if (sx(v) as i32) < 129 { v } else { 128 };
        let fade = sx(v);
        (r.0[5], r.0[0]) = (0, fade);
        for k in 0..2 {
            r.0[5] = k;
            port(c, &r, 0x8004_ef70, 0x8004_5a68, &[k + 22, 0, 0, 0, fade, 0, 0], crate::psxfront::abi_scale_shape_extended);
        }
    })
}

/// `tScreenControllerConfig::GetShapeInfo` (SCREENCONTROLLER.CPP, 0x80045ab4): 24 shapes of
/// `zpCtrl`, 66 swapped from `aCtrl`.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (count, swap, file) = (c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    c.m.w16(count, 24);
    c.m.w32(file, SHAPE_FILE);
    c.m.w16(swap, 66);
    c.m.w32(swap_file, SWAP_FILE);
    c.r[2] = SWAP_FILE;
}

/// `tScreenControllerConfig::Initialize` (SCREENCONTROLLER.CPP, 0x80045ae4): the rumble off,
/// the player's pad, no picture, the screen's; no menu, the pad's state found and its picture
/// loaded, the labels' state and kind, the configs checked; nothing animating, the labels out.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w8(this + SHAKING, 0);
        port(c, &r, 0x8004_3594, 0x8004_5afc, &[this], abi_turn_off_shakers);
        port(c, &r, 0x8004_3500, 0x8004_5b04, &[this], abi_clear_actuators);
        let m = &mut c.m;
        m.w32(this + RESTART, 1);
        let app = m.r32(FE_APP);
        m.w16(this + SLIDER, 0xFFFF);
        let player = b(m, app + 557);
        m.w32(this + QUIET_AT, 0);
        m.w16(this + PICTURE, 0xFFFF);
        m.w16(this + ANSWER, 0xFFFF);
        m.w32(this + PLAYER, player);
        port(c, &r, 0x8002_60cc, 0x8004_5b38, &[this], crate::fescreen::abi_initialize);
        c.m.w8(this + STATE, 0);
        let item = menus(&mut c.m).wrapping_add(11048);
        port(c, &r, 0x8001_de88, 0x8004_5b54, &[item, 1, 0], crate::femenuoptions::abi_sliding_menu_set_menu);
        port(c, &r, 0x8004_3b60, 0x8004_5b60, &[this, 1], abi_set_current_controller);
        port(c, &r, 0x8004_3ad4, 0x8004_5b68, &[this], abi_swap_in_controller);
        let m = &mut c.m;
        let labels = (hs(m, this + FADE) as i32) < 128;
        let state = b(m, this + STATE);
        m.w8(this + CHECKED, state as u8);
        if !labels {
            m.w8(this + SHOWN, state as u8);
        }
        if m.r8(this + SHOWN) == 6 {
            m.w8(this + SHOWN, 5);
        }
        let p = m.r32(this + PLAYER);
        let kind = b(m, p.wrapping_add(FRONT_END) + 866);
        m.w8(this + KIND, kind as u8);
        m.w8(this + SHOWN_KIND, kind as u8);
        port(c, &r, 0x8004_37e8, 0x8004_5bc0, &[this], abi_check_configs);
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
    })
}

/// `tScreenControllerConfig::Cleanup` (SCREENCONTROLLER.CPP, 0x80045c08): the rumble off, the
/// pads' communication restarted, the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8004_3500, 0x8004_5c1c, &[this], abi_clear_actuators);
        port(c, &r, 0x8004_3594, 0x8004_5c24, &[this], abi_turn_off_shakers);
        r.call_ptr_leaving(c, PAD_START_COM, &[], 0x8004_5c2c);
        port(c, &r, 0x8002_61c0, 0x8004_5c34, &[this], crate::fescreen::abi_cleanup);
    })
}

/// `tScreenControllerConfig::tScreenControllerConfig` (SCREENCONTROLLER.CPP, 0x80045c44): a
/// screen with its dialog, nothing animating, the first player.
pub fn abi_screen_controller(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8004_5c58, &[this], crate::fescreen::abi_screen);
        c.m.w32(this + 96, VTABLE);
        port(c, &r, 0x8001_9bcc, 0x8004_5c6c, &[this + DIALOG], crate::fedialog::abi_yes_no);
        let m = &mut c.m;
        for f in [ANIMATING, TURNING, IN, OUT] {
            m.w16(this + f, 0);
        }
        m.w32(this + PLAYER, 0);
        c.r[2] = this;
    })
}

/// `tScreenControllerConfig::GetHelpText` (SCREENCONTROLLER.CPP, 0x80045c94): the help's word
/// for the state (a NeGcon's, a digital pad's, an analog one's) or none.
pub fn abi_get_help_text(c: &mut Cpu) {
    let this = c.r[4];
    c.r[2] = match c.m.r8(this + STATE) {
        2 => 539,
        1 => 538,
        5 | 6 => 537,
        _ => u32::MAX,
    };
}

/// `tScreenControllerConfig::~tScreenControllerConfig` (SCREENCONTROLLER.CPP, 0x80045cf4):
/// the dialog's and the screen's.
pub fn abi_screen_controller_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this, flags);
        port(c, &r, 0x8002_6098, 0x8004_5d18, &[this + DIALOG, 2], crate::fescreen::abi_screen_destructor);
        port(c, &r, 0x8002_6098, 0x8004_5d24, &[this, flags], crate::fescreen::abi_screen_destructor);
    })
}

/// `tScreenControllerConfig::tScreenControllerConfig` entered as the original's callers `jal`
/// it.
pub(crate) fn screen_controller_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_5c44, None, &[]);
    abi_screen_controller(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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
        let kind = m.r8(FRONT_END + player + 866);
        m.w8(s + KIND, kind);
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
            ("tScreenControllerConfig::CheckConfigs", 0x8004_37e8, nfs4rc::f_800437e8, |c, _| {
                // (the state the one checked: a new one resets the pad, which waits with `timedwait`)
                let state = c.m.r8(SCREEN + STATE);
                c.m.w8(SCREEN + CHECKED, state);
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

    #[test]
    fn screencontroller_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xC1E + k as u64, shuffle, &args);
        }
    }
}
