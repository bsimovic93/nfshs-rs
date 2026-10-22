//! AIHIGH.CPP: the AI's high level, an object per car (`highLevelAIObjs`) of the class its
//! role takes: a human (`AIHigh_Human`), an opponent, traffic, a police car, nothing; in a
//! "be the cop" race the human cops, their AI wingmen and the speeders they chase. Each tick
//! its virtual `HighExecute` runs, at the car's simulation rate, and moves the car's state
//! (AISTATE.CPP) on. The classes are in the AIH_*.CPP files; this one makes the objects and
//! runs them.

use crate::abi::{self, Saved};
use crate::aistate;
use crate::globals::{GAME_SETUP, HUMAN_RACE_CARS, NUM_HUMAN_RACE_CARS, SIM_COUNTER};
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
pub const VT_BTC_AI_PERP: u32 = 0x8005_5770;
pub const VT_BTC_HUMAN_PERP: u32 = 0x8005_5790;
pub const VT_BTC_PERP2: u32 = 0x8005_57B0;
pub const VT_PLAYER: u32 = 0x8005_5868;
pub const VT_HUMAN: u32 = 0x8005_5898;
pub const VT_OPPONENT: u32 = 0x8005_58C8;

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

/// `SetState` (inline): the object's state replaced by `state` of kind `kind`, the old one
/// deleted (its destructor's call returning to `ra`).
pub fn set_state(c: &mut Cpu, r: &Saved, this: u32, state: u32, kind: i32, ra: u32) {
    let old = c.m.r32(this.wrapping_add(4));
    delete_state(c, r, old, ra);
    c.m.w32(this.wrapping_add(4), state);
    c.m.w32(this.wrapping_add(8), kind as u32);
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


/// `operator new` (the library's): `size` bytes from the heap, called returning to `ra`.
fn new(c: &mut Cpu, r: &Saved, size: u32, ra: u32) -> u32 {
    r.call(c, nfs4rc::f_800a48c0, &[size], ra)
}

/// `AIHigh_StartUp` (AIHIGH.CPP, 0x8005b584): the states' pool; each car's object by its role:
/// in a "be the cop" race the human cops, human speeders, AI speeders, traffic and wingmen;
/// else humans, opponents, traffic and police; nothing for the rest. The cop game type by
/// them.
pub fn start_up(c: &mut Cpu) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[4]) = (0, 0);
        r.enter(c, 0x8007_01bc, 0x8005_b5b4, |c| aistate::start_up(&mut c.m));
        let btc = is_btc(&mut c.m);
        r.0[3] = 0;
        let (mut cops, mut human_cops) = (0u32, 0);
        let n = crate::globals::NUM_CARS.get(&mut c.m);
        for i in 0..n.max(0) as u32 {
            let m = &mut c.m;
            let car = Car(m.r32(crate::globals::CARS_LIST + 4 * i));
            (r.0[1], r.0[3]) = (car.0, i);
            let f = car.car_flags(m);
            let o = if btc {
                (r.0[5], r.0[6]) = (crate::globals::CARS_LIST + 4 * i, HIGH_LEVEL_AI_OBJS + 4 * i);
                if f & CAR_HUMAN_COP != 0 {
                    let p = new(c, &r, 140, 0x8005_b66c);
                    let index = cops as i32;
                    cops += 1;
                    r.0[2] = cops;
                    r.enter(c, 0x8005_d724, 0x8005_b680, |c| crate::aih_btccop::human_cop_construct(c, crate::types::AIHighBTCHumanCop(p), car, index)).0
                } else if f & CAR_HUMAN != 0 {
                    let p = new(c, &r, 136, 0x8005_b698);
                    r.0[0] = p;
                    let perp = crate::types::AIHighBTCPerp(p);
                    r.enter(c, 0x8005_c8e4, 0x8005_b6a8, |c| crate::aih_basicperp::construct(c, perp.ai_high_basic_perp(), car));
                    let m = &mut c.m;
                    set_vt(m, p, VT_BTC_PERP);
                    perp.set_caught_(m, 1);
                    perp.set_hud_activated_(m, 0);
                    perp.set_original_activation_cop_(m, crate::types::AIHighBTCHumanCop(0));
                    set_vt(m, p, VT_BTC_HUMAN_PERP);
                    p
                } else if f & CAR_OPPONENT != 0 {
                    let p = new(c, &r, 172, 0x8005_b6e0);
                    r.enter(c, 0x8006_0ae4, 0x8005_b6ec, |c| crate::aih_btcperp::ai_perp_construct(c, crate::types::AIHighBTCAIPerp(p), car)).0
                } else if f & CAR_TRAFFIC != 0 {
                    let p = new(c, &r, 36, 0x8005_b704);
                    r.enter(c, 0x8006_6ef8, 0x8005_b710, |c| crate::aih_traf::construct(c, crate::types::AIHighTraffic(p), car)).0
                } else if f & CAR_COP != 0 {
                    let p = new(c, &r, 124, 0x8005_b728);
                    let index = cops as i32;
                    cops += 1;
                    r.0[2] = cops;
                    r.enter(c, 0x8005_eae0, 0x8005_b73c, |c| crate::aih_btccop::wingman_construct(c, crate::types::AIHighBTCWingman(p), car, index)).0
                } else {
                    let p = new(c, &r, 24, 0x8005_b74c);
                    r.0[0] = p;
                    r.enter(c, 0x8005_bb08, 0x8005_b75c, |c| base_construct(c, AIHighBase(p), car));
                    set_vt(&mut c.m, p, VT_NONE);
                    p
                }
            } else {
                (r.0[4], r.0[5], r.0[6]) = (crate::globals::CARS_LIST + 4 * i, HIGH_LEVEL_AI_OBJS + 4 * i, VT_NONE);
                if f & CAR_HUMAN != 0 {
                    let p = new(c, &r, 176, 0x8005_b830);
                    r.enter(c, 0x8006_3a9c, 0x8005_b83c, |c| crate::aih_hum::construct(c, crate::types::AIHighHuman(p), car)).0
                } else if f & CAR_OPPONENT != 0 {
                    let p = new(c, &r, 192, 0x8005_b854);
                    r.enter(c, 0x8006_42d8, 0x8005_b860, |c| crate::aih_opp::construct(c, crate::types::AIHighOpponent(p), car)).0
                } else if f & CAR_TRAFFIC != 0 {
                    let p = new(c, &r, 36, 0x8005_b878);
                    r.enter(c, 0x8006_6ef8, 0x8005_b884, |c| crate::aih_traf::construct(c, crate::types::AIHighTraffic(p), car)).0
                } else if f & CAR_COP != 0 {
                    let p = new(c, &r, 108, 0x8005_b89c);
                    let index = cops as i32;
                    cops += 1;
                    r.0[2] = cops;
                    r.enter(c, 0x8006_43f0, 0x8005_b8b0, |c| crate::aih_cop::construct(c, crate::types::AIHighCop(p), car, index)).0
                } else {
                    let p = new(c, &r, 24, 0x8005_b8c0);
                    r.0[0] = p;
                    r.enter(c, 0x8005_bb08, 0x8005_b8d0, |c| base_construct(c, AIHighBase(p), car));
                    set_vt(&mut c.m, p, VT_NONE);
                    p
                }
            };
            if btc {
                // (s0 the object, whichever class: the next car's constructor saves it)
                r.0[0] = o;
            }
            let m = &mut c.m;
            m.w32(HIGH_LEVEL_AI_OBJS + 4 * i, o);
            if btc && car.car_flags(m) & CAR_HUMAN_COP != 0 {
                human_cops += 1;
                r.0[4] = human_cops;
            }
        }
        let m = &mut c.m;
        let kind = if btc {
            if human_cops == 2 {
                3
            } else if human_cops == 1 && cops == human_cops {
                4
            } else {
                2
            }
        } else {
            (cops > 0) as i32
        };
        COP_GAME_TYPE.set(m, kind);
    })
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(c);
}

/// `AIHigh_Restart1` (AIHIGH.CPP, 0x8005b92c): the objects freed.
pub fn abi_restart1(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8005_b974, 0x8005_b93c, clean_up);
    })
}

/// `AIHigh_Restart2` (AIHIGH.CPP, 0x8005b94c): the states and objects anew.
pub fn abi_restart2(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.enter(c, 0x8007_01fc, 0x8005_b95c, |c| aistate::start_up(&mut c.m));
        r.enter(c, 0x8005_b584, 0x8005_b964, start_up);
    })
}

/// `AIHigh_CleanUp` (AIHIGH.CPP, 0x8005b974): each car's object deleted (its virtual
/// destructor).
pub fn clean_up(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        for i in 0..crate::globals::NUM_CARS.get(&mut c.m).max(0) as u32 {
            (r.0[0], r.0[1]) = (HIGH_LEVEL_AI_OBJS + 4 * i, i);
            let o = c.m.r32(HIGH_LEVEL_AI_OBJS + 4 * i);
            if o != 0 {
                let (p, f) = virtual_fn(&mut c.m, o, 2);
                r.call_ptr(c, f, &[p, 3], 0x8005_b9c8);
                c.m.w32(HIGH_LEVEL_AI_OBJS + 4 * i, 0);
            }
        }
    })
}

pub fn abi_clean_up(c: &mut Cpu) {
    clean_up(c);
}

/// `AIHigh_Execute` (AIHIGH.CPP, 0x8005ba00, each AI tick): each car's object's virtual
/// `HighExecute`, at the car's simulation rate (`Sched_ExecuteCheck` sets the AI's time step)
/// unless its scheduling is off.
pub fn execute(c: &mut Cpu) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        let mut i = 0;
        while i < crate::globals::NUM_CARS.get(&mut c.m) {
            let m = &mut c.m;
            let at = 4 * i as u32;
            (r.0[0], r.0[2], r.0[3]) = (HIGH_LEVEL_AI_OBJS + at, crate::globals::CARS_LIST + at, i as u32);
            let o = m.r32(HIGH_LEVEL_AI_OBJS + at);
            let car = Car(m.r32(crate::globals::CARS_LIST + at));
            i += 1;
            if o == 0 {
                continue;
            }
            // (the virtual call's a1..a3 as the original leaves them: the car, or what
            // Sched_ExecuteCheck leaves; a callee may home them)
            let mut args = [0, c.r[5], c.r[6], car.0];
            let run = if AIHighBase(o).scheduling_off_(m) != 0 {
                true
            } else {
                r.0[1] = 0;
                m.w32(sp + 16, AI_TIME);
                m.w32(sp + 20, AI_ELAPSED);
                m.w32(sp + 24, AI_I_TIME);
                let full = car.force_no_sim_optz(m);
                m.w32(sp + 28, full as u32);
                let (dist, id) = (car.n().dist_to_player(m), m.r32(car.0) as i32);
                let step = if SIM_COUNTER.get(m) < 16 {
                    dist
                } else if full != 0 {
                    0
                } else {
                    (dist / 16 / 0x1_0000).min(19)
                };
                args[1..].copy_from_slice(&[0, step as u32, id as u32]);
                r.enter(c, 0x800b_6e90, 0x8005_baa4, |c| crate::schedule::execute_check(c, 1, 0, dist, id, AI_TIME, AI_ELAPSED, AI_I_TIME, full))
            };
            r.0[1] = run as u32;
            if run {
                let (p, f) = virtual_fn(&mut c.m, o, 1);
                args[0] = p;
                r.call_ptr(c, f, &args, 0x8005_badc);
            }
        }
    })
}

pub fn abi_execute(c: &mut Cpu) {
    execute(c);
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, COP_CARS, NUM_CARS, NUM_COP_CARS};
    use crate::types::AIStateBase;
    use crate::test_util::{Rng, against_original, same_stack};

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

    /// More arguments of the "be the cop" shuffle: a human cop's object, an AI police car's
    /// (a wingman), a speeder's.
    pub(crate) const BTC_ARGS: u32 = ARGS + 0x70;

    /// The shuffle as a "be the cop" race: one or two human cops (the first humans), AI police
    /// cars (wingmen) and speeders (AI or human); their objects of those classes with any
    /// chase, freeze, stage, time, wingman request and role, boss, spike belt; each object in
    /// a state of any kind (built by the original's constructors: none, driving, idle, a chase
    /// after another car, not active, going to a slice); any cop game type.
    pub(crate) fn btc_shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let m = &mut c.m;
        let n = NUM_CARS.get(m) as u32;
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let now = SIM_COUNTER.get(m);
        GAME_SETUP.set_race_type(m, [1, 5][pick(2) as usize]);
        COP_GAME_TYPE.set(m, [2, 2, 3, 4, 1][pick(5) as usize]);
        let humans = if pick(3) == 0 { 2 } else { 1 };
        NUM_HUMAN_RACE_CARS.set(m, humans);
        // (roles: the human cops first, then AI police cars and speeders)
        let role: Vec<i32> = (0..n as usize).map(|i| if i < humans as usize { 0 } else { [1, 2, 2, 3][pick(4) as usize] }).collect();
        let obj_of = |i: usize| OBJS + 0x100 * i as u32;
        let perps: Vec<usize> = (0..n as usize).filter(|&i| role[i] >= 2).collect();
        let hcs: Vec<usize> = (0..n as usize).filter(|&i| role[i] == 0).collect();
        let wingmen: Vec<usize> = (0..n as usize).filter(|&i| role[i] == 1).collect();
        let mut race = Vec::new();
        for (i, &car) in cars.iter().enumerate() {
            let f = match role[i] {
                0 => CAR_HUMAN | CAR_HUMAN_COP,
                1 => CAR_COP,
                2 => CAR_OPPONENT,
                _ => CAR_HUMAN | 2,
            };
            car.update_car_flags(m, |x| x & !(CAR_HUMAN | 2 | CAR_HUMAN_COP | CAR_OPPONENT | CAR_TRAFFIC | CAR_COP) | f);
            if role[i] == 0 {
                m.w32(HUMAN_RACE_CARS + 4 * i as u32, car.0);
            }
            if role[i] != 1 {
                race.push(car);
            }
        }
        crate::globals::NUM_RACE_CARS.set(m, race.len() as i32);
        for (k, car) in race.iter().enumerate() {
            m.w32(crate::globals::RACE_CARS + 4 * k as u32, car.0);
        }
        // (the states' pool all free)
        for i in 0..10 {
            m.w32(0x8010_F0B0 + 4 * i, 0);
        }
        let mut cops = 0;
        for (i, &car) in cars.iter().enumerate() {
            let m = &mut c.m;
            let o = obj_of(i);
            let some = |p: &mut dyn FnMut(u32) -> i32, v: &Vec<usize>| if v.is_empty() || p(4) == 0 { 0 } else { obj_of(v[p(v.len() as u32) as usize]) };
            match role[i] {
                0 | 1 => {
                    set_vt(m, o, if role[i] == 0 { VT_BTC_HUMAN_COP } else { VT_BTC_WINGMAN });
                    m.w32(o + 0x18, pick(2) as u32);
                    m.w32(o + 0x1C, if role[i] == 0 { i as u32 } else { cops });
                    cops += 1;
                    let p = some(&mut pick, &perps);
                    m.w32(o + 0x58, p);
                    m.w32(o + 0x5C, pick(6) as u32);
                    m.w32(o + 0x60, [0, 0, 1, 2, 3, 4][pick(6) as usize] as u32);
                    if role[i] == 0 {
                        m.w32(o + 0x64, (pick(5) - 1) as u32);
                        m.w32(o + 0x68, pick(3) as u32);
                        m.w32(o + 0x6C, [0x1_0000, 0x8000, 0x1_8000][pick(3) as usize]);
                        m.w32(o + 0x70, [pick(400) - 300, pick(4000)][pick(2) as usize] as u32);
                        m.w32(o + 0x74, [0, now - pick(4000)][pick(2) as usize] as u32);
                        m.w32(o + 0x78, pick(6) as u32);
                        m.w32(o + 0x7C, pick(2) as u32);
                        m.w32(o + 0x80, [1, -1][pick(2) as usize] as u32);
                        m.w32(o + 0x84, pick(2) as u32);
                        m.w32(o + 0x88, pick(0x40_0000) as u32);
                    } else {
                        m.w32(o + 0x64, pick(4) as u32);
                        m.w32(o + 0x68, pick(4) as u32);
                        let boss = some(&mut pick, &hcs);
                        m.w32(o + 0x6C, boss);
                        m.w32(o + 0x70, pick(2) as u32);
                        let belt = crate::globals::SPIKE_BELT.slice_(m);
                        m.w32(o + 0x74, [belt, pick(500)][pick(2) as usize] as u32);
                        m.w32(o + 0x78, pick(0x8_0000) as u32);
                    }
                }
                _ => {
                    set_vt(m, o, if role[i] == 2 { VT_BTC_AI_PERP } else { VT_BTC_HUMAN_PERP });
                    m.w32(o + 0x7C, pick(2) as u32);
                    m.w32(o + 0x80, pick(2) as u32);
                    let hc = some(&mut pick, &hcs);
                    m.w32(o + 0x84, hc);
                }
            }
            // (its state, of a kind)
            let kind = [0, 2, 3, 4, 4, 7, 9][pick(7) as usize];
            let st = crate::aistate::pool_new(m);
            let target = cars[pick(n) as usize];
            match kind {
                2 => {
                    crate::test_util::run(c, nfs4rc::f_800703bc, &[st, car.0]);
                }
                4 => {
                    let rel = ARGS + 0x60;
                    c.m.w32(rel, (pick(0x8_0000) - 0x4_0000) as u32);
                    c.m.w32(rel + 8, (pick(0x10_0000) - 0x8_0000) as u32);
                    crate::test_util::run(c, nfs4rc::f_800704dc, &[st, car.0, target.0, rel, 512, 0x3C_0000, 0x19_0000, 2, 0x1_0000]);
                    let ch = crate::types::AIStateChase(st);
                    let m = &mut c.m;
                    ch.set_barrier_ticks32_(m, [0, 0, 170][pick(3) as usize]);
                    ch.set_in_target_region_(m, pick(20));
                    ch.set_lat_meters_between_(m, pick(0x20_0000) - 0x10_0000);
                    ch.set_long_meters_between_(m, pick(0x20_0000) - 0x10_0000);
                }
                9 => {
                    let slice = car.n().sim_road_info().slice(&mut c.m) as i32;
                    crate::test_util::run(c, nfs4rc::f_80072c4c, &[st, car.0, (slice + pick(10) - 5).max(0) as u32, 0]);
                }
                _ => {
                    let m = &mut c.m;
                    crate::aistate::base_construct(m, AIStateBase(st), car);
                    let vt = match kind {
                        3 => 0x8005_5DD0,
                        7 => 0x8005_56F4,
                        _ => VT_STATE_NONE,
                    };
                    m.w32(st + 4, vt);
                    m.w32(st + 12, 1);
                }
            }
            let m = &mut c.m;
            m.w32(o + 4, st);
            m.w32(o + 8, kind as u32);
        }
        let m = &mut c.m;
        let one = |p: &mut dyn FnMut(u32) -> i32, v: &Vec<usize>| if v.is_empty() { 0 } else { obj_of(v[p(v.len() as u32) as usize]) };
        let a = [one(&mut pick, &hcs), one(&mut pick, &wingmen), one(&mut pick, &perps)];
        for (k, v) in a.into_iter().enumerate() {
            m.w32(BTC_ARGS + 4 * k as u32, v);
        }
    }

    /// The pursuit shuffle's police and traffic trigger managers, their triggers, a traffic
    /// path, an accident scene.
    pub(crate) const COP_MANAGER: u32 = 0x8014_2000;
    pub(crate) const TRAFFIC_MANAGER: u32 = 0x8014_2400;
    const COP_TRIGGERS: u32 = 0x8014_2800;
    const TRAFFIC_TRIGGERS: u32 = 0x8014_2E00;
    const PATH: u32 = 0x8014_3400;
    const SCENE: u32 = 0x8014_3600;
    /// More arguments of the pursuit shuffle: a player's object, a police car's, a traffic
    /// car's (0 if none).
    pub(crate) const PURSUIT_ARGS: u32 = ARGS + 0x80;

    /// A trigger manager of `n` triggers at `at` (0x50 bytes apart) of the kinds `kinds` near
    /// `slice`, some resting.
    fn triggers(m: &mut ps1rt::Mem, pick: &mut dyn FnMut(u32) -> i32, manager: u32, at: u32, n: u32, kinds: &[i32], slice: i32, now: i32) {
        let mgr = crate::types::AITriggerTriggerManager(manager);
        mgr.set_num_triggers_(m, n as i32);
        let slices = crate::globals::NUM_SLICES.get(m);
        let mut s = (slice - 20 + pick(10)).max(0);
        for i in 0..n {
            let t = at + 0x50 * i;
            for w in (0..0x50).step_by(4) {
                m.w32(t + w, [0, 1, pick(0x10_0000) as u32][pick(3) as usize]);
            }
            let kind = kinds[pick(kinds.len() as u32) as usize];
            m.w32(t, kind as u32);
            s = (s + 1 + pick(8)).min(slices - 1);
            m.w32(t + 4, s as u32);
            m.w32(t + 8, [1, -1, 0][pick(3) as usize] as u32);
            for k in 0..9 {
                m.w32(t + 0x18 + 4 * k, if k % 4 == 0 { 0x1_0000 } else { 0 });
            }
            m.w32(t + 0x38, (1 + pick(3)) as u32);
            if kind == 5 {
                m.w32(t + 0x3C, PATH);
            }
            mgr.set_triggers_(m, i, t);
        }
        for i in 0..100 {
            mgr.set_check_time_(m, i, now - [0, 2561, pick(6000)][pick(3) as usize]);
        }
        for i in 0..9 {
            mgr.set_last_trigger_checked_(m, i, pick(n.max(1)));
        }
    }

    /// The shuffle as a pursuit race: humans and opponents (players, the race cars), police
    /// cars of either type and traffic; their objects of those classes: the players' police
    /// game (any level, engagement, warnings, busts, police-free time; opponents attacking or
    /// not), the police's chase, blockade (any mode, kind, place), spike belt, aggression,
    /// traffic held or ignoring the police, at an accident; each in a state of a kind its
    /// class has (built by the original's constructors); police and roving traffic triggers
    /// near the cars, a traffic path; any laps, police or not.
    pub(crate) fn pursuit_shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let m = &mut c.m;
        let n = NUM_CARS.get(m) as u32;
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        let now = SIM_COUNTER.get(m);
        GAME_SETUP.set_race_type(m, 0);
        GAME_SETUP.set_cops(m, [1, 1, 0][pick(3) as usize]);
        GAME_SETUP.set_num_laps(m, [2, 4, 3][pick(3) as usize]);
        GAME_SETUP.set_comm_mode(m, pick(3));
        let humans = if pick(3) == 0 { 2.min(n as i32) } else { 1 };
        NUM_HUMAN_RACE_CARS.set(m, humans);
        // (roles: humans first, then opponents, police and traffic)
        let role: Vec<i32> = (0..n as usize).map(|i| if i < humans as usize { 0 } else { [1, 2, 2, 3][pick(4) as usize] }).collect();
        let obj_of = |i: usize| OBJS + 0x100 * i as u32;
        let players: Vec<usize> = (0..n as usize).filter(|&i| role[i] <= 1).collect();
        let (mut race, mut cops, mut traffic) = (Vec::new(), Vec::new(), Vec::new());
        for (i, &car) in cars.iter().enumerate() {
            let f = [CAR_HUMAN | 2, CAR_OPPONENT, CAR_COP | [0, 0x40][pick(2) as usize], CAR_TRAFFIC][role[i] as usize];
            car.update_car_flags(m, |x| x & !(CAR_HUMAN | 2 | CAR_HUMAN_COP | CAR_OPPONENT | CAR_TRAFFIC | CAR_COP | 0x40 | 0x400) | f | [0, 0, 0x400][pick(3) as usize] * (role[i] >= 2) as i32);
            match role[i] {
                0 => {
                    m.w32(HUMAN_RACE_CARS + 4 * i as u32, car.0);
                    race.push(car);
                }
                1 => race.push(car),
                2 => {
                    car.set_ai_flags(m, [2, 0xC, 8, 0xC, 0xA, 4][pick(6) as usize]);
                    cops.push(car);
                }
                _ => traffic.push(car),
            }
            car.set_wipe_out_end_tick(m, now + pick(1400) - 700);
            car.stats().set_num_fines(m, pick(4));
            car.stats().set_num_warnings(m, pick(3));
            car.stats().set_num_arrests(m, [0, 0, 1][pick(3) as usize]);
            car.set_pull_over(m, pick(2));
            car.n().set_sim_optz(m, [0, 0, 1][pick(3) as usize]);
            car.set_desired_direction(m, [1, -1][pick(2) as usize]);
        }
        for (list, count, v) in [(crate::globals::RACE_CARS, crate::globals::NUM_RACE_CARS, &race), (COP_CARS, NUM_COP_CARS, &cops), (crate::globals::TRAFFIC_CARS, crate::globals::NUM_TRAFFIC_CARS, &traffic)] {
            count.set(m, v.len() as i32);
            for (k, car) in v.iter().enumerate() {
                m.w32(list + 4 * k as u32, car.0);
            }
        }
        // (the trigger managers, near a car's slice)
        let slice = cars[pick(n) as usize].n().sim_road_info().slice(m) as i32;
        let k = pick(10) as u32;
        let any = pick(4);
        triggers(m, &mut pick, COP_MANAGER, COP_TRIGGERS, k, &[1, 2, 3, any], slice, now);
        let k = pick(10) as u32;
        let any = pick(6);
        triggers(m, &mut pick, TRAFFIC_MANAGER, TRAFFIC_TRIGGERS, k, &[5, 5, any], slice, now);
        m.w32(0x8013_D8D8, [COP_MANAGER, COP_MANAGER, 0][pick(3) as usize]);
        m.w32(0x8013_D8DC, [TRAFFIC_MANAGER, TRAFFIC_MANAGER, 0][pick(3) as usize]);
        let p = cars[0].n().position().get(m);
        for k in 0..4 {
            crate::types::Coord(PATH + 20 * k).set(m, p);
            m.w32(PATH + 20 * k + 12, pick(0x20_0000) as u32);
            m.w32(PATH + 20 * k + 16, pick(100) as u32);
        }
        crate::types::Coord(SCENE + 16).set(m, p);
        for k in 0..9 {
            m.w32(SCENE + 32 + 4 * k, if k % 4 == 0 { 0x1_0000 } else { 0 });
        }
        // (the pool free for the states)
        for i in 0..10 {
            m.w32(0x8010_F0B0 + 4 * i, 0);
        }
        let mut cop_index = 0;
        for (i, &car) in cars.iter().enumerate() {
            let m = &mut c.m;
            let o = obj_of(i);
            let some_player = |p: &mut dyn FnMut(u32) -> i32| if players.is_empty() || p(4) == 0 { 0 } else { obj_of(players[p(players.len() as u32) as usize]) };
            let kind;
            match role[i] {
                0 | 1 => {
                    set_vt(m, o, if role[i] == 0 { VT_HUMAN } else { VT_OPPONENT });
                    m.w32(o + 0x70, pick(3) as u32);
                    m.w32(o + 0x74, pick(3) as u32);
                    m.w32(o + 0x7C, pick(3) as u32);
                    m.w32(o + 0x80, pick(5) as u32);
                    m.w32(o + 0x84, pick(0x1_0000) as u32);
                    let s = car.n().sim_road_info().slice(m) as i32;
                    m.w32(o + 0x88, (s + pick(80) - 40) as u32);
                    let game = 0x8010_E178 + 8 * pick(6) as u32;
                    let levels = m.r32(game) as i32;
                    let level = pick(levels.max(1) as u32);
                    m.w32(o + 0x8C, [pick(0x8_0000) - 0x3_0000, pick(0x100_0000)][pick(2) as usize] as u32);
                    m.w32(o + 0x90, game);
                    m.w32(o + 0x94, level as u32);
                    m.w32(o + 0x98, [level, pick(levels.max(1) as u32), -1][pick(3) as usize] as u32);
                    let at = m.r32(game + 4).wrapping_add(0x34 * level as u32);
                    m.w32(o + 0x9C, at);
                    m.w32(o + 0xA0, pick(0x10_0000) as u32);
                    m.w32(o + 0xA4, pick(2) as u32);
                    m.w32(o + 0xA8, pick(0x800) as u32);
                    m.w32(o + 0xAC, [pick(100), 700 + pick(300)][pick(2) as usize] as u32);
                    if role[i] == 1 {
                        m.w32(o + 0xB0, pick(4) as u32);
                        m.w32(o + 0xB4, [0, cars[0].0][pick(2) as usize]);
                        m.w32(o + 0xB8, pick(6) as u32);
                        m.w32(o + 0xBC, (pick(200) - 50) as u32);
                    }
                    kind = [0, 2, 2, 4][pick(4) as usize];
                }
                2 => {
                    set_vt(m, o, crate::aih_cop::VT_COP);
                    m.w32(o + 0x18, pick(2) as u32);
                    m.w32(o + 0x1C, cop_index);
                    cop_index += 1;
                    m.w32(o + 0x20, pick(5) as u32);
                    let target = some_player(&mut pick);
                    m.w32(o + 0x24, target);
                    m.w32(o + 0x28, pick(8) as u32);
                    m.w32(o + 0x2C, pick(4) as u32);
                    m.w32(o + 0x30, [-1, slice + pick(20)][pick(2) as usize] as u32);
                    m.w32(o + 0x34, (slice + pick(40) - 20).max(0) as u32);
                    m.w32(o + 0x38, [1, -1][pick(2) as usize] as u32);
                    m.w32(o + 0x3C, (pick(0x10_0000) - 0x8_0000) as u32);
                    m.w32(o + 0x40, (pick(512) - 256) as u32);
                    m.w32(o + 0x44, pick(2) as u32);
                    m.w32(o + 0x48, pick(0x4_0000) as u32);
                    m.w32(o + 0x4C, (pick(200) - 100) as u32);
                    m.w16(o + 0x50, pick(8) as u16);
                    m.w32(o + 0x54, pick(3) as u32);
                    let perp = some_player(&mut pick);
                    m.w32(o + 0x58, perp);
                    m.w32(o + 0x5C, [0, 0, 1][pick(3) as usize] as u32);
                    m.w32(o + 0x60, pick(6) as u32);
                    m.w32(o + 0x64, [-1, slice + pick(20)][pick(2) as usize] as u32);
                    m.w32(o + 0x68, pick(3) as u32);
                    kind = [0, 1, 1, 2, 3, 4, 4, 5, 9][pick(9) as usize];
                }
                _ => {
                    set_vt(m, o, crate::aih_traf::VT_TRAFFIC);
                    m.w32(o + 0x18, pick(2) as u32);
                    m.w32(o + 0x1C, [0, 0, 1][pick(3) as usize] as u32);
                    m.w32(o + 0x20, [0, 0, SCENE][pick(3) as usize]);
                    kind = [0, 1, 1, 2, 2, 3, 6][pick(7) as usize];
                }
            }
            // (its state, of the kind)
            let st = crate::aistate::pool_new(m);
            let target = cars[pick(n) as usize];
            match kind {
                1 => {
                    crate::test_util::run(c, nfs4rc::f_80071d6c, &[st, car.0]);
                }
                2 => {
                    crate::test_util::run(c, nfs4rc::f_800703bc, &[st, car.0]);
                }
                4 => {
                    let rel = ARGS + 0x60;
                    c.m.w32(rel, (pick(0x8_0000) - 0x4_0000) as u32);
                    c.m.w32(rel + 8, (pick(0x10_0000) - 0x8_0000) as u32);
                    crate::test_util::run(c, nfs4rc::f_800704dc, &[st, car.0, target.0, rel, 512, 0x3C_0000, 0x19_0000, 2, 0x1_0000]);
                    let ch = crate::types::AIStateChase(st);
                    let m = &mut c.m;
                    ch.set_barrier_ticks32_(m, [0, 0, 170][pick(3) as usize]);
                    ch.set_in_target_region_(m, pick(20));
                    ch.set_lat_meters_between_(m, pick(0x20_0000) - 0x10_0000);
                    ch.set_long_meters_between_(m, pick(0x20_0000) - 0x10_0000);
                    ch.set_murder_mode_(m, [0, 0, 1][pick(3) as usize]);
                    ch.set_slow_down_end_time_(m, now + pick(400) - 200);
                }
                5 => {
                    let s = car.n().sim_road_info().slice(&mut c.m) as u32;
                    c.m.w32(ARGS + 0x60 - 0x10, 0);
                    crate::test_util::run(c, nfs4rc::f_80071a00, &[st, car.0, s, SCENE + 16, SCENE + 32, pick(0x40_0000) as u32, pick(0x4_0000) as u32, s + pick(20) as u32]);
                }
                6 if c.m.r32(TRAFFIC_TRIGGERS) == 5 => {
                    crate::test_util::run(c, nfs4rc::f_8007212c, &[st, car.0, TRAFFIC_TRIGGERS]);
                }
                9 => {
                    let s = car.n().sim_road_info().slice(&mut c.m) as i32;
                    crate::test_util::run(c, nfs4rc::f_80072c4c, &[st, car.0, (s + pick(10) - 5).max(0) as u32, 0]);
                }
                _ => {
                    let m = &mut c.m;
                    crate::aistate::base_construct(m, AIStateBase(st), car);
                    m.w32(st + 4, if kind == 3 { 0x8005_5DD0 } else { VT_STATE_NONE });
                    m.w32(st + 12, 1);
                }
            }
            let m = &mut c.m;
            m.w32(o + 4, st);
            m.w32(o + 8, kind as u32);
        }
        let m = &mut c.m;
        let one = |p: &mut dyn FnMut(u32) -> i32, r: i32| {
            let v: Vec<usize> = (0..n as usize).filter(|&i| if r == 0 { role[i] <= 1 } else { role[i] == r }).collect();
            if v.is_empty() { 0 } else { obj_of(v[p(v.len() as u32) as usize]) }
        };
        let a = [one(&mut pick, 0), one(&mut pick, 2), one(&mut pick, 3), one(&mut pick, 1)];
        for (k, v) in a.into_iter().enumerate() {
            m.w32(PURSUIT_ARGS + 4 * k as u32, v);
        }
    }

    /// The pursuit arguments: a player's object, a police car's, a traffic car's, an
    /// opponent's (the first object when there is none).
    pub(crate) fn pursuit(c: &mut Cpu) -> [u32; 4] {
        std::array::from_fn(|k| match c.m.r32(PURSUIT_ARGS + 4 * k as u32) {
            0 => OBJS,
            v => v,
        })
    }

    /// The shuffle's arguments: a car, another, a cop index, a car index, a cop type, a small
    /// number, an object.
    pub(crate) fn args(c: &mut Cpu) -> [u32; 7] {
        std::array::from_fn(|k| c.m.r32(ARGS + 4 * k as u32))
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    /// The shuffles the whole AI's entries run on: the race's, "be the cop"'s, a pursuit's;
    /// each object with a state of its own (in a race no two share one: the first's tick
    /// could delete it under the second), each police car's type 0 or 1 (it indexes the
    /// blockade's stack arrays: out of range, the original's saved registers go).
    pub(crate) const SHUFFLES: [(&str, fn(&mut Cpu, &mut Rng)); 3] = [("race", |c, r| own_states(c, r, shuffle)), ("btc", |c, r| own_states(c, r, crate::aih_btccop::tests::shuffle)), ("pursuit", |c, r| own_states(c, r, pursuit_shuffle))];

    fn own_states(c: &mut Cpu, rng: &mut Rng, s: fn(&mut Cpu, &mut Rng)) {
        s(c, rng);
        let m = &mut c.m;
        let mut seen = Vec::new();
        for i in 0..NUM_CARS.get(m) {
            let o = obj(m, i);
            let car = m.r32(CARS_LIST + 4 * i as u32);
            let police = (0..NUM_COP_CARS.get(m) as u32).any(|k| m.r32(COP_CARS + 4 * k) == car);
            if o != 0 && (police || Car(car).car_flags(m) & CAR_COP != 0) {
                let t = m.r32(o + 0x18);
                m.w32(o + 0x18, t & 1);
            }
            let st = m.r32(o.wrapping_add(4));
            if st == 0 || !seen.contains(&st) {
                seen.push(st);
                continue;
            }
            let copy = aistate::pool_new(m);
            for w in (0..160).step_by(4) {
                let v = m.r32(st + w);
                m.w32(copy + w, v);
            }
            m.w32(o + 4, copy);
            seen.push(copy);
        }
    }

    /// A shuffle's objects made anew by the original `AIHigh_StartUp` (from the heap), for
    /// what deletes them.
    fn started(c: &mut Cpu, rng: &mut Rng, s: fn(&mut Cpu, &mut Rng)) {
        s(c, rng);
        crate::test_util::run(c, nfs4rc::f_8005b584, &[]);
    }

    fn started_race(c: &mut Cpu, rng: &mut Rng) {
        started(c, rng, SHUFFLES[0].1);
    }

    fn started_btc(c: &mut Cpu, rng: &mut Rng) {
        started(c, rng, SHUFFLES[1].1);
    }

    #[test]
    fn core_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7]) -> Vec<u32>, Port); 4] = [
            ("StartUp", nfs4rc::f_8005b584, |_, _| vec![], |c, _| {
                start_up(c);
                None
            }),
            ("Restart2", nfs4rc::f_8005b94c, |_, _| vec![], |c, _| {
                abi_restart2(c);
                None
            }),
            ("Execute", nfs4rc::f_8005ba00, |_, _| vec![], |c, _| {
                execute(c);
                None
            }),
            ("BaseConstruct", nfs4rc::f_8005bb08, |_, a| vec![OBJS + 0x700, a[0]], |c, a| Some(base_construct(c, AIHighBase(a[0]), Car(a[1])).0)),
        ];
        for (j, (sname, s)) in SHUFFLES.into_iter().enumerate() {
            for (k, (name, original, call, port)) in cases.iter().copied().enumerate() {
                against_original(&format!("{name} ({sname})"), 400, 0x3B10 + 16 * j as u64 + k as u64, s, original, &|c| {
                    let a = args(c);
                    call(c, a)
                }, port);
            }
        }
    }

    /// Deleting the objects (`AIHigh_StartUp`'s, from the heap) and their states.
    #[test]
    fn core_delete_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7]) -> Vec<u32>, Port); 6] = [
            ("CleanUp", nfs4rc::f_8005b974, |_, _| vec![], |c, _| {
                clean_up(c);
                None
            }),
            ("Restart1", nfs4rc::f_8005b92c, |_, _| vec![], |c, _| {
                abi_restart1(c);
                None
            }),
            ("BaseDestruct", nfs4rc::f_8005bbb4, |c, a| vec![obj(&mut c.m, a[3] as i32 % 3), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_base_destruct(c);
                None
            }),
            ("NoneDestruct", nfs4rc::f_8005bc58, |c, a| vec![obj(&mut c.m, a[3] as i32 % 3), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_none_destruct(c);
                None
            }),
            ("BTCPerpDestruct", nfs4rc::f_8005bc28, |c, a| vec![obj(&mut c.m, a[3] as i32 % 3), a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_btc_perp_destruct(c);
                None
            }),
            ("StateNoneDestruct", nfs4rc::f_8005bc80, |c, a| {
                let o = obj(&mut c.m, a[3] as i32 % 3);
                vec![c.m.r32(o + 4), a[4] * 3]
            }, |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_state_none_destruct(c);
                None
            }),
        ];
        for (j, (sname, s)) in [("race", started_race as fn(&mut Cpu, &mut Rng)), ("btc", started_btc)].into_iter().enumerate() {
            for (k, (name, original, call, port)) in cases.iter().copied().enumerate() {
                against_original(&format!("{name} ({sname})"), 300, 0x3B60 + 16 * j as u64 + k as u64, s, original, &|c| {
                    let a = args(c);
                    call(c, a)
                }, port);
            }
        }
    }

    /// The stack the AI's tick and start leave (each object's, each state's frames).
    #[test]
    fn core_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 3] = [
            ("Execute", 0x8005_ba00, nfs4rc::f_8005ba00),
            ("StartUp", 0x8005_b584, nfs4rc::f_8005b584),
            ("Restart2", 0x8005_b94c, nfs4rc::f_8005b94c),
        ];
        for (j, (sname, s)) in SHUFFLES.into_iter().enumerate() {
            for (k, (name, addr, original)) in cases.into_iter().enumerate() {
                same_stack(&format!("{name} ({sname})"), addr, original, 0x8006_0000, 0x801F_FF00, 200, 0x3BA0 + 16 * j as u64 + k as u64, s, &|_, _| vec![]);
            }
        }
        for (j, (sname, s)) in [("race", started_race as fn(&mut Cpu, &mut Rng)), ("btc", started_btc)].into_iter().enumerate() {
            same_stack(&format!("CleanUp ({sname})"), 0x8005_b974, nfs4rc::f_8005b974, 0x8006_0000, 0x801F_FF00, 200, 0x3BE0 + j as u64, s, &|_, _| vec![]);
        }
    }
}
