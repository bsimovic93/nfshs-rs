//! AI.CPP: how an AI car picks its lane each AI cycle. Each of the three lanes around it (left,
//! its own, right) gets merits (`CarLogic_gObs`): against oncoming lanes, slower cars ahead,
//! cars alongside, barriers and the road's edges, objects and spike belts, lanes ending; for the
//! best line and to block or follow a human (the AI script's reactions). The best lane gives the
//! position across the road it steers for and the speed it may keep behind the car in it.

use crate::abi::{self, Saved};
use crate::aiscript::{do_re_action, get_reaction_ticks_left, submit_player_action};
use crate::aiworld::{
    apx_spline_distance, apx_spline_distance_from_slice, apx_spline_distance_to_slice, calc_lateral_velocity, calculate_delta_road_yaw,
    game_odometer, get_profile_mask, is_driveable_lane_using_mask, lane_index, spline_distance,
};
use crate::fixed::fixedmult;
use crate::globals::{
    AIDATARECORD_BEST_LINE, AITUNE_DRIVE_SIDE, AITUNE_ONE_WAY, AI_ELAPSED_TIME, BWORLDSM_SLICES, CARS_LIST, GAME_SETUP, HUMAN_RACE_CARS,
    NUM_AI_RACE_CARS, NUM_CARS, NUM_COP_CARS, NUM_HUMAN_RACE_CARS, NUM_SLICES, NUM_TRAFFIC_CARS, SIM_COUNTER, SORTED_LIST, TOTAL_SORTED_LIST,
};
use crate::types::{Car, Coord, TrkNewSlice};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

/// `CarLogic_gObs`: the merits of the lanes left of the car, its own and right of it.
const OBS: u32 = 0x8010_D940;
/// `AI_Info` (`AI_tInfo`): per lane (left, own, right) the car blocking it and its distance,
/// the lane's speed (to keep behind it) and further ahead, the merits chosen from; the lane
/// chosen and which of the three it is; how much the road turns ahead.
const INFO: u32 = 0x8010_D94C;
const BLOCKING: u32 = INFO;
const BLOCKING_DIST: u32 = INFO + 0xC;
const LANE_SPEEDS: u32 = INFO + 0x18;
const LANE_SPEEDS_AHEAD: u32 = INFO + 0x24;
const LANE_WEIGHTS: u32 = INFO + 0x30;
const DESIRED_LANE: u32 = INFO + 0x3C;
const DESIRED_LANE_SIDE: u32 = INFO + 0x40;
const DELTA_YAW: u32 = INFO + 0x44;
/// `curveynessLevelStarts[2]`: a road this curvy is no place to block.
const CURVY: u32 = 0x8010_D938;
/// `Object_customObjInst`, `Object_customSimObjs` (`Group*`): the track's objects to avoid.
const CUSTOM_OBJ_INST: u32 = 0x8013_E5A4;
const CUSTOM_SIM_OBJS: u32 = 0x8013_E5A8;
/// A flag byte (gp+0): the object avoidance's slice position isn't set up yet; the position
/// (`spos`, a `BWorldSm_Pos`) it finds objects' slices with.
const SPOS_UNSET: u32 = 0x8013_D83C;
const SPOS: u32 = 0x8013_F1E8;

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

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

fn car_slice(m: &mut Mem, car: Car) -> i32 {
    car.n().sim_road_info().slice(m) as i32
}

fn lanes_of(m: &mut Mem, slice: i32) -> i32 {
    slice_at(m, slice).lane_count(m) as i32
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

fn obs(m: &mut Mem, k: u32) -> i32 {
    m.r32(OBS + 4 * k) as i32
}

fn add_obs(m: &mut Mem, k: u32, v: i32) {
    let o = obs(m, k).wrapping_add(v);
    m.w32(OBS + 4 * k, o as u32);
}

fn info(m: &mut Mem, at: u32, k: u32) -> i32 {
    m.r32(at + 4 * k) as i32
}

fn blocking(m: &mut Mem, k: u32) -> Car {
    Car(m.r32(BLOCKING + 4 * k))
}

/// Whether `lane` is the outermost lane of a slice with `lanes` (a lane each side of its
/// lanes: 6 minus the left lanes, 7 plus the right ones).
fn edge_lane(lanes: i32, lane: i32) -> bool {
    lane == 6 - (lanes >> 4) || lane == (lanes & 0xF) + 7
}

/// A pursuit race in which a human is a cop (the first human, or the second of two; the
/// original's test of the count reads `== 2`).
fn against_human_cop(m: &mut Mem) -> bool {
    let race = GAME_SETUP.race_type(m);
    if race != 1 && race != 5 {
        return false;
    }
    let human = |m: &mut Mem, i: u32| Car(m.r32(HUMAN_RACE_CARS + 4 * i));
    human(m, 0).car_flags(m) & CAR_HUMAN_COP != 0 || (NUM_HUMAN_RACE_CARS.get(m) == 2 && human(m, 1).car_flags(m) & CAR_HUMAN_COP != 0)
}

/// One step of the game's quick random numbers: the product (`randtemp`).
fn fast_random_step(m: &mut Mem) -> u32 {
    let t = crate::globals::FAST_RANDOM.get(m).wrapping_mul(crate::globals::RAND_SEED.get(m));
    crate::globals::RAND_TEMP.set(m, t);
    crate::globals::FAST_RANDOM.set(m, t & 0xFFFF);
    t
}

/// `x / 6` as the compiler divides by 6, shifted down `shift` bits before its sign is taken off.
fn div6(x: i32, shift: u32) -> i32 {
    let hi = ((x as i64 * 0x2AAA_AAAB) >> 32) as i32;
    (hi >> shift).wrapping_sub(x >> 31)
}

/// `AI_StartUp`, `AI_CleanUp`, `AI_Main_OverallSetup` (AI.CPP, 0x8005816c, 0x80058174,
/// 0x8005817c): nothing.
pub fn abi_nothing(_: &mut Cpu) {}

/// `AI_CarAvoidance` (AI.CPP, 0x80058184): the lanes' merits by the other cars.
pub fn car_avoidance(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8005_8e84, 0x8005_8198, |c| setup_oncoming_lane_demerits(c, car));
        r.enter(c, 0x8005_9510, 0x8005_81a0, |c| calculate_lane_speeds(c, car));
        r.enter(c, 0x8005_98a4, 0x8005_81a8, |c| calc_merits_based_on_speed(c, car));
        r.enter(c, 0x8005_9c24, 0x8005_81b0, |c| check_for_clear_lanes(c, car));
        r.enter(c, 0x8005_a47c, 0x8005_81b8, |c| check_for_cars_on_side(c, car));
    })
}

pub fn abi_car_avoidance(c: &mut Cpu) {
    car_avoidance(c, Car(c.r[4]));
}

/// `AI_WorldAvoidance` (AI.CPP, 0x800581c8): the lanes' merits by the road and what is on it.
pub fn world_avoidance(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8005_8ac4, 0x8005_81dc, |c| check_for_barriers(c, car));
        r.enter(c, 0x8005_8fdc, 0x8005_81e4, |c| handle_change_in_num_lanes(c, car));
        handle_shoulders_and_off_road(&mut c.m, car);
        r.enter(c, 0x8005_9e94, 0x8005_81f4, |c| calc_best_line_merits(c, car));
        r.enter(c, 0x8005_a118, 0x8005_81fc, |c| avoid_objects(c, car));
        r.enter(c, 0x8005_a14c, 0x8005_8204, |c| avoid_spike_belt(c, car));
    })
}

pub fn abi_world_avoidance(c: &mut Cpu) {
    world_avoidance(c, Car(c.r[4]));
}

/// `AI_InitAIInfo` (AI.CPP, 0x80058214): no cars blocking, the road's turn ahead, no lane.
pub fn init_ai_info(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = INFO;
        let m = &mut c.m;
        for at in [BLOCKING + 8, BLOCKING + 4, BLOCKING, BLOCKING_DIST + 8, BLOCKING_DIST + 4, BLOCKING_DIST] {
            m.w32(at, 0);
        }
        let yaw = r.enter(c, 0x8007_3f38, 0x8005_8244, |c| calculate_delta_road_yaw(c, car));
        c.m.w32(DELTA_YAW, yaw as u32);
        c.m.w32(DESIRED_LANE, 0);
    })
}

pub fn abi_init_ai_info(c: &mut Cpu) {
    init_ai_info(c, Car(c.r[4]));
}

/// `AI_GenericBeginCycle` (AI.CPP, 0x8005825c).
pub fn generic_begin_cycle(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.enter(c, 0x8005_8214, 0x8005_826c, |c| init_ai_info(c, car));
        clear_lane_merits(&mut c.m);
    })
}

pub fn abi_generic_begin_cycle(c: &mut Cpu) {
    generic_begin_cycle(c, Car(c.r[4]));
}

/// `AI_GenericCycle` (AI.CPP, 0x80058284): the lanes' merits.
pub fn generic_cycle(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8005_b0b0, 0x8005_8298, |c| maybe_change_lane_slack(c, car));
        r.enter(c, 0x8005_8804, 0x8005_82a0, |c| do_reactions_and_behavior(c, car));
        r.enter(c, 0x8005_af14, 0x8005_82a8, |c| push_finished_cars_to_side(c, car));
        r.enter(c, 0x8005_af04, 0x8005_82b0, |_| keep_cars_in_lane());
        r.enter(c, 0x8005_a378, 0x8005_82b8, |c| handle_traffic_honking(c, car));
        r.enter(c, 0x8005_8184, 0x8005_82c0, |c| car_avoidance(c, car));
        r.enter(c, 0x8005_81c8, 0x8005_82c8, |c| world_avoidance(c, car));
    })
}

pub fn abi_generic_cycle(c: &mut Cpu) {
    generic_cycle(c, Car(c.r[4]));
}

/// `AI_GenericEndCycle` (AI.CPP, 0x800582d8): the lane, the position across the road and the
/// speed.
pub fn generic_end_cycle(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8005_a5cc, 0x8005_82ec, |c| process_observations_and_choose_lane(c, car));
        r.enter(c, 0x8005_a94c, 0x8005_82f4, |c| calculate_desired_lat_position(c, car));
        r.enter(c, 0x8005_ab80, 0x8005_82fc, |c| calculate_adjusted_desired_speed(c, car));
    })
}

pub fn abi_generic_end_cycle(c: &mut Cpu) {
    generic_end_cycle(c, Car(c.r[4]));
}

/// `AI_TargetLane` (AI.CPP, 0x8005830c): a merit for the side `lane` is on.
pub fn target_lane(m: &mut Mem, car: Car, lane: i32) {
    let own = car.lane_index(m);
    if own < lane {
        add_obs(m, 2, 0x5_0000);
    } else if lane < own {
        add_obs(m, 0, 0x5_0000);
    }
}

pub fn abi_target_lane(c: &mut Cpu) {
    target_lane(&mut c.m, Car(c.r[4]), c.r[5] as i32);
}

/// `AI_ClearLaneMerits` (AI.CPP, 0x8005835c).
pub fn clear_lane_merits(m: &mut Mem) {
    for k in [2, 1, 0] {
        m.w32(OBS + 4 * k, 0);
    }
}

pub fn abi_clear_lane_merits(c: &mut Cpu) {
    clear_lane_merits(&mut c.m);
}

/// `AI_DoReactions` (AI.CPP, 0x80058384): the AI script's reactions: the horn held at the
/// human, a pattern of honks by the tick, swerving to one side or the other near the human (by
/// the reaction's time left), following the human's lane when it is close ahead (not in a
/// pursuit).
pub fn do_reactions(c: &mut Cpu, car: Car) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        let script = car.script();
        let m = &mut c.m;
        let pursuit = |m: &mut Mem| GAME_SETUP.race_type(m) == 1;
        let horn = do_re_action(m, script, 8);
        if horn != -1 && !pursuit(m) {
            // (on: the original passes 1, what its register holds then)
            r.call(c, nfs4rc::f_80075018, &[car.0, 1], 0x8005_83e4); // AudioClc_SetHorn
        } else if car.control().horn(m) != 0 {
            r.call(c, nfs4rc::f_80075018, &[car.0, 0], 0x8005_83e4);
        }
        let m = &mut c.m;
        if do_re_action(m, script, 4096) != -1 && !pursuit(m) {
            let k = (SIM_COUNTER.get(m) & 0x18) >> 3;
            if k == 0 || k == 3 {
                r.0[0] = script.0;
            }
            let (a, b, d) = [(4, 16, 8), (2, 32, 12), (7, 40, 16), (1, 32, 0)][k as usize];
            r.call(c, nfs4rc::f_80075088, &[car.0, a, b, d], 0x8005_84a8); // AudioClc_HonkHorn
        }
        r.0[0] = script.0;
        let m = &mut c.m;
        let target = do_re_action(m, script, 512);
        if target != -1 {
            let other = Car(m.r32(CARS_LIST.wrapping_add((target as u32).wrapping_mul(4))));
            if 0xC_0000 < apx_spline_distance(m, car, other).wrapping_abs() {
                let side = if (get_reaction_ticks_left(m, script) / 32) & 1 != 0 { 0 } else { 2 };
                add_obs(m, side, 0x1E_0000);
            }
        }
        let m = &mut c.m;
        let follow = do_re_action(m, script, 32);
        if follow != -1 {
            let other = Car(m.r32(CARS_LIST.wrapping_add((follow as u32).wrapping_mul(4))));
            r.0[0] = other.0;
            let d = r.enter(c, 0x8007_3b04, 0x8005_8580, |c| spline_distance(c, car, other));
            let m = &mut c.m;
            if d.wrapping_mul(car.direction(m)) <= 0x3_FFFF {
                let lane = other.lane_index(m);
                target_lane(m, car, lane);
            }
        }
    })
}

pub fn abi_do_reactions(c: &mut Cpu) {
    do_reactions(c, Car(c.r[4]));
}

/// `AI_CheckForPlayerActions` (AI.CPP, 0x800585c4): what a human does to the car, for its
/// script (past the first 800 m): hit it (0), passed it fast (1), in its lane ahead (2) or
/// behind (3), swapped places with it ahead (4) or behind (5), honked at it in the day (6).
pub fn check_for_player_actions(c: &mut Cpu, car: Car, human: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        r.0[2] = human.0;
        let m = &mut c.m;
        let gap = apx_spline_distance(m, car, human).wrapping_abs();
        r.0[4] = gap as u32;
        if game_odometer(m, car) <= 0x31F_FFFF {
            return;
        }
        let script = car.script();
        let submit = |c: &mut Cpu, r: &Saved, action: i32, ra: u32| {
            let (index, now) = (human.car_index(&mut c.m), SIM_COUNTER.get(&mut c.m));
            r.enter(c, 0x8006_ff80, ra, |c| submit_player_action(&mut c.m, script, index, action, now));
        };
        let hit = human.n().collision();
        let now = SIM_COUNTER.get(m);
        if now.wrapping_sub(hit.last_time(m)) < 15 && hit.last_other_obj(m).0 != 0 && hit.last_other_obj(m).0 == car.0 {
            submit(c, &r, 0, 0x8005_8654);
        }
        let m = &mut c.m;
        let dir = car.direction(m);
        let close = gap <= 0xB_FFFF;
        if human.current_speed(m).wrapping_mul(dir).wrapping_add(0x28_0000) < car.current_speed(m).wrapping_mul(dir) && close {
            submit(c, &r, 1, 0x8005_86b0);
        }
        let m = &mut c.m;
        if close && human.lane_index(m) == car.lane_index(m) {
            let ahead = game_odometer(m, human);
            r.0[0] = ahead as u32;
            let action = if game_odometer(m, car) < ahead { 2 } else { 3 };
            submit(c, &r, action, 0x8005_8724);
        }
        let m = &mut c.m;
        if human.swap_car(m) == car {
            r.0[3] = crate::globals::SIM_GLOBAL.0;
            if SIM_COUNTER.get(m).wrapping_sub(car.swap_time(m)) < 15 {
                let mine = game_odometer(m, car);
                r.0[0] = mine as u32;
                let action = if game_odometer(m, human) < mine { 4 } else { 5 };
                submit(c, &r, action, 0x8005_879c);
            }
        }
        let m = &mut c.m;
        if gap <= 0x1D_FFFF && human.control().horn(m) != 0 && GAME_SETUP.time(m) == 0 {
            submit(c, &r, 6, 0x8005_87e4);
        }
    })
}

pub fn abi_check_for_player_actions(c: &mut Cpu) {
    check_for_player_actions(c, Car(c.r[4]), Car(c.r[5]));
}

/// `AI_DoReactionsAndBehavior` (AI.CPP, 0x80058804): the script's reactions, what each human
/// does, and an AI racer blocking them.
pub fn do_reactions_and_behavior(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = car.0;
        r.enter(c, 0x8005_8384, 0x8005_8824, |c| do_reactions(c, car));
        let mut i = 0;
        while i < NUM_CARS.get(&mut c.m) {
            r.0[1] = CARS_LIST + 4 * i as u32;
            r.0[2] = i as u32;
            let m = &mut c.m;
            let other = Car(m.r32(CARS_LIST + 4 * i as u32));
            r.0[0] = other.0;
            if other != car && other.n().active(m) != 0 && other.car_flags(m) & CAR_HUMAN != 0 {
                r.enter(c, 0x8005_85c4, 0x8005_8884, |c| check_for_player_actions(c, car, other));
                if car.car_flags(&mut c.m) & CAR_AI_RACER != 0 {
                    r.enter(c, 0x8005_88c8, 0x8005_88a0, |c| opponent_block_player(c, car, other));
                }
            }
            i += 1;
        }
    })
}

pub fn abi_do_reactions_and_behavior(c: &mut Cpu) {
    do_reactions_and_behavior(c, Car(c.r[4]));
}

/// `AI_OpponentBlockPlayer` (AI.CPP, 0x800588c8): an AI racer out of the mellow zone, on a
/// road that isn't too curvy, blocks a human: told to by its script, against the human's lane
/// while it is just ahead; else into it while the human is within the personality's blocking
/// range behind, on the road and faster than 20 m/s.
pub fn opponent_block_player(c: &mut Cpu, car: Car, human: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        r.0[2] = human.0;
        if r.enter(c, 0x8005_ae18, 0x8005_88f0, |c| is_mellow_zone(c, car, 0x190_0000)) {
            return;
        }
        let m = &mut c.m;
        if m.r32(DELTA_YAW) as i32 >= m.r32(CURVY) as i32 {
            return;
        }
        let slice = car_slice(m, human);
        r.0[3] = slice as u32;
        let d = r.enter(c, 0x8007_3b04, 0x8005_8920, |c| spline_distance(c, car, human));
        let m = &mut c.m;
        let d = d.wrapping_mul(car.direction(m));
        let toward = |m: &mut Mem, by: i32| {
            let (theirs, mine) = (human.lane_index(m), car.lane_index(m));
            let k = if theirs < mine { 0 } else if theirs == mine { 1 } else { 2 };
            add_obs(m, k, by);
        };
        if do_re_action(m, car.script(), 1024) != -1 && d >= 0 && d <= 0x31_FFFF {
            toward(m, -0x4_0000);
            return;
        }
        let p = car.personality(m);
        if d < -0x6_0000 || d >= p.block_max_distance(m) || d < p.block_min_distance(m) {
            return;
        }
        let (lanes, lane) = (lanes_of(m, slice), human.lane_index(m));
        if lane < 7 - (lanes >> 4) || (lanes & 0xF) + 6 < lane {
            return;
        }
        if human.current_speed(m).wrapping_mul(human.direction(m)) <= 0x14_0000 {
            return;
        }
        toward(m, 0x4_0000);
    })
}

pub fn abi_opponent_block_player(c: &mut Cpu) {
    opponent_block_player(c, Car(c.r[4]), Car(c.r[5]));
}

/// `AI_CheckForBarriers` (AI.CPP, 0x80058ac4): lanes that can't be driven here are bad; ahead
/// (a slice per 6 m at its speed), while the road's lanes change (and for 73 tick units after),
/// those that end before then are bad too.
pub fn check_for_barriers(c: &mut Cpu, car: Car) {
    abi::frame(c, 96, |c, sp| {
        let m = &mut c.m;
        for k in 0..3 {
            let v = m.r32(0x8005_5574 + 4 * k);
            m.w32(sp + 32 + 4 * k, v);
        }
        let lane = car.lane_index(m);
        for (k, l) in [(0u32, lane.wrapping_sub(1)), (1, lane), (2, lane.wrapping_add(1))] {
            m.w32(sp + 16 + 4 * k, get_profile_mask(l) as u32);
        }
        let dir = car.direction(m);
        m.w32(sp + 48, dir as u32);
        let here = car_slice(m, car);
        let ms = (car.current_speed(m) / 0x1_0000).wrapping_abs();
        let ahead = div6(ms << 18, 16);
        let near = slice_on(m, here, ahead.wrapping_mul(dir));
        let far = slice_on(m, here, ahead.wrapping_add(1).wrapping_mul(dir));
        let profile = slice_at(m, here).paved_profile(m);
        for k in 0..3 {
            let mask = m.r32(sp + 16 + 4 * k) as i32;
            if is_driveable_lane_using_mask(m, here, mask) == 0 {
                add_obs(m, k, -0xA_0000);
            }
        }
        if profile != slice_at(m, near).paved_profile(m) || profile != slice_at(m, far).paved_profile(m) {
            car.set_barrier_think_harder(m, 73);
        }
        let think = car.barrier_think_harder(m);
        if think <= 0 {
            return;
        }
        // (about one slice in 25)
        let step = ahead.wrapping_mul(2621) / 0x1_0000;
        let step = if step == 0 { 1 } else { step };
        let elapsed = AI_ELAPSED_TIME.get(m);
        car.set_barrier_think_harder(m, think.wrapping_sub(elapsed));
        let mut i = 0;
        while i < ahead {
            let dir = m.r32(sp + 48) as i32;
            let s = slice_on(m, here, i.wrapping_mul(dir));
            for k in 0..3 {
                if m.r32(sp + 32 + 4 * k) != 0 {
                    let mask = m.r32(sp + 16 + 4 * k) as i32;
                    if is_driveable_lane_using_mask(m, s, mask) == 0 {
                        m.w32(sp + 32 + 4 * k, 0);
                        add_obs(m, k, -0x28_0000);
                    }
                }
            }
            i = i.wrapping_add(step);
        }
    })
}

pub fn abi_check_for_barriers(c: &mut Cpu) {
    check_for_barriers(c, Car(c.r[4]));
}

/// `AI_SetupOncomingLaneDemerits` (AI.CPP, 0x80058e84): with traffic or police on a two-way
/// road (not in the mellow zone, not chasing), the lanes on the oncoming side are bad (worse
/// for traffic).
pub fn setup_oncoming_lane_demerits(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        if r.enter(c, 0x8005_ae18, 0x8005_8e9c, |c| is_mellow_zone(c, car, 0x3C_0000)) {
            return;
        }
        let m = &mut c.m;
        let bad = if car.car_flags(m) & CAR_TRAFFIC != 0 { -0xC_0000 } else { -0x4_0000 };
        if NUM_TRAFFIC_CARS.get(m) == 0 && NUM_COP_CARS.get(m) == 0 {
            return;
        }
        if AITUNE_ONE_WAY.get(m) != 0 || car.ai_flags(m) & AI_CHASING != 0 {
            return;
        }
        let lane = car.lane_index(m);
        if car.direction(m) == AITUNE_DRIVE_SIDE.get(m) {
            if lane.wrapping_sub(1) < 7 {
                add_obs(m, 0, bad);
            }
            if car.lane_index(m) < 7 {
                add_obs(m, 1, bad);
            }
        } else {
            if car.lane_index(m) >= 7 {
                add_obs(m, 1, bad);
            }
            if car.lane_index(m).wrapping_add(1) >= 7 {
                add_obs(m, 2, bad);
            }
        }
    })
}

pub fn abi_setup_oncoming_lane_demerits(c: &mut Cpu) {
    setup_oncoming_lane_demerits(c, Car(c.r[4]));
}

/// `AI_HandleChangeInNumLanes` (AI.CPP, 0x80058fdc): when the car's lane ends ahead (at least
/// 5 slices, by its speed) but is a lane here, the side toward the road's centre is better.
pub fn handle_change_in_num_lanes(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    let ahead = (fixedmult(car.current_speed(m).wrapping_abs(), 27306) / 0x1_0000).max(5);
    let here = car_slice(m, car);
    let dir = car.direction(m);
    let s = slice_on(m, here, ahead.wrapping_mul(dir));
    let lane = car.lane_index(m);
    let there = lanes_of(m, s);
    if lane >= 7 - (there >> 4) && lane <= (there & 0xF) + 6 {
        return;
    }
    let now = lanes_of(m, here);
    if lane < 7 - (now >> 4) || (now & 0xF) + 6 < lane {
        return;
    }
    add_obs(m, if lane < 7 { 2 } else { 0 }, 0x28_0000);
}

pub fn abi_handle_change_in_num_lanes(c: &mut Cpu) {
    handle_change_in_num_lanes(c, Car(c.r[4]));
}

/// `AI_HandleShouldersAndOffRoad` (AI.CPP, 0x8005914c): the shoulders (the lane outside the
/// road's lanes) are bad (traffic's less behind a slow racer, more behind others; much worse
/// where the car doesn't fit between it and the edge), off the road far worse.
pub fn handle_shoulders_and_off_road(m: &mut Mem, car: Car) {
    let mut shoulder = -0x4_E666;
    let here = car_slice(m, car);
    if car.car_flags(m) & CAR_TRAFFIC != 0 {
        let ahead = blocking(m, 1);
        shoulder = -0x26_0000;
        if ahead.0 != 0 && ahead.car_flags(m) & CAR_RACING != 0 && 0x8_E38E < ahead.speed(m) {
            shoulder = 0x5_0000;
        }
    }
    let s = slice_at(m, here);
    let half = car.n().dimension().x(m);
    let room_left = |m: &mut Mem| {
        let lanes = s.lane_count(m) as i32;
        ((s.left_drive(m) as i32) << 8).wrapping_sub(((s.avg_paved_width_lf(m) as i32) << 15).wrapping_mul(lanes >> 4))
    };
    let room_right = |m: &mut Mem| {
        let lanes = s.lane_count(m) as i32;
        ((s.right_drive(m) as i32) << 8).wrapping_sub(((s.avg_paved_width_rt(m) as i32) << 15).wrapping_mul(lanes & 0xF))
    };
    let lanes = s.lane_count(m) as i32;
    let lane = car.lane_index(m);
    if edge_lane(lanes, lane.wrapping_sub(1)) {
        add_obs(m, 0, shoulder);
        let o = obs(m, 0);
        if room_left(m) < half {
            m.w32(OBS, o.wrapping_add(-0x64_0000) as u32);
        }
    }
    let lanes = s.lane_count(m) as i32;
    let lane = car.lane_index(m);
    if edge_lane(lanes, lane.wrapping_add(1)) {
        add_obs(m, 2, shoulder);
        let o = obs(m, 2);
        if room_right(m) < half {
            m.w32(OBS + 8, o.wrapping_add(-0x64_0000) as u32);
        }
    }
    let lanes = s.lane_count(m) as i32;
    let lane = car.lane_index(m);
    if edge_lane(lanes, lane) {
        add_obs(m, 1, shoulder);
        let left = car.lane_index(m) < 7;
        let tight = if left { room_left(m) < half } else { room_right(m) < half };
        if tight {
            add_obs(m, 1, -0x64_0000);
        }
    }
    let off = |m: &mut Mem, lane: i32| {
        let lanes = s.lane_count(m) as i32;
        lane < 6 - (lanes >> 4) || (lanes & 0xF) + 7 < lane
    };
    let lane = car.lane_index(m);
    if off(m, lane.wrapping_sub(1)) {
        add_obs(m, 0, -0x3E8_0000);
    }
    let lane = car.lane_index(m);
    if off(m, lane) {
        add_obs(m, 1, -0x3E8_0000);
    }
    let lane = car.lane_index(m);
    if off(m, lane.wrapping_add(1)) {
        add_obs(m, 2, -0x3E8_0000);
    }
}

pub fn abi_handle_shoulders_and_off_road(c: &mut Cpu) {
    handle_shoulders_and_off_road(&mut c.m, Car(c.r[4]));
}

/// `AI_CalculateLaneSpeeds` (AI.CPP, 0x80059510): each lane's speed: the car's own wanted
/// speed, else that of the nearest car ahead in it (in the order down the road, within the
/// personality's look-ahead), plus or minus some for its distance (more when fast, less when
/// near); that car blocks the lane.
pub fn calculate_lane_speeds(c: &mut Cpu, car: Car) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        r.0[3] = INFO;
        let m = &mut c.m;
        let look = car.personality(m).oncoming_look_ahead(m);
        m.w32(sp + 20, look as u32);
        for k in 0..3 {
            let want = car.desired_speed(m) as u32;
            m.w32(LANE_SPEEDS_AHEAD + 4 * k, want);
            m.w32(LANE_SPEEDS + 4 * k, want);
        }
        let lane = car.lane_index(m);
        let masks = [1u32.wrapping_shl(lane.wrapping_sub(1) as u32), 1u32.wrapping_shl(lane as u32), 1u32.wrapping_shl(lane.wrapping_add(1) as u32)];
        (r.0[8], r.0[7], r.0[6]) = (masks[0], masks[1], masks[2]);
        let order = car.sort_index(m);
        m.w32(sp + 16, order as u32);
        let (mut seen, mut found, mut i) = (0u32, 0, 0i32);
        while found != 3 && i < NUM_CARS.get(&mut c.m) {
            (r.0[1], r.0[4], r.0[5]) = (seen, found as u32, i as u32);
            let m = &mut c.m;
            let n = NUM_CARS.get(m);
            let at = order.wrapping_add(i.wrapping_mul(car.direction(m))).wrapping_add(n).wrapping_rem(n);
            let other = Car(m.r32(SORTED_LIST.wrapping_add((at as u32).wrapping_mul(4))));
            i += 1;
            if other == car || other.n().active(m) == 0 {
                continue;
            }
            r.0[0] = other.0;
            let d = r.enter(c, 0x8007_3b04, 0x8005_9650, |c| spline_distance(c, other, car));
            let m = &mut c.m;
            let ahead = d.wrapping_mul(car.direction(m));
            let fast = 0x16_38E2 < car.current_speed(m).wrapping_abs();
            let (mut per_m, mut per_m_ahead) = if fast { (0x1_4000, 0x8E38) } else { (0xD555, 0x6666) };
            if ahead <= 0xE_FFFF {
                (per_m, per_m_ahead) = (0x2AAA, 0x1999);
            }
            if ahead <= car.n().dimension().z(m) || ahead >= look {
                continue;
            }
            let racer = car.car_flags(m) & (CAR_AI_RACER | CAR_COP) != 0;
            let metres = d / 0x1_0000;
            let gap = if car.direction(m) == 1 {
                let t = metres - 2;
                if racer {
                    if t > 0 { t } else { 1 }
                } else if t >= 0 {
                    t
                } else {
                    0
                }
            } else {
                let mut t = metres + 2;
                if racer && t >= 2 {
                    t = -1;
                }
                if t <= 0 { t } else { 0 }
            };
            let speed = other.current_speed(m);
            let (s, s_ahead) = (speed.wrapping_add(gap.wrapping_mul(per_m)), speed.wrapping_add(gap.wrapping_mul(per_m_ahead)));
            let in_lane = other.car_in_lane(m) as u32;
            let mut take = |m: &mut Mem, k: u32, mask: u32| {
                if in_lane & mask != 0 && seen & mask == 0 {
                    seen |= mask;
                    found += 1;
                    m.w32(LANE_SPEEDS + 4 * k, s as u32);
                    m.w32(LANE_SPEEDS_AHEAD + 4 * k, s_ahead as u32);
                    m.w32(BLOCKING + 4 * k, other.0);
                    m.w32(BLOCKING_DIST + 4 * k, ahead as u32);
                }
            };
            take(m, 1, masks[1]);
            if (car.lane_index(m).wrapping_sub(1) as u32) < 14 {
                take(m, 0, masks[0]);
            }
            if (car.lane_index(m).wrapping_add(1) as u32) < 14 {
                take(m, 2, masks[2]);
            }
        }
    })
}

pub fn abi_calculate_lane_speeds(c: &mut Cpu) {
    calculate_lane_speeds(c, Car(c.r[4]));
}

/// `AI_CalcMeritsBasedOnSpeed` (AI.CPP, 0x800598a4): lanes blocked within 30 m are bad, and the
/// slower a lane is than the car's speed and its wanted speed (past 600 m, the wanted speed
/// only for the lane's speed when going forward), the worse.
pub fn calc_merits_based_on_speed(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    let (want, speed) = (car.desired_speed(m), car.current_speed(m));
    let started = 0x257_FFFF < game_odometer(m, car);
    for k in 0..3 {
        if blocking(m, k).0 != 0 && info(m, BLOCKING_DIST, k) <= 0x1D_FFFF {
            add_obs(m, k, -0xE_0000);
        }
    }
    let forward = car.direction(m) == 1;
    for k in 0..3 {
        let blocked = |m: &mut Mem| blocking(m, k).0 != 0;
        let merit = |m: &mut Mem, slower: i32, k_mult: i32| {
            let v = fixedmult(slower, k_mult);
            add_obs(m, k, v);
        };
        let lane_speed = info(m, LANE_SPEEDS, k);
        if forward {
            if speed >= lane_speed && blocked(m) {
                merit(m, speed.wrapping_sub(lane_speed), -0x1_4CCC);
            }
            let lane_speed = info(m, LANE_SPEEDS, k);
            if want >= lane_speed && blocked(m) && started {
                merit(m, want.wrapping_sub(lane_speed), -0x8000);
            }
            let ahead = info(m, LANE_SPEEDS_AHEAD, k);
            if speed >= ahead && blocked(m) {
                merit(m, speed.wrapping_sub(ahead), -0x8000);
            }
            let ahead = info(m, LANE_SPEEDS_AHEAD, k);
            if want >= ahead && blocked(m) && started {
                merit(m, want.wrapping_sub(ahead), -6553);
            }
        } else {
            if lane_speed >= speed && blocked(m) {
                merit(m, lane_speed.wrapping_sub(speed), -0x1_4CCC);
            }
            let lane_speed = info(m, LANE_SPEEDS, k);
            if lane_speed >= want && blocked(m) {
                merit(m, lane_speed.wrapping_sub(want), -0x8000);
            }
            let ahead = info(m, LANE_SPEEDS_AHEAD, k);
            if ahead >= speed && blocked(m) {
                merit(m, ahead.wrapping_sub(speed), -0x8000);
            }
            let ahead = info(m, LANE_SPEEDS_AHEAD, k);
            if ahead >= want && blocked(m) {
                merit(m, ahead.wrapping_sub(want), -6553);
            }
        }
    }
}

pub fn abi_calc_merits_based_on_speed(c: &mut Cpu) {
    calc_merits_based_on_speed(c, Car(c.r[4]));
}

/// `AI_CheckForClearLanes` (AI.CPP, 0x80059c24): an AI racer out of the mellow zone likes the
/// lanes with no car in them from 4 m behind to 106 m ahead (the cars down the road).
pub fn check_for_clear_lanes(c: &mut Cpu, car: Car) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = car.0;
        let m = &mut c.m;
        let order = car.sort_index(m);
        r.0[0] = order as u32;
        r.0[1] = 0;
        if car.car_flags(m) & CAR_AI_RACER == 0 {
            return;
        }
        if r.enter(c, 0x8005_ae18, 0x8005_9c70, |c| is_mellow_zone(c, car, 0)) {
            return;
        }
        let m = &mut c.m;
        for k in 0..3 {
            m.w32(sp + 16 + 4 * k, 0x4_0000);
        }
        let dir = car.direction(m);
        let (mut seen, mut found) = (0u32, 0);
        let mut i = dir;
        let mut at = dir.wrapping_mul(dir).wrapping_add(order);
        (r.0[6], r.0[7], r.0[8]) = (1, dir as u32, SORTED_LIST);
        loop {
            (r.0[1], r.0[2], r.0[4], r.0[5]) = (seen, found, i as u32, at as u32);
            let m = &mut c.m;
            let n = NUM_CARS.get(m);
            if i >= n {
                break;
            }
            let other = Car(m.r32(SORTED_LIST.wrapping_add((at.wrapping_add(n).wrapping_rem(n) as u32).wrapping_mul(4))));
            if other != car && other.n().active(m) != 0 {
                r.0[0] = other.0;
                let d = r.enter(c, 0x8007_3b04, 0x8005_9d24, |c| spline_distance(c, other, car));
                let m = &mut c.m;
                if (d.wrapping_add(0x4_0000) as u32) <= 0x6D_FFFF {
                    let in_lane = other.car_in_lane(m) as u32;
                    let mut clear = |m: &mut Mem, k: u32, lane: i32| {
                        if in_lane.wrapping_shr(lane as u32) & 1 != 0 && seen.wrapping_shr(lane as u32) & 1 == 0 {
                            m.w32(sp + 16 + 4 * k, 0);
                            found += 1;
                            seen |= 1u32.wrapping_shl(lane as u32);
                        }
                    };
                    let lane = car.lane_index(m);
                    clear(m, 1, lane);
                    let lane = car.lane_index(m);
                    if lane > 0 {
                        clear(m, 0, lane - 1);
                    }
                    let lane = car.lane_index(m).wrapping_add(1);
                    if (lane as u32) < 14 {
                        clear(m, 2, lane);
                    }
                }
            }
            at = at.wrapping_add(dir);
            i += 1;
            if found == 3 {
                break;
            }
        }
        let m = &mut c.m;
        for k in 0..3 {
            let v = m.r32(sp + 16 + 4 * k) as i32;
            add_obs(m, k, v);
        }
    })
}

pub fn abi_check_for_clear_lanes(c: &mut Cpu) {
    check_for_clear_lanes(c, Car(c.r[4]));
}

/// `AI_CalcBestLineMerits` (AI.CPP, 0x80059e94): an AI racer's preferred position across the
/// road: the track's best line where it looks ahead (by the personality's ability), less its
/// lane slack.
pub fn calc_best_line_merits(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    if car.car_flags(m) & CAR_AI_RACER == 0 {
        return;
    }
    let line = crate::types::AIDataRecord(AIDATARECORD_BEST_LINE.get(m)).data_buffer_(m);
    let look = car.look_ahead_slice(m) as u32;
    let at = m.r8(line.wrapping_add(look)) as i8 as i32;
    let pos = fixedmult(car.personality(m).best_line_ability(m), at << 14);
    car.set_preferred_lateral_position(m, pos);
    car.set_preferred_lateral_position_power(m, 0x5_0000);
    let slack = car.lane_slack(m).wrapping_mul(car.direction(m));
    car.set_preferred_lateral_position(m, pos.wrapping_sub(slack));
}

pub fn abi_calc_best_line_merits(c: &mut Cpu) {
    calc_best_line_merits(c, Car(c.r[4]));
}

/// `AI_AddCollidableObjects` (AI.CPP, 0x80059f1c): the objects of `group` within 100 m ahead are
/// obstacles across the road where they stand (by their width; some worse to hit).
pub fn add_collidable_objects(c: &mut Cpu, car: Car, group: u32) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        r.0[4] = group;
        r.0[0] = group.wrapping_add(4);
        let pos = crate::types::BWorldSmPos(SPOS);
        if c.m.r8(SPOS_UNSET) != 0 {
            c.m.w8(SPOS_UNSET, 0);
            r.enter(c, 0x8007_f788, 0x8005_9f60, |c| crate::bworldsm::set_slice(c, 0, pos));
        }
        r.0[3] = 0x8014_0000;
        let mut i = 0;
        let mut obj = group.wrapping_add(4);
        while i < c.m.r32(group) as i32 {
            (r.0[0], r.0[2]) = (obj, i as u32);
            let m = &mut c.m;
            let at = Coord(sp + 24);
            let v = Coord(obj).get(m);
            at.set(m, v);
            r.enter(c, 0x8007_f438, 0x8005_9fa4, |c| crate::bworldsm::find_closest_slice(c, at, pos));
            let m = &mut c.m;
            let slice = pos.slice(m) as i32;
            let d = r.enter(c, 0x8007_3a74, 0x8005_9fb0, |c| apx_spline_distance_from_slice(&mut c.m, slice, car));
            let m = &mut c.m;
            let ahead = d.wrapping_mul(car.direction(m));
            if (ahead.wrapping_sub(1) as u32) <= 0x63_FFFE {
                let here = car_slice(m, car);
                let s = slice_at(m, here);
                let center = Coord(sp + 40);
                let v = Vec3::new(s.center(m, 0), s.center(m, 1), s.center(m, 2));
                center.set(m, v);
                let (p, o) = (at.get(m), center.get(m));
                let d = Coord(sp + 56);
                let v = Vec3::new(p.x.wrapping_sub(o.x), p.y.wrapping_sub(o.y), p.z.wrapping_sub(o.z));
                d.set(m, v);
                let rm = car.n().road_matrix();
                let mul = |a: i32, b: i32| (a / 256).wrapping_mul(b / 256);
                let across = mul(rm.m(m, 0), v.x).wrapping_add(mul(rm.m(m, 1), v.y)).wrapping_add(mul(rm.m(m, 2), v.z));
                let penalty = if m.r8(obj + 19) == 1 { -0x28_0000 } else { -0xD_0000 };
                let half = (m.r16(obj + 12) as i16 as i32) << 9;
                let arg = pos.slice(m) as i32 as u32;
                m.w32(sp + 16, arg);
                r.enter(c, 0x8005_a1d4, 0x8005_a0ec, |c| submit_obstacle(c, car, penalty, across.wrapping_sub(half), across.wrapping_add(half), slice));
            }
            obj = obj.wrapping_add(20);
            i += 1;
        }
    })
}

pub fn abi_add_collidable_objects(c: &mut Cpu) {
    add_collidable_objects(c, Car(c.r[4]), c.r[5]);
}

/// `AI_AvoidObjects` (AI.CPP, 0x8005a118): the track's objects, when it has any.
pub fn avoid_objects(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        if c.m.r32(CUSTOM_OBJ_INST) != 0 {
            let group = c.m.r32(CUSTOM_SIM_OBJS);
            let r = Saved::of(c);
            r.enter(c, 0x8005_9f1c, 0x8005_a13c, |c| add_collidable_objects(c, car, group));
        }
    })
}

pub fn abi_avoid_objects(c: &mut Cpu) {
    avoid_objects(c, Car(c.r[4]));
}

/// `AI_AvoidSpikeBelt` (AI.CPP, 0x8005a14c): a spike belt within 100 m ahead is a bad obstacle.
pub fn avoid_spike_belt(c: &mut Cpu, car: Car) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let found = r.call(c, nfs4rc::f_8007e2a0, &[sp + 24, sp + 28, sp + 32], 0x8005_a16c); // BWorld_GetSpikeBelt
        if found == 0 {
            return;
        }
        let m = &mut c.m;
        let slice = m.r32(sp + 24) as i32;
        let d = r.enter(c, 0x8007_3a74, 0x8005_a180, |c| apx_spline_distance_from_slice(&mut c.m, slice, car));
        let m = &mut c.m;
        if (d.wrapping_mul(car.direction(m)).wrapping_sub(1) as u32) > 0x63_FFFE {
            return;
        }
        let (left, width) = (m.r32(sp + 28) as i32, m.r32(sp + 32) as i32);
        m.w32(sp + 16, slice as u32);
        r.enter(c, 0x8005_a1d4, 0x8005_a1c4, |c| submit_obstacle(c, car, -0x28_0000, left, left.wrapping_add(width), slice));
    })
}

pub fn abi_avoid_spike_belt(c: &mut Cpu) {
    avoid_spike_belt(c, Car(c.r[4]));
}

/// `AI_SubmitObstacle` (AI.CPP, 0x8005a1d4): an obstacle from `left` to `right` across the road
/// at `slice`: `penalty` on each of the three lanes it covers (when it covers all three, not on
/// the side the car is further from).
pub fn submit_obstacle(c: &mut Cpu, car: Car, penalty: i32, left: i32, right: i32, slice: i32) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1], r.0[2], r.0[3], r.0[4]) = (slice as u32, car.0, penalty as u32, left as u32, right as u32);
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8005_a214); // memset
        let low = r.enter(c, 0x8007_3dac, 0x8005_a220, |c| lane_index(&mut c.m, slice, left));
        r.0[0] = low as u32;
        let high = r.enter(c, 0x8007_3dac, 0x8005_a230, |c| lane_index(&mut c.m, slice, right));
        let m = &mut c.m;
        if (high as u32) >= 14 || (low as u32) >= 14 {
            return;
        }
        for (k, by) in [(0u32, -1i32), (1, 0), (2, 1)] {
            let lane = car.lane_index(m).wrapping_add(by);
            if lane >= low && high >= lane {
                m.w32(sp + 16 + 4 * k, penalty as u32);
            }
        }
        let hit = |m: &mut Mem, k: u32| m.r32(sp + 16 + 4 * k) != 0;
        if hit(m, 0) && hit(m, 1) && hit(m, 2) {
            let at = car.road_position(m);
            if at.wrapping_sub(left).wrapping_abs() < at.wrapping_sub(right).wrapping_abs() {
                m.w32(sp + 16, 0);
            } else {
                m.w32(sp + 24, 0);
            }
        }
        for k in 0..3 {
            let v = m.r32(sp + 16 + 4 * k) as i32;
            add_obs(m, k, v);
        }
    })
}

pub fn abi_submit_obstacle(c: &mut Cpu) {
    let slice = abi::arg(c, 4) as i32;
    submit_obstacle(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, slice);
}

/// `AI_HandleTrafficHonking` (AI.CPP, 0x8005a378): oncoming traffic in sight of a player, while
/// the lane ahead moves, honks now and then (not in a two-player game).
pub fn handle_traffic_honking(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let visible = r.call(c, nfs4rc::f_80068fec, &[car.0], 0x8005_a38c); // AILife_IsCarInAnyVisibleArea
        let m = &mut c.m;
        if car.car_flags(m) & CAR_TRAFFIC == 0 {
            return;
        }
        let dir = car.direction(m);
        let oncoming = if GAME_SETUP.reverse_track(m) != 0 { dir != -1 } else { dir != 1 };
        if !oncoming || visible == 0 || m.r32(LANE_SPEEDS + 4) as i32 <= -0x3_0000 {
            return;
        }
        let t = fast_random_step(m);
        let roll = ((t >> 8) & 0xFFFF).wrapping_mul(1000) >> 16;
        if GAME_SETUP.comm_mode(m) == 1 || roll >= 5 || car.current_speed(m) == 0 {
            return;
        }
        r.call(c, nfs4rc::f_80075088, &[car.0, 2, 32, 8], 0x8005_a46c); // AudioClc_HonkHorn
    })
}

pub fn abi_handle_traffic_honking(c: &mut Cpu) {
    handle_traffic_honking(c, Car(c.r[4]));
}

/// `AI_CheckForCarsOnSide` (AI.CPP, 0x8005a47c): a car alongside (within 10 m, and its length
/// and a half) makes that side's lane bad (not for traffic).
pub fn check_for_cars_on_side(c: &mut Cpu, car: Car) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = car.0;
        let m = &mut c.m;
        if car.car_flags(m) & CAR_TRAFFIC != 0 {
            return;
        }
        let radius = car.n().dimension().z(m);
        let near = (radius << 1).wrapping_add(radius / 2);
        (r.0[4], r.0[5], r.0[6], r.0[7]) = (0x8011_0000, OBS, -0x6_0000i32 as u32, near as u32);
        let mut i = 0;
        if NUM_CARS.get(m) <= 0 {
            return;
        }
        loop {
            (r.0[1], r.0[2]) = (i as u32, CARS_LIST + 4 * i as u32);
            let m = &mut c.m;
            let other = Car(m.r32(CARS_LIST + 4 * i as u32));
            r.0[0] = other.0;
            if other != car && other.n().active(m) != 0 {
                let d = r.enter(c, 0x8007_3b04, 0x8005_a524, |c| spline_distance(c, other, car)).wrapping_abs();
                let m = &mut c.m;
                if d <= 0xA_0000 && d < near {
                    let (mine, theirs) = (car.lane_index(m), other.lane_index(m));
                    if theirs == mine.wrapping_sub(1) {
                        add_obs(m, 0, -0x6_0000);
                    } else if theirs == mine.wrapping_add(1) {
                        add_obs(m, 2, -0x6_0000);
                    }
                }
            }
            i += 1;
            if i >= NUM_CARS.get(&mut c.m) {
                break;
            }
        }
    })
}

pub fn abi_check_for_cars_on_side(c: &mut Cpu) {
    check_for_cars_on_side(c, Car(c.r[4]));
}

/// `AI_ProcessObservationsAndChooseLane` (AI.CPP, 0x8005a5cc): the merits weigh the lanes.
pub fn process_observations_and_choose_lane(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    for k in 0..3 {
        let v = obs(m, k) as u32;
        m.w32(LANE_WEIGHTS + 4 * k, v);
    }
    choose_best_lane(m, car);
}

pub fn abi_process_observations_and_choose_lane(c: &mut Cpu) {
    process_observations_and_choose_lane(c, Car(c.r[4]));
}

/// `AI_ChooseBestLane` (AI.CPP, 0x8005a610): the lane with the best weight (its own when tied,
/// right over left), within the road's 14.
pub fn choose_best_lane(m: &mut Mem, car: Car) {
    let mut best = info(m, LANE_WEIGHTS, 1);
    m.w32(DESIRED_LANE_SIDE, 1);
    let lane = car.lane_index(m);
    m.w32(DESIRED_LANE, lane as u32);
    let left = info(m, LANE_WEIGHTS, 0);
    if best < left {
        best = left;
        m.w32(DESIRED_LANE_SIDE, 0);
        let lane = car.lane_index(m).wrapping_sub(1);
        m.w32(DESIRED_LANE, lane as u32);
    }
    if best < info(m, LANE_WEIGHTS, 2) {
        m.w32(DESIRED_LANE_SIDE, 2);
        let lane = car.lane_index(m).wrapping_add(1);
        m.w32(DESIRED_LANE, lane as u32);
    }
    let lane = m.r32(DESIRED_LANE) as i32;
    if lane < 0 {
        m.w32(DESIRED_LANE, 0);
    } else if lane >= 14 {
        m.w32(DESIRED_LANE, 13);
    }
}

pub fn abi_choose_best_lane(c: &mut Cpu) {
    choose_best_lane(&mut c.m, Car(c.r[4]));
}

/// The lane (0 left, 1 own, 2 right) a position across the road is in, relative to the car's.
fn side_of(lane: i32, own: i32) -> u32 {
    if lane == own { 1 } else if lane < own { 0 } else { 2 }
}

/// The best of the weights at sp+16 (the first of equals; right over left as `ChooseBestLane`
/// does it but from the left).
fn best_of(m: &mut Mem, sp: u32) -> u32 {
    let (a, b, c) = (m.r32(sp + 16) as i32, m.r32(sp + 20) as i32, m.r32(sp + 24) as i32);
    let (mut best, mut v) = (0, a);
    if a < b {
        (best, v) = (1, b);
    }
    if v < c {
        best = 2;
    }
    best
}

/// `AI_CheckPreferredLateralPosition` (AI.CPP, 0x8005a69c): when both edges of the car at its
/// preferred position are in the best lane (with the preference's weight), it steers there.
pub fn check_preferred_lateral_position(c: &mut Cpu, car: Car) -> bool {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let edge = |c: &mut Cpu, r: &Saved, by: i32, ra: u32| {
            let m = &mut c.m;
            let at = car.preferred_lateral_position(m).wrapping_add(by);
            let slice = car_slice(m, car);
            let lane = r.enter(c, 0x8007_3dac, ra, |c| lane_index(&mut c.m, slice, at));
            let m = &mut c.m;
            let side = side_of(lane, car.lane_index(m));
            for k in 0..3 {
                let v = m.r32(LANE_WEIGHTS + 4 * k);
                m.w32(sp + 16 + 4 * k, v);
            }
            let power = car.preferred_lateral_position_power(m);
            let w = (m.r32(sp + 16 + 4 * side) as i32).wrapping_add(power);
            m.w32(sp + 16 + 4 * side, w as u32);
            (side, best_of(m, sp))
        };
        let half = car.n().dimension().x(&mut c.m);
        let (side_l, best_l) = edge(c, &r, half.wrapping_neg().wrapping_sub(0x4000), 0x8005_a6cc);
        (r.0[1], r.0[2]) = (best_l, side_l);
        let half = car.n().dimension().x(&mut c.m);
        let (side_r, best_r) = edge(c, &r, half.wrapping_add(0x4000), 0x8005_a780);
        if best_l == side_l && best_r == side_r {
            let m = &mut c.m;
            let at = car.preferred_lateral_position(m);
            car.set_desired_lat_pos(m, at);
            return true;
        }
        false
    })
}

pub fn abi_check_preferred_lateral_position(c: &mut Cpu) {
    c.r[2] = check_preferred_lateral_position(c, Car(c.r[4])) as u32;
}

/// `AI_TryToShareLanes` (AI.CPP, 0x8005a850): beside the car blocking its lane, when there is
/// room for one and a half of the car on a side of it within the lane: in the middle of the
/// wider side.
pub fn try_to_share_lanes(m: &mut Mem, car: Car, other: Car) -> bool {
    let half = car.n().dimension().x(m);
    let room = half.wrapping_add(half / 2);
    let lane = m.r32(DESIRED_LANE) as i32;
    let theirs = car_slice(m, other);
    let s = slice_at(m, theirs);
    let (from, to) = if lane >= 7 {
        let w = (s.avg_paved_width_rt(m) as i32) << 15;
        let a = (lane - 7).wrapping_mul(w);
        (a, a.wrapping_add(w))
    } else {
        let w = (s.avg_paved_width_lf(m) as i32) << 15;
        let b = (lane - 6).wrapping_mul(w);
        (b.wrapping_sub(w), b)
    };
    let (pos, span) = (other.road_position(m), other.road_span(m));
    let near = pos.wrapping_sub(span);
    let left = near.wrapping_sub(from);
    let far = pos.wrapping_add(span);
    let right = to.wrapping_sub(far);
    if !(room < right || room < left) {
        return false;
    }
    let at = if left < right { far.wrapping_add(right / 2) } else { near.wrapping_sub(left / 2) };
    car.set_desired_lat_pos(m, at);
    true
}

pub fn abi_try_to_share_lanes(c: &mut Cpu) {
    c.r[2] = try_to_share_lanes(&mut c.m, Car(c.r[4]), Car(c.r[5])) as u32;
}

/// `AI_CalculateDesiredLatPosition` (AI.CPP, 0x8005a94c): where across the road to drive: the
/// preferred position when it fits the best lane, beside a car within 20 m in it when there is
/// room (out of the mellow zone), else the middle of the lane (the shoulder's inside edge, its
/// own width in), on the drivable road.
pub fn calculate_desired_lat_position(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        let side = m.r32(DESIRED_LANE_SIDE);
        let slice = car_slice(m, car);
        r.0[3] = slice as u32;
        let ahead = Car(m.r32(INFO.wrapping_add(side.wrapping_mul(4))));
        r.0[1] = ahead.0;
        if car.preferred_lateral_position_power(m) > 0 && r.enter(c, 0x8005_a69c, 0x8005_a998, |c| check_preferred_lateral_position(c, car)) {
            return;
        }
        r.0[2] = 0;
        let mut share = false;
        if ahead.0 != 0 {
            let m = &mut c.m;
            let d = apx_spline_distance(m, ahead, car);
            let near = if d > 0 { apx_spline_distance(m, ahead, car) <= 0x13_FFFF } else { apx_spline_distance(m, ahead, car).wrapping_neg() <= 0x13_FFFF };
            if near && !r.enter(c, 0x8005_ae18, 0x8005_aa0c, |c| is_mellow_zone(c, car, 0x3E8_0000)) {
                share = true;
            }
        }
        let m = &mut c.m;
        if share && try_to_share_lanes(m, car, ahead) {
            return;
        }
        let s = slice_at(m, slice);
        let lanes = s.lane_count(m) as i32;
        let lane = m.r32(DESIRED_LANE) as i32;
        let (lf, rt) = ((s.avg_paved_width_lf(m) as i32) << 15, (s.avg_paved_width_rt(m) as i32) << 15);
        let half = car.n().dimension().x(m);
        let at = if edge_lane(lanes, lane) {
            if lane < 7 { (lane - 6).wrapping_mul(lf).wrapping_sub(half) } else { (lane - 7).wrapping_mul(rt).wrapping_add(half) }
        } else if lane < 7 {
            (lane - 6).wrapping_mul(lf).wrapping_sub(((lf as u32) >> 1) as i32)
        } else {
            (lane - 7).wrapping_mul(rt).wrapping_add(((rt as u32) >> 1) as i32)
        };
        car.set_desired_lat_pos(m, at);
        let edge = ((s.right_drive(m) as i32) << 8).wrapping_sub(car.n().dimension().x(m));
        if edge < car.desired_lat_pos(m) {
            car.set_desired_lat_pos(m, edge);
        }
        let edge = car.n().dimension().x(m).wrapping_sub((s.left_drive(m) as i32) << 8);
        if car.desired_lat_pos(m) < edge {
            car.set_desired_lat_pos(m, edge);
        }
    })
}

pub fn abi_calculate_desired_lat_position(c: &mut Cpu) {
    calculate_desired_lat_position(c, Car(c.r[4]));
}

/// `AI_CalculateAdjustedDesiredSpeed` (AI.CPP, 0x8005ab80): the car keeps behind the car in its
/// lane at that lane's speed, or the new lane's when it can get there before closing on it
/// (the time across against the time along, with a fifth to spare); no slower than 1.3 m/s
/// backward or 1.3 m/s forward; changing lanes behind a car stopped (under 10 m/s), at least
/// 6.7 m/s.
pub fn calculate_adjusted_desired_speed(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = INFO;
        r.0[1] = car.0;
        let m = &mut c.m;
        let ahead = blocking(m, 1);
        let mut speed = info(m, LANE_SPEEDS, 1);
        if m.r32(DESIRED_LANE) as i32 != car.lane_index(m) && ahead.0 != 0 && ahead.n().dead_timer(m) == 0 {
            let d = r.enter(c, 0x8007_3b04, 0x8005_abe4, |c| spline_distance(c, car, ahead));
            let gap = if d > 0 {
                r.enter(c, 0x8007_3b04, 0x8005_abf8, |c| spline_distance(c, car, ahead))
            } else {
                r.enter(c, 0x8007_3b04, 0x8005_ac0c, |c| spline_distance(c, car, ahead)).wrapping_neg()
            };
            r.0[4] = gap as u32;
            let m = &mut c.m;
            let blocker = blocking(m, 1);
            let d = car.current_speed(m).wrapping_sub(blocker.current_speed(m));
            let closing = if d > 0 { d } else { blocker.current_speed(m).wrapping_sub(car.current_speed(m)) };
            r.0[2] = closing as u32;
            let d = car.desired_lat_pos(m).wrapping_sub(car.road_position(m));
            let across = if d > 0 { d } else { car.road_position(m).wrapping_sub(car.desired_lat_pos(m)) };
            r.0[0] = across as u32;
            let lat = r.enter(c, 0x8007_41fc, 0x8005_ac58, |c| calc_lateral_velocity(&mut c.m, car));
            let lat = if lat > 0 {
                r.enter(c, 0x8007_41fc, 0x8005_ac68, |c| calc_lateral_velocity(&mut c.m, car))
            } else {
                r.enter(c, 0x8007_41fc, 0x8005_ac78, |c| calc_lateral_velocity(&mut c.m, car)).wrapping_neg()
            };
            let a = fixedmult(across, closing);
            let b = fixedmult(gap, lat);
            let m = &mut c.m;
            speed = if fixedmult(a, 0x1_3333) < b {
                if car.lane_index(m) < m.r32(DESIRED_LANE) as i32 { info(m, LANE_SPEEDS, 2) } else { info(m, LANE_SPEEDS, 0) }
            } else {
                info(m, LANE_SPEEDS, 1)
            };
        }
        let m = &mut c.m;
        if car.direction(m) == 1 {
            if speed < car.desired_speed(m) {
                car.set_desired_speed(m, speed);
            }
            if car.desired_speed(m) < -0x1_4CCC {
                car.set_desired_speed(m, -0x1_4CCC);
            }
        } else {
            if car.desired_speed(m) < speed {
                car.set_desired_speed(m, speed);
            }
            if 0x1_4CCC < car.desired_speed(m) {
                car.set_desired_speed(m, 0x1_4CCC);
            }
        }
        let blocker = blocking(m, 1);
        if m.r32(DESIRED_LANE) as i32 != car.lane_index(m) && blocker.0 != 0 && blocker.current_speed(m).wrapping_abs() <= 0x9_FFFF {
            let want = car.desired_speed(m);
            if car.direction(m) == 1 {
                if want < 0x6_AAAA {
                    car.set_desired_speed(m, 0x6_AAAA);
                }
            } else if -0x6_AAAA < want {
                car.set_desired_speed(m, -0x6_AAAA);
            }
        }
    })
}

pub fn abi_calculate_adjusted_desired_speed(c: &mut Cpu) {
    calculate_adjusted_desired_speed(c, Car(c.r[4]));
}

/// `AI_IsMellowZone` (AI.CPP, 0x8005ae18): whether an AI racer (in a lane, not on a shoulder) is
/// near enough the start to keep calm: within 50 m (100 with five or more AI racers, 12.5 with
/// one, none without) and `extra` past the start, its place down the order 5.25 m each.
pub fn is_mellow_zone(c: &mut Cpu, car: Car, extra: i32) -> bool {
    let m = &mut c.m;
    let order = car.sort_index(m);
    let odometer = game_odometer(m, car);
    if car.car_flags(m) & CAR_AI_RACER == 0 {
        return false;
    }
    let here = car_slice(m, car);
    let lanes = lanes_of(m, here);
    if edge_lane(lanes, car.lane_index(m)) {
        return false;
    }
    let n = NUM_AI_RACE_CARS.get(m);
    let mut zone: i32 = if n < 5 { 0x320_0000 } else { 0x640_0000 };
    if n < 2 {
        zone = 0xC8_0000;
    }
    if n == 0 {
        zone = 0;
    }
    let at = odometer.wrapping_add(order.wrapping_mul(21) << 18);
    zone.wrapping_add(extra) >= at
}

pub fn abi_is_mellow_zone(c: &mut Cpu) {
    c.r[2] = is_mellow_zone(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AI_KeepCarsInLane` (AI.CPP, 0x8005af04): nothing.
pub fn keep_cars_in_lane() {}

pub fn abi_keep_cars_in_lane(_: &mut Cpu) {}

/// `AI_PushFinishedCarsToSide` (AI.CPP, 0x8005af14): a finished racer past its place's distance
/// from the line (not against a human cop) wants the shoulder side: its own lane if on the
/// edge, else the side it is on.
pub fn push_finished_cars_to_side(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    if car.car_flags(m) & CAR_RACING == 0 || car.stats().finish_type(m) != FINISHED || against_human_cop(m) {
        return;
    }
    let gap = apx_spline_distance_to_slice(m, car, 0).wrapping_abs();
    let n = NUM_CARS.get(m);
    let mut place = 0;
    while place < n && m.r32(TOTAL_SORTED_LIST + 4 * place as u32) != car.0 {
        place += 1;
    }
    if gap < place.wrapping_mul(5) << 19 {
        return;
    }
    let here = car_slice(m, car);
    let lanes = lanes_of(m, here);
    let lane = car.lane_index(m);
    let k = if edge_lane(lanes, lane) { 1 } else if lane < 7 { 0 } else { 2 };
    add_obs(m, k, 0x96_0000);
}

pub fn abi_push_finished_cars_to_side(c: &mut Cpu) {
    push_finished_cars_to_side(c, Car(c.r[4]));
}

/// `AI_MaybeChangeLaneSlack` (AI.CPP, 0x8005b0b0): now and then (every 16th slice for AI
/// racers, every other for traffic, staggered by the car; not humans) a new lane slack, by the
/// personality's chance.
pub fn maybe_change_lane_slack(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let here = car_slice(m, car);
        let by = car.car_index(m) << 1;
        let s = slice_on(m, here, by);
        let flags = car.car_flags(m);
        if flags & CAR_HUMAN != 0 || (flags & CAR_AI_RACER != 0 && (s >> 4) << 4 != s) || (flags & CAR_TRAFFIC != 0 && (s >> 1) << 1 != s) {
            return;
        }
        let t = fast_random_step(m);
        let roll = (car.personality(m).swerve_change_prob(m).wrapping_mul(((t >> 8) & 0xFFFF) as i32) as u32) >> 16;
        if roll == 1 {
            choose_new_lane_slack(m, car);
        }
    })
}

pub fn abi_maybe_change_lane_slack(c: &mut Cpu) {
    maybe_change_lane_slack(c, Car(c.r[4]));
}

/// `AI_ChooseNewLaneSlack` (AI.CPP, 0x8005b1cc): one of the personality's four lane swerves.
pub fn choose_new_lane_slack(m: &mut Mem, car: Car) {
    let t = fast_random_step(m);
    let swerve = car.personality(m).lane_swerve(m, (t >> 8) & 3);
    car.set_lane_slack(m, swerve);
}

pub fn abi_choose_new_lane_slack(c: &mut Cpu) {
    choose_new_lane_slack(&mut c.m, Car(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, against_original};

    /// The AI speeds' shuffled race (the car at `SCRATCH`, another at `SCRATCH+4`), with any
    /// lane merits and AI info, every car in any lanes and on any slice near, the scripts in
    /// any state, the humans honking or swapping places, preferred positions, slack, barriers
    /// and objects; an obstacle's penalty and edges (`SCRATCH+8..16`).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let car = Car(m.r32(SCRATCH));
        let n = NUM_CARS.get(m);
        let cars: Vec<Car> = (0..n as u32).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let other = cars[pick(n as u32) as usize];
        m.w32(SCRATCH + 4, other.0);
        for k in 0..3 {
            m.w32(OBS + 4 * k, (pick(0x40_0000) - 0x20_0000) as u32);
            m.w32(BLOCKING + 4 * k, [0, cars[pick(n as u32) as usize].0][pick(2) as usize]);
            m.w32(BLOCKING_DIST + 4 * k, pick(0x40_0000) as u32);
            m.w32(LANE_SPEEDS + 4 * k, (pick(0x40_0000) - 0x20_0000) as u32);
            m.w32(LANE_SPEEDS_AHEAD + 4 * k, (pick(0x40_0000) - 0x20_0000) as u32);
            m.w32(LANE_WEIGHTS + 4 * k, (pick(0x40_0000) - 0x20_0000) as u32);
        }
        m.w32(DESIRED_LANE, [pick(14), pick(18) - 2][pick(2) as usize] as u32);
        m.w32(DESIRED_LANE_SIDE, pick(3) as u32);
        m.w32(DELTA_YAW, pick(400) as u32);
        let here = car_slice(m, car);
        let slices = NUM_SLICES.get(m);
        for &o in &cars {
            o.set_lane_index(m, [pick(14), pick(3) + 5, pick(18) - 2][pick(3) as usize]);
            o.set_car_in_lane(m, pick(0x4000));
            if pick(2) == 0 {
                let s = (here + pick(20) - 10).rem_euclid(slices);
                o.n().sim_road_info().set_slice(m, s as i16);
                let p = car.n().position().get(m);
                o.n().position().set(m, Vec3::new(p.x + pick(0x40_0000) - 0x20_0000, p.y, p.z + pick(0x40_0000) - 0x20_0000));
            }
            o.set_road_position(m, pick(0x40_0000) - 0x20_0000);
            o.set_road_span(m, pick(0x2_0000));
            o.set_sort_index(m, pick(n as u32));
            o.set_swap_car(m, [Car(0), car][pick(2) as usize]);
            let t = SIM_COUNTER.get(m) - pick(30);
            o.set_swap_time(m, t);
            o.control().set_horn(m, pick(2) as i8);
            o.set_preferred_lateral_position(m, pick(0x40_0000) - 0x20_0000);
            o.set_preferred_lateral_position_power(m, [0, 0x5_0000][pick(2) as usize]);
            o.set_lane_slack(m, pick(0x4_0000) - 0x2_0000);
            o.set_barrier_think_harder(m, [0, pick(80)][pick(2) as usize]);
            o.set_desired_lat_pos(m, pick(0x40_0000) - 0x20_0000);
            o.n().set_dead_timer(m, [0, 0, pick(10) as u16][pick(3) as usize]);
            let hit = o.n().collision();
            let t = SIM_COUNTER.get(m) - pick(30);
            hit.set_last_time(m, t);
            hit.set_last_other_obj(m, crate::types::NewtonObj([0, car.0, other.0][pick(3) as usize]));
            let s = o.script();
            s.set_action_index(m, [7, pick(7)][pick(2) as usize]);
            s.set_detect_action(m, [7, pick(7)][pick(2) as usize]);
            s.set_reaction(m, [1, pick(0x4000)][pick(2) as usize]);
            s.set_reaction_index(m, pick(4) - 1);
            s.set_reaction_ticks_left(m, pick(64) - 8);
            s.set_action_hum_car_index(m, pick(n as u32));
            s.set_detect_hum_car_index(m, pick(n as u32));
            let t = SIM_COUNTER.get(m) - pick(3000);
            s.set_last_action_time(m, t);
            for i in 0..7 {
                s.set_last_reaction_index(m, i, pick(4) - 1);
            }
            // (states the game reaches: no action without script details, and each reaction
            // lasting, or the original loops for ever)
            let data = m.r32(s.0 + 0x1c);
            if data == 0 {
                s.set_action_index(m, 7);
                s.set_detect_action(m, 7);
            } else {
                for action in 0..8u32 {
                    for i in 0..4u32 {
                        let at = data + action * 8 + 4 + i;
                        if m.r8(at) == 0 {
                            m.w8(at, 1 + pick(8) as u8);
                        }
                    }
                }
            }
        }
        m.w32(SCRATCH + 8, [-0x28_0000, -0xD_0000][pick(2) as usize] as u32);
        let left = pick(0x30_0000) - 0x18_0000;
        m.w32(SCRATCH + 12, left as u32);
        m.w32(SCRATCH + 16, (left + pick(0x8_0000)) as u32);
        m.w32(SCRATCH + 20, here as u32);
        m.w32(SCRATCH + 24, pick(14) as u32);
        m.w8(SPOS_UNSET, pick(2) as u8);
        crate::globals::AITUNE_ONE_WAY.set(m, [0, 0, 1][pick(3) as usize]);
        GAME_SETUP.set_comm_mode(m, pick(2));
        GAME_SETUP.set_time(m, pick(2));
        // traffic and police about, a spike belt ahead (gSpikeBelt and its slice, edge and
        // width), out of the mellow zone, a straight road, the blocking ranges
        NUM_TRAFFIC_CARS.set(m, pick(3));
        NUM_COP_CARS.set(m, pick(2));
        let ahead = (here + pick(30)).rem_euclid(slices);
        for (at, v) in [(0x8013_DA88u32, pick(2)), (0x8013_DA8C, ahead), (0x8013_DA94, pick(0x20_0000) - 0x10_0000), (0x8013_DAA0, pick(0x8_0000))] {
            m.w32(at, v as u32);
        }
        if pick(2) == 0 {
            car.n().set_total_slice(m, 2000 + pick(2000) as u16);
            car.set_sort_index(m, pick(3));
            m.w32(DELTA_YAW, pick(20) as u32);
        }
        let p = car.personality(m);
        p.set_block_max_distance(m, pick(0x40_0000));
        p.set_block_min_distance(m, pick(0x10_0000) - 0x8_0000);
        p.set_oncoming_look_ahead(m, pick(0x80_0000));
    }

    fn arg(c: &mut Cpu, k: u32) -> u32 {
        c.m.r32(SCRATCH + 4 * k)
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    fn car_case(name: &'static str, original: fn(&mut Cpu), port: fn(&mut Cpu, Car)) -> (&'static str, fn(&mut Cpu), &'static [u32], Port, fn(&mut Cpu, Car)) {
        (name, original, &[0], |_, _| None, port)
    }

    #[test]
    fn ai_fuzz() {
        let cars: [(&str, fn(&mut Cpu), &[u32], Port, fn(&mut Cpu, Car)); 29] = [
            car_case("AI_InitAIInfo", nfs4rc::f_80058214, init_ai_info),
            car_case("AI_DoReactions", nfs4rc::f_80058384, do_reactions),
            car_case("AI_DoReactionsAndBehavior", nfs4rc::f_80058804, do_reactions_and_behavior),
            car_case("AI_CheckForBarriers", nfs4rc::f_80058ac4, check_for_barriers),
            car_case("AI_SetupOncomingLaneDemerits", nfs4rc::f_80058e84, setup_oncoming_lane_demerits),
            car_case("AI_HandleChangeInNumLanes", nfs4rc::f_80058fdc, handle_change_in_num_lanes),
            car_case("AI_HandleShouldersAndOffRoad", nfs4rc::f_8005914c, |c, car| handle_shoulders_and_off_road(&mut c.m, car)),
            car_case("AI_CalculateLaneSpeeds", nfs4rc::f_80059510, calculate_lane_speeds),
            car_case("AI_CalcMeritsBasedOnSpeed", nfs4rc::f_800598a4, calc_merits_based_on_speed),
            car_case("AI_CheckForClearLanes", nfs4rc::f_80059c24, check_for_clear_lanes),
            car_case("AI_CalcBestLineMerits", nfs4rc::f_80059e94, calc_best_line_merits),
            car_case("AI_AvoidObjects", nfs4rc::f_8005a118, avoid_objects),
            car_case("AI_AvoidSpikeBelt", nfs4rc::f_8005a14c, avoid_spike_belt),
            car_case("AI_HandleTrafficHonking", nfs4rc::f_8005a378, handle_traffic_honking),
            car_case("AI_CheckForCarsOnSide", nfs4rc::f_8005a47c, check_for_cars_on_side),
            car_case("AI_ProcessObservationsAndChooseLane", nfs4rc::f_8005a5cc, process_observations_and_choose_lane),
            car_case("AI_ChooseBestLane", nfs4rc::f_8005a610, |c, car| choose_best_lane(&mut c.m, car)),
            car_case("AI_CalculateDesiredLatPosition", nfs4rc::f_8005a94c, calculate_desired_lat_position),
            car_case("AI_CalculateAdjustedDesiredSpeed", nfs4rc::f_8005ab80, calculate_adjusted_desired_speed),
            car_case("AI_PushFinishedCarsToSide", nfs4rc::f_8005af14, push_finished_cars_to_side),
            car_case("AI_MaybeChangeLaneSlack", nfs4rc::f_8005b0b0, maybe_change_lane_slack),
            car_case("AI_ChooseNewLaneSlack", nfs4rc::f_8005b1cc, |c, car| choose_new_lane_slack(&mut c.m, car)),
            car_case("AI_GenericBeginCycle", nfs4rc::f_8005825c, generic_begin_cycle),
            car_case("AI_CarAvoidance", nfs4rc::f_80058184, car_avoidance),
            car_case("AI_WorldAvoidance", nfs4rc::f_800581c8, world_avoidance),
            car_case("AI_GenericEndCycle", nfs4rc::f_800582d8, generic_end_cycle),
            car_case("AI_GenericCycle", nfs4rc::f_80058284, generic_cycle),
            car_case("AIScript_ProcessActions", nfs4rc::f_80070008, |c, car| process_actions_script(c, car)),
            car_case("AIScript_Startup", nfs4rc::f_8006ff3c, |c, car| crate::aiscript::startup(&mut c.m, car.script())),
        ];
        for (k, (name, original, _, _, port)) in cars.into_iter().enumerate() {
            // (the script functions take the script, the others the car)
            let script = name.starts_with("AIScript");
            let args = move |c: &mut Cpu| {
                let car = arg(c, 0);
                if script { vec![car + 0x4f4, 2] } else { vec![car] }
            };
            thread_local!(static PORT: std::cell::Cell<Option<fn(&mut Cpu, Car)>> = const { std::cell::Cell::new(None) });
            PORT.with(|p| p.set(Some(port)));
            let run: Port = if script {
                |c, a| {
                    PORT.with(|p| p.get().unwrap())(c, Car(a[0] - 0x4f4));
                    None
                }
            } else {
                |c, a| {
                    PORT.with(|p| p.get().unwrap())(c, Car(a[0]));
                    None
                }
            };
            against_original(name, 3_000, 0xA60 + k as u64, shuffle, original, &args, run);
        }
        let others: [(&str, fn(&mut Cpu), &[u32], Port); 10] = [
            ("AI_TargetLane", nfs4rc::f_8005830c, &[0, 6], |c, a| {
                target_lane(&mut c.m, Car(a[0]), a[1] as i32);
                None
            }),
            ("AI_CheckForPlayerActions", nfs4rc::f_800585c4, &[0, 1], |c, a| {
                check_for_player_actions(c, Car(a[0]), Car(a[1]));
                None
            }),
            ("AI_OpponentBlockPlayer", nfs4rc::f_800588c8, &[0, 1], |c, a| {
                opponent_block_player(c, Car(a[0]), Car(a[1]));
                None
            }),
            ("AI_CheckPreferredLateralPosition", nfs4rc::f_8005a69c, &[0], |c, a| Some(check_preferred_lateral_position(c, Car(a[0])) as u32)),
            ("AI_TryToShareLanes", nfs4rc::f_8005a850, &[0, 1], |c, a| Some(try_to_share_lanes(&mut c.m, Car(a[0]), Car(a[1])) as u32)),
            ("AI_IsMellowZone", nfs4rc::f_8005ae18, &[0, 3], |c, a| Some(is_mellow_zone(c, Car(a[0]), a[1] as i32) as u32)),
            ("AIScript_DoReAction", nfs4rc::f_80070170, &[100, 3], |c, a| Some(crate::aiscript::do_re_action(&mut c.m, crate::types::AIScript(a[0]), a[1] as i32) as u32)),
            ("AIScript_GetReactionTicksLeft", nfs4rc::f_800701a0, &[100], |c, a| {
                Some(crate::aiscript::get_reaction_ticks_left(&mut c.m, crate::types::AIScript(a[0])) as u32)
            }),
            ("AIScript_SubmitPlayerAction", nfs4rc::f_8006ff80, &[100, 6, 6, 101], |c, a| {
                crate::aiscript::submit_player_action(&mut c.m, crate::types::AIScript(a[0]), a[1] as i32, a[2] as i32, a[3] as i32);
                None
            }),
            ("AI_SubmitObstacle", nfs4rc::f_8005a1d4, &[0, 2, 3, 4], |c, a| {
                let slice = abi::arg(c, 4) as i32;
                submit_obstacle(c, Car(a[0]), a[1] as i32, a[2] as i32, a[3] as i32, slice);
                None
            }),
        ];
        for (k, (name, original, which, port)) in others.into_iter().enumerate() {
            let args = move |c: &mut Cpu| {
                let car = arg(c, 0);
                let v: Vec<u32> = which.iter().map(|&w| match w {
                    100 => car + 0x4f4,
                    101 => SIM_COUNTER.get(&mut c.m) as u32,
                    w => arg(c, w),
                }).collect();
                // (the obstacle's slice, its fifth argument, at sp+16)
                let sp = c.r[29];
                let slice = arg(c, 5);
                c.m.w32(sp + 16, slice);
                v
            };
            against_original(name, 3_000, 0xB60 + k as u64, shuffle, original, &args, port);
        }
    }

    fn process_actions_script(c: &mut Cpu, car: Car) {
        crate::aiscript::process_actions_and_reactions(&mut c.m, car.script(), 2);
    }

    #[test]
    fn aitune_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 4] = [
            ("AITune_GetOneWay", nfs4rc::f_80073748, |c, _| Some(crate::aitune::get_one_way(&mut c.m) as u32)),
            ("AITune_GetDriveSide", nfs4rc::f_800737a0, |c, _| Some(crate::aitune::get_drive_side(&mut c.m) as u32)),
            ("AITune_GetRoughLapTimes", nfs4rc::f_800737e0, |c, _| Some(crate::aitune::get_rough_lap_times(&mut c.m) as u32)),
            ("AITune_StartUp1", nfs4rc::f_80073820, |c, _| {
                crate::aitune::start_up1(&mut c.m);
                None
            }),
        ];
        let tracks = |c: &mut Cpu, rng: &mut Rng| {
            shuffle(c, rng);
            let t = (rng.next() % 16) as i32;
            GAME_SETUP.set_track(&mut c.m, t);
        };
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 200, 0xC60 + k as u64, tracks, original, &|_| vec![], port);
        }
    }

    /// The stack an AI cycle leaves (what runs after, still recompiled, reads it).
    #[test]
    fn cycle_stack_fuzz() {
        let top = crate::test_util::SCRATCH_TOP;
        for (name, addr, original) in [("AI_GenericCycle", 0x8005_8284u32, nfs4rc::f_80058284 as fn(&mut Cpu)), ("AI_GenericEndCycle", 0x8005_82d8, nfs4rc::f_800582d8)] {
            crate::test_util::same_stack(name, addr, original, 0x8006_0000, top, 2_000, 0x57A4, shuffle, &|c, _| vec![arg(c, 0)]);
        }
    }
}
