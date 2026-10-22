//! AIHIGH.CPP: the AI's high level, an object per car (`highLevelAIObjs`) of the class its
//! role takes: a human (`AIHigh_Human`), an opponent, traffic, a police car, nothing; in a
//! "be the cop" race the human cops, their AI wingmen and the speeders they chase. Each tick
//! its virtual `HighExecute` runs, at the car's simulation rate, and moves the car's state
//! (AISTATE.CPP) on. The classes are in the AIH_*.CPP files; this one makes the objects and
//! runs them.

use crate::abi::{self, Saved};
use crate::aistate;
use crate::globals::{GAME_SETUP, HUMAN_RACE_CARS, NUM_HUMAN_RACE_CARS};
use crate::types::{AIHighBase, AIStateBase, Car};
use ps1rt::{Cpu, Mem};

/// `highLevelAIObjs`: each car's object, by its index in `Cars_gList`.
pub const HIGH_LEVEL_AI_OBJS: u32 = 0x8010_D994;
/// `AIHigh_CopGameType`: no police (0), police (1), a human cop with AI speeders (2), two
/// human cops (3), a human cop and a human speeder (4).
pub const COP_GAME_TYPE: crate::globals::I32 = crate::globals::I32(0x8013_D84C);
/// `AI_time`, `AI_elapsedTime`, `AI_iTime`: the AI's time step, as `Sched_ExecuteCheck` sets it.
pub const AI_TIME: u32 = 0x8013_D840;
pub const AI_ELAPSED: u32 = 0x8013_D844;
pub const AI_I_TIME: u32 = 0x8013_D848;

// The classes' vtables (at +0x14 of an object): per entry a `this` adjustment and the function;
// 1 `HighExecute`, 2 the destructor, 3 `CheckForCrimes` (the perps) or `FreezeAndEndChase`
// (the "be the cop" police).
pub const VT_BTC_PERP: u32 = 0x8005_559C;
pub const VT_NONE: u32 = 0x8005_55BC;
pub const VT_BASE: u32 = 0x8005_55D4;
pub const VT_BASIC_PERP: u32 = 0x8005_563C;
pub const VT_BASIC_COP: u32 = 0x8005_566C;
pub const VT_BTC_WINGMAN: u32 = 0x8005_5694;
pub const VT_BTC_HUMAN_COP: u32 = 0x8005_56B4;
pub const VT_BTC_COP: u32 = 0x8005_56D4;
pub const VT_BTC_AI_PERP: u32 = 0x8005_576C;
pub const VT_BTC_HUMAN_PERP: u32 = 0x8005_578C;
pub const VT_BTC_PERP2: u32 = 0x8005_57AC;
pub const VT_PLAYER: u32 = 0x8005_5864;
pub const VT_HUMAN: u32 = 0x8005_5894;
pub const VT_OPPONENT: u32 = 0x8005_58C4;

// The states' vtables as this file has them (the inline classes' copies): `AIState_None`,
// `AIState_Base`.
pub const VT_STATE_NONE: u32 = 0x8005_55EC;
pub const VT_STATE_BASE: u32 = 0x8005_560C;

/// `Car_tObj.carFlags`.
pub const CAR_HUMAN: i32 = 0x4;
pub const CAR_OPPONENT: i32 = 0x8;
pub const CAR_TRAFFIC: i32 = 0x10;
pub const CAR_COP: i32 = 0x20;
pub const CAR_HUMAN_COP: i32 = 0x200;

/// The virtual function `k` of a high-level object: the object to call it on, and the function.
pub fn virtual_fn(m: &mut Mem, this: u32, k: u32) -> (u32, u32) {
    let vt = m.r32(this.wrapping_add(0x14));
    let adjust = m.r16(vt.wrapping_add(8 * k)) as i16 as i32;
    (this.wrapping_add(adjust as u32), m.r32(vt.wrapping_add(8 * k + 4)))
}

pub fn set_vt(m: &mut Mem, this: u32, vt: u32) {
    m.w32(this.wrapping_add(0x14), vt);
}

/// The object of car `i` of `Cars_gList`.
pub fn obj(m: &mut Mem, i: i32) -> u32 {
    m.r32(HIGH_LEVEL_AI_OBJS.wrapping_add((i as u32).wrapping_mul(4)))
}

/// A "be the cop" race: a cop race (1, 5) with a human cop (the first human, or the second of
/// two).
pub fn is_btc(m: &mut Mem) -> bool {
    let race = GAME_SETUP.race_type(m);
    if race != 1 && race != 5 {
        return false;
    }
    let first = Car(m.r32(HUMAN_RACE_CARS));
    if first.car_flags(m) & CAR_HUMAN_COP != 0 {
        return true;
    }
    NUM_HUMAN_RACE_CARS.get(m) == 2 && Car(m.r32(HUMAN_RACE_CARS + 4)).car_flags(m) & CAR_HUMAN_COP != 0
}

/// Delete the object's state (its virtual destructor), if it has one.
fn delete_state(c: &mut Cpu, r: &Saved, state: u32, ra: u32) {
    if state != 0 {
        let (obj, f) = aistate::virtual_fn(&mut c.m, state, 2);
        r.call_ptr(c, f, &[obj, 3], ra);
    }
}

/// `AIHigh_Base::AIHigh_Base(Car_tObj*)` (AIHIGH.CPP, 0x8005bb08): for `car`, its state none
/// (from the states' pool), scheduled, its last traffic trigger check at its slice.
pub fn base_construct(c: &mut Cpu, this: AIHighBase, car: Car) -> AIHighBase {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        set_vt(m, this.0, VT_BASE);
        this.set_car_obj_(m, car);
        m.w32(this.0 + 4, 0);
        this.set_state_type_(m, 0);
        let state = aistate::pool_new(m);
        r.0[1] = state;
        let car = this.car_obj_(m);
        aistate::base_construct(m, AIStateBase(state), car);
        m.w32(state.wrapping_add(4), VT_STATE_NONE);
        // (`SetState`: the state it had, none, deleted)
        let old = m.r32(this.0 + 4);
        delete_state(c, &r, old, 0x8005_bb84);
        let m = &mut c.m;
        m.w32(this.0 + 4, state);
        this.set_state_type_(m, 0);
        this.set_scheduling_off_(m, 0);
        let slice = this.car_obj_(m).n().sim_road_info().slice(m) as i32;
        this.set_last_traffic_trigger_check_slice_(m, slice);
        this
    })
}

pub fn abi_base_construct(c: &mut Cpu) {
    c.r[2] = base_construct(c, AIHighBase(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_Base::~AIHigh_Base` (AIHIGH.CPP, 0x8005bbb4): its state deleted; itself when
/// `flags` asks.
pub fn base_destruct(c: &mut Cpu, this: AIHighBase, flags: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.0[1] = flags;
        set_vt(&mut c.m, this.0, VT_BASE);
        let state = c.m.r32(this.0 + 4);
        if state != 0 {
            delete_state(c, &r, state, 0x8005_bbfc);
            c.m.w32(this.0 + 4, 0);
        }
        if flags & 1 != 0 {
            r.call(c, nfs4rc::f_800a4910, &[this.0], 0x8005_bc14); // operator delete
        }
    })
}

pub fn abi_base_destruct(c: &mut Cpu) {
    base_destruct(c, AIHighBase(c.r[4]), c.r[5]);
}

/// A destructor of the header's classes that sets its vtable (`vt`, or none) and runs the
/// base's from the call site returning to `ra`.
pub fn destruct_to_base(c: &mut Cpu, this: u32, flags: u32, vt: Option<u32>, ra: u32) {
    abi::frame(c, 24, |c, _| {
        if let Some(vt) = vt {
            set_vt(&mut c.m, this, vt);
        }
        let r = Saved::of(c);
        r.enter(c, 0x8005_bbb4, ra, |c| base_destruct(c, AIHighBase(this), flags));
    })
}

/// `AIHigh_BTC_Perp::~AIHigh_BTC_Perp` (AIHIGH.H, 0x8005bc28).
pub fn abi_btc_perp_destruct(c: &mut Cpu) {
    destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8005_bc40);
}

/// `AIHigh_None::HighExecute` (AIHIGH.H, 0x8005bc50): nothing. Also `AIState_None::Execute`
/// (AISTATE.H, 0x8005bc78) and the other copies of it.
pub fn abi_nothing(_: &mut Cpu) {}

/// `AIHigh_None::~AIHigh_None` (AIHIGH.H, 0x8005bc58).
pub fn abi_none_destruct(c: &mut Cpu) {
    destruct_to_base(c, c.r[4], c.r[5], None, 0x8005_bc68);
}

/// `AIState_None::~AIState_None` and `AIState_Base::~AIState_Base` as the AIH files have them
/// (AISTATE.H, 0x8005bc80...): the base state's vtable (`vt`); back to the pool when `flags`
/// asks.
pub fn state_destruct(c: &mut Cpu, this: u32, flags: u32, vt: u32) {
    c.m.w32(this.wrapping_add(4), vt);
    if flags & 1 != 0 {
        aistate::pool_delete(&mut c.m, this);
    }
}

pub fn abi_state_none_destruct(c: &mut Cpu) {
    state_destruct(c, c.r[4], c.r[5], VT_STATE_BASE);
}

/// `AIState_Base::TestForRelease` (AISTATE.H, 0x8005bcb4 and its copies): no.
pub fn abi_state_test_for_release(c: &mut Cpu) {
    c.r[2] = 0;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, COP_CARS, NUM_CARS, NUM_COP_CARS, SIM_COUNTER};
    use crate::test_util::Rng;

    /// RAM the race leaves unused: a high-level object per car (0x100 bytes each), the
    /// arguments, more cars (copies of the race's).
    pub(crate) const OBJS: u32 = 0x8014_0000;
    pub(crate) const ARGS: u32 = 0x8014_0800;
    const MORE_CARS: u32 = 0x8014_6000;
    /// The race's own objects (a human's, two opponents'), from the heap.
    pub(crate) const HEAP_OBJS: [u32; 3] = [0x801B_A120, 0x801B_A1E0, 0x801B_A2B0];
    /// States of the pool the race's objects have.
    const STATES: [u32; 3] = [0x8010_EA70, 0x8010_EC50, 0x8010_EB10];

    /// The AI lives' shuffled race with up to three more cars (copies), each car of a role
    /// (a human, a human cop, an opponent, traffic, a police car), on duty or parked, flying,
    /// active, upright or not, in any lane, just hit by another car or not, finished or not,
    /// near one another in height; the police list from them; an object per car (at `OBJS`,
    /// of its role's class) with any crime, pull-over time, chasers and arresting police car;
    /// any skill and cop race; the arguments (cars, small numbers, indexes) at `ARGS`.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::ailife::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let more = pick(4) as u32;
        for k in 0..more {
            let from = m.r32(CARS_LIST + 4 * (k % 3));
            let to = MORE_CARS + k * 0x900;
            for w in (0..Car::SIZE).step_by(4) {
                let v = m.r32(from + w);
                m.w32(to + w, v);
            }
            m.w32(CARS_LIST + 4 * (3 + k), to);
            Car(to).set_car_index(m, (3 + k) as i32);
        }
        let n = 3 + more;
        NUM_CARS.set(m, n as i32);
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let now = [pick(600), 600 + pick(4000)][pick(2) as usize];
        SIM_COUNTER.set(m, now);
        let near = cars[pick(n) as usize].n().position().get(m);
        let mut cops = Vec::new();
        for (i, &car) in cars.iter().enumerate() {
            let role = [CAR_HUMAN | 2, CAR_HUMAN | 2 | CAR_HUMAN_COP, CAR_OPPONENT, CAR_TRAFFIC, CAR_COP, CAR_COP][pick(6) as usize];
            car.update_car_flags(m, |f| f & !(CAR_HUMAN | 2 | CAR_HUMAN_COP | CAR_OPPONENT | CAR_TRAFFIC | CAR_COP) | role);
            if role & CAR_COP != 0 || pick(6) == 0 {
                cops.push(car);
            }
            car.set_ai_flags(m, [2, 2, 0, 4, 6][pick(5) as usize]);
            car.n().set_flight_time(m, [0, 0, 0, 3][pick(4) as usize]);
            car.n().set_active(m, [1, 1, 1, 0][pick(4) as usize]);
            if pick(4) != 0 {
                for k in 0..9 {
                    m.w32(car.0 + 0xF0 + 4 * k, if k % 4 == 0 { 0x1_0000 } else { 0 });
                }
            } else {
                for k in 0..9 {
                    m.w32(car.0 + 0xF0 + 4 * k, (pick(0x2_0000) - 0x1_0000) as u32);
                }
            }
            let reach = [0x8_0000, 0x20_0000, 0x80_0000][pick(3) as usize];
            let p = car.n().position();
            p.set_x(m, near.x.wrapping_add(pick(2 * reach as u32) - reach));
            p.set_y(m, near.y.wrapping_add(pick(0x8_0000) - 0x4_0000));
            p.set_z(m, near.z.wrapping_add(pick(2 * reach as u32) - reach));
            let hit = car.n().collision();
            hit.set_last_time(m, now - pick(40));
            let other = [0, cars[pick(n) as usize].0][pick(2) as usize];
            hit.set_last_other_obj(m, crate::types::NewtonObj(other));
            hit.set_last_impulse(m, pick(0x30_0000));
            car.stats().set_finish_type(m, [0, 0, 0, 1, 2][pick(5) as usize]);
            car.set_lane_index(m, pick(14));
            car.n().set_speed_xz(m, pick(0x40_0000));
            car.set_speed(m, [pick(0x8_0000), pick(0x40_0000)][pick(2) as usize]);
            let speed = [pick(0x10_0000) - 0x8_0000, pick(0x40_0000) - 0x20_0000, (0x3_0000 + pick(0x2_0000)) * [1, -1][pick(2) as usize]][pick(3) as usize];
            car.set_current_speed(m, speed);
            let _ = i;
        }
        NUM_COP_CARS.set(m, cops.len() as i32);
        for (k, cop) in cops.iter().enumerate() {
            m.w32(COP_CARS + 4 * k as u32, cop.0);
        }
        m.w32(HUMAN_RACE_CARS, cars[pick(n) as usize].0);
        m.w32(HUMAN_RACE_CARS + 4, cars[pick(n) as usize].0);
        let ncops = cops.len().max(1) as u32;
        for (i, &car) in cars.iter().enumerate() {
            let o = OBJS + 0x100 * i as u32;
            for w in (0..0x100).step_by(4) {
                m.w32(o + w, [0, 1, pick(6) as u32, pick(0x10_0000) as u32][pick(4) as usize]);
            }
            let b = AIHighBase(o);
            b.set_car_obj_(m, car);
            m.w32(o + 4, [0, STATES[0], STATES[1], STATES[2]][pick(4) as usize]);
            let f = car.car_flags(m);
            let vt = if f & CAR_HUMAN != 0 { VT_HUMAN } else if f & CAR_OPPONENT != 0 { VT_OPPONENT } else { VT_NONE };
            set_vt(m, o, vt);
            m.w32(o + 0x68, (now - pick(2000)) as u32);
            m.w32(o + 0x6C, [0, cars[pick(n) as usize].0][pick(2) as usize]);
            m.w32(o + 0x78, [0, 0, 0, 1, 2, 3, 4][pick(7) as usize] as u32);
            for p in 0..6 {
                let (cop, at) = if pick(2) == 0 { (-1, -1) } else { (pick(ncops), pick(n)) };
                m.w32(o + 0x1C + 8 * p, cop as u32);
                m.w32(o + 0x20 + 8 * p, at as u32);
                m.w32(o + 0x4C + 4 * p, pick(6) as u32);
            }
            m.w32(HIGH_LEVEL_AI_OBJS + 4 * i as u32, o);
        }
        GAME_SETUP.set_skill(m, pick(3));
        GAME_SETUP.set_race_type(m, [0, 1, 1, 5, 2][pick(5) as usize]);
        let this = pick(n) as u32;
        if pick(2) == 0 {
            // (an arrest scene: the object's car slow, with a crime long after its last
            // pull-over; a police car (or human cop) just ahead of it, about as slow)
            let (perp, cop) = (cars[this as usize], cars[pick(n) as usize]);
            let o = OBJS + 0x100 * this;
            m.w32(o + 0x68, (now - 2000 - pick(100)) as u32);
            m.w32(o + 0x78, [0, 1, 2, 3, 4][pick(5) as usize] as u32);
            perp.stats().set_finish_type(m, 0);
            perp.set_current_speed(m, pick(0x4_0000) - 0x2_0000);
            perp.n().set_speed_xz(m, pick(0x4_0000));
            perp.set_speed(m, [pick(0x4_0000), pick(0x10_0000)][pick(2) as usize]);
            let flag = [CAR_COP, CAR_HUMAN_COP | CAR_HUMAN][pick(2) as usize];
            cop.update_car_flags(m, |f| f | flag);
            cop.set_ai_flags(m, 2);
            cop.n().set_flight_time(m, 0);
            cop.n().set_active(m, 1);
            cop.set_current_speed(m, pick(0x4_0000) - 0x2_0000);
            cop.set_speed(m, [pick(0x4_0000), pick(0x10_0000)][pick(2) as usize]);
            let p = perp.n().position().get(m);
            let q = cop.n().position();
            q.set_x(m, p.x.wrapping_add(pick(0x14_0000) - 0xA_0000));
            q.set_y(m, p.y.wrapping_add(pick(0x2_0000) - 0x1_0000));
            q.set_z(m, p.z.wrapping_add(pick(0x14_0000) - 0x4_0000));
            let slice = perp.n().sim_road_info().slice(m);
            cop.n().sim_road_info().set_slice(m, slice);
            let lane = perp.lane_index(m) + pick(3) - 1;
            cop.set_lane_index(m, lane);
            if pick(2) == 0 {
                m.w32(o + 0x6C, 0);
            }
            let across = perp.road_position(m) + (0x1_8000 + pick(0x3_0000)) * [1, -1][pick(2) as usize];
            cop.set_road_position(m, across);
            m.w32(ARGS + 0x50, cop.0);
        } else {
            m.w32(ARGS + 0x50, cars[pick(n) as usize].0);
        }
        // (the police's blockades: a target object, near its car's slice, either way, any
        // elements and speech; the spike belt and road block, near the humans or not)
        for (i, &car) in cars.iter().enumerate() {
            let o = OBJS + 0x100 * i as u32;
            let target = cars[pick(n) as usize];
            let ti = target.car_index(m) as u32;
            m.w32(o + 0x24, [0, OBJS + 0x100 * ti][(pick(5) != 0) as usize]);
            m.w32(o + 0x28, pick(8) as u32);
            let slice = target.stats().slice(m);
            m.w32(o + 0x34, (slice + pick(80) - 40).max(0) as u32);
            m.w32(o + 0x38, [1, -1][pick(2) as usize] as u32);
            m.w16(o + 0x50, pick(8) as u16);
            if m.r32(ARGS + 0x50) != car.0 && i as u32 != this {
                car.set_road_position(m, [pick(0x10_0000) - 0x8_0000, pick(0x4_0000)][pick(2) as usize]);
            }
        }
        let belt = crate::globals::SPIKE_BELT;
        belt.set_active_(m, pick(2));
        let human = Car(m.r32(HUMAN_RACE_CARS));
        let slice = human.n().sim_road_info().slice(m) as i32;
        let slices = crate::globals::NUM_SLICES.get(m);
        belt.set_slice_(m, [(slice + pick(20) - 10).clamp(0, slices - 1), pick(slices as u32)][pick(2) as usize]);
        belt.set_freshen_time_(m, now - pick(640));
        crate::aih_basiccop::ROAD_BLOCK_STATE.set(m, [0, 1, 2][pick(3) as usize]);
        crate::aih_basiccop::CUSTOM_SLICE_NUM.set(m, [(slice + pick(20) - 10).clamp(0, slices - 1), pick(slices as u32)][pick(2) as usize]);
        // (a point near the road's centre, for placing on it)
        m.w32(ARGS + 0x40, (pick(0x10_0000) - 0x8_0000) as u32);
        m.w32(ARGS + 0x44, pick(0x1_0000) as u32);
        m.w32(ARGS + 0x48, (pick(0x10_0000) - 0x8_0000) as u32);
        let s = cars[this as usize].n().sim_road_info().slice(m) as i32 as u32;
        m.w32(ARGS + 0x4C, s);
        let args = [cars[pick(n) as usize].0, cars[pick(n) as usize].0, pick(ncops) as u32, pick(n) as u32, pick(2) as u32, pick(6) as u32, OBJS + 0x100 * this];
        for (k, a) in args.into_iter().enumerate() {
            m.w32(ARGS + 4 * k as u32, a);
        }
    }

    /// The shuffle's arguments: a car, another, a cop index, a car index, a cop type, a small
    /// number, an object.
    pub(crate) fn args(c: &mut Cpu) -> [u32; 7] {
        std::array::from_fn(|k| c.m.r32(ARGS + 4 * k as u32))
    }
}
