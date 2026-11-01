//! STATTOOL.CPP: the track records' tools: an index sorting times, a time written as
//! minutes:seconds.hundredths (the separators by language), names upper-cased (the accented
//! letters too) and title-cased, the default records read from `zrecord.dat` (`znfsrec.dat`),
//! a track's records (`tRecordBuffer`, 17 records of 20 bytes: +0 the name, +8 the car, +12 the
//! time, +16 the lap: the best lap's, then two kinds' top eight), whether a human races, and an
//! AI's name.

use crate::abi::{self, Saved};
use crate::femenuextended::{SPRINTF, TEXT_WORD};
use crate::fetools;
use crate::globals::FRONT_END;
use ps1rt::{Cpu, Mem};

/// The race's stats (`Car_tStats`, 160 bytes each): +108 their flags (4 a human's).
pub(crate) const STATS_SIZE: u32 = 160;
pub(crate) const FLAGS: u32 = 108;
pub(crate) const HUMAN: u32 = 4;
/// A record (20 bytes): its time and lap (+0 its name, +8 its car); a track's 17 (340 bytes);
/// the tracks' records (`TrackRecords`, by track).
pub(crate) const RECORD_SIZE: u32 = 20;
pub(crate) const TIME: u32 = 12;
pub(crate) const LAP: u32 = 16;
pub(crate) const RECORDS_SIZE: u32 = 340;
pub(crate) const TRACK_RECORDS: u32 = 0x8011_604C;
/// How many cars race: the humans and the AI (`GameSetup_gData`'s).
const HUMANS: u32 = 0x8011_486C;
const AIS: u32 = 0x8011_4870;
/// The AIs' names (8 bytes each, by personality); the separators after the minutes and the
/// seconds (by language); the language (`frontEnd + 0x340`); the CD's directory.
const AI_NAMES: u32 = 0x8011_442C;
const MINUTES_SEPARATORS: u32 = 0x8005_2E30;
const SECONDS_SEPARATORS: u32 = 0x8005_2E38;
const LANGUAGE: u32 = FRONT_END + 0x340;
const CD_PATH: u32 = 0x8011_77B0;
/// "TempSort", "%02d%c%02d%c%02d", "records", "%sznfsrec.dat", "%szrecord.dat", "trkrcrds".
const TEMP_SORT: u32 = 0x8001_24C4;
const TIME_FORMAT: u32 = 0x8001_24D0;
const RECORDS_NAME: u32 = 0x8001_255C;
const NFS_RECORDS_FILE: u32 = 0x8001_2564;
const RECORDS_FILE: u32 = 0x8001_2574;
const TRACK_RECORDS_NAME: u32 = 0x8001_2584;
/// The default records' file's size (11 tracks of 17).
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

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
pub(crate) fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `Stattool_nCreateIndex` (STATTOOL.CPP, 0x8004ad40): `index` the order of the `n` `values`,
/// smallest first (an insertion sort on a copy; equal values the later first).
pub fn abi_n_create_index(c: &mut Cpu) {
    let (n, values, index) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = n;
        r.0[3] = values;
        r.0[2] = index;
        let temp = r.call_leaving(c, RESERVE, &[TEMP_SORT, n.wrapping_add(1) << 2, 16], 0x8004_ad7c);
        let m = &mut c.m;
        let n = n as i32;
        for k in 0..n.max(0) as u32 {
            m.w16(index + 2 * k, k as u16);
            let v = m.r32(values + 4 * k);
            m.w32(temp + 4 * k, v);
        }
        r.0[0] = 1;
        if n != 1 {
            for k in 1..n.max(1) {
                let k = k as u32;
                let (v, id) = (m.r32(temp + 4 * k), m.r16(index + 2 * k));
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
            r.0[0] = n.max(1) as u32;
        }
        r.call_leaving(c, PURGE, &[temp], 0x8004_ae58);
    })
}

/// `Stattool_ParseTime` (STATTOOL.CPP, 0x8004ae74): `time` (ticks) written to `out` as
/// minutes, seconds and hundredths with the language's separators.
pub fn abi_parse_time(c: &mut Cpu) {
    let (time, out) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = out;
        let f = r.call_leaving(c, FLOAT, &[time], 0x8004_ae8c);
        let f = r.call_leaving(c, DIVIDE, &[f, TICKS_PER_HUNDREDTH], 0x8004_ae9c);
        let t = r.call_leaving(c, FIX, &[f], 0x8004_aea4) as i32;
        let minutes = t / 6000;
        let minutes = minutes as i16 as i32;
        let t = t.wrapping_sub(minutes.wrapping_mul(6000));
        let seconds = (t / 100) as i16 as i32;
        let hundredths = t.wrapping_sub(seconds.wrapping_mul(100)) as i16 as i32;
        let m = &mut c.m;
        let language = m.r8(LANGUAGE) as u32;
        let first = m.r8(MINUTES_SEPARATORS + language) as u32;
        let second = m.r8(SECONDS_SEPARATORS + language) as u32;
        let args = [out, TIME_FORMAT, minutes as u32, first, seconds as u32, second, hundredths as u32];
        r.call_leaving(c, SPRINTF, &args, 0x8004_af7c);
    })
}

/// `UserNameUpperCaseOneLetter` (STATTOOL.CPP, 0x8004af8c): `*p` upper-cased (a..z, and the
/// accented letters by the table: some to their accented capitals, the rest to plain ones; the
/// French e-grave stays accented).
pub fn abi_user_name_upper_case_one_letter(c: &mut Cpu) {
    let p = c.r[4];
    let m = &mut c.m;
    let ch = m.r8(p);
    if ch.wrapping_sub(97) < 26 {
        m.w8(p, ch.wrapping_add(224));
        return;
    }
    let k = m.r8(p).wrapping_sub(224);
    let to = match k {
        0 => Some(192),
        1 => Some(65),
        4 => Some(196),
        5 => Some(197),
        8 => Some(if m.r8(LANGUAGE) == 4 { 200 } else { 69 }),
        9 => Some(69),
        12 => Some(204),
        13 => Some(73),
        17 => Some(209),
        18 => Some(210),
        19 => Some(79),
        22 => Some(214),
        25 => Some(217),
        26 => Some(85),
        28 => Some(220),
        _ => None,
    };
    if let Some(v) = to {
        m.w8(p, v);
    }
}

/// `Stattool_SamNelsonsUpperLowerStringConverterForRecords` (STATTOOL.CPP, 0x8004b0ac): `p`
/// lower-cased, then its first letter and each after a space upper-cased.
pub fn abi_title_case(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = p;
        port(c, &r, 0x8002_6970, 0x8004_b0c8, &[p], fetools::abi_s_lower);
        r.0[1] = c.m.r8(p) as u32;
        port(c, &r, 0x8004_af8c, 0x8004_b0d4, &[p], abi_user_name_upper_case_one_letter);
        let mut p = p.wrapping_add(1);
        r.0[0] = p;
        r.0[2] = 32;
        while c.m.r8(p) != 0 {
            if r.0[1] == 32 {
                port(c, &r, 0x8004_af8c, 0x8004_b0f8, &[p], abi_user_name_upper_case_one_letter);
            }
            r.0[1] = c.m.r8(p) as u32;
            p = p.wrapping_add(1);
            r.0[0] = p;
        }
    })
}

/// `StatTool_UpperCaseItKeepingInMindThoseBloodySpecialCharacters` (STATTOOL.CPP, 0x8004b128):
/// `p` upper-cased letter by letter.
pub fn abi_upper_case(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        let mut p = p;
        r.0[0] = p;
        while c.m.r8(p) != 0 {
            port(c, &r, 0x8004_af8c, 0x8004_b150, &[p], abi_user_name_upper_case_one_letter);
            p = p.wrapping_add(1);
            r.0[0] = p;
        }
    })
}

/// `Stattool_GetAllDefaultRecords` (STATTOOL.CPP, 0x8004b174): the default records (11 tracks of
/// 17) into `out`, their names title-cased.
pub fn abi_get_all_default_records(c: &mut Cpu) {
    let (out, nfs) = (c.r[4], c.r[5]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[6] = out;
        r.0[0] = nfs;
        let buffer = r.call_leaving(c, RESERVE, &[RECORDS_NAME, DEFAULT_SIZE, 16], 0x8004_b1b4);
        r.0[5] = buffer;
        port(c, &r, 0x8004_b26c, 0x8004_b1c4, &[buffer, nfs], abi_read_default_records);
        r.0[4] = 0;
        r.0[3] = 0;
        for track in 0..11 {
            r.0[4] = track;
            r.0[3] = track * 17;
            for k in 0..17 {
                r.0[2] = k;
                let at = (track * 17 + k) * RECORD_SIZE;
                let (to, from) = (out.wrapping_add(at), buffer.wrapping_add(at));
                r.0[1] = to;
                r.0[0] = from;
                r.call_leaving(c, STRCPY, &[to, from], 0x8004_b1f4);
                port(c, &r, 0x8004_b0ac, 0x8004_b1fc, &[to], abi_title_case);
                let m = &mut c.m;
                for f in [8, 12, 16] {
                    let v = m.r32(from + f);
                    m.w32(to + f, v);
                }
            }
            r.0[2] = 17;
        }
        r.0[4] = 11;
        r.0[3] = 11 * 17;
        r.call_leaving(c, PURGE, &[buffer], 0x8004_b244);
    })
}

/// `Stattool_ReadDefaultRecords` (STATTOOL.CPP, 0x8004b26c): `zrecord.dat` (`znfsrec.dat`) read
/// into `buffer` when it is the right size, else `buffer` cleared.
pub fn abi_read_default_records(c: &mut Cpu) {
    let (buffer, nfs) = (c.r[4], c.r[5]);
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = buffer;
        let name = sp + 16;
        let format = if nfs != 0 { NFS_RECORDS_FILE } else { RECORDS_FILE };
        let path = c.m.r32(CD_PATH);
        r.call_leaving(c, SPRINTF, &[name, format, path], 0x8004_b2b4);
        if r.call_leaving(c, FILESIZE, &[name], 0x8004_b2bc) == DEFAULT_SIZE {
            r.call_leaving(c, LOAD_FILE, &[name, buffer], 0x8004_b2d4);
        } else {
            r.call_leaving(c, BLOCKCLEAR, &[buffer, DEFAULT_SIZE], 0x8004_b2e4);
        }
    })
}

/// `Stattool_GetRecords` (STATTOOL.CPP, 0x8004b2f4): track `track`'s records into `buffer`.
pub fn abi_get_records(c: &mut Cpu) {
    let (track, buffer) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let from = TRACK_RECORDS.wrapping_add(sx(track).wrapping_mul(RECORDS_SIZE));
        Saved::of(c).call_leaving(c, BLOCKMOVE, &[from, buffer, RECORDS_SIZE], 0x8004_b32c);
    })
}

/// `Stattool_ReturnRecordLapTime` (STATTOOL.CPP, 0x8004b33c): track `track`'s best lap.
pub fn abi_return_record_lap_time(c: &mut Cpu) {
    let track = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = track;
        let buffer = r.call_leaving(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16], 0x8004_b360);
        r.0[0] = buffer;
        port(c, &r, 0x8004_b2f4, 0x8004_b374, &[sx(track), buffer], abi_get_records);
        r.call_leaving(c, MEMCPY, &[sp + 16, buffer, RECORD_SIZE], 0x8004_b384);
        r.call_leaving(c, PURGE, &[buffer], 0x8004_b38c);
        c.r[2] = c.m.r32(sp + 16 + LAP);
    })
}

/// `Stattool_CheckForHumanCar` (STATTOOL.CPP, 0x8004b3a0): whether any of the race's cars is a
/// human's.
pub fn abi_check_for_human_car(c: &mut Cpu) {
    let stats = c.r[4];
    let m = &mut c.m;
    let n = sx((m.r16(HUMANS) as u32).wrapping_add(m.r16(AIS) as u32)) as i32;
    let mut found = 0;
    let mut k = 0u32;
    while (k as i32) < n {
        if m.r32(stat(stats, k) + FLAGS) & HUMAN != 0 {
            found = 1;
        }
        k += 1;
        if found == 1 {
            break;
        }
    }
    c.r[2] = found;
}

/// `Stattool_GetAINameFromPersonality` (STATTOOL.CPP, 0x8004b428): personality `p`'s name
/// ("Driver" past the 15).
pub fn abi_get_ai_name_from_personality(c: &mut Cpu) {
    let p = c.r[4];
    abi::frame(c, 24, |c, _| {
        c.r[2] = if p < 15 {
            AI_NAMES + p * 8
        } else {
            Saved::of(c).call_leaving(c, TEXT_WORD, &[DRIVER], 0x8004_b454)
        };
    })
}
