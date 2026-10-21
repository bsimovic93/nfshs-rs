//! AISPEEDS.CPP: how fast the AI's cars want to go. A racer's speed is its curve speed for the
//! bends ahead (slowing in time with its brakes), times the catch-up glue (by its distance
//! from the humans, or from the human cop in a pursuit), its caravan's spacing, the AI script,
//! its damage, nitrous, the track and night and the tournament's multiplier; finished racers
//! pull over. Police and traffic go at the road's legal speed (traffic randomized, slower
//! oncoming), cops at their top speed when chasing. Also the leader board, the upgrades' effects
//! and the tuning file.

use crate::abi::{self, Saved};
use crate::aiworld::{apx_spline_distance, apx_spline_distance_to_slice, game_odometer};
use crate::fixed::{fixeddiv, fixedmult};
use crate::globals::{
    AIDATARECORD_TRACK_CURVE, AISPEEDS_TRACK_AND_NIGHT_MULT, AISPEEDS_TRACK_SPEEDS, AISPEEDS_WEATHER_MULT_FACTORS, AI_ELAPSED_TIME,
    AI_RACE_CARS, AERO_UPGRADE, BWORLDSM_SLICES, CARAVAN_INFO, CARS_LIST, ENGINE_UPGRADE, FAST_RANDOM, GAME_SETUP, GLUE_TABLE,
    HUMAN_RACE_CARS, LEADER_BOARD, NUM_AI_RACE_CARS, NUM_CARS, NUM_HUMAN_RACE_CARS, NUM_SLICES, PATHS, RAND_SEED, RAND_TEMP,
    SIM_COUNTER, SUSPENSION_UPGRADE, TOTAL_SORTED_LIST,
};
use crate::aidatarecord::{curve_speed_table_get, track_curve_get};
use crate::types::{AIDataRecord, AISpeedsSlotInfo, Car, TrkNewSlice};
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`.
const CAR_RACING: i32 = 0x1;
const CAR_HUMAN: i32 = 0x4;
const CAR_AI_RACER: i32 = 0x8;
const CAR_TRAFFIC: i32 = 0x10;
const CAR_COP: i32 = 0x20;
const CAR_HUMAN_COP: i32 = 0x200;
/// `Car_tObj.AIFlags`: a police car chasing.
const AI_CHASING: i32 = 0x2;
/// `Car_tStats.finishType`: finished.
const FINISHED: i32 = 2;
/// `GameSetup_tData.raceType`: the pursuits (with a human cop), a knockout.
const RACE_PURSUIT: i32 = 1;
const RACE_PURSUIT_2: i32 = 5;
const RACE_KNOCKOUT: i32 = 3;
/// The slowest the police and traffic drive (2.5 m/s... 10 km/h).
const MIN_TRAFFIC_SPEED: i32 = 0x8_E38E;

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

fn car_slice(m: &mut Mem, car: Car) -> i32 {
    car.n().sim_road_info().slice(m) as i32
}

fn human(m: &mut Mem, i: u32) -> Car {
    Car(m.r32(HUMAN_RACE_CARS + 4 * i))
}

/// `a * b` as two 8.8 numbers multiply (each truncated to 1/256 first).
fn mul_8_8(a: i32, b: i32) -> i32 {
    (a / 256).wrapping_mul(b / 256)
}

/// A pursuit race in which a human is a cop (the first human, or the second of two).
fn against_human_cop(m: &mut Mem) -> bool {
    let race = GAME_SETUP.race_type(m);
    if race != RACE_PURSUIT && race != RACE_PURSUIT_2 {
        return false;
    }
    human(m, 0).car_flags(m) & CAR_HUMAN_COP != 0 || (NUM_HUMAN_RACE_CARS.get(m) == 2 && human(m, 1).car_flags(m) & CAR_HUMAN_COP != 0)
}

/// The slice `by` slices on from `slice` (back when negative), round the track.
fn slice_on(m: &mut Mem, slice: i32, by: i32) -> i32 {
    let s = slice.wrapping_add(by);
    if by < 0 {
        if s < 0 { s.wrapping_add(NUM_SLICES.get(m)) } else { s }
    } else if s < NUM_SLICES.get(m) {
        s
    } else {
        s.wrapping_sub(NUM_SLICES.get(m))
    }
}

/// The glue table's entry for a gap (16.16 m): one per 60 m, 10 for none, within 0..20.
fn glue_for_gap(m: &mut Mem, gap: i32) -> i32 {
    let hi = ((gap as i64 * 0x8888_8889u32 as i32 as i64) >> 32) as i32;
    let i = (hi.wrapping_add(gap) >> 21).wrapping_sub(gap >> 31).wrapping_add(10);
    let i = if i < 0 { 0 } else { i.min(20) };
    m.r32(GLUE_TABLE.wrapping_add(4 * i as u32)) as i32
}

/// One step of the game's quick random numbers: the product it keeps (`randtemp`).
fn fast_random_step(m: &mut Mem) -> u32 {
    let t = FAST_RANDOM.get(m).wrapping_mul(RAND_SEED.get(m));
    RAND_TEMP.set(m, t);
    FAST_RANDOM.set(m, t & 0xFFFF);
    t
}

/// The track's curvature at a slice, a car's speed for a curvature.
fn track_curve(m: &mut Mem, slice: i32) -> i32 {
    let table = AIDataRecord(AIDATARECORD_TRACK_CURVE.get(m));
    track_curve_get(m, table, slice)
}

fn curve_speed(m: &mut Mem, car: Car, curve: i32) -> i32 {
    let table = AIDataRecord(car.curve_speed_table(m));
    curve_speed_table_get(m, table, curve)
}

/// `AISpeeds_StartUp` (AISPEEDS.CPP, 0x8006ddb4): the rain's speed factors' memory, and the
/// tuning file read.
pub fn start_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let p = r.call(c, nfs4rc::f_800e5f28, &[0x8013_D8C0, 260, 0], 0x8006_ddd0); // reservememadr("aiwther")
        AISPEEDS_WEATHER_MULT_FACTORS.set(&mut c.m, p);
        r.enter(c, 0x8006_ddec, 0x8006_dddc, read_tuning_info);
    })
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(c);
}

/// `AISpeeds_ReadTuningInfo` (AISPEEDS.CPP, 0x8006ddec): `tuning.bin`: the caravan of each AI
/// slot (how long it keeps a distance, the distances, how often it falls back), a tournament
/// race's multiplier for the AI racers' car type by the skill, the track's (and night's) speed
/// factor, the rain's factors by curvature, the upgrades.
pub fn read_tuning_info(c: &mut Cpu) {
    abi::frame(c, 168, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = 0;
        let paths = c.m.r32(PATHS + 8);
        r.call_leaving(c, nfs4rc::f_800e50a8, &[sp + 16, 0x8005_5CA0, paths], 0x8006_de20); // sprintf("%stuning.bin")
        let file = r.call_leaving(c, nfs4rc::f_800bc7dc, &[sp + 16, 0, 1], 0x8006_de30); // Udff_Opena
        r.0[1] = file;
        let get = |c: &mut Cpu, r: &Saved, ra: u32| r.call_leaving(c, nfs4rc::f_800bc8f8, &[file], ra) as i32; // Udff_GetInt
        get(c, &r, 0x8006_de3c);
        for slot in 0..6u32 {
            let info = AISpeedsSlotInfo(CARAVAN_INFO).at(slot);
            r.0[0] = info.0;
            r.0[2] = slot;
            let v = get(c, &r, 0x8006_de58);
            info.set_distance_maintain_time32(&mut c.m, v << 5);
            let v = get(c, &r, 0x8006_de68);
            info.set_min_distance_meters(&mut c.m, v);
            let v = get(c, &r, 0x8006_de74);
            info.set_max_distance_meters(&mut c.m, v);
            let v = get(c, &r, 0x8006_de80);
            info.set_fall_back_random_time_tick_percent(&mut c.m, fixeddiv(0x1_0000, v << 21) as u32);
        }
        (r.0[0], r.0[2]) = (0, 6);
        let m = &mut c.m;
        if GAME_SETUP.race_type(m) == 0 && NUM_AI_RACE_CARS.get(m) > 0 {
            let kind = Car(m.r32(AI_RACE_CARS)).car_info(m).car_type(m);
            r.0[2] = GAME_SETUP.0;
            r.0[3] = kind as u32;
            for k in 0..22 {
                r.0[0] = k as u32;
                let v = get(c, &r, 0x8006_dedc);
                c.m.w32(sp + 128, v as u32);
                let v = get(c, &r, 0x8006_dee8);
                c.m.w32(sp + 132, v as u32);
                let v = get(c, &r, 0x8006_def4);
                c.m.w32(sp + 136, v as u32);
                if k == kind {
                    let m = &mut c.m;
                    let skill = GAME_SETUP.skill(m) as u32;
                    let v = m.r32(sp.wrapping_add(128).wrapping_add(skill.wrapping_mul(4))) as i32;
                    GAME_SETUP.set_tournament_multiplier(m, v);
                }
            }
        } else {
            for k in 0..22 {
                r.0[0] = k;
                get(c, &r, 0x8006_df38);
                get(c, &r, 0x8006_df40);
                get(c, &r, 0x8006_df48);
            }
        }
        AISPEEDS_TRACK_AND_NIGHT_MULT.set(&mut c.m, 0);
        r.0[3] = GAME_SETUP.0;
        for track in 0..12 {
            r.0[0] = track as u32;
            get(c, &r, 0x8006_df7c);
            let day = get(c, &r, 0x8006_df84);
            r.0[2] = day as u32;
            let night = get(c, &r, 0x8006_df90);
            let m = &mut c.m;
            let night = if GAME_SETUP.time(m) == 0 { 0x1_0000 } else { night };
            if track == GAME_SETUP.track(m) {
                AISPEEDS_TRACK_AND_NIGHT_MULT.set(m, fixedmult(day, night));
            }
        }
        r.0[0] = 12;
        let step = get(c, &r, 0x8006_dfd0);
        let m = &mut c.m;
        let factors = AISPEEDS_WEATHER_MULT_FACTORS.get(m);
        let mut at = 0i32;
        for i in 0..65u32 {
            m.w32(factors.wrapping_add(4 * i), 0x1_0000i32.wrapping_sub(at / 64) as u32);
            at = at.wrapping_add(step);
        }
        for (table, ra) in [(ENGINE_UPGRADE, 0x8006_e024), (SUSPENSION_UPGRADE, 0x8006_e038), (AERO_UPGRADE, 0x8006_e04c)] {
            r.call_leaving(c, nfs4rc::f_800bc910, &[file, table, 16], ra); // Udff_GetBuffer
        }
        r.call_leaving(c, nfs4rc::f_800bc8a4, &[file], 0x8006_e054); // Udff_Close
    })
}

pub fn abi_read_tuning_info(c: &mut Cpu) {
    read_tuning_info(c);
}

/// `AISpeeds_CleanUp` (AISPEEDS.CPP, 0x8006e070): the rain's factors freed.
pub fn clean_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let p = AISPEEDS_WEATHER_MULT_FACTORS.get(&mut c.m);
        if p != 0 {
            AISPEEDS_WEATHER_MULT_FACTORS.set(&mut c.m, 0);
            Saved::of(c).call(c, nfs4rc::f_800e612c, &[p], 0x8006_e088); // purgememadr
        }
    })
}

pub fn abi_clean_up(c: &mut Cpu) {
    clean_up(c);
}

/// `AISpeeds_SuperDuperSpeedUpTheCarsAtTheStartBecauseWeCannotActuallyHandleRenderingTheseCars`
/// (AISPEEDS.CPP, 0x8006e09c): in the first 30 s of a race (not a pursuit) against one human
/// who is behind, the AI racers further back go faster (20% more per place), to spread out.
pub fn super_duper_speed_up(m: &mut Mem, car: Car) -> i32 {
    let ai = NUM_AI_RACE_CARS.get(m);
    let behind = ai.wrapping_sub(car.ai_slot(m));
    if ai < 2
        || GAME_SETUP.race_type(m) == RACE_PURSUIT
        || SIM_COUNTER.get(m) >= 1920
        || NUM_HUMAN_RACE_CARS.get(m) != 1
        || human(m, 0).n().total_slice(m) >= car.n().total_slice(m)
    {
        return 0x1_0000;
    }
    behind.wrapping_mul(13107).wrapping_add(0x1_0000)
}

pub fn abi_super_duper_speed_up(c: &mut Cpu) {
    c.r[2] = super_duper_speed_up(&mut c.m, Car(c.r[4])) as u32;
}

/// `AISpeeds_CalcOpponentTopSpeed` (AISPEEDS.CPP, 0x8006e138): an AI racer's wanted speed
/// (signed by its direction): its curve speed times its glue (none falling back for the
/// caravan), the script's and the damage's factors (`*original` without them); the glue eased
/// in over the first 150 slices, the start's speed-up, both limited near the humans; the glue
/// its gravity too; a finished racer stops on the road past its place's distance (5 m/s within
/// 400 m of the line).
pub fn calc_opponent_top_speed(c: &mut Cpu, car: Car, original: u32) -> i32 {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = car.0;
        r.0[6] = original;
        let curve = r.enter(c, 0x8006_e734, 0x8006_e168, |c| calc_opponent_curve_speed(c, car));
        r.0[4] = curve as u32;
        let m = &mut c.m;
        let glue = if car.fall_behind_car(m).0 != 0 {
            0x1_0000
        } else if against_human_cop(m) {
            r.enter(c, 0x8006_e89c, 0x8006_e1f4, |c| btc_get_glue_factor(c, car))
        } else {
            r.enter(c, 0x8006_ee8c, 0x8006_e204, |c| get_glue_factor(c, car))
        };
        r.0[0] = glue as u32;
        let script = r.enter(c, 0x8006_f8a4, 0x8006_e210, |c| get_script_factor(c, car));
        r.0[5] = script as u32;
        let nitrous = car.speed_nitrous(&mut c.m);
        r.0[1] = nitrous as u32;
        let damage = r.enter(c, 0x8006_f098, 0x8006_e220, |c| get_damage_factor(&mut c.m, car));
        r.0[2] = damage as u32;
        let m = &mut c.m;
        let caravan = if GAME_SETUP.race_type(m) != RACE_PURSUIT && NUM_AI_RACE_CARS.get(m) >= 2 {
            r.enter(c, 0x8006_ead0, 0x8006_e258, |c| get_caravan_factor(c, car))
        } else {
            0x1_0000
        };
        let m = &mut c.m;
        let x = mul_8_8(GAME_SETUP.tournament_multiplier(m), glue);
        let x = (caravan / 256).wrapping_mul(x / 256);
        let x = (AISPEEDS_TRACK_AND_NIGHT_MULT.get(m) / 256).wrapping_mul(x / 256);
        let mut top = (nitrous / 256).wrapping_mul(x / 256);
        let mut speed = (top / 256).wrapping_mul(mul_8_8(damage, script) / 256);
        r.0[1] = top as u32;
        r.0[0] = speed as u32;
        if GAME_SETUP.race_type(m) != RACE_PURSUIT {
            r.0[2] = 0x1_0000;
        }
        if GAME_SETUP.race_type(m) != RACE_PURSUIT && (car.n().total_slice(m) as u32) < 150 {
            let ease = |m: &mut Mem, g: i32| {
                let k = 0x1_0000i32.wrapping_sub(g).wrapping_mul(car.n().total_slice(m) as i32);
                0x1_0000i32.wrapping_sub(fixedmult(k, 436))
            };
            top = ease(m, top);
            speed = ease(m, speed);
            r.0[1] = top as u32;
            r.0[0] = speed as u32;
        }
        let boost = r.enter(c, 0x8006_e09c, 0x8006_e3d8, |c| super_duper_speed_up(&mut c.m, car));
        if boost != 0x1_0000 {
            top = mul_8_8(top, boost);
            speed = mul_8_8(speed, boost);
            r.0[1] = top as u32;
            r.0[0] = speed as u32;
        }
        let glue = r.enter(c, 0x8006_f1b0, 0x8006_e434, |c| limit_glue_multiplier(c, car, speed));
        car.set_ai_glue(&mut c.m, glue);
        let top = r.enter(c, 0x8006_f1b0, 0x8006_e444, |c| limit_glue_multiplier(c, car, top));
        let m = &mut c.m;
        let glue = car.ai_glue(m);
        car.n().set_gravity_mult(m, if 0x1_0000 < glue { glue } else { 0x1_0000 });
        let mut want = (curve / 256).wrapping_mul(car.ai_glue(m) / 256);
        m.w32(original, (curve / 256).wrapping_mul(top / 256) as u32);
        if car.car_flags(m) & CAR_RACING != 0 && car.stats().finish_type(m) == FINISHED {
            // (the original reads its human cop test with `numHumanRaceCars == finishType`)
            if !against_human_cop(m) {
                let gap = apx_spline_distance_to_slice(m, car, 0).wrapping_abs();
                let pos = car.road_position(m);
                let half = car.n().dimension().x(m);
                let (inside, outside) = (pos.wrapping_sub(half), pos.wrapping_add(half));
                let n = NUM_CARS.get(m);
                let mut place = 0;
                while place < n && m.r32(TOTAL_SORTED_LIST + 4 * place as u32) != car.0 {
                    place += 1;
                }
                if gap >= place.wrapping_mul(5) << 19 {
                    let here = car_slice(m, car);
                    let s = slice_at(m, here);
                    let lanes = s.lane_count(m) as i32;
                    let left = ((s.avg_paved_width_lf(m) as i32) << 15).wrapping_mul(lanes >> 4).wrapping_neg();
                    let right = ((s.avg_paved_width_rt(m) as i32) << 15).wrapping_mul(lanes & 0xF);
                    let on_road = inside >= left && right >= inside && outside >= left && right >= outside;
                    want = if on_road && gap <= 0x190_0000 { 0x11_C71C } else { 0 };
                }
            }
        }
        let dir = car.direction(m);
        let o = m.r32(original) as i32;
        m.w32(original, o.wrapping_mul(dir) as u32);
        want.wrapping_mul(car.direction(m))
    })
}

pub fn abi_calc_opponent_top_speed(c: &mut Cpu) {
    c.r[2] = calc_opponent_top_speed(c, Car(c.r[4]), c.r[5]) as u32;
}

/// `AISpeeds_NeedToSlowDownForCurve` (AISPEEDS.CPP, 0x8006e690): whether a car at `speed`
/// must brake now to be at `target` within `dist` (its braking distances, by m/s, plus an
/// eighth).
pub fn need_to_slow_down_for_curve(m: &mut Mem, car: Car, dist: i32, speed: i32, target: i32) -> bool {
    if speed < target {
        return false;
    }
    let table = car.brake_info(m).0;
    let index = |v: i32| (v / 0x1_0000).wrapping_abs().min(128) as u32;
    let stop = |m: &mut Mem, v: i32| (m.r8(table.wrapping_add(index(v))) as i32) << 17;
    let d = stop(m, speed).wrapping_sub(stop(m, target));
    let d = d.wrapping_add(d >> 3);
    d >= dist
}

pub fn abi_need_to_slow_down_for_curve(c: &mut Cpu) {
    c.r[2] = need_to_slow_down_for_curve(&mut c.m, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// `AISpeeds_CalcOpponentCurveSpeed` (AISPEEDS.CPP, 0x8006e734): an AI racer's speed for the
/// road ahead: from 200 m down, every 25 m, the curve speed there (slower in the rain) the
/// first one it must already brake for, or here.
pub fn calc_opponent_curve_speed(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    let speed = car.current_speed(m).wrapping_abs();
    let mut ahead = 200i32;
    loop {
        let hi = ((ahead as i64 * 0x2AAA_AAAB) >> 32) as i32;
        let n = hi.wrapping_sub(ahead >> 31).wrapping_mul(car.direction(m));
        let here = car_slice(m, car);
        let slice = slice_on(m, here, n);
        let curve = track_curve(m, slice);
        let mut target = curve_speed(m, car, curve);
        if GAME_SETUP.weather(m) != 0 {
            let factors = AISPEEDS_WEATHER_MULT_FACTORS.get(m);
            let f = m.r32(factors.wrapping_add(((curve / 4) as u32).wrapping_mul(4))) as i32;
            target = fixedmult(target, f);
        }
        if ahead == 0 || need_to_slow_down_for_curve(m, car, ahead << 16, speed, target) {
            return target;
        }
        ahead -= 25;
    }
}

pub fn abi_calc_opponent_curve_speed(c: &mut Cpu) {
    c.r[2] = calc_opponent_curve_speed(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_BTCGetGlueFactor` (AISPEEDS.CPP, 0x8006e89c): an AI racer's glue in a pursuit,
/// by its distance from the nearest human cop (none when that cop is under road-side control or
/// slower than 10 m/s; police cars none), the glue table's softened by the race's modifier, at
/// least 0.4, by the car's speed factor.
pub fn btc_get_glue_factor(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    if car.car_flags(m) & CAR_COP != 0 {
        return 0x1_0000;
    }
    let (mut gap, mut cop) = (0x270F_0000i32, Car(0));
    for i in 0..NUM_HUMAN_RACE_CARS.get(m).max(0) as u32 {
        let h = human(m, i);
        if h.car_flags(m) & CAR_HUMAN_COP != 0 {
            let d = apx_spline_distance(m, car, h);
            if d.wrapping_abs() < gap.wrapping_abs() {
                gap = d;
                cop = h;
            }
        }
    }
    let gap = gap.wrapping_mul(car.direction(m));
    if cop.rs_control(m) != 0 || (cop.current_speed(m).wrapping_add(0x9_FFFF) as u32) <= 0x13_FFFE {
        return 0x1_0000;
    }
    let mut glue = glue_for_gap(m, gap);
    if glue <= 0xFFFF {
        glue = 0x1_0000i32.wrapping_sub(fixedmult(0x1_0000i32.wrapping_sub(glue), car.btc_glue_modifier(m)));
    }
    fixedmult(glue.max(26214), car.speed_factor(m))
}

pub fn abi_btc_get_glue_factor(c: &mut Cpu) {
    c.r[2] = btc_get_glue_factor(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_GetNextAICar` (AISPEEDS.CPP, 0x8006ea58): the AI racer ahead.
pub fn get_next_ai_car(m: &mut Mem, car: Car) -> Car {
    car.next_ai_racer(m)
}

pub fn abi_get_next_ai_car(c: &mut Cpu) {
    c.r[2] = get_next_ai_car(&mut c.m, Car(c.r[4])).0;
}

/// `AISpeeds_GetPrevAICar` (AISPEEDS.CPP, 0x8006ea64): the AI racer behind (in the order
/// down the road), none for the last.
pub fn get_prev_ai_car(m: &mut Mem, car: Car) -> Car {
    let mut prev = Car(0);
    for i in 0..NUM_CARS.get(m).max(0) as u32 {
        let c = Car(m.r32(TOTAL_SORTED_LIST + 4 * i));
        if c == car {
            return prev;
        }
        if c.car_flags(m) & CAR_AI_RACER != 0 {
            prev = c;
        }
    }
    Car(0)
}

pub fn abi_get_prev_ai_car(c: &mut Cpu) {
    c.r[2] = get_prev_ai_car(&mut c.m, Car(c.r[4])).0;
}

/// `AISpeeds_GetCaravanFactor` (AISPEEDS.CPP, 0x8006ead0): an AI racer keeping its slot's
/// distance behind the AI racer ahead: 0.9 while falling back behind another; slower or faster
/// (to 0.6 or 1.5) by how far it is from that distance (unless the one ahead is slow or slowing
/// down: then as fast, 1.1 when it waits for this one); a new distance at the slot's interval;
/// one now and then falling back to let the one behind past (not the leader when a human leads);
/// the one ahead waiting when this one is more damaged.
pub fn get_caravan_factor(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    let slot = car.ai_slot(m);
    let ahead = get_next_ai_car(m, car);
    let mut factor;
    let back = car.fall_behind_car(m);
    if back.0 != 0 {
        factor = 0xE666;
        if back.ai_slot(m) < slot {
            car.set_fall_behind_car(m, Car(0));
        }
    } else {
        let mut keep_up = false;
        if ahead.0 != 0 && ahead.fall_behind_car(m).0 == 0 && 0xD_6491 < ahead.speed(m) {
            let dir = car.direction(m);
            let orig = ahead.original_desired_speed(m).wrapping_mul(dir) / 256;
            keep_up = ahead.desired_speed(m).wrapping_mul(dir) >= orig.wrapping_mul(179);
        }
        if keep_up {
            let gap = game_odometer(m, ahead).wrapping_sub(game_odometer(m, car));
            let want = car.caravan_follow_behind_distance_meters(m);
            factor = 0x1_0000;
            if want.wrapping_add(0xA_0000) < gap {
                factor = if want.wrapping_add(0x3E8_0000) < gap { 0x1_8000 } else { 0x1_3333 };
            } else if gap < want.wrapping_sub(0xA_0000) {
                factor = if gap < want.wrapping_add(-0x3E8_0000) { 0x9999 } else { 0xCCCC };
            }
        } else {
            // (with none ahead, the original reads the word at 0x764)
            let waits = ahead.fall_behind_car(m) == car;
            factor = if waits { 0x1_1999 } else { 0x1_0000 };
        }
    }
    let info = AISpeedsSlotInfo(CARAVAN_INFO).at(slot as u32);
    if info.distance_maintain_time32(m) != 0 {
        let e = AI_ELAPSED_TIME.get(m);
        car.update_caravan_timer(m, |t| t.wrapping_sub(e));
    }
    if car.caravan_timer(m) < 0 {
        let a = fast_random_step(m);
        let time = info.distance_maintain_time32(m) / 2;
        let t = time.wrapping_add(((time.wrapping_mul(((a & 0xFF_FF00) >> 8) as i32) as u32) >> 16) as i32);
        car.set_caravan_timer(m, t);
        let b = fast_random_step(m);
        let span = info.max_distance_meters(m).wrapping_sub(info.min_distance_meters(m));
        let d = info.min_distance_meters(m).wrapping_add(((span.wrapping_mul(((b & 0xFF_FF00) >> 8) as i32) as u32) >> 16) as i32);
        car.set_caravan_follow_behind_distance_meters(m, d << 16);
    }
    let laps_end = GAME_SETUP.num_laps(m).wrapping_mul(NUM_SLICES.get(m)).wrapping_sub(333);
    if slot < NUM_AI_RACE_CARS.get(m) - 1 && car.fall_behind_car(m).0 == 0 && (car.n().total_slice(m) as i32) < laps_end {
        let human_leads = (slot as u32) < 2 && m.r32(LEADER_BOARD) == m.r32(HUMAN_RACE_CARS);
        if !human_leads {
            let rand = (fast_random_step(m) >> 8) & 0xFFFF;
            let ahead_waits = ahead.0 != 0 && ahead.fall_behind_car(m).0 != 0;
            if !ahead_waits && rand < (info.fall_back_random_time_tick_percent(m) as i32).wrapping_mul(AI_ELAPSED_TIME.get(m)) as u32 {
                let prev = get_prev_ai_car(m, car);
                if prev.0 != 0 {
                    car.set_fall_behind_car(m, prev);
                }
            }
        }
    }
    if ahead.0 != 0 && ahead.damage_mult(m) < car.damage_mult(m).wrapping_sub(6553) && ahead.fall_behind_car(m).0 == 0 {
        ahead.set_fall_behind_car(m, car);
    }
    factor
}

pub fn abi_get_caravan_factor(c: &mut Cpu) {
    c.r[2] = get_caravan_factor(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_GetGlueFactor` (AISPEEDS.CPP, 0x8006ee8c): an AI racer's catch-up glue: none
/// while the leading human is slower than 10 m/s; from the glue table by the gap between the
/// leading AI racer and the leading human (all of it when the human is last or first; at 0.9,
/// or half in a knockout, in between); no slower than 1 while the car isn't 60 m ahead of the
/// leading human.
pub fn get_glue_factor(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    let lb = LEADER_BOARD;
    let odo = |m: &mut Mem, k: u32| {
        let c = Car(m.r32(lb + 4 * k));
        game_odometer(m, c)
    };
    let (human_at, lead_ai, last_ai) = (odo(m, 1), odo(m, 2), odo(m, 3));
    let lead_human = Car(m.r32(lb + 4));
    let (mut glue, mut scale) = (0x1_0000, 0x1_0000);
    if lead_human.current_speed(m).wrapping_abs() > 0x9_FFFF {
        glue = glue_for_gap(m, lead_ai.wrapping_sub(human_at));
        if human_at >= last_ai && lead_ai >= human_at {
            scale = if GAME_SETUP.race_type(m) == RACE_KNOCKOUT { 0x8000 } else { 0xE666 };
        }
    }
    if game_odometer(m, car) < human_at.wrapping_add(0x3C_0000) && glue <= 0xFFFF {
        glue = 0x1_0000;
    }
    if 0x1_0000 < glue {
        fixedmult(glue - 0x1_0000, scale).wrapping_add(0x1_0000)
    } else {
        0x1_0000i32.wrapping_sub(fixedmult(0x1_0000i32.wrapping_sub(glue), scale))
    }
}

pub fn abi_get_glue_factor(c: &mut Cpu) {
    c.r[2] = get_glue_factor(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_GetDamageFactor` (AISPEEDS.CPP, 0x8006f098): how much slower its damage makes the
/// car (its damage zones, more against a human cop), at least half speed; kept as its
/// `damageMult`.
pub fn get_damage_factor(m: &mut Mem, car: Car) -> i32 {
    let sum = (0..8).fold(0i32, |s, i| s.wrapping_add(car.n().damage(m, i)));
    car.set_damage_mult(m, sum);
    let k = if against_human_cop(m) { 327 } else { 72 };
    let d = fixedmult(car.damage_mult(m), k);
    car.set_damage_mult(m, d);
    let f = 0x1_0000i32.wrapping_sub(d).max(0x8000);
    car.set_damage_mult(m, f);
    f
}

pub fn abi_get_damage_factor(c: &mut Cpu) {
    c.r[2] = get_damage_factor(&mut c.m, Car(c.r[4])) as u32;
}

/// `AISpeeds_LimitGlueMultiplier` (AISPEEDS.CPP, 0x8006f1b0): the glue kept within 0.6..1.4
/// when a human is within 120 m.
pub fn limit_glue_multiplier(c: &mut Cpu, car: Car, glue: i32) -> i32 {
    let m = &mut c.m;
    if (glue.wrapping_add(-0x999A) as u32) <= 0xCCCB {
        return glue;
    }
    let mut near = 0x2710_0000;
    for i in 0..NUM_HUMAN_RACE_CARS.get(m).max(0) as u32 {
        let h = human(m, i);
        let d = apx_spline_distance(m, car, h).wrapping_abs();
        if d < near {
            near = d;
        }
    }
    if 0x78_0000 < near {
        return glue;
    }
    glue.min(0x1_6666).max(0x9999)
}

pub fn abi_limit_glue_multiplier(c: &mut Cpu) {
    c.r[2] = limit_glue_multiplier(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AISpeeds_CalcCopTopSpeed` (AISPEEDS.CPP, 0x8006f2a4): a chasing police car's wanted speed:
/// its curve speed within its top speed, by the track and its nitrous (its glue, and gravity).
pub fn calc_cop_top_speed(c: &mut Cpu, car: Car) -> i32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let curve = r.enter(c, 0x8006_e734, 0x8006_f2b8, |c| calc_opponent_curve_speed(c, car));
        let m = &mut c.m;
        let speed = curve.min(car.cop_top_speed(m));
        let glue = mul_8_8(AISPEEDS_TRACK_AND_NIGHT_MULT.get(m), car.speed_nitrous(m));
        car.set_ai_glue(m, glue);
        car.n().set_gravity_mult(m, if 0x1_0000 < glue { glue } else { 0x1_0000 });
        mul_8_8(speed, car.ai_glue(m)).wrapping_mul(car.direction(m))
    })
}

pub fn abi_calc_cop_top_speed(c: &mut Cpu) {
    c.r[2] = calc_cop_top_speed(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_CalcTrafficTopSpeed` (AISPEEDS.CPP, 0x8006f36c): traffic's (and a patrolling
/// police car's) wanted speed: three quarters of the road's legal speed, oncoming cars at their
/// own speed (when slower) by how fast the car they pass goes (against a human cop: those going the other way
/// than the first human), traffic randomized and by its speed factor, at least 2.5 m/s.
pub fn calc_traffic_top_speed(c: &mut Cpu, car: Car) -> i32 {
    abi::frame(c, 32, |c, _| {
        let m = &mut c.m;
        let slice = car_slice(m, car);
        let mut speed = fixedmult(get_legal_speed(m, slice), 0xC000);
        let oncoming = if against_human_cop(m) {
            car.direction(m) != human(m, 0).direction(m)
        } else {
            let dir = car.direction(m);
            if GAME_SETUP.reverse_track(m) != 0 { dir != -1 } else { dir != 1 }
        };
        if oncoming {
            // (the original asks for it again when it is the slower)
            speed = speed.min(calculate_oncoming_car_speed(m, car));
        }
        if car.car_flags(m) & CAR_TRAFFIC != 0 {
            speed = randomize_traffic_speed(m, car, speed);
            speed = mul_8_8(speed, car.speed_factor(m));
        }
        speed.max(MIN_TRAFFIC_SPEED).wrapping_mul(car.direction(m))
    })
}

pub fn abi_calc_traffic_top_speed(c: &mut Cpu) {
    c.r[2] = calc_traffic_top_speed(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_GetLegalSpeed` (AISPEEDS.CPP, 0x8006f50c): the track's speed limit at a slice (its
/// table of slices and speeds).
pub fn get_legal_speed(m: &mut Mem, slice: i32) -> i32 {
    let track = GAME_SETUP.track(m) as u32;
    let mut at = m.r32(AISPEEDS_TRACK_SPEEDS.wrapping_add(track.wrapping_mul(4)));
    while (m.r16(at) as i32) < slice {
        at = at.wrapping_add(4);
    }
    (m.r16(at.wrapping_add(2)) as i32) << 8
}

pub fn abi_get_legal_speed(c: &mut Cpu) {
    c.r[2] = get_legal_speed(&mut c.m, c.r[4] as i32) as u32;
}

/// `AISpeeds_RandomizeTrafficSpeed` (AISPEEDS.CPP, 0x8006f550): traffic's speed by its random
/// factor, no slower than 15 m/s under it, nor under 2.5 m/s.
pub fn randomize_traffic_speed(m: &mut Mem, car: Car, speed: i32) -> i32 {
    let factor = car.traffic_speed_randomizing_factor(m);
    if speed <= 0 {
        return mul_8_8(speed, factor);
    }
    let s = (speed >> 8).wrapping_mul(factor / 256).max(speed.wrapping_add(-0xD_6666));
    if 0x8_E665 < s { s } else { 0x8_E666 }
}

pub fn abi_randomize_traffic_speed(c: &mut Cpu) {
    c.r[2] = randomize_traffic_speed(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AISpeeds_CalcDesiredSpeed` (AISPEEDS.CPP, 0x8006f5dc, the AI's each tick): the speed the
/// car wants (and without the factors): an AI racer's, a police car's chasing or not, traffic's.
pub fn calc_desired_speed(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        let flags = car.car_flags(m);
        car.set_desired_speed(m, 0);
        if flags & CAR_AI_RACER != 0 {
            let original = car.0 + 0x560;
            let v = r.enter(c, 0x8006_e138, 0x8006_f608, |c| calc_opponent_top_speed(c, car, original));
            car.set_desired_speed(&mut c.m, v);
            return;
        }
        let v = if flags & CAR_COP != 0 {
            if car.ai_flags(m) & AI_CHASING != 0 {
                r.enter(c, 0x8006_f2a4, 0x8006_f638, |c| calc_cop_top_speed(c, car))
            } else {
                r.enter(c, 0x8006_f36c, 0x8006_f648, |c| calc_traffic_top_speed(c, car))
            }
        } else if flags & CAR_TRAFFIC != 0 {
            r.enter(c, 0x8006_f36c, 0x8006_f668, |c| calc_traffic_top_speed(c, car))
        } else {
            return;
        };
        let m = &mut c.m;
        car.set_desired_speed(m, v);
        car.set_original_desired_speed(m, v);
    })
}

pub fn abi_calc_desired_speed(c: &mut Cpu) {
    calc_desired_speed(c, Car(c.r[4]));
}

/// `AISpeeds_CalculateOncomingCarSpeed` (AISPEEDS.CPP, 0x8006f680): an oncoming car's speed,
/// slower the faster the car it passes (its basis car) goes.
pub fn calculate_oncoming_car_speed(m: &mut Mem, car: Car) -> i32 {
    let s = car.basis_car(m).current_speed(m).wrapping_abs();
    match s {
        _ if s <= 0xD_5554 => 0x16_38E3,
        _ if s <= 0x1A_AAA9 => 0xD_5555,
        _ if s <= 0x27_FFFF => 0x8_E38E,
        _ if s <= 0x37_8E37 => 0x4_71C7,
        _ if s <= 0x47_1C70 => 0x2_38E3,
        _ => 0x2_C51E,
    }
}

pub fn abi_calculate_oncoming_car_speed(c: &mut Cpu) {
    c.r[2] = calculate_oncoming_car_speed(&mut c.m, Car(c.r[4])) as u32;
}

/// `AISpeeds_SetTrafficSpeedRandomFactor` (AISPEEDS.CPP, 0x8006f728): one of four random
/// speed factors (the original's table, copied to its stack).
pub fn set_traffic_speed_random_factor(c: &mut Cpu, car: Car) {
    abi::frame(c, 16, |c, sp| {
        let m = &mut c.m;
        for k in 0..4 {
            let v = m.r32(0x8005_5CB0 + 4 * k);
            m.w32(sp + 4 * k, v);
        }
        let t = fast_random_step(m);
        let v = m.r32(sp + ((t >> 6) & 0xC));
        car.set_traffic_speed_randomizing_factor(m, v as i32);
    })
}

pub fn abi_set_traffic_speed_random_factor(c: &mut Cpu) {
    set_traffic_speed_random_factor(c, Car(c.r[4]));
}

/// `AISpeeds_MaintainLeaderBoard` (AISPEEDS.CPP, 0x8006f7a4, each tick): the leading racer (not
/// one falling back), the leading human, the leading and last AI racers; each AI racer's slot
/// (from the back) and the one ahead of it.
pub fn maintain_leader_board(m: &mut Mem) {
    let lb = LEADER_BOARD;
    for k in 0..4 {
        m.w32(lb + 4 * k, 0);
    }
    let (mut slot, mut behind) = (0, Car(0));
    let mut i = NUM_CARS.get(m) - 1;
    while i >= 0 {
        let car = Car(m.r32(TOTAL_SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4))));
        let flags = car.car_flags(m);
        if flags & CAR_RACING != 0 && m.r32(lb) == 0 && car.fall_behind_car(m).0 == 0 {
            m.w32(lb, car.0);
        }
        if car.car_flags(m) & CAR_AI_RACER != 0 {
            car.set_next_ai_racer(m, behind);
            behind = car;
            car.set_ai_slot(m, slot);
            if m.r32(lb + 8) == 0 {
                m.w32(lb + 8, car.0);
            }
            slot += 1;
            m.w32(lb + 12, car.0);
        }
        if car.car_flags(m) & CAR_HUMAN != 0 && m.r32(lb + 4) == 0 {
            m.w32(lb + 4, car.0);
        }
        i -= 1;
    }
}

pub fn abi_maintain_leader_board(c: &mut Cpu) {
    maintain_leader_board(&mut c.m);
}

/// `AISpeeds_GetScriptFactor` (AISPEEDS.CPP, 0x8006f8a4): the AI script's speed: 1.5 with
/// reaction 4 on, half with 16, else 1.
pub fn get_script_factor(c: &mut Cpu, car: Car) -> i32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let script = car.script().0;
        r.0[0] = script;
        let fast = r.call(c, nfs4rc::f_80070170, &[script, 4], 0x8006_f8c4) as i32; // AIScript_DoReAction
        r.0[1] = fast as u32;
        if fast != -1 {
            return 0x1_8000;
        }
        let slow = r.call(c, nfs4rc::f_80070170, &[script, 16], 0x8006_f8dc) as i32;
        if slow != fast { 0x8000 } else { 0x1_0000 }
    })
}

pub fn abi_get_script_factor(c: &mut Cpu) {
    c.r[2] = get_script_factor(c, Car(c.r[4])) as u32;
}

/// The effect `k` (0 acceleration, 1 braking, 2 handling, 3 top speed) of the upgrades car
/// `index` has (the engine, suspension and aero ones).
fn upgrade_mult(m: &mut Mem, index: i32, k: u32) -> i32 {
    let car = Car(m.r32(CARS_LIST.wrapping_add((index as u32).wrapping_mul(4))));
    let mut v = 0x1_0000;
    for (field, table) in [(0x3C, ENGINE_UPGRADE), (0x40, SUSPENSION_UPGRADE), (0x44, AERO_UPGRADE)] {
        let info = car.car_info(m).0;
        if m.r32(info.wrapping_add(field)) == 1 {
            v = fixedmult(v, m.r32(table + 4 * k) as i32);
        }
    }
    v
}

/// `AISpeeds_GetUpgradeAccMult`, `...BrakeMult`, `...HandlingMult`, `...TopSpeedMult`
/// (AISPEEDS.CPP, 0x8006f90c, 0x8006f9c8, 0x8006fa84, 0x8006fb40).
pub fn get_upgrade_acc_mult(m: &mut Mem, index: i32) -> i32 {
    upgrade_mult(m, index, 0)
}

pub fn get_upgrade_brake_mult(m: &mut Mem, index: i32) -> i32 {
    upgrade_mult(m, index, 1)
}

pub fn get_upgrade_handling_mult(m: &mut Mem, index: i32) -> i32 {
    upgrade_mult(m, index, 2)
}

pub fn get_upgrade_top_speed_mult(m: &mut Mem, index: i32) -> i32 {
    upgrade_mult(m, index, 3)
}

pub fn abi_get_upgrade_acc_mult(c: &mut Cpu) {
    c.r[2] = get_upgrade_acc_mult(&mut c.m, c.r[4] as i32) as u32;
}

pub fn abi_get_upgrade_brake_mult(c: &mut Cpu) {
    c.r[2] = get_upgrade_brake_mult(&mut c.m, c.r[4] as i32) as u32;
}

pub fn abi_get_upgrade_handling_mult(c: &mut Cpu) {
    c.r[2] = get_upgrade_handling_mult(&mut c.m, c.r[4] as i32) as u32;
}

pub fn abi_get_upgrade_top_speed_mult(c: &mut Cpu) {
    c.r[2] = get_upgrade_top_speed_mult(&mut c.m, c.r[4] as i32) as u32;
}

/// `AISpeeds_CalcHumanTopSpeed` (AISPEEDS.CPP, 0x8006fbfc): a human's curve speed.
pub fn calc_human_top_speed(c: &mut Cpu, car: Car) -> i32 {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.enter(c, 0x8006_fc1c, 0x8006_fc0c, |c| calc_human_curve_speed(c, car))
    })
}

pub fn abi_calc_human_top_speed(c: &mut Cpu) {
    c.r[2] = calc_human_top_speed(c, Car(c.r[4])) as u32;
}

/// `AISpeeds_CalcHumanCurveSpeed` (AISPEEDS.CPP, 0x8006fc1c): the speed for the sharpest
/// curve within 20 slices ahead of a human (every 4th), a little over its curve speed.
pub fn calc_human_curve_speed(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    let slice = car_slice(m, car);
    let first = if slice < NUM_SLICES.get(m) { slice } else { slice.wrapping_sub(NUM_SLICES.get(m)) };
    let mut sharpest = track_curve(m, first);
    for k in [4, 8, 12, 16, 20] {
        let by = car.direction(m).wrapping_mul(k);
        let s = slice_on(m, slice, by);
        sharpest = sharpest.max(track_curve(m, s));
    }
    let curve = (sharpest.wrapping_mul(0x1_A666) / 0x1_0000).min(255);
    curve_speed(m, car, curve)
}

pub fn abi_calc_human_curve_speed(c: &mut Cpu) {
    c.r[2] = calc_human_curve_speed(c, Car(c.r[4])) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, against_original};

    /// The AI physics' shuffled car (at `SCRATCH`) among the race's cars in any order, with any
    /// AI slots, caravans (falling back or not, distances, timers), damage, nitrous, speed
    /// factors, the humans anywhere, any leader board, laps, track direction and race.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aiphysic::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m);
        let cars: Vec<Car> = (0..n as u32).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let any_car = |p: &mut dyn FnMut(u32) -> i32| [Car(0), cars[p(n as u32) as usize]][p(2) as usize];
        for i in (1..n as u32).rev() {
            let j = pick(i + 1) as u32;
            let (a, b) = (m.r32(TOTAL_SORTED_LIST + 4 * i), m.r32(TOTAL_SORTED_LIST + 4 * j));
            m.w32(TOTAL_SORTED_LIST + 4 * i, b);
            m.w32(TOTAL_SORTED_LIST + 4 * j, a);
        }
        for &car in &cars {
            car.set_ai_slot(m, pick(6));
            let behind = [Car(0), Car(0), any_car(&mut pick)][pick(3) as usize];
            car.set_fall_behind_car(m, behind);
            let ahead = any_car(&mut pick);
            car.set_next_ai_racer(m, ahead);
            car.set_caravan_timer(m, pick(200) - 50);
            car.set_caravan_follow_behind_distance_meters(m, pick(0x400_0000));
            car.set_damage_mult(m, pick(0x1_0000));
            car.set_speed_nitrous(m, [0x1_0000, 0x1_3333][pick(2) as usize]);
            car.set_speed_factor(m, 0xC000 + pick(0x8000));
            car.set_btc_glue_modifier(m, pick(0x1_0000));
            car.set_cop_top_speed(m, pick(0x40_0000));
            car.set_traffic_speed_randomizing_factor(m, 0xC000 + pick(0x8000));
            car.set_rs_control(m, [0, 0, 1][pick(3) as usize]);
            car.set_original_desired_speed(m, pick(0x40_0000) - 0x20_0000);
            car.set_desired_speed(m, pick(0x40_0000) - 0x20_0000);
            car.set_speed(m, pick(0x40_0000));
            car.set_current_speed(m, pick(0x40_0000) - 0x20_0000);
            let basis = cars[pick(n as u32) as usize];
            car.set_basis_car(m, basis);
            car.n().set_total_slice(m, [pick(200), pick(4000)][pick(2) as usize] as u16);
            let slices = NUM_SLICES.get(m) as u32;
            car.n().sim_road_info().set_slice(m, pick(slices) as i16);
            let flip = pick(4) == 0;
            car.update_car_flags(m, |f| if flip { f ^ CAR_AI_RACER } else { f });
            for z in 0..10 {
                let d = [0, pick(0x8000), pick(0x10_0000)][pick(3) as usize];
                car.n().set_damage(m, z, d);
            }
        }
        for k in 0..4 {
            m.w32(LEADER_BOARD + 4 * k, cars[pick(n as u32) as usize].0);
        }
        NUM_AI_RACE_CARS.set(m, pick(6));
        NUM_HUMAN_RACE_CARS.set(m, [1, 1, 2][pick(3) as usize]);
        AI_ELAPSED_TIME.set(m, [1, 2, 4][pick(3) as usize]);
        AISPEEDS_TRACK_AND_NIGHT_MULT.set(m, 0xE000 + pick(0x4000));
        GAME_SETUP.set_tournament_multiplier(m, 0xE000 + pick(0x4000));
        GAME_SETUP.set_num_laps(m, pick(4) + 1);
        GAME_SETUP.set_reverse_track(m, pick(2));
        GAME_SETUP.set_race_type(m, [0, 1, 2, 3, 5][pick(5) as usize]);
        for i in 0..3u32 {
            for k in [0x3C, 0x40, 0x44] {
                let info = Car(m.r32(CARS_LIST + 4 * i)).car_info(m).0;
                m.w32(info + k, pick(2) as u32);
            }
        }
        m.w32(SCRATCH + 4, pick(0x20_0000) as u32);
        m.w32(SCRATCH + 8, pick(0x40_0000) as u32);
        m.w32(SCRATCH + 12, pick(n as u32) as u32);
        let slices = NUM_SLICES.get(m) as u32;
        m.w32(SCRATCH + 16, pick(slices) as u32);
    }

    fn arg(c: &mut Cpu, k: u32) -> u32 {
        c.m.r32(SCRATCH + 4 * k)
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aispeeds_fuzz() {
        let cases: [(&str, fn(&mut Cpu), &[u32], Port); 26] = [
            ("AISpeeds_SuperDuperSpeedUp", nfs4rc::f_8006e09c, &[0], |c, a| Some(super_duper_speed_up(&mut c.m, Car(a[0])) as u32)),
            ("AISpeeds_NeedToSlowDownForCurve", nfs4rc::f_8006e690, &[0, 1, 2, 1], |c, a| {
                Some(need_to_slow_down_for_curve(&mut c.m, Car(a[0]), a[1] as i32, a[2] as i32, a[3] as i32) as u32)
            }),
            ("AISpeeds_CalcOpponentCurveSpeed", nfs4rc::f_8006e734, &[0], |c, a| Some(calc_opponent_curve_speed(c, Car(a[0])) as u32)),
            ("AISpeeds_BTCGetGlueFactor", nfs4rc::f_8006e89c, &[0], |c, a| Some(btc_get_glue_factor(c, Car(a[0])) as u32)),
            ("AISpeeds_GetNextAICar", nfs4rc::f_8006ea58, &[0], |c, a| Some(get_next_ai_car(&mut c.m, Car(a[0])).0)),
            ("AISpeeds_GetPrevAICar", nfs4rc::f_8006ea64, &[0], |c, a| Some(get_prev_ai_car(&mut c.m, Car(a[0])).0)),
            ("AISpeeds_GetCaravanFactor", nfs4rc::f_8006ead0, &[0], |c, a| Some(get_caravan_factor(c, Car(a[0])) as u32)),
            ("AISpeeds_GetGlueFactor", nfs4rc::f_8006ee8c, &[0], |c, a| Some(get_glue_factor(c, Car(a[0])) as u32)),
            ("AISpeeds_GetDamageFactor", nfs4rc::f_8006f098, &[0], |c, a| Some(get_damage_factor(&mut c.m, Car(a[0])) as u32)),
            ("AISpeeds_LimitGlueMultiplier", nfs4rc::f_8006f1b0, &[0, 1], |c, a| Some(limit_glue_multiplier(c, Car(a[0]), a[1] as i32) as u32)),
            ("AISpeeds_CalcCopTopSpeed", nfs4rc::f_8006f2a4, &[0], |c, a| Some(calc_cop_top_speed(c, Car(a[0])) as u32)),
            ("AISpeeds_CalcTrafficTopSpeed", nfs4rc::f_8006f36c, &[0], |c, a| Some(calc_traffic_top_speed(c, Car(a[0])) as u32)),
            ("AISpeeds_GetLegalSpeed", nfs4rc::f_8006f50c, &[4], |c, a| Some(get_legal_speed(&mut c.m, a[0] as i32) as u32)),
            ("AISpeeds_RandomizeTrafficSpeed", nfs4rc::f_8006f550, &[0, 2], |c, a| Some(randomize_traffic_speed(&mut c.m, Car(a[0]), a[1] as i32) as u32)),
            ("AISpeeds_CalcDesiredSpeed", nfs4rc::f_8006f5dc, &[0], |c, a| {
                calc_desired_speed(c, Car(a[0]));
                None
            }),
            ("AISpeeds_CalculateOncomingCarSpeed", nfs4rc::f_8006f680, &[0], |c, a| Some(calculate_oncoming_car_speed(&mut c.m, Car(a[0])) as u32)),
            ("AISpeeds_SetTrafficSpeedRandomFactor", nfs4rc::f_8006f728, &[0], |c, a| {
                set_traffic_speed_random_factor(c, Car(a[0]));
                None
            }),
            ("AISpeeds_MaintainLeaderBoard", nfs4rc::f_8006f7a4, &[], |c, _| {
                maintain_leader_board(&mut c.m);
                None
            }),
            ("AISpeeds_GetScriptFactor", nfs4rc::f_8006f8a4, &[0], |c, a| Some(get_script_factor(c, Car(a[0])) as u32)),
            ("AISpeeds_GetUpgradeAccMult", nfs4rc::f_8006f90c, &[3], |c, a| Some(get_upgrade_acc_mult(&mut c.m, a[0] as i32) as u32)),
            ("AISpeeds_GetUpgradeBrakeMult", nfs4rc::f_8006f9c8, &[3], |c, a| Some(get_upgrade_brake_mult(&mut c.m, a[0] as i32) as u32)),
            ("AISpeeds_GetUpgradeHandlingMult", nfs4rc::f_8006fa84, &[3], |c, a| Some(get_upgrade_handling_mult(&mut c.m, a[0] as i32) as u32)),
            ("AISpeeds_GetUpgradeTopSpeedMult", nfs4rc::f_8006fb40, &[3], |c, a| Some(get_upgrade_top_speed_mult(&mut c.m, a[0] as i32) as u32)),
            ("AISpeeds_CalcHumanTopSpeed", nfs4rc::f_8006fbfc, &[0], |c, a| Some(calc_human_top_speed(c, Car(a[0])) as u32)),
            ("AISpeeds_CalcHumanCurveSpeed", nfs4rc::f_8006fc1c, &[0], |c, a| Some(calc_human_curve_speed(c, Car(a[0])) as u32)),
            ("AISpeeds_CalcOpponentTopSpeed", nfs4rc::f_8006e138, &[0, 100], |c, a| Some(calc_opponent_top_speed(c, Car(a[0]), a[1]) as u32)),
        ];
        for (k, (name, original, which, port)) in cases.into_iter().enumerate() {
            let args = move |c: &mut Cpu| {
                let car = arg(c, 0);
                // (the out-parameter: the car's original wanted speed)
                which.iter().map(|&w| if w == 100 { car + 0x560 } else { arg(c, w) }).collect()
            };
            against_original(name, 4_000, 0xA20 + k as u64, shuffle, original, &args, port);
        }
    }

    /// The stack the AI's speeds leave (the rest of the AI, still recompiled, reads it).
    #[test]
    fn desired_speed_stack_fuzz() {
        let top = crate::test_util::SCRATCH_TOP;
        crate::test_util::same_stack("AISpeeds_CalcDesiredSpeed", 0x8006_f5dc, nfs4rc::f_8006f5dc, 0x8007_0000, top, 5_000, 0x57A3, shuffle, &|c, _| vec![arg(c, 0)]);
    }
}
