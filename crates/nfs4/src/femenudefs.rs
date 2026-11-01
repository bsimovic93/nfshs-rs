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

/// `MenuExtended_SetOnePlayer` (FEMENUDEFS.CPP, 0x8002c160, eight bytes before its symbol): one
/// player (the menu's background), but not for the second player (nothing).
pub fn abi_set_one_player(c: &mut Cpu) {
    let cmd = c.r[4];
    let app = c.m.r32(crate::globals::FE_APP);
    c.r[2] = app;
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

/// The menus' callbacks as the items hold them (`MenuExtended_SetOnePlayer` from its first
/// instruction, eight bytes before its symbol, which loads the application).
const SET_ONE_PLAYER: u32 = 0x8002_C160;
const SET_TWO_PLAYER: u32 = 0x8002_C1AC;
const SET_TEST_DRIVE: u32 = 0x8002_C1FC;
const SET_SINGLE_RACE: u32 = 0x8002_C218;
const SET_TOURNAMENT: u32 = 0x8002_C258;
const SET_SPECIAL_EVENT: u32 = 0x8002_C2A0;
const SET_SOLO_RACE: u32 = 0x8002_C2E8;
const GO_TO_TWO_PLAYER_SINGLE_RACE: u32 = 0x8002_C308;
const SET_DUEL_RACE: u32 = 0x8002_C41C;
const SET_FULL_GRID: u32 = 0x8002_C45C;
const TRANSITION_FROM_POST_GAME_TO_MAIN_MENU: u32 = 0x8002_C514;
const TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME: u32 = 0x8002_C530;
const GO_TO_CAR_SELECT: u32 = 0x8002_C57C;
const GO_TO_DEALER: u32 = 0x8002_C910;
const GO_TO_SELLER: u32 = 0x8002_C978;
const GO_TO_UPGRADES: u32 = 0x8002_C9E0;
const GO_TO_BEST_OF_ONE: u32 = 0x8002_CC04;
const GO_TO_BEST_OF_THREE: u32 = 0x8002_CC60;
const GO_TO_BEST_OF_FIVE: u32 = 0x8002_CCBC;
const GO_TO_RACE: u32 = 0x8002_CD18;
const GO_TO_2_PLAYER_RACE: u32 = 0x8002_CE74;
const GO_TO_TOURN_TRACK_INFO: u32 = 0x8002_CFC0;
const GO_TO_SPECIAL_EVENT_TRACK_INFO: u32 = 0x8002_D128;
const ENTER_USER_NAME: u32 = 0x8002_D294;
const GO_TO_SHOWROOM: u32 = 0x8002_D2F8;
const GO_TO_DEALER_SHOWROOM: u32 = 0x8002_D338;
const SET_HP_SOLO_RACE: u32 = 0x8002_D378;
const SET_HP_DUEL_RACE: u32 = 0x8002_D3A4;
const SET_HOT_PURSUIT: u32 = 0x8002_D3D0;
const SELL_CAR: u32 = 0x8002_D420;
const BUY_CAR: u32 = 0x8002_D578;
const PURCHASE_UPGRADE1: u32 = 0x8002_D80C;
const PURCHASE_UPGRADE2: u32 = 0x8002_D82C;
const PURCHASE_UPGRADE3: u32 = 0x8002_D84C;
const SAVE_GAME: u32 = 0x8002_D86C;
const LOAD_GAME: u32 = 0x8002_D950;
const TIER_FINISHED: u32 = 0x8002_DA28;
const POST_GAME_MENU: u32 = 0x8002_DBC4;
const FINISHED_PLAYER1_GET_NAME: u32 = 0x8002_DCC4;
const FINISHED_PLAYER2_GET_NAME: u32 = 0x8002_DDD4;
const SET_PINK_SLIPS: u32 = 0x8002_DE74;
const AWARD_PINK_SLIPS_CAR: u32 = 0x8002_DF34;
const GO_TO_GARAGE: u32 = 0x8002_E1E8;
const GO_TO_TOURN_TROPHY_ROOM: u32 = 0x8002_E290;
const GO_TO_SE_TROPHY_ROOM: u32 = 0x8002_E29C;
const SET_BEGINNER: u32 = 0x8002_E2AC;
const SET_INTERMEDIATE: u32 = 0x8002_E2B8;
const SET_EXPERT: u32 = 0x8002_E2C8;
const EXIT_TOURNEY: u32 = 0x8002_E2D8;
const EXIT_PINK_SLIPS_EARLY: u32 = 0x8002_E368;
const PINK_SLIPS_CONTINUE: u32 = 0x8002_E498;
/// The members' constructors (the items', the menus', the list iterators'), and the sliders'
/// `SetDimensions`.
const CAR_ITERATOR: u32 = 0x8001_7A14;
const CAR_COLOR_ITERATOR: u32 = 0x8001_85E0;
const NFS4_BUTTON: u32 = 0x8001_A60C;
const NFS4_CHOICE: u32 = 0x8001_A99C;
const NFS4_MENU: u32 = 0x8001_B1F8;
const TWO_PLAYER_MENU: u32 = 0x8001_B79C;
const BOTTOM_MENU: u32 = 0x8001_B8A8;
const BLANK_MENU: u32 = 0x8001_B9F8;
const OPTIONS: u32 = 0x8001_BBAC;
const FADE: u32 = 0x8001_C4B8;
const OPTIONS_MENU: u32 = 0x8001_C614;
const INSIDE_BOX_MENU: u32 = 0x8001_CEF0;
const SLIDING_MENU: u32 = 0x8001_D2C0;
const AUDIO_SLIDER: u32 = 0x8001_E65C;
const SONG_MENU: u32 = 0x8001_EA80;
const INSIDE_SLIDER: u32 = 0x8001_F3E8;
const TWO_WAY_SLIDER: u32 = 0x8001_F5D4;
const USER_NAME: u32 = 0x8001_FD98;
const TRACK_ITERATOR: u32 = 0x8002_1EB4;
const ITERATOR: u32 = 0x8002_3DF0;
const INDEXED_ITERATOR: u32 = 0x8002_3F80;
const RANGE_ITERATOR: u32 = 0x8002_4554;
const RANGE_INDEXED_ITERATOR: u32 = 0x8002_4644;
const CHOICE: u32 = 0x8002_4924;
const SET_DIMENSIONS: u32 = 0x8002_5330;
const BUTTON: u32 = 0x8002_5348;
const TOURNAMENT_ITERATOR: u32 = 0x8003_3ED8;
/// The application.
const FE_APP: u32 = crate::globals::FE_APP;

/// A member constructed: the stack's words written (the arguments from the fifth on at
/// sp+16.., the constructor's spilled pointers from sp+72), `args` in a0.. (those after left
/// as the last call left them), constructor `f` called (routed to its port).
fn new(c: &mut Cpu, r: &Saved, sp: u32, f: u32, args: &[u32], stack: &[(u32, u32)], ra: u32) {
    for &(at, v) in stack {
        c.m.w32(sp + at, v);
    }
    c.r[4..4 + args.len()].copy_from_slice(args);
    r.call_ptr_leaving(c, f, &[], ra);
}

/// `tGlobalMenuDefs::tGlobalMenuDefs` (FEMENUDEFS.CPP, 0x8002e504): every menu of the front
/// end and every item on them constructed in place in turn (each menu after its items, the
/// items given the menus they go to and their callbacks; the derived items' vtables written
/// after their bases' constructors), the sliders sized, a few flags and fields set; `this`.
/// One straight run of code: each row is one constructor's call as the original's registers
/// and stack have it.
pub fn abi_global_menu_defs(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 640, |c, sp| {
        let mut r = Saved::of(c);
        new(c, &r, sp, NFS4_BUTTON, &[this, 92, this + 1112, SET_ONE_PLAYER], &[(16, 30), (20, 10), (72, this + 1112), (640, this)], 0x8002_E560);
        new(c, &r, sp, NFS4_BUTTON, &[this + 44, 93, this + 2336, SET_TWO_PLAYER], &[(16, 40), (20, 10), (76, this + 44), (80, this + 2336)], 0x8002_E5A0);
        r.0[3] = this + 88;
        new(c, &r, sp, NFS4_BUTTON, &[this + 88, 95, this + 388, 0], &[(16, 50), (20, 10), (84, this + 388)], 0x8002_E5D4);
        r.0[0] = this + 132;
        new(c, &r, sp, NFS4_BUTTON, &[this + 132, 94, this + 9456, 0], &[(16, 60), (20, 10)], 0x8002_E600);
        let main_screen = c.m.r32(MAIN_SCREEN);
        new(c, &r, sp, NFS4_MENU, &[this + 176, 0x1014, main_screen, 0], &[(16, 0), (20, 0), (24, 181), (28, this), (32, this + 44), (36, this + 132), (40, this + 88), (44, 0)], 0x8002_E644);
        r.0[0] = this + 13192;
        new(c, &r, sp, NFS4_BUTTON, &[this + 300, 100, this + 13192, GO_TO_TOURN_TROPHY_ROOM], &[(16, 50), (20, 10), (88, this + 300)], 0x8002_E680);
        r.0[3] = this + 344;
        new(c, &r, sp, NFS4_BUTTON, &[this + 344, 106, this + 13192, GO_TO_SE_TROPHY_ROOM], &[(16, 50), (20, 10)], 0x8002_E6B0);
        new(c, &r, sp, NFS4_MENU, &[this + 388, 0x1014, main_screen, 0], &[(16, 0), (20, 0), (24, 96), (28, this + 300), (32, this + 344), (36, 0)], 0x8002_E6E8);
        let g1 = c.m.r32(0x8005_2518);
        new(c, &r, sp, BLANK_MENU, &[this + 512, 0x4010, g1, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF)], 0x8002_E714);
        r.0[3] = this + 636;
        new(c, &r, sp, NFS4_BUTTON, &[this + 636, 162, this + 3360, SET_BEGINNER], &[(16, 140), (20, 10), (92, this + 3360)], 0x8002_E74C);
        new(c, &r, sp, NFS4_BUTTON, &[this + 680, 163, this + 3360, SET_INTERMEDIATE], &[(16, 150), (20, 10), (96, this + 680)], 0x8002_E780);
        r.0[0] = this + 724;
        new(c, &r, sp, NFS4_BUTTON, &[this + 724, 164, this + 3360, SET_EXPERT], &[(16, 160), (20, 10)], 0x8002_E7B0);
        r.0[8] = this + 768;
        new(c, &r, sp, NFS4_MENU, &[this + 768, 0x1004, main_screen, 0], &[(16, 0), (20, 0), (24, 184), (28, this + 636), (32, this + 680), (36, this + 724), (40, 0)], 0x8002_E7F4);
        new(c, &r, sp, NFS4_BUTTON, &[this + 892, 97, this + 3484, SET_TEST_DRIVE], &[(16, 240), (20, 10), (100, this + 892), (104, this + 3484)], 0x8002_E834);
        new(c, &r, sp, NFS4_BUTTON, &[this + 936, 115, this + 1368, SET_SINGLE_RACE], &[(16, 70), (20, 10), (108, this + 936), (112, this + 1368)], 0x8002_E874);
        r.0[3] = this + 980;
        new(c, &r, sp, NFS4_BUTTON, &[this + 980, 114, this + 1580, SET_HOT_PURSUIT], &[(16, 80), (20, 10), (116, this + 1580)], 0x8002_E8AC);
        new(c, &r, sp, NFS4_BUTTON, &[this + 1024, 100, this + 1808, SET_TOURNAMENT], &[(16, 90), (20, 10), (120, this + 1024), (124, this + 1808)], 0x8002_E8EC);
        r.0[0] = this + 1068;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1068, 106, this + 2036, SET_SPECIAL_EVENT], &[(16, 100), (20, 10), (128, this + 2036)], 0x8002_E924);
        new(c, &r, sp, NFS4_MENU, &[this + 1112, 0x1004, main_screen, 0], &[(16, 0), (20, 0), (24, 182), (28, this + 892), (32, this + 936), (36, this + 980), (40, this + 1024), (44, this + 1068), (48, 0)], 0x8002_E970);
        r.0[3] = this + 1236;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1236, 109, this + 3360, SET_SOLO_RACE], &[(16, 110), (20, 10)], 0x8002_E9A0);
        new(c, &r, sp, NFS4_BUTTON, &[this + 1280, 110, this + 768, SET_DUEL_RACE], &[(16, 120), (20, 10), (132, this + 1280)], 0x8002_E9D4);
        r.0[0] = this + 1324;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1324, 112, this + 768, SET_FULL_GRID], &[(16, 130), (20, 10)], 0x8002_EA04);
        new(c, &r, sp, NFS4_MENU, &[this + 1368, 0x1004, main_screen, 0], &[(16, 0), (20, 0), (24, 183), (28, this + 1236), (32, this + 1280), (36, this + 1324), (40, 0)], 0x8002_EA40);
        new(c, &r, sp, NFS4_BUTTON, &[this + 1492, 109, this + 768, SET_HP_SOLO_RACE], &[(16, 110), (20, 10), (136, this + 1492)], 0x8002_EA74);
        r.0[0] = this + 1536;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1536, 110, this + 768, SET_HP_DUEL_RACE], &[(16, 120), (20, 10)], 0x8002_EAA4);
        new(c, &r, sp, NFS4_MENU, &[this + 1580, 0x1004, main_screen, 0], &[(16, 0), (20, 0), (24, 183), (28, this + 1492), (32, this + 1536), (36, 0)], 0x8002_EADC);
        new(c, &r, sp, TOURNAMENT_ITERATOR, &[this + 1704, 0x8011_59D8, TOURNAMENT_MANAGER], &[(140, this + 1704)], 0x8002_EB00);
        r.0[0] = GO_TO_TOURN_TRACK_INFO;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1724, 91, 0, GO_TO_TOURN_TRACK_INFO], &[(16, 34), (20, 10), (144, this + 1724)], 0x8002_EB38);
        r.0[3] = this + 1768;
        new(c, &r, sp, NFS4_CHOICE, &[this + 1768, 149, this + 1704, 44], &[(16, 10)], 0x8002_EB5C);
        let g2 = c.m.r32(0x8005_24EC);
        new(c, &r, sp, NFS4_MENU, &[this + 1808, 0x1000, g2, 0], &[(16, 0), (20, GO_TO_TOURN_TRACK_INFO), (24, 102), (28, this + 1724), (32, this + 1768), (36, 0)], 0x8002_EB94);
        r.0[3] = this + 1932;
        new(c, &r, sp, TOURNAMENT_ITERATOR, &[this + 1932, 0x8011_59D9, TOURNAMENT_MANAGER], &[], 0x8002_EBB8);
        r.0[0] = GO_TO_SPECIAL_EVENT_TRACK_INFO;
        new(c, &r, sp, NFS4_BUTTON, &[this + 1952, 91, 0, GO_TO_SPECIAL_EVENT_TRACK_INFO], &[(16, 34), (20, 10), (148, this + 1952)], 0x8002_EBF0);
        new(c, &r, sp, NFS4_CHOICE, &[this + 1996, 106, this + 1932, 54], &[(16, 10), (152, this + 1996)], 0x8002_EC18);
        let g3 = c.m.r32(0x8005_24EC);
        new(c, &r, sp, NFS4_MENU, &[this + 2036, 0x1000, g3, 0], &[(16, 0), (20, GO_TO_SPECIAL_EVENT_TRACK_INFO), (24, 101), (28, this + 1952), (32, this + 1996), (36, 0)], 0x8002_EC54);
        new(c, &r, sp, NFS4_BUTTON, &[this + 2160, 97, this + 3484, SET_TEST_DRIVE], &[(16, 240), (20, 10), (156, this + 2160)], 0x8002_EC88);
        new(c, &r, sp, NFS4_BUTTON, &[this + 2204, 115, 0, GO_TO_TWO_PLAYER_SINGLE_RACE], &[(16, 70), (20, 10), (160, this + 2204)], 0x8002_ECBC);
        r.0[3] = this + 2248;
        new(c, &r, sp, NFS4_BUTTON, &[this + 2248, 114, this + 768, SET_HOT_PURSUIT], &[(16, 80), (20, 10)], 0x8002_ECEC);
        r.0[0] = 170;
        new(c, &r, sp, NFS4_BUTTON, &[this + 2292, 107, this + 2592, SET_PINK_SLIPS], &[(16, 170), (20, 10), (164, this + 2292), (168, this + 2592)], 0x8002_ED2C);
        new(c, &r, sp, NFS4_MENU, &[this + 2336, 0x1004, main_screen, 0], &[(16, 0), (20, 0), (24, 182), (28, this + 2160), (32, this + 2204), (36, this + 2248), (40, this + 2292), (44, 0)], 0x8002_ED74);
        new(c, &r, sp, NFS4_BUTTON, &[this + 2460, 193, this + 3360, GO_TO_BEST_OF_ONE], &[(16, 170), (20, 10), (172, this + 2460)], 0x8002_EDA4);
        r.0[3] = this + 2504;
        new(c, &r, sp, NFS4_BUTTON, &[this + 2504, 194, this + 2960, GO_TO_BEST_OF_THREE], &[(16, 170), (20, 10), (176, this + 2960)], 0x8002_EDD8);
        new(c, &r, sp, NFS4_BUTTON, &[this + 2548, 195, this + 3084, GO_TO_BEST_OF_FIVE], &[(16, 170), (20, 10), (180, this + 2548), (184, this + 3084)], 0x8002_EE14);
        new(c, &r, sp, NFS4_MENU, &[this + 2592, 0x1000, main_screen, 0], &[(16, 0), (20, 0), (24, 183), (28, this + 2460), (32, this + 2504), (36, this + 2548), (40, 0)], 0x8002_EE54);
        r.0[8] = this + 2716;
        new(c, &r, sp, NFS4_BUTTON, &[this + 2716, 91, 0, GO_TO_CAR_SELECT], &[(16, 36), (20, 10)], 0x8002_EE84);
        r.0[0] = this + 3208;
        new(c, &r, sp, NFS4_CHOICE, &[this + 2760, 196, this + 3208, 46], &[(16, 10), (188, this + 2760)], 0x8002_EEB4);
        new(c, &r, sp, NFS4_CHOICE, &[this + 2800, 197, this + 3208, 46], &[(16, 10), (192, this + 2800)], 0x8002_EEDC);
        r.0[3] = this + 2840;
        new(c, &r, sp, NFS4_CHOICE, &[this + 2840, 198, this + 3208, 46], &[(16, 10)], 0x8002_EF00);
        r.0[2] = this + 2880;
        new(c, &r, sp, NFS4_CHOICE, &[this + 2880, 199, this + 3208, 46], &[(16, 10)], 0x8002_EF24);
        r.0[4] = this + 2920;
        new(c, &r, sp, NFS4_CHOICE, &[this + 2920, 200, this + 3208, 46], &[(16, 10)], 0x8002_EF48);
        let g4 = c.m.r32(0x8005_22F4);
        new(c, &r, sp, NFS4_MENU, &[this + 2960, 0x1400, g4, 0], &[(16, this + 3988), (20, GO_TO_CAR_SELECT), (24, 191), (28, this + 2716), (32, this + 2760), (36, this + 2800), (40, this + 2840), (44, 0), (196, this + 3988)], 0x8002_EFA0);
        let g5 = c.m.r32(0x8005_22F4);
        new(c, &r, sp, NFS4_MENU, &[this + 3084, 0x1400, g5, 0], &[(16, this + 3988), (20, GO_TO_CAR_SELECT), (24, 192), (28, this + 2716), (32, this + 2760), (36, this + 2800), (40, this + 2840), (44, this + 2880), (48, this + 2920), (52, 0)], 0x8002_EFF8);
        new(c, &r, sp, TRACK_ITERATOR, &[this + 3208, 0x8011_58DB, 0x8011_58FE, TRACK_MANAGER], &[], 0x8002_F01C);
        r.0[8] = this + 3232;
        new(c, &r, sp, NFS4_BUTTON, &[this + 3232, 91, 0, GO_TO_CAR_SELECT], &[(16, 28), (20, 10)], 0x8002_F04C);
        new(c, &r, sp, NFS4_CHOICE, &[this + 3276, 148, this + 3208, 38], &[(16, 10), (200, this + 3276)], 0x8002_F074);
        r.0[0] = this + 3316;
        new(c, &r, sp, NFS4_BUTTON, &[this + 3316, 213, this + 4164, 0], &[(16, 58), (20, 10), (204, this + 4164)], 0x8002_F0A8);
        let g6 = c.m.r32(0x8005_2544);
        r.0[3] = 201;
        new(c, &r, sp, NFS4_MENU, &[this + 3360, 0x1600, g6, 0], &[(16, this + 3988), (20, GO_TO_CAR_SELECT), (24, 201), (28, this + 3232), (32, this + 3276), (36, this + 3316), (40, 0)], 0x8002_F0F0);
        let g7 = c.m.r32(0x8005_2544);
        new(c, &r, sp, NFS4_MENU, &[this + 3484, 0x1600, g7, 0], &[(16, 0), (20, GO_TO_CAR_SELECT), (24, 201), (28, this + 3232), (32, this + 3276), (36, this + 3316), (40, 0)], 0x8002_F130);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3608, 0x8005_1E04, 0x8011_58E5, 0x8011_58FE], &[(208, this + 3608)], 0x8002_F160);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3628, 0x8005_1E14, 0x8011_58EA, 0x8011_58FE], &[(212, this + 3628)], 0x8002_F190);
        r.0[0] = 0x8005_1DFC;
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3648, 0x8005_1DFC, 0x8011_58EF, 0x8011_58FE], &[(216, this + 3648)], 0x8002_F1C4);
        r.0[3] = this + 3668;
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3668, 0x8005_1DFC, 0x8011_58CE, 0x8011_58FE], &[], 0x8002_F1EC);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3688, 0x8005_1DFC, 0x8011_58D3, 0x8011_58FE], &[(220, this + 3688)], 0x8002_F218);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 3708, 0x8005_1DFC, 0x8011_58F4, 0x8011_58FE], &[(224, this + 3708)], 0x8002_F244);
        new(c, &r, sp, ITERATOR, &[this + 3728, 0x8005_1DFC, 0x8011_58D9], &[(228, this + 3728)], 0x8002_F268);
        r.0[6] = this + 3744;
        new(c, &r, sp, CHOICE, &[this + 3744, 203, this + 3608], &[], 0x8002_F280);
        c.m.w32(this + 3768, 0x8001_0850);
        r.0[0] = 0x8001_0850;
        r.0[7] = this + 3776;
        new(c, &r, sp, CHOICE, &[this + 3776, 205, this + 3628], &[], 0x8002_F2A4);
        c.m.w32(this + 3800, 0x8001_0850);
        r.0[8] = this + 3808;
        new(c, &r, sp, CHOICE, &[this + 3808, 206, this + 3648], &[], 0x8002_F2C0);
        c.m.w32(this + 3832, 0x8001_07F8);
        c.m.w16(this + 3840, 128);
        r.0[0] = 0x8001_07F8;
        r.0[3] = 128;
        new(c, &r, sp, CHOICE, &[this + 3844, 207, this + 3668], &[(232, this + 3844)], 0x8002_F2F0);
        c.m.w32(this + 3868, 0x8001_07F8);
        c.m.w16(this + 3876, 128);
        new(c, &r, sp, CHOICE, &[this + 3880, 208, this + 3688], &[(236, this + 3880)], 0x8002_F318);
        c.m.w32(this + 3904, 0x8001_07F8);
        c.m.w16(this + 3912, 128);
        new(c, &r, sp, CHOICE, &[this + 3916, 209, this + 3708], &[(240, this + 3916)], 0x8002_F340);
        c.m.w32(this + 3940, 0x8001_07F8);
        c.m.w16(this + 3948, 128);
        new(c, &r, sp, CHOICE, &[this + 3952, 211, this + 3728], &[(244, this + 3952)], 0x8002_F368);
        c.m.w32(this + 3976, 0x8001_07F8);
        c.m.w16(this + 3984, 128);
        new(c, &r, sp, OPTIONS, &[this + 3988, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 186), (28, 0xFFFF_FFFF), (32, this + 3744), (36, this + 3776), (40, this + 3808), (44, this + 3844), (48, this + 3880), (52, this + 3916), (56, this + 3952), (60, 0)], 0x8002_F3D0);
        r.0[0] = this + 4120;
        new(c, &r, sp, NFS4_BUTTON, &[this + 4120, 0, 0, 0], &[(16, 0xFFFF_FFFF), (20, 0xFFFF_FFFF)], 0x8002_F3FC);
        let g8 = c.m.r32(0x8005_2560);
        c.m.w32(this + 4144, 0x8001_1480);
        new(c, &r, sp, OPTIONS_MENU, &[this + 4164, 0x1000, g8, 0], &[(16, 0), (20, 0), (24, 213), (28, 1), (32, 10), (36, 0)], 0x8002_F444);
        r.0[0] = this + 4292;
        new(c, &r, sp, NFS4_BUTTON, &[this + 4292, 91, 0, GO_TO_GARAGE], &[(16, 33), (20, 10)], 0x8002_F474);
        let g9 = c.m.r32(0x8005_255C);
        new(c, &r, sp, NFS4_MENU, &[this + 4336, 0x1004, g9, 0], &[(16, 0), (20, 0), (24, 250), (28, this + 4292), (32, 0)], 0x8002_F4A8);
        new(c, &r, sp, CAR_ITERATOR, &[this + 4460, 0x8011_5914, CAR_MANAGER], &[(248, this + 4460)], 0x8002_F4CC);
        let fe_app = c.m.r32(FE_APP);
        new(c, &r, sp, CAR_COLOR_ITERATOR, &[this + 4488, 0x8011_5918, fe_app + 556, 0x8011_5914], &[(16, 48), (20, CAR_MANAGER), (252, this + 4488)], 0x8002_F510);
        new(c, &r, sp, NFS4_BUTTON, &[this + 4520, 190, 0, GO_TO_RACE], &[(16, 128), (20, 10), (256, this + 4520)], 0x8002_F544);
        new(c, &r, sp, NFS4_CHOICE, &[this + 4564, 147, this + 4460, 28], &[(16, 10), (260, this + 4564)], 0x8002_F56C);
        new(c, &r, sp, NFS4_CHOICE, &[this + 4604, 289, this + 4488, 38], &[(16, 10), (264, this + 4604)], 0x8002_F594);
        r.0[0] = this + 4644;
        new(c, &r, sp, NFS4_BUTTON, &[this + 4644, 275, 0, GO_TO_SHOWROOM], &[(16, 48), (20, 10)], 0x8002_F5C4);
        let car_select = c.m.r32(CAR_SELECT);
        new(c, &r, sp, NFS4_MENU, &[this + 4688, 0x1A00, car_select, 0], &[(16, this + 8408), (20, GO_TO_RACE), (24, 187), (28, this + 4520), (32, this + 4564), (36, this + 4604), (40, this + 4644), (44, 0), (268, this + 8408)], 0x8002_F620);
        new(c, &r, sp, CAR_ITERATOR, &[this + 4812, 0x8011_59DB, CAR_MANAGER], &[(272, this + 4812)], 0x8002_F644);
        r.0[8] = this + 4840;
        new(c, &r, sp, NFS4_CHOICE, &[this + 4840, 147, this + 4812, 28], &[(16, 10)], 0x8002_F668);
        new(c, &r, sp, NFS4_BUTTON, &[this + 4880, 117, this + 7144, 0], &[(16, 58), (20, 10), (276, this + 4880), (280, this + 7144)], 0x8002_F6A4);
        r.0[0] = this + 4924;
        new(c, &r, sp, NFS4_BUTTON, &[this + 4924, 146, 0, GO_TO_UPGRADES], &[(16, 68), (20, 10)], 0x8002_F6D4);
        r.0[3] = 144;
        new(c, &r, sp, NFS4_MENU, &[this + 4968, 0x1A00, car_select, 0], &[(16, this + 8408), (20, GO_TO_RACE), (24, 144), (28, this + 4520), (32, this + 4840), (36, this + 4880), (40, this + 4924), (44, 0)], 0x8002_F728);
        new(c, &r, sp, NFS4_MENU, &[this + 5092, 0x1A00, car_select, 0], &[(16, this + 8408), (20, GO_TO_RACE), (24, 144), (28, this + 4520), (32, this + 4924), (36, 0)], 0x8002_F76C);
        r.0[3] = this + 5216;
        new(c, &r, sp, CAR_ITERATOR, &[this + 5216, 0x8011_5BD5, CAR_MANAGER], &[], 0x8002_F790);
        new(c, &r, sp, NFS4_BUTTON, &[this + 5244, 190, 0, GO_TO_RACE], &[(16, 42), (20, 10), (284, this + 5244)], 0x8002_F7C4);
        new(c, &r, sp, NFS4_CHOICE, &[this + 5288, 147, this + 4460, 12], &[(16, 10)], 0x8002_F7E4);
        new(c, &r, sp, NFS4_CHOICE, &[this + 5328, 289, this + 4488, 22], &[(16, 10)], 0x8002_F804);
        new(c, &r, sp, NFS4_BUTTON, &[this + 5368, 121, this + 7496, GO_TO_DEALER], &[(16, 0xFFFF_FFFF), (20, 10), (288, this + 5368), (292, this + 7496)], 0x8002_F844);
        r.0[0] = this + 5412;
        new(c, &r, sp, NFS4_CHOICE, &[this + 5412, 189, this + 5216, 32], &[(16, 10)], 0x8002_F868);
        let duel_select = c.m.r32(DUEL_SELECT);
        r.0[3] = 0x8005_0000;
        new(c, &r, sp, NFS4_MENU, &[this + 5452, 0x1800, duel_select, 0], &[(16, this + 8408), (20, GO_TO_RACE), (24, 187), (28, this + 5244), (32, this + 4840), (36, this + 5368), (40, this + 5412), (44, 0)], 0x8002_F8BC);
        new(c, &r, sp, NFS4_MENU, &[this + 5576, 0x1800, duel_select, 0], &[(16, this + 8408), (20, GO_TO_RACE), (24, 187), (28, this + 5244), (32, this + 4564), (36, this + 4604), (40, this + 5412), (44, 0)], 0x8002_F910);
        r.0[0] = GO_TO_2_PLAYER_RACE;
        new(c, &r, sp, NFS4_BUTTON, &[this + 5700, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10), (296, this + 5700)], 0x8002_F948);
        new(c, &r, sp, NFS4_CHOICE, &[this + 5744, 147, this + 4460, 12], &[(16, 10), (300, this + 5744)], 0x8002_F970);
        r.0[3] = this + 5784;
        new(c, &r, sp, NFS4_CHOICE, &[this + 5784, 289, this + 4488, 22], &[(16, 10)], 0x8002_F994);
        let g10 = c.m.r32(0x8005_24D4);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 5824, 0x1008, g10, 0], &[(16, this + 8540), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 5700), (32, this + 5744), (36, this + 5784), (40, 0), (304, this + 8540)], 0x8002_F9E0);
        new(c, &r, sp, NFS4_BUTTON, &[this + 5948, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10), (308, this + 5948)], 0x8002_FA10);
        new(c, &r, sp, NFS4_CHOICE, &[this + 5992, 147, this + 4460, 12], &[(16, 10), (312, this + 5992)], 0x8002_FA38);
        r.0[3] = this + 6032;
        new(c, &r, sp, NFS4_CHOICE, &[this + 6032, 289, this + 4488, 22], &[(16, 10)], 0x8002_FA5C);
        let g11 = c.m.r32(0x8005_24D8);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 6072, 0x1008, g11, 0], &[(16, this + 8672), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 5948), (32, this + 5992), (36, this + 6032), (40, 0), (316, this + 8672)], 0x8002_FAA8);
        new(c, &r, sp, NFS4_BUTTON, &[this + 6196, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10)], 0x8002_FAD0);
        r.0[3] = this + 6240;
        new(c, &r, sp, NFS4_CHOICE, &[this + 6240, 147, this + 4812, 12], &[(16, 10)], 0x8002_FAF4);
        let g12 = c.m.r32(0x8005_24D4);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 6280, 0x1008, g12, 0], &[(16, this + 8540), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 5700), (32, this + 6240), (36, 0)], 0x8002_FB34);
        new(c, &r, sp, NFS4_BUTTON, &[this + 6404, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10), (320, this + 6404)], 0x8002_FB64);
        r.0[3] = this + 6448;
        new(c, &r, sp, NFS4_CHOICE, &[this + 6448, 147, this + 4812, 12], &[(16, 10)], 0x8002_FB88);
        let g13 = c.m.r32(0x8005_24D8);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 6488, 0x1008, g13, 0], &[(16, this + 8672), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 6404), (32, this + 6448), (36, 0)], 0x8002_FBC8);
        r.0[3] = this + 6612;
        new(c, &r, sp, CAR_ITERATOR, &[this + 6612, 0x8011_59DD, CAR_MANAGER], &[], 0x8002_FBEC);
        new(c, &r, sp, NFS4_BUTTON, &[this + 6640, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10), (324, this + 6640)], 0x8002_FC1C);
        new(c, &r, sp, NFS4_CHOICE, &[this + 6684, 147, this + 6612, 12], &[(16, 10), (328, this + 6684)], 0x8002_FC44);
        let g14 = c.m.r32(0x8005_24DC);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 6724, 0x1008, g14, 0], &[(16, this + 8804), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 6640), (32, this + 6684), (36, 0), (332, this + 8804)], 0x8002_FC8C);
        new(c, &r, sp, NFS4_BUTTON, &[this + 6848, 190, 0, GO_TO_2_PLAYER_RACE], &[(16, 42), (20, 10), (336, this + 6848)], 0x8002_FCBC);
        new(c, &r, sp, NFS4_CHOICE, &[this + 6892, 147, this + 6612, 12], &[(16, 10), (340, this + 6892)], 0x8002_FCE4);
        let g15 = c.m.r32(0x8005_24E0);
        new(c, &r, sp, TWO_PLAYER_MENU, &[this + 6932, 0x1008, g15, 0], &[(16, this + 8936), (20, GO_TO_2_PLAYER_RACE), (24, 187), (28, this + 6848), (32, this + 6892), (36, 0), (344, this + 8936)], 0x8002_FD2C);
        r.0[3] = this + 7056;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7056, 121, this + 7496, GO_TO_DEALER], &[(16, 88), (20, 10)], 0x8002_FD5C);
        r.0[0] = this + 7100;
        r.0[8] = this + 7732;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7100, 122, this + 7732, GO_TO_SELLER], &[(16, 78), (20, 10)], 0x8002_FD90);
        new(c, &r, sp, NFS4_MENU, &[this + 7144, 0x2200, car_select, 0], &[(16, 0), (20, 0), (24, 145), (28, this + 7056), (32, this + 7100), (36, 0)], 0x8002_FDC4);
        r.0[0] = 0x8011_59E0;
        r.0[3] = this + 7268;
        new(c, &r, sp, CAR_ITERATOR, &[this + 7268, 0x8011_59E0, CAR_MANAGER], &[], 0x8002_FDEC);
        new(c, &r, sp, CAR_COLOR_ITERATOR, &[this + 7296, 0x8011_5918, fe_app + 556, 0x8011_59E0], &[(16, 48), (20, CAR_MANAGER), (348, this + 7296)], 0x8002_FE2C);
        new(c, &r, sp, NFS4_CHOICE, &[this + 7328, 147, this + 7268, 28], &[(16, 10), (352, this + 7328)], 0x8002_FE54);
        new(c, &r, sp, NFS4_CHOICE, &[this + 7368, 289, this + 7296, 38], &[(16, 10), (356, this + 7368)], 0x8002_FE7C);
        r.0[3] = this + 7408;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7408, 118, 0, BUY_CAR], &[(16, 88), (20, 10)], 0x8002_FEAC);
        r.0[0] = this + 7452;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7452, 275, 0, GO_TO_DEALER_SHOWROOM], &[(16, 48), (20, 10)], 0x8002_FEDC);
        new(c, &r, sp, NFS4_MENU, &[this + 7496, 0x2240, car_select, 0], &[(16, 0), (20, 0), (24, 145), (28, this + 7328), (32, this + 7368), (36, this + 7408), (40, this + 7452), (44, 0)], 0x8002_FF20);
        r.0[0] = this + 7620;
        new(c, &r, sp, CAR_ITERATOR, &[this + 7620, 0x8011_59E1, CAR_MANAGER], &[], 0x8002_FF44);
        r.0[3] = this + 7648;
        new(c, &r, sp, NFS4_CHOICE, &[this + 7648, 147, this + 7620, 28], &[(16, 10)], 0x8002_FF68);
        r.0[0] = this + 7688;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7688, 120, 0, SELL_CAR], &[(16, 78), (20, 10)], 0x8002_FF98);
        new(c, &r, sp, NFS4_MENU, &[this + 7732, 0x2200, car_select, 0], &[(16, 0), (20, 0), (24, 145), (28, this + 7648), (32, this + 7688), (36, 0)], 0x8002_FFCC);
        new(c, &r, sp, NFS4_BUTTON, &[this + 7856, 151, 0, PURCHASE_UPGRADE1], &[(16, 98), (20, 10), (360, this + 7856)], 0x8003_0000);
        r.0[3] = this + 7900;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7900, 152, 0, PURCHASE_UPGRADE2], &[(16, 108), (20, 10)], 0x8003_0030);
        r.0[0] = this + 7944;
        new(c, &r, sp, NFS4_BUTTON, &[this + 7944, 153, 0, PURCHASE_UPGRADE3], &[(16, 118), (20, 10)], 0x8003_0060);
        new(c, &r, sp, NFS4_MENU, &[this + 7988, 0x2200, car_select, 0], &[(16, 0), (20, 0), (24, 146), (28, this + 7856), (32, this + 7900), (36, this + 7944), (40, 0)], 0x8003_00A0);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 8112, 0x8005_1E1C, 0x8011_5916, fe_app + 556], &[(364, this + 8112)], 0x8003_00D0);
        r.0[3] = this + 8132;
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 8132, 0x8005_1DFC, 0x8011_5BCC, fe_app + 556], &[], 0x8003_0100);
        new(c, &r, sp, ITERATOR, &[this + 8152, 0x8005_1DFC, 0x8011_5BCE], &[(368, this + 8152)], 0x8003_0128);
        r.0[1] = this + 8168;
        new(c, &r, sp, ITERATOR, &[this + 8168, 0x8005_1E0C, 0x8011_58DA], &[], 0x8003_014C);
        new(c, &r, sp, CHOICE, &[this + 8184, 267, this + 8112], &[(372, this + 8184)], 0x8003_0168);
        c.m.w32(this + 8208, 0x8001_0850);
        r.0[0] = 0x8001_0850;
        new(c, &r, sp, CHOICE, &[this + 8216, 268, this + 8132], &[(376, this + 8216)], 0x8003_0194);
        c.m.w32(this + 8240, 0x8001_0850);
        new(c, &r, sp, CHOICE, &[this + 8248, 274, this + 8152], &[(380, this + 8248)], 0x8003_01B8);
        c.m.w32(this + 8272, 0x8001_0850);
        r.0[8] = this + 8280;
        new(c, &r, sp, CHOICE, &[this + 8280, 267, this + 8112], &[], 0x8003_01D8);
        c.m.w32(this + 8304, 0x8001_0850);
        new(c, &r, sp, CHOICE, &[this + 8312, 268, this + 8132], &[(384, this + 8312)], 0x8003_01F8);
        c.m.w32(this + 8336, 0x8001_0850);
        r.0[3] = this + 8344;
        new(c, &r, sp, CHOICE, &[this + 8344, 274, this + 8152], &[], 0x8003_0218);
        c.m.w32(this + 8368, 0x8001_0850);
        new(c, &r, sp, CHOICE, &[this + 8376, 271, this + 8168], &[(388, this + 8376)], 0x8003_0238);
        c.m.w32(this + 8400, 0x8001_0850);
        r.0[0] = 188;
        new(c, &r, sp, OPTIONS, &[this + 8408, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 188), (28, 0xFFFF_FFFF), (32, this + 8184), (36, this + 8216), (40, this + 8248), (44, this + 8376), (48, 0)], 0x8003_0290);
        let g16 = c.m.r32(0x8005_24D4);
        new(c, &r, sp, OPTIONS, &[this + 8540, 0x1008, g16, 0], &[(16, 0), (20, 0), (24, 188), (28, 0), (32, this + 8184), (36, this + 8216), (40, this + 8248), (44, 0)], 0x8003_02D4);
        let g17 = c.m.r32(0x8005_24D8);
        new(c, &r, sp, OPTIONS, &[this + 8672, 0x1008, g17, 0], &[(16, 0), (20, 0), (24, 188), (28, 1), (32, this + 8280), (36, this + 8312), (40, this + 8344), (44, 0)], 0x8003_0314);
        let g18 = c.m.r32(0x8005_24DC);
        new(c, &r, sp, OPTIONS, &[this + 8804, 0x1008, g18, 0], &[(16, 0), (20, 0), (24, 188), (28, 0), (32, this + 8184), (36, this + 8216), (40, this + 8248), (44, 0)], 0x8003_0358);
        let g19 = c.m.r32(0x8005_24E0);
        new(c, &r, sp, OPTIONS, &[this + 8936, 0x1008, g19, 0], &[(16, 0), (20, 0), (24, 188), (28, 1), (32, this + 8280), (36, this + 8312), (40, this + 8344), (44, 0)], 0x8003_0398);
        new(c, &r, sp, BLANK_MENU, &[this + 9068, 0x4000, 0, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF)], 0x8003_03C0);
        new(c, &r, sp, NFS4_BUTTON, &[this + 9192, 454, this + 10208, 0], &[(16, 180), (20, 10), (392, this + 9192), (396, this + 10208)], 0x8003_03FC);
        new(c, &r, sp, NFS4_BUTTON, &[this + 9236, 455, this + 10856, 0], &[(16, 190), (20, 10), (400, this + 9236), (404, this + 10856)], 0x8003_0438);
        new(c, &r, sp, NFS4_BUTTON, &[this + 9280, 457, this + 11116, 0], &[(16, 200), (20, 10), (408, this + 9280), (412, this + 11116)], 0x8003_0474);
        r.0[3] = this + 9324;
        new(c, &r, sp, NFS4_BUTTON, &[this + 9324, 458, this + 12460, 0], &[(16, 210), (20, 10)], 0x8003_04A0);
        new(c, &r, sp, NFS4_BUTTON, &[this + 9368, 459, 0, ENTER_USER_NAME], &[(16, 220), (20, 10), (416, this + 9368)], 0x8003_04D4);
        r.0[0] = this + 9412;
        new(c, &r, sp, NFS4_BUTTON, &[this + 9412, 461, this + 14832, 0], &[(16, 230), (20, 10)], 0x8003_0500);
        new(c, &r, sp, NFS4_MENU, &[this + 9456, 0x1014, main_screen, 0], &[(16, 0), (20, 0), (24, 94), (28, this + 9192), (32, this + 9236), (36, this + 9280), (40, this + 9324), (44, this + 9368), (48, this + 9412), (52, 0)], 0x8003_0558);
        r.0[0] = this + 9580;
        new(c, &r, sp, RANGE_ITERATOR, &[this + 9580, 0, 127, 0x8011_5BF9], &[], 0x8003_057C);
        new(c, &r, sp, RANGE_ITERATOR, &[this + 9596, 0, 127, 0x8011_5BFA], &[(420, this + 9596)], 0x8003_05A4);
        r.0[3] = this + 9612;
        new(c, &r, sp, RANGE_ITERATOR, &[this + 9612, 0, 127, 0x8011_5BFC], &[], 0x8003_05C8);
        new(c, &r, sp, RANGE_ITERATOR, &[this + 9628, 0, 127, 0x8011_5BFB], &[(424, this + 9628)], 0x8003_05F0);
        new(c, &r, sp, RANGE_ITERATOR, &[this + 9644, 0, 127, 0x8011_5BFD], &[(428, this + 9644)], 0x8003_0618);
        new(c, &r, sp, ITERATOR, &[this + 9660, 0x8005_1E60, 0x8011_5BFE], &[(432, this + 9660)], 0x8003_0640);
        new(c, &r, sp, AUDIO_SLIDER, &[this + 9676, 470, this + 9580, 0], &[(436, this + 9676)], 0x8003_0660);
        new(c, &r, sp, AUDIO_SLIDER, &[this + 9732, 471, this + 9596, 1], &[(440, this + 9732)], 0x8003_0680);
        r.0[8] = this + 9788;
        new(c, &r, sp, AUDIO_SLIDER, &[this + 9788, 472, this + 9612, 2], &[], 0x8003_069C);
        new(c, &r, sp, AUDIO_SLIDER, &[this + 9844, 473, this + 9628, 3], &[(444, this + 9844)], 0x8003_06BC);
        r.0[3] = this + 9900;
        new(c, &r, sp, AUDIO_SLIDER, &[this + 9900, 474, this + 9644, 4], &[], 0x8003_06D8);
        new(c, &r, sp, FADE, &[this + 9956, 475, this + 9660], &[(448, this + 9956)], 0x8003_06F4);
        c.m.w32(this + 9980, 0x8001_0D88);
        r.0[0] = this + 10000;
        new(c, &r, sp, SLIDING_MENU, &[this + 10000, 476, 350, 43], &[(16, 0xFFFF_FF9A), (20, 13), (24, 0)], 0x8003_0734);
        c.m.w32(this + 10024, 0x8001_0DE0);
        new(c, &r, sp, SONG_MENU, &[this + 10072, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 0), (28, 0)], 0x8003_0768);
        let g20 = c.m.r32(0x8005_2D78);
        new(c, &r, sp, OPTIONS_MENU, &[this + 10208, 0x1010, g20, 0], &[(16, 0), (20, 0), (24, 0x264), (28, 38), (32, 10), (36, this + 9676), (40, this + 9732), (44, this + 9788), (48, this + 9844), (52, this + 9900), (56, this + 9956), (60, this + 10000), (64, 0)], 0x8003_07D0);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10336, 0x8005_1E24, 0x8011_5BD8, fe_app + 557], &[(452, this + 10336)], 0x8003_0800);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10356, 0x8005_1E30, 0x8011_5BDC, fe_app + 557], &[(456, this + 10356)], 0x8003_0834);
        r.0[3] = this + 10376;
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10376, 0x8005_1E38, 0x8011_5BDE, fe_app + 557], &[], 0x8003_0864);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10396, 0x8005_1DFC, 0x8011_5BE0, fe_app + 557], &[(460, this + 10396)], 0x8003_0898);
        r.0[0] = this + 10416;
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10416, 0x8005_1DFC, 0x8011_5BE2, fe_app + 557], &[], 0x8003_08C8);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10436, 0x8005_1DFC, 0x8011_5BE4, fe_app + 557], &[(464, this + 10436)], 0x8003_08FC);
        new(c, &r, sp, ITERATOR, &[this + 10456, 0x8005_1E40, 0x8011_5C4E], &[(468, this + 10456)], 0x8003_0924);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10472, 0x8005_1E48, 0x8011_5C4F, fe_app + 557], &[(472, this + 10472)], 0x8003_0958);
        r.0[5] = this + 10492;
        new(c, &r, sp, FADE, &[this + 10492, 480, this + 10336], &[], 0x8003_0970);
        c.m.w32(this + 10516, 0x8001_0D88);
        r.0[8] = this + 10536;
        new(c, &r, sp, FADE, &[this + 10536, 482, this + 10356], &[], 0x8003_0994);
        c.m.w32(this + 10560, 0x8001_0D88);
        new(c, &r, sp, FADE, &[this + 10580, 483, this + 10376], &[(476, this + 10580)], 0x8003_09BC);
        c.m.w32(this + 10604, 0x8001_0D88);
        new(c, &r, sp, FADE, &[this + 10624, 484, this + 10396], &[(480, this + 10624)], 0x8003_09E8);
        c.m.w32(this + 10648, 0x8001_0D30);
        r.0[0] = 0x8001_0D30;
        new(c, &r, sp, FADE, &[this + 10672, 487, this + 10416], &[(484, this + 10672)], 0x8003_0A14);
        c.m.w32(this + 10696, 0x8001_0D30);
        r.0[3] = this + 10720;
        new(c, &r, sp, FADE, &[this + 10720, 488, this + 10436], &[], 0x8003_0A34);
        c.m.w32(this + 10744, 0x8001_0D30);
        new(c, &r, sp, FADE, &[this + 10768, 485, this + 10456], &[(488, this + 10768)], 0x8003_0A54);
        c.m.w32(this + 10792, 0x8001_0D88);
        r.0[0] = this + 10812;
        new(c, &r, sp, FADE, &[this + 10812, 486, this + 10472], &[], 0x8003_0A7C);
        let g21 = c.m.r32(0x8005_2D68);
        c.m.w32(this + 10836, 0x8001_0D88);
        new(c, &r, sp, OPTIONS_MENU, &[this + 10856, 0x1020, g21, 0], &[(16, 0), (20, 0), (24, 478), (28, 1), (32, 10), (36, this + 10492), (40, this + 10536), (44, this + 10580), (48, this + 10624), (52, this + 10768), (56, this + 10812), (60, this + 10672), (64, this + 10720), (68, 0)], 0x8003_0AF4);
        new(c, &r, sp, INDEXED_ITERATOR, &[this + 10984, 0x8005_1E58, 0x8011_5C1A, fe_app + 557], &[(492, this + 10984)], 0x8003_0B28);
        r.0[0] = this + 11004;
        new(c, &r, sp, FADE, &[this + 11004, 0x20A, this + 10984], &[], 0x8003_0B40);
        c.m.w32(this + 11028, 0x8001_0C18);
        new(c, &r, sp, SLIDING_MENU, &[this + 11048, 0x20B, 172, 72], &[(16, 0), (20, 13), (24, 1), (496, this + 11048)], 0x8003_0B80);
        let g22 = c.m.r32(0x8005_2D60);
        new(c, &r, sp, OPTIONS_MENU, &[this + 11116, 0x1020, g22, 0], &[(16, 0), (20, 0), (24, 0x209), (28, 0), (32, 10), (36, this + 11004), (40, this + 11048), (44, 0)], 0x8003_0BC4);
        r.0[0] = this + 11244;
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11244, 0, 127, 0x8011_5C11], &[(16, fe_app + 557)], 0x8003_0BF8);
        new(c, &r, sp, INSIDE_SLIDER, &[this + 11264, 0x20F, this + 11244], &[(500, this + 11264)], 0x8003_0C14);
        c.m.w32(this + 11288, 0x8001_0A08);
        r.0[0] = 0x8001_0A08;
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11304, 0, 127, 0x8011_5C13], &[(16, fe_app + 557), (504, this + 11304)], 0x8003_0C5C);
        new(c, &r, sp, INSIDE_SLIDER, &[this + 11324, 0x210, this + 11304], &[(508, this + 11324)], 0x8003_0C78);
        c.m.w32(this + 11348, 0x8001_0A08);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11364, 0, 127, 0x8011_5C09], &[(16, fe_app + 557), (512, this + 11364)], 0x8003_0CB8);
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11384, 0x212, this + 11364, 0], &[(516, this + 11384)], 0x8003_0CD8);
        r.0[0] = 0x8005_0000;
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11432, 0, 127, 0x8011_5C0B], &[(16, fe_app + 557), (520, this + 11432)], 0x8003_0D10);
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11452, 0x214, this + 11432, 1], &[(524, this + 11452)], 0x8003_0D30);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11500, 0, 127, 0x8011_5C0D], &[(16, fe_app + 557), (528, this + 11500)], 0x8003_0D64);
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11520, 0x211, this + 11500, 2], &[(532, this + 11520)], 0x8003_0D84);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11568, 0, 127, 0x8011_5C0F], &[(16, fe_app + 557), (536, this + 11568)], 0x8003_0DB8);
        r.0[3] = this + 11588;
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11588, 0x213, this + 11568, 3], &[], 0x8003_0DD4);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11636, 0, 127, 0x8011_5C01], &[(16, fe_app + 557), (540, this + 11636)], 0x8003_0E08);
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11656, 0x215, this + 11636, 0], &[(544, this + 11656)], 0x8003_0E28);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11704, 0, 127, 0x8011_5C03], &[(16, fe_app + 557), (548, this + 11704)], 0x8003_0E5C);
        r.0[8] = this + 11724;
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11724, 0x216, this + 11704, 1], &[], 0x8003_0E78);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11772, 0, 255, 0x8011_5C05], &[(16, fe_app + 557), (552, this + 11772)], 0x8003_0EAC);
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11792, 0x217, this + 11772, 2], &[(556, this + 11792)], 0x8003_0ECC);
        new(c, &r, sp, RANGE_INDEXED_ITERATOR, &[this + 11840, 0, 255, 0x8011_5C07], &[(16, fe_app + 557), (560, this + 11840)], 0x8003_0F00);
        r.0[0] = this + 11860;
        new(c, &r, sp, TWO_WAY_SLIDER, &[this + 11860, 0x218, this + 11840, 3], &[], 0x8003_0F1C);
        new(c, &r, sp, INSIDE_BOX_MENU, &[this + 11908, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 0), (28, this + 11264), (32, this + 11324), (36, 0)], 0x8003_0F54);
        new(c, &r, sp, INSIDE_BOX_MENU, &[this + 12024, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 0), (28, this + 11384), (32, this + 11452), (36, this + 11520), (40, this + 11588), (44, 0)], 0x8003_0F98);
        new(c, &r, sp, INSIDE_BOX_MENU, &[this + 12140, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 0), (28, this + 11264), (32, this + 11324), (36, this + 11452), (40, this + 11588), (44, 0)], 0x8003_0FDC);
        new(c, &r, sp, INSIDE_BOX_MENU, &[this + 12256, 0x1000, 0, 0], &[(16, 0), (20, 0), (24, 0), (28, this + 11656), (32, this + 11724), (36, this + 11792), (40, this + 11860), (44, 0)], 0x8003_101C);
        r.0[8] = this + 12372;
        new(c, &r, sp, BUTTON, &[this + 12372, 0x287, 0, SAVE_GAME], &[], 0x8003_103C);
        c.m.w32(this + 12396, 0x8001_0A60);
        r.0[0] = this + 12416;
        new(c, &r, sp, BUTTON, &[this + 12416, 0x288, 0, LOAD_GAME], &[], 0x8003_1068);
        let memcard_screen = c.m.r32(MEMCARD_SCREEN);
        c.m.w32(this + 12440, 0x8001_0A60);
        new(c, &r, sp, OPTIONS_MENU, &[this + 12460, 0x1020, memcard_screen, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF), (28, 46), (32, 10), (36, this + 12416), (40, this + 12372), (44, 0), (564, this + 12460)], 0x8003_10C4);
        r.0[0] = this + 12588;
        new(c, &r, sp, USER_NAME, &[this + 12588, 505], &[], 0x8003_10D8);
        new(c, &r, sp, USER_NAME, &[this + 12728, 505], &[(568, this + 12728)], 0x8003_10F0);
        new(c, &r, sp, USER_NAME, &[this + 12868, 505], &[(572, this + 12868)], 0x8003_1108);
        let name_screen = c.m.r32(NAME_SCREEN);
        new(c, &r, sp, OPTIONS_MENU, &[this + 13008, 0x1120, name_screen, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF), (28, 32), (32, 10), (36, this + 12588), (40, 0), (576, this + 13008)], 0x8003_1154);
        let g23 = c.m.r32(0x8005_24F0);
        r.0[3] = 0x8005_0000;
        new(c, &r, sp, ITERATOR, &[this + 13136, g23 + 216, g23 + 204], &[(580, this + 13136)], 0x8003_1178);
        r.0[0] = this + 13152;
        new(c, &r, sp, NFS4_CHOICE, &[this + 13152, 95, this + 13136, 0xFFFF_FFFF], &[(16, 0)], 0x8003_1198);
        let g24 = c.m.r32(0x8005_24F0);
        c.m.w32(this + 13176, 0x8001_14D8);
        new(c, &r, sp, BOTTOM_MENU, &[this + 13192, 0x4010, g24, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF), (28, this + 13152), (32, 0)], 0x8003_11D4);
        new(c, &r, sp, NFS4_BUTTON, &[this + 13316, 91, 0, PINK_SLIPS_CONTINUE], &[(16, 0), (20, 10), (584, this + 13316)], 0x8003_1204);
        r.0[0] = this + 13360;
        new(c, &r, sp, NFS4_BUTTON, &[this + 13360, 157, 0, EXIT_PINK_SLIPS_EARLY], &[(16, 0), (20, 10)], 0x8003_1230);
        let g25 = c.m.r32(0x8005_2300);
        r.0[3] = 0x2F5;
        new(c, &r, sp, NFS4_MENU, &[this + 13404, 0x1004, g25, 0], &[(16, 0), (20, 0), (24, 0x2F5), (28, this + 13316), (32, this + 13360), (36, 0)], 0x8003_126C);
        new(c, &r, sp, NFS4_BUTTON, &[this + 13528, 91, this + 4336, 0], &[(16, 0), (20, 10), (588, this + 13528)], 0x8003_129C);
        r.0[0] = this + 13572;
        new(c, &r, sp, NFS4_BUTTON, &[this + 13572, 156, 0, EXIT_TOURNEY], &[(16, 0), (20, 10)], 0x8003_12C8);
        let standings3_screen = c.m.r32(STANDINGS3_SCREEN);
        new(c, &r, sp, NFS4_MENU, &[this + 13616, 0x1004, standings3_screen, 0], &[(16, 0), (20, 0), (24, 0x2F5), (28, this + 13528), (32, this + 13572), (36, 0)], 0x8003_1300);
        r.0[0] = this + 13740;
        new(c, &r, sp, NFS4_BUTTON, &[this + 13740, 91, this + 13908, 0], &[(16, 0), (20, 10), (592, this + 13908)], 0x8003_1330);
        let standings_screen = c.m.r32(STANDINGS_SCREEN);
        new(c, &r, sp, NFS4_MENU, &[this + 13784, 0x1004, standings_screen, 0], &[(16, 0), (20, 0), (24, 0x2F5), (28, this + 13740), (32, 0)], 0x8003_1360);
        let g26 = c.m.r32(0x8005_2DD8);
        new(c, &r, sp, BLANK_MENU, &[this + 13908, 0x8000, g26, 0], &[(16, 0), (20, TIER_FINISHED), (24, 0xFFFF_FFFF)], 0x8003_1390);
        new(c, &r, sp, OPTIONS_MENU, &[this + 14032, 0x1040, name_screen, 0], &[(16, 0), (20, FINISHED_PLAYER1_GET_NAME), (24, 0xFFFF_FFFF), (28, 32), (32, 10), (36, this + 12728), (40, 0)], 0x8003_13E0);
        new(c, &r, sp, OPTIONS_MENU, &[this + 14160, 0x1080, name_screen, 0], &[(16, 0), (20, FINISHED_PLAYER2_GET_NAME), (24, 0xFFFF_FFFF), (28, 32), (32, 10), (36, this + 12868), (40, 0)], 0x8003_1430);
        r.0[0] = this + 14288;
        new(c, &r, sp, NFS4_BUTTON, &[this + 14288, 91, 0, POST_GAME_MENU], &[(16, 0xFFFF_FFFF), (20, 0xFFFF_FFFF)], 0x8003_1460);
        let g27 = c.m.r32(0x8005_2560);
        c.m.w32(this + 14312, 0x8001_1480);
        new(c, &r, sp, OPTIONS_MENU, &[this + 14332, 0x0002_1004, g27, 0], &[(16, 0), (20, 0), (24, 213), (28, 1), (32, 10), (36, this + 14288), (40, 0)], 0x8003_14B4);
        let pink_standings = c.m.r32(PINK_STANDINGS);
        new(c, &r, sp, BLANK_MENU, &[this + 14460, 0x8000, pink_standings, 0], &[(16, 0), (20, AWARD_PINK_SLIPS_CAR), (24, 0xFFFF_FFFF)], 0x8003_14E8);
        let g28 = c.m.r32(0x8005_2DDC);
        r.0[0] = TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME;
        new(c, &r, sp, BLANK_MENU, &[this + 14584, 0x8000, g28, 0], &[(16, 0), (20, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME), (24, 0xFFFF_FFFF)], 0x8003_151C);
        let g29 = c.m.r32(0x8005_2DE0);
        new(c, &r, sp, BLANK_MENU, &[this + 14708, 0x8000, g29, 0], &[(16, 0), (20, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME), (24, 0xFFFF_FFFF)], 0x8003_1548);
        new(c, &r, sp, BLANK_MENU, &[this + 14832, 0x1010, main_screen, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF)], 0x8003_1574);
        r.0[0] = this + 14956;
        new(c, &r, sp, BUTTON, &[this + 14956, 0x28B, 0, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU], &[], 0x8003_1594);
        c.m.w32(this + 14980, 0x8001_0A60);
        new(c, &r, sp, OPTIONS_MENU, &[this + 15000, 0x1040, memcard_screen, 0], &[(16, 0), (20, 0), (24, 0xFFFF_FFFF), (28, 46), (32, 10), (36, this + 14956), (40, this + 12372), (44, 0)], 0x8003_15E8);
        c.m.w32(this + 5912, this + 6072);
        c.m.w32(this + 6368, this + 6488);
        c.m.w32(this + 6812, this + 6932);
        r.0[0] = 5;
        new(c, &r, sp, SET_DIMENSIONS, &[this + 9676, 0, 0, 120], &[(16, 5)], 0x8003_1620);
        new(c, &r, sp, SET_DIMENSIONS, &[this + 9732, 0, 0, 120], &[(16, 5)], 0x8003_163C);
        new(c, &r, sp, SET_DIMENSIONS, &[this + 9788, 0, 0, 120], &[(16, 5)], 0x8003_1658);
        new(c, &r, sp, SET_DIMENSIONS, &[this + 9844, 0, 0, 120], &[(16, 5)], 0x8003_1674);
        new(c, &r, sp, SET_DIMENSIONS, &[this + 9900, 0, 0, 120], &[(16, 5)], 0x8003_1690);
        let g30 = c.m.r32(this + 13740);
        c.m.w32(this + 6628, 32);
        c.m.w32(this + 4828, 2);
        c.m.w32(this + 7284, 1);
        c.m.w32(this + 7636, 2);
        c.m.w16(this + 10308, 0);
        c.m.w16(this + 10956, 0);
        c.m.w16(this + 11216, 1);
        c.m.w32(this + 13740, g30 | 64);
        c.m.w16(this + 12560, 0);
        c.m.w16(this + 13108, 0);
        c.m.w16(this + 4264, 1);
        c.m.w16(this + 612, 0);
        // the scratch registers as the original leaves them
        c.r[2] = this;
        c.r[3] = g30 | 64;
        c.r[4] = 2;
        c.r[8] = this + 13008;
        c.r[9] = 1;
    })
}

/// `tGlobalMenuDefs::tGlobalMenuDefs` entered as the original's callers `jal` it.
pub(crate) fn global_menu_defs_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8002_e504, None, &[]);
    abi_global_menu_defs(c);
}

/// The members destroyed, the last constructed first: (offset, destructor, return address).
const MEMBERS: [(u32, u32, u32); 249] = [
    (15000, 0x8001_C6A4, 0x8003_1744), (14956, 0x8002_539C, 0x8003_1750), (14832, 0x8001_BA5C, 0x8003_175C), (14708, 0x8001_BA5C, 0x8003_1768),
    (14584, 0x8001_BA5C, 0x8003_1774), (14460, 0x8001_BA5C, 0x8003_1780), (14332, 0x8001_C6A4, 0x8003_178C), (14288, 0x8001_A674, 0x8003_1798),
    (14160, 0x8001_C6A4, 0x8003_17A4), (14032, 0x8001_C6A4, 0x8003_17B0), (13908, 0x8001_BA5C, 0x8003_17BC), (13784, 0x8001_B2B8, 0x8003_17C8),
    (13740, 0x8001_A674, 0x8003_17D4), (13616, 0x8001_B2B8, 0x8003_17E0), (13572, 0x8001_A674, 0x8003_17EC), (13528, 0x8001_A674, 0x8003_17F8),
    (13404, 0x8001_B2B8, 0x8003_1804), (13360, 0x8001_A674, 0x8003_1810), (13316, 0x8001_A674, 0x8003_181C), (13192, 0x8001_B910, 0x8003_1828),
    (13152, 0x8001_AA00, 0x8003_1834), (13136, 0x8002_3E14, 0x8003_1840), (13008, 0x8001_C6A4, 0x8003_184C), (12868, 0x8002_4784, 0x8003_1858),
    (12728, 0x8002_4784, 0x8003_1864), (12588, 0x8002_4784, 0x8003_1870), (12460, 0x8001_C6A4, 0x8003_187C), (12416, 0x8002_539C, 0x8003_1888),
    (12372, 0x8002_539C, 0x8003_1894), (12256, 0x8001_CF64, 0x8003_18A0), (12140, 0x8001_CF64, 0x8003_18AC), (12024, 0x8001_CF64, 0x8003_18B8),
    (11908, 0x8001_CF64, 0x8003_18C4), (11860, 0x8001_F628, 0x8003_18D0), (11840, 0x8002_4690, 0x8003_18DC), (11792, 0x8001_F628, 0x8003_18E8),
    (11772, 0x8002_4690, 0x8003_18F4), (11724, 0x8001_F628, 0x8003_1900), (11704, 0x8002_4690, 0x8003_190C), (11656, 0x8001_F628, 0x8003_1918),
    (11636, 0x8002_4690, 0x8003_1924), (11588, 0x8001_F628, 0x8003_1930), (11568, 0x8002_4690, 0x8003_193C), (11520, 0x8001_F628, 0x8003_1948),
    (11500, 0x8002_4690, 0x8003_1954), (11452, 0x8001_F628, 0x8003_1960), (11432, 0x8002_4690, 0x8003_196C), (11384, 0x8001_F628, 0x8003_1978),
    (11364, 0x8002_4690, 0x8003_1984), (11324, 0x8001_F428, 0x8003_1990), (11304, 0x8002_4690, 0x8003_199C), (11264, 0x8001_F428, 0x8003_19A8),
    (11244, 0x8002_4690, 0x8003_19B4), (11116, 0x8001_C6A4, 0x8003_19C0), (11048, 0x8001_D368, 0x8003_19CC), (11004, 0x8002_4974, 0x8003_19D8),
    (10984, 0x8002_3FC4, 0x8003_19E4), (10856, 0x8001_C6A4, 0x8003_19F0), (10812, 0x8002_4974, 0x8003_19FC), (10768, 0x8002_4974, 0x8003_1A08),
    (10720, 0x8002_4974, 0x8003_1A14), (10672, 0x8002_4974, 0x8003_1A20), (10624, 0x8002_4974, 0x8003_1A2C), (10580, 0x8002_4974, 0x8003_1A38),
    (10536, 0x8002_4974, 0x8003_1A44), (10492, 0x8002_4974, 0x8003_1A50), (10472, 0x8002_3FC4, 0x8003_1A5C), (10456, 0x8002_3E14, 0x8003_1A68),
    (10436, 0x8002_3FC4, 0x8003_1A74), (10416, 0x8002_3FC4, 0x8003_1A80), (10396, 0x8002_3FC4, 0x8003_1A8C), (10376, 0x8002_3FC4, 0x8003_1A98),
    (10356, 0x8002_3FC4, 0x8003_1AA4), (10336, 0x8002_3FC4, 0x8003_1AB0), (10208, 0x8001_C6A4, 0x8003_1ABC), (10072, 0x8001_EB10, 0x8003_1AC8),
    (10000, 0x8001_D368, 0x8003_1AD4), (9956, 0x8002_4974, 0x8003_1AE0), (9900, 0x8001_E6A8, 0x8003_1AEC), (9844, 0x8001_E6A8, 0x8003_1AF8),
    (9788, 0x8001_E6A8, 0x8003_1B04), (9732, 0x8001_E6A8, 0x8003_1B10), (9676, 0x8001_E6A8, 0x8003_1B1C), (9660, 0x8002_3E14, 0x8003_1B28),
    (9644, 0x8002_45B0, 0x8003_1B34), (9628, 0x8002_45B0, 0x8003_1B40), (9612, 0x8002_45B0, 0x8003_1B4C), (9596, 0x8002_45B0, 0x8003_1B58),
    (9580, 0x8002_45B0, 0x8003_1B64), (9456, 0x8001_B2B8, 0x8003_1B70), (9412, 0x8001_A674, 0x8003_1B7C), (9368, 0x8001_A674, 0x8003_1B88),
    (9324, 0x8001_A674, 0x8003_1B94), (9280, 0x8001_A674, 0x8003_1BA0), (9236, 0x8001_A674, 0x8003_1BAC), (9192, 0x8001_A674, 0x8003_1BB8),
    (9068, 0x8001_BA5C, 0x8003_1BC4), (8936, 0x8001_BC24, 0x8003_1BD0), (8804, 0x8001_BC24, 0x8003_1BDC), (8672, 0x8001_BC24, 0x8003_1BE8),
    (8540, 0x8001_BC24, 0x8003_1BF4), (8408, 0x8001_BC24, 0x8003_1C00), (8376, 0x8002_4974, 0x8003_1C0C), (8344, 0x8002_4974, 0x8003_1C18),
    (8312, 0x8002_4974, 0x8003_1C24), (8280, 0x8002_4974, 0x8003_1C30), (8248, 0x8002_4974, 0x8003_1C3C), (8216, 0x8002_4974, 0x8003_1C48),
    (8184, 0x8002_4974, 0x8003_1C54), (8168, 0x8002_3E14, 0x8003_1C60), (8152, 0x8002_3E14, 0x8003_1C6C), (8132, 0x8002_3FC4, 0x8003_1C78),
    (8112, 0x8002_3FC4, 0x8003_1C84), (7988, 0x8001_B2B8, 0x8003_1C90), (7944, 0x8001_A674, 0x8003_1C9C), (7900, 0x8001_A674, 0x8003_1CA8),
    (7856, 0x8001_A674, 0x8003_1CB4), (7732, 0x8001_B2B8, 0x8003_1CC0), (7688, 0x8001_A674, 0x8003_1CCC), (7648, 0x8001_AA00, 0x8003_1CD8),
    (7620, 0x8001_7A70, 0x8003_1CE4), (7496, 0x8001_B2B8, 0x8003_1CF0), (7452, 0x8001_A674, 0x8003_1CFC), (7408, 0x8001_A674, 0x8003_1D08),
    (7368, 0x8001_AA00, 0x8003_1D14), (7328, 0x8001_AA00, 0x8003_1D20), (7296, 0x8001_8660, 0x8003_1D2C), (7268, 0x8001_7A70, 0x8003_1D38),
    (7144, 0x8001_B2B8, 0x8003_1D44), (7100, 0x8001_A674, 0x8003_1D50), (7056, 0x8001_A674, 0x8003_1D5C), (6932, 0x8001_B808, 0x8003_1D68),
    (6892, 0x8001_AA00, 0x8003_1D74), (6848, 0x8001_A674, 0x8003_1D80), (6724, 0x8001_B808, 0x8003_1D8C), (6684, 0x8001_AA00, 0x8003_1D98),
    (6640, 0x8001_A674, 0x8003_1DA4), (6612, 0x8001_7A70, 0x8003_1DB0), (6488, 0x8001_B808, 0x8003_1DBC), (6448, 0x8001_AA00, 0x8003_1DC8),
    (6404, 0x8001_A674, 0x8003_1DD4), (6280, 0x8001_B808, 0x8003_1DE0), (6240, 0x8001_AA00, 0x8003_1DEC), (6196, 0x8001_A674, 0x8003_1DF8),
    (6072, 0x8001_B808, 0x8003_1E04), (6032, 0x8001_AA00, 0x8003_1E10), (5992, 0x8001_AA00, 0x8003_1E1C), (5948, 0x8001_A674, 0x8003_1E28),
    (5824, 0x8001_B808, 0x8003_1E34), (5784, 0x8001_AA00, 0x8003_1E40), (5744, 0x8001_AA00, 0x8003_1E4C), (5700, 0x8001_A674, 0x8003_1E58),
    (5576, 0x8001_B2B8, 0x8003_1E64), (5452, 0x8001_B2B8, 0x8003_1E70), (5412, 0x8001_AA00, 0x8003_1E7C), (5368, 0x8001_A674, 0x8003_1E88),
    (5328, 0x8001_AA00, 0x8003_1E94), (5288, 0x8001_AA00, 0x8003_1EA0), (5244, 0x8001_A674, 0x8003_1EAC), (5216, 0x8001_7A70, 0x8003_1EB8),
    (5092, 0x8001_B2B8, 0x8003_1EC4), (4968, 0x8001_B2B8, 0x8003_1ED0), (4924, 0x8001_A674, 0x8003_1EDC), (4880, 0x8001_A674, 0x8003_1EE8),
    (4840, 0x8001_AA00, 0x8003_1EF4), (4812, 0x8001_7A70, 0x8003_1F00), (4688, 0x8001_B2B8, 0x8003_1F0C), (4644, 0x8001_A674, 0x8003_1F18),
    (4604, 0x8001_AA00, 0x8003_1F24), (4564, 0x8001_AA00, 0x8003_1F30), (4520, 0x8001_A674, 0x8003_1F3C), (4488, 0x8001_8660, 0x8003_1F48),
    (4460, 0x8001_7A70, 0x8003_1F54), (4336, 0x8001_B2B8, 0x8003_1F60), (4292, 0x8001_A674, 0x8003_1F6C), (4164, 0x8001_C6A4, 0x8003_1F78),
    (4120, 0x8001_A674, 0x8003_1F84), (3988, 0x8001_BC24, 0x8003_1F90), (3952, 0x8002_4974, 0x8003_1F9C), (3916, 0x8002_4974, 0x8003_1FA8),
    (3880, 0x8002_4974, 0x8003_1FB4), (3844, 0x8002_4974, 0x8003_1FC0), (3808, 0x8002_4974, 0x8003_1FCC), (3776, 0x8002_4974, 0x8003_1FD8),
    (3744, 0x8002_4974, 0x8003_1FE4), (3728, 0x8002_3E14, 0x8003_1FF0), (3708, 0x8002_3FC4, 0x8003_1FFC), (3688, 0x8002_3FC4, 0x8003_2008),
    (3668, 0x8002_3FC4, 0x8003_2014), (3648, 0x8002_3FC4, 0x8003_2020), (3628, 0x8002_3FC4, 0x8003_202C), (3608, 0x8002_3FC4, 0x8003_2038),
    (3484, 0x8001_B2B8, 0x8003_2044), (3360, 0x8001_B2B8, 0x8003_2050), (3316, 0x8001_A674, 0x8003_205C), (3276, 0x8001_AA00, 0x8003_2068),
    (3232, 0x8001_A674, 0x8003_2074), (3208, 0x8002_1F0C, 0x8003_2080), (3084, 0x8001_B2B8, 0x8003_208C), (2960, 0x8001_B2B8, 0x8003_2098),
    (2920, 0x8001_AA00, 0x8003_20A4), (2880, 0x8001_AA00, 0x8003_20B0), (2840, 0x8001_AA00, 0x8003_20BC), (2800, 0x8001_AA00, 0x8003_20C8),
    (2760, 0x8001_AA00, 0x8003_20D4), (2716, 0x8001_A674, 0x8003_20E0), (2592, 0x8001_B2B8, 0x8003_20EC), (2548, 0x8001_A674, 0x8003_20F8),
    (2504, 0x8001_A674, 0x8003_2104), (2460, 0x8001_A674, 0x8003_2110), (2336, 0x8001_B2B8, 0x8003_211C), (2292, 0x8001_A674, 0x8003_2128),
    (2248, 0x8001_A674, 0x8003_2134), (2204, 0x8001_A674, 0x8003_2140), (2160, 0x8001_A674, 0x8003_214C), (2036, 0x8001_B2B8, 0x8003_2158),
    (1996, 0x8001_AA00, 0x8003_2164), (1952, 0x8001_A674, 0x8003_2170), (1932, 0x8003_3F28, 0x8003_217C), (1808, 0x8001_B2B8, 0x8003_2188),
    (1768, 0x8001_AA00, 0x8003_2194), (1724, 0x8001_A674, 0x8003_21A0), (1704, 0x8003_3F28, 0x8003_21AC), (1580, 0x8001_B2B8, 0x8003_21B8),
    (1536, 0x8001_A674, 0x8003_21C4), (1492, 0x8001_A674, 0x8003_21D0), (1368, 0x8001_B2B8, 0x8003_21DC), (1324, 0x8001_A674, 0x8003_21E8),
    (1280, 0x8001_A674, 0x8003_21F4), (1236, 0x8001_A674, 0x8003_2200), (1112, 0x8001_B2B8, 0x8003_220C), (1068, 0x8001_A674, 0x8003_2218),
    (1024, 0x8001_A674, 0x8003_2224), (980, 0x8001_A674, 0x8003_2230), (936, 0x8001_A674, 0x8003_223C), (892, 0x8001_A674, 0x8003_2248),
    (768, 0x8001_B2B8, 0x8003_2254), (724, 0x8001_A674, 0x8003_2260), (680, 0x8001_A674, 0x8003_226C), (636, 0x8001_A674, 0x8003_2278),
    (512, 0x8001_BA5C, 0x8003_2284), (388, 0x8001_B2B8, 0x8003_2290), (344, 0x8001_A674, 0x8003_229C), (300, 0x8001_A674, 0x8003_22A8),
    (176, 0x8001_B2B8, 0x8003_22B4), (132, 0x8001_A674, 0x8003_22C0), (88, 0x8001_A674, 0x8003_22CC), (44, 0x8001_A674, 0x8003_22D8),
    (0, 0x8001_A674, 0x8003_22E4),
];

/// `tGlobalMenuDefs::~tGlobalMenuDefs` (FEMENUDEFS.CPP, 0x80031720): every member destroyed
/// (each told 2: not to free itself), the last constructed first; freed if `flags` & 1.
pub fn abi_global_menu_defs_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (flags, this);
        for &(at, dtor, ra) in &MEMBERS {
            r.call_ptr_leaving(c, dtor, &[this + at, 2], ra);
        }
        r.0[0] = flags & 1;
        if r.0[0] != 0 {
            r.call_leaving(c, nfs4rc::f_800a4910, &[this], 0x8003_22f8);
        }
    })
}

/// `tGlobalMenuDefs::~tGlobalMenuDefs` entered as the original's callers `jal` it.
pub(crate) fn global_menu_defs_destructor_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_1720, None, &[]);
    abi_global_menu_defs_destructor(c);
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
            ("MenuExtended_SetOnePlayer", 0x8002_c160, nfs4rc::f_8002c160, |_, _| vec![CMD]),
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
            // (rebuilt in place over the front end's own, and destroyed, freed or not)
            ("tGlobalMenuDefs", 0x8002_e504, nfs4rc::f_8002e504, |c, _| vec![menus(&mut c.m)]),
            ("~tGlobalMenuDefs", 0x8003_1720, nfs4rc::f_80031720, |c, r| vec![menus(&mut c.m), 2 + pick(r, 2)]),
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
