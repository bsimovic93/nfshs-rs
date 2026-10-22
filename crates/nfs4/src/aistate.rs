//! AISTATE.CPP: an AI car's state (C++ objects of 10 kinds, from a pool of 10 in the retail
//! build), each with a virtual `Execute` run each AI tick: normal driving, idling, chasing a
//! target (a police car after a human: far, close, approaching; around barriers), waiting off
//! the road, purgatory (a traffic car parked off the track until it may live again), roving
//! traffic, donuts, going to a slice, cruising.

use crate::abi::{self, Saved};
use crate::aiworld::{apx_spline_distance, apx_spline_distance_to_slice, check_for_barrier_between_lanes};
use crate::fixed::fixedmult;
use crate::globals::{
    AI_ELAPSED_TIME, BWORLDSM_SLICES, CARS_LIST, FAST_RANDOM, GAME_SETUP, HUMAN_RACE_CARS, NUM_CARS, NUM_HUMAN_RACE_CARS, NUM_SLICES,
    NUM_TRAFFIC_CARS, RAND_SEED, RAND_TEMP, SIM_COUNTER, SORTED_LIST,
};
use crate::types::{
    AIStateBase, AIStateChase, AIStateCruise, AIStateDonuts, AIStateGotoSlice, AIStateIdle, AIStateOffroad, AIStateRovingTraffic, Car, Coord, Matrix, Trigger,
    TrkNewSlice,
};
use ps1rt::{Cpu, Mem};

// The classes' vtables.
const VT_BASE: u32 = 0x8005_5E10;
const VT_NORMAL: u32 = 0x8005_5DF0;
const VT_CHASE: u32 = 0x8005_5DB0;
const VT_OFFROAD: u32 = 0x8005_5D90;
const VT_NON_ACTIVE: u32 = 0x8005_5D70;
const VT_PURGATORY: u32 = 0x8005_5D50;
const VT_IDLE: u32 = 0x8005_5DD0;
const VT_ROVING_TRAFFIC: u32 = 0x8005_5D30;
const VT_DONUTS: u32 = 0x8005_5D10;
const VT_GOTO_SLICE: u32 = 0x8005_5CF0;
const VT_CRUISE: u32 = 0x8005_5CD0;

/// The states' pool: 10 objects of 160 bytes, and whether each is taken.
const POOL: u32 = 0x8010_EA70;
const POOL_TAKEN: u32 = 0x8010_F0B0;
/// `AIState_Purgatory::numTrafficCarsInPurgatory`.
pub const NUM_IN_PURGATORY: u32 = 0x8013_F084;
/// `AITune_LifeTimer`: a traffic car's ticks in purgatory by the number of traffic cars;
/// `AITune_MaxTraffic`: how many may live by density (two-player races 4 on).
const LIFE_TIMER: u32 = 0x8010_F2A0;
const MAX_TRAFFIC: u32 = 0x8010_F280;
/// The buses' car names ("SBUS", "TBUS"): five times as long in purgatory.
const SCHOOL_BUS: u32 = 0x8013_D8C8;
const TOUR_BUS: u32 = 0x8013_D8D0;

/// `Car_tObj.carFlags`.
const CAR_HUMAN: i32 = 0x4;
const CAR_TRAFFIC: i32 = 0x10;
/// Not simulated yet (purgatory's timer waits).
const CAR_NOT_STARTED: i32 = 0x20;
const CAR_LIFE_BASIS: i32 = 0x100;
/// Out of the race (an idle car).
const CAR_OUT: i32 = 0x400;
/// Waiting off the road.
const CAR_OFFROAD: i32 = 0x800;
/// `Car_tObj.AIFlags`: parked.
const AI_PARKED: i32 = 0x4;
/// No nitrous (`accNitrous`, `speedNitrous`).
const NO_NITROUS: i32 = 0x1_0000;

/// The virtual function `k` of a state (its vtable at +4: per entry a `this` adjustment and the
/// function): the object to call it on, and the function.
pub(crate) fn virtual_fn(m: &mut Mem, this: u32, k: u32) -> (u32, u32) {
    let vt = m.r32(this.wrapping_add(4));
    let adjust = m.r16(vt.wrapping_add(8 * k)) as i16 as i32;
    (this.wrapping_add(adjust as u32), m.r32(vt.wrapping_add(8 * k + 4)))
}

fn set_vt(m: &mut Mem, this: u32, vt: u32) {
    m.w32(this.wrapping_add(4), vt);
}

/// One step of the game's quick random numbers: the product it keeps (`randtemp`).
fn fast_random_step(m: &mut Mem) -> u32 {
    let t = FAST_RANDOM.get(m).wrapping_mul(RAND_SEED.get(m));
    RAND_TEMP.set(m, t);
    FAST_RANDOM.set(m, t & 0xFFFF);
    t
}

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

/// The car has no target point.
fn clear_target(m: &mut Mem, car: Car) {
    let t = car.target_pos();
    t.set_z(m, 0);
    t.set_y(m, 0);
    t.set_x(m, 0);
    car.set_target_lat_pos(m, 0);
}

/// The ground distance between two cars: the larger axis plus a quarter of the smaller.
fn ground_distance(m: &mut Mem, a: Car, b: Car) -> i32 {
    let (p, q) = (a.n().position(), b.n().position());
    let dx = p.x(m).wrapping_sub(q.x(m)).wrapping_abs();
    let dz = p.z(m).wrapping_sub(q.z(m)).wrapping_abs();
    if dz < dx { dx.wrapping_add(dz >> 2) } else { dz.wrapping_add(dx >> 2) }
}

/// `AIState_StartUp` (AISTATE.CPP, 0x800701bc): the pool empty, nobody in purgatory.
pub fn start_up(m: &mut Mem) {
    for i in (0..10).rev() {
        m.w32(POOL_TAKEN + 4 * i, 0);
    }
    purgatory_start_up(m);
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(&mut c.m);
}

/// `AIState_Restart` (AISTATE.CPP, 0x800701fc): as the start-up.
pub fn abi_restart(c: &mut Cpu) {
    start_up(&mut c.m);
}

/// `AIState_CleanUp` (AISTATE.CPP, 0x8007023c): nothing.
pub fn abi_nothing(_: &mut Cpu) {}

/// `AIState_Base::StateExecute` (AISTATE.CPP, 0x80070244, each AI tick): an active AI car's
/// scripted reactions moved on (not while it is dead); its state's `Execute`.
pub fn state_execute(c: &mut Cpu, this: AIStateBase) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let car = this.car_obj_(m);
        if car.n().active(m) != 0 && car.car_flags(m) & CAR_HUMAN == 0 && car.n().dead_timer(m) == 0 {
            let elapsed = AI_ELAPSED_TIME.get(m);
            crate::aiscript::process_actions_and_reactions(m, car.script(), elapsed);
        }
        let (obj, f) = virtual_fn(&mut c.m, this.0, 1);
        r.call_ptr(c, f, &[obj], 0x8007_02bc);
    })
}

pub fn abi_state_execute(c: &mut Cpu) {
    state_execute(c, AIStateBase(c.r[4]));
}

/// `AIState_Base::AIState_Base(Car_tObj*)` (AISTATE.CPP, 0x800702cc).
pub fn base_construct(m: &mut Mem, this: AIStateBase, car: Car) -> AIStateBase {
    set_vt(m, this.0, VT_BASE);
    this.set_car_obj_(m, car);
    this
}

pub fn abi_base_construct(c: &mut Cpu) {
    c.r[2] = base_construct(&mut c.m, AIStateBase(c.r[4]), Car(c.r[5])).0;
}

/// `AIState_Base::operator new` (AISTATE.CPP, 0x800702e4; the retail build's pool): the first
/// free object, or none (0).
pub fn pool_new(m: &mut Mem) -> u32 {
    for i in 0..10 {
        if m.r32(POOL_TAKEN + 4 * i) == 0 {
            m.w32(POOL_TAKEN + 4 * i, 1);
            return POOL + 160 * i;
        }
    }
    0
}

pub fn abi_pool_new(c: &mut Cpu) {
    c.r[2] = pool_new(&mut c.m);
}

/// `AIState_Base::operator delete` (AISTATE.CPP, 0x80070330): the pool's object `p` free again.
pub fn pool_delete(m: &mut Mem, p: u32) {
    for i in 0..10 {
        if p == POOL + 160 * i {
            m.w32(POOL_TAKEN + 4 * i, 0);
            return;
        }
    }
}

pub fn abi_pool_delete(c: &mut Cpu) {
    pool_delete(&mut c.m, c.r[4]);
}

/// The AI's work on a car each tick (`AI_GenericBeginCycle`, `...Cycle`, `...EndCycle`), from
/// the call sites returning to `ra`, with the state's `then` between the first and the second.
fn generic_cycles(c: &mut Cpu, r: &Saved, car: Car, ra: [u32; 3], then: Option<(AIStateChase, u32)>) {
    r.enter(c, 0x8005_825c, ra[0], |c| crate::ai::generic_begin_cycle(c, car));
    if let Some((chase, ra)) = then {
        r.enter(c, 0x8007_14a4, ra, |c| check_for_barriers_and_target_around_them(c, chase));
    }
    r.enter(c, 0x8005_8284, ra[1], |c| crate::ai::generic_cycle(c, car));
    r.enter(c, 0x8005_82d8, ra[2], |c| crate::ai::generic_end_cycle(c, car));
}

fn calc_desired_speed(c: &mut Cpu, r: &Saved, car: Car, ra: u32) {
    r.enter(c, 0x8006_f5dc, ra, |c| crate::aispeeds::calc_desired_speed(c, car));
}

/// `AIState_Normal::Execute` (AISTATE.CPP, 0x8007036c): the car's wanted speed; the AI's
/// cycles.
pub fn normal_execute(c: &mut Cpu, this: AIStateBase) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let car = this.car_obj_(&mut c.m);
        calc_desired_speed(c, &r, car, 0x8007_0388);
        let car = this.car_obj_(&mut c.m);
        generic_cycles(c, &r, car, [0x8007_0394, 0x8007_03a0, 0x8007_03ac], None);
    })
}

pub fn abi_normal_execute(c: &mut Cpu) {
    normal_execute(c, AIStateBase(c.r[4]));
}

/// `AIState_Normal::AIState_Normal(Car_tObj*)` (AISTATE.CPP, 0x800703bc): no target point, the
/// car active.
pub fn normal_construct(m: &mut Mem, this: AIStateBase, car: Car) -> AIStateBase {
    base_construct(m, this, car);
    set_vt(m, this.0, VT_NORMAL);
    let car = this.car_obj_(m);
    clear_target(m, car);
    let car = this.car_obj_(m);
    car.n().set_active(m, 1);
    this
}

pub fn abi_normal_construct(c: &mut Cpu) {
    c.r[2] = normal_construct(&mut c.m, AIStateBase(c.r[4]), Car(c.r[5])).0;
}

/// `AIState_Idle::Execute` (AISTATE.CPP, 0x80070414): idling in place, still where it is; else
/// at a quarter of its wanted speed toward its idle position (within 2 m of it, still), there.
pub fn idle_execute(c: &mut Cpu, this: AIStateIdle) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let car = this.ai_state_base().car_obj_(m);
        if this.idle_in_place_flag_(m) != 0 {
            car.set_desired_speed(m, 0);
            let at = car.road_position(m);
            car.set_desired_lat_pos(m, at);
            return;
        }
        let off = car.road_position(m).wrapping_sub(this.road_position_(m)).wrapping_add(0xFFFF) as u32;
        if off > 0x1_FFFE {
            calc_desired_speed(c, &r, car, 0x8007_048c);
            let car = this.ai_state_base().car_obj_(&mut c.m);
            let v = car.desired_speed(&mut c.m) / 4;
            car.set_desired_speed(&mut c.m, v);
        } else {
            car.set_desired_speed(m, 0);
        }
        let m = &mut c.m;
        let at = this.road_position_(m);
        this.ai_state_base().car_obj_(m).set_desired_lat_pos(m, at);
    })
}

pub fn abi_idle_execute(c: &mut Cpu) {
    idle_execute(c, AIStateIdle(c.r[4]));
}

/// `AIState_Idle::SetIdlePosition` (AISTATE.CPP, 0x800704d0): idle toward `at` across the road.
pub fn set_idle_position(m: &mut Mem, this: AIStateIdle, at: i32) {
    this.set_road_position_(m, at);
    this.set_idle_in_place_flag_(m, 0);
}

pub fn abi_set_idle_position(c: &mut Cpu) {
    set_idle_position(&mut c.m, AIStateIdle(c.r[4]), c.r[5] as i32);
}

/// `AIState_Chase::AIState_Chase(Car_tObj*, Car_tObj*, coorddef*, int, int, int, int, int)`
/// (AISTATE.CPP, 0x800704dc): `car` chasing `target` (`rel` from it) seen with a delay
/// (`delay`), with `nitrous` ticks of nitrous used within `fore`/`aft` of the humans, at
/// `aggression`.
#[allow(clippy::too_many_arguments)]
pub fn chase_construct(c: &mut Cpu, this: AIStateChase, car: Car, target: Car, rel: Coord, nitrous: i32, fore: i32, aft: i32, aggression: i32, delay: i32) -> AIStateChase {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0 = [car.0, delay as u32, target.0, nitrous as u32, rel.0, fore as u32, aft as u32, aggression as u32, this.0];
        base_construct(&mut c.m, this.ai_state_base(), car);
        set_vt(&mut c.m, this.0, VT_CHASE);
        let d = this.delay_car_();
        r.enter(c, 0x8005_b218, 0x8007_0550, |c| crate::aidelaycar::construct(c, d, car, target, delay));
        this.set_nitrous_ticks_(&mut c.m, nitrous);
        r.enter(c, 0x8007_066c, 0x8007_0564, |c| set_target(c, this, target, rel));
        let m = &mut c.m;
        let car = this.ai_state_base().car_obj_(m);
        car.set_acc_nitrous(m, NO_NITROUS);
        car.set_speed_nitrous(m, NO_NITROUS);
        this.set_nitrous_min_fore_distance_(m, fore);
        this.set_nitrous_min_aft_distance_(m, aft);
        this.set_aggression_level_(m, aggression);
        this.set_slow_down_end_time_(m, 0);
        this.set_no_turn_around_end_time_(m, 0);
        this.set_barrier_ticks32_(m, 0);
        this
    })
}

pub fn abi_chase_construct(c: &mut Cpu) {
    let a: [u32; 5] = std::array::from_fn(|k| abi::arg(c, 4 + k as u32));
    c.r[2] = chase_construct(c, AIStateChase(c.r[4]), Car(c.r[5]), Car(c.r[6]), Coord(c.r[7]), a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32, a[4] as i32).0;
}

/// `AIState_Chase::~AIState_Chase` (AISTATE.CPP, 0x800705e4): no target point or preferred
/// position, no nitrous; freed with `flags` bit 0.
pub fn chase_destruct(m: &mut Mem, this: AIStateChase, flags: u32) {
    set_vt(m, this.0, VT_CHASE);
    let car = this.ai_state_base().car_obj_(m);
    clear_target(m, car);
    let car = this.ai_state_base().car_obj_(m);
    car.set_preferred_lateral_position(m, 0);
    car.set_preferred_lateral_position_power(m, 0);
    car.set_acc_nitrous(m, NO_NITROUS);
    car.set_speed_nitrous(m, NO_NITROUS);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_chase_destruct(c: &mut Cpu) {
    chase_destruct(&mut c.m, AIStateChase(c.r[4]), c.r[5]);
}

/// Which side of `half` (-1 below its negative, 1 above it, else 0) `v` is.
fn region(v: i32, half: i32) -> i32 {
    if v < half.wrapping_neg() {
        -1
    } else if half < v {
        1
    } else {
        0
    }
}

/// `AIState_Chase::SetTarget` (AISTATE.CPP, 0x8007066c): the target (the delayed copy following
/// a new one) and the point `rel` from it to make for; which side of the target's body that
/// is along and across; no murder mode; set up.
pub fn set_target(c: &mut Cpu, this: AIStateChase, target: Car, rel: Coord) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = target.0;
        r.0[2] = rel.0;
        if this.target_car_(&mut c.m) != target {
            let d = this.delay_car_();
            r.enter(c, 0x8005_b24c, 0x8007_06a0, |c| crate::aidelaycar::set_new_target_car(&mut c.m, d, target));
        }
        let m = &mut c.m;
        this.set_target_car_(m, target);
        let p = rel.get(m);
        this.rel_position_().set(m, p);
        this.set_long_target_region_(m, 0);
        this.set_lat_target_region_(m, 0);
        let t = this.target_car_(m);
        let (half, x) = (t.n().dimension().x(m), this.rel_position_().x(m));
        let lat = region(x, half);
        if lat != 0 {
            this.set_lat_target_region_(m, lat);
        }
        let t = this.target_car_(m);
        let (half, z) = (t.n().dimension().z(m), this.rel_position_().z(m));
        let long = region(z, half);
        if long != 0 {
            this.set_long_target_region_(m, long);
        }
        this.set_murder_mode_(m, 0);
        this.set_murder_end_time_(m, 0);
        this.set_in_target_region_(m, 0);
        r.enter(c, 0x8007_0778, 0x8007_0734, |c| set_up(c, this));
    })
}

pub fn abi_set_target(c: &mut Cpu) {
    set_target(c, AIStateChase(c.r[4]), Car(c.r[5]), Coord(c.r[6]));
}

/// `AIState_Chase::SetMurderMode` (AISTATE.CPP, 0x8007074c): unless already, `mode` for `ticks`.
pub fn set_murder_mode(m: &mut Mem, this: AIStateChase, mode: i32, ticks: i32) {
    if this.murder_mode_(m) == 0 {
        this.set_murder_mode_(m, mode);
        let end = SIM_COUNTER.get(m).wrapping_add(ticks);
        this.set_murder_end_time_(m, end);
    }
}

pub fn abi_set_murder_mode(c: &mut Cpu) {
    set_murder_mode(&mut c.m, AIStateChase(c.r[4]), c.r[5] as i32, c.r[6] as i32);
}

/// `AIState_Chase::SetUp` (AISTATE.CPP, 0x80070778, each tick): the delayed target moved on;
/// the car's and the target's directions, the gaps along the road (to the delayed target's
/// point, kept at sp+16) and across; no target point; murder mode over at its time; the
/// nitrous used counted down; no nitrous or preferred position yet.
pub fn set_up(c: &mut Cpu, this: AIStateChase) {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let d = this.delay_car_();
        r.0[1] = d.0;
        r.enter(c, 0x8005_b334, 0x8007_0798, |c| crate::aidelaycar::update(c, d));
        let m = &mut c.m;
        let car = this.ai_state_base().car_obj_(m);
        let dir = if car.current_speed(m) < 0 { -1 } else { 1 };
        this.set_car_dir_(m, dir);
        let dir = if d.current_speed_(m) < 0 { -1 } else { 1 };
        this.set_target_dir_(m, dir);
        let across = car.road_position(m).wrapping_sub(d.road_position_(m));
        this.set_lat_meters_between_(m, across);
        let p = d.position_().get(m);
        Coord(sp + 16).set(m, p);
        let slice = d.slice_(m);
        let along = r.enter(c, 0x8007_3b5c, 0x8007_0810, |c| crate::aiworld::spline_distance_to_point(c, car, slice, Coord(sp + 16)));
        let m = &mut c.m;
        this.set_long_meters_between_(m, along);
        clear_target(m, car);
        if this.murder_mode_(m) != 0 && SIM_COUNTER.get(m) >= this.murder_end_time_(m) {
            this.set_murder_mode_(m, 0);
            this.set_murder_end_time_(m, 0);
        }
        let car = this.ai_state_base().car_obj_(m);
        if car.acc_nitrous(m) != NO_NITROUS {
            let left = this.nitrous_ticks_(m).wrapping_sub(AI_ELAPSED_TIME.get(m));
            this.set_nitrous_ticks_(m, left);
        }
        car.set_acc_nitrous(m, NO_NITROUS);
        car.set_speed_nitrous(m, NO_NITROUS);
        car.set_preferred_lateral_position(m, 0);
        car.set_preferred_lateral_position_power(m, 0);
    })
}

pub fn abi_set_up(c: &mut Cpu) {
    set_up(c, AIStateChase(c.r[4]));
}

/// `AIState_Chase::DoSlowNitrous` (AISTATE.CPP, 0x800708d4): slowed down (half acceleration,
/// three quarters of the top speed).
pub fn do_slow_nitrous(m: &mut Mem, this: AIStateChase) {
    let car = this.ai_state_base().car_obj_(m);
    car.set_acc_nitrous(m, 0x8000);
    car.set_speed_nitrous(m, 0xC000);
}

pub fn abi_do_slow_nitrous(c: &mut Cpu) {
    do_slow_nitrous(&mut c.m, AIStateChase(c.r[4]));
}

/// `AIState_Chase::DoNitrous` (AISTATE.CPP, 0x800708f0): with nitrous left and not slowed down,
/// three times the acceleration and 2.5 times the top speed; with `check`, none while a human
/// is within its fore distance ahead or aft distance behind.
pub fn do_nitrous(c: &mut Cpu, this: AIStateChase, check: bool) {
    abi::frame(c, 40, |c, _| {
        let m = &mut c.m;
        if this.nitrous_ticks_(m) <= 0 || SIM_COUNTER.get(m) < this.slow_down_end_time_(m) {
            return;
        }
        let car = this.ai_state_base().car_obj_(m);
        car.set_acc_nitrous(m, 0x3_0000);
        car.set_speed_nitrous(m, 0x2_8000);
        if !check {
            return;
        }
        let mut i = 0;
        while i < NUM_HUMAN_RACE_CARS.get(m) {
            let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
            let car = this.ai_state_base().car_obj_(m);
            let ahead = apx_spline_distance(m, car, human).wrapping_mul(car.direction(m));
            if ahead > 0 && ahead < this.nitrous_min_fore_distance_(m) {
                car.set_acc_nitrous(m, NO_NITROUS);
                this.ai_state_base().car_obj_(m).set_speed_nitrous(m, NO_NITROUS);
            }
            if ahead < 0 && this.nitrous_min_aft_distance_(m).wrapping_neg() < ahead {
                let car = this.ai_state_base().car_obj_(m);
                car.set_acc_nitrous(m, NO_NITROUS);
                this.ai_state_base().car_obj_(m).set_speed_nitrous(m, NO_NITROUS);
            }
            i += 1;
        }
    })
}

pub fn abi_do_nitrous(c: &mut Cpu) {
    do_nitrous(c, AIStateChase(c.r[4]), c.r[5] != 0);
}

/// `AIState_Chase::Execute` (AISTATE.CPP, 0x80070a3c, each tick): set up; far off and much
/// faster or slower than the target, far targeting; going the target's way behind a target
/// that isn't nearly stopped, close targeting; else approaching (following it near, slowly);
/// slowed down until its time.
pub fn chase_execute(c: &mut Cpu, this: AIStateChase) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8007_0778, 0x8007_0a50, |c| set_up(c, this));
        let m = &mut c.m;
        let long = this.long_meters_between_(m);
        let near = fixedmult(long, 1638).wrapping_abs();
        let (car, target) = (this.ai_state_base().car_obj_(m), this.target_car_(m));
        let target_speed = target.current_speed(m);
        let closing = car.current_speed(m).wrapping_sub(target_speed).wrapping_abs();
        let far = closing < near && 0x8C_0000 < long.wrapping_abs();
        let stopped = target_speed.wrapping_abs() <= 0x6_AAA9;
        enum Next {
            Far,
            Close,
            Approach(bool),
        }
        let next = if far {
            Next::Far
        } else {
            let (car_dir, target_dir) = (this.car_dir_(m), this.target_dir_(m));
            if car_dir.wrapping_mul(target_dir) == 1 && !stopped {
                Next::Close
            } else if car_dir.wrapping_mul(long) >= 0 && !stopped {
                Next::Far
            } else if 0x1A_AAAA < closing || far {
                Next::Approach(false)
            } else {
                Next::Approach(long.wrapping_abs() <= 0x50_0000)
            }
        };
        match next {
            Next::Far => r.enter(c, 0x8007_0bb0, 0x8007_0b2c, |c| far_targeting(c, this)),
            Next::Close => r.enter(c, 0x8007_0ce8, 0x8007_0afc, |c| close_targeting(c, this)),
            Next::Approach(follow) => r.enter(c, 0x8007_1198, 0x8007_0b7c, |c| approach_targeting(c, this, follow)),
        }
        let m = &mut c.m;
        if SIM_COUNTER.get(m) < this.slow_down_end_time_(m) {
            do_slow_nitrous(m, this);
        }
    })
}

pub fn abi_chase_execute(c: &mut Cpu) {
    chase_execute(c, AIStateChase(c.r[4]));
}

/// After its no-turn-around time, the car turned to face the target (back when it is ahead).
fn face_target(m: &mut Mem, this: AIStateChase) {
    this.set_in_target_region_(m, 0);
    if this.no_turn_around_end_time_(m) < SIM_COUNTER.get(m) {
        let dir = if this.long_meters_between_(m) > 0 { -1 } else { 1 };
        this.ai_state_base().car_obj_(m).set_desired_direction(m, dir);
    }
}

/// `AIState_Chase::FarTargeting` (AISTATE.CPP, 0x80070bb0): facing the target, nitrous (away
/// from the humans), its wanted speed; the cycles, around barriers.
pub fn far_targeting(c: &mut Cpu, this: AIStateChase) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        face_target(&mut c.m, this);
        r.enter(c, 0x8007_08f0, 0x8007_0c14, |c| do_nitrous(c, this, true));
        let car = this.ai_state_base().car_obj_(&mut c.m);
        calc_desired_speed(c, &r, car, 0x8007_0c20);
        let car = this.ai_state_base().car_obj_(&mut c.m);
        generic_cycles(c, &r, car, [0x8007_0c2c, 0x8007_0c40, 0x8007_0c4c], Some((this, 0x8007_0c34)));
    })
}

pub fn abi_far_targeting(c: &mut Cpu) {
    far_targeting(c, AIStateChase(c.r[4]));
}

/// `AIState_Chase::CalculateCloseTargettingAheadSlowDownFactor` (AISTATE.CPP, 0x80070c5c): the
/// share of the target's speed by the gap ahead (95% within 30 m, down to 60% past 200 m).
pub fn close_targeting_ahead_slow_down_factor(m: &mut Mem, this: AIStateChase) -> i32 {
    let gap = this.long_meters_between_(m).wrapping_abs();
    if gap <= 0x1D_FFFF {
        0xF333
    } else if gap <= 0x63_FFFF {
        0xCCCC
    } else if gap <= 0x95_FFFF {
        0xC000
    } else if gap <= 0xC7_FFFF {
        0xB333
    } else {
        0x9999
    }
}

pub fn abi_close_targeting_ahead_slow_down_factor(c: &mut Cpu) {
    c.r[2] = close_targeting_ahead_slow_down_factor(&mut c.m, AIStateChase(c.r[4])) as u32;
}

/// `AIState_Chase::CloseTargeting` (AISTATE.CPP, 0x80070ce8): near the target going its way.
/// Which side of the target's body the car is (across, and along with 2 m to spare; in murder
/// mode the target's centre, more nitrous); the time in the target region counted; the speed
/// from the delayed target's: faster when behind where it should be, slower ahead (by the gap,
/// or by aggression when about to pass alongside); then lined up across with the target point
/// (in murder mode its road position; else beside the target to pass), within the road.
pub fn close_targeting(c: &mut Cpu, this: AIStateChase) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        r.0[4] = 0;
        r.0[5] = 0;
        let dir = this.ai_state_base().car_obj_(&mut c.m).direction(&mut c.m);
        r.0[8] = dir as u32;
        r.enter(c, 0x8007_08f0, 0x8007_0d34, |c| do_nitrous(c, this, true));
        r.0[0] = 0;
        let (want_long, want_lat) = if this.murder_mode_(&mut c.m) != 0 {
            r.0[2] = 0;
            r.0[3] = 0;
            r.enter(c, 0x8007_08f0, 0x8007_0d58, |c| do_nitrous(c, this, false));
            (0, 0)
        } else {
            let m = &mut c.m;
            (this.long_target_region_(m), this.lat_target_region_(m))
        };
        r.0[2] = want_long as u32;
        r.0[3] = want_lat as u32;
        let m = &mut c.m;
        let target = this.target_car_(m);
        let lat = region(this.lat_meters_between_(m).wrapping_mul(dir), target.n().dimension().x(m));
        let along = this.long_meters_between_(m).wrapping_mul(dir);
        let half = target.n().dimension().z(m);
        let long = if along < 0x2_0000i32.wrapping_sub(half) {
            -1
        } else if half.wrapping_add(0x2_0000) < along {
            1
        } else {
            0
        };
        let wide = region(along, half.wrapping_add(0x2_0000));
        if lat == want_lat && long == want_long {
            let t = this.in_target_region_(m).wrapping_add(AI_ELAPSED_TIME.get(m));
            this.set_in_target_region_(m, t);
        } else {
            this.set_in_target_region_(m, 0);
        }
        // (`pass`: which side to pass the target on, 0 none; `slow`: -1 hold back, -2 by
        // aggression; `line_up`: line up across)
        let (mut slow, mut pass, mut line_up) = (0, 0, true);
        if wide.wrapping_mul(want_long) == -1 {
            pass = if want_lat != 0 { want_lat } else { -1 };
        } else if lat.wrapping_mul(want_lat) == -1 && wide == 0 {
            slow = -1;
            line_up = false;
        } else if want_lat == 0 && wide == 0 && this.murder_mode_(m) == 0 {
            line_up = false;
        } else if want_long == 1 && wide == want_long && lat == 0 && this.long_meters_between_(m) <= 0x13_FFFF {
            slow = -2;
        }
        r.0[4] = lat as u32;
        r.0[5] = long as u32;
        r.0[6] = pass as u32;
        r.0[7] = line_up as u32;
        let base = this.delay_car_().current_speed_(m);
        let base = if base > 0 { base } else { base.wrapping_neg() };
        let speed = if long < want_long || slow == 1 {
            Some(fixedmult(base, 0x1_6666))
        } else if want_long < long || slow == -1 {
            Some(fixedmult(base, close_targeting_ahead_slow_down_factor(m, this)))
        } else if 0x14_0000 < this.long_meters_between_(m) {
            Some(fixedmult(base, close_targeting_ahead_slow_down_factor(m, this)))
        } else if slow == -2 {
            let k = match this.aggression_level_(m) {
                0 => 0xCA3D,
                1 => 0xBAE1,
                _ => 0xA666,
            };
            Some(fixedmult(base, k))
        } else {
            None
        };
        let speed = speed.unwrap_or(base);
        r.0[0] = speed as u32;
        let car = this.ai_state_base().car_obj_(m);
        calc_desired_speed(c, &r, car, 0x8007_0ff0);
        let m = &mut c.m;
        let car = this.ai_state_base().car_obj_(m);
        let want = car.desired_speed(m).wrapping_abs();
        car.set_desired_speed(m, want);
        let speed = speed.min(car.desired_speed(m)).max(0x5_0000).wrapping_mul(dir);
        car.set_desired_speed(m, speed);
        r.0[0] = speed as u32;
        r.enter(c, 0x8005_825c, 0x8007_1064, |c| crate::ai::generic_begin_cycle(c, car));
        if line_up {
            let m = &mut c.m;
            let d = this.delay_car_();
            let (at, power) = if this.murder_mode_(m) != 0 {
                (d.road_position_(m), 0x1E_0000)
            } else {
                let off = if pass != 0 { this.target_car_(m).n().dimension().x(m).wrapping_add(0x6_0000) } else { this.rel_position_().x(m) };
                let at = d.road_position_(m).wrapping_add(off.wrapping_mul(dir));
                let here = car.n().sim_road_info().slice(m) as i32;
                let s = slice_at(m, here);
                let lanes = s.lane_count(m) as i32;
                let left = ((s.avg_paved_width_lf(m) as i32) << 15).wrapping_mul(lanes >> 4).wrapping_neg();
                let right = ((s.avg_paved_width_rt(m) as i32) << 15).wrapping_mul(lanes & 0xF);
                (at.max(left).min(right), 0xF_0000)
            };
            car.set_preferred_lateral_position(m, at);
            car.set_preferred_lateral_position_power(m, power);
        }
        r.enter(c, 0x8007_14a4, 0x8007_1150, |c| check_for_barriers_and_target_around_them(c, this));
        r.enter(c, 0x8005_8284, 0x8007_115c, |c| crate::ai::generic_cycle(c, car));
        r.enter(c, 0x8005_82d8, 0x8007_1168, |c| crate::ai::generic_end_cycle(c, car));
    })
}

pub fn abi_close_targeting(c: &mut Cpu) {
    close_targeting(c, AIStateChase(c.r[4]));
}

/// `AIState_Chase::ApproachTargeting` (AISTATE.CPP, 0x80071198): facing the target, its wanted
/// speed held to a limit by the distance to it and the aggression; the cycles, around
/// barriers; with `follow`, making for the target (its position and road position).
pub fn approach_targeting(c: &mut Cpu, this: AIStateChase, follow: bool) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        r.0[2] = follow as u32;
        let m = &mut c.m;
        let (car, target) = (this.ai_state_base().car_obj_(m), this.target_car_(m));
        let dist = ground_distance(m, car, target);
        r.0[0] = dist as u32;
        face_target(m, this);
        calc_desired_speed(c, &r, car, 0x8007_1260);
        let m = &mut c.m;
        // (the limits beyond 150, 100, 50, 25, 10 and 6 m)
        let steps: [i32; 6] = [0x96_0000, 0x64_0000, 0x32_0000, 0x19_0000, 0xA_0000, 0x6_0000];
        let limits: Option<[i32; 6]> = match this.aggression_level_(m) {
            0 => Some([0x3C_0000, 0x32_0000, 0x28_0000, 0x23_0000, 0x14_0000, 0x3_0000]),
            1 => Some([0x3C_0000, 0x32_0000, 0x28_0000, 0x23_0000, 0xA_0000, 0x3_0000]),
            2 => Some([0x50_0000, 0x46_0000, 0x32_0000, 0x28_0000, 0xE_0000, 0x6_0000]),
            _ => None,
        };
        let limit = match limits {
            None => 0x19_0000,
            Some(l) => steps.iter().position(|&s| s < dist).map_or(0, |k| l[k]),
        };
        let car = this.ai_state_base().car_obj_(m);
        let want = car.desired_speed(m);
        let want = if car.direction(m) == 1 { want.min(limit) } else { want.max(limit.wrapping_neg()) };
        car.set_desired_speed(m, want);
        let car = this.ai_state_base().car_obj_(m);
        generic_cycles(c, &r, car, [0x8007_140c, 0x8007_1420, 0x8007_142c], Some((this, 0x8007_1414)));
        if follow {
            let m = &mut c.m;
            let (car, target) = (this.ai_state_base().car_obj_(m), this.target_car_(m));
            let p = target.n().position().get(m);
            car.target_pos().set(m, p);
            let at = target.road_position(m);
            car.set_target_lat_pos(m, at);
            let at = this.target_car_(m).road_position(m);
            car.set_preferred_lateral_position(m, at);
            car.set_preferred_lateral_position_power(m, 0xF_0000);
        }
    })
}

pub fn abi_approach_targeting(c: &mut Cpu) {
    approach_targeting(c, AIStateChase(c.r[4]), c.r[5] != 0);
}

/// Whether `lane` is one of slice `s`'s (6 is the first right of the centre).
fn lane_on_road(m: &mut Mem, s: i32, lane: i32) -> bool {
    let lanes = slice_at(m, s).lane_count(m) as i32;
    lane >= 6 - (lanes >> 4) && lane <= (lanes & 0xF) + 7
}

/// `AIState_Chase::CheckForBarriersAndTargetAroundThem` (AISTATE.CPP, 0x800714a4): within
/// 500 m of the target, both in lanes of the road, with a barrier between the target's lane and
/// the car's at the target's slice: none at the car's, its lanes made for the target's (the
/// barrier time reset); else the barrier time counted (none: reset).
pub fn check_for_barriers_and_target_around_them(c: &mut Cpu, this: AIStateChase) {
    abi::frame(c, 40, |c, _| {
        let m = &mut c.m;
        if this.long_meters_between_(m).wrapping_abs() > 0x1F4_0000 {
            return;
        }
        let (car, target) = (this.ai_state_base().car_obj_(m), this.target_car_(m));
        let (slice, lane) = (car.n().sim_road_info().slice(m) as i32, car.lane_index(m));
        let (target_slice, target_lane) = (target.n().sim_road_info().slice(m) as i32, target.lane_index(m));
        if !lane_on_road(m, slice, lane) || !lane_on_road(m, target_slice, target_lane) {
            return;
        }
        let there = check_for_barrier_between_lanes(m, target_slice, target_lane, lane);
        let here = check_for_barrier_between_lanes(m, slice, target_lane, lane);
        if there == 0 {
            this.set_barrier_ticks32_(m, 0);
        } else if here == 0 {
            this.set_barrier_ticks32_(m, 0);
            for _ in 0..3 {
                let car = this.ai_state_base().car_obj_(m);
                crate::ai::target_lane(m, car, target_lane);
            }
        } else {
            let t = this.barrier_ticks32_(m).wrapping_add(AI_ELAPSED_TIME.get(m));
            this.set_barrier_ticks32_(m, t);
        }
    })
}

pub fn abi_check_for_barriers_and_target_around_them(c: &mut Cpu) {
    check_for_barriers_and_target_around_them(c, AIStateChase(c.r[4]));
}

/// The slice `by` on from `s`, round the track (the wrap decided by `by`'s sign).
fn slice_on(m: &mut Mem, s: i32, by: i32) -> i32 {
    let t = s.wrapping_add(by);
    if by >= 0 {
        if t < NUM_SLICES.get(m) { t } else { t.wrapping_sub(NUM_SLICES.get(m)) }
    } else if t >= 0 {
        t
    } else {
        t.wrapping_add(NUM_SLICES.get(m))
    }
}

/// `AIState_Chase::FindBarrierEndSlice` (AISTATE.CPP, 0x800715f4): where the barrier between
/// the car's lane and the target's ends: from the first lane on the car's side with a barrier,
/// along the road each way (every 4th slice up to 83) following the barrier's side while it
/// stays on the road; the nearer end, 6 slices past it (the distances kept at sp+16, sp+20).
pub fn find_barrier_end_slice(c: &mut Cpu, this: AIStateChase) -> i32 {
    abi::frame(c, 64, |c, sp| {
        let m = &mut c.m;
        let (car, target) = (this.ai_state_base().car_obj_(m), this.target_car_(m));
        let (own, theirs) = (car.lane_index(m), target.lane_index(m));
        let from = car.n().sim_road_info().slice(m) as i32;
        let (low, high) = if theirs < own { (theirs, own) } else { (own, theirs) };
        let mut lane = low;
        while lane < high {
            if check_for_barrier_between_lanes(m, from, lane, lane) != 0 {
                break;
            }
            lane += 1;
        }
        m.w32(sp + 20, 83);
        m.w32(sp + 16, 83);
        let mut ahead = from.wrapping_add(83);
        if ahead >= NUM_SLICES.get(m) {
            ahead = ahead.wrapping_sub(NUM_SLICES.get(m));
        }
        let mut behind = from.wrapping_sub(83);
        if behind < 0 {
            behind = behind.wrapping_add(NUM_SLICES.get(m));
        }
        // (one way: the end slice found and the distance kept at `keep`)
        let scan = |m: &mut Mem, sign: i32, keep: u32| -> Option<i32> {
            let mut l = lane;
            let mut k: i32 = 0;
            while k < 83 {
                let s = slice_on(m, from, k.wrapping_mul(sign));
                let found = check_for_barrier_between_lanes(m, s, l + 1, l - 1);
                if found & 2 == 0 {
                    if found & 4 != 0 && lane_on_road(m, s, l - 1) {
                        l -= 1;
                    } else if found & 1 == 0 || !lane_on_road(m, s, l + 1) {
                        m.w32(sp + keep, k as u32);
                        return Some(s);
                    } else {
                        l += 1;
                    }
                }
                k += 4;
            }
            None
        };
        if let Some(s) = scan(m, 1, 16) {
            ahead = s;
        }
        if let Some(s) = scan(m, -1, 20) {
            behind = s;
        }
        let (fore, aft) = (m.r32(sp + 16) as i32, m.r32(sp + 20) as i32);
        if fore < aft {
            let s = ahead.wrapping_add(6);
            if s < NUM_SLICES.get(m) { s } else { ahead.wrapping_sub(NUM_SLICES.get(m).wrapping_sub(6)) }
        } else {
            let s = behind.wrapping_sub(6);
            if s >= 0 { s } else { behind.wrapping_add(NUM_SLICES.get(m).wrapping_sub(6)) }
        }
    })
}

pub fn abi_find_barrier_end_slice(c: &mut Cpu) {
    c.r[2] = find_barrier_end_slice(c, AIStateChase(c.r[4])) as u32;
}

/// `AIState_Offroad::~AIState_Offroad` (AISTATE.CPP, 0x8007198c): no target point, not off the
/// road; freed with `flags` bit 0.
pub fn offroad_destruct(m: &mut Mem, this: AIStateOffroad, flags: u32) {
    set_vt(m, this.0, VT_OFFROAD);
    let car = this.ai_state_base().car_obj_(m);
    clear_target(m, car);
    let car = this.ai_state_base().car_obj_(m);
    car.update_car_flags(m, |f| f & !CAR_OFFROAD);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_offroad_destruct(c: &mut Cpu) {
    offroad_destruct(&mut c.m, AIStateOffroad(c.r[4]), c.r[5]);
}

/// `AIState_Offroad::AIState_Offroad(Car_tObj*, int, coorddef*, matrixtdef*, int, int, int)`
/// (AISTATE.CPP, 0x80071a00): `car` waiting off the road at `pos`, `mat` (at `slice`), to be let
/// go toward `target_slice` at `mph`, within the distance its target covers in `release`.
#[allow(clippy::too_many_arguments)]
pub fn offroad_construct(m: &mut Mem, this: AIStateOffroad, car: Car, slice: i32, pos: Coord, mat: Matrix, mph: i32, release: i32, target_slice: i32) -> AIStateOffroad {
    base_construct(m, this.ai_state_base(), car);
    set_vt(m, this.0, VT_OFFROAD);
    this.set_let_go_(m, 0);
    this.set_start_slice_(m, slice);
    let p = pos.get(m);
    this.start_position_().set(m, p);
    for k in 0..9 {
        let v = m.r32(mat.0 + 4 * k);
        m.w32(this.start_orientation_().0 + 4 * k, v);
    }
    let heading = Coord(this.start_orientation_().0 + 24).get(m);
    this.start_heading_().set(m, heading);
    this.set_max_speed_mps_(m, mph.wrapping_mul(0x4700));
    this.set_target_slice_(m, target_slice);
    this.set_release_time_(m, release);
    let center = Coord(slice_at(m, target_slice).0).get(m);
    this.target_position_().set(m, center);
    let car = this.ai_state_base().car_obj_(m);
    car.update_car_flags(m, |f| f | CAR_OFFROAD);
    this
}

pub fn abi_offroad_construct(c: &mut Cpu) {
    let a: [u32; 4] = std::array::from_fn(|k| abi::arg(c, 4 + k as u32));
    c.r[2] = offroad_construct(&mut c.m, AIStateOffroad(c.r[4]), Car(c.r[5]), c.r[6] as i32, Coord(c.r[7]), Matrix(a[0]), a[1] as i32, a[2] as i32, a[3] as i32).0;
}

/// `AIState_Offroad::UnleashIfInRange` (AISTATE.CPP, 0x80071b48): let go when `other` is within
/// what it covers in the release time (20 m at least) along the road.
pub fn unleash_if_in_range(c: &mut Cpu, this: AIStateOffroad, other: Car) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = other.0;
        r.0[1] = this.0;
        let car = this.ai_state_base().car_obj_(&mut c.m);
        let d = r.enter(c, 0x8007_3b04, 0x8007_1b68, |c| crate::aiworld::spline_distance(c, car, other)).wrapping_abs();
        let m = &mut c.m;
        let reach = fixedmult(other.n().speed_xz(m), this.release_time_(m)).max(0x14_0000);
        if d < reach {
            this.set_let_go_(m, 1);
        }
    })
}

pub fn abi_unleash_if_in_range(c: &mut Cpu) {
    unleash_if_in_range(c, AIStateOffroad(c.r[4]), Car(c.r[5]));
}

/// `AIState_Offroad::Execute` (AISTATE.CPP, 0x80071bc0, each tick): until let go, held still at
/// its start (the zero vector made at sp+16); then driven at its speed toward its target
/// slice's centre; no target across the road.
pub fn offroad_execute(c: &mut Cpu, this: AIStateOffroad) {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        if this.let_go_(&mut c.m) == 0 {
            r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8007_1bec); // memset
            let m = &mut c.m;
            let car = this.ai_state_base().car_obj_(m);
            let zero = Coord(sp + 16).get(m);
            car.n().linear_vel().set(m, zero);
            let zero = Coord(sp + 16).get(m);
            car.n().angular_vel().set(m, zero);
            car.set_desired_speed(m, 0);
            let p = this.start_position_().get(m);
            car.n().position().set(m, p);
            for k in 0..9 {
                let v = m.r32(this.start_orientation_().0 + 4 * k);
                m.w32(car.n().orient_mat().0 + 4 * k, v);
            }
            let t = car.target_pos();
            t.set_z(m, 0);
            t.set_y(m, 0);
            t.set_x(m, 0);
        } else {
            let m = &mut c.m;
            let car = this.ai_state_base().car_obj_(m);
            let target_slice = this.target_slice_(m);
            let d = apx_spline_distance_to_slice(m, car, target_slice);
            this.set_long_meters_between_(m, d);
            let speed = this.max_speed_mps_(m);
            car.set_desired_speed(m, speed);
            if this.long_meters_between_(m) > 0 {
                car.set_desired_direction(m, -1);
                let v = car.desired_speed(m).wrapping_neg();
                car.set_desired_speed(m, v);
            } else {
                car.set_desired_direction(m, 1);
            }
            generic_cycles(c, &r, car, [0x8007_1d1c, 0x8007_1d28, 0x8007_1d34], None);
            let m = &mut c.m;
            let p = this.target_position_().get(m);
            this.ai_state_base().car_obj_(m).target_pos().set(m, p);
        }
        let m = &mut c.m;
        this.ai_state_base().car_obj_(m).set_target_lat_pos(m, 0);
    })
}

pub fn abi_offroad_execute(c: &mut Cpu) {
    offroad_execute(c, AIStateOffroad(c.r[4]));
}

/// `AIState_Purgatory::AIState_Purgatory(Car_tObj*)` (AISTATE.CPP, 0x80071d6c): the car parked
/// inactive off the track (at slice 0, 10 m apart by its index: the offset at sp+16), the first
/// car its basis, for a random time by the traffic's size (five times as long for a bus); a
/// traffic car counted.
pub fn purgatory_construct(c: &mut Cpu, this: AIStateBase, car: Car) -> AIStateBase {
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.0[1] = this.0;
        base_construct(&mut c.m, this, car);
        set_vt(&mut c.m, this.0, VT_NON_ACTIVE);
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8007_1da4); // memset
        let m = &mut c.m;
        let ahead = car.car_index(m).wrapping_mul(5) << 17;
        m.w32(sp + 20, ahead as u32);
        let obj = this.car_obj_(m).n();
        r.enter(c, 0x800a_22a8, 0x8007_1dcc, |c| crate::newton::set_initial_slice_position_orientation_etc(c, obj, 0, Coord(sp + 16), 1));
        let m = &mut c.m;
        this.car_obj_(m).n().set_active(m, 0);
        set_vt(m, this.0, VT_PURGATORY);
        let t = FAST_RANDOM.get(m).wrapping_mul(RAND_SEED.get(m));
        let first = Car(m.r32(CARS_LIST));
        this.car_obj_(m).set_basis_car(m, first);
        let traffic = NUM_TRAFFIC_CARS.get(m) as u32;
        let life = m.r32(LIFE_TIMER.wrapping_add(traffic.wrapping_mul(4)));
        RAND_TEMP.set(m, t);
        let ticks = (life.wrapping_mul((t >> 8) & 0xFFFF) >> 16) as i32 + 1;
        this.car_obj_(m).set_physics_model_timer(m, ticks);
        FAST_RANDOM.set(m, t & 0xFFFF);
        r.0[0] = 0;
        let name = this.car_obj_(&mut c.m).0.wrapping_add(0x240);
        let mut bus = r.call(c, nfs4rc::f_800e6968, &[name, SCHOOL_BUS], 0x8007_1e78) == 0; // strcmp
        if !bus {
            let name = this.car_obj_(&mut c.m).0.wrapping_add(0x240);
            bus = r.call(c, nfs4rc::f_800e6968, &[name, TOUR_BUS], 0x8007_1e90) == 0;
        }
        let m = &mut c.m;
        if bus {
            let car = this.car_obj_(m);
            let v = car.physics_model_timer(m).wrapping_mul(5);
            car.set_physics_model_timer(m, v);
        }
        let car = this.car_obj_(m);
        car.update_ai_flags(m, |f| f | AI_PARKED);
        if this.car_obj_(m).car_flags(m) & CAR_TRAFFIC != 0 {
            let n = m.r32(NUM_IN_PURGATORY).wrapping_add(1);
            m.w32(NUM_IN_PURGATORY, n);
        }
        this
    })
}

pub fn abi_purgatory_construct(c: &mut Cpu) {
    c.r[2] = purgatory_construct(c, AIStateBase(c.r[4]), Car(c.r[5])).0;
}

/// `AIState_Purgatory::~AIState_Purgatory` (AISTATE.CPP, 0x80071f1c): out of purgatory: no
/// collision, not parked (a traffic car uncounted), going forward, its basis the last basis car
/// in the race order, active; freed with `flags` bit 0.
pub fn purgatory_destruct(m: &mut Mem, this: AIStateBase, flags: u32) {
    set_vt(m, this.0, VT_PURGATORY);
    let car = this.car_obj_(m);
    m.w32(car.collision().0, 0);
    car.n().collision().set_disable_collision_timer(m, 0);
    car.update_ai_flags(m, |f| f & !AI_PARKED);
    if car.car_flags(m) & CAR_TRAFFIC != 0 {
        let n = m.r32(NUM_IN_PURGATORY).wrapping_sub(1);
        m.w32(NUM_IN_PURGATORY, n);
    }
    car.set_direction(m, 1);
    car.set_desired_direction(m, 1);
    let mut i = NUM_CARS.get(m).wrapping_sub(1);
    while i >= 0 {
        let other = Car(m.r32(SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4))));
        if other.car_flags(m) & CAR_LIFE_BASIS != 0 {
            car.set_basis_car(m, other);
            break;
        }
        i -= 1;
    }
    set_vt(m, this.0, VT_NON_ACTIVE);
    car.n().set_active(m, 1);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_purgatory_destruct(c: &mut Cpu) {
    purgatory_destruct(&mut c.m, AIStateBase(c.r[4]), c.r[5]);
}

/// `AIState_Purgatory::TestForRelease` (AISTATE.CPP, 0x8007203c): its time up and fewer traffic
/// cars living than the density allows (another tick to wait otherwise).
pub fn purgatory_test_for_release(m: &mut Mem, this: AIStateBase) -> bool {
    let car = this.car_obj_(m);
    if car.physics_model_timer(m) > 0 {
        return false;
    }
    let living = NUM_TRAFFIC_CARS.get(m).wrapping_sub(m.r32(NUM_IN_PURGATORY) as i32);
    let mut k = (GAME_SETUP.traffic_density(m) as u32).wrapping_mul(4);
    if GAME_SETUP.comm_mode(m) == 1 {
        k = k.wrapping_add(16);
    }
    if living < m.r32(MAX_TRAFFIC.wrapping_add(k)) as i32 {
        return true;
    }
    car.set_physics_model_timer(m, 1);
    false
}

pub fn abi_purgatory_test_for_release(c: &mut Cpu) {
    c.r[2] = purgatory_test_for_release(&mut c.m, AIStateBase(c.r[4])) as u32;
}

/// `AIState_Purgatory::Execute` (AISTATE.CPP, 0x800720b8, each tick): no collision; its time
/// counted down once the race has run 7.5 s (not before the car is simulated).
pub fn purgatory_execute(m: &mut Mem, this: AIStateBase) {
    let car = this.car_obj_(m);
    m.w32(car.collision().0, 0);
    car.n().collision().set_disable_collision_timer(m, 0);
    if car.car_flags(m) & CAR_NOT_STARTED == 0 && SIM_COUNTER.get(m) >= 960 {
        let v = car.physics_model_timer(m).wrapping_sub(AI_ELAPSED_TIME.get(m));
        car.set_physics_model_timer(m, v);
    }
}

pub fn abi_purgatory_execute(c: &mut Cpu) {
    purgatory_execute(&mut c.m, AIStateBase(c.r[4]));
}

/// `AIState_Purgatory::StartUp` (AISTATE.CPP, 0x80072120): nobody in purgatory.
pub fn purgatory_start_up(m: &mut Mem) {
    m.w32(NUM_IN_PURGATORY, 0);
}

pub fn abi_purgatory_start_up(c: &mut Cpu) {
    purgatory_start_up(&mut c.m);
}

/// `AIState_RovingTraffic::AIState_RovingTraffic(Car_tObj*, trigger_t*)` (AISTATE.CPP,
/// 0x8007212c): `car` driving the traffic path of `trigger` from its first point.
pub fn roving_traffic_construct(m: &mut Mem, this: AIStateRovingTraffic, car: Car, trigger: Trigger) -> AIStateRovingTraffic {
    base_construct(m, this.ai_state_base(), car);
    set_vt(m, this.0, VT_ROVING_TRAFFIC);
    let path = trigger.traffic_path();
    let points = path.path(m);
    m.w32(this.0 + 8, points);
    this.set_path_index_(m, 0);
    this.set_wait_tick_(m, 0);
    let n = path.num_points(m);
    this.set_num_path_points_(m, n);
    let car = this.ai_state_base().car_obj_(m);
    car.update_car_flags(m, |f| f | CAR_OFFROAD);
    this
}

pub fn abi_roving_traffic_construct(c: &mut Cpu) {
    c.r[2] = roving_traffic_construct(&mut c.m, AIStateRovingTraffic(c.r[4]), Car(c.r[5]), Trigger(c.r[6])).0;
}

/// `AIState_RovingTraffic::CheckIfCarIsNearbyAndStop` (AISTATE.CPP, 0x80072194): `*result` 2
/// for `other` the car itself, inactive or more than 10 m across from it; 0 more than 12 m
/// away along the road; else, ahead of the car (the offset at sp+16), it honks and stops (1).
pub fn check_if_car_is_nearby_and_stop(c: &mut Cpu, this: AIStateRovingTraffic, other: Car, result: u32) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = other.0;
        r.0[2] = this.0;
        r.0[3] = result;
        let car = this.ai_state_base().car_obj_(&mut c.m);
        if car == other || other.n().active(&mut c.m) == 0 {
            c.m.w32(result, 2);
            return;
        }
        let d = r.enter(c, 0x8007_3b04, 0x8007_21dc, |c| crate::aiworld::spline_distance(c, car, other));
        let along = if d > 0 {
            r.enter(c, 0x8007_3b04, 0x8007_21f0, |c| crate::aiworld::spline_distance(c, car, other))
        } else {
            r.enter(c, 0x8007_3b04, 0x8007_2204, |c| crate::aiworld::spline_distance(c, car, other)).wrapping_neg()
        };
        let m = &mut c.m;
        if 0xC_0000 < along {
            m.w32(result, 0);
            return;
        }
        let across = car.road_position(m).wrapping_sub(other.road_position(m));
        if across > 0 {
            if 0x9_FFFF < across {
                m.w32(result, 2);
                return;
            }
        } else if 0x9_FFFF < other.road_position(m).wrapping_sub(car.road_position(m)) {
            m.w32(result, 2);
            return;
        }
        let (p, q) = (other.n().position().get(m), car.n().position().get(m));
        let off = Coord(sp + 16);
        off.set_x(m, p.x.wrapping_sub(q.x));
        off.set_y(m, p.y.wrapping_sub(q.y));
        off.set_z(m, p.z.wrapping_sub(q.z));
        let forward = car.n().orient_mat().0 + 0x18;
        let a = fixedmult(m.r32(forward) as i32, off.x(m));
        let b = fixedmult(m.r32(forward + 4) as i32, off.y(m));
        r.0[1] = b as u32;
        let ahead = a.wrapping_add(b).wrapping_add(fixedmult(m.r32(forward + 8) as i32, off.z(m)));
        if ahead <= 0 {
            return;
        }
        r.0[0] = ahead as u32;
        r.call(c, nfs4rc::f_80075088, &[car.0, 4, 16, 8], 0x8007_2304); // AudioClc_HonkHorn
        let m = &mut c.m;
        this.ai_state_base().car_obj_(m).set_desired_speed(m, 0);
        m.w32(result, 1);
    })
}

pub fn abi_check_if_car_is_nearby_and_stop(c: &mut Cpu) {
    check_if_car_is_nearby_and_stop(c, AIStateRovingTraffic(c.r[4]), Car(c.r[5]), c.r[6]);
}

/// The dot product of two 16.16 vectors in the original's quick way ((a/256)(b/256)).
fn dot_256(m: &mut Mem, a: u32, b: u32) -> i32 {
    (0..3).fold(0i32, |s, k| s.wrapping_add((m.r32(a + 4 * k) as i32 / 256).wrapping_mul(m.r32(b + 4 * k) as i32 / 256)))
}

/// `AIState_RovingTraffic::Execute` (AISTATE.CPP, 0x80072348, each tick): toward its path's
/// point (its speed in mph; across the road from the slice's centre, kept at sp+16 and sp+32);
/// within 100 m² of it (the squared offset in 1/16 m at sp+48), waiting there its time, then
/// on to the next; stopped behind the first nearby car ahead in the race order either way
/// (the result at sp+64).
pub fn roving_traffic_execute(c: &mut Cpu, this: AIStateRovingTraffic) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this.0;
        let m = &mut c.m;
        let point = |m: &mut Mem| m.r32(this.0 + 8).wrapping_add((this.path_index_(m) as u32).wrapping_mul(20));
        let car = this.ai_state_base().car_obj_(m);
        let p = Coord(point(m)).get(m);
        car.target_pos().set(m, p);
        let at = point(m);
        let speed = m.r32(at + 12) as i32;
        car.set_desired_speed(m, speed.wrapping_mul(29255));
        let here = car.n().sim_road_info().slice(m) as i32;
        let center = Coord(slice_at(m, here).0).get(m);
        Coord(sp + 16).set(m, center);
        let t = car.target_pos().get(m);
        let rel = Coord(sp + 32);
        rel.set_x(m, t.x.wrapping_sub(center.x));
        rel.set_y(m, t.y.wrapping_sub(center.y));
        rel.set_z(m, t.z.wrapping_sub(center.z));
        let across = dot_256(m, car.n().road_matrix().0, sp + 32);
        car.set_target_lat_pos(m, across);
        let (t, q) = (car.target_pos().get(m), car.n().position().get(m));
        let dx = t.x.wrapping_sub(q.x) >> 12;
        let dy = t.y.wrapping_sub(q.y);
        let dz = t.z.wrapping_sub(q.z) >> 12;
        m.w32(sp + 48, dx as u32);
        m.w32(sp + 52, dy as u32);
        m.w32(sp + 56, dz as u32);
        if dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) < 10000 {
            if this.wait_tick_(m) == 0 {
                let at = point(m);
                let wait = m.r32(at + 16) as i32;
                let until = SIM_COUNTER.get(m).wrapping_add(wait);
                this.set_wait_tick_(m, until);
            }
            if SIM_COUNTER.get(m) < this.wait_tick_(m) {
                this.ai_state_base().car_obj_(m).set_desired_speed(m, 0);
            } else {
                this.set_wait_tick_(m, 0);
                this.update_path_index_(m, |i| i.wrapping_add(1));
            }
        }
        r.0[3] = 2;
        let result = sp + 64;
        m.w32(result, 2);
        let mut i = this.ai_state_base().car_obj_(m).sort_index(m).wrapping_add(1);
        while i < NUM_CARS.get(&mut c.m) && c.m.r32(result) == 2 {
            let other = Car(c.m.r32(SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4))));
            i += 1;
            r.0[0] = i as u32;
            r.0[1] = SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4));
            r.enter(c, 0x8007_2194, 0x8007_265c, |c| check_if_car_is_nearby_and_stop(c, this, other, result));
        }
        if c.m.r32(result) == 1 {
            return;
        }
        let m = &mut c.m;
        m.w32(result, 2);
        let mut i = this.ai_state_base().car_obj_(m).sort_index(m).wrapping_sub(1);
        r.0[0] = i as u32;
        r.0[1] = SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4));
        while i >= 0 && c.m.r32(result) == 2 {
            let other = Car(c.m.r32(SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4))));
            i -= 1;
            r.0[0] = i as u32;
            r.0[1] = SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4));
            r.enter(c, 0x8007_2194, 0x8007_26c8, |c| check_if_car_is_nearby_and_stop(c, this, other, result));
        }
    })
}

pub fn abi_roving_traffic_execute(c: &mut Cpu) {
    roving_traffic_execute(c, AIStateRovingTraffic(c.r[4]));
}

/// `AIState_Donuts::~AIState_Donuts` (AISTATE.CPP, 0x800726ec): no target point, no donuts;
/// freed with `flags` bit 0.
pub fn donuts_destruct(m: &mut Mem, this: AIStateDonuts, flags: u32) {
    set_vt(m, this.0, VT_DONUTS);
    let car = this.ai_state_base().car_obj_(m);
    clear_target(m, car);
    this.ai_state_base().car_obj_(m).set_donut_mode(m, 0);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_donuts_destruct(c: &mut Cpu) {
    donuts_destruct(&mut c.m, AIStateDonuts(c.r[4]), c.r[5]);
}

/// `AIState_Donuts::Execute` (AISTATE.CPP, 0x80072750, each tick): the car going the way it
/// faces along the road. Back on the road (modes 1, 3): toward the slice 3 on (its centre at
/// sp+48; in mode 1 within 2.5 m of the centre line, mode 2). Spinning: aiming 6 m to its side
/// and a looking-forward distance ahead (shrinking by 1 m a tick, then 20 to 40 m at random;
/// the vectors at sp+16, sp+32), off the road's edge by 2 m back to mode 1. Slowly (4.4 m/s).
pub fn donuts_execute(c: &mut Cpu, this: AIStateDonuts) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let car = this.ai_state_base().car_obj_(m);
        let slice = car.n().sim_road_info().slice(m) as i32;
        r.0[1] = slice as u32;
        let (forward, road) = (car.n().orient_mat().0 + 0x18, car.n().road_matrix().0 + 0x18);
        let dir = if dot_256(m, forward, road) > 0 { 1 } else { -1 };
        car.set_direction(m, dir);
        car.set_desired_direction(m, dir);
        let mode = this.donut_mode_(m);
        let aim = Coord(sp + 48);
        if mode == 1 || mode == 3 {
            let s = if dot_256(m, forward, road) >= 0 {
                let s = slice.wrapping_add(3);
                if s < NUM_SLICES.get(m) { s } else { slice.wrapping_sub(NUM_SLICES.get(m).wrapping_sub(3)) }
            } else {
                let s = slice.wrapping_sub(3);
                if s >= 0 { s } else { slice.wrapping_add(NUM_SLICES.get(m).wrapping_sub(3)) }
            };
            let center = Coord(slice_at(m, s).0).get(m);
            aim.set(m, center);
            r.0[2] = 0;
            if mode == 1 && car.road_position(m).wrapping_abs() <= 0x2_7FFF {
                this.set_donut_mode_(m, 2);
            }
        } else {
            let look = this.donut_look_forward_(m).wrapping_sub(0x1_0000);
            this.set_donut_look_forward_(m, look);
            if look <= 0x7FFF {
                let t = fast_random_step(m);
                let k = (((t >> 8) & 0xFFFF).wrapping_mul(5) >> 14) << 16;
                this.set_donut_look_forward_(m, (k as i32).wrapping_add(0x14_0000));
            }
            let side = Coord(sp + 16);
            let ahead = Coord(sp + 32);
            let o = car.n().orient_mat().0;
            for k in 0..3 {
                let v = m.r32(o + 4 * k);
                m.w32(side.0 + 4 * k, v);
                let v = m.r32(o + 24 + 4 * k);
                m.w32(ahead.0 + 4 * k, v);
            }
            for k in 0..3 {
                let v = fixedmult(0x6_0000, m.r32(side.0 + 4 * k) as i32);
                m.w32(side.0 + 4 * k, v as u32);
            }
            for k in 0..3 {
                let look = this.donut_look_forward_(m);
                let v = fixedmult(look, m.r32(ahead.0 + 4 * k) as i32);
                m.w32(ahead.0 + 4 * k, v as u32);
            }
            let p = car.n().position().get(m);
            let (s, a) = (side.get(m), ahead.get(m));
            aim.set_x(m, p.x.wrapping_add(s.x).wrapping_add(a.x));
            aim.set_y(m, p.y.wrapping_add(s.y).wrapping_add(a.y));
            aim.set_z(m, p.z.wrapping_add(s.z).wrapping_add(a.z));
            let (at, across) = (car.n().sim_road_info().slice(m) as i32, Coord(car.n().road_matrix().0));
            let lat = r.enter(c, 0x800a_46c4, 0x8007_2b2c, |c| crate::newton::calculate_road_position_from_slice_and_position(c, at, aim, across));
            r.0[2] = lat as u32;
            let m = &mut c.m;
            let at = car.road_position(m);
            let s = slice_at(m, slice);
            let lanes = s.lane_count(m) as i32;
            let off = if at < 0 {
                let left = ((s.avg_paved_width_lf(m) as i32) << 15).wrapping_mul(lanes >> 4);
                at.wrapping_sub(0x2_0000) < left.wrapping_neg()
            } else {
                let right = ((s.avg_paved_width_rt(m) as i32) << 15).wrapping_mul(lanes & 0xF);
                right < at.wrapping_add(0x2_0000)
            };
            if off {
                this.set_donut_mode_(m, 1);
            }
        }
        let m = &mut c.m;
        let mode = this.donut_mode_(m);
        car.set_donut_mode(m, mode);
        generic_cycles(c, &r, car, [0x8007_2be8, 0x8007_2bf4, 0x8007_2c00], None);
        let m = &mut c.m;
        let p = aim.get(m);
        car.target_pos().set(m, p);
        car.set_target_lat_pos(m, r.0[2] as i32);
        car.set_desired_speed(m, 0x4_71C7);
    })
}

pub fn abi_donuts_execute(c: &mut Cpu) {
    donuts_execute(c, AIStateDonuts(c.r[4]));
}

/// `AIState_GotoSlice::AIState_GotoSlice(Car_tObj*, int, int)` (AISTATE.CPP, 0x80072c4c):
/// normal driving to `slice` (stopping there with `stop`).
pub fn goto_slice_construct(c: &mut Cpu, this: AIStateGotoSlice, car: Car, slice: i32, stop: i32) -> AIStateGotoSlice {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = slice as u32;
        r.0[2] = stop as u32;
        let base = this.ai_state_normal().ai_state_base();
        r.enter(c, 0x8007_03bc, 0x8007_2c70, |c| normal_construct(&mut c.m, base, car));
        let m = &mut c.m;
        set_vt(m, this.0, VT_GOTO_SLICE);
        this.set_target_slice_(m, slice);
        this.set_stop_when_arrived_at_slice_(m, stop);
        this
    })
}

pub fn abi_goto_slice_construct(c: &mut Cpu) {
    c.r[2] = goto_slice_construct(c, AIStateGotoSlice(c.r[4]), Car(c.r[5]), c.r[6] as i32, c.r[7] as i32).0;
}

/// `AIState_GotoSlice::Execute` (AISTATE.CPP, 0x80072ca0, each tick): facing the target slice;
/// driving normally; to stop there, no faster than the distance allows (4 m/s within 12 m, up
/// to 200 m/s).
pub fn goto_slice_execute(c: &mut Cpu, this: AIStateGotoSlice) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let base = this.ai_state_normal().ai_state_base();
        let (slice, car) = (this.target_slice_(&mut c.m), base.car_obj_(&mut c.m));
        let d = r.enter(c, 0x8007_3a74, 0x8007_2cc4, |c| crate::aiworld::apx_spline_distance_from_slice(&mut c.m, slice, car));
        r.0[1] = d as u32;
        let m = &mut c.m;
        base.car_obj_(m).set_desired_direction(m, if d < 0 { -1 } else { 1 });
        r.enter(c, 0x8007_036c, 0x8007_2cf0, |c| normal_execute(c, base));
        let m = &mut c.m;
        if this.stop_when_arrived_at_slice_(m) == 0 {
            return;
        }
        let gap = d.wrapping_abs();
        let limit = if gap <= 0xB_FFFF {
            0x4_0000
        } else if gap <= 0x31_FFFF {
            0x14_0000
        } else if gap <= 0x95_FFFF {
            0x28_0000
        } else if gap <= 0x18F_FFFF {
            0x50_0000
        } else {
            0xC8_0000
        };
        let car = base.car_obj_(m);
        let want = car.desired_speed(m);
        car.set_desired_speed(m, if want >= 0 { want.min(limit) } else { want.max(limit.wrapping_neg()) });
    })
}

pub fn abi_goto_slice_execute(c: &mut Cpu) {
    goto_slice_execute(c, AIStateGotoSlice(c.r[4]));
}

/// `AIState_GotoSlice::InTargetSliceRange` (AISTATE.CPP, 0x80072db8): within `range` of the
/// target slice.
pub fn in_target_slice_range(c: &mut Cpu, this: AIStateGotoSlice, range: i32) -> bool {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let (car, slice) = (this.ai_state_normal().ai_state_base().car_obj_(m), this.target_slice_(m));
        apx_spline_distance_to_slice(m, car, slice).wrapping_abs() < range
    })
}

pub fn abi_in_target_slice_range(c: &mut Cpu) {
    c.r[2] = in_target_slice_range(c, AIStateGotoSlice(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIState_Cruise::AIState_Cruise(Car_tObj*, cruiseMode_t, int)` (AISTATE.CPP, 0x80072dfc):
/// normal driving at a set speed (mode 0), at a share of its wanted speed (1), at the traffic's
/// (2).
pub fn cruise_construct(c: &mut Cpu, this: AIStateCruise, car: Car, mode: i32, value: i32) -> AIStateCruise {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = mode as u32;
        r.0[2] = value as u32;
        let base = this.ai_state_normal().ai_state_base();
        r.enter(c, 0x8007_03bc, 0x8007_2e20, |c| normal_construct(&mut c.m, base, car));
        let m = &mut c.m;
        set_vt(m, this.0, VT_CRUISE);
        this.set_cruise_mode_(m, mode);
        if mode == 0 {
            this.set_cruise_speed_(m, value);
        } else if mode == 1 {
            this.set_cruise_factor_(m, value);
        }
        this
    })
}

pub fn abi_cruise_construct(c: &mut Cpu) {
    c.r[2] = cruise_construct(c, AIStateCruise(c.r[4]), Car(c.r[5]), c.r[6] as i32, c.r[7] as i32).0;
}

/// `AIState_Cruise::Execute` (AISTATE.CPP, 0x80072e68, each tick): its speed by its mode; the
/// cycles.
pub fn cruise_execute(c: &mut Cpu, this: AIStateCruise) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let car = this.ai_state_normal().ai_state_base().car_obj_(&mut c.m);
        match this.cruise_mode_(&mut c.m) {
            0 => {
                let m = &mut c.m;
                let v = this.cruise_speed_(m).wrapping_mul(car.direction(m));
                car.set_desired_speed(m, v);
            }
            1 => {
                calc_desired_speed(c, &r, car, 0x8007_2edc);
                let m = &mut c.m;
                let v = fixedmult(car.desired_speed(m), this.cruise_factor_(m));
                car.set_desired_speed(m, v);
            }
            2 => {
                r.enter(c, 0x8006_f36c, 0x8007_2f08, |c| crate::aispeeds::calc_traffic_top_speed(c, car));
            }
            _ => {}
        }
        let car = this.ai_state_normal().ai_state_base().car_obj_(&mut c.m);
        generic_cycles(c, &r, car, [0x8007_2f14, 0x8007_2f20, 0x8007_2f2c], None);
    })
}

pub fn abi_cruise_execute(c: &mut Cpu) {
    cruise_execute(c, AIStateCruise(c.r[4]));
}

/// The destructors of the states with nothing to undo (`~AIState_Cruise` 0x80072f3c,
/// `~AIState_GotoSlice` 0x80072f70, `~AIState_Normal` 0x800730dc, `~AIState_Base` 0x80073118):
/// freed with `flags` bit 0.
pub fn base_destruct(m: &mut Mem, this: AIStateBase, flags: u32) {
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_base_destruct(c: &mut Cpu) {
    base_destruct(&mut c.m, AIStateBase(c.r[4]), c.r[5]);
}

/// `AIState_RovingTraffic::TestForRelease` (AISTATE.CPP, 0x80072fa4): past the path's end.
pub fn roving_traffic_test_for_release(m: &mut Mem, this: AIStateRovingTraffic) -> bool {
    this.path_index_(m) >= this.num_path_points_(m)
}

pub fn abi_roving_traffic_test_for_release(c: &mut Cpu) {
    c.r[2] = roving_traffic_test_for_release(&mut c.m, AIStateRovingTraffic(c.r[4])) as u32;
}

/// `AIState_RovingTraffic::~AIState_RovingTraffic` (AISTATE.CPP, 0x80072fbc): no target point,
/// off its path; freed with `flags` bit 0.
pub fn roving_traffic_destruct(m: &mut Mem, this: AIStateRovingTraffic, flags: u32) {
    set_vt(m, this.0, VT_ROVING_TRAFFIC);
    let car = this.ai_state_base().car_obj_(m);
    clear_target(m, car);
    let car = this.ai_state_base().car_obj_(m);
    car.update_car_flags(m, |f| f & !CAR_OFFROAD);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_roving_traffic_destruct(c: &mut Cpu) {
    roving_traffic_destruct(&mut c.m, AIStateRovingTraffic(c.r[4]), c.r[5]);
}

/// `AIState_NonActive::~AIState_NonActive` (AISTATE.CPP, 0x80073038): the car active again;
/// freed with `flags` bit 0.
pub fn non_active_destruct(m: &mut Mem, this: AIStateBase, flags: u32) {
    set_vt(m, this.0, VT_NON_ACTIVE);
    this.car_obj_(m).n().set_active(m, 1);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_non_active_destruct(c: &mut Cpu) {
    non_active_destruct(&mut c.m, AIStateBase(c.r[4]), c.r[5]);
}

/// `AIState_Idle::~AIState_Idle` (AISTATE.CPP, 0x80073084): the car no longer idle; freed with
/// `flags` bit 0.
pub fn idle_destruct(m: &mut Mem, this: AIStateIdle, flags: u32) {
    set_vt(m, this.0, VT_IDLE);
    let car = this.ai_state_base().car_obj_(m);
    car.update_car_flags(m, |f| f & !CAR_OUT);
    set_vt(m, this.0, VT_BASE);
    if flags & 1 != 0 {
        pool_delete(m, this.0);
    }
}

pub fn abi_idle_destruct(c: &mut Cpu) {
    idle_destruct(&mut c.m, AIStateIdle(c.r[4]), c.r[5]);
}

/// `AIState_Base::TestForRelease` (AISTATE.CPP, 0x80073110): never.
pub fn abi_base_test_for_release(c: &mut Cpu) {
    c.r[2] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused: a chase, an idle, an off-road and another state; the
    /// arguments (a car, another, a slice, small numbers, flags) at `ARGS`.
    const CHASE: u32 = 0x8014_5000;
    const IDLE: u32 = 0x8014_5100;
    const OFFROAD: u32 = 0x8014_5200;
    const OTHER: u32 = 0x8014_5300;
    const ARGS: u32 = 0x8014_5400;

    /// The AI speeds' shuffled race; the states: a chase after any car (seen with a delay, in
    /// any region, murder mode, nitrous, aggression, slow-down and turn-around times near now),
    /// the target often within 160 m; an idle (in place or not, near the car or far), an off-road (let go or not, its target
    /// slice near), another state of any of four kinds; the pool partly taken; traffic cars in
    /// purgatory; cars on nitrous or not, parked, traffic, life bases, not started; any traffic
    /// density and two-player mode.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let now = SIM_COUNTER.get(m);
        for &car in &cars {
            car.set_acc_nitrous(m, [NO_NITROUS, 0x3_0000][pick(2) as usize]);
            let flags = [0, CAR_TRAFFIC, CAR_LIFE_BASIS, CAR_NOT_STARTED, CAR_TRAFFIC | CAR_LIFE_BASIS][pick(5) as usize];
            car.update_car_flags(m, |f| f & !(CAR_TRAFFIC | CAR_LIFE_BASIS | CAR_NOT_STARTED | CAR_OFFROAD) | flags);
            car.set_physics_model_timer(m, pick(200) - 50);
            car.n().set_dead_timer(m, [0, 0, 5][pick(3) as usize]);
        }
        let car = cars[pick(n) as usize];
        let target = cars[pick(n) as usize];
        if target != car && pick(2) == 0 {
            // (the target anywhere within 160 m of the car)
            let p = car.n().position().get(m);
            let reach = [0x10_0000, 0x40_0000, 0xA0_0000][pick(3) as usize];
            let q = target.n().position();
            q.set_x(m, p.x.wrapping_add(pick(2 * reach as u32) - reach));
            q.set_z(m, p.z.wrapping_add(pick(2 * reach as u32) - reach));
        }
        let chase = AIStateChase(CHASE);
        chase.ai_state_base().set_car_obj_(m, car);
        m.w32(CHASE + 4, VT_CHASE);
        let d = chase.delay_car_();
        d.set_basis_car_(m, car);
        d.set_target_car_(m, target);
        d.set_delay_factor_(m, pick(0x1_0001));
        d.set_delta_meters_(m, pick(0x400_0000) - 0x200_0000);
        let slices = NUM_SLICES.get(m) as u32;
        d.set_slice_(m, pick(slices));
        let tp = target.n().position().get(m);
        d.position_().set(m, tp);
        let at = target.road_position(m).wrapping_add(pick(0x8_0000) - 0x4_0000);
        d.set_road_position_(m, at);
        d.set_current_speed_(m, pick(0x80_0000) - 0x40_0000);
        chase.set_target_car_(m, target);
        let rel = [pick(0x8_0000) - 0x4_0000, 0, pick(0x10_0000) - 0x8_0000];
        chase.rel_position_().set(m, rel.into());
        chase.set_long_target_region_(m, pick(3) - 1);
        chase.set_lat_target_region_(m, pick(3) - 1);
        chase.set_murder_mode_(m, [0, 0, 1][pick(3) as usize]);
        chase.set_murder_end_time_(m, now + pick(400) - 200);
        chase.set_nitrous_ticks_(m, pick(400) - 100);
        chase.set_nitrous_min_fore_distance_(m, pick(0x100_0000));
        chase.set_nitrous_min_aft_distance_(m, pick(0x100_0000));
        chase.set_aggression_level_(m, pick(4));
        chase.set_slow_down_end_time_(m, now + pick(400) - 200);
        chase.set_no_turn_around_end_time_(m, now + pick(400) - 200);
        chase.set_long_meters_between_(m, [pick(0x40_0000) - 0x20_0000, pick(0x400_0000) - 0x200_0000][pick(2) as usize]);
        chase.set_in_target_region_(m, pick(100));
        chase.set_barrier_ticks32_(m, pick(100));
        let idle = AIStateIdle(IDLE);
        idle.ai_state_base().set_car_obj_(m, cars[pick(n) as usize]);
        let at = idle.ai_state_base().car_obj_(m).road_position(m);
        idle.set_road_position_(m, at.wrapping_add([pick(0x1_0000), pick(0x10_0000)][pick(2) as usize] - 0x8000));
        idle.set_idle_in_place_flag_(m, pick(2));
        let off = AIStateOffroad(OFFROAD);
        off.ai_state_base().set_car_obj_(m, cars[pick(n) as usize]);
        m.w32(OFFROAD + 4, VT_OFFROAD);
        off.set_let_go_(m, pick(2));
        let slice = off.ai_state_base().car_obj_(m).n().sim_road_info().slice(m) as i32;
        off.set_target_slice_(m, (slice + pick(60) - 30).max(0));
        off.set_max_speed_mps_(m, pick(0x40_0000));
        off.set_release_time_(m, pick(0x4_0000));
        let p = cars[pick(n) as usize].n().position().get(m);
        off.start_position_().set(m, p);
        for k in 0..9 {
            m.w32(off.start_orientation_().0 + 4 * k, (pick(0x2_0000) - 0x1_0000) as u32);
        }
        let other = AIStateBase(OTHER);
        other.set_car_obj_(m, cars[pick(n) as usize]);
        m.w32(OTHER + 4, [VT_NORMAL, VT_CHASE, VT_OFFROAD, VT_PURGATORY][pick(4) as usize]);
        if m.r32(OTHER + 4) == VT_CHASE || m.r32(OTHER + 4) == VT_OFFROAD {
            // (a copy of the chase or off-road state's fields)
            let from = if m.r32(OTHER + 4) == VT_CHASE { CHASE } else { OFFROAD };
            for k in 2..0x25 {
                let v = m.r32(from + 4 * k);
                m.w32(OTHER + 4 * k, v);
            }
        }
        for i in 0..10 {
            m.w32(POOL_TAKEN + 4 * i, pick(2) as u32);
        }
        m.w32(NUM_IN_PURGATORY, pick(4) as u32);
        NUM_TRAFFIC_CARS.set(m, pick(5));
        GAME_SETUP.set_traffic_density(m, pick(4));
        GAME_SETUP.set_comm_mode(m, pick(3));
        let args = [cars[pick(n) as usize].0, cars[pick(n) as usize].0, (slice + pick(40) - 20).max(0) as u32, pick(3) as u32, pick(300) as u32];
        for (k, a) in args.into_iter().enumerate() {
            m.w32(ARGS + 4 * k as u32, a);
        }
    }

    fn args(c: &mut Cpu) -> [u32; 5] {
        std::array::from_fn(|k| c.m.r32(ARGS + 4 * k as u32))
    }

    /// The arguments from the fifth on where the caller puts them (sp+16), as for the original.
    fn home(c: &mut Cpu, a: &[u32]) {
        let sp = c.r[29];
        for (k, v) in a.iter().skip(4).enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, *v);
        }
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aistate_fuzz() {
        // (each call's arguments from the shuffle's: a car, another, a slice, a small number,
        // another; the port takes them as the original does)
        let cases: [(&str, fn(&mut Cpu), fn([u32; 5]) -> Vec<u32>, Port); 34] = [
            ("StartUp", nfs4rc::f_800701bc, |_| vec![], |c, _| {
                start_up(&mut c.m);
                None
            }),
            ("StateExecute", nfs4rc::f_80070244, |_| vec![OTHER], |c, a| {
                state_execute(c, AIStateBase(a[0]));
                None
            }),
            ("BaseConstruct", nfs4rc::f_800702cc, |a| vec![OTHER, a[0]], |c, a| Some(base_construct(&mut c.m, AIStateBase(a[0]), Car(a[1])).0)),
            ("PoolNew", nfs4rc::f_800702e4, |_| vec![160], |c, _| Some(pool_new(&mut c.m))),
            ("PoolDelete", nfs4rc::f_80070330, |a| vec![POOL + 160 * (a[4] % 12)], |c, a| {
                pool_delete(&mut c.m, a[0]);
                None
            }),
            ("NormalExecute", nfs4rc::f_8007036c, |_| vec![OTHER], |c, a| {
                normal_execute(c, AIStateBase(a[0]));
                None
            }),
            ("NormalConstruct", nfs4rc::f_800703bc, |a| vec![OTHER, a[0]], |c, a| Some(normal_construct(&mut c.m, AIStateBase(a[0]), Car(a[1])).0)),
            ("IdleExecute", nfs4rc::f_80070414, |_| vec![IDLE], |c, a| {
                idle_execute(c, AIStateIdle(a[0]));
                None
            }),
            ("SetIdlePosition", nfs4rc::f_800704d0, |a| vec![IDLE, a[4] << 12], |c, a| {
                set_idle_position(&mut c.m, AIStateIdle(a[0]), a[1] as i32);
                None
            }),
            ("ChaseConstruct", nfs4rc::f_800704dc, |a| vec![OTHER, a[0], a[1], CHASE + 0x4c, a[4], a[4] << 14, a[4] << 13, a[3], a[4] << 8], |c, a| {
                home(c, a);
                Some(chase_construct(c, AIStateChase(a[0]), Car(a[1]), Car(a[2]), Coord(a[3]), a[4] as i32, a[5] as i32, a[6] as i32, a[7] as i32, a[8] as i32).0)
            }),
            ("ChaseDestruct", nfs4rc::f_800705e4, |a| vec![CHASE, a[3] & 1], |c, a| {
                chase_destruct(&mut c.m, AIStateChase(a[0]), a[1]);
                None
            }),
            ("SetTarget", nfs4rc::f_8007066c, |a| vec![CHASE, a[1], CHASE + 0x4c], |c, a| {
                set_target(c, AIStateChase(a[0]), Car(a[1]), Coord(a[2]));
                None
            }),
            ("SetMurderMode", nfs4rc::f_8007074c, |a| vec![CHASE, a[3], a[4]], |c, a| {
                set_murder_mode(&mut c.m, AIStateChase(a[0]), a[1] as i32, a[2] as i32);
                None
            }),
            ("SetUp", nfs4rc::f_80070778, |_| vec![CHASE], |c, a| {
                set_up(c, AIStateChase(a[0]));
                None
            }),
            ("DoSlowNitrous", nfs4rc::f_800708d4, |_| vec![CHASE], |c, a| {
                do_slow_nitrous(&mut c.m, AIStateChase(a[0]));
                None
            }),
            ("DoNitrous", nfs4rc::f_800708f0, |a| vec![CHASE, a[3] & 1], |c, a| {
                do_nitrous(c, AIStateChase(a[0]), a[1] != 0);
                None
            }),
            ("ChaseExecute", nfs4rc::f_80070a3c, |_| vec![CHASE], |c, a| {
                chase_execute(c, AIStateChase(a[0]));
                None
            }),
            ("FarTargeting", nfs4rc::f_80070bb0, |_| vec![CHASE], |c, a| {
                far_targeting(c, AIStateChase(a[0]));
                None
            }),
            ("CloseSlowDownFactor", nfs4rc::f_80070c5c, |_| vec![CHASE], |c, a| Some(close_targeting_ahead_slow_down_factor(&mut c.m, AIStateChase(a[0])) as u32)),
            ("CloseTargeting", nfs4rc::f_80070ce8, |_| vec![CHASE], |c, a| {
                close_targeting(c, AIStateChase(a[0]));
                None
            }),
            ("ApproachTargeting", nfs4rc::f_80071198, |a| vec![CHASE, a[3] & 1], |c, a| {
                approach_targeting(c, AIStateChase(a[0]), a[1] != 0);
                None
            }),
            ("CheckForBarriers", nfs4rc::f_800714a4, |_| vec![CHASE], |c, a| {
                check_for_barriers_and_target_around_them(c, AIStateChase(a[0]));
                None
            }),
            ("FindBarrierEndSlice", nfs4rc::f_800715f4, |_| vec![CHASE], |c, a| Some(find_barrier_end_slice(c, AIStateChase(a[0])) as u32)),
            ("OffroadDestruct", nfs4rc::f_8007198c, |a| vec![OFFROAD, a[3] & 1], |c, a| {
                offroad_destruct(&mut c.m, AIStateOffroad(a[0]), a[1]);
                None
            }),
            ("OffroadConstruct", nfs4rc::f_80071a00, |a| vec![OTHER, a[0], a[2], OFFROAD + 0xc, OFFROAD + 0x18, a[4], a[4] << 10, a[2]], |c, a| {
                home(c, a);
                Some(offroad_construct(&mut c.m, AIStateOffroad(a[0]), Car(a[1]), a[2] as i32, Coord(a[3]), Matrix(a[4]), a[5] as i32, a[6] as i32, a[7] as i32).0)
            }),
            ("UnleashIfInRange", nfs4rc::f_80071b48, |a| vec![OFFROAD, a[1]], |c, a| {
                unleash_if_in_range(c, AIStateOffroad(a[0]), Car(a[1]));
                None
            }),
            ("OffroadExecute", nfs4rc::f_80071bc0, |_| vec![OFFROAD], |c, a| {
                offroad_execute(c, AIStateOffroad(a[0]));
                None
            }),
            ("PurgatoryConstruct", nfs4rc::f_80071d6c, |a| vec![OTHER, a[0]], |c, a| Some(purgatory_construct(c, AIStateBase(a[0]), Car(a[1])).0)),
            ("PurgatoryDestruct", nfs4rc::f_80071f1c, |a| vec![OTHER, a[3] & 1], |c, a| {
                purgatory_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("PurgatoryTestForRelease", nfs4rc::f_8007203c, |_| vec![OTHER], |c, a| Some(purgatory_test_for_release(&mut c.m, AIStateBase(a[0])) as u32)),
            ("PurgatoryExecute", nfs4rc::f_800720b8, |_| vec![OTHER], |c, a| {
                purgatory_execute(&mut c.m, AIStateBase(a[0]));
                None
            }),
            ("PurgatoryStartUp", nfs4rc::f_80072120, |_| vec![], |c, _| {
                purgatory_start_up(&mut c.m);
                None
            }),
            ("Restart", nfs4rc::f_800701fc, |_| vec![], |c, _| {
                start_up(&mut c.m);
                None
            }),
            ("CleanUp", nfs4rc::f_8007023c, |_| vec![], |_, _| None),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x2A10 + k as u64, shuffle, original, &|c| call(args(c)), port);
        }
    }

    /// The stack the chase's steps, the off-road state and purgatory leave (their callees'
    /// frames, the delayed target's point, the barrier search's distances).
    #[test]
    fn state_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu), u32); 9] = [
            ("ChaseExecute", 0x8007_0a3c, nfs4rc::f_80070a3c, CHASE),
            ("CloseTargeting", 0x8007_0ce8, nfs4rc::f_80070ce8, CHASE),
            ("ApproachTargeting", 0x8007_1198, nfs4rc::f_80071198, CHASE),
            ("FarTargeting", 0x8007_0bb0, nfs4rc::f_80070bb0, CHASE),
            ("FindBarrierEndSlice", 0x8007_15f4, nfs4rc::f_800715f4, CHASE),
            ("OffroadExecute", 0x8007_1bc0, nfs4rc::f_80071bc0, OFFROAD),
            ("StateExecute", 0x8007_0244, nfs4rc::f_80070244, OTHER),
            ("PurgatoryConstruct", 0x8007_1d6c, nfs4rc::f_80071d6c, OTHER),
            ("UnleashIfInRange", 0x8007_1b48, nfs4rc::f_80071b48, OFFROAD),
        ];
        for (k, (name, addr, original, this)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 500, 0x2A50 + k as u64, shuffle, &|c, _| {
                let a = args(c);
                let second = match name {
                    "PurgatoryConstruct" => a[0],
                    "UnleashIfInRange" => a[1],
                    _ => a[3] & 1,
                };
                vec![this, second, 0, 0]
            });
        }
    }

    /// More RAM the race leaves unused: a roving traffic state and its path, donuts, going to a
    /// slice, cruising; a traffic path trigger.
    const ROVING: u32 = 0x8014_5500;
    const DONUTS: u32 = 0x8014_5600;
    const GOTO: u32 = 0x8014_5700;
    const CRUISE: u32 = 0x8014_5800;
    const PATH: u32 = 0x8014_6000;
    const TRIGGER: u32 = 0x8014_6800;

    /// The first shuffle; the other states: roving along a path of 1 to 6 points near its car
    /// (some reached, waiting or not; the other car to check often beside it), donuts in any mode and looking-forward distance, going to
    /// a slice near (stopping or not), cruising in any mode; a traffic path trigger.
    fn shuffle2(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let now = SIM_COUNTER.get(m);
        let roving = AIStateRovingTraffic(ROVING);
        let car = cars[pick(n) as usize];
        roving.ai_state_base().set_car_obj_(m, car);
        m.w32(ROVING + 4, VT_ROVING_TRAFFIC);
        let points = 1 + pick(6);
        m.w32(ROVING + 8, PATH);
        roving.set_num_path_points_(m, points);
        roving.set_path_index_(m, pick(points as u32 + 1).min(points - 1));
        roving.set_wait_tick_(m, [0, now + pick(200) - 100][pick(2) as usize]);
        let p = car.n().position().get(m);
        for k in 0..6u32 {
            let at = PATH + 20 * k;
            let near = [0x2_0000, 0x10_0000, 0x80_0000][pick(3) as usize];
            m.w32(at, p.x.wrapping_add(pick(2 * near as u32) - near) as u32);
            m.w32(at + 4, p.y as u32);
            m.w32(at + 8, p.z.wrapping_add(pick(2 * near as u32) - near) as u32);
            m.w32(at + 12, pick(80) as u32);
            m.w32(at + 16, pick(300) as u32);
        }
        let other = Car(m.r32(ARGS + 4));
        if other != car && pick(2) == 0 {
            // (the other car beside or behind it, within 16 m along and 12 m across)
            let q = other.n().position();
            q.set_x(m, p.x.wrapping_add(pick(0x18_0000) - 0xC_0000));
            q.set_z(m, p.z.wrapping_add(pick(0x20_0000) - 0x10_0000));
            let at = car.road_position(m).wrapping_add(pick(0x18_0000) - 0xC_0000);
            other.set_road_position(m, at);
            let slice = car.n().sim_road_info().slice(m);
            other.n().sim_road_info().set_slice(m, slice);
        }
        m.w32(TRIGGER, 5);
        m.w32(TRIGGER + 0x38, points as u32);
        m.w32(TRIGGER + 0x3c, PATH);
        let donuts = AIStateDonuts(DONUTS);
        donuts.ai_state_base().set_car_obj_(m, cars[pick(n) as usize]);
        m.w32(DONUTS + 4, VT_DONUTS);
        donuts.set_donut_mode_(m, pick(4));
        donuts.set_donut_look_forward_(m, [pick(0x1_0000), pick(0x30_0000)][pick(2) as usize]);
        let goto = AIStateGotoSlice(GOTO);
        let car = cars[pick(n) as usize];
        goto.ai_state_normal().ai_state_base().set_car_obj_(m, car);
        m.w32(GOTO + 4, VT_GOTO_SLICE);
        let slice = car.n().sim_road_info().slice(m) as i32;
        goto.set_target_slice_(m, (slice + pick(200) - 100).max(0));
        goto.set_stop_when_arrived_at_slice_(m, pick(2));
        let cruise = AIStateCruise(CRUISE);
        cruise.ai_state_normal().ai_state_base().set_car_obj_(m, cars[pick(n) as usize]);
        m.w32(CRUISE + 4, VT_CRUISE);
        cruise.set_cruise_mode_(m, pick(4));
        cruise.set_cruise_speed_(m, pick(0x40_0000));
        cruise.set_cruise_factor_(m, pick(0x2_0000));
    }

    #[test]
    fn aistate2_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn([u32; 5]) -> Vec<u32>, Port); 22] = [
            ("RovingTrafficConstruct", nfs4rc::f_8007212c, |a| vec![OTHER, a[0], TRIGGER], |c, a| Some(roving_traffic_construct(&mut c.m, AIStateRovingTraffic(a[0]), Car(a[1]), Trigger(a[2])).0)),
            ("CheckIfCarIsNearbyAndStop", nfs4rc::f_80072194, |a| vec![ROVING, a[1], ARGS + 32], |c, a| {
                check_if_car_is_nearby_and_stop(c, AIStateRovingTraffic(a[0]), Car(a[1]), a[2]);
                None
            }),
            ("RovingTrafficExecute", nfs4rc::f_80072348, |_| vec![ROVING], |c, a| {
                roving_traffic_execute(c, AIStateRovingTraffic(a[0]));
                None
            }),
            ("DonutsDestruct", nfs4rc::f_800726ec, |a| vec![DONUTS, a[3] & 1], |c, a| {
                donuts_destruct(&mut c.m, AIStateDonuts(a[0]), a[1]);
                None
            }),
            ("DonutsExecute", nfs4rc::f_80072750, |_| vec![DONUTS], |c, a| {
                donuts_execute(c, AIStateDonuts(a[0]));
                None
            }),
            ("GotoSliceConstruct", nfs4rc::f_80072c4c, |a| vec![OTHER, a[0], a[2], a[3] & 1], |c, a| Some(goto_slice_construct(c, AIStateGotoSlice(a[0]), Car(a[1]), a[2] as i32, a[3] as i32).0)),
            ("GotoSliceExecute", nfs4rc::f_80072ca0, |_| vec![GOTO], |c, a| {
                goto_slice_execute(c, AIStateGotoSlice(a[0]));
                None
            }),
            ("InTargetSliceRange", nfs4rc::f_80072db8, |a| vec![GOTO, a[4] << 14], |c, a| Some(in_target_slice_range(c, AIStateGotoSlice(a[0]), a[1] as i32) as u32)),
            ("CruiseConstruct", nfs4rc::f_80072dfc, |a| vec![OTHER, a[0], a[3], a[4] << 12], |c, a| Some(cruise_construct(c, AIStateCruise(a[0]), Car(a[1]), a[2] as i32, a[3] as i32).0)),
            ("CruiseExecute", nfs4rc::f_80072e68, |_| vec![CRUISE], |c, a| {
                cruise_execute(c, AIStateCruise(a[0]));
                None
            }),
            ("CruiseDestruct", nfs4rc::f_80072f3c, |a| vec![CRUISE, a[3] & 1], |c, a| {
                base_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("GotoSliceDestruct", nfs4rc::f_80072f70, |a| vec![GOTO, a[3] & 1], |c, a| {
                base_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("RovingTrafficTestForRelease", nfs4rc::f_80072fa4, |_| vec![ROVING], |c, a| Some(roving_traffic_test_for_release(&mut c.m, AIStateRovingTraffic(a[0])) as u32)),
            ("RovingTrafficDestruct", nfs4rc::f_80072fbc, |a| vec![ROVING, a[3] & 1], |c, a| {
                roving_traffic_destruct(&mut c.m, AIStateRovingTraffic(a[0]), a[1]);
                None
            }),
            ("NonActiveExecute", nfs4rc::f_80073030, |_| vec![OTHER], |_, _| None),
            ("NonActiveDestruct", nfs4rc::f_80073038, |a| vec![OTHER, a[3] & 1], |c, a| {
                non_active_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("IdleDestruct", nfs4rc::f_80073084, |a| vec![IDLE, a[3] & 1], |c, a| {
                idle_destruct(&mut c.m, AIStateIdle(a[0]), a[1]);
                None
            }),
            ("NormalDestruct", nfs4rc::f_800730dc, |a| vec![OTHER, a[3] & 1], |c, a| {
                base_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("BaseTestForRelease", nfs4rc::f_80073110, |_| vec![OTHER], |_, _| Some(0)),
            ("BaseDestruct", nfs4rc::f_80073118, |a| vec![POOL + 160 * (a[4] % 11), a[3] & 1], |c, a| {
                base_destruct(&mut c.m, AIStateBase(a[0]), a[1]);
                None
            }),
            ("StateExecuteRoving", nfs4rc::f_80070244, |_| vec![ROVING], |c, a| {
                state_execute(c, AIStateBase(a[0]));
                None
            }),
            ("StateExecuteDonuts", nfs4rc::f_80070244, |_| vec![DONUTS], |c, a| {
                state_execute(c, AIStateBase(a[0]));
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x2B10 + k as u64, shuffle2, original, &|c| call(args(c)), port);
        }
    }

    /// The stack the roving traffic, donuts, going to a slice and cruising leave.
    #[test]
    fn state2_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu), u32); 5] = [
            ("RovingTrafficExecute", 0x8007_2348, nfs4rc::f_80072348, ROVING),
            ("DonutsExecute", 0x8007_2750, nfs4rc::f_80072750, DONUTS),
            ("GotoSliceExecute", 0x8007_2ca0, nfs4rc::f_80072ca0, GOTO),
            ("CruiseExecute", 0x8007_2e68, nfs4rc::f_80072e68, CRUISE),
            ("GotoSliceConstruct", 0x8007_2c4c, nfs4rc::f_80072c4c, OTHER),
        ];
        for (k, (name, addr, original, this)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 500, 0x2B50 + k as u64, shuffle2, &|c, _| {
                let a = args(c);
                vec![this, a[0], a[2], a[3] & 1]
            });
        }
    }
}
