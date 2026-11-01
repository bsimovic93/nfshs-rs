//! FEAPP.CPP: the front-end application (`tFEApplication`, `gFEApp`): its four dialogs, each
//! player's menu and screen (the ones being left and the next), the menus' stack to go back
//! through, the music each menu plays, the help's icons at the bottom, a frame drawn (each
//! player's half: menus' and screens' transitions, their drawing, the dialogs), and the main
//! loop that runs the front end until a race starts.
//!
//! Menus and screens are called through their vtables (a menu's vptr at +104, a screen's at
//! +96), with the registers the original has at its `jalr`.

use crate::abi::{self, Saved};
use crate::femenu::{PLAY_FE_SFX, method};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, GET_DRAWENV, SET_DRAW_AREA, SPRINTF, STRLEN, TEXT_PIXELS, TEXT_WORD, add_prim};
use crate::fetextrender;
use crate::globals::{FE_APP, FLIP, FRONT_END, GAME_TICKS, PAD_INFO, PLAYER1_VIEW, SCREEN_HEIGHT, SHAPES, SONGS_SCREEN};
use ps1rt::{Cpu, Mem};

// The application: +0 the music's menu flags (0xf000 of the first player's menu's), +4 each
// player's menu, +12 each one's screen, +20 the menu each goes to next, +28 the screen, +36 the
// menu being left, +44 a message dialog, +196 each player's menus gone through (16 each), +324
// how many, +332 each one's last key, +340 the half drawn's top, +344 the help dialog, +556 the
// player being run or drawn, +557 the player whose key came last, +560 each one's menu to be
// drawn over the one left, +568 the timed message dialog, +720 the message without input.
const MUSIC_FLAGS: u32 = 0;
const MENUS: u32 = 4;
const SCREENS: u32 = 12;
const LEAVING: u32 = 36;
const MESSAGE: u32 = 44;
const HALF_TOP: u32 = 340;
const HELP: u32 = 344;
const PLAYER: u32 = 556;
const OVERLAID: u32 = 560;
const TIMED_MESSAGE: u32 = 568;
const NO_INPUT_MESSAGE: u32 = 720;

/// A menu's vptr and its virtual calls used here (`TransitionOff` at 40, `IsTransitionOn` 56,
/// `TransitionIsFinished` 64, `Draw` 72, `UpdateTransition` 80); a screen's vptr, its `Cleanup`.
const MENU_VPTR: u32 = 104;
const MENU_TRANSITION_OFF: u32 = 40;
const MENU_IN_TRANSITION: u32 = 56;
const MENU_TRANSITION_IS_FINISHED: u32 = 64;
const MENU_DRAW: u32 = 72;
const MENU_UPDATE_TRANSITION: u32 = 80;
const SCREEN_VPTR: u32 = 96;
const SCREEN_CLEANUP: u32 = 56;

/// The dialogs' vtables: the base's, a message's, the help's, a message with a timeout, one
/// taking no input.
const DIALOG_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const HELP_VT: u32 = 0x8001_0548;
const TIMED_MESSAGE_VT: u32 = 0x8001_0090;
const NO_INPUT_VT: u32 = 0x8001_0428;

/// The music's volume (0x8007bc44), its songs (`AudioMus_PlaySong`, called by address: the host
/// logs it) by the menus' flags, and stopped; the front end's music volume (`frontEnd + 833`).
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
const PLAY_SONG: u32 = 0x8007_B9C4;
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
const SONGS: [(u32, u32, u32); 4] = [(0x1000, 0x8001_0044, 0x8001_3e80), (0x2000, 0x8001_004C, 0x8001_3e9c), (0x4000, 0x8001_0054, 0x8001_3eb8), (0x8000, 0x8001_0060, 0x8001_3ed4)];
const VOLUME: u32 = FRONT_END + 833;
/// The clock's start and end; whether frames wait for the vertical blank (`Draw_gDoVSync`).
const CLOCK_START: fn(&mut Cpu) = nfs4rc::f_8008c4d4;
const CLOCK_END: fn(&mut Cpu) = nfs4rc::f_8008c530;
const DO_VSYNC: u32 = 0x8013_EA90;

/// The help's icons: their colour, the help text, the options text; the pads' buttons' shapes
/// (the PlayStation pad's and the NeGcon's help and options buttons).
const ICON_COLOR: u32 = 0x0078_6E14;
const HELP_TEXT: u32 = 253;
const OPTIONS_TEXT: u32 = 254;
const NEGCON: u32 = 0x23;

/// The help's shapes (59 of them, the front end's shapes table) and `Texture_MenuReleaseClutId`.
const HELP_SHAPES: u32 = 59;
const RELEASE_CLUT: fn(&mut Cpu) = nfs4rc::f_800e05b0;

/// The frame's drawing (`FeAudio_systemtask`, `Draw_StartFrameRender`,
/// `Draw_StartRenderingView`, `Draw_StopRenderingView`, the frame shown).
const SYSTEM_TASK: fn(&mut Cpu) = crate::feaudio::systemtask_entry;
const START_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf044;
const START_VIEW: fn(&mut Cpu) = nfs4rc::f_800bedac;
const STOP_VIEW: fn(&mut Cpu) = nfs4rc::f_800bee9c;
const SHOW_FRAME: fn(&mut Cpu) = nfs4rc::f_800bf0f0;
/// The front end's menus (`*0x80051ee0`): +0 the "pad 1 out" menu item, +44 the "a pad out"
/// one, +176 the main menu.
const FE_MENUS: u32 = SONGS_SCREEN;
const PAD_OUT_ITEM: u32 = 0;
const A_PAD_OUT_ITEM: u32 = 44;
const MAIN_MENU: u32 = 176;
const NFS4_MENU: u32 = 176;
/// The keys those items get (down, up).
const KEY_DOWN: u32 = 1024;
const KEY_UP: u32 = 512;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// Player `p`'s word at `field`.
fn of(this: u32, field: u32, p: u32) -> u32 {
    this.wrapping_add(p << 2).wrapping_add(field)
}

/// The word at `field` of the player being run or drawn (`+556`).
fn current(m: &mut Mem, this: u32, field: u32) -> u32 {
    let p = m.r8(this + PLAYER) as u32;
    m.r32(of(this, field, p))
}

/// A call to a function that is ported too (its port entered as the original's caller enters
/// it): `f`, the original at `addr`, with `args` in a0.. (four at most), returning to `ra`.
/// `Redraw`: hooked (the host autosaves between front-end frames), so called by address;
/// `Cpu::call` routes it to the port after the host.
const REDRAW: u32 = 0x8001_3894;

fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// A dialog as the dialogs' constructors leave it: for either player, 288 wide, hidden.
fn dialog(m: &mut Mem, d: u32) {
    m.w32(d + 96, DIALOG_VT);
    m.w32(d + 112, 0);
    for f in [110, 130, 126, 124, 108, 106, 104, 102] {
        m.w16(d + f, 0);
    }
    m.w16(d + 128, 288);
    m.w16(d + 100, u16::MAX);
    m.w16(d + 136, 0);
    m.w32(d + 120, 0);
}

/// A message dialog (its text unfaded, on more than a line).
fn message(m: &mut Mem, d: u32) {
    m.w32(d + 96, MESSAGE_VT);
    m.w32(d + 148, 0);
    m.w32(d + 132, 0);
    m.w32(d + 120, 0);
    m.w32(d + 140, 128);
}

/// `tFEApplication::tFEApplication` (FEAPP.CPP, 0x800130f8): its four dialogs made (a message,
/// the help timing out after 1400 ticks, a message timing out after 1152, one taking no
/// input), no keys, no top times.
pub fn fe_application(c: &mut Cpu, this: u32) -> u32 {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[7] = this;
        let (d0, d1, d2, d3) = (this + MESSAGE, this + HELP, this + TIMED_MESSAGE, this + NO_INPUT_MESSAGE);
        r.0[0] = d0;
        r.enter_leaving(c, 0x8002_6040, 0x8001_3130, |c| crate::fescreen::screen(c, d0));
        r.0[1] = d1;
        r.0[2] = DIALOG_VT;
        r.0[5] = 288;
        r.0[4] = u32::MAX;
        r.0[3] = MESSAGE_VT;
        r.0[6] = 128;
        // (each dialog's fields set before the next one's screen is made, as the original
        // does)
        dialog(&mut c.m, d0);
        message(&mut c.m, d0);
        r.enter_leaving(c, 0x8002_6040, 0x8001_31a4, |c| crate::fescreen::screen(c, d1));
        r.0[0] = d2;
        let m = &mut c.m;
        dialog(m, d1);
        m.w32(d1 + 96, HELP_VT);
        m.w16(d1 + 144, u16::MAX);
        m.w32(d1 + 120, 1400);
        r.enter_leaving(c, 0x8002_6040, 0x8001_3200, |c| crate::fescreen::screen(c, d2));
        r.0[1] = d3;
        let m = &mut c.m;
        dialog(m, d2);
        message(m, d2);
        m.w32(d2 + 96, TIMED_MESSAGE_VT);
        m.w32(d2 + 120, 1152);
        r.enter_leaving(c, 0x8002_6040, 0x8001_326c, |c| crate::fescreen::screen(c, d3));
        let m = &mut c.m;
        dialog(m, d3);
        message(m, d3);
        m.w32(d3 + 96, NO_INPUT_VT);
        for p in 0..2 {
            m.w32(of(this, 872, p), 0);
            m.w32(of(this, 880, p), 0);
            m.w32(of(this, 888, p), u32::MAX);
        }
        this
    })
}

pub fn abi_fe_application(c: &mut Cpu) {
    c.r[2] = fe_application(c, c.r[4]);
}

/// `tFEApplication::~tFEApplication` (FEAPP.CPP, 0x8001331c): its dialogs' screens destroyed,
/// it deleted when asked.
pub fn abi_fe_application_dtor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[flags, this]);
        for (d, ra) in [(NO_INPUT_MESSAGE, 0x8001_3340), (TIMED_MESSAGE, 0x8001_334c), (HELP, 0x8001_3358), (MESSAGE, 0x8001_3364)] {
            r.enter_leaving(c, 0x8002_6098, ra, |c| {
                c.r[4] = this + d;
                c.r[5] = 2;
                crate::fescreen::abi_screen_destructor(c)
            });
        }
        r.0[0] = flags & 1;
        if flags & 1 != 0 {
            r.call_leaving(c, nfs4rc::f_800a4910, &[this], 0x8001_3378);
        }
    })
}

/// The music's volume, from the front end's setting (`* 35 / 64`).
fn music_volume(m: &mut Mem) -> u32 {
    let v = m.r8(VOLUME) as u32;
    (v.wrapping_mul(35) as i32 >> 6) as u32
}

/// `tFEApplication::PerformMenuInitialization` (FEAPP.CPP, 0x8001338c): no music flags, the
/// music's volume, no dialogs, the clock started, frames waiting for the vertical blank, the
/// text's blending off.
pub fn abi_perform_menu_initialization(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        c.m.w32(this + MUSIC_FLAGS, 0);
        let volume = music_volume(&mut c.m);
        r.call_leaving(c, MUSIC_VOLUME, &[volume], 0x8001_33bc);
        r.enter_leaving(c, 0x8001_8840, 0x8001_33c4, crate::fedialog::abi_initialize_class);
        r.call_leaving(c, CLOCK_START, &[], 0x8001_33cc);
        c.m.w32(DO_VSYNC, 1);
        r.call_leaving(c, fetextrender::set_abr_entry, &[0, 0], 0x8001_33e4);
    })
}

/// `tFEApplication::PerformMenuDestruction` (FEAPP.CPP, 0x800133f4): the clock ended, each
/// player's screen cleaned up and gone, the music stopped, frames not waiting, the text's
/// blending off.
pub fn perform_menu_destruction(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.call_leaving(c, CLOCK_END, &[], 0x8001_3410);
        for p in 0..2 {
            r.0[1] = p;
            let at = of(this, 0, p);
            r.0[0] = at;
            let screen = c.m.r32(at + SCREENS);
            if screen != 0 {
                let (obj, f) = method(&mut c.m, screen, SCREEN_VPTR, SCREEN_CLEANUP);
                r.call_ptr_leaving(c, f, &[obj], 0x8001_344c);
            }
            c.m.w32(at + SCREENS, 0);
        }
        r.0[1] = 2;
        r.call_leaving(c, STOP_SONG, &[1000], 0x8001_3470);
        c.m.w32(DO_VSYNC, 0);
        r.call_leaving(c, fetextrender::set_abr_entry, &[0, 0], 0x8001_3484);
    })
}

pub fn abi_perform_menu_destruction(c: &mut Cpu) {
    perform_menu_destruction(c, c.r[4]);
}

/// Whether either pad is plugged in and not a NeGcon (`not` false), or either is a NeGcon.
fn pads(m: &mut Mem, negcon: bool) -> bool {
    if negcon {
        return m.r8(PAD_INFO + 5) as u32 == NEGCON || m.r8(PAD_INFO + 37) as u32 == NEGCON;
    }
    (m.r8(PAD_INFO + 4) == 0 && m.r8(PAD_INFO + 5) as u32 != NEGCON) || (m.r8(PAD_INFO + 36) == 0 && m.r8(PAD_INFO + 37) as u32 != NEGCON)
}

/// `tFEApplication::DrawHelpIcons` (FEAPP.CPP, 0x8001349c): at the bottom left, the help
/// button (the PlayStation pad's and the NeGcon's) and "help", then the options button and
/// "options" when a player's menu has options; for a vertical menu (+100), "help" written
/// down the side a letter at a time, its buttons under it.
pub fn draw_help_icons(c: &mut Cpu, this: u32) {
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[5] = ICON_COLOR;
        r.0[3] = 14;
        c.m.w32(sp + 36, ICON_COLOR);
        r.0[2] = c.m.r32(SCREEN_HEIGHT).wrapping_sub(25);
        let menu = c.m.r32(this + MENUS);
        let place = sp + 32;
        let shape = |c: &mut Cpu, r: &Saved, k: u32, x: u32, y: u32, ra: u32| {
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k, 24, x, y, 0, 0, place], ra);
        };
        if menu != 0 && c.m.r16(menu + 100) != 0 {
            // down the side
            c.m.w8(sp + 73, 0);
            let word = r.call_leaving(c, TEXT_WORD, &[HELP_TEXT], 0x8001_3500);
            let text = sp + 56;
            r.0[1] = text;
            r.call_leaving(c, SPRINTF, &[text, word], 0x8001_3514);
            let n = r.call_leaving(c, STRLEN, &[text], 0x8001_351c);
            r.0[0] = n.wrapping_sub(1);
            if (r.0[0] as i32) >= 0 {
                r.0[4] = 0xE_0000;
                loop {
                    let y = sx(r.0[2]);
                    r.0[2] = r.0[2].wrapping_sub(8);
                    let b = c.m.r8(text.wrapping_add(r.0[0]));
                    r.0[0] = r.0[0].wrapping_sub(1);
                    c.m.w8(sp + 72, b);
                    r.call_leaving(c, fetextrender::full_text_rgb_entry, &[sp + 72, 14, y, ICON_COLOR, 0, 0], 0x8001_3560);
                    if (r.0[0] as i32) < 0 {
                        break;
                    }
                }
            }
            r.0[2] = r.0[2].wrapping_add(8);
            if pads(&mut c.m, false) {
                r.0[2] = r.0[2].wrapping_sub(8);
                shape(c, &r, 53, r.0[3], r.0[2], 0x8001_35d8);
            }
            if pads(&mut c.m, true) {
                shape(c, &r, 54, r.0[3].wrapping_sub(1), r.0[2].wrapping_sub(9), 0x8001_3624);
            }
            return;
        }
        if pads(&mut c.m, false) {
            shape(c, &r, 53, r.0[3], r.0[2], 0x8001_3698);
            r.0[3] = r.0[3].wrapping_add(14);
        }
        if pads(&mut c.m, true) {
            shape(c, &r, 54, r.0[3], r.0[2], 0x8001_36e8);
            r.0[3] = r.0[3].wrapping_add(15);
        }
        let word = r.call_leaving(c, TEXT_WORD, &[HELP_TEXT], 0x8001_36f4);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(r.0[3]), sx(r.0[2]), ICON_COLOR, 0, 0], 0x8001_3718);
        let word = r.call_leaving(c, TEXT_WORD, &[HELP_TEXT], 0x8001_3720);
        let w = r.call_leaving(c, TEXT_PIXELS, &[word], 0x8001_3728);
        r.0[0] = w;
        let word = r.call_leaving(c, TEXT_WORD, &[HELP_TEXT], 0x8001_3734);
        let len = r.call_leaving(c, STRLEN, &[word], 0x8001_373c);
        r.0[0] = w.wrapping_sub(len);
        r.0[3] = r.0[3].wrapping_add(5).wrapping_add(r.0[0]);
        let m = &mut c.m;
        let menu = m.r32(this + MENUS);
        let other = m.r32(this + MENUS + 4);
        let options = m.r32(menu + 92) != 0 || (other != 0 && m.r32(other + 92) != 0);
        if !options {
            return;
        }
        if pads(&mut c.m, false) {
            shape(c, &r, 55, r.0[3], r.0[2], 0x8001_37f0);
            r.0[3] = r.0[3].wrapping_add(14);
        }
        if pads(&mut c.m, true) {
            shape(c, &r, 56, r.0[3], r.0[2], 0x8001_3840);
            r.0[3] = r.0[3].wrapping_add(15);
        }
        let word = r.call_leaving(c, TEXT_WORD, &[OPTIONS_TEXT], 0x8001_384c);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(r.0[3]), sx(r.0[2]), ICON_COLOR, 0, 0], 0x8001_3870);
    })
}

pub fn abi_draw_help_icons(c: &mut Cpu) {
    draw_help_icons(c, c.r[4]);
}

/// The draw area for a half of the screen (from `top`, `h` high) linked into the ordering
/// table.
fn draw_area(c: &mut Cpu, r: &Saved, sp: u32, env: u32, top: u32, h: u32, ra: u32) {
    let m = &mut c.m;
    m.w16(sp + 16, 0);
    let y = (m.r16(env + 2) as u32).wrapping_add(top);
    m.w16(sp + 20, 512);
    m.w16(sp + 22, h as u16);
    m.w16(sp + 18, y as u16);
    let prim = add_prim(m);
    r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, sp + 16], ra);
}

/// `tFEApplication::Redraw` (FEAPP.CPP, 0x80013894): a frame: the dialogs, the help's icons,
/// a pad out told (its menu item given a key); each player's half from the second's up (its
/// menus' and screen's transitions, the screen's background, the menu or the one left under
/// the next, the screen's foreground), the frame shown.
pub fn redraw(c: &mut Cpu, this: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let was = c.m.r8(this + PLAYER) as u32;
        r.0[6] = was;
        r.call_leaving(c, SYSTEM_TASK, &[0], 0x8001_38c8);
        r.0[0] = 0x8014_0000;
        r.call_leaving(c, START_FRAME, &[], 0x8001_38d0);
        let view = c.m.r32(PLAYER1_VIEW);
        r.call_leaving(c, START_VIEW, &[view], 0x8001_38dc);
        let (view, flip) = (c.m.r32(PLAYER1_VIEW), c.m.r32(FLIP));
        let env = r.call_leaving(c, GET_DRAWENV, &[view, flip], 0x8001_38f0);
        r.0[5] = env;
        r.0[4] = if c.m.r32(this + MENUS + 4) != 0 {
            let h = c.m.r32(SCREEN_HEIGHT);
            h.wrapping_add(h >> 31) >> 1
        } else {
            c.m.r16(SCREEN_HEIGHT) as u32
        };
        r.enter_leaving(c, 0x8001_8860, 0x8001_392c, crate::fedialog::abi_draw_all_dialogs);
        r.enter_leaving(c, 0x8001_349c, 0x8001_3934, |c| draw_help_icons(c, this));
        // a pad out: the items that tell it set going
        let fe = c.m.r32(FE_MENUS);
        if c.m.r8(PAD_INFO + 4) != 0 || c.m.r8(PAD_INFO + 36) != 0 {
            let v = c.m.r32(fe + A_PAD_OUT_ITEM) | 1;
            c.m.w32(fe + A_PAD_OUT_ITEM, v);
            let menu = c.m.r32(this + MENUS);
            c.m.w32(sp + 32, KEY_UP);
            let k = c.m.r32(menu + 8);
            let item = c.m.r32(menu.wrapping_add(k << 2).wrapping_add(16));
            if item == fe + A_PAD_OUT_ITEM {
                port(c, &r, 0x8001_b390, 0x8001_39b4, &[fe + NFS4_MENU, 0, sp + 32, sp + 24], crate::femenuextended::abi_menu_nfs4_process_input);
            }
        } else {
            let v = c.m.r32(fe + A_PAD_OUT_ITEM) & !1;
            c.m.w32(fe + A_PAD_OUT_ITEM, v);
        }
        let fe = c.m.r32(FE_MENUS);
        if c.m.r8(PAD_INFO + 4) != 0 {
            let v = c.m.r32(fe + PAD_OUT_ITEM) | 1;
            c.m.w32(fe + PAD_OUT_ITEM, v);
            let menu = c.m.r32(this + MENUS);
            let k = c.m.r32(menu + 8);
            let item = c.m.r32(menu.wrapping_add(k << 2).wrapping_add(16));
            c.m.w32(sp + 36, KEY_DOWN);
            if item == fe {
                port(c, &r, 0x8001_b390, 0x8001_3a3c, &[item + MAIN_MENU, 0, sp + 36, sp + 24], crate::femenuextended::abi_menu_nfs4_process_input);
            }
        } else {
            let v = c.m.r32(fe + PAD_OUT_ITEM) & !1;
            c.m.w32(fe + PAD_OUT_ITEM, v);
        }
        r.0[3] = 1;
        r.0[2] = 0x00FF_FFFF;
        let h = c.m.r16(SCREEN_HEIGHT) as u32;
        draw_area(c, &r, sp, env, 0, h, 0x8001_3ae4);
        // each player's half, the second's first
        loop {
            let p = r.0[3] & 0xFF;
            c.m.w8(this + PLAYER, r.0[3] as u8);
            let top = p.wrapping_mul(sx(r.0[4]));
            c.m.w16(this + HALF_TOP, top as u16);
            let player = |m: &mut Mem| m.r8(this + PLAYER) as u32;
            let menu = c.m.r32(of(this, MENUS, p));
            if menu != 0 {
                let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_UPDATE_TRANSITION);
                r.call_ptr_leaving(c, f, &[obj], 0x8001_3b38);
            }
            let p = player(&mut c.m);
            let leaving = c.m.r32(of(this, LEAVING, p));
            if leaving != 0 {
                let (obj, f) = method(&mut c.m, leaving, MENU_VPTR, MENU_UPDATE_TRANSITION);
                r.call_ptr_leaving(c, f, &[obj], 0x8001_3b74);
            }
            let p = player(&mut c.m);
            let screen = c.m.r32(of(this, SCREENS, p));
            if screen != 0 {
                port(c, &r, 0x8002_6868, 0x8001_3b9c, &[screen], crate::fescreen::abi_update_transition);
            }
            let p = player(&mut c.m);
            let menu = c.m.r32(of(this, MENUS, p));
            if menu != 0 {
                let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
                if r.call_ptr_leaving(c, f, &[obj], 0x8001_3bd8) != 0 {
                    let p = player(&mut c.m);
                    let menu = c.m.r32(of(this, MENUS, p));
                    let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_DRAW);
                    r.call_ptr_leaving(c, f, &[obj], 0x8001_3c14);
                }
            }
            let p = player(&mut c.m);
            let screen = c.m.r32(of(this, SCREENS, p));
            if screen != 0 {
                port(c, &r, 0x8002_620c, 0x8001_3c3c, &[screen, 0], crate::fescreen::abi_draw);
            }
            let p = player(&mut c.m);
            let at = of(this, 0, p);
            let menu = c.m.r32(at + MENUS);
            if menu != 0 {
                let mut draw = true;
                if c.m.r32(at + OVERLAID) != 0 {
                    r.0[0] = 0;
                    let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_IN_TRANSITION);
                    if r.call_ptr_leaving(c, f, &[obj], 0x8001_3c88) == 0 {
                        let p = player(&mut c.m);
                        let menu = c.m.r32(of(this, MENUS, p));
                        let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
                        let finished = r.call_ptr_leaving(c, f, &[obj], 0x8001_3cc4);
                        r.0[0] = (finished == 0) as u32;
                    }
                    draw = r.0[0] != 0;
                } else {
                    r.0[0] = 0;
                }
                if draw {
                    let p = player(&mut c.m);
                    let menu = c.m.r32(of(this, MENUS, p));
                    let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_DRAW);
                    r.call_ptr_leaving(c, f, &[obj], 0x8001_3d04);
                }
                let p = player(&mut c.m);
                let leaving = c.m.r32(of(this, LEAVING, p));
                if leaving != 0 {
                    let (obj, f) = method(&mut c.m, leaving, MENU_VPTR, MENU_DRAW);
                    r.call_ptr_leaving(c, f, &[obj], 0x8001_3d40);
                }
            }
            let p = player(&mut c.m);
            let screen = c.m.r32(of(this, SCREENS, p));
            if screen != 0 {
                port(c, &r, 0x8002_620c, 0x8001_3d68, &[screen, 1], crate::fescreen::abi_draw);
            }
            let top = c.m.r16(this + HALF_TOP) as u32;
            draw_area(c, &r, sp, env, top, r.0[4], 0x8001_3ddc);
            r.0[3] = r.0[3].wrapping_sub(1);
            if (sx(r.0[3]) as i32) < 0 {
                break;
            }
        }
        c.m.w8(this + PLAYER, was as u8);
        let view = c.m.r32(PLAYER1_VIEW);
        r.call_leaving(c, STOP_VIEW, &[view], 0x8001_3e04);
        r.call_leaving(c, SHOW_FRAME, &[], 0x8001_3e0c);
    })
}

pub fn abi_redraw(c: &mut Cpu) {
    redraw(c, c.r[4]);
}

/// `tFEApplication::UpdateMusic` (FEAPP.CPP, 0x80013e34): the music's volume, the song the
/// menus' flags ask for (the menu's, the garage's, the showcase's, victory's), else none.
pub fn update_music(c: &mut Cpu, this: u32) {
    let volume = music_volume(&mut c.m);
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_leaving(c, MUSIC_VOLUME, &[volume], 0x8001_3e64);
        let flags = c.m.r32(this + MUSIC_FLAGS);
        r.0[0] = flags;
        match SONGS.iter().find(|s| flags & s.0 != 0) {
            Some(&(_, song, ra)) => {
                r.call_ptr_leaving(c, PLAY_SONG, &[song], ra);
            }
            None => {
                r.call_leaving(c, STOP_SONG, &[1000], 0x8001_3ee4);
            }
        }
    })
}

pub fn abi_update_music(c: &mut Cpu) {
    update_music(c, c.r[4]);
}

/// `tFEApplication::SetMenu` (FEAPP.CPP, 0x80013ef4): `player`'s next menu (the first player's
/// changing the music when its flags do); the menu now going off unless already finished; the
/// menu's screen next.
pub fn set_menu(c: &mut Cpu, this: u32, player: u32, menu: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[menu, this, player]);
        let p = sx(player);
        if menu == c.m.r32(of(this, MENUS, p)) {
            return;
        }
        if p == 0 {
            let flags = c.m.r32(menu) & 0xF000;
            if flags != c.m.r32(this + MUSIC_FLAGS) {
                c.m.w32(this + MUSIC_FLAGS, flags);
                r.enter_leaving(c, 0x8001_3e34, 0x8001_3f54, |c| update_music(c, this));
            }
        }
        let (obj, f) = method(&mut c.m, menu, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
        let finished = r.call_ptr_leaving(c, f, &[obj], 0x8001_3f70);
        let at = of(this, 0, sx(player));
        if finished != 1 {
            let now = c.m.r32(at + MENUS);
            c.m.w32(at + 20, menu);
            if now != 0 && menu != 0 {
                let (obj, f) = method(&mut c.m, now, MENU_VPTR, MENU_TRANSITION_OFF);
                r.call_ptr_leaving(c, f, &[obj], 0x8001_3fb8);
            }
        } else {
            c.m.w32(at + 20, menu);
        }
        let screen = c.m.r32(menu + 80);
        r.enter_leaving(c, 0x8001_3ff8, 0x8001_3fe0, |c| set_screen(c, this, sx(player), screen));
    })
}

pub fn abi_set_menu(c: &mut Cpu) {
    set_menu(c, c.r[4], c.r[5], c.r[6]);
}

/// `tFEApplication::SetScreen` (FEAPP.CPP, 0x80013ff8): `player`'s next screen, the screen now
/// going off.
pub fn set_screen(c: &mut Cpu, this: u32, player: u32, screen: u32) {
    abi::frame(c, 24, |c, _| {
        let at = of(this, 0, sx(player));
        let now = c.m.r32(at + SCREENS);
        if screen == now {
            return;
        }
        c.m.w32(at + 28, screen);
        if now == 0 || screen == 0 {
            return;
        }
        port(c, &Saved::of(c), 0x8002_6824, 0x8001_4038, &[now, 2, 0], crate::fescreen::abi_transition_off);
    })
}

pub fn abi_set_screen(c: &mut Cpu) {
    set_screen(c, c.r[4], c.r[5], c.r[6]);
}

/// `tFEApplication::DisplayHelp` (FEAPP.CPP, 0x80014048): the help shown on `page`.
pub fn abi_display_help(c: &mut Cpu) {
    let (this, page) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let help = this + HELP;
        c.m.w16(help + 144, page as u16);
        Saved::of(c).call_ptr_leaving(c, 0x8001_8980, &[help], 0x8001_405c);
    })
}

/// `FreeHelpShapeCluts` (FEAPP.CPP, 0x8001406c): the help shapes' CLUTs freed.
pub fn abi_free_help_shape_cluts(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = 0x8005_0000;
        for k in 0..HELP_SHAPES {
            r.0[0] = k;
            let at = k << 5;
            r.0[1] = at;
            let shape = c.m.r32(SHAPES).wrapping_add(at);
            if c.m.r32(shape + 4) != 0 {
                let clut = lh(&mut c.m, shape + 4);
                r.call_leaving(c, RELEASE_CLUT, &[clut], 0x8001_40b0);
                let shape = c.m.r32(SHAPES).wrapping_add(at);
                c.m.w32(shape + 4, 0);
            }
        }
    })
}

/// `tFEApplication::RunFrontEnd` (FEAPP.CPP, 0x8001585c): the main loop from the main menu.
pub fn abi_run_front_end(c: &mut Cpu) {
    let this = c.r[4];
    let main = c.m.r32(FE_MENUS).wrapping_add(MAIN_MENU);
    abi::frame(c, 24, |c, _| {
        c.r[2] = Saved::of(c).enter_leaving(c, 0x8001_43c8, 0x8001_5874, |c| main_loop(c, this, main));
    })
}

/// A menu's or screen's virtual call: `obj`'s method at `slot` (its vptr at `vptr`) with `args`
/// after the object, returning to `ra`.
fn vcall(c: &mut Cpu, r: &Saved, obj: u32, vptr: u32, slot: u32, args: &[u32], ra: u32) -> u32 {
    let (this, f) = method(&mut c.m, obj, vptr, slot);
    let mut a = vec![this];
    a.extend_from_slice(args);
    r.call_ptr_leaving(c, f, &a, ra)
}

/// A screen's virtual `Initialize` and `ProcessInput`; a menu's `Initialize`, `TransitionOn`,
/// key mask and `ProcessInput`.
const SCREEN_INITIALIZE: u32 = 48;
const SCREEN_PROCESS_INPUT: u32 = 72;
const SCREEN_TRANSITION_IS_FINISHED: u32 = 64;
const MENU_INITIALIZE: u32 = 16;
const MENU_TRANSITION_ON: u32 = 48;
const MENU_KEY_MASK: u32 = 32;
const MENU_PROCESS_INPUT: u32 = 24;
const DIALOG_PROCESS_INPUT: u32 = 72;

/// The memory left (`largestunused`, kept at 0x800519a0); `memset`; the race's start
/// (`GameSetup_gData + 0x24`: 0 a race, 2 quit).
const LARGEST_UNUSED: fn(&mut Cpu) = nfs4rc::f_800e5e70;
const MEMORY_LEFT: u32 = 0x8005_19A0;
const MEMSET: fn(&mut Cpu) = nfs4rc::f_800e4f04;
const GAME_START: u32 = 0x8011_44C8;
/// The commands' jump table (a menu command's type, 1..8) and its targets: to a menu (pushed
/// on the stack), to a menu (the stack emptied), to the second player's menu, back, a race,
/// the two players' race (pink slips saved first), a race, quit.
const COMMANDS: u32 = 0x8001_0070;
const GO_TO_PUSH: u32 = 0x8001_4D60;
const GO_TO_RESET: u32 = 0x8001_4DB8;
const GO_TO_SECOND: u32 = 0x8001_4DF8;
const BACK: u32 = 0x8001_4E88;
const RACE: u32 = 0x8001_5398;
const TWO_PLAYERS: u32 = 0x8001_51F4;
const QUIT: u32 = 0x8001_54A0;
/// The race type (`frontEnd + 4`: 6 pink slips), whether the player's car is from the garage
/// (`+298`), the cars chosen (stock `+92`, garage `+291`, pink slips `+293`), the car manager;
/// the pink slips' saving (its dialogs, the second card's message, `PlayerName`), the
/// memory card's start and end (0x800347d4, 0x80034938).
const RACE_TYPE: u32 = FRONT_END + 4;
const PINK_SLIPS: u32 = 6;
const CAR_MANAGER: u32 = 0x8011_552C;
const SAVE_PINK_SLIPS: fn(&mut Cpu) = crate::fememcard::save_pink_slips_with_dialogs_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const CARD_START: fn(&mut Cpu) = crate::fememcard::init_memcard_entry;
const CARD_END: fn(&mut Cpu) = crate::fememcard::deinit_memcard_entry;
const RUN_DEMO_VIDEO: fn(&mut Cpu) = run_demo_video_entry;
/// The front end idle this long (ticks) runs the demo video.
const IDLE: i32 = 3840;

/// `tFEApplication::MainLoop` (FEAPP.CPP, 0x800143c8): the front end from `first`: each frame,
/// for each player, the next menu taken when the one going off is done (or the screen's), the
/// second player's menu gone when the first's comes back, the next screen when the one going
/// off is done (cleaned up, the next initialised, coming on); the player's key (the help
/// dialog hidden by help; the top dialog's, the screen's, the menu's input), its command; the
/// frame drawn; the demo after a minute idle. Returns 0 for a race, 2 to quit.
pub fn main_loop(c: &mut Cpu, this: u32, first: u32) -> u32 {
    abi::frame(c, 408, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[0] = first;
        let (stamps, commands, keys) = (sp + 16, sp + 104, sp + 120);
        let (depth_at, push_at, draw_at, frame_at, active_at, mask_at, keys_at) = (sp + 336, sp + 344, sp + 348, sp + 352, sp + 356, sp + 360, sp + 364);
        c.m.w16(depth_at, u16::MAX);
        c.m.w32(push_at, 0);
        r.call_leaving(c, MEMSET, &[stamps, 0, 8], 0x8001_4418);
        c.m.w8(this + 557, 0);
        r.enter_leaving(c, 0x8001_338c, 0x8001_4424, abi_perform_menu_initialization);
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(active_at, ticks);
        c.m.w32(FLIP, 0);
        c.m.w8(this + PLAYER, 0);
        loop {
            let p = c.m.r8(this + PLAYER) as u32;
            for f in [324, MENUS, SCREENS, 20, 28, OVERLAID, 332, LEAVING] {
                let p = c.m.r8(this + PLAYER) as u32;
                c.m.w32(of(this, f, p), 0);
            }
            let next = (p + 1) as u8;
            c.m.w8(this + PLAYER, next);
            if next >= 2 {
                break;
            }
        }
        r.enter_leaving(c, 0x8001_3ef4, 0x8001_450c, |c| set_menu(c, this, 0, first));
        r.0[7] = commands;
        let player = |m: &mut Mem| m.r8(this + PLAYER) as u32;
        loop {
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w32(draw_at, 1);
            c.m.w32(keys_at, keys);
            c.m.w32(frame_at, ticks);
            c.m.w8(this + PLAYER, 0);
            while player(&mut c.m) < 2 {
                let p = player(&mut c.m);
                let at = of(this, 0, p);
                // the next menu, once the one going off is done
                if c.m.r32(at + 20) != 0 {
                    let now = c.m.r32(at + MENUS);
                    r.0[0] = 0;
                    let mut ready = true;
                    if now != 0 {
                        if vcall(c, &r, now, MENU_VPTR, MENU_IN_TRANSITION, &[], 0x8001_4590) != 0 {
                            let screen = current(&mut c.m, this, SCREENS);
                            if screen != 0 {
                                let v = vcall(c, &r, screen, SCREEN_VPTR, SCREEN_TRANSITION_IS_FINISHED, &[], 0x8001_45d4);
                                r.0[0] = (v != 0) as u32;
                            }
                        }
                        ready = r.0[0] != 0;
                        if ready {
                            let now = current(&mut c.m, this, MENUS);
                            r.0[0] = 0;
                            if now != 0 {
                                r.0[0] = vcall(c, &r, now, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[], 0x8001_461c);
                            }
                        }
                    }
                    if ready {
                        let p = player(&mut c.m);
                        let next = c.m.r32(of(this, 20, p));
                        let finished = vcall(c, &r, next, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[], 0x8001_4654);
                        let p = player(&mut c.m);
                        let leaving = if finished != 0 { c.m.r32(of(this, MENUS, p)) } else { 0 };
                        c.m.w32(of(this, LEAVING, p), leaving);
                        let p = player(&mut c.m);
                        let next = c.m.r32(of(this, 20, p));
                        c.m.w32(of(this, MENUS, p), next);
                        let p = player(&mut c.m);
                        let menu = c.m.r32(of(this, MENUS, p));
                        vcall(c, &r, menu, MENU_VPTR, MENU_INITIALIZE, &[], 0x8001_46dc);
                        let p = player(&mut c.m);
                        c.m.w32(of(this, 20, p), 0);
                        if r.0[0] != 0 && c.m.r32(this + 28) == 0 {
                            let at = of(this, 324, p);
                            let depth = c.m.r32(at).wrapping_sub(1);
                            c.m.w32(draw_at, 0);
                            c.m.w32(at, depth);
                        } else {
                            let menu = c.m.r32(of(this, MENUS, p));
                            vcall(c, &r, menu, MENU_VPTR, MENU_TRANSITION_ON, &[], 0x8001_475c);
                            if c.m.r32(push_at) != 0 {
                                let menu = current(&mut c.m, this, MENUS);
                                let second = c.m.r32(menu + 88);
                                c.m.w32(push_at, 0);
                                r.enter_leaving(c, 0x8001_3ef4, 0x8001_4794, |c| set_menu(c, this, 1, second));
                            }
                        }
                        let ticks = c.m.r32(GAME_TICKS);
                        c.m.w32(active_at, ticks);
                    }
                }
                // the second player's menu gone when the first's has come back
                if player(&mut c.m) == 1 {
                    let menu = c.m.r32(this + MENUS + 4);
                    if menu != 0
                        && vcall(c, &r, menu, MENU_VPTR, MENU_IN_TRANSITION, &[], 0x8001_47e8) != 0
                        && (c.m.r16(depth_at) as i16) < 0
                    {
                        let leaving = current(&mut c.m, this, LEAVING);
                        if leaving == 0 || vcall(c, &r, leaving, MENU_VPTR, MENU_IN_TRANSITION, &[], 0x8001_4840) != 0 {
                            let p = player(&mut c.m);
                            c.m.w32(of(this, MENUS, p), 0);
                            let p = player(&mut c.m);
                            let screen = c.m.r32(of(this, SCREENS, p));
                            if screen != 0 {
                                vcall(c, &r, screen, SCREEN_VPTR, SCREEN_CLEANUP, &[], 0x8001_4898);
                            }
                            let p = player(&mut c.m);
                            c.m.w32(of(this, SCREENS, p), 0);
                        }
                    }
                }
                // the next screen, once the one going off is done
                let p = player(&mut c.m);
                let at = of(this, 0, p);
                if c.m.r32(at + 28) != 0 {
                    let screen = c.m.r32(at + SCREENS);
                    let mut take = true;
                    if screen != 0 {
                        take = vcall(c, &r, screen, SCREEN_VPTR, SCREEN_TRANSITION_IS_FINISHED, &[], 0x8001_48f8) != 0;
                        if take {
                            let at = of(this, 0, player(&mut c.m));
                            take = c.m.r32(at + 20) == 0;
                            let screen = c.m.r32(at + SCREENS);
                            if take && screen != 0 {
                                vcall(c, &r, screen, SCREEN_VPTR, SCREEN_CLEANUP, &[], 0x8001_494c);
                            }
                        }
                    }
                    if take {
                        let p = player(&mut c.m);
                        let next = c.m.r32(of(this, 28, p));
                        c.m.w32(of(this, SCREENS, p), next);
                        let left = r.call_leaving(c, LARGEST_UNUSED, &[], 0x8001_4968);
                        c.m.w32(MEMORY_LEFT, left);
                        let screen = current(&mut c.m, this, SCREENS);
                        vcall(c, &r, screen, SCREEN_VPTR, SCREEN_INITIALIZE, &[], 0x8001_49a4);
                        let p = player(&mut c.m);
                        c.m.w32(of(this, 28, p), 0);
                        let screen = current(&mut c.m, this, SCREENS);
                        port(c, &r, 0x8002_6844, 0x8001_49d4, &[screen, 2, 0], crate::fescreen::abi_transition_on);
                    }
                }
                // the player's keys and command
                let menu = current(&mut c.m, this, MENUS);
                if menu != 0 {
                    let flags = c.m.r32(menu);
                    r.0[8] = player(&mut c.m);
                    r.0[1] = r.0[8];
                    let both = flags & 0x10 != 0 || (c.m.r8(FRONT_END + 3) == 1 && flags & 8 == 0);
                    if both {
                        r.0[1] = 0;
                        r.0[8] = 1;
                    }
                    let menu = current(&mut c.m, this, MENUS);
                    let flags = c.m.r32(menu);
                    if flags & 0x20 != 0 {
                        r.0[8] = c.m.r8(this + 557) as u32;
                        r.0[1] = r.0[8];
                    }
                    if flags & 0x40 != 0 {
                        r.0[8] = 0;
                        r.0[1] = 0;
                    }
                    if flags & 0x80 != 0 {
                        r.0[8] = 1;
                        r.0[1] = 1;
                    }
                    r.0[0] = 0;
                    if vcall(c, &r, menu, MENU_VPTR, MENU_IN_TRANSITION, &[], 0x8001_4abc) != 0 {
                        let screen = current(&mut c.m, this, SCREENS);
                        if screen != 0 {
                            let v = vcall(c, &r, screen, SCREEN_VPTR, SCREEN_TRANSITION_IS_FINISHED, &[], 0x8001_4b00);
                            r.0[0] = (v != 0) as u32;
                        }
                    }
                    r.0[6] = r.0[1];
                    if r.0[0] != 0 {
                        let menu = current(&mut c.m, this, MENUS);
                        let mask = vcall(c, &r, menu, MENU_VPTR, MENU_KEY_MASK, &[], 0x8001_4b40);
                        c.m.w32(mask_at, mask);
                        if let Some(code) = run_keys(c, &mut r, this, sp) {
                            return code;
                        }
                    }
                }
                let next = (player(&mut c.m) + 1) as u8;
                c.m.w8(this + PLAYER, next);
            }
            if c.m.r32(draw_at) != 0 {
                r.call_ptr_leaving(c, REDRAW, &[this], 0x8001_54f0);
            }
            let idle = c.m.r32(frame_at).wrapping_sub(c.m.r32(active_at)) as i32;
            if idle > IDLE {
                r.call_leaving(c, RUN_DEMO_VIDEO, &[this], 0x8001_5514);
                let ticks = c.m.r32(GAME_TICKS);
                c.m.w32(active_at, ticks);
            }
        }
    })
}

pub fn abi_main_loop(c: &mut Cpu) {
    c.r[2] = main_loop(c, c.r[4], c.r[5]);
}

/// `MainLoop`'s keys (from the first player its menu's flags allow to the last) and their
/// commands for the player being run: `Some` the loop's result when a race starts or the front
/// end is quit.
fn run_keys(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) -> Option<u32> {
    let (stamps, commands) = (sp + 16, sp + 104);
    let (depth_at, push_at, frame_at, active_at, mask_at, keys_at) = (sp + 336, sp + 344, sp + 352, sp + 356, sp + 360, sp + 364);
    let player = |m: &mut Mem| m.r8(this + PLAYER) as u32;
    loop {
        let k = sx(r.0[6]);
        r.0[0] = k;
        if (r.0[8] as i32) < (k as i32) {
            return None;
        }
        // the key
        c.m.w32(commands.wrapping_add(k << 3), 0);
        let mask = c.m.r32(mask_at);
        let key = r.enter_leaving(c, 0x8002_3ca8, 0x8001_4b6c, |c| crate::feinput::get_key_from_player(c, k, mask));
        let keys = c.m.r32(keys_at);
        let at = keys.wrapping_add(k << 2);
        c.m.w32(at, key);
        if key != 0 {
            c.m.w8(this + 557, r.0[6] as u8);
        }
        let elapsed = c.m.r32(GAME_TICKS).wrapping_sub(c.m.r32(stamps.wrapping_add(k << 2)));
        if elapsed >= 16 || mask & c.m.r32(at) == 0 {
            let v = c.m.r32(at);
            c.m.w32(this.wrapping_add(k << 2).wrapping_add(332), v);
        }
        let off = sx(r.0[6]) << 2;
        r.0[0] = off;
        let key_at = c.m.r32(keys_at).wrapping_add(off);
        r.0[1] = key_at;
        if c.m.r32(key_at) != 0 {
            let top = port(c, r, 0x8001_8970, 0x8001_4bf4, &[], crate::fedialog::abi_get_top_most_dialog);
            let now = c.m.r32(frame_at);
            r.0[0] = top;
            c.m.w32(active_at, now);
            c.m.w32(stamps.wrapping_add(off), now);
            let help = this + HELP;
            if c.m.r32(key_at) == 4 && c.m.r32(help + 112) != 0 {
                c.m.w32(key_at, 1);
                r.call_ptr_leaving(c, 0x8001_8a44, &[help], 0x8001_4c30);
            }
            let k = sx(r.0[6]);
            let (key, cmd) = (c.m.r32(keys_at).wrapping_add(k << 2), commands.wrapping_add(k << 3));
            if top != 0 && c.m.r32(key) != 4 {
                vcall(c, r, top, SCREEN_VPTR, DIALOG_PROCESS_INPUT, &[k, key, cmd], 0x8001_4c74);
            }
            let screen = current(&mut c.m, this, SCREENS);
            if screen != 0 {
                let key = c.m.r32(keys_at).wrapping_add(k << 2);
                vcall(c, r, screen, SCREEN_VPTR, SCREEN_PROCESS_INPUT, &[k, key, cmd], 0x8001_4cc4);
            }
            let key = c.m.r32(keys_at).wrapping_add(k << 2);
            if c.m.r32(key) != 1 {
                let menu = current(&mut c.m, this, MENUS);
                vcall(c, r, menu, MENU_VPTR, MENU_PROCESS_INPUT, &[k, key, cmd], 0x8001_4d1c);
            }
        }
        // its command
        let cmd = commands.wrapping_add(sx(r.0[6]) << 3);
        let kind = c.m.r32(cmd).wrapping_sub(1);
        let target = if kind < 8 { c.m.r32(COMMANDS + 4 * kind) } else { 0 };
        let p = player(&mut c.m);
        let push = |c: &mut Cpu, p: u32| {
            let m = &mut c.m;
            let depth = m.r32(of(this, 324, p));
            let menu = m.r32(of(this, MENUS, p));
            m.w32(this.wrapping_add((depth << 2).wrapping_add(p << 6)).wrapping_add(196), menu);
        };
        match target {
            GO_TO_PUSH | GO_TO_RESET => {
                let ra = if target == GO_TO_PUSH { 0x8001_4d68 } else { 0x8001_4dc0 };
                r.call_leaving(c, PLAY_FE_SFX, &[0], ra);
                let p = player(&mut c.m);
                if target == GO_TO_PUSH {
                    push(c, p);
                    let p = player(&mut c.m);
                    let v = c.m.r32(of(this, 324, p)).wrapping_add(1);
                    c.m.w32(of(this, 324, p), v);
                } else {
                    c.m.w32(of(this, 324, p), 0);
                }
                let p = player(&mut c.m);
                let menu = c.m.r32(cmd + 4);
                r.enter_leaving(c, 0x8001_3ef4, 0x8001_4df0, |c| set_menu(c, this, p, menu));
            }
            GO_TO_SECOND => {
                r.call_leaving(c, PLAY_FE_SFX, &[0], 0x8001_4e00);
                let p = player(&mut c.m);
                push(c, p);
                let p = player(&mut c.m);
                c.m.w32(push_at, 1);
                let v = c.m.r32(of(this, 324, p)).wrapping_add(1);
                c.m.w32(of(this, 324, p), v);
                let (p, menu) = (player(&mut c.m), c.m.r32(cmd + 4));
                let depth = c.m.r16(of(this, 324, p));
                c.m.w16(depth_at, depth);
                r.enter_leaving(c, 0x8001_3ef4, 0x8001_4e80, |c| set_menu(c, this, p, menu));
                c.m.w32(this + 328, 0);
            }
            BACK => back(c, r, this, sp),
            TWO_PLAYERS => {
                let other = of(this, OVERLAID, 1u32.wrapping_sub(p));
                if c.m.r32(other) == 0 {
                    c.m.w32(of(this, OVERLAID, p), 1);
                } else {
                    c.m.w32(other, 0);
                    if c.m.r8(RACE_TYPE) as u32 != PINK_SLIPS || !save_pink_slips(c, r, this, sp) {
                        return Some(start_race(c, r, this, sp));
                    }
                }
            }
            RACE => return Some(start_race(c, r, this, sp)),
            QUIT => {
                c.m.w32(GAME_START, 2);
                r.enter_leaving(c, 0x8001_33f4, 0x8001_54b4, |c| perform_menu_destruction(c, this));
                return Some(2);
            }
            _ => {}
        }
        r.0[6] = r.0[6].wrapping_add(1);
    }
}

/// `MainLoop`'s back: off the second player's options (its menus going off) or one menu back
/// down the player's stack, with a sound (the second player with none left: just the sound);
/// the players' overlaid menus put away first.
fn back(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) {
    let depth_at = sp + 336;
    let player = |m: &mut Mem| m.r8(this + PLAYER) as u32;
    let depth = |m: &mut Mem| m.r16(depth_at) as i16 as i32;
    let p = player(&mut c.m);
    let down = match p {
        0 => depth(&mut c.m) < c.m.r32(this + 324) as i32,
        1 => c.m.r32(this + 328) as i32 > 0,
        _ => false,
    };
    if !down {
        let p = player(&mut c.m);
        let at = of(this, OVERLAID, p);
        if c.m.r32(at) != 0 {
            c.m.w32(at, 0);
            return;
        }
        c.m.w32(of(this, OVERLAID, 1u32.wrapping_sub(p)), 0);
        let p = player(&mut c.m);
        c.m.w32(of(this, OVERLAID, p), 0);
    }
    let off = |c: &mut Cpu, r: &Saved, at: u32, ra: u32| {
        let menu = c.m.r32(this + at);
        if menu != 0 {
            vcall(c, r, menu, MENU_VPTR, MENU_TRANSITION_OFF, &[], ra);
        }
    };
    let p = player(&mut c.m);
    if p == 0 && c.m.r32(this + 324) as i32 == depth(&mut c.m) && c.m.r32(this + MENUS + 4) != 0 {
        // the second player's options going off
        off(c, r, LEAVING, 0x8001_4fa0);
        off(c, r, LEAVING + 4, 0x8001_4fcc);
        let menu = c.m.r32(this + MENUS + 4);
        c.m.w16(depth_at, u16::MAX);
        vcall(c, r, menu, MENU_VPTR, MENU_TRANSITION_OFF, &[], 0x8001_4ff4);
        let screen = c.m.r32(this + SCREENS + 4);
        port(c, r, 0x8002_6824, 0x8001_5004, &[screen, 2, 0], crate::fescreen::abi_transition_off);
        c.m.w32(this + 328, 0);
    } else if player(&mut c.m) == 1 {
        if c.m.r32(this + 328) as i32 > 0 {
            back_down(c, r, this);
            return;
        }
        if c.m.r32(this + MENUS + 4) != 0 {
            let to = (depth(&mut c.m) - 1) as u32;
            c.m.w32(this + 324, to);
            let menu = c.m.r32(this + MENUS + 4);
            vcall(c, r, menu, MENU_VPTR, MENU_TRANSITION_OFF, &[], 0x8001_5074);
            off(c, r, LEAVING, 0x8001_50a0);
            off(c, r, LEAVING + 4, 0x8001_50cc);
            c.m.w16(depth_at, u16::MAX);
            let screen = c.m.r32(this + SCREENS + 4);
            port(c, r, 0x8002_6824, 0x8001_50e4, &[screen, 2, 0], crate::fescreen::abi_transition_off);
            let depth0 = c.m.r32(this + 324);
            let menu = c.m.r32(this.wrapping_add(depth0 << 2).wrapping_add(196));
            r.enter_leaving(c, 0x8001_3ef4, 0x8001_5100, |c| set_menu(c, this, 0, menu));
        }
    }
    let p = player(&mut c.m);
    if c.m.r32(of(this, 324, p)) as i32 > 0 {
        back_down(c, r, this);
    } else if p == 1 {
        r.call_leaving(c, PLAY_FE_SFX, &[1], 0x8001_51ec);
    }
}

/// One menu back down the player's stack, with a sound (kept on it when the menu there is
/// already finished going on).
fn back_down(c: &mut Cpu, r: &mut Saved, this: u32) {
    let player = |m: &mut Mem| m.r8(this + PLAYER) as u32;
    r.call_leaving(c, PLAY_FE_SFX, &[1], 0x8001_5128);
    let p = player(&mut c.m);
    let depth = c.m.r32(of(this, 324, p)).wrapping_sub(1);
    c.m.w32(of(this, 324, p), depth);
    let p = player(&mut c.m);
    let depth = c.m.r32(of(this, 324, p));
    let menu = c.m.r32(this.wrapping_add((depth << 2).wrapping_add(p << 6)).wrapping_add(196));
    r.enter_leaving(c, 0x8001_3ef4, 0x8001_517c, |c| set_menu(c, this, p, menu));
    let menu = current(&mut c.m, this, MENUS);
    if vcall(c, r, menu, MENU_VPTR, MENU_TRANSITION_IS_FINISHED, &[], 0x8001_51b0) != 0 {
        let p = player(&mut c.m);
        let v = c.m.r32(of(this, 324, p)).wrapping_add(1);
        c.m.w32(of(this, 324, p), v);
    }
}

/// The two players' pink slips race: each player's car saved to the card at stake (the second
/// card's player told by name); whether the saving failed (back to the menus, the music
/// again).
fn save_pink_slips(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) -> bool {
    r.call_leaving(c, STOP_SONG, &[400], 0x8001_5250);
    r.call_leaving(c, CARD_START, &[0, 1], 0x8001_525c);
    r.0[3] = 0;
    r.0[1] = 0;
    r.0[4] = 0x8005_0000;
    let text = sp + 24;
    r.0[5] = text;
    while (r.0[1] as i32) < 2 && r.0[3] == 0 {
        let k = r.0[1];
        r.0[0] = c.m.r32(FE_APP).wrapping_add(NO_INPUT_MESSAGE);
        let word = r.call_leaving(c, TEXT_WORD, &[k + 662], 0x8001_528c);
        let dialog = c.m.r32(FE_APP).wrapping_add(NO_INPUT_MESSAGE);
        c.m.w32(r.0[0] + 144, word);
        r.call_ptr_leaving(c, 0x8001_8980, &[dialog], 0x8001_529c);
        loop {
            let app = c.m.r32(FE_APP);
            if c.m.r32(app + 852) == 1 {
                break;
            }
            r.call_ptr_leaving(c, REDRAW, &[app], 0x8001_52c0);
        }
        let car = c.m.r8(FRONT_END + 293 + k) as u32;
        r.0[3] = r.call_leaving(c, SAVE_PINK_SLIPS, &[sx(k), 0, car], 0x8001_52e4);
        if r.0[3] != 0 && k == 1 {
            let format = r.call_leaving(c, TEXT_WORD, &[664], 0x8001_5300);
            r.0[0] = format;
            let name = r.call_leaving(c, PLAYER_NAME, &[0], 0x8001_530c);
            r.call_leaving(c, SPRINTF, &[text, format, name, 1], 0x8001_5320);
            let app = c.m.r32(FE_APP);
            c.m.w32(app + 864, text);
            r.call_leaving(c, SAVE_PINK_SLIPS, &[0, 1, u32::MAX], 0x8001_5338);
        }
        r.0[1] = k + 1;
        let dialog = c.m.r32(FE_APP).wrapping_add(NO_INPUT_MESSAGE);
        r.call_ptr_leaving(c, 0x8001_8a44, &[dialog], 0x8001_5348);
    }
    r.call_leaving(c, CARD_END, &[], 0x8001_5358);
    if r.0[3] == 0 {
        return false;
    }
    r.enter_leaving(c, 0x8001_3e34, 0x8001_5368, |c| update_music(c, this));
    let volume = music_volume(&mut c.m);
    r.call_leaving(c, MUSIC_VOLUME, &[volume], 0x8001_5390);
    true
}

/// A race started: the player's car (a stock car, the garage's or the pink slips'), its sound,
/// 256 ticks of the sound going on, the menus' end. Returns 0.
fn start_race(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) -> u32 {
    let car = sp + 128;
    let p = c.m.r8(this + PLAYER) as u32;
    if c.m.r8(RACE_TYPE) as u32 == PINK_SLIPS {
        let k = c.m.r8(FRONT_END + 293 + p) as u32;
        port(c, r, 0x8001_71a0, 0x8001_5440, &[CAR_MANAGER, k, car, p], crate::fecars::abi_get_pink_slips_car);
    } else if c.m.r8(FRONT_END + 298) == 0 {
        let k = c.m.r8(FRONT_END + 92 + p) as u32;
        port(c, r, 0x8001_6fd8, 0x8001_53e4, &[CAR_MANAGER, k, car], crate::fecars::abi_get_stock_car);
    } else {
        let k = c.m.r8(FRONT_END + 291 + p) as u32;
        port(c, r, 0x8001_7094, 0x8001_5410, &[CAR_MANAGER, k, car, p], crate::fecars::abi_get_garage_car);
    }
    let sound = c.m.r8(sp + 320) as u32;
    if sound != 0 {
        r.call_leaving(c, PLAY_FE_SFX, &[sound], 0x8001_5458);
    }
    let from = c.m.r32(GAME_TICKS);
    r.0[0] = from;
    while c.m.r32(GAME_TICKS).wrapping_sub(from) < 256 {
        r.call_leaving(c, SYSTEM_TASK, &[0], 0x8001_5484);
    }
    c.m.w32(GAME_START, 0);
    r.enter_leaving(c, 0x8001_33f4, 0x8001_5498, |c| perform_menu_destruction(c, this));
    0
}

/// The memory's largest free block (noted at `MEMORY_FREE`), the demo video's number (the
/// two alternate), the music's volume setting.
const LARGEST_FREE: fn(&mut Cpu) = nfs4rc::f_800e5e70;
const MEMORY_FREE: u32 = 0x8005_19A0;
const DEMO_VIDEO: u32 = 0x8005_19A4;
/// The host's `ClearImage` and `DrawSync`.
const CLEAR_IMAGE: u32 = 0x800E_E35C;
const DRAW_SYNC: u32 = 0x800E_E1D8;

/// VRAM cleared (1024 by 512, black) and drawn.
fn clear_vram(c: &mut Cpu, r: &Saved, rect: u32, ras: (u32, u32)) {
    let m = &mut c.m;
    m.w16(rect, 0);
    m.w16(rect + 2, 0);
    m.w16(rect + 4, 1024);
    m.w16(rect + 6, 512);
    r.call_ptr_leaving(c, CLEAR_IMAGE, &[rect, 0, 0, 0], ras.0);
    r.call_ptr_leaving(c, DRAW_SYNC, &[0], ras.1);
}

/// `tFEApplication::RunDemoVideo` (FEAPP.CPP, 0x800140e8): from the main menu only: the music
/// stopped, the menu and the screen transitioned off (frames drawn until both are done), the
/// screen cleaned up, the front end's graphics freed, the demo video played over cleared VRAM;
/// everything back (the graphics, the fonts, the loading text, the help's shapes), the menu and
/// the screen initialised and on, the other demo video next time, the music at its volume.
pub fn abi_run_demo_video(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[5] = this;
        let all = c.m.r32(SONGS_SCREEN);
        if c.m.r32(this + MENUS) != all + 176 {
            return;
        }
        r.call_leaving(c, nfs4rc::f_8007b798, &[120], 0x8001_412c);
        port(c, &r, 0x8001_5a70, 0x8001_4134, &[0], crate::feaudio::abi_systemtask);
        let menu = c.m.r32(this + MENUS);
        vcall(c, &r, menu, 104, 40, &[], 0x8001_4158);
        let screen = c.m.r32(this + 12);
        port(c, &r, 0x8002_6824, 0x8001_4168, &[screen, 2, 0], crate::fescreen::abi_transition_off);
        loop {
            r.0[0] = 0;
            let menu = c.m.r32(this + MENUS);
            let mut busy = vcall(c, &r, menu, 104, 56, &[], 0x8001_418c) == 0;
            if !busy {
                let screen = c.m.r32(this + 12);
                busy = vcall(c, &r, screen, 96, 64, &[], 0x8001_41b8) == 0;
            }
            if !busy {
                break;
            }
            r.0[0] = 1;
            r.call_ptr_leaving(c, REDRAW, &[this], 0x8001_41d4);
            port(c, &r, 0x8001_5a70, 0x8001_41dc, &[0], crate::feaudio::abi_systemtask);
            c.tick();
        }
        let screen = c.m.r32(this + 12);
        vcall(c, &r, screen, 96, 56, &[], 0x8001_4208);
        r.0[0] = 0x8005_0000;
        r.call_leaving(c, nfs4rc::f_800bcb04, &[], 0x8001_4210);
        r.0[2] = 1024;
        r.call_leaving(c, nfs4rc::f_800bca4c, &[], 0x8001_4218);
        r.0[1] = 512;
        port(c, &r, 0x8004_dd18, 0x8001_4220, &[], crate::psxfront::abi_free_draw_memory);
        r.0[4] = 0x8005_0000;
        port(c, &r, 0x8002_6cb0, 0x8001_4228, &[], crate::fetools::abi_deinit);
        port(c, &r, 0x8001_406c, 0x8001_4230, &[], abi_free_help_shape_cluts);
        let free = r.call_leaving(c, LARGEST_FREE, &[], 0x8001_4238);
        let rect = sp + 56;
        r.0[3] = rect;
        c.m.w32(MEMORY_FREE, free);
        clear_vram(c, &r, rect, (0x8001_4264, 0x8001_426c));
        let k = c.m.r8(DEMO_VIDEO) as u32;
        port(c, &r, 0x8004_d3dc, 0x8001_4278, &[k], crate::movie::abi_play_movie);
        clear_vram(c, &r, rect, (0x8001_429c, 0x8001_42a4));
        let free = r.call_leaving(c, LARGEST_FREE, &[], 0x8001_42ac);
        c.m.w32(MEMORY_FREE, free);
        port(c, &r, 0x8004_dca4, 0x8001_42b4, &[], crate::psxfront::abi_allocate_draw_memory);
        port(c, &r, 0x8002_6bc8, 0x8001_42bc, &[], crate::fetools::abi_init);
        port(c, &r, 0x8002_5ad8, 0x8001_42c4, &[], crate::fescreen::abi_display_loading_text);
        r.call_leaving(c, nfs4rc::f_800bc958, &[0xD800, 0x1_8000], 0x8001_42d4);
        r.call_leaving(c, nfs4rc::f_800780f0, &[], 0x8001_42dc);
        port(c, &r, 0x8004_f09c, 0x8001_42e4, &[], crate::psxfront::abi_load_all_help_shapes);
        let free = r.call_leaving(c, LARGEST_FREE, &[], 0x8001_42ec);
        let menu = c.m.r32(this + MENUS);
        c.m.w32(MEMORY_FREE, free);
        vcall(c, &r, menu, 104, 16, &[], 0x8001_4310);
        let screen = c.m.r32(this + 12);
        vcall(c, &r, screen, 96, 48, &[], 0x8001_4334);
        let menu = c.m.r32(this + MENUS);
        vcall(c, &r, menu, 104, 48, &[], 0x8001_4358);
        let screen = c.m.r32(this + 12);
        port(c, &r, 0x8002_6844, 0x8001_4368, &[screen, 2, 0], crate::fescreen::abi_transition_on);
        let k = c.m.r32(DEMO_VIDEO);
        c.m.w32(DEMO_VIDEO, 5u32.wrapping_sub(k));
        port(c, &r, 0x8001_3e34, 0x8001_4380, &[this], abi_update_music);
        let v = c.m.r8(VOLUME) as u32;
        r.call_leaving(c, nfs4rc::f_8007bc44, &[(v.wrapping_mul(35) as i32 >> 6) as u32], 0x8001_43a4);
    })
}

/// The race's first car's stats; the track raced; the names' grids' names and screen.
const RACE_STATS: u32 = 0x8011_0E08;
const TRACK_RACED: u32 = 0x8013_DAEC;
const NAMES: u32 = 0x8011_5C1C;
const NAME_SCREEN: u32 = 0x8005_2E44;

/// The name entry for player `p` (its grid's menu at `grid`, the screen's menu at `menu`).
fn name_entry(m: &mut Mem, grid: u32, menu: u32, p: u32) -> u32 {
    let all = m.r32(SONGS_SCREEN);
    let (grid, menu) = (all + grid, all + menu);
    m.w16(grid + 128, p as u16);
    m.w32(grid + 28, NAMES + 8 * p);
    let screen = m.r32(NAME_SCREEN);
    m.w16(grid + 32, 7);
    m.w16(grid + 36, 0);
    m.w16(grid + 34, 0);
    m.w32(screen + 100, menu);
    menu
}

/// `tFEApplication::RunPostGame` (FEAPP.CPP, 0x80015554): after a race (but a test drive or
/// the pink slips'): the new records cleared, both pads reset, no names wanted; a record lap
/// (whose: one player's, or two players' first or second) or top times (lap or race, first or
/// second) noted for each player as the records to enter their name for; then the front end
/// from the first player's name entry, the second's, or the post-game menu.
pub fn abi_run_post_game(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[2]) = (FRONT_END, this);
        let kind = c.m.r8(RACE_TYPE) as u32;
        let mut menu = None;
        if kind != PINK_SLIPS && !(kind == 0 && c.m.r8(FRONT_END + 298) == 0) {
            r.0[5] = RACE_STATS;
            port(c, &r, 0x8004_ad14, 0x8001_55ac, &[], crate::statchk::abi_clear_new_records);
            (r.0[4], r.0[3]) = (FRONT_END, u32::MAX);
            for p in 0..2u32 {
                (r.0[1], r.0[0]) = (p + 1, this + 4 * p);
                let pad = c.m.r8(FRONT_END + p + 866) as u32;
                port(c, &r, 0x8002_7104, 0x8001_55d0, &[p, pad], crate::front::abi_reset_psx_controller);
                let m = &mut c.m;
                m.w32(this + 4 * p + 872, 0);
                m.w32(this + 4 * p + 880, 0);
                m.w32(this + 4 * p + 888, u32::MAX);
            }
            r.0[0] = this + 8;
            let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
            if port(c, &r, 0x8004_9cc8, 0x8001_55fc, &[RACE_STATS, track, sp + 24], crate::statchk::abi_is_record_lap_time) != 0 {
                let m = &mut c.m;
                let who = m.r16(sp + 24) as i16 as i32 as u32;
                let v = if m.r8(FRONT_END + 3) == 1 { if who != 0 { 8 } else { 7 } } else { 6 };
                m.w32(this.wrapping_add(who << 2) + 888, v);
                m.w32(this.wrapping_add(who << 2) + 880, 1);
            }
            let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
            let top = sx(port(c, &r, 0x8004_a194, 0x8001_566c, &[RACE_STATS, track], crate::statchk::abi_is_top_time));
            let m = &mut c.m;
            if top != 0 {
                let who = m.r16(sp + 24) as i16 as i32;
                if m.r8(FRONT_END + 3) == 1 {
                    if top & 3 != 0 {
                        m.w32(this + 880, 1);
                        let v = if m.r32(this + 888) == u32::MAX || who != 0 {
                            if top & 2 != 0 { 1 } else { 4 }
                        } else if top & 2 != 0 {
                            10
                        } else {
                            13
                        };
                        m.w32(this + 888, v);
                    }
                    if top & 0xC != 0 {
                        m.w32(this + 884, 1);
                        let v = if m.r32(this + 892) != u32::MAX && who == 1 {
                            if top & 8 != 0 { 11 } else { 14 }
                        } else if top & 8 != 0 {
                            2
                        } else {
                            5
                        };
                        m.w32(this + 892, v);
                    }
                } else {
                    m.w32(this + 880, 1);
                    let v = if m.r32(this + 888) == u32::MAX {
                        if top & 2 != 0 { 0 } else { 3 }
                    } else if top & 2 != 0 {
                        9
                    } else {
                        12
                    };
                    m.w32(this + 888, v);
                }
            }
            if m.r32(this + 880) != 0 {
                menu = Some(name_entry(m, 12728, 14032, 0));
            } else if m.r32(this + 884) != 0 {
                menu = Some(name_entry(m, 12868, 14160, 1));
            }
        }
        let menu = match menu {
            Some(v) => v,
            None => {
                port(c, &r, 0x8002_dbc4, 0x8001_5828, &[sp + 16], crate::femenudefs::abi_post_game_menu);
                c.m.r32(sp + 20)
            }
        };
        c.r[2] = port(c, &r, 0x8001_43c8, 0x8001_5838, &[this, menu], abi_main_loop);
    })
}

/// `tFEApplication::RunDemoVideo` entered as the original's callers `jal` it.
fn run_demo_video_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_40e8, None, &[]);
    abi_run_demo_video(c);
}

/// `tFEApplication::RunPostGame` entered as the original's callers `jal` it.
pub(crate) fn run_post_game_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_5554, None, &[]);
    abi_run_post_game(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, MENU, MENU2, NEW, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The application's size (to its last field, +896) and where a copy goes.
    const APP_SIZE: u32 = 900;
    const COPY: u32 = 0x8014_4000;
    /// The pads' library state (`PAD_update` copies it), see FEINPUT's tests.
    const PAD_GLOBAL: u32 = 0x8013_FBF8;
    /// Routines returning 0 and 1 (interpreted), the test menus' vtables copied.
    const STUBS: u32 = 0x8014_7800;
    const FAKE_VTS: u32 = 0x8014_7900;

    fn app(m: &mut Mem) -> u32 {
        m.r32(FE_APP)
    }

    /// FEMENU's structures, then the pads plugged in or not and their kinds; the game's
    /// application with each player's menus (the game's, FEMENU's test menus, none for the
    /// second) and their options and vertical flags, the menus left and overlaid, the half
    /// drawn; the music's flags; a copy of it.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for p in 0..2 {
            let status = (pick(rng, 3) == 0) as u8;
            let kind = [0x41, 0x73, 0x23][pick(rng, 3) as usize] as u8;
            m.w8(PAD_INFO + 32 * p + 4, status);
            m.w8(PAD_INFO + 32 * p + 5, kind);
            m.w8(PAD_GLOBAL + 34 * p, status);
            m.w8(PAD_GLOBAL + 34 * p + 1, kind);
        }
        let a = app(m);
        let real = m.r32(a + MENUS);
        m.w32(a + MENUS, [real, real, MENU, MENU2][pick(rng, 4) as usize]);
        m.w32(a + MENUS + 4, [0, 0, MENU2][pick(rng, 3) as usize]);
        for menu in [MENU, MENU2] {
            m.w16(menu + 100, (pick(rng, 3) == 0) as u16);
        }
        m.w32(a + LEAVING, [0, MENU2][pick(rng, 2) as usize]);
        for p in 0..2 {
            m.w32(of(a, OVERLAID, p), pick(rng, 2));
        }
        // (the test menus' transitions answering either way: their vtables copied, the two
        // transition calls made routines returning 0 or 1)
        for (k, w) in [0x03E0_0008, 0x0000_1021, 0x03E0_0008, 0x2402_0001].into_iter().enumerate() {
            m.w32(STUBS + 4 * k as u32, w);
        }
        for (menu, vt) in [(MENU, FAKE_VTS), (MENU2, FAKE_VTS + 0x100)] {
            let real_vt = m.r32(menu + MENU_VPTR);
            for k in 0..32 {
                let v = m.r32(real_vt + 4 * k);
                m.w32(vt + 4 * k, v);
            }
            for slot in [MENU_IN_TRANSITION, MENU_TRANSITION_IS_FINISHED] {
                let f = m.r32(real_vt + slot + 4);
                m.w32(vt + slot + 4, [f, STUBS, STUBS + 8][pick(rng, 3) as usize]);
            }
            m.w32(menu + MENU_VPTR, vt);
        }
        m.w32(a + MUSIC_FLAGS, [0, 0x1000, 0x2000][pick(rng, 3) as usize]);
        m.w8(a + PLAYER, pick(rng, 2) as u8);
        for k in 0..APP_SIZE {
            let b = m.r8(a + k);
            m.w8(COPY + k, b);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tFEApplication", 0x8001_30f8, nfs4rc::f_800130f8, |c, _| {
                for k in 0..APP_SIZE / 4 {
                    c.m.w32(NEW + 4 * k, 0);
                }
                vec![NEW]
            }),
            ("~tFEApplication", 0x8001_331c, nfs4rc::f_8001331c, |_, r| vec![COPY, 2 * pick(r, 2)]),
            ("PerformMenuInitialization", 0x8001_338c, nfs4rc::f_8001338c, |c, _| vec![app(&mut c.m)]),
            ("PerformMenuDestruction", 0x8001_33f4, nfs4rc::f_800133f4, |c, _| vec![app(&mut c.m)]),
            ("DrawHelpIcons", 0x8001_349c, nfs4rc::f_8001349c, |c, _| vec![app(&mut c.m)]),
            ("Redraw", 0x8001_3894, nfs4rc::f_80013894, |c, _| vec![app(&mut c.m)]),
            ("SetMenu", 0x8001_3ef4, nfs4rc::f_80013ef4, |c, r| {
                let a = app(&mut c.m);
                let now = c.m.r32(a + MENUS);
                vec![a, pick(r, 2), [now, MENU, MENU2][pick(r, 3) as usize]]
            }),
            ("SetScreen", 0x8001_3ff8, nfs4rc::f_80013ff8, |c, r| {
                let a = app(&mut c.m);
                let now = c.m.r32(a + SCREENS);
                vec![a, pick(r, 2), [0, now, a + MESSAGE][pick(r, 3) as usize]]
            }),
            ("DisplayHelp", 0x8001_4048, nfs4rc::f_80014048, |c, r| vec![app(&mut c.m), pick(r, 3)]),
            ("FreeHelpShapeCluts", 0x8001_406c, nfs4rc::f_8001406c, |_, _| vec![]),
        ]
    }

    #[test]
    fn feapp_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xA990 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 150, 0xA991 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn feapp_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 80, 0xA9A1 + k as u64, shuffle, &args);
        }
    }
}
