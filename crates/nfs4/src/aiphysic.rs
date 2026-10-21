//! AIPHYSIC.CPP: how the AI's cars move. Each tick (at the rate their distance from the players
//! allows) a car's speed and lane come from the AI's decisions; near the players and for the
//! police it drives with the "cool" physics (toward a point down the road, in or out of
//! control), elsewhere with the simple physics, which moves it along the road and across it
//! directly. The engine revs and the gears follow the speed; blowouts, pulling over, finishing
//! and flying stop the car.

use crate::abi::{self, Saved};
use crate::fixed::{fixeddiv, fixedmult};
use crate::globals::{
    AIDATARECORD_TRACK_CURVE, AIPHYSIC_CONFIG, HUMAN_COP_HIT_BRAKE, HUMAN_COP_HIT_GRIP, HUMAN_COP_HIT_TICKS, AIPHYSIC_ELAPSED_TIME, AIPHYSIC_I_TIME, AIPHYSIC_TIME, AITUNE_BTC_PERP_ACC_MULTS,
    BWORLDSM_SLICES, GAME_SETUP, HUMAN_RACE_CARS, NUM_HUMAN_RACE_CARS, NUM_SLICES, SIM_COUNTER, SIM_GLOBAL, TOP_SPEED_CAP,
};
use crate::types::{AIPhysicBrakeInfo, Car, Coord, TrkNewSlice};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`.
const CAR_RACING: i32 = 0x1;
const CAR_AI: i32 = 0x2;
const CAR_HUMAN: i32 = 0x4;
const CAR_AI_RACER: i32 = 0x8;
const CAR_TRAFFIC: i32 = 0x10;
const CAR_COP: i32 = 0x20;
const CAR_HUMAN_COP: i32 = 0x200;
/// Out of the race (stopped wherever it is when it flies).
const CAR_OUT: i32 = 0x400;
/// Always the cool physics.
const CAR_COOL_PHYSICS: i32 = 0x800;

/// `Car_tObj.AIFlags`: the car is parked (no physics at all).
const AI_PARKED: i32 = 0x4;
/// `Car_tObj.AIFlags`: the cool physics ran last time (the simple physics then starts its
/// lane ramp from where the car is).
const AI_COOL_LAST: i32 = 0x10;

/// `Car_tStats.finishType`: finished.
const FINISHED: i32 = 2;
/// `Cars_tRenderInfo.signalLight`: blinking.
const SIGNAL_ON: i16 = 0x80;
/// `GameSetup_tData.raceType`: the pursuit races (a human can be a cop).
const RACE_PURSUIT: i32 = 1;
const RACE_PURSUIT_2: i32 = 5;
/// The car types `Cars_topSpeedCap` has (the traffic and police after).
const CAPPED_TYPES: i32 = 22;

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

/// The slice the car is on.
fn car_slice(m: &mut Mem, car: Car) -> TrkNewSlice {
    let slice = car.n().sim_road_info().slice(m) as i32;
    slice_at(m, slice)
}

/// `a * b` as two 8.8 numbers multiply (each truncated to 1/256 first).
fn mul_8_8(a: i32, b: i32) -> i32 {
    (a / 256).wrapping_mul(b / 256)
}

/// The paved road's half widths at a slice (16.16 metres): left (negative) and right of the
/// centre line, its lanes times their average widths.
fn paved(m: &mut Mem, s: TrkNewSlice) -> (i32, i32) {
    let lanes = s.lane_count(m) as i32;
    let left = ((s.avg_paved_width_lf(m) as i32) << 15).wrapping_mul(lanes >> 4);
    let right = ((s.avg_paved_width_rt(m) as i32) << 15).wrapping_mul(lanes & 0xF);
    (left.wrapping_neg(), right)
}

/// `x / 6` as the compiler divides by 6 (the high word of `x * 0x2aaaaaab`), and shifted
/// down `shift` bits before its sign is taken off.
fn div6(x: i32, shift: u32) -> i32 {
    let hi = ((x as i64 * 0x2AAA_AAAB) >> 32) as i32;
    (hi >> shift).wrapping_sub(x >> 31)
}

/// The slice `ahead` slices on from `slice` (behind when negative), round the track.
fn slice_ahead(m: &mut Mem, slice: i32, ahead: i32) -> i32 {
    let s = slice.wrapping_add(ahead);
    if ahead >= 0 {
        if s < NUM_SLICES.get(m) { s } else { s.wrapping_sub(NUM_SLICES.get(m)) }
    } else if s < 0 {
        s.wrapping_add(NUM_SLICES.get(m))
    } else {
        s
    }
}

/// `AIPhysic_StopCar` (AIPHYSIC.CPP, 0x80069a38): the car slowed (its velocity scaled by
/// `vel_factor`, its yaw rate by `spin_factor`, both 16.16), its accelerations cleared.
pub fn stop_car(m: &mut Mem, car: Car, vel_factor: i32, spin_factor: i32) {
    for acc in [car.angular_acc(), car.linear_acc_ch(), car.linear_acc_rh()] {
        acc.set(m, Vec3::default());
    }
    let f = vel_factor / 256;
    let vel = car.n().linear_vel();
    vel.update_x(m, |v| f.wrapping_mul(v) / 256);
    vel.update_y(m, |v| f.wrapping_mul(v) / 256);
    vel.update_z(m, |v| f.wrapping_mul(v) / 256);
    car.n().angular_vel().update_y(m, |v| mul_8_8(v, spin_factor));
}

pub fn abi_stop_car(c: &mut Cpu) {
    stop_car(&mut c.m, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32);
}

/// `AIPhysic_RevEngine` (AIPHYSIC.CPP, 0x80069afc): a car waiting at the start revs its engine,
/// up and down (odd and even grid places at different rates), between 500 rpm and the
/// redline.
pub fn rev_engine(m: &mut Mem, car: Car) {
    let red_line = car.red_line(m) / 0x1_0000;
    let rate = (car.car_index(m) % 2 + 1).wrapping_mul(AIPHYSIC_ELAPSED_TIME.get(m));
    let mut step = rate.wrapping_mul(140);
    if car.flywheel_rpm(m) & 1 != 0 {
        step = (step >> 1).wrapping_neg();
        if step & 1 != 0 {
            step = step.wrapping_sub(1);
        }
    }
    let rpm = car.flywheel_rpm(m).wrapping_add(step);
    car.set_flywheel_rpm(m, rpm);
    if red_line < rpm {
        car.set_flywheel_rpm(m, red_line / 2 | 1);
    }
    if car.flywheel_rpm(m) < 501 {
        car.set_flywheel_rpm(m, 500);
    }
}

pub fn abi_rev_engine(c: &mut Cpu) {
    rev_engine(&mut c.m, Car(c.r[4]));
}

/// `AIPhysic_HandleShifting` (AIPHYSIC.CPP, 0x80069bcc): the gear for the speed (none while a
/// shift lasts), and the engine's rpm an eighth of the way to where the speed puts it in that
/// gear (3000 mid-shift).
pub fn handle_shifting(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 3000;
        r.0[1] = car.0;
        let mut target = 3000;
        let timer = car.ai_shift_timer(&mut c.m);
        if timer > 0 {
            let elapsed = AIPHYSIC_ELAPSED_TIME.get(&mut c.m);
            car.set_ai_shift_timer(&mut c.m, timer.wrapping_sub(elapsed));
        } else {
            let gear = r.enter(c, 0x8006_9d34, 0x8006_9c14, |c| calculate_gear(c, car));
            let m = &mut c.m;
            let speed = car.current_speed(m).wrapping_abs();
            let over = speed.wrapping_sub(gear_bottom_speed(m, car, gear));
            // (the rpm range of the gear: from 0 in the first two, 4000 from the third on)
            let low = if gear < 3 { 0 } else { 0x0FA0_0000 };
            let range = gear_inv_speed_range(m, car, gear);
            let x = mul_8_8(over, range) / 256;
            let x = x.wrapping_mul(car.red_line(m).wrapping_sub(low) / 256);
            target = low.wrapping_add(x) / 0x1_0000;
            car.control().set_gear(m, gear as i8);
        }
        let m = &mut c.m;
        let rpm = car.flywheel_rpm(m);
        if rpm < target {
            car.set_flywheel_rpm(m, rpm.wrapping_add(target.wrapping_sub(rpm) / 8));
        } else if target < rpm {
            car.set_flywheel_rpm(m, rpm.wrapping_sub(rpm.wrapping_sub(target) / 8));
        }
    })
}

pub fn abi_handle_shifting(c: &mut Cpu) {
    handle_shifting(c, Car(c.r[4]));
}

/// `AIPhysic_CalculateGear` (AIPHYSIC.CPP, 0x80069d34): the gear for the car's speed: first
/// below 2 m/s, neutral driving backwards; else from the current gear up or down until the
/// speed is in its range (no higher than the top gear), each change starting a shift.
pub fn calculate_gear(c: &mut Cpu, car: Car) -> i32 {
    let m = &mut c.m;
    let speed = car.current_speed(m).wrapping_abs();
    let mut gear = car.control().gear(m) as u8 as i32;
    if speed <= 0x1_FFFF {
        return 1;
    }
    if car.drive_direction(m) == -1 {
        return 0;
    }
    loop {
        let too_fast = gear_top_speed(m, car, gear) < speed;
        if !too_fast && speed >= gear_bottom_speed(m, car, gear) {
            return gear;
        }
        if too_fast {
            if gear_top_speed(m, car, gear + 1) == 0 {
                return gear;
            }
            gear += 1;
        } else {
            gear -= 1;
        }
        let duration = car.ai_shift_duration(m);
        car.set_ai_shift_timer(m, duration);
    }
}

pub fn abi_calculate_gear(c: &mut Cpu) {
    c.r[2] = calculate_gear(c, Car(c.r[4])) as u32;
}

/// `topSpeeds[i]` and `invTopSpeeds[i]` (the original reads past them for gears past the top).
fn top_speed(m: &mut Mem, car: Car, i: i32) -> i32 {
    m.r32(car.0.wrapping_add(0x598).wrapping_add((i as u32).wrapping_mul(4))) as i32
}

fn inv_top_speed(m: &mut Mem, car: Car, i: i32) -> i32 {
    m.r32(car.0.wrapping_add(0x5b4).wrapping_add((i as u32).wrapping_mul(4))) as i32
}

/// `AIPhysic_GearInvSpeedRange` (AIPHYSIC.CPP, 0x80069e38): 1 / the speed range of `gear`
/// (from second up; first and neutral as second).
pub fn gear_inv_speed_range(m: &mut Mem, car: Car, gear: i32) -> i32 {
    inv_top_speed(m, car, gear.max(2) - 2)
}

pub fn abi_gear_inv_speed_range(c: &mut Cpu) {
    c.r[2] = gear_inv_speed_range(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_GearTopSpeed` (AIPHYSIC.CPP, 0x80069e60): the top speed in `gear` (first and
/// neutral as second).
pub fn gear_top_speed(m: &mut Mem, car: Car, gear: i32) -> i32 {
    top_speed(m, car, gear.max(2) - 2)
}

pub fn abi_gear_top_speed(c: &mut Cpu) {
    c.r[2] = gear_top_speed(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_GearBottomSpeed` (AIPHYSIC.CPP, 0x80069e88): the lowest speed for `gear` (the
/// gear below's top speed; 0 below the third).
pub fn gear_bottom_speed(m: &mut Mem, car: Car, gear: i32) -> i32 {
    if gear < 3 { 0 } else { top_speed(m, car, gear - 3) }
}

pub fn abi_gear_bottom_speed(c: &mut Cpu) {
    c.r[2] = gear_bottom_speed(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_CalcAcceleration` (AIPHYSIC.CPP, 0x80069eb0): the car's acceleration at `speed`:
/// none mid-shift or (for the capped car types) past their top speed with its upgrades and
/// glue, else from its acceleration table by the speed in m/s (just under the cap above it),
/// with its nitrous, its upgrades, 90% in the rain, a police car's multiplier, an AI racer's
/// in a pursuit race against a human cop by the skill, the AI script's slowdown and the
/// speed-up at the start.
pub fn calc_acceleration(c: &mut Cpu, car: Car, speed: i32) -> i32 {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = car.0;
        let mut speed = speed.wrapping_abs();
        r.0[2] = speed as u32;
        let m = &mut c.m;
        if car.ai_shift_timer(m) > 0 {
            return 0;
        }
        let kind = car.car_info(m).car_type(m);
        if kind < CAPPED_TYPES {
            let cap = m.r32(TOP_SPEED_CAP.wrapping_add((kind as u32) << 2)) as i32;
            r.0[1] = cap as u32;
            let upgraded = fixedmult(cap, car.top_speed_upgrade_mult(m));
            r.0[0] = upgraded as u32;
            if fixedmult(upgraded, car.ai_glue(m)) < speed || upgraded < speed {
                return 0;
            }
            if cap < speed {
                speed = cap.wrapping_add(-0x2_38E3);
                r.0[2] = speed as u32;
            }
        }
        let index = (speed / 0x1_0000).min(111).max(0);
        let record = car.acceleration_record(m);
        let mut acc = r.call(c, nfs4rc::f_8006d9f4, &[record, index as u32], 0x8006_9f94) as i32; // AIDataRecord_AccTable_t::Get
        let m = &mut c.m;
        let nitrous = car.acc_nitrous(m);
        if nitrous != 0x1_0000 {
            acc = mul_8_8(acc, nitrous);
        }
        acc = mul_8_8(acc, car.acc_upgrade_mult(m));
        if GAME_SETUP.weather(m) > 0 {
            acc = (acc / 256).wrapping_mul(230);
        }
        if car.car_flags(m) & CAR_COP != 0 {
            acc = mul_8_8(acc, car.cop_acc_mult(m));
        }
        let race = GAME_SETUP.race_type(m);
        if race == RACE_PURSUIT || race == RACE_PURSUIT_2 {
            let human_cop = |m: &mut Mem, i: u32| Car(m.r32(HUMAN_RACE_CARS + 4 * i)).car_flags(m) & CAR_HUMAN_COP != 0;
            let against_cop = human_cop(m, 0) || (NUM_HUMAN_RACE_CARS.get(m) == 2 && human_cop(m, 1));
            if against_cop && car.car_flags(m) & CAR_AI_RACER != 0 {
                let skill = GAME_SETUP.skill(m) as u32;
                let mult = m.r32(AITUNE_BTC_PERP_ACC_MULTS.0.wrapping_add(skill.wrapping_mul(4))) as i32;
                acc = mul_8_8(acc, mult);
            }
        }
        r.0[0] = acc as u32;
        acc = r.enter(c, 0x8006_a190, 0x8006_a148, |c| modify_acceleration_according_to_script(c, car, acc));
        let boost = crate::aispeeds::super_duper_speed_up(&mut c.m, car);
        if boost != 0x1_0000 {
            acc = fixedmult(acc, boost);
        }
        acc
    })
}

pub fn abi_calc_acceleration(c: &mut Cpu) {
    c.r[2] = calc_acceleration(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_ModifyAccelerationAccordingToScript` (AIPHYSIC.CPP, 0x8006a190): `acc` at 1.5
/// times when the AI script's reaction 4 is on.
pub fn modify_acceleration_according_to_script(c: &mut Cpu, car: Car, acc: i32) -> i32 {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = acc as u32;
        let on = r.call(c, nfs4rc::f_80070170, &[car.script().0, 4], 0x8006_a1ac) as i32; // AIScript_DoReAction
        if on == -1 { acc } else { (acc / 256).wrapping_mul(384) }
    })
}

pub fn abi_modify_acceleration_according_to_script(c: &mut Cpu) {
    c.r[2] = modify_acceleration_according_to_script(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_CalcDeceleration` (AIPHYSIC.CPP, 0x8006a1e8): the car's braking, more with its
/// glue over 1.
pub fn calc_deceleration(m: &mut Mem, car: Car) -> i32 {
    let decel = car.brake_info(m).deceleration_(m);
    let glue = car.ai_glue(m);
    if 0x1_0000 < glue { mul_8_8(decel, glue) } else { decel }
}

pub fn abi_calc_deceleration(c: &mut Cpu) {
    c.r[2] = calc_deceleration(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIPhysic_CheckDesiredDirection` (AIPHYSIC.CPP, 0x8006a234): the car turns the way it
/// wants to go once it is nearly stopped (under 2.5 m/s).
pub fn check_desired_direction(m: &mut Mem, car: Car) {
    if car.speed(m) < 0x8_E38E {
        let dir = car.desired_direction(m);
        car.set_direction(m, dir);
    }
}

pub fn abi_check_desired_direction(c: &mut Cpu) {
    check_desired_direction(&mut c.m, Car(c.r[4]));
}

/// `AIPhysic_HandleSignalling` (AIPHYSIC.CPP, 0x8006a27c): traffic indicates the way it changes
/// lanes, while it is more than 4 m from where it wants to be.
pub fn handle_signalling(m: &mut Mem, car: Car) {
    if car.car_flags(m) & CAR_TRAFFIC == 0 {
        return;
    }
    let (at, want) = (car.road_position(m), car.desired_lat_pos(m));
    let signal = car.render();
    if want < at.wrapping_add(-0x4_0000) {
        signal.set_signal_light(m, 1, 0);
        signal.update_signal_light(m, 0, |v| v | SIGNAL_ON);
    } else if at.wrapping_add(0x4_0000) < want {
        signal.set_signal_light(m, 0, 0);
        signal.update_signal_light(m, 1, |v| v | SIGNAL_ON);
    } else {
        signal.set_signal_light(m, 0, 0);
        signal.set_signal_light(m, 1, 0);
    }
}

pub fn abi_handle_signalling(c: &mut Cpu) {
    handle_signalling(&mut c.m, Car(c.r[4]));
}

/// `AIPhysic_HandleWipeoutTimer` (AIPHYSIC.CPP, 0x8006a2f0): when an AI racer's next wipeout
/// is due, the one after it, at the personality's interval plus a random part.
pub fn handle_wipeout_timer(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    if car.car_flags(m) & CAR_AI_RACER == 0 {
        return;
    }
    let now = SIM_COUNTER.get(m);
    if car.wipe_out_start_tick(m) < now {
        let rand = crate::physics::fast_random(c) as i32;
        let m = &mut c.m;
        let p = car.personality(m);
        let next = now.wrapping_add(p.minimum_between_wipeout_ticks(m)).wrapping_add(rand & p.random_between_wipeout_ticks(m));
        car.set_wipe_out_start_tick(m, next);
    }
}

pub fn abi_handle_wipeout_timer(c: &mut Cpu) {
    handle_wipeout_timer(c, Car(c.r[4]));
}

/// `AIPhysic_Main` (AIPHYSIC.CPP, 0x8006a384, an AI car's scheduled physics): when its
/// distance lets it run this tick, the speed it wants, its grip, indicators, sideways speed
/// and the lane it ramps toward; then (unless it is parked or inactive) the engine revving
/// before the start, or the gears and the physics: stopped while flying (if out of the race),
/// upside down, on a blown tyre, pulling over, finished or wanting no speed, else the cool or
/// the simple physics, put back on the road when it is lost, and the next wipeout.
pub fn main(c: &mut Cpu, car: Car) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        let no_optz = car.force_no_sim_optz(m);
        let (dist, id) = (car.n().dist_to_player(m), car.n().obj_id(m));
        for (k, v) in [AIPHYSIC_TIME.0, AIPHYSIC_ELAPSED_TIME.0, AIPHYSIC_I_TIME.0, no_optz as u32].into_iter().enumerate() {
            m.w32(sp + 16 + 4 * k as u32, v);
        }
        if !crate::schedule::execute_check(c, 1, 1, dist, id, AIPHYSIC_TIME.0, AIPHYSIC_ELAPSED_TIME.0, AIPHYSIC_I_TIME.0, no_optz) {
            return;
        }
        if car.car_flags(&mut c.m) & CAR_AI != 0 {
            crate::aiworld::calc_speed(&mut c.m, car);
        }
        check_desired_direction(&mut c.m, car);
        r.enter(c, 0x8006_d2a8, 0x8006_a40c, |c| check_for_grip_reduction(c, car));
        handle_signalling(&mut c.m, car);
        let lat = r.enter(c, 0x8007_41fc, 0x8006_a41c, |c| crate::aiworld::calc_lateral_velocity(&mut c.m, car));
        car.set_lateral_velocity(&mut c.m, lat);
        let cool = use_cool_physics(&mut c.m, car);
        r.0[1] = cool as u32;
        r.enter(c, 0x8006_cf68, 0x8006_a43c, |c| calculate_ramped_desired_lat_pos(c, car, cool as i32));
        let m = &mut c.m;
        if car.ai_flags(m) & AI_PARKED != 0 || car.n().active(m) == 0 {
            return;
        }
        if SIM_GLOBAL.game_started(m) != 1 {
            rev_engine(m, car);
            return;
        }
        r.enter(c, 0x8006_9bcc, 0x8006_a488, |c| handle_shifting(c, car));
        let m = &mut c.m;
        if car.n().flight_time(m) != 0 {
            if car.car_flags(m) & CAR_OUT != 0 {
                stop_car(m, car, 0xFAE1, 0xFAE1);
            }
            return;
        }
        if car.speed(m) <= 0x3_FFFF {
            car.linear_vel_ch().set_z(m, 0);
            car.set_rear_skid(m, 0);
            car.set_front_skid(m, 0);
        }
        // (on its roof)
        if car.n().flight_time(m) == 0 && car.n().orient_mat().m(m, 4) < 19660 {
            stop_car(m, car, 0xF0A3, 0xFAE1);
            return;
        }
        if car.blowout(m) != 0 {
            stop_car(m, car, 0xFD70, 0xFAE1);
            return;
        }
        if car.pull_over(m) != 0 {
            let speed = car.speed(m);
            if speed <= 0x2_38E2 {
                stop_car(m, car, 0xE666, 0xE666);
                return;
            }
            if speed <= 0x8_E38D {
                stop_car(m, car, 0xF5C2, 0xFAE1);
                return;
            }
        }
        let finished = car.car_flags(m) & CAR_RACING != 0 && car.stats().finish_type(m) == FINISHED;
        if (finished && car.current_speed(m).wrapping_abs() <= 0x1_FFFF) || car.desired_speed(m) == 0 {
            stop_car(m, car, 0xF0A3, 0xFAE1);
            return;
        }
        if cool {
            r.enter(c, 0x8006_af18, 0x8006_a5e0, |c| cool_physics(c, car));
        } else {
            r.enter(c, 0x8006_a730, 0x8006_a5f0, |c| simple_physics(c, car));
        }
        r.enter(c, 0x8006_a614, 0x8006_a5f8, |c| check_for_bad_position(c, car));
        handle_wipeout_timer(c, car);
    })
}

pub fn abi_main(c: &mut Cpu) {
    main(c, Car(c.r[4]));
}

/// `AIPhysic_CheckForBadPosition` (AIPHYSIC.CPP, 0x8006a614): a car flung off (over 115 m/s
/// up or sideways, or along the ground when fully simulated) or more than 35 m off the side of
/// the road is put back on it.
pub fn check_for_bad_position(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let n = car.n();
        const FAST: i32 = 0x73_0000;
        let flung = FAST < n.linear_vel().z(m)
            || FAST < n.linear_vel().x(m)
            || (FAST < n.speed_xz(m) && n.sim_optz(m) == 0);
        let s = car_slice(m, car);
        let at = car.road_position(m);
        let off = at < (-0x23_0000i32).wrapping_sub((s.left_drive(m) as i32) << 8)
            || ((s.right_drive(m) as i32) << 8).wrapping_add(0x23_0000) < at;
        if flung || off {
            let r = Saved::of(c);
            r.enter(c, 0x8008_7088, 0x8006_a6dc, |c| crate::cars::reset_collided_cars(c, car, 1));
        }
    })
}

pub fn abi_check_for_bad_position(c: &mut Cpu) {
    check_for_bad_position(c, Car(c.r[4]));
}

/// `AIPhysics_UseCoolPhysics` (AIPHYSIC.CPP, 0x8006a6ec): whether the car drives with the cool
/// physics: when it always does, when it is fully simulated, and police cars also at the
/// first level of simplification.
pub fn use_cool_physics(m: &mut Mem, car: Car) -> bool {
    let flags = car.car_flags(m);
    let optz = car.n().sim_optz(m);
    flags & CAR_COOL_PHYSICS != 0 || optz == 0 || (flags & CAR_COP != 0 && optz < 2)
}

pub fn abi_use_cool_physics(c: &mut Cpu) {
    c.r[2] = use_cool_physics(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIPhysic_SimplePhysics` (AIPHYSIC.CPP, 0x8006a730): a car far from the players moved
/// straight along the road and across it: its lane ramp starting from where it is after the
/// cool physics, an AI racer's wipeout (6 s slower) when one is due on a sharp bend at speed,
/// its speed along and across the road, its velocity from them and the road's directions,
/// facing down the road with no spin, its sideways slide, and the slice it looks ahead to
/// (its speed in slices, within the configured range).
pub fn simple_physics(c: &mut Cpu, car: Car) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        if car.ai_flags(m) & AI_COOL_LAST != 0 {
            let at = car.road_position(m);
            car.set_ramp_desired_lat_pos(m, at);
            car.update_ai_flags(m, |f| f & !AI_COOL_LAST);
        }
        if car.car_flags(m) & CAR_AI_RACER != 0 {
            r.0[1] = SIM_GLOBAL.0;
            if car.wipe_out_start_tick(m) < SIM_COUNTER.get(m) {
                let (curves, slice) = (AIDATARECORD_TRACK_CURVE.get(m), car.n().sim_road_info().slice(m) as i32);
                let curve = r.call(c, nfs4rc::f_8006dba4, &[curves, slice as u32], 0x8006_a7b8) as i32; // AIDataRecord_TrackCurve_t::Get
                let m = &mut c.m;
                if curve >= 31 && 0x16_38E3 < car.speed(m) {
                    let end = SIM_COUNTER.get(m).wrapping_add(384);
                    car.set_wipe_out_end_tick(m, end);
                }
            }
        }
        r.enter(c, 0x8006_aa9c, 0x8006_a7f0, |c| simple_physics_long_vel(c, car));
        r.enter(c, 0x8006_adf4, 0x8006_a7f8, |c| simple_physics_lat_vel(c, car));
        let m = &mut c.m;
        let n = car.n();
        let (fwd, right) = (Coord(sp + 16), Coord(sp + 32));
        let o = n.orient_mat();
        let v = Vec3::new(o.m(m, 6), o.m(m, 7), o.m(m, 8));
        fwd.set(m, v);
        let rm = n.road_matrix();
        let v = Vec3::new(rm.m(m, 0), rm.m(m, 1), rm.m(m, 2));
        right.set(m, v);
        let lane = car.lane_change_speed(m);
        right.update_x(m, |v| fixedmult(lane, v));
        right.update_y(m, |v| fixedmult(lane, v));
        right.update_z(m, |v| fixedmult(lane, v));
        let along = car.current_speed(m).wrapping_mul(car.direction(m));
        fwd.update_x(m, |v| fixedmult(along, v));
        fwd.update_y(m, |v| fixedmult(along, v));
        fwd.update_z(m, |v| fixedmult(along, v));
        let (f, s) = (fwd.get(m), right.get(m));
        n.linear_vel().set(m, Vec3::new(f.x.wrapping_add(s.x), f.y.wrapping_add(s.y), f.z.wrapping_add(s.z)));
        let across = car.lane_change_speed(m).wrapping_mul(car.direction(m));
        let speed = car.speed(m);
        car.linear_vel_ch().set_z(m, speed);
        car.linear_vel_ch().set_x(m, across);
        let across = across.wrapping_abs();
        car.set_slide(m, if (across / 256).wrapping_mul(25) < across { 0x8000 } else { 0 });
        n.angular_vel().set(m, Vec3::default());
        if 0x3_0000 < car.speed(m) {
            let backward = (car.direction(m) == -1) as i32;
            r.enter(c, 0x800a_2100, 0x8006_a990, |c| crate::newton::copy_road_matrix_to_orient_mat(c, n, backward));
        }
        let m = &mut c.m;
        let cfg = AIPHYSIC_CONFIG;
        let speed = car.current_speed(m).wrapping_abs();
        let mut ahead = div6(speed, 16);
        if ahead >= 0 {
            let min = div6(cfg.min_lookahead(m), 0);
            if ahead < min && speed >= 0 {
                ahead = min;
            }
        }
        let max = div6(cfg.max_lookahead(m), 0);
        if max < ahead {
            ahead = max;
        }
        let ahead = ahead.wrapping_mul(car.direction(m));
        let here = n.sim_road_info().slice(m) as i32;
        let slice = slice_ahead(m, here, ahead);
        car.set_look_ahead_slice(m, slice);
    })
}

pub fn abi_simple_physics(c: &mut Cpu) {
    simple_physics(c, Car(c.r[4]));
}

/// `AIPhysic_SimplePhysics_LongVel` (AIPHYSIC.CPP, 0x8006aa9c): the speed along the road
/// toward the speed the car wants (none when pulling over, a quarter wiping out): braking
/// (the brake pedal going down) while it is too fast or wants to go the other way, else
/// accelerating (the pedal coming up), never past what it wants.
pub fn simple_physics_long_vel(c: &mut Cpu, car: Car) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let acc = r.enter(c, 0x8006_9eb0, 0x8006_aabc, |c| {
            let speed = car.current_speed(&mut c.m);
            calc_acceleration(c, car, speed)
        });
        let m = &mut c.m;
        let decel = calc_deceleration(m, car);
        if car.pull_over(m) != 0 {
            car.set_desired_speed(m, 0);
        }
        if SIM_COUNTER.get(m) < car.wipe_out_end_tick(m) {
            car.update_desired_speed(m, |v| v / 4);
        }
        let control = car.control();
        let elapsed = AIPHYSIC_ELAPSED_TIME.get(m);
        let i_time = AIPHYSIC_I_TIME.get(m);
        let brake = |m: &mut Mem| {
            let level = (control.brake_level(m) as u8 as i32).wrapping_add(elapsed.wrapping_mul(4));
            car.linear_acc_ch().set_z(m, decel);
            control.set_brake_level(m, if level < 255 { level as i8 } else { -1 });
            if control.brake_level(m) as u8 >= 4 {
                control.set_desired_brake_level(m, 1);
            }
            mul_8_8(decel, i_time)
        };
        let release = |m: &mut Mem| {
            let level = control.brake_level(m) as u8 as i32;
            if elapsed < level {
                control.set_brake_level(m, level.wrapping_sub(elapsed as u8 as i32) as i8);
            } else {
                control.set_brake_level(m, 0);
            }
        };
        let dir = car.direction(m);
        let wanted_dir = car.desired_direction(m) == dir;
        if dir == -1 {
            if car.current_speed(m) < car.desired_speed(m) || !wanted_dir {
                let speed = car.current_speed(m).wrapping_add(brake(m));
                car.set_current_speed(m, speed);
                if car.direction(m) == car.desired_direction(m) {
                    let want = car.desired_speed(m);
                    car.set_current_speed(m, if want < speed { want } else { speed });
                }
            } else {
                let step = mul_8_8(acc, i_time);
                car.update_current_speed(m, |v| v.wrapping_sub(step));
                release(m);
                if control.brake_level(m) == 0 {
                    control.set_desired_brake_level(m, 0);
                }
                car.linear_acc_ch().set_z(m, acc);
                let want = car.desired_speed(m);
                if car.current_speed(m) < want {
                    car.set_current_speed(m, want);
                }
            }
        } else if car.desired_speed(m) < car.current_speed(m) || !wanted_dir {
            let speed = car.current_speed(m).wrapping_sub(brake(m));
            car.set_current_speed(m, speed);
            if car.direction(m) == car.desired_direction(m) {
                let want = car.desired_speed(m);
                car.set_current_speed(m, if want < speed { speed } else { want });
            }
        } else {
            let step = mul_8_8(acc, i_time);
            car.update_current_speed(m, |v| v.wrapping_add(step));
            release(m);
            if control.brake_level(m) == 0 {
                control.set_desired_brake_level(m, 0);
            }
            car.linear_acc_ch().set_z(m, acc);
            let want = car.desired_speed(m);
            if want < car.current_speed(m) {
                car.set_current_speed(m, want);
            }
        }
    })
}

pub fn abi_simple_physics_long_vel(c: &mut Cpu) {
    simple_physics_long_vel(c, Car(c.r[4]));
}

/// `AIPhysic_SimplePhysics_LatVel` (AIPHYSIC.CPP, 0x8006adf4): across the road: over 25 m/s
/// the car jumps to its lane ramp's position, else it moves toward it no faster than it
/// drives (none under 3 m/s).
pub fn simple_physics_lat_vel(c: &mut Cpu, car: Car) {
    abi::frame(c, 48, |c, sp| {
        let m = &mut c.m;
        let mut to_go = 0;
        if 0x3_0000 < car.speed(m) {
            to_go = car.ramp_desired_lat_pos(m).wrapping_sub(car.road_position(m));
        }
        if 0x19_0000 < car.speed(m) {
            let rm = car.n().road_matrix();
            let step = Coord(sp + 16);
            let v = Vec3::new(rm.m(m, 0), rm.m(m, 1), rm.m(m, 2));
            step.set(m, v);
            step.update_x(m, |v| fixedmult(to_go, v));
            step.update_y(m, |v| fixedmult(to_go, v));
            step.update_z(m, |v| fixedmult(to_go, v));
            let s = step.get(m);
            let pos = car.n().position();
            pos.update_x(m, |v| v.wrapping_add(s.x));
            pos.update_y(m, |v| v.wrapping_add(s.y));
            car.set_lane_change_speed(m, 0);
            pos.update_z(m, |v| v.wrapping_add(s.z));
            return;
        }
        let limit = car.current_speed(m).wrapping_abs();
        let speed = if to_go < limit.wrapping_neg() {
            limit.wrapping_neg()
        } else if limit < to_go {
            limit
        } else {
            to_go
        };
        car.set_lane_change_speed(m, speed);
    })
}

pub fn abi_simple_physics_lat_vel(c: &mut Cpu) {
    simple_physics_lat_vel(c, Car(c.r[4]));
}

/// `AIPhysic_CoolPhysics` (AIPHYSIC.CPP, 0x8006af18): a car near the players driven toward a
/// point down the road: which way it drives (no lane ramp when reversing against its way),
/// turning back from a wall, its velocities in its own frame and the angle to the point,
/// then in or out of control, and its velocities back.
pub fn cool_physics(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        car.update_ai_flags(m, |f| f | AI_COOL_LAST);
        handle_direction(m, car);
        let m = &mut c.m;
        if car.direction(m).wrapping_mul(car.current_speed(m)) < 0 && car.drive_direction(m) != -1 {
            car.set_ramp_desired_lat_pos(m, 0);
        }
        if hit_wall_check(m, car) {
            change_direction(m, car, 64);
        }
        r.enter(c, 0x8006_b0b4, 0x8006_af8c, |c| preperation(c, car));
        let out = r.enter(c, 0x8006_ba60, 0x8006_af94, |c| check_if_out_of_control(c, car));
        if out {
            r.enter(c, 0x8006_bc00, 0x8006_afa4, |c| out_of_control_physics(c, car));
        } else {
            r.enter(c, 0x8006_c2c8, 0x8006_afb4, |c| in_control_physics(c, car));
        }
        r.enter(c, 0x8006_cb7c, 0x8006_afbc, |c| finish_up(c, car));
    })
}

pub fn abi_cool_physics(c: &mut Cpu) {
    cool_physics(c, Car(c.r[4]));
}

/// `AIPhysic_HandleDirection` (AIPHYSIC.CPP, 0x8006afcc): a car reversing keeps on reversing
/// while it moves away from its lane ramp by more than 10 m, then drives forward again after
/// its reversing time; its wanted speed within 20 m/s either way, the ramp reset.
pub fn handle_direction(m: &mut Mem, car: Car) {
    if car.drive_direction(m) != -1 {
        return;
    }
    let (ramp, at, lat) = (car.ramp_desired_lat_pos(m), car.road_position(m), car.lateral_velocity(m));
    let away = (at < ramp.wrapping_add(-0xA_0000) && lat > 0) || (ramp.wrapping_add(0xA_0000) < at && lat < 0);
    if away {
        let t = SIM_COUNTER.get(m).wrapping_sub(24);
        car.set_drive_direction_timer(m, t);
    }
    let now = SIM_COUNTER.get(m);
    if car.drive_direction_reverse_time(m) < now.wrapping_sub(car.drive_direction_timer(m)) {
        car.set_drive_direction(m, 1);
        car.set_drive_direction_timer(m, now);
    }
    if 0x14_0000 < car.desired_speed(m) {
        car.set_desired_speed(m, 0x14_0000);
    }
    if 0x14_0000 < car.desired_speed(m).wrapping_neg() {
        car.set_desired_speed(m, -0x14_0000);
    }
    car.set_ramp_desired_lat_pos(m, 0);
}

pub fn abi_handle_direction(c: &mut Cpu) {
    handle_direction(&mut c.m, Car(c.r[4]));
}

/// The car's yaw (1024ths of a turn) from its forward vector, as `intatan` gives it (called
/// recompiled for its frame), the way it drives.
fn yaw(c: &mut Cpu, r: &Saved, x: i32, z: i32, ra: u32) -> i32 {
    r.call(c, nfs4rc::f_800e6724, &[x as u32, z as u32], ra) as i32
}

/// `AIPhysic_Preperation` (AIPHYSIC.CPP, 0x8006b0b4): the car's velocity and spin in its own
/// frame, its sideways slide, the way it faces (backward when reversing) and the angle from
/// that to the point it drives toward.
pub fn preperation(c: &mut Cpu, car: Car) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        let m = &mut c.m;
        let n = car.n();
        let o = n.orient_mat();
        let dot = |m: &mut Mem, v: Coord, row: u32| {
            let a = fixedmult(v.x(m), o.m(m, 3 * row)).wrapping_add(fixedmult(v.y(m), o.m(m, 3 * row + 1)));
            a.wrapping_add(fixedmult(v.z(m), o.m(m, 3 * row + 2)))
        };
        for row in 0..3 {
            let v = dot(m, n.linear_vel(), row);
            m.w32(car.linear_vel_ch().0 + 4 * row, v as u32);
        }
        for row in 0..3 {
            let v = dot(m, n.angular_vel(), row);
            m.w32(car.angular_vel_ch().0 + 4 * row, v as u32);
        }
        // (s0 and s1 as the last products leave them)
        r.0[0] = car.angular_vel_ch().z(m) as u32;
        r.0[1] = fixedmult(n.angular_vel().y(m), o.m(m, 7)) as u32;
        let side = car.linear_vel_ch().x(m).wrapping_abs();
        car.set_slide(m, if (side / 256).wrapping_mul(25) < side { 0x8000 } else { 0 });
        let fwd = Coord(sp + 16);
        let v = Vec3::new(o.m(m, 6), o.m(m, 7), o.m(m, 8));
        fwd.set(m, v);
        let (x, z) = (fwd.x(m), fwd.z(m));
        let a_car = if car.drive_direction(m) == 1 {
            yaw(c, &r, x, z, 0x8006_b2b8)
        } else {
            yaw(c, &r, x.wrapping_neg(), z.wrapping_neg(), 0x8006_b2b8)
        };
        r.0[0] = a_car as u32;
        car.set_a_car(&mut c.m, a_car);
        r.enter(c, 0x8006_b43c, 0x8006_b2c8, |c| get_desired_vector(c, car));
        let m = &mut c.m;
        let (dx, dz) = (car.desired_vector().x(m), car.desired_vector().z(m));
        let a_desired = yaw(c, &r, dx, dz, 0x8006_b2d8);
        let m = &mut c.m;
        let d = a_car.wrapping_sub(a_desired);
        let d = if d < -512 {
            d + 1024
        } else if d < 513 {
            d
        } else {
            d - 1024
        };
        car.set_a_car_wrt_desired(m, d);
        car.set_a_desired(m, a_desired);
    })
}

pub fn abi_preperation(c: &mut Cpu) {
    preperation(c, Car(c.r[4]));
}

/// `AIPhysic_CalculateRoadPosition` (AIPHYSIC.CPP, 0x8006b31c): how far `pos` is across the
/// road from `slice`'s centre (16.16 metres, right positive).
pub fn calculate_road_position(c: &mut Cpu, pos: Coord, slice: i32) -> i32 {
    abi::frame(c, 48, |c, sp| {
        let m = &mut c.m;
        let s = slice_at(m, slice);
        let center = Coord(sp);
        let v = Vec3::new(s.center(m, 0), s.center(m, 1), s.center(m, 2));
        center.set(m, v);
        let d = Coord(sp + 16);
        let (p, o) = (pos.get(m), center.get(m));
        d.set(m, Vec3::new(p.x.wrapping_sub(o.x), p.y.wrapping_sub(o.y), p.z.wrapping_sub(o.z)));
        let right = Coord(sp + 32);
        let r = |m: &mut Mem, i| (s.right(m, i) as i32) << 9;
        let v = Vec3::new(r(m, 0), r(m, 1), r(m, 2));
        right.set(m, v);
        let (d, r) = (d.get(m), right.get(m));
        mul_8_8(r.x, d.x).wrapping_add(mul_8_8(r.y, d.y)).wrapping_add(mul_8_8(r.z, d.z))
    })
}

pub fn abi_calculate_road_position(c: &mut Cpu) {
    c.r[2] = calculate_road_position(c, Coord(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIPhysic_TargetedGetDesiredVector` (AIPHYSIC.CPP, 0x8006ba24): the point to drive toward
/// is the car's target, its lane ramp the target's.
pub fn targeted_get_desired_vector(m: &mut Mem, car: Car) {
    let (t, p) = (car.target_pos().get(m), car.n().position().get(m));
    car.desired_vector().set(m, Vec3::new(t.x.wrapping_sub(p.x), t.y.wrapping_sub(p.y), t.z.wrapping_sub(p.z)));
    let lat = car.target_lat_pos(m);
    car.set_ramp_desired_lat_pos(m, lat);
}

pub fn abi_targeted_get_desired_vector(c: &mut Cpu) {
    targeted_get_desired_vector(&mut c.m, Car(c.r[4]));
}

/// `AIPhysic_CheckIfOutOfControl` (AIPHYSIC.CPP, 0x8006ba60): whether the car drives out of
/// control: while wiping out, when slow (under 3 m/s); else when it faces more than 40 degrees
/// off its way, reverses, is slow for the out-of-control model, is off the paved road, on a
/// sharp bend, or slides sideways at more than 40% of its speed (under 15 m/s).
pub fn check_if_out_of_control(c: &mut Cpu, car: Car) -> bool {
    abi::frame(c, 24, |c, _| {
        let dir = car.direction(&mut c.m);
        let bend = crate::aiworld::calc_road_bend(&mut c.m, car, dir << 3);
        let bend = bend.wrapping_abs();
        let m = &mut c.m;
        if SIM_COUNTER.get(m) < car.wipe_out_end_tick(m) {
            return car.current_speed(m).wrapping_abs() <= 0x2_FFFF;
        }
        if car.a_car_wrt_desired(m).wrapping_abs() >= 116 || car.drive_direction(m) == -1 {
            return true;
        }
        let speed = car.current_speed(m).wrapping_abs();
        if speed < AIPHYSIC_CONFIG.ooc_model().vel_limit_range(m).wrapping_add(-0x3_0000) {
            return true;
        }
        let s = car_slice(m, car);
        let (left, right) = paved(m, s);
        let at = car.road_position(m);
        if at < left || right < at || 0xCCCC < bend {
            return true;
        }
        let side = car.linear_vel_ch().x(m).wrapping_abs();
        (car.speed(m) / 256).wrapping_mul(102) < side && speed <= 0xE_FFFF
    })
}

pub fn abi_check_if_out_of_control(c: &mut Cpu) {
    c.r[2] = check_if_out_of_control(c, Car(c.r[4])) as u32;
}

/// `x / d` as the compiler divides by a constant: the high word of `x * magic` (plus `x`
/// when `add`), shifted down `shift` bits, its sign taken off.
fn magic_div(x: i32, magic: u32, add: bool, shift: u32) -> i32 {
    let hi = ((x as i64 * magic as i32 as i64) >> 32) as i32;
    let hi = if add { hi.wrapping_add(x) } else { hi };
    (hi >> shift).wrapping_sub(x >> 31)
}

/// `x` within `-limit..=limit`, as the original's two comparisons put it.
fn limit(x: i32, limit: i32) -> i32 {
    x.min(limit).max(limit.wrapping_neg())
}

/// `AIPhysic_GetDesiredVector` (AIPHYSIC.CPP, 0x8006b43c): the point the car drives toward
/// (its target's, when it has one): its lane ramp's position on the slice it looks ahead to,
/// at its speed a slice per 6 m/s plus half a slice (within the configured range), nearer on
/// the sharper bends of a narrow road; nearer again (by a fifth, at least 4 slices, 4 more
/// times) while that point is off the paved road and the lanes toward it can't be driven.
pub fn get_desired_vector(c: &mut Cpu, car: Car) {
    abi::frame(c, 88, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        r.0[4] = 0;
        let m = &mut c.m;
        let slice = car.n().sim_road_info().slice(m) as i32;
        r.0[3] = slice as u32;
        let t = car.target_pos().get(m);
        if t.x != 0 || t.y != 0 || t.z != 0 {
            targeted_get_desired_vector(m, car);
            return;
        }
        let speed = car.current_speed(m).wrapping_abs();
        r.0[0] = speed as u32;
        let cfg = AIPHYSIC_CONFIG;
        let mut ahead = div6(fixedmult(speed, 0xB333).wrapping_add(0x3_0000), 16);
        if ahead >= 0 {
            let min = div6(cfg.min_lookahead(m), 0);
            if ahead < min && speed >= 0 {
                ahead = min;
            }
        }
        let max = div6(cfg.max_lookahead(m), 0);
        if max < ahead {
            ahead = max;
        }
        r.0[1] = ahead as u32;
        let slices_ahead = |m: &mut Mem, ahead: i32| ahead.wrapping_mul(car.drive_direction(m)).wrapping_mul(car.direction(m));
        let n = slices_ahead(m, ahead);
        let look = slice_ahead(m, slice, n);
        car.set_look_ahead_slice(m, look);
        let bend = crate::aiworld::calc_road_bend(&mut c.m, car, n);
        let m = &mut c.m;
        let look = car.look_ahead_slice(m);
        let s = slice_at(m, look);
        let (left, right) = paved(m, s);
        if right.wrapping_sub(left) <= 0x11_FFFF {
            // (under 18 m wide)
            let bend = bend.wrapping_abs();
            let scale = |ahead: i32, k: i32| ahead.wrapping_mul(k) / 0x1_0000;
            if 0xF333 < bend {
                ahead = scale(ahead, 51773);
            }
            if 0xE666 < bend {
                ahead = scale(ahead, 55050);
            } else if 0xCCCC < bend {
                ahead = scale(ahead, 58982);
            } else if 0xC000 < bend {
                ahead = scale(ahead, 62259);
            }
        }
        let (center, point, across) = (Coord(sp + 16), Coord(sp + 32), Coord(sp + 48));
        let mut tries = 0;
        loop {
            r.0[1] = ahead as u32;
            let m = &mut c.m;
            let n = slices_ahead(m, ahead);
            let look = slice_ahead(m, slice, n);
            car.set_look_ahead_slice(m, look);
            let s = slice_at(m, look);
            let v = Vec3::new(s.center(m, 0), s.center(m, 1), s.center(m, 2));
            center.set(m, v);
            let v = Vec3::new((s.right(m, 0) as i32) << 9, (s.right(m, 1) as i32) << 9, (s.right(m, 2) as i32) << 9);
            across.set(m, v);
            let ramp = car.ramp_desired_lat_pos(m);
            let (o, a) = (center.get(m), across.get(m));
            let v = Vec3::new(fixedmult(ramp, a.x).wrapping_add(o.x), fixedmult(ramp, a.y).wrapping_add(o.y), fixedmult(ramp, a.z).wrapping_add(o.z));
            point.set(m, v);
            let at = r.enter(c, 0x8006_b31c, 0x8006_b880, |c| calculate_road_position(c, point, slice));
            let m = &mut c.m;
            let s = slice_at(m, slice);
            let (left, right) = paved(m, s);
            let pos = car.road_position(m);
            // (off the paved road, further out than the car)
            let off = (right < at && pos < at) || (at < left && at < pos);
            let ok = if off {
                let side: i32 = if at < pos { -1 } else { 1 };
                r.0[0] = side as u32;
                let lane = r.enter(c, 0x8007_3dac, 0x8006_b92c, |c| crate::aiworld::lane_index(&mut c.m, slice, pos.wrapping_add(side << 19)));
                if crate::aiworld::is_driveable_lane(&mut c.m, slice, lane) != 0 {
                    let pos = car.road_position(&mut c.m);
                    let lane = r.enter(c, 0x8007_3dac, 0x8006_b950, |c| crate::aiworld::lane_index(&mut c.m, slice, pos.wrapping_add(side << 18)));
                    crate::aiworld::is_driveable_lane(&mut c.m, slice, lane) != 0
                } else {
                    false
                }
            } else {
                true
            };
            tries += 1;
            r.0[4] = tries;
            ahead = (ahead.wrapping_mul(52428) / 0x1_0000).max(4);
            if tries >= 5 || ok {
                break;
            }
        }
        let m = &mut c.m;
        let (p, at) = (point.get(m), car.n().position().get(m));
        car.desired_vector().set(m, Vec3::new(p.x.wrapping_sub(at.x), p.y.wrapping_sub(at.y), p.z.wrapping_sub(at.z)));
    })
}

pub fn abi_get_desired_vector(c: &mut Cpu) {
    get_desired_vector(c, Car(c.r[4]));
}

/// The brake pedal coming up (by the time step), the brake off once it is up.
fn release_brake(m: &mut Mem, car: Car) {
    let control = car.control();
    let (level, elapsed) = (control.brake_level(m) as u8 as i32, AIPHYSIC_ELAPSED_TIME.get(m));
    if elapsed < level {
        control.set_brake_level(m, level.wrapping_sub(elapsed as u8 as i32) as i8);
    } else {
        control.set_brake_level(m, 0);
    }
    if control.brake_level(m) == 0 {
        control.set_desired_brake_level(m, 0);
    }
}

/// The brake pedal going down (4 per tick unit, to 255), the brake on from 4.
fn press_brake(m: &mut Mem, car: Car) {
    let control = car.control();
    let level = (control.brake_level(m) as u8 as i32).wrapping_add(AIPHYSIC_ELAPSED_TIME.get(m).wrapping_mul(4));
    control.set_brake_level(m, if level < 255 { level as i8 } else { -1 });
    if control.brake_level(m) as u8 >= 4 {
        control.set_desired_brake_level(m, 1);
    }
}

/// The accelerations a cool-physics step ends with: `side` and `along` in the car's frame,
/// the yaw `spin`, none from the road.
fn set_accelerations(m: &mut Mem, car: Car, side: i32, along: i32, spin: i32) {
    car.linear_acc_ch().set(m, Vec3::new(side, 0, along));
    car.linear_acc_rh().set(m, Vec3::default());
    car.angular_acc().set(m, Vec3::new(0, spin, 0));
}

/// The tyres' skid marks and sounds.
fn set_skids(m: &mut Mem, car: Car, skid: i32) {
    car.set_front_skid(m, skid);
    car.set_rear_skid(m, skid);
}

/// `AIPhysic_OutOfControlPhysics` (AIPHYSIC.CPP, 0x8006bc00): a car out of control (no
/// wipeout after it) steered by the out-of-control model: the yaw rate and sideways speed it
/// wants from the angle to its point (the yaw rate five times in a donut), within the model's
/// limits and its speed's, give its yaw and sideways accelerations within the car's; along
/// the car it accelerates toward its wanted speed (reversing, out at the model's speed limit)
/// or brakes; going backward against its way, all of them less; skids by its sideways speed
/// coming (full reversing fast or in a donut).
pub fn out_of_control_physics(c: &mut Cpu, car: Car) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8011_0000;
        r.0[1] = car.0;
        r.0[4] = 0;
        let cfg = AIPHYSIC_CONFIG;
        let m = &mut c.m;
        car.set_drag(m, 0);
        let ahead = car.direction(m).wrapping_mul(cfg.latvelcalc_lookahead(m));
        let future = r.enter(c, 0x8007_409c, 0x8006_bc48, |c| crate::aiworld::calc_future_lateral_vel(c, car, ahead));
        r.0[5] = future as u32;
        let m = &mut c.m;
        let backward = car.direction(m).wrapping_mul(car.current_speed(m)) < 0 && car.drive_direction(m) != -1;
        r.0[4] = backward as u32;
        let now = SIM_COUNTER.get(m);
        car.set_wipe_out_end_tick(m, now);
        r.0[0] = cfg.0;
        let model = cfg.ooc_model();
        let k = fixedmult(128, model.dangle_to_dav(m));
        let mut dav = limit(car.a_car_wrt_desired(m).wrapping_neg().wrapping_mul(k), model.max_dav(m));
        let speed = car.speed(m);
        let mut dlvel = limit(limit((dav / 256).wrapping_mul(2560), model.max_dlvel(m)), speed);
        let (cs, range) = (car.current_speed(m), model.vel_limit_range(m));
        if range.wrapping_neg() < cs && cs < range {
            let s = speed / 256;
            dlvel = limit(dlvel, s.wrapping_mul(model.lat_vel_limit_factor(m) / 256).wrapping_abs());
            dav = limit(dav, s.wrapping_mul(model.ang_vel_limit_factor(m) / 256).wrapping_abs());
        }
        if car.donut_mode(m) == 2 {
            dav = dav.wrapping_add(dav << 2);
        }
        let mut spin = mul_8_8(car.angular_vel_ch().y(m).wrapping_sub(dav), model.dav_to_aa(m)).wrapping_neg();
        if 0x12_0000 < car.current_speed(m).wrapping_abs() {
            spin /= 2;
        }
        spin = limit(spin, car.max_aa(m));
        let side = mul_8_8(car.linear_vel_ch().x(m).wrapping_sub(dlvel), model.dlvel_to_clacc(m)).wrapping_neg();
        let mut side = limit(side, car.max_clacc(m));
        r.0[2] = spin as u32;
        r.0[3] = side as u32;
        let ahead = car.linear_vel_ch().z(m);
        let t = car.target_pos().get(m);
        let want = if car.drive_direction(m) == -1 && t.x == 0 && t.y == 0 && t.z == 0 {
            model.vel_limit_range(m).wrapping_neg()
        } else {
            car.desired_speed(m).wrapping_mul(car.direction(m)).wrapping_mul(car.drive_direction(m))
        };
        let short = if want >= 0 { ahead >= -0x2_0000 && ahead <= want } else { ahead >= want && ahead <= 0x2_0000 };
        let mut along = 0;
        if car.pull_over(m) == 0 && short && car.desired_direction(m) == car.direction(m) {
            let ahead = car.linear_vel_ch().z(m);
            let acc = r.enter(c, 0x8006_9eb0, 0x8006_c034, |c| calc_acceleration(c, car, ahead));
            let m = &mut c.m;
            along = acc.wrapping_mul(car.drive_direction(m));
            release_brake(m, car);
        } else if car.linear_vel_ch().z(m).wrapping_abs() > 0 {
            let decel = calc_deceleration(m, car);
            along = decel.wrapping_neg();
            press_brake(m, car);
            if car.linear_vel_ch().z(m) < 0 {
                along = decel;
            }
        }
        if backward {
            let fast = car.speed(&mut c.m).wrapping_add(-0x14_0001) as u32 > 0x1D_FFFE;
            if fast {
                spin = fixedmult(spin, 0xC000);
                along = along / 2 + along / 4;
            } else {
                spin /= 4;
                along /= 4;
            }
            side = side / 4 + side / 8;
        }
        let m = &mut c.m;
        let skid = future.wrapping_abs().wrapping_sub(cfg.skid_value(m));
        let reversing = car.direction(m).wrapping_mul(car.current_speed(m)) < 0;
        if (reversing && 0x3_8E38 < car.speed(m)) || car.donut_mode(m) != 0 {
            set_skids(m, car, 0xA_0000);
        } else {
            set_skids(m, car, skid.max(0));
        }
        set_accelerations(m, car, side, along, spin);
    })
}

pub fn abi_out_of_control_physics(c: &mut Cpu) {
    out_of_control_physics(c, Car(c.r[4]));
}

/// `AIPhysic_GetRearEndDamageFactor` (AIPHYSIC.CPP, 0x8006c270): how damaged the car's rear is
/// (the rear damage zones, at most 1).
pub fn get_rear_end_damage_factor(m: &mut Mem, car: Car) -> i32 {
    let d = |m: &mut Mem, i| car.n().damage(m, i);
    let sum = d(m, 4).wrapping_add(d(m, 5)).wrapping_add(d(m, 6)).wrapping_add(d(m, 9));
    fixedmult(sum, 327).min(0x1_0000)
}

pub fn abi_get_rear_end_damage_factor(c: &mut Cpu) {
    c.r[2] = get_rear_end_damage_factor(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIPhysic_InControlPhysics` (AIPHYSIC.CPP, 0x8006c2c8): a car in control steered by the
/// in-control model: its grip by its speed and grip factor; an AI racer due one wipes out on
/// a fast spin at speed; while it wipes out (or the AI script says to fishtail) it has no
/// sideways grip nor yaw control and its spin grows (to 0.2 a tick); hit by a human cop just
/// now, less grip and a brake by the skill; a fishtail swings it from side to side for its
/// time (one starts when it is far off its angle and turning further, a damaged rear sooner);
/// its yaw toward the point it drives at and its sideways speed against, within the car's
/// limits; along the car it accelerates toward its wanted speed or brakes; skids by its
/// sideways speed (or the fishtail's).
pub fn in_control_physics(c: &mut Cpu, car: Car) {
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        let m = &mut c.m;
        let dir = car.direction(m);
        m.w32(sp + 56, dir as u32);
        car.set_drag(m, 0);
        let to = Coord(sp + 16);
        let v = car.desired_vector().get(m);
        to.set(m, v);
        r.call(c, nfs4rc::f_8009e744, &[to.0], 0x8006_c328); // Math_QDNormalizeVector (for its frame)
        let m = &mut c.m;
        let t = to.get(m);
        m.w32(sp + 64, 0);
        let across = Coord(sp + 32);
        across.set(m, Vec3::new(t.z, t.y, t.x.wrapping_neg()));
        r.0[4] = 0;
        let vel = car.n().linear_vel().get(m);
        let a = across.get(m);
        let vy = fixedmult(vel.y, a.y);
        let lat = fixedmult(vel.x, a.x).wrapping_add(vy).wrapping_add(fixedmult(vel.z, a.z));
        r.0[1] = vy as u32;
        let lat = lat.wrapping_mul(car.direction(m));
        let mut max_spin = car.max_aa(m);
        r.0[7] = max_spin as u32;
        // (how far the car is from its lane ramp: worked out, never used)
        let off = car.road_position(m).wrapping_sub(car.ramp_desired_lat_pos(m));
        m.w32(sp + 52, off as u32);
        m.w32(sp + 48, lat as u32);
        let ahead = car.linear_vel_ch().z(m);
        let mut grip = fixedmult(fixedmult(ahead, 98).wrapping_add(0x1_0000), car.grip_factor(m));
        r.0[0] = grip as u32;
        if car.car_flags(m) & CAR_AI_RACER != 0 {
            let now = SIM_COUNTER.get(m);
            if car.wipe_out_start_tick(m) < now && car.n().angular_vel().y(m).wrapping_abs() >= 1001 && 0x16_38E3 < car.speed(m) {
                car.set_wipe_out_end_tick(m, now.wrapping_add(384));
            }
        }
        r.0[1] = SIM_GLOBAL.0;
        let since_hit = SIM_COUNTER.get(m).wrapping_sub(car.n().collision().last_time(m));
        r.0[3] = since_hit as u32;
        let fishtail = r.call(c, nfs4rc::f_80070170, &[car.script().0, 2048], 0x8006_c444) as i32 != -1; // AIScript_DoReAction
        let m = &mut c.m;
        let mut skid = 0;
        if fishtail || SIM_COUNTER.get(m) < car.wipe_out_end_tick(m) {
            let now = SIM_COUNTER.get(m);
            if now.wrapping_sub(car.n().collision().last_time(m)) < 64 {
                car.set_wipe_out_end_tick(m, now);
            }
            max_spin = 0;
            grip = 0;
            let spin = car.n().angular_vel();
            let more = spin.y(m).wrapping_abs().wrapping_add(458).min(13107);
            if spin.y(m) > 0 {
                spin.set_y(m, more);
            } else {
                spin.set_y(m, more.wrapping_neg());
            }
            skid = 0xA_0000;
        }
        r.0[0] = grip as u32;
        r.0[4] = skid as u32;
        r.0[7] = max_spin as u32;
        let skill = GAME_SETUP.skill(m);
        r.0[8] = skill as u32;
        if since_hit < 16 {
            car.update_wipe_out_start_tick(m, |t| t.wrapping_sub(20));
        }
        let window = m.r8(HUMAN_COP_HIT_TICKS.wrapping_add(skill as u32)) as i32;
        let other = car.n().collision().last_other_obj(m);
        let cop_hit = since_hit < window && other.0 != 0 && Car(other.0).car_flags(m) & CAR_HUMAN_COP != 0;
        if cop_hit {
            max_spin = 0;
            r.0[7] = 0;
            m.w32(sp + 64, 1);
            let mult = m.r32(HUMAN_COP_HIT_GRIP.wrapping_add((skill as u32).wrapping_mul(4))) as i32;
            grip = fixedmult(grip, mult);
        }
        let model = AIPHYSIC_CONFIG.ic_model();
        let g = grip / 256;
        let mut max_side = (car.max_clacc(m) / 256).wrapping_mul(g);
        let mut side_gain = (model.dlvel_to_clacc(m) / 256).wrapping_mul(g);
        let glue = car.ai_glue(m);
        if 0x1_0000 < glue {
            max_side = mul_8_8(glue, max_side);
            side_gain = mul_8_8(glue, side_gain);
        }
        r.0[3] = max_side as u32;
        r.0[5] = side_gain as u32;
        r.0[6] = SIM_GLOBAL.0;
        let mut angle = car.a_car_wrt_desired(m);
        r.0[1] = angle as u32;
        let wanted = car.desired_speed(m);
        m.w32(sp + 60, wanted as u32);
        let now = SIM_COUNTER.get(m);
        let fishtail_end = car.ai_fishtail_end_tick(m);
        if now < fishtail_end {
            // the fishtail's swing, side to side every 40 ticks
            let left = fishtail_end.wrapping_sub(now);
            let sign: i32 = if magic_div(now, 0x6666_6667, false, 4) & 1 != 0 { -1 } else { 1 };
            let into = 150i32.wrapping_sub(left);
            let swing = if into < 50 {
                magic_div(sign.wrapping_mul(30).wrapping_mul(into), 0x51EB_851F, false, 4)
            } else {
                sign.wrapping_mul(magic_div(left, 0x9249_2493, true, 2).wrapping_add(10))
            };
            angle = angle.wrapping_add(swing);
            if left >= 51 {
                skid = magic_div(left.wrapping_mul(5) << 18, 0x51EB_851F, false, 4);
            }
        } else {
            let p = car.personality(m);
            let off = angle.wrapping_abs();
            r.0[0] = off as u32;
            if p.fishtail_angle(m) < off && angle.wrapping_mul(car.angular_acc().y(m)) > 0 {
                r.0[0] = (off << 16) as u32;
                let damage = r.enter(c, 0x8006_c270, 0x8006_c740, |c| get_rear_end_damage_factor(&mut c.m, car));
                let m = &mut c.m;
                let start = 0x1_0000i32.wrapping_sub(damage).wrapping_mul(20).wrapping_add(p.fishtail_angle(m).wrapping_sub(20) << 16);
                if start < off << 16 {
                    let end = SIM_COUNTER.get(m).wrapping_add(150);
                    car.set_ai_fishtail_end_tick(m, end);
                }
            }
        }
        r.0[1] = angle as u32;
        r.0[4] = skid as u32;
        let m = &mut c.m;
        let k = fixedmult(128, model.dangle_to_dav(m));
        let mut dav = limit(angle.wrapping_neg().wrapping_mul(k), model.max_dav(m));
        let (cs, range) = (car.current_speed(m), model.vel_limit_range(m));
        if range.wrapping_neg() < cs && cs < range {
            let s = car.speed(m) / 256;
            dav = limit(dav, s.wrapping_mul(model.ang_vel_limit_factor(m) / 256).wrapping_abs());
        }
        let spin = fixedmult(car.angular_vel_ch().y(m).wrapping_sub(dav), model.dav_to_aa(m)).wrapping_neg();
        let spin = limit(spin, max_spin);
        r.0[1] = spin as u32;
        let side = dir.wrapping_neg().wrapping_mul(fixedmult(lat, side_gain));
        let side = limit(side, max_side);
        r.0[0] = side as u32;
        let slower = car.current_speed(m).wrapping_mul(dir) < wanted.wrapping_mul(dir);
        let mut along;
        if slower && car.pull_over(m) == 0 && car.desired_direction(m) == car.direction(m) {
            let cs = car.current_speed(m);
            along = r.enter(c, 0x8006_9eb0, 0x8006_ca1c, |c| calc_acceleration(c, car, cs));
            release_brake(&mut c.m, car);
        } else {
            let decel = calc_deceleration(m, car);
            along = decel.wrapping_neg();
            press_brake(m, car);
            if car.linear_vel_ch().z(m) < 0 {
                along = decel;
            }
        }
        let m = &mut c.m;
        if cop_hit {
            along = (m.r32(HUMAN_COP_HIT_BRAKE.wrapping_add((skill as u32).wrapping_mul(4))) as i32).wrapping_neg();
        }
        if skid == 0 {
            skid = lat.wrapping_abs().wrapping_sub(AIPHYSIC_CONFIG.skid_value(m));
        }
        set_skids(m, car, skid.max(0));
        set_accelerations(m, car, side, along, spin);
    })
}

pub fn abi_in_control_physics(c: &mut Cpu) {
    in_control_physics(c, Car(c.r[4]));
}

/// `AIPhysic_FinishUp` (AIPHYSIC.CPP, 0x8006cb7c): after the cool physics, gravity's pull in
/// the car's frame (with the road's acceleration along it for that), the accelerations back
/// to the world's (from the car's frame and the road's), the velocities stepped by them and
/// slowed by the drag, once per tick unit.
pub fn finish_up(c: &mut Cpu, car: Car) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        let m = &mut c.m;
        let (ch, rh) = (car.linear_acc_ch(), car.linear_acc_rh());
        let z = rh.z(m);
        ch.update_z(m, |v| v.wrapping_add(z));
        r.enter(c, 0x8008_67a8, 0x8006_cbac, |c| crate::cars::do_gravity_effects_on_acc(c, car));
        let m = &mut c.m;
        let z = rh.z(m);
        ch.update_z(m, |v| v.wrapping_sub(z));
        let (to_car, to_road) = (sp + 32, sp + 72);
        crate::fixed::transpose(m, car.n().orient_mat().0, to_car);
        crate::fixed::transpose(m, car.n().road_matrix().0, to_road);
        let dot = |m: &mut Mem, t: u32, row: u32, v: Vec3| {
            let e = |m: &mut Mem, k: u32| m.r32(t + 12 * row + 4 * k) as i32;
            let a = fixedmult(v.x, e(m, 0)).wrapping_add(fixedmult(v.y, e(m, 1)));
            a.wrapping_add(fixedmult(v.z, e(m, 2)))
        };
        let lin = car.linear_acc();
        let a = ch.get(m);
        let v = Vec3::new(dot(m, to_car, 0, a), dot(m, to_car, 1, a), dot(m, to_car, 2, a));
        lin.set(m, v);
        let b = rh.get(m);
        let road = [dot(m, to_road, 0, b), dot(m, to_road, 1, b), dot(m, to_road, 2, b)];
        lin.update_x(m, |v| v.wrapping_add(road[0]));
        lin.update_y(m, |v| v.wrapping_add(road[1]));
        lin.update_z(m, |v| v.wrapping_add(road[2]));
        let w = car.angular_acc().get(m);
        let spin = Coord(sp + 16);
        let v = Vec3::new(dot(m, to_car, 0, w), dot(m, to_car, 1, w), dot(m, to_car, 2, w));
        spin.set(m, v);
        car.angular_acc().set(m, v);
        let i_time = AIPHYSIC_I_TIME.get(m);
        let vel = car.n().linear_vel();
        let acc = lin.get(m);
        vel.update_x(m, |v| v.wrapping_add(fixedmult(acc.x, i_time)));
        vel.update_y(m, |v| v.wrapping_add(fixedmult(acc.y, i_time)));
        vel.update_z(m, |v| v.wrapping_add(fixedmult(acc.z, i_time)));
        let mut ticks = 0;
        while car.drag(m) != 0 && ticks < AIPHYSIC_ELAPSED_TIME.get(m) {
            ticks += 1;
            let drag = car.drag(m);
            vel.update_x(m, |v| fixedmult(v, drag));
            let drag = car.drag(m);
            vel.update_y(m, |v| fixedmult(v, drag));
            let drag = car.drag(m);
            vel.update_z(m, |v| fixedmult(v, drag));
        }
        let i = i_time / 256;
        let w = car.angular_acc().get(m);
        let s = car.n().angular_vel();
        s.update_x(m, |v| v.wrapping_add((w.x / 256).wrapping_mul(i)));
        s.update_y(m, |v| v.wrapping_add((w.y / 256).wrapping_mul(i)));
        s.update_z(m, |v| v.wrapping_add((w.z / 256).wrapping_mul(i)));
    })
}

pub fn abi_finish_up(c: &mut Cpu) {
    finish_up(c, Car(c.r[4]));
}

/// `AIPhysic_CalculateRampedDesiredLatPos` (AIPHYSIC.CPP, 0x8006cf68): the lane position the
/// car steers for, ramped toward the one it wants at 0.2 m per tick unit (straight there with
/// `ramp_type` 1, the cool physics), and caught up to the car when it is already past it
/// (by more than 1 m) on the way there.
pub fn calculate_ramped_desired_lat_pos(c: &mut Cpu, car: Car, ramp_type: i32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        if ramp_type == 1 {
            let want = car.desired_lat_pos(&mut c.m);
            car.set_ramp_desired_lat_pos(&mut c.m, want);
            return;
        }
        // (its result unused)
        r.enter(c, 0x8007_3f38, 0x8006_cf94, |c| crate::aiworld::calculate_delta_road_yaw(c, car));
        let m = &mut c.m;
        let step = AIPHYSIC_ELAPSED_TIME.get(m).wrapping_mul(13107);
        let (at, want) = (car.road_position(m), car.desired_lat_pos(m));
        if want < at && at < car.ramp_desired_lat_pos(m).wrapping_add(-0x1_0000) {
            car.set_ramp_desired_lat_pos(m, at);
        } else if car.ramp_desired_lat_pos(m).wrapping_add(0x1_0000) < at && at < want {
            car.set_ramp_desired_lat_pos(m, at);
        }
        let ramp = car.ramp_desired_lat_pos(m);
        let want = car.desired_lat_pos(m);
        if ramp < want {
            let to = ramp.wrapping_add(step);
            car.set_ramp_desired_lat_pos(m, if want < to { want } else { to });
        } else if want < ramp {
            let to = ramp.wrapping_sub(step);
            car.set_ramp_desired_lat_pos(m, if to < want { want } else { to });
        }
    })
}

pub fn abi_calculate_ramped_desired_lat_pos(c: &mut Cpu) {
    calculate_ramped_desired_lat_pos(c, Car(c.r[4]), c.r[5] as i32);
}

/// `AIPhysic_HitWallCheck` (AIPHYSIC.CPP, 0x8006d070): whether the car is stuck off its lanes
/// against something: outside the slice's lanes and either reversing for more than 8 tick
/// units or on a road tilted over 15 degrees sideways.
pub fn hit_wall_check(m: &mut Mem, car: Car) -> bool {
    let lanes = car_slice(m, car).lane_count(m) as i32;
    let lane = car.lane_index(m);
    if lane >= 7 - (lanes >> 4) && lane <= (lanes & 0xF) + 6 {
        return false;
    }
    if car.drive_direction(m) == -1 {
        let elapsed = AIPHYSIC_ELAPSED_TIME.get(m);
        car.update_time_off_road(m, |t| t.wrapping_add(elapsed));
    } else {
        car.set_time_off_road(m, 0);
    }
    car.time_off_road(m) >= 9 || car.n().road_matrix().m(m, 4) <= 0xD999
}

pub fn abi_hit_wall_check(c: &mut Cpu) {
    c.r[2] = hit_wall_check(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIPhysic_ProcessBarrierCollision` (AIPHYSIC.CPP, 0x8006d120): an AI car hitting a barrier
/// under 10 m/s backs off it.
pub fn process_barrier_collision(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    if car.car_flags(m) & CAR_HUMAN == 0 && car.current_speed(m).wrapping_abs() <= 0x9_FFFF {
        change_direction(m, car, 96);
    }
}

pub fn abi_process_barrier_collision(c: &mut Cpu) {
    process_barrier_collision(c, Car(c.r[4]));
}

/// `AIPhysic_ProcessCollision` (AIPHYSIC.CPP, 0x8006d174): a hard hit from another object
/// under 10 m/s makes the car back off (traffic for longer).
pub fn process_collision(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    let hit = car.n().collision();
    if 0xD999 < hit.impulse(m) && hit.other_obj(m).0 != 0 && car.current_speed(m).wrapping_abs() <= 0x9_FFFF {
        let time = if car.car_flags(m) & CAR_TRAFFIC != 0 { 160 } else { 96 };
        change_direction(m, car, time);
    }
}

pub fn abi_process_collision(c: &mut Cpu) {
    process_collision(c, Car(c.r[4]));
}

/// `AIPhysic_ChangeDirection` (AIPHYSIC.CPP, 0x8006d1f0): a car reversing for more than half
/// its reversing time drives forward again, one driving forward as long (under 20 m/s)
/// reverses; for `time` tick units.
pub fn change_direction(m: &mut Mem, car: Car, time: i32) {
    let dir = car.drive_direction(m);
    let since = SIM_COUNTER.get(m).wrapping_sub(car.drive_direction_timer(m));
    let half = car.drive_direction_reverse_time(m) / 2;
    let to = if dir == -1 {
        if half >= since {
            return;
        }
        1
    } else {
        if dir != 1 || half >= since || 0x13_FFFF < car.current_speed(m).wrapping_abs() {
            return;
        }
        -1
    };
    car.set_drive_direction(m, to);
    car.set_drive_direction_reverse_time(m, time);
    let now = SIM_COUNTER.get(m);
    car.set_drive_direction_timer(m, now);
}

pub fn abi_change_direction(c: &mut Cpu) {
    change_direction(&mut c.m, Car(c.r[4]), c.r[5] as i32);
}

/// `AIPhysic_CheckForGripReduction` (AIPHYSIC.CPP, 0x8006d2a8): a car that lost grip gets it
/// back at the personality's rate (all at once after a hit with no other object, or once a
/// quarter of the way back); one with full grip, fully simulated, an AI racer or a police car,
/// may lose it on a sharp bend (by the personality's chance per second).
pub fn check_for_grip_reduction(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let grip = car.grip_factor(m);
        let p = car.personality(m);
        if grip <= 0xFFFF {
            let elapsed = AIPHYSIC_ELAPSED_TIME.get(m);
            let grip = grip.wrapping_add(elapsed.wrapping_mul(p.grip_loss_recovery_per_tick(m)));
            car.set_grip_factor(m, grip);
            let hit = car.n().collision();
            if SIM_COUNTER.get(m).wrapping_sub(hit.last_time(m)) < 64 && hit.last_other_obj(m).0 == 0 {
                car.set_grip_factor(m, 0x1_0000);
                return;
            }
            let min = p.grip_loss_min_factor(m);
            if min.wrapping_add(0x1_0000i32.wrapping_sub(min) / 4) < grip {
                car.set_grip_factor(m, 0x1_0000);
            }
            return;
        }
        if car.n().sim_optz(m) != 0 || car.car_flags(m) & (CAR_AI_RACER | CAR_COP) == 0 {
            return;
        }
        let bend = crate::aiworld::calc_road_bend(m, car, 1);
        // (the original asks for the bend twice)
        let sharp = if bend > 0 { bend >= 2001 } else { bend.wrapping_neg() >= 2001 };
        if !sharp {
            return;
        }
        let rand = crate::physics::fast_random(c) as i32;
        let m = &mut c.m;
        let chance = AIPHYSIC_ELAPSED_TIME.get(m).wrapping_mul(p.grip_loss_prob_per_second(m)) / 32;
        if rand < chance {
            let min = p.grip_loss_min_factor(m);
            car.set_grip_factor(m, min);
        }
    })
}

pub fn abi_check_for_grip_reduction(c: &mut Cpu) {
    check_for_grip_reduction(c, Car(c.r[4]));
}

/// `AIPhysic_StartUp`, `AIPhysic_CleanUp`, `AIPhysic_Reset` (AIPHYSIC.CPP, 0x8006d43c,
/// 0x8006d444, 0x8006d44c): nothing.
pub fn start_up() {}

pub fn abi_start_up(_: &mut Cpu) {}

pub fn abi_clean_up(_: &mut Cpu) {}

pub fn abi_reset(_: &mut Cpu) {}

/// `AIPhysic_ResetCar` (AIPHYSIC.CPP, 0x8006d454): no pulling over, no blown tyre.
pub fn reset_car(m: &mut Mem, car: Car) {
    car.set_pull_over(m, 0);
    car.set_blowout(m, 0);
}

pub fn abi_reset_car(c: &mut Cpu) {
    reset_car(&mut c.m, Car(c.r[4]));
}

/// `AIPhysic_InitCar` (AIPHYSIC.CPP, 0x8006d460): an AI car's braking (12 m/s², 11 for racers
/// and police, by its personality and an AI racer's brake upgrade) and its table of the
/// distances to stop from each speed (in 1/2 m, by m/s).
pub fn init_car(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[4] = car.0;
        let m = &mut c.m;
        let flags = car.car_flags(m);
        if flags & CAR_AI == 0 {
            return;
        }
        let base = if flags & (CAR_AI_RACER | CAR_COP) == 0 { 0xC_0000 } else { 0xB_0000 };
        let p = car.personality(m);
        let mut decel = mul_8_8(p.brake_multiplier(m), base);
        r.0[0] = decel as u32;
        if flags & CAR_AI_RACER != 0 {
            let index = car.car_index(m);
            let mult = r.enter(c, 0x8006_f9c8, 0x8006_d4ec, |c| crate::aispeeds::get_upgrade_brake_mult(&mut c.m, index));
            decel = mul_8_8(decel, mult);
            r.0[0] = decel as u32;
        }
        let info = AIPhysicBrakeInfo(r.call(c, nfs4rc::f_800a48c0, &[AIPhysicBrakeInfo::SIZE], 0x8006_d518)); // operator new
        let m = &mut c.m;
        info.set_deceleration_(m, decel);
        let inv = fixeddiv(0x1_0000, decel);
        for i in 0..128 {
            let v = i << 16;
            let d = fixedmult(fixedmult(v, inv), v) / 2;
            info.set_brake_table_(m, i as u32, (d / 0x2_0000) as u8);
        }
        car.set_brake_info(m, info);
    })
}

pub fn abi_init_car(c: &mut Cpu) {
    init_car(c, Car(c.r[4]));
}

/// `AIPhysic_DeInitCar` (AIPHYSIC.CPP, 0x8006d5d4): an AI car's braking table freed.
pub fn de_init_car(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        let info = car.brake_info(m);
        if car.car_flags(m) & CAR_AI != 0 && info.0 != 0 {
            car.set_brake_info(m, AIPhysicBrakeInfo(0));
            r.call(c, nfs4rc::f_800a4910, &[info.0], 0x8006_d610); // operator delete
        }
    })
}

pub fn abi_de_init_car(c: &mut Cpu) {
    de_init_car(c, Car(c.r[4]));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, NUM_CARS};
    use crate::test_util::{Rng, SCRATCH, against_original};

    /// One of the race's cars (its address at `SCRATCH`) in any state the AI physics reads:
    /// any speeds and directions, reversing or not, mid-shift or not, any gear and revs, in or
    /// off its lanes, wiping out, losing grip, flying, upside down, on a blown tyre, pulling
    /// over, finished, parked, any flags, simplified or not; any tick and time step, before the
    /// start or not, in the rain, a pursuit race against a human cop.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m);
        let car = Car(m.r32(CARS_LIST + 4 * pick(n as u32) as u32));
        m.w32(SCRATCH, car.0);
        let any = |p: &mut dyn FnMut(u32) -> i32, scale: i32| [p(2 * scale as u32) - scale, p(0x2_0000) - 0x1_0000, 0, p(0x40_0000) - 0x20_0000][p(4) as usize];
        let now = 1000 + pick(100_000);
        SIM_COUNTER.set(m, now);
        SIM_GLOBAL.set_game_started(m, [1, 1, 1, 0][pick(4) as usize]);
        let elapsed = [1, 2, 4, 8][pick(4) as usize];
        AIPHYSIC_ELAPSED_TIME.set(m, elapsed);
        AIPHYSIC_I_TIME.set(m, elapsed * 0x400);
        GAME_SETUP.set_weather(m, pick(2));
        GAME_SETUP.set_race_type(m, [0, 1, 5, 2][pick(4) as usize]);
        GAME_SETUP.set_skill(m, pick(3));
        if pick(4) == 0 {
            let human = Car(m.r32(HUMAN_RACE_CARS));
            human.update_car_flags(m, |f| f ^ CAR_HUMAN_COP);
        }
        let flags = [CAR_AI | CAR_AI_RACER | CAR_RACING, CAR_AI | CAR_TRAFFIC, CAR_AI | CAR_COP, CAR_HUMAN | CAR_RACING, pick(0x1000)][pick(5) as usize];
        car.set_car_flags(m, flags | [0, 0, 0, CAR_OUT, CAR_COOL_PHYSICS][pick(5) as usize]);
        car.set_ai_flags(m, [0, AI_COOL_LAST, AI_PARKED, pick(0x40)][pick(4) as usize]);
        let speed = [pick(0x40_0000) - 0x20_0000, pick(0x4_0000) - 0x2_0000, 0, pick(0x200_0000) - 0x100_0000][pick(4) as usize];
        car.set_current_speed(m, speed);
        car.set_speed(m, [speed.wrapping_abs(), pick(0x40_0000), pick(0x4_0000)][pick(3) as usize]);
        car.set_desired_speed(m, [speed + pick(0x8_0000) - 0x4_0000, 0, pick(0x40_0000) - 0x20_0000][pick(3) as usize]);
        car.set_direction(m, [1, -1][pick(2) as usize]);
        car.set_desired_direction(m, [1, -1][pick(2) as usize]);
        car.set_drive_direction(m, [1, -1, 1, 0][pick(4) as usize]);
        car.set_drive_direction_timer(m, now - pick(300));
        car.set_drive_direction_reverse_time(m, pick(200));
        car.set_lane_index(m, pick(16));
        let at = any(&mut pick, 0x30_0000);
        car.set_road_position(m, at);
        car.set_desired_lat_pos(m, at + any(&mut pick, 0x10_0000));
        car.set_ramp_desired_lat_pos(m, at + any(&mut pick, 0x10_0000));
        car.set_target_lat_pos(m, any(&mut pick, 0x10_0000));
        car.set_lateral_velocity(m, any(&mut pick, 0x8_0000));
        car.set_lane_change_speed(m, any(&mut pick, 0x8_0000));
        car.set_time_off_road(m, pick(16));
        car.set_ai_shift_timer(m, [0, 0, pick(64) - 16][pick(3) as usize]);
        car.set_ai_shift_duration(m, pick(64));
        car.control().set_gear(m, pick(8) as i8);
        car.control().set_brake_level(m, pick(256) as i8);
        car.set_flywheel_rpm(m, pick(10_000));
        car.set_red_line(m, (5000 + pick(4000)) << 16);
        car.set_grip_factor(m, [0x1_0000, pick(0x1_0000), 0x2_0000][pick(3) as usize]);
        car.set_wipe_out_start_tick(m, now + pick(200) - 100);
        car.set_wipe_out_end_tick(m, now + pick(800) - 400);
        car.set_acc_nitrous(m, [0x1_0000, 0x1_8000][pick(2) as usize]);
        car.set_ai_glue(m, [0x1_0000, 0x1_4000, 0xC000][pick(3) as usize]);
        car.set_pull_over(m, [0, 0, 1][pick(3) as usize]);
        car.set_blowout(m, [0, 0, 0, 1][pick(4) as usize]);
        car.stats().set_finish_type(m, [0, 0, FINISHED][pick(3) as usize]);
        let nobj = car.n();
        nobj.set_flight_time(m, [0, 0, 0, pick(20) as u16][pick(4) as usize]);
        nobj.set_active(m, [1, 1, 1, 0][pick(4) as usize]);
        nobj.set_sim_optz(m, [0, 0, 1, 2][pick(4) as usize]);
        let v = nobj.linear_vel();
        v.set(m, Vec3::new(any(&mut pick, 0x80_0000), any(&mut pick, 0x8_0000), any(&mut pick, 0x80_0000)));
        nobj.set_speed_xz(m, pick(0x100_0000));
        nobj.angular_vel().set(m, Vec3::new(any(&mut pick, 0x4_0000), any(&mut pick, 0x4_0000), any(&mut pick, 0x4_0000)));
        if pick(4) == 0 {
            // upside down, or turned round
            for k in [[3, 4, 5], [0, 2, 6]][pick(2) as usize] {
                nobj.orient_mat().update_m(m, k, |x| x.wrapping_neg());
            }
        }
        if pick(4) == 0 {
            nobj.road_matrix().set_m(m, 4, pick(0x1_0000));
        }
        let hit = nobj.collision();
        hit.set_impulse(m, [0, pick(0x2_0000)][pick(2) as usize]);
        hit.set_last_time(m, now - pick(128));
        hit.set_last_other_obj(m, crate::types::NewtonObj([0, car.0][pick(2) as usize]));
        let slices = NUM_SLICES.get(m);
        nobj.sim_road_info().set_slice(m, pick(slices as u32) as i16);
        // the cool physics': a target or not, donuts, fishtails, drag, damage, the angles
        let p = nobj.position().get(m);
        let target = [Vec3::default(), Vec3::default(), Vec3::new(p.x + pick(0x40_0000) - 0x20_0000, p.y, p.z + pick(0x40_0000) - 0x20_0000)][pick(3) as usize];
        car.target_pos().set(m, target);
        car.set_donut_mode(m, [0, 0, 1, 2][pick(4) as usize]);
        car.set_ai_fishtail_end_tick(m, now + [-1, -200, pick(160), pick(40)][pick(4) as usize]);
        car.set_max_aa(m, pick(0x4_0000));
        car.set_max_clacc(m, pick(0x20_0000));
        car.set_drag(m, [0, 0, 0xF000 + pick(0x1000)][pick(3) as usize]);
        car.set_a_car_wrt_desired(m, [pick(1024) - 512, pick(240) - 120, pick(40) - 20][pick(3) as usize]);
        for i in 0..10 {
            nobj.set_damage(m, i, [0, pick(0x1_0000), pick(0x40_0000)][pick(3) as usize]);
        }
        let any3 = |p: &mut dyn FnMut(u32) -> i32, scale: i32| Vec3::new(any(p, scale), any(p, scale), any(p, scale));
        car.linear_acc_rh().set(m, any3(&mut pick, 0x10_0000));
        car.angular_acc().set(m, any3(&mut pick, 0x4_0000));
        car.linear_vel_ch().set(m, any3(&mut pick, 0x40_0000));
        car.angular_vel_ch().set(m, any3(&mut pick, 0x4_0000));
        let v = any3(&mut pick, 0x40_0000);
        car.desired_vector().set(m, v);
        let person = car.personality(m);
        person.set_fishtail_angle(m, pick(120));
    }

    fn the_car(c: &mut Cpu) -> Vec<u32> {
        vec![c.m.r32(SCRATCH)]
    }

    #[test]
    fn gears_fuzz() {
        against_original("AIPhysic_HandleShifting", 20_000, 0xA1, shuffle, nfs4rc::f_80069bcc, &the_car, |c, a| {
            handle_shifting(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_RevEngine", 5_000, 0xA2, shuffle, nfs4rc::f_80069afc, &the_car, |c, a| {
            rev_engine(&mut c.m, Car(a[0]));
            None
        });
        for g in 0..9u32 {
            against_original("AIPhysic_GearTopSpeed", 200, 0xA3 + g as u64, shuffle, nfs4rc::f_80069e60, &move |c| vec![c.m.r32(SCRATCH), g], |c, a| {
                Some(gear_top_speed(&mut c.m, Car(a[0]), a[1] as i32) as u32)
            });
            against_original("AIPhysic_GearBottomSpeed", 200, 0xB3 + g as u64, shuffle, nfs4rc::f_80069e88, &move |c| vec![c.m.r32(SCRATCH), g], |c, a| {
                Some(gear_bottom_speed(&mut c.m, Car(a[0]), a[1] as i32) as u32)
            });
            against_original("AIPhysic_GearInvSpeedRange", 200, 0xC3 + g as u64, shuffle, nfs4rc::f_80069e38, &move |c| vec![c.m.r32(SCRATCH), g], |c, a| {
                Some(gear_inv_speed_range(&mut c.m, Car(a[0]), a[1] as i32) as u32)
            });
        }
    }

    #[test]
    fn acceleration_fuzz() {
        let speed_too = |c: &mut Cpu| {
            let car = c.m.r32(SCRATCH);
            vec![car, Car(car).current_speed(&mut c.m) as u32]
        };
        against_original("AIPhysic_CalcAcceleration", 20_000, 0xA4, shuffle, nfs4rc::f_80069eb0, &speed_too, |c, a| {
            Some(calc_acceleration(c, Car(a[0]), a[1] as i32) as u32)
        });
        against_original("AIPhysic_CalcDeceleration", 2_000, 0xA5, shuffle, nfs4rc::f_8006a1e8, &the_car, |c, a| {
            Some(calc_deceleration(&mut c.m, Car(a[0])) as u32)
        });
    }

    #[test]
    fn simple_physics_fuzz() {
        against_original("AIPhysic_SimplePhysics", 20_000, 0xA6, shuffle, nfs4rc::f_8006a730, &the_car, |c, a| {
            simple_physics(c, Car(a[0]));
            None
        });
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn small_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 13] = [
            ("AIPhysic_CheckDesiredDirection", nfs4rc::f_8006a234, |c, a| {
                check_desired_direction(&mut c.m, Car(a[0]));
                None
            }),
            ("AIPhysic_HandleSignalling", nfs4rc::f_8006a27c, |c, a| {
                handle_signalling(&mut c.m, Car(a[0]));
                None
            }),
            ("AIPhysic_HandleWipeoutTimer", nfs4rc::f_8006a2f0, |c, a| {
                handle_wipeout_timer(c, Car(a[0]));
                None
            }),
            ("AIPhysic_CheckForBadPosition", nfs4rc::f_8006a614, |c, a| {
                check_for_bad_position(c, Car(a[0]));
                None
            }),
            ("AIPhysics_UseCoolPhysics", nfs4rc::f_8006a6ec, |c, a| Some(use_cool_physics(&mut c.m, Car(a[0])) as u32)),
            ("AIPhysic_HandleDirection", nfs4rc::f_8006afcc, |c, a| {
                handle_direction(&mut c.m, Car(a[0]));
                None
            }),
            ("AIPhysic_TargetedGetDesiredVector", nfs4rc::f_8006ba24, |c, a| {
                targeted_get_desired_vector(&mut c.m, Car(a[0]));
                None
            }),
            ("AIPhysic_CheckIfOutOfControl", nfs4rc::f_8006ba60, |c, a| Some(check_if_out_of_control(c, Car(a[0])) as u32)),
            ("AIPhysic_HitWallCheck", nfs4rc::f_8006d070, |c, a| Some(hit_wall_check(&mut c.m, Car(a[0])) as u32)),
            ("AIPhysic_ProcessBarrierCollision", nfs4rc::f_8006d120, |c, a| {
                process_barrier_collision(c, Car(a[0]));
                None
            }),
            ("AIPhysic_ProcessCollision", nfs4rc::f_8006d174, |c, a| {
                process_collision(c, Car(a[0]));
                None
            }),
            ("AIPhysic_CheckForGripReduction", nfs4rc::f_8006d2a8, |c, a| {
                check_for_grip_reduction(c, Car(a[0]));
                None
            }),
            ("AIPhysic_ResetCar", nfs4rc::f_8006d454, |c, a| {
                reset_car(&mut c.m, Car(a[0]));
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 5_000, 0xB00 + k as u64, shuffle, original, &the_car, port);
        }
        for t in [64u32, 96, 160] {
            against_original("AIPhysic_ChangeDirection", 3_000, 0xC00 + t as u64, shuffle, nfs4rc::f_8006d1f0, &move |c| vec![c.m.r32(SCRATCH), t], |c, a| {
                change_direction(&mut c.m, Car(a[0]), a[1] as i32);
                None
            });
        }
        for (k, (v, s)) in [(0xFAE1u32, 0xFAE1u32), (0x1_0040, 0x10_0000), (0xFFFF_FFFF, 0xFFD0_0000)].into_iter().enumerate() {
            against_original("AIPhysic_StopCar", 2_000, 0xD00 + k as u64, shuffle, nfs4rc::f_80069a38, &move |c| vec![c.m.r32(SCRATCH), v, s], |c, a| {
                stop_car(&mut c.m, Car(a[0]), a[1] as i32, a[2] as i32);
                None
            });
        }
        for ramp in [0u32, 1] {
            against_original("AIPhysic_CalculateRampedDesiredLatPos", 5_000, 0xE00 + ramp as u64, shuffle, nfs4rc::f_8006cf68, &move |c| vec![c.m.r32(SCRATCH), ramp], |c, a| {
                calculate_ramped_desired_lat_pos(c, Car(a[0]), a[1] as i32);
                None
            });
        }
        let pos_and_slice = |c: &mut Cpu| {
            let car = Car(c.m.r32(SCRATCH));
            vec![car.n().position().0, car.n().sim_road_info().slice(&mut c.m) as u32]
        };
        against_original("AIPhysic_CalculateRoadPosition", 5_000, 0xE10, shuffle, nfs4rc::f_8006b31c, &pos_and_slice, |c, a| {
            Some(calculate_road_position(c, Coord(a[0]), a[1] as i32) as u32)
        });
    }

    #[test]
    fn desired_vector_fuzz() {
        against_original("AIPhysic_GetDesiredVector", 10_000, 0xAC, shuffle, nfs4rc::f_8006b43c, &the_car, |c, a| {
            get_desired_vector(c, Car(a[0]));
            None
        });
    }

    #[test]
    fn cool_physics_fuzz() {
        against_original("AIPhysic_Preperation", 10_000, 0xA7, shuffle, nfs4rc::f_8006b0b4, &the_car, |c, a| {
            preperation(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_OutOfControlPhysics", 10_000, 0xAD, shuffle, nfs4rc::f_8006bc00, &the_car, |c, a| {
            out_of_control_physics(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_InControlPhysics", 20_000, 0xAE, shuffle, nfs4rc::f_8006c2c8, &the_car, |c, a| {
            in_control_physics(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_FinishUp", 10_000, 0xAF, shuffle, nfs4rc::f_8006cb7c, &the_car, |c, a| {
            finish_up(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_GetRearEndDamageFactor", 2_000, 0xB0, shuffle, nfs4rc::f_8006c270, &the_car, |c, a| {
            Some(get_rear_end_damage_factor(&mut c.m, Car(a[0])) as u32)
        });
        // (last: it runs all of the above)
        against_original("AIPhysic_CoolPhysics", 10_000, 0xA8, shuffle, nfs4rc::f_8006af18, &the_car, |c, a| {
            cool_physics(c, Car(a[0]));
            None
        });
    }

    #[test]
    fn main_fuzz() {
        against_original("AIPhysic_Main", 20_000, 0xA9, shuffle, nfs4rc::f_8006a384, &the_car, |c, a| {
            main(c, Car(a[0]));
            None
        });
    }

    #[test]
    fn init_car_fuzz() {
        against_original("AIPhysic_InitCar", 2_000, 0xAA, shuffle, nfs4rc::f_8006d460, &the_car, |c, a| {
            init_car(c, Car(a[0]));
            None
        });
        against_original("AIPhysic_DeInitCar", 2_000, 0xAB, shuffle, nfs4rc::f_8006d5d4, &the_car, |c, a| {
            de_init_car(c, Car(a[0]));
            None
        });
    }

    /// The stack the physics leave (the AI after them, still recompiled, reads it): from the
    /// schedule's call, on the race's scratchpad stack.
    #[test]
    fn main_stack_fuzz() {
        let top = crate::test_util::SCRATCH_TOP;
        crate::test_util::same_stack("AIPhysic_Main", 0x8006_a384, nfs4rc::f_8006a384, 0x800b_721c, top, 5_000, 0x57A1, shuffle, &|c, _| the_car(c));
        crate::test_util::same_stack("AIPhysic_InitCar", 0x8006_d460, nfs4rc::f_8006d460, 0x8008_ad00, 0x801F_FE00, 500, 0x57A2, shuffle, &|c, _| the_car(c));
    }
}
