//! STATTOOL.CPP: the track records' tools: an index sorting times, a time written as
//! minutes:seconds.hundredths (the separators by language), names upper-cased (the accented
//! letters too) and title-cased, the default records read from `zrecord.dat` (`znfsrec.dat`),
//! a track's records (`tRecordBuffer`, 17 records of 20 bytes: +0 the name, +8 the car, +12 the
//! time, +16 the lap: the best lap's, then two kinds' top eight), whether a human races, and an
//! AI's name.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::femenuextended::{SPRINTF, TEXT_WORD};
use crate::fetools;
use crate::globals::FRONT_END;
use ps1rt::{Cpu, Mem};

/// The race's stats (`Car_tStats`, 160 bytes each): +108 their flags (4 a human's).
pub(crate) const STATS_SIZE: u32 = 160;
pub(crate) const FLAGS: u32 = 108;
pub(crate) const HUMAN: u32 = 4;
/// A record (20 bytes): +0 its name, +8 its car, +12 its time, +16 its lap; a track's 17 (340
/// bytes); the tracks' records (`TrackRecords`, by track).
pub(crate) const RECORD_SIZE: u32 = 20;
pub(crate) const CAR: u32 = 8;
pub(crate) const TIME: u32 = 12;
pub(crate) const LAP: u32 = 16;
pub(crate) const RECORDS_PER_TRACK: u32 = 17;
pub(crate) const RECORDS_SIZE: u32 = 340;
pub(crate) const TRACK_RECORDS: u32 = 0x8011_604C;
/// How many cars race: the humans and the AI (`GameSetup_gData`'s).
const HUMANS: u32 = 0x8011_486C;
const AIS: u32 = 0x8011_4870;
/// The AIs' names (8 bytes each, by personality, 15 of them); the separators after the minutes
/// and the seconds (by language); the language (`frontEnd + 0x340`; 4 French); the CD's
/// directory.
const AI_NAMES: u32 = 0x8011_442C;
const AI_NAME_COUNT: u32 = 15;
const MINUTES_SEPARATORS: u32 = 0x8005_2E30;
const SECONDS_SEPARATORS: u32 = 0x8005_2E38;
const LANGUAGE: u32 = FRONT_END + 0x340;
const FRENCH: u8 = 4;
const CD_PATH: u32 = 0x8011_77B0;
/// "TempSort", "%02d%c%02d%c%02d", "records", "%sznfsrec.dat", "%szrecord.dat", "trkrcrds".
const TEMP_SORT: u32 = 0x8001_24C4;
const TIME_FORMAT: u32 = 0x8001_24D0;
const RECORDS_NAME: u32 = 0x8001_255C;
const NFS_RECORDS_FILE: u32 = 0x8001_2564;
const RECORDS_FILE: u32 = 0x8001_2574;
const TRACK_RECORDS_NAME: u32 = 0x8001_2584;
/// The default records' tracks and their file's size (11 tracks of 17).
const DEFAULT_TRACKS: u32 = 11;
const DEFAULT_SIZE: u32 = 3740;
/// "Driver", for AIs past the names.
const DRIVER: u32 = 751;

/// `reservememadr`, `purgememadr`, `memcpy`, `strcpy`, `blockmove`, `filesize`,
/// `loadfileatadrz`, `blockclear`; `__floatsisf`, `__divsf3`, `__fixsfsi`.
pub(crate) const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
pub(crate) const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
pub(crate) const MEMCPY: fn(&mut Cpu) = nfs4rc::f_800eb420;
pub(crate) const STRCPY: fn(&mut Cpu) = nfs4rc::f_800e6714;
pub(crate) const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const FILESIZE: fn(&mut Cpu) = nfs4rc::f_800e6258;
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e649c;
const BLOCKCLEAR: fn(&mut Cpu) = nfs4rc::f_800f24d4;
const FLOAT: fn(&mut Cpu) = nfs4rc::f_800f6b80;
const DIVIDE: fn(&mut Cpu) = nfs4rc::f_800f7040;
const FIX: fn(&mut Cpu) = nfs4rc::f_800f4294;

/// 0.64 (a `float`): ticks per hundredth.
const TICKS_PER_HUNDREDTH: u32 = 0x3F23_D70A;

pub(crate) fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

pub(crate) fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// Car `k`'s stats.
pub(crate) fn stat(stats: u32, k: u32) -> u32 {
    stats.wrapping_add(sx(k).wrapping_mul(STATS_SIZE))
}

/// `Stattool_nCreateIndex` (STATTOOL.CPP, 0x8004ad40): `index` (`short`s) the order of the `n`
/// `values`, smallest first: an insertion sort on a copy (in memory reserved for it; equal
/// values keep their order).
pub fn n_create_index(c: &mut Cpu, n: u32, values: u32, index: u32) {
    let temp = call(c, RESERVE, &[TEMP_SORT, n.wrapping_add(1) << 2, 16]);
    let m = &mut c.m;
    let n = n as i32;
    for k in 0..n.max(0) as u32 {
        m.w16(index + 2 * k, k as u16);
        let v = m.r32(values + 4 * k);
        m.w32(temp + 4 * k, v);
    }
    for k in 1..n.max(1) as u32 {
        let (v, id) = (m.r32(temp + 4 * k), m.r16(index + 2 * k));
        // the larger ones before it moved up one
        let mut j = k as i32 - 1;
        while j >= 0 {
            let at = m.r32(temp + 4 * j as u32);
            if (at as i32) < v as i32 {
                break;
            }
            m.w32(temp + 4 * (j as u32 + 1), at);
            let i = m.r16(index + 2 * j as u32);
            m.w16(index + 2 * (j as u32 + 1), i);
            j -= 1;
        }
        let j = (j + 1) as u32;
        m.w32(temp.wrapping_add(4 * j), v);
        m.w16(index.wrapping_add(2 * j), id);
    }
    call(c, PURGE, &[temp]);
}

pub fn abi_n_create_index(c: &mut Cpu) {
    n_create_index(c, c.r[4], c.r[5], c.r[6]);
}

/// `Stattool_ParseTime` (STATTOOL.CPP, 0x8004ae74): `time` (ticks) written to `out` as
/// minutes, seconds and hundredths with the language's separators (the hundredths through the
/// game's `float`s).
pub fn parse_time(c: &mut Cpu, time: u32, out: u32) {
    let f = call(c, FLOAT, &[time]);
    let f = call(c, DIVIDE, &[f, TICKS_PER_HUNDREDTH]);
    let t = call(c, FIX, &[f]) as i32;
    let minutes = (t / 6000) as i16 as i32;
    let t = t.wrapping_sub(minutes.wrapping_mul(6000));
    let seconds = (t / 100) as i16 as i32;
    let hundredths = t.wrapping_sub(seconds.wrapping_mul(100)) as i16 as i32;
    let m = &mut c.m;
    let language = m.r8(LANGUAGE) as u32;
    let first = m.r8(MINUTES_SEPARATORS + language) as u32;
    let second = m.r8(SECONDS_SEPARATORS + language) as u32;
    call(c, SPRINTF, &[out, TIME_FORMAT, minutes as u32, first, seconds as u32, second, hundredths as u32]);
}

pub fn abi_parse_time(c: &mut Cpu) {
    parse_time(c, c.r[4], c.r[5]);
}

/// `UserNameUpperCaseOneLetter` (STATTOOL.CPP, 0x8004af8c): `*p` upper-cased (a..z, and the
/// accented letters (Latin-1) by the table: some to their accented capitals, the rest to plain
/// ones; e-grave stays accented in French).
pub fn user_name_upper_case_one_letter(m: &mut Mem, p: u32) {
    let ch = m.r8(p);
    if ch.wrapping_sub(b'a') < 26 {
        m.w8(p, ch.wrapping_sub(b'a' - b'A'));
        return;
    }
    let to = match m.r8(p) {
        0xE0 => Some(0xC0),
        0xE1 => Some(b'A'),
        0xE4 => Some(0xC4),
        0xE5 => Some(0xC5),
        0xE8 => Some(if m.r8(LANGUAGE) == FRENCH { 0xC8 } else { b'E' }),
        0xE9 => Some(b'E'),
        0xEC => Some(0xCC),
        0xED => Some(b'I'),
        0xF1 => Some(0xD1),
        0xF2 => Some(0xD2),
        0xF3 => Some(b'O'),
        0xF6 => Some(0xD6),
        0xF9 => Some(0xD9),
        0xFA => Some(b'U'),
        0xFC => Some(0xDC),
        _ => None,
    };
    if let Some(v) = to {
        m.w8(p, v);
    }
}

pub fn abi_user_name_upper_case_one_letter(c: &mut Cpu) {
    user_name_upper_case_one_letter(&mut c.m, c.r[4]);
}

/// `Stattool_SamNelsonsUpperLowerStringConverterForRecords` (STATTOOL.CPP, 0x8004b0ac): `p`
/// lower-cased, then its first letter and each after a space upper-cased.
pub fn title_case(c: &mut Cpu, p: u32) {
    fetools::s_lower(c, p);
    let m = &mut c.m;
    let mut before = m.r8(p);
    user_name_upper_case_one_letter(m, p);
    let mut p = p.wrapping_add(1);
    while m.r8(p) != 0 {
        if before == b' ' {
            user_name_upper_case_one_letter(m, p);
        }
        before = m.r8(p);
        p = p.wrapping_add(1);
    }
}

pub fn abi_title_case(c: &mut Cpu) {
    title_case(c, c.r[4]);
}

/// `StatTool_UpperCaseItKeepingInMindThoseBloodySpecialCharacters` (STATTOOL.CPP, 0x8004b128):
/// `p` upper-cased letter by letter.
pub fn upper_case(m: &mut Mem, mut p: u32) {
    while m.r8(p) != 0 {
        user_name_upper_case_one_letter(m, p);
        p = p.wrapping_add(1);
    }
}

pub fn abi_upper_case(c: &mut Cpu) {
    upper_case(&mut c.m, c.r[4]);
}

/// `Stattool_GetAllDefaultRecords` (STATTOOL.CPP, 0x8004b174): the default records (11 tracks of
/// 17) into `out` (from memory reserved for the file), their names title-cased.
pub fn get_all_default_records(c: &mut Cpu, out: u32, nfs: u32) {
    let buffer = call(c, RESERVE, &[RECORDS_NAME, DEFAULT_SIZE, 16]);
    read_default_records(c, buffer, nfs);
    for k in 0..DEFAULT_TRACKS * RECORDS_PER_TRACK {
        let at = k * RECORD_SIZE;
        let (to, from) = (out.wrapping_add(at), buffer.wrapping_add(at));
        call(c, STRCPY, &[to, from]);
        title_case(c, to);
        let m = &mut c.m;
        for f in [CAR, TIME, LAP] {
            let v = m.r32(from + f);
            m.w32(to + f, v);
        }
    }
    call(c, PURGE, &[buffer]);
}

pub fn abi_get_all_default_records(c: &mut Cpu) {
    get_all_default_records(c, c.r[4], c.r[5]);
}

/// `Stattool_ReadDefaultRecords` (STATTOOL.CPP, 0x8004b26c): `zrecord.dat` (`znfsrec.dat`) read
/// into `buffer` when it is the right size, else `buffer` cleared.
pub fn read_default_records(c: &mut Cpu, buffer: u32, nfs: u32) {
    with_buffer(c, 88, |c, name| {
        let format = if nfs != 0 { NFS_RECORDS_FILE } else { RECORDS_FILE };
        let path = c.m.r32(CD_PATH);
        call(c, SPRINTF, &[name, format, path]);
        if call(c, FILESIZE, &[name]) == DEFAULT_SIZE {
            call(c, LOAD_FILE, &[name, buffer]);
        } else {
            call(c, BLOCKCLEAR, &[buffer, DEFAULT_SIZE]);
        }
    })
}

pub fn abi_read_default_records(c: &mut Cpu) {
    read_default_records(c, c.r[4], c.r[5]);
}

/// `Stattool_GetRecords` (STATTOOL.CPP, 0x8004b2f4): track `track`'s records into `buffer`.
pub fn get_records(c: &mut Cpu, track: u32, buffer: u32) {
    let from = TRACK_RECORDS.wrapping_add(sx(track).wrapping_mul(RECORDS_SIZE));
    call(c, BLOCKMOVE, &[from, buffer, RECORDS_SIZE]);
}

pub fn abi_get_records(c: &mut Cpu) {
    get_records(c, c.r[4], c.r[5]);
}

/// `Stattool_ReturnRecordLapTime` (STATTOOL.CPP, 0x8004b33c): track `track`'s best lap (its
/// records read into memory reserved for them, the first copied out).
pub fn return_record_lap_time(c: &mut Cpu, track: u32) -> u32 {
    let buffer = call(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16]);
    get_records(c, sx(track), buffer);
    with_buffer(c, RECORD_SIZE, |c, best| {
        call(c, MEMCPY, &[best, buffer, RECORD_SIZE]);
        call(c, PURGE, &[buffer]);
        c.m.r32(best + LAP)
    })
}

pub fn abi_return_record_lap_time(c: &mut Cpu) {
    c.r[2] = return_record_lap_time(c, c.r[4]);
}

/// `Stattool_CheckForHumanCar` (STATTOOL.CPP, 0x8004b3a0): whether any of the race's cars is a
/// human's.
pub fn check_for_human_car(m: &mut Mem, stats: u32) -> bool {
    let n = sx((m.r16(HUMANS) as u32).wrapping_add(m.r16(AIS) as u32)) as i32;
    (0..n.max(0) as u32).any(|k| m.r32(stat(stats, k) + FLAGS) & HUMAN != 0)
}

pub fn abi_check_for_human_car(c: &mut Cpu) {
    c.r[2] = check_for_human_car(&mut c.m, c.r[4]) as u32;
}

/// `Stattool_GetAINameFromPersonality` (STATTOOL.CPP, 0x8004b428): personality `p`'s name
/// ("Driver" past the 15).
pub fn get_ai_name_from_personality(c: &mut Cpu, p: u32) -> u32 {
    if p < AI_NAME_COUNT { AI_NAMES + p * 8 } else { call(c, TEXT_WORD, &[DRIVER]) }
}

pub fn abi_get_ai_name_from_personality(c: &mut Cpu) {
    c.r[2] = get_ai_name_from_personality(c, c.r[4]);
}
