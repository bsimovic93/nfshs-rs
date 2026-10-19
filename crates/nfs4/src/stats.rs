//! STATS.CPP: the race's standings. Each car's laps, lap times and top speeds as it goes, the
//! order of the cars on the road (`Stats_racePosition`, rebuilt every frame), the catch-up glue
//! by the gap to the leader, the checkpoint gaps on the HUD, and the end of the race with the
//! finishing times of the cars that had not finished yet.

use crate::abi;
use crate::fixed::div;
use crate::globals::{
    AI_RACE_CARS, COPSPEAK_QUEUE_HEAD, COPSPEAK_QUEUE_PLAY, COPSPEAK_SPCH_HANDLE, GAME_SETUP, HUMAN_RACE_CARS, NUM_AI_RACE_CARS,
    NUM_HUMAN_RACE_CARS, NUM_RACE_CARS, NUM_SLICES, RACE_CARS, RACE_POSITION, SIM_COUNTER, SIM_END_GAME, TOP_SPEED_CAP,
};
use crate::types::{Car, CarStats};
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`: a human player's car.
const CAR_HUMAN: i32 = 0x4;
/// `Car_tObj.carFlags`: the car's top speed is capped (`Cars_topSpeedCap`).
const CAR_TOP_SPEED_CAPPED: i32 = 0x8;
/// `Car_tObj.carFlags`: the player gave up the race (a time trial's best times don't count).
const CAR_QUIT: i32 = 0x200;
/// `Car_tStats.finishType`: finished (by crossing the line, or given a time at the end).
const FINISHED: i32 = 2;
/// The lap time of a race that doesn't count.
const NO_TIME: i32 = 99999;
/// `Stats_tPosition` entries: the cars on the road, first to last.
const POSITIONS: u32 = 6;

fn race_car(m: &mut Mem, i: i32) -> Car {
    Car(m.r32(RACE_CARS.wrapping_add((i as u32) << 2)))
}

fn human_car(m: &mut Mem, i: i32) -> Car {
    Car(m.r32(HUMAN_RACE_CARS.wrapping_add((i as u32) << 2)))
}

fn ai_race_car(m: &mut Mem, i: i32) -> Car {
    Car(m.r32(AI_RACE_CARS.wrapping_add((i as u32) << 2)))
}

/// `Stats_tPosition` `i`: `car` (its index), `slice` (slices driven), `sliceTime` (when it got
/// there), `isHuman`.
fn position(i: i32) -> u32 {
    RACE_POSITION.wrapping_add((i as u32).wrapping_mul(16))
}

/// The address of `stats.time[lap]` and `stats.finalLapTime[lap]`: the original indexes them
/// by the lap without a bound (races of more than 4 laps write past them).
fn lap_time_at(stats: CarStats, lap: i32) -> u32 {
    stats.0.wrapping_add(0x1c).wrapping_add((lap as u32) << 2)
}

fn final_lap_time_at(stats: CarStats, lap: i32) -> u32 {
    stats.0.wrapping_add(0x8c).wrapping_add((lap as u32) << 2)
}

/// The address of `stats.topSpeed[lap]` (the lap is only checked to be under 4).
fn top_speed_at(stats: CarStats, lap: i32) -> u32 {
    stats.0.wrapping_add(0x2c).wrapping_add((lap as u32) << 2)
}

/// A human player quit a time trial or a knockout (race types 1 and 5): the times don't count.
fn human_quit_time_trial(m: &mut Mem) -> bool {
    let race_type = GAME_SETUP.race_type(m);
    if race_type != 1 && race_type != 5 {
        return false;
    }
    human_car(m, 0).car_flags(m) & CAR_QUIT != 0 || (NUM_HUMAN_RACE_CARS.get(m) == 2 && human_car(m, 1).car_flags(m) & CAR_QUIT != 0)
}

/// `Stats_DoPlayerGlue` (STATS.CPP, 0x800b8c2c): each car's place, and with catch-up on in a
/// two-player race, the humans' glue by how far they are behind the leading human (100% within
/// 5 slices, up to 110% from 60 on); 100% for both once one has finished.
pub fn do_player_glue(c: &mut Cpu) {
    let m = &mut c.m;
    let mut leader = 99;
    let mut i = 0;
    while i < NUM_RACE_CARS.get(m) {
        let car = race_car(m, i);
        let place = get_position(m, car);
        car.stats().set_position(m, place);
        if m.r32(position(i) + 12) != 0 && leader == 99 {
            leader = i;
        }
        i += 1;
    }
    if GAME_SETUP.catchup_logic(m) == 0 || GAME_SETUP.comm_mode(m) != 1 {
        return;
    }
    let first = human_car(m, 0);
    if first.stats().finish_type(m) == FINISHED || human_car(m, 1).stats().finish_type(m) == FINISHED {
        first.set_glue(m, 0x1_0000);
        human_car(m, 1).set_glue(m, 0x1_0000);
        return;
    }
    let leader_slices = position(leader) + 4;
    let mut i = 0;
    while i < NUM_HUMAN_RACE_CARS.get(m) {
        let car = human_car(m, i);
        let behind = (m.r32(leader_slices) as i32).wrapping_sub(car.stats().slice_total(m));
        let glue = match behind {
            ..5 => 0x1_0000,
            5..10 => 0x1_0666,
            10..30 => 0x1_0CCC,
            30..60 => 0x1_1333,
            _ => 0x1_1999,
        };
        car.set_glue(m, glue);
        i += 1;
    }
}

/// `Stats_ClearPosition` (STATS.CPP, 0x800b8de8): empty the order of the cars before each car
/// puts itself in (`Stats_TrackStats`).
pub fn clear_position(m: &mut Mem) {
    for i in 0..POSITIONS as i32 {
        let at = position(i);
        m.w32(at, -1i32 as u32);
        m.w32(at + 4, -NO_TIME as u32);
        m.w32(at + 8, 0);
        m.w32(at + 12, 0);
    }
}

/// `Stats_GetPosition` (STATS.CPP, 0x800b8e28): the car's place (from 1), or 0 if it isn't in
/// the order.
pub fn get_position(m: &mut Mem, car: Car) -> i32 {
    let index = car.car_index(m);
    (0..NUM_RACE_CARS.get(m)).find(|&i| m.r32(position(i)) as i32 == index).map_or(0, |i| i + 1)
}

/// `Stats_GetNumOpponents` (STATS.CPP, 0x800b8e7c): how many cars are in the order.
pub fn get_num_opponents(m: &mut Mem) -> i32 {
    (0..NUM_RACE_CARS.get(m)).filter(|&i| m.r32(position(i)) as i32 != -1).count() as i32
}

/// `Stats_TrackStats` (STATS.CPP, 0x800b8ed0, every other tick per race car): the lap's top
/// speed (a capped car's a little under its cap), the lap time when a lap is done and the
/// finish on the last, the slices driven, and the car's place in the order.
pub fn track_stats(c: &mut Cpu, car: Car) {
    let now = SIM_COUNTER.get(&mut c.m);
    if now & 1 != 0 {
        return;
    }
    let slices = NUM_SLICES.get(&mut c.m);
    let stats = car.stats();
    let lap = stats.lap(&mut c.m);
    if lap < 4 {
        let speed = car.linear_vel_ch().z(&mut c.m);
        if (c.m.r32(top_speed_at(stats, lap)) as i32) < speed {
            let car_type = car.car_info(&mut c.m).car_type(&mut c.m);
            let top = if car.car_flags(&mut c.m) & CAR_TOP_SPEED_CAPPED != 0 && car_type < 19 {
                let cap = |c: &mut Cpu| {
                    let r = abi::rand(c) as i32;
                    let car_type = car.car_info(&mut c.m).car_type(&mut c.m);
                    let cap = c.m.r32(TOP_SPEED_CAP.wrapping_add((car_type as u32) << 2)) as i32;
                    cap.wrapping_sub(r.wrapping_mul(3))
                };
                if cap(c) < car.linear_vel_ch().z(&mut c.m) { cap(c) } else { car.linear_vel_ch().z(&mut c.m) }
            } else {
                car.linear_vel_ch().z(&mut c.m)
            };
            c.m.w32(top_speed_at(stats, lap), top as u32);
        }
    }
    let m = &mut c.m;
    let lap = stats.lap(m);
    if lap != car.lap(m) && stats.finish_type(m) == 0 {
        let time = if human_quit_time_trial(m) { NO_TIME } else { now.wrapping_sub(stats.lap_time(m)) };
        let at = lap_time_at(stats, stats.lap(m));
        m.w32(at, time as u32);
        stats.set_lap_time(m, now);
        let lap = car.lap(m);
        stats.set_lap(m, lap);
        if lap == GAME_SETUP.num_laps(m) && !human_quit_time_trial(m) {
            let total = stats.lap(m).wrapping_add(1).wrapping_mul(slices);
            stats.set_slice_total(m, total);
            stats.set_slice_time(m, now);
            stats.set_finish_type(m, FINISHED);
            stats.update_lap_time(m, |t| t.wrapping_sub(512));
        }
    }
    let slice = car.n().sim_road_info().slice(m) as i32;
    let slice = if GAME_SETUP.reverse_track(m) != 0 { slices.wrapping_sub(slice).wrapping_sub(1) } else { slice };
    if stats.slice(m) != slice {
        stats.set_slice(m, slice);
        if stats.finish_type(m) == 0 {
            stats.set_slice_time(m, now);
            let laps = stats.lap(m).wrapping_sub(car.unlap(m));
            let total = laps.wrapping_mul(slices).wrapping_add(stats.slice(m));
            stats.set_slice_total(m, total);
        }
    }
    // the order: in before the first car behind (fewer slices, or as many reached later)
    let n = NUM_RACE_CARS.get(m);
    for i in 0..n {
        let at = position(i);
        let (total, other) = (stats.slice_total(m), m.r32(at + 4) as i32);
        if !(other < total || (total == other && stats.slice_time(m) < m.r32(at + 8) as i32)) {
            continue;
        }
        let mut j = n - 2;
        while j >= i {
            for k in 0..4 {
                let v = m.r32(position(j) + 4 * k);
                m.w32(position(j + 1) + 4 * k, v);
            }
            j -= 1;
        }
        let entry = [car.car_index(m), stats.slice_total(m), stats.slice_time(m), car.car_flags(m) & CAR_HUMAN];
        for (k, v) in entry.into_iter().enumerate() {
            m.w32(at + 4 * k as u32, v as u32);
        }
        break;
    }
}

/// The race's end for the cars still driving (`Stats_ExtrapolateOpponentTimes`'s estimate):
/// the time they would take for the whole race at their pace so far.
fn extrapolated_time(m: &mut Mem, car: Car) -> i32 {
    let stats = car.stats();
    let driven = stats.slice_total(m);
    let driven = if driven > 0 { driven } else { 1 };
    let time = if driven < 100 { driven.wrapping_mul(13) } else { stats.slice_time(m).wrapping_sub(512) };
    let length = NUM_SLICES.get(m).wrapping_mul(GAME_SETUP.num_laps(m)).wrapping_add(stats.extract_slice(m));
    div(time.wrapping_mul(length), driven)
}

/// The final lap times, and the best of them that isn't 0 if better than the best so far.
fn copy_final_lap_times(m: &mut Mem, stats: CarStats) {
    let mut lap = 0;
    while lap < GAME_SETUP.num_laps(m) {
        let time = m.r32(lap_time_at(stats, lap));
        m.w32(final_lap_time_at(stats, lap), time);
        let time = m.r32(final_lap_time_at(stats, lap)) as i32;
        if time < stats.final_best_lap(m) && time != 0 {
            stats.set_final_best_lap(m, time);
        }
        lap += 1;
    }
}

/// `Stats_ExtrapolateOpponentTimes` (STATS.CPP, 0x800b92d8): the race is over (`how` 2) or
/// was left (1): the final standings. The humans still driving get the time their pace so far
/// gives (twice it) and the finish type 1 (in a two-player race only the local player); the
/// AI cars still driving their pace's time too, plus a random bit while the first human still
/// drives, and a best lap of at most their average lap less a random bit; then the places by
/// total time.
pub fn extrapolate_opponent_times(c: &mut Cpu, how: i32) {
    let mut i = 0;
    while i < NUM_HUMAN_RACE_CARS.get(&mut c.m) {
        let m = &mut c.m;
        let car = human_car(m, i);
        let stats = car.stats();
        if stats.finish_type(m) != FINISHED {
            let time = extrapolated_time(m, car);
            stats.set_lap_time(m, time.wrapping_mul(2));
            if GAME_SETUP.race_type(m) != 2 || GAME_SETUP.local_car(m) == i {
                stats.set_finish_type(m, 1);
            }
        }
        if how == 1 {
            let race_type = GAME_SETUP.race_type(m);
            if race_type != 2 {
                if stats.finish_type(m) != FINISHED {
                    stats.set_finish_type(m, how);
                }
            } else if stats.finish_type(m) != race_type && GAME_SETUP.local_car(m) == i {
                stats.set_finish_type(m, how);
            }
            let place = get_position(m, car);
            stats.set_final_position(m, place);
            let time = stats.lap_time(m);
            stats.set_final_total_time(m, time);
            let mut lap = 0;
            while lap < GAME_SETUP.num_laps(m) {
                let time = m.r32(lap_time_at(stats, lap));
                m.w32(final_lap_time_at(stats, lap), time);
                lap += 1;
            }
        }
        let place = get_position(m, car);
        stats.set_final_position(m, place);
        let time = stats.lap_time(m);
        stats.set_final_total_time(m, time);
        let v = stats.finish_type(m);
        stats.set_final_finish_type(m, v);
        let v = stats.time(m, 0);
        stats.set_final_best_lap(m, v);
        let v = stats.num_warnings(m);
        stats.set_final_num_warnings(m, v);
        let v = stats.num_fines(m);
        stats.set_final_num_fines(m, v);
        let v = stats.num_arrests(m);
        stats.set_final_num_arrests(m, v);
        stats.set_final_damage(m, 0);
        for k in 0..10 {
            let d = car.n().damage(m, k);
            stats.update_final_damage(m, |v| v.wrapping_add(d));
        }
        copy_final_lap_times(m, stats);
        i += 1;
    }

    let mut i = 0;
    while i < NUM_AI_RACE_CARS.get(&mut c.m) {
        let car = ai_race_car(&mut c.m, i);
        let stats = car.stats();
        if stats.finish_type(&mut c.m) != FINISHED {
            let time = extrapolated_time(&mut c.m, car);
            let time = if human_car(&mut c.m, 0).stats().finish_type(&mut c.m) != FINISHED {
                let r = abi::rand(c) as i32;
                let extra = GAME_SETUP.num_laps(&mut c.m).wrapping_mul(r);
                time.wrapping_add(extra / 128)
            } else {
                time
            };
            stats.set_lap_time(&mut c.m, time);
            stats.set_finish_type(&mut c.m, FINISHED);
        }
        let m = &mut c.m;
        let place = get_position(m, car);
        stats.set_final_position(m, place);
        let time = stats.lap_time(m);
        stats.set_final_total_time(m, time);
        let v = stats.finish_type(m);
        stats.set_final_finish_type(m, v);
        let v = stats.num_arrests(m);
        stats.set_final_num_arrests(m, v);
        stats.set_final_best_lap(m, NO_TIME);
        let mut lap = 0;
        while lap < GAME_SETUP.num_laps(m) {
            let time = m.r32(lap_time_at(stats, lap)) as i32;
            if time < stats.final_best_lap(m) && time > 0 {
                stats.set_final_best_lap(m, time);
            }
            lap += 1;
        }
        let r = abi::rand(c) as i32;
        let m = &mut c.m;
        let average = div(stats.final_total_time(m), GAME_SETUP.num_laps(m));
        let best = average.wrapping_sub(r / 48);
        if best < stats.final_best_lap(m) {
            stats.set_final_best_lap(m, best);
        }
        copy_final_lap_times(m, stats);
        i += 1;
    }

    let m = &mut c.m;
    let n = NUM_RACE_CARS.get(m);
    for i in 0..n {
        let mut place = 1;
        for j in 0..n {
            if i == j {
                continue;
            }
            let (mine, theirs) = (race_car(m, i).stats().final_total_time(m), race_car(m, j).stats().final_total_time(m));
            if theirs < mine || (mine == theirs && j < i) {
                place += 1;
            }
        }
        race_car(m, i).stats().set_final_position(m, place);
    }
}

/// `Stats_TrackEndGame` (STATS.CPP, 0x800b9a80): every 64 ticks, each human's gap in slices
/// to the car a place ahead (the second car when leading, the leader in a checkpoint race of
/// type 1), and that gap in time at the speed of the leader or the human (gap * 384 / speed)
/// for the HUD; and the race's end, when every human finished 320 ticks ago (640 while the cop
/// still talks).
pub fn track_end_game(c: &mut Cpu) {
    abi::frame(c, 72, |c, sp| {
        let m = &mut c.m;
        m.w32(sp + 16, 0);
        let now = SIM_COUNTER.get(m);
        if NUM_RACE_CARS.get(m) >= 2 && now / 64 * 64 == now.wrapping_sub(1) {
            let length = GAME_SETUP.num_laps(m).wrapping_mul(NUM_SLICES.get(m));
            let mut i = 0;
            while i < NUM_HUMAN_RACE_CARS.get(m) {
                let human = human_car(m, i);
                let mine = human.stats().slice_total(m).min(length);
                m.w32(sp + 20, mine as u32);
                let place = get_position(m, human);
                let target = if place == 1 {
                    2
                } else if GAME_SETUP.checkpoint_type(m) == 1 {
                    1
                } else {
                    place - 1
                };
                let (mut theirs, mut speed) = (0, 0);
                let mut j = 0;
                while j < NUM_RACE_CARS.get(m) {
                    let car = race_car(m, j);
                    if get_position(m, car) == target {
                        theirs = car.stats().slice_total(m).min(length);
                        speed = if place == 1 {
                            car.linear_vel_ch().z(m).wrapping_abs() >> 16
                        } else {
                            m.r16(human.linear_vel_ch().0 + 10) as i16 as i32
                        };
                        break;
                    }
                    j += 1;
                }
                let gap = theirs.wrapping_sub(mine);
                let stats = human.stats();
                stats.set_checkpoint_update(m, gap);
                if speed >= 16 && stats.finish_type(m) != FINISHED {
                    stats.set_checkpoint_difference(m, div(gap.wrapping_mul(3) << 7, speed));
                    stats.set_checkpoint_display(m, 1);
                } else {
                    stats.set_checkpoint_difference(m, 0);
                    stats.set_checkpoint_display(m, 0);
                }
                i += 1;
            }
        }
        if SIM_END_GAME.get(m) != 0 {
            return;
        }
        let (play, head, handle) = (COPSPEAK_QUEUE_PLAY.get(m), COPSPEAK_QUEUE_HEAD.get(m), COPSPEAK_SPCH_HANDLE.get(m));
        let humans = NUM_HUMAN_RACE_CARS.get(m);
        let mut done = 0;
        for i in 0..humans {
            let stats = human_car(m, i).stats();
            if stats.finish_type(m) < FINISHED {
                continue;
            }
            let at = stats.slice_time(m);
            let now = SIM_COUNTER.get(m);
            let quiet = play == head && handle == -1;
            if at.wrapping_add(320) < now && (quiet || at.wrapping_add(640) < now) {
                done += 1;
                m.w32(sp + 16, done as u32);
            }
        }
        if done == NUM_HUMAN_RACE_CARS.get(m) {
            SIM_END_GAME.set(m, 1);
            extrapolate_opponent_times(c, 2);
        }
    })
}

pub fn abi_do_player_glue(c: &mut Cpu) {
    do_player_glue(c);
}

pub fn abi_clear_position(c: &mut Cpu) {
    clear_position(&mut c.m);
}

pub fn abi_get_position(c: &mut Cpu) {
    c.r[2] = get_position(&mut c.m, Car(c.r[4])) as u32;
}

pub fn abi_get_num_opponents(c: &mut Cpu) {
    c.r[2] = get_num_opponents(&mut c.m) as u32;
}

pub fn abi_track_stats(c: &mut Cpu) {
    track_stats(c, Car(c.r[4]));
}

pub fn abi_extrapolate_opponent_times(c: &mut Cpu) {
    let how = c.r[4] as i32;
    extrapolate_opponent_times(c, how);
}

pub fn abi_track_end_game(c: &mut Cpu) {
    track_end_game(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::Rng;

    /// The race's RAM with the standings shuffled: each car's laps, slices, times, finish
    /// and speed; the race's laps, type, catch-up, direction; one human or two (the second an
    /// AI car taken over); the order table as it was or rebuilt partly; the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let cars = NUM_RACE_CARS.get(m);
        let laps = 1 + pick(8);
        GAME_SETUP.set_num_laps(m, laps);
        GAME_SETUP.set_race_type(m, [0, 0, 1, 2, 5, 3][pick(6) as usize]);
        GAME_SETUP.set_catchup_logic(m, pick(2));
        GAME_SETUP.set_comm_mode(m, pick(3));
        GAME_SETUP.set_reverse_track(m, pick(2));
        GAME_SETUP.set_checkpoint_type(m, pick(3));
        GAME_SETUP.set_local_car(m, pick(2));
        if pick(2) == 0 {
            NUM_HUMAN_RACE_CARS.set(m, 2);
            let second = m.r32(RACE_CARS + 4);
            m.w32(HUMAN_RACE_CARS + 4, second);
        }
        let slices = NUM_SLICES.get(m);
        for i in 0..cars {
            let car = race_car(m, i);
            let s = car.stats();
            let lap = pick(laps as u32 + 1);
            s.set_lap(m, lap);
            car.set_lap(m, if pick(3) == 0 { lap + 1 } else { lap });
            car.set_unlap(m, pick(4) / 3);
            s.set_slice_total(m, [pick(100), lap * slices + pick(slices as u32), -pick(3), laps * slices + 5][pick(4) as usize]);
            s.set_slice_time(m, pick(20_000));
            s.set_slice(m, pick(slices as u32));
            s.set_lap_time(m, pick(20_000));
            s.set_extract_slice(m, pick(50));
            s.set_finish_type(m, [0, 0, 0, 1, 2][pick(5) as usize]);
            for k in 0..4 {
                s.set_top_speed(m, k, pick(0x60_0000));
            }
            for k in 0..laps {
                m.w32(lap_time_at(s, k), [0, pick(10_000) as u32][pick(2) as usize]);
            }
            s.set_num_warnings(m, pick(3));
            car.linear_vel_ch().set_z(m, pick(0x80_0000) - 0x10_0000);
            car.update_car_flags(m, |f| f & !(CAR_QUIT | CAR_TOP_SPEED_CAPPED) | [0, CAR_QUIT][(pick(6) == 0) as usize] | [0, CAR_TOP_SPEED_CAPPED][pick(2) as usize]);
            car.n().sim_road_info().set_slice(m, pick(slices as u32) as i16);
        }
        if pick(2) == 0 {
            clear_position(m);
            for i in 0..pick(cars as u32 + 1) {
                let index = race_car(m, i).car_index(m);
                m.w32(position(i), index as u32);
                m.w32(position(i) + 4, pick(200) as u32);
                m.w32(position(i) + 8, pick(2000) as u32);
                m.w32(position(i) + 12, (pick(2) * 4) as u32);
            }
        }
        SIM_COUNTER.set(m, [pick(20_000), pick(300) * 64 + 1][pick(2) as usize]);
        SIM_END_GAME.set(m, (pick(4) == 0) as i32);
        COPSPEAK_QUEUE_PLAY.set(m, pick(2));
        COPSPEAK_QUEUE_HEAD.set(m, pick(2));
        COPSPEAK_SPCH_HANDLE.set(m, pick(2) - 1);
    }

    /// `port` against the recompiled `original` on shuffled standings.
    fn against_original(name: &str, cases: u32, seed: u64, original: fn(&mut Cpu), args: &dyn Fn(&mut Cpu) -> Vec<u32>, port: fn(&mut Cpu, &[u32]) -> Option<u32>) {
        crate::test_util::against_original(name, cases, seed, shuffle, original, args, port);
    }

    fn car(c: &mut Cpu, i: u32) -> u32 {
        c.m.r32(RACE_CARS + 4 * i)
    }

    #[test]
    fn positions_fuzz() {
        against_original("Stats_ClearPosition", 200, 0x5C1, nfs4rc::f_800b8de8, &|_| vec![], |c, _| {
            clear_position(&mut c.m);
            None
        });
        against_original("Stats_GetPosition", 5_000, 0x6E7, nfs4rc::f_800b8e28, &|c| vec![car(c, 1)], |c, a| Some(get_position(&mut c.m, Car(a[0])) as u32));
        against_original("Stats_GetNumOpponents", 2_000, 0x0AA, nfs4rc::f_800b8e7c, &|_| vec![], |c, _| Some(get_num_opponents(&mut c.m) as u32));
    }

    #[test]
    fn track_stats_fuzz() {
        for k in 0..3 {
            against_original("Stats_TrackStats", 5_000, 0x7A5 + k as u64, nfs4rc::f_800b8ed0, &|c| vec![car(c, k)], |c, a| {
                track_stats(c, Car(a[0]));
                None
            });
        }
    }

    #[test]
    fn player_glue_fuzz() {
        against_original("Stats_DoPlayerGlue", 5_000, 0x61BE, nfs4rc::f_800b8c2c, &|_| vec![], |c, _| {
            do_player_glue(c);
            None
        });
    }

    #[test]
    fn end_game_fuzz() {
        against_original("Stats_TrackEndGame", 5_000, 0xE6D, nfs4rc::f_800b9a80, &|_| vec![], |c, _| {
            track_end_game(c);
            None
        });
        for how in [1, 2] {
            against_original("Stats_ExtrapolateOpponentTimes", 5_000, 0xE7A + how as u64, nfs4rc::f_800b92d8, &|_| vec![how], |c, a| {
                extrapolate_opponent_times(c, a[0] as i32);
                None
            });
        }
    }
}
