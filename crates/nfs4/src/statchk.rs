//! STATCHK.CPP: the track records checked and saved after a race: the best lap (a human's,
//! the fastest of the cars not of classes 7 and 8, beating the record or the first set), the
//! top eight times (each human car's finish placed into the kind's eight, the sorted names and
//! times saved, the new ones flagged for the records screen), cleared flags.

use crate::abi::{self, Saved};
use crate::fecars;
use crate::globals::{CAR_MANAGER, FRONT_END};
use crate::stattool::{self, BLOCKMOVE, FLAGS, HUMAN, LAP, MEMCPY, PURGE, RECORD_SIZE, RECORDS_SIZE, RESERVE, STATS_SIZE, STRCPY, TIME, TRACK_RECORDS, lh, port, stat, sx};
use ps1rt::{Cpu, Mem};

// A car's stats (`Car_tStats`): +60 its place, +96 its sim number, +108 its flags (4 a human's,
// 0x200 no records), +124 off the race, +128 its state (2 finished), +136 its time, +156 its
// best lap.
const PLACE: u32 = 60;
const SIM: u32 = 96;
const NO_RECORDS: u32 = 0x200;
const OUT: u32 = 124;
const STATE: u32 = 128;
const FINISHED: u32 = 2;
const RACE_TIME: u32 = 136;
const BEST_LAP: u32 = 156;
/// A car description's class (+2); classes 7 and 8 (the cop cars) set no records.
const CLASS: u32 = 2;
/// How many players (`frontEnd + 3`); the player racing alone (`GameSetup_gData`'s); the new
/// records (8 words) and a new best lap.
const PLAYERS: u32 = FRONT_END + 3;
const SOLO_PLAYER: u32 = 0x8011_44C0;
const NEW_RECORDS: u32 = 0x8005_2E08;
const NEW_LAP_RECORD: u32 = 0x8005_2E28;
/// "ranklap", "rankbst", "trkrcrds", "toprcrds", "carttime", "carttrnk", "records".
const RANK_LAP: u32 = 0x8001_246C;
const RANK_BEST: u32 = 0x8001_2474;
const TRACK_RECORDS_NAME: u32 = 0x8001_247C;
const TOP_RECORDS: u32 = 0x8001_2488;
const CAR_TIME: u32 = 0x8001_2494;
const CAR_RANK: u32 = 0x8001_24A0;
const RECORDS_NAME: u32 = 0x8001_24AC;
/// A time put last (by the car's place) in place of none.
const NO_TIME: u32 = 144_000;

/// `Front_GetTrackRaced`, `PlayerName`, the track's kind (2 one with its own records),
/// `memset`, `strcmp`.
const TRACK_RACED: fn(&mut Cpu) = crate::front::get_track_raced_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const TRACK_KIND: fn(&mut Cpu) = crate::front::get_laps_for_type_entry;
const MEMSET: fn(&mut Cpu) = nfs4rc::f_800e4f04;
const STRCMP: fn(&mut Cpu) = nfs4rc::f_800e6968;

/// A car of classes 7 or 8.
fn cop_car(m: &mut Mem, car: u32) -> bool {
    m.r8(car + CLASS).wrapping_sub(7) < 2
}

/// `time` beats `record`, or the last place (`last`) is empty.
fn beats(record: u32, last: u32, time: u32) -> bool {
    record as i32 >= time as i32 || (last == 0 && time as i32 > 0)
}

/// A car's description by its sim number.
fn car_of(c: &mut Cpu, r: &Saved, st: u32, ra: u32) -> u32 {
    let sim = lh(&mut c.m, st + SIM);
    port(c, r, 0x8001_63e4, ra, &[CAR_MANAGER.0, sim], fecars::abi_get_car_from_sim_id)
}

/// `StatChk_IsRecordLapTime` (STATCHK.CPP, 0x80049cc8): with a human racing (and no car
/// barred), the cars ranked by best lap (cop cars last); the first human one with a lap (as the
/// original indexes it) checked against the track's record: whether it beats it (or there is
/// none), the car in `*car`.
pub fn abi_is_record_lap_time(c: &mut Cpu) {
    let (stats, n, out) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[7] = stats;
        r.0[0] = n;
        c.m.w32(sp + 112, out);
        c.m.w16(sp + 48, 0);
        let human = port(c, &r, 0x8004_b3a0, 0x8004_9d08, &[stats], stattool::abi_check_for_human_car);
        r.0[6] = human;
        c.m.w16(sp + 40, n as u16);
        if sx(human) != 1 {
            c.r[2] = 0;
            return;
        }
        let n = sx(n);
        r.0[0] = n;
        let laps = r.call_leaving(c, RESERVE, &[RANK_LAP, n << 2, 16], 0x8004_9d44);
        r.0[5] = laps;
        let index = r.call_leaving(c, RESERVE, &[RANK_BEST, n << 1, 16], 0x8004_9d5c);
        r.0[4] = index;
        let records = r.call_leaving(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16], 0x8004_9d74);
        r.0[3] = 0;
        r.0[8] = records;
        let purge_all = |c: &mut Cpu, r: &Saved, ras: [u32; 3]| {
            for (at, ra) in [laps, index, records].into_iter().zip(ras) {
                r.call_leaving(c, PURGE, &[at], ra);
            }
        };
        if n as i32 > 0 {
            c.m.w32(sp + 56, n);
            let mut k = 0u32;
            loop {
                let s1 = sx(k);
                r.0[1] = s1;
                r.0[2] = s1 << 2;
                let st = stat(stats, s1);
                r.0[0] = st;
                let car = car_of(c, &r, st, 0x8004_9dac);
                let m = &mut c.m;
                if m.r32(st + FLAGS) & NO_RECORDS != 0 {
                    purge_all(c, &r, [0x8004_9f14, 0x8004_9f1c, 0x8004_9f24]);
                    c.r[2] = 0;
                    return;
                }
                let v = if cop_car(m, car) { s1.wrapping_add(1).wrapping_mul(NO_TIME) } else { m.r32(st + BEST_LAP) };
                m.w32(laps.wrapping_add(s1 << 2), v);
                k = k.wrapping_add(1);
                r.0[3] = k;
                if sx(k) as i32 >= m.r32(sp + 56) as i32 {
                    break;
                }
            }
        }
        let n = lh(&mut c.m, sp + 40);
        if (n as i32) < 2 {
            c.m.w16(index, 0);
        } else {
            port(c, &r, 0x8004_ad40, 0x8004_9e50, &[n, laps, index], stattool::abi_n_create_index);
        }
        // the first human car with a lap
        let m = &mut c.m;
        let n = lh(m, sp + 40) as i32;
        let mut k = 0u32;
        if n > 0 {
            loop {
                let j = lh(m, index.wrapping_add(sx(k) << 1));
                if m.r32(stat(stats, j) + FLAGS) & HUMAN != 0 {
                    let jj = lh(m, index.wrapping_add(j << 1));
                    if m.r32(stat(stats, jj) + BEST_LAP) as i32 > 0 {
                        r.0[6] = 1;
                        break;
                    }
                }
                k = k.wrapping_add(1);
                r.0[6] = 0;
                if sx(k) as i32 >= n {
                    break;
                }
            }
        }
        let ras = [0x8004_9fb8, 0x8004_9fc0, 0x8004_9fc8];
        if r.0[6] << 16 == 0 {
            purge_all(c, &r, ras);
            c.r[2] = c.m.r16(sp + 48) as u32;
            return;
        }
        let car = c.m.r16(index.wrapping_add(sx(k) << 1)) as u32;
        r.0[0] = car;
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_9f40);
        port(c, &r, 0x8004_b2f4, 0x8004_9f50, &[sx(track), records], stattool::abi_get_records);
        r.call_leaving(c, MEMCPY, &[sp + 16, records, RECORD_SIZE], 0x8004_9f60);
        let m = &mut c.m;
        let lap = m.r32(stat(stats, car) + BEST_LAP);
        let record = m.r32(sp + 16 + LAP);
        if (record as i32 >= lap as i32 || record == 0) && lap as i32 > 0 {
            m.w16(sp + 48, 1);
        }
        let out = m.r32(sp + 112);
        m.w16(out, car as u16);
        purge_all(c, &r, ras);
        c.r[2] = c.m.r16(sp + 48) as u32;
    })
}

/// `StatChk_SaveRecordLapTime` (STATCHK.CPP, 0x8004a004): car `k`'s best lap (not a cop car's)
/// saved as the track's record: the player's name, the car, its time (finished) and lap; a new
/// lap record flagged.
pub fn abi_save_record_lap_time(c: &mut Cpu) {
    let (stats, k) = (c.r[4], c.r[6]);
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = stats;
        let st = stat(stats, k);
        r.0[0] = st;
        r.0[3] = k;
        let car = car_of(c, &r, st, 0x8004_a050);
        r.0[2] = car;
        if cop_car(&mut c.m, car) {
            return;
        }
        let records = r.call_leaving(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16], 0x8004_a07c);
        r.0[1] = records;
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_a084);
        port(c, &r, 0x8004_b2f4, 0x8004_a094, &[sx(track), records], stattool::abi_get_records);
        r.call_leaving(c, MEMCPY, &[sp + 16, records + RECORD_SIZE, RECORD_SIZE], 0x8004_a0a4);
        let m = &mut c.m;
        let time = if m.r32(st + OUT) == 0 && m.r32(st + STATE) == FINISHED { m.r32(st + RACE_TIME) } else { 0 };
        m.w32(sp + 52, time);
        let lap = m.r32(stat(stats, k) + BEST_LAP);
        m.w32(sp + 56, lap);
        let id = m.r8(car) as i8 as i32 as u32;
        m.w32(sp + 48, id);
        let name = r.call_leaving(c, PLAYER_NAME, &[sx(k)], 0x8004_a104);
        r.0[0] = sp + 40;
        r.call_leaving(c, STRCPY, &[sp + 40, name], 0x8004_a114);
        r.call_leaving(c, MEMCPY, &[records, sp + 40, RECORD_SIZE], 0x8004_a124);
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_a12c);
        let to = TRACK_RECORDS.wrapping_add(sx(track).wrapping_mul(RECORDS_SIZE));
        r.call_leaving(c, BLOCKMOVE, &[records, to, RECORDS_SIZE], 0x8004_a160);
        c.m.w32(NEW_LAP_RECORD, 1);
        r.call_leaving(c, PURGE, &[records], 0x8004_a174);
    })
}

/// The cars by place (`order[place - 1]`, `short`s).
fn order_by_place(m: &mut Mem, stats: u32, order: u32, n: u32) {
    for k in 0..n {
        let place = m.r32(stat(stats, k) + PLACE);
        m.w16(order.wrapping_add(place << 1).wrapping_sub(2), k as u16);
    }
}

/// `StatChk_IsTopTime` (STATCHK.CPP, 0x8004a194): which human cars' finishing times (with
/// one or two players) enter the kind's top eight (bit 0 the first player's, 2 the second's)
/// and beat its first (bits 1 and 3); of two players both in, only the faster counts. No car
/// barred from records.
pub fn abi_is_top_time(c: &mut Cpu) {
    let (stats, n) = (c.r[4], c.r[5]);
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[6] = stats;
        r.0[0] = n;
        r.0[7] = 0;
        let kind = r.call_leaving(c, TRACK_KIND, &[], 0x8004_a1d0);
        r.0[3] = kind;
        for (at, ra) in [(16, 0x8004_a1e4), (24, 0x8004_a1f4), (32, 0x8004_a204)] {
            r.call_leaving(c, MEMSET, &[sp + at, 0, 8], ra);
        }
        let top = r.call_leaving(c, RESERVE, &[TOP_RECORDS, 360, 16], 0x8004_a218);
        let count = sx(n);
        r.0[1] = count;
        r.0[8] = top;
        let times = r.call_leaving(c, RESERVE, &[CAR_TIME, count << 2, 16], 0x8004_a238);
        c.m.w32(sp + 48, times);
        let order = r.call_leaving(c, RESERVE, &[CAR_RANK, count << 1, 16], 0x8004_a250);
        r.0[2] = order;
        c.m.w16(sp + 40, n as u16);
        if count as i32 > 0 {
            order_by_place(&mut c.m, stats, order, count);
        }
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_a2b4);
        port(c, &r, 0x8004_b2f4, 0x8004_a2c4, &[sx(track), top], stattool::abi_get_records);
        let first = if sx(kind) == 2 { 1 } else { 9 };
        r.0[1] = 0;
        r.0[3] = 1;
        let kind_top = top + first * RECORD_SIZE;
        let last = kind_top + 7 * RECORD_SIZE;
        r.0[5] = kind_top;
        r.0[4] = last;
        let mut k = 0u32;
        loop {
            let s0 = sx(k);
            r.0[0] = s0;
            if s0 as i32 >= lh(&mut c.m, sp + 40) as i32 {
                break;
            }
            let car = car_of(c, &r, stat(stats, s0), 0x8004_a330);
            let m = &mut c.m;
            let idx = lh(m, order.wrapping_add(s0 << 1));
            let st = stat(stats, idx);
            let flags = m.r32(st + FLAGS);
            if flags & NO_RECORDS != 0 {
                r.call_leaving(c, PURGE, &[top], 0x8004_a590);
                let times = c.m.r32(sp + 48);
                r.call_leaving(c, PURGE, &[times], 0x8004_a59c);
                r.call_leaving(c, PURGE, &[order], 0x8004_a5a4);
                c.r[2] = 0;
                return;
            }
            if !cop_car(m, car) {
                if m.r8(PLAYERS) < 3 && flags & HUMAN != 0 && m.r32(st + STATE) == FINISHED {
                    r.0[7] = 1;
                }
                if r.0[7] == 1 {
                    let time = m.r32(st + RACE_TIME);
                    let at = sp.wrapping_add(idx << 2);
                    r.0[7] = 0;
                    let last_time = m.r32(last + TIME);
                    if beats(m.r32(kind_top + 6 * RECORD_SIZE + TIME), last_time, time) {
                        m.w32(at + 24, 1);
                    } else if beats(last_time, last_time, time) {
                        m.w32(at + 24, 1);
                        m.w32(at + 16, 1);
                    }
                    if beats(m.r32(kind_top + TIME), m.r32(last + TIME), time) {
                        m.w32(at + 24, 1);
                        m.w32(at + 32, 1);
                    }
                }
            }
            k = k.wrapping_add(1);
            r.0[1] = k;
        }
        // of two players both in, the faster
        let m = &mut c.m;
        let w = |m: &mut Mem, at: u32| m.r32(sp + at) != 0;
        let faster_second = (m.r32(stats + STATS_SIZE + RACE_TIME) as i32) < m.r32(stats + RACE_TIME) as i32;
        let mut both = true;
        if w(m, 16) {
            if w(m, 20) {
                both = false;
                m.w32(sp + if faster_second { 24 } else { 28 }, 0);
            } else if w(m, 28) {
                both = false;
                m.w32(sp + 24, 0);
            }
        }
        if both && w(m, 20) && w(m, 24) {
            m.w32(sp + 28, 0);
        }
        if w(m, 32) && w(m, 36) {
            m.w32(sp + if faster_second { 32 } else { 36 }, 0);
        }
        let mut v = w(m, 24) as u32;
        if w(m, 32) {
            v |= 2;
        }
        if w(m, 28) {
            v |= 4;
        }
        if w(m, 36) {
            v |= 8;
        }
        r.0[0] = v;
        r.call_leaving(c, PURGE, &[top], 0x8004_a5f8);
        let times = c.m.r32(sp + 48);
        r.call_leaving(c, PURGE, &[times], 0x8004_a604);
        r.call_leaving(c, PURGE, &[order], 0x8004_a60c);
        c.r[2] = v;
    })
}

/// `StatChk_SaveTopTime` (STATCHK.CPP, 0x8004a640): each human car's finishing time (one or
/// two players; with more only the solo player's) beating the kind's eighth put in its place:
/// the eight sorted by time (empty ones last), the player's name and car, the records saved;
/// each player's new records found by time and name and flagged (their times cleared in the
/// copy).
pub fn abi_save_top_time(c: &mut Cpu) {
    let (stats, n) = (c.r[4], c.r[5]);
    abi::frame(c, 184, |c, sp| {
        let mut r = Saved::of(c);
        r.0[6] = 0;
        r.0[8] = RECORD_SIZE;
        r.0[1] = 0;
        let m = &mut c.m;
        m.w32(sp + 184, stats);
        m.w16(sp + 96, n as u16);
        m.w32(sp + 104, 0);
        m.w16(sp + 120, 0);
        for k in 0..2 {
            m.w32(sp + 88 + 4 * k, 0);
        }
        r.0[1] = 2;
        r.0[0] = 8 * RECORD_SIZE;
        let top = r.call_leaving(c, RESERVE, &[TOP_RECORDS, 360, 16], 0x8004_a6d4);
        r.0[7] = top;
        let count = lh(&mut c.m, sp + 96);
        r.0[2] = count;
        let times = r.call_leaving(c, RESERVE, &[CAR_TIME, count << 2, 16], 0x8004_a6f8);
        c.m.w32(sp + 132, times);
        let order = r.call_leaving(c, RESERVE, &[CAR_RANK, count << 1, 16], 0x8004_a710);
        c.m.w32(sp + 136, order);
        let temp = r.call_leaving(c, RESERVE, &[RECORDS_NAME, 8 * RECORD_SIZE, 16], 0x8004_a728);
        r.0[1] = 0;
        c.m.w32(sp + 128, temp);
        if count as i32 > 0 {
            order_by_place(&mut c.m, stats, order, count);
            r.0[1] = count;
        }
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_a78c);
        port(c, &r, 0x8004_b2f4, 0x8004_a79c, &[sx(track), top], stattool::abi_get_records);
        let kind = r.call_leaving(c, TRACK_KIND, &[], 0x8004_a7a4);
        c.m.w16(sp + 112, if kind == 2 { 1 } else { 9 });
        r.0[3] = 0;
        if lh(&mut c.m, sp + 96) as i32 > 0 {
            let index = sp + 48;
            r.0[4] = index;
            let first = c.m.r16(sp + 112) as u32;
            r.0[2] = first;
            let kind_top = top + first * RECORD_SIZE;
            r.0[5] = kind_top;
            let mut k = 0u32;
            loop {
                let slot = order.wrapping_add(sx(k) << 1);
                r.0[0] = slot;
                let idx = lh(&mut c.m, slot);
                let car = car_of(c, &r, stat(stats, idx), 0x8004_a824);
                let m = &mut c.m;
                'car: {
                    if cop_car(m, car) {
                        break 'car;
                    }
                    let idx = lh(m, slot);
                    if m.r8(PLAYERS) < 3 || idx == m.r32(SOLO_PLAYER) {
                        let st = stat(stats, idx);
                        if m.r32(st + FLAGS) & HUMAN != 0 && m.r32(st + STATE) == FINISHED {
                            r.0[6] = 1;
                        }
                    }
                    if r.0[6] != 1 {
                        break 'car;
                    }
                    let last = m.r32(kind_top + 7 * RECORD_SIZE + TIME);
                    let time = m.r32(stat(stats, idx) + RACE_TIME);
                    r.0[6] = 0;
                    if !beats(last, last, time) {
                        break 'car;
                    }
                    // in the eighth's place
                    m.w32(sp + 76, time);
                    let id = m.r8(car) as i8 as i32 as u32;
                    m.w32(sp + 72, id);
                    r.0[1] = m.r16(sp + 112) as u32;
                    m.w32(sp + 104, 1);
                    let lap = m.r32(stat(stats, idx) + BEST_LAP);
                    m.w32(sp + 80, lap);
                    let record = |m: &mut Mem| {
                        for f in (0..20).step_by(4) {
                            let v = m.r32(sp + 64 + f);
                            m.w32(kind_top + 7 * RECORD_SIZE + f, v);
                        }
                    };
                    record(m);
                    // sorted
                    let values = sp + 16;
                    let mut j = r.0[1];
                    while (sx(j) as i32) < (first + 8) as i32 {
                        let a = sx(j);
                        let mut v = m.r32(top.wrapping_add(a.wrapping_mul(RECORD_SIZE)) + TIME);
                        if v == 0 {
                            v = a.wrapping_add(1).wrapping_mul(NO_TIME);
                        }
                        m.w32(values.wrapping_add(a.wrapping_sub(first) << 2), v);
                        j = j.wrapping_add(1);
                        r.0[1] = j;
                    }
                    port(c, &r, 0x8004_ad40, 0x8004_aa30, &[8, values, index], stattool::abi_n_create_index);
                    let m = &mut c.m;
                    for j in 0..8 {
                        if lh(m, index + 2 * j) == 7 {
                            m.w16(sp + 120, (j + 1) as u16);
                        }
                    }
                    r.0[1] = 8;
                    for j in 0..8 {
                        r.0[1] = j;
                        let at = lh(m, index + 2 * j);
                        if at == lh(m, slot) {
                            let v = lh(m, sp + 120);
                            m.w32(sp.wrapping_add(at << 2) + 88, v);
                            break;
                        }
                        r.0[1] = j + 1;
                    }
                    let idx = lh(m, slot);
                    r.0[1] = 0;
                    let name = r.call_leaving(c, PLAYER_NAME, &[idx], 0x8004_aae0);
                    r.call_leaving(c, STRCPY, &[sp + 64, name], 0x8004_aaec);
                    record(&mut c.m);
                    r.call_leaving(c, MEMCPY, &[temp, kind_top, 8 * RECORD_SIZE], 0x8004_ab28);
                    for j in 0..8 {
                        r.0[1] = j;
                        let at = lh(&mut c.m, index + 2 * j);
                        let to = top.wrapping_add(first.wrapping_add(j).wrapping_mul(RECORD_SIZE));
                        let from = temp.wrapping_add(at.wrapping_mul(RECORD_SIZE));
                        r.call_leaving(c, MEMCPY, &[to, from, RECORD_SIZE], 0x8004_ab6c);
                    }
                    r.0[1] = 8;
                }
                k = k.wrapping_add(1);
                r.0[3] = k;
                if sx(k) as i32 >= lh(&mut c.m, sp + 96) as i32 {
                    break;
                }
            }
        }
        if c.m.r32(sp + 104) == 1 {
            let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_abc0);
            let to = TRACK_RECORDS.wrapping_add(sx(track).wrapping_mul(RECORDS_SIZE));
            r.call_leaving(c, BLOCKMOVE, &[top, to, RECORDS_SIZE], 0x8004_abf4);
        }
        // the players' new records
        let first = c.m.r16(sp + 112) as u32;
        r.0[8] = first;
        r.0[1] = 0;
        for p in 0..2u32 {
            r.0[1] = p;
            if c.m.r32(sp + 88 + 4 * p) == 0 {
                continue;
            }
            r.0[3] = 1;
            r.0[5] = p;
            let st = stat(stats, p);
            r.0[6] = st;
            r.0[4] = 0;
            for j in 1..9u32 {
                r.0[3] = j;
                let at = first.wrapping_add(j).wrapping_mul(RECORD_SIZE);
                r.0[0] = at;
                let rec = top.wrapping_add(at).wrapping_sub(RECORD_SIZE);
                r.0[2] = rec;
                if c.m.r32(rec + TIME) == c.m.r32(st + RACE_TIME) {
                    let name = r.call_leaving(c, PLAYER_NAME, &[p], 0x8004_ac74);
                    if r.call_leaving(c, STRCMP, &[rec, name], 0x8004_ac84) == 0 {
                        c.m.w32(NEW_RECORDS + r.0[4], 1);
                        c.m.w32(rec + TIME, 0);
                    }
                }
                r.0[4] += 4;
            }
            r.0[3] = 9;
        }
        r.0[1] = 2;
        r.call_leaving(c, PURGE, &[temp], 0x8004_acc4);
        r.call_leaving(c, PURGE, &[top], 0x8004_accc);
        r.call_leaving(c, PURGE, &[times], 0x8004_acd8);
        r.call_leaving(c, PURGE, &[order], 0x8004_ace4);
    })
}

/// `StatChk_ClearNewRecords` (STATCHK.CPP, 0x8004ad14): no new records.
pub fn abi_clear_new_records(c: &mut Cpu) {
    let m = &mut c.m;
    for k in (0..8).rev() {
        m.w32(NEW_RECORDS + 4 * k, 0);
    }
    m.w32(NEW_LAP_RECORD, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};
    use crate::types::CarManager;

    /// The race's stats (`Car_tStats`, 8 cars); outputs, a records buffer, values and an index
    /// to sort, a name.
    const STATS: u32 = 0x8011_0E08;
    const OUTPUT: u32 = 0x8014_7000;
    const RECORDS: u32 = 0x8014_5000;
    const VALUES: u32 = 0x8014_7400;
    const INDEX: u32 = 0x8014_7440;
    const NAME: u32 = 0x8014_7480;
    /// The humans and AIs racing; the front end's mode and the pink slips' chosen track (what
    /// `Front_GetTrackRaced` reads), the language.
    const HUMANS: u32 = 0x8011_486C;
    const AIS: u32 = 0x8011_4870;
    const MODE: u32 = FRONT_END + 4;
    const CHOSEN: u32 = FRONT_END + 0x46;
    const LANGUAGE: u32 = FRONT_END + 0x340;

    /// FEMENU's structures, then: the cars' stats (sim numbers of the game's cars, places a
    /// shuffled order, now and then a human's, barred, out, finished; times and laps, some
    /// none, some a record's), how many race, the players, the solo player, the track raced, the
    /// language; the tracks' records (times and laps, some none, names the players' or not; now
    /// and then the eights sorted with both players between the last two, or one best lap
    /// everywhere and on some cars);
    /// the new records' flags, a records buffer of garbage; values to sort (ties); a name of
    /// letters, accented ones and spaces.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let cm = CarManager(CAR_MANAGER.0);
        let (cars, n) = (cm.cars(m), cm.num_cars(m));
        let mut places: Vec<u32> = (1..9).collect();
        for k in (1..places.len()).rev() {
            places.swap(k, pick(rng, k as u32 + 1) as usize);
        }
        for k in 0..8u32 {
            let st = STATS + STATS_SIZE * k;
            let sim = cars.at(pick(rng, n.max(1))).sim_number(m) as u16;
            m.w16(st + SIM, [sim, pick(rng, 16) as u16][(pick(rng, 4) == 0) as usize]);
            m.w32(st + PLACE, places[k as usize]);
            let human = if k < 2 { pick(rng, 4) != 0 } else { pick(rng, 6) == 0 };
            let barred = if pick(rng, 24) == 0 { NO_RECORDS } else { 0 };
            m.w32(st + FLAGS, (human as u32 * HUMAN) | barred | pick(rng, 2) * 0x10);
            m.w32(st + OUT, (pick(rng, 5) == 0) as u32);
            m.w32(st + STATE, [0, 1, 2, 2, 2][pick(rng, 5) as usize]);
            m.w32(st + RACE_TIME, [0, 1, pick(rng, 100_000), pick(rng, 200_000)][pick(rng, 4) as usize]);
            m.w32(st + BEST_LAP, [0, 1, pick(rng, 20_000), pick(rng, 40_000)][pick(rng, 4) as usize]);
        }
        m.w16(HUMANS, pick(rng, 3) as u16);
        m.w16(AIS, pick(rng, 7) as u16);
        m.w8(PLAYERS, pick(rng, 4) as u8);
        m.w32(SOLO_PLAYER, pick(rng, 3));
        m.w8(MODE, [0, 1, 3][pick(rng, 3) as usize]);
        m.w8(CHOSEN, pick(rng, 4) as u8);
        m.w8(LANGUAGE, pick(rng, 5) as u8);
        let names = [0x8011_5C2C, 0x8011_5C34];
        for k in 0..12 * 17 {
            let rec = TRACK_RECORDS + RECORD_SIZE * k;
            let name = [None, Some(names[0]), Some(names[1])][pick(rng, 3) as usize];
            for j in 0..8 {
                let v = match name {
                    Some(at) => m.r8(at + j),
                    None if j < 7 => b'A' + pick(rng, 26) as u8,
                    None => 0,
                };
                m.w8(rec + j, v);
            }
            m.w32(rec + 8, pick(rng, 30));
            m.w32(rec + TIME, [0, pick(rng, 100_000), pick(rng, 200_000)][pick(rng, 3) as usize]);
            m.w32(rec + LAP, [0, pick(rng, 20_000), pick(rng, 40_000)][pick(rng, 3) as usize]);
        }
        // now and then each kind's eight sorted (1000 apart) and the first two cars human,
        // finished, between the seventh and eighth; every best lap one value, some cars' too
        if pick(rng, 3) == 0 {
            for t in 0..12 {
                for j in 1..17 {
                    m.w32(TRACK_RECORDS + RECORDS_SIZE * t + RECORD_SIZE * j + TIME, 1000 * ((j - 1) % 8 + 1));
                }
            }
            for k in 0..2 {
                let st = STATS + STATS_SIZE * k;
                let v = m.r32(st + FLAGS) | HUMAN;
                m.w32(st + FLAGS, v & !NO_RECORDS);
                m.w32(st + STATE, FINISHED);
                m.w32(st + RACE_TIME, [7000 + pick(rng, 1001), pick(rng, 9000)][pick(rng, 2) as usize]);
            }
        }
        if pick(rng, 3) == 0 {
            let lap = 1 + pick(rng, 20_000);
            for t in 0..12 {
                m.w32(TRACK_RECORDS + RECORDS_SIZE * t + LAP, lap);
            }
            for k in 0..8 {
                if pick(rng, 2) == 0 {
                    m.w32(STATS + STATS_SIZE * k + BEST_LAP, lap);
                }
            }
        }
        // some times and laps a record's
        for k in 0..8u32 {
            let st = STATS + STATS_SIZE * k;
            if pick(rng, 4) == 0 {
                let rec = TRACK_RECORDS + RECORD_SIZE * pick(rng, 12 * 17);
                let (t, l) = (m.r32(rec + TIME), m.r32(rec + LAP));
                m.w32(st + RACE_TIME, t);
                m.w32(st + BEST_LAP, l);
            }
        }
        for k in 0..10 {
            m.w32(VALUES + 4 * k, [0, 5, 5, pick(rng, 100), rng.next() as u32][pick(rng, 5) as usize]);
        }
        for k in 0..9 {
            m.w32(NEW_RECORDS + 4 * k, pick(rng, 2));
        }
        for k in (0..3740).step_by(4) {
            m.w32(RECORDS + k, rng.next() as u32);
        }
        let len = pick(rng, 12);
        for k in 0..12 {
            let ch = [b'a' + pick(rng, 26) as u8, b'A' + pick(rng, 26) as u8, b' ', 192 + pick(rng, 64) as u8, pick(rng, 256) as u8][pick(rng, 5) as usize];
            m.w8(NAME + k, if k < len { ch.max(1) } else { 0 });
        }
    }

    fn negative(r: &mut Rng) -> u32 {
        0u32.wrapping_sub(pick(r, 100_000))
    }

    fn cases() -> Vec<Case> {
        vec![
            ("StatChk_IsRecordLapTime", 0x8004_9cc8, nfs4rc::f_80049cc8, |_, r| vec![STATS, pick(r, 9), OUTPUT]),
            ("StatChk_SaveRecordLapTime", 0x8004_a004, nfs4rc::f_8004a004, |_, r| vec![STATS, 8, pick(r, 8)]),
            ("StatChk_IsTopTime", 0x8004_a194, nfs4rc::f_8004a194, |_, r| vec![STATS, pick(r, 9)]),
            ("StatChk_SaveTopTime", 0x8004_a640, nfs4rc::f_8004a640, |_, r| vec![STATS, pick(r, 9)]),
            ("StatChk_ClearNewRecords", 0x8004_ad14, nfs4rc::f_8004ad14, |_, _| vec![]),
            ("Stattool_nCreateIndex", 0x8004_ad40, nfs4rc::f_8004ad40, |_, r| vec![pick(r, 11), VALUES, INDEX]),
            ("Stattool_ParseTime", 0x8004_ae74, nfs4rc::f_8004ae74, |_, r| {
                vec![[0, 1, pick(r, 1000), pick(r, 500_000), negative(r)][pick(r, 5) as usize], OUTPUT]
            }),
            ("UserNameUpperCaseOneLetter", 0x8004_af8c, nfs4rc::f_8004af8c, |c, r| {
                c.m.w8(NAME, pick(r, 256) as u8);
                vec![NAME]
            }),
            ("Stattool_SamNelsonsUpperLowerStringConverterForRecords", 0x8004_b0ac, nfs4rc::f_8004b0ac, |_, _| vec![NAME]),
            ("StatTool_UpperCaseItKeepingInMindThoseBloodySpecialCharacters", 0x8004_b128, nfs4rc::f_8004b128, |_, _| vec![NAME]),
            ("Stattool_GetAllDefaultRecords", 0x8004_b174, nfs4rc::f_8004b174, |_, r| vec![RECORDS, pick(r, 2)]),
            ("Stattool_ReadDefaultRecords", 0x8004_b26c, nfs4rc::f_8004b26c, |_, r| vec![RECORDS, pick(r, 2)]),
            ("Stattool_GetRecords", 0x8004_b2f4, nfs4rc::f_8004b2f4, |_, r| vec![pick(r, 12), OUTPUT]),
            ("Stattool_ReturnRecordLapTime", 0x8004_b33c, nfs4rc::f_8004b33c, |_, r| vec![pick(r, 12)]),
            ("Stattool_CheckForHumanCar", 0x8004_b3a0, nfs4rc::f_8004b3a0, |_, _| vec![STATS]),
            ("Stattool_GetAINameFromPersonality", 0x8004_b428, nfs4rc::f_8004b428, |_, r| vec![pick(r, 20)]),
        ]
    }

    #[test]
    fn statchk_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x810 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x811 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
