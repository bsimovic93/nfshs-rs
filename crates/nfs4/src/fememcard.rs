//! FEMEMCARD.CPP: the front end's saves on the memory card: the save's data (`tMemCardData`,
//! 5292 bytes: +0 its size, +4 the bonuses, +8 the front end's structure, +1112 the cars,
//! +1336 the tracks, +1352 the tournaments, +1528 the records, +5268 a random id, +5272 five
//! checksums), the card layer started and stopped around a save (the pads' timer off), the
//! confirm dialogs, saving and loading a player's game and the pink slips' cars, with their
//! messages.

use crate::abi::{self, Saved};
use crate::globals::{FE_APP, FRONT_END, GAME_TICKS, PATHS};
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
const CAR_MANAGER: u32 = 0x8011_552C;
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

/// A port called from a port.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// A word of the application's.
fn app_word(c: &mut Cpu, at: u32) -> u32 {
    let app = c.m.r32(FE_APP);
    c.m.r32(app + at)
}

/// The application's frame (the host's hook).
fn redraw(c: &mut Cpu, r: &Saved, ra: u32) {
    let app = c.m.r32(FE_APP);
    r.call_ptr_leaving(c, REDRAW, &[app], ra);
}

/// A message box built in place at `d` (a screen first): the dialog's, the message's and the
/// message box's fields in turn (288 wide, nothing shown, for either player).
fn message_box(c: &mut Cpu, r: &Saved, d: u32, ra: u32) {
    port(c, r, 0x8002_6040, ra, &[d], crate::fescreen::abi_screen);
    let m = &mut c.m;
    m.w32(d + 96, DIALOG_VT);
    m.w32(d + 112, 0);
    for at in [110, 130, 126, 124, 108, 106, 104, 102] {
        m.w16(d + at, 0);
    }
    m.w16(d + 128, 288);
    m.w16(d + 100, 0xFFFF);
    m.w32(d + 96, MESSAGE_VT);
    m.w16(d + 136, 0);
    m.w32(d + 120, 0);
    m.w32(d + 148, 0);
    m.w32(d + 132, 0);
    m.w32(d + 120, 0);
    m.w32(d + 140, 128);
    m.w32(d + 96, MESSAGE_BOX_VT);
}

/// Waits for a dialog's word `flag` to be 1, a frame drawn between tries (`ra` the loop's,
/// `last` the frame after).
fn wait_shown(c: &mut Cpu, r: &Saved, flag: impl Fn(&mut Cpu) -> u32, ra: u32, last: u32) {
    while flag(c) != 1 {
        redraw(c, r, ra);
    }
    redraw(c, r, last);
}

/// The card's events polled until `event` (or another than `event` when `until_not`): a frame
/// swapped and a vertical blank between tries.
fn wait_event(c: &mut Cpu, r: &Saved, card: u32, event: u32, until_not: bool, ra: u32, frame_ra: u32, vsync_ra: u32) {
    loop {
        let v = port(c, r, 0x8005_0368, ra, &[card], crate::memcard::abi_handlecardevents);
        if (v == event) != until_not {
            return;
        }
        r.call_leaving(c, FRAME, &[], frame_ra);
        r.call_ptr_leaving(c, VSYNC, &[0], vsync_ra);
    }
}

/// One poll of the card's events in a save's loop: a frame, the events, the system's task,
/// a vertical blank. The event.
fn poll(c: &mut Cpu, r: &mut Saved, card: u32, ras: [u32; 4]) -> u32 {
    r.call_leaving(c, FRAME, &[], ras[0]);
    let v = port(c, r, 0x8005_0368, ras[1], &[card], crate::memcard::abi_handlecardevents);
    r.0[0] = v;
    r.call_ptr_leaving(c, SYSTEM_TASK, &[0], ras[2]);
    r.call_ptr_leaving(c, VSYNC, &[0], ras[3]);
    v
}

/// `ChecksumData` (FEMEMCARD.CPP, 0x800343d0): the save data's size and its five checksums.
pub fn abi_checksum_data(c: &mut Cpu) {
    let d = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = d;
        c.m.w32(d, 5292);
        let parts = [(8, 1104, 0x8003_43f4), (1112, 224, 0x8003_4404), (1336, 16, 0x8003_4414), (1352, 176, 0x8003_4424), (1528, 3740, 0x8003_4434)];
        for (k, (at, len, ra)) in parts.into_iter().enumerate() {
            let v = r.call_leaving(c, CHECKSUM, &[d + at, len], ra);
            c.m.w32(d + 5272 + 4 * k as u32, v);
        }
    })
}

/// `VerifySuccessfulRead` (FEMEMCARD.CPP, 0x80034448): whether the read save data's size and
/// checksums are right.
pub fn abi_verify_successful_read(c: &mut Cpu) {
    let d = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = d;
        let parts = [(8, 1104, 0x8003_4468), (1112, 224, 0x8003_4480), (1336, 16, 0x8003_44a0), (1352, 176, 0x8003_44c0), (1528, 3740, 0x8003_44e0)];
        let mut ok = 1u32;
        for (k, (at, len, ra)) in parts.into_iter().enumerate() {
            let v = r.call_leaving(c, CHECKSUM, &[d + at, len], ra);
            ok &= (c.m.r32(d + 5272 + 4 * k as u32) == v) as u32;
            r.0[0] = ok;
        }
        ok &= (c.m.r32(d) == 5292) as u32;
        c.r[2] = ok;
    })
}

/// `MakeWayForMemoryCard` (FEMEMCARD.CPP, 0x80034520): the pads' timer stopped.
pub fn abi_make_way(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).call_leaving(c, DEL_TIMER, &[PAD_TIMER], 0x8003_4534);
    })
}

/// `BringThatBeatBack` (FEMEMCARD.CPP, 0x80034544): the pads' timer back.
pub fn abi_bring_back(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).call_leaving(c, ADD_TIMER, &[PAD_TIMER], 0x8003_4558);
    })
}

/// `Confirm` (FEMEMCARD.CPP, 0x80034568): a yes-or-no with the text `word` and the answers
/// `answers` (the application's message box hidden meanwhile, shown again after): the answer
/// (-1 the card pulled: told so). The card reached unless answered yes.
pub fn abi_confirm(c: &mut Cpu) {
    let (word, answers) = (c.r[4], c.r[5]);
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[4]) = (word, answers);
        r.enter_leaving(c, 0x8003_4544, 0x8003_4590, abi_bring_back);
        let d = sp + 16;
        r.0[0] = d;
        port(c, &r, 0x8001_9bcc, 0x8003_4598, &[d], crate::fedialog::abi_yes_no);
        r.0[1] = 0x8005_0000;
        c.m.w32(d + 96, YES_NO_VT);
        let app = c.m.r32(FE_APP);
        r.0[3] = 0;
        if c.m.r32(app + 720 + 112) != 0 {
            r.0[3] = 1;
            port(c, &r, 0x8001_8a44, 0x8003_45cc, &[app + 720], crate::fedialog::abi_hide);
        }
        let text = r.call_leaving(c, TEXT_WORD, &[word], 0x8003_45d4);
        let m = &mut c.m;
        m.w32(d + 144, text);
        m.w32(d + 164, 659);
        m.w32(d + 160, answers);
        m.w16(d + 136, 0);
        if m.r8(LANGUAGE) == 3 {
            m.w16(d + 124, 0);
            m.w16(d + 126, 10);
        }
        let v = port(c, &r, 0x8001_9928, 0x8003_460c, &[d], crate::fedialog::abi_run) as u16 as i16 as i32 as u32;
        r.0[2] = v;
        let shown = r.0[3];
        if v == u32::MAX {
            let app = c.m.r32(FE_APP);
            r.0[0] = app + 568;
            let player = c.m.r32(CARD_PLAYER);
            let text = r.call_leaving(c, TEXT_WORD, &[player.wrapping_add(812)], 0x8003_4634);
            let app2 = c.m.r32(FE_APP);
            c.m.w32(app + 568 + 144, text);
            port(c, &r, 0x8001_8980, 0x8003_4644, &[app2 + 568], crate::fedialog::abi_display);
            wait_shown(c, &r, |c| app_word(c, 700), 0x8003_4668, 0x8003_4678);
            c.m.w32(CARD_DONE, 1);
        } else if v == 0 {
            c.m.w32(CARD_DONE, 1);
        } else if shown != 0 {
            r.0[0] = 0x8005_0000;
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_8980, 0x8003_46a8, &[app + 720], crate::fedialog::abi_display);
            while app_word(c, 852) != 1 {
                redraw(c, &r, 0x8003_46cc);
            }
        }
        redraw(c, &r, 0x8003_46e4);
        r.enter_leaving(c, 0x8003_4520, 0x8003_46ec, abi_make_way);
        port(c, &r, 0x8002_6098, 0x8003_46f8, &[d, 2], crate::fescreen::abi_screen_destructor);
        c.r[2] = v;
    })
}

/// `OverwriteConfirm` and `FormatConfirm` (FEMEMCARD.CPP, 0x8003471c, 0x80034750): the card
/// player's question.
fn confirm_word(c: &mut Cpu, at: u32, answers: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let word = c.m.r32(CARD_PLAYER).wrapping_add(at);
        port(c, &r, 0x8003_4568, ra, &[word, answers], abi_confirm);
    })
}

pub fn abi_overwrite_confirm(c: &mut Cpu) {
    confirm_word(c, 804, 656, 0x8003_4738);
}

pub fn abi_format_confirm(c: &mut Cpu) {
    confirm_word(c, 808, 657, 0x8003_476c);
}

/// `OverwriteAlwaysYes` (FEMEMCARD.CPP, 0x80034748): yes.
pub fn abi_overwrite_always_yes(c: &mut Cpu) {
    c.r[2] = 1;
}

/// `LoadingProc` (FEMEMCARD.CPP, 0x8003477c): nothing.
pub fn abi_loading_proc(_c: &mut Cpu) {}

/// `LoadingRedrawProc` and `SavingProc` (FEMEMCARD.CPP, 0x80034784, 0x800347ac): a frame.
pub fn abi_loading_redraw_proc(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| redraw(c, &Saved::of(c), 0x8003_479c));
}

pub fn abi_saving_proc(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| redraw(c, &Saved::of(c), 0x8003_47c4));
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
/// `redraw`); the pads back.
pub fn abi_init_memcard(c: &mut Cpu) {
    let (redraw_cb, always_yes) = (c.r[4], c.r[5]);
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (redraw_cb, always_yes);
        if c.m.r32(INITED) != 0 {
            return;
        }
        let front_end = c.m.r32(USE_FRONT_END);
        c.m.w32(INITED, 1);
        if front_end != 0 {
            r.call_leaving(c, STOP_SONG, &[0], 0x8003_4814);
            r.call_leaving(c, TIMED_WAIT, &[5], 0x8003_481c);
        }
        r.call_leaving(c, PAD_STOP, &[], 0x8003_4824);
        spin(c, 60);
        r.call_leaving(c, DEL_TIMER, &[PAD_TIMER], 0x8003_4854);
        r.call_leaving(c, TIMED_WAIT, &[20], 0x8003_485c);
        port(c, &r, 0x8004_f8c8, 0x8003_4864, &[0], crate::memcard::abi_init);
        port(c, &r, 0x8004_fa04, 0x8003_486c, &[sp + 16], crate::memcard::abi_getopts);
        let m = &mut c.m;
        m.w32(sp + 20, GAME_CODE);
        for (at, v) in [(28, 512), (30, 256), (32, 32), (34, 96)] {
            m.w16(sp + at, v);
        }
        m.w32(sp + 16, 0);
        m.w32(sp + 24, 0);
        m.w32(sp + 36, FORMAT_CONFIRM);
        m.w32(sp + 40, if always_yes != 0 { OVERWRITE_ALWAYS_YES } else { OVERWRITE_CONFIRM });
        m.w32(sp + 44, if redraw_cb != 0 { LOADING_REDRAW_PROC } else { LOADING_PROC });
        m.w32(sp + 48, SAVING_PROC);
        port(c, &r, 0x8004_fa7c, 0x8003_48f8, &[sp + 16], crate::memcard::abi_setopts);
        r.call_leaving(c, ADD_TIMER, &[PAD_TIMER], 0x8003_4904);
        r.call_leaving(c, TIMED_WAIT, &[20], 0x8003_490c);
        r.call_leaving(c, PAD_INIT, &[], 0x8003_4914);
        r.call_leaving(c, TIMED_WAIT, &[20], 0x8003_491c);
        r.call_leaving(c, FRAME, &[], 0x8003_4924);
    })
}

/// `DeInit_Memcard` (FEMEMCARD.CPP, 0x80034938): the card layer stopped, the pads' timer back,
/// the pads stopped for 192 ticks and started; the front end's music back.
pub fn abi_deinit_memcard(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        if c.m.r32(INITED) == 0 {
            return;
        }
        r.0[0] = PAD_TIMER;
        c.m.w32(INITED, 0);
        r.call_leaving(c, DEL_TIMER, &[PAD_TIMER], 0x8003_4964);
        r.enter_leaving(c, 0x8004_f9c0, 0x8003_496c, crate::memcard::abi_restore);
        r.call_leaving(c, ADD_TIMER, &[PAD_TIMER], 0x8003_4974);
        r.call_leaving(c, PAD_STOP, &[], 0x8003_497c);
        spin(c, 192);
        r.call_leaving(c, PAD_INIT, &[], 0x8003_49a8);
        r.call_leaving(c, FRAME, &[], 0x8003_49b0);
        if c.m.r32(USE_FRONT_END) != 0 {
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_3e34, 0x8003_49d0, &[app], crate::feapp::abi_update_music);
        }
    })
}

/// `Init_MemcardFile` (FEMEMCARD.CPP, 0x800349e0): the save file `NFS4` of 5376 bytes, its
/// title the player's (card 5 the second's: the word and the name, or the word for no name),
/// none for the pink slips'.
pub fn abi_init_memcard_file(c: &mut Cpu) {
    let (file, card, pinks) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[1], r.0[0]) = (file, card, pinks);
        r.call_leaving(c, BLOCKCLEAR, &[file, 44], 0x8003_4a08);
        c.m.w32(file, FILE_NAME);
        if pinks != 0 {
            c.m.w8(SAVE_NAME, 0);
        } else {
            let player = (card as u16 as i16 == 5) as u32;
            r.0[0] = player;
            if port(c, &r, 0x8002_ba50, 0x8003_4a3c, &[player], crate::front::abi_player_name_exist) != 0 {
                let word = r.call_leaving(c, TEXT_WORD, &[633], 0x8003_4a4c);
                let name = port(c, &r, 0x8002_ba7c, 0x8003_4a58, &[player], crate::front::abi_player_name);
                r.0[0] = word;
                r.call_leaving(c, SPRINTF, &[SAVE_NAME, TWO_STRINGS, word, name], 0x8003_4a74);
            } else {
                let word = r.call_leaving(c, TEXT_WORD, &[634], 0x8003_4a84);
                r.call_leaving(c, SPRINTF, &[SAVE_NAME, word], 0x8003_4a94);
            }
        }
        let m = &mut c.m;
        m.w32(file + 4, SAVE_NAME);
        m.w32(file + 8, 5376);
        m.w32(file + 12, 0);
        m.w32(file + 16, 0);
    })
}

/// The application's message box (+568) shown with word `word` until it's drawn.
fn tell(c: &mut Cpu, r: &mut Saved, word: u32, ras: [u32; 4]) {
    r.0[1] = 0x8005_0000;
    let app = c.m.r32(FE_APP);
    r.0[0] = app + 568;
    let text = r.call_leaving(c, TEXT_WORD, &[word], ras[0]);
    let app2 = c.m.r32(FE_APP);
    c.m.w32(app + 568 + 144, text);
    port(c, r, 0x8001_8980, ras[1], &[app2 + 568], crate::fedialog::abi_display);
    wait_shown(c, r, |c| app_word(c, 700), ras[2], ras[3]);
}

/// The save's card for player `p`: 1 or 5.
fn card_of(p: u32) -> u32 {
    ((p << 16) as i32 >> 14) as u32 | 1
}

/// `SaveGame` (FEMEMCARD.CPP, 0x80034ac8): player `p`'s game saved on their card ("saving"
/// shown): the pads' vibration noted, the save data made (the front end, tournaments, tracks,
/// cars, records, a random id, the bonuses) and checksummed, the icon loaded; the card's events
/// followed until the save is written or fails (a full card, none, an unformatted one: their
/// messages). 1 written (or failed: the message shown), 0 not.
pub fn abi_save_game(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 5880, |c, sp| {
        let mut r = Saved::of(c);
        r.0[5] = p;
        let p16 = p as u16 as i16 as i32 as u32;
        c.m.w32(CARD_PLAYER, p16);
        c.m.w32(CARD_ACTIVE, 1);
        for k in 0..2u32 {
            let info = crate::globals::PAD_INFO + (((k != 0) as u32) << 5);
            c.m.w32(FE + 1096 + 4 * k, 1);
            if c.m.r8(info + 5) != 0x73 {
                c.m.w32(FE + 1096 + 4 * k, 0);
            }
        }
        let d = sp + 5568;
        r.0[0] = d;
        message_box(c, &r, d, 0x8003_4b70);
        let text = r.call_leaving(c, TEXT_WORD, &[p16.wrapping_add(631)], 0x8003_4bf4);
        let m = &mut c.m;
        m.w32(d + 144, text);
        m.w16(d + 124, 0);
        m.w16(d + 126, 50);
        port(c, &r, 0x8001_8980, 0x8003_4c0c, &[d], crate::fedialog::abi_display);
        r.0[0] = 0x8005_0000;
        r.0[3] = 0;
        while c.m.r32(d + 132) != 1 {
            redraw(c, &r, 0x8003_4c30);
        }
        (r.0[6], r.0[2], r.0[1]) = (0, 652, 21);
        redraw(c, &r, 0x8003_4c4c);
        let card = card_of(p);
        r.0[0] = card;
        c.m.w16(sp + 5832, card as u16);
        c.m.w32(CARD_DONE, 0);
        r.enter_leaving(c, 0x8003_4520, 0x8003_4c6c, abi_make_way);
        r.0[0] = card << 16;
        port(c, &r, 0x8003_49e0, 0x8003_4c80, &[sp + 5720, card, 0], abi_init_memcard_file);
        c.m.w32(sp + 5740, sp + 16);
        wait_event(c, &r, card, 21, true, 0x8003_4c90, 0x8003_4ca0, 0x8003_4ca8);
        r.call_leaving(c, BLOCKMOVE, &[FE, sp + 24, 1104], 0x8003_4cc0);
        port(c, &r, 0x8003_3638, 0x8003_4cd0, &[TOURNAMENT_MANAGER, sp + 1368], crate::fetourn::abi_save_tournament);
        port(c, &r, 0x8002_1bdc, 0x8003_4ce0, &[TRACK_MANAGER, sp + 1352], crate::fetracks::abi_save_tracks);
        port(c, &r, 0x8001_6c38, 0x8003_4cf0, &[CAR_MANAGER, sp + 1128], crate::fecars::abi_save_cars);
        r.call_leaving(c, BLOCKMOVE, &[TRACK_RECORDS, sp + 1544, 3740], 0x8003_4d04);
        let id = r.call_leaving(c, RAND, &[], 0x8003_4d0c);
        c.m.w32(sp + 5284, id);
        port(c, &r, 0x8002_3768, 0x8003_4d18, &[sp + 20], crate::fecheats::abi_save_bonus);
        r.0[0] = sp + 5768;
        let path = c.m.r32(CD_PATH);
        r.call_leaving(c, SPRINTF, &[sp + 5768, ICON_FILE, path], 0x8003_4d3c);
        r.0[1] = card << 16;
        let icon = r.call_leaving(c, LOAD_SHAPE, &[sp + 5768, 0], 0x8003_4d48);
        r.0[7] = icon;
        c.m.w32(sp + 5736, 512);
        let shape = r.call_leaving(c, SHAPE_POINTER, &[icon, 0], 0x8003_4d60);
        c.m.w32(sp + 5744, shape);
        c.m.w32(sp + 5748, 0);
        c.m.w32(sp + 5752, 0);
        port(c, &r, 0x8003_43d0, 0x8003_4d74, &[sp + 16], abi_checksum_data);
        (r.0[8], r.0[4]) = (crate::memcard::STATE, ((p << 16) as i32 >> 14) as u32);
        while r.0[3] == 0 {
            let e = poll(c, &mut r, card, [0x8003_4d94, 0x8003_4d9c, 0x8003_4da8, 0x8003_4db0]);
            r.0[0] = e.wrapping_sub(2);
            match e {
                22 => {
                    if c.m.r32(crate::memcard::STATE + 52) != 0 {
                        port(c, &r, 0x8004_ff34, 0x8003_4df4, &[card, sp + 5720], crate::memcard::abi_savefile);
                    }
                }
                12 => {
                    (r.0[2], r.0[3], r.0[6]) = (652, 1, 1);
                }
                2 | 3 | 7 | 10 | 11 | 13 | 23 => {
                    let at = port(c, &r, 0x8005_072c, 0x8003_4e14, &[r.0[4].wrapping_add(1)], crate::memcard::abi_getcard);
                    r.0[2] = match c.m.r32(at) as i32 {
                        -3 => 670,
                        -1 => 812,
                        _ => 806,
                    };
                    r.0[3] = 1;
                }
                _ => {}
            }
        }
        (r.0[1], r.0[0]) = (22, card << 16);
        wait_event(c, &r, card, 22, false, 0x8003_4e50, 0x8003_4e60, 0x8003_4e68);
        r.call_leaving(c, PURGE, &[icon], 0x8003_4e78);
        r.enter_leaving(c, 0x8003_4544, 0x8003_4e80, abi_bring_back);
        if c.m.r32(CARD_DONE) == 0 {
            r.0[1] = 0x8005_0000;
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_8a44, 0x8003_4ea0, &[app + 720], crate::fedialog::abi_hide);
            let word = r.0[2].wrapping_add(p16);
            tell(c, &mut r, word, [0x8003_4eb8, 0x8003_4ec8, 0x8003_4eec, 0x8003_4efc]);
        }
        r.0[0] = d;
        let screen = c.m.r32(MEMCARD_SCREEN);
        c.m.w32(screen + 1440, 1);
        port(c, &r, 0x8001_8a44, 0x8003_4f18, &[d], crate::fedialog::abi_hide);
        redraw(c, &r, 0x8003_4f28);
        c.m.w32(CARD_ACTIVE, 0);
        port(c, &r, 0x8002_6098, 0x8003_4f3c, &[d, 2], crate::fescreen::abi_screen_destructor);
        c.r[2] = r.0[6];
    })
}

/// `LoadGame` (FEMEMCARD.CPP, 0x80034f70): player `p`'s game loaded from their card ("loading"
/// shown unless `quiet` and not `pinks`), up to three tries (none with `pinks`): the save read
/// and checked, then the first player's front end, tournaments, tracks, records and bonuses
/// and each player's name and cars, or (`quiet`) their pink slips' cars, name and id; the pads
/// set; the errors' messages (`pinks`). The result: 0 loaded, 1 bad, 2 none, 3 no card.
pub fn abi_load_game(c: &mut Cpu) {
    let (p, quiet, pinks) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 5832, |c, sp| {
        let mut r = Saved::of(c);
        let p16 = p as u16 as i16 as i32 as u32;
        let card = (p16 << 2) | 1;
        (r.0[1], r.0[4]) = (p16, card);
        let d = sp + 5568;
        r.0[0] = d;
        c.m.w32(CARD_PLAYER, p16);
        c.m.w32(CARD_ACTIVE, 1);
        let language = c.m.r8(LANGUAGE) as u32;
        c.m.w32(sp + 5836, quiet);
        c.m.w32(sp + 5840, pinks);
        c.m.w16(sp + 5768, p as u16);
        c.m.w32(sp + 5776, language);
        message_box(c, &r, d, 0x8003_4fe8);
        if pinks != 0 || quiet != 0 {
            let text = r.call_leaving(c, TEXT_WORD, &[p16.wrapping_add(631)], 0x8003_507c);
            c.m.w32(d + 144, text);
            if quiet == 0 {
                c.m.w16(d + 124, 0);
                c.m.w16(d + 126, 50);
            }
            port(c, &r, 0x8001_8980, 0x8003_50a0, &[d], crate::fedialog::abi_display);
            r.0[0] = 0x8005_0000;
            while c.m.r32(d + 132) != 1 {
                redraw(c, &r, 0x8003_50c4);
            }
            redraw(c, &r, 0x8003_50d8);
        }
        (r.0[1], r.0[3]) = (0, 0);
        c.m.w32(sp + 5780, if pinks != 0 { 0 } else { 44 });
        c.m.w32(CARD_DONE, 0);
        r.0[2] = 654;
        r.0[4] = card << 16;
        c.m.w32(sp + 5784, p << 16);
        loop {
            r.enter_leaving(c, 0x8003_4520, 0x8003_5118, abi_make_way);
            let tries = c.m.r32(sp + 5780).wrapping_add(1);
            c.m.w32(sp + 5780, tries);
            port(c, &r, 0x8003_49e0, 0x8003_5134, &[sp + 5720, card, 1], abi_init_memcard_file);
            wait_event(c, &r, card, 21, true, 0x8003_513c, 0x8003_5150, 0x8003_5158);
            c.m.w32(sp + 5740, sp + 16);
            c.m.w32(sp + 5736, 0);
            let (fp, s6) = (p16 << 3, p16 << 2);
            let s7 = s6.wrapping_add(FE);
            (r.0[5], r.0[8], r.0[6], r.0[7]) = (p16, fp, s6, s7);
            while r.0[1] == 0 {
                r.0[0] = 22;
                let e = poll(c, &mut r, card, [0x8003_5194, 0x8003_519c, 0x8003_51a8, 0x8003_51b0]);
                r.0[0] = e.wrapping_sub(2);
                let bad = |c: &mut Cpu, r: &mut Saved, ras: [u32; 2]| {
                    let at = port(c, r, 0x8005_072c, ras[0], &[s6.wrapping_add(1)], crate::memcard::abi_getcard);
                    r.0[0] = at;
                    let app = c.m.r32(FE_APP);
                    port(c, r, 0x8001_8a44, ras[1], &[app + 720], crate::fedialog::abi_hide);
                    let state = c.m.r32(at);
                    r.0[0] = state;
                    let (s2, s3) = match state as i32 {
                        -2 => (672, 2),
                        -1 => (812, 3),
                        _ => (810, 1),
                    };
                    (r.0[2], r.0[3], r.0[1]) = (s2, s3, 1);
                };
                match e {
                    22 => {
                        port(c, &r, 0x8004_fba0, 0x8003_51ec, &[card, sp + 5720, 0], crate::memcard::abi_loadfile);
                    }
                    15 => {
                        if port(c, &r, 0x8003_4448, 0x8003_51fc, &[sp + 16], abi_verify_successful_read) == 0 {
                            bad(c, &mut r, [0x8003_52fc, 0x8003_5310]);
                            continue;
                        }
                        r.0[3] = 0;
                        if quiet != 0 {
                            let id = c.m.r32(sp + 5284);
                            c.m.w32(s7 + 1084, id);
                            port(c, &r, 0x8001_6cb0, 0x8003_52c0, &[CAR_MANAGER, sp + 1128, p16], crate::fecars::abi_load_pink_slips_cars);
                            r.call_leaving(c, SPRINTF, &[FE + 0x364 + fp, sp + 892], 0x8003_52d8);
                            let v = c.m.r32(sp + 1368);
                            c.m.w32(s7 + 76, v);
                        } else {
                            if p16 != 0 {
                                r.call_leaving(c, SPRINTF, &[FE + 0x364 + fp, sp + 892], 0x8003_5284);
                            } else {
                                r.call_leaving(c, BLOCKMOVE, &[sp + 24, FE, 1104], 0x8003_522c);
                                port(c, &r, 0x8003_3680, 0x8003_523c, &[TOURNAMENT_MANAGER, sp + 1368], crate::fetourn::abi_load_tournament);
                                port(c, &r, 0x8002_1bb0, 0x8003_524c, &[TRACK_MANAGER, sp + 1352], crate::fetracks::abi_load_tracks);
                                r.call_leaving(c, BLOCKMOVE, &[sp + 1544, TRACK_RECORDS, 3740], 0x8003_525c);
                                port(c, &r, 0x8002_3778, 0x8003_5268, &[sp + 20], crate::fecheats::abi_load_bonus);
                            }
                            port(c, &r, 0x8001_6b9c, 0x8003_529c, &[CAR_MANAGER, sp + 1128, p16], crate::fecars::abi_load_cars);
                        }
                        r.0[2] = 654;
                        r.enter_leaving(c, 0x8002_752c, 0x8003_52ec, crate::front::abi_set_pads);
                        r.0[1] = 1;
                    }
                    2 | 3 | 7 | 10 | 11 | 16 | 23 => bad(c, &mut r, [0x8003_5338, 0x8003_534c]),
                    _ => {}
                }
            }
            r.0[0] = 22;
            wait_event(c, &r, card, 22, false, 0x8003_5390, 0x8003_53a0, 0x8003_53a8);
            r.enter_leaving(c, 0x8003_4544, 0x8003_53b8, abi_bring_back);
            // (a retry polls nothing: the done flag stays set)
            if r.0[2] == 654 || (c.m.r32(sp + 5780) as i32) >= 3 {
                break;
            }
        }
        if pinks != 0 && c.m.r32(CARD_DONE) == 0 {
            r.0[1] = 0x8005_0000;
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_8a44, 0x8003_5408, &[app + 720], crate::fedialog::abi_hide);
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_8a44, 0x8003_5414, &[app + 568], crate::fedialog::abi_hide);
            let word = r.0[2].wrapping_add(p16);
            tell(c, &mut r, word, [0x8003_5430, 0x8003_5440, 0x8003_5464, 0x8003_5474]);
        }
        r.0[0] = FE;
        let mode = c.m.r8(p16.wrapping_add(FE) + 866) as u32;
        port(c, &r, 0x8002_7104, 0x8003_5498, &[p16, mode], crate::front::abi_reset_psx_controller);
        if quiet == 0 {
            if c.m.r8(FE + 4) == 6 {
                c.m.w8(FE + 4, 0);
            }
            let sfx = c.m.r8(FE + 834) as u32;
            c.m.w32(SFX_LEVEL, sfx);
        }
        if pinks != 0 || quiet != 0 {
            port(c, &r, 0x8001_8a44, 0x8003_54f0, &[d], crate::fedialog::abi_hide);
        }
        if pinks != 0 {
            redraw(c, &r, 0x8003_550c);
        }
        c.m.w32(CARD_ACTIVE, 0);
        let screen = c.m.r32(MEMCARD_SCREEN);
        let front_end2 = c.m.r32(USE_FRONT_END2);
        c.m.w32(screen + 1440, 1);
        if front_end2 != 0 {
            let language = c.m.r8(sp + 5776);
            c.m.w8(LANGUAGE, language);
        }
        port(c, &r, 0x8002_6098, 0x8003_554c, &[d, 2], crate::fescreen::abi_screen_destructor);
        c.r[2] = r.0[3];
    })
}

/// `SavePinkSlipsCars` (FEMEMCARD.CPP, 0x80035580): player `p`'s pink slips' cars (`which`)
/// saved into their save: the save read and checked (its id must be the player's), the cars
/// put in, checksummed and written. 0 done, else the error: 1 another save, 2 none or bad, 3
/// an error, 4 no card, 5 unformatted, 6 full.
pub fn abi_save_pink_slips_cars(c: &mut Cpu) {
    let (p, which) = (c.r[4], c.r[5]);
    abi::frame(c, 5744, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[7]) = (p, p);
        c.m.w16(sp + 5680, which as u16);
        r.enter_leaving(c, 0x8003_4520, 0x8003_55bc, abi_make_way);
        let p16 = p as u16 as i16 as i32 as u32;
        let card = (p16 << 2).wrapping_add(1);
        (r.0[0], r.0[3], r.0[5], r.0[2], r.0[1]) = (p16, 0, 0, 0, sp + 5568);
        c.m.w16(sp + 5688, card as u16);
        (r.0[4], r.0[6]) = (card << 16, 21);
        let path = c.m.r32(CD_PATH);
        c.m.w32(CARD_PLAYER, p16);
        c.m.w32(CARD_ACTIVE, 1);
        c.m.w32(CARD_DONE, 0);
        r.call_leaving(c, SPRINTF, &[sp + 5568, ICON_FILE, path], 0x8003_5620);
        let icon = r.call_leaving(c, LOAD_SHAPE, &[sp + 5568, 0], 0x8003_562c);
        r.0[8] = icon;
        port(c, &r, 0x8003_49e0, 0x8003_5640, &[sp + 5632, card as u16 as i16 as i32 as u32, 1], abi_init_memcard_file);
        let card = card as u16 as i16 as i32 as u32;
        wait_event(c, &r, card, 21, true, 0x8003_5648, 0x8003_5658, 0x8003_5660);
        let s4 = p16 << 2;
        (r.0[7], r.0[4]) = (p16, s4);
        c.m.w32(sp + 5696, s4.wrapping_add(FE));
        c.m.w32(sp + 5652, sp + 16);
        c.m.w32(sp + 5648, 0);
        r.0[1] = card << 16;
        while r.0[3] == 0 {
            let e = poll(c, &mut r, card, [0x8003_56a4, 0x8003_56ac, 0x8003_56b8, 0x8003_56c0]);
            let bad = |c: &mut Cpu, r: &mut Saved| {
                let at = port(c, r, 0x8005_072c, 0x8003_587c, &[s4.wrapping_add(1)], crate::memcard::abi_getcard);
                r.0[2] = match c.m.r32(at) as i32 {
                    -2 => 5,
                    -1 => 4,
                    _ => 2,
                };
                r.0[3] = 1;
            };
            match e {
                22 => {
                    port(c, &r, 0x8004_fba0, 0x8003_56fc, &[card, sp + 5632, 0], crate::memcard::abi_loadfile);
                }
                15 => {
                    if port(c, &r, 0x8003_4448, 0x8003_570c, &[sp + 16], abi_verify_successful_read) == 0 {
                        bad(c, &mut r);
                        continue;
                    }
                    r.0[3] = 1;
                    let fe = c.m.r32(sp + 5696);
                    if c.m.r32(fe + 1084) != c.m.r32(sp + 5284) {
                        r.0[2] = 1;
                        continue;
                    }
                    let which = c.m.r16(sp + 5680) as i16 as i32 as u32;
                    port(c, &r, 0x8001_6d54, 0x8003_5758, &[CAR_MANAGER, sp + 1128, p16, which], crate::fecars::abi_save_pink_slips_cars);
                    r.0[0] = 21;
                    wait_event(c, &r, card, 21, true, 0x8003_5764, 0x8003_5774, 0x8003_577c);
                    c.m.w32(sp + 5648, 512);
                    let shape = r.call_leaving(c, SHAPE_POINTER, &[icon, 0], 0x8003_5794);
                    c.m.w32(sp + 5656, shape);
                    c.m.w32(sp + 5660, 0);
                    c.m.w32(sp + 5664, 0);
                    port(c, &r, 0x8003_43d0, 0x8003_57a8, &[sp + 16], abi_checksum_data);
                    (r.0[6], r.0[3]) = (0x8001_17A8, crate::memcard::STATE);
                    while r.0[5] == 0 {
                        let e = poll(c, &mut r, card, [0x8003_57c8, 0x8003_57d0, 0x8003_57dc, 0x8003_57e4]);
                        r.0[0] = e.wrapping_sub(2);
                        match e {
                            22 => {
                                if c.m.r32(crate::memcard::STATE + 52) != 0 {
                                    port(c, &r, 0x8004_ff34, 0x8003_5820, &[card, sp + 5632], crate::memcard::abi_savefile);
                                }
                            }
                            12 => {
                                (r.0[5], r.0[2]) = (1, 0);
                            }
                            2 | 3 | 7 | 10 | 11 | 13 | 23 => {
                                let app = c.m.r32(FE_APP);
                                port(c, &r, 0x8001_8a44, 0x8003_5844, &[app + 720], crate::fedialog::abi_hide);
                                let at = port(c, &r, 0x8005_072c, 0x8003_584c, &[s4.wrapping_add(1)], crate::memcard::abi_getcard);
                                r.0[2] = match c.m.r32(at) as i32 {
                                    -3 => 6,
                                    -1 => 4,
                                    _ => 3,
                                };
                                r.0[5] = 1;
                            }
                            _ => {}
                        }
                    }
                    r.0[3] = 1;
                }
                2 | 16 => bad(c, &mut r),
                3 | 7 | 10 | 11 | 23 => {
                    (r.0[3], r.0[2]) = (1, 2);
                }
                _ => {}
            }
        }
        (r.0[1], r.0[0]) = (22, card << 16);
        wait_event(c, &r, card, 22, false, 0x8003_58c4, 0x8003_58d4, 0x8003_58dc);
        r.call_leaving(c, PURGE, &[icon], 0x8003_58ec);
        r.enter_leaving(c, 0x8003_4544, 0x8003_58f4, abi_bring_back);
        c.m.w32(CARD_ACTIVE, 0);
        c.r[2] = r.0[2];
    })
}

/// `SavePinkSlipsCarsWithErrorDialogs` (FEMEMCARD.CPP, 0x80035930): the pink slips' cars
/// saved ("saving" shown, up to three tries a moment apart); on an error its message (with the
/// winner's or loser's name, `kind` 1 or 2) and a retry asked. The result.
pub fn abi_save_pink_slips_with_dialogs(c: &mut Cpu) {
    let (p, kind, which) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 1536, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[1], r.0[8]) = (p, which, kind);
        let ask = sp + 16;
        port(c, &r, 0x8001_9bcc, 0x8003_5970, &[ask], crate::fedialog::abi_yes_no);
        let note = sp + 184;
        r.0[0] = note;
        message_box(c, &r, note, 0x8003_597c);
        let p16 = p as u16 as i16 as i32 as u32;
        (r.0[6], r.0[3], r.0[4], r.0[5], r.0[7]) = (0, 0x8005_0000, sp + 1344, p16, which << 16);
        let m = &mut c.m;
        m.w32(ask + 160, 658);
        m.w32(ask + 164, 659);
        m.w16(ask + 136, 1);
        m.w16(ask + 100, p as u16);
        loop {
            let app = c.m.r32(FE_APP);
            port(c, &r, 0x8001_8980, 0x8003_5a34, &[app + 720], crate::fedialog::abi_display);
            while app_word(c, 852) != 1 {
                redraw(c, &r, 0x8003_5a58);
            }
            r.0[0] = 0;
            redraw(c, &r, 0x8003_5a68);
            let saving = sp + 1344;
            message_box(c, &r, saving, 0x8003_5a70);
            let text = r.call_leaving(c, TEXT_WORD, &[p16.wrapping_add(631)], 0x8003_5aec);
            let m = &mut c.m;
            m.w32(saving + 144, text);
            m.w16(saving + 124, 0);
            m.w16(saving + 126, 50);
            port(c, &r, 0x8001_8980, 0x8003_5b04, &[saving], crate::fedialog::abi_display);
            while c.m.r32(saving + 132) != 1 {
                redraw(c, &r, 0x8003_5b24);
            }
            redraw(c, &r, 0x8003_5b38);
            let which16 = which as u16 as i16 as i32 as u32;
            let mut result;
            let mut k = 0u32;
            loop {
                result = r.enter_leaving(c, 0x8003_5580, 0x8003_5b44, |c| {
                    (c.r[4], c.r[5]) = (p16, which16);
                    abi_save_pink_slips_cars(c);
                    c.r[2]
                });
                r.0[2] = result;
                if result != 0 {
                    r.call_leaving(c, TIMED_WAIT, &[5], 0x8003_5b58);
                }
                k += 1;
                r.0[0] = k;
                if result == 0 || k >= 3 {
                    break;
                }
            }
            port(c, &r, 0x8001_8a44, 0x8003_5b74, &[saving], crate::fedialog::abi_hide);
            redraw(c, &r, 0x8003_5b80);
            if result != 0 {
                let app = c.m.r32(FE_APP);
                port(c, &r, 0x8001_8a44, 0x8003_5b94, &[app + 720], crate::fedialog::abi_hide);
                let word = c.m.r32(ERROR_WORDS + (result << 2)).wrapping_add(p16);
                let text = r.call_leaving(c, TEXT_WORD, &[word], 0x8003_5bb0);
                r.call_leaving(c, SPRINTF, &[sp + 336, text], 0x8003_5bbc);
                let kind16 = kind as u16 as i16;
                if kind16 != 0 {
                    let word = if kind16 == 2 { 666 } else { 665 };
                    let format = r.call_leaving(c, TEXT_WORD, &[word], 0x8003_5be0);
                    r.0[0] = format;
                    let name = port(c, &r, 0x8002_ba7c, 0x8003_5bec, &[p16], crate::front::abi_player_name);
                    r.0[1] = sp + 840;
                    r.call_leaving(c, SPRINTF, &[sp + 840, format, name], 0x8003_5c00);
                    let m = &mut c.m;
                    m.w32(note + 144, sp + 840);
                    m.w16(sp + 308, 0);
                    m.w16(sp + 310, (-60i16) as u16);
                    port(c, &r, 0x8001_8980, 0x8003_5c18, &[note], crate::fedialog::abi_display);
                }
                c.m.w32(ask + 144, sp + 336);
                let answer = port(c, &r, 0x8001_9928, 0x8003_5c28, &[ask], crate::fedialog::abi_run) as u16 as i16 as i32 as u32;
                r.0[6] = answer;
                port(c, &r, 0x8001_8a44, 0x8003_5c38, &[note], crate::fedialog::abi_hide);
            }
            port(c, &r, 0x8002_6098, 0x8003_5c44, &[saving, 2], crate::fescreen::abi_screen_destructor);
            if result == 0 || r.0[6] == 0 {
                port(c, &r, 0x8002_6098, 0x8003_5c5c, &[note, 2], crate::fescreen::abi_screen_destructor);
                port(c, &r, 0x8002_6098, 0x8003_5c68, &[ask, 2], crate::fescreen::abi_screen_destructor);
                c.r[2] = result;
                return;
            }
        }
    })
}

/// FEMEMCARD.CPP's ports entered as the original's callers `jal` them.
macro_rules! entries {
    ($($entry:ident = $addr:expr, $f:ident;)*) => {
        $(pub(crate) fn $entry(c: &mut Cpu) {
            abi::leave_prologue(c, $addr, None, &[]);
            $f(c);
        })*
    };
}

entries! {
    init_memcard_entry = 0x8003_47d4, abi_init_memcard;
    deinit_memcard_entry = 0x8003_4938, abi_deinit_memcard;
    load_game_entry = 0x8003_4f70, abi_load_game;
    save_pink_slips_with_dialogs_entry = 0x8003_5930, abi_save_pink_slips_with_dialogs;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn fememcard_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x991 + k as u64, shuffle, &args);
        }
    }
}
