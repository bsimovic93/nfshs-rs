//! FEMEMCARD.CPP: the front end's saves on the memory card: the save's data (`tMemCardData`,
//! 5292 bytes: +0 its size, +4 the bonuses, +8 the front end's structure, +1112 the cars,
//! +1336 the tracks, +1352 the tournaments, +1528 the records, +5268 a random id, +5272 five
//! checksums), the card layer started and stopped around a save (the pads' timer off), the
//! confirm dialogs, saving and loading a player's game and the pink slips' cars, with their
//! messages.
//!
//! Written as FEMENU.CPP is (see there). The dialogs' `Display` and `Hide`, the application's
//! frame and libmcrd are the host's or hooked (called by address); FRONT.CPP's functions too
//! until it is rewritten.

use crate::call::{call, call_addr, with_buffer};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PAD_INFO, PATHS};
use crate::memcard;
use crate::types::{CarManager, DialogInteractive, DialogYesNo, FEApplication, Screen};
use ps1rt::Cpu;

const FE: u32 = FRONT_END;
/// The player whose card is in use, the card layer's state (1 once the card was reached),
/// whether a save is going on, the layer started; the save's name; the pads' timer handler.
const CARD_PLAYER: u32 = 0x8005_1EF0;
const CARD_DONE: u32 = 0x8005_1F00;
const CARD_ACTIVE: u32 = 0x8005_1F04;
const INITED: u32 = 0x8005_1F08;
const SAVE_NAME: u32 = 0x8005_2FF8;
const PAD_TIMER: u32 = 0x8008_C428;
/// The game's code (`"SLUS-00826"`), the file's name (`"NFS4"`), `"%s%s"`, `"%szMem.psh"`;
/// the errors' words by result.
const GAME_CODE: u32 = 0x8005_1EF4;
const FILE_NAME: u32 = 0x8001_1684;
const TWO_STRINGS: u32 = 0x8001_168C;
const ICON_FILE: u32 = 0x8001_1694;
const ERROR_WORDS: u32 = 0x8005_1F0C;
/// The managers, the records, the language, the effects' level; the front end's use; the
/// memory card screen (its +1440 redraw flag).
const CAR_MANAGER: CarManager = crate::globals::CAR_MANAGER;
const TRACK_MANAGER: u32 = 0x8011_5D08;
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const TRACK_RECORDS: u32 = 0x8011_604C;
const LANGUAGE: u32 = FE + 832;
const SFX_LEVEL: u32 = 0x8013_D948;
const USE_FRONT_END: u32 = 0x8013_E55C;
const USE_FRONT_END2: u32 = 0x8013_E560;
const MEMCARD_SCREEN: u32 = 0x8005_2D80;
const CD_PATH: u32 = PATHS + 0x80;
/// The dialogs' vtables: the yes-or-no's, the base's, the message's, the message box's.
const YES_NO_VT: u32 = 0x8001_0308;
const DIALOG_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const MESSAGE_BOX_VT: u32 = 0x8001_0428;
/// The callbacks the card layer is given.
const FORMAT_CONFIRM: u32 = 0x8003_4750;
const OVERWRITE_ALWAYS_YES: u32 = 0x8003_4748;
const OVERWRITE_CONFIRM: u32 = 0x8003_471C;
const LOADING_REDRAW_PROC: u32 = 0x8003_4784;
const LOADING_PROC: u32 = 0x8003_477C;
const SAVING_PROC: u32 = 0x8003_47AC;

const CHECKSUM: fn(&mut Cpu) = nfs4rc::f_800f6aa8;
const BLOCKCLEAR: fn(&mut Cpu) = nfs4rc::f_800f24d4;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const ADD_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb944;
const DEL_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb9a4;
const TIMED_WAIT: fn(&mut Cpu) = nfs4rc::f_800e8e70;
const LOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f2970;
const SHAPE_POINTER: fn(&mut Cpu) = nfs4rc::f_800f17ec;
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
/// The pads stopped and started again; the screen's buffers swapped (a frame).
const PAD_STOP: fn(&mut Cpu) = nfs4rc::f_800e4d60;
const PAD_INIT: fn(&mut Cpu) = nfs4rc::f_800e4cf4;
const FRAME: fn(&mut Cpu) = nfs4rc::f_800dcea4;
/// The host's: `systemtask`, `VSync`, the application's frame (hooked).
const SYSTEM_TASK: u32 = 0x800E_77F0;
const VSYNC: u32 = 0x800F_2C78;
const REDRAW: u32 = 0x8001_3894;
/// `tDialog::Display`, `Hide` (hooked); FRONT.CPP's `PlayerNameExist`, `PlayerName`,
/// `SetPads`, `ResetPSXController`.
const DISPLAY: u32 = 0x8001_8980;
const HIDE: u32 = 0x8001_8A44;
const PLAYER_NAME_EXIST: u32 = 0x8002_BA50;
const PLAYER_NAME: u32 = 0x8002_BA7C;
const SET_PADS: u32 = 0x8002_752C;
const RESET_CONTROLLER: u32 = 0x8002_7104;

/// The save data's parts: its size, the bonuses, the front end's structure, the cars, the
/// tracks, the tournaments, the records, a random id, the checksums.
const DATA_SIZE: u32 = 5292;
const BONUSES: u32 = 4;
const FE_DATA: u32 = 8;
const CARS: u32 = 1112;
const TRACKS: u32 = 1336;
const TOURNAMENTS: u32 = 1352;
const RECORDS: u32 = 1528;
const SAVE_ID: u32 = 5268;
const CHECKSUMS: u32 = 5272;
/// The card layer's file (`MCRD_FILEINFO`, 44 bytes) and options (36 bytes); the file's size, a
/// new file.
const FILE_INFO_SIZE: u32 = 44;
const OPTIONS_SIZE: u32 = 36;
const FILE_SIZE: u32 = 5376;
const NEW_FILE: u32 = 0x200;
/// A dialog: +96 its vtable, +100 its player, +132 shown, +144 its text; a message box's size,
/// a yes-or-no's; the application's message (+568) and message box (+720); a message's room.
const VPTR: u32 = 96;
const DIALOG_PLAYER: u32 = 100;
const SHOWN: u32 = 132;
const TEXT: u32 = 144;
const DIALOG_SIZE: u32 = 152;
const YES_NO_SIZE: u32 = 168;
const APP_MESSAGE: u32 = 568;
const APP_MESSAGE_BOX: u32 = 720;
const TEXT_SIZE: u32 = 504;
/// The words (each the first player's; the second's follows): saving, saved, loaded, the card
/// full, unformatted, the save failed, the load failed, the card pulled; the overwrite and
/// format questions; "no".
const SAVING: u32 = 631;
const SAVED: u32 = 652;
const LOADED: u32 = 654;
const CARD_FULL: u32 = 670;
const UNFORMATTED: u32 = 672;
const SAVE_FAILED: u32 = 806;
const LOAD_FAILED: u32 = 810;
const PULLED: u32 = 812;
const OVERWRITE_WORD: u32 = 804;
const FORMAT_WORD: u32 = 808;
const NO_WORD: u32 = 659;
/// The front end's: the race type (+4), the pink slips' money (+76, by player), the players'
/// names (+0x364, 8 bytes each), the effects' level (+834), the pads' modes (+866), the pink
/// slips' save ids (+1084, by player), the pads' vibration (+1096, by player).
const RACE_TYPE: u32 = 4;
const PINKS_MONEY: u32 = 76;
const PLAYER_NAMES: u32 = 0x364;
const FE_SFX_LEVEL: u32 = 834;
const PAD_MODES: u32 = 866;
const PINKS_IDS: u32 = 1084;
const VIBRATION: u32 = FE + 1096;
/// A DualShock (a pad's kind); the memory card screen's redraw flag.
const DUAL_SHOCK: u8 = 0x73;
const SCREEN_REDRAW: u32 = 1440;

/// The save's card for player `p`: 1 or 5.
fn card_of(p: u32) -> u32 {
    ((p << 16) as i32 >> 14) as u32 | 1
}

/// A word of the application's.
fn app_word(c: &mut Cpu, at: u32) -> u32 {
    let app = c.m.r32(FE_APP);
    c.m.r32(app + at)
}

/// The application's frame (the host's hook).
fn redraw(c: &mut Cpu) {
    let app = c.m.r32(FE_APP);
    call_addr(c, REDRAW, &[app]);
}

/// `tDialog::Display` and `Hide` (hooked: the host notes the dialog).
fn show(c: &mut Cpu, d: u32) {
    call_addr(c, DISPLAY, &[d]);
}

fn hide(c: &mut Cpu, d: u32) {
    call_addr(c, HIDE, &[d]);
}

/// A message box built in place at `d` (a screen first): the dialog's, the message's and the
/// message box's fields in turn (288 wide, nothing shown, for either player).
fn message_box(c: &mut Cpu, d: u32) {
    crate::fescreen::screen(c, Screen(d));
    let m = &mut c.m;
    m.w32(d + VPTR, DIALOG_VT);
    m.w32(d + 112, 0);
    for at in [110, 130, 126, 124, 108, 106, 104, 102] {
        m.w16(d + at, 0);
    }
    m.w16(d + 128, 288);
    m.w16(d + DIALOG_PLAYER, 0xFFFF);
    m.w32(d + VPTR, MESSAGE_VT);
    m.w16(d + 136, 0);
    m.w32(d + 120, 0);
    m.w32(d + 148, 0);
    m.w32(d + SHOWN, 0);
    m.w32(d + 120, 0);
    m.w32(d + 140, 128);
    m.w32(d + VPTR, MESSAGE_BOX_VT);
}

/// A dialog destroyed (its screen's destructor; not freed).
fn destroy(c: &mut Cpu, d: u32) {
    crate::fescreen::screen_destructor(c, Screen(d), 2);
}

/// Frames drawn until the dialog's word at `flag` is 1, and one more.
fn wait_shown(c: &mut Cpu, flag: impl Fn(&mut Cpu) -> u32) {
    while flag(c) != 1 {
        redraw(c);
    }
    redraw(c);
}

/// A message box shown with `word` (placed down the screen) until it's drawn.
fn show_message(c: &mut Cpu, d: u32, word: u32, place: bool) {
    let text = call(c, TEXT_WORD, &[word]);
    let m = &mut c.m;
    m.w32(d + TEXT, text);
    if place {
        m.w16(d + 124, 0);
        m.w16(d + 126, 50);
    }
    show(c, d);
    wait_shown(c, |c| c.m.r32(d + SHOWN));
}

/// The card's events polled until `event` (or another than `event` when `until_not`): a frame
/// swapped and a vertical blank between tries.
fn wait_event(c: &mut Cpu, card: u32, event: u32, until_not: bool) {
    loop {
        let v = memcard::handlecardevents(c, card);
        if (v == event) != until_not {
            return;
        }
        call(c, FRAME, &[]);
        call_addr(c, VSYNC, &[0]);
    }
}

/// One poll of the card's events in a save's loop: a frame, the events, the system's task,
/// a vertical blank. The event.
fn poll(c: &mut Cpu, card: u32) -> u32 {
    call(c, FRAME, &[]);
    let v = memcard::handlecardevents(c, card);
    call_addr(c, SYSTEM_TASK, &[0]);
    call_addr(c, VSYNC, &[0]);
    v
}

/// The save data's parts (the front end's structure, the cars, the tracks, the tournaments, the
/// records) with a checksum each.
const PARTS: [(u32, u32); 5] = [(FE_DATA, 1104), (CARS, 224), (TRACKS, 16), (TOURNAMENTS, 176), (RECORDS, 3740)];

/// `ChecksumData` (FEMEMCARD.CPP, 0x800343d0): the save data's size and its five checksums.
pub fn checksum_data(c: &mut Cpu, d: u32) {
    c.m.w32(d, DATA_SIZE);
    for (k, (at, len)) in PARTS.into_iter().enumerate() {
        let v = call(c, CHECKSUM, &[d + at, len]);
        c.m.w32(d + CHECKSUMS + 4 * k as u32, v);
    }
}

pub fn abi_checksum_data(c: &mut Cpu) {
    checksum_data(c, c.r[4]);
}

/// `VerifySuccessfulRead` (FEMEMCARD.CPP, 0x80034448): whether the read save data's size and
/// checksums are right.
pub fn verify_successful_read(c: &mut Cpu, d: u32) -> bool {
    let mut ok = true;
    for (k, (at, len)) in PARTS.into_iter().enumerate() {
        let v = call(c, CHECKSUM, &[d + at, len]);
        ok &= c.m.r32(d + CHECKSUMS + 4 * k as u32) == v;
    }
    ok && c.m.r32(d) == DATA_SIZE
}

pub fn abi_verify_successful_read(c: &mut Cpu) {
    c.r[2] = verify_successful_read(c, c.r[4]) as u32;
}

/// `MakeWayForMemoryCard` (FEMEMCARD.CPP, 0x80034520): the pads' timer stopped.
pub fn make_way(c: &mut Cpu) {
    call(c, DEL_TIMER, &[PAD_TIMER]);
}

pub fn abi_make_way(c: &mut Cpu) {
    make_way(c);
}

/// `BringThatBeatBack` (FEMEMCARD.CPP, 0x80034544): the pads' timer back.
pub fn bring_back(c: &mut Cpu) {
    call(c, ADD_TIMER, &[PAD_TIMER]);
}

pub fn abi_bring_back(c: &mut Cpu) {
    bring_back(c);
}

/// `Confirm` (FEMEMCARD.CPP, 0x80034568): a yes-or-no with the text `word` and the answers
/// `answers` (the application's message box hidden meanwhile, shown again after): the answer
/// (-1 the card pulled: told so). The card reached unless answered yes.
pub fn confirm(c: &mut Cpu, word: u32, answers: u32) -> u32 {
    bring_back(c);
    with_buffer(c, YES_NO_SIZE, |c, d| {
        crate::fedialog::yes_no(c, DialogYesNo(d));
        c.m.w32(d + VPTR, YES_NO_VT);
        let app = c.m.r32(FE_APP);
        let shown = c.m.r32(app + APP_MESSAGE_BOX + 112) != 0;
        if shown {
            hide(c, app + APP_MESSAGE_BOX);
        }
        let text = call(c, TEXT_WORD, &[word]);
        let m = &mut c.m;
        m.w32(d + TEXT, text);
        m.w32(d + 164, NO_WORD);
        m.w32(d + 160, answers);
        m.w16(d + 136, 0);
        if m.r8(LANGUAGE) == 3 {
            m.w16(d + 124, 0);
            m.w16(d + 126, 10);
        }
        let v = crate::fedialog::run(c, DialogInteractive(d)) as u16 as i16 as i32 as u32;
        if v == u32::MAX {
            // the card pulled
            let app = c.m.r32(FE_APP);
            let player = c.m.r32(CARD_PLAYER);
            let text = call(c, TEXT_WORD, &[player.wrapping_add(PULLED)]);
            let app2 = c.m.r32(FE_APP);
            c.m.w32(app + APP_MESSAGE + TEXT, text);
            show(c, app2 + APP_MESSAGE);
            wait_shown(c, |c| app_word(c, APP_MESSAGE + SHOWN));
            c.m.w32(CARD_DONE, 1);
        } else if v == 0 {
            c.m.w32(CARD_DONE, 1);
        } else if shown {
            let app = c.m.r32(FE_APP);
            show(c, app + APP_MESSAGE_BOX);
            while app_word(c, APP_MESSAGE_BOX + SHOWN) != 1 {
                redraw(c);
            }
        }
        redraw(c);
        make_way(c);
        destroy(c, d);
        v
    })
}

pub fn abi_confirm(c: &mut Cpu) {
    c.r[2] = confirm(c, c.r[4], c.r[5]);
}

/// `OverwriteConfirm` (FEMEMCARD.CPP, 0x8003471c): the card player's overwrite question.
pub fn abi_overwrite_confirm(c: &mut Cpu) {
    let word = c.m.r32(CARD_PLAYER).wrapping_add(OVERWRITE_WORD);
    c.r[2] = confirm(c, word, 656);
}

/// `FormatConfirm` (FEMEMCARD.CPP, 0x80034750): the card player's format question.
pub fn abi_format_confirm(c: &mut Cpu) {
    let word = c.m.r32(CARD_PLAYER).wrapping_add(FORMAT_WORD);
    c.r[2] = confirm(c, word, 657);
}

/// `OverwriteAlwaysYes` (FEMEMCARD.CPP, 0x80034748): yes.
pub fn abi_overwrite_always_yes(c: &mut Cpu) {
    c.r[2] = 1;
}

/// `LoadingProc` (FEMEMCARD.CPP, 0x8003477c): nothing.
pub fn abi_loading_proc(_c: &mut Cpu) {}

/// `LoadingRedrawProc` and `SavingProc` (FEMEMCARD.CPP, 0x80034784, 0x800347ac): a frame.
pub fn abi_loading_redraw_proc(c: &mut Cpu) {
    redraw(c);
}

pub fn abi_saving_proc(c: &mut Cpu) {
    redraw(c);
}

/// Spins until `ticks` passed since now (the original's busy wait on the clock).
fn spin(c: &mut Cpu, ticks: i32) {
    let start = c.m.r32(GAME_TICKS);
    while (c.m.r32(GAME_TICKS).wrapping_sub(start) as i32) < ticks {
        c.tick();
    }
}

/// `Init_Memcard` (FEMEMCARD.CPP, 0x800347d4): once, the card layer started: the music
/// stopped (in the front end), the pads stopped for a second, their timer off, the layer
/// initialized with the game's code, the 512-by-256 icons' place, the confirm callbacks (the
/// overwrite always yes when `always_yes`) and the wait callbacks (a frame drawn when
/// `redraw_cb`); the pads back.
pub fn init_memcard(c: &mut Cpu, redraw_cb: bool, always_yes: bool) {
    if c.m.r32(INITED) != 0 {
        return;
    }
    let front_end = c.m.r32(USE_FRONT_END);
    c.m.w32(INITED, 1);
    if front_end != 0 {
        call(c, STOP_SONG, &[0]);
        call(c, TIMED_WAIT, &[5]);
    }
    call(c, PAD_STOP, &[]);
    spin(c, 60);
    call(c, DEL_TIMER, &[PAD_TIMER]);
    call(c, TIMED_WAIT, &[20]);
    memcard::init(c, 0);
    with_buffer(c, OPTIONS_SIZE, |c, o| {
        memcard::getopts(&mut c.m, o);
        let m = &mut c.m;
        m.w32(o + 4, GAME_CODE);
        for (at, v) in [(12, 512), (14, 256), (16, 32), (18, 96)] {
            m.w16(o + at, v);
        }
        m.w32(o, 0);
        m.w32(o + 8, 0);
        m.w32(o + 20, FORMAT_CONFIRM);
        m.w32(o + 24, if always_yes { OVERWRITE_ALWAYS_YES } else { OVERWRITE_CONFIRM });
        m.w32(o + 28, if redraw_cb { LOADING_REDRAW_PROC } else { LOADING_PROC });
        m.w32(o + 32, SAVING_PROC);
        memcard::setopts(c, o);
    });
    call(c, ADD_TIMER, &[PAD_TIMER]);
    call(c, TIMED_WAIT, &[20]);
    call(c, PAD_INIT, &[]);
    call(c, TIMED_WAIT, &[20]);
    call(c, FRAME, &[]);
}

pub fn abi_init_memcard(c: &mut Cpu) {
    init_memcard(c, c.r[4] != 0, c.r[5] != 0);
}

/// `DeInit_Memcard` (FEMEMCARD.CPP, 0x80034938): the card layer stopped, the pads' timer back,
/// the pads stopped for 192 ticks and started; the front end's music back.
pub fn deinit_memcard(c: &mut Cpu) {
    if c.m.r32(INITED) == 0 {
        return;
    }
    c.m.w32(INITED, 0);
    call(c, DEL_TIMER, &[PAD_TIMER]);
    memcard::restore(c);
    call(c, ADD_TIMER, &[PAD_TIMER]);
    call(c, PAD_STOP, &[]);
    spin(c, 192);
    call(c, PAD_INIT, &[]);
    call(c, FRAME, &[]);
    if c.m.r32(USE_FRONT_END) != 0 {
        let app = c.m.r32(FE_APP);
        crate::feapp::update_music(c, FEApplication(app));
    }
}

pub fn abi_deinit_memcard(c: &mut Cpu) {
    deinit_memcard(c);
}

/// `Init_MemcardFile` (FEMEMCARD.CPP, 0x800349e0): the save file `NFS4` of 5376 bytes, its
/// title the player's (card 5 the second's: the word and the name, or the word for no name),
/// none for the pink slips'.
pub fn init_memcard_file(c: &mut Cpu, file: u32, card: u32, pinks: bool) {
    call(c, BLOCKCLEAR, &[file, FILE_INFO_SIZE]);
    c.m.w32(file, FILE_NAME);
    if pinks {
        c.m.w8(SAVE_NAME, 0);
    } else {
        let player = (card as u16 as i16 == 5) as u32;
        if call_addr(c, PLAYER_NAME_EXIST, &[player]) != 0 {
            let word = call(c, TEXT_WORD, &[633]);
            let name = call_addr(c, PLAYER_NAME, &[player]);
            call(c, SPRINTF, &[SAVE_NAME, TWO_STRINGS, word, name]);
        } else {
            let word = call(c, TEXT_WORD, &[634]);
            call(c, SPRINTF, &[SAVE_NAME, word]);
        }
    }
    let m = &mut c.m;
    m.w32(file + 4, SAVE_NAME);
    m.w32(file + 8, FILE_SIZE);
    m.w32(file + 12, 0);
    m.w32(file + 16, 0);
}

pub fn abi_init_memcard_file(c: &mut Cpu) {
    init_memcard_file(c, c.r[4], c.r[5], c.r[6] != 0);
}

/// The application's message (+568) shown with word `word` until it's drawn.
fn tell(c: &mut Cpu, word: u32) {
    let app = c.m.r32(FE_APP);
    let text = call(c, TEXT_WORD, &[word]);
    let app2 = c.m.r32(FE_APP);
    c.m.w32(app + APP_MESSAGE + TEXT, text);
    show(c, app2 + APP_MESSAGE);
    wait_shown(c, |c| app_word(c, APP_MESSAGE + SHOWN));
}

/// The icon file read (`zMem.psh`), and the save file's: a new file (its header) with the
/// first shape as its icon.
fn load_icon(c: &mut Cpu) -> u32 {
    with_buffer(c, 64, |c, name| {
        let path = c.m.r32(CD_PATH);
        call(c, SPRINTF, &[name, ICON_FILE, path]);
        call(c, LOAD_SHAPE, &[name, 0])
    })
}

fn icon_file(c: &mut Cpu, info: u32, icon: u32) {
    c.m.w32(info + 16, NEW_FILE);
    let shape = call(c, SHAPE_POINTER, &[icon, 0]);
    let m = &mut c.m;
    m.w32(info + 24, shape);
    m.w32(info + 28, 0);
    m.w32(info + 32, 0);
}

/// A card's state (-1 none, -2 unformatted, -3 full).
fn card_state(c: &mut Cpu, card: u32) -> i32 {
    let at = memcard::getcard(&mut c.m, card);
    c.m.r32(at) as i32
}

/// `SaveGame` (FEMEMCARD.CPP, 0x80034ac8): player `p`'s game saved on their card ("saving"
/// shown): the pads' vibration noted, the save data made (the front end, tournaments, tracks,
/// cars, records, a random id, the bonuses) and checksummed, the icon loaded; the card's events
/// followed until the save is written or fails (a full card, none, an unformatted one: their
/// messages). Whether it was written.
pub fn save_game(c: &mut Cpu, p: u32) -> bool {
    let p16 = p as u16 as i16 as i32 as u32;
    c.m.w32(CARD_PLAYER, p16);
    c.m.w32(CARD_ACTIVE, 1);
    // (a pad vibrates if it's a DualShock)
    for k in 0..2u32 {
        let info = PAD_INFO + (k << 5);
        c.m.w32(VIBRATION + 4 * k, 1);
        if c.m.r8(info + 5) != DUAL_SHOCK {
            c.m.w32(VIBRATION + 4 * k, 0);
        }
    }
    with_buffer(c, DATA_SIZE + DIALOG_SIZE + FILE_INFO_SIZE, |c, data| {
        let (d, info) = (data + DATA_SIZE, data + DATA_SIZE + DIALOG_SIZE);
        message_box(c, d);
        show_message(c, d, p16.wrapping_add(SAVING), true);
        let card = card_of(p);
        c.m.w32(CARD_DONE, 0);
        make_way(c);
        init_memcard_file(c, info, card, false);
        c.m.w32(info + 20, data);
        wait_event(c, card, 21, true);
        call(c, BLOCKMOVE, &[FE, data + FE_DATA, 1104]);
        crate::fetourn::save_tournament(&mut c.m, TOURNAMENT_MANAGER, data + TOURNAMENTS);
        crate::fetracks::save_tracks(&mut c.m, TRACK_MANAGER, data + TRACKS);
        crate::fecars::save_cars(c, CAR_MANAGER, data + CARS);
        call(c, BLOCKMOVE, &[TRACK_RECORDS, data + RECORDS, 3740]);
        let id = call(c, RAND, &[]);
        c.m.w32(data + SAVE_ID, id);
        crate::fecheats::save_bonus(&mut c.m, data + BONUSES);
        let icon = load_icon(c);
        icon_file(c, info, icon);
        checksum_data(c, data);
        // the card's events until the save is written or fails
        let (word, written) = loop {
            match poll(c, card) {
                22 => {
                    if c.m.r32(memcard::STATE + 52) != 0 {
                        memcard::savefile(c, card, info);
                    }
                }
                12 => break (SAVED, true),
                2 | 3 | 7 | 10 | 11 | 13 | 23 => {
                    let word = match card_state(c, card) {
                        -3 => CARD_FULL,
                        -1 => PULLED,
                        _ => SAVE_FAILED,
                    };
                    break (word, false);
                }
                _ => {}
            }
        };
        wait_event(c, card, 22, false);
        call(c, PURGE, &[icon]);
        bring_back(c);
        if c.m.r32(CARD_DONE) == 0 {
            let app = c.m.r32(FE_APP);
            hide(c, app + APP_MESSAGE_BOX);
            tell(c, word.wrapping_add(p16));
        }
        let screen = c.m.r32(MEMCARD_SCREEN);
        c.m.w32(screen + SCREEN_REDRAW, 1);
        hide(c, d);
        redraw(c);
        c.m.w32(CARD_ACTIVE, 0);
        destroy(c, d);
        written
    })
}

pub fn abi_save_game(c: &mut Cpu) {
    c.r[2] = save_game(c, c.r[4]) as u32;
}

/// `LoadGame` (FEMEMCARD.CPP, 0x80034f70): player `p`'s game loaded from their card ("loading"
/// shown unless `quiet` and not `pinks`), up to three tries (none but with `pinks`): the save
/// read and checked, then the first player's front end, tournaments, tracks, records and
/// bonuses and each player's name and cars, or (`quiet`) their pink slips' cars, name and id;
/// the pads set; the errors' messages (`pinks`). The result: 0 loaded, 1 bad, 2 none, 3 no
/// card.
pub fn load_game(c: &mut Cpu, p: u32, quiet: bool, pinks: bool) -> u32 {
    let p16 = p as u16 as i16 as i32 as u32;
    let card = (p16 << 2) | 1;
    c.m.w32(CARD_PLAYER, p16);
    c.m.w32(CARD_ACTIVE, 1);
    let language = c.m.r8(LANGUAGE);
    with_buffer(c, DATA_SIZE + DIALOG_SIZE + FILE_INFO_SIZE, |c, data| {
        let (d, info) = (data + DATA_SIZE, data + DATA_SIZE + DIALOG_SIZE);
        message_box(c, d);
        if pinks || quiet {
            show_message(c, d, p16.wrapping_add(SAVING), !quiet);
        }
        let (mut done, mut result, mut word) = (false, 0, LOADED);
        // (only the pink slips' load tries again)
        let mut tries = if pinks { 0 } else { 44 };
        c.m.w32(CARD_DONE, 0);
        loop {
            make_way(c);
            tries += 1;
            init_memcard_file(c, info, card, true);
            wait_event(c, card, 21, true);
            c.m.w32(info + 20, data);
            c.m.w32(info + 16, 0);
            // (a retry polls nothing: done stays set)
            while !done {
                let e = poll(c, card);
                let mut bad = |c: &mut Cpu| {
                    let state = card_state(c, card);
                    let app = c.m.r32(FE_APP);
                    hide(c, app + APP_MESSAGE_BOX);
                    (word, result) = match state {
                        -2 => (UNFORMATTED, 2),
                        -1 => (PULLED, 3),
                        _ => (LOAD_FAILED, 1),
                    };
                    done = true;
                };
                match e {
                    22 => memcard::loadfile(c, card, info, false),
                    15 => {
                        if !verify_successful_read(c, data) {
                            bad(c);
                            continue;
                        }
                        result = 0;
                        let name = FE + PLAYER_NAMES + (p16 << 3);
                        if quiet {
                            let id = c.m.r32(data + SAVE_ID);
                            c.m.w32(FE + PINKS_IDS + (p16 << 2), id);
                            crate::fecars::load_pink_slips_cars(c, CAR_MANAGER, data + CARS, p16);
                            call(c, SPRINTF, &[name, data + FE_DATA + PLAYER_NAMES]);
                            let money = c.m.r32(data + TOURNAMENTS);
                            c.m.w32(FE + PINKS_MONEY + (p16 << 2), money);
                        } else {
                            if p16 != 0 {
                                call(c, SPRINTF, &[name, data + FE_DATA + PLAYER_NAMES]);
                            } else {
                                call(c, BLOCKMOVE, &[data + FE_DATA, FE, 1104]);
                                crate::fetourn::load_tournament(&mut c.m, TOURNAMENT_MANAGER, data + TOURNAMENTS);
                                crate::fetracks::load_tracks(&mut c.m, TRACK_MANAGER, data + TRACKS);
                                call(c, BLOCKMOVE, &[data + RECORDS, TRACK_RECORDS, 3740]);
                                crate::fecheats::load_bonus(c, data + BONUSES);
                            }
                            crate::fecars::load_cars(c, CAR_MANAGER, data + CARS, p16);
                        }
                        word = LOADED;
                        call_addr(c, SET_PADS, &[]);
                        done = true;
                    }
                    2 | 3 | 7 | 10 | 11 | 16 | 23 => bad(c),
                    _ => {}
                }
            }
            wait_event(c, card, 22, false);
            bring_back(c);
            if word == LOADED || tries as i32 >= 3 {
                break;
            }
        }
        if pinks && c.m.r32(CARD_DONE) == 0 {
            let app = c.m.r32(FE_APP);
            hide(c, app + APP_MESSAGE_BOX);
            let app = c.m.r32(FE_APP);
            hide(c, app + APP_MESSAGE);
            tell(c, word.wrapping_add(p16));
        }
        let mode = c.m.r8(p16.wrapping_add(FE) + PAD_MODES) as u32;
        call_addr(c, RESET_CONTROLLER, &[p16, mode]);
        if !quiet {
            if c.m.r8(FE + RACE_TYPE) == 6 {
                c.m.w8(FE + RACE_TYPE, 0);
            }
            let sfx = c.m.r8(FE + FE_SFX_LEVEL) as u32;
            c.m.w32(SFX_LEVEL, sfx);
        }
        if pinks || quiet {
            hide(c, d);
        }
        if pinks {
            redraw(c);
        }
        c.m.w32(CARD_ACTIVE, 0);
        let screen = c.m.r32(MEMCARD_SCREEN);
        let front_end2 = c.m.r32(USE_FRONT_END2);
        c.m.w32(screen + SCREEN_REDRAW, 1);
        if front_end2 != 0 {
            c.m.w8(LANGUAGE, language);
        }
        destroy(c, d);
        result
    })
}

pub fn abi_load_game(c: &mut Cpu) {
    c.r[2] = load_game(c, c.r[4], c.r[5] != 0, c.r[6] != 0);
}

/// `SavePinkSlipsCars` (FEMEMCARD.CPP, 0x80035580): player `p`'s pink slips' cars (`which`)
/// saved into their save: the save read and checked (its id must be the player's), the cars
/// put in, checksummed and written. 0 done, else the error: 1 another save, 2 none or bad, 3
/// an error, 4 no card, 5 unformatted, 6 full.
pub fn save_pink_slips_cars(c: &mut Cpu, p: u32, which: u32) -> u32 {
    make_way(c);
    let p16 = p as u16 as i16 as i32 as u32;
    let card = (p16 << 2).wrapping_add(1) as u16 as i16 as i32 as u32;
    c.m.w32(CARD_PLAYER, p16);
    c.m.w32(CARD_ACTIVE, 1);
    c.m.w32(CARD_DONE, 0);
    let icon = load_icon(c);
    let result = with_buffer(c, DATA_SIZE + FILE_INFO_SIZE, |c, data| {
        let info = data + DATA_SIZE;
        init_memcard_file(c, info, card, true);
        wait_event(c, card, 21, true);
        c.m.w32(info + 20, data);
        c.m.w32(info + 16, 0);
        loop {
            match poll(c, card) {
                22 => memcard::loadfile(c, card, info, false),
                15 => {
                    if !verify_successful_read(c, data) {
                        break match card_state(c, card) {
                            -2 => 5,
                            -1 => 4,
                            _ => 2,
                        };
                    }
                    if c.m.r32(FE + PINKS_IDS + (p16 << 2)) != c.m.r32(data + SAVE_ID) {
                        break 1;
                    }
                    let which = which as u16 as i16 as i32 as u32;
                    crate::fecars::save_pink_slips_cars(c, CAR_MANAGER, data + CARS, p16, which);
                    wait_event(c, card, 21, true);
                    icon_file(c, info, icon);
                    checksum_data(c, data);
                    // written
                    break loop {
                        match poll(c, card) {
                            22 => {
                                if c.m.r32(memcard::STATE + 52) != 0 {
                                    memcard::savefile(c, card, info);
                                }
                            }
                            12 => break 0,
                            2 | 3 | 7 | 10 | 11 | 13 | 23 => {
                                let app = c.m.r32(FE_APP);
                                hide(c, app + APP_MESSAGE_BOX);
                                break match card_state(c, card) {
                                    -3 => 6,
                                    -1 => 4,
                                    _ => 3,
                                };
                            }
                            _ => {}
                        }
                    };
                }
                2 | 16 => {
                    break match card_state(c, card) {
                        -2 => 5,
                        -1 => 4,
                        _ => 2,
                    };
                }
                3 | 7 | 10 | 11 | 23 => break 2,
                _ => {}
            }
        }
    });
    wait_event(c, card, 22, false);
    call(c, PURGE, &[icon]);
    bring_back(c);
    c.m.w32(CARD_ACTIVE, 0);
    result
}

pub fn abi_save_pink_slips_cars(c: &mut Cpu) {
    c.r[2] = save_pink_slips_cars(c, c.r[4], c.r[5]);
}

/// `SavePinkSlipsCarsWithErrorDialogs` (FEMEMCARD.CPP, 0x80035930): the pink slips' cars
/// saved ("saving" shown, up to three tries a moment apart); on an error its message (with the
/// winner's or loser's name, `kind` 1 or 2) and a retry asked. The result.
pub fn save_pink_slips_with_dialogs(c: &mut Cpu, p: u32, kind: u32, which: u32) -> u32 {
    let p16 = p as u16 as i16 as i32 as u32;
    with_buffer(c, YES_NO_SIZE + 2 * DIALOG_SIZE + 2 * TEXT_SIZE, |c, ask| {
        let note = ask + YES_NO_SIZE;
        let saving = note + DIALOG_SIZE;
        let (message, line) = (saving + DIALOG_SIZE, saving + DIALOG_SIZE + TEXT_SIZE);
        crate::fedialog::yes_no(c, DialogYesNo(ask));
        message_box(c, note);
        let m = &mut c.m;
        m.w32(ask + 160, 658);
        m.w32(ask + 164, NO_WORD);
        m.w16(ask + 136, 1);
        m.w16(ask + DIALOG_PLAYER, p as u16);
        let mut answer = 0;
        loop {
            let app = c.m.r32(FE_APP);
            show(c, app + APP_MESSAGE_BOX);
            while app_word(c, APP_MESSAGE_BOX + SHOWN) != 1 {
                redraw(c);
            }
            redraw(c);
            message_box(c, saving);
            show_message(c, saving, p16.wrapping_add(SAVING), true);
            let which16 = which as u16 as i16 as i32 as u32;
            let mut result;
            let mut k = 0;
            loop {
                result = save_pink_slips_cars(c, p16, which16);
                if result != 0 {
                    call(c, TIMED_WAIT, &[5]);
                }
                k += 1;
                if result == 0 || k >= 3 {
                    break;
                }
            }
            hide(c, saving);
            redraw(c);
            if result != 0 {
                // the error, with the player's name; a retry asked
                let app = c.m.r32(FE_APP);
                hide(c, app + APP_MESSAGE_BOX);
                let word = c.m.r32(ERROR_WORDS + (result << 2)).wrapping_add(p16);
                let text = call(c, TEXT_WORD, &[word]);
                call(c, SPRINTF, &[message, text]);
                let kind16 = kind as u16 as i16;
                if kind16 != 0 {
                    let format = call(c, TEXT_WORD, &[if kind16 == 2 { 666 } else { 665 }]);
                    let name = call_addr(c, PLAYER_NAME, &[p16]);
                    call(c, SPRINTF, &[line, format, name]);
                    let m = &mut c.m;
                    m.w32(note + TEXT, line);
                    m.w16(note + 124, 0);
                    m.w16(note + 126, (-60i16) as u16);
                    show(c, note);
                }
                c.m.w32(ask + TEXT, message);
                answer = crate::fedialog::run(c, DialogInteractive(ask)) as u16 as i16 as i32 as u32;
                hide(c, note);
            }
            destroy(c, saving);
            if result == 0 || answer == 0 {
                destroy(c, note);
                destroy(c, ask);
                return result;
            }
        }
    })
}

pub fn abi_save_pink_slips_with_dialogs(c: &mut Cpu) {
    c.r[2] = save_pink_slips_with_dialogs(c, c.r[4], c.r[5], c.r[6]);
}

/// For callers not rewritten yet.
pub(crate) fn init_memcard_entry(c: &mut Cpu) {
    abi_init_memcard(c);
}

pub(crate) fn deinit_memcard_entry(c: &mut Cpu) {
    abi_deinit_memcard(c);
}

pub(crate) fn save_pink_slips_with_dialogs_entry(c: &mut Cpu) {
    abi_save_pink_slips_with_dialogs(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// Save data (5292 bytes) and a save file's definition.
    const DATA: u32 = 0x8014_4000;
    const FILE: u32 = 0x8014_5800;

    /// Save data of any bytes, its size and checksums now and then right (the original's own
    /// `ChecksumData` run on it), or one checksum off; the players' names (none, letters).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        for k in 0..5292 {
            c.m.w8(DATA + k, rng.next() as u8);
        }
        if pick(rng, 3) != 0 {
            crate::abi::call(c, nfs4rc::f_800343d0, &[DATA]);
            if pick(rng, 2) == 0 {
                let at = [0, 5272, 5276, 5280, 5284, 5288][pick(rng, 6) as usize];
                let v = c.m.r32(DATA + at) ^ 1;
                c.m.w32(DATA + at, v);
            }
        }
        for p in 0..2 {
            let len = pick(rng, 8);
            for j in 0..8 {
                c.m.w8(FE + 0x364 + 8 * p + j, if j < len { b'a' + pick(rng, 26) as u8 } else { 0 });
            }
        }
        c.m.w8(FE + 3, pick(rng, 3) as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("ChecksumData", 0x8003_43d0, nfs4rc::f_800343d0, |_, _| vec![DATA]),
            ("VerifySuccessfulRead", 0x8003_4448, nfs4rc::f_80034448, |_, _| vec![DATA]),
            ("MakeWayForMemoryCard", 0x8003_4520, nfs4rc::f_80034520, |_, _| vec![]),
            ("BringThatBeatBack", 0x8003_4544, nfs4rc::f_80034544, |_, _| vec![]),
            ("OverwriteAlwaysYes", 0x8003_4748, nfs4rc::f_80034748, |_, _| vec![]),
            ("LoadingProc", 0x8003_477c, nfs4rc::f_8003477c, |_, _| vec![]),
            ("Init_MemcardFile", 0x8003_49e0, nfs4rc::f_800349e0, |_, r| vec![FILE, [1, 5, pick(r, 9), 0x10005][pick(r, 4) as usize], pick(r, 2)]),
        ]
    }

    #[test]
    fn fememcard_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x980 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x981 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
