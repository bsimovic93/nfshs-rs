//! FRONT.CPP: the front end's structure (`frontEnd`, 0x801158B8) and its glue: the defaults
//! (players' settings and cars, the pads' sticks, the levels), the pads' values and modes, the
//! memory card's checks at the start, the graphics' start, the laps, the players' names and
//! licence plates, the music's play list, the track raced; the race's stream (`Front_Init*`,
//! `Front_Append*`) and the front end's screens and run.
//!
//! `frontEnd` (bytes but where noted): +0 a half, +2..+7 the race's kind and mode (+3 the
//! players, 1 two; +4 the kind: 0 a free race, 1 single race, 2 a tournament, 6 pink slips;
//! +6 the difficulty), +9, +10 on, +12 the pink slips' winners (a half per slot), +22.. five
//! per-race slots (+22 night, +27 weather, +35 the tracks, +45 the laps' choice, +50 the
//! direction, +55 mirrored, +60 traffic), +40 the record lap time (a word), +70 the race's
//! slot, +72, +73 the pink slips' wins, +84..+88 halves, +90 a half (the pink slips' winner),
//! +92 the stock cars, +94 per player, +96 the cars' colours (48 per player), +192 the cop
//! lights, +288.., +291 the garage cars, +293 the pink slips' cars, +298 the options on, +299,
//! +300 the mission's tier and number, +304 the cars' flags (a half per player), +500/+596/+692
//! the cars' settings (48 per player), +788.. per player (lights, +791..+795 every other the
//! driving aids), +797 the opponent's car, +798 the opponents (1 one, 2 a class's), +800 the
//! view, +804..+819 the dashboard, +824 (4 per player), +832 the language, +833..+837 the
//! levels (music, effects, narration, engine, ambient), +838, +841..+855 the analog sticks'
//! calibrations (every other byte by pad), +857, +859, +862 the pads' kinds (halves), +866 the
//! pads' modes, +868 (8 per player), +0x364 the names, +0x374 in capitals, +0x384 the licences
//! (8 each), +916..+921, +924 the songs on (40 words), +1092 the play list's switch, +1096 the
//! pads' vibration (words).

use crate::call::{call, call_addr, with_buffer};
use crate::globals::{FE_APP, FRONT_END, GAME_SETUP, GAME_TICKS, PAD_INFO, PATHS};
use crate::types::{CarManager, FEApplication, Screen};
use crate::{fecars, fecheats, fememcard, fescreen, fetracks};
use ps1rt::{Cpu, Mem};

const FE: u32 = FRONT_END;
/// `frontEnd`'s fields (see above).
const PLAYERS: u32 = 3;
const RACE_KIND: u32 = 4;
const TRACKS: u32 = 35;
const RECORD_LAP: u32 = 40;
const LAPS_CHOICE: u32 = 45;
const TRAFFIC: u32 = 60;
const SLOT: u32 = 70;
const CAR_COLOURS: u32 = 96;
const GARAGE_CAR: u32 = 291;
const PINK_SLIPS_CARS: u32 = 293;
const OPTIONS_ON: u32 = 298;
const MISSION_TIER: u32 = 299;
const MISSION: u32 = 300;
const CAR_FLAGS: u32 = 304;
const LANGUAGE: u32 = 832;
const CALIBRATIONS: u32 = 841;
const PAD_KINDS: u32 = 862;
const PAD_MODES: u32 = 866;
const NAMES: u32 = 0x364;
const CAPS_NAMES: u32 = 0x374;
const LICENCES: u32 = 0x384;
const SONGS_ON: u32 = 924;
const PLAY_LIST_ON: u32 = 1092;
const VIBRATION: u32 = 1096;
/// The race's kinds.
const SINGLE_RACE: u8 = 1;
const TOURNAMENT_RACE: u8 = 2;
const PINK_SLIPS_RACE: u8 = 6;
/// A stable pad (`PadGetState`).
const PAD_STABLE: u32 = 6;
/// A track's information (`tTrackInfo`, 48 bytes): +27 its language.
const TRACK_INFO_SIZE: u32 = 48;
const TRACK_LANGUAGE: u32 = 27;

/// The managers: the cars', the tracks', the tournaments', the credits'.
const CAR_MANAGER: CarManager = CarManager(0x8011_552C);
const TRACK_MANAGER: u32 = 0x8011_5D08;
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const CREDIT_MANAGER: u32 = 0x8011_6014;
/// The screens (`tAllScreens`), the menus (FEMENUDEFS').
const ALL_SCREENS: u32 = 0x8005_1DBC;
const ALL_MENUS: u32 = 0x8005_1EE0;
/// The levels: music, effects, engine, narration, ambient.
const MUSIC_LEVEL: u32 = 0x8013_D944;
const SFX_LEVEL: u32 = 0x8013_D948;
const ENGINE_LEVEL: u32 = 0x8013_D94C;
const NARRATION_LEVEL: u32 = 0x8013_D950;
const AMBIENT_LEVEL: u32 = 0x8013_D954;
/// The front end's use (`gUseFrontend` +0xc the big file's handle, +0x10, +0x14); the memory
/// card's state words; the records (`Stats_gTrackRecords`).
const BIG_FILE: u32 = 0x8013_E558;
const USE_FRONT_END: u32 = 0x8013_E55C;
const USE_FRONT_END2: u32 = 0x8013_E560;
const CARD_STATE: u32 = 0x8005_1D98;
const CARD_RETRY: u32 = 0x8005_1DC0;
const CARD_FIRST: u32 = 0x8005_1C50;
const TRACK_RECORDS: u32 = 0x8011_604C;
/// The formats and names: `"%szzFE.viv"`, `"ymus"`, `"game*"`, `"amus"`; the CD's front-end
/// path (`Paths` +0x8c).
const FE_VIV: u32 = 0x8001_144C;
const YMUS: u32 = 0x8001_1458;
const GAME_SONGS: u32 = 0x8001_1460;
const AMUS: u32 = 0x8001_1468;
const FE_PATH: u32 = PATHS + 0x8C;

const PAD_UPDATE: fn(&mut Cpu) = nfs4rc::f_800e4e08;
const TIMED_WAIT: fn(&mut Cpu) = nfs4rc::f_800e8e70;
const PAD_SET_MAIN_MODE: fn(&mut Cpu) = nfs4rc::f_800f0ac0;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const CLEAN_UP_LICENSE: fn(&mut Cpu) = nfs4rc::f_800bd3d8;
const CREATE_LICENSE: fn(&mut Cpu) = nfs4rc::f_800bd044;
const INIT_LINK_MODE: fn(&mut Cpu) = nfs4rc::f_800f1328;
const RESET_DCT_BUFFER: fn(&mut Cpu) = nfs4rc::f_800dd0ac;
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
const MUSIC_CLEAN_UP: fn(&mut Cpu) = nfs4rc::f_8007b720;
const MUSIC_START_UP: fn(&mut Cpu) = nfs4rc::f_8007b5ac;
/// The songs matching a name (a count, then 64 bytes each: +36 the song's number).
const MUSIC_FIND: fn(&mut Cpu) = nfs4rc::f_8007b214;
/// The host's: `PadGetState`, `PadInfoMode`, `FILE_addbigsync`, `NFS4_LoadingIcon` (hooked).
const PAD_GET_STATE: u32 = 0x800F_07FC;
const PAD_INFO_MODE: u32 = 0x800F_08BC;
const FILE_ADD_BIG_SYNC: u32 = 0x800E_B354;
const LOADING_ICON: u32 = 0x800A_512C;

/// `Front_DeleteAll` (FRONT.CPP, 0x80027008): the screens, the application and the menus
/// deleted (those there).
pub fn delete_all(c: &mut Cpu) {
    let screens = c.m.r32(ALL_SCREENS);
    if screens != 0 {
        all_screens_dtor(c, screens, 3);
    }
    let app = c.m.r32(FE_APP);
    if app != 0 {
        crate::feapp::fe_application_dtor(c, FEApplication(app), 3);
    }
    let menus = c.m.r32(ALL_MENUS);
    if menus != 0 {
        crate::femenudefs::global_menu_defs_destructor(c, menus, 3);
    }
}

pub fn abi_delete_all(c: &mut Cpu) {
    delete_all(c);
}

/// `Front_ResetSettingsForCar` (FRONT.CPP, 0x8002706c): player `p`'s car `car` set back: its
/// flag off, its settings 50, 50 and 3.
pub fn reset_settings_for_car(m: &mut Mem, p: u32, car: u32) -> u32 {
    let flags = FE + CAR_FLAGS + 2 * p;
    let v = m.r16(flags) as u32 & !(1u32 << (car & 31));
    m.w16(flags, v as u16);
    let at = car.wrapping_add(p.wrapping_mul(48)).wrapping_add(FE);
    m.w8(at + 596, 50);
    m.w8(at + 500, 50);
    m.w8(at + 692, 3);
    3
}

pub fn abi_reset_settings_for_car(c: &mut Cpu) {
    c.r[2] = reset_settings_for_car(&mut c.m, c.r[4], c.r[5]);
}

/// `Front_ResetSettingsMenu` (FRONT.CPP, 0x800270bc): every car of player `p` set back.
pub fn reset_settings_menu(m: &mut Mem, p: u32) {
    for car in 0..48 {
        reset_settings_for_car(m, p, car);
    }
}

pub fn abi_reset_settings_menu(c: &mut Cpu) {
    reset_settings_menu(&mut c.m, c.r[4]);
}

/// `Front_ResetPSXController` (FRONT.CPP, 0x80027104): a moment waited, the pads read; pad
/// `pad`'s kind noted, its mode `mode`.
pub fn reset_psx_controller(c: &mut Cpu, pad: u32, mode: u32) {
    call(c, TIMED_WAIT, &[10]);
    call(c, PAD_UPDATE, &[]);
    let m = &mut c.m;
    let kind = m.r8(PAD_INFO + 32 * pad + 5);
    m.w16(FE + PAD_KINDS + 2 * pad, kind as u16);
    m.w8(FE + PAD_MODES + pad, mode as u8);
}

pub fn abi_reset_psx_controller(c: &mut Cpu) {
    reset_psx_controller(c, c.r[4], c.r[5]);
}

/// For callers not rewritten yet.
pub(crate) fn reset_psx_controller_entry(c: &mut Cpu) {
    abi_reset_psx_controller(c);
}

/// `Front_ResetPSXAnalogs` (FRONT.CPP, 0x80027170): pad `pad`'s sticks' calibrations, the
/// defaults.
pub fn reset_psx_analogs(m: &mut Mem, pad: u32) {
    let at = pad.wrapping_add(FE);
    for (k, v) in [121, 10, 255, 255, 127, 6, 127, 6].into_iter().enumerate() {
        m.w8(at + CALIBRATIONS + 2 * k as u32, v);
    }
}

pub fn abi_reset_psx_analogs(c: &mut Cpu) {
    reset_psx_analogs(&mut c.m, c.r[4]);
}

/// `GetPSXPadValue` (FRONT.CPP, 0x800271b4): the pads read; key `key` of pad `pad` as the
/// game's controls take it: an analog pad's (0x53, 0x73) stick directions and a NeGcon's
/// (0x23) twist, buttons and shoulder with their calibrations (low bit 1), else the key itself
/// (`pad << 26 | key << 8 | 2`).
pub fn get_psx_pad_value(c: &mut Cpu, key: u32, pad: u32) -> u32 {
    call(c, PAD_UPDATE, &[]);
    let m = &mut c.m;
    let info = PAD_INFO + (pad << 5);
    let kind = if m.r8(info + 4) == 0 { m.r8(info + 5) as u32 } else { 0 };
    let o = |m: &mut Mem, k: u32| m.r8(pad.wrapping_add(FE + k)) as u32;
    let up = |v: u32| v.wrapping_add(128);
    let down = |v: u32| 127u32.wrapping_sub(v);
    let sh = pad << 30;
    match (kind, key) {
        (0x53 | 0x73, 0x8000_0000) => sh | (down(o(m, 855)) << 16) | 0x200_0000 | (down(o(m, 853)) << 8) | 1,
        (0x53 | 0x73, 0x10_0000) => sh | (down(o(m, 851)) << 16) | 0x100_0000 | (down(o(m, 849)) << 8) | 1,
        (0x53 | 0x73, 0x20_0000) => sh | (up(o(m, 851)) << 16) | (up(o(m, 849)) << 8) | 1,
        (0x53 | 0x73, 0x40_0000) => sh | (up(o(m, 851)) << 16) | 0x100_0000 | (up(o(m, 849)) << 8) | 1,
        (0x53 | 0x73, 0x80_0000) => sh | (down(o(m, 851)) << 16) | (down(o(m, 853)) << 8) | 1,
        (0x53 | 0x73, 0x1000_0000) => sh | (down(o(m, 855)) << 16) | 0x300_0000 | (down(o(m, 853)) << 8) | 1,
        (0x53 | 0x73, 0x2000_0000) => sh | (up(o(m, 855)) << 16) | 0x200_0000 | (up(o(m, 853)) << 8) | 1,
        (0x53 | 0x73, 0x4000_0000) => sh | (up(o(m, 855)) << 16) | 0x300_0000 | (up(o(m, 853)) << 8) | 1,
        (0x23, 0x8000) => sh | 0x200_0000 | (o(m, 847) << 8) | 1,
        (0x23, 0x4000) => sh | 0x100_0000 | (o(m, 845) << 8) | 1,
        (0x23, 0x400) => sh | 0x30A_FF01,
        (0x23, 0x20_0000) => sh | (up(o(m, 843)) << 16) | (up(o(m, 841)) << 8) | 1,
        (0x23, 0x80_0000) => sh | (down(o(m, 843)) << 16) | (down(o(m, 841)) << 8) | 1,
        _ => (pad << 26) | (key << 8) | 2,
    }
}

pub fn abi_get_psx_pad_value(c: &mut Cpu) {
    c.r[2] = get_psx_pad_value(c, c.r[4], c.r[5]);
}

/// `SetPads` (FRONT.CPP, 0x8002752c): each pad, once stable (waited for up to 128 ticks), put
/// in the mode it has (4, or 7 when it vibrates) if it lists it.
pub fn set_pads(c: &mut Cpu) {
    for p in 0..2u32 {
        let want = if c.m.r32(FE + VIBRATION + 4 * p) != 0 { 7 } else { 4 };
        let port = p << 4;
        let start = c.m.r32(GAME_TICKS);
        loop {
            let now = c.m.r32(GAME_TICKS);
            if (now.wrapping_sub(start) as i32) >= 128 || call_addr(c, PAD_GET_STATE, &[port]) == PAD_STABLE {
                break;
            }
        }
        if call_addr(c, PAD_GET_STATE, &[port]) != PAD_STABLE {
            continue;
        }
        // the mode wanted among those the pad lists (the last such)
        let count = call_addr(c, PAD_INFO_MODE, &[port, 4, u32::MAX]);
        let mut found = None;
        let mut k = 0u32;
        while (k as i32) < count as i32 {
            if call_addr(c, PAD_INFO_MODE, &[port, 4, k]) == want {
                found = Some(k);
            }
            k += 1;
        }
        if let Some(k) = found {
            call(c, PAD_SET_MAIN_MODE, &[port, k, 0]);
        }
    }
}

pub fn abi_set_pads(c: &mut Cpu) {
    set_pads(c);
}

/// `InitFrontEndStructure` (FRONT.CPP, 0x80027668): the front end's defaults: the race's
/// settings and slots cleared, the record lap time of the first track; for each player the
/// settings, the cars' settings from their cars, the pads' kinds and sticks (and the pads
/// reset); every car's settings; the levels 85, the language 0; the credits set up.
pub fn init_front_end_structure(c: &mut Cpu) {
    let m = &mut c.m;
    m.w16(FE, 0);
    m.w8(FE + 7, 1);
    m.w8(FE + 2, 0);
    for at in [84, 86, 88] {
        m.w16(FE + at, 0);
    }
    for at in [3, 4, 6] {
        m.w8(FE + at, 0);
    }
    m.w8(FE + 916, 20);
    for k in 0..5 {
        for at in [35, 45, 55, 50, 22, 27, 60, 65, 919, 92] {
            m.w8(FE + k + at, 0);
        }
    }
    let track = m.r8(FE + TRACKS) as u32;
    for at in [33, 34, 296, 297] {
        m.w8(FE + at, 0);
    }
    m.w8(FE + 790, 1);
    m.w8(FE + 44, 0);
    let lap = crate::stattool::return_record_lap_time(c, track);
    let m = &mut c.m;
    m.w32(FE + RECORD_LAP, lap);
    for at in [9, 10, 918] {
        m.w8(FE + at, 1);
    }
    for at in [288, 289, 290, 295, 299, 300, 799] {
        m.w8(FE + at, 0);
    }
    // each player's settings; their cars' colours the cars' own
    let mut base = 0u32;
    for p in 0..2u32 {
        let (fe1, fe4, fe2) = (FE + p, FE + 4 * p, FE + 2 * p);
        let m = &mut c.m;
        m.w32(fe4 + VIBRATION, 1);
        m.w8(fe1 + 788, 0);
        m.w8(fe1 + 94, 1);
        m.w8(fe1 + 800, 0);
        for at in [802, 804, 806, 808, 810, 812] {
            m.w8(fe1 + at, 1);
        }
        m.w8(fe1 + 814, 0);
        m.w8(fe1 + 72, 0);
        m.w32(fe4 + 76, 0);
        m.w16(FE + 90, 0xFFFF);
        m.w8(fe4 + 817, 3);
        m.w8(fe4 + 818, 6);
        m.w8(fe4 + 819, 7);
        m.w8(fe4 + 816, 0);
        m.w8(FE + (p << 3) + 868, 0);
        for k in 0..4 {
            m.w8(FE + k + 4 * p + 824, 0);
        }
        for at in [791, 793, 795] {
            m.w8(fe1 + at, 1);
        }
        for k in 0..48u32 {
            let car = fecars::get_car_from_id(m, CAR_MANAGER, k as i32);
            let v = m.r8(car + 4);
            m.w8(FE + CAR_COLOURS + k + base, v);
        }
        m.w16(fe2 + PAD_KINDS, 65);
        m.w8(fe1 + PAD_MODES, 0);
        m.w8(fe1 + 921, 1);
        m.w8(fe1 + 857, 85);
        m.w8(fe1 + 859, 85);
        reset_psx_analogs(m, p);
        reset_psx_controller(c, p, 0);
        base += 48;
    }
    let m = &mut c.m;
    m.w8(FE + 917, 0);
    reset_settings_menu(m, 0);
    reset_settings_menu(m, 1);
    m.w8(FE + 798, 1);
    m.w8(FE + 838, 2);
    m.w8(FE + 797, 0);
    m.w8(FE + LANGUAGE, 0);
    for at in 833..838 {
        m.w8(FE + at, 85);
    }
    m.w8(FE + 839, 0);
    for level in [MUSIC_LEVEL, SFX_LEVEL, NARRATION_LEVEL, AMBIENT_LEVEL] {
        m.w32(level, 85);
    }
    m.w8(FE + 840, 0);
    m.w32(FE + PLAY_LIST_ON, 0);
    crate::fecredits::setup(m, CREDIT_MANAGER);
}

pub fn abi_init_front_end_structure(c: &mut Cpu) {
    init_front_end_structure(c);
}

/// `LoadConfig` (FRONT.CPP, 0x80027960): nothing; 0.
pub fn abi_load_config(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `Front_InitialMemCardCheck` (FRONT.CPP, 0x80027968): the card's state cleared, the loading
/// icon shown (the host's), the records' defaults, the front end's structure; language 0.
pub fn initial_mem_card_check(c: &mut Cpu) {
    for at in [USE_FRONT_END2, USE_FRONT_END, CARD_STATE, CARD_RETRY] {
        c.m.w32(at, 0);
    }
    call_addr(c, LOADING_ICON, &[]);
    c.m.w32(CARD_FIRST, 1);
    crate::stattool::get_all_default_records(c, TRACK_RECORDS, 0);
    init_front_end_structure(c);
    c.m.w8(FE + LANGUAGE, 0);
}

pub fn abi_initial_mem_card_check(c: &mut Cpu) {
    initial_mem_card_check(c);
}

/// For callers not rewritten yet.
pub(crate) fn initial_mem_card_check_entry(c: &mut Cpu) {
    initial_mem_card_check(c);
}

/// `Front_SecondaryMemCardCheck` (FRONT.CPP, 0x800279cc): the card started; each card checked
/// (up to three tries while it answers), the retry flag cleared; the card ended, the pads set.
pub fn secondary_mem_card_check(c: &mut Cpu) -> u32 {
    c.m.w32(USE_FRONT_END, 0);
    c.m.w32(USE_FRONT_END2, 1);
    fememcard::init_memcard(c, false, false);
    for card in 0..2u32 {
        if c.m.r32(CARD_RETRY) == 0 {
            for _ in 0..3 {
                let v = fememcard::load_game(c, card, false, false);
                let ok = (v << 16 == 0) as u32;
                c.m.w32(CARD_RETRY, ok);
                if ok != 0 {
                    break;
                }
            }
        }
        c.m.w32(CARD_RETRY, 0);
    }
    fememcard::deinit_memcard(c);
    set_pads(c);
    c.m.w32(USE_FRONT_END, 1);
    1
}

pub fn abi_secondary_mem_card_check(c: &mut Cpu) {
    c.r[2] = secondary_mem_card_check(c);
}

/// For callers not rewritten yet.
pub(crate) fn secondary_mem_card_check_entry(c: &mut Cpu) {
    abi_secondary_mem_card_check(c);
}

/// `Front_InitGraphics` (FRONT.CPP, 0x80027aa0): the front end's big file (`zzFE.viv`) opened
/// (stuck for ever when it can't be), the movie buffer reset, the PlayStation's front end
/// started.
pub fn init_graphics(c: &mut Cpu) {
    call(c, INIT_LINK_MODE, &[0, 1, 1]);
    let opened = with_buffer(c, 48, |c, name| {
        let path = c.m.r32(FE_PATH);
        call(c, SPRINTF, &[name, FE_VIV, path]);
        call_addr(c, FILE_ADD_BIG_SYNC, &[name, 0, 100, BIG_FILE])
    });
    if opened == 0 {
        loop {
            c.tick();
        }
    }
    call(c, RESET_DCT_BUFFER, &[]);
    crate::psxfront::init_psx_front_end(c);
}

pub fn abi_init_graphics(c: &mut Cpu) {
    init_graphics(c);
}

/// `Front_InitGraphicsAndDisplayLoading` (FRONT.CPP, 0x80027b18): `Front_InitGraphics`.
pub fn abi_init_graphics_and_display_loading(c: &mut Cpu) {
    init_graphics(c);
}

/// `Front_GetLapsForType` (FRONT.CPP, 0x80028004): the laps: a tournament's track's, else the
/// race slot's choice of 2 or 4 (the original's table is on its stack: a choice past those
/// reads its stack; here 0).
pub fn get_laps_for_type(m: &mut Mem) -> u32 {
    if m.r8(FE + RACE_KIND) != TOURNAMENT_RACE {
        let slot = m.r8(FE + SLOT) as u32;
        let choice = m.r8(slot + FE + LAPS_CHOICE) as usize;
        [2, 4].get(choice).copied().unwrap_or(0)
    } else {
        let t = TOURNAMENT_MANAGER;
        let (tier, definition) = (m.r32(t + 4), m.r32(t + 24));
        let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
        let tournament = first.wrapping_add(m.r32(t + 8));
        m.r8(definition.wrapping_add(tournament.wrapping_mul(84)) + 105) as u32
    }
}

pub fn abi_get_laps_for_type(c: &mut Cpu) {
    c.r[2] = get_laps_for_type(&mut c.m);
}

/// For callers not rewritten yet.
pub(crate) fn get_laps_for_type_entry(c: &mut Cpu) {
    abi_get_laps_for_type(c);
}

/// `Front_InitStream` (FRONT.CPP, 0x800280ac): the stream's counts cleared, the race's car list
/// made.
pub fn init_stream(m: &mut Mem, stream: u32) {
    for at in [0, 2, 4] {
        m.w16(stream + at, 0);
    }
    fecars::initialize_ingame_car_list(m);
}

pub fn abi_init_stream(c: &mut Cpu) {
    init_stream(&mut c.m, c.r[4]);
}

/// `Front_EnableLocalSpeech` (FRONT.CPP, 0x8002a724): a single race's track speaks its own
/// language (one of the first three, or the sixth) other than the front end's.
pub fn enable_local_speech(c: &mut Cpu) -> bool {
    if c.m.r8(FE + RACE_KIND) != SINGLE_RACE {
        return false;
    }
    let slot = c.m.r8(FE + SLOT) as u32;
    let track = c.m.r8(slot + FE + TRACKS) as u32;
    let language = with_buffer(c, TRACK_INFO_SIZE, |c, info| {
        fetracks::get_track(c, TRACK_MANAGER, track, info);
        c.m.r8(info + TRACK_LANGUAGE) as u32
    });
    language != c.m.r8(FE + LANGUAGE) as u32 && (language < 3 || language == 6)
}

pub fn abi_enable_local_speech(c: &mut Cpu) {
    c.r[2] = enable_local_speech(c) as u32;
}

/// For callers not rewritten yet.
pub(crate) fn enable_local_speech_entry(c: &mut Cpu) {
    abi_enable_local_speech(c);
}

/// `Front_GetInGameVars` (FRONT.CPP, 0x8002b750): the levels, the music's volume, the setup's
/// settings and each pad's mode and sticks back from the game's setup.
pub fn get_in_game_vars(c: &mut Cpu) {
    let music = c.m.r8(MUSIC_LEVEL);
    c.m.w8(FE + 833, music);
    let v = (music as u32).wrapping_mul(35) as i32 >> 6;
    call(c, MUSIC_VOLUME, &[v as u32]);
    let m = &mut c.m;
    let gs = GAME_SETUP.0;
    let (sfx, engine, narration, ambient) = (m.r8(SFX_LEVEL), m.r8(ENGINE_LEVEL), m.r8(NARRATION_LEVEL), m.r8(AMBIENT_LEVEL));
    let (a, b) = (m.r16(gs + 184), m.r8(gs + 236));
    m.w8(FE + 834, sfx);
    m.w8(FE + 836, engine);
    m.w8(FE + 835, narration);
    m.w8(FE + 837, ambient);
    m.w16(FE + 90, a);
    m.w8(FE + 838, b);
    for p in 0..2 {
        let t = gs + 4 * p;
        for (from, to) in [(96, PAD_MODES), (104, 843), (112, 841), (120, 847), (128, 845), (168, 857), (176, 859)] {
            let v = m.r8(t + from);
            m.w8(FE + p + to, v);
        }
    }
}

pub fn abi_get_in_game_vars(c: &mut Cpu) {
    get_in_game_vars(c);
}

/// `SetLicensePlate` (FRONT.CPP, 0x8002b84c): each player's licence (+0x384, 8 each) from their
/// name, its letters upper case then lower, and the car's plate made from it.
pub fn set_license_plate(c: &mut Cpu) {
    for p in 0..2u32 {
        let name = player_name(c, p);
        let licence = (p << 3) + FE + LICENCES;
        call(c, SPRINTF, &[licence, name]);
        crate::stattool::upper_case(&mut c.m, licence);
        crate::fetools::s_lower(c, licence);
        call(c, CLEAN_UP_LICENSE, &[p]);
        call(c, CREATE_LICENSE, &[licence, 0, p]);
    }
}

pub fn abi_set_license_plate(c: &mut Cpu) {
    set_license_plate(c);
}

/// For callers not rewritten yet.
pub(crate) fn set_license_plate_entry(c: &mut Cpu) {
    set_license_plate(c);
}

/// `SetPlayList` (FRONT.CPP, 0x8002b8f4): the in-game music's songs looked up (`game*`); unless
/// `keep`, the songs on are those; the play list switched on, the front end's music back.
pub fn set_play_list(c: &mut Cpu, keep: u32) {
    call(c, MUSIC_CLEAN_UP, &[]);
    call(c, MUSIC_START_UP, &[0xC000, 0x1_8000, YMUS]);
    let songs = call(c, MUSIC_FIND, &[GAME_SONGS, 0]);
    let m = &mut c.m;
    if keep == 0 {
        for k in (0..40).rev() {
            m.w32(FE + SONGS_ON + 4 * k, 0);
        }
        let mut k = 0u32;
        while (k as i32) < m.r32(songs) as i32 {
            let song = m.r32(songs.wrapping_add(64 * k) + 36);
            m.w32((song << 2).wrapping_add(FE) + SONGS_ON, 1);
            k += 1;
        }
    }
    m.w32(FE + PLAY_LIST_ON, 1);
    call(c, PURGE, &[songs]);
    call(c, MUSIC_CLEAN_UP, &[]);
    call(c, MUSIC_START_UP, &[0xD800, 0x1_8000, AMUS]);
}

pub fn abi_set_play_list(c: &mut Cpu) {
    set_play_list(c, c.r[4]);
}

/// For callers not rewritten yet.
pub(crate) fn set_play_list_entry(c: &mut Cpu) {
    abi_set_play_list(c);
}

/// `Front_GetTrackRaced` (FRONT.CPP, 0x8002b9e0): the track raced: a tournament's last, else
/// the race slot's track's number.
pub fn get_track_raced(c: &mut Cpu) -> u32 {
    let v = if c.m.r8(FE + RACE_KIND) == TOURNAMENT_RACE {
        crate::fetourn::get_last_track_raced(&mut c.m, TOURNAMENT_MANAGER)
    } else {
        let slot = c.m.r8(FE + SLOT) as u32;
        let track = c.m.r8(slot + FE + TRACKS) as u32;
        with_buffer(c, TRACK_INFO_SIZE, |c, info| {
            fetracks::get_track(c, TRACK_MANAGER, track, info);
            c.m.r8(info) as i8 as i32 as u32
        })
    };
    v as u16 as i16 as i32 as u32
}

pub fn abi_get_track_raced(c: &mut Cpu) {
    c.r[2] = get_track_raced(c);
}

/// For callers not rewritten yet.
pub(crate) fn get_track_raced_entry(c: &mut Cpu) {
    abi_get_track_raced(c);
}

/// `PlayerNameExist` (FRONT.CPP, 0x8002ba50): whether player `p` has a name.
pub fn player_name_exist(c: &mut Cpu, p: u32) -> bool {
    call(c, STRLEN, &[(p << 3).wrapping_add(FE + NAMES)]) != 0
}

pub fn abi_player_name_exist(c: &mut Cpu) {
    c.r[2] = player_name_exist(c, c.r[4]) as u32;
}

/// For callers not rewritten yet.
pub(crate) fn player_name_exist_entry(c: &mut Cpu) {
    abi_player_name_exist(c);
}

/// The word for player `p` without a name: `Player`, or `Player 1` and `2` with two.
fn player_word(c: &mut Cpu, p: u32) -> u32 {
    let word = if c.m.r8(FE + PLAYERS) == 1 { p.wrapping_add(81) } else { 79 };
    call(c, TEXT_WORD, &[word])
}

/// `PlayerName` (FRONT.CPP, 0x8002ba7c): player `p`'s name in capitals (copied to +0x374), or
/// the word for the player.
pub fn player_name(c: &mut Cpu, p: u32) -> u32 {
    let name = (p << 3).wrapping_add(FE + CAPS_NAMES);
    call(c, SPRINTF, &[name, (p << 3).wrapping_add(FE + NAMES)]);
    if call(c, STRLEN, &[name]) != 0 {
        crate::stattool::upper_case(&mut c.m, name);
        name
    } else {
        player_word(c, p)
    }
}

pub fn abi_player_name(c: &mut Cpu) {
    c.r[2] = player_name(c, c.r[4]);
}

/// For callers not rewritten yet.
pub(crate) fn player_name_entry(c: &mut Cpu) {
    abi_player_name(c);
}

/// `PlayerNameMixedCase` (FRONT.CPP, 0x8002bb08): player `p`'s name as typed, or the word for
/// the player.
pub fn player_name_mixed_case(c: &mut Cpu, p: u32) -> u32 {
    let name = (p << 3).wrapping_add(FE + NAMES);
    if call(c, STRLEN, &[name]) != 0 { name } else { player_word(c, p) }
}

pub fn abi_player_name_mixed_case(c: &mut Cpu) {
    c.r[2] = player_name_mixed_case(c, c.r[4]);
}

/// For callers not rewritten yet.
pub(crate) fn player_name_mixed_case_entry(c: &mut Cpu) {
    abi_player_name_mixed_case(c);
}

/// The race's stream (`tFEStream`): +0 its size so far (a half, 2 a car), +2 the cars' memory
/// (a half, 6 a car not an added model, up to 13), +6 the player cars (a half), +8 their
/// records (204 bytes: +0 the car, +2 its class, +175 the colours, +196, +197 the colour,
/// +199 a cop car's lights), +416 the opponents (a half), +420 the race's car list (20 bytes:
/// +0 a player's, +4 the personality, +8 the place, +12 the car, +16 its colour, +17 its
/// upgrades), +580 the tournament's traffic (a half, the cars from +582), +684 the opponents'
/// six cars.
const CAR_RECORD: u32 = 204;
const LIST: u32 = 420;
const COP_CLASS: u8 = 7;

/// Player car `k`'s record.
fn record(stream: u32, k: u32) -> u32 {
    stream.wrapping_add(8).wrapping_add(k.wrapping_mul(CAR_RECORD))
}

/// The stream's player cars so far (a `short`).
fn player_cars(c: &mut Cpu, stream: u32) -> u32 {
    c.m.r16(stream + 6) as i16 as i32 as u32
}

/// The last car's colour from its own colours.
fn own_colour(c: &mut Cpu, stream: u32) {
    let k = player_cars(c, stream);
    let rec = record(stream, k);
    let m = &mut c.m;
    let t1 = m.r8(rec + 197);
    let v = m.r8(rec.wrapping_add(t1 as u32) + 175);
    m.w8(rec + 197, v);
}

/// One more player car.
fn next_car(c: &mut Cpu, stream: u32) {
    let v = c.m.r16(stream + 6).wrapping_add(1);
    c.m.w16(stream + 6, v);
}

/// The race's car list's entry `k` (from +420, 20 bytes).
fn list_entry(stream: u32, k: u32) -> u32 {
    stream.wrapping_add(k.wrapping_mul(20)).wrapping_add(LIST)
}

/// A car (a word) and its colour (a byte) in `pair` for the car manager's line-up: whether
/// it's an added model; then added to the line-up.
fn added_model(m: &mut Mem, pair: u32) -> bool {
    fecars::is_car_an_added_model(m, CAR_MANAGER, pair, pair + 4)
}

fn add_to_lineup(m: &mut Mem, pair: u32) {
    fecars::add_car_to_ingame_list(m, CAR_MANAGER, pair, pair + 4);
}

/// The stream's cars' memory grown by `by` (up to `limit`).
fn more_memory(m: &mut Mem, stream: u32, by: u16, limit: i16) -> bool {
    let memory = m.r16(stream + 2);
    let room = (memory as i16) < limit;
    if room {
        m.w16(stream + 2, memory.wrapping_add(by));
    }
    room
}

/// `Front_InitPlayerCars` (FRONT.CPP, 0x800280dc): the players' cars into the stream: a
/// tournament's garage car; the pink slips' two; else each player's (two with two players)
/// stock car in its chosen colour, or garage car (a cop car's lights from the settings); then
/// each listed for the race (the memory of those not added models counted).
pub fn init_player_cars(c: &mut Cpu, stream: u32) {
    c.m.w16(stream + 6, 0);
    let kind = c.m.r8(FE + RACE_KIND);
    if kind == TOURNAMENT_RACE {
        let car = c.m.r8(FE + GARAGE_CAR) as u32;
        fecars::get_garage_car(c, CAR_MANAGER, car, stream + 8, 0);
        own_colour(c, stream);
        next_car(c, stream);
    } else if kind == PINK_SLIPS_RACE {
        let car = c.m.r8(FE + PINK_SLIPS_CARS) as u32;
        fecars::get_pink_slips_car(c, CAR_MANAGER, car, stream + 8, 0);
        own_colour(c, stream);
        next_car(c, stream);
        let k = player_cars(c, stream);
        let car = c.m.r8(FE + PINK_SLIPS_CARS + 1) as u32;
        fecars::get_pink_slips_car(c, CAR_MANAGER, car, record(stream, k), 1);
        own_colour(c, stream);
        next_car(c, stream);
    } else {
        let mut p = 0u32;
        loop {
            let k = player_cars(c, stream);
            let rec = record(stream, k);
            if c.m.r8(FE + OPTIONS_ON) == 0 {
                // the stock car in its chosen colour
                let car = c.m.r8(p + FE + 92) as u32;
                fecars::get_stock_car(c, CAR_MANAGER, car, rec);
                let k = player_cars(c, stream);
                let rec = record(stream, k);
                let m = &mut c.m;
                let model = m.r8(rec) as i8 as i32 as u32;
                let choice = m.r8(model.wrapping_add(p.wrapping_mul(48)).wrapping_add(FE) + CAR_COLOURS) as u32;
                let v = m.r8(rec.wrapping_add(choice) + 175);
                m.w8(rec + 197, v);
            } else {
                let car = c.m.r8(p + FE + GARAGE_CAR) as u32;
                fecars::get_garage_car(c, CAR_MANAGER, car, rec, p);
                own_colour(c, stream);
            }
            // a cop car's lights from the settings
            let k = player_cars(c, stream);
            let rec = record(stream, k);
            if c.m.r8(rec + 2) == COP_CLASS {
                let m = &mut c.m;
                let model = m.r8(rec) as i8 as i32 as u32;
                let lights = m.r8(model.wrapping_add(k.wrapping_mul(48)).wrapping_add(FE) + 192);
                m.w8(rec + 197, 0);
                m.w8(rec + 199, lights);
            }
            next_car(c, stream);
            if c.m.r8(FE + PLAYERS) != 1 {
                break;
            }
            p = p.wrapping_add(1);
            if p as u16 as i16 >= 2 {
                break;
            }
        }
    }
    // each listed for the race
    with_buffer(c, 8, |c, pair| {
        let mut k = 0u32;
        while (k as u16 as i16 as i32) < player_cars(c, stream) as i32 {
            let k16 = k as u16 as i16 as i32 as u32;
            let rec = record(stream, k16);
            let m = &mut c.m;
            let car = m.r8(rec) as i8 as i32 as u32;
            m.w32(pair, car);
            let colour = m.r8(rec + 197);
            m.w8(pair + 4, colour);
            if !added_model(m, pair) {
                more_memory(m, stream, 6, 13);
            }
            add_to_lineup(m, pair);
            let entry = list_entry(stream, k16);
            let size = m.r16(stream).wrapping_add(2);
            m.w16(stream, size);
            let (car, colour) = (m.r32(pair), m.r8(pair + 4));
            m.w32(entry, 1);
            m.w32(entry + 12, car);
            m.w8(entry + 16, colour);
            let v = m.r8(rec + 196);
            k = k.wrapping_add(1);
            m.w8(entry + 17, v);
        }
    });
}

pub fn abi_init_player_cars(c: &mut Cpu) {
    init_player_cars(c, c.r[4]);
}

/// The tournament manager's current tournament's record (84 bytes from the definition's +36).
fn tournament_record(m: &mut Mem) -> u32 {
    let t = TOURNAMENT_MANAGER;
    let (tier, definition) = (m.r32(t + 4), m.r32(t + 24));
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
    let tournament = first.wrapping_add(m.r32(t + 8));
    definition.wrapping_add(tournament.wrapping_mul(84)).wrapping_add(36)
}

/// `Front_InitTourneyTraffic` (FRONT.CPP, 0x800284a0): a tournament with traffic: three
/// traffic cars (+580 their count, +582 the cars) from the opponents' six (+684, in turn),
/// each listed for the race unless an added model (counted in the cars' memory).
pub fn init_tourney_traffic(c: &mut Cpu, stream: u32) {
    with_buffer(c, 8, |c, pair| {
        let m = &mut c.m;
        m.w8(pair + 4, 0);
        let tournament = tournament_record(m);
        m.w16(stream + 580, 0);
        if m.r8(FE + RACE_KIND) != TOURNAMENT_RACE || m.r8(tournament + 4) == 0 {
            return;
        }
        let mut k = 0u32;
        loop {
            let car = m.r8(stream.wrapping_add(k as u16 as i16 as i32 as u32) + 684) as u32;
            let next = k.wrapping_add(1);
            k = if (next as u16 as i16) < 6 { next } else { 0 };
            m.w32(pair, car);
            if !added_model(m, pair) {
                let memory = m.r16(stream + 2).wrapping_add(1);
                m.w16(stream + 2, memory);
                add_to_lineup(m, pair);
            }
            let n = m.r16(stream + 580) as i16 as i32 as u32;
            let car = m.r16(pair);
            m.w16(stream.wrapping_add(n << 1) + 582, car);
            let size = m.r16(stream).wrapping_add(1);
            let n = m.r16(stream + 580).wrapping_add(1);
            m.w16(stream + 580, n);
            m.w16(stream, size);
            if n as i16 >= 3 {
                return;
            }
        }
    });
}

pub fn abi_init_tourney_traffic(c: &mut Cpu) {
    init_tourney_traffic(c, c.r[4]);
}

/// `Front_InitOpponentCars` (FRONT.CPP, 0x80028614): the opponents (+416 their count): a
/// tournament's (its line-up, or with ten racers the class's of the player's car) or a
/// same-class race's, each its car (+57 of the record) made like an available one, listed and
/// coloured (the record's colours, or the player's with ten racers; none with the cheat off a
/// tournament's first race), their places; else a single race's one opponent, the chosen stock
/// car (in the class when memory is short), or none.
pub fn init_opponent_cars(c: &mut Cpu, stream: u32) {
    c.m.w16(stream + 416, 0);
    let kind = c.m.r8(FE + RACE_KIND);
    if kind == TOURNAMENT_RACE || (kind == 0 && c.m.r8(FE + 798) == 2) {
        opponents(c, stream);
        return;
    }
    let m = &mut c.m;
    if m.r8(FE + RACE_KIND) < 2 && m.r8(FE + 798) == 1 {
        one_opponent(c, stream);
    } else {
        m.w8(stream + 428, 1);
        m.w8(stream + 448, 2);
    }
}

pub fn abi_init_opponent_cars(c: &mut Cpu) {
    init_opponent_cars(c, c.r[4]);
}

/// A tournament's or a class race's opponents.
fn opponents(c: &mut Cpu, stream: u32) {
    let m = &mut c.m;
    let t2 = tournament_record(m);
    let racers = if m.r8(FE + RACE_KIND) == TOURNAMENT_RACE { m.r8(t2 + 3) as u32 } else { 10 };
    // (ten racers: the class of the player's car; its colours)
    let ten = racers == 10;
    let roster = if ten {
        let definition = m.r32(TOURNAMENT_MANAGER + 24);
        let class = (m.r8(definition + 26) as u32).wrapping_add(m.r8(stream + 8) as u32);
        definition.wrapping_add(class.wrapping_mul(84)).wrapping_add(36)
    } else {
        tournament_record(m)
    };
    let mut count = 5u32;
    if m.r8(FE + RACE_KIND) == TOURNAMENT_RACE {
        count = m.r32(TOURNAMENT_MANAGER + 16).wrapping_sub(1);
    }
    if (count << 16) as i32 > 0 {
        with_buffer(c, 8, |c, pair| {
            let mut k = 0u32;
            loop {
                let k16 = k as u16 as i16 as i32 as u32;
                let m = &mut c.m;
                let id = m.r8(roster.wrapping_add(k16) + 57) as u32;
                m.w32(pair, id);
                let car = fecars::get_car_from_id(m, CAR_MANAGER, id as i32);
                let colour = m.r8(car + 4);
                m.w8(pair + 4, colour);
                if !added_model(m, pair) {
                    more_memory(m, stream, 3, 16);
                }
                fecars::find_similar_car(m, CAR_MANAGER, pair, pair + 4);
                add_to_lineup(m, pair);
                let entry = list_entry(stream, k16.wrapping_add(1));
                let (id, colour) = (m.r32(pair), m.r8(pair + 4) as u32);
                m.w32(entry, 0);
                m.w32(entry + 12, id);
                let v = m.r8(car.wrapping_add(colour) + 175);
                m.w8(entry + 16, v);
                let v = if ten { m.r8(stream + 204) } else { m.r8(roster.wrapping_add(k16) + 62) };
                m.w8(entry + 17, v);
                if m.r8(FE + RACE_KIND) == TOURNAMENT_RACE && m.r8(FE + 290) == 0 {
                    let on = fecheats::is_cheat_enabled(m, 13);
                    let v = if on && m.r8(FE + 34) != 0 { m.r8(roster.wrapping_add(k16) + 62) } else { 0 };
                    m.w8(entry + 17, v);
                }
                k = k.wrapping_add(1);
                let size = m.r16(stream).wrapping_add(2);
                let n = m.r16(stream + 416).wrapping_add(1);
                m.w16(stream, size);
                m.w16(stream + 416, n);
                if ((k << 16) as i32) >= ((count << 16) as i32) {
                    break;
                }
            }
        });
    }
    // their personalities and places: the tournament's line-up, or in order
    let m = &mut c.m;
    if m.r8(FE + RACE_KIND) == TOURNAMENT_RACE {
        crate::fetourn::update_car_lineup(m, TOURNAMENT_MANAGER);
        let n = m.r32(TOURNAMENT_MANAGER + 16).wrapping_sub(1);
        m.w16(stream + 416, n as u16);
        let places = (count as u16 as i16 as i32).wrapping_add(1);
        let lineup = TOURNAMENT_MANAGER + 376;
        let mut k = 0i32;
        while k < places {
            let from = lineup.wrapping_add(20 * k as u32);
            let to = list_entry(stream, k as u32);
            let v = m.r32(from + 4);
            m.w32(to + 4, v);
            let v = m.r8(from + 8);
            m.w8(to + 8, v);
            k += 1;
        }
    } else {
        for k in 0..5u32 {
            let entry = list_entry(stream, k + 1);
            m.w32(entry + 4, k);
            m.w8(entry + 8, (k + 1) as u8);
        }
        m.w8(stream + 428, 6);
    }
}

/// A single race's one opponent: the chosen stock car (in its class's when memory is short).
fn one_opponent(c: &mut Cpu, stream: u32) {
    let choice = c.m.r8(FE + 797) as u32;
    with_buffer(c, CAR_RECORD + 8 + 12, |c, info| {
        let (pair, list) = (info + CAR_RECORD, info + CAR_RECORD + 8);
        fecars::get_stock_car(c, CAR_MANAGER, choice, info);
        let m = &mut c.m;
        let id = m.r8(info) as i8 as i32 as u32;
        let colour = m.r8(info + 4) as u32;
        m.w32(pair, id);
        let v = m.r8(info + 175 + colour);
        m.w8(pair + 4, v);
        if !added_model(m, pair) && !more_memory(m, stream, 3, 16) {
            let class = m.r8(info + 2) as u32;
            fecars::get_class_list(m, CAR_MANAGER, class, 3, list);
            fecars::find_similar_car(m, CAR_MANAGER, pair, pair + 4);
        }
        let n = m.r16(stream + 416);
        let first = m.r8(stream + 204);
        m.w32(stream + 440, 0);
        m.w32(stream + 444, 0);
        m.w16(stream + 416, n.wrapping_add(1));
        let (id, colour) = (m.r32(pair), m.r8(pair + 4));
        let place = m.r8(stream + 416).wrapping_add(1);
        m.w8(stream + 448, 1);
        m.w8(stream + 457, first);
        m.w32(stream + 452, id);
        m.w8(stream + 456, colour);
        m.w8(stream + 428, place);
        add_to_lineup(m, pair);
    });
    let size = c.m.r16(stream).wrapping_add(2);
    c.m.w16(stream, size);
}

/// The missions' manager; the cop cars (by the player cars' highest kind, 20 bytes each, then
/// by the stream's class, +680): the others' (+542), the pursuers' (+540).
const MISSION_MANAGER: u32 = 0x8013_E594;
const OTHER_COP_CARS: u32 = 0x8005_1D04;
const PURSUER_CARS: u32 = 0x8005_1C78;
/// A mission's stage (44 bytes): +0 the car, +1 its colour, +6 and +10 its chasers (none
/// below 0). A mission: +10 its stages.
const STAGE_SIZE: u32 = 44;
const STAGES: u32 = 10;

/// The best (highest) model of the stream's player cars, and their highest class.
fn best_player_car(c: &mut Cpu, stream: u32) -> (u32, u32) {
    let (mut best, mut class) = (0i32, 0i32);
    for k in 0..player_cars(c, stream) as i16 as i32 {
        let rec = record(stream, k as u32);
        best = best.max(c.m.r8(rec) as i8 as i32);
        class = class.max(c.m.r8(rec + 2) as i32);
    }
    (best as u32, class as u32)
}

/// Whether all the stream's player cars are cops (none is not).
fn all_cops(c: &mut Cpu, stream: u32) -> bool {
    (0..player_cars(c, stream) as i16 as i32).all(|k| c.m.r8(record(stream, k as u32) + 2) == COP_CLASS)
}

/// `Front_InitMissions` (FRONT.CPP, 0x80028ba8): a single race whose player cars are all cops:
/// the missions' description (the second with traffic or two players), the mission (+600) and
/// its stages (+604) for the best car's tier.
pub fn init_missions(c: &mut Cpu, stream: u32) {
    c.m.w32(stream + 600, 0);
    c.m.w32(stream + 604, 0);
    if c.m.r8(FE + RACE_KIND) != SINGLE_RACE || !all_cops(c, stream) {
        return;
    }
    let (best, _) = best_player_car(c, stream);
    let m = &mut c.m;
    let (tier, mission) = (m.r8(FE + MISSION_TIER), m.r8(FE + MISSION));
    m.w8(FE + MISSION_TIER, best.wrapping_sub(22) as u8);
    m.w8(FE + MISSION, 0);
    let second = (m.r16(stream + 580) as i16) > 0 || m.r8(FE + PLAYERS) == 1;
    crate::femission::load_description(c, MISSION_MANAGER, second);
    let m = &mut c.m;
    crate::femission::get_mission_to_race(m, MISSION_MANAGER, stream + 600);
    let (a, b) = (m.r8(FE + MISSION_TIER) as u32, m.r8(FE + MISSION) as u32);
    crate::femission::get_mission_stages(m, MISSION_MANAGER, a, b, stream + 604);
    m.w8(FE + MISSION_TIER, tier);
    m.w8(FE + MISSION, mission);
}

pub fn abi_init_missions(c: &mut Cpu) {
    init_missions(c, c.r[4]);
}

/// `Front_InitCopCars` (FRONT.CPP, 0x80028cfc): the cops (+540 the pursuers, +542 the others,
/// +544 their cars, +568 their colours): a mission's chaser (the best player car in the first
/// car's lights, at the first stage that has one) unless with two players; else in a single race
/// without cop players, four pursuers alone, two and one with one player and no opponents, two
/// others with two players, their cars by the player cars' highest class, each listed for the
/// race unless an added model.
pub fn init_cop_cars(c: &mut Cpu, stream: u32) {
    c.m.w16(stream + 540, 0);
    c.m.w16(stream + 542, 0);
    let (best, class) = best_player_car(c, stream);
    let m = &mut c.m;
    let mission = m.r32(stream + 600);
    if mission != 0 && m.r32(stream + 604) != 0 {
        if m.r8(FE + PLAYERS) == 1 {
            return;
        }
        for k in 0..m.r8(mission + STAGES) as u32 {
            let stage = m.r32(stream + 604).wrapping_add(k * STAGE_SIZE);
            if (m.r16(stage + 6) as i16) >= 0 || (m.r16(stage + 10) as i16) >= 0 {
                let n = m.r16(stream + 540) as i16 as i32 as u32;
                m.w32(stream.wrapping_add(n << 2) + 544, best);
                let lights = m.r8(record(stream, 0) + 199);
                m.w16(stream.wrapping_add(n << 1) + 568, lights as u16);
                m.w16(stream + 540, (n + 1) as u16);
                let size = m.r16(stream).wrapping_add(2);
                m.w16(stream, size);
                return;
            }
        }
        return;
    }
    if m.r8(FE + RACE_KIND) != SINGLE_RACE {
        return;
    }
    if (0..player_cars(c, stream) as i16 as i32).any(|k| c.m.r8(record(stream, k as u32) + 2) == COP_CLASS) {
        return;
    }
    let m = &mut c.m;
    let cars = m.r16(stream + 6);
    let (pursuers, others) = match (cars, m.r16(stream + 416)) {
        (1, 0) => (2, 1),
        (2, _) => (0, 2),
        _ => (4, 0),
    };
    m.w16(stream + 540, pursuers);
    m.w16(stream + 542, others);
    with_buffer(c, 8, |c, pair| {
        let m = &mut c.m;
        m.w8(pair + 4, 0);
        for k in 0..(pursuers + others) as u32 {
            let table = if k < others as u32 { OTHER_COP_CARS } else { PURSUER_CARS };
            let car_class = m.r8(stream + 680) as u32;
            let car = m.r32(table + class * 20 + 4 * car_class);
            m.w32(pair, car);
            if !added_model(m, pair) {
                let memory = m.r16(stream + 2).wrapping_add(3);
                m.w16(stream + 2, memory);
                add_to_lineup(m, pair);
            }
            m.w32(stream + 544 + 4 * k, car);
            let size = m.r16(stream).wrapping_add(2);
            m.w16(stream, size);
            let v = m.r8(stream + 680);
            m.w16(stream + 568 + 2 * k, v as u16);
        }
    });
}

pub fn abi_init_cop_cars(c: &mut Cpu) {
    init_cop_cars(c, c.r[4]);
}

/// `rand`.
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;

/// `Front_InitPerps` (FRONT.CPP, 0x800290bc): a mission's perps (+594 their count, +596 the
/// stages done, +608 the perps: 8 bytes, the car and its colour): each stage's car in its
/// colour (the index of the car's colour equal to it, 16 when none is), listed for the race
/// unless an added model.
pub fn init_perps(c: &mut Cpu, stream: u32) {
    let m = &mut c.m;
    let mission = m.r32(stream + 600);
    m.w16(stream + 594, 0);
    m.w16(stream + 596, 0);
    if mission == 0 {
        return;
    }
    with_buffer(c, 8, |c, pair| {
        let m = &mut c.m;
        let mut k = 0u32;
        loop {
            let mission = m.r32(stream + 600);
            if k >= m.r8(mission + STAGES) as u32 {
                break;
            }
            let stage = m.r32(stream + 604).wrapping_add(k * STAGE_SIZE);
            let (id, colour) = (m.r8(stage) as u32, m.r8(stage + 1));
            m.w32(pair, id);
            let car = fecars::get_car_from_id(m, CAR_MANAGER, id as i32);
            let index = (0..16).find(|&j| m.r8(car + 175 + j) as i8 as i32 == colour as i32).unwrap_or(16);
            m.w8(pair + 4, index as u8);
            if !added_model(m, pair) {
                more_memory(m, stream, 6, 16);
                add_to_lineup(m, pair);
                let size = m.r16(stream).wrapping_add(2);
                m.w16(stream, size);
                let n = m.r16(stream + 594) as i16 as i32 as u32;
                m.w32(stream.wrapping_add(n << 3) + 608, id);
                m.w8(stream.wrapping_add(n << 3) + 612, colour);
                m.w16(stream + 594, (n + 1) as u16);
            }
            let done = m.r16(stream + 596).wrapping_add(1);
            m.w16(stream + 596, done);
            k += 1;
        }
    });
}

pub fn abi_init_perps(c: &mut Cpu) {
    init_perps(c, c.r[4]);
}

/// The track's settings (+704 the track, then direction, mirrored, night, weather) and where a
/// race slot keeps them.
const TRACK_SETTINGS: u32 = 704;
const SLOT_SETTINGS: [u32; 4] = [50, 55, 22, 27];

/// `Front_InitTrack` (FRONT.CPP, 0x80029284): the track (+656 its information, +704 its number
/// and settings): a tournament's to race; else the race slot's, its settings for a single race,
/// a pink slips' race or with the options on, else none, the time of day full (+736); the
/// settings left unset (2 and up) chosen at random.
pub fn init_track(c: &mut Cpu, stream: u32) {
    let settings = stream + TRACK_SETTINGS;
    if c.m.r8(FE + RACE_KIND) == TOURNAMENT_RACE {
        crate::fetourn::get_track_to_race(c, TOURNAMENT_MANAGER, settings);
        let m = &mut c.m;
        let track = m.r8(settings) as i8 as i32 as u32;
        let info = fetracks::get_track_by_id(m, TRACK_MANAGER, track);
        for k in 0..TRACK_INFO_SIZE {
            let v = m.r8(info + k);
            m.w8(stream + 656 + k, v);
        }
    } else {
        let slot = c.m.r8(FE + SLOT) as u32;
        let track = c.m.r8(slot + FE + TRACKS) as u32;
        fetracks::get_track(c, TRACK_MANAGER, track, stream + 656);
        let m = &mut c.m;
        let number = m.r8(stream + 656);
        m.w8(settings, number);
        let kind = m.r8(FE + RACE_KIND);
        let own = m.r8(FE + OPTIONS_ON) == 1 || kind == SINGLE_RACE || kind == PINK_SLIPS_RACE;
        for (k, at) in SLOT_SETTINGS.into_iter().enumerate() {
            let v = if own { m.r8(slot + FE + at) } else { 0 };
            m.w8(settings + 1 + k as u32, v);
        }
        m.w32(stream + 736, 0x1_0000);
    }
    for k in 1..=4 {
        if c.m.r8(settings + k) >= 2 {
            let v = call(c, RAND, &[]);
            c.m.w8(settings + k, (v & 1) as u8);
        }
    }
}

pub fn abi_init_track(c: &mut Cpu) {
    init_track(c, c.r[4]);
}

/// `Front_InitTraffic` (FRONT.CPP, 0x80029458): traffic for the race: on with the slot's
/// traffic option (always in a pink slips' race; in a race of kinds 0 and 1, by the options and
/// their switch, a single race the first slot's), unless the track's class is past 2 or it is
/// mirrored, two players race a single race, or one races a same-class race; then six cars
/// (three with two players; one or two in a single race by the track's night) from the
/// opponents' (+684, in turn), listed for the race unless added models.
pub fn init_traffic(c: &mut Cpu, stream: u32) {
    let m = &mut c.m;
    let two = m.r8(FE + PLAYERS) == 1;
    let kind = m.r8(FE + RACE_KIND);
    let count = match kind {
        SINGLE_RACE if m.r8(stream + 707) == 1 => 1,
        SINGLE_RACE => 2,
        _ if two => 3,
        _ => 6,
    };
    let slot = m.r8(FE + SLOT) as u32;
    let option = m.r8(slot + FE + TRAFFIC) != 0;
    let on = if kind >= 2 {
        kind == PINK_SLIPS_RACE && option
    } else {
        let on = if m.r8(FE + OPTIONS_ON) == 0 && kind == 0 {
            true
        } else if kind == SINGLE_RACE {
            m.r8(FE + TRAFFIC) != 0
        } else {
            option
        };
        let track_says_no = m.r8(stream + 658) >= 3 || m.r8(stream + 660) != 0;
        let racers_say_no = if two { kind == SINGLE_RACE } else { m.r8(FE + 798) == 2 };
        on && !track_says_no && !racers_say_no
    };
    if !on {
        return;
    }
    m.w16(stream + 580, 0);
    with_buffer(c, 8, |c, pair| {
        let m = &mut c.m;
        m.w8(pair + 4, 0);
        for k in 0..count {
            let car = m.r8(stream + 684 + k % 6) as u32;
            m.w32(pair, car);
            if !added_model(m, pair) {
                add_to_lineup(m, pair);
            }
            m.w16(stream + 582 + 2 * k, car as u16);
            m.w16(stream + 580, (k + 1) as u16);
        }
    });
}

pub fn abi_init_traffic(c: &mut Cpu) {
    init_traffic(c, c.r[4]);
}

/// The race stream's tags: a car's settings (each tag, the car's index, the value).
mod tag {
    pub const MODEL: u32 = 260;
    /// The driver: 1 a player, 65 a player's cop, 2 an opponent or a perp, 4 traffic, 16 and
    /// 8 the cops (the others, the pursuers).
    pub const DRIVER: u32 = 261;
    pub const HUMAN: u32 = 262;
    pub const AIDS: [u32; 3] = [263, 264, 265];
    pub const COLOUR: u32 = 266;
    pub const COLOUR_2: u32 = 267;
    pub const DAMAGE: u32 = 268;
    pub const EXTRA_0: u32 = 269;
    pub const EXTRA_2: u32 = 272;
    pub const LIGHTS: u32 = 273;
    pub const EXTRA_1: u32 = 274;
    pub const GEARBOX: u32 = 275;
    pub const PERSONALITY: u32 = 276;
    pub const LIGHTS_2: u32 = 277;
    pub const SKILL: u32 = 280;
    pub const PLACE: u32 = 281;
    pub const VIEW: u32 = 282;
    pub const NOT_FOURTH_VIEW: u32 = 283;
    pub const DASHBOARD: [(u32, u32); 6] = [(284, 804), (285, 806), (286, 808), (287, 812), (288, 810), (289, 814)];
    pub const RGB: u32 = 290;
    pub const TINT: u32 = 291;
    pub const SHADING: u32 = 292;
    pub const COP_LIGHTS: u32 = 293;
    pub const PER_PLAYER: [(u32, u32); 4] = [(294, 816), (295, 817), (296, 818), (297, 819)];
    /// The stream's own (each tag, then the value): the counts of player cars, opponents; the
    /// mission's stages and kind, the stages done; the cops; the traffic's density; the track's.
    pub const PLAYER_CARS: u32 = 29;
    pub const OPPONENTS: u32 = 30;
    pub const END_OF_CARS: u32 = 14;
    pub const COPS: u32 = 12;
    pub const MISSION: u32 = 37;
    pub const MISSION_KIND: u32 = 38;
    pub const DENSITY: u32 = 13;
    pub const TRACK_VIEW: u32 = 26;
    pub const TIME_OF_DAY: u32 = 11;
}

/// The stream's words as the `Front_Append*` write them: at a cursor, one after another.
struct Out(u32);

impl Out {
    fn put(&mut self, m: &mut Mem, v: u32) {
        m.w32(self.0, v);
        self.0 = self.0.wrapping_add(4);
    }

    /// A tag and its value.
    fn pair(&mut self, m: &mut Mem, tag: u32, v: u32) {
        self.put(m, tag);
        self.put(m, v);
    }

    /// A car's setting: its tag, the car's index (`i`) and its value.
    fn car(&mut self, m: &mut Mem, i: u32, tag: u32, v: u32) {
        self.put(m, tag);
        self.put(m, i);
        self.put(m, v);
    }
}

/// The car being written's index (+4 of the stream, a `short`), and the next.
fn car_index(m: &mut Mem, stream: u32) -> u32 {
    m.r16(stream + 4) as i16 as i32 as u32
}

fn next_index(m: &mut Mem, stream: u32) {
    let i = m.r16(stream + 4).wrapping_add(1);
    m.w16(stream + 4, i);
}

/// A car's colour `colour` as RGB (its +68 table of 4 bytes).
fn car_rgb(m: &mut Mem, car: u32, colour: u32) -> u32 {
    let at = car.wrapping_add(colour << 2);
    let (w, b) = (m.r32(at + 68), m.r8(at + 70) as u32);
    b | (w & 0xFF00) | ((w & 0xFF) << 16)
}

/// The extras from the list's bits (+17 of an entry): their three settings.
fn extras(m: &mut Mem, o: &mut Out, i: u32, entry: u32) {
    let bits = m.r8(entry + 17) as u32;
    o.car(m, i, tag::EXTRA_2, (bits >> 2) & 1);
    o.car(m, i, tag::EXTRA_1, (bits >> 1) & 1);
    o.car(m, i, tag::EXTRA_0, bits & 1);
}

/// The tracks' default view (by the track's +42).
const TRACK_DEFAULTS: u32 = 0x8005_1AF8;

fn track_default_view(m: &mut Mem, track: u32) -> u32 {
    let k = m.r8(track + 42) as u32;
    m.r16(TRACK_DEFAULTS + (k << 1)) as i16 as i32 as u32
}

/// `Front_AppendPlayerCarData` (FRONT.CPP, 0x80029690): the player cars' settings into the
/// race's stream at `out`: the place, model, the player's, colour, the lights (on with the
/// player's setting and the car's, always for the cop car 28), the extras, the colour's RGB,
/// the time of day's tint, the shading, the cop lights, the driver, the gearbox (2, automatic,
/// with the upgrade), the three driving aids, then the display's settings; the cars' counts
/// last. The cursor after.
pub fn append_player_car_data(m: &mut Mem, out: u32, stream: u32) -> u32 {
    let mut o = Out(out);
    for k in 0..player_cars_m(m, stream) {
        let (entry, rec, fe) = (list_entry(stream, k), record(stream, k), k.wrapping_add(FE));
        let i = car_index(m, stream);
        let colour = m.r8(rec + 197) as u32;
        let cop = m.r8(rec) as i8 == 28;
        let lights_on = m.r8(fe + 788) != 0;
        let values = [
            (tag::PLACE, m.r8(entry + 8) as i8 as i32 as u32),
            (tag::MODEL, m.r8(rec + 1) as u32),
            (tag::HUMAN, m.r8(fe + 94) as u32),
            (tag::COLOUR, colour),
            (tag::COLOUR_2, 0),
            (tag::LIGHTS, ((lights_on && m.r8(rec + 3) != 0) || cop) as u32),
            (tag::LIGHTS_2, ((lights_on && m.r8(rec + 194) != 0) || cop) as u32),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        extras(m, &mut o, i, entry);
        o.car(m, i, tag::DAMAGE, 0);
        let rgb = car_rgb(m, rec, colour);
        o.car(m, i, tag::RGB, rgb);
        let night = m.r8(stream + 707) as u32;
        let values = [
            (tag::TINT, m.r32(rec + (night << 2) + 148)),
            (tag::SHADING, m.r8(rec + colour + 132) as u32),
            (tag::COP_LIGHTS, m.r8(rec + 199) as u32),
            (tag::DRIVER, if m.r8(rec + 2) == COP_CLASS { 65 } else { 1 }),
            (tag::GEARBOX, if m.r8(rec + 196) & 2 != 0 { 2 } else { m.r8(rec + 6) as u32 }),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        for (t, at) in tag::AIDS.into_iter().zip([791, 793, 795]) {
            let v = m.r8(fe + at) as u32;
            o.car(m, i, t, v);
        }
        o.0 = output_display_settings(m, o.0, i, k, stream + 656);
        next_index(m, stream);
    }
    let cars = m.r16(stream + 6) as i16 as i32 as u32;
    o.pair(m, tag::PLAYER_CARS, cars);
    let opponents = m.r16(stream + 416) as i16 as i32 as u32;
    o.pair(m, tag::OPPONENTS, opponents);
    o.pair(m, tag::END_OF_CARS, 0);
    o.0
}

pub fn abi_append_player_car_data(c: &mut Cpu) {
    c.r[2] = append_player_car_data(&mut c.m, c.r[4], c.r[5]);
}

/// The stream's player cars so far, from memory.
fn player_cars_m(m: &mut Mem, stream: u32) -> u32 {
    (m.r16(stream + 6) as i16).max(0) as u32
}

/// `Front_AppendOpponentData` (FRONT.CPP, 0x80029b74): the opponents' settings into the stream
/// (each after the player cars in the list: its place, model, the driver, its personality as
/// both the AI's settings, its colour, the extras, the colour's RGB, the time of day's tint,
/// the shading). The cursor after.
pub fn append_opponent_data(m: &mut Mem, out: u32, stream: u32) -> u32 {
    let mut o = Out(out);
    let mut k = 0;
    while k < m.r16(stream + 416) as i16 as i32 {
        let players = m.r16(stream + 6) as i16 as i32 as u32;
        let entry = list_entry(stream, (k as u32).wrapping_add(players));
        let id = m.r16(entry + 12) as i16 as i32;
        let car = fecars::get_car_from_id(m, CAR_MANAGER, id);
        let i = car_index(m, stream);
        let colour = m.r8(entry + 16) as u32;
        let values = [
            (tag::PLACE, m.r8(entry + 8) as i8 as i32 as u32),
            (tag::MODEL, m.r8(car + 1) as u32),
            (tag::HUMAN, 1),
            (tag::DRIVER, 2),
            (tag::PERSONALITY, m.r32(entry + 4)),
            (tag::SKILL, m.r32(entry + 4)),
            (tag::COLOUR, colour),
            (tag::COLOUR_2, 0),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        extras(m, &mut o, i, entry);
        o.car(m, i, tag::DAMAGE, 0);
        o.car(m, i, tag::COP_LIGHTS, 0);
        let rgb = car_rgb(m, car, colour);
        o.car(m, i, tag::RGB, rgb);
        let night = m.r8(stream + 707) as u32;
        let tint = m.r32(car.wrapping_add(night << 2) + 148);
        o.car(m, i, tag::TINT, tint);
        let shading = m.r8(car.wrapping_add(colour) + 132) as u32;
        o.car(m, i, tag::SHADING, shading);
        next_index(m, stream);
        k += 1;
    }
    o.0
}

pub fn abi_append_opponent_data(c: &mut Cpu) {
    c.r[2] = append_opponent_data(&mut c.m, c.r[4], c.r[5]);
}

/// `Front_AppendCopData` (FRONT.CPP, 0x80029ea8): the cops into the stream (their marker first
/// when there are any): each its model, the driver (the others first), the skill (the front
/// end's difficulty, +6, and 5), its colour (+568). The cursor after.
pub fn append_cop_data(m: &mut Mem, out: u32, stream: u32) -> u32 {
    let mut o = Out(out);
    let cops = |m: &mut Mem| (m.r16(stream + 540) as i16 as i32) + (m.r16(stream + 542) as i16 as i32);
    if cops(m) > 0 {
        o.pair(m, tag::COPS, 1);
    }
    let mut k = 0;
    while k < cops(m) {
        let id = m.r16(stream + 544 + 4 * k as u32) as i16 as i32;
        let car = fecars::get_car_from_id(m, CAR_MANAGER, id);
        let i = car_index(m, stream);
        let other = k < m.r16(stream + 542) as i16 as i32;
        let colour = m.r16(stream + 568 + 2 * k as u32) as i16 as i32 as u32;
        let values = [
            (tag::MODEL, m.r8(car + 1) as u32),
            (tag::HUMAN, 1),
            (tag::DRIVER, if other { 16 } else { 8 }),
            (tag::SKILL, (m.r8(FE + 6) as u32).wrapping_add(5)),
            (tag::COLOUR, 0),
            (tag::COLOUR_2, 0),
            (tag::DAMAGE, 0),
            (tag::COP_LIGHTS, colour),
            (tag::EXTRA_2, 0),
            (tag::EXTRA_1, 0),
            (tag::EXTRA_0, 0),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        next_index(m, stream);
        k += 1;
    }
    o.0
}

pub fn abi_append_cop_data(c: &mut Cpu) {
    c.r[2] = append_cop_data(&mut c.m, c.r[4], c.r[5]);
}

/// `Front_AppendPerpData` (FRONT.CPP, 0x8002a0fc): the mission (its stages and its kind, or
/// none) and the perps into the stream: each its model, the driver, its colour; the stages
/// done last. The cursor after.
pub fn append_perp_data(m: &mut Mem, out: u32, stream: u32) -> u32 {
    let mut o = Out(out);
    let mission = m.r32(stream + 600);
    let stages = if mission != 0 { m.r8(mission + STAGES) as u32 } else { 0 };
    o.pair(m, tag::MISSION, stages);
    let kind = if mission != 0 { m.r16(mission + 8) as u32 } else { 0 };
    o.pair(m, tag::MISSION_KIND, kind);
    let mut k = 0;
    while k < m.r16(stream + 594) as i16 as i32 {
        let perp = stream + 8 * k as u32;
        let id = m.r16(perp + 608) as i16 as i32;
        let car = fecars::get_car_from_id(m, CAR_MANAGER, id);
        let i = car_index(m, stream);
        let values = [
            (tag::MODEL, m.r8(car + 1) as u32),
            (tag::HUMAN, 1),
            (tag::COLOUR, m.r8(perp + 612) as u32),
            (tag::COLOUR_2, 0),
            (tag::DRIVER, 2),
            (tag::SKILL, 0),
            (tag::DAMAGE, 1),
            (tag::COP_LIGHTS, 0),
            (tag::EXTRA_2, 0),
            (tag::EXTRA_1, 0),
            (tag::EXTRA_0, 0),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        next_index(m, stream);
        k += 1;
    }
    let done = m.r16(stream + 596) as i16 as i32 as u32;
    o.pair(m, tag::MISSION, done);
    o.0
}

pub fn abi_append_perp_data(c: &mut Cpu) {
    c.r[2] = append_perp_data(&mut c.m, c.r[4], c.r[5]);
}

/// `Front_AppendTrafficData` (FRONT.CPP, 0x8002a394): the traffic cars into the stream: each
/// its model, the driver (4) and skill (8); then the traffic's density (a third of the cars,
/// at least one). The cursor after.
pub fn append_traffic_data(m: &mut Mem, out: u32, stream: u32) -> u32 {
    let mut o = Out(out);
    let mut k = 0;
    while k < m.r16(stream + 580) as i16 as i32 {
        let id = m.r16(stream + 582 + 2 * k as u32) as i16 as i32;
        let car = fecars::get_car_from_id(m, CAR_MANAGER, id);
        let i = car_index(m, stream);
        let model = m.r8(car + 1) as u32;
        o.car(m, i, tag::MODEL, model);
        let values = [
            (tag::HUMAN, 1),
            (tag::COLOUR, 0),
            (tag::COLOUR_2, 0),
            (tag::DRIVER, 4),
            (tag::SKILL, 8),
            (tag::DAMAGE, 1),
            (tag::COP_LIGHTS, 0),
            (tag::EXTRA_2, 0),
            (tag::EXTRA_1, 0),
            (tag::EXTRA_0, 0),
        ];
        for (t, v) in values {
            o.car(m, i, t, v);
        }
        next_index(m, stream);
        k += 1;
    }
    let cars = m.r16(stream + 580) as i16 as i32;
    let density = if cars > 0 { (cars / 3).max(1) } else { cars / 3 };
    o.pair(m, tag::DENSITY, density as u32);
    o.0
}

pub fn abi_append_traffic_data(c: &mut Cpu) {
    c.r[2] = append_traffic_data(&mut c.m, c.r[4], c.r[5]);
}

/// The view setting for the race (+800: 1 the first, 2 the second, else the track's default).
const VIEW: u32 = 800;

/// `Front_AppendTrackData` (FRONT.CPP, 0x8002a5e4): the track's settings into the stream: the
/// view, the direction, the mirroring, the weather, the night, the track's +1, the time of day.
/// The cursor after.
pub fn append_track_data(c: &mut Cpu, out: u32, stream: u32) -> u32 {
    let slot = c.m.r8(FE + SLOT) as u32;
    let track = c.m.r8(slot + FE + TRACKS) as u32;
    let view = with_buffer(c, TRACK_INFO_SIZE, |c, info| {
        fetracks::get_track(c, TRACK_MANAGER, track, info);
        match c.m.r8(FE + VIEW) {
            1 => 0,
            2 => 1,
            _ => track_default_view(&mut c.m, info),
        }
    });
    let m = &mut c.m;
    let mut o = Out(out);
    o.pair(m, tag::TRACK_VIEW, view);
    for (t, at) in [(24, 706), (25, 705), (20, 708), (23, 707), (18, 657)] {
        let v = m.r8(stream + at) as u32;
        o.pair(m, t, v);
    }
    let time = m.r32(stream + 736);
    o.pair(m, tag::TIME_OF_DAY, time);
    o.0
}

pub fn abi_append_track_data(c: &mut Cpu) {
    c.r[2] = append_track_data(c, c.r[4], c.r[5]);
}

/// `OutputDisplaySettings` (FRONT.CPP, 0x80027dec): player `p`'s display settings into the
/// stream at `out` for car `i`: the view (+800: 1 the first, 2 the second, 3 and up the third,
/// 0 the track's default), whether it isn't the fourth, the six dashboard settings and the
/// four per player. The cursor after.
pub fn output_display_settings(m: &mut Mem, out: u32, i: u32, p: u32, track: u32) -> u32 {
    let fe = p.wrapping_add(FE);
    let setting = m.r8(fe + VIEW);
    let view = match setting {
        0 => track_default_view(m, track),
        1 => 0,
        2 => 1,
        _ => 2,
    };
    let mut o = Out(out);
    o.car(m, i, tag::VIEW, view);
    o.car(m, i, tag::NOT_FOURTH_VIEW, (setting != 3) as u32);
    for (t, at) in tag::DASHBOARD {
        let v = m.r8(fe + at) as u32;
        o.car(m, i, t, v);
    }
    let fe4 = (p << 2).wrapping_add(FE);
    for (t, at) in tag::PER_PLAYER {
        let v = m.r8(fe4 + at) as u32;
        o.car(m, i, t, v);
    }
    o.0
}

pub fn abi_output_display_settings(c: &mut Cpu) {
    c.r[2] = output_display_settings(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `operator new`, `operator delete`.
const NEW: fn(&mut Cpu) = nfs4rc::f_800a48c0;
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;
/// The sizes: the screens', the application's, the menus'.
const ALL_SCREENS_SIZE: u32 = 15320;
const FE_APP_SIZE: u32 = 896;
const ALL_MENUS_SIZE: u32 = 15128;

/// `tAllScreens`' screens, at their offsets: the main menu's; the car selects (the first
/// player's, the second's, the tournament's, the pink slips', the cop chase's, the used cars'),
/// the dialogs between them; the options', the results', the tournament's and so on.
mod screen {
    pub const MAIN: u32 = 0;
    pub const CAR_SELECTS: [u32; 6] = [1464, 2392, 3368, 4448, 5528, 6628];
    pub const DIALOGS: [u32; 4] = [4296, 5376, 6456, 7556];
    pub const TOURN_SELECT: u32 = 9192;
    pub const STANDINGS: u32 = 9904;
    pub const TROPHY_ROOM: u32 = 10444;
    pub const CONTROLLER: u32 = 10892;
    /// The controller screen's dialog.
    pub const CONTROLLER_DIALOG: u32 = 11076;
    pub const AUDIO: u32 = 11372;
    pub const MEM_CARD: u32 = 11496;
    /// The two standings like the tournament's.
    pub const STANDINGS_2: u32 = 13536;
    pub const STANDINGS_3: u32 = 13684;
}

/// The plain screens (`tScreen` made, its class's vtable set), at their offsets.
const PLAIN_SCREENS: [(u32, u32); 12] = [
    (screen::MAIN, 0x8001_1858),
    (7728, 0x8001_1FA0),
    (7844, 0x8001_1F28),
    (8520, 0x8001_1EB8),
    (10052, 0x8001_2390),
    (10788, 0x8001_1E40),
    (11272, 0x8001_2080),
    (12940, 0x8001_2698),
    (13144, 0x8001_2328),
    (13832, 0x8001_18D8),
    (14544, 0x8001_22C0),
    (14932, 0x8001_2258),
];

/// The screens made like the tournament's standings, with their own classes.
const STANDINGS_SCREENS: [(u32, u32); 2] = [(screen::STANDINGS_2, 0x8001_1950), (screen::STANDINGS_3, 0x8001_19A0)];

/// The car selects' classes after the first's own: the second player's, the tournament's and
/// the others'.
const CAR_SELECT_VTS: [u32; 6] = [0, 0x8001_1C30, 0x8001_1BA0, 0x8001_1BA0, 0x8001_1B10, 0x8001_1B10];

/// Where each screen of `tAllScreens` is reached from (and its offset in it).
const SCREEN_POINTERS: [(u32, u32); 26] = [
    (0x8005_22E8, 0), (0x8005_24CC, 1464), (0x8005_24D0, 2392), (0x8005_24D4, 3368), (0x8005_24D8, 4448),
    (0x8005_24DC, 5528), (0x8005_24E0, 6628), (0x8005_2560, 7728), (0x8005_255C, 7844), (0x8005_2544, 8520),
    (0x8005_24EC, 9192), (0x8005_22F8, 9904), (0x8005_2DD8, 10052), (0x8005_24F0, 10444), (0x8005_2518, 10788),
    (0x8005_2D60, 10892), (0x8005_2D68, 11272), (0x8005_2D78, 11372), (0x8005_2D80, 11496), (0x8005_2E44, 12940),
    (0x8005_2DD4, 13144), (0x8005_2300, 13536), (0x8005_22FC, 13684), (0x8005_22F4, 13832), (0x8005_2DDC, 14544),
    (0x8005_2DE0, 14932),
];

/// `Front_ConstructAll` (FRONT.CPP, 0x80026e78): the screens made (each reached from its
/// global), the application, the menus.
pub fn construct_all(c: &mut Cpu) {
    let at = call(c, NEW, &[ALL_SCREENS_SIZE]);
    let screens = all_screens(c, at);
    c.m.w32(ALL_SCREENS, screens);
    for (global, offset) in SCREEN_POINTERS {
        c.m.w32(global, screens.wrapping_add(offset));
    }
    let at = call(c, NEW, &[FE_APP_SIZE]);
    let app = crate::feapp::fe_application(c, FEApplication(at));
    let at = call(c, NEW, &[ALL_MENUS_SIZE]);
    c.m.w32(FE_APP, app.0);
    let menus = crate::femenudefs::global_menu_defs(c, at);
    c.m.w32(ALL_MENUS, menus);
}

pub fn abi_construct_all(c: &mut Cpu) {
    construct_all(c);
}

/// The memory left (the largest free block, at the front end's start and end); the front end
/// in use; a post-race flag; the setup's quit flag (+0x24); the race's finishers (a word).
const MEMORY_LEFT: u32 = 0x8005_19A0;
const USE: u32 = 0x8013_E54C;
const POST_RACE: u32 = 0x8013_E569;
const QUIT: u32 = 0x24;
const FINISHERS: u32 = 0x8011_0E8C;
/// The tournament manager's money (a word).
const MONEY: u32 = 20;
/// The pink slips: each slot's winner (a half from +12), the wins of each player (+72, +73).
const PINK_WINNERS: u32 = 12;
const PINK_WINS: u32 = 72;
const LARGEST_UNUSED: fn(&mut Cpu) = nfs4rc::f_800e5e70;
const TO_MAIN_MENU: fn(&mut Cpu) = nfs4rc::f_8002c514;
/// The cars' sound banks freed (the car code's), the words unloaded.
const CAR_SOUNDS_END: fn(&mut Cpu) = nfs4rc::f_800bcb04;
const UNLOAD_WORDS: fn(&mut Cpu) = nfs4rc::f_800b9f98;
/// `FILE_delbigsync` (the host's).
const FILE_DEL_BIG_SYNC: u32 = 0x800E_B3CC;

/// How the front end is run: 0 from the start, 1 for a test, 2 after a race.
const AFTER_RACE: u32 = 2;

/// `Front_Menu` (FRONT.CPP, 0x80027b38): the front end run: the memory left noted, the
/// commentary opened, the showroom, everything constructed; a player with no car nor money
/// given the cheapest car's price; then the main menu (from the start or a test) or, after a
/// race, the tournament advanced or the pink slips' winner counted, and the post-race screens;
/// everything deleted and shut, the big file closed, the record lap time of the first track.
/// The application's result.
pub fn front_menu(c: &mut Cpu, mode: u32) -> u32 {
    c.m.w32(CARD_FIRST, 1);
    let left = call(c, LARGEST_UNUSED, &[]);
    c.m.w32(MEMORY_LEFT, left);
    let language = c.m.r8(FE + LANGUAGE) as u32;
    crate::feaudio::init_commentary(c, language);
    crate::psxfront::initialize_spinning_cars(c);
    construct_all(c);
    let m = &mut c.m;
    let owned = fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as i16;
    if owned <= 0 && (m.r32(TOURNAMENT_MANAGER + MONEY) as i32) < 1 {
        let price = fecars::cheapest_car_stock_price(m, CAR_MANAGER) as u32;
        let money = m.r32(TOURNAMENT_MANAGER + MONEY).wrapping_add(price.wrapping_add(1));
        m.w32(TOURNAMENT_MANAGER + MONEY, money);
    }
    let mut result = 0;
    if mode <= 1 {
        // (from the start `LoadConfig`, which does nothing)
        if c.m.r32(USE) != 0 {
            result = with_buffer(c, 32, |c, command| {
                call(c, TO_MAIN_MENU, &[command]);
                let app = FEApplication(c.m.r32(FE_APP));
                crate::feapp::run_front_end(c, app)
            });
        }
    } else if mode == AFTER_RACE {
        c.m.w8(POST_RACE, 1);
        let m = &mut c.m;
        let finished = m.r32(GAME_SETUP.0 + QUIT) == 0;
        let kind = m.r8(FE + RACE_KIND);
        if kind == TOURNAMENT_RACE && finished {
            crate::fetourn::advance_to_next_track(c, TOURNAMENT_MANAGER);
            crate::fetourn::update_award_information(c, TOURNAMENT_MANAGER);
        } else if kind == PINK_SLIPS_RACE && finished {
            let winner = m.r16(FE + 90) as i16;
            let few = (m.r32(FINISHERS) as i32) < 2;
            let slot = m.r8(FE + SLOT) as u32;
            let first_won = (few && winner != 0) || winner == 1;
            m.w16(FE + PINK_WINNERS + 2 * slot, (!first_won) as u16);
            let wins = FE + PINK_WINS + (!first_won) as u32;
            let v = m.r8(wins).wrapping_add(1);
            m.w8(wins, v);
        }
        let app = FEApplication(c.m.r32(FE_APP));
        result = crate::feapp::run_post_game(c, app);
    }
    delete_all(c);
    crate::feaudio::deinit_commentary(c);
    call(c, CAR_SOUNDS_END, &[]);
    call(c, UNLOAD_WORDS, &[]);
    crate::fetools::deinit(c);
    crate::psxfront::psx_exit_frontend(c);
    let big = c.m.r32(BIG_FILE);
    call_addr(c, FILE_DEL_BIG_SYNC, &[big, 100]);
    c.m.w32(BIG_FILE, 0);
    let track = c.m.r8(FE + TRACKS) as u32;
    let lap = crate::stattool::return_record_lap_time(c, track);
    c.m.w32(FE + RECORD_LAP, lap);
    let left = call(c, LARGEST_UNUSED, &[]);
    c.m.w32(MEMORY_LEFT, left);
    result
}

pub fn abi_front_menu(c: &mut Cpu) {
    c.r[2] = front_menu(c, c.r[4]);
}

/// A dialog inlined into the screens (its classes' vtables in turn: the dialog's, the
/// message's, the box's).
const DIALOG_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const BOX_VT: u32 = 0x8001_0488;

/// A dialog constructed inline at `d` (a screen: 288 wide, nothing shown, for either player),
/// through its classes' vtables.
fn inline_dialog(c: &mut Cpu, d: u32) {
    let this = fescreen::screen(c, Screen(d));
    let m = &mut c.m;
    this.set_vtable(m, DIALOG_VT);
    m.w16(d + 128, 288);
    m.w32(d + 112, 0);
    for at in [110, 130, 126, 124, 108, 106, 104, 102] {
        m.w16(d + at, 0);
    }
    m.w16(d + 100, 0xFFFF);
    m.w16(d + 136, 0);
    this.set_vtable(m, MESSAGE_VT);
    m.w32(d + 148, 0);
    m.w32(d + 132, 0);
    m.w32(d + 120, 0);
    m.w32(d + 140, 128);
    this.set_vtable(m, BOX_VT);
}

/// `tAllScreens::tAllScreens` (FRONT.CPP, 0x8002bb74): every screen of the front end
/// constructed in place, each given its class's vtable. `this`.
pub fn all_screens(c: &mut Cpu, this: u32) -> u32 {
    for (k, at) in screen::CAR_SELECTS.into_iter().enumerate() {
        crate::screencarselect::screen_car_select(c, this + at);
        if CAR_SELECT_VTS[k] != 0 {
            Screen(this + at).set_vtable(&mut c.m, CAR_SELECT_VTS[k]);
        }
    }
    for at in screen::DIALOGS {
        inline_dialog(c, this + at);
    }
    for (at, vt) in PLAIN_SCREENS {
        fescreen::screen(c, Screen(this + at)).set_vtable(&mut c.m, vt);
    }
    for (at, vt) in STANDINGS_SCREENS {
        crate::screenpost::screen_tournament_standings(c, Screen(this + at)).set_vtable(&mut c.m, vt);
    }
    crate::screentournselect::screen_tourn_select(c, Screen(this + screen::TOURN_SELECT));
    crate::screenpost::screen_tournament_standings(c, Screen(this + screen::STANDINGS));
    crate::screentrophyroom::screen_trophy_room(c, Screen(this + screen::TROPHY_ROOM));
    crate::screencontroller::screen_controller(c, this + screen::CONTROLLER);
    crate::screenaudio::screen_audio(c, Screen(this + screen::AUDIO));
    crate::screenmemcard::screen_memcard(c, this + screen::MEM_CARD);
    this
}

pub fn abi_all_screens(c: &mut Cpu) {
    c.r[2] = all_screens(c, c.r[4]);
}

/// `tAllScreens::~tAllScreens` (FRONT.CPP, 0x8002bf8c): every screen destroyed, last first
/// (in place); the object freed when `flags` asks.
pub fn all_screens_dtor(c: &mut Cpu, this: u32, flags: u32) {
    let plain = |c: &mut Cpu, at: u32| fescreen::screen_destructor(c, Screen(this + at), 2);
    for at in [14932, 14544, 13832, 13684, 13536, 13144, 12940, 11496, 11372, 11272, screen::CONTROLLER_DIALOG, 10892, 10788] {
        plain(c, at);
    }
    crate::screentrophyroom::destructor(c, Screen(this + screen::TROPHY_ROOM), 2);
    plain(c, 10052);
    plain(c, 9904);
    crate::screentournselect::destructor(c, Screen(this + screen::TOURN_SELECT), 2);
    for at in [8520, 7844, 7728] {
        plain(c, at);
    }
    for (dialog, select) in screen::DIALOGS.into_iter().rev().zip(screen::CAR_SELECTS.into_iter().rev()) {
        plain(c, dialog);
        crate::screencarselect::screen_car_select_destructor(c, this + select, 2);
    }
    crate::screencarselect::screen_car_select_destructor(c, this + 2392, 2);
    crate::screencarselect::screen_car_select_destructor(c, this + 1464, 2);
    plain(c, screen::MAIN);
    if flags & 1 != 0 {
        call(c, DELETE, &[this]);
    }
}

pub fn abi_all_screens_dtor(c: &mut Cpu) {
    all_screens_dtor(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    const STREAM: u32 = 0x8014_7000;

    /// FETOURN's tournament manager (its definition loaded) and FEINPUT's pads; the front end's
    /// players, race kind, slot, tracks and laps' choices (past the two the table has now and
    /// then), the players' names (none, letters), the language, the sticks' calibrations, the
    /// cars' flags and settings, the levels.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::fetourn::tests::shuffle(c, rng);
        crate::feinput::tests::pads(c, rng);
        let m = &mut c.m;
        m.w8(FE + 3, pick(rng, 3) as u8);
        m.w8(FE + 4, pick(rng, 4) as u8);
        m.w8(FE + 70, pick(rng, 5) as u8);
        for k in 0..5 {
            m.w8(FE + 35 + k, pick(rng, 10) as u8);
            m.w8(FE + 45 + k, [0, 1, 0, 1, pick(rng, 8)][pick(rng, 5) as usize] as u8);
        }
        for p in 0..2 {
            for (k, at) in [0x364u32, 0x374, 0x384].into_iter().enumerate() {
                let len = if k == 0 && pick(rng, 3) == 0 { 0 } else { pick(rng, 8) };
                for j in 0..8 {
                    let b = if j < len { [b'a', b'Z', b'e', 0xE9, b'1'][pick(rng, 5) as usize] } else { 0 };
                    m.w8(FE + at + 8 * p + j, b);
                }
            }
        }
        m.w8(FE + 832, pick(rng, 8) as u8);
        for k in 0..20 {
            m.w8(FE + 840 + k, rng.next() as u8);
        }
        for k in 0..450 {
            m.w8(FE + 300 + k, rng.next() as u8);
        }
        for k in 96..184 {
            m.w8(GAME_SETUP.0 + k, rng.next() as u8);
        }
        // the pads' modes as `PadInfoMode` lists them (4 the analog's, 7 the vibrating one's)
        m.w32(crate::test_util::PAD_MODES, pick(rng, 5));
        for k in 0..7 {
            m.w32(crate::test_util::PAD_MODES + 4 + 4 * k, [4, 7, 1, 2, 4, 7, pick(rng, 9)][pick(rng, 7) as usize]);
        }
        for level in [MUSIC_LEVEL, SFX_LEVEL, ENGINE_LEVEL, NARRATION_LEVEL, AMBIENT_LEVEL] {
            m.w32(level, pick(rng, 128));
        }
        m.w32(FE + 1096, pick(rng, 2));
        m.w32(FE + 1100, pick(rng, 2));
    }

    fn key(r: &mut Rng) -> u32 {
        [0x8000_0000, 0x10_0000, 0x20_0000, 0x40_0000, 0x80_0000, 0x1000_0000, 0x2000_0000, 0x4000_0000, 0x8000, 0x4000, 0x400, 1 << pick(r, 32), r.next() as u32][pick(r, 13) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Front_ResetSettingsForCar", 0x8002_706c, nfs4rc::f_8002706c, |_, r| vec![pick(r, 2), [0, 15, 16, 31, 32, 47, pick(r, 48)][pick(r, 7) as usize]]),
            ("Front_ResetSettingsMenu", 0x8002_70bc, nfs4rc::f_800270bc, |_, r| vec![pick(r, 2)]),
            ("Front_ResetPSXAnalogs", 0x8002_7170, nfs4rc::f_80027170, |_, r| vec![pick(r, 2)]),
            ("GetPSXPadValue", 0x8002_71b4, nfs4rc::f_800271b4, |_, r| vec![key(r), pick(r, 2)]),
            ("SetPads", 0x8002_752c, nfs4rc::f_8002752c, |_, _| vec![]),
            ("LoadConfig", 0x8002_7960, nfs4rc::f_80027960, |_, _| vec![]),
            ("Front_GetLapsForType", 0x8002_8004, nfs4rc::f_80028004, |_, _| vec![]),
            ("Front_EnableLocalSpeech", 0x8002_a724, nfs4rc::f_8002a724, |_, _| vec![]),
            ("Front_GetInGameVars", 0x8002_b750, nfs4rc::f_8002b750, |_, _| vec![]),
            ("SetLicensePlate", 0x8002_b84c, nfs4rc::f_8002b84c, |_, _| vec![]),
            ("Front_GetTrackRaced", 0x8002_b9e0, nfs4rc::f_8002b9e0, |_, _| vec![]),
            ("PlayerNameExist", 0x8002_ba50, nfs4rc::f_8002ba50, |_, r| vec![pick(r, 2)]),
            ("PlayerName", 0x8002_ba7c, nfs4rc::f_8002ba7c, |_, r| vec![pick(r, 2)]),
            ("PlayerNameMixedCase", 0x8002_bb08, nfs4rc::f_8002bb08, |_, r| vec![pick(r, 2)]),
        ]
    }

    #[test]
    fn front_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x900 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x901 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    /// The stream's builders' state over `shuffle`'s: the stream of any bytes, its counts in
    /// range (0..2 player cars, 0..5 opponents, cops, perps, traffic), its cars' ids, kinds and
    /// colours the game's (ids under 48, colours under 16), now and then a cop player (kind 7);
    /// a mission with stages (or none) among the structures; the front end's kind (0, 1, 2, 6),
    /// its garage, pink slips' and stock choices, the opponents' choice and kind, the options.
    fn stream_shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let m = &mut c.m;
        for k in 0..760 {
            m.w8(STREAM + k, rng.next() as u8);
        }
        m.w16(STREAM, pick(rng, 20) as u16);
        m.w16(STREAM + 2, [0, 10, 12, 13, 15, 16, pick(rng, 20)][pick(rng, 7) as usize] as u16);
        m.w16(STREAM + 4, pick(rng, 8) as u16);
        let cars = pick(rng, 3);
        m.w16(STREAM + 6, cars as u16);
        for k in 0..2 {
            let rec = STREAM + 8 + 204 * k;
            m.w8(rec, [pick(rng, 48), 28, 22, 23, 25][pick(rng, 5) as usize] as u8);
            m.w8(rec + 2, [7, pick(rng, 8), pick(rng, 8)][pick(rng, 3) as usize] as u8);
            m.w8(rec + 197, pick(rng, 16) as u8);
            m.w8(rec + 196, rng.next() as u8);
        }
        m.w16(STREAM + 416, pick(rng, 6) as u16);
        for k in 0..8 {
            let entry = STREAM + 420 + 20 * k;
            m.w32(entry + 12, pick(rng, 48));
            m.w8(entry + 16, pick(rng, 16) as u8);
        }
        m.w16(STREAM + 540, pick(rng, 5) as u16);
        m.w16(STREAM + 542, pick(rng, 3) as u16);
        for k in 0..6 {
            m.w32(STREAM + 544 + 4 * k, pick(rng, 48));
            m.w8(STREAM + 684 + k, pick(rng, 48) as u8);
        }
        m.w16(STREAM + 580, pick(rng, 7) as u16);
        for k in 0..6 {
            m.w16(STREAM + 582 + 2 * k, pick(rng, 48) as u16);
        }
        m.w16(STREAM + 594, pick(rng, 4) as u16);
        for k in 0..4 {
            m.w16(STREAM + 608 + 8 * k, pick(rng, 48) as u16);
            m.w8(STREAM + 612 + 8 * k, pick(rng, 16) as u8);
        }
        m.w8(STREAM + 680, pick(rng, 5) as u8);
        m.w8(STREAM + 8, pick(rng, 3) as u8);
        for k in 705..709 {
            m.w8(STREAM + k, [0, 1, 2, pick(rng, 256)][pick(rng, 4) as usize] as u8);
        }
        m.w8(STREAM + 658, pick(rng, 5) as u8);
        m.w8(STREAM + 660, (pick(rng, 3) == 0) as u8);
        // a mission (+10 its stages, 44 bytes each from STAGES: +0 a car, +1 its colour, +6 and
        // +10 the chasers' halves)
        let mission = [0, MISSION][pick(rng, 3).min(1) as usize];
        m.w32(STREAM + 600, mission);
        m.w32(STREAM + 604, [0, STAGES, STAGES][pick(rng, 3) as usize]);
        m.w8(MISSION + 10, pick(rng, 4) as u8);
        m.w16(MISSION + 8, rng.next() as u16);
        for k in 0..4 {
            let s = STAGES + 44 * k;
            m.w8(s, pick(rng, 48) as u8);
            m.w8(s + 1, [pick(rng, 16), rng.next() as u32 & 0xFF][pick(rng, 2) as usize] as u8);
            m.w16(s + 6, [0xFFFF, 0, 3][pick(rng, 3) as usize] as u16);
            m.w16(s + 10, [0xFFFF, 0, 3][pick(rng, 3) as usize] as u16);
        }
        m.w8(FE + 4, [0, 1, 2, 6, pick(rng, 8)][pick(rng, 5) as usize] as u8);
        for at in [291, 292, 293, 294] {
            m.w8(FE + at, pick(rng, 4) as u8);
        }
        m.w8(FE + 92, pick(rng, 20) as u8);
        m.w8(FE + 93, pick(rng, 20) as u8);
        m.w8(FE + 797, pick(rng, 20) as u8);
        m.w8(FE + 798, pick(rng, 3) as u8);
        // the players' display settings (+800..+831)
        for k in 800..832 {
            m.w8(FE + k, rng.next() as u8);
        }
        m.w8(FE + 800, pick(rng, 4) as u8);
        m.w8(FE + 801, pick(rng, 5) as u8);
        m.w8(STREAM + 656 + 42, pick(rng, 8) as u8);
        for at in [290, 298, 34, 60, 61, 62, 63, 64] {
            m.w8(FE + at, pick(rng, 2) as u8);
        }
    }

    const MISSION: u32 = 0x8014_7400;
    const STAGES: u32 = 0x8014_7440;
    const OUT: u32 = 0x8014_7600;

    fn stream_cases() -> Vec<Case> {
        vec![
            ("Front_InitStream", 0x8002_80ac, nfs4rc::f_800280ac, |_, _| vec![STREAM]),
            ("Front_InitPlayerCars", 0x8002_80dc, nfs4rc::f_800280dc, |_, _| vec![STREAM]),
            ("Front_InitTourneyTraffic", 0x8002_84a0, nfs4rc::f_800284a0, |_, _| vec![STREAM]),
            ("Front_InitOpponentCars", 0x8002_8614, nfs4rc::f_80028614, |_, _| vec![STREAM]),
            ("Front_InitCopCars", 0x8002_8cfc, nfs4rc::f_80028cfc, |_, _| vec![STREAM]),
            ("Front_InitPerps", 0x8002_90bc, nfs4rc::f_800290bc, |_, _| vec![STREAM]),
            ("Front_InitTrack", 0x8002_9284, nfs4rc::f_80029284, |_, _| vec![STREAM]),
            ("Front_InitTraffic", 0x8002_9458, nfs4rc::f_80029458, |_, _| vec![STREAM]),
            ("Front_AppendPlayerCarData", 0x8002_9690, nfs4rc::f_80029690, |_, _| vec![OUT, STREAM]),
            ("Front_AppendOpponentData", 0x8002_9b74, nfs4rc::f_80029b74, |_, _| vec![OUT, STREAM]),
            ("Front_AppendCopData", 0x8002_9ea8, nfs4rc::f_80029ea8, |_, _| vec![OUT, STREAM]),
            ("Front_AppendPerpData", 0x8002_a0fc, nfs4rc::f_8002a0fc, |_, _| vec![OUT, STREAM]),
            ("Front_AppendTrafficData", 0x8002_a394, nfs4rc::f_8002a394, |_, _| vec![OUT, STREAM]),
            ("Front_AppendTrackData", 0x8002_a5e4, nfs4rc::f_8002a5e4, |_, _| vec![OUT, STREAM]),
            ("OutputDisplaySettings", 0x8002_7dec, nfs4rc::f_80027dec, |_, r| vec![OUT, pick(r, 8), pick(r, 2), STREAM + 656]),
        ]
    }

    #[test]
    fn front_stream_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in stream_cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x920 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x921 + k as u64, stream_shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    /// The screens' object, built at 0x80144000 (15320 bytes) over FETOURN's and FEINPUT's
    /// shuffle; for the destructor, built first by the original's constructor.
    const SCREENS: u32 = 0x8014_4000;

    fn built_shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        crate::abi::call(c, nfs4rc::f_8002bb74, &[SCREENS]);
    }

    #[test]
    fn front_screens_fuzz() {
        let Some(ram) = fe_ram() else { return };
        let cases: [(&str, u32, fn(&mut Cpu), fn(&mut Cpu, &mut Rng), fn(&mut Cpu, &mut Rng) -> Vec<u32>); 2] = [
            ("tAllScreens::tAllScreens", 0x8002_bb74, nfs4rc::f_8002bb74, shuffle, |_, _| vec![SCREENS]),
            ("tAllScreens::~tAllScreens", 0x8002_bf8c, nfs4rc::f_8002bf8c, built_shuffle, |_, r| vec![SCREENS, [0, 2][pick(r, 2) as usize]]),
        ];
        for (k, (name, addr, original, shuffle, args)) in cases.into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x940 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 50, 0x941 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
