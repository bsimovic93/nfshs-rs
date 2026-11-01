//! FEMENUDEFS.CPP: the front end's menus' callbacks (`MenuExtended_*`): the race kinds and
//! modes chosen (one or two players, test drive, single race, tournament, special event, hot
//! pursuit, duel, full grid, pink slips, best of one, three or five), the screens gone to (the
//! car select, the dealer, the seller, the upgrades, the showroom, the garage, the track infos,
//! the trophy rooms), buying and selling cars and upgrades, saving and loading the game, the
//! post-race menus, the players' names; each sets its `tMenuCommand` (+0 the command, +4 the
//! menu).

use crate::abi::{self, Saved};
use crate::globals::FRONT_END as FE;
use ps1rt::Cpu;

/// The menus, the main screen, the car select, the car manager, the second pad's state.
const ALL_MENUS: u32 = 0x8005_1EE0;
const MAIN_SCREEN: u32 = 0x8005_22E8;
const CAR_SELECT: u32 = 0x8005_24CC;
const CAR_MANAGER: u32 = 0x8011_552C;
const PAD2_STATE: u32 = 0x8013_FBC8;
/// The yes-or-no dialog's vtable (a local one's).
const YES_NO_VT: u32 = 0x8001_02A8;

const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;

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

fn menus(m: &mut ps1rt::Mem) -> u32 {
    m.r32(ALL_MENUS)
}

/// The command: `kind` (0 none, 1 a menu, 2 a transition) to the menus' `menu` (an offset).
fn command(m: &mut ps1rt::Mem, cmd: u32, kind: u32, menu: u32) {
    let all = menus(m);
    m.w32(cmd, kind);
    m.w32(cmd + 4, all + menu);
}

/// The main screen's background swapped for the menu's (`SwapBackground(-1)`).
fn swap_background(c: &mut Cpu, r: &Saved, ra: u32) {
    let main = c.m.r32(MAIN_SCREEN);
    port(c, r, 0x8003_7118, ra, &[main, u32::MAX], crate::screenmain::abi_swap_background);
}

/// The race settings `[(offset, value)]` in the front end's structure.
fn set(m: &mut ps1rt::Mem, bytes: &[(u32, u8)]) {
    for &(at, v) in bytes {
        m.w8(FE + at, v);
    }
}

/// A leaf callback setting race settings.
fn settings(c: &mut Cpu, bytes: &[(u32, u8)]) {
    set(&mut c.m, bytes);
}

/// A callback setting race settings, then the menu's background.
fn settings_swap(c: &mut Cpu, bytes: &[(u32, u8)], ra: u32) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        set(&mut c.m, bytes);
        swap_background(c, &r, ra);
    })
}

/// `MenuExtended_SetOnePlayer` (FEMENUDEFS.CPP, 0x8002c168): one player (the menu's background),
/// but not for the second player (nothing). (Its test reads the application through whatever
/// its caller left in v0.)
pub fn abi_set_one_player(c: &mut Cpu) {
    let (cmd, app) = (c.r[4], c.r[2]);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        if c.m.r8(app.wrapping_add(557)) == 1 {
            c.m.w32(cmd, 0);
            return;
        }
        c.m.w8(FE + 3, 0);
        swap_background(c, &r, 0x8002_c19c);
    })
}

/// `MenuExtended_SetTwoPlayer` (FEMENUDEFS.CPP, 0x8002c1ac): two players with the second pad
/// in (no duel), else nothing.
pub fn abi_set_two_player(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        if c.m.r8(PAD2_STATE) != 0 {
            c.m.w32(cmd, 0);
            return;
        }
        set(&mut c.m, &[(3, 1), (798, 0)]);
        swap_background(c, &r, 0x8002_c1ec);
    })
}

/// `MenuExtended_SetTestDrive` (FEMENUDEFS.CPP, 0x8002c1fc).
pub fn abi_set_test_drive(c: &mut Cpu) {
    settings(c, &[(298, 0), (798, 0), (70, 0), (4, 0)]);
}

/// `MenuExtended_SetSingleRace` (FEMENUDEFS.CPP, 0x8002c218).
pub fn abi_set_single_race(c: &mut Cpu) {
    settings_swap(c, &[(298, 1), (70, 0), (4, 0)], 0x8002_c248);
}

/// `MenuExtended_SetTournament` (FEMENUDEFS.CPP, 0x8002c258).
pub fn abi_set_tournament(c: &mut Cpu) {
    settings_swap(c, &[(298, 1), (70, 0), (290, 0), (4, 2)], 0x8002_c290);
}

/// `MenuExtended_SetSpecialEvent` (FEMENUDEFS.CPP, 0x8002c2a0).
pub fn abi_set_special_event(c: &mut Cpu) {
    settings_swap(c, &[(298, 1), (290, 1), (70, 0), (4, 2)], 0x8002_c2d8);
}

/// `MenuExtended_SetSoloRace` (FEMENUDEFS.CPP, 0x8002c2e8).
pub fn abi_set_solo_race(c: &mut Cpu) {
    settings(c, &[(70, 0), (4, 0), (798, 0), (298, 1)]);
}

/// A yes-or-no dialog on the stack at `d` asking `word` (yes 802, no 803, no answer chosen),
/// run; its answer.
fn ask(c: &mut Cpu, r: &Saved, d: u32, word: u32, ras: (u32, u32)) -> u32 {
    let t = r.call_leaving(c, TEXT_WORD, &[word], ras.0);
    let m = &mut c.m;
    m.w32(d + 144, t);
    m.w32(d + 160, 802);
    m.w32(d + 164, 803);
    m.w16(d + 136, 0);
    port(c, r, 0x8001_9928, ras.1, &[d], crate::fedialog::abi_run)
}

/// `MenuExtended_GoToTwoPlayerSingleRace` (FEMENUDEFS.CPP, 0x8002c308): with cars in the
/// garage, a solo race's menu; else asked whether to test drive instead: yes, the car select
/// for a test drive (its list refreshed).
pub fn abi_go_to_two_player_single_race(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 200, |c, sp| {
        let mut r = Saved::of(c);
        let d = sp + 16;
        (r.0[1], r.0[0]) = (cmd, d);
        port(c, &r, 0x8001_9bcc, 0x8002_c328, &[d], crate::fedialog::abi_yes_no);
        c.m.w32(d + 96, YES_NO_VT);
        let n = port(c, &r, 0x8001_747c, 0x8002_c344, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
        if (sx(n) as i32) > 0 {
            port(c, &r, 0x8002_c2e8, 0x8002_c3e4, &[cmd], abi_set_solo_race);
            command(&mut c.m, cmd, 1, 3360);
        } else {
            let v = ask(c, &r, d, 66, (0x8002_c358, 0x8002_c378));
            if sx(v) == 1 {
                r.0[0] = 0x8005_0000;
                let m = &mut c.m;
                m.w8(FE + 4, 0);
                m.w32(cmd, 1);
                let list = menus(m) + 7268;
                port(c, &r, 0x8001_81f4, 0x8002_c3a8, &[list, u32::MAX], crate::fecars::abi_list_iterator_car_decrement);
                let list = menus(&mut c.m) + 7268;
                port(c, &r, 0x8001_81d4, 0x8002_c3b8, &[list, u32::MAX], crate::fecars::abi_list_iterator_car_increment);
                let m = &mut c.m;
                let all = menus(m);
                m.w32(cmd + 4, all + 7496);
                let select = m.r32(CAR_SELECT);
                port(c, &r, 0x8003_b874, 0x8002_c3d4, &[select, 2], crate::screencarselect::abi_set_state);
            }
        }
        port(c, &r, 0x8002_6098, 0x8002_c408, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_SetDuelRace` (FEMENUDEFS.CPP, 0x8002c41c).
pub fn abi_set_duel_race(c: &mut Cpu) {
    settings_swap(c, &[(70, 0), (4, 0), (798, 1)], 0x8002_c44c);
}

/// `MenuExtended_SetFullGrid` (FEMENUDEFS.CPP, 0x8002c45c).
pub fn abi_set_full_grid(c: &mut Cpu) {
    settings_swap(c, &[(70, 0), (4, 0), (798, 2)], 0x8002_c48c);
}

/// `AskTheUserToSaveTheGame` (FEMENUDEFS.CPP, 0x8002c49c): asked whether to save (never a
/// cheater); the answer.
pub fn abi_ask_the_user_to_save_the_game(c: &mut Cpu) {
    abi::frame(c, 192, |c, sp| {
        let mut r = Saved::of(c);
        let v = port(c, &r, 0x8002_37f0, 0x8002_c4ac, &[], crate::fecheats::abi_is_the_user_a_cry_baby_cheater);
        if v ^ 1 == 0 {
            c.r[2] = 0;
            return;
        }
        let d = sp + 16;
        port(c, &r, 0x8001_9bcc, 0x8002_c4c0, &[d], crate::fedialog::abi_yes_no);
        r.0[0] = d;
        let v = ask(c, &r, d, 818, (0x8002_c4c8, 0x8002_c4ec));
        let answer = sx(v);
        r.0[0] = answer;
        port(c, &r, 0x8002_6098, 0x8002_c500, &[d, 2], crate::fescreen::abi_screen_destructor);
        c.r[2] = answer;
    })
}

/// `MenuExtended_TransitionFromPostGameToMainMenu` (FEMENUDEFS.CPP, 0x8002c514).
pub fn abi_transition_from_post_game_to_main_menu(c: &mut Cpu) {
    let cmd = c.r[4];
    command(&mut c.m, cmd, 2, 176);
}

/// `MenuExtended_TransitionFromPostGameToMainMenuAndSaveGame` (FEMENUDEFS.CPP, 0x8002c530):
/// asked to save: the save menu, else the main menu.
pub fn abi_transition_from_post_game_to_main_menu_and_save_game(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = cmd;
        if port(c, &r, 0x8002_c49c, 0x8002_c544, &[], abi_ask_the_user_to_save_the_game) != 0 {
            command(&mut c.m, cmd, 2, 15000);
        } else {
            port(c, &r, 0x8002_c514, 0x8002_c56c, &[cmd], abi_transition_from_post_game_to_main_menu);
        }
    })
}

/// The track manager, the duel's car select, the memory card screen, the card layer's flag;
/// the application's frame (hooked: the host autosaves).
const TRACK_MANAGER: u32 = 0x8011_5D08;
const DUEL_SELECT: u32 = 0x8005_24D0;
const MEMCARD_SCREEN: u32 = 0x8005_2D80;
const INITED: u32 = 0x8005_1F08;
const REDRAW: u32 = 0x8001_3894;

fn app(m: &mut ps1rt::Mem) -> u32 {
    m.r32(crate::globals::FE_APP)
}

/// A car list's iterator (the menus' `list`) stepped back and on for `player`, its value
/// refreshed.
fn refresh(c: &mut Cpu, r: &Saved, list: u32, player: u32, ras: (u32, u32)) {
    let at = menus(&mut c.m) + list;
    port(c, r, 0x8001_81f4, ras.0, &[at, player], crate::fecars::abi_list_iterator_car_decrement);
    let at = menus(&mut c.m) + list;
    port(c, r, 0x8001_81d4, ras.1, &[at, player], crate::fecars::abi_list_iterator_car_increment);
}

fn set_state(c: &mut Cpu, r: &Saved, screen: u32, state: u32, ra: u32) {
    let s = c.m.r32(screen);
    port(c, r, 0x8003_b874, ra, &[s, state], crate::screencarselect::abi_set_state);
}

/// `MenuExtended_GoToCarSelect` (FEMENUDEFS.CPP, 0x8002c57c): (the track available, else a
/// message) the car select for the race: two players' (pink slips', garage or stock cars),
/// the duel's, or one player's (stock or garage); the car lists' items set for the race kind;
/// the lists refreshed (both players' stock lists, or the garage's for those with cars), the
/// duel's opponents' too.
pub fn abi_go_to_car_select(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let dialog = app(m) + 44;
        (r.0[2], r.0[0], r.0[1]) = (cmd, FE, dialog);
        let slot = m.r8(FE + 70) as u32;
        let track = m.r8(FE + slot + 35) as u32;
        port(c, &r, 0x8002_1c04, 0x8002_c5c4, &[TRACK_MANAGER, track, sp + 16], crate::fetracks::abi_get_track);
        if c.m.r8(sp + 19) == 0 {
            let t = r.call_leaving(c, TEXT_WORD, &[236], 0x8002_c5dc);
            c.m.w32(dialog + 144, t);
            port(c, &r, 0x8001_8980, 0x8002_c5e8, &[dialog], crate::fedialog::abi_display);
            return;
        }
        let m = &mut c.m;
        if m.r8(FE + 3) == 1 {
            let menu = if m.r8(FE + 4) == 6 {
                6724
            } else if m.r8(FE + 298) == 0 {
                5824
            } else {
                6280
            };
            command(m, cmd, 3, menu);
        } else if m.r8(FE + 798) == 1 {
            let menu = if m.r8(FE + 4) == 1 { 5576 } else { 5452 };
            command(m, cmd, 1, menu);
            set_state(c, &r, DUEL_SELECT, 7, 0x8002_c700);
        } else {
            let (state, menu) = if m.r8(FE + 298) == 0 { (0, 4688) } else { (1, 4968) };
            command(m, cmd, 1, menu);
            set_state(c, &r, CAR_SELECT, state, 0x8002_c700);
        }
        let m = &mut c.m;
        let all = menus(m);
        match m.r8(FE + 4) {
            1 => {
                let v = if m.r8(FE + 798) == 0 || m.r8(FE + 3) == 1 { 9 } else { 1 };
                m.w32(all + 4476, v);
            }
            6 => {
                m.w32(all + 6628, 32);
                return;
            }
            _ => {
                if m.r8(FE + 298) == 0 {
                    m.w32(all + 4476, 1);
                } else {
                    let v = if m.r8(FE + 4) == 2 { 64 } else { 2 };
                    m.w32(all + 4828, v);
                }
            }
        }
        (r.0[1], r.0[0]) = (FE, 0x8005_0000);
        if c.m.r8(FE + 298) == 0 {
            refresh(c, &r, 4460, 0, (0x8002_c7ec, 0x8002_c7fc));
            refresh(c, &r, 4460, 1, (0x8002_c80c, 0x8002_c8a8));
        } else {
            let n = port(c, &r, 0x8001_747c, 0x8002_c82c, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
            let m = &mut c.m;
            if (sx(n) as i32) > 0 || (m.r8(FE + 4) == 1 && m.r8(FE + 798) != 1) {
                refresh(c, &r, 4812, 0, (0x8002_c868, 0x8002_c878));
                if c.m.r8(FE + 3) == 1 {
                    refresh(c, &r, 4812, 1, (0x8002_c894, 0x8002_c8a8));
                }
            }
        }
        if c.m.r8(FE + 798) == 1 && c.m.r8(FE + 4) != 2 {
            refresh(c, &r, 5216, u32::MAX, (0x8002_c8dc, 0x8002_c8ec));
            let all = menus(&mut c.m);
            c.m.w32(all + 5232, 1);
        }
    })
}

/// A car select state's menu: the command, the state, its car list refreshed (for any player).
fn go_to_list(c: &mut Cpu, menu: u32, state: u32, list: u32, ras: [u32; 3]) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8005_0000;
        command(&mut c.m, cmd, 1, menu);
        set_state(c, &r, CAR_SELECT, state, ras[0]);
        refresh(c, &r, list, u32::MAX, (ras[1], ras[2]));
    })
}

/// `MenuExtended_GoToDealer` (FEMENUDEFS.CPP, 0x8002c910): the dealer's cars.
pub fn abi_go_to_dealer(c: &mut Cpu) {
    go_to_list(c, 7496, 2, 7268, [0x8002_c948, 0x8002_c958, 0x8002_c968]);
}

/// `MenuExtended_GoToSeller` (FEMENUDEFS.CPP, 0x8002c978): the garage's cars to sell.
pub fn abi_go_to_seller(c: &mut Cpu) {
    go_to_list(c, 7732, 3, 7620, [0x8002_c9b0, 0x8002_c9c0, 0x8002_c9d0]);
}

/// `MenuExtended_GoToUpgrades` (FEMENUDEFS.CPP, 0x8002c9e0).
pub fn abi_go_to_upgrades(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        command(&mut c.m, cmd, 1, 7988);
        set_state(c, &r, CAR_SELECT, 4, 0x8002_ca10);
    })
}

/// `GenericMenuSaveGame` (FEMENUDEFS.CPP, 0x8002ca20): the memory card screen's message
/// "saving", a frame drawn; (unless the card layer's up and no dialog's wanted) the saving
/// dialog shown and drawn until it's in, the card layer started if down; the game saved
/// (`SaveGame`); the layer stopped if started here, the dialog hidden; the save's result.
pub fn abi_generic_menu_save_game(c: &mut Cpu) {
    let dialog_wanted = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (dialog_wanted, 0x8005_0000);
        let m = &mut c.m;
        let a = app(m);
        let screen = m.r32(MEMCARD_SCREEN);
        m.w32(screen + 1412, 639);
        r.call_ptr_leaving(c, REDRAW, &[a], 0x8002_ca54);
        r.0[2] = 0;
        if c.m.r32(INITED) == 0 || dialog_wanted != 0 {
            let d = app(&mut c.m) + 720;
            r.0[0] = d;
            let t = r.call_leaving(c, TEXT_WORD, &[643], 0x8002_ca80);
            c.m.w32(d + 144, t);
            port(c, &r, 0x8001_8980, 0x8002_ca90, &[d], crate::fedialog::abi_display);
            loop {
                let a = app(&mut c.m);
                if c.m.r32(a + 852) ^ 1 == 0 {
                    r.call_ptr_leaving(c, REDRAW, &[a], 0x8002_cac4);
                    break;
                }
                r.call_ptr_leaving(c, REDRAW, &[a], 0x8002_cab4);
                c.tick();
            }
            if c.m.r32(INITED) == 0 {
                r.0[2] = 1;
                port(c, &r, 0x8003_47d4, 0x8002_cae8, &[1, 0], crate::fememcard::abi_init_memcard);
            }
        }
        let v = port(c, &r, 0x8003_4ac8, 0x8002_caf0, &[0], crate::fememcard::abi_save_game);
        let screen = c.m.r32(MEMCARD_SCREEN);
        c.m.w32(screen + 1412, u32::MAX);
        r.0[0] = v;
        if r.0[2] != 0 {
            port(c, &r, 0x8003_4938, 0x8002_cb10, &[], crate::fememcard::abi_deinit_memcard);
        }
        let d = app(&mut c.m) + 720;
        port(c, &r, 0x8001_8a44, 0x8002_cb20, &[d], crate::fedialog::abi_hide);
        c.r[2] = v;
    })
}

/// `PinkSlipsPreSave` (FEMENUDEFS.CPP, 0x8002cb3c): (a cheater: on) asked whether to save
/// before the pink slips: yes, saved (its result); cancelled: no; no: on.
pub fn abi_pink_slips_pre_save(c: &mut Cpu) {
    abi::frame(c, 200, |c, sp| {
        let mut r = Saved::of(c);
        let v = port(c, &r, 0x8002_37f0, 0x8002_cb50, &[], crate::fecheats::abi_is_the_user_a_cry_baby_cheater);
        r.0[1] = 1;
        if v ^ 1 == 0 {
            c.r[2] = 1;
            return;
        }
        let d = sp + 16;
        r.0[0] = d;
        port(c, &r, 0x8001_9bcc, 0x8002_cb68, &[d], crate::fedialog::abi_yes_no);
        c.m.w32(d + 96, YES_NO_VT);
        let v = sx(ask(c, &r, d, 628, (0x8002_cb7c, 0x8002_cb9c)));
        let result = if v == 1 {
            r.0[1] = port(c, &r, 0x8002_ca20, 0x8002_cbd4, &[1], abi_generic_menu_save_game);
            port(c, &r, 0x8002_6098, 0x8002_cbe4, &[d, 2], crate::fescreen::abi_screen_destructor);
            r.0[1]
        } else if v == u32::MAX {
            port(c, &r, 0x8002_6098, 0x8002_cbc4, &[d, 2], crate::fescreen::abi_screen_destructor);
            0
        } else {
            port(c, &r, 0x8002_6098, 0x8002_cbe4, &[d, 2], crate::fescreen::abi_screen_destructor);
            1
        };
        c.r[2] = result;
    })
}

/// The tournament manager; the name entry's names and screen.
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const NAMES: u32 = 0x8011_5C1C;
const NAME_SCREEN: u32 = 0x8005_2E44;

/// `MenuExtended_GoToBestOf*`: (the pink slips' save asked first) the pink slips' menu of
/// `menu`, or nothing; the races' count.
fn go_to_best_of(c: &mut Cpu, menu: u32, races: u8, ra: u32) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = cmd;
        if port(c, &r, 0x8002_cb3c, ra, &[], abi_pink_slips_pre_save) != 0 {
            command(&mut c.m, cmd, 1, menu);
        } else {
            c.m.w32(cmd, 0);
        }
        set(&mut c.m, &[(70, 0), (71, races)]);
    })
}

/// `MenuExtended_GoToBestOfOne` (FEMENUDEFS.CPP, 0x8002cc04).
pub fn abi_go_to_best_of_one(c: &mut Cpu) {
    go_to_best_of(c, 3360, 1, 0x8002_cc18);
}

/// `MenuExtended_GoToBestOfThree` (FEMENUDEFS.CPP, 0x8002cc60).
pub fn abi_go_to_best_of_three(c: &mut Cpu) {
    go_to_best_of(c, 2960, 3, 0x8002_cc74);
}

/// `MenuExtended_GoToBestOfFive` (FEMENUDEFS.CPP, 0x8002ccbc).
pub fn abi_go_to_best_of_five(c: &mut Cpu) {
    go_to_best_of(c, 3084, 5, 0x8002_ccd0);
}

/// The application's message dialog showing `word` (the race refused).
fn refuse(c: &mut Cpu, r: &Saved, dialog: u32, word: u32, cmd: u32, ras: (u32, u32)) {
    let t = r.call_leaving(c, TEXT_WORD, &[word], ras.0);
    c.m.w32(dialog + 144, t);
    port(c, r, 0x8001_8980, ras.1, &[dialog], crate::fedialog::abi_display);
    c.m.w32(cmd, 0);
}

/// `MenuExtended_GoToRace` (FEMENUDEFS.CPP, 0x8002cd18): the race, unless: no garage car for a
/// garage race (but hot pursuit's and pink slips'), no car for the tournament, the chosen car
/// no police car for hot pursuit, the chosen stock car unavailable: its message instead.
pub fn abi_go_to_race(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        m.w32(cmd, 5);
        let dialog = app(m) + 44;
        let garage = m.r8(FE + 298) as u32;
        (r.0[3], r.0[2], r.0[0], r.0[1]) = (cmd, FE, garage, dialog);
        let mut refused = None;
        if garage == 1 {
            let n = port(c, &r, 0x8001_747c, 0x8002_cd70, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
            let kind = c.m.r8(FE + 4);
            if (sx(n) as i32) <= 0 && kind != 1 && kind != 6 {
                refused = Some(171);
            }
        }
        if refused.is_none() && c.m.r8(FE + 4) == 2 {
            let n = port(c, &r, 0x8001_74c0, 0x8002_cdb8, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_tourney_cars);
            if (sx(n) as i32) <= 0 {
                refused = Some(242);
            }
        }
        if refused.is_none() && c.m.r8(FE + 4) == 1 {
            let id = c.m.r8(FE + 92) as u32;
            port(c, &r, 0x8001_6fd8, 0x8002_cdf4, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
            if c.m.r8(sp + 207) == 0 {
                refused = Some(243);
            }
        }
        if refused.is_none() && c.m.r8(FE + 298) == 0 {
            let id = c.m.r8(FE + 92) as u32;
            port(c, &r, 0x8001_6fd8, 0x8002_ce30, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
            if c.m.r8(sp + 23) == 0 {
                refused = Some(244);
            }
        }
        if let Some(word) = refused {
            refuse(c, &r, dialog, word, cmd, (0x8002_ce48, 0x8002_ce54));
        }
    })
}

/// `MenuExtended_GoTo2PlayerRace` (FEMENUDEFS.CPP, 0x8002ce74): the two players' race (the pink
/// slips' as it is), unless the player has no garage car for a garage race (but hot pursuit's),
/// their stock car's unavailable, or not a police car for hot pursuit: its message.
pub fn abi_go_to_2_player_race(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let dialog = app(m) + 44;
        (r.0[3], r.0[1], r.0[2]) = (cmd, FE, dialog);
        m.w32(cmd, 6);
        if m.r8(FE + 4) == 6 {
            return;
        }
        let garage = m.r8(FE + 298) as u32;
        r.0[0] = garage;
        let mut refused = None;
        if garage == 1 {
            let n = port(c, &r, 0x8001_747c, 0x8002_ced8, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
            if (sx(n) as i32) <= 0 && c.m.r8(FE + 4) != 1 {
                refused = Some(171);
            }
        }
        let player_car = |m: &mut ps1rt::Mem| {
            let a = app(m);
            let p = m.r8(a + 556) as u32;
            m.r8(FE + p + 92) as u32
        };
        if refused.is_none() && c.m.r8(FE + 298) == 0 {
            let id = player_car(&mut c.m);
            port(c, &r, 0x8001_6fd8, 0x8002_cf30, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
            if c.m.r8(sp + 23) == 0 {
                refused = Some(244);
            }
        }
        if refused.is_none() && c.m.r8(FE + 4) == 1 {
            let id = player_car(&mut c.m);
            port(c, &r, 0x8001_6fd8, 0x8002_cf7c, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
            if c.m.r8(sp + 207) == 0 {
                refused = Some(243);
            }
        }
        if let Some(word) = refused {
            refuse(c, &r, dialog, word, cmd, (0x8002_cf94, 0x8002_cfa0));
        }
    })
}

/// A track info's entry: the chosen tournament (`tier` its tier's byte in the definition, the
/// front end's `choice`), its fee (too little money: the message; else asked, and on yes paid
/// with its sound), the tournament started (`special` or not), its track info's menu.
fn go_to_track_info(c: &mut Cpu, special: u32, choice: u32, tier: u32, ras: [u32; 10]) {
    let cmd = c.r[4];
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[3], r.0[1]) = (cmd, TOURNAMENT_MANAGER);
        let m = &mut c.m;
        let def = m.r32(TOURNAMENT_MANAGER + 24);
        let k = m.r8(FE + choice) as u32;
        m.w8(FE + 290, special as u8);
        let t = (m.r8(def + tier) as u32).wrapping_add(k);
        let entry = def.wrapping_add(t.wrapping_mul(84)).wrapping_add(36);
        r.0[2] = entry;
        let fee = m.r32(entry + 48);
        if (fee as i32) > 0 {
            if (m.r32(TOURNAMENT_MANAGER + 20) as i32) < fee as i32 {
                let d = app(m) + 44;
                r.0[0] = d;
                let t = r.call_leaving(c, TEXT_WORD, &[247], ras[0]);
                c.m.w32(d + 144, t);
                port(c, &r, 0x8001_8980, ras[1], &[d], crate::fedialog::abi_display);
                return;
            }
            let d = sp + 16;
            port(c, &r, 0x8001_9bcc, ras[2], &[d], crate::fedialog::abi_yes_no);
            let t = r.call_leaving(c, TEXT_WORD, &[248], ras[3]);
            r.0[0] = d;
            let m = &mut c.m;
            m.w32(d + 144, t);
            let (yes, no) = if special != 0 { (802, 803) } else { (803, 802) };
            m.w32(d + 160, yes);
            m.w32(d + 164, no);
            m.w16(d + 136, 0);
            let v = port(c, &r, 0x8001_9928, ras[4], &[d], crate::fedialog::abi_run);
            if v << 16 == 0 {
                port(c, &r, 0x8002_6098, ras[5], &[d, 2], crate::fescreen::abi_screen_destructor);
                return;
            }
            r.call_leaving(c, nfs4rc::f_8007a08c, &[26], ras[6]);
            let m = &mut c.m;
            let money = m.r32(TOURNAMENT_MANAGER + 20).wrapping_sub(m.r32(entry + 48));
            m.w32(TOURNAMENT_MANAGER + 20, money);
            port(c, &r, 0x8002_6098, ras[7], &[d, 2], crate::fescreen::abi_screen_destructor);
        }
        let which = c.m.r8(FE + choice) as u32;
        port(c, &r, 0x8003_28d4, ras[8], &[TOURNAMENT_MANAGER, special, which], crate::fetourn::abi_start_new_tournament);
        command(&mut c.m, cmd, 1, 4336);
        let _ = ras[9];
    })
}

/// `MenuExtended_GoToTournTrackInfo` (FEMENUDEFS.CPP, 0x8002cfc0).
pub fn abi_go_to_tourn_track_info(c: &mut Cpu) {
    go_to_track_info(c, 0, 288, 2, [0x8002_d054, 0x8002_d060, 0x8002_d070, 0x8002_d078, 0x8002_d09c, 0x8002_d0b0, 0x8002_d0c0, 0x8002_d0dc, 0x8002_d0f4, 0]);
}

/// `MenuExtended_GoToSpecialEventTrackInfo` (FEMENUDEFS.CPP, 0x8002d128).
pub fn abi_go_to_special_event_track_info(c: &mut Cpu) {
    go_to_track_info(c, 1, 289, 14, [0x8002_d1c0, 0x8002_d1cc, 0x8002_d1dc, 0x8002_d1e4, 0x8002_d208, 0x8002_d21c, 0x8002_d22c, 0x8002_d248, 0x8002_d260, 0]);
}

/// `MenuExtended_EnterUserName` (FEMENUDEFS.CPP, 0x8002d294): the name grid for the player
/// (7 letters, from the start, their name's buffer), the name screen's menu.
pub fn abi_enter_user_name(c: &mut Cpu) {
    let cmd = c.r[4];
    let m = &mut c.m;
    let all = menus(m);
    let a = app(m);
    let grid = all + 12588;
    let p = m.r8(a + 557) as u32;
    m.w16(grid + 32, 7);
    let screen = m.r32(NAME_SCREEN);
    let menu = all + 13008;
    m.w16(grid + 36, 0);
    m.w16(grid + 34, 0);
    m.w16(grid + 128, p as u16);
    m.w32(grid + 28, NAMES + 8 * p);
    m.w32(screen + 100, menu);
    m.w32(cmd, 1);
    m.w32(cmd + 4, menu);
}

/// `MenuExtended_GoToShowroom` (FEMENUDEFS.CPP, 0x8002d2f8).
pub fn abi_go_to_showroom(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        command(&mut c.m, cmd, 1, 9068);
        set_state(c, &r, CAR_SELECT, 5, 0x8002_d328);
    })
}

/// `MenuExtended_GoToDealerShowroom` (FEMENUDEFS.CPP, 0x8002d338).
pub fn abi_go_to_dealer_showroom(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        command(&mut c.m, cmd, 1, 9068);
        set_state(c, &r, CAR_SELECT, 6, 0x8002_d368);
    })
}

/// `MenuExtended_SetHPSoloRace` (FEMENUDEFS.CPP, 0x8002d378).
pub fn abi_set_hp_solo_race(c: &mut Cpu) {
    set(&mut c.m, &[(70, 0), (4, 1), (798, 0)]);
    let all = menus(&mut c.m);
    c.m.w32(all + 4476, 11);
}

/// `MenuExtended_SetHPDuelRace` (FEMENUDEFS.CPP, 0x8002d3a4).
pub fn abi_set_hp_duel_race(c: &mut Cpu) {
    set(&mut c.m, &[(70, 0), (4, 1), (798, 1)]);
    let all = menus(&mut c.m);
    c.m.w32(all + 4476, 1);
}

/// `MenuExtended_SetHotPursuit` (FEMENUDEFS.CPP, 0x8002d3d0): hot pursuit, the car list's kind
/// for one or two players.
pub fn abi_set_hot_pursuit(c: &mut Cpu) {
    let m = &mut c.m;
    set(m, &[(298, 0), (70, 0), (4, 1)]);
    let v = if m.r8(FE + 3) == 1 { 9 } else { 1 };
    let all = menus(m);
    m.w32(all + 4476, v);
    m.w8(FE + 798, 0);
}

/// The player's car, the money; a save going on; the loading dialog's vtable and the player
/// it asks for.
const PLAYER_CAR: u32 = 0x8011_59E1;
const MONEY: u32 = 0x8011_5DA4;
const CARD_ACTIVE: u32 = 0x8005_1F04;
const LOAD_VT: u32 = 0x8001_0308;
const LOAD_PLAYER: u32 = 0x8005_1EF0;
const PLAY_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;

/// The application's message dialog showing `word`.
fn message(c: &mut Cpu, r: &mut Saved, reg: usize, word: u32, ras: (u32, u32)) {
    let d = app(&mut c.m) + 44;
    r.0[reg] = d;
    let t = r.call_leaving(c, TEXT_WORD, &[word], ras.0);
    c.m.w32(d + 144, t);
    port(c, r, 0x8001_8980, ras.1, &[d], crate::fedialog::abi_display);
}

/// `MenuExtended_SellCar` (FEMENUDEFS.CPP, 0x8002d420): (the last car sold only with the money
/// for the cheapest car) asked, on yes the car sold for its used price, the garage list
/// refreshed, the till's sound; else a message.
pub fn abi_sell_car(c: &mut Cpu) {
    abi::frame(c, 200, |c, sp| {
        let mut r = Saved::of(c);
        let money = c.m.r32(MONEY);
        (r.0[1], r.0[0], r.0[2]) = (CAR_MANAGER, money, 0);
        let id = c.m.r8(FE + 291) as u32;
        let price = port(c, &r, 0x8001_648c, 0x8002_d458, &[CAR_MANAGER, id], crate::fecars::abi_calc_used_price);
        r.0[0] = money.wrapping_add(price);
        let n = port(c, &r, 0x8001_747c, 0x8002_d468, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
        let mut ok = true;
        if (sx(n) as i32) < 2 {
            let cheapest = port(c, &r, 0x8001_6434, 0x8002_d484, &[CAR_MANAGER], crate::fecars::abi_cheapest_car_stock_price);
            ok = !((r.0[0] as i32) < cheapest as i32);
        }
        if !ok {
            message(c, &mut r, 0, 170, (0x8002_d554, 0x8002_d560));
            return;
        }
        r.0[2] = 1;
        let d = sp + 16;
        port(c, &r, 0x8001_9bcc, 0x8002_d4a4, &[d], crate::fedialog::abi_yes_no);
        let t = r.call_leaving(c, TEXT_WORD, &[166], 0x8002_d4ac);
        r.0[1] = d;
        let m = &mut c.m;
        m.w32(d + 144, t);
        m.w32(d + 160, 802);
        m.w32(d + 164, 803);
        m.w16(d + 136, 0);
        let v = port(c, &r, 0x8001_9928, 0x8002_d4d0, &[d], crate::fedialog::abi_run);
        if v << 16 != 0 {
            let car = c.m.r8(PLAYER_CAR) as u32;
            let gain = port(c, &r, 0x8001_6674, 0x8002_d4f0, &[CAR_MANAGER, car, 0], crate::fecars::abi_sell_car);
            r.0[0] = 0x8005_0000;
            let money = c.m.r32(MONEY).wrapping_add(gain);
            c.m.w32(MONEY, money);
            refresh(c, &r, 7620, 0, (0x8002_d518, 0x8002_d528));
            r.call_leaving(c, PLAY_SFX, &[26], 0x8002_d530);
        }
        port(c, &r, 0x8002_6098, 0x8002_d53c, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_BuyCar` (FEMENUDEFS.CPP, 0x8002d578): the dealer's car (a full garage, or too
/// little money: a message, with a buzz) asked, on yes bought in its colour and paid, the till's
/// sound.
pub fn abi_buy_car(c: &mut Cpu) {
    abi::frame(c, 416, |c, sp| {
        let mut r = Saved::of(c);
        let d = app(&mut c.m) + 44;
        (r.0[2], r.0[3], r.0[0]) = (CAR_MANAGER, FE, d);
        let id = c.m.r8(FE + 296) as u32;
        port(c, &r, 0x8001_6fd8, 0x8002_d5bc, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
        let n = port(c, &r, 0x8001_747c, 0x8002_d5c8, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
        r.0[1] = d;
        if !((sx(n) as i32) < 32) {
            let t = r.call_leaving(c, TEXT_WORD, &[76], 0x8002_d6a0);
            c.m.w32(d + 144, t);
            port(c, &r, 0x8001_8980, 0x8002_d6b0, &[d], crate::fedialog::abi_display);
            return;
        }
        r.0[1] = TOURNAMENT_MANAGER;
        if (c.m.r32(MONEY) as i32) < c.m.r32(sp + 48) as i32 {
            r.call_leaving(c, PLAY_SFX, &[10], 0x8002_d684);
            let t = r.call_leaving(c, TEXT_WORD, &[168], 0x8002_d68c);
            c.m.w32(d + 144, t);
            port(c, &r, 0x8001_8980, 0x8002_d6b0, &[d], crate::fedialog::abi_display);
            return;
        }
        let yn = sp + 224;
        r.0[0] = yn;
        port(c, &r, 0x8001_9bcc, 0x8002_d608, &[yn], crate::fedialog::abi_yes_no);
        let t = r.call_leaving(c, TEXT_WORD, &[165], 0x8002_d610);
        let m = &mut c.m;
        m.w32(yn + 144, t);
        m.w32(yn + 160, 802);
        m.w32(yn + 164, 803);
        m.w16(yn + 136, 0);
        let v = port(c, &r, 0x8001_9928, 0x8002_d630, &[yn], crate::fedialog::abi_run);
        if v << 16 != 0 {
            let m = &mut c.m;
            let car = m.r8(sp + 16) as i8 as i32 as u32;
            let colour = m.r8(car.wrapping_add(FE) + 96) as u32;
            let cost = port(c, &r, 0x8001_6598, 0x8002_d654, &[CAR_MANAGER, car, colour, 0], crate::fecars::abi_purchase_car);
            let money = c.m.r32(MONEY).wrapping_sub(cost);
            c.m.w32(MONEY, money);
            r.call_leaving(c, PLAY_SFX, &[26], 0x8002_d668);
        }
        port(c, &r, 0x8002_6098, 0x8002_d674, &[yn, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_PurchaseUpgrade` (FEMENUDEFS.CPP, 0x8002d6cc): upgrade `k` for the garage car
/// (had: nothing; too little money: a message) asked, on yes bought and paid, the till's sound.
pub fn abi_purchase_upgrade(c: &mut Cpu) {
    let k = c.r[4];
    abi::frame(c, 416, |c, sp| {
        let mut r = Saved::of(c);
        let bit = 1u32 << (k & 31);
        (r.0[0], r.0[4], r.0[3], r.0[2]) = (k, CAR_MANAGER, FE, bit);
        let id = c.m.r8(FE + 291) as u32;
        port(c, &r, 0x8001_7094, 0x8002_d718, &[CAR_MANAGER, id, sp + 16, 0], crate::fecars::abi_get_garage_car);
        let m = &mut c.m;
        if m.r8(sp + 212) as u32 & bit != 0 {
            return;
        }
        r.0[1] = TOURNAMENT_MANAGER;
        let price = m.r32(sp.wrapping_add(k.wrapping_add(1) << 2) + 48);
        if (m.r32(MONEY) as i32) < price as i32 {
            message(c, &mut r, 0, 169, (0x8002_d7e0, 0x8002_d7ec));
            return;
        }
        let yn = sp + 224;
        r.0[0] = yn;
        port(c, &r, 0x8001_9bcc, 0x8002_d760, &[yn], crate::fedialog::abi_yes_no);
        let t = r.call_leaving(c, TEXT_WORD, &[167], 0x8002_d768);
        let m = &mut c.m;
        m.w32(yn + 144, t);
        m.w32(yn + 160, 802);
        m.w32(yn + 164, 803);
        m.w16(yn + 136, 0);
        let v = port(c, &r, 0x8001_9928, 0x8002_d788, &[yn], crate::fedialog::abi_run);
        if v << 16 != 0 {
            let id = c.m.r8(FE + 291) as u32;
            let cost = port(c, &r, 0x8001_67f4, 0x8002_d7a8, &[CAR_MANAGER, id, sx(bit), 0], crate::fecars::abi_purchase_upgrade);
            let money = c.m.r32(MONEY).wrapping_sub(cost);
            c.m.w32(MONEY, money);
            r.call_leaving(c, PLAY_SFX, &[26], 0x8002_d7bc);
        }
        port(c, &r, 0x8002_6098, 0x8002_d7c8, &[yn, 2], crate::fescreen::abi_screen_destructor);
    })
}

fn purchase(c: &mut Cpu, k: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        port(c, &r, 0x8002_d6cc, ra, &[k], abi_purchase_upgrade);
    })
}

/// `MenuExtended_PurchaseUpgrade1` (FEMENUDEFS.CPP, 0x8002d80c).
pub fn abi_purchase_upgrade1(c: &mut Cpu) {
    purchase(c, 1, 0x8002_d81c);
}

/// `MenuExtended_PurchaseUpgrade2` (FEMENUDEFS.CPP, 0x8002d82c).
pub fn abi_purchase_upgrade2(c: &mut Cpu) {
    purchase(c, 2, 0x8002_d83c);
}

/// `MenuExtended_PurchaseUpgrade3` (FEMENUDEFS.CPP, 0x8002d84c).
pub fn abi_purchase_upgrade3(c: &mut Cpu) {
    purchase(c, 0, 0x8002_d85c);
}

/// `MenuExtended_SaveGame` (FEMENUDEFS.CPP, 0x8002d86c): with no save going on and the load
/// item on, the game saved (no dialog).
pub fn abi_save_game(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let m = &mut c.m;
        if m.r32(CARD_ACTIVE) == 0 {
            let all = menus(m);
            if (m.r32(all + 12372) ^ 1) & 1 != 0 {
                port(c, &r, 0x8002_ca20, 0x8002_d8ac, &[0], abi_generic_menu_save_game);
            }
        }
    })
}

/// `GenericMenuLoadGame` (FEMENUDEFS.CPP, 0x8002d8bc): with no save going on, the memory card
/// screen's message "loading", a frame drawn, `player`'s game loaded; the dialog hidden.
pub fn abi_generic_menu_load_game(c: &mut Cpu) {
    let player = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = player;
        if c.m.r32(CARD_ACTIVE) == 0 {
            (r.0[0], r.0[1]) = (0x8005_0000, 0x8005_0000);
            let m = &mut c.m;
            let a = app(m);
            let screen = m.r32(MEMCARD_SCREEN);
            m.w32(screen + 1412, 638);
            r.call_ptr_leaving(c, REDRAW, &[a], 0x8002_d8fc);
            port(c, &r, 0x8003_4f70, 0x8002_d910, &[sx(player), 0, 1], crate::fememcard::abi_load_game);
            let m = &mut c.m;
            let d = app(m) + 720;
            let screen = m.r32(MEMCARD_SCREEN);
            m.w32(screen + 1412, u32::MAX);
            port(c, &r, 0x8001_8a44, 0x8002_d928, &[d], crate::fedialog::abi_hide);
        }
        let d = app(&mut c.m) + 720;
        port(c, &r, 0x8001_8a44, 0x8002_d938, &[d], crate::fedialog::abi_hide);
    })
}

/// `MenuExtended_LoadGame` (FEMENUDEFS.CPP, 0x8002d950): with the load item on, asked (the
/// memory card screen's player's) whether to load: yes, loaded; else the dialog hidden.
pub fn abi_load_game(c: &mut Cpu) {
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        let d = sp + 16;
        r.0[0] = d;
        let all = menus(&mut c.m);
        if c.m.r32(all + 12416) & 1 != 0 {
            return;
        }
        port(c, &r, 0x8001_9bcc, 0x8002_d984, &[d], crate::fedialog::abi_yes_no);
        r.0[1] = 0x8005_0000;
        let m = &mut c.m;
        let screen = m.r32(MEMCARD_SCREEN);
        m.w32(d + 96, LOAD_VT);
        let p = m.r16(screen + 1436) as i16 as i32 as u32;
        m.w32(d + 160, 802);
        m.w32(d + 164, 803);
        m.w16(d + 136, 0);
        m.w32(LOAD_PLAYER, p);
        let t = r.call_leaving(c, TEXT_WORD, &[705], 0x8002_d9c0);
        c.m.w32(d + 144, t);
        let v = port(c, &r, 0x8001_9928, 0x8002_d9cc, &[d], crate::fedialog::abi_run);
        if sx(v) == 1 {
            let screen = c.m.r32(MEMCARD_SCREEN);
            let p = c.m.r16(screen + 1436) as i16 as i32 as u32;
            port(c, &r, 0x8002_d8bc, 0x8002_d9f4, &[p], abi_generic_menu_load_game);
        } else {
            let h = app(&mut c.m) + 568;
            port(c, &r, 0x8001_8a44, 0x8002_da08, &[h], crate::fedialog::abi_hide);
        }
        port(c, &r, 0x8002_6098, 0x8002_da14, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// The race's results (the cop's tickets and the target), the cop cars' ids (5 a car, by
/// colour), the first player's race stats, the track raced, the post-race screens.
const RESULTS: u32 = 0x8011_44A4;
const COP_CARS: u32 = 0x8005_1E68;
const STATS: u32 = 0x8011_0E08;
const TRACK_RACED: u32 = 0x8013_DAEC;
const STANDINGS_SCREEN: u32 = 0x8005_22F8;
const STANDINGS3_SCREEN: u32 = 0x8005_22FC;

/// `MenuExtended_TierFinished` (FEMENUDEFS.CPP, 0x8002da28): the award's screen if there's an
/// award, else (asked to save) the save menu or the main menu.
pub fn abi_tier_finished(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = cmd;
        c.m.w32(cmd, 2);
        port(c, &r, 0x8003_3a10, 0x8002_da50, &[TOURNAMENT_MANAGER, sp + 16], crate::fetourn::abi_get_award_information);
        let menu = if c.m.r32(sp + 64) != 0 {
            14708
        } else if port(c, &r, 0x8002_c49c, 0x8002_da74, &[], abi_ask_the_user_to_save_the_game) != 0 {
            15000
        } else {
            176
        };
        let all = menus(&mut c.m);
        c.m.w32(cmd + 4, all + menu);
    })
}

/// `MenuExtended_DidUserWinBeTheCop` (FEMENUDEFS.CPP, 0x8002daa8): one player's hot pursuit as a
/// police car, more tickets than the target: the car's cop car (by its colour) won, made
/// available and noted with its colour (unless already available); whether.
pub fn abi_did_user_win_be_the_cop(c: &mut Cpu) {
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[3], r.0[2], r.0[1]) = (0, CAR_MANAGER, FE);
        let id = c.m.r8(FE + 92) as u32;
        port(c, &r, 0x8001_6fd8, 0x8002_dae4, &[CAR_MANAGER, id, sp + 16], crate::fecars::abi_get_stock_car);
        let m = &mut c.m;
        c.r[2] = 0;
        if m.r8(sp + 18) != 7 || m.r8(FE + 4) != 1 || m.r8(FE + 3) == 1 {
            return;
        }
        if !((m.r32(RESULTS + 428) as i32) < m.r32(RESULTS + 440) as i32) {
            return;
        }
        let car = m.r8(sp + 16) as i8 as i32 as u32;
        let colour = m.r8(FE.wrapping_add(car) + 192) as i8 as i32 as u32;
        let k = car.wrapping_sub(22).wrapping_mul(5).wrapping_add(colour);
        let cop = m.r16(COP_CARS.wrapping_add(k << 2)) as i16 as i32 as u32;
        let info = port(c, &r, 0x8001_6374, 0x8002_db64, &[CAR_MANAGER, cop], crate::fecars::abi_get_car_from_id);
        r.0[0] = info;
        if c.m.r8(info + 7) != 0 {
            c.r[2] = 0;
            return;
        }
        let model = c.m.r8(info) as i8 as i32 as u32;
        port(c, &r, 0x8001_6f34, 0x8002_db88, &[CAR_MANAGER, model, 1], crate::fecars::abi_set_car_available);
        let m = &mut c.m;
        let car = m.r8(sp + 16) as i8 as i32 as u32;
        let won = m.r8(info);
        m.w8(FE + 301, won);
        let v = m.r8(FE.wrapping_add(car) + 192);
        m.w8(FE + 302, v);
        c.r[2] = 1;
    })
}

/// `MenuExtended_PostGameMenu` (FEMENUDEFS.CPP, 0x8002dbc4): the new records cleared; after a
/// tournament's race its standings (the final ones once over), after the pink slips' their
/// standings, after hot pursuit the cop car won's screen, else the main menu.
pub fn abi_post_game_menu(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = cmd;
        port(c, &r, 0x8004_ad14, 0x8002_dbd8, &[], crate::statchk::abi_clear_new_records);
        c.m.w32(cmd, 2);
        let menu = match c.m.r8(FE + 4) {
            2 => {
                let done = port(c, &r, 0x8003_2b0c, 0x8002_dc2c, &[TOURNAMENT_MANAGER], crate::fetourn::abi_is_tournament_finished);
                let m = &mut c.m;
                let all = menus(m);
                let (menu, screen) = if done << 16 != 0 { (13784, STANDINGS_SCREEN) } else { (13616, STANDINGS3_SCREEN) };
                m.w32(cmd + 4, all + menu);
                let s = m.r32(screen);
                m.w32(s + 132, 1);
                return;
            }
            6 => 13404,
            1 => {
                if port(c, &r, 0x8002_daa8, 0x8002_dc90, &[], abi_did_user_win_be_the_cop) != 0 {
                    14584
                } else {
                    176
                }
            }
            _ => 176,
        };
        let all = menus(&mut c.m);
        c.m.w32(cmd + 4, all + menu);
    })
}

/// The first car's race on the track: its lap record and top time saved if it made them.
fn save_records(c: &mut Cpu, r: &mut Saved, sp: u32, ras: [u32; 4]) {
    (r.0[1], r.0[0]) = (STATS, 0x8014_0000);
    let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
    if port(c, r, 0x8004_9cc8, ras[0], &[STATS, track, sp + 16], crate::statchk::abi_is_record_lap_time) != 0 {
        let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
        let lap = c.m.r16(sp + 16) as i16 as i32 as u32;
        port(c, r, 0x8004_a004, ras[1], &[STATS, track, lap], crate::statchk::abi_save_record_lap_time);
    }
    let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
    if port(c, r, 0x8004_a194, ras[2], &[STATS, track], crate::statchk::abi_is_top_time) << 16 != 0 {
        let track = c.m.r16(TRACK_RACED) as i16 as i32 as u32;
        port(c, r, 0x8004_a640, ras[3], &[STATS, track], crate::statchk::abi_save_top_time);
    }
}

/// `MenuExtended_FinishedPlayer1GetName` (FEMENUDEFS.CPP, 0x8002dcc4): the first player's name
/// entered: with a second player's name to come, the grid set for it (their name's buffer);
/// else the records saved and the records screen.
pub fn abi_finished_player1_get_name(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = cmd;
        let m = &mut c.m;
        let a = app(m);
        m.w32(cmd, 2);
        if m.r32(a + 884) != 0 && m.r32(a + 876) == 0 {
            let all = menus(m);
            let grid = all + 12868;
            m.w16(grid + 128, 1);
            m.w32(grid + 28, NAMES + 8);
            m.w16(grid + 32, 7);
            let screen = m.r32(NAME_SCREEN);
            m.w16(grid + 36, 0);
            m.w16(grid + 34, 0);
            m.w32(screen + 100, all + 14160);
            m.w32(cmd + 4, all + 14160);
            return;
        }
        save_records(c, &mut r, sp, [0x8002_dd6c, 0x8002_dd84, 0x8002_dd90, 0x8002_dda8]);
        let all = menus(&mut c.m);
        c.m.w32(cmd + 4, all + 14332);
    })
}

/// `MenuExtended_FinishedPlayer2GetName` (FEMENUDEFS.CPP, 0x8002ddd4): the records saved, the
/// records screen.
pub fn abi_finished_player2_get_name(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = cmd;
        save_records(c, &mut r, sp, [0x8002_de08, 0x8002_de20, 0x8002_de2c, 0x8002_de44]);
        command(&mut c.m, cmd, 2, 14332);
    })
}

/// `MenuExtended_SetPinkSlips` (FEMENUDEFS.CPP, 0x8002de74): the menu's background; the pink
/// slips: two players, no duel, no track, no wins, no tracks chosen.
pub fn abi_set_pink_slips(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        swap_background(c, &r, 0x8002_de8c);
        let m = &mut c.m;
        set(m, &[(4, 6), (3, 1), (798, 0)]);
        m.w16(FE + 90, 0xFFFF);
        set(m, &[(70, 0), (72, 0), (73, 0)]);
        for k in 0..5 {
            m.w16(FE + 12 + 2 * k, 0xFFFF);
        }
    })
}

/// The pink slips' standings screen; the tournament manager's tier (+12).
const PINK_STANDINGS: u32 = 0x8005_2DD4;

/// The application's dialog shown and frames drawn until it's in.
fn show_and_wait(c: &mut Cpu, r: &mut Saved, ras: (u32, u32), last: Option<(usize, u32)>) {
    loop {
        let a = app(&mut c.m);
        if c.m.r32(a + 852) ^ 1 == 0 {
            if let Some((k, v)) = last {
                r.0[k] = v;
            }
            r.call_ptr_leaving(c, REDRAW, &[a], ras.1);
            return;
        }
        r.call_ptr_leaving(c, REDRAW, &[a], ras.0);
        c.tick();
    }
}

/// `MenuExtended_AwardPinkSlipsCar` (FEMENUDEFS.CPP, 0x8002df34): the pink slips' winner told
/// (by name and number) while the loser's car is moved: the card layer up, the loser's car
/// added to the winner's pink slips with its upgrades, saved; then "loading", the game loaded
/// back, the layer down; the music's buffer refilled (frames drawn with the help line); the
/// main menu.
pub fn abi_award_pink_slips_car(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 512, |c, sp| {
        let mut r = Saved::of(c);
        let d = sp + 104;
        (r.0[5], r.0[0]) = (cmd, d);
        port(c, &r, 0x8001_9bcc, 0x8002_df64, &[d], crate::fedialog::abi_yes_no);
        let m = &mut c.m;
        m.w32(d + 160, 658);
        m.w32(d + 164, 659);
        let standings = m.r32(PINK_STANDINGS);
        m.w16(d + 136, 1);
        let winner = m.r16(standings + 388) as i16 as i32 as u32;
        r.0[3] = winner;
        let format = r.call_leaving(c, TEXT_WORD, &[667], 0x8002_df90);
        r.0[0] = format;
        let name = port(c, &r, 0x8002_ba7c, 0x8002_df9c, &[winner], crate::front::abi_player_name);
        r.call_leaving(c, nfs4rc::f_800e50a8, &[sp + 24, format, name, winner.wrapping_add(1)], 0x8002_dfb0);
        r.0[4] = 0x8005_0000;
        let dialog = app(&mut c.m) + 720;
        c.m.w32(dialog + 144, sp + 24);
        port(c, &r, 0x8001_8980, 0x8002_dfcc, &[dialog], crate::fedialog::abi_display);
        show_and_wait(c, &mut r, (0x8002_dff0, 0x8002_e000), Some((2, 0x8011_0000)));
        port(c, &r, 0x8003_47d4, 0x8002_e00c, &[0, 1], crate::fememcard::abi_init_memcard);
        (r.0[2], r.0[1]) = (CAR_MANAGER, FE);
        let loser = 1u32.wrapping_sub(winner);
        let id = c.m.r8(FE.wrapping_add(loser) + 293) as u32;
        port(c, &r, 0x8001_71a0, 0x8002_e03c, &[CAR_MANAGER, id, sp + 272, sx(loser)], crate::fecars::abi_get_pink_slips_car);
        r.0[0] = winner;
        let m = &mut c.m;
        let (car, colour) = (m.r8(sp + 272) as i8 as i32 as u32, m.r8(sp + 469) as u32);
        port(c, &r, 0x8001_6a48, 0x8002_e054, &[CAR_MANAGER, car, colour, winner], crate::fecars::abi_add_to_pink_slips_list);
        r.0[1] = FE.wrapping_add(winner);
        let m = &mut c.m;
        let (id, upgrades) = (m.r8(FE.wrapping_add(winner) + 293) as u32, m.r8(sp + 468) as u32);
        port(c, &r, 0x8001_6ac4, 0x8002_e06c, &[CAR_MANAGER, id, upgrades, winner], crate::fecars::abi_add_upgrades_to_pink_slips_list);
        port(c, &r, 0x8003_5930, 0x8002_e07c, &[winner, 2, u32::MAX], crate::fememcard::abi_save_pink_slips_with_dialogs);
        let dialog = app(&mut c.m) + 720;
        port(c, &r, 0x8001_8a44, 0x8002_e088, &[dialog], crate::fedialog::abi_hide);
        let dialog = app(&mut c.m) + 720;
        r.0[0] = dialog;
        command(&mut c.m, cmd, 2, 176);
        let t = r.call_leaving(c, TEXT_WORD, &[629], 0x8002_e0b0);
        r.0[1] = 0x8005_0000;
        c.m.w32(dialog + 144, t);
        let dialog = app(&mut c.m) + 720;
        port(c, &r, 0x8001_8980, 0x8002_e0c4, &[dialog], crate::fedialog::abi_display);
        show_and_wait(c, &mut r, (0x8002_e0e8, 0x8002_e0f8), None);
        port(c, &r, 0x8002_d8bc, 0x8002_e100, &[0], abi_generic_menu_load_game);
        port(c, &r, 0x8003_4938, 0x8002_e108, &[], crate::fememcard::abi_deinit_memcard);
        let dialog = app(&mut c.m) + 720;
        port(c, &r, 0x8001_8a44, 0x8002_e114, &[dialog], crate::fedialog::abi_hide);
        loop {
            let b = r.call_leaving(c, nfs4rc::f_8007aa48, &[], 0x8002_e11c);
            r.0[0] = b;
            let t = r.call_leaving(c, nfs4rc::f_8007a9c4, &[], 0x8002_e124);
            r.0[0] = ((b as i32) < t as i32) as u32;
            if r.0[0] == 0 {
                break;
            }
            port(c, &r, 0x8001_5a70, 0x8002_e138, &[0], crate::feaudio::abi_systemtask);
            let a = app(&mut c.m);
            r.call_ptr_leaving(c, REDRAW, &[a], 0x8002_e148);
            port(c, &r, 0x8002_12e0, 0x8002_e16c, &[638, 480, 220, 1, 1, 4], crate::fetextrender::abi_menu_text_positioned_justify);
            let t = r.call_leaving(c, TEXT_WORD, &[638], 0x8002_e174);
            let w = r.call_leaving(c, nfs4rc::f_800e4a2c, &[t], 0x8002_e17c);
            r.call_ptr_leaving(c, 0x8004_F12C, &[0, 480, 220, 0u32.wrapping_sub(w).wrapping_sub(5), 7], 0x8002_e19c);
            c.tick();
        }
        command(&mut c.m, cmd, 2, 176);
        port(c, &r, 0x8002_6098, 0x8002_e1c4, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_GoToGarage` (FEMENUDEFS.CPP, 0x8002e1e8): the garage's cars (the list's kind,
/// refreshed), the garage's menu (a tournament's or not), the car select's garage state.
pub fn abi_go_to_garage(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (cmd, 0x8005_0000);
        let m = &mut c.m;
        m.w8(FE + 298, 1);
        let list = menus(m) + 4812;
        m.w32(list + 16, 64);
        refresh(c, &r, 4812, u32::MAX, (0x8002_e224, 0x8002_e234));
        let m = &mut c.m;
        let menu = if m.r32(TOURNAMENT_MANAGER + 12) == 0 { 4968 } else { 5092 };
        command(m, cmd, 1, menu);
        set_state(c, &r, CAR_SELECT, 1, 0x8002_e27c);
    })
}

/// `MenuExtended_GoToTournTrophyRoom` (FEMENUDEFS.CPP, 0x8002e290).
pub fn abi_go_to_tourn_trophy_room(c: &mut Cpu) {
    c.m.w8(FE + 290, 0);
}

/// `MenuExtended_GoToSETrophyRoom` (FEMENUDEFS.CPP, 0x8002e29c).
pub fn abi_go_to_se_trophy_room(c: &mut Cpu) {
    c.m.w8(FE + 290, 1);
}

/// `MenuExtended_SetBeginner` (FEMENUDEFS.CPP, 0x8002e2ac).
pub fn abi_set_beginner(c: &mut Cpu) {
    c.m.w8(FE + 6, 0);
}

/// `MenuExtended_SetIntermediate` (FEMENUDEFS.CPP, 0x8002e2b8).
pub fn abi_set_intermediate(c: &mut Cpu) {
    c.m.w8(FE + 6, 1);
}

/// `MenuExtended_SetExpert` (FEMENUDEFS.CPP, 0x8002e2c8).
pub fn abi_set_expert(c: &mut Cpu) {
    c.m.w8(FE + 6, 2);
}

/// A yes-or-no dialog on the stack at `d` asking `word`, made and run (`ExitTourney`'s order:
/// the answers before the question).
fn ask_quit(c: &mut Cpu, r: &mut Saved, d: u32, ras: (u32, u32, u32)) -> u32 {
    port(c, r, 0x8001_9bcc, ras.0, &[d], crate::fedialog::abi_yes_no);
    r.0[0] = d;
    let m = &mut c.m;
    m.w32(d + 160, 802);
    m.w32(d + 164, 803);
    m.w16(d + 136, 0);
    let t = r.call_leaving(c, TEXT_WORD, &[158], ras.1);
    c.m.w32(d + 144, t);
    port(c, r, 0x8001_9928, ras.2, &[d], crate::fedialog::abi_run)
}

/// `MenuExtended_ExitTourney` (FEMENUDEFS.CPP, 0x8002e2d8): asked: yes, the main menu; no,
/// nothing.
pub fn abi_exit_tourney(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 200, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = cmd;
        let d = sp + 16;
        if ask_quit(c, &mut r, d, (0x8002_e2f4, 0x8002_e314, 0x8002_e320)) << 16 != 0 {
            command(&mut c.m, cmd, 2, 176);
        } else {
            c.m.w32(cmd, 0);
        }
        port(c, &r, 0x8002_6098, 0x8002_e354, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_ExitPinkSlipsEarly` (FEMENUDEFS.CPP, 0x8002e368): asked: yes, each player's
/// pink slips cars saved back to their card (a message by name and number), the card layer
/// down, the main menu, the race kind back to a single race.
pub fn abi_exit_pink_slips_early(c: &mut Cpu) {
    let cmd = c.r[4];
    abi::frame(c, 288, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = cmd;
        let d = sp + 16;
        if ask_quit(c, &mut r, d, (0x8002_e38c, 0x8002_e3ac, 0x8002_e3b8)) << 16 != 0 {
            port(c, &r, 0x8003_47d4, 0x8002_e3cc, &[0, 1], crate::fememcard::abi_init_memcard);
            (r.0[1], r.0[2]) = (0, sp + 184);
            for p in 0..2u32 {
                let format = r.call_leaving(c, TEXT_WORD, &[664], 0x8002_e3e8);
                r.0[0] = format;
                let name = port(c, &r, 0x8002_ba7c, 0x8002_e3f4, &[p], crate::front::abi_player_name);
                r.0[0] = p + 1;
                r.call_leaving(c, nfs4rc::f_800e50a8, &[sp + 184, format, name, p + 1], 0x8002_e40c);
                r.0[1] = p + 1;
                let a = app(&mut c.m);
                c.m.w32(a + 864, sp + 184);
                port(c, &r, 0x8003_5930, 0x8002_e430, &[sx(p), 1, u32::MAX], crate::fememcard::abi_save_pink_slips_with_dialogs);
            }
            port(c, &r, 0x8003_4938, 0x8002_e440, &[], crate::fememcard::abi_deinit_memcard);
            let dialog = app(&mut c.m) + 720;
            port(c, &r, 0x8001_8a44, 0x8002_e450, &[dialog], crate::fedialog::abi_hide);
            command(&mut c.m, cmd, 2, 176);
            c.m.w8(FE + 4, 0);
        }
        port(c, &r, 0x8002_6098, 0x8002_e47c, &[d, 2], crate::fescreen::abi_screen_destructor);
    })
}

/// `MenuExtended_PinkSlipsContinue` (FEMENUDEFS.CPP, 0x8002e498): with no player past half the
/// races, the next race; else the pink slips' win.
pub fn abi_pink_slips_continue(c: &mut Cpu) {
    let cmd = c.r[4];
    let m = &mut c.m;
    let need = (m.r8(FE + 71) as u32 >> 1) + 1;
    if (m.r8(FE + 72) as u32) < need && (m.r8(FE + 73) as u32) < need {
        let v = m.r8(FE + 70).wrapping_add(1);
        m.w8(FE + 70, v);
        m.w32(cmd, 5);
    } else {
        command(m, cmd, 2, 14460);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// A menu command.
    const CMD: u32 = 0x8014_7000;

    /// FEMENU's structures, then the front end's race settings (players, kind, the pink
    /// slips' races and wins, the chosen cars, garage or stock, duel or grid, the tournaments
    /// chosen), the application's players and names' flags, the second pad, the menus' load
    /// and save items, a save going on or not, the money (none for the cases that would ask to
    /// pay: the dialog's `Run` waits on frames) and the tier, the race's results, the command.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        // (the main screen's own shuffle: its backgrounds' shapes, which `SwapBackground` sets up)
        crate::screenmain::tests::shuffle(c, rng);
        let m = &mut c.m;
        m.w8(FE + 3, pick(rng, 3) as u8);
        m.w8(FE + 4, pick(rng, 8) as u8);
        m.w8(FE + 70, pick(rng, 5) as u8);
        m.w8(FE + 71, [1, 3, 5][pick(rng, 3) as usize]);
        m.w8(FE + 72, pick(rng, 4) as u8);
        m.w8(FE + 73, pick(rng, 4) as u8);
        let cars = m.r32(CAR_MANAGER);
        for at in [92, 93, 291, 292, 296, 297] {
            m.w8(FE + at, [0, 1, pick(rng, cars.max(1)), cars.wrapping_sub(1)][pick(rng, 4) as usize] as u8);
        }
        m.w8(FE + 298, pick(rng, 2) as u8);
        m.w8(FE + 798, pick(rng, 3) as u8);
        m.w8(FE + 288, pick(rng, 4) as u8);
        m.w8(FE + 289, pick(rng, 4) as u8);
        let a = app(m);
        m.w8(a + 556, pick(rng, 2) as u8);
        m.w8(a + 557, pick(rng, 2) as u8);
        m.w32(a + 876, pick(rng, 2));
        m.w32(a + 884, pick(rng, 2));
        m.w8(PAD2_STATE, pick(rng, 2) as u8);
        let all = menus(m);
        for at in [12372, 12416] {
            let v = m.r32(all + at) & !1;
            m.w32(all + at, v | pick(rng, 2));
        }
        m.w32(CARD_ACTIVE, pick(rng, 2));
        m.w32(MONEY, [0, 1000, 100_000, 10_000_000][pick(rng, 4) as usize]);
        m.w32(TOURNAMENT_MANAGER + 12, pick(rng, 3));
        m.w32(RESULTS + 428, pick(rng, 10));
        m.w32(RESULTS + 440, pick(rng, 10));
        m.w32(CMD, pick(rng, 7));
        m.w32(CMD + 4, rng.next() as u32);
    }

    /// The command, with less money than anything costs (the dialogs asking to pay are `Run`).
    fn broke(c: &mut Cpu, _r: &mut Rng) -> Vec<u32> {
        c.m.w32(MONEY, 0x8000_0000);
        vec![CMD]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("MenuExtended_SetOnePlayer", 0x8002_c168, nfs4rc::f_8002c168, |c, _| {
                let a = app(&mut c.m);
                c.r[2] = a;
                vec![CMD]
            }),
            ("MenuExtended_SetTwoPlayer", 0x8002_c1ac, nfs4rc::f_8002c1ac, |_, _| vec![CMD]),
            ("MenuExtended_SetTestDrive", 0x8002_c1fc, nfs4rc::f_8002c1fc, |_, _| vec![CMD]),
            ("MenuExtended_SetSingleRace", 0x8002_c218, nfs4rc::f_8002c218, |_, _| vec![CMD]),
            ("MenuExtended_SetTournament", 0x8002_c258, nfs4rc::f_8002c258, |_, _| vec![CMD]),
            ("MenuExtended_SetSpecialEvent", 0x8002_c2a0, nfs4rc::f_8002c2a0, |_, _| vec![CMD]),
            ("MenuExtended_SetSoloRace", 0x8002_c2e8, nfs4rc::f_8002c2e8, |_, _| vec![CMD]),
            ("MenuExtended_SetDuelRace", 0x8002_c41c, nfs4rc::f_8002c41c, |_, _| vec![CMD]),
            ("MenuExtended_SetFullGrid", 0x8002_c45c, nfs4rc::f_8002c45c, |_, _| vec![CMD]),
            ("MenuExtended_TransitionFromPostGameToMainMenu", 0x8002_c514, nfs4rc::f_8002c514, |_, _| vec![CMD]),
            ("MenuExtended_GoToCarSelect", 0x8002_c57c, nfs4rc::f_8002c57c, |_, _| vec![CMD]),
            ("MenuExtended_GoToDealer", 0x8002_c910, nfs4rc::f_8002c910, |_, _| vec![CMD]),
            ("MenuExtended_GoToSeller", 0x8002_c978, nfs4rc::f_8002c978, |_, _| vec![CMD]),
            ("MenuExtended_GoToUpgrades", 0x8002_c9e0, nfs4rc::f_8002c9e0, |_, _| vec![CMD]),
            ("MenuExtended_GoToRace", 0x8002_cd18, nfs4rc::f_8002cd18, |_, _| vec![CMD]),
            ("MenuExtended_GoTo2PlayerRace", 0x8002_ce74, nfs4rc::f_8002ce74, |_, _| vec![CMD]),
            ("MenuExtended_GoToTournTrackInfo", 0x8002_cfc0, nfs4rc::f_8002cfc0, broke),
            ("MenuExtended_GoToSpecialEventTrackInfo", 0x8002_d128, nfs4rc::f_8002d128, broke),
            ("MenuExtended_EnterUserName", 0x8002_d294, nfs4rc::f_8002d294, |_, _| vec![CMD]),
            ("MenuExtended_GoToShowroom", 0x8002_d2f8, nfs4rc::f_8002d2f8, |_, _| vec![CMD]),
            ("MenuExtended_GoToDealerShowroom", 0x8002_d338, nfs4rc::f_8002d338, |_, _| vec![CMD]),
            ("MenuExtended_SetHPSoloRace", 0x8002_d378, nfs4rc::f_8002d378, |_, _| vec![CMD]),
            ("MenuExtended_SetHPDuelRace", 0x8002_d3a4, nfs4rc::f_8002d3a4, |_, _| vec![CMD]),
            ("MenuExtended_SetHotPursuit", 0x8002_d3d0, nfs4rc::f_8002d3d0, |_, _| vec![CMD]),
            ("MenuExtended_BuyCar", 0x8002_d578, nfs4rc::f_8002d578, broke),
            ("MenuExtended_PurchaseUpgrade", 0x8002_d6cc, nfs4rc::f_8002d6cc, |c, r| {
                c.m.w32(MONEY, 0x8000_0000);
                vec![pick(r, 3)]
            }),
            ("MenuExtended_PurchaseUpgrade1", 0x8002_d80c, nfs4rc::f_8002d80c, broke),
            ("MenuExtended_PurchaseUpgrade2", 0x8002_d82c, nfs4rc::f_8002d82c, broke),
            ("MenuExtended_PurchaseUpgrade3", 0x8002_d84c, nfs4rc::f_8002d84c, broke),
            ("MenuExtended_SaveGame", 0x8002_d86c, nfs4rc::f_8002d86c, |c, _| {
                // (the save itself waits on the card: never started)
                let all = menus(&mut c.m);
                let v = c.m.r32(all + 12372) | 1;
                c.m.w32(all + 12372, v);
                vec![CMD]
            }),
            ("GenericMenuLoadGame", 0x8002_d8bc, nfs4rc::f_8002d8bc, |c, _| {
                c.m.w32(CARD_ACTIVE, 1);
                vec![0]
            }),
            ("MenuExtended_LoadGame", 0x8002_d950, nfs4rc::f_8002d950, |c, _| {
                let all = menus(&mut c.m);
                let v = c.m.r32(all + 12416) | 1;
                c.m.w32(all + 12416, v);
                vec![CMD]
            }),
            ("MenuExtended_DidUserWinBeTheCop", 0x8002_daa8, nfs4rc::f_8002daa8, |_, _| vec![]),
            ("MenuExtended_PostGameMenu", 0x8002_dbc4, nfs4rc::f_8002dbc4, |_, _| vec![CMD]),
            ("MenuExtended_FinishedPlayer1GetName", 0x8002_dcc4, nfs4rc::f_8002dcc4, |_, _| vec![CMD]),
            ("MenuExtended_FinishedPlayer2GetName", 0x8002_ddd4, nfs4rc::f_8002ddd4, |_, _| vec![CMD]),
            ("MenuExtended_SetPinkSlips", 0x8002_de74, nfs4rc::f_8002de74, |_, _| vec![CMD]),
            ("MenuExtended_GoToGarage", 0x8002_e1e8, nfs4rc::f_8002e1e8, |_, _| vec![CMD]),
            ("MenuExtended_GoToTournTrophyRoom", 0x8002_e290, nfs4rc::f_8002e290, |_, _| vec![CMD]),
            ("MenuExtended_GoToSETrophyRoom", 0x8002_e29c, nfs4rc::f_8002e29c, |_, _| vec![CMD]),
            ("MenuExtended_SetBeginner", 0x8002_e2ac, nfs4rc::f_8002e2ac, |_, _| vec![CMD]),
            ("MenuExtended_SetIntermediate", 0x8002_e2b8, nfs4rc::f_8002e2b8, |_, _| vec![CMD]),
            ("MenuExtended_SetExpert", 0x8002_e2c8, nfs4rc::f_8002e2c8, |_, _| vec![CMD]),
            ("MenuExtended_PinkSlipsContinue", 0x8002_e498, nfs4rc::f_8002e498, |_, _| vec![CMD]),
        ]
    }

    #[test]
    fn femenudefs_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xF0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0xF0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn femenudefs_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0xF1E + k as u64, shuffle, &args);
        }
    }
}
