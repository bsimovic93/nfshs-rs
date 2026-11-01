//! FRONT.CPP: the front end's structure (`frontEnd`, 0x801158B8) and its glue: the defaults
//! (players' settings and cars, the pads' sticks, the levels), the pads' values and modes, the
//! memory card's checks at the start, the graphics' start, the laps, the players' names and
//! licence plates, the music's play list, the track raced; the race's stream (`Front_Init*`,
//! `Front_Append*`) and the front end's menus.
//!
//! `frontEnd` (bytes but where noted): +0 a half, +2..+7 the race's kind and mode (+3 the
//! players, +4 the kind: 1 single race, 2 a tournament), +9, +10 on, +22.. five per-race
//! slots (+35 the tracks, +45 the laps' choice), +40 the record lap time (a word), +70 the
//! race's slot, +84..+88 halves, +90 a half, +94 per player, +96 the cars' settings (48 per
//! player), +288.., +304 the cars' flags (a half per player), +500/+596/+692 the cars' settings
//! (48 per player), +788.. per player (+790, +791..+795 every other), +816..+819, +824 (4 per
//! player), +832 the language, +833..+837 the levels (music, effects, narration, engine,
//! ambient), +838, +841..+855 the analog sticks' calibrations (every other byte by pad), +857,
//! +859, +862 the pads' kinds (halves), +866 the pads' modes, +868 (8 per player), +916..+921,
//! +924 the songs on (40 words), +1092 the play list's switch, +1096 the pads' vibration (words).

use crate::abi::{self, Saved};
use crate::globals::{FE_APP, FRONT_END, GAME_SETUP, GAME_TICKS, PAD_INFO, PATHS};
use ps1rt::Cpu;

const FE: u32 = FRONT_END;
/// The managers: the cars', the tracks', the tournaments', the credits'.
const CAR_MANAGER: u32 = 0x8011_552C;
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
/// The memory card's: its start, its end, a card's check (FEMEMCARD.CPP).
const CARD_START: fn(&mut Cpu) = crate::fememcard::init_memcard_entry;
const CARD_END: fn(&mut Cpu) = crate::fememcard::deinit_memcard_entry;
const CARD_CHECK: fn(&mut Cpu) = crate::fememcard::load_game_entry;
/// The menus' destructor.
const ALL_MENUS_DTOR: fn(&mut Cpu) = nfs4rc::f_80031720;
/// The host's: `PadGetState`, `PadInfoMode`, `FILE_addbigsync`, `NFS4_LoadingIcon` (hooked).
const PAD_GET_STATE: u32 = 0x800F_07FC;
const PAD_INFO_MODE: u32 = 0x800F_08BC;
const FILE_ADD_BIG_SYNC: u32 = 0x800E_B354;
const LOADING_ICON: u32 = 0x800A_512C;

/// A port called from a port: `f` (the original at `addr`) entered from the call site returning
/// to `ra` with `args` in a0.., the callee's scratch registers left. Its v0.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `Front_DeleteAll` (FRONT.CPP, 0x80027008): the screens, the application and the menus
/// deleted (those there).
pub fn abi_delete_all(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let screens = c.m.r32(ALL_SCREENS);
        if screens != 0 {
            port(c, &r, 0x8002_bf8c, 0x8002_7024, &[screens, 3], abi_all_screens_dtor);
        }
        let app = c.m.r32(FE_APP);
        if app != 0 {
            port(c, &r, 0x8001_331c, 0x8002_7040, &[app, 3], crate::feapp::abi_fe_application_dtor);
        }
        let menus = c.m.r32(ALL_MENUS);
        if menus != 0 {
            r.call_leaving(c, ALL_MENUS_DTOR, &[menus, 3], 0x8002_705c);
        }
    })
}

/// `Front_ResetSettingsForCar` (FRONT.CPP, 0x8002706c): player `p`'s car `car` set back: its
/// flag off, its settings 50, 50 and 3.
pub fn abi_reset_settings_for_car(c: &mut Cpu) {
    let (p, car) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let flags = FE + 304 + 2 * p;
    let v = m.r16(flags) as u32 & !(1u32 << (car & 31));
    m.w16(flags, v as u16);
    let at = car.wrapping_add(p.wrapping_mul(48)).wrapping_add(FE);
    m.w8(at + 596, 50);
    m.w8(at + 500, 50);
    m.w8(at + 692, 3);
    c.r[2] = 3;
}

/// `Front_ResetSettingsMenu` (FRONT.CPP, 0x800270bc): every car of player `p` set back.
pub fn abi_reset_settings_menu(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = p;
        for car in 0..48 {
            r.0[0] = car;
            port(c, &r, 0x8002_706c, 0x8002_70e0, &[p, car], abi_reset_settings_for_car);
        }
    })
}

/// `Front_ResetPSXController` (FRONT.CPP, 0x80027104): a moment waited, the pads read; pad
/// `pad`'s kind noted, its mode `mode`.
pub fn abi_reset_psx_controller(c: &mut Cpu) {
    let (pad, mode) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (pad, mode);
        r.call_leaving(c, TIMED_WAIT, &[10], 0x8002_7124);
        r.call_leaving(c, PAD_UPDATE, &[], 0x8002_712c);
        let m = &mut c.m;
        let kind = m.r8(PAD_INFO + 32 * pad + 5);
        m.w16(FE + 862 + 2 * pad, kind as u16);
        m.w8(FE + 866 + pad, mode as u8);
    })
}

/// `Front_ResetPSXAnalogs` (FRONT.CPP, 0x80027170): pad `pad`'s sticks' calibrations, the
/// defaults.
pub fn abi_reset_psx_analogs(c: &mut Cpu) {
    let at = c.r[4].wrapping_add(FE);
    for (k, v) in [121, 10, 255, 255, 127, 6, 127, 6].into_iter().enumerate() {
        c.m.w8(at + 841 + 2 * k as u32, v);
    }
    (c.r[2], c.r[3]) = (6, 127);
}

/// `GetPSXPadValue` (FRONT.CPP, 0x800271b4): the pads read; key `key` of pad `pad` as the
/// game's controls take it: an analog pad's (0x53, 0x73) stick directions and a NeGcon's
/// (0x23) twist, buttons and shoulder with their calibrations (low bit 1), else the key itself
/// (`pad << 26 | key << 8 | 2`).
pub fn abi_get_psx_pad_value(c: &mut Cpu) {
    let (key, pad) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (key, pad);
        r.call_leaving(c, PAD_UPDATE, &[], 0x8002_71d0);
        let m = &mut c.m;
        let info = PAD_INFO + (pad << 5);
        let kind = if m.r8(info + 4) == 0 { m.r8(info + 5) as u32 } else { 0 };
        let o = |m: &mut ps1rt::Mem, k: u32| m.r8(pad.wrapping_add(FE + k)) as u32;
        let up = |v: u32| v.wrapping_add(128);
        let down = |v: u32| 127u32.wrapping_sub(v);
        let sh = pad << 30;
        let v = match (kind, key) {
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
        };
        c.r[2] = v;
    })
}

/// `SetPads` (FRONT.CPP, 0x8002752c): each pad, once stable (waited for up to 128 ticks), put
/// in the mode it has (4, or 7 when it vibrates) if it lists it.
pub fn abi_set_pads(c: &mut Cpu) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[8] = 0x8014_0000;
        for p in 0..2u32 {
            r.0[5] = p;
            r.0[7] = FE + 4 * p;
            r.0[6] = 4;
            let port_id = p << 4;
            r.0[1] = port_id;
            if c.m.r32(FE + 1096 + 4 * p) != 0 {
                r.0[6] = 7;
            }
            let start = c.m.r32(GAME_TICKS);
            r.0[0] = start;
            loop {
                let now = c.m.r32(GAME_TICKS);
                if (now.wrapping_sub(start) as i32) >= 128 {
                    break;
                }
                if r.call_ptr_leaving(c, PAD_GET_STATE, &[port_id], 0x8002_75ac) == 6 {
                    break;
                }
            }
            let state = r.call_ptr_leaving(c, PAD_GET_STATE, &[port_id], 0x8002_75c4);
            r.0[4] = 0;
            if state != 6 {
                continue;
            }
            r.0[3] = 0;
            let count = r.call_ptr_leaving(c, PAD_INFO_MODE, &[port_id, 4, u32::MAX], 0x8002_75e4);
            r.0[2] = count;
            let mut k = 0u32;
            r.0[0] = 0;
            while (k as i32) < count as i32 {
                if r.call_ptr_leaving(c, PAD_INFO_MODE, &[port_id, 4, k], 0x8002_7604) == r.0[6] {
                    r.0[3] = 1;
                    r.0[4] = k;
                }
                k += 1;
                r.0[0] = k;
            }
            if r.0[3] != 0 {
                r.call_leaving(c, PAD_SET_MAIN_MODE, &[port_id, r.0[4], 0], 0x8002_762c);
            }
        }
    })
}

/// `InitFrontEndStructure` (FRONT.CPP, 0x80027668): the front end's defaults: the race's
/// settings and slots cleared, the record lap time of the first track; for each player the
/// settings, the cars' settings from their cars, the pads' kinds and sticks (and the pads
/// reset); every car's settings; the levels 85, the language 0; the credits set up.
pub fn abi_init_front_end_structure(c: &mut Cpu) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = 0;
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
        (r.0[3], r.0[6], r.0[5], r.0[7], r.0[1], r.0[8], r.0[2]) = (0, 1, FE, FE, FE, 0, FE);
        m.w32(sp + 16, 0);
        let track = m.r8(FE + 35) as u32;
        r.0[0] = 1;
        for at in [33, 34, 296, 297] {
            m.w8(FE + at, 0);
        }
        m.w8(FE + 790, 1);
        m.w8(FE + 44, 0);
        let lap = port(c, &r, 0x8004_b33c, 0x8002_775c, &[track], crate::stattool::abi_return_record_lap_time);
        let m = &mut c.m;
        m.w32(FE + 40, lap);
        for at in [9, 10, 918] {
            m.w8(FE + at, 1);
        }
        for at in [288, 289, 290, 295, 299, 300, 799] {
            m.w8(FE + at, 0);
        }
        for p in 0..2u32 {
            let (s1, s2, s7, fp) = (FE + p, FE + 4 * p, FE + 2 * p, 4 * p);
            (r.0[1], r.0[2], r.0[7], r.0[8], r.0[3]) = (s1, s2, s7, fp, p);
            r.0[0] = 0;
            let m = &mut c.m;
            m.w32(s2 + 1096, 1);
            m.w8(s1 + 788, 0);
            m.w8(s1 + 94, 1);
            m.w8(s1 + 800, 0);
            for at in [802, 804, 806, 808, 810, 812] {
                m.w8(s1 + at, 1);
            }
            m.w8(s1 + 814, 0);
            m.w8(s1 + 72, 0);
            m.w32(s2 + 76, 0);
            m.w16(FE + 90, 0xFFFF);
            m.w8(s2 + 817, 3);
            m.w8(s2 + 818, 6);
            m.w8(s2 + 819, 7);
            m.w8(s2 + 816, 0);
            m.w8(FE + (p << 3) + 868, 0);
            for k in 0..4 {
                m.w8(FE + k + fp + 824, 0);
            }
            let base = m.r32(sp + 16);
            r.0[4] = base;
            for at in [791, 793, 795] {
                m.w8(s1 + at, 1);
            }
            for k in 0..48u32 {
                r.0[0] = k;
                let car = port(c, &r, 0x8001_6374, 0x8002_7834, &[CAR_MANAGER, k as u16 as i16 as i32 as u32, 0x8011_0000], crate::fecars::abi_get_car_from_id);
                r.0[0] = k + 1;
                let v = c.m.r8(car + 4);
                c.m.w8(FE + 96 + k + base, v);
            }
            let m = &mut c.m;
            m.w16(s7 + 862, 65);
            m.w8(s1 + 866, 0);
            m.w8(s1 + 921, 1);
            r.0[4] = 85;
            m.w8(s1 + 857, 85);
            m.w8(s1 + 859, 85);
            port(c, &r, 0x8002_7170, 0x8002_7878, &[p], abi_reset_psx_analogs);
            port(c, &r, 0x8002_7104, 0x8002_7884, &[p, 0], abi_reset_psx_controller);
            let v = c.m.r32(sp + 16).wrapping_add(48);
            c.m.w32(sp + 16, v);
        }
        (r.0[1], r.0[7], r.0[8], r.0[2], r.0[3]) = (FE + 2, FE + 4, 8, FE + 8, 2);
        r.0[0] = FE;
        c.m.w8(FE + 917, 0);
        port(c, &r, 0x8002_70bc, 0x8002_78c0, &[0], abi_reset_settings_menu);
        port(c, &r, 0x8002_70bc, 0x8002_78c8, &[1], abi_reset_settings_menu);
        let m = &mut c.m;
        m.w8(FE + 798, 1);
        m.w8(FE + 838, 2);
        m.w8(FE + 797, 0);
        m.w8(FE + 832, 0);
        for at in 833..838 {
            m.w8(FE + at, 85);
        }
        m.w8(FE + 839, 0);
        for level in [MUSIC_LEVEL, SFX_LEVEL, NARRATION_LEVEL, AMBIENT_LEVEL] {
            m.w32(level, 85);
        }
        m.w8(FE + 840, 0);
        m.w32(FE + 1092, 0);
        port(c, &r, 0x8003_5ca0, 0x8002_7930, &[CREDIT_MANAGER], crate::fecredits::abi_setup);
    })
}

/// `LoadConfig` (FRONT.CPP, 0x80027960): nothing; 0.
pub fn abi_load_config(c: &mut Cpu) {
    c.r[2] = 0;
}

/// `Front_InitialMemCardCheck` (FRONT.CPP, 0x80027968): the card's state cleared, the loading
/// icon shown (the host's), the records' defaults, the front end's structure; language 0.
pub fn abi_initial_mem_card_check(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        for at in [USE_FRONT_END2, USE_FRONT_END, CARD_STATE, CARD_RETRY] {
            c.m.w32(at, 0);
        }
        r.call_ptr_leaving(c, LOADING_ICON, &[], 0x8002_7994);
        c.m.w32(CARD_FIRST, 1);
        port(c, &r, 0x8004_b174, 0x8002_79b0, &[TRACK_RECORDS, 0], crate::stattool::abi_get_all_default_records);
        r.enter_leaving(c, 0x8002_7668, 0x8002_79b8, abi_init_front_end_structure);
        c.m.w8(FE + 832, 0);
        c.r[2] = 0x8011_0000;
    })
}

/// `Front_SecondaryMemCardCheck` (FRONT.CPP, 0x800279cc): the card started; each card checked
/// (up to three tries while it answers), the retry flag cleared; the card ended, the pads set.
pub fn abi_secondary_mem_card_check(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        c.m.w32(USE_FRONT_END, 0);
        c.m.w32(USE_FRONT_END2, 1);
        r.call_leaving(c, CARD_START, &[0, 0], 0x8002_7a04);
        r.0[3] = 0x8005_0000;
        for card in 0..2u32 {
            r.0[1] = card;
            r.0[0] = 0;
            if c.m.r32(CARD_RETRY) == 0 {
                r.0[2] = card << 16;
                let mut tries = 0;
                loop {
                    let v = r.call_leaving(c, CARD_CHECK, &[card as u16 as i16 as i32 as u32, 0, 0], 0x8002_7a3c);
                    let ok = (v << 16 == 0) as u32;
                    tries += 1;
                    r.0[0] = tries;
                    c.m.w32(CARD_RETRY, ok);
                    if tries >= 3 || ok != 0 {
                        break;
                    }
                }
            }
            c.m.w32(CARD_RETRY, 0);
        }
        r.0[1] = 2;
        r.call_leaving(c, CARD_END, &[], 0x8002_7a70);
        r.enter_leaving(c, 0x8002_752c, 0x8002_7a78, abi_set_pads);
        c.m.w32(USE_FRONT_END, 1);
        c.r[2] = 1;
    })
}

/// `Front_InitGraphics` (FRONT.CPP, 0x80027aa0): the front end's big file (`zzFE.viv`) opened
/// (stuck for ever when it can't be), the movie buffer reset, the PlayStation's front end
/// started.
pub fn abi_init_graphics(c: &mut Cpu) {
    abi::frame(c, 64, |c, sp| {
        let r = Saved::of(c);
        r.call_leaving(c, INIT_LINK_MODE, &[0, 1, 1], 0x8002_7ab8);
        let path = c.m.r32(FE_PATH);
        r.call_leaving(c, SPRINTF, &[sp + 16, FE_VIV, path], 0x8002_7ad0);
        if r.call_ptr_leaving(c, FILE_ADD_BIG_SYNC, &[sp + 16, 0, 100, BIG_FILE], 0x8002_7ae8) == 0 {
            loop {
                c.tick();
            }
        }
        r.call_leaving(c, RESET_DCT_BUFFER, &[], 0x8002_7b00);
        r.enter_leaving(c, 0x8004_e2e0, 0x8002_7b08, crate::psxfront::abi_init_psx_front_end);
    })
}

/// `Front_InitGraphicsAndDisplayLoading` (FRONT.CPP, 0x80027b18): `Front_InitGraphics`.
pub fn abi_init_graphics_and_display_loading(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter_leaving(c, 0x8002_7aa0, 0x8002_7b28, abi_init_graphics);
    })
}

/// `Front_GetLapsForType` (FRONT.CPP, 0x80028004): the laps: a tournament's track's, else the
/// race slot's choice of 2 or 4 (a choice past those reads the stack above the table).
pub fn abi_get_laps_for_type(c: &mut Cpu) {
    abi::frame(c, 8, |c, sp| {
        let m = &mut c.m;
        m.w16(sp, 2);
        m.w16(sp + 2, 4);
        c.r[2] = if m.r8(FE + 4) != 2 {
            let slot = m.r8(FE + 70) as u32;
            let choice = m.r8(slot + FE + 45) as u32;
            m.r16(sp + 2 * choice) as i16 as i32 as u32
        } else {
            let t = TOURNAMENT_MANAGER;
            let (tier, definition) = (m.r32(t + 4), m.r32(t + 24));
            let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
            let tournament = first.wrapping_add(m.r32(t + 8));
            m.r8(definition.wrapping_add(tournament.wrapping_mul(84)) + 105) as u32
        };
    })
}

/// `Front_InitStream` (FRONT.CPP, 0x800280ac): the stream's counts cleared, the race's car list
/// made.
pub fn abi_init_stream(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 24, |c, _| {
        for at in [0, 2, 4] {
            c.m.w16(stream + at, 0);
        }
        port(c, &Saved::of(c), 0x8001_76c0, 0x8002_80cc, &[CAR_MANAGER], crate::fecars::abi_initialize_ingame_car_list);
    })
}

/// `Front_EnableLocalSpeech` (FRONT.CPP, 0x8002a724): a single race's track speaks its own
/// language (one of the first three, or the sixth) other than the front end's: 1, else 0.
pub fn abi_enable_local_speech(c: &mut Cpu) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = FE;
        r.0[1] = 0;
        c.r[2] = 0;
        if c.m.r8(FE + 4) != 1 {
            return;
        }
        let slot = c.m.r8(FE + 70) as u32;
        let track = c.m.r8(slot + FE + 35) as u32;
        port(c, &r, 0x8002_1c04, 0x8002_a768, &[TRACK_MANAGER, track, sp + 16], crate::fetracks::abi_get_track);
        let language = c.m.r8(sp + 43) as u32;
        c.r[2] = (language != c.m.r8(FE + 832) as u32 && (language < 3 || language == 6)) as u32;
    })
}

/// `Front_GetInGameVars` (FRONT.CPP, 0x8002b750): the levels, the music's volume, the setup's
/// settings and each pad's mode and sticks back from the game's setup.
pub fn abi_get_in_game_vars(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = FE;
        let music = c.m.r8(MUSIC_LEVEL);
        c.m.w8(FE + 833, music);
        let v = (music as u32).wrapping_mul(35) as i32 >> 6;
        r.call_leaving(c, MUSIC_VOLUME, &[v as u32], 0x8002_b78c);
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
            for (from, to) in [(96, 866), (104, 843), (112, 841), (120, 847), (128, 845), (168, 857), (176, 859)] {
                let v = m.r8(t + from);
                m.w8(FE + p + to, v);
            }
        }
    })
}

/// `SetLicensePlate` (FRONT.CPP, 0x8002b84c): each player's licence (+0x384, 8 each) from their
/// name, its letters upper case then lower, and the car's plate made from it.
pub fn abi_set_license_plate(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = FE + 0x384;
        for p in 0..2u32 {
            r.0[2] = p;
            r.0[1] = p;
            let name = port(c, &r, 0x8002_ba7c, 0x8002_b880, &[p], abi_player_name);
            let licence = (p << 3) + FE + 0x384;
            r.0[0] = licence;
            r.call_leaving(c, SPRINTF, &[licence, name], 0x8002_b894);
            port(c, &r, 0x8004_b128, 0x8002_b89c, &[licence], crate::stattool::abi_upper_case);
            port(c, &r, 0x8002_6970, 0x8002_b8a4, &[licence], crate::fetools::abi_s_lower);
            r.call_leaving(c, CLEAN_UP_LICENSE, &[p], 0x8002_b8ac);
            r.call_leaving(c, CREATE_LICENSE, &[licence, 0, p], 0x8002_b8bc);
        }
    })
}

/// `SetPlayList` (FRONT.CPP, 0x8002b8f4): the in-game music's songs looked up (`game*`); unless
/// `keep`, the songs on are those; the play list switched on, the front end's music back.
pub fn abi_set_play_list(c: &mut Cpu) {
    let keep = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = keep;
        r.call_leaving(c, MUSIC_CLEAN_UP, &[], 0x8002_b908);
        r.call_leaving(c, MUSIC_START_UP, &[0xC000, 0x1_8000, YMUS], 0x8002_b920);
        let songs = r.call_leaving(c, MUSIC_FIND, &[GAME_SONGS, 0], 0x8002_b930);
        let m = &mut c.m;
        if keep == 0 {
            for k in (0..40).rev() {
                m.w32(FE + 924 + 4 * k, 0);
            }
            let mut k = 0u32;
            while (k as i32) < m.r32(songs) as i32 {
                let song = m.r32(songs.wrapping_add(64 * k) + 36);
                m.w32((song << 2).wrapping_add(FE) + 924, 1);
                k += 1;
            }
        }
        m.w32(FE + 1092, 1);
        r.call_leaving(c, PURGE, &[songs], 0x8002_b9b0);
        r.call_leaving(c, MUSIC_CLEAN_UP, &[], 0x8002_b9b8);
        r.call_leaving(c, MUSIC_START_UP, &[0xD800, 0x1_8000, AMUS], 0x8002_b9d0);
    })
}

/// `Front_GetTrackRaced` (FRONT.CPP, 0x8002b9e0): the track raced: a tournament's last, else
/// the race slot's track's number.
pub fn abi_get_track_raced(c: &mut Cpu) {
    abi::frame(c, 72, |c, sp| {
        let r = Saved::of(c);
        let v = if c.m.r8(FE + 4) == 2 {
            port(c, &r, 0x8003_35c8, 0x8002_ba0c, &[TOURNAMENT_MANAGER], crate::fetourn::abi_get_last_track_raced)
        } else {
            let slot = c.m.r8(FE + 70) as u32;
            let track = c.m.r8(slot + FE + 35) as u32;
            port(c, &r, 0x8002_1c04, 0x8002_ba2c, &[TRACK_MANAGER, track, sp + 16], crate::fetracks::abi_get_track);
            c.m.r8(sp + 16) as i8 as i32 as u32
        };
        c.r[2] = v as u16 as i16 as i32 as u32;
    })
}

/// `PlayerNameExist` (FRONT.CPP, 0x8002ba50): whether player `p` has a name.
pub fn abi_player_name_exist(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 24, |c, _| {
        let n = Saved::of(c).call_leaving(c, STRLEN, &[(p << 3).wrapping_add(FE + 0x364)], 0x8002_ba6c);
        c.r[2] = (n != 0) as u32;
    })
}

/// `PlayerName` (FRONT.CPP, 0x8002ba7c): player `p`'s name in capitals (copied to +0x374), or
/// the word for the player (`Player`, or `Player 1`/`2` with two).
pub fn abi_player_name(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = p;
        r.0[1] = FE + 0x374;
        let name = (p << 3).wrapping_add(FE + 0x374);
        r.0[0] = name;
        r.call_leaving(c, SPRINTF, &[name, (p << 3).wrapping_add(FE + 0x364)], 0x8002_bab4);
        if r.call_leaving(c, STRLEN, &[name], 0x8002_babc) != 0 {
            port(c, &r, 0x8004_b128, 0x8002_bacc, &[name], crate::stattool::abi_upper_case);
            c.r[2] = name;
        } else {
            let word = if c.m.r8(FE + 3) == 1 { p.wrapping_add(81) } else { 79 };
            r.call_leaving(c, TEXT_WORD, &[word], 0x8002_baf0);
        }
    })
}

/// `PlayerNameMixedCase` (FRONT.CPP, 0x8002bb08): player `p`'s name as typed, or the word for
/// the player.
pub fn abi_player_name_mixed_case(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = p;
        r.0[1] = FE + 0x364;
        let name = (p << 3).wrapping_add(FE + 0x364);
        r.0[0] = name;
        if r.call_leaving(c, STRLEN, &[name], 0x8002_bb38) != 0 {
            c.r[2] = name;
        } else {
            let word = if c.m.r8(FE + 3) == 1 { p.wrapping_add(81) } else { 79 };
            r.call_leaving(c, TEXT_WORD, &[word], 0x8002_bb5c);
        }
    })
}

/// The race's stream (`tFEStream`): +0 its size so far (a half, 2 a car), +2 the cars' memory
/// (a half, 6 a car not an added model, up to 13), +6 the player cars (a half), +8 their
/// records (204 bytes: +0 the car, +2 its kind, +175 the colours, +196, +197 the colour,
/// +199), +420 the race's car list (20 bytes: +0 on, +12 the car, +16 its colour, +17).
const CAR_RECORD: u32 = 204;
const LIST: u32 = 420;

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

/// `Front_InitPlayerCars` (FRONT.CPP, 0x800280dc): the players' cars into the stream: a
/// tournament's garage car; the pink slips' two; else each player's (two with two players)
/// stock car in its chosen colour, or garage car (a cop car's lights from the settings); then
/// each listed for the race (the memory of those not added models counted).
pub fn abi_init_player_cars(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[3]) = (stream, FE);
        c.m.w16(stream + 6, 0);
        let kind = c.m.r8(FE + 4) as u32;
        if kind == 2 {
            let car = c.m.r8(FE + 291) as u32;
            port(c, &r, 0x8001_7094, 0x8002_812c, &[CAR_MANAGER, car, stream + 8, 0], crate::fecars::abi_get_garage_car);
            own_colour(c, stream);
            next_car(c, stream);
        } else {
            r.0[2] = 0;
            if kind == 6 {
                r.0[0] = CAR_MANAGER;
                let car = c.m.r8(FE + 293) as u32;
                port(c, &r, 0x8001_71a0, 0x8002_8158, &[CAR_MANAGER, car, stream + 8, 0], crate::fecars::abi_get_pink_slips_car);
                own_colour(c, stream);
                next_car(c, stream);
                let k = player_cars(c, stream);
                let car = c.m.r8(FE + 294) as u32;
                port(c, &r, 0x8001_71a0, 0x8002_81d0, &[CAR_MANAGER, car, record(stream, k), 1], crate::fecars::abi_get_pink_slips_car);
                own_colour(c, stream);
                next_car(c, stream);
            } else {
                let mut p = 0u32;
                loop {
                    r.0[2] = p;
                    let k = player_cars(c, stream);
                    let rec = record(stream, k);
                    if c.m.r8(FE + 298) == 0 {
                        r.0[0] = p;
                        let car = c.m.r8(p + FE + 92) as u32;
                        r.enter_leaving(c, 0x8001_6fd8, 0x8002_8268, |c| {
                            (c.r[4], c.r[5], c.r[6]) = (CAR_MANAGER, car, rec);
                            crate::fecars::abi_get_stock_car(c)
                        });
                        let k = player_cars(c, stream);
                        let rec = record(stream, k);
                        let m = &mut c.m;
                        let model = m.r8(rec) as i8 as i32 as u32;
                        let choice = m.r8(model.wrapping_add(p.wrapping_mul(48)).wrapping_add(FE) + 96) as u32;
                        let v = m.r8(rec.wrapping_add(choice) + 175);
                        m.w8(rec + 197, v);
                    } else {
                        let car = c.m.r8(p + FE + 291) as u32;
                        port(c, &r, 0x8001_7094, 0x8002_82e8, &[CAR_MANAGER, car, rec, p], crate::fecars::abi_get_garage_car);
                        own_colour(c, stream);
                    }
                    let k = player_cars(c, stream);
                    let rec = record(stream, k);
                    if c.m.r8(rec + 2) == 7 {
                        let m = &mut c.m;
                        let model = m.r8(rec) as i8 as i32 as u32;
                        let lights = m.r8(model.wrapping_add(k.wrapping_mul(48)).wrapping_add(FE) + 192);
                        m.w8(rec + 197, 0);
                        m.w8(rec + 199, lights);
                    }
                    next_car(c, stream);
                    if c.m.r8(FE + 3) != 1 {
                        break;
                    }
                    p = p.wrapping_add(1);
                    r.0[2] = p;
                    if p as u16 as i16 >= 2 {
                        break;
                    }
                }
            }
        }
        r.0[2] = 0;
        r.0[0] = 0x8011_0000;
        let mut k = 0u32;
        while (k as u16 as i16 as i32) < player_cars(c, stream) as i32 {
            let k16 = k as u16 as i16 as i32 as u32;
            let rec = stream.wrapping_add(k16.wrapping_mul(CAR_RECORD));
            let m = &mut c.m;
            let car = m.r8(rec + 8) as i8 as i32 as u32;
            m.w32(sp + 16, car);
            let colour = m.r8(rec + 205);
            m.w8(sp + 20, colour);
            let added = port(c, &r, 0x8001_7740, 0x8002_83ec, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_is_car_an_added_model);
            if added != 1 {
                let m = &mut c.m;
                let memory = m.r16(stream + 2);
                if (memory as i16) < 13 {
                    m.w16(stream + 2, memory.wrapping_add(6));
                }
            }
            port(c, &r, 0x8001_77c0, 0x8002_8420, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_add_car_to_ingame_list);
            let m = &mut c.m;
            let entry = stream.wrapping_add(k16.wrapping_mul(20));
            let size = m.r16(stream).wrapping_add(2);
            m.w16(stream, size);
            let (car, colour) = (m.r32(sp + 16), m.r8(sp + 20));
            m.w32(entry + LIST, 1);
            m.w32(entry + LIST + 12, car);
            m.w8(entry + LIST + 16, colour);
            let v = m.r8(stream.wrapping_add(k16.wrapping_mul(CAR_RECORD)) + 204);
            k = k.wrapping_add(1);
            r.0[2] = k;
            m.w8(entry + LIST + 17, v);
        }
    })
}

/// The tournament manager's current tournament's record (84 bytes from the definition's +36).
fn tournament_record(c: &mut Cpu) -> u32 {
    let m = &mut c.m;
    let t = TOURNAMENT_MANAGER;
    let (tier, definition) = (m.r32(t + 4), m.r32(t + 24));
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
    let tournament = first.wrapping_add(m.r32(t + 8));
    definition.wrapping_add(tournament.wrapping_mul(84)).wrapping_add(36)
}

/// `Front_InitTourneyTraffic` (FRONT.CPP, 0x800284a0): a tournament with traffic: three
/// traffic cars (+580 their count, +582 the cars) from the opponents' six (+684, in turn),
/// each listed for the race unless an added model (counted in the cars' memory).
pub fn abi_init_tourney_traffic(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = stream;
        c.m.w8(sp + 20, 0);
        let tournament = tournament_record(c);
        c.m.w16(stream + 580, 0);
        if c.m.r8(FE + 4) != 2 || c.m.r8(tournament + 4) == 0 {
            return;
        }
        (r.0[1], r.0[4], r.0[3], r.0[2], r.0[5]) = (0, CAR_MANAGER, sp + 16, sp + 20, 3);
        loop {
            let k = r.0[1];
            let next = k.wrapping_add(1);
            let car = c.m.r8(stream.wrapping_add(k as u16 as i16 as i32 as u32) + 684) as u32;
            r.0[1] = if (next as u16 as i16) < 6 { next } else { 0 };
            c.m.w32(sp + 16, car);
            let added = r.enter_leaving(c, 0x8001_7740, 0x8002_8590, |c| {
                (c.r[4], c.r[5], c.r[6]) = (CAR_MANAGER, sp + 16, sp + 20);
                crate::fecars::abi_is_car_an_added_model(c);
                c.r[2]
            });
            if added != 1 {
                let memory = c.m.r16(stream + 2).wrapping_add(1);
                c.m.w16(stream + 2, memory);
                port(c, &r, 0x8001_77c0, 0x8002_85b4, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_add_car_to_ingame_list);
            }
            let m = &mut c.m;
            let n = m.r16(stream + 580) as i16 as i32 as u32;
            let car = m.r16(sp + 16);
            m.w16(stream.wrapping_add(n << 1) + 582, car);
            let size = m.r16(stream).wrapping_add(1);
            let n = m.r16(stream + 580).wrapping_add(1);
            m.w16(stream + 580, n);
            m.w16(stream, size);
            if n as i16 >= 3 {
                break;
            }
        }
    })
}

/// The race's car list's entry `k` (from +420, 20 bytes).
fn list_entry(stream: u32, k: u32) -> u32 {
    stream.wrapping_add(k.wrapping_mul(20)).wrapping_add(LIST)
}

/// `Front_InitOpponentCars` (FRONT.CPP, 0x80028614): the opponents (+416 their count): a
/// tournament's (its line-up, or with ten racers the class's of the player's car) or a
/// same-class race's, each its car (+57 of the record) made like an available one, listed and
/// coloured (the record's colours, or the player's with ten racers; none with the cheat off a
/// tournament's first race), their places; else a single race's one opponent, the chosen stock
/// car (in the class when memory is short), or none.
pub fn abi_init_opponent_cars(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 312, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = stream;
        c.m.w16(stream + 416, 0);
        let kind = c.m.r8(FE + 4) as u32;
        let racing = kind == 2 || (kind == 0 && c.m.r8(FE + 798) == 2);
        if racing {
            c.m.w32(sp + 264, 0);
            let t2 = tournament_record(c);
            let racers = if kind == 2 { c.m.r8(t2 + 3) as u32 } else { 10 };
            let record = if racers == 10 {
                let m = &mut c.m;
                let definition = m.r32(TOURNAMENT_MANAGER + 24);
                let class = (m.r8(definition + 26) as u32).wrapping_add(m.r8(stream + 8) as u32);
                m.w32(sp + 264, 1);
                definition.wrapping_add(class.wrapping_mul(84)).wrapping_add(36)
            } else {
                tournament_record(c)
            };
            r.0[3] = record;
            let mut count = 5u32;
            if c.m.r8(FE + 4) == 2 {
                count = c.m.r32(TOURNAMENT_MANAGER + 16).wrapping_sub(1);
            }
            r.0[7] = count;
            r.0[1] = 0;
            if (count << 16) as i32 > 0 {
                (r.0[4], r.0[6], r.0[5], r.0[8]) = (CAR_MANAGER, sp + 248, sp + 252, FE);
                let mut k = 0u32;
                loop {
                    let k16 = k as u16 as i16 as i32 as u32;
                    let id = c.m.r8(record.wrapping_add(k16) + 57) as u32;
                    c.m.w32(sp + 248, id);
                    let car = port(c, &r, 0x8001_6374, 0x8002_87cc, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
                    r.0[0] = car;
                    let colour = c.m.r8(car + 4);
                    c.m.w8(sp + 252, colour);
                    let added = port(c, &r, 0x8001_7740, 0x8002_87e8, &[CAR_MANAGER, sp + 248, sp + 252], crate::fecars::abi_is_car_an_added_model);
                    if added != 1 {
                        let memory = c.m.r16(stream + 2);
                        if (memory as i16) < 16 {
                            c.m.w16(stream + 2, memory.wrapping_add(3));
                        }
                    }
                    c.m.w32(sp + 16, 0);
                    port(c, &r, 0x8001_7860, 0x8002_8824, &[CAR_MANAGER, sp + 248, sp + 252, 0], crate::fecars::abi_find_similar_car);
                    port(c, &r, 0x8001_77c0, 0x8002_8834, &[CAR_MANAGER, sp + 248, sp + 252], crate::fecars::abi_add_car_to_ingame_list);
                    let m = &mut c.m;
                    let entry = list_entry(stream, k16.wrapping_add(1));
                    let (id, colour) = (m.r32(sp + 248), m.r8(sp + 252) as u32);
                    m.w32(entry, 0);
                    m.w32(entry + 12, id);
                    let v = m.r8(car.wrapping_add(colour) + 175);
                    m.w8(entry + 16, v);
                    let v = if m.r32(sp + 264) != 0 { m.r8(stream + 204) } else { m.r8(record.wrapping_add(k16) + 62) };
                    m.w8(entry + 17, v);
                    if c.m.r8(FE + 4) == 2 && c.m.r8(FE + 290) == 0 {
                        let on = port(c, &r, 0x8002_373c, 0x8002_88c0, &[13], crate::fecheats::abi_is_cheat_enabled);
                        let v = if on != 0 && c.m.r8(FE + 34) != 0 { c.m.r8(record.wrapping_add(k16) + 62) } else { 0 };
                        c.m.w8(entry + 17, v);
                    }
                    k = k.wrapping_add(1);
                    r.0[1] = k;
                    let m = &mut c.m;
                    let size = m.r16(stream).wrapping_add(2);
                    let n = m.r16(stream + 416).wrapping_add(1);
                    m.w16(stream, size);
                    m.w16(stream + 416, n);
                    if ((k << 16) as i32) >= ((count << 16) as i32) {
                        break;
                    }
                }
            }
            r.0[1] = 0;
            if c.m.r8(FE + 4) == 2 {
                r.0[0] = TOURNAMENT_MANAGER;
                port(c, &r, 0x8003_3714, 0x8002_8970, &[TOURNAMENT_MANAGER], crate::fetourn::abi_update_car_lineup);
                let m = &mut c.m;
                let n = m.r32(TOURNAMENT_MANAGER + 16).wrapping_sub(1);
                m.w16(stream + 416, n as u16);
                let places = (count as u16 as i16 as i32).wrapping_add(1);
                let lineup = TOURNAMENT_MANAGER + 376;
                if places > 0 {
                    let mut k = 0i32;
                    loop {
                        let from = lineup.wrapping_add(20 * k as u32);
                        let to = list_entry(stream, k as u32);
                        let v = m.r32(from + 4);
                        m.w32(to + 4, v);
                        k += 1;
                        let v = m.r8(from + 8);
                        m.w8(to + 8, v);
                        if (k as i16 as i32) >= places {
                            break;
                        }
                    }
                }
            } else {
                let m = &mut c.m;
                for k in 0..5u32 {
                    let entry = list_entry(stream, k + 1);
                    m.w32(entry + 4, k);
                    m.w8(entry + 8, (k + 1) as u8);
                }
                m.w8(stream + 428, 6);
            }
            return;
        }
        let m = &mut c.m;
        if m.r8(FE + 4) < 2 && m.r8(FE + 798) == 1 {
            r.0[1] = CAR_MANAGER;
            let choice = m.r8(FE + 797) as u32;
            r.enter_leaving(c, 0x8001_6fd8, 0x8002_8a78, |c| {
                (c.r[4], c.r[5], c.r[6]) = (CAR_MANAGER, choice, sp + 24);
                crate::fecars::abi_get_stock_car(c)
            });
            (r.0[4], r.0[3]) = (sp + 256, sp + 260);
            let m = &mut c.m;
            let id = m.r8(sp + 24) as i8 as i32 as u32;
            let colour = m.r8(sp + 28) as u32;
            m.w32(sp + 256, id);
            let v = m.r8(sp + 199 + colour);
            m.w8(sp + 260, v);
            let added = port(c, &r, 0x8001_7740, 0x8002_8aa8, &[CAR_MANAGER, sp + 256, sp + 260], crate::fecars::abi_is_car_an_added_model);
            if added != 1 {
                let memory = c.m.r16(stream + 2);
                if (memory as i16) < 16 {
                    c.m.w16(stream + 2, memory.wrapping_add(3));
                } else {
                    r.0[0] = sp + 232;
                    let class = c.m.r8(sp + 26) as u32;
                    port(c, &r, 0x8001_75ac, 0x8002_8ae8, &[CAR_MANAGER, class, 3, sp + 232], crate::fecars::abi_get_class_list);
                    c.m.w32(sp + 16, sp + 232);
                    port(c, &r, 0x8001_7860, 0x8002_8b00, &[CAR_MANAGER, sp + 256, sp + 260, 3], crate::fecars::abi_find_similar_car);
                }
            }
            let m = &mut c.m;
            let n = m.r16(stream + 416);
            let first = m.r8(stream + 204);
            m.w32(stream + 440, 0);
            m.w32(stream + 444, 0);
            m.w16(stream + 416, n.wrapping_add(1));
            let (id, colour) = (m.r32(sp + 256), m.r8(sp + 260));
            let place = m.r8(stream + 416).wrapping_add(1);
            m.w8(stream + 448, 1);
            m.w8(stream + 457, first);
            m.w32(stream + 452, id);
            m.w8(stream + 456, colour);
            m.w8(stream + 428, place);
            port(c, &r, 0x8001_77c0, 0x8002_8b54, &[CAR_MANAGER, sp + 256, sp + 260], crate::fecars::abi_add_car_to_ingame_list);
            let size = c.m.r16(stream).wrapping_add(2);
            c.m.w16(stream, size);
        } else {
            m.w8(stream + 428, 1);
            m.w8(stream + 448, 2);
        }
    })
}

/// The missions' manager; the cop cars (by the player cars' highest kind, 20 bytes each, then
/// by the stream's class, +680): the others' (+542), the pursuers' (+540).
const MISSION_MANAGER: u32 = 0x8013_E594;
const OTHER_COP_CARS: u32 = 0x8005_1D04;
const PURSUER_CARS: u32 = 0x8005_1C78;

/// `Front_InitMissions` (FRONT.CPP, 0x80028ba8): a single race whose player cars are all cops
/// (kind 7): the missions' description (the second with traffic or two players), the mission
/// (+600) and its stages (+604) for the class of the best car.
pub fn abi_init_missions(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = stream;
        c.m.w32(stream + 600, 0);
        c.m.w32(stream + 604, 0);
        if c.m.r8(FE + 4) != 1 {
            return;
        }
        let mut best = 0u32;
        let cars = player_cars(c, stream);
        if (cars as i32) > 0 {
            let mut k = 0u32;
            loop {
                let rec = record(stream, k as u16 as i16 as i32 as u32);
                if c.m.r8(rec + 2) != 7 {
                    return;
                }
                let car = c.m.r8(rec) as i8 as i32;
                if (best as i32) < car {
                    best = car as u32;
                }
                k = k.wrapping_add(1);
                if (k as u16 as i16 as i32) >= cars as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        let (tier, mission) = (m.r8(FE + 299), m.r8(FE + 300));
        (r.0[2], r.0[3]) = (tier as u32, mission as u32);
        m.w8(FE + 299, best.wrapping_sub(22) as u8);
        m.w8(FE + 300, 0);
        let second = ((m.r16(stream + 580) as i16) > 0 || m.r8(FE + 3) == 1) as u32;
        r.0[0] = 0x8014_0000;
        port(c, &r, 0x8003_41b4, 0x8002_8cac, &[MISSION_MANAGER, second, FE, best], crate::femission::abi_load_description);
        r.0[0] = MISSION_MANAGER;
        port(c, &r, 0x8003_438c, 0x8002_8cbc, &[MISSION_MANAGER, stream + 600], crate::femission::abi_get_mission_to_race);
        r.0[0] = FE;
        let (a, b) = (c.m.r8(FE + 299) as u32, c.m.r8(FE + 300) as u32);
        port(c, &r, 0x8003_4324, 0x8002_8cd8, &[MISSION_MANAGER, a, b, stream + 604], crate::femission::abi_get_mission_stages);
        c.m.w8(FE + 299, tier);
        c.m.w8(FE + 300, mission);
    })
}

/// `Front_InitCopCars` (FRONT.CPP, 0x80028cfc): the cops (+540 the pursuers, +542 the others,
/// +544 their cars, +568 their colours): a mission's chaser (the best player car, at the first
/// stage that has one) unless with two players; else in a single race without cop players, four
/// pursuers alone, two and one with one player and no opponents, two others with two players,
/// their cars by the player cars' highest kind, each listed for the race unless an added model.
pub fn abi_init_cop_cars(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3]) = (stream, 0);
        let m = &mut c.m;
        m.w32(sp + 16, 22);
        m.w8(sp + 20, 0);
        m.w16(stream + 540, 0);
        m.w16(stream + 542, 0);
        let (mut best, mut kind) = (0u32, 0u32);
        let cars = player_cars(c, stream);
        if (cars as i32) > 0 {
            let mut k = 0u32;
            loop {
                let rec = stream.wrapping_add((k as u16 as i16 as i32 as u32).wrapping_mul(CAR_RECORD));
                let car = c.m.r8(rec + 8) as i8 as i32;
                if (best as i32) < car {
                    best = car as u32;
                }
                let v = c.m.r8(rec + 10) as u32;
                if (kind as i32) < v as i32 {
                    kind = v;
                }
                k = k.wrapping_add(1);
                r.0[3] = k;
                if (k as u16 as i16 as i32) >= cars as i32 {
                    break;
                }
            }
        }
        let mission = c.m.r32(stream + 600);
        if mission != 0 && c.m.r32(stream + 604) != 0 {
            if c.m.r8(FE + 3) == 1 || c.m.r8(mission + 10) == 0 {
                return;
            }
            let mut k = 0u32;
            r.0[3] = 0;
            loop {
                let k16 = k as u16 as i16 as i32 as u32;
                let stage = c.m.r32(stream + 604).wrapping_add(k16.wrapping_mul(44));
                if (c.m.r16(stage + 6) as i16) >= 0 || (c.m.r16(stage + 10) as i16) >= 0 {
                    let m = &mut c.m;
                    let n = m.r16(stream + 540) as i16 as i32 as u32;
                    m.w32(stream.wrapping_add(n << 2) + 544, best);
                    let n = m.r16(stream + 540) as i16 as i32 as u32;
                    let colour = m.r8(stream + 207);
                    m.w16(stream.wrapping_add(n << 1) + 568, colour as u16);
                    let (n, size) = (m.r16(stream + 540).wrapping_add(1), m.r16(stream).wrapping_add(2));
                    m.w16(stream + 540, n);
                    m.w16(stream, size);
                    return;
                }
                k = k.wrapping_add(1);
                r.0[3] = k;
                let t3 = c.m.r32(stream + 600);
                let stages = c.m.r8(t3 + 10) as i32;
                if (k as u16 as i16 as i32) >= stages {
                    return;
                }
            }
        }
        if c.m.r8(FE + 4) != 1 {
            return;
        }
        let cars = player_cars(c, stream);
        if (cars as i32) > 0 {
            let mut k = 0u32;
            r.0[3] = 0;
            loop {
                let rec = stream.wrapping_add((k as u16 as i16 as i32 as u32).wrapping_mul(CAR_RECORD));
                if c.m.r8(rec + 10) == 7 {
                    return;
                }
                k = k.wrapping_add(1);
                r.0[3] = k;
                if (k as u16 as i16 as i32) >= cars as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        let cars = m.r16(stream + 6);
        let (pursuers, others) = if cars == 1 && m.r16(stream + 416) == 0 {
            (2, 1)
        } else if cars == 2 {
            (0, 2)
        } else {
            (4, 0)
        };
        m.w16(stream + 540, pursuers);
        m.w16(stream + 542, others);
        let total = (m.r16(stream + 540) as i16 as i32) + (m.r16(stream + 542) as i16 as i32);
        r.0[3] = 0;
        if total <= 0 {
            return;
        }
        let column = kind.wrapping_mul(20);
        (r.0[7], r.0[4], r.0[6], r.0[5]) = (OTHER_COP_CARS, column, PURSUER_CARS, CAR_MANAGER);
        let mut k = 0u32;
        loop {
            let m = &mut c.m;
            let others = m.r16(stream + 542) as u32;
            let table = if ((k << 16) as i32) < ((others << 16) as i32) { OTHER_COP_CARS } else { PURSUER_CARS };
            let class = m.r8(stream + 680) as u32;
            let car = m.r32((class << 2).wrapping_add(column).wrapping_add(table));
            m.w32(sp + 16, car);
            (r.0[0], r.0[1]) = (sp + 16, sp + 20);
            let added = port(c, &r, 0x8001_7740, 0x8002_900c, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_is_car_an_added_model);
            if added != 1 {
                let memory = c.m.r16(stream + 2).wrapping_add(3);
                c.m.w16(stream + 2, memory);
                port(c, &r, 0x8001_77c0, 0x8002_9030, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_add_car_to_ingame_list);
            }
            let m = &mut c.m;
            let k16 = k as u16 as i16 as i32 as u32;
            let car = m.r32(sp + 16);
            m.w32(stream.wrapping_add(k16 << 2) + 544, car);
            k = k.wrapping_add(1);
            r.0[3] = k;
            let size = m.r16(stream).wrapping_add(2);
            m.w16(stream, size);
            let class = m.r8(stream + 680);
            m.w16(stream.wrapping_add(k16 << 1) + 568, class as u16);
            let total = (m.r16(stream + 540) as i16 as i32) + (m.r16(stream + 542) as i16 as i32);
            if (k as u16 as i16 as i32) >= total {
                break;
            }
        }
    })
}

/// `rand`, `blockmove`.
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;

/// `Front_InitPerps` (FRONT.CPP, 0x800290bc): a mission's perps (+594 their count, +596 the
/// stages done, +608 the perps: 8 bytes, the car and its colour): each stage's car (+0) in its
/// colour (+1, the index of the car's colour equal to it, 16 when none is), listed for the race
/// unless an added model.
pub fn abi_init_perps(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = stream;
        let mission = c.m.r32(stream + 600);
        c.m.w16(stream + 594, 0);
        c.m.w16(stream + 596, 0);
        if mission == 0 {
            return;
        }
        (r.0[1], r.0[5], r.0[4], r.0[3], r.0[2]) = (0, 0x8011_0000, CAR_MANAGER, sp + 16, sp + 20);
        let mut k = 0u32;
        loop {
            let k16 = k as u16 as i16 as i32;
            let t4 = c.m.r32(stream + 600);
            let stages = c.m.r8(t4 + 10) as i32;
            if k16 >= stages {
                return;
            }
            let at = (k16 as u32).wrapping_mul(44);
            let m = &mut c.m;
            let t5 = m.r32(stream + 604);
            let id = m.r8(at.wrapping_add(t5)) as u32;
            m.w32(sp + 16, id);
            let t6 = m.r32(stream + 604);
            let colour = m.r8(at.wrapping_add(t6) + 1);
            m.w8(sp + 20, colour);
            let car = port(c, &r, 0x8001_6374, 0x8002_915c, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
            let m = &mut c.m;
            let want = m.r8(sp + 20) as i32;
            let mut j = 0u32;
            while (j as i32) < 16 {
                if m.r8(car.wrapping_add(j) + 175) as i8 as i32 == want {
                    break;
                }
                j += 1;
            }
            m.w8(sp + 20, j as u8);
            let added = port(c, &r, 0x8001_7740, 0x8002_91b0, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_is_car_an_added_model);
            if added != 1 {
                let memory = c.m.r16(stream + 2);
                if (memory as i16) < 16 {
                    c.m.w16(stream + 2, memory.wrapping_add(6));
                }
                port(c, &r, 0x8001_77c0, 0x8002_91e4, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_add_car_to_ingame_list);
                let m = &mut c.m;
                let size = m.r16(stream).wrapping_add(2);
                let n = m.r16(stream + 594) as i16 as i32 as u32;
                m.w16(stream, size);
                let id = m.r32(sp + 16);
                m.w32(stream.wrapping_add(n << 3) + 608, id);
                let n = m.r16(stream + 594) as i16 as i32 as u32;
                let t7 = m.r32(stream + 604);
                let colour = m.r8(at.wrapping_add(t7) + 1);
                m.w8(stream.wrapping_add(n << 3) + 612, colour);
                let n = m.r16(stream + 594).wrapping_add(1);
                m.w16(stream + 594, n);
            }
            let done = c.m.r16(stream + 596).wrapping_add(1);
            k = k.wrapping_add(1);
            r.0[1] = k;
            c.m.w16(stream + 596, done);
        }
    })
}

/// `Front_InitTrack` (FRONT.CPP, 0x80029284): the track (+656 its information, +704 its number
/// and settings): a tournament's to race; else the race slot's, its settings (direction,
/// mirrored, night, weather) for a single race, a pink slips' race or with the options on, else
/// none; the time of day full; the settings left unset (2 and up) chosen at random.
pub fn abi_init_track(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (FE, stream);
        if c.m.r8(FE + 4) == 2 {
            port(c, &r, 0x8003_27dc, 0x8002_92bc, &[TOURNAMENT_MANAGER, stream + 704], crate::fetourn::abi_get_track_to_race);
            let track = c.m.r8(stream + 704) as i8 as i32 as u32;
            let info = port(c, &r, 0x8002_1e60, 0x8002_92cc, &[TRACK_MANAGER, track], crate::fetracks::abi_get_track_by_id);
            r.call_leaving(c, BLOCKMOVE, &[info, stream + 656, 48], 0x8002_92dc);
        } else {
            let slot = c.m.r8(FE + 70) as u32;
            let track = c.m.r8(slot + FE + 35) as u32;
            port(c, &r, 0x8002_1c04, 0x8002_9300, &[TRACK_MANAGER, track, stream + 656], crate::fetracks::abi_get_track);
            let m = &mut c.m;
            let number = m.r8(stream + 656);
            m.w8(stream + 704, number);
            let settings = stream + 704;
            let kind = m.r8(FE + 4);
            if m.r8(FE + 298) == 1 || kind == 1 || kind == 6 {
                for (k, at) in [50, 55, 22, 27].into_iter().enumerate() {
                    let slot = m.r8(FE + 70) as u32;
                    let v = m.r8(slot + FE + at);
                    m.w8(settings + 1 + k as u32, v);
                }
            } else {
                for k in [4, 3, 2, 1] {
                    m.w8(settings + k, 0);
                }
            }
            m.w32(stream + 736, 0x1_0000);
        }
        for (k, ra) in [(705, 0x8002_93d0), (706, 0x8002_93f4), (707, 0x8002_9418), (708, 0x8002_943c)] {
            if c.m.r8(stream + k) >= 2 {
                let v = r.call_leaving(c, RAND, &[], ra);
                c.m.w8(stream + k, (v & 1) as u8);
            }
        }
    })
}

/// `Front_InitTraffic` (FRONT.CPP, 0x80029458): traffic for the race: on with the slot's
/// traffic option (a pink slips' race), or in a race of the front end's kinds 0 and 1 by the
/// options and their switch (+60, a single race's own), unless the track's class is past 2 or
/// it is mirrored, two players race a single race, or one races a same-class race; then
/// six cars (three with two players; one or two in a single race by the track's setting) from
/// the opponents' (+684, in turn), listed for the race unless added models.
pub fn abi_init_traffic(c: &mut Cpu) {
    let stream = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = stream;
        let m = &mut c.m;
        m.w8(sp + 20, 0);
        let mut count = if m.r8(FE + 3) == 1 { 3 } else { 6 };
        let kind = m.r8(FE + 4) as u32;
        if kind == 1 {
            count = if m.r8(stream + 707) == 1 { 1 } else { 2 };
        }
        let slot = m.r8(FE + 70) as u32;
        let mut on = m.r8(slot + FE + 60) != 0;
        let mut checks = true;
        if kind >= 2 {
            checks = false;
            if kind != 6 {
                on = false;
            }
        } else if m.r8(FE + 298) == 0 && kind == 0 {
            on = true;
        } else if kind == 1 {
            on = m.r8(FE + 60) != 0;
        }
        if checks {
            if m.r8(stream + 658) >= 3 || m.r8(stream + 660) != 0 {
                on = false;
            }
            if m.r8(FE + 3) == 1 {
                if m.r8(FE + 4) == 1 {
                    on = false;
                }
            } else if m.r8(FE + 798) == 2 {
                on = false;
            }
        }
        r.0[0] = 0;
        if !on {
            return;
        }
        m.w16(stream + 580, 0);
        if count == 0 {
            return;
        }
        (r.0[4], r.0[2], r.0[1], r.0[5]) = (CAR_MANAGER, sp + 16, sp + 20, count);
        loop {
            let k = r.0[0];
            let next = k.wrapping_add(1);
            let car = c.m.r8(stream.wrapping_add(k as u16 as i16 as i32 as u32) + 684) as u32;
            r.0[0] = if (next as u16 as i16) < 6 { next } else { 0 };
            c.m.w32(sp + 16, car);
            let added = port(c, &r, 0x8001_7740, 0x8002_961c, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_is_car_an_added_model);
            if added != 1 {
                port(c, &r, 0x8001_77c0, 0x8002_9634, &[CAR_MANAGER, sp + 16, sp + 20], crate::fecars::abi_add_car_to_ingame_list);
            }
            let m = &mut c.m;
            let n = m.r16(stream + 580) as i16 as i32 as u32;
            let car = m.r16(sp + 16);
            m.w16(stream.wrapping_add(n << 1) + 582, car);
            let n = m.r16(stream + 580).wrapping_add(1);
            m.w16(stream + 580, n);
            if (n as i16 as i32) >= count as i32 {
                break;
            }
        }
    })
}


/// The stream's words as the `Front_Append*` write them: at a cursor, one after another.
struct Out(u32);

impl Out {
    fn put(&mut self, m: &mut ps1rt::Mem, v: u32) {
        m.w32(self.0, v);
        self.0 = self.0.wrapping_add(4);
    }

    /// A setting: its tag, the car's index (+4 of the stream, a `short`) and its value.
    fn set(&mut self, m: &mut ps1rt::Mem, stream: u32, tag: u32, v: u32) {
        self.put(m, tag);
        let i = m.r16(stream + 4) as i16 as i32 as u32;
        self.put(m, i);
        self.put(m, v);
    }
}

/// `Front_AppendPlayerCarData` (FRONT.CPP, 0x80029690): the player cars' settings into the
/// race's stream at `out` (tag, car, value): the place, model, player's setting, colour, the
/// lights' settings, the extras (from the list's bits), the colour's RGB, the time of day's
/// tint, the shading, the cop lights, the kind (65 a cop), the gearbox, the three driving
/// aids, then the display's settings; the cars' counts last. The cursor after.
pub fn abi_append_player_car_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (stream, 0);
        let mut o = Out(out);
        if (player_cars(c, stream) as i32) > 0 {
            r.0[2] = FE;
            let mut k = 0u32;
            loop {
                let k16 = k as u16 as i16 as i32 as u32;
                let entry = list_entry(stream, k16);
                let rec = record(stream, k16);
                let fe = k16.wrapping_add(FE);
                let m = &mut c.m;
                let v = m.r8(entry + 8) as i8 as i32 as u32;
                o.set(m, stream, 281, v);
                let v = m.r8(rec + 1) as u32;
                o.set(m, stream, 260, v);
                let v = m.r8(fe + 94) as u32;
                o.set(m, stream, 262, v);
                let v = m.r8(rec + 197) as u32;
                o.set(m, stream, 266, v);
                o.set(m, stream, 267, 0);
                let cop = m.r8(rec) as i8 == 28;
                let v = (m.r8(fe + 788) != 0 && m.r8(rec + 3) != 0) || cop;
                o.set(m, stream, 273, v as u32);
                let v = (m.r8(fe + 788) != 0 && m.r8(rec + 194) != 0) || cop;
                o.set(m, stream, 277, v as u32);
                let v = (m.r8(entry + 17) as u32 >> 2) & 1;
                o.set(m, stream, 272, v);
                let v = (m.r8(entry + 17) as u32 >> 1) & 1;
                o.set(m, stream, 274, v);
                let v = m.r8(entry + 17) as u32 & 1;
                o.set(m, stream, 269, v);
                o.set(m, stream, 268, 0);
                let colour = rec.wrapping_add((m.r8(rec + 197) as u32) << 2);
                let (w, b) = (m.r32(colour + 68), m.r8(colour + 70) as u32);
                o.set(m, stream, 290, b | (w & 0xFF00) | ((w & 0xFF) << 16));
                let t8 = m.r8(stream + 707);
                let v = m.r32(rec.wrapping_add((t8 as u32) << 2) + 148);
                o.set(m, stream, 291, v);
                let t9 = m.r8(rec + 197);
                let v = m.r8(rec.wrapping_add(t9 as u32) + 132) as u32;
                o.set(m, stream, 292, v);
                let v = m.r8(rec + 199) as u32;
                o.set(m, stream, 293, v);
                let v = if m.r8(rec + 2) == 7 { 65 } else { 1 };
                o.set(m, stream, 261, v);
                let v = if m.r8(rec + 196) & 2 != 0 { 2 } else { m.r8(rec + 6) as u32 };
                o.set(m, stream, 275, v);
                for (tag, at) in [(263, 791), (264, 793), (265, 795)] {
                    let v = m.r8(fe + at) as u32;
                    o.set(m, stream, tag, v);
                }
                let i = m.r16(stream + 4) as i16 as i32 as u32;
                o.0 = port(c, &r, 0x8002_7dec, 0x8002_9ae8, &[o.0, i, k16, stream + 656], abi_output_display_settings);
                k = k.wrapping_add(1);
                r.0[1] = k;
                let m = &mut c.m;
                let i = m.r16(stream + 4).wrapping_add(1);
                m.w16(stream + 4, i);
                if (k as u16 as i16 as i32) >= player_cars(c, stream) as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        o.put(m, 29);
        let v = m.r16(stream + 6) as i16 as i32 as u32;
        o.put(m, v);
        o.put(m, 30);
        let v = m.r16(stream + 416) as i16 as i32 as u32;
        o.put(m, v);
        o.put(m, 14);
        o.put(m, 0);
        c.r[2] = o.0;
    })
}

/// A car's colour `colour` as RGB (its +68 table of 4 bytes).
fn car_rgb(m: &mut ps1rt::Mem, car: u32, colour: u32) -> u32 {
    let at = car.wrapping_add(colour << 2);
    let (w, b) = (m.r32(at + 68), m.r8(at + 70) as u32);
    b | (w & 0xFF00) | ((w & 0xFF) << 16)
}

/// `Front_AppendOpponentData` (FRONT.CPP, 0x80029b74): the opponents' settings into the stream
/// (each after the player cars in the list: its place, model, the AI's two settings, the
/// opponent's, its colour, the extras, the colour's RGB, the time of day's tint, the shading).
/// The cursor after.
pub fn abi_append_opponent_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[2], r.0[3]) = (out, stream, 0);
        let mut o = Out(out);
        if (c.m.r16(stream + 416) as i16) > 0 {
            let mut k = 0u32;
            loop {
                let n = (k as u16 as i16 as i32 as u32).wrapping_add(player_cars(c, stream));
                let entry = list_entry(stream, n);
                r.0[0] = entry;
                r.0[1] = o.0;
                let id = c.m.r16(entry + 12) as i16 as i32 as u32;
                let car = port(c, &r, 0x8001_6374, 0x8002_9bd8, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
                let m = &mut c.m;
                let v = m.r8(entry + 8) as i8 as i32 as u32;
                o.set(m, stream, 281, v);
                let v = m.r8(car + 1) as u32;
                o.set(m, stream, 260, v);
                o.set(m, stream, 262, 1);
                o.set(m, stream, 261, 2);
                let v = m.r32(entry + 4);
                o.set(m, stream, 276, v);
                let v = m.r32(entry + 4);
                o.set(m, stream, 280, v);
                let v = m.r8(entry + 16) as u32;
                o.set(m, stream, 266, v);
                o.set(m, stream, 267, 0);
                let v = (m.r8(entry + 17) as u32 >> 2) & 1;
                o.set(m, stream, 272, v);
                let v = (m.r8(entry + 17) as u32 >> 1) & 1;
                o.set(m, stream, 274, v);
                let v = m.r8(entry + 17) as u32 & 1;
                o.set(m, stream, 269, v);
                o.set(m, stream, 268, 0);
                o.set(m, stream, 293, 0);
                let colour = m.r8(entry + 16) as u32;
                let v = car_rgb(m, car, colour);
                o.set(m, stream, 290, v);
                let t10 = m.r8(stream + 707);
                let v = m.r32(car.wrapping_add((t10 as u32) << 2) + 148);
                o.set(m, stream, 291, v);
                k = k.wrapping_add(1);
                r.0[3] = k;
                let t11 = m.r8(entry + 16);
                let v = m.r8(car.wrapping_add(t11 as u32) + 132) as u32;
                o.set(m, stream, 292, v);
                let i = m.r16(stream + 4).wrapping_add(1);
                m.w16(stream + 4, i);
                if (k as u16 as i16 as i32) >= m.r16(stream + 416) as i16 as i32 {
                    break;
                }
            }
        }
        c.r[2] = o.0;
    })
}

/// `Front_AppendCopData` (FRONT.CPP, 0x80029ea8): the cops into the stream (a cops' marker
/// first when there are any): each its model, the AI's kind (16 the others, 8 the pursuers),
/// the difficulty (the race's +6 and 5), its colour. The cursor after.
pub fn abi_append_cop_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = stream;
        let mut o = Out(out);
        let cops = |m: &mut ps1rt::Mem| (m.r16(stream + 540) as i16 as i32) + (m.r16(stream + 542) as i16 as i32);
        if cops(&mut c.m) > 0 {
            o.put(&mut c.m, 12);
            o.put(&mut c.m, 1);
        }
        let mut k = 0u32;
        r.0[3] = 0;
        loop {
            let k16 = k as u16 as i16 as i32;
            r.0[2] = k16 as u32;
            if k16 >= cops(&mut c.m) {
                break;
            }
            r.0[0] = o.0;
            let id = c.m.r16(stream.wrapping_add((k16 as u32) << 2) + 544) as i16 as i32 as u32;
            let car = port(c, &r, 0x8001_6374, 0x8002_9f2c, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
            let m = &mut c.m;
            let v = m.r8(car + 1) as u32;
            o.set(m, stream, 260, v);
            o.set(m, stream, 262, 1);
            let ai = if k16 < m.r16(stream + 542) as i16 as i32 { 16 } else { 8 };
            o.set(m, stream, 261, ai);
            k = k.wrapping_add(1);
            r.0[3] = k;
            let v = (m.r8(FE + 6) as u32).wrapping_add(5);
            o.set(m, stream, 280, v);
            for tag in [266, 267, 268] {
                o.set(m, stream, tag, 0);
            }
            let v = m.r16(stream.wrapping_add((k16 as u32) << 1) + 568) as i16 as i32 as u32;
            o.set(m, stream, 293, v);
            for tag in [272, 274, 269] {
                o.set(m, stream, tag, 0);
            }
            let i = m.r16(stream + 4).wrapping_add(1);
            m.w16(stream + 4, i);
        }
        c.r[2] = o.0;
    })
}

/// `Front_AppendPerpData` (FRONT.CPP, 0x8002a0fc): the mission (its stages and its kind, or
/// none) and the perps into the stream: each its model, the AI's kind, its colour; the stages
/// done last. The cursor after.
pub fn abi_append_perp_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = stream;
        let mut o = Out(out);
        let m = &mut c.m;
        let mission = m.r32(stream + 600);
        o.put(m, 37);
        let t12 = m.r32(stream + 600);
        let v = if mission != 0 { m.r8(t12 + 10) as u32 } else { 0 };
        o.put(m, v);
        o.put(m, 38);
        let t13 = m.r32(stream + 600);
        let v = if mission != 0 { m.r16(t13 + 8) as u32 } else { 0 };
        o.put(m, v);
        r.0[3] = 0;
        if (m.r16(stream + 594) as i16) > 0 {
            r.0[4] = 1;
            let mut k = 0u32;
            loop {
                let perp = stream.wrapping_add(((k << 16) as i32 >> 13) as u32);
                (r.0[0], r.0[1]) = (perp, o.0);
                let id = c.m.r16(perp + 608) as i16 as i32 as u32;
                let car = port(c, &r, 0x8001_6374, 0x8002_a1c4, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
                k = k.wrapping_add(1);
                r.0[3] = k;
                let m = &mut c.m;
                let v = m.r8(car + 1) as u32;
                o.set(m, stream, 260, v);
                o.set(m, stream, 262, 1);
                let v = m.r8(perp + 612) as u32;
                o.set(m, stream, 266, v);
                o.set(m, stream, 267, 0);
                o.set(m, stream, 261, 2);
                o.set(m, stream, 280, 0);
                o.set(m, stream, 268, 1);
                for tag in [293, 272, 274, 269] {
                    o.set(m, stream, tag, 0);
                }
                let i = m.r16(stream + 4).wrapping_add(1);
                m.w16(stream + 4, i);
                if (k as u16 as i16 as i32) >= m.r16(stream + 594) as i16 as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        o.put(m, 37);
        let v = m.r16(stream + 596) as i16 as i32 as u32;
        o.put(m, v);
        c.r[2] = o.0;
    })
}

/// The tracks' default for the setting `+800` (by the track's +42).
const TRACK_DEFAULTS: u32 = 0x8005_1AF8;

/// `Front_AppendTrafficData` (FRONT.CPP, 0x8002a394): the traffic cars into the stream: each
/// its model, the AI's kind (4) and setting (8); then the traffic's density (a third of the
/// cars, at least one). The cursor after.
pub fn abi_append_traffic_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = stream;
        let mut o = Out(out);
        r.0[2] = 0;
        if (c.m.r16(stream + 580) as i16) > 0 {
            r.0[3] = 1;
            let mut k = 0u32;
            loop {
                r.0[0] = o.0;
                let id = c.m.r16(stream.wrapping_add(((k << 16) as i32 >> 15) as u32) + 582) as i16 as i32 as u32;
                let car = port(c, &r, 0x8001_6374, 0x8002_a3e4, &[CAR_MANAGER, id], crate::fecars::abi_get_car_from_id);
                k = k.wrapping_add(1);
                r.0[2] = k;
                let m = &mut c.m;
                let v = m.r8(car + 1) as u32;
                o.set(m, stream, 260, v);
                for (tag, v) in [(262, 1), (266, 0), (267, 0), (261, 4), (280, 8), (268, 1), (293, 0), (272, 0), (274, 0), (269, 0)] {
                    o.set(m, stream, tag, v);
                }
                let i = m.r16(stream + 4).wrapping_add(1);
                m.w16(stream + 4, i);
                if (k as u16 as i16 as i32) >= m.r16(stream + 580) as i16 as i32 {
                    break;
                }
            }
        }
        let m = &mut c.m;
        let cars = m.r16(stream + 580) as i16 as i32;
        let third = cars / 3;
        let density = if cars <= 0 || third > 0 { third } else { 1 };
        o.put(m, 13);
        o.put(m, density as u32);
        c.r[2] = o.0;
    })
}

/// `Front_AppendTrackData` (FRONT.CPP, 0x8002a5e4): the track's settings into the stream: the
/// setting +800 (0 for 1, 1 for 2, else the track's default), the direction, the mirroring,
/// the weather, the night, the track's +1, the time of day. The cursor after.
pub fn abi_append_track_data(c: &mut Cpu) {
    let (out, stream) = (c.r[4], c.r[5]);
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[2], r.0[0]) = (out, stream, FE);
        let slot = c.m.r8(FE + 70) as u32;
        let track = c.m.r8(slot + FE + 35) as u32;
        port(c, &r, 0x8002_1c04, 0x8002_a624, &[TRACK_MANAGER, track, sp + 16], crate::fetracks::abi_get_track);
        let m = &mut c.m;
        let v = match m.r8(FE + 800) {
            1 => 0,
            2 => 1,
            _ => {
                let t14 = m.r8(sp + 58);
                m.r16(TRACK_DEFAULTS + ((t14 as u32) << 1)) as i16 as i32 as u32
            }
        };
        let mut o = Out(out);
        o.put(m, 26);
        o.put(m, v);
        for (tag, at) in [(24, 706), (25, 705), (20, 708), (23, 707), (18, 657)] {
            o.put(m, tag);
            let v = m.r8(stream + at) as u32;
            o.put(m, v);
        }
        o.put(m, 11);
        let v = m.r32(stream + 736);
        o.put(m, v);
        c.r[2] = o.0;
    })
}

/// `OutputDisplaySettings` (FRONT.CPP, 0x80027dec): player `p`'s display settings into the
/// stream at `out` for car `i`: the view (+800: 1 the first, 2 the second, 3 and up the third,
/// 0 the track's default), whether it isn't the fourth, the six dashboard settings (+804..+814)
/// and the four per player (+816..+819). The cursor after.
pub fn abi_output_display_settings(c: &mut Cpu) {
    let (out, i, p, track) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    let fe = p.wrapping_add(FE);
    let view = match m.r8(fe + 800) {
        1 => 0,
        0 => {
            let k = m.r8(track + 42) as u32;
            m.r16(TRACK_DEFAULTS + (k << 1)) as i16 as i32 as u32
        }
        2 => 1,
        _ => 2,
    };
    let mut o = Out(out);
    for (tag, v) in [(282, view), (283, (m.r8(fe + 800) != 3) as u32)] {
        o.put(m, tag);
        o.put(m, i);
        o.put(m, v);
    }
    for (tag, at) in [(284, 804), (285, 806), (286, 808), (287, 812), (288, 810), (289, 814)] {
        let v = m.r8(fe + at) as u32;
        o.put(m, tag);
        o.put(m, i);
        o.put(m, v);
    }
    let fe4 = (p << 2).wrapping_add(FE);
    for (tag, at) in [(294, 816), (295, 817), (296, 818), (297, 819)] {
        let v = m.r8(fe4 + at) as u32;
        o.put(m, tag);
        o.put(m, i);
        o.put(m, v);
    }
    c.r[2] = o.0;
}

/// `operator new`; the menus' constructor (FEMENUDEFS).
const NEW: fn(&mut Cpu) = nfs4rc::f_800a48c0;
const ALL_MENUS_CTOR: fn(&mut Cpu) = nfs4rc::f_8002e504;
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
pub fn abi_construct_all(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let at = r.call_leaving(c, NEW, &[15320], 0x8002_6e88);
        let screens = port(c, &r, 0x8002_bb74, 0x8002_6e90, &[at], abi_all_screens);
        c.m.w32(ALL_SCREENS, screens);
        for (global, offset) in SCREEN_POINTERS {
            c.m.w32(global, screens.wrapping_add(offset));
        }
        let at = r.call_leaving(c, NEW, &[896], 0x8002_6fd4);
        let app = port(c, &r, 0x8001_30f8, 0x8002_6fdc, &[at], crate::feapp::abi_fe_application);
        let at = r.call_leaving(c, NEW, &[15128], 0x8002_6fec);
        c.m.w32(FE_APP, app);
        let menus = r.call_leaving(c, ALL_MENUS_CTOR, &[at], 0x8002_6ff4);
        c.m.w32(ALL_MENUS, menus);
    })
}

/// The memory left (the largest free block, at the front end's start and end); the front end
/// in use; a post-race flag; the setup's quit flag (+0x24); the race's finishers (a word).
const MEMORY_LEFT: u32 = 0x8005_19A0;
const USE: u32 = 0x8013_E54C;
const POST_RACE: u32 = 0x8013_E569;
const FINISHERS: u32 = 0x8011_0E8C;
const LARGEST_UNUSED: fn(&mut Cpu) = nfs4rc::f_800e5e70;
const TO_MAIN_MENU: fn(&mut Cpu) = nfs4rc::f_8002c514;
const RUN_POST_GAME: fn(&mut Cpu) = nfs4rc::f_80015554;
/// The cars' sound banks freed (the car code's), the words unloaded.
const CAR_SOUNDS_END: fn(&mut Cpu) = nfs4rc::f_800bcb04;
const UNLOAD_WORDS: fn(&mut Cpu) = nfs4rc::f_800b9f98;
/// `FILE_delbigsync` (the host's).
const FILE_DEL_BIG_SYNC: u32 = 0x800E_B3CC;

/// `Front_Menu` (FRONT.CPP, 0x80027b38): the front end run: the memory left noted, the
/// commentary opened, the showroom, everything constructed; a player with no car nor money
/// given the cheapest car's price; then the main menu (`mode` 0 or 1, from the start or a
/// test) or, after a race (2), the tournament advanced or the pink slips' winner counted, and
/// the post-race screens; everything deleted and shut, the big file closed, the record lap time
/// of the first track. The application's result.
pub fn abi_front_menu(c: &mut Cpu) {
    let mode = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[3], r.0[4]) = (mode, 0, 1);
        c.m.w32(CARD_FIRST, 1);
        let left = r.call_leaving(c, LARGEST_UNUSED, &[], 0x8002_7b70);
        (r.0[0], r.0[5]) = (0, FE);
        let language = c.m.r8(FE + 832) as u32;
        c.m.w32(MEMORY_LEFT, left);
        port(c, &r, 0x8001_6200, 0x8002_7b90, &[language, 0], crate::feaudio::abi_init_commentary);
        r.enter_leaving(c, 0x8004_dd38, 0x8002_7b98, crate::psxfront::abi_initialize_spinning_cars);
        r.enter_leaving(c, 0x8002_6e78, 0x8002_7ba0, abi_construct_all);
        r.0[2] = CAR_MANAGER;
        let owned = port(c, &r, 0x8001_747c, 0x8002_7bb4, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
        let broke = if ((owned << 16) as i32) > 0 { 0 } else { ((c.m.r32(TOURNAMENT_MANAGER + 20) as i32) < 1) as u32 };
        r.0[0] = broke;
        if broke != 0 {
            let price = port(c, &r, 0x8001_6434, 0x8002_7bdc, &[CAR_MANAGER], crate::fecars::abi_cheapest_car_stock_price);
            let money = c.m.r32(TOURNAMENT_MANAGER + 20).wrapping_add(price.wrapping_add(1));
            c.m.w32(TOURNAMENT_MANAGER + 20, money);
        }
        if mode == 0 || mode == 1 {
            if mode == 0 {
                r.enter_leaving(c, 0x8002_7960, 0x8002_7c2c, abi_load_config);
            }
            if c.m.r32(USE) != 0 {
                r.call_leaving(c, TO_MAIN_MENU, &[sp + 16], 0x8002_7c48);
                let app = c.m.r32(FE_APP);
                let v = port(c, &r, 0x8001_585c, 0x8002_7c58, &[app], crate::feapp::abi_run_front_end);
                r.0[3] = v;
            }
        } else if mode == 2 {
            c.m.w8(POST_RACE, 1);
            let m = &mut c.m;
            if m.r8(FE + 4) == 2 && m.r32(GAME_SETUP.0 + 0x24) == 0 {
                r.0[0] = TOURNAMENT_MANAGER;
                port(c, &r, 0x8003_3184, 0x8002_7c94, &[TOURNAMENT_MANAGER], crate::fetourn::abi_advance_to_next_track);
                port(c, &r, 0x8003_3a50, 0x8002_7c9c, &[TOURNAMENT_MANAGER], crate::fetourn::abi_update_award_information);
            } else if m.r8(FE + 4) == 6 && m.r32(GAME_SETUP.0 + 0x24) == 0 {
                let winner = m.r16(FE + 90) as i16;
                let few = (m.r32(FINISHERS) as i32) < 2;
                let slot = m.r8(FE + 70) as u32;
                let pinks = (slot << 1).wrapping_add(FE) + 12;
                if (few && winner != 0) || winner == 1 {
                    m.w16(pinks, 0);
                    let v = m.r8(FE + 72).wrapping_add(1);
                    m.w8(FE + 72, v);
                } else {
                    m.w16(pinks, 1);
                    let v = m.r8(FE + 73).wrapping_add(1);
                    m.w8(FE + 73, v);
                }
            }
            let app = c.m.r32(FE_APP);
            let v = r.call_leaving(c, RUN_POST_GAME, &[app], 0x8002_7d68);
            r.0[3] = v;
        }
        // (each s-register set in the delay slot of the call before which it's listed)
        r.0[0] = 0x8014_0000;
        r.enter_leaving(c, 0x8002_7008, 0x8002_7d74, abi_delete_all);
        r.0[1] = 0x8011_0000;
        r.enter_leaving(c, 0x8001_62a0, 0x8002_7d7c, crate::feaudio::abi_deinit_commentary);
        r.0[1] = FE;
        r.call_leaving(c, CAR_SOUNDS_END, &[], 0x8002_7d84);
        r.call_leaving(c, UNLOAD_WORDS, &[], 0x8002_7d8c);
        r.enter_leaving(c, 0x8002_6cb0, 0x8002_7d94, crate::fetools::abi_deinit);
        r.enter_leaving(c, 0x8004_e158, 0x8002_7d9c, crate::psxfront::abi_psx_exit_frontend);
        let big = c.m.r32(BIG_FILE);
        r.call_ptr_leaving(c, FILE_DEL_BIG_SYNC, &[big, 100], 0x8002_7da8);
        let track = c.m.r8(FE + 35) as u32;
        c.m.w32(BIG_FILE, 0);
        let lap = port(c, &r, 0x8004_b33c, 0x8002_7db4, &[track], crate::stattool::abi_return_record_lap_time);
        c.m.w32(FE + 40, lap);
        let left = r.call_leaving(c, LARGEST_UNUSED, &[], 0x8002_7dbc);
        c.m.w32(MEMORY_LEFT, left);
        c.r[2] = r.0[3];
    })
}

/// The screens' constructors and destructors still recompiled: the car select's (three of
/// them), the controller's, the memory card's; `operator delete`.
const CAR_SELECT_CTOR: fn(&mut Cpu) = nfs4rc::f_8003ae58;
const CAR_SELECT_DTOR: fn(&mut Cpu) = nfs4rc::f_8003af68;
const CONTROLLER_CTOR: fn(&mut Cpu) = nfs4rc::f_80045c44;
const MEM_CARD_CTOR: fn(&mut Cpu) = nfs4rc::f_80047fcc;
const DELETE: fn(&mut Cpu) = nfs4rc::f_800a4910;
/// A dialog inlined into the screens (its classes' vtables in turn: the dialog's, the
/// message's, the box's).
const DIALOG_VT: u32 = 0x8001_05A8;
const MESSAGE_VT: u32 = 0x8001_04E8;
const BOX_VT: u32 = 0x8001_0488;
/// A screen's vtable (+96).
const VPTR: u32 = 96;

/// A dialog constructed inline at `d` (288 wide, nothing shown, for either player), through
/// its classes' vtables.
fn inline_dialog(m: &mut ps1rt::Mem, d: u32) {
    m.w16(d + 128, 288);
    m.w32(d + VPTR, DIALOG_VT);
    m.w32(d + 112, 0);
    for at in [110, 130, 126, 124, 108, 106, 104, 102] {
        m.w16(d + at, 0);
    }
    m.w16(d + 100, 0xFFFF);
    m.w16(d + 136, 0);
    m.w32(d + 120, 0);
    m.w32(d + VPTR, MESSAGE_VT);
    m.w32(d + 148, 0);
    m.w32(d + 132, 0);
    m.w32(d + 120, 0);
    m.w32(d + 140, 128);
    m.w32(d + VPTR, BOX_VT);
}

/// `tAllScreens::tAllScreens` (FRONT.CPP, 0x8002bb74): every screen of the front end
/// constructed in place, each given its class's vtable (three dialogs among them). `this`.
pub fn abi_all_screens(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let screen = |c: &mut Cpu, r: &Saved, at: u32, ra: u32| {
            port(c, r, 0x8002_6040, ra, &[this + at], crate::fescreen::abi_screen);
        };
        let vt = |c: &mut Cpu, at: u32, v: u32| c.m.w32(this + at + VPTR, v);
        screen(c, &r, 0, 0x8002_bba8);
        vt(c, 0, 0x8001_1858);
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 1464], 0x8002_bbbc);
        r.0[0] = this + 2392;
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 2392], 0x8002_bbc8);
        r.0[2] = this + 3368;
        vt(c, 2392, 0x8001_1C30);
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 3368], 0x8002_bbe0);
        r.0[0] = this + 4296;
        vt(c, 3368, 0x8001_1BA0);
        screen(c, &r, 4296, 0x8002_bbf8);
        (r.0[2], r.0[7], r.0[4], r.0[8], r.0[3]) = (this + 4448, DIALOG_VT, MESSAGE_VT, 128, BOX_VT);
        inline_dialog(&mut c.m, this + 4296);
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 4448], 0x8002_bc78);
        r.0[0] = this + 5376;
        vt(c, 4448, 0x8001_1BA0);
        screen(c, &r, 5376, 0x8002_bc90);
        r.0[5] = this + 5528;
        inline_dialog(&mut c.m, this + 5376);
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 5528], 0x8002_bcf4);
        r.0[0] = this + 6456;
        vt(c, 5528, 0x8001_1BA0);
        screen(c, &r, 6456, 0x8002_bd0c);
        (r.0[6], r.0[2]) = (this + 6628, 0x8001_1B10);
        inline_dialog(&mut c.m, this + 6456);
        vt(c, 5528, 0x8001_1B10);
        r.call_leaving(c, CAR_SELECT_CTOR, &[this + 6628], 0x8002_bd7c);
        r.0[0] = this + 7556;
        vt(c, 6628, 0x8001_1BA0);
        screen(c, &r, 7556, 0x8002_bd94);
        r.0[5] = this + 7728;
        inline_dialog(&mut c.m, this + 7556);
        vt(c, 6628, 0x8001_1B10);
        screen(c, &r, 7728, 0x8002_bdfc);
        r.0[2] = this + 7844;
        vt(c, 7728, 0x8001_1FA0);
        screen(c, &r, 7844, 0x8002_be14);
        r.0[0] = this + 8520;
        vt(c, 7844, 0x8001_1F28);
        screen(c, &r, 8520, 0x8002_be2c);
        vt(c, 8520, 0x8001_1EB8);
        port(c, &r, 0x8003_fb48, 0x8002_be40, &[this + 9192], crate::screentournselect::abi_screen_tourn_select);
        port(c, &r, 0x8003_9bb8, 0x8002_be48, &[this + 9904], crate::screenpost::abi_screen_tournament_standings);
        r.0[0] = this + 10052;
        screen(c, &r, 10052, 0x8002_be54);
        vt(c, 10052, 0x8001_2390);
        port(c, &r, 0x8004_0950, 0x8002_be68, &[this + 10444], crate::screentrophyroom::abi_screen_trophy_room);
        r.0[0] = this + 10788;
        screen(c, &r, 10788, 0x8002_be74);
        vt(c, 10788, 0x8001_1E40);
        r.call_leaving(c, CONTROLLER_CTOR, &[this + 10892], 0x8002_be88);
        r.0[0] = this + 11272;
        screen(c, &r, 11272, 0x8002_be94);
        vt(c, 11272, 0x8001_2080);
        port(c, &r, 0x8004_654c, 0x8002_bea8, &[this + 11372], crate::screenaudio::abi_screen_audio);
        r.call_leaving(c, MEM_CARD_CTOR, &[this + 11496], 0x8002_beb0);
        r.0[0] = this + 12940;
        screen(c, &r, 12940, 0x8002_bebc);
        r.0[2] = this + 13144;
        vt(c, 12940, 0x8001_2698);
        screen(c, &r, 13144, 0x8002_bed4);
        r.0[0] = this + 13536;
        vt(c, 13144, 0x8001_2328);
        port(c, &r, 0x8003_9bb8, 0x8002_beec, &[this + 13536], crate::screenpost::abi_screen_tournament_standings);
        r.0[3] = this + 13684;
        vt(c, 13536, 0x8001_1950);
        r.0[0] = 0x8001_19A0;
        port(c, &r, 0x8003_9bb8, 0x8002_bf0c, &[this + 13684], crate::screenpost::abi_screen_tournament_standings);
        r.0[2] = this + 13832;
        vt(c, 13684, 0x8001_19A0);
        screen(c, &r, 13832, 0x8002_bf1c);
        r.0[3] = this + 14544;
        vt(c, 13832, 0x8001_18D8);
        screen(c, &r, 14544, 0x8002_bf34);
        r.0[0] = this + 14932;
        vt(c, 14544, 0x8001_22C0);
        screen(c, &r, 14932, 0x8002_bf4c);
        vt(c, 14932, 0x8001_2258);
        c.r[2] = this;
    })
}

/// `tAllScreens::~tAllScreens` (FRONT.CPP, 0x8002bf8c): every screen destroyed, last first
/// (`2`: in place); the object freed when `flags` asks.
pub fn abi_all_screens_dtor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[1]) = (this, flags);
        let screen = |c: &mut Cpu, r: &Saved, at: u32, ra: u32| {
            port(c, r, 0x8002_6098, ra, &[this + at, 2], crate::fescreen::abi_screen_destructor);
        };
        for (at, ra) in [(14932, 0x8002_bfb4), (14544, 0x8002_bfc0), (13832, 0x8002_bfcc), (13684, 0x8002_bfd8), (13536, 0x8002_bfe4), (13144, 0x8002_bff0), (12940, 0x8002_bffc), (11496, 0x8002_c008), (11372, 0x8002_c014), (11272, 0x8002_c020)] {
            screen(c, &r, at, ra);
        }
        r.0[0] = this + 10892;
        screen(c, &r, 11076, 0x8002_c030);
        screen(c, &r, 10892, 0x8002_c03c);
        screen(c, &r, 10788, 0x8002_c048);
        port(c, &r, 0x8004_0990, 0x8002_c054, &[this + 10444, 2], crate::screentrophyroom::abi_destructor);
        screen(c, &r, 10052, 0x8002_c060);
        screen(c, &r, 9904, 0x8002_c06c);
        port(c, &r, 0x8003_fb7c, 0x8002_c078, &[this + 9192, 2], crate::screentournselect::abi_destructor);
        for (at, ra) in [(8520, 0x8002_c084), (7844, 0x8002_c090), (7728, 0x8002_c09c)] {
            screen(c, &r, at, ra);
        }
        for (dialog, select, ra1, ra2) in [(7556, 6628, 0x8002_c0ac, 0x8002_c0b8), (6456, 5528, 0x8002_c0c8, 0x8002_c0d4), (5376, 4448, 0x8002_c0e4, 0x8002_c0f0), (4296, 3368, 0x8002_c100, 0x8002_c10c)] {
            r.0[0] = this + select;
            screen(c, &r, dialog, ra1);
            r.call_leaving(c, CAR_SELECT_DTOR, &[this + select, 2], ra2);
        }
        r.call_leaving(c, CAR_SELECT_DTOR, &[this + 2392, 2], 0x8002_c118);
        r.call_leaving(c, CAR_SELECT_DTOR, &[this + 1464, 2], 0x8002_c124);
        screen(c, &r, 0, 0x8002_c130);
        r.0[1] = flags & 1;
        if flags & 1 != 0 {
            r.call_leaving(c, DELETE, &[this], 0x8002_c144);
        }
    })
}

/// FRONT.CPP's ports entered as the original's callers `jal` them.
macro_rules! entries {
    ($($entry:ident = $addr:expr, $f:ident;)*) => {
        $(pub(crate) fn $entry(c: &mut Cpu) {
            abi::leave_prologue(c, $addr, None, &[]);
            $f(c);
        })*
    };
}

entries! {
    reset_psx_controller_entry = 0x8002_7104, abi_reset_psx_controller;
    initial_mem_card_check_entry = 0x8002_7968, abi_initial_mem_card_check;
    secondary_mem_card_check_entry = 0x8002_79cc, abi_secondary_mem_card_check;
    get_laps_for_type_entry = 0x8002_8004, abi_get_laps_for_type;
    enable_local_speech_entry = 0x8002_a724, abi_enable_local_speech;
    set_license_plate_entry = 0x8002_b84c, abi_set_license_plate;
    set_play_list_entry = 0x8002_b8f4, abi_set_play_list;
    get_track_raced_entry = 0x8002_b9e0, abi_get_track_raced;
    player_name_exist_entry = 0x8002_ba50, abi_player_name_exist;
    player_name_entry = 0x8002_ba7c, abi_player_name;
    player_name_mixed_case_entry = 0x8002_bb08, abi_player_name_mixed_case;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn front_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x911 + k as u64, shuffle, &args);
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

    #[test]
    fn front_stream_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in stream_cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x931 + k as u64, stream_shuffle, &args);
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
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 30, 0x951 + k as u64, shuffle, &args);
        }
    }
}
