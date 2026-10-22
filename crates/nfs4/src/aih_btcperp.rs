//! AIH_BTCPERP.CPP: the speeders of a "be the cop" race: `AIHigh_BTC_Perp` (pulled over,
//! beaten, arrested or let go on a false arrest; the human cops told), `AIHigh_BTC_HumanPerp`
//! (a human speeder, each stage's start near the human cop) and `AIHigh_BTC_AIPerp` (an AI
//! speeder: its stage's car, speed and personality, cruising until it meets the police,
//! fleeing then, swerving or turning round to dodge them).

use crate::abi::{self, Saved};
use crate::aih_basiccop::{ROAD_BLOCK_STATE, fast_random_step, per_mille};
use crate::aih_basicperp::{self, orient, speech, speech_call};
use crate::aih_btccop::{self, human_cop_hud_on, human_cop_release_and_start_chase};
use crate::aihigh::{self, CAR_HUMAN, CAR_HUMAN_COP, VT_BASIC_PERP, VT_BTC_AI_PERP, VT_BTC_PERP2, obj};
use crate::aiworld::{apx_spline_distance, find_barrier_less_lane_and_position};
use crate::fixed::{fixeddiv, fixedmult};
use crate::globals::{AI_ELAPSED_TIME, CARS_LIST, GAME_SETUP, HUMAN_RACE_CARS, NUM_CARS, NUM_HUMAN_RACE_CARS, NUM_SLICES, SIM_COUNTER};
use crate::types::{AIHighBTCAIPerp, AIHighBTCCop, AIHighBTCHumanCop, AIHighBTCHumanPerp, AIHighBTCPerp, AIHighBasicPerp, AIStateBase, Car, Coord};
use ps1rt::{Cpu, Mem};

/// `Camera_gInfo`: the cameras' state (the cop camera's target, two views).
const CAMERA: u32 = 0x8011_0570;
/// `strategyChart`: an AI speeder's moves (1 swerve, 2 turn round) by where the closest
/// police car is (across: left, beside, right; along: far ahead, ahead, beside, behind, far
/// behind).
const STRATEGY_CHART: u32 = 0x8010_DAD8;
/// `AIHigh_BTC_uTurnProb1000Skills`: the chance of a U-turn per tick, by the skill.
const U_TURN_PROB: u32 = 0x8010_DAE8;
/// `AITune_BTC` (by the skill): the speeders' glue, speed and weight factors.
const AITUNE_BTC: u32 = 0x8010_F0D8;

/// `perpMode_`: cruising (0), meeting the police (1), met (2), fleeing (4), escaped (5).
const MODE_MET: i32 = 2;
const MODE_FLEEING: i32 = 4;
const MODE_ESCAPED: i32 = 5;

/// The states of this file (its copies of the inline classes' vtables): not active, the base.
const VT_STATE_NON_ACTIVE: u32 = 0x8005_57D0;
const VT_STATE_BASE: u32 = 0x8005_57F0;

fn car_of(m: &mut Mem, this: AIHighBTCPerp) -> Car {
    this.ai_high_basic_perp().ai_high_base().car_obj_(m)
}

fn base(this: AIHighBTCPerp) -> AIHighBasicPerp {
    this.ai_high_basic_perp()
}

/// For each active human cop (with `mask` 0x200), or each police car (0x220), its object and
/// index: `f` called from the original's call returning to `ra`, the loop's registers.
fn each_cop(c: &mut Cpu, mask: i32, mut f: impl FnMut(&mut Cpu, &mut Saved, u32, u32)) {
    let mut r = Saved::of(c);
    for i in 0..NUM_CARS.get(&mut c.m) {
        let m = &mut c.m;
        let car = Car(m.r32(CARS_LIST + 4 * i as u32));
        (r.0[0], r.0[1]) = (CARS_LIST + 4 * i as u32, i as u32);
        if car.car_flags(m) & mask == 0 || car.n().active(m) == 0 {
            continue;
        }
        let index = car.car_index(m);
        let o = obj(m, index);
        f(c, &mut r, o, i as u32);
    }
}

/// `AIHigh_BTC_Perp::ReleaseCops` (AIH_BTCPERP.CPP, 0x8005fea4): each active human cop lets go
/// and starts its chase; the car simulated by distance again.
pub fn release_cops(c: &mut Cpu, this: AIHighBTCPerp) {
    abi::frame(c, 40, |c, _| {
        each_cop(c, CAR_HUMAN_COP, |c, r, o, _| {
            (r.0[2], r.0[3]) = (this.0, aihigh::HIGH_LEVEL_AI_OBJS);
            r.enter(c, 0x8005_db10, 0x8005_ff34, |c| human_cop_release_and_start_chase(c, AIHighBTCHumanCop(o), this));
        });
        car_of(&mut c.m, this).set_force_no_sim_optz(&mut c.m, 0);
    })
}

pub fn abi_release_cops(c: &mut Cpu) {
    release_cops(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::HandleCops` (AIH_BTCPERP.CPP, 0x8005ff68): the pull-over.
pub fn handle_cops(c: &mut Cpu, this: AIHighBTCPerp) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8006_0248, 0x8005_ff78, |c| handle_pull_over(c, this));
    })
}

pub fn abi_handle_cops(c: &mut Cpu) {
    handle_cops(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::IsFalseArrest` (AIH_BTCPERP.CPP, 0x8005ff88): an AI speeder, 2% of the
/// time, finds a human cop not just ahead (within 3 m across, 8 m along): it honks and it is
/// a false arrest.
pub fn is_false_arrest(c: &mut Cpu, this: AIHighBTCPerp) -> bool {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this.0;
        let m = &mut c.m;
        let chance = per_mille(m);
        let car = car_of(m, this);
        if car.car_flags(m) & CAR_HUMAN != 0 || chance < 980 {
            return false;
        }
        r.0[5] = 0;
        r.0[4] = CARS_LIST;
        for i in 0..NUM_CARS.get(m) {
            let m = &mut c.m;
            (r.0[4], r.0[5]) = (CARS_LIST + 4 * i as u32, i as u32);
            let cop = Car(m.r32(CARS_LIST + 4 * i as u32));
            if cop.car_flags(m) & CAR_HUMAN_COP == 0 {
                continue;
            }
            let (p, q) = (car.n().position().get(m), cop.n().position().get(m));
            let d = [q.x.wrapping_sub(p.x), q.y.wrapping_sub(p.y), q.z.wrapping_sub(p.z)];
            for (k, v) in d.iter().enumerate() {
                m.w32(sp + 16 + 4 * k as u32, *v as u32);
            }
            let o: [i32; 9] = std::array::from_fn(|k| orient(m, car, k as u32));
            let side = fixedmult(d[0], o[0]).wrapping_add(fixedmult(d[1], o[1])).wrapping_add(fixedmult(d[2], o[2])).wrapping_abs();
            let along7 = fixedmult(d[1], o[7]);
            let ahead = fixedmult(d[0], o[6]).wrapping_add(along7).wrapping_add(fixedmult(d[2], o[8]));
            let near = side <= 0x3_0000 && ahead <= 0x8_0000;
            if near && ahead >= 0 {
                continue;
            }
            if near {
                r.0[4] = r.0[4].wrapping_add(4);
            }
            (r.0[0], r.0[1], r.0[2]) = (along7 as u32, ahead as u32, side as u32);
            r.call(c, nfs4rc::f_80075088, &[car.0, 2, 128, 32], 0x8006_016c); // AudioClc_HonkHorn
            return true;
        }
        false
    })
}

pub fn abi_is_false_arrest(c: &mut Cpu) {
    c.r[2] = is_false_arrest(c, AIHighBTCPerp(c.r[4])) as u32;
}

/// A human cop presses on (the gas, or the brake fully) when it pulled a speeder over.
fn pressing(m: &mut Mem, car: Car) -> bool {
    car.car_flags(m) & CAR_HUMAN_COP != 0 && (m.r16(car.0 + 0x43C) != 0 || m.r8(car.0 + 0x445) == 1)
}

/// `AIHigh_BTC_Perp::CheckForControlsPressed` (AIH_BTCPERP.CPP, 0x800601a8): a human cop
/// (either of two) presses the gas or a direction, or pulls the hand brake.
pub fn check_for_controls_pressed(m: &mut Mem) -> bool {
    let first = Car(m.r32(HUMAN_RACE_CARS));
    let mut pressed = pressing(m, first);
    if NUM_HUMAN_RACE_CARS.get(m) == 2 {
        let second = Car(m.r32(HUMAN_RACE_CARS + 4));
        if pressing(m, second) {
            pressed = true;
        }
    }
    pressed
}

pub fn abi_check_for_controls_pressed(c: &mut Cpu) {
    c.r[2] = check_for_controls_pressed(&mut c.m) as u32;
}

/// `AIHigh_BTC_Perp::HandlePullOver` (AIH_BTCPERP.CPP, 0x80060248): pulled over, the police
/// arrest it; 1.5 s of beating, then a false arrest (the speeder off again, the radio saying
/// so) or the busted overlay shown; once shown, a human cop's controls 5 s after the
/// pull-over complete the arrest (caught). Not pulled over, while the stage has time, a
/// police car catching it pulls it over.
pub fn handle_pull_over(c: &mut Cpu, this: AIHighBTCPerp) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let b = base(this);
        if b.pull_over_mode_(&mut c.m) != 0 {
            r.enter(c, 0x8006_0420, 0x8006_0274, |c| notify_cops_of_arrest(c, this));
            let m = &mut c.m;
            let e = AI_ELAPSED_TIME.get(m);
            b.update_beating_ticks_left_(m, |t| t.wrapping_sub(e));
            if b.beating_ticks_left_(m) <= 0 && this.hud_activated_(m) == 0 {
                if r.enter(c, 0x8005_ff88, 0x8006_02a8, |c| is_false_arrest(c, this)) {
                    let m = &mut c.m;
                    let now = SIM_COUNTER.get(m);
                    b.set_last_pull_over_time_(m, now.wrapping_sub(640));
                    r.enter(c, 0x8006_0590, 0x8006_02c4, |c| notify_cops_of_false_arrest(c, this));
                    let m = &mut c.m;
                    car_of(m, this).set_pull_over(m, 0);
                    b.set_pull_over_mode_(m, 0);
                    let first = Car(m.r32(CARS_LIST));
                    speech(c, &r, first, 7, 0x8006_02e4, 0x8006_0300, &[]);
                } else if this.hud_activated_(&mut c.m) == 0 {
                    r.enter(c, 0x8006_0648, 0x8006_0320, |c| notify_human_cops_of_arrest_hud(c, this));
                    this.set_hud_activated_(&mut c.m, 1);
                }
            }
            let pressed = r.enter(c, 0x8006_01a8, 0x8006_0330, |c| check_for_controls_pressed(&mut c.m));
            let m = &mut c.m;
            if b.beating_ticks_left_(m) > 0 || b.pull_over_mode_(m) == 0 || !pressed {
                return;
            }
            let shown = this.hud_activated_(m);
            r.0[1] = shown as u32;
            if shown != 1 {
                return;
            }
            let now = SIM_COUNTER.get(m);
            if now.wrapping_sub(b.last_pull_over_time_(m)) < 321 {
                return;
            }
            b.set_last_pull_over_time_(m, now);
            b.basic_perp_info_().set_crime_(m, 0);
            r.enter(c, 0x8006_04d8, 0x8006_0394, |c| notify_cops_of_arrest_complete(c, this));
            let m = &mut c.m;
            car_of(m, this).set_pull_over(m, 0);
            b.set_pull_over_mode_(m, 0);
            this.set_caught_(m, 1);
            this.set_hud_activated_(m, 0);
        } else {
            let m = &mut c.m;
            let cop = this.original_activation_cop_(m);
            let caught = cop.time_left_(m) >= 6 && r.enter(c, 0x8005_bf7c, 0x8006_03d8, |c| aih_basicperp::check_if_caught(c, b));
            if caught {
                let m = &mut c.m;
                car_of(m, this).set_pull_over(m, 1);
                b.set_beating_ticks_left_(m, 96);
                b.set_pull_over_mode_(m, 2);
                let now = SIM_COUNTER.get(m);
                b.set_last_pull_over_time_(m, now);
            }
        }
    })
}

pub fn abi_handle_pull_over(c: &mut Cpu) {
    handle_pull_over(c, AIHighBTCPerp(c.r[4]));
}

/// Each active police car (or human cop) told, through the call at `addr` returning to `ra`.
fn notify(c: &mut Cpu, this: AIHighBTCPerp, addr: u32, ra: u32, f: fn(&mut Cpu, AIHighBTCCop, AIHighBTCPerp)) {
    abi::frame(c, 40, |c, _| {
        each_cop(c, aihigh::CAR_COP | CAR_HUMAN_COP, |c, r, o, _| {
            (r.0[2], r.0[3]) = (aihigh::HIGH_LEVEL_AI_OBJS, this.0);
            r.enter(c, addr, ra, |c| f(c, AIHighBTCCop(o), this));
        });
    })
}

/// `AIHigh_BTC_Perp::NotifyCopsOfArrest` (AIH_BTCPERP.CPP, 0x80060420): the police start the
/// arrest.
pub fn notify_cops_of_arrest(c: &mut Cpu, this: AIHighBTCPerp) {
    notify(c, this, 0x8005_d600, 0x8006_04b0, |c, cop, _| aih_btccop::cop_start_arrest(c, cop));
}

pub fn abi_notify_cops_of_arrest(c: &mut Cpu) {
    notify_cops_of_arrest(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::NotifyCopsOfArrestComplete` (AIH_BTCPERP.CPP, 0x800604d8): the police
/// finish it.
pub fn notify_cops_of_arrest_complete(c: &mut Cpu, this: AIHighBTCPerp) {
    notify(c, this, 0x8005_d670, 0x8006_0568, |c, cop, _| aih_btccop::cop_finish_arrest(c, cop));
}

pub fn abi_notify_cops_of_arrest_complete(c: &mut Cpu) {
    notify_cops_of_arrest_complete(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::NotifyCopsOfFalseArrest` (AIH_BTCPERP.CPP, 0x80060590): the police chasing
/// it learn it was a false arrest.
pub fn notify_cops_of_false_arrest(c: &mut Cpu, this: AIHighBTCPerp) {
    notify(c, this, 0x8005_d6d0, 0x8006_0620, |c, cop, perp| aih_btccop::cop_false_arrest(&mut c.m, cop, perp));
}

pub fn abi_notify_cops_of_false_arrest(c: &mut Cpu) {
    notify_cops_of_false_arrest(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::NotifyHumanCopsOfArrestHud` (AIH_BTCPERP.CPP, 0x80060648): each active
/// human cop's busted overlay on, by the arresting car.
pub fn notify_human_cops_of_arrest_hud(c: &mut Cpu, this: AIHighBTCPerp) {
    abi::frame(c, 40, |c, _| {
        each_cop(c, CAR_HUMAN_COP, |c, r, o, _| {
            (r.0[2], r.0[3]) = (this.0, aihigh::HIGH_LEVEL_AI_OBJS);
            let by = base(this).last_arresting_cop_(&mut c.m);
            r.enter(c, 0x8005_ea74, 0x8006_06d8, |c| human_cop_hud_on(c, AIHighBTCHumanCop(o), this, 0, by));
        });
    })
}

pub fn abi_notify_human_cops_of_arrest_hud(c: &mut Cpu) {
    notify_human_cops_of_arrest_hud(c, AIHighBTCPerp(c.r[4]));
}

/// `AIHigh_BTC_Perp::ClearForNewStage` (AIH_BTCPERP.CPP, 0x80060700): no chasers, a speeding
/// crime, not caught, on the human cop's lap; the human cop has its speeder.
pub fn clear_for_new_stage(c: &mut Cpu, this: AIHighBTCPerp, cop: AIHighBTCHumanCop) {
    let m = &mut c.m;
    let b = base(this);
    aih_basicperp::clear(m, b);
    b.basic_perp_info_().set_cops_assigned_(m, 0, 0);
    b.basic_perp_info_().set_cops_assigned_(m, 1, 0);
    b.basic_perp_info_().set_crime_(m, 1);
    this.set_caught_(m, 0);
    let car = car_of(m, this);
    car.set_unlap(m, 0);
    let lap = Car(m.r32(cop.0)).lap(m);
    car.set_lap(m, lap);
    cop.set_need_perp_(m, 0);
}

pub fn abi_clear_for_new_stage(c: &mut Cpu) {
    clear_for_new_stage(c, AIHighBTCPerp(c.r[4]), AIHighBTCHumanCop(c.r[5]));
}

/// `AIHigh_BTC_Perp::CheckForActivation` (AIH_BTCPERP.CPP, 0x80060764): the first human cop
/// needing a speeder for its stage, if this is it (a human speeder, or the stage's car).
pub fn check_for_activation(m: &mut Mem, this: AIHighBTCPerp) -> u32 {
    for i in 0..NUM_CARS.get(m) {
        let car = Car(m.r32(CARS_LIST + 4 * i as u32));
        if car.car_flags(m) & CAR_HUMAN_COP == 0 || car.n().active(m) == 0 {
            continue;
        }
        let index = car.car_index(m);
        let cop = AIHighBTCHumanCop(obj(m, index));
        let needs = if cop.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) == 0 { cop.need_perp_(m) } else { 0 };
        if needs == 0 {
            continue;
        }
        let me = car_of(m, this);
        if me.car_flags(m) & CAR_HUMAN != 0 {
            return cop.0;
        }
        let stage = cop.current_stage_(m) as u32;
        let kind = m.r32(GAME_SETUP.0.wrapping_add(0x1BC).wrapping_add(stage.wrapping_mul(0x34)));
        let info = me.car_info(m).0;
        if kind == m.r32(info) {
            return cop.0;
        }
    }
    0
}

pub fn abi_check_for_activation(c: &mut Cpu) {
    c.r[2] = check_for_activation(&mut c.m, AIHighBTCPerp(c.r[4]));
}

/// The car put on `slice`: `by` slices on from `from` (wrapped round the track as 16 bits).
fn slice_on(m: &mut Mem, car: Car, from: Car, by: i32) {
    let s = from.n().sim_road_info().slice(m) as i16 as i32;
    let at = s.wrapping_add(by);
    let n = NUM_SLICES.get(m);
    let wrapped = if by >= 0 {
        if at >= n { at.wrapping_sub(n & 0xFFFF) } else { at }
    } else if at < 0 {
        at.wrapping_add(n & 0xFFFF)
    } else {
        at
    };
    car.n().sim_road_info().set_slice(m, wrapped as i16);
}

/// `AIHigh_BTC_HumanPerp::NewStage` (AIH_BTCPERP.CPP, 0x8006086c): the stage's speeder, 16
/// slices from the human cop the way it faces, at rest in a barrier-free lane; the cops let
/// go; the radio and dispatcher say so.
pub fn human_perp_new_stage(c: &mut Cpu, this: AIHighBTCHumanPerp, cop: AIHighBTCHumanCop) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        let perp = this.ai_high_btc_perp();
        let m = &mut c.m;
        let dir = cop.initial_direction_(m);
        (r.0[0], r.0[1], r.0[2]) = (this.0, dir as u32, cop.0);
        perp.set_original_activation_cop_(m, cop);
        r.enter(c, 0x8006_0700, 0x8006_0894, |c| clear_for_new_stage(c, perp, cop));
        r.enter(c, 0x8005_fea4, 0x8006_089c, |c| release_cops(c, perp));
        let m = &mut c.m;
        fast_random_step(m);
        let car = car_of(m, perp);
        let cop_car = Car(m.r32(cop.0));
        slice_on(m, car, cop_car, dir << 4);
        car.set_desired_direction(m, dir);
        car.set_direction(m, dir);
        m.w32(sp + 28, 0);
        m.w32(sp + 24, 0);
        r.enter(c, 0x8007_4258, 0x8006_09ac, |c| find_barrier_less_lane_and_position(&mut c.m, car, sp + 24, sp + 28));
        let m = &mut c.m;
        let (slice, lat, dir) = (car.n().sim_road_info().slice(m) as i16 as i32, m.r32(sp + 28) as i32, car.direction(m));
        m.w32(sp + 16, 0);
        m.w32(sp + 20, 0);
        r.enter(c, 0x8006_8450, 0x8006_09d0, |c| crate::ailife::place_car_at_location_at(c, car, slice, lat, dir, 0, 0));
        speech(c, &r, cop_car, 16, 0x8006_09dc, 0x8006_09f8, &[]);
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8006_0a00); // Speech::Dispatch
        speech_call(c, &r, d, 1, 0x8006_0a1c, &[car.0]);
        let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[cop_car.0], 0x8006_0a28);
        speech_call(c, &r, mobile, 1, 0x8006_0a44, &[car.0]);
        let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[cop_car.0], 0x8006_0a50);
        speech_call(c, &r, mobile, 6, 0x8006_0a6c, &[car.0]);
        r.call(c, nfs4rc::f_800bc7ac, &[], 0x8006_0a74); // TrgSfx_RestartTrgSfx
    })
}

pub fn abi_human_perp_new_stage(c: &mut Cpu) {
    human_perp_new_stage(c, AIHighBTCHumanPerp(c.r[4]), AIHighBTCHumanCop(c.r[5]));
}

/// `AIHigh_BTC_HumanPerp::HighExecute` (AIH_BTCPERP.CPP, 0x80060a8c, virtual): caught, the
/// next stage once a human cop needs a speeder; else the pull-over.
pub fn human_perp_high_execute(c: &mut Cpu, this: AIHighBTCHumanPerp) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let perp = this.ai_high_btc_perp();
        if perp.caught_(&mut c.m) == 1 {
            let cop = r.enter(c, 0x8006_0764, 0x8006_0ab4, |c| check_for_activation(&mut c.m, perp));
            if cop != 0 {
                r.enter(c, 0x8006_086c, 0x8006_0ac4, |c| human_perp_new_stage(c, this, AIHighBTCHumanCop(cop)));
                return;
            }
        }
        r.enter(c, 0x8005_ff68, 0x8006_0ad4, |c| handle_cops(c, perp));
    })
}

pub fn abi_human_perp_high_execute(c: &mut Cpu) {
    human_perp_high_execute(c, AIHighBTCHumanPerp(c.r[4]));
}

/// `AIHigh_BTC_AIPerp::AIHigh_BTC_AIPerp(Car_tObj*)` (AIH_BTCPERP.CPP, 0x80060ae4): caught (so
/// waiting for a stage), cruising, contact far off, its car's mass kept.
pub fn ai_perp_construct(c: &mut Cpu, this: AIHighBTCAIPerp, car: Car) -> AIHighBTCAIPerp {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let perp = this.ai_high_btc_perp();
        r.enter(c, 0x8005_c8e4, 0x8006_0af8, |c| aih_basicperp::construct(c, base(perp), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BTC_PERP2);
        perp.set_caught_(m, 1);
        let car = car_of(m, perp);
        aihigh::set_vt(m, this.0, VT_BTC_AI_PERP);
        perp.set_hud_activated_(m, 0);
        perp.set_original_activation_cop_(m, AIHighBTCHumanCop(0));
        this.set_perp_mode_(m, 0);
        this.set_creation_time_(m, 0);
        this.set_made_contact_time_(m, 0);
        this.set_time_until_contact_(m, 0xFA00);
        this.set_escape_duration_(m, 0);
        let mass = car.n().mass(m);
        this.set_original_mass_(m, mass);
        let inv = car.n().mass_inv(m);
        this.set_closest_cop_car_obj_(m, Car(0));
        this.set_closest_cop_car_distance_meters_(m, 0);
        this.set_original_mass_inv_(m, inv);
        this
    })
}

pub fn abi_ai_perp_construct(c: &mut Cpu) {
    c.r[2] = ai_perp_construct(c, AIHighBTCAIPerp(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_BTC_AIPerp::~AIHigh_BTC_AIPerp` (AIH_BTCPERP.CPP, 0x80060b6c): its car's mass back.
pub fn abi_ai_perp_destruct(c: &mut Cpu) {
    let (this, flags) = (AIHighBTCAIPerp(c.r[4]), c.r[5]);
    let m = &mut c.m;
    aihigh::set_vt(m, this.0, VT_BTC_AI_PERP);
    let car = car_of(m, this.ai_high_btc_perp());
    let mass = this.original_mass_(m);
    car.n().set_mass(m, mass);
    let inv = this.original_mass_inv_(m);
    car.n().set_mass_inv(m, inv);
    aihigh::destruct_to_base(c, this.0, flags, Some(VT_BASIC_PERP), 0x8006_0bac);
}

/// `AIHigh_BTC_AIPerp::AvoidCops` (AIH_BTCPERP.CPP, 0x80060bbc): with the closest human cop
/// (not road-side controlled) going its way within 500 m, by where it is (across and along),
/// the strategy chart's moves by chance: pull over (more likely if already pulling over),
/// unless both go fast; or a U-turn when both go fast.
pub fn avoid_cops(m: &mut Mem, this: AIHighBTCAIPerp) {
    let mut stop = false;
    let cop = this.closest_cop_car_obj_(m);
    let car = car_of(m, this.ai_high_btc_perp());
    'out: {
        if cop.0 == 0 || cop.rs_control(m) != 0 {
            break 'out;
        }
        let dir = cop.direction(m);
        if dir != car.direction(m) {
            break 'out;
        }
        let dist = this.closest_cop_car_distance_meters_(m);
        if dist.wrapping_abs() > 0x1F3_FFFF {
            break 'out;
        }
        let across = car.road_position(m).wrapping_sub(cop.road_position(m)).wrapping_mul(dir);
        let along = dist.wrapping_mul(dir);
        let width = cop.n().dimension().x(m);
        let col = if across < width.wrapping_neg() {
            0
        } else if width < across {
            2
        } else {
            1
        };
        let length = cop.n().dimension().z(m);
        let row = if along > 0x19_0000 {
            0
        } else if length < along {
            1
        } else if along < -0x19_0000 {
            4
        } else if along >= length.wrapping_neg() {
            2
        } else {
            3
        };
        let chart = |m: &mut Mem| m.r8(STRATEGY_CHART + (row * 3 + col) as u32);
        let fast = |m: &mut Mem| cop.current_speed(m).wrapping_abs() > 0x11_C71C && car.current_speed(m).wrapping_abs() > 0x11_C71C;
        if chart(m) & 1 != 0 {
            let t = fast_random_step(m);
            let chance = (t >> 8 & 0xFFFF).wrapping_mul(1000) >> 16;
            let odds = (AI_ELAPSED_TIME.get(m).wrapping_mul(7)).wrapping_add(car.pull_over(m).wrapping_mul(500)) as u32;
            if chance < odds && fast(m) {
                stop = true;
                break 'out;
            }
        }
        if chart(m) & 2 != 0 {
            let t = fast_random_step(m);
            let skill = GAME_SETUP.skill(m) as u32;
            let odds = (m.r32(U_TURN_PROB.wrapping_add(skill.wrapping_mul(4))) as i32).wrapping_mul(AI_ELAPSED_TIME.get(m)) as u32;
            let chance = (t >> 8 & 0xFFFF).wrapping_mul(1000) >> 16;
            if chance < odds && fast(m) {
                let d = car.direction(m);
                if d == car.desired_direction(m) {
                    car.set_desired_direction(m, d.wrapping_neg());
                }
            }
        }
    }
    car.set_pull_over(m, stop as i32);
}

pub fn abi_avoid_cops(c: &mut Cpu) {
    avoid_cops(&mut c.m, AIHighBTCAIPerp(c.r[4]));
}

/// `AIHigh_BTC_AIPerp::CalculateTimeTillContact` (AIH_BTCPERP.CPP, 0x80060f00): before meeting
/// the police, the ticks until the closest human cop catches up at their speeds (when they
/// differ by over half a metre a second; else 1000 s... and when it never will, 0xfa00).
pub fn calculate_time_till_contact(c: &mut Cpu, this: AIHighBTCAIPerp) {
    let m = &mut c.m;
    let cop = this.closest_cop_car_obj_(m);
    if cop.0 == 0 || this.perp_mode_(m) as u32 >= 2 {
        this.set_time_until_contact_(m, 0xFA00);
        return;
    }
    let car = car_of(m, this.ai_high_btc_perp());
    let closing = car.current_speed(m).wrapping_sub(cop.current_speed(m));
    let dist = this.closest_cop_car_distance_meters_(m);
    let t = if (closing.wrapping_add(32767) as u32) > 0xFFFE {
        let q = fixeddiv(dist, closing);
        (if q < 0 { q.wrapping_add(1023) } else { q } >> 10).wrapping_neg()
    } else {
        0x3E8_0000
    };
    this.set_time_until_contact_(m, t);
    if this.time_until_contact_(m) < 0 {
        this.set_time_until_contact_(m, 0xFA00);
    }
}

pub fn abi_calculate_time_till_contact(c: &mut Cpu) {
    calculate_time_till_contact(c, AIHighBTCAIPerp(c.r[4]));
}

/// `AIHigh_BTC_AIPerp::FindClosestCop` (AIH_BTCPERP.CPP, 0x80060fa4): the closest human cop
/// along the road and its distance; none.
pub fn find_closest_cop(c: &mut Cpu, this: AIHighBTCAIPerp) {
    let m = &mut c.m;
    let car = car_of(m, this.ai_high_btc_perp());
    let (mut best, mut best_d, mut at) = (0x270F_0000, 0x270F_0000, -1);
    for i in 0..NUM_HUMAN_RACE_CARS.get(m) {
        let cop = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
        if cop.car_flags(m) & CAR_HUMAN_COP == 0 {
            continue;
        }
        let d = apx_spline_distance(m, car, cop);
        if d.wrapping_abs() < best_d {
            (best, best_d, at) = (d, d.wrapping_abs(), cop.car_index(m));
        }
    }
    if at == -1 {
        this.set_closest_cop_car_obj_(m, Car(0));
        return;
    }
    this.set_closest_cop_car_distance_meters_(m, best);
    let cop = Car(m.r32(CARS_LIST.wrapping_add((at as u32).wrapping_mul(4))));
    this.set_closest_cop_car_obj_(m, cop);
}

pub fn abi_find_closest_cop(c: &mut Cpu) {
    find_closest_cop(c, AIHighBTCAIPerp(c.r[4]));
}

/// The cop camera on the speeder (`on`), its views (the car's index as the target).
fn cop_camera(m: &mut Mem, car: Car, on: u32) {
    m.w32(CAMERA + 0x80, on);
    let index = m.r8(car.0 + 0x254);
    m.w32(CAMERA + 0x190, on);
    m.w8(CAMERA + 0x84, index);
    let index = m.r8(car.0 + 0x254);
    m.w8(CAMERA + 0x194, index);
}

/// The police met: the contact time, the radio (once), the cop camera on the speeder.
fn meet(c: &mut Cpu, r: &mut Saved, this: AIHighBTCAIPerp, ra: [u32; 2]) {
    let m = &mut c.m;
    let now = SIM_COUNTER.get(m);
    this.set_made_contact_time_(m, now);
    r.0[0] = MODE_MET as u32;
    let car = car_of(m, this.ai_high_btc_perp());
    if this.perp_mode_(m) != MODE_MET {
        let cop = this.ai_high_btc_perp().original_activation_cop_(m);
        let cop_car = Car(m.r32(cop.0));
        speech(c, r, cop_car, 1, ra[0], ra[1], &[car.0]);
    }
    let m = &mut c.m;
    this.set_perp_mode_(m, MODE_MET);
    cop_camera(m, car, MODE_MET as u32);
}

/// The speeder out of play once caught (or with no state): a not-active state, the car put
/// on the road at the start and taken out of the race; cruising again.
fn out_of_play(c: &mut Cpu, r: &mut Saved, this: AIHighBTCAIPerp, sp: u32) {
    let m = &mut c.m;
    let s = crate::aistate::pool_new(m);
    r.0[2] = s;
    let car = car_of(m, this.ai_high_btc_perp());
    r.0[0] = car.0;
    crate::aistate::base_construct(m, AIStateBase(s), car);
    m.w32(s.wrapping_add(4), VT_STATE_NON_ACTIVE);
    r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_1398); // memset
    let m = &mut c.m;
    let index = car.car_index(m);
    m.w32(sp + 20, (index.wrapping_mul(5) << 17) as u32);
    let obj = crate::types::NewtonObj(m.r32(s));
    r.enter(c, 0x800a_22a8, 0x8006_13c0, |c| crate::newton::set_initial_slice_position_orientation_etc(c, obj, 0, Coord(sp + 16), 1));
    let m = &mut c.m;
    Car(m.r32(s)).n().set_active(m, 0);
    aihigh::set_state(c, r, this.0, s, 7, 0x8006_13f8);
    this.set_perp_mode_(&mut c.m, 0);
}

/// `AIHigh_BTC_AIPerp::HighExecute` (AIH_BTCPERP.CPP, 0x800610a4, virtual): each AI tick, the
/// closest human cop and the time till contact; driving: meeting the police within 5 s,
/// fleeing, escaping after its escape time (the cops let go; the cop camera off 1 s before,
/// with the radio), its human cop asked to keep up (80% of its speed) when ahead; escaped,
/// the pull-over and dodging the police, out of play once caught; not active, a stage's
/// start when a human cop needs it (scheduled again); cruising, meeting the police, then
/// driving and fleeing. Then its state.
pub fn ai_perp_high_execute(c: &mut Cpu, this: AIHighBTCAIPerp) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let perp = this.ai_high_btc_perp();
        let car = car_of(&mut c.m, perp);
        if car.n().active(&mut c.m) != 0 {
            r.enter(c, 0x8006_0fa4, 0x8006_10dc, |c| find_closest_cop(c, this));
            r.enter(c, 0x8006_0f00, 0x8006_10e4, |c| calculate_time_till_contact(c, this));
        }
        let kind = c.m.r32(this.0 + 8);
        match kind {
            2 => {
                match this.perp_mode_(&mut c.m) {
                    1 if this.time_until_contact_(&mut c.m) < 320 => meet(c, &mut r, this, [0x8006_1188, 0x8006_11a4]),
                    2 => this.set_perp_mode_(&mut c.m, MODE_FLEEING),
                    4 => {
                        let m = &mut c.m;
                        r.0[0] = crate::globals::SIM_GLOBAL.0;
                        if this.escape_duration_(m) < SIM_COUNTER.get(m).wrapping_sub(this.made_contact_time_(m)) {
                            this.set_perp_mode_(m, MODE_ESCAPED);
                            r.enter(c, 0x8005_fea4, 0x8006_1218, |c| release_cops(c, perp));
                        }
                        let m = &mut c.m;
                        if this.escape_duration_(m).wrapping_sub(64) < SIM_COUNTER.get(m).wrapping_sub(this.made_contact_time_(m)) {
                            r.0[0] = CAMERA;
                            if m.r32(CAMERA + 0x80) != 0 {
                                let cop = perp.original_activation_cop_(m);
                                let cop_car = Car(m.r32(cop.0));
                                speech(c, &r, cop_car, 6, 0x8006_1260, 0x8006_127c, &[car.0]);
                                r.call(c, nfs4rc::f_800866f8, &[3], 0x8006_1284); // the camera's mode
                            }
                            cop_camera(&mut c.m, car, 0);
                        }
                        let m = &mut c.m;
                        let cop = perp.original_activation_cop_(m);
                        let cop_car = Car(m.r32(cop.0));
                        if cop_car.direction(m) == car.direction(m) {
                            let d = apx_spline_distance(m, car, cop_car);
                            if d.wrapping_mul(car.direction(m)) > 0 {
                                let speed = fixedmult(car.current_speed(m).wrapping_abs(), 0xCCCC);
                                cop.set_requested_desired_speed_(m, speed);
                            }
                        }
                    }
                    _ => {}
                }
                if this.perp_mode_(&mut c.m) == MODE_ESCAPED {
                    r.enter(c, 0x8005_ff68, 0x8006_1338, |c| handle_cops(c, perp));
                    if base(perp).pull_over_mode_(&mut c.m) != 2 {
                        r.enter(c, 0x8006_0bbc, 0x8006_1350, |c| avoid_cops(&mut c.m, this));
                    }
                    if perp.caught_(&mut c.m) != 0 {
                        out_of_play(c, &mut r, this, sp);
                    }
                }
            }
            0 => out_of_play(c, &mut r, this, sp),
            7 => {
                let cop = r.enter(c, 0x8006_0764, 0x8006_1414, |c| check_for_activation(&mut c.m, perp));
                if cop != 0 {
                    r.enter(c, 0x8006_1564, 0x8006_1424, |c| ai_perp_new_stage(c, this, AIHighBTCHumanCop(cop)));
                    this.ai_high_btc_perp().ai_high_basic_perp().ai_high_base().set_scheduling_off_(&mut c.m, 0);
                } else {
                    this.ai_high_btc_perp().ai_high_basic_perp().ai_high_base().set_scheduling_off_(&mut c.m, 1);
                }
            }
            10 => {
                let m = &mut c.m;
                if this.perp_mode_(m) == 0 && this.time_until_contact_(m) < 320 {
                    meet(c, &mut r, this, [0x8006_148c, 0x8006_14a8]);
                } else {
                    let mode = this.perp_mode_(m);
                    r.0[2] = mode as u32;
                    if mode == MODE_MET {
                        let s = crate::aistate::pool_new(m);
                        r.enter(c, 0x8007_03bc, 0x8006_1504, |c| crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car));
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, mode, 0x8006_1530);
                        this.set_perp_mode_(&mut c.m, MODE_FLEEING);
                    }
                }
            }
            _ => {}
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8006_154c, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_ai_perp_high_execute(c: &mut Cpu) {
    ai_perp_high_execute(c, AIHighBTCAIPerp(c.r[4]));
}

/// `AIHigh_BTC_AIPerp::NewStage` (AIH_BTCPERP.CPP, 0x80061564): the stage's speeder: repaired,
/// its lights (on at night), simulated always, the cop camera on it; a third of the time
/// driving towards the human cop (fast), else cruising ahead of it or (with the cop at rest)
/// driving away, its escape time by those; its glue, speed and weight from the stage and the
/// skill, its personality, colours and map; the radio and dispatcher say so.
pub fn ai_perp_new_stage(c: &mut Cpu, this: AIHighBTCAIPerp, cop: AIHighBTCHumanCop) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        let perp = this.ai_high_btc_perp();
        let m = &mut c.m;
        let stage = cop.current_stage_(m);
        let cop_car = Car(m.r32(cop.0));
        (r.0[0], r.0[1], r.0[3], r.0[5]) = (cop.0, stage as u32, this.0, cop_car.0);
        perp.set_original_activation_cop_(m, cop);
        r.enter(c, 0x8006_0700, 0x8006_159c, |c| clear_for_new_stage(c, perp, cop));
        let m = &mut c.m;
        let car = car_of(m, perp);
        for k in 0..10 {
            car.n().set_damage(m, k, 0);
        }
        let render = car.0;
        m.w16(render + 0x8B4, 0);
        m.w16(render + 0x8B6, 0);
        if GAME_SETUP.time(m) != 0 {
            m.w16(render + 0x8B4, 51);
            m.w16(render + 0x8B6, 2);
        }
        m.w16(render + 0x8B8, 0);
        m.w16(render + 0x8BA, 0);
        m.w16(render + 0x87E, 0);
        car.set_force_no_sim_optz(m, 1);
        cop_camera(m, car, 1);
        r.call(c, nfs4rc::f_800a61e4, &[], 0x8006_165c); // Object_ClearCustomObjects
        let m = &mut c.m;
        let t = fast_random_step(m);
        let dir = cop.initial_direction_(m);
        ROAD_BLOCK_STATE.set(m, 0);
        let chance = ((t >> 8 & 0xFFFF).wrapping_mul(1000) >> 16) as i32;
        let moving = cop.initial_movement_(m);
        // (towards the cop or not, driving or cruising, the escape time, the slices ahead)
        let (sign, towards, driving, ahead);
        if chance < 333 {
            (sign, towards, driving) = (-1, 0, 1);
            cop_car.set_desired_speed(m, 0xD_5555);
            cop.set_requested_desired_speed_(m, 0xD_5555);
            if moving != 0 {
                this.set_escape_duration_(m, 640);
                ahead = 225;
            } else {
                this.set_escape_duration_(m, 384);
                ahead = 400;
            }
        } else if moving == 0 {
            (sign, towards, driving, ahead) = (1, 0, 1, 400);
            cop_car.set_desired_speed(m, 0x2C_71C7);
            cop.set_requested_desired_speed_(m, 0x2C_71C7);
            this.set_escape_duration_(m, 384);
        } else {
            (sign, towards, driving, ahead) = (1, 1, 0, 40);
            cop_car.set_desired_speed(m, 0x2C_71C7);
            cop.set_requested_desired_speed_(m, 0x2C_71C7);
            this.set_escape_duration_(m, 480);
        }
        r.0[4] = driving as u32;
        let sixth = (((ahead as i64) * 0x2AAA_AAAB) >> 32) as i32;
        let by = sixth.wrapping_mul(sign).wrapping_mul(dir);
        slice_on(m, car, cop_car, by);
        let facing = sign.wrapping_mul(dir);
        car.set_desired_direction(m, if towards == 1 { facing } else { facing.wrapping_neg() });
        let d = car.desired_direction(m);
        car.set_direction(m, d);
        m.w32(sp + 28, 0);
        m.w32(sp + 24, 0);
        r.enter(c, 0x8007_4258, 0x8006_1858, |c| find_barrier_less_lane_and_position(&mut c.m, car, sp + 24, sp + 28));
        let m = &mut c.m;
        let (slice, lat, dir) = (car.n().sim_road_info().slice(m) as i16 as i32, m.r32(sp + 28) as i32, car.direction(m));
        let speed = if driving == 1 { 0x1F_1C71 } else { 0x11_C71C };
        m.w32(sp + 16, speed as u32);
        m.w32(sp + 20, 0);
        r.0[2] = 0x8011_0000;
        r.enter(c, 0x8006_8450, 0x8006_189c, |c| crate::ailife::place_car_at_location_at(c, car, slice, lat, dir, speed, 0));
        r.0[2] = GAME_SETUP.0;
        r.call(c, nfs4rc::f_80083dfc, &[], 0x8006_18a4); // Camera_Update
        let m = &mut c.m;
        let info = GAME_SETUP.0.wrapping_add((stage as u32).wrapping_mul(0x34));
        (r.0[0], r.0[1]) = (info, AITUNE_BTC);
        let tune = |m: &mut Mem, k: u32| {
            let skill = GAME_SETUP.skill(m) as u32;
            m.r32(AITUNE_BTC.wrapping_add(skill.wrapping_mul(28)).wrapping_add(k)) as i32
        };
        let glue = fixedmult(m.r32(info + 0x1EC) as i32, tune(m, 0));
        car.set_btc_glue_modifier(m, glue);
        let speed = fixedmult(m.r32(info + 0x1E4) as i32, tune(m, 4));
        car.set_speed_factor(m, speed);
        let mass = fixedmult(this.original_mass_(m), m.r32(info + 0x1E8) as i32);
        let mass = fixedmult(mass, tune(m, 8));
        car.n().set_mass(m, mass);
        let inv = fixeddiv(0x1_0000, car.n().mass(m));
        car.n().set_mass_inv(m, inv);
        let personality = m.r32(info + 0x1CC) as i32;
        r.enter(c, 0x8006_9234, 0x8006_1994, |c| crate::aiperson::set_personality(c, car, personality));
        let colour = c.m.r32(info + 0x1C0);
        r.call(c, nfs4rc::f_800ae8c4, &[car.0, colour], 0x8006_19a4); // R3DCar_ChangeTrafficColor
        let m = &mut c.m;
        let car_info = car.car_info(m).0;
        let v = m.r32(info + 0x1C4);
        m.w32(car_info + 0x9C, v);
        let v = m.r32(info + 0x1C8);
        m.w32(car_info + 0x94, v);
        r.call(c, nfs4rc::f_800d661c, &[], 0x8006_19d4); // Hud_InitMap
        let m = &mut c.m;
        let now = SIM_COUNTER.get(m);
        this.set_creation_time_(m, now);
        if driving == 1 {
            let s = crate::aistate::pool_new(m);
            r.enter(c, 0x8007_03bc, 0x8006_1a04, |c| crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car));
            r.0[0] = s;
            aihigh::set_state(c, &r, this.0, s, 2, 0x8006_1a30);
            this.set_perp_mode_(&mut c.m, 1);
        } else {
            let s = crate::aistate::pool_new(m);
            r.enter(c, 0x8007_2dfc, 0x8006_1a60, |c| crate::aistate::cruise_construct(c, crate::types::AIStateCruise(s), car, 1, 0x8000));
            r.0[0] = s;
            aihigh::set_state(c, &r, this.0, s, 10, 0x8006_1a8c);
            this.set_perp_mode_(&mut c.m, 0);
        }
        speech(c, &r, cop_car, 16, 0x8006_1aa4, 0x8006_1ac0, &[]);
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8006_1ac8); // Speech::Dispatch
        speech_call(c, &r, d, 1, 0x8006_1ae4, &[car.0]);
        r.call(c, nfs4rc::f_800bc7ac, &[], 0x8006_1aec); // TrgSfx_RestartTrgSfx
    })
}

pub fn abi_ai_perp_new_stage(c: &mut Cpu) {
    ai_perp_new_stage(c, AIHighBTCAIPerp(c.r[4]), AIHighBTCHumanCop(c.r[5]));
}

/// `AIHigh_BTC_HumanPerp::~AIHigh_BTC_HumanPerp` (AIHIGH.H, 0x80061b10) and
/// `AIHigh_BTC_Perp`'s (0x80061b38).
pub fn abi_human_perp_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8006_1b28);
}

pub fn abi_perp_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8006_1b50);
}

/// `AIState_NonActive::~AIState_NonActive` (AISTATE.H, 0x80061b68, this file's copy).
pub fn abi_non_active_destruct(c: &mut Cpu) {
    aih_btccop::non_active_destruct(&mut c.m, c.r[4], c.r[5], VT_STATE_NON_ACTIVE, VT_STATE_BASE);
}

/// `AIState_Base::~AIState_Base` (AISTATE.H, 0x80061bbc, this file's copy).
pub fn abi_state_base_destruct(c: &mut Cpu) {
    aihigh::state_destruct(c, c.r[4], c.r[5], VT_STATE_BASE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{BTC_ARGS, HEAP_OBJS, OBJS, args};
    use crate::test_util::{Rng, against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    /// The "be the cop" arguments: a human cop, a wingman, a speeder (objects; 0 if none).
    fn btc(c: &mut Cpu) -> [u32; 3] {
        std::array::from_fn(|k| c.m.r32(BTC_ARGS + 4 * k as u32))
    }

    /// The first object of the class (its vtable), or the speeder argument, or the first object.
    fn of_class(c: &mut Cpu, vt: u32) -> u32 {
        let n = NUM_CARS.get(&mut c.m) as u32;
        (0..n).map(|i| OBJS + 0x100 * i).find(|&o| c.m.r32(o + 0x14) == vt).unwrap_or_else(|| match btc(c)[2] {
            0 => OBJS,
            v => v,
        })
    }

    fn ai_perp(c: &mut Cpu) -> u32 {
        of_class(c, VT_BTC_AI_PERP)
    }

    fn human_perp(c: &mut Cpu) -> u32 {
        of_class(c, crate::aihigh::VT_BTC_HUMAN_PERP)
    }

    fn or_obj(v: u32) -> u32 {
        if v != 0 { v } else { OBJS }
    }

    /// The "be the cop" shuffle with the speeders' modes, times and closest cops of a race.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aih_btccop::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        let now = SIM_COUNTER.get(m);
        AI_ELAPSED_TIME.set(m, [1, 4, 40, 200][pick(4) as usize]);
        for i in 0..n {
            let o = OBJS + 0x100 * i;
            let vt = m.r32(o + 0x14);
            if vt != VT_BTC_AI_PERP && vt != crate::aihigh::VT_BTC_HUMAN_PERP {
                continue;
            }
            let p = AIHighBTCAIPerp(o);
            p.set_perp_mode_(m, [0, 1, 2, 4, 5][pick(5) as usize]);
            p.set_creation_time_(m, now - pick(2000));
            p.set_made_contact_time_(m, [0, now - pick(3000)][pick(2) as usize]);
            p.set_time_until_contact_(m, pick(800) - 100);
            p.set_escape_duration_(m, [pick(2000), 480 + pick(1000)][pick(2) as usize]);
            p.set_original_mass_(m, 0x1_0000 + pick(0x1_0000));
            p.set_original_mass_inv_(m, 0x8000 + pick(0x8000));
            let cop = Car(m.r32(CARS_LIST + 4 * pick(n) as u32));
            p.set_closest_cop_car_obj_(m, [Car(0), cop][pick(4).min(1) as usize]);
            p.set_closest_cop_car_distance_meters_(m, pick(0x200_0000) - 0x100_0000);
            // (often a close pursuit: the cop the same way, both fast, side by side or so)
            let me = Car(m.r32(o));
            if cop.0 != 0 && pick(2) == 0 {
                p.set_closest_cop_car_distance_meters_(m, [pick(0x60_0000) - 0x30_0000, pick(0x3C0_0000) - 0x1E0_0000][pick(2) as usize]);
                cop.set_rs_control(m, 0);
                let dir = me.direction(m);
                cop.set_direction(m, dir);
                let fast = |p: &mut dyn FnMut(u32) -> i32| (0x11_C71C + p(0x20_0000) - 0x4_0000) * [1, -1][p(2) as usize];
                let v = fast(&mut pick);
                cop.set_current_speed(m, v);
                let v = fast(&mut pick);
                me.set_current_speed(m, v);
                let at = cop.road_position(m);
                me.set_road_position(m, at + pick(0x8_0000) - 0x4_0000);
                me.set_pull_over(m, pick(2));
                me.set_desired_direction(m, [dir, -dir][pick(2) as usize]);
            }
            // (cruising, its state's kind 10, now and then)
            if pick(4) == 0 {
                m.w32(o + 8, 10);
            }
            let b = AIHighBasicPerp(o);
            b.set_pull_over_mode_(m, pick(4));
            b.set_beating_ticks_left_(m, pick(40));
            b.set_last_pull_over_time_(m, now - pick(1000));
        }
    }

    #[test]
    fn btcperp_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7], [u32; 3]) -> Vec<u32>, Port); 24] = [
            ("ReleaseCops", nfs4rc::f_8005fea4, |c, _, _| vec![ai_perp(c)], |c, a| {
                release_cops(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("HandleCops", nfs4rc::f_8005ff68, |c, _, _| vec![ai_perp(c)], |c, a| {
                handle_cops(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("IsFalseArrest", nfs4rc::f_8005ff88, |c, _, _| vec![ai_perp(c)], |c, a| Some(is_false_arrest(c, AIHighBTCPerp(a[0])) as u32)),
            ("IsFalseArrestHuman", nfs4rc::f_8005ff88, |c, _, _| vec![human_perp(c)], |c, a| Some(is_false_arrest(c, AIHighBTCPerp(a[0])) as u32)),
            ("CheckForControlsPressed", nfs4rc::f_800601a8, |_, _, _| vec![], |c, _| Some(check_for_controls_pressed(&mut c.m) as u32)),
            ("HandlePullOver", nfs4rc::f_80060248, |c, _, _| vec![ai_perp(c)], |c, a| {
                handle_pull_over(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("HandlePullOverHuman", nfs4rc::f_80060248, |c, _, _| vec![human_perp(c)], |c, a| {
                handle_pull_over(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("NotifyCopsOfArrest", nfs4rc::f_80060420, |c, _, _| vec![ai_perp(c)], |c, a| {
                notify_cops_of_arrest(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("NotifyCopsOfArrestComplete", nfs4rc::f_800604d8, |c, _, _| vec![ai_perp(c)], |c, a| {
                notify_cops_of_arrest_complete(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("NotifyCopsOfFalseArrest", nfs4rc::f_80060590, |c, _, _| vec![ai_perp(c)], |c, a| {
                notify_cops_of_false_arrest(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("NotifyHumanCopsOfArrestHud", nfs4rc::f_80060648, |c, _, _| vec![ai_perp(c)], |c, a| {
                notify_human_cops_of_arrest_hud(c, AIHighBTCPerp(a[0]));
                None
            }),
            ("ClearForNewStage", nfs4rc::f_80060700, |c, _, b| vec![ai_perp(c), or_obj(b[0])], |c, a| {
                clear_for_new_stage(c, AIHighBTCPerp(a[0]), AIHighBTCHumanCop(a[1]));
                None
            }),
            ("CheckForActivation", nfs4rc::f_80060764, |c, _, _| vec![ai_perp(c)], |c, a| Some(check_for_activation(&mut c.m, AIHighBTCPerp(a[0])))),
            ("HumanPerpNewStage", nfs4rc::f_8006086c, |c, _, b| vec![human_perp(c), or_obj(b[0])], |c, a| {
                human_perp_new_stage(c, AIHighBTCHumanPerp(a[0]), AIHighBTCHumanCop(a[1]));
                None
            }),
            ("HumanPerpHighExecute", nfs4rc::f_80060a8c, |c, _, _| vec![human_perp(c)], |c, a| {
                human_perp_high_execute(c, AIHighBTCHumanPerp(a[0]));
                None
            }),
            ("AIPerpConstruct", nfs4rc::f_80060ae4, |_, a, _| vec![OBJS + 0x700, a[0]], |c, a| Some(ai_perp_construct(c, AIHighBTCAIPerp(a[0]), Car(a[1])).0)),
            ("AvoidCops", nfs4rc::f_80060bbc, |c, _, _| vec![ai_perp(c)], |c, a| {
                avoid_cops(&mut c.m, AIHighBTCAIPerp(a[0]));
                None
            }),
            ("CalculateTimeTillContact", nfs4rc::f_80060f00, |c, _, _| vec![ai_perp(c)], |c, a| {
                calculate_time_till_contact(c, AIHighBTCAIPerp(a[0]));
                None
            }),
            ("FindClosestCop", nfs4rc::f_80060fa4, |c, _, _| vec![ai_perp(c)], |c, a| {
                find_closest_cop(c, AIHighBTCAIPerp(a[0]));
                None
            }),
            ("AIPerpHighExecute", nfs4rc::f_800610a4, |c, _, _| vec![ai_perp(c)], |c, a| {
                ai_perp_high_execute(c, AIHighBTCAIPerp(a[0]));
                None
            }),
            ("AIPerpNewStage", nfs4rc::f_80061564, |c, _, b| vec![ai_perp(c), or_obj(b[0])], |c, a| {
                ai_perp_new_stage(c, AIHighBTCAIPerp(a[0]), AIHighBTCHumanCop(a[1]));
                None
            }),
            ("AIPerpDestruct", nfs4rc::f_80060b6c, |c, a, _| vec![ai_perp(c), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_ai_perp_destruct(c);
                None
            }),
            ("HumanPerpDestruct", nfs4rc::f_80061b10, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_human_perp_destruct(c);
                None
            }),
            ("PerpDestruct", nfs4rc::f_80061b38, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_perp_destruct(c);
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3C10 + k as u64, shuffle, original, &|c| {
                let (a, b) = (args(c), btc(c));
                call(c, a, b)
            }, port);
        }
    }

    /// The states' destructors (this file's copies).
    #[test]
    fn btcperp_state_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 2] = [
            ("NonActiveDestruct", nfs4rc::f_80061b68, |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_non_active_destruct(c);
                None
            }),
            ("StateBaseDestruct", nfs4rc::f_80061bbc, |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_state_base_destruct(c);
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 500, 0x3C40 + k as u64, shuffle, original, &|c| {
                let (a, o) = (args(c), ai_perp(c));
                vec![c.m.r32(o + 4), a[4] * 3]
            }, port);
        }
    }

    /// The stack the speeders' ticks leave (their states', the new stages').
    #[test]
    fn btcperp_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 7] = [
            ("AIPerpHighExecute", 0x8006_10a4, nfs4rc::f_800610a4),
            ("AIPerpNewStage", 0x8006_1564, nfs4rc::f_80061564),
            ("HumanPerpNewStage", 0x8006_086c, nfs4rc::f_8006086c),
            ("HumanPerpHighExecute", 0x8006_0a8c, nfs4rc::f_80060a8c),
            ("HandlePullOver", 0x8006_0248, nfs4rc::f_80060248),
            ("IsFalseArrest", 0x8005_ff88, nfs4rc::f_8005ff88),
            ("AIPerpConstruct", 0x8006_0ae4, nfs4rc::f_80060ae4),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 300, 0x3C50 + k as u64, shuffle, &|c, _| {
                let (a, b) = (args(c), btc(c));
                match name {
                    "AIPerpNewStage" => vec![ai_perp(c), or_obj(b[0])],
                    "HumanPerpNewStage" => vec![human_perp(c), or_obj(b[0])],
                    "HumanPerpHighExecute" => vec![human_perp(c)],
                    "AIPerpConstruct" => vec![OBJS + 0x700, a[0]],
                    _ => vec![ai_perp(c)],
                }
            });
        }
    }
}
