//! FEMENUDEFS.CPP: the front end's menus' callbacks (`MenuExtended_*`): the race kinds and
//! modes chosen (one or two players, test drive, single race, tournament, special event, hot
//! pursuit, duel, full grid, pink slips, best of one, three or five), the screens gone to (the
//! car select, the dealer, the seller, the upgrades, the showroom, the garage, the track infos,
//! the trophy rooms), buying and selling cars and upgrades, saving and loading the game, the
//! post-race menus, the players' names; each sets its `tMenuCommand` (+0 the command, +4 the
//! menu). Then the menus themselves (`tGlobalMenuDefs`), built and destroyed.

use crate::call::{call, call_addr, with_buffer};
use crate::globals::FRONT_END as FE;
use crate::types::{CarInfo, CarManager, DialogInteractive, DialogYesNo, Screen};
use crate::{fecars, fememcard};
use ps1rt::{Cpu, Mem};

/// The menus, the main screen, the car select, the car manager, the second pad's state.
const ALL_MENUS: u32 = 0x8005_1EE0;
const MAIN_SCREEN: u32 = 0x8005_22E8;
const CAR_SELECT: u32 = 0x8005_24CC;
const CAR_MANAGER: CarManager = CarManager(0x8011_552C);
const PAD2_STATE: u32 = 0x8013_FBC8;
/// The yes-or-no dialog's vtable (a local one's), the loading one's and the player it asks for.
const YES_NO_VT: u32 = 0x8001_02A8;
const LOAD_VT: u32 = 0x8001_0308;
const LOAD_PLAYER: u32 = 0x8005_1EF0;
/// A yes-or-no dialog (on the stack): +96 its vtable, +136 the answer chosen, +144 its text,
/// +160 and +164 the answers' words.
const YES_NO_SIZE: u32 = 168;
const DIALOG_VPTR: u32 = 96;
const DIALOG_TEXT: u32 = 144;
/// The words: yes, no.
const YES: u32 = 802;
const NO: u32 = 803;

const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const PLAY_SFX: fn(&mut Cpu) = nfs4rc::f_8007a08c;
/// The till's sound, the buzz.
const SFX_TILL: u32 = 26;
const SFX_BUZZ: u32 = 10;
/// `AskTheUserToSaveTheGame`: hooked (the host autosaves in its place), so called by address.
const ASK_TO_SAVE: u32 = 0x8002_C49C;
/// `tDialog::Display`, `tDialog::Hide`; the application's frame (all hooked: by address).
const DIALOG_DISPLAY: u32 = 0x8001_8980;
const DIALOG_HIDE: u32 = 0x8001_8A44;
const REDRAW: u32 = 0x8001_3894;

/// `frontEnd`'s: the players (1 two), the race's kind (1 single race, 2 a tournament, 6 pink
/// slips), the difficulty, the pink slips' winner and wins and races, the slot, the stock car,
/// the tournaments chosen, the special event flag, the garage car, the pink slips' cars, the
/// second's car, the garage, the cop car won, the opponents.
const PLAYERS: u32 = 3;
const RACE_KIND: u32 = 4;
const DIFFICULTY: u32 = 6;
const PINK_WINNERS: u32 = 12;
const SLOT: u32 = 70;
const RACES: u32 = 71;
const WINS: u32 = 72;
const PINK_WINNER: u32 = 90;
const STOCK_CAR: u32 = 92;
const COLOURS: u32 = 96;
const LIVERIES: u32 = 192;
const SPECIAL: u32 = 290;
const GARAGE_CAR: u32 = 291;
const PINK_CARS: u32 = 293;
const SECOND_CAR: u32 = 296;
const OPTIONS_ON: u32 = 298;
const COP_WON: u32 = 301;
const OPPONENTS: u32 = 798;
const SINGLE_RACE: u8 = 1;
const TOURNAMENT: u8 = 2;
const PINK_SLIPS: u8 = 6;

/// The commands: none, a menu, a transition, two players' menu, the race, two players' race.
const CMD_NONE: u32 = 0;
const CMD_MENU: u32 = 1;
const CMD_TRANSITION: u32 = 2;
const CMD_TWO_PLAYER_MENU: u32 = 3;
const CMD_RACE: u32 = 5;
const CMD_TWO_PLAYER_RACE: u32 = 6;

/// The menus' offsets in `tGlobalMenuDefs` that the callbacks go to.
const MAIN_MENU: u32 = 176;
const SAVE_MENU: u32 = 15000;

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

fn menus(m: &mut Mem) -> u32 {
    m.r32(ALL_MENUS)
}

fn app(m: &mut Mem) -> u32 {
    m.r32(crate::globals::FE_APP)
}

/// The command: `kind` to the menus' `menu` (an offset).
fn command(m: &mut Mem, cmd: u32, kind: u32, menu: u32) {
    let all = menus(m);
    m.w32(cmd, kind);
    m.w32(cmd + 4, all + menu);
}

/// The main screen's background swapped for the menu's (`SwapBackground(-1)`).
fn swap_background(c: &mut Cpu) {
    let main = c.m.r32(MAIN_SCREEN);
    crate::screenmain::swap_background(c, main, u32::MAX);
}

/// The race settings `[(offset, value)]` in the front end's structure.
fn set(m: &mut Mem, bytes: &[(u32, u8)]) {
    for &(at, v) in bytes {
        m.w8(FE + at, v);
    }
}

/// The owned cars of player 0 (a `short`).
fn owned_cars(m: &mut Mem) -> i16 {
    fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as i16
}

/// The application's message dialog (+44) showing `word`.
fn message(c: &mut Cpu, word: u32) {
    let d = app(&mut c.m) + 44;
    let t = call(c, TEXT_WORD, &[word]);
    c.m.w32(d + DIALOG_TEXT, t);
    call_addr(c, DIALOG_DISPLAY, &[d]);
}

/// A yes-or-no dialog asking `word` (answers `yes` and `no`, none chosen) on the stack, `setup`
/// first (its vtable), run; `then` with its answer (a `short`) before it's destroyed.
fn ask<R>(c: &mut Cpu, word: u32, (yes, no): (u32, u32), setup: impl FnOnce(&mut Mem, u32), then: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    with_buffer(c, YES_NO_SIZE, |c, d| {
        crate::fedialog::yes_no(c, DialogYesNo(d));
        setup(&mut c.m, d);
        let t = call(c, TEXT_WORD, &[word]);
        let m = &mut c.m;
        m.w32(d + DIALOG_TEXT, t);
        m.w32(d + 160, yes);
        m.w32(d + 164, no);
        m.w16(d + 136, 0);
        let answer = sx(crate::fedialog::run(c, DialogInteractive(d)));
        let r = then(c, answer);
        crate::fescreen::screen_destructor(c, Screen(d), 2);
        r
    })
}

/// No set-up for a dialog.
fn plain(_: &mut Mem, _: u32) {}

/// The local yes-or-no's vtable.
fn local_vt(m: &mut Mem, d: u32) {
    m.w32(d + DIALOG_VPTR, YES_NO_VT);
}

/// `MenuExtended_SetOnePlayer` (FEMENUDEFS.CPP, 0x8002c160, eight bytes before its symbol): one
/// player (the menu's background), but not for the second player (nothing).
pub fn set_one_player(c: &mut Cpu, cmd: u32) {
    let a = app(&mut c.m);
    if c.m.r8(a.wrapping_add(557)) == 1 {
        c.m.w32(cmd, CMD_NONE);
        return;
    }
    c.m.w8(FE + PLAYERS, 0);
    swap_background(c);
}

pub fn abi_set_one_player(c: &mut Cpu) {
    set_one_player(c, c.r[4]);
}

/// `MenuExtended_SetTwoPlayer` (FEMENUDEFS.CPP, 0x8002c1ac): two players with the second pad
/// in (no duel), else nothing.
pub fn set_two_player(c: &mut Cpu, cmd: u32) {
    if c.m.r8(PAD2_STATE) != 0 {
        c.m.w32(cmd, CMD_NONE);
        return;
    }
    set(&mut c.m, &[(PLAYERS, 1), (OPPONENTS, 0)]);
    swap_background(c);
}

pub fn abi_set_two_player(c: &mut Cpu) {
    set_two_player(c, c.r[4]);
}

/// `MenuExtended_SetTestDrive` (FEMENUDEFS.CPP, 0x8002c1fc).
pub fn abi_set_test_drive(c: &mut Cpu) {
    set(&mut c.m, &[(OPTIONS_ON, 0), (OPPONENTS, 0), (SLOT, 0), (RACE_KIND, 0)]);
}

/// `MenuExtended_SetSingleRace` (FEMENUDEFS.CPP, 0x8002c218).
pub fn abi_set_single_race(c: &mut Cpu) {
    set(&mut c.m, &[(OPTIONS_ON, 1), (SLOT, 0), (RACE_KIND, 0)]);
    swap_background(c);
}

/// `MenuExtended_SetTournament` (FEMENUDEFS.CPP, 0x8002c258).
pub fn abi_set_tournament(c: &mut Cpu) {
    set(&mut c.m, &[(OPTIONS_ON, 1), (SLOT, 0), (SPECIAL, 0), (RACE_KIND, TOURNAMENT)]);
    swap_background(c);
}

/// `MenuExtended_SetSpecialEvent` (FEMENUDEFS.CPP, 0x8002c2a0).
pub fn abi_set_special_event(c: &mut Cpu) {
    set(&mut c.m, &[(OPTIONS_ON, 1), (SPECIAL, 1), (SLOT, 0), (RACE_KIND, TOURNAMENT)]);
    swap_background(c);
}

/// `MenuExtended_SetSoloRace` (FEMENUDEFS.CPP, 0x8002c2e8).
pub fn set_solo_race(m: &mut Mem) {
    set(m, &[(SLOT, 0), (RACE_KIND, 0), (OPPONENTS, 0), (OPTIONS_ON, 1)]);
}

pub fn abi_set_solo_race(c: &mut Cpu) {
    set_solo_race(&mut c.m);
}

/// A car list's iterator (the menus' `list`) stepped back and on for `player`, its value
/// refreshed.
fn refresh(c: &mut Cpu, list: u32, player: u32) {
    let at = menus(&mut c.m) + list;
    fecars::list_iterator_car_decrement(c, at, player);
    let at = menus(&mut c.m) + list;
    fecars::list_iterator_car_increment(c, at, player);
}

/// The car select at `screen`'s global set to `state`.
fn set_state(c: &mut Cpu, screen: u32, state: u32) {
    let s = c.m.r32(screen);
    crate::screencarselect::set_state(c, s, state);
}

/// The menus' car lists: the stock cars, the garage's, the opponent's, the dealer's, the
/// seller's, the pink slips'.
const STOCK_LIST: u32 = 4460;
const GARAGE_LIST: u32 = 4812;
const OPPONENT_LIST: u32 = 5216;
const DEALER_LIST: u32 = 7268;
const SELLER_LIST: u32 = 7620;
/// A car list's +16, its kind.
const LIST_KIND: u32 = 16;

/// `MenuExtended_GoToTwoPlayerSingleRace` (FEMENUDEFS.CPP, 0x8002c308): with cars in the
/// garage, a solo race's menu; else asked whether to test drive instead: yes, the dealer's
/// cars (its list refreshed) for a test drive.
pub fn go_to_two_player_single_race(c: &mut Cpu, cmd: u32) {
    with_buffer(c, YES_NO_SIZE, |c, d| {
        crate::fedialog::yes_no(c, DialogYesNo(d));
        local_vt(&mut c.m, d);
        if owned_cars(&mut c.m) > 0 {
            set_solo_race(&mut c.m);
            command(&mut c.m, cmd, CMD_MENU, 3360);
        } else {
            let t = call(c, TEXT_WORD, &[66]);
            let m = &mut c.m;
            m.w32(d + DIALOG_TEXT, t);
            m.w32(d + 160, YES);
            m.w32(d + 164, NO);
            m.w16(d + 136, 0);
            if sx(crate::fedialog::run(c, DialogInteractive(d))) == 1 {
                let m = &mut c.m;
                m.w8(FE + RACE_KIND, 0);
                m.w32(cmd, CMD_MENU);
                refresh(c, DEALER_LIST, u32::MAX);
                let all = menus(&mut c.m);
                c.m.w32(cmd + 4, all + 7496);
                set_state(c, CAR_SELECT, 2);
            }
        }
        crate::fescreen::screen_destructor(c, Screen(d), 2);
    });
}

pub fn abi_go_to_two_player_single_race(c: &mut Cpu) {
    go_to_two_player_single_race(c, c.r[4]);
}

/// `MenuExtended_SetDuelRace` (FEMENUDEFS.CPP, 0x8002c41c).
pub fn abi_set_duel_race(c: &mut Cpu) {
    set(&mut c.m, &[(SLOT, 0), (RACE_KIND, 0), (OPPONENTS, 1)]);
    swap_background(c);
}

/// `MenuExtended_SetFullGrid` (FEMENUDEFS.CPP, 0x8002c45c).
pub fn abi_set_full_grid(c: &mut Cpu) {
    set(&mut c.m, &[(SLOT, 0), (RACE_KIND, 0), (OPPONENTS, 2)]);
    swap_background(c);
}

/// `AskTheUserToSaveTheGame` (FEMENUDEFS.CPP, 0x8002c49c): asked whether to save (never a
/// cheater); the answer.
pub fn ask_the_user_to_save_the_game(c: &mut Cpu) -> u32 {
    if crate::fecheats::is_the_user_a_cry_baby_cheater(&mut c.m) {
        return 0;
    }
    ask(c, 818, (YES, NO), plain, |_, answer| answer)
}

pub fn abi_ask_the_user_to_save_the_game(c: &mut Cpu) {
    c.r[2] = ask_the_user_to_save_the_game(c);
}

/// `MenuExtended_TransitionFromPostGameToMainMenu` (FEMENUDEFS.CPP, 0x8002c514).
pub fn transition_from_post_game_to_main_menu(m: &mut Mem, cmd: u32) {
    command(m, cmd, CMD_TRANSITION, MAIN_MENU);
}

pub fn abi_transition_from_post_game_to_main_menu(c: &mut Cpu) {
    transition_from_post_game_to_main_menu(&mut c.m, c.r[4]);
}

/// `MenuExtended_TransitionFromPostGameToMainMenuAndSaveGame` (FEMENUDEFS.CPP, 0x8002c530):
/// asked to save: the save menu, else the main menu.
pub fn transition_from_post_game_to_main_menu_and_save_game(c: &mut Cpu, cmd: u32) {
    let menu = if call_addr(c, ASK_TO_SAVE, &[]) != 0 { SAVE_MENU } else { MAIN_MENU };
    command(&mut c.m, cmd, CMD_TRANSITION, menu);
}

pub fn abi_transition_from_post_game_to_main_menu_and_save_game(c: &mut Cpu) {
    transition_from_post_game_to_main_menu_and_save_game(c, c.r[4]);
}

/// The track manager, the duel's car select, the memory card screen (+1412 its message, +1436
/// its player), the card layer's flag.
const TRACK_MANAGER: u32 = 0x8011_5D08;
const DUEL_SELECT: u32 = 0x8005_24D0;
const MEMCARD_SCREEN: u32 = 0x8005_2D80;
const INITED: u32 = 0x8005_1F08;
const CARD_MESSAGE: u32 = 1412;
const CARD_PLAYER: u32 = 1436;

/// `MenuExtended_GoToCarSelect` (FEMENUDEFS.CPP, 0x8002c57c): (the track available, else a
/// message) the car select for the race: two players' (pink slips', stock or garage cars),
/// the duel's, or one player's (stock or garage); the car lists' kinds set for the race kind;
/// the lists refreshed (both players' stock lists, or the garage's for those with cars), the
/// duel's opponents' too.
pub fn go_to_car_select(c: &mut Cpu, cmd: u32) {
    let available = with_buffer(c, 48, |c, track| {
        let m = &mut c.m;
        let slot = m.r8(FE + SLOT) as u32;
        let k = m.r8(FE + slot + 35) as u32;
        crate::fetracks::get_track(c, TRACK_MANAGER, k, track);
        c.m.r8(track + 3) != 0
    });
    if !available {
        message(c, 236);
        return;
    }
    let m = &mut c.m;
    let kind = m.r8(FE + RACE_KIND);
    let garage = m.r8(FE + OPTIONS_ON) != 0;
    if m.r8(FE + PLAYERS) == 1 {
        let menu = if kind == PINK_SLIPS {
            6724
        } else if !garage {
            5824
        } else {
            6280
        };
        command(m, cmd, CMD_TWO_PLAYER_MENU, menu);
    } else if m.r8(FE + OPPONENTS) == 1 {
        command(m, cmd, CMD_MENU, if kind == SINGLE_RACE { 5576 } else { 5452 });
        set_state(c, DUEL_SELECT, 7);
    } else {
        let (state, menu) = if !garage { (0, 4688) } else { (1, 4968) };
        command(m, cmd, CMD_MENU, menu);
        set_state(c, CAR_SELECT, state);
    }
    // the lists' kinds: the stock cars' (9: with hot pursuit's), the garage's (64: a
    // tournament's), the pink slips'
    let m = &mut c.m;
    let all = menus(m);
    match kind {
        SINGLE_RACE => {
            let v = if m.r8(FE + OPPONENTS) == 0 || m.r8(FE + PLAYERS) == 1 { 9 } else { 1 };
            m.w32(all + STOCK_LIST + LIST_KIND, v);
        }
        PINK_SLIPS => {
            m.w32(all + 6612 + LIST_KIND, 32);
            return;
        }
        _ => {
            if !garage {
                m.w32(all + STOCK_LIST + LIST_KIND, 1);
            } else {
                m.w32(all + GARAGE_LIST + LIST_KIND, if kind == TOURNAMENT { 64 } else { 2 });
            }
        }
    }
    if !garage {
        refresh(c, STOCK_LIST, 0);
        refresh(c, STOCK_LIST, 1);
    } else if owned_cars(&mut c.m) > 0 || (c.m.r8(FE + RACE_KIND) == SINGLE_RACE && c.m.r8(FE + OPPONENTS) != 1) {
        refresh(c, GARAGE_LIST, 0);
        if c.m.r8(FE + PLAYERS) == 1 {
            refresh(c, GARAGE_LIST, 1);
        }
    }
    if c.m.r8(FE + OPPONENTS) == 1 && c.m.r8(FE + RACE_KIND) != TOURNAMENT {
        refresh(c, OPPONENT_LIST, u32::MAX);
        let all = menus(&mut c.m);
        c.m.w32(all + OPPONENT_LIST + LIST_KIND, 1);
    }
}

pub fn abi_go_to_car_select(c: &mut Cpu) {
    go_to_car_select(c, c.r[4]);
}

/// A car select state's menu: the command, the state, its car list refreshed (for any player).
fn go_to_list(c: &mut Cpu, cmd: u32, menu: u32, state: u32, list: u32) {
    command(&mut c.m, cmd, CMD_MENU, menu);
    set_state(c, CAR_SELECT, state);
    refresh(c, list, u32::MAX);
}

/// `MenuExtended_GoToDealer` (FEMENUDEFS.CPP, 0x8002c910): the dealer's cars.
pub fn abi_go_to_dealer(c: &mut Cpu) {
    go_to_list(c, c.r[4], 7496, 2, DEALER_LIST);
}

/// `MenuExtended_GoToSeller` (FEMENUDEFS.CPP, 0x8002c978): the garage's cars to sell.
pub fn abi_go_to_seller(c: &mut Cpu) {
    go_to_list(c, c.r[4], 7732, 3, SELLER_LIST);
}

/// `MenuExtended_GoToUpgrades` (FEMENUDEFS.CPP, 0x8002c9e0).
pub fn abi_go_to_upgrades(c: &mut Cpu) {
    command(&mut c.m, c.r[4], CMD_MENU, 7988);
    set_state(c, CAR_SELECT, 4);
}

/// Frames drawn until the application's card dialog (+720) is in (its +852, the shown flag,
/// 1), then one more.
fn draw_until_shown(c: &mut Cpu) {
    loop {
        let a = app(&mut c.m);
        let shown = c.m.r32(a + 852) == 1;
        call_addr(c, REDRAW, &[a]);
        if shown {
            return;
        }
        c.tick();
    }
}

/// The application's card dialog (+720) showing `text`.
fn card_dialog(c: &mut Cpu, text: u32) {
    let d = app(&mut c.m) + 720;
    c.m.w32(d + DIALOG_TEXT, text);
    call_addr(c, DIALOG_DISPLAY, &[d]);
}

fn hide_card_dialog(c: &mut Cpu) {
    let d = app(&mut c.m) + 720;
    call_addr(c, DIALOG_HIDE, &[d]);
}

/// The memory card screen's message (-1 none).
fn card_message(m: &mut Mem, word: u32) {
    let screen = m.r32(MEMCARD_SCREEN);
    m.w32(screen + CARD_MESSAGE, word);
}

/// `GenericMenuSaveGame` (FEMENUDEFS.CPP, 0x8002ca20): the memory card screen's message
/// "saving", a frame drawn; (unless the card layer's up and no dialog's wanted) the saving
/// dialog shown and drawn until it's in, the card layer started if down; the game saved
/// (`SaveGame`); the layer stopped if started here, the dialog hidden; the save's result.
pub fn generic_menu_save_game(c: &mut Cpu, dialog_wanted: bool) -> bool {
    card_message(&mut c.m, 639);
    let a = app(&mut c.m);
    call_addr(c, REDRAW, &[a]);
    let mut started = false;
    if c.m.r32(INITED) == 0 || dialog_wanted {
        let t = call(c, TEXT_WORD, &[643]);
        card_dialog(c, t);
        draw_until_shown(c);
        if c.m.r32(INITED) == 0 {
            started = true;
            fememcard::init_memcard(c, true, false);
        }
    }
    let saved = fememcard::save_game(c, 0);
    card_message(&mut c.m, u32::MAX);
    if started {
        fememcard::deinit_memcard(c);
    }
    hide_card_dialog(c);
    saved
}

pub fn abi_generic_menu_save_game(c: &mut Cpu) {
    c.r[2] = generic_menu_save_game(c, c.r[4] != 0) as u32;
}

/// `PinkSlipsPreSave` (FEMENUDEFS.CPP, 0x8002cb3c): (a cheater: on) asked whether to save
/// before the pink slips: yes, saved (its result); cancelled: no; no: on.
pub fn pink_slips_pre_save(c: &mut Cpu) -> bool {
    if crate::fecheats::is_the_user_a_cry_baby_cheater(&mut c.m) {
        return true;
    }
    ask(c, 628, (YES, NO), local_vt, |c, answer| match answer {
        1 => generic_menu_save_game(c, true),
        u32::MAX => false,
        _ => true,
    })
}

pub fn abi_pink_slips_pre_save(c: &mut Cpu) {
    c.r[2] = pink_slips_pre_save(c) as u32;
}

/// The tournament manager (+20 the money, +12 the tier, +24 the definition); the name entry's
/// names and screen.
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const NAMES: u32 = 0x8011_5C1C;
const NAME_SCREEN: u32 = 0x8005_2E44;

/// `MenuExtended_GoToBestOf*`: (the pink slips' save asked first) the pink slips' menu of
/// `menu`, or nothing; the races' count.
fn go_to_best_of(c: &mut Cpu, cmd: u32, menu: u32, races: u8) {
    if pink_slips_pre_save(c) {
        command(&mut c.m, cmd, CMD_MENU, menu);
    } else {
        c.m.w32(cmd, CMD_NONE);
    }
    set(&mut c.m, &[(SLOT, 0), (RACES, races)]);
}

/// `MenuExtended_GoToBestOfOne` (FEMENUDEFS.CPP, 0x8002cc04).
pub fn abi_go_to_best_of_one(c: &mut Cpu) {
    go_to_best_of(c, c.r[4], 3360, 1);
}

/// `MenuExtended_GoToBestOfThree` (FEMENUDEFS.CPP, 0x8002cc60).
pub fn abi_go_to_best_of_three(c: &mut Cpu) {
    go_to_best_of(c, c.r[4], 2960, 3);
}

/// `MenuExtended_GoToBestOfFive` (FEMENUDEFS.CPP, 0x8002ccbc).
pub fn abi_go_to_best_of_five(c: &mut Cpu) {
    go_to_best_of(c, c.r[4], 3084, 5);
}

/// Stock car `id`'s description: whether it's available, and whether it's a police car
/// (`fPursuitAvailable`).
fn stock_car_flags(c: &mut Cpu, id: u32) -> (bool, bool) {
    with_buffer(c, CarInfo::SIZE, |c, car| {
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        let m = &mut c.m;
        let info = CarInfo(car);
        (info.available(m) != 0, info.pursuit_available(m) != 0)
    })
}

/// The race refused with `word`'s message (no command).
fn refuse(c: &mut Cpu, cmd: u32, word: u32) {
    message(c, word);
    c.m.w32(cmd, CMD_NONE);
}

/// `MenuExtended_GoToRace` (FEMENUDEFS.CPP, 0x8002cd18): the race, unless: no garage car for a
/// garage race (but hot pursuit's and pink slips'), no car for the tournament, the chosen car
/// no police car for hot pursuit, the chosen stock car unavailable: its message instead.
pub fn go_to_race(c: &mut Cpu, cmd: u32) {
    c.m.w32(cmd, CMD_RACE);
    let m = &mut c.m;
    let kind = m.r8(FE + RACE_KIND);
    let word = if m.r8(FE + OPTIONS_ON) == 1 && owned_cars(m) <= 0 && kind != SINGLE_RACE && kind != PINK_SLIPS {
        Some(171)
    } else if kind == TOURNAMENT && fecars::get_num_tourney_cars(c, CAR_MANAGER, 0) as i16 <= 0 {
        Some(242)
    } else {
        let id = c.m.r8(FE + STOCK_CAR) as u32;
        if kind == SINGLE_RACE && !stock_car_flags(c, id).1 {
            Some(243)
        } else if c.m.r8(FE + OPTIONS_ON) == 0 && !stock_car_flags(c, id).0 {
            Some(244)
        } else {
            None
        }
    };
    if let Some(word) = word {
        refuse(c, cmd, word);
    }
}

pub fn abi_go_to_race(c: &mut Cpu) {
    go_to_race(c, c.r[4]);
}

/// `MenuExtended_GoTo2PlayerRace` (FEMENUDEFS.CPP, 0x8002ce74): the two players' race (the pink
/// slips' as it is), unless the player has no garage car for a garage race (but hot pursuit's),
/// their stock car's unavailable, or not a police car for hot pursuit: its message.
pub fn go_to_2_player_race(c: &mut Cpu, cmd: u32) {
    c.m.w32(cmd, CMD_TWO_PLAYER_RACE);
    let m = &mut c.m;
    let kind = m.r8(FE + RACE_KIND);
    if kind == PINK_SLIPS {
        return;
    }
    let a = app(m);
    let p = m.r8(a + 556) as u32;
    let id = m.r8(FE + p + STOCK_CAR) as u32;
    let word = if m.r8(FE + OPTIONS_ON) == 1 && owned_cars(m) <= 0 && kind != SINGLE_RACE {
        Some(171)
    } else if c.m.r8(FE + OPTIONS_ON) == 0 && !stock_car_flags(c, id).0 {
        Some(244)
    } else if kind == SINGLE_RACE && !stock_car_flags(c, id).1 {
        Some(243)
    } else {
        None
    };
    if let Some(word) = word {
        refuse(c, cmd, word);
    }
}

pub fn abi_go_to_2_player_race(c: &mut Cpu) {
    go_to_2_player_race(c, c.r[4]);
}

/// A track info's entry: the chosen tournament (`tier` its tier's byte in the definition, the
/// front end's `choice`), its fee (too little money: the message; else asked (the answers
/// swapped for a tournament), and on yes paid with the till's sound), the tournament started
/// (`special` or not), its track info's menu.
fn go_to_track_info(c: &mut Cpu, cmd: u32, special: u32, choice: u32, tier: u32) {
    let m = &mut c.m;
    let def = m.r32(TOURNAMENT_MANAGER + 24);
    let k = m.r8(FE + choice) as u32;
    m.w8(FE + SPECIAL, special as u8);
    let t = (m.r8(def + tier) as u32).wrapping_add(k);
    let entry = def.wrapping_add(t.wrapping_mul(84)).wrapping_add(36);
    let fee = m.r32(entry + 48) as i32;
    if fee > 0 {
        if (m.r32(TOURNAMENT_MANAGER + 20) as i32) < fee {
            message(c, 247);
            return;
        }
        let answers = if special != 0 { (YES, NO) } else { (NO, YES) };
        let paid = ask(c, 248, answers, plain, |c, answer| {
            if answer == 0 {
                return false;
            }
            call(c, PLAY_SFX, &[SFX_TILL]);
            let m = &mut c.m;
            let money = m.r32(TOURNAMENT_MANAGER + 20).wrapping_sub(m.r32(entry + 48));
            m.w32(TOURNAMENT_MANAGER + 20, money);
            true
        });
        if !paid {
            return;
        }
    }
    let which = c.m.r8(FE + choice) as u32;
    crate::fetourn::start_new_tournament(c, TOURNAMENT_MANAGER, special, which);
    command(&mut c.m, cmd, CMD_MENU, 4336);
}

/// `MenuExtended_GoToTournTrackInfo` (FEMENUDEFS.CPP, 0x8002cfc0).
pub fn abi_go_to_tourn_track_info(c: &mut Cpu) {
    go_to_track_info(c, c.r[4], 0, 288, 2);
}

/// `MenuExtended_GoToSpecialEventTrackInfo` (FEMENUDEFS.CPP, 0x8002d128).
pub fn abi_go_to_special_event_track_info(c: &mut Cpu) {
    go_to_track_info(c, c.r[4], 1, 289, 14);
}

/// The name grid (`tMenuItemUserName`) at the menus' `grid` for player `p`: 7 letters, from
/// the start, their name's buffer; the name screen's menu `menu`, gone to.
fn name_grid(m: &mut Mem, cmd: u32, grid: u32, p: u32, menu: u32) {
    let all = menus(m);
    let (grid, menu) = (all + grid, all + menu);
    m.w16(grid + 32, 7);
    m.w16(grid + 36, 0);
    m.w16(grid + 34, 0);
    m.w16(grid + 128, p as u16);
    m.w32(grid + 28, NAMES + 8 * p);
    let screen = m.r32(NAME_SCREEN);
    m.w32(screen + 100, menu);
    m.w32(cmd + 4, menu);
}

/// `MenuExtended_EnterUserName` (FEMENUDEFS.CPP, 0x8002d294): the name grid for the player
/// entering it.
pub fn enter_user_name(m: &mut Mem, cmd: u32) {
    let a = app(m);
    let p = m.r8(a + 557) as u32;
    name_grid(m, cmd, 12588, p, 13008);
    m.w32(cmd, CMD_MENU);
}

pub fn abi_enter_user_name(c: &mut Cpu) {
    enter_user_name(&mut c.m, c.r[4]);
}

/// `MenuExtended_GoToShowroom` (FEMENUDEFS.CPP, 0x8002d2f8).
pub fn abi_go_to_showroom(c: &mut Cpu) {
    command(&mut c.m, c.r[4], CMD_MENU, 9068);
    set_state(c, CAR_SELECT, 5);
}

/// `MenuExtended_GoToDealerShowroom` (FEMENUDEFS.CPP, 0x8002d338).
pub fn abi_go_to_dealer_showroom(c: &mut Cpu) {
    command(&mut c.m, c.r[4], CMD_MENU, 9068);
    set_state(c, CAR_SELECT, 6);
}

/// `MenuExtended_SetHPSoloRace` (FEMENUDEFS.CPP, 0x8002d378).
pub fn abi_set_hp_solo_race(c: &mut Cpu) {
    let m = &mut c.m;
    set(m, &[(SLOT, 0), (RACE_KIND, SINGLE_RACE), (OPPONENTS, 0)]);
    let all = menus(m);
    m.w32(all + STOCK_LIST + LIST_KIND, 11);
}

/// `MenuExtended_SetHPDuelRace` (FEMENUDEFS.CPP, 0x8002d3a4).
pub fn abi_set_hp_duel_race(c: &mut Cpu) {
    let m = &mut c.m;
    set(m, &[(SLOT, 0), (RACE_KIND, SINGLE_RACE), (OPPONENTS, 1)]);
    let all = menus(m);
    m.w32(all + STOCK_LIST + LIST_KIND, 1);
}

/// `MenuExtended_SetHotPursuit` (FEMENUDEFS.CPP, 0x8002d3d0): hot pursuit, the car list's kind
/// for one or two players.
pub fn abi_set_hot_pursuit(c: &mut Cpu) {
    let m = &mut c.m;
    set(m, &[(OPTIONS_ON, 0), (SLOT, 0), (RACE_KIND, SINGLE_RACE)]);
    let v = if m.r8(FE + PLAYERS) == 1 { 9 } else { 1 };
    let all = menus(m);
    m.w32(all + STOCK_LIST + LIST_KIND, v);
    m.w8(FE + OPPONENTS, 0);
}

/// The player's car, the money; a save going on.
const PLAYER_CAR: u32 = 0x8011_59E1;
const MONEY: u32 = 0x8011_5DA4;
const CARD_ACTIVE: u32 = 0x8005_1F04;

/// `MenuExtended_SellCar` (FEMENUDEFS.CPP, 0x8002d420): (the last car sold only with the money
/// for the cheapest car after) asked, on yes the car sold for its used price, the seller's list
/// refreshed, the till's sound; else a message.
pub fn sell_car(c: &mut Cpu) {
    let m = &mut c.m;
    let id = m.r8(FE + GARAGE_CAR) as i32;
    let after = m.r32(MONEY).wrapping_add(fecars::calc_used_price(m, CAR_MANAGER, id) as u32);
    let ok = owned_cars(m) >= 2 || after as i32 >= fecars::cheapest_car_stock_price(m, CAR_MANAGER);
    if !ok {
        message(c, 170);
        return;
    }
    ask(c, 166, (YES, NO), plain, |c, answer| {
        if answer & 0xFFFF == 0 {
            return;
        }
        let m = &mut c.m;
        let car = m.r8(PLAYER_CAR) as u32;
        let gain = fecars::sell_car(m, CAR_MANAGER, car, 0) as u32;
        let money = m.r32(MONEY).wrapping_add(gain);
        m.w32(MONEY, money);
        refresh(c, SELLER_LIST, 0);
        call(c, PLAY_SFX, &[SFX_TILL]);
    });
}

pub fn abi_sell_car(c: &mut Cpu) {
    sell_car(c);
}

/// `MenuExtended_BuyCar` (FEMENUDEFS.CPP, 0x8002d578): the dealer's car (a full garage, or too
/// little money: a message, with a buzz) asked, on yes bought in its colour and paid, the till's
/// sound.
pub fn buy_car(c: &mut Cpu) {
    with_buffer(c, CarInfo::SIZE, |c, car| {
        let id = c.m.r8(FE + SECOND_CAR) as u32;
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        let m = &mut c.m;
        let info = CarInfo(car);
        if owned_cars(m) >= 32 {
            message(c, 76);
            return;
        }
        if (m.r32(MONEY) as i32) < info.prices(m, 0) {
            call(c, PLAY_SFX, &[SFX_BUZZ]);
            message(c, 168);
            return;
        }
        ask(c, 165, (YES, NO), plain, |c, answer| {
            if answer & 0xFFFF == 0 {
                return;
            }
            let m = &mut c.m;
            let model = info.car_id(m) as i32;
            let colour = m.r8(FE.wrapping_add(model as u32) + COLOURS) as i32;
            let cost = fecars::purchase_car(m, CAR_MANAGER, model, colour, 0) as u32;
            let money = m.r32(MONEY).wrapping_sub(cost);
            m.w32(MONEY, money);
            call(c, PLAY_SFX, &[SFX_TILL]);
        });
    });
}

pub fn abi_buy_car(c: &mut Cpu) {
    buy_car(c);
}

/// `MenuExtended_PurchaseUpgrade` (FEMENUDEFS.CPP, 0x8002d6cc): upgrade `k` for the garage car
/// (had: nothing; too little money: a message) asked, on yes bought and paid, the till's sound.
pub fn purchase_upgrade(c: &mut Cpu, k: u32) {
    let bit = 1u32 << (k & 31);
    with_buffer(c, CarInfo::SIZE, |c, car| {
        let id = c.m.r8(FE + GARAGE_CAR) as u32;
        fecars::get_garage_car(c, CAR_MANAGER, id, car, 0);
        let m = &mut c.m;
        let info = CarInfo(car);
        if info.upgrades(m) as u32 & bit != 0 {
            return;
        }
        let price = m.r32(car.wrapping_add(k.wrapping_add(1) << 2) + 32);
        if (m.r32(MONEY) as i32) < price as i32 {
            message(c, 169);
            return;
        }
        ask(c, 167, (YES, NO), plain, |c, answer| {
            if answer & 0xFFFF == 0 {
                return;
            }
            let m = &mut c.m;
            let id = m.r8(FE + GARAGE_CAR) as i32;
            let cost = fecars::purchase_upgrade(m, CAR_MANAGER, id, sx(bit) as i32, 0) as u32;
            let money = m.r32(MONEY).wrapping_sub(cost);
            m.w32(MONEY, money);
            call(c, PLAY_SFX, &[SFX_TILL]);
        });
    });
}

pub fn abi_purchase_upgrade(c: &mut Cpu) {
    purchase_upgrade(c, c.r[4]);
}

/// `MenuExtended_PurchaseUpgrade1` (FEMENUDEFS.CPP, 0x8002d80c).
pub fn abi_purchase_upgrade1(c: &mut Cpu) {
    purchase_upgrade(c, 1);
}

/// `MenuExtended_PurchaseUpgrade2` (FEMENUDEFS.CPP, 0x8002d82c).
pub fn abi_purchase_upgrade2(c: &mut Cpu) {
    purchase_upgrade(c, 2);
}

/// `MenuExtended_PurchaseUpgrade3` (FEMENUDEFS.CPP, 0x8002d84c).
pub fn abi_purchase_upgrade3(c: &mut Cpu) {
    purchase_upgrade(c, 0);
}

/// `MenuExtended_SaveGame` (FEMENUDEFS.CPP, 0x8002d86c): with no save going on and the save
/// item on, the game saved (no dialog).
pub fn save_game(c: &mut Cpu) {
    let m = &mut c.m;
    if m.r32(CARD_ACTIVE) == 0 {
        let all = menus(m);
        if m.r32(all + 12372) & 1 == 0 {
            generic_menu_save_game(c, false);
        }
    }
}

pub fn abi_save_game(c: &mut Cpu) {
    save_game(c);
}

/// `GenericMenuLoadGame` (FEMENUDEFS.CPP, 0x8002d8bc): with no save going on, the memory card
/// screen's message "loading", a frame drawn, `player`'s game loaded; the dialog hidden.
pub fn generic_menu_load_game(c: &mut Cpu, player: u32) {
    if c.m.r32(CARD_ACTIVE) == 0 {
        card_message(&mut c.m, 638);
        let a = app(&mut c.m);
        call_addr(c, REDRAW, &[a]);
        fememcard::load_game(c, sx(player), false, true);
        card_message(&mut c.m, u32::MAX);
        hide_card_dialog(c);
    }
    hide_card_dialog(c);
}

pub fn abi_generic_menu_load_game(c: &mut Cpu) {
    generic_menu_load_game(c, c.r[4]);
}

/// `MenuExtended_LoadGame` (FEMENUDEFS.CPP, 0x8002d950): with the load item on, asked (the
/// memory card screen's player's) whether to load: yes, loaded; else the application's help
/// (+568) hidden.
pub fn load_game(c: &mut Cpu) {
    let all = menus(&mut c.m);
    if c.m.r32(all + 12416) & 1 != 0 {
        return;
    }
    let card_player = |m: &mut Mem| {
        let screen = m.r32(MEMCARD_SCREEN);
        m.r16(screen + CARD_PLAYER) as i16 as i32 as u32
    };
    let setup = |m: &mut Mem, d: u32| {
        m.w32(d + DIALOG_VPTR, LOAD_VT);
        let p = card_player(m);
        m.w32(LOAD_PLAYER, p);
    };
    ask(c, 705, (YES, NO), setup, |c, answer| {
        if answer == 1 {
            let p = card_player(&mut c.m);
            generic_menu_load_game(c, p);
        } else {
            let h = app(&mut c.m) + 568;
            call_addr(c, DIALOG_HIDE, &[h]);
        }
    });
}

pub fn abi_load_game(c: &mut Cpu) {
    load_game(c);
}

/// The race's results (the cop's tickets and the target), the cop cars' ids (5 a car, by
/// livery), the first player's race stats, the track raced, the post-race screens.
const RESULTS: u32 = 0x8011_44A4;
const COP_CARS: u32 = 0x8005_1E68;
const STATS: u32 = 0x8011_0E08;
const TRACK_RACED: u32 = 0x8013_DAEC;
const STANDINGS_SCREEN: u32 = 0x8005_22F8;
const STANDINGS3_SCREEN: u32 = 0x8005_22FC;

/// `MenuExtended_TierFinished` (FEMENUDEFS.CPP, 0x8002da28): the award's screen if there's an
/// award, else (asked to save) the save menu or the main menu.
pub fn tier_finished(c: &mut Cpu, cmd: u32) {
    c.m.w32(cmd, CMD_TRANSITION);
    let award = with_buffer(c, 80, |c, info| {
        crate::fetourn::get_award_information(&mut c.m, TOURNAMENT_MANAGER, info);
        c.m.r32(info + 48) != 0
    });
    let menu = if award {
        14708
    } else if call_addr(c, ASK_TO_SAVE, &[]) != 0 {
        SAVE_MENU
    } else {
        MAIN_MENU
    };
    let all = menus(&mut c.m);
    c.m.w32(cmd + 4, all + menu);
}

pub fn abi_tier_finished(c: &mut Cpu) {
    tier_finished(c, c.r[4]);
}

/// `MenuExtended_DidUserWinBeTheCop` (FEMENUDEFS.CPP, 0x8002daa8): one player's hot pursuit as a
/// police car, more tickets than the target: the car's cop car (by its livery) won, made
/// available and noted with its livery (unless already available); whether.
pub fn did_user_win_be_the_cop(c: &mut Cpu) -> bool {
    with_buffer(c, CarInfo::SIZE, |c, car| {
        let id = c.m.r8(FE + STOCK_CAR) as u32;
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        let m = &mut c.m;
        let mine = CarInfo(car);
        if mine.car_class(m) != 7 || m.r8(FE + RACE_KIND) != SINGLE_RACE || m.r8(FE + PLAYERS) == 1 {
            return false;
        }
        if m.r32(RESULTS + 428) as i32 >= m.r32(RESULTS + 440) as i32 {
            return false;
        }
        let model = mine.car_id(m) as i32 as u32;
        let livery = m.r8(FE.wrapping_add(model) + LIVERIES) as i8 as i32 as u32;
        let k = model.wrapping_sub(22).wrapping_mul(5).wrapping_add(livery);
        let cop = m.r16(COP_CARS.wrapping_add(k << 2)) as i16 as i32;
        let won = CarInfo(fecars::get_car_from_id(m, CAR_MANAGER, cop));
        if won.available(m) != 0 {
            return false;
        }
        let id = won.car_id(m) as i32 as u32;
        fecars::set_car_available(m, CAR_MANAGER, id, 1);
        let v = won.car_id(m) as u8;
        m.w8(FE + COP_WON, v);
        let v = m.r8(FE.wrapping_add(model) + LIVERIES);
        m.w8(FE + COP_WON + 1, v);
        true
    })
}

pub fn abi_did_user_win_be_the_cop(c: &mut Cpu) {
    c.r[2] = did_user_win_be_the_cop(c) as u32;
}

/// `MenuExtended_PostGameMenu` (FEMENUDEFS.CPP, 0x8002dbc4): the new records cleared; after a
/// tournament's race its standings (the final ones once over), after the pink slips' their
/// standings, after hot pursuit the cop car won's screen, else the main menu.
pub fn post_game_menu(c: &mut Cpu, cmd: u32) {
    crate::statchk::clear_new_records(&mut c.m);
    c.m.w32(cmd, CMD_TRANSITION);
    let menu = match c.m.r8(FE + RACE_KIND) {
        TOURNAMENT => {
            let m = &mut c.m;
            let done = crate::fetourn::is_tournament_finished(m, TOURNAMENT_MANAGER);
            let (menu, screen) = if done { (13784, STANDINGS_SCREEN) } else { (13616, STANDINGS3_SCREEN) };
            let s = m.r32(screen);
            m.w32(s + 132, 1);
            menu
        }
        PINK_SLIPS => 13404,
        SINGLE_RACE if did_user_win_be_the_cop(c) => 14584,
        _ => MAIN_MENU,
    };
    let all = menus(&mut c.m);
    c.m.w32(cmd + 4, all + menu);
}

pub fn abi_post_game_menu(c: &mut Cpu) {
    post_game_menu(c, c.r[4]);
}

/// The first car's race on the track: its lap record and top time saved if it made them.
fn save_records(c: &mut Cpu) {
    let track = |c: &mut Cpu| c.m.r16(TRACK_RACED) as i16 as i32 as u32;
    let t = track(c);
    let lap = with_buffer(c, 8, |c, car| {
        crate::statchk::is_record_lap_time(c, STATS, t, car).then(|| c.m.r16(car) as i16 as i32 as u32)
    });
    if let Some(car) = lap {
        crate::statchk::save_record_lap_time(c, STATS, car);
    }
    let t = track(c);
    if crate::statchk::is_top_time(c, STATS, t) & 0xFFFF != 0 {
        let t = track(c);
        crate::statchk::save_top_time(c, STATS, t);
    }
}

/// `MenuExtended_FinishedPlayer1GetName` (FEMENUDEFS.CPP, 0x8002dcc4): the first player's name
/// entered: with a second player's name to come, the grid set for it; else the records saved
/// and the records screen.
pub fn finished_player1_get_name(c: &mut Cpu, cmd: u32) {
    let m = &mut c.m;
    let a = app(m);
    m.w32(cmd, CMD_TRANSITION);
    if m.r32(a + 884) != 0 && m.r32(a + 876) == 0 {
        name_grid(m, cmd, 12868, 1, 14160);
        return;
    }
    save_records(c);
    let all = menus(&mut c.m);
    c.m.w32(cmd + 4, all + 14332);
}

pub fn abi_finished_player1_get_name(c: &mut Cpu) {
    finished_player1_get_name(c, c.r[4]);
}

/// `MenuExtended_FinishedPlayer2GetName` (FEMENUDEFS.CPP, 0x8002ddd4): the records saved, the
/// records screen.
pub fn finished_player2_get_name(c: &mut Cpu, cmd: u32) {
    save_records(c);
    command(&mut c.m, cmd, CMD_TRANSITION, 14332);
}

pub fn abi_finished_player2_get_name(c: &mut Cpu) {
    finished_player2_get_name(c, c.r[4]);
}

/// `MenuExtended_SetPinkSlips` (FEMENUDEFS.CPP, 0x8002de74): the menu's background; the pink
/// slips: two players, no duel, no winner, no wins, no tracks won.
pub fn abi_set_pink_slips(c: &mut Cpu) {
    swap_background(c);
    let m = &mut c.m;
    set(m, &[(RACE_KIND, PINK_SLIPS), (PLAYERS, 1), (OPPONENTS, 0)]);
    m.w16(FE + PINK_WINNER, 0xFFFF);
    set(m, &[(SLOT, 0), (WINS, 0), (WINS + 1, 0)]);
    for k in 0..5 {
        m.w16(FE + PINK_WINNERS + 2 * k, 0xFFFF);
    }
}

/// The pink slips' standings screen (+388 the winner).
const PINK_STANDINGS: u32 = 0x8005_2DD4;
/// The music's buffer: its fill, its target; `textwidth`; the host's `PSXDrawSquare`.
const MUSIC_FILLED: fn(&mut Cpu) = nfs4rc::f_8007aa48;
const MUSIC_TARGET: fn(&mut Cpu) = nfs4rc::f_8007a9c4;
const TEXT_WIDTH: fn(&mut Cpu) = nfs4rc::f_800e4a2c;
const DRAW_SQUARE: u32 = 0x8004_F12C;

/// `MenuExtended_AwardPinkSlipsCar` (FEMENUDEFS.CPP, 0x8002df34): the pink slips' winner told
/// (by name and number) while the loser's car is moved: the card layer up, the loser's car
/// added to the winner's pink slips with its upgrades, saved; then "loading", the game loaded
/// back, the layer down; the music's buffer refilled (frames drawn with the "loading" line);
/// the main menu.
pub fn award_pink_slips_car(c: &mut Cpu, cmd: u32) {
    with_buffer(c, YES_NO_SIZE + 80 + CarInfo::SIZE, |c, d| {
        let (text, car) = (d + YES_NO_SIZE, d + YES_NO_SIZE + 80);
        crate::fedialog::yes_no(c, DialogYesNo(d));
        let m = &mut c.m;
        m.w32(d + 160, 658);
        m.w32(d + 164, 659);
        m.w16(d + 136, 1);
        let standings = m.r32(PINK_STANDINGS);
        let winner = m.r16(standings + 388) as i16 as i32 as u32;
        let format = call(c, TEXT_WORD, &[667]);
        let name = crate::front::player_name(c, winner);
        call(c, SPRINTF, &[text, format, name, winner.wrapping_add(1)]);
        card_dialog(c, text);
        draw_until_shown(c);
        fememcard::init_memcard(c, false, true);
        // the loser's car to the winner's pink slips, with its upgrades
        let loser = 1u32.wrapping_sub(winner);
        let id = c.m.r8(FE.wrapping_add(loser) + PINK_CARS) as u32;
        fecars::get_pink_slips_car(c, CAR_MANAGER, id, car, sx(loser));
        let m = &mut c.m;
        let info = CarInfo(car);
        let (model, colour) = (info.car_id(m) as i32, info.color(m) as i32);
        fecars::add_to_pink_slips_list(m, CAR_MANAGER, model, colour, winner as i32);
        let (id, upgrades) = (m.r8(FE.wrapping_add(winner) + PINK_CARS) as i32, info.upgrades(m) as i32);
        fecars::add_upgrades_to_pink_slips_list(m, CAR_MANAGER, id, upgrades, winner as i32);
        fememcard::save_pink_slips_with_dialogs(c, winner, 2, u32::MAX);
        hide_card_dialog(c);
        command(&mut c.m, cmd, CMD_TRANSITION, MAIN_MENU);
        // the game loaded back
        let t = call(c, TEXT_WORD, &[629]);
        card_dialog(c, t);
        draw_until_shown(c);
        generic_menu_load_game(c, 0);
        fememcard::deinit_memcard(c);
        hide_card_dialog(c);
        // the music's buffer refilled
        while (call(c, MUSIC_FILLED, &[]) as i32) < call(c, MUSIC_TARGET, &[]) as i32 {
            crate::feaudio::systemtask(c);
            let a = app(&mut c.m);
            call_addr(c, REDRAW, &[a]);
            crate::fetextrender::menu_text_positioned_justify(c, 638, 480, 220, 1, 1, 4);
            let t = call(c, TEXT_WORD, &[638]);
            let w = call(c, TEXT_WIDTH, &[t]);
            call_addr(c, DRAW_SQUARE, &[0, 480, 220, 0u32.wrapping_sub(w).wrapping_sub(5), 7]);
            c.tick();
        }
        command(&mut c.m, cmd, CMD_TRANSITION, MAIN_MENU);
        crate::fescreen::screen_destructor(c, Screen(d), 2);
    });
}

pub fn abi_award_pink_slips_car(c: &mut Cpu) {
    award_pink_slips_car(c, c.r[4]);
}

/// `MenuExtended_GoToGarage` (FEMENUDEFS.CPP, 0x8002e1e8): the garage's cars (the list's kind,
/// refreshed), the garage's menu (a tournament's or not), the car select's garage state.
pub fn go_to_garage(c: &mut Cpu, cmd: u32) {
    let m = &mut c.m;
    m.w8(FE + OPTIONS_ON, 1);
    let all = menus(m);
    m.w32(all + GARAGE_LIST + LIST_KIND, 64);
    refresh(c, GARAGE_LIST, u32::MAX);
    let m = &mut c.m;
    let menu = if m.r32(TOURNAMENT_MANAGER + 12) == 0 { 4968 } else { 5092 };
    command(m, cmd, CMD_MENU, menu);
    set_state(c, CAR_SELECT, 1);
}

pub fn abi_go_to_garage(c: &mut Cpu) {
    go_to_garage(c, c.r[4]);
}

/// `MenuExtended_GoToTournTrophyRoom` (FEMENUDEFS.CPP, 0x8002e290).
pub fn abi_go_to_tourn_trophy_room(c: &mut Cpu) {
    c.m.w8(FE + SPECIAL, 0);
}

/// `MenuExtended_GoToSETrophyRoom` (FEMENUDEFS.CPP, 0x8002e29c).
pub fn abi_go_to_se_trophy_room(c: &mut Cpu) {
    c.m.w8(FE + SPECIAL, 1);
}

/// `MenuExtended_SetBeginner` (FEMENUDEFS.CPP, 0x8002e2ac).
pub fn abi_set_beginner(c: &mut Cpu) {
    c.m.w8(FE + DIFFICULTY, 0);
}

/// `MenuExtended_SetIntermediate` (FEMENUDEFS.CPP, 0x8002e2b8).
pub fn abi_set_intermediate(c: &mut Cpu) {
    c.m.w8(FE + DIFFICULTY, 1);
}

/// `MenuExtended_SetExpert` (FEMENUDEFS.CPP, 0x8002e2c8).
pub fn abi_set_expert(c: &mut Cpu) {
    c.m.w8(FE + DIFFICULTY, 2);
}

/// `MenuExtended_ExitTourney` (FEMENUDEFS.CPP, 0x8002e2d8): asked: yes, the main menu; no,
/// nothing.
pub fn exit_tourney(c: &mut Cpu, cmd: u32) {
    ask(c, 158, (YES, NO), plain, |c, answer| {
        if answer & 0xFFFF != 0 {
            command(&mut c.m, cmd, CMD_TRANSITION, MAIN_MENU);
        } else {
            c.m.w32(cmd, CMD_NONE);
        }
    });
}

pub fn abi_exit_tourney(c: &mut Cpu) {
    exit_tourney(c, c.r[4]);
}

/// `MenuExtended_ExitPinkSlipsEarly` (FEMENUDEFS.CPP, 0x8002e368): asked: yes, each player's
/// pink slips cars saved back to their card (a message by name and number), the card layer
/// down, the main menu, the race kind back to a free race.
pub fn exit_pink_slips_early(c: &mut Cpu, cmd: u32) {
    ask(c, 158, (YES, NO), plain, |c, answer| {
        if answer & 0xFFFF == 0 {
            return;
        }
        fememcard::init_memcard(c, false, true);
        with_buffer(c, 104, |c, text| {
            for p in 0..2u32 {
                let format = call(c, TEXT_WORD, &[664]);
                let name = crate::front::player_name(c, p);
                call(c, SPRINTF, &[text, format, name, p + 1]);
                let a = app(&mut c.m);
                c.m.w32(a + 864, text);
                fememcard::save_pink_slips_with_dialogs(c, sx(p), 1, u32::MAX);
            }
        });
        fememcard::deinit_memcard(c);
        hide_card_dialog(c);
        command(&mut c.m, cmd, CMD_TRANSITION, MAIN_MENU);
        c.m.w8(FE + RACE_KIND, 0);
    });
}

pub fn abi_exit_pink_slips_early(c: &mut Cpu) {
    exit_pink_slips_early(c, c.r[4]);
}

/// `MenuExtended_PinkSlipsContinue` (FEMENUDEFS.CPP, 0x8002e498): with no player past half the
/// races, the next race; else the pink slips' win.
pub fn pink_slips_continue(m: &mut Mem, cmd: u32) {
    let need = (m.r8(FE + RACES) as u32 >> 1) + 1;
    if (m.r8(FE + WINS) as u32) < need && (m.r8(FE + WINS + 1) as u32) < need {
        let v = m.r8(FE + SLOT).wrapping_add(1);
        m.w8(FE + SLOT, v);
        m.w32(cmd, CMD_RACE);
    } else {
        command(m, cmd, CMD_TRANSITION, 14460);
    }
}

pub fn abi_pink_slips_continue(c: &mut Cpu) {
    pink_slips_continue(&mut c.m, c.r[4]);
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

/// The front end's application.
const FE_APP: u32 = crate::globals::FE_APP;

/// `tGlobalMenuDefs::tGlobalMenuDefs` (FEMENUDEFS.CPP, 0x8002e504): every menu of the front
/// end and every item on them constructed in place in turn (each menu after its items, the
/// items given the menus they go to and their callbacks; the derived items' vtables written
/// after their bases' constructors), the sliders sized, a few flags and fields set; `this`.
/// Each call is a member's constructor with its arguments: an item's (its word, the menu it
/// goes to, its callback, its place), a menu's (its flags, its screen (read from the screen's
/// global, FRONT's `SCREEN_POINTERS`), its callbacks and title, its items, none last), an
/// iterator's (its value, its list).
pub fn global_menu_defs(c: &mut Cpu, this: u32) -> u32 {
    call_addr(c, NFS4_BUTTON, &[this, 92, this + 1112, SET_ONE_PLAYER, 30, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 44, 93, this + 2336, SET_TWO_PLAYER, 40, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 88, 95, this + 388, 0, 50, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 132, 94, this + 9456, 0, 60, 10]);
    let main_screen = c.m.r32(MAIN_SCREEN);
    call_addr(c, NFS4_MENU, &[this + 176, 0x1014, main_screen, 0, 0, 0, 181, this, this + 44, this + 132, this + 88, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 300, 100, this + 13192, GO_TO_TOURN_TROPHY_ROOM, 50, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 344, 106, this + 13192, GO_TO_SE_TROPHY_ROOM, 50, 10]);
    call_addr(c, NFS4_MENU, &[this + 388, 0x1014, main_screen, 0, 0, 0, 96, this + 300, this + 344, 0]);
    let screen = c.m.r32(0x8005_2518);
    call_addr(c, BLANK_MENU, &[this + 512, 0x4010, screen, 0, 0, 0, 0xFFFF_FFFF]);
    call_addr(c, NFS4_BUTTON, &[this + 636, 162, this + 3360, SET_BEGINNER, 140, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 680, 163, this + 3360, SET_INTERMEDIATE, 150, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 724, 164, this + 3360, SET_EXPERT, 160, 10]);
    call_addr(c, NFS4_MENU, &[this + 768, 0x1004, main_screen, 0, 0, 0, 184, this + 636, this + 680, this + 724, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 892, 97, this + 3484, SET_TEST_DRIVE, 240, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 936, 115, this + 1368, SET_SINGLE_RACE, 70, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 980, 114, this + 1580, SET_HOT_PURSUIT, 80, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 1024, 100, this + 1808, SET_TOURNAMENT, 90, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 1068, 106, this + 2036, SET_SPECIAL_EVENT, 100, 10]);
    call_addr(c, NFS4_MENU, &[this + 1112, 0x1004, main_screen, 0, 0, 0, 182, this + 892, this + 936, this + 980, this + 1024, this + 1068, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 1236, 109, this + 3360, SET_SOLO_RACE, 110, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 1280, 110, this + 768, SET_DUEL_RACE, 120, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 1324, 112, this + 768, SET_FULL_GRID, 130, 10]);
    call_addr(c, NFS4_MENU, &[this + 1368, 0x1004, main_screen, 0, 0, 0, 183, this + 1236, this + 1280, this + 1324, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 1492, 109, this + 768, SET_HP_SOLO_RACE, 110, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 1536, 110, this + 768, SET_HP_DUEL_RACE, 120, 10]);
    call_addr(c, NFS4_MENU, &[this + 1580, 0x1004, main_screen, 0, 0, 0, 183, this + 1492, this + 1536, 0]);
    call_addr(c, TOURNAMENT_ITERATOR, &[this + 1704, 0x8011_59D8, TOURNAMENT_MANAGER]);
    call_addr(c, NFS4_BUTTON, &[this + 1724, 91, 0, GO_TO_TOURN_TRACK_INFO, 34, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 1768, 149, this + 1704, 44, 10]);
    let screen = c.m.r32(0x8005_24EC);
    call_addr(c, NFS4_MENU, &[this + 1808, 0x1000, screen, 0, 0, GO_TO_TOURN_TRACK_INFO, 102, this + 1724, this + 1768, 0]);
    call_addr(c, TOURNAMENT_ITERATOR, &[this + 1932, 0x8011_59D9, TOURNAMENT_MANAGER]);
    call_addr(c, NFS4_BUTTON, &[this + 1952, 91, 0, GO_TO_SPECIAL_EVENT_TRACK_INFO, 34, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 1996, 106, this + 1932, 54, 10]);
    let screen = c.m.r32(0x8005_24EC);
    call_addr(c, NFS4_MENU, &[this + 2036, 0x1000, screen, 0, 0, GO_TO_SPECIAL_EVENT_TRACK_INFO, 101, this + 1952, this + 1996, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 2160, 97, this + 3484, SET_TEST_DRIVE, 240, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 2204, 115, 0, GO_TO_TWO_PLAYER_SINGLE_RACE, 70, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 2248, 114, this + 768, SET_HOT_PURSUIT, 80, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 2292, 107, this + 2592, SET_PINK_SLIPS, 170, 10]);
    call_addr(c, NFS4_MENU, &[this + 2336, 0x1004, main_screen, 0, 0, 0, 182, this + 2160, this + 2204, this + 2248, this + 2292, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 2460, 193, this + 3360, GO_TO_BEST_OF_ONE, 170, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 2504, 194, this + 2960, GO_TO_BEST_OF_THREE, 170, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 2548, 195, this + 3084, GO_TO_BEST_OF_FIVE, 170, 10]);
    call_addr(c, NFS4_MENU, &[this + 2592, 0x1000, main_screen, 0, 0, 0, 183, this + 2460, this + 2504, this + 2548, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 2716, 91, 0, GO_TO_CAR_SELECT, 36, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 2760, 196, this + 3208, 46, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 2800, 197, this + 3208, 46, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 2840, 198, this + 3208, 46, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 2880, 199, this + 3208, 46, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 2920, 200, this + 3208, 46, 10]);
    let screen = c.m.r32(0x8005_22F4);
    call_addr(c, NFS4_MENU, &[this + 2960, 0x1400, screen, 0, this + 3988, GO_TO_CAR_SELECT, 191, this + 2716, this + 2760, this + 2800, this + 2840, 0]);
    let screen = c.m.r32(0x8005_22F4);
    call_addr(c, NFS4_MENU, &[this + 3084, 0x1400, screen, 0, this + 3988, GO_TO_CAR_SELECT, 192, this + 2716, this + 2760, this + 2800, this + 2840, this + 2880, this + 2920, 0]);
    call_addr(c, TRACK_ITERATOR, &[this + 3208, 0x8011_58DB, 0x8011_58FE, TRACK_MANAGER]);
    call_addr(c, NFS4_BUTTON, &[this + 3232, 91, 0, GO_TO_CAR_SELECT, 28, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 3276, 148, this + 3208, 38, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 3316, 213, this + 4164, 0, 58, 10]);
    let screen = c.m.r32(0x8005_2544);
    call_addr(c, NFS4_MENU, &[this + 3360, 0x1600, screen, 0, this + 3988, GO_TO_CAR_SELECT, 201, this + 3232, this + 3276, this + 3316, 0]);
    let screen = c.m.r32(0x8005_2544);
    call_addr(c, NFS4_MENU, &[this + 3484, 0x1600, screen, 0, 0, GO_TO_CAR_SELECT, 201, this + 3232, this + 3276, this + 3316, 0]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3608, 0x8005_1E04, 0x8011_58E5, 0x8011_58FE]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3628, 0x8005_1E14, 0x8011_58EA, 0x8011_58FE]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3648, 0x8005_1DFC, 0x8011_58EF, 0x8011_58FE]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3668, 0x8005_1DFC, 0x8011_58CE, 0x8011_58FE]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3688, 0x8005_1DFC, 0x8011_58D3, 0x8011_58FE]);
    call_addr(c, INDEXED_ITERATOR, &[this + 3708, 0x8005_1DFC, 0x8011_58F4, 0x8011_58FE]);
    call_addr(c, ITERATOR, &[this + 3728, 0x8005_1DFC, 0x8011_58D9]);
    call_addr(c, CHOICE, &[this + 3744, 203, this + 3608]);
    c.m.w32(this + 3768, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 3776, 205, this + 3628]);
    c.m.w32(this + 3800, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 3808, 206, this + 3648]);
    c.m.w32(this + 3832, 0x8001_07F8);
    c.m.w16(this + 3840, 128);
    call_addr(c, CHOICE, &[this + 3844, 207, this + 3668]);
    c.m.w32(this + 3868, 0x8001_07F8);
    c.m.w16(this + 3876, 128);
    call_addr(c, CHOICE, &[this + 3880, 208, this + 3688]);
    c.m.w32(this + 3904, 0x8001_07F8);
    c.m.w16(this + 3912, 128);
    call_addr(c, CHOICE, &[this + 3916, 209, this + 3708]);
    c.m.w32(this + 3940, 0x8001_07F8);
    c.m.w16(this + 3948, 128);
    call_addr(c, CHOICE, &[this + 3952, 211, this + 3728]);
    c.m.w32(this + 3976, 0x8001_07F8);
    c.m.w16(this + 3984, 128);
    call_addr(c, OPTIONS, &[this + 3988, 0x1000, 0, 0, 0, 0, 186, 0xFFFF_FFFF, this + 3744, this + 3776, this + 3808, this + 3844, this + 3880, this + 3916, this + 3952, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 4120, 0, 0, 0, 0xFFFF_FFFF, 0xFFFF_FFFF]);
    let screen = c.m.r32(0x8005_2560);
    c.m.w32(this + 4144, 0x8001_1480);
    // (an empty item list: the constructor takes its first item whatever it is and reads on to
    // a 0, so the original's menu got the items the call before left on its stack, the options'
    // (the same every time); likewise the song menu below, the main menu's)
    call_addr(c, OPTIONS_MENU, &[this + 4164, 0x1000, screen, 0, 0, 0, 213, 1, 10, 0, this + 3808, this + 3844, this + 3880, this + 3916, this + 3952, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 4292, 91, 0, GO_TO_GARAGE, 33, 10]);
    let screen = c.m.r32(0x8005_255C);
    call_addr(c, NFS4_MENU, &[this + 4336, 0x1004, screen, 0, 0, 0, 250, this + 4292, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 4460, 0x8011_5914, CAR_MANAGER.0]);
    let fe_app = c.m.r32(FE_APP);
    call_addr(c, CAR_COLOR_ITERATOR, &[this + 4488, 0x8011_5918, fe_app + 556, 0x8011_5914, 48, CAR_MANAGER.0]);
    call_addr(c, NFS4_BUTTON, &[this + 4520, 190, 0, GO_TO_RACE, 128, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 4564, 147, this + 4460, 28, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 4604, 289, this + 4488, 38, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 4644, 275, 0, GO_TO_SHOWROOM, 48, 10]);
    let car_select = c.m.r32(CAR_SELECT);
    call_addr(c, NFS4_MENU, &[this + 4688, 0x1A00, car_select, 0, this + 8408, GO_TO_RACE, 187, this + 4520, this + 4564, this + 4604, this + 4644, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 4812, 0x8011_59DB, CAR_MANAGER.0]);
    call_addr(c, NFS4_CHOICE, &[this + 4840, 147, this + 4812, 28, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 4880, 117, this + 7144, 0, 58, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 4924, 146, 0, GO_TO_UPGRADES, 68, 10]);
    call_addr(c, NFS4_MENU, &[this + 4968, 0x1A00, car_select, 0, this + 8408, GO_TO_RACE, 144, this + 4520, this + 4840, this + 4880, this + 4924, 0]);
    call_addr(c, NFS4_MENU, &[this + 5092, 0x1A00, car_select, 0, this + 8408, GO_TO_RACE, 144, this + 4520, this + 4924, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 5216, 0x8011_5BD5, CAR_MANAGER.0]);
    call_addr(c, NFS4_BUTTON, &[this + 5244, 190, 0, GO_TO_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5288, 147, this + 4460, 12, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5328, 289, this + 4488, 22, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 5368, 121, this + 7496, GO_TO_DEALER, 0xFFFF_FFFF, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5412, 189, this + 5216, 32, 10]);
    let duel_select = c.m.r32(DUEL_SELECT);
    call_addr(c, NFS4_MENU, &[this + 5452, 0x1800, duel_select, 0, this + 8408, GO_TO_RACE, 187, this + 5244, this + 4840, this + 5368, this + 5412, 0]);
    call_addr(c, NFS4_MENU, &[this + 5576, 0x1800, duel_select, 0, this + 8408, GO_TO_RACE, 187, this + 5244, this + 4564, this + 4604, this + 5412, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 5700, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5744, 147, this + 4460, 12, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5784, 289, this + 4488, 22, 10]);
    let screen = c.m.r32(0x8005_24D4);
    call_addr(c, TWO_PLAYER_MENU, &[this + 5824, 0x1008, screen, 0, this + 8540, GO_TO_2_PLAYER_RACE, 187, this + 5700, this + 5744, this + 5784, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 5948, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 5992, 147, this + 4460, 12, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 6032, 289, this + 4488, 22, 10]);
    let screen = c.m.r32(0x8005_24D8);
    call_addr(c, TWO_PLAYER_MENU, &[this + 6072, 0x1008, screen, 0, this + 8672, GO_TO_2_PLAYER_RACE, 187, this + 5948, this + 5992, this + 6032, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 6196, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 6240, 147, this + 4812, 12, 10]);
    let screen = c.m.r32(0x8005_24D4);
    call_addr(c, TWO_PLAYER_MENU, &[this + 6280, 0x1008, screen, 0, this + 8540, GO_TO_2_PLAYER_RACE, 187, this + 5700, this + 6240, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 6404, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 6448, 147, this + 4812, 12, 10]);
    let screen = c.m.r32(0x8005_24D8);
    call_addr(c, TWO_PLAYER_MENU, &[this + 6488, 0x1008, screen, 0, this + 8672, GO_TO_2_PLAYER_RACE, 187, this + 6404, this + 6448, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 6612, 0x8011_59DD, CAR_MANAGER.0]);
    call_addr(c, NFS4_BUTTON, &[this + 6640, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 6684, 147, this + 6612, 12, 10]);
    let screen = c.m.r32(0x8005_24DC);
    call_addr(c, TWO_PLAYER_MENU, &[this + 6724, 0x1008, screen, 0, this + 8804, GO_TO_2_PLAYER_RACE, 187, this + 6640, this + 6684, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 6848, 190, 0, GO_TO_2_PLAYER_RACE, 42, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 6892, 147, this + 6612, 12, 10]);
    let screen = c.m.r32(0x8005_24E0);
    call_addr(c, TWO_PLAYER_MENU, &[this + 6932, 0x1008, screen, 0, this + 8936, GO_TO_2_PLAYER_RACE, 187, this + 6848, this + 6892, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 7056, 121, this + 7496, GO_TO_DEALER, 88, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7100, 122, this + 7732, GO_TO_SELLER, 78, 10]);
    call_addr(c, NFS4_MENU, &[this + 7144, 0x2200, car_select, 0, 0, 0, 145, this + 7056, this + 7100, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 7268, 0x8011_59E0, CAR_MANAGER.0]);
    call_addr(c, CAR_COLOR_ITERATOR, &[this + 7296, 0x8011_5918, fe_app + 556, 0x8011_59E0, 48, CAR_MANAGER.0]);
    call_addr(c, NFS4_CHOICE, &[this + 7328, 147, this + 7268, 28, 10]);
    call_addr(c, NFS4_CHOICE, &[this + 7368, 289, this + 7296, 38, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7408, 118, 0, BUY_CAR, 88, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7452, 275, 0, GO_TO_DEALER_SHOWROOM, 48, 10]);
    call_addr(c, NFS4_MENU, &[this + 7496, 0x2240, car_select, 0, 0, 0, 145, this + 7328, this + 7368, this + 7408, this + 7452, 0]);
    call_addr(c, CAR_ITERATOR, &[this + 7620, 0x8011_59E1, CAR_MANAGER.0]);
    call_addr(c, NFS4_CHOICE, &[this + 7648, 147, this + 7620, 28, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7688, 120, 0, SELL_CAR, 78, 10]);
    call_addr(c, NFS4_MENU, &[this + 7732, 0x2200, car_select, 0, 0, 0, 145, this + 7648, this + 7688, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 7856, 151, 0, PURCHASE_UPGRADE1, 98, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7900, 152, 0, PURCHASE_UPGRADE2, 108, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 7944, 153, 0, PURCHASE_UPGRADE3, 118, 10]);
    call_addr(c, NFS4_MENU, &[this + 7988, 0x2200, car_select, 0, 0, 0, 146, this + 7856, this + 7900, this + 7944, 0]);
    call_addr(c, INDEXED_ITERATOR, &[this + 8112, 0x8005_1E1C, 0x8011_5916, fe_app + 556]);
    call_addr(c, INDEXED_ITERATOR, &[this + 8132, 0x8005_1DFC, 0x8011_5BCC, fe_app + 556]);
    call_addr(c, ITERATOR, &[this + 8152, 0x8005_1DFC, 0x8011_5BCE]);
    call_addr(c, ITERATOR, &[this + 8168, 0x8005_1E0C, 0x8011_58DA]);
    call_addr(c, CHOICE, &[this + 8184, 267, this + 8112]);
    c.m.w32(this + 8208, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8216, 268, this + 8132]);
    c.m.w32(this + 8240, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8248, 274, this + 8152]);
    c.m.w32(this + 8272, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8280, 267, this + 8112]);
    c.m.w32(this + 8304, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8312, 268, this + 8132]);
    c.m.w32(this + 8336, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8344, 274, this + 8152]);
    c.m.w32(this + 8368, 0x8001_0850);
    call_addr(c, CHOICE, &[this + 8376, 271, this + 8168]);
    c.m.w32(this + 8400, 0x8001_0850);
    call_addr(c, OPTIONS, &[this + 8408, 0x1000, 0, 0, 0, 0, 188, 0xFFFF_FFFF, this + 8184, this + 8216, this + 8248, this + 8376, 0]);
    let screen = c.m.r32(0x8005_24D4);
    call_addr(c, OPTIONS, &[this + 8540, 0x1008, screen, 0, 0, 0, 188, 0, this + 8184, this + 8216, this + 8248, 0]);
    let screen = c.m.r32(0x8005_24D8);
    call_addr(c, OPTIONS, &[this + 8672, 0x1008, screen, 0, 0, 0, 188, 1, this + 8280, this + 8312, this + 8344, 0]);
    let screen = c.m.r32(0x8005_24DC);
    call_addr(c, OPTIONS, &[this + 8804, 0x1008, screen, 0, 0, 0, 188, 0, this + 8184, this + 8216, this + 8248, 0]);
    let screen = c.m.r32(0x8005_24E0);
    call_addr(c, OPTIONS, &[this + 8936, 0x1008, screen, 0, 0, 0, 188, 1, this + 8280, this + 8312, this + 8344, 0]);
    call_addr(c, BLANK_MENU, &[this + 9068, 0x4000, 0, 0, 0, 0, 0xFFFF_FFFF]);
    call_addr(c, NFS4_BUTTON, &[this + 9192, 454, this + 10208, 0, 180, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 9236, 455, this + 10856, 0, 190, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 9280, 457, this + 11116, 0, 200, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 9324, 458, this + 12460, 0, 210, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 9368, 459, 0, ENTER_USER_NAME, 220, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 9412, 461, this + 14832, 0, 230, 10]);
    call_addr(c, NFS4_MENU, &[this + 9456, 0x1014, main_screen, 0, 0, 0, 94, this + 9192, this + 9236, this + 9280, this + 9324, this + 9368, this + 9412, 0]);
    call_addr(c, RANGE_ITERATOR, &[this + 9580, 0, 127, 0x8011_5BF9]);
    call_addr(c, RANGE_ITERATOR, &[this + 9596, 0, 127, 0x8011_5BFA]);
    call_addr(c, RANGE_ITERATOR, &[this + 9612, 0, 127, 0x8011_5BFC]);
    call_addr(c, RANGE_ITERATOR, &[this + 9628, 0, 127, 0x8011_5BFB]);
    call_addr(c, RANGE_ITERATOR, &[this + 9644, 0, 127, 0x8011_5BFD]);
    call_addr(c, ITERATOR, &[this + 9660, 0x8005_1E60, 0x8011_5BFE]);
    call_addr(c, AUDIO_SLIDER, &[this + 9676, 470, this + 9580, 0]);
    call_addr(c, AUDIO_SLIDER, &[this + 9732, 471, this + 9596, 1]);
    call_addr(c, AUDIO_SLIDER, &[this + 9788, 472, this + 9612, 2]);
    call_addr(c, AUDIO_SLIDER, &[this + 9844, 473, this + 9628, 3]);
    call_addr(c, AUDIO_SLIDER, &[this + 9900, 474, this + 9644, 4]);
    call_addr(c, FADE, &[this + 9956, 475, this + 9660]);
    c.m.w32(this + 9980, 0x8001_0D88);
    call_addr(c, SLIDING_MENU, &[this + 10000, 476, 350, 43, 0xFFFF_FF9A, 13, 0]);
    c.m.w32(this + 10024, 0x8001_0DE0);
    call_addr(c, SONG_MENU, &[this + 10072, 0x1000, 0, 0, 0, 0, 0, 0, this + 9236, this + 9280, this + 9324, this + 9368, this + 9412, 0]);
    let screen = c.m.r32(0x8005_2D78);
    call_addr(c, OPTIONS_MENU, &[this + 10208, 0x1010, screen, 0, 0, 0, 0x264, 38, 10, this + 9676, this + 9732, this + 9788, this + 9844, this + 9900, this + 9956, this + 10000, 0]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10336, 0x8005_1E24, 0x8011_5BD8, fe_app + 557]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10356, 0x8005_1E30, 0x8011_5BDC, fe_app + 557]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10376, 0x8005_1E38, 0x8011_5BDE, fe_app + 557]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10396, 0x8005_1DFC, 0x8011_5BE0, fe_app + 557]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10416, 0x8005_1DFC, 0x8011_5BE2, fe_app + 557]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10436, 0x8005_1DFC, 0x8011_5BE4, fe_app + 557]);
    call_addr(c, ITERATOR, &[this + 10456, 0x8005_1E40, 0x8011_5C4E]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10472, 0x8005_1E48, 0x8011_5C4F, fe_app + 557]);
    call_addr(c, FADE, &[this + 10492, 480, this + 10336]);
    c.m.w32(this + 10516, 0x8001_0D88);
    call_addr(c, FADE, &[this + 10536, 482, this + 10356]);
    c.m.w32(this + 10560, 0x8001_0D88);
    call_addr(c, FADE, &[this + 10580, 483, this + 10376]);
    c.m.w32(this + 10604, 0x8001_0D88);
    call_addr(c, FADE, &[this + 10624, 484, this + 10396]);
    c.m.w32(this + 10648, 0x8001_0D30);
    call_addr(c, FADE, &[this + 10672, 487, this + 10416]);
    c.m.w32(this + 10696, 0x8001_0D30);
    call_addr(c, FADE, &[this + 10720, 488, this + 10436]);
    c.m.w32(this + 10744, 0x8001_0D30);
    call_addr(c, FADE, &[this + 10768, 485, this + 10456]);
    c.m.w32(this + 10792, 0x8001_0D88);
    call_addr(c, FADE, &[this + 10812, 486, this + 10472]);
    let screen = c.m.r32(0x8005_2D68);
    c.m.w32(this + 10836, 0x8001_0D88);
    call_addr(c, OPTIONS_MENU, &[this + 10856, 0x1020, screen, 0, 0, 0, 478, 1, 10, this + 10492, this + 10536, this + 10580, this + 10624, this + 10768, this + 10812, this + 10672, this + 10720, 0]);
    call_addr(c, INDEXED_ITERATOR, &[this + 10984, 0x8005_1E58, 0x8011_5C1A, fe_app + 557]);
    call_addr(c, FADE, &[this + 11004, 0x20A, this + 10984]);
    c.m.w32(this + 11028, 0x8001_0C18);
    call_addr(c, SLIDING_MENU, &[this + 11048, 0x20B, 172, 72, 0, 13, 1]);
    let screen = c.m.r32(0x8005_2D60);
    call_addr(c, OPTIONS_MENU, &[this + 11116, 0x1020, screen, 0, 0, 0, 0x209, 0, 10, this + 11004, this + 11048, 0]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11244, 0, 127, 0x8011_5C11, fe_app + 557]);
    call_addr(c, INSIDE_SLIDER, &[this + 11264, 0x20F, this + 11244]);
    c.m.w32(this + 11288, 0x8001_0A08);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11304, 0, 127, 0x8011_5C13, fe_app + 557]);
    call_addr(c, INSIDE_SLIDER, &[this + 11324, 0x210, this + 11304]);
    c.m.w32(this + 11348, 0x8001_0A08);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11364, 0, 127, 0x8011_5C09, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11384, 0x212, this + 11364, 0]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11432, 0, 127, 0x8011_5C0B, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11452, 0x214, this + 11432, 1]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11500, 0, 127, 0x8011_5C0D, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11520, 0x211, this + 11500, 2]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11568, 0, 127, 0x8011_5C0F, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11588, 0x213, this + 11568, 3]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11636, 0, 127, 0x8011_5C01, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11656, 0x215, this + 11636, 0]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11704, 0, 127, 0x8011_5C03, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11724, 0x216, this + 11704, 1]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11772, 0, 255, 0x8011_5C05, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11792, 0x217, this + 11772, 2]);
    call_addr(c, RANGE_INDEXED_ITERATOR, &[this + 11840, 0, 255, 0x8011_5C07, fe_app + 557]);
    call_addr(c, TWO_WAY_SLIDER, &[this + 11860, 0x218, this + 11840, 3]);
    call_addr(c, INSIDE_BOX_MENU, &[this + 11908, 0x1000, 0, 0, 0, 0, 0, this + 11264, this + 11324, 0]);
    call_addr(c, INSIDE_BOX_MENU, &[this + 12024, 0x1000, 0, 0, 0, 0, 0, this + 11384, this + 11452, this + 11520, this + 11588, 0]);
    call_addr(c, INSIDE_BOX_MENU, &[this + 12140, 0x1000, 0, 0, 0, 0, 0, this + 11264, this + 11324, this + 11452, this + 11588, 0]);
    call_addr(c, INSIDE_BOX_MENU, &[this + 12256, 0x1000, 0, 0, 0, 0, 0, this + 11656, this + 11724, this + 11792, this + 11860, 0]);
    call_addr(c, BUTTON, &[this + 12372, 0x287, 0, SAVE_GAME]);
    c.m.w32(this + 12396, 0x8001_0A60);
    call_addr(c, BUTTON, &[this + 12416, 0x288, 0, LOAD_GAME]);
    let memcard_screen = c.m.r32(MEMCARD_SCREEN);
    c.m.w32(this + 12440, 0x8001_0A60);
    call_addr(c, OPTIONS_MENU, &[this + 12460, 0x1020, memcard_screen, 0, 0, 0, 0xFFFF_FFFF, 46, 10, this + 12416, this + 12372, 0]);
    call_addr(c, USER_NAME, &[this + 12588, 505]);
    call_addr(c, USER_NAME, &[this + 12728, 505]);
    call_addr(c, USER_NAME, &[this + 12868, 505]);
    let name_screen = c.m.r32(NAME_SCREEN);
    call_addr(c, OPTIONS_MENU, &[this + 13008, 0x1120, name_screen, 0, 0, 0, 0xFFFF_FFFF, 32, 10, this + 12588, 0]);
    let screen = c.m.r32(0x8005_24F0);
    call_addr(c, ITERATOR, &[this + 13136, screen + 216, screen + 204]);
    call_addr(c, NFS4_CHOICE, &[this + 13152, 95, this + 13136, 0xFFFF_FFFF, 0]);
    let screen = c.m.r32(0x8005_24F0);
    c.m.w32(this + 13176, 0x8001_14D8);
    call_addr(c, BOTTOM_MENU, &[this + 13192, 0x4010, screen, 0, 0, 0, 0xFFFF_FFFF, this + 13152, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 13316, 91, 0, PINK_SLIPS_CONTINUE, 0, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 13360, 157, 0, EXIT_PINK_SLIPS_EARLY, 0, 10]);
    let screen = c.m.r32(0x8005_2300);
    call_addr(c, NFS4_MENU, &[this + 13404, 0x1004, screen, 0, 0, 0, 0x2F5, this + 13316, this + 13360, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 13528, 91, this + 4336, 0, 0, 10]);
    call_addr(c, NFS4_BUTTON, &[this + 13572, 156, 0, EXIT_TOURNEY, 0, 10]);
    let standings3_screen = c.m.r32(STANDINGS3_SCREEN);
    call_addr(c, NFS4_MENU, &[this + 13616, 0x1004, standings3_screen, 0, 0, 0, 0x2F5, this + 13528, this + 13572, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 13740, 91, this + 13908, 0, 0, 10]);
    let standings_screen = c.m.r32(STANDINGS_SCREEN);
    call_addr(c, NFS4_MENU, &[this + 13784, 0x1004, standings_screen, 0, 0, 0, 0x2F5, this + 13740, 0]);
    let screen = c.m.r32(0x8005_2DD8);
    call_addr(c, BLANK_MENU, &[this + 13908, 0x8000, screen, 0, 0, TIER_FINISHED, 0xFFFF_FFFF]);
    call_addr(c, OPTIONS_MENU, &[this + 14032, 0x1040, name_screen, 0, 0, FINISHED_PLAYER1_GET_NAME, 0xFFFF_FFFF, 32, 10, this + 12728, 0]);
    call_addr(c, OPTIONS_MENU, &[this + 14160, 0x1080, name_screen, 0, 0, FINISHED_PLAYER2_GET_NAME, 0xFFFF_FFFF, 32, 10, this + 12868, 0]);
    call_addr(c, NFS4_BUTTON, &[this + 14288, 91, 0, POST_GAME_MENU, 0xFFFF_FFFF, 0xFFFF_FFFF]);
    let screen = c.m.r32(0x8005_2560);
    c.m.w32(this + 14312, 0x8001_1480);
    call_addr(c, OPTIONS_MENU, &[this + 14332, 0x0002_1004, screen, 0, 0, 0, 213, 1, 10, this + 14288, 0]);
    let pink_standings = c.m.r32(PINK_STANDINGS);
    call_addr(c, BLANK_MENU, &[this + 14460, 0x8000, pink_standings, 0, 0, AWARD_PINK_SLIPS_CAR, 0xFFFF_FFFF]);
    let screen = c.m.r32(0x8005_2DDC);
    call_addr(c, BLANK_MENU, &[this + 14584, 0x8000, screen, 0, 0, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME, 0xFFFF_FFFF]);
    let screen = c.m.r32(0x8005_2DE0);
    call_addr(c, BLANK_MENU, &[this + 14708, 0x8000, screen, 0, 0, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU_AND_SAVE_GAME, 0xFFFF_FFFF]);
    call_addr(c, BLANK_MENU, &[this + 14832, 0x1010, main_screen, 0, 0, 0, 0xFFFF_FFFF]);
    call_addr(c, BUTTON, &[this + 14956, 0x28B, 0, TRANSITION_FROM_POST_GAME_TO_MAIN_MENU]);
    c.m.w32(this + 14980, 0x8001_0A60);
    call_addr(c, OPTIONS_MENU, &[this + 15000, 0x1040, memcard_screen, 0, 0, 0, 0xFFFF_FFFF, 46, 10, this + 14956, this + 12372, 0]);
    c.m.w32(this + 5912, this + 6072);
    c.m.w32(this + 6368, this + 6488);
    c.m.w32(this + 6812, this + 6932);
    call_addr(c, SET_DIMENSIONS, &[this + 9676, 0, 0, 120, 5]);
    call_addr(c, SET_DIMENSIONS, &[this + 9732, 0, 0, 120, 5]);
    call_addr(c, SET_DIMENSIONS, &[this + 9788, 0, 0, 120, 5]);
    call_addr(c, SET_DIMENSIONS, &[this + 9844, 0, 0, 120, 5]);
    call_addr(c, SET_DIMENSIONS, &[this + 9900, 0, 0, 120, 5]);
    let flags = c.m.r32(this + 13740);
    c.m.w32(this + 6628, 32);
    c.m.w32(this + 4828, 2);
    c.m.w32(this + 7284, 1);
    c.m.w32(this + 7636, 2);
    c.m.w16(this + 10308, 0);
    c.m.w16(this + 10956, 0);
    c.m.w16(this + 11216, 1);
    c.m.w32(this + 13740, flags | 64);
    c.m.w16(this + 12560, 0);
    c.m.w16(this + 13108, 0);
    c.m.w16(this + 4264, 1);
    c.m.w16(this + 612, 0);
    this
}

pub fn abi_global_menu_defs(c: &mut Cpu) {
    c.r[2] = global_menu_defs(c, c.r[4]);
}

/// The members destroyed, the last constructed first: (offset, destructor).
const MEMBERS: [(u32, u32); 249] = [
    (15000, 0x8001_C6A4), (14956, 0x8002_539C), (14832, 0x8001_BA5C), (14708, 0x8001_BA5C), (14584, 0x8001_BA5C),
    (14460, 0x8001_BA5C), (14332, 0x8001_C6A4), (14288, 0x8001_A674), (14160, 0x8001_C6A4), (14032, 0x8001_C6A4),
    (13908, 0x8001_BA5C), (13784, 0x8001_B2B8), (13740, 0x8001_A674), (13616, 0x8001_B2B8), (13572, 0x8001_A674),
    (13528, 0x8001_A674), (13404, 0x8001_B2B8), (13360, 0x8001_A674), (13316, 0x8001_A674), (13192, 0x8001_B910),
    (13152, 0x8001_AA00), (13136, 0x8002_3E14), (13008, 0x8001_C6A4), (12868, 0x8002_4784), (12728, 0x8002_4784),
    (12588, 0x8002_4784), (12460, 0x8001_C6A4), (12416, 0x8002_539C), (12372, 0x8002_539C), (12256, 0x8001_CF64),
    (12140, 0x8001_CF64), (12024, 0x8001_CF64), (11908, 0x8001_CF64), (11860, 0x8001_F628), (11840, 0x8002_4690),
    (11792, 0x8001_F628), (11772, 0x8002_4690), (11724, 0x8001_F628), (11704, 0x8002_4690), (11656, 0x8001_F628),
    (11636, 0x8002_4690), (11588, 0x8001_F628), (11568, 0x8002_4690), (11520, 0x8001_F628), (11500, 0x8002_4690),
    (11452, 0x8001_F628), (11432, 0x8002_4690), (11384, 0x8001_F628), (11364, 0x8002_4690), (11324, 0x8001_F428),
    (11304, 0x8002_4690), (11264, 0x8001_F428), (11244, 0x8002_4690), (11116, 0x8001_C6A4), (11048, 0x8001_D368),
    (11004, 0x8002_4974), (10984, 0x8002_3FC4), (10856, 0x8001_C6A4), (10812, 0x8002_4974), (10768, 0x8002_4974),
    (10720, 0x8002_4974), (10672, 0x8002_4974), (10624, 0x8002_4974), (10580, 0x8002_4974), (10536, 0x8002_4974),
    (10492, 0x8002_4974), (10472, 0x8002_3FC4), (10456, 0x8002_3E14), (10436, 0x8002_3FC4), (10416, 0x8002_3FC4),
    (10396, 0x8002_3FC4), (10376, 0x8002_3FC4), (10356, 0x8002_3FC4), (10336, 0x8002_3FC4), (10208, 0x8001_C6A4),
    (10072, 0x8001_EB10), (10000, 0x8001_D368), (9956, 0x8002_4974), (9900, 0x8001_E6A8), (9844, 0x8001_E6A8),
    (9788, 0x8001_E6A8), (9732, 0x8001_E6A8), (9676, 0x8001_E6A8), (9660, 0x8002_3E14), (9644, 0x8002_45B0),
    (9628, 0x8002_45B0), (9612, 0x8002_45B0), (9596, 0x8002_45B0), (9580, 0x8002_45B0), (9456, 0x8001_B2B8),
    (9412, 0x8001_A674), (9368, 0x8001_A674), (9324, 0x8001_A674), (9280, 0x8001_A674), (9236, 0x8001_A674),
    (9192, 0x8001_A674), (9068, 0x8001_BA5C), (8936, 0x8001_BC24), (8804, 0x8001_BC24), (8672, 0x8001_BC24),
    (8540, 0x8001_BC24), (8408, 0x8001_BC24), (8376, 0x8002_4974), (8344, 0x8002_4974), (8312, 0x8002_4974),
    (8280, 0x8002_4974), (8248, 0x8002_4974), (8216, 0x8002_4974), (8184, 0x8002_4974), (8168, 0x8002_3E14),
    (8152, 0x8002_3E14), (8132, 0x8002_3FC4), (8112, 0x8002_3FC4), (7988, 0x8001_B2B8), (7944, 0x8001_A674),
    (7900, 0x8001_A674), (7856, 0x8001_A674), (7732, 0x8001_B2B8), (7688, 0x8001_A674), (7648, 0x8001_AA00),
    (7620, 0x8001_7A70), (7496, 0x8001_B2B8), (7452, 0x8001_A674), (7408, 0x8001_A674), (7368, 0x8001_AA00),
    (7328, 0x8001_AA00), (7296, 0x8001_8660), (7268, 0x8001_7A70), (7144, 0x8001_B2B8), (7100, 0x8001_A674),
    (7056, 0x8001_A674), (6932, 0x8001_B808), (6892, 0x8001_AA00), (6848, 0x8001_A674), (6724, 0x8001_B808),
    (6684, 0x8001_AA00), (6640, 0x8001_A674), (6612, 0x8001_7A70), (6488, 0x8001_B808), (6448, 0x8001_AA00),
    (6404, 0x8001_A674), (6280, 0x8001_B808), (6240, 0x8001_AA00), (6196, 0x8001_A674), (6072, 0x8001_B808),
    (6032, 0x8001_AA00), (5992, 0x8001_AA00), (5948, 0x8001_A674), (5824, 0x8001_B808), (5784, 0x8001_AA00),
    (5744, 0x8001_AA00), (5700, 0x8001_A674), (5576, 0x8001_B2B8), (5452, 0x8001_B2B8), (5412, 0x8001_AA00),
    (5368, 0x8001_A674), (5328, 0x8001_AA00), (5288, 0x8001_AA00), (5244, 0x8001_A674), (5216, 0x8001_7A70),
    (5092, 0x8001_B2B8), (4968, 0x8001_B2B8), (4924, 0x8001_A674), (4880, 0x8001_A674), (4840, 0x8001_AA00),
    (4812, 0x8001_7A70), (4688, 0x8001_B2B8), (4644, 0x8001_A674), (4604, 0x8001_AA00), (4564, 0x8001_AA00),
    (4520, 0x8001_A674), (4488, 0x8001_8660), (4460, 0x8001_7A70), (4336, 0x8001_B2B8), (4292, 0x8001_A674),
    (4164, 0x8001_C6A4), (4120, 0x8001_A674), (3988, 0x8001_BC24), (3952, 0x8002_4974), (3916, 0x8002_4974),
    (3880, 0x8002_4974), (3844, 0x8002_4974), (3808, 0x8002_4974), (3776, 0x8002_4974), (3744, 0x8002_4974),
    (3728, 0x8002_3E14), (3708, 0x8002_3FC4), (3688, 0x8002_3FC4), (3668, 0x8002_3FC4), (3648, 0x8002_3FC4),
    (3628, 0x8002_3FC4), (3608, 0x8002_3FC4), (3484, 0x8001_B2B8), (3360, 0x8001_B2B8), (3316, 0x8001_A674),
    (3276, 0x8001_AA00), (3232, 0x8001_A674), (3208, 0x8002_1F0C), (3084, 0x8001_B2B8), (2960, 0x8001_B2B8),
    (2920, 0x8001_AA00), (2880, 0x8001_AA00), (2840, 0x8001_AA00), (2800, 0x8001_AA00), (2760, 0x8001_AA00),
    (2716, 0x8001_A674), (2592, 0x8001_B2B8), (2548, 0x8001_A674), (2504, 0x8001_A674), (2460, 0x8001_A674),
    (2336, 0x8001_B2B8), (2292, 0x8001_A674), (2248, 0x8001_A674), (2204, 0x8001_A674), (2160, 0x8001_A674),
    (2036, 0x8001_B2B8), (1996, 0x8001_AA00), (1952, 0x8001_A674), (1932, 0x8003_3F28), (1808, 0x8001_B2B8),
    (1768, 0x8001_AA00), (1724, 0x8001_A674), (1704, 0x8003_3F28), (1580, 0x8001_B2B8), (1536, 0x8001_A674),
    (1492, 0x8001_A674), (1368, 0x8001_B2B8), (1324, 0x8001_A674), (1280, 0x8001_A674), (1236, 0x8001_A674),
    (1112, 0x8001_B2B8), (1068, 0x8001_A674), (1024, 0x8001_A674), (980, 0x8001_A674), (936, 0x8001_A674),
    (892, 0x8001_A674), (768, 0x8001_B2B8), (724, 0x8001_A674), (680, 0x8001_A674), (636, 0x8001_A674),
    (512, 0x8001_BA5C), (388, 0x8001_B2B8), (344, 0x8001_A674), (300, 0x8001_A674), (176, 0x8001_B2B8),
    (132, 0x8001_A674), (88, 0x8001_A674), (44, 0x8001_A674), (0, 0x8001_A674),
];

/// `operator delete`.
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;

/// `tGlobalMenuDefs::~tGlobalMenuDefs` (FEMENUDEFS.CPP, 0x80031720): every member destroyed
/// (each told 2: not to free itself), the last constructed first; freed if `flags` & 1.
pub fn global_menu_defs_destructor(c: &mut Cpu, this: u32, flags: u32) {
    for &(at, dtor) in &MEMBERS {
        call_addr(c, dtor, &[this + at, 2]);
    }
    if flags & 1 != 0 {
        call(c, DELETE, &[this]);
    }
}

pub fn abi_global_menu_defs_destructor(c: &mut Cpu) {
    global_menu_defs_destructor(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

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
        let cars = m.r32(CAR_MANAGER.0);
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

}
