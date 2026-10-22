//! AIH_BTCCOP.CPP: the police of a "be the cop" race: `AIHigh_BTC_Cop` (a police car chasing a
//! speeder, arresting it), `AIHigh_BTC_HumanCop` (a human cop: the stages, each a speeder to
//! bust before the time runs out, its start on the road, the radio's calls for a wingman,
//! a blockade or a spike belt) and `AIHigh_BTC_Wingman` (an AI police car helping a human cop).

use crate::abi::{self, Saved};
use crate::aih_basiccop::{self, ROAD_BLOCK_STATE, fast_random_step};
use crate::aih_basicperp::{self, speech, speech_call};
use crate::aihigh::{self, COP_GAME_TYPE, HIGH_LEVEL_AI_OBJS, VT_BTC_COP, VT_BTC_HUMAN_COP, VT_BTC_WINGMAN, obj};
use crate::aiworld::{apx_spline_distance, calc_road_bend, find_barrier_less_lane_and_position, is_driveable_lane, is_driveable_lane_in_slice_range};
use crate::globals::{
    AI_ELAPSED_TIME, AIDATARECORD_TRACK_CURVE, AITUNE_DRIVE_SIDE, BWORLDSM_SLICES, FORCE_HUMAN_HAND_BRAKE, GAME_SETUP, HUMAN_RACE_CARS, I32, NUM_RACE_CARS, NUM_SLICES,
    NUM_TRAFFIC_CARS, RACE_CARS, SIM_COUNTER, SIM_END_GAME, SPIKE_BELT, STACK_SPEED_UP_ENABLED, TRAFFIC_CARS, W_SAVE_PTR,
};
use crate::types::{AIDataRecord, AIHighBTCCop, AIHighBTCHumanCop, AIHighBTCPerp, AIHighBTCWingman, AIHighBasicPerp, AIStateBase, Car, Coord};
use ps1rt::{Cpu, Mem};

/// `AIH_BTCCop_chasePositions`: where a chaser drives, relative to the speeder, by its place.
const CHASE_POSITIONS: u32 = 0x8010_DA00;
/// `AIH_BTC_Cop_1HC1HP_StageTimes`: each stage's time with a human cop and a human speeder.
const STAGE_TIMES_1HC1HP: u32 = 0x8010_D9F4;
/// `AITune_BTC` (`AITune_BTC_t[3]`, by the skill): the base chase time and when the wingman,
/// a blockade and a spike belt can be called.
const AITUNE_BTC: u32 = 0x8010_F0D8;
/// `gBlockadeTypes`: the blockades' elements (flags), five kinds.
const BLOCKADE_TYPES: u32 = 0x8013_D858;
/// The tick of the last request on the radio (a human cop's event button).
const LAST_REQUEST_TIME: I32 = I32(0x8013_D850);
/// Whether the time running out still has to be said on the radio.
const TIME_UP_SPEECH: I32 = I32(0x8013_D854);

/// `freezeMode_`: chasing (0), frozen at a stage's start (1), a false arrest (2), arresting
/// (3), arrested (4).
const FREEZE_STAGE_START: i32 = 1;
const FREEZE_FALSE_ARREST: i32 = 2;
const FREEZE_ARRESTING: i32 = 3;
const FREEZE_ARRESTED: i32 = 4;

/// `Car_tObj.AIFlags`: on duty.
const AI_ON_DUTY: i32 = 0x2;

fn tune(m: &mut Mem, field: u32) -> i32 {
    let skill = GAME_SETUP.skill(m) as u32;
    m.r32(AITUNE_BTC.wrapping_add(skill.wrapping_mul(28)).wrapping_add(field)) as i32
}

/// The current stage's speeder's data (`GameSetup_gData.perpInfo[stage]`), field at `field`.
fn perp_info(m: &mut Mem, this: AIHighBTCHumanCop, field: u32) -> i32 {
    let stage = this.current_stage_(m) as u32;
    m.r32(GAME_SETUP.0.wrapping_add(0x1BC).wrapping_add(stage.wrapping_mul(0x34)).wrapping_add(field)) as i32
}

/// `slice + by` wrapped round the track (by one lap at most), as the original adds a step.
fn slice_plus(m: &mut Mem, slice: i32, by: i32) -> i32 {
    let s = slice.wrapping_add(by);
    let n = NUM_SLICES.get(m);
    if by >= 0 {
        if s >= n { s - n } else { s }
    } else if s < 0 {
        s + n
    } else {
        s
    }
}

/// `AIHigh_BTC_Cop::AIHigh_BTC_Cop(Car_tObj*, int)` (AIH_BTCCOP.CPP, 0x8005d35c): no target,
/// first in the chase, chasing.
pub fn cop_construct(c: &mut Cpu, this: AIHighBTCCop, car: Car, index: i32) -> AIHighBTCCop {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8005_c954, 0x8005_d370, |c| aih_basiccop::construct(c, this.ai_high_basic_cop(), car, index));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BTC_COP);
        this.set_perp_target_(m, AIHighBTCPerp(0));
        this.set_chase_index_(m, 0);
        this.set_freeze_mode_(m, 0);
        this
    })
}

pub fn abi_cop_construct(c: &mut Cpu) {
    c.r[2] = cop_construct(c, AIHighBTCCop(c.r[4]), Car(c.r[5]), c.r[6] as i32).0;
}

/// `AIHigh_BTC_Cop::AssignToPlayer` (AIH_BTCCOP.CPP, 0x8005d39c): chase `perp` (or nobody): off
/// the last one's chasers, onto its.
pub fn cop_assign_to_player(c: &mut Cpu, this: AIHighBTCCop, perp: AIHighBTCPerp) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this.0, perp.0);
        let m = &mut c.m;
        let base = this.ai_high_basic_cop();
        let old = this.perp_target_(m);
        if old.0 != 0 {
            let (cop, car, kind) = (base.cop_index_(m), base.ai_high_base().car_obj_(m).car_index(m), base.type_(m));
            aih_basicperp::remove_chaser(m, AIHighBasicPerp(old.0), cop, kind);
            let _ = car;
        }
        this.set_perp_target_(&mut c.m, perp);
        if perp.0 != 0 {
            let m = &mut c.m;
            let (cop, car, kind) = (base.cop_index_(m), base.ai_high_base().car_obj_(m).car_index(m), base.type_(m));
            r.enter(c, 0x8005_c6f8, 0x8005_d3f8, |c| aih_basicperp::add_chaser(c, AIHighBasicPerp(perp.0), cop, car, kind));
        }
    })
}

pub fn abi_cop_assign_to_player(c: &mut Cpu) {
    cop_assign_to_player(c, AIHighBTCCop(c.r[4]), AIHighBTCPerp(c.r[5]));
}

/// `AIHigh_BTC_Cop::GetCheckChasePosition` (AIH_BTCCOP.CPP, 0x8005d40c): its place among the
/// speeder's chasers brought up to date, and where that place drives (`out`); whether the
/// place changed.
pub fn cop_get_check_chase_position(c: &mut Cpu, this: AIHighBTCCop, out: Coord) -> bool {
    let at = abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1], r.0[2]) = (this.0, 0, out.0);
        let m = &mut c.m;
        let base = this.ai_high_basic_cop();
        let (perp, cop, car) = (this.perp_target_(m), base.cop_index_(m), base.ai_high_base().car_obj_(m).car_index(m));
        r.enter(c, 0x8005_c74c, 0x8005_d440, |c| aih_basicperp::check_chaser_position(c, AIHighBasicPerp(perp.0), cop, car))
    });
    let m = &mut c.m;
    let changed = at != this.chase_index_(m);
    if changed {
        this.set_chase_index_(m, at);
    }
    let p = Coord(CHASE_POSITIONS.wrapping_add((this.chase_index_(m) as u32).wrapping_mul(12))).get(m);
    out.set(m, p);
    changed
}

pub fn abi_cop_get_check_chase_position(c: &mut Cpu) {
    c.r[2] = cop_get_check_chase_position(c, AIHighBTCCop(c.r[4]), Coord(c.r[5])) as u32;
}

/// `AIHigh_BTC_Cop::CheckForNewTarget` (AIH_BTCCOP.CPP, 0x8005d4a8): chase the closest active
/// racer with a crime (not a human cop) along the road; nobody when none has one. Whether it
/// changed to a new speeder.
pub fn cop_check_for_new_target(c: &mut Cpu, this: AIHighBTCCop) -> bool {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let old = this.perp_target_(m).0;
        let me = this.ai_high_basic_cop().ai_high_base().car_obj_(m);
        let (mut best, mut best_d) = (0u32, 0x2710_0000);
        let n = NUM_RACE_CARS.get(m);
        for i in 0..n {
            let car = Car(m.r32(RACE_CARS + 4 * i as u32));
            if car.n().active(m) == 0 || car.car_flags(m) & aihigh::CAR_HUMAN_COP != 0 {
                continue;
            }
            let index = car.car_index(m);
            let o = obj(m, index);
            r.0[0] = o;
            if AIHighBasicPerp(o).basic_perp_info_().crime_(m) == 0 {
                continue;
            }
            let d = apx_spline_distance(m, me, car).wrapping_abs();
            if d < best_d {
                (best_d, best) = (d, o);
            }
        }
        (r.0[1], r.0[2], r.0[3], r.0[4], r.0[5], r.0[6], r.0[7]) = (RACE_CARS + 4 * n.max(0) as u32, n.max(0) as u32, best, best_d as u32, this.0, old, HIGH_LEVEL_AI_OBJS);
        if best != 0 && best != old {
            r.enter(c, 0x8005_d39c, 0x8005_d5b0, |c| cop_assign_to_player(c, this, AIHighBTCPerp(best)));
            return true;
        }
        if old != 0 && best == 0 {
            r.enter(c, 0x8005_d39c, 0x8005_d5d0, |c| cop_assign_to_player(c, this, AIHighBTCPerp(0)));
        }
        false
    })
}

pub fn abi_cop_check_for_new_target(c: &mut Cpu) {
    c.r[2] = cop_check_for_new_target(c, AIHighBTCCop(c.r[4])) as u32;
}

/// `AIHigh_BTC_Cop::StartArrest` (AIH_BTCCOP.CPP, 0x8005d600): unless frozen, arresting; a human
/// cop's radio says so.
pub fn cop_start_arrest(c: &mut Cpu, this: AIHighBTCCop) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let m = &mut c.m;
        if this.freeze_mode_(m) != 0 {
            return;
        }
        this.set_freeze_mode_(m, FREEZE_ARRESTING);
        let car = this.ai_high_basic_cop().ai_high_base().car_obj_(m);
        if car.car_flags(m) & aihigh::CAR_HUMAN_COP != 0 {
            speech(c, &r, car, 9, 0x8005_d63c, 0x8005_d660, &[1]);
        }
    })
}

pub fn abi_cop_start_arrest(c: &mut Cpu) {
    cop_start_arrest(c, AIHighBTCCop(c.r[4]));
}

/// `AIHigh_BTC_Cop::FinishArrest` (AIH_BTCCOP.CPP, 0x8005d670): arrested (when arresting or
/// chasing), the chase ended (its virtual `FreezeAndEndChase`), the busted overlay off.
pub fn cop_finish_arrest(c: &mut Cpu, this: AIHighBTCCop) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let mode = this.freeze_mode_(&mut c.m);
        if mode != FREEZE_ARRESTING && mode != 0 {
            return;
        }
        this.set_freeze_mode_(&mut c.m, FREEZE_ARRESTED);
        let (o, f) = aihigh::virtual_fn(&mut c.m, this.0, 3);
        r.call_ptr(c, f, &[o], 0x8005_d6b8);
        r.enter(c, 0x8005_d6f4, 0x8005_d6c0, |c| cop_hud_off(c, this));
    })
}

pub fn abi_cop_finish_arrest(c: &mut Cpu) {
    cop_finish_arrest(c, AIHighBTCCop(c.r[4]));
}

/// `AIHigh_BTC_Cop::FalseArrest` (AIH_BTCCOP.CPP, 0x8005d6d0): `perp`, its target, wasn't the
/// one: a false arrest.
pub fn cop_false_arrest(m: &mut Mem, this: AIHighBTCCop, perp: AIHighBTCPerp) {
    if this.perp_target_(m) == perp {
        this.set_freeze_mode_(m, FREEZE_FALSE_ARREST);
    }
}

pub fn abi_cop_false_arrest(c: &mut Cpu) {
    cop_false_arrest(&mut c.m, AIHighBTCCop(c.r[4]), AIHighBTCPerp(c.r[5]));
}

/// `AIHigh_BTC_Cop::HudOff` (AIH_BTCCOP.CPP, 0x8005d6f4): the first police car's busted
/// overlay off.
pub fn cop_hud_off(c: &mut Cpu, this: AIHighBTCCop) {
    abi::frame(c, 24, |c, _| {
        if this.ai_high_basic_cop().cop_index_(&mut c.m) == 0 {
            Saved::of(c).call(c, nfs4rc::f_800da3d0, &[], 0x8005_d714); // Hud_BustedOverlayOff
        }
    })
}

pub fn abi_cop_hud_off(c: &mut Cpu) {
    cop_hud_off(c, AIHighBTCCop(c.r[4]));
}

/// Half the time 1, else -1 (`fastRandom` per mille under 500).
fn random_sign(m: &mut Mem) -> i32 {
    if aih_basiccop::per_mille(m) < 500 { 1 } else { -1 }
}

/// 3 in 4, and not with a human speeder: whether a stage starts moving.
fn random_moving(m: &mut Mem) -> u32 {
    if aih_basiccop::per_mille(m) < 750 { (COP_GAME_TYPE.get(m) != 4) as u32 } else { 0 }
}

/// `AIHigh_BTC_HumanCop::AIHigh_BTC_HumanCop(Car_tObj*, int)` (AIH_BTCCOP.CPP, 0x8005d724):
/// before the first stage, the time of the skill, its car at rest on the first slice; the
/// first human cop's first stage then begins, on a random stretch free of barriers, either
/// way, moving (3 in 4, or on a sharp bend) unless against a human speeder.
pub fn human_cop_construct(c: &mut Cpu, this: AIHighBTCHumanCop, car: Car, index: i32) -> AIHighBTCHumanCop {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (index as u32, this.0);
        let cop = this.ai_high_btc_cop();
        r.enter(c, 0x8005_d35c, 0x8005_d744, |c| cop_construct(c, cop, car, index));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BTC_HUMAN_COP);
        this.set_initial_movement_(m, 1);
        this.set_current_stage_(m, -1);
        this.set_stage_time_multiplier_(m, 0x1_0000);
        this.set_need_perp_(m, 0);
        this.set_initial_direction_(m, 0);
        this.set_requested_desired_speed_(m, 0);
        this.set_stage_repeat_count_(m, 0);
        let time = tune(m, 12);
        this.set_time_left_(m, time);
        let me = car_of(m, this);
        me.set_unlap(m, 0);
        m.w32(sp + 16, 0);
        m.w32(sp + 20, 0);
        LAST_REQUEST_TIME.set(m, 0);
        r.enter(c, 0x8006_8450, 0x8005_d7c8, |c| crate::ailife::place_car_at_location_at(c, me, 0, 0, 1, 0, 0));
        if index != 0 {
            return this;
        }
        let m = &mut c.m;
        let dir = random_sign(m);
        let mut moving = random_moving(m);
        (r.0[0], r.0[2]) = (dir as u32, moving);
        r.enter(c, 0x8005_d940, 0x8005_d8a8, |c| find_random_barrier_free_area(c, 0, 100, 500));
        let m = &mut c.m;
        if calc_road_bend(m, me, 3).wrapping_abs() >= 24249 && COP_GAME_TYPE.get(m) != 4 {
            moving = 1;
            r.0[2] = 1;
        }
        let slice = me.n().sim_road_info().slice(m) as i16 as i32;
        r.enter(c, 0x8005_de40, 0x8005_d924, |c| human_cop_new_stage(c, this, slice, dir, moving as i32));
        this
    })
}

pub fn abi_human_cop_construct(c: &mut Cpu) {
    c.r[2] = human_cop_construct(c, AIHighBTCHumanCop(c.r[4]), Car(c.r[5]), c.r[6] as i32).0;
}

fn car_of(m: &mut Mem, this: AIHighBTCHumanCop) -> Car {
    this.ai_high_btc_cop().ai_high_basic_cop().ai_high_base().car_obj_(m)
}

/// `AIHigh_BTC_HumanCop::FindRandomBarrierFreeArea` (AIH_BTCCOP.CPP, 0x8005d940): from a random
/// slice within `spread` of `slice`, stepping a fifth of `range` at a time (30 tries), the
/// first whose `range` slices either side have lanes 6 and 7 drivable.
pub fn find_random_barrier_free_area(c: &mut Cpu, slice: i32, range: i32, spread: i32) -> i32 {
    let m = &mut c.m;
    let t = fast_random_step(m);
    let r = (t >> 8 & 0xFFFF) << 1;
    let off = ((spread.wrapping_mul(r as i32) as u32) >> 16) as i32;
    let off = off.wrapping_sub(spread);
    let mut s = slice_plus(m, slice, off);
    let step = range / 5;
    for _ in 0..30 {
        let from = slice_plus(m, s, range.wrapping_neg());
        let six = is_driveable_lane_in_slice_range(m, from, range << 1, 1, 6);
        let seven = is_driveable_lane_in_slice_range(m, from, range << 1, 1, 7);
        if six && seven {
            return s;
        }
        s = slice_plus(m, s, step);
    }
    s
}

pub fn abi_find_random_barrier_free_area(c: &mut Cpu) {
    c.r[2] = find_random_barrier_free_area(c, c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// `AIHigh_BTC_HumanCop::ReleaseAndStartChase` (AIH_BTCCOP.CPP, 0x8005db10): on duty, a target
/// looked for; the chase on when it is `perp`; the traffic let live again.
pub fn human_cop_release_and_start_chase(c: &mut Cpu, this: AIHighBTCHumanCop, perp: AIHighBTCPerp) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (perp.0, this.0);
        let m = &mut c.m;
        let car = car_of(m, this);
        car.update_ai_flags(m, |f| f | AI_ON_DUTY);
        let cop = this.ai_high_btc_cop();
        r.enter(c, 0x8005_d4a8, 0x8005_db40, |c| cop_check_for_new_target(c, cop));
        let m = &mut c.m;
        if cop.perp_target_(m) == perp {
            cop.set_freeze_mode_(m, FREEZE_FALSE_ARREST);
            let now = SIM_COUNTER.get(m);
            this.set_chase_start_time_(m, now);
        }
        reset_clear_traffic_to_purgatory(m);
    })
}

pub fn abi_human_cop_release_and_start_chase(c: &mut Cpu) {
    human_cop_release_and_start_chase(c, AIHighBTCHumanCop(c.r[4]), AIHighBTCPerp(c.r[5]));
}

/// `AIHigh_BTC_HumanCop::FreezeAndEndChase` (AIH_BTCCOP.CPP, 0x8005db80, virtual): the first
/// human cop's next stage, 7 slices on (either way by the stage) from a random stretch free
/// of barriers 100 on, either way, moving (3 in 4, or on a sharp curve) unless against a
/// human speeder.
pub fn human_cop_freeze_and_end_chase(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this.0;
        let m = &mut c.m;
        if this.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) != 0 {
            return;
        }
        let dir = random_sign(m);
        let mut moving = random_moving(m);
        let car = car_of(m, this);
        let slice = car.n().sim_road_info().slice(m) as i16 as i32;
        let curve = AIDataRecord(AIDATARECORD_TRACK_CURVE.get(m));
        if crate::aidatarecord::track_curve_get(m, curve, slice) >= 66 && COP_GAME_TYPE.get(m) != 4 {
            moving = 1;
        }
        (r.0[0], r.0[1]) = (dir as u32, moving);
        let at = r.enter(c, 0x8005_d940, 0x8005_dcb8, |c| find_random_barrier_free_area(c, slice, 100, 1));
        let m = &mut c.m;
        let by = if this.current_stage_(m).wrapping_add(1) & 1 != 0 { dir.wrapping_sub(dir << 3) } else { (dir << 3).wrapping_sub(dir) };
        let at = slice_plus(m, at, by);
        car_of(m, this).n().sim_road_info().set_slice(m, at as i16);
        r.enter(c, 0x8005_de40, 0x8005_dd3c, |c| human_cop_new_stage(c, this, at, dir, moving as i32));
    })
}

pub fn abi_human_cop_freeze_and_end_chase(c: &mut Cpu) {
    human_cop_freeze_and_end_chase(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::CheckConditionWithCop0` (AIH_BTCCOP.CPP, 0x8005dd54): the second
/// human cop follows the first's stages, starting 14 slices from it (either way by the
/// stage), and drives at its wanted speed.
pub fn human_cop_check_condition_with_cop0(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let m = &mut c.m;
        if this.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) == 0 {
            return;
        }
        let first = AIHighBTCHumanCop(m.r32(HIGH_LEVEL_AI_OBJS));
        r.0[0] = first.0;
        let stage = this.current_stage_(m);
        if stage < first.current_stage_(m) {
            let (dir, moving) = (first.initial_direction_(m), first.initial_movement_(m));
            let slice = car_of(m, first).n().sim_road_info().slice(m) as i16 as i32;
            let by = dir.wrapping_mul(14);
            let by = if stage.wrapping_add(1) & 1 != 0 { by } else { by.wrapping_neg() };
            let at = slice_plus(m, slice, by);
            r.enter_leaving(c, 0x8005_de40, 0x8005_de18, |c| human_cop_new_stage(c, this, at, dir, moving));
        }
        let m = &mut c.m;
        let speed = car_of(m, first).desired_speed(m);
        car_of(m, this).set_desired_speed(m, speed);
    })
}

pub fn abi_human_cop_check_condition_with_cop0(c: &mut Cpu) {
    human_cop_check_condition_with_cop0(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::NewStage` (AIH_BTCCOP.CPP, 0x8005de40): the next stage (one more
/// arrest counted), or the race's end after the last speeder; the spike belt, blockade and
/// custom objects cleared; the car put on `slice` facing `dir`: moving at 50 mph, or at rest
/// at the road's edge on the driving side (with a human speeder, at rest in a barrier-free
/// lane); the stage's time added (the first human cop's shown); frozen until released.
pub fn human_cop_new_stage(c: &mut Cpu, this: AIHighBTCHumanCop, slice: i32, dir: i32, moving: i32) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3], r.0[4]) = (this.0, slice as u32, dir as u32);
        let m = &mut c.m;
        this.update_current_stage_(m, |s| s.wrapping_add(1));
        GAME_SETUP.update_perp_arrests(m, |n| n.wrapping_add(1));
        let kind = COP_GAME_TYPE.get(m);
        r.0[0] = moving as u32;
        let over = if kind == 2 || kind == 3 { this.current_stage_(m) >= GAME_SETUP.num_perps(m) } else { kind == 4 && this.current_stage_(m) >= 3 };
        if over {
            SIM_END_GAME.set(m, 1);
            // (a3 still the argument, for a later call that homes it)
            c.r[7] = moving as u32;
            return;
        }
        r.call(c, nfs4rc::f_8007e1c0, &[], 0x8005_def4); // BWorld_InitSpikeBelt
        SPIKE_BELT.set_active_(&mut c.m, 0);
        r.call(c, nfs4rc::f_800a61e4, &[], 0x8005_df00); // Object_ClearCustomObjects
        let m = &mut c.m;
        this.set_initial_direction_(m, dir);
        this.set_initial_movement_(m, moving);
        let car = car_of(m, this);
        car.n().sim_road_info().set_slice(m, slice as i16);
        car.set_direction(m, dir);
        ROAD_BLOCK_STATE.set(m, 0);
        m.w32(sp + 28, 0);
        m.w32(sp + 24, 0);
        r.enter(c, 0x8007_4258, 0x8005_df38, |c| find_barrier_less_lane_and_position(&mut c.m, car, sp + 24, sp + 28));
        let m = &mut c.m;
        let (lat, speed, yaw);
        if COP_GAME_TYPE.get(m) == 4 {
            (lat, speed, yaw) = (m.r32(sp + 28) as i32, 0, 0);
        } else if moving != 0 {
            (lat, speed, yaw) = (m.r32(sp + 28) as i32, dir.wrapping_mul(0xD_5555), 0);
        } else {
            let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
            r.0[5] = (slice as u32) << 5;
            let b29 = m.r8(s + 29) as i32;
            let right = ((m.r8(s + 31) as i32) << 15).wrapping_mul(b29 & 15);
            let left_lanes = b29 >> 4;
            let left = ((m.r8(s + 30) as i32) << 15).wrapping_mul(left_lanes);
            let mut right = right.min(((m.r16(s + 26) as i16 as i32) << 8) - 0x8000);
            let mut left = left.min(((m.r16(s + 24) as i16 as i32) << 8) - 0x8000);
            if is_driveable_lane(m, slice, 6 - left_lanes) == 0 {
                left = left.wrapping_sub(0x2_0000);
            }
            let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
            let b29 = m.r8(s + 29) as i32;
            if is_driveable_lane(m, slice, (b29 & 15) + 7) == 0 {
                right = right.wrapping_sub(0x2_0000);
            }
            (r.0[0], r.0[1]) = (left as u32, right as u32);
            let first = this.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) == 0;
            let side = AITUNE_DRIVE_SIDE.get(m);
            if (first && side == 1) || (!first && side == -1) {
                (lat, yaw) = (right, (dir << 8).wrapping_add(512));
            } else {
                (lat, yaw) = (left.wrapping_neg(), 512i32.wrapping_sub(dir << 8));
            }
            speed = 0;
        }
        m.w32(sp + 16, speed as u32);
        m.w32(sp + 20, yaw as u32);
        r.enter(c, 0x8006_8450, 0x8005_e0c8, |c| crate::ailife::place_car_at_location_at(c, car, slice, lat, dir, speed, yaw));
        let m = &mut c.m;
        let stage = this.current_stage_(m) as u32;
        let time = if COP_GAME_TYPE.get(m) == 4 { m.r32(STAGE_TIMES_1HC1HP.wrapping_add(stage.wrapping_mul(4))) as i32 } else { perp_info(m, this, 0x14) };
        let time = (time << 5).max(0);
        let time = time.wrapping_mul(this.stage_time_multiplier_(m)) / 0x1_0000;
        r.0[0] = time as u32;
        if this.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) == 0 {
            r.call(c, nfs4rc::f_800da4f8, &[(time << 1) as u32], 0x8005_e168); // Hud_BTC_BonusTime
        }
        let m = &mut c.m;
        this.set_need_perp_(m, 1);
        this.set_chase_start_time_(m, 0);
        this.set_wingman_status_(m, 0);
        this.ai_high_btc_cop().set_freeze_mode_(m, FREEZE_STAGE_START);
        this.update_time_left_(m, |t| t.wrapping_add(time));
        r.call_leaving(c, nfs4rc::f_800bc7ac, &[], 0x8005_e18c); // TrgSfx_RestartTrgSfx
    })
}

pub fn abi_human_cop_new_stage(c: &mut Cpu) {
    human_cop_new_stage(c, AIHighBTCHumanCop(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `AIHigh_BTC_HumanCop::UpdateAndCheckTimeLeft` (AIH_BTCCOP.CPP, 0x8005e1b0): the time runs
/// (not while arresting or frozen) and is shown with the speeder's name; out of time, the
/// radio says so once, and 5 s later the race ends with the busted overlay.
pub fn human_cop_update_and_check_time_left(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let cop = this.ai_high_btc_cop();
        let mode = cop.freeze_mode_(m);
        if mode != FREEZE_ARRESTING && mode != FREEZE_STAGE_START {
            let e = AI_ELAPSED_TIME.get(m);
            this.update_time_left_(m, |t| t.wrapping_sub(e));
        }
        let perp = cop.perp_target_(m);
        let me = car_of(m, this);
        let (name, time) = if perp.0 != 0 {
            let car = Car(m.r32(perp.0));
            (car.car_info(m).0.wrapping_add(92), this.time_left_(m) as u32)
        } else {
            (0, 0)
        };
        let manual = (me.rs_control(m) == 0) as u32;
        r.call(c, nfs4rc::f_800da1b4, &[name, time, manual], 0x8005_e234); // the HUD's chase time
        let m = &mut c.m;
        if this.time_left_(m) >= 0 {
            return;
        }
        if TIME_UP_SPEECH.get(m) != 0 {
            speech(c, &r, me, 7, 0x8005_e260, 0x8005_e27c, &[]);
            TIME_UP_SPEECH.set(&mut c.m, 0);
        }
        let m = &mut c.m;
        if this.time_left_(m) < -160 {
            SIM_END_GAME.set(m, 1);
            TIME_UP_SPEECH.set(m, 1);
            let perp = cop.perp_target_(m);
            r.enter(c, 0x8005_ea74, 0x8005_e2b4, |c| human_cop_hud_on(c, this, perp, 1, me));
        }
    })
}

pub fn abi_human_cop_update_and_check_time_left(c: &mut Cpu) {
    human_cop_update_and_check_time_left(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::UpdateFreezeModeAndPullOverMode` (AIH_BTCCOP.CPP, 0x8005e2c4): out of
/// time, the hand brake on and the car steered to stop; arresting, it pulls over and stops;
/// frozen at a stage's start, it waits (on duty again when released) in its start's
/// direction, pulled over when the stage starts at rest; otherwise driven by the player, on
/// duty if it was pulled over or road-side controlled.
pub fn human_cop_update_freeze_mode_and_pull_over_mode(m: &mut Mem, this: AIHighBTCHumanCop) {
    let car = car_of(m, this);
    if this.time_left_(m) < 0 {
        FORCE_HUMAN_HAND_BRAKE.set(m, 1);
        let dir = car.direction(m);
        car.set_rs_control(m, dir);
        this.set_requested_desired_speed_(m, 0);
        return;
    }
    let cop = this.ai_high_btc_cop();
    let mode = cop.freeze_mode_(m);
    if mode == FREEZE_ARRESTING {
        car.set_rs_control(m, 0);
        car.set_pull_over(m, 1);
        this.set_requested_desired_speed_(m, 0);
    } else if mode == FREEZE_STAGE_START {
        car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
        let dir = this.initial_direction_(m);
        car.set_rs_control(m, dir);
        let still = (this.initial_movement_(m) == 0) as i32;
        car.set_pull_over(m, still);
        if car.pull_over(m) != 0 {
            this.set_requested_desired_speed_(m, 0);
        }
    } else {
        if car.pull_over(m) == 1 || car.rs_control(m) != 0 {
            car.update_ai_flags(m, |f| f | AI_ON_DUTY);
        }
        this.set_requested_desired_speed_(m, 0);
        cop.set_freeze_mode_(m, 0);
        car.set_rs_control(m, 0);
        car.set_pull_over(m, 0);
    }
}

pub fn abi_human_cop_update_freeze_mode_and_pull_over_mode(c: &mut Cpu) {
    human_cop_update_freeze_mode_and_pull_over_mode(&mut c.m, AIHighBTCHumanCop(c.r[4]));
}

/// The speech's `Speech::Dispatch()` (the dispatcher's voice, or the stand-in), then its
/// virtual function `k`.
fn dispatch(c: &mut Cpu, r: &Saved, k: u32, ra_dispatch: u32, ra: u32) {
    let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], ra_dispatch);
    speech_call(c, r, d, k, ra, &[]);
}

/// `AIHigh_BTC_HumanCop::RequestWingman` (AIH_BTCCOP.CPP, 0x8005e3e8): unless arresting, the
/// radio asks; once the stage's wingman time has passed in the chase (and no wingman yet),
/// one is called; else the dispatcher refuses.
pub fn human_cop_request_wingman(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let m = &mut c.m;
        let mode = this.ai_high_btc_cop().freeze_mode_(m);
        if mode == FREEZE_ARRESTING || mode == FREEZE_ARRESTED {
            return;
        }
        let wait = (perp_info(m, this, 0x18) << 6).wrapping_add(tune(m, 16));
        r.0[0] = wait as u32;
        let car = car_of(m, this);
        speech(c, &r, car, 12, 0x8005_e468, 0x8005_e484, &[]);
        let m = &mut c.m;
        let start = this.chase_start_time_(m);
        if start != 0 && wait != 0 && wait < SIM_COUNTER.get(m).wrapping_sub(start) && this.wingman_status_(m) != 4 {
            this.set_wingman_status_(m, 1);
            return;
        }
        dispatch(c, &r, 3, 0x8005_e4d4, 0x8005_e4f0);
    })
}

pub fn abi_human_cop_request_wingman(c: &mut Cpu) {
    human_cop_request_wingman(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::RequestBlockader` (AIH_BTCCOP.CPP, 0x8005e504): unless arresting, the
/// radio asks for a spike belt or a blockade; once the stage's time for it has passed in the
/// chase (and no blockader yet), the dispatcher sends one; else refuses.
pub fn human_cop_request_blockader(c: &mut Cpu, this: AIHighBTCHumanCop, spike: bool) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this.0, spike as u32);
        let m = &mut c.m;
        let mode = this.ai_high_btc_cop().freeze_mode_(m);
        if mode == FREEZE_ARRESTING || mode == FREEZE_ARRESTED {
            return;
        }
        let car = car_of(m, this);
        let wait = if spike {
            speech(c, &r, car, 11, 0x8005_e544, 0x8005_e560, &[]);
            let m = &mut c.m;
            (perp_info(m, this, 0x1C) << 6).wrapping_add(tune(m, 24))
        } else {
            speech(c, &r, car, 10, 0x8005_e5bc, 0x8005_e5d8, &[]);
            let m = &mut c.m;
            (perp_info(m, this, 0x20) << 6).wrapping_add(tune(m, 20))
        };
        let m = &mut c.m;
        let start = this.chase_start_time_(m);
        if start != 0 && wait > 0 && wait < SIM_COUNTER.get(m).wrapping_sub(start) && this.wingman_status_(m) != 5 {
            dispatch(c, &r, 4, 0x8005_e66c, 0x8005_e688);
            this.set_wingman_status_(&mut c.m, if spike { 3 } else { 2 });
            return;
        }
        dispatch(c, &r, 3, 0x8005_e6ac, 0x8005_e6c8);
    })
}

pub fn abi_human_cop_request_blockader(c: &mut Cpu) {
    human_cop_request_blockader(c, AIHighBTCHumanCop(c.r[4]), c.r[5] != 0);
}

/// `AIHigh_BTC_HumanCop::CheckForWingmanRequest` (AIH_BTCCOP.CPP, 0x8005e6dc): a wingman called
/// (now on its way).
pub fn human_cop_check_for_wingman_request(m: &mut Mem, this: AIHighBTCHumanCop) -> bool {
    if this.wingman_status_(m) == 1 {
        this.set_wingman_status_(m, 4);
        return true;
    }
    false
}

pub fn abi_human_cop_check_for_wingman_request(c: &mut Cpu) {
    c.r[2] = human_cop_check_for_wingman_request(&mut c.m, AIHighBTCHumanCop(c.r[4])) as u32;
}

/// `AIHigh_BTC_HumanCop::CheckForBlockaderRequest` (AIH_BTCCOP.CPP, 0x8005e700): a blockade (2)
/// or spike belt (3, `spike` set) sent (now on its way).
pub fn human_cop_check_for_blockader_request(m: &mut Mem, this: AIHighBTCHumanCop, spike: u32) -> bool {
    m.w32(spike, 0);
    let s = this.wingman_status_(m);
    if s == 2 || s == 3 {
        if s == 3 {
            m.w32(spike, 1);
        }
        this.set_wingman_status_(m, 5);
        return true;
    }
    false
}

pub fn abi_human_cop_check_for_blockader_request(c: &mut Cpu) {
    c.r[2] = human_cop_check_for_blockader_request(&mut c.m, AIHighBTCHumanCop(c.r[4]), c.r[5]) as u32;
}

/// `AIHigh_BTC_HumanCop::UpdateWingmanRole` (AIH_BTCCOP.CPP, 0x8005e744): the wingman's new
/// role noted: none (0), wingman (1), blockader (2, 3).
pub fn human_cop_update_wingman_role(m: &mut Mem, this: AIHighBTCHumanCop, role: i32) {
    match role {
        0 => this.set_wingman_status_(m, 0),
        1 => this.set_wingman_status_(m, 4),
        2 | 3 => this.set_wingman_status_(m, 5),
        _ => {}
    }
}

pub fn abi_human_cop_update_wingman_role(c: &mut Cpu) {
    human_cop_update_wingman_role(&mut c.m, AIHighBTCHumanCop(c.r[4]), c.r[5] as i32);
}

/// The traffic's objects' `forcePurgatory_`.
fn traffic_to_purgatory(m: &mut Mem, v: u32) {
    for i in 0..NUM_TRAFFIC_CARS.get(m).max(0) as u32 {
        let car = Car(m.r32(TRAFFIC_CARS + 4 * i));
        let index = car.car_index(m);
        let o = obj(m, index);
        m.w32(o.wrapping_add(0x1C), v);
    }
}

/// `AIHigh_BTC_HumanCop::ClearTrafficToPurgatory` (AIH_BTCCOP.CPP, 0x8005e790): all the traffic
/// sent to purgatory (a stage's start).
pub fn clear_traffic_to_purgatory(m: &mut Mem) {
    traffic_to_purgatory(m, 1);
}

pub fn abi_clear_traffic_to_purgatory(c: &mut Cpu) {
    clear_traffic_to_purgatory(&mut c.m);
}

/// `AIHigh_BTC_HumanCop::ResetClearTrafficToPurgatory` (AIH_BTCCOP.CPP, 0x8005e7e8): the traffic
/// let live again.
pub fn reset_clear_traffic_to_purgatory(m: &mut Mem) {
    traffic_to_purgatory(m, 0);
}

pub fn abi_reset_clear_traffic_to_purgatory(c: &mut Cpu) {
    reset_clear_traffic_to_purgatory(&mut c.m);
}

/// `AIHigh_BTC_HumanCop::SetDesiredSpeed` (AIH_BTCCOP.CPP, 0x8005e83c): road-side controlled,
/// the speed asked for, up to its top speed.
pub fn human_cop_set_desired_speed(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let car = car_of(m, this);
        if car.rs_control(m) == 0 {
            return;
        }
        let top = r.enter(c, 0x8006_fbfc, 0x8005_e86c, |c| crate::aispeeds::calc_human_top_speed(c, car));
        let m = &mut c.m;
        let want = this.requested_desired_speed_(m);
        car_of(m, this).set_desired_speed(m, if want < top { want } else { top });
    })
}

pub fn abi_human_cop_set_desired_speed(c: &mut Cpu) {
    human_cop_set_desired_speed(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::HighExecute` (AIH_BTCCOP.CPP, 0x8005e89c, virtual): each AI tick: the
/// traffic held at the first one's stage start; facing the way its speeder drives; its speed,
/// the second cop's stages, the freeze, the time, the target and the place in its chase; with
/// AI speeders, the event button's calls (a wingman, a blockade, a spike belt; 10 s apart);
/// the car's state.
pub fn human_cop_high_execute(c: &mut Cpu, this: AIHighBTCHumanCop) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let cop = this.ai_high_btc_cop();
        if cop.ai_high_basic_cop().cop_index_(m) == 0 && cop.freeze_mode_(m) == FREEZE_STAGE_START {
            clear_traffic_to_purgatory(m);
            // (the registers it leaves; a later call homes a3)
            (c.r[6], c.r[7]) = (1, HIGH_LEVEL_AI_OBJS);
        }
        let perp = cop.perp_target_(m);
        if perp.0 != 0 && cop.freeze_mode_(m) != FREEZE_STAGE_START {
            let dir = Car(m.r32(perp.0)).direction(m);
            car_of(m, this).set_desired_direction(m, dir);
        }
        r.enter(c, 0x8005_e83c, 0x8005_e914, |c| human_cop_set_desired_speed(c, this));
        r.enter_leaving(c, 0x8005_dd54, 0x8005_e91c, |c| human_cop_check_condition_with_cop0(c, this));
        r.enter(c, 0x8005_e2c4, 0x8005_e924, |c| human_cop_update_freeze_mode_and_pull_over_mode(&mut c.m, this));
        r.enter(c, 0x8005_e1b0, 0x8005_e92c, |c| human_cop_update_and_check_time_left(c, this));
        r.enter(c, 0x8005_d4a8, 0x8005_e934, |c| cop_check_for_new_target(c, cop));
        if cop.perp_target_(&mut c.m).0 != 0 {
            r.enter(c, 0x8005_d40c, 0x8005_e94c, |c| cop_get_check_chase_position(c, cop, Coord(sp + 16)));
        }
        if COP_GAME_TYPE.get(&mut c.m) == 2 {
            r.0[1] = crate::globals::SIM_GLOBAL.0;
            let m = &mut c.m;
            let car = car_of(m, this);
            let event = car.0 + 0x44B; // control.queuedEvent
            if SIM_COUNTER.get(m).wrapping_sub(LAST_REQUEST_TIME.get(m)) < 641 {
                let e = m.r8(event);
                if (4..7).contains(&e) {
                    m.w8(event, 0);
                }
            } else {
                let calls: [(u8, u32, u32); 3] = [(4, 0x8005_e3e8, 0x8005_e9a0), (5, 0x8005_e504, 0x8005_e9d8), (6, 0x8005_e504, 0x8005_ea10)];
                for (e, addr, ra) in calls {
                    if c.m.r8(event) != e {
                        continue;
                    }
                    r.enter(c, addr, ra, |c| match e {
                        4 => human_cop_request_wingman(c, this),
                        _ => human_cop_request_blockader(c, this, e == 6),
                    });
                    let m = &mut c.m;
                    m.w8(event, 0);
                    let now = SIM_COUNTER.get(m);
                    LAST_REQUEST_TIME.set(m, now);
                }
            }
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8005_ea60, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_human_cop_high_execute(c: &mut Cpu) {
    human_cop_high_execute(c, AIHighBTCHumanCop(c.r[4]));
}

/// `AIHigh_BTC_HumanCop::HudOn` (AIH_BTCCOP.CPP, 0x8005ea74): the first human cop's busted
/// overlay (or another's when `out`, out of time): the chase's time, the speeder's name, the
/// human's car (a speeder's own car is the first human's).
pub fn human_cop_hud_on(c: &mut Cpu, this: AIHighBTCHumanCop, perp: AIHighBTCPerp, out: i32, car: Car) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let car = if car.car_flags(m) & 2 != 0 { Car(m.r32(HUMAN_RACE_CARS)) } else { car };
        if this.ai_high_btc_cop().ai_high_basic_cop().cop_index_(m) != 0 && out == 0 {
            return;
        }
        let time = SIM_COUNTER.get(m).wrapping_sub(this.chase_start_time_(m));
        let name = Car(m.r32(perp.0)).car_info(m).0.wrapping_add(92);
        let index = car.car_index(m) as i16 as i32;
        Saved::of(c).call(c, nfs4rc::f_800da218, &[time as u32, name, (out == 0) as u32, index as u32], 0x8005_ead0); // Hud_BustedOverlayOn
    })
}

pub fn abi_human_cop_hud_on(c: &mut Cpu) {
    human_cop_hud_on(c, AIHighBTCHumanCop(c.r[4]), AIHighBTCPerp(c.r[5]), c.r[6] as i32, Car(c.r[7]));
}

/// `AIHigh_BTC_Wingman::AIHigh_BTC_Wingman(Car_tObj*, int)` (AIH_BTCCOP.CPP, 0x8005eae0): no
/// role, no human cop, no spike belt.
pub fn wingman_construct(c: &mut Cpu, this: AIHighBTCWingman, car: Car, index: i32) -> AIHighBTCWingman {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8005_d35c, 0x8005_eaf4, |c| cop_construct(c, this.ai_high_btc_cop(), car, index));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BTC_WINGMAN);
        this.set_spike_belt_placed_(m, 0);
        this.set_spike_belt_slice_(m, 0);
        this.set_spike_belt_intercept_release_time_(m, 0);
        this.set_current_role_(m, 0);
        this.set_new_role_(m, 0);
        this.set_new_human_boss_(m, AIHighBTCHumanCop(0));
        this
    })
}

pub fn abi_wingman_construct(c: &mut Cpu) {
    c.r[2] = wingman_construct(c, AIHighBTCWingman(c.r[4]), Car(c.r[5]), c.r[6] as i32).0;
}


/// The states of this file (the inline classes' copies of their vtables): not active, the
/// base; and AISTATE.CPP's idle state.
const VT_STATE_NON_ACTIVE: u32 = 0x8005_56F4;
const VT_STATE_BASE2: u32 = 0x8005_5714;
const VT_STATE_IDLE: u32 = 0x8005_5DD0;

/// The state kinds (`stateType_`).
const STATE_NORMAL: i32 = 2;
const STATE_IDLE: i32 = 3;
const STATE_CHASE: i32 = 4;
const STATE_NON_ACTIVE: i32 = 7;
const STATE_GOTO_SLICE: i32 = 9;

/// The wingman's roles: none, a wingman, a blockader, a spike belt.
const ROLE_WINGMAN: i32 = 1;

/// The car of the speeder the police car chases.
fn perp_car(m: &mut Mem, cop: AIHighBTCCop) -> Car {
    let perp = cop.perp_target_(m).0;
    Car(m.r32(perp))
}

fn wingman_car(m: &mut Mem, this: AIHighBTCWingman) -> Car {
    this.ai_high_btc_cop().ai_high_basic_cop().ai_high_base().car_obj_(m)
}

/// A new idle state (in place) for the car.
fn new_idle(m: &mut Mem, car: Car) -> u32 {
    let s = crate::aistate::pool_new(m);
    crate::aistate::base_construct(m, AIStateBase(s), car);
    m.w32(s.wrapping_add(4), VT_STATE_IDLE);
    m.w32(s.wrapping_add(12), 1);
    s
}

/// A new normal state (`AIState_Normal`'s constructor entered from `ra`).
fn new_normal(c: &mut Cpu, r: &Saved, car: Car, ra: u32) -> u32 {
    let s = crate::aistate::pool_new(&mut c.m);
    r.enter(c, 0x8007_03bc, ra, |c| crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car));
    s
}

/// A new chase after `target` from where the wingman's place in the chase is (`rel`, on the
/// stack), its arguments past the fourth at sp+16.
fn new_chase(c: &mut Cpu, r: &Saved, sp: u32, car: Car, target: Car, rel: Coord, ra: u32) -> u32 {
    let s = crate::aistate::pool_new(&mut c.m);
    let args = [512, 0x3C_0000, 0x19_0000, 2, 0x1_0000];
    for (k, v) in args.iter().enumerate() {
        c.m.w32(sp + 16 + 4 * k as u32, *v as u32);
    }
    r.enter(c, 0x8007_04dc, ra, |c| crate::aistate::chase_construct(c, crate::types::AIStateChase(s), car, target, rel, args[0], args[1], args[2], args[3], args[4]));
    s
}

/// The wingman out of play: a not-active state, the car put on the road at the start (`at`
/// on the stack, its height by the car's index) and taken out of the race.
fn wingman_non_active(c: &mut Cpu, r: &mut Saved, this: AIHighBTCWingman, at: u32, ra: [u32; 3]) {
    let m = &mut c.m;
    let s = crate::aistate::pool_new(m);
    r.0[2] = s;
    let car = wingman_car(m, this);
    r.0[0] = car.0;
    crate::aistate::base_construct(m, AIStateBase(s), car);
    m.w32(s.wrapping_add(4), VT_STATE_NON_ACTIVE);
    r.call(c, nfs4rc::f_800e4f04, &[at, 0, 12], ra[0]); // memset
    let m = &mut c.m;
    let index = car.car_index(m);
    m.w32(at + 4, (index.wrapping_mul(5) << 17) as u32);
    let obj = crate::types::NewtonObj(m.r32(s));
    r.enter(c, 0x800a_22a8, ra[1], |c| crate::newton::set_initial_slice_position_orientation_etc(c, obj, 0, Coord(at), 1));
    let m = &mut c.m;
    Car(m.r32(s)).n().set_active(m, 0);
    aihigh::set_state(c, r, this.0, s, STATE_NON_ACTIVE, ra[2]);
}

/// When the wingman's arrest has finished: out of play, its role none.
fn wingman_end(c: &mut Cpu, r: &mut Saved, this: AIHighBTCWingman, at: u32, ra: [u32; 4]) {
    let cop = this.ai_high_btc_cop();
    r.enter(c, 0x8005_d39c, ra[0], |c| cop_assign_to_player(c, cop, AIHighBTCPerp(0)));
    wingman_non_active(c, r, this, at, [ra[1], ra[2], ra[3]]);
    this.set_new_role_(&mut c.m, 0);
    this.set_current_role_(&mut c.m, 0);
}

/// `AIHigh_BTC_Wingman::HighExecute` (AIH_BTCCOP.CPP, 0x8005eb2c, virtual): each AI tick, the
/// spike belt checked and a human cop's request taken; by its state: none, out of play;
/// driving, off to block (when out of sight) or chasing the closest speeder; idle at a
/// blockade, after the speeder once close (within 50 m, or arriving before the spike belt's
/// time) or back to driving; chasing, around barriers, harder when close behind, back to
/// driving with nobody to chase or when called to block; out of play, on its way as wingman
/// or blockader; going to a slice, driving once there. A finished arrest takes it out of
/// play. Then its state.
pub fn wingman_high_execute(c: &mut Cpu, this: AIHighBTCWingman) {
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let cop = this.ai_high_btc_cop();
        r.enter(c, 0x8005_c9dc, 0x8005_eb48, aih_basiccop::check_spike_belt);
        r.enter(c, 0x8005_f5b8, 0x8005_eb50, |c| wingman_check_for_activation(c, this));
        let m = &mut c.m;
        let car = wingman_car(m, this);
        let freeze = |c: &mut Cpu, r: &Saved, ra: u32| r.enter(c, 0x8005_f6e8, ra, |c| wingman_update_freeze_mode_and_pull_over_mode(&mut c.m, this));
        match m.r32(this.0 + 8) {
            0 => {
                car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                let s = crate::aistate::pool_new(m);
                r.0[2] = s;
                r.0[0] = car.0;
                crate::aistate::base_construct(m, AIStateBase(s), car);
                m.w32(s.wrapping_add(4), VT_STATE_NON_ACTIVE);
                r.call(c, nfs4rc::f_800e4f04, &[sp + 40, 0, 12], 0x8005_ebcc); // memset
                let m = &mut c.m;
                let index = car.car_index(m);
                m.w32(sp + 44, (index.wrapping_mul(5) << 17) as u32);
                let obj = crate::types::NewtonObj(m.r32(s));
                r.enter(c, 0x800a_22a8, 0x8005_ebf4, |c| crate::newton::set_initial_slice_position_orientation_etc(c, obj, 0, Coord(sp + 40), 1));
                let m = &mut c.m;
                Car(m.r32(s)).n().set_active(m, 0);
                aihigh::set_state(c, &r, this.0, s, STATE_NON_ACTIVE, 0x8005_ec2c);
            }
            2 => {
                car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                let new = this.new_role_(m);
                if new != this.current_role_(m) && (new == 2 || new == 3) {
                    if r.enter(c, 0x8006_8fec, 0x8005_ec7c, |c| crate::ailife::is_car_in_any_visible_area(c, car)) == 0 {
                        speech(c, &r, car, 16, 0x8005_ec90, 0x8005_ecac, &[]);
                        let m = &mut c.m;
                        let (boss, new) = (this.new_human_boss_(m), this.new_role_(m));
                        this.set_current_role_(m, new);
                        r.enter(c, 0x8005_f864, 0x8005_eccc, |c| wingman_setup_blockader(c, this, boss, new == 3));
                        let s = new_idle(&mut c.m, car);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8005_ed24);
                    }
                } else {
                    r.enter(c, 0x8005_d4a8, 0x8005_ed34, |c| cop_check_for_new_target(c, cop));
                    if cop.perp_target_(&mut c.m).0 != 0 {
                        r.enter(c, 0x8005_d40c, 0x8005_ed4c, |c| cop_get_check_chase_position(c, cop, Coord(sp + 40)));
                        let target = perp_car(&mut c.m, cop);
                        let s = new_chase(c, &r, sp, car, target, Coord(sp + 40), 0x8005_ed98);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_CHASE, 0x8005_edc4);
                    }
                }
                if freeze(c, &r, 0x8005_edd8) {
                    wingman_end(c, &mut r, this, sp + 40, [0x8005_ede8, 0x8005_ee20, 0x8005_f408, 0x8005_f440]);
                }
            }
            4 => {
                car.update_ai_flags(m, |f| f | AI_ON_DUTY);
                let chase = crate::types::AIStateChase(m.r32(this.0 + 4));
                r.0[2] = chase.0;
                r.enter(c, 0x8005_d218, 0x8005_ee5c, |c| aih_basiccop::handle_blockade_speech(c, cop.ai_high_basic_cop()));
                if r.enter(c, 0x8005_d40c, 0x8005_ee68, |c| cop_get_check_chase_position(c, cop, Coord(sp + 40))) {
                    let target = perp_car(&mut c.m, cop);
                    r.enter(c, 0x8007_066c, 0x8005_ee84, |c| crate::aistate::set_target(c, chase, target, Coord(sp + 40)));
                }
                if chase.barrier_ticks32_(&mut c.m) >= 161 {
                    let end = r.enter(c, 0x8007_15f4, 0x8005_eea0, |c| crate::aistate::find_barrier_end_slice(c, chase));
                    r.0[0] = end as u32;
                    let s = crate::aistate::pool_new(&mut c.m);
                    r.enter(c, 0x8007_2c4c, 0x8005_eec0, |c| crate::aistate::goto_slice_construct(c, crate::types::AIStateGotoSlice(s), car, end, 0));
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_GOTO_SLICE, 0x8005_eeec);
                }
                if r.enter(c, 0x8005_d4a8, 0x8005_ef00, |c| cop_check_for_new_target(c, cop)) {
                    r.0[0] = sp + 56;
                    r.enter(c, 0x8005_d40c, 0x8005_ef14, |c| cop_get_check_chase_position(c, cop, Coord(sp + 56)));
                    let target = perp_car(&mut c.m, cop);
                    r.enter(c, 0x8007_066c, 0x8005_ef28, |c| crate::aistate::set_target(c, chase, target, Coord(sp + 56)));
                }
                let m = &mut c.m;
                if chase.in_target_region_(m) > 8 && chase.lat_meters_between_(m).wrapping_abs() < 0xE_0000 && chase.long_meters_between_(m).wrapping_abs() < 0xF_0000 {
                    crate::aistate::set_murder_mode(m, chase, 1, 768);
                }
                if cop.perp_target_(&mut c.m).0 == 0 {
                    r.enter(c, 0x8005_d39c, 0x8005_efa4, |c| cop_assign_to_player(c, cop, AIHighBTCPerp(0)));
                    let s = new_normal(c, &r, car, 0x8005_efb8);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_efe4);
                }
                let m = &mut c.m;
                let new = this.new_role_(m);
                if new != this.current_role_(m) && (new == 2 || new == 3) {
                    car.update_desired_direction(m, |d| d.wrapping_neg());
                    r.enter(c, 0x8005_d39c, 0x8005_f02c, |c| cop_assign_to_player(c, cop, AIHighBTCPerp(0)));
                    let s = new_normal(c, &r, car, 0x8005_f040);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_f06c);
                    speech(c, &r, car, 7, 0x8005_f080, 0x8005_f09c, &[]);
                }
                if freeze(c, &r, 0x8005_f0a4) {
                    wingman_end(c, &mut r, this, sp + 72, [0x8005_f0b4, 0x8005_f0ec, 0x8005_f408, 0x8005_f440]);
                }
            }
            3 => {
                car.update_ai_flags(m, |f| f | AI_ON_DUTY);
                r.enter(c, 0x8005_d4a8, 0x8005_f128, |c| cop_check_for_new_target(c, cop));
                r.0[0] = 0;
                let m = &mut c.m;
                if cop.perp_target_(m).0 == 0 {
                    this.set_new_role_(m, ROLE_WINGMAN);
                    this.set_current_role_(m, ROLE_WINGMAN);
                    let s = new_normal(c, &r, car, 0x8005_f158);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_f184);
                    return;
                }
                r.enter(c, 0x8005_d40c, 0x8005_f1a0, |c| cop_get_check_chase_position(c, cop, Coord(sp + 40)));
                let m = &mut c.m;
                if this.spike_belt_placed_(m) != 0 && SPIKE_BELT.slice_(m) == this.spike_belt_slice_(m) {
                    let now = SIM_COUNTER.get(m);
                    SPIKE_BELT.set_freshen_time_(m, now);
                }
                let perp = cop.perp_target_(m).0;
                let target = Car(m.r32(perp));
                let d = apx_spline_distance(m, car, target);
                let far = d.wrapping_abs();
                let go = if far <= 0x31_FFFF {
                    true
                } else if far > 0x12B_FFFF {
                    false
                } else {
                    let speed = target.current_speed(m);
                    if speed.wrapping_abs() > 0x4_71C7 {
                        let t = crate::fixed::fixeddiv(d, speed);
                        t > 0 && t < this.spike_belt_intercept_release_time_(m)
                    } else {
                        false
                    }
                };
                if go {
                    r.0[0] = 1;
                    this.set_spike_belt_placed_(m, 0);
                    this.set_new_role_(m, ROLE_WINGMAN);
                    this.set_current_role_(m, ROLE_WINGMAN);
                    let s = new_chase(c, &r, sp, car, target, Coord(sp + 40), 0x8005_f2f0);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_CHASE, 0x8005_f31c);
                }
                let m = &mut c.m;
                let new = this.new_role_(m);
                if new != this.current_role_(m) && new == ROLE_WINGMAN {
                    this.set_current_role_(m, new);
                    let s = new_normal(c, &r, car, 0x8005_f358);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_f384);
                }
                if freeze(c, &r, 0x8005_f398) {
                    wingman_end(c, &mut r, this, sp + 56, [0x8005_f3a8, 0x8005_f3e0, 0x8005_f408, 0x8005_f440]);
                }
            }
            7 => {
                car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                r.0[2] = 1;
                let new = this.new_role_(m);
                if this.current_role_(m) != new {
                    if new == ROLE_WINGMAN {
                        this.set_current_role_(m, ROLE_WINGMAN);
                        let boss = this.new_human_boss_(m);
                        r.enter(c, 0x8005_f728, 0x8005_f49c, |c| wingman_setup_wingman(c, this, boss));
                        let s = new_normal(c, &r, car, 0x8005_f55c);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_f588);
                    } else if new == 2 || new == 3 {
                        let boss = this.new_human_boss_(m);
                        this.set_current_role_(m, new);
                        r.enter(c, 0x8005_f864, 0x8005_f4c8, |c| wingman_setup_blockader(c, this, boss, new == 3));
                        let s = new_idle(&mut c.m, car);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8005_f51c);
                    }
                }
            }
            9 => {
                let goto = crate::types::AIStateGotoSlice(m.r32(this.0 + 4));
                r.0[0] = goto.0;
                r.enter(c, 0x8005_d39c, 0x8005_f534, |c| cop_assign_to_player(c, cop, AIHighBTCPerp(0)));
                if r.enter(c, 0x8007_2db8, 0x8005_f540, |c| crate::aistate::in_target_slice_range(c, goto, 0xA_0000)) {
                    let s = new_normal(c, &r, car, 0x8005_f55c);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8005_f588);
                }
            }
            _ => {}
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8005_f5a0, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_wingman_high_execute(c: &mut Cpu) {
    wingman_high_execute(c, AIHighBTCWingman(c.r[4]));
}

/// `AIHigh_BTC_Wingman::CheckForActivation` (AIH_BTCCOP.CPP, 0x8005f5b8): each active human
/// cop's request for a wingman or a blockader (a spike belt) taken, that human cop its boss;
/// the boss told the role it had. Whether its role changes.
pub fn wingman_check_for_activation(c: &mut Cpu, this: AIHighBTCWingman) -> bool {
    abi::frame(c, 48, |c, sp| {
        let m = &mut c.m;
        m.w32(sp + 16, 0);
        for i in 0..crate::globals::NUM_CARS.get(m) {
            let car = Car(m.r32(crate::globals::CARS_LIST + 4 * i as u32));
            if car.car_flags(m) & aihigh::CAR_HUMAN_COP == 0 || car.n().active(m) == 0 {
                continue;
            }
            let index = car.car_index(m);
            let boss = AIHighBTCHumanCop(obj(m, index));
            if human_cop_check_for_wingman_request(m, boss) {
                this.set_new_role_(m, ROLE_WINGMAN);
                this.set_new_human_boss_(m, boss);
            }
            if human_cop_check_for_blockader_request(m, boss, sp + 16) {
                let role = if m.r32(sp + 16) != 0 { 3 } else { 2 };
                this.set_new_role_(m, role);
                this.set_new_human_boss_(m, boss);
            }
        }
        let boss = this.new_human_boss_(m);
        if boss.0 != 0 {
            let role = this.current_role_(m);
            human_cop_update_wingman_role(m, boss, role);
        }
        this.current_role_(m) != this.new_role_(m)
    })
}

pub fn abi_wingman_check_for_activation(c: &mut Cpu) {
    c.r[2] = wingman_check_for_activation(c, AIHighBTCWingman(c.r[4])) as u32;
}

/// `AIHigh_BTC_Wingman::UpdateFreezeModeAndPullOverMode` (AIH_BTCCOP.CPP, 0x8005f6e8): arresting,
/// it pulls over; else drives on, chasing again. Whether an arrest just finished.
pub fn wingman_update_freeze_mode_and_pull_over_mode(m: &mut Mem, this: AIHighBTCWingman) -> bool {
    let cop = this.ai_high_btc_cop();
    let car = wingman_car(m, this);
    if cop.freeze_mode_(m) == FREEZE_ARRESTING {
        car.set_pull_over(m, 1);
        return false;
    }
    car.set_pull_over(m, 0);
    let mode = cop.freeze_mode_(m);
    cop.set_freeze_mode_(m, 0);
    mode == FREEZE_ARRESTED
}

pub fn abi_wingman_update_freeze_mode_and_pull_over_mode(c: &mut Cpu) {
    c.r[2] = wingman_update_freeze_mode_and_pull_over_mode(&mut c.m, AIHighBTCWingman(c.r[4])) as u32;
}

/// `AIHigh_BTC_Wingman::SetupWingman` (AIH_BTCCOP.CPP, 0x8005f728): the wingman put 28 slices
/// before or after its boss (by chance), the way the boss drives and at its speed; the
/// dispatcher says it is coming.
pub fn wingman_setup_wingman(c: &mut Cpu, this: AIHighBTCWingman, boss: AIHighBTCHumanCop) {
    abi::frame(c, 32, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let car = car_of(m, boss);
        let dir: i32 = if car.current_speed(m) < 0 { -1 } else { 1 };
        let side = random_sign(m);
        let slice = car.n().sim_road_info().slice(m) as i16 as i32;
        let slice = slice_plus(m, slice, side.wrapping_mul(dir.wrapping_mul(28)));
        let speed = car.current_speed(m);
        m.w32(sp + 20, 0);
        m.w32(sp + 16, speed as u32);
        let me = wingman_car(m, this);
        r.enter(c, 0x8006_8450, 0x8005_f830, |c| crate::ailife::place_car_at_location_at(c, me, slice, 0, dir, speed, 0));
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8005_f838); // Speech::Dispatch
        speech_call(c, &r, d, 5, 0x8005_f854, &[me.0]);
    })
}

pub fn abi_wingman_setup_wingman(c: &mut Cpu) {
    wingman_setup_wingman(c, AIHighBTCWingman(c.r[4]), AIHighBTCHumanCop(c.r[5]));
}

/// `AIHigh_BTC_Wingman::SetupBlockader` (AIH_BTCCOP.CPP, 0x8005f864): a blockade ahead of its
/// boss's speeder (a sixth of the gap, 500 to 1500 m, on from whichever leads) or 83 slices
/// ahead of the boss; one of the blockade kinds by chance (its elements placed off the
/// scratchpad stack when on it); the blockader on the road there, across it; for a spike
/// belt, laid 6 slices on towards the boss across 80% of the lanes, its intercept time
/// random; the dispatcher says so.
pub fn wingman_setup_blockader(c: &mut Cpu, this: AIHighBTCWingman, boss: AIHighBTCHumanCop, spike: bool) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let b = this.ai_high_btc_cop().ai_high_basic_cop().blockade_();
        let boss_car = car_of(m, boss);
        (r.0[2], r.0[3], r.0[5], r.0[6]) = (this.0, boss_car.0, boss.0, spike as u32);
        let perp = boss.ai_high_btc_cop().perp_target_(m);
        let at;
        if perp.0 != 0 {
            let target = Car(m.r32(perp.0));
            let dir = if target.current_speed(m) < 0 { -1 } else { 1 };
            (r.0[1], r.0[4]) = (dir as u32, target.0);
            let d = apx_spline_distance(m, target, boss_car);
            let gap = d.wrapping_abs().max(0x1F4_0000).min(0x5DC_0000);
            let sixth = ((((gap as i64) * 0x2AAA_AAAB) >> 32) as i32 >> 16).wrapping_sub(gap >> 31);
            let by = sixth.wrapping_mul(dir);
            let from = if d.wrapping_mul(dir) >= 0 { target } else { boss_car };
            let slice = from.n().sim_road_info().slice(m) as i16 as i32;
            at = slice_plus(m, slice, by);
            b.set_blockade_speech_flags(m, 1);
            b.set_target(m, crate::types::AIHighPlayer(perp.0));
        } else {
            let dir: i32 = if boss_car.current_speed(m) < 0 { -1 } else { 1 };
            let slice = boss_car.n().sim_road_info().slice(m) as i16 as i32;
            at = slice_plus(m, slice, dir.wrapping_mul(83));
            b.set_blockade_speech_flags(m, 0);
            b.set_target(m, crate::types::AIHighPlayer(0));
        }
        r.0[0] = at as u32;
        let dir = boss_car.direction(m);
        b.set_direction(m, dir);
        let slice = slice_plus(m, at, dir.wrapping_neg());
        let t = fast_random_step(m);
        b.set_slice(m, slice);
        ROAD_BLOCK_STATE.set(m, 1);
        r.0[1] = 1;
        let kind = (t >> 8 & 0xFFFF) % 5;
        let flags = m.r8(BLOCKADE_TYPES + kind) as i32;
        b.set_flags(m, flags);
        if flags != 0 {
            if STACK_SPEED_UP_ENABLED.get(m) != 0 {
                // (SetSp: off the scratchpad stack and back)
                let swap_sp = |c: &mut Cpu| {
                    let sp = c.r[29];
                    c.r[29] = W_SAVE_PTR.get(&mut c.m);
                    W_SAVE_PTR.set(&mut c.m, sp);
                };
                swap_sp(c);
                STACK_SPEED_UP_ENABLED.set(&mut c.m, 0);
                r.enter(c, 0x8005_cf80, 0x8005_fb6c, |c| aih_basiccop::setup_blockade_elements(c, b));
                swap_sp(c);
                STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
            } else {
                r.enter(c, 0x8005_cf80, 0x8005_fb9c, |c| aih_basiccop::setup_blockade_elements(c, b));
            }
        }
        let m = &mut c.m;
        m.w32(sp + 16, 256);
        let me = wingman_car(m, this);
        let boss_dir = car_of(m, boss).direction(m);
        r.enter(c, 0x8006_8cd4, 0x8005_fbbc, |c| crate::ailife::reencarnate_cop_by_lat_pos_and_rotation(c, me, at, boss_dir, 0, 256));
        if spike {
            let m = &mut c.m;
            let d = apx_spline_distance(m, me, boss_car);
            let toward: i32 = if d < 0 { -1 } else { 1 };
            let slice = me.n().sim_road_info().slice(m) as i16 as i32;
            let belt_at = slice_plus(m, slice, toward.wrapping_mul(6));
            this.set_spike_belt_slice_(m, belt_at);
            let t = fast_random_step(m);
            let time = ((t >> 8 & 0xFFFF).wrapping_mul(85197) >> 16).wrapping_add(0xD999);
            this.set_spike_belt_intercept_release_time_(m, time as i32);
            let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((this.spike_belt_slice_(m) as u32) << 5);
            let b29 = m.r8(s + 29) as i32;
            let left = crate::fixed::fixedmult(((m.r8(s + 30) as i32) << 15).wrapping_mul(b29 >> 4), 0xCCCC);
            let right = crate::fixed::fixedmult(((m.r8(s + 31) as i32) << 15).wrapping_mul(b29 & 15), 0xCCCC);
            r.0[1] = left as u32;
            let belt = SPIKE_BELT;
            belt.set_left_lat_pos_(m, left.wrapping_neg());
            belt.set_right_lat_pos_(m, right);
            belt.set_active_(m, 1);
            let at = this.spike_belt_slice_(m);
            belt.set_slice_(m, at);
            let now = SIM_COUNTER.get(m);
            belt.set_freshen_time_(m, now);
            r.0[0] = 1;
            r.call(c, nfs4rc::f_8007e26c, &[at as u32, left.wrapping_neg() as u32, left.wrapping_add(right) as u32], 0x8005_fd64); // BWorld_SetSpikeBelt
            this.set_spike_belt_placed_(&mut c.m, 1);
        }
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8005_fd70); // Speech::Dispatch
        speech_call(c, &r, d, 5, 0x8005_fd8c, &[me.0]);
    })
}

pub fn abi_wingman_setup_blockader(c: &mut Cpu) {
    wingman_setup_blockader(c, AIHighBTCWingman(c.r[4]), AIHighBTCHumanCop(c.r[5]), c.r[6] != 0);
}

/// `AIHigh_BTC_Wingman::~AIHigh_BTC_Wingman` (AIHIGH.H, 0x8005fdb4), `AIHigh_BTC_HumanCop`'s
/// (0x8005fdd4) and `AIHigh_BTC_Cop`'s (0x8005fdf4).
pub fn abi_wingman_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8005_fdc4);
}

pub fn abi_human_cop_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8005_fde4);
}

pub fn abi_cop_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8005_fe04);
}

/// `AIState_NonActive::~AIState_NonActive` (AISTATE.H, 0x8005fe1c, this file's copy, its
/// vtables `vt` and the base's `base_vt`): its car active again; back to the pool when
/// `flags` asks.
pub fn non_active_destruct(m: &mut Mem, this: u32, flags: u32, vt: u32, base_vt: u32) {
    m.w32(this.wrapping_add(4), vt);
    let car = Car(m.r32(this));
    car.n().set_active(m, 1);
    m.w32(this.wrapping_add(4), base_vt);
    if flags & 1 != 0 {
        crate::aistate::pool_delete(m, this);
    }
}

pub fn abi_non_active_destruct(c: &mut Cpu) {
    non_active_destruct(&mut c.m, c.r[4], c.r[5], VT_STATE_NON_ACTIVE, VT_STATE_BASE2);
}

/// `AIState_Base::~AIState_Base` (AISTATE.H, 0x8005fe70, this file's copy).
pub fn abi_state_base_destruct(c: &mut Cpu) {
    aihigh::state_destruct(c, c.r[4], c.r[5], VT_STATE_BASE2);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::aihigh::tests::{ARGS, BTC_ARGS, HEAP_OBJS, OBJS, args, btc_shuffle};
    use crate::test_util::{Rng, against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    /// The "be the cop" arguments: a human cop, a wingman, a speeder (objects; 0 if none).
    fn btc(c: &mut Cpu) -> [u32; 3] {
        std::array::from_fn(|k| c.m.r32(BTC_ARGS + 4 * k as u32))
    }

    /// The "be the cop" shuffle with some traffic among the cars and the time-up speech either
    /// way.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        btc_shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = crate::globals::NUM_CARS.get(m) as u32;
        let k = pick(n + 1);
        NUM_TRAFFIC_CARS.set(m, k);
        for i in 0..k as u32 {
            let car = m.r32(crate::globals::CARS_LIST + 4 * pick(n) as u32);
            m.w32(TRAFFIC_CARS + 4 * i, car);
        }
        TIME_UP_SPEECH.set(m, pick(2));
        let t = SIM_COUNTER.get(m) - pick(1300);
        LAST_REQUEST_TIME.set(m, t);
        let car = Car(m.r32(BTC_ARGS));
        if car.0 > 0x8000_0000 {
            m.w8(car.0 + 0x44B, [0, 4, 5, 6, 7][pick(5) as usize]);
            car.set_rs_control(m, pick(3) - 1);
            car.set_pull_over(m, pick(2));
        }
    }

    /// A human cop's object, or the first object when there is none (a wingman's, a speeder's,
    /// likewise).
    fn or_obj(v: u32) -> u32 {
        if v != 0 { v } else { OBJS }
    }

    #[test]
    fn btccop_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7], [u32; 3]) -> Vec<u32>, Port); 39] = [
            ("CopConstruct", nfs4rc::f_8005d35c, |_, a, _| vec![OBJS + 0x700, a[0], a[2]], |c, a| Some(cop_construct(c, AIHighBTCCop(a[0]), Car(a[1]), a[2] as i32).0)),
            ("AssignToPlayer", nfs4rc::f_8005d39c, |_, _, b| vec![or_obj(b[1]), [0, b[2]][(b[2] != 0) as usize]], |c, a| {
                cop_assign_to_player(c, AIHighBTCCop(a[0]), AIHighBTCPerp(a[1]));
                None
            }),
            ("GetCheckChasePosition", nfs4rc::f_8005d40c, |c, a, b| {
                let o = or_obj(b[1]);
                if c.m.r32(o + 0x58) == 0 {
                    c.m.w32(o + 0x58, a[6]);
                }
                vec![o, ARGS + 0x60]
            }, |c, a| Some(cop_get_check_chase_position(c, AIHighBTCCop(a[0]), Coord(a[1])) as u32)),
            ("CheckForNewTarget", nfs4rc::f_8005d4a8, |_, _, b| vec![or_obj(b[(b[1] != 0) as usize])], |c, a| Some(cop_check_for_new_target(c, AIHighBTCCop(a[0])) as u32)),
            ("StartArrest", nfs4rc::f_8005d600, |_, _, b| vec![or_obj(b[0]), b[2]], |c, a| {
                cop_start_arrest(c, AIHighBTCCop(a[0]));
                None
            }),
            ("FinishArrest", nfs4rc::f_8005d670, |_, _, b| vec![or_obj(b[1]), b[2]], |c, a| {
                cop_finish_arrest(c, AIHighBTCCop(a[0]));
                None
            }),
            ("FalseArrest", nfs4rc::f_8005d6d0, |c, _, b| {
                let o = or_obj(b[1]);
                let p = c.m.r32(o + 0x58);
                vec![o, [p, b[2]][(b[2] & 4 == 0) as usize]]
            }, |c, a| {
                cop_false_arrest(&mut c.m, AIHighBTCCop(a[0]), AIHighBTCPerp(a[1]));
                None
            }),
            ("HudOff", nfs4rc::f_8005d6f4, |_, _, b| vec![or_obj(b[0])], |c, a| {
                cop_hud_off(c, AIHighBTCCop(a[0]));
                None
            }),
            ("HumanCopConstruct", nfs4rc::f_8005d724, |_, a, _| vec![OBJS + 0x700, a[0], a[4]], |c, a| Some(human_cop_construct(c, AIHighBTCHumanCop(a[0]), Car(a[1]), a[2] as i32).0)),
            ("FindRandomBarrierFreeArea", nfs4rc::f_8005d940, |c, a, b| vec![or_obj(b[0]), c.m.r32(ARGS + 0x4C), [100, 20, 3][a[5] as usize % 3], [500, 1, 50][a[3] as usize % 3]], |c, a| {
                Some(find_random_barrier_free_area(c, a[1] as i32, a[2] as i32, a[3] as i32) as u32)
            }),
            ("ReleaseAndStartChase", nfs4rc::f_8005db10, |_, _, b| vec![or_obj(b[0]), b[2]], |c, a| {
                human_cop_release_and_start_chase(c, AIHighBTCHumanCop(a[0]), AIHighBTCPerp(a[1]));
                None
            }),
            ("HumanCopFreezeAndEndChase", nfs4rc::f_8005db80, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_freeze_and_end_chase(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("CheckConditionWithCop0", nfs4rc::f_8005dd54, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_check_condition_with_cop0(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("NewStage", nfs4rc::f_8005de40, |c, a, b| vec![or_obj(b[0]), c.m.r32(ARGS + 0x4C), [1, -1i32 as u32][a[4] as usize], a[5] & 1], |c, a| {
                human_cop_new_stage(c, AIHighBTCHumanCop(a[0]), a[1] as i32, a[2] as i32, a[3] as i32);
                None
            }),
            ("UpdateAndCheckTimeLeft", nfs4rc::f_8005e1b0, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_update_and_check_time_left(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("HumanCopUpdateFreezeMode", nfs4rc::f_8005e2c4, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_update_freeze_mode_and_pull_over_mode(&mut c.m, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("RequestWingman", nfs4rc::f_8005e3e8, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_request_wingman(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("RequestBlockader", nfs4rc::f_8005e504, |_, a, b| vec![or_obj(b[0]), a[4]], |c, a| {
                human_cop_request_blockader(c, AIHighBTCHumanCop(a[0]), a[1] != 0);
                None
            }),
            ("CheckForWingmanRequest", nfs4rc::f_8005e6dc, |_, _, b| vec![or_obj(b[0])], |c, a| Some(human_cop_check_for_wingman_request(&mut c.m, AIHighBTCHumanCop(a[0])) as u32)),
            ("CheckForBlockaderRequest", nfs4rc::f_8005e700, |_, _, b| vec![or_obj(b[0]), ARGS + 0x6C], |c, a| {
                Some(human_cop_check_for_blockader_request(&mut c.m, AIHighBTCHumanCop(a[0]), a[1]) as u32)
            }),
            ("UpdateWingmanRole", nfs4rc::f_8005e744, |_, a, b| vec![or_obj(b[0]), a[5] - 1], |c, a| {
                human_cop_update_wingman_role(&mut c.m, AIHighBTCHumanCop(a[0]), a[1] as i32);
                None
            }),
            ("ClearTrafficToPurgatory", nfs4rc::f_8005e790, |_, _, b| vec![or_obj(b[0])], |c, _| {
                clear_traffic_to_purgatory(&mut c.m);
                None
            }),
            ("ResetClearTrafficToPurgatory", nfs4rc::f_8005e7e8, |_, _, b| vec![or_obj(b[0])], |c, _| {
                reset_clear_traffic_to_purgatory(&mut c.m);
                None
            }),
            ("SetDesiredSpeed", nfs4rc::f_8005e83c, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_set_desired_speed(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("HumanCopHighExecute", nfs4rc::f_8005e89c, |_, _, b| vec![or_obj(b[0])], |c, a| {
                human_cop_high_execute(c, AIHighBTCHumanCop(a[0]));
                None
            }),
            ("HudOn", nfs4rc::f_8005ea74, |_, a, b| vec![or_obj(b[0]), or_obj(b[2]), a[4], a[0]], |c, a| {
                human_cop_hud_on(c, AIHighBTCHumanCop(a[0]), AIHighBTCPerp(a[1]), a[2] as i32, Car(a[3]));
                None
            }),
            ("WingmanConstruct", nfs4rc::f_8005eae0, |_, a, _| vec![OBJS + 0x700, a[0], a[2]], |c, a| Some(wingman_construct(c, AIHighBTCWingman(a[0]), Car(a[1]), a[2] as i32).0)),
            ("WingmanHighExecute", nfs4rc::f_8005eb2c, |_, _, b| vec![or_obj(b[1])], |c, a| {
                wingman_high_execute(c, AIHighBTCWingman(a[0]));
                None
            }),
            ("WingmanCheckForActivation", nfs4rc::f_8005f5b8, |_, _, b| vec![or_obj(b[1])], |c, a| Some(wingman_check_for_activation(c, AIHighBTCWingman(a[0])) as u32)),
            ("WingmanUpdateFreezeMode", nfs4rc::f_8005f6e8, |_, _, b| vec![or_obj(b[1])], |c, a| {
                Some(wingman_update_freeze_mode_and_pull_over_mode(&mut c.m, AIHighBTCWingman(a[0])) as u32)
            }),
            ("SetupWingman", nfs4rc::f_8005f728, |_, _, b| vec![or_obj(b[1]), or_obj(b[0])], |c, a| {
                wingman_setup_wingman(c, AIHighBTCWingman(a[0]), AIHighBTCHumanCop(a[1]));
                None
            }),
            ("SetupBlockader", nfs4rc::f_8005f864, |_, a, b| vec![or_obj(b[1]), or_obj(b[0]), a[4]], |c, a| {
                wingman_setup_blockader(c, AIHighBTCWingman(a[0]), AIHighBTCHumanCop(a[1]), a[2] != 0);
                None
            }),
            ("WingmanDestruct", nfs4rc::f_8005fdb4, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_wingman_destruct(c);
                None
            }),
            ("HumanCopDestruct", nfs4rc::f_8005fdd4, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_human_cop_destruct(c);
                None
            }),
            ("CopDestruct", nfs4rc::f_8005fdf4, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_cop_destruct(c);
                None
            }),
            ("NonActiveDestruct", nfs4rc::f_8005fe1c, |c, a, b| vec![c.m.r32(or_obj(b[1]) + 4), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_non_active_destruct(c);
                None
            }),
            ("StateBaseDestruct", nfs4rc::f_8005fe70, |c, a, b| vec![c.m.r32(or_obj(b[1]) + 4), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_state_base_destruct(c);
                None
            }),
            ("TestForRelease", nfs4rc::f_8005fe68, |_, _, _| vec![], |_, _| Some(0)),
            ("CopFreezeAndEndChase", nfs4rc::f_8005d6ec, |_, _, _| vec![], |_, _| None),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3D10 + k as u64, shuffle, original, &|c| {
                let (a, b) = (args(c), btc(c));
                call(c, a, b)
            }, port);
        }
    }

    /// The stack the police's ticks leave (their states' frames, the blockade's objects).
    #[test]
    fn btccop_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 9] = [
            ("HumanCopHighExecute", 0x8005_e89c, nfs4rc::f_8005e89c),
            ("WingmanHighExecute", 0x8005_eb2c, nfs4rc::f_8005eb2c),
            ("HumanCopFreezeAndEndChase", 0x8005_db80, nfs4rc::f_8005db80),
            ("NewStage", 0x8005_de40, nfs4rc::f_8005de40),
            ("SetupBlockader", 0x8005_f864, nfs4rc::f_8005f864),
            ("SetupWingman", 0x8005_f728, nfs4rc::f_8005f728),
            ("HumanCopConstruct", 0x8005_d724, nfs4rc::f_8005d724),
            ("ReleaseAndStartChase", 0x8005_db10, nfs4rc::f_8005db10),
            ("RequestBlockader", 0x8005_e504, nfs4rc::f_8005e504),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 300, 0x3D50 + k as u64, shuffle, &|c, rng| {
                let (a, b) = (args(c), btc(c));
                match name {
                    "WingmanHighExecute" => vec![or_obj(b[1])],
                    "NewStage" => vec![or_obj(b[0]), c.m.r32(ARGS + 0x4C), [1, -1i32 as u32][a[4] as usize], a[5] & 1],
                    "SetupBlockader" => vec![or_obj(b[1]), or_obj(b[0]), (rng.next() & 1) as u32],
                    "SetupWingman" => vec![or_obj(b[1]), or_obj(b[0])],
                    "HumanCopConstruct" => vec![OBJS + 0x700, a[0], a[4]],
                    "ReleaseAndStartChase" => vec![or_obj(b[0]), b[2]],
                    "RequestBlockader" => vec![or_obj(b[0]), a[4]],
                    _ => vec![or_obj(b[0])],
                }
            });
        }
    }
}
