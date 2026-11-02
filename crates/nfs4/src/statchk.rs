//! STATCHK.CPP: the track records checked and saved after a race: the best lap (a human's,
//! the fastest of the cars not of classes 7 and 8, beating the record or the first set), the
//! top eight times (each human car's finish placed into the kind's eight, the sorted names and
//! times saved, the new ones flagged for the records screen), cleared flags.
//!
//! Written as FEMENU.CPP is (see there). The top times keep flags per player on the stack,
//! indexed by the car: only the two players' cars (0 and 1) are ever humans, which is all the
//! rewrite keeps (a human car further on would have the original write over its other locals).

use crate::call::{call, with_buffer};
use crate::fecars;
use crate::globals::{CAR_MANAGER, FRONT_END};
use crate::stattool::{self, BLOCKMOVE, CAR, FLAGS, HUMAN, LAP, MEMCPY, PURGE, RECORD_SIZE, RECORDS_SIZE, RESERVE, STATS_SIZE, STRCPY, TIME, TRACK_RECORDS, lh, stat, sx};
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
/// A track's records: the best lap's, then the top eight of the kind with records of its own
/// (from 1), or of the others (from 9).
const TOP: u32 = 8;
const OWN_KIND: u32 = 2;

/// `Front_GetTrackRaced`, `PlayerName`, the track's kind (`Front_GetLapsForType`), `memset`,
/// `strcmp`.
const TRACK_RACED: fn(&mut Cpu) = crate::front::get_track_raced_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const TRACK_KIND: fn(&mut Cpu) = crate::front::get_laps_for_type_entry;
const STRCMP: fn(&mut Cpu) = nfs4rc::f_800e6968;

/// A car of classes 7 or 8.
fn cop_car(m: &mut Mem, car: u32) -> bool {
    m.r8(car + CLASS).wrapping_sub(7) < 2
}

/// `time` beats `record`, or the last place (`last`) is empty.
fn beats(record: u32, last: u32, time: u32) -> bool {
    record as i32 >= time as i32 || (last == 0 && time as i32 > 0)
}

/// A car's description by its stats' sim number.
fn car_of(m: &mut Mem, st: u32) -> u32 {
    let sim = lh(m, st + SIM);
    fecars::get_car_from_sim_id(m, CAR_MANAGER, sim as i32)
}

/// The track raced's records into `buffer`.
fn track_records(c: &mut Cpu, buffer: u32) {
    let track = call(c, TRACK_RACED, &[]);
    stattool::get_records(c, sx(track), buffer);
}

/// The track raced's records in the game's.
fn save_track_records(c: &mut Cpu, from: u32) {
    let track = call(c, TRACK_RACED, &[]);
    let to = TRACK_RECORDS.wrapping_add(sx(track).wrapping_mul(RECORDS_SIZE));
    call(c, BLOCKMOVE, &[from, to, RECORDS_SIZE]);
}

fn purge(c: &mut Cpu, blocks: &[u32]) {
    for &at in blocks {
        call(c, PURGE, &[at]);
    }
}

/// The first of the kind's eight records (by the track's kind).
fn first_top(c: &mut Cpu) -> u32 {
    if sx(call(c, TRACK_KIND, &[])) == OWN_KIND { 1 } else { 9 }
}

/// `StatChk_IsRecordLapTime` (STATCHK.CPP, 0x80049cc8): with a human racing (and no car
/// barred), the cars ranked by best lap (cop cars last); the first human one with a lap (as the
/// original indexes it: the lap of the car the human's rank names) checked against the track's
/// record: whether it beats it (or there is none, or no cars), the car in `*car`.
pub fn is_record_lap_time(c: &mut Cpu, stats: u32, n: u32, car_out: u32) -> bool {
    if !stattool::check_for_human_car(&mut c.m, stats) {
        return false;
    }
    let n = sx(n);
    let laps = call(c, RESERVE, &[RANK_LAP, n << 2, 16]);
    let index = call(c, RESERVE, &[RANK_BEST, n << 1, 16]);
    let records = call(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16]);
    for k in 0..(n as i32).max(0) as u32 {
        let st = stat(stats, k);
        let car = car_of(&mut c.m, st);
        let m = &mut c.m;
        if m.r32(st + FLAGS) & NO_RECORDS != 0 {
            purge(c, &[laps, index, records]);
            return false;
        }
        let v = if cop_car(m, car) { k.wrapping_add(1).wrapping_mul(NO_TIME) } else { m.r32(st + BEST_LAP) };
        m.w32(laps.wrapping_add(k << 2), v);
    }
    if (n as i32) < 2 {
        c.m.w16(index, 0);
    } else {
        stattool::n_create_index(c, n, laps, index);
    }
    // the first human car with a lap (none checked: the first)
    let m = &mut c.m;
    let mut found = Some(0);
    if n as i32 > 0 {
        found = (0..n).find(|&k| {
            let j = lh(m, index.wrapping_add(k << 1));
            if m.r32(stat(stats, j) + FLAGS) & HUMAN == 0 {
                return false;
            }
            let jj = lh(m, index.wrapping_add(j << 1));
            m.r32(stat(stats, jj) + BEST_LAP) as i32 > 0
        });
    }
    let Some(k) = found else {
        purge(c, &[laps, index, records]);
        return false;
    };
    let car = c.m.r16(index.wrapping_add(k << 1)) as u32;
    track_records(c, records);
    let record = with_buffer(c, RECORD_SIZE, |c, best| {
        call(c, MEMCPY, &[best, records, RECORD_SIZE]);
        c.m.r32(best + LAP)
    });
    let m = &mut c.m;
    let lap = m.r32(stat(stats, car) + BEST_LAP);
    let beaten = (record as i32 >= lap as i32 || record == 0) && lap as i32 > 0;
    m.w16(car_out, car as u16);
    purge(c, &[laps, index, records]);
    beaten
}

pub fn abi_is_record_lap_time(c: &mut Cpu) {
    c.r[2] = is_record_lap_time(c, c.r[4], c.r[5], c.r[6]) as u32;
}

/// `StatChk_SaveRecordLapTime` (STATCHK.CPP, 0x8004a004): car `k`'s best lap (not a cop car's)
/// saved as the track's record: the player's name, the car, its time (finished) and lap; a new
/// lap record flagged.
pub fn save_record_lap_time(c: &mut Cpu, stats: u32, k: u32) {
    let st = stat(stats, k);
    let car = car_of(&mut c.m, st);
    if cop_car(&mut c.m, car) {
        return;
    }
    let records = call(c, RESERVE, &[TRACK_RECORDS_NAME, 360, 16]);
    track_records(c, records);
    with_buffer(c, RECORD_SIZE, |c, record| {
        let m = &mut c.m;
        let time = if m.r32(st + OUT) == 0 && m.r32(st + STATE) == FINISHED { m.r32(st + RACE_TIME) } else { 0 };
        m.w32(record + TIME, time);
        let lap = m.r32(stat(stats, k) + BEST_LAP);
        m.w32(record + LAP, lap);
        let id = m.r8(car) as i8 as i32 as u32;
        m.w32(record + CAR, id);
        let name = call(c, PLAYER_NAME, &[sx(k)]);
        call(c, STRCPY, &[record, name]);
        call(c, MEMCPY, &[records, record, RECORD_SIZE]);
    });
    save_track_records(c, records);
    c.m.w32(NEW_LAP_RECORD, 1);
    call(c, PURGE, &[records]);
}

pub fn abi_save_record_lap_time(c: &mut Cpu) {
    save_record_lap_time(c, c.r[4], c.r[6]);
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
/// and beat its first (bits 1 and 3); of two players both in, only the faster counts (and of
/// one in last place, the other). Nothing when a car is barred from records.
pub fn is_top_time(c: &mut Cpu, stats: u32, n: u32) -> u32 {
    let first = first_top(c);
    let top = call(c, RESERVE, &[TOP_RECORDS, 360, 16]);
    let count = sx(n);
    let times = call(c, RESERVE, &[CAR_TIME, count << 2, 16]);
    let order = call(c, RESERVE, &[CAR_RANK, count << 1, 16]);
    if count as i32 > 0 {
        order_by_place(&mut c.m, stats, order, count);
    }
    track_records(c, top);
    let (kind_top, last) = (top + first * RECORD_SIZE, top + (first + TOP - 1) * RECORD_SIZE);
    // per player: in last place, in the eight, before the first
    let (mut eighth, mut in_top, mut beats_first) = ([false; 2], [false; 2], [false; 2]);
    for k in 0..(count as i32).max(0) as u32 {
        let car = car_of(&mut c.m, stat(stats, k));
        let m = &mut c.m;
        let idx = lh(m, order.wrapping_add(k << 1));
        let st = stat(stats, idx);
        let flags = m.r32(st + FLAGS);
        if flags & NO_RECORDS != 0 {
            purge(c, &[top, times, order]);
            return 0;
        }
        if cop_car(m, car) || !(m.r8(PLAYERS) < 3 && flags & HUMAN != 0 && m.r32(st + STATE) == FINISHED) {
            continue;
        }
        let p = idx as usize;
        assert!(p < 2, "IsTopTime: a human car past the players' (the original writes over its locals)");
        let time = m.r32(st + RACE_TIME);
        let last_time = m.r32(last + TIME);
        if beats(m.r32(kind_top + 6 * RECORD_SIZE + TIME), last_time, time) {
            in_top[p] = true;
        } else if beats(last_time, last_time, time) {
            in_top[p] = true;
            eighth[p] = true;
        }
        if beats(m.r32(kind_top + TIME), m.r32(last + TIME), time) {
            in_top[p] = true;
            beats_first[p] = true;
        }
    }
    // of two players both in, the faster
    let m = &mut c.m;
    let second_faster = (m.r32(stats + STATS_SIZE + RACE_TIME) as i32) < m.r32(stats + RACE_TIME) as i32;
    let slower = if second_faster { 0 } else { 1 };
    let mut both = true;
    if eighth[0] {
        if eighth[1] {
            both = false;
            in_top[slower] = false;
        } else if in_top[1] {
            both = false;
            in_top[0] = false;
        }
    }
    if both && eighth[1] && in_top[0] {
        in_top[1] = false;
    }
    if beats_first[0] && beats_first[1] {
        beats_first[slower] = false;
    }
    let v = in_top[0] as u32 | (beats_first[0] as u32) << 1 | (in_top[1] as u32) << 2 | (beats_first[1] as u32) << 3;
    purge(c, &[top, times, order]);
    v
}

pub fn abi_is_top_time(c: &mut Cpu) {
    c.r[2] = is_top_time(c, c.r[4], c.r[5]);
}

/// `StatChk_SaveTopTime` (STATCHK.CPP, 0x8004a640): each human car's finishing time (one or
/// two players; with more only the solo player's) beating the kind's eighth put in its place:
/// the eight sorted by time (empty ones last), the player's name and car, the records saved;
/// each player's new records found by time and name and flagged (their times cleared in the
/// copy).
pub fn save_top_time(c: &mut Cpu, stats: u32, n: u32) {
    let top = call(c, RESERVE, &[TOP_RECORDS, 360, 16]);
    let count = sx(n);
    let times = call(c, RESERVE, &[CAR_TIME, count << 2, 16]);
    let order = call(c, RESERVE, &[CAR_RANK, count << 1, 16]);
    let temp = call(c, RESERVE, &[RECORDS_NAME, TOP * RECORD_SIZE, 16]);
    if count as i32 > 0 {
        order_by_place(&mut c.m, stats, order, count);
    }
    track_records(c, top);
    let first = first_top(c);
    let kind_top = top + first * RECORD_SIZE;
    let eighth = kind_top + (TOP - 1) * RECORD_SIZE;
    // the place each player's time took (from 1), and the last one found (kept across cars)
    let mut new_place = [0u32; 2];
    let mut place = 0u32;
    let mut saved = false;
    with_buffer(c, 32 + 16 + RECORD_SIZE, |c, buffer| {
        let (values, index, record) = (buffer, buffer + 32, buffer + 48);
        for k in 0..(count as i32).max(0) as u32 {
            let slot = order.wrapping_add(k << 1);
            let idx = lh(&mut c.m, slot);
            let car = car_of(&mut c.m, stat(stats, idx));
            let m = &mut c.m;
            if cop_car(m, car) {
                continue;
            }
            let idx = lh(m, slot);
            let st = stat(stats, idx);
            let racing = m.r8(PLAYERS) < 3 || idx == m.r32(SOLO_PLAYER);
            if !(racing && m.r32(st + FLAGS) & HUMAN != 0 && m.r32(st + STATE) == FINISHED) {
                continue;
            }
            let last = m.r32(eighth + TIME);
            let time = m.r32(st + RACE_TIME);
            if !beats(last, last, time) {
                continue;
            }
            // in the eighth's place, then the eight sorted (empty ones last by their place)
            let id = m.r8(car) as i8 as i32 as u32;
            m.w32(record + CAR, id);
            m.w32(record + TIME, time);
            saved = true;
            let lap = m.r32(st + BEST_LAP);
            m.w32(record + LAP, lap);
            for f in [CAR, TIME, LAP] {
                let v = m.r32(record + f);
                m.w32(eighth + f, v);
            }
            for j in 0..TOP {
                let mut v = m.r32(top.wrapping_add((first + j).wrapping_mul(RECORD_SIZE)) + TIME);
                if v == 0 {
                    v = (first + j).wrapping_add(1).wrapping_mul(NO_TIME);
                }
                m.w32(values + 4 * j, v);
            }
            stattool::n_create_index(c, TOP, values, index);
            let m = &mut c.m;
            for j in 0..TOP {
                if lh(m, index + 2 * j) == TOP - 1 {
                    place = j + 1;
                }
            }
            // (the original looks for the car among the sorted places)
            if let Some(j) = (0..TOP).find(|&j| lh(m, index + 2 * j) == lh(m, slot)) {
                let p = lh(m, index + 2 * j) as usize;
                assert!(p < 2, "SaveTopTime: a human car past the players' (the original writes over its locals)");
                new_place[p] = place;
            }
            let idx = lh(&mut c.m, slot);
            let name = call(c, PLAYER_NAME, &[idx]);
            call(c, STRCPY, &[record, name]);
            let m = &mut c.m;
            for f in (0..RECORD_SIZE).step_by(4) {
                let v = m.r32(record + f);
                m.w32(eighth + f, v);
            }
            call(c, MEMCPY, &[temp, kind_top, TOP * RECORD_SIZE]);
            for j in 0..TOP {
                let at = lh(&mut c.m, index + 2 * j);
                let to = top.wrapping_add(first.wrapping_add(j).wrapping_mul(RECORD_SIZE));
                call(c, MEMCPY, &[to, temp.wrapping_add(at.wrapping_mul(RECORD_SIZE)), RECORD_SIZE]);
            }
        }
    });
    if saved {
        save_track_records(c, top);
    }
    // the players' new records, by time and name
    for p in 0..2u32 {
        if new_place[p as usize] == 0 {
            continue;
        }
        let st = stat(stats, p);
        for j in 0..TOP {
            let rec = top.wrapping_add((first + j).wrapping_mul(RECORD_SIZE));
            if c.m.r32(rec + TIME) != c.m.r32(st + RACE_TIME) {
                continue;
            }
            let name = call(c, PLAYER_NAME, &[p]);
            if call(c, STRCMP, &[rec, name]) == 0 {
                c.m.w32(NEW_RECORDS + 4 * j, 1);
                c.m.w32(rec + TIME, 0);
            }
        }
    }
    purge(c, &[temp, top, times, order]);
}

pub fn abi_save_top_time(c: &mut Cpu) {
    save_top_time(c, c.r[4], c.r[5]);
}

/// `StatChk_ClearNewRecords` (STATCHK.CPP, 0x8004ad14): no new records.
pub fn clear_new_records(m: &mut Mem) {
    for k in (0..8).rev() {
        m.w32(NEW_RECORDS + 4 * k, 0);
    }
    m.w32(NEW_LAP_RECORD, 0);
}

pub fn abi_clear_new_records(c: &mut Cpu) {
    clear_new_records(&mut c.m);
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
            let human = k < 2 && pick(rng, 4) != 0;
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
                m.w32(st + RACE_TIME, [7000 + pick(rng, 1001), pick(rng, 9000), 1 + pick(rng, 1000)][pick(rng, 3) as usize]);
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
            // (the record's car is a human's, 0 or 1: past them `PlayerName` prints the fuzzed
            // bytes after the names as `sprintf`'s format, reading leftover registers)
            ("StatChk_SaveRecordLapTime", 0x8004_a004, nfs4rc::f_8004a004, |_, r| vec![STATS, 8, pick(r, 2)]),
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
