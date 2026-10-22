//! AIH_TRAF.CPP: `AIHigh_Traffic`, a traffic car: purgatory until it may live (at an accident
//! scene, on a roving traffic trigger, or reborn where the race needs it); driving until out
//! of the race's life; pulling over for a police car stopped or racing past (or ignoring
//! police for good, by chance); waiting until the police are gone.

use crate::abi::{self, Saved};
use crate::aih_basiccop::fast_random_step;
use crate::aihigh::{self, obj};
use crate::aiworld::apx_spline_distance;
use crate::globals::{BWORLDSM_SLICES, COP_CARS, NUM_CARS, NUM_COP_CARS, NUM_SLICES, TOTAL_SORTED_LIST};
use crate::types::{AIHighTraffic, AIStateBase, AIStateIdle, AITriggerTriggerManager, Car, Coord, Matrix};
use ps1rt::{Cpu, Mem};

/// The class's vtable.
pub const VT_TRAFFIC: u32 = 0x8005_5980;
/// `triggerManagerTraffic`: the roving traffic's trigger manager.
const TRIGGER_MANAGER_TRAFFIC: u32 = 0x8013_D8DC;
const VT_STATE_IDLE: u32 = 0x8005_5DD0;

/// The state kinds (`stateType_`).
const STATE_PURGATORY: i32 = 1;
const STATE_NORMAL: i32 = 2;
const STATE_IDLE: i32 = 3;
const STATE_ROVING: i32 = 6;

/// `Car_tObj.carFlags`: out of the race (parked at an accident or after one).
const CAR_OUT: i32 = 0x400;
/// `Car_tObj.AIFlags`: parked.
const AI_PARKED: i32 = 0x4;

fn car_of(m: &mut Mem, this: AIHighTraffic) -> Car {
    Car(m.r32(this.0))
}

/// `AIHigh_Traffic::CheckForCops` (AIH_TRAF.CPP, 0x8006648c): the closest police car on the road
/// along it (its distance in `dist`), or none.
pub fn check_for_cops(c: &mut Cpu, this: AIHighTraffic, dist: u32) -> Car {
    abi::frame(c, 48, |c, _| {
        let m = &mut c.m;
        let car = car_of(m, this);
        let mut best = Car(0);
        m.w32(dist, 0x4E20_0000);
        for i in 0..NUM_COP_CARS.get(m) {
            let cop = Car(m.r32(COP_CARS + 4 * i as u32));
            if cop.ai_flags(m) & AI_PARKED != 0 {
                continue;
            }
            let d = apx_spline_distance(m, car, cop).wrapping_abs();
            if d < m.r32(dist) as i32 {
                best = cop;
                m.w32(dist, d as u32);
            }
        }
        best
    })
}

pub fn abi_check_for_cops(c: &mut Cpu) {
    c.r[2] = check_for_cops(c, AIHighTraffic(c.r[4]), c.r[5]).0;
}

/// `AIHigh_Traffic::CopCheck` (AIH_TRAF.CPP, 0x80066560): the closest police car within 75 m:
/// stopped (`stopped` set; its object when it chases someone), or going over 2 m/s (its
/// object); else none.
pub fn cop_check(c: &mut Cpu, this: AIHighTraffic, stopped: u32) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = stopped;
        c.m.w32(stopped, 0);
        r.0[0] = 0;
        if NUM_COP_CARS.get(&mut c.m) == 0 {
            return 0;
        }
        let cop = r.enter(c, 0x8006_648c, 0x8006_6594, |c| check_for_cops(c, this, sp + 16));
        if cop.0 == 0 {
            return 0;
        }
        let m = &mut c.m;
        let speed = cop.current_speed(m).wrapping_abs();
        let near = m.r32(sp + 16) as i32 <= 0x4A_FFFF;
        if speed <= 0x1_FFFF {
            if near {
                m.w32(stopped, 1);
                let index = cop.car_index(m);
                let o = obj(m, index);
                return if m.r32(o + 0x58) != 0 { o } else { 0 };
            }
            return 0;
        }
        if !near {
            return 0;
        }
        let index = cop.car_index(m);
        obj(m, index)
    })
}

pub fn abi_cop_check(c: &mut Cpu) {
    c.r[2] = cop_check(c, AIHighTraffic(c.r[4]), c.r[5]);
}

fn new_state(c: &mut Cpu, r: &Saved, car: Car, kind: i32, ra: u32) -> u32 {
    let s = crate::aistate::pool_new(&mut c.m);
    match kind {
        STATE_NORMAL => r.enter(c, 0x8007_03bc, ra, |c| {
            crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car);
        }),
        _ => r.enter(c, 0x8007_1d6c, ra, |c| {
            crate::aistate::purgatory_construct(c, AIStateBase(s), car);
        }),
    }
    s
}

fn new_idle(m: &mut Mem, car: Car) -> u32 {
    let s = crate::aistate::pool_new(m);
    crate::aistate::base_construct(m, AIStateBase(s), car);
    m.w32(s.wrapping_add(4), VT_STATE_IDLE);
    m.w32(s.wrapping_add(12), 1);
    s
}

/// The state's virtual `TestForRelease`, called returning to `ra`.
fn test_for_release(c: &mut Cpu, r: &Saved, this: AIHighTraffic, ra: u32) -> bool {
    let state = c.m.r32(this.0 + 4);
    let (o, f) = crate::aistate::virtual_fn(&mut c.m, state, 3);
    r.call_ptr(c, f, &[o], ra) != 0
}

/// `AIHigh_Traffic::HighExecute` (AIH_TRAF.CPP, 0x8006666c, virtual): each AI tick (lapless), by
/// its state: none, purgatory (idle when out of the race) and put on the road at the start;
/// purgatory: at an accident scene, parked there (out of the race); else once released (and
/// not held), on a roving traffic trigger roving, or reborn driving; driving: purgatory when
/// out of life or sent; near a stopped police car (or 4 in 5 times by a passing one), pulled
/// over to the roadside (idle), else ignoring the police from then on; idle: purgatory out of
/// life, driving again once the police are gone (unless out of the race); roving: purgatory
/// out of life or sent, driving once released. Then its state.
pub fn high_execute(c: &mut Cpu, this: AIHighTraffic) {
    abi::frame(c, 216, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let m = &mut c.m;
        let car = car_of(m, this);
        car.set_unlap(m, 0);
        car.set_lap(m, 0);
        let purgatory = |c: &mut Cpu, r: &mut Saved, ra: u32| {
            let s = new_state(c, r, car, STATE_PURGATORY, ra);
            r.0[0] = s;
            aihigh::set_state(c, r, this.0, s, STATE_PURGATORY, ra + 0x2c);
        };
        let normal = |c: &mut Cpu, r: &mut Saved, ra: u32| {
            let s = new_state(c, r, car, STATE_NORMAL, ra);
            r.0[0] = s;
            aihigh::set_state(c, r, this.0, s, STATE_NORMAL, ra + 0x2c);
        };
        match m.r32(this.0 + 8) {
            0 => {
                let m = &mut c.m;
                m.w32(sp + 24, 0);
                m.w32(sp + 28, 0x64_0000);
                m.w32(sp + 32, 0);
                if car.car_flags(m) & CAR_OUT != 0 {
                    let s = new_idle(m, car);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_6764);
                } else {
                    let s = new_state(c, &r, car, STATE_PURGATORY, 0x8006_6780);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_PURGATORY, 0x8006_67ac);
                }
                let obj = crate::types::NewtonObj(car.0);
                r.enter(c, 0x800a_22a8, 0x8006_67cc, |c| crate::newton::set_initial_slice_position_orientation_etc(c, obj, 0, Coord(sp + 24), 1));
                return;
            }
            1 => {
                r.0[0] = 0;
                let accident = this.accident_data_(&mut c.m);
                if accident != 0 {
                    let s = new_idle(&mut c.m, car);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_683c);
                    let m = &mut c.m;
                    m.w16(sp + 40, 0);
                    let at = crate::types::BWorldSmPos(sp + 40);
                    r.enter(c, 0x8007_f438, 0x8006_685c, |c| crate::bworldsm::find_closest_slice(c, Coord(accident + 16), at));
                    let m = &mut c.m;
                    let slice = m.r16(sp + 40) as i16 as i32;
                    m.w32(sp + 16, accident + 32);
                    r.enter(c, 0x8006_8794, 0x8006_687c, |c| crate::ailife::reencarnate_traffic_by_position(c, car, slice, 1, Coord(accident + 16), Matrix(accident + 32)));
                    let m = &mut c.m;
                    car.update_car_flags(m, |f| f | CAR_OUT);
                    this.set_accident_data_(m, 0);
                } else {
                    let released = test_for_release(c, &r, this, 0x8006_68c0) && this.force_purgatory_(&mut c.m) == 0;
                    r.0[0] = released as u32;
                    if released {
                        let trigger = r.enter(c, 0x8006_6f38, 0x8006_68e4, |c| check_for_new_triggers(c, this));
                        r.0[0] = trigger;
                        if trigger != 0 {
                            if c.m.r32(trigger) == 5 {
                                let s = crate::aistate::pool_new(&mut c.m);
                                let t = crate::types::Trigger(trigger);
                                r.enter(c, 0x8007_212c, 0x8006_6928, |c| {
                                    crate::aistate::roving_traffic_construct(&mut c.m, crate::types::AIStateRovingTraffic(s), car, t);
                                });
                                r.0[2] = s;
                                aihigh::set_state(c, &r, this.0, s, STATE_ROVING, 0x8006_6954);
                                let m = &mut c.m;
                                m.w32(sp + 16, trigger + 12);
                                let (slice, pos) = (m.r32(trigger + 4) as i32, m.r32(trigger + 0x3C));
                                r.enter(c, 0x8006_8794, 0x8006_697c, |c| crate::ailife::reencarnate_traffic_by_position(c, car, slice, 1, Coord(pos), Matrix(trigger + 12)));
                            }
                        } else {
                            normal(c, &mut r, 0x8006_6998);
                            r.enter(c, 0x8006_86e4, 0x8006_69d8, |c| crate::ailife::reencarnate_traffic(c, car));
                        }
                    }
                }
            }
            2 => {
                if r.enter(c, 0x8006_7e50, 0x8006_69ec, |c| crate::ailife::evaluate_life(c, car)) {
                    purgatory(c, &mut r, 0x8006_6a08);
                } else if this.force_purgatory_(&mut c.m) != 0 {
                    purgatory(c, &mut r, 0x8006_6a60);
                } else {
                    let cop = r.enter(c, 0x8006_6560, 0x8006_6a9c, |c| cop_check(c, this, sp + 176));
                    let m = &mut c.m;
                    if cop == 0 {
                        this.set_ignore_cops_(m, 0);
                    } else if this.ignore_cops_(m) == 0 {
                        let t = fast_random_step(m);
                        let roll = (t >> 8 & 0xFFFF).wrapping_mul(5) >> 15;
                        let stopped = m.r32(sp + 176) != 0;
                        if stopped || (1..8).contains(&roll) {
                            let slice = car.n().sim_road_info().slice(m) as i16 as i32;
                            r.0[2] = slice as u32;
                            let s = new_idle(m, car);
                            r.0[3] = 1;
                            r.0[0] = s;
                            aihigh::set_state(c, &r, this.0, s, STATE_IDLE, if stopped { 0x8006_6b68 } else { 0x8006_6c80 });
                            let m = &mut c.m;
                            let b = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
                            let b29 = m.r8(b + 29) as i32;
                            let width = if car.direction(m) == 1 {
                                ((m.r8(b + 31) as i32) << 15).wrapping_mul(b29 & 15)
                            } else {
                                ((m.r8(b + 30) as i32) << 15).wrapping_mul(b29 >> 4)
                            };
                            m.w32(sp + 184, width as u32);
                            crate::aistate::set_idle_position(m, AIStateIdle(s), width);
                        } else if roll == 0 {
                            let s = new_idle(m, car);
                            r.0[0] = s;
                            aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_6c0c);
                        } else {
                            this.set_ignore_cops_(m, 1);
                        }
                    }
                }
            }
            3 => {
                if r.enter(c, 0x8006_7e50, 0x8006_6d20, |c| crate::ailife::evaluate_life(c, car)) {
                    purgatory(c, &mut r, 0x8006_6d3c);
                } else if r.enter(c, 0x8006_6560, 0x8006_6d78, |c| cop_check(c, this, sp + 180)) == 0 && car.car_flags(&mut c.m) & CAR_OUT == 0 {
                    normal(c, &mut r, 0x8006_6e98);
                }
            }
            6 => {
                if r.enter(c, 0x8006_7e50, 0x8006_6db0, |c| crate::ailife::evaluate_life(c, car)) {
                    purgatory(c, &mut r, 0x8006_6dcc);
                } else if this.force_purgatory_(&mut c.m) != 0 {
                    purgatory(c, &mut r, 0x8006_6e24);
                } else if test_for_release(c, &r, this, 0x8006_6e7c) {
                    normal(c, &mut r, 0x8006_6e98);
                }
            }
            _ => {}
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8006_6edc, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_high_execute(c: &mut Cpu) {
    high_execute(c, AIHighTraffic(c.r[4]));
}

/// `AIHigh_Traffic::AIHigh_Traffic(Car_tObj*)` (AIH_TRAF.CPP, 0x80066ef8): heeding the police,
/// not held, no accident.
pub fn construct(c: &mut Cpu, this: AIHighTraffic, car: Car) -> AIHighTraffic {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8005_bb08, 0x8006_6f0c, |c| aihigh::base_construct(c, this.ai_high_base(), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_TRAFFIC);
        this.set_ignore_cops_(m, 0);
        this.set_force_purgatory_(m, 0);
        this.set_accident_data_(m, 0);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighTraffic(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_Traffic::CheckForNewTriggers` (AIH_TRAF.CPP, 0x80066f38): from the last car up, each
/// human (or human cop): the roving traffic triggers on its way since it was last checked (up
/// to 45 slices ahead, under 50 at a time), the first out of the humans' sight (by a chance
/// that always passes); the trigger, or none (0).
pub fn check_for_new_triggers(c: &mut Cpu, this: AIHighTraffic) -> u32 {
    let _ = this;
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let manager = AITriggerTriggerManager(m.r32(TRIGGER_MANAGER_TRAFFIC));
        let mut i = NUM_CARS.get(m) - 1;
        m.w32(sp + 20, TOTAL_SORTED_LIST.wrapping_add((i as u32).wrapping_mul(4)));
        r.0[7] = i as u32;
        while i >= 0 {
            let m = &mut c.m;
            let at = m.r32(sp + 20);
            let car = Car(m.r32(at));
            r.0[5] = car.0;
            if car.car_flags(m) & 0x204 != 0 {
                let index = car.car_index(m);
                let o = obj(m, index);
                let dir: i32 = if car.current_speed(m) < 0 { -1 } else { 1 };
                let slice = car.n().sim_road_info().slice(m) as i16 as i32;
                let by = dir.wrapping_mul(45);
                let s = slice.wrapping_add(by);
                let n = NUM_SLICES.get(m);
                let ahead = if by >= 0 {
                    if s >= n { s - n } else { s }
                } else if s < 0 {
                    s + n
                } else {
                    s
                };
                let last = m.r32(o + 0x10) as i32;
                m.w32(o + 0x10, ahead as u32);
                let (mut from, to) = if last < ahead { (last, ahead) } else { (ahead, last) };
                let few = to - from < 50;
                let odds = 0x32_0000i32.wrapping_mul(25);
                (r.0[4], r.0[6], r.0[8]) = (to as u32, odds as u32, few as u32);
                while from < to && few {
                    r.0[1] = from as u32;
                    let found = r.enter(c, 0x8007_3404, 0x8006_70a4, |c| crate::aitrigger::check_for_trigger_at_slice(c, manager, index, from));
                    r.0[3] = found as u32;
                    if found != -1 {
                        let m = &mut c.m;
                        let o4 = odds << 2;
                        let limit = (if o4 < 0 { o4 | 0xFFFF } else { o4 }) >> 16;
                        let t = fast_random_step(m);
                        let roll = ((t >> 8 & 0xFFFF).wrapping_mul(25) >> 14) as i32;
                        (r.0[0], r.0[2]) = (roll as u32, limit as u32);
                        let seen = r.enter(c, 0x8006_900c, 0x8006_7118, |c| crate::ailife::is_slice_in_any_visible_area(&mut c.m, from)) != 0;
                        if !seen && roll < limit {
                            return crate::aitrigger::get_trigger(&mut c.m, manager, found, sp + 16);
                        }
                    }
                    from += 1;
                }
            }
            i -= 1;
            r.0[7] = i as u32;
            let at = c.m.r32(sp + 20);
            c.m.w32(sp + 20, at.wrapping_sub(4));
        }
        0
    })
}

pub fn abi_check_for_new_triggers(c: &mut Cpu) {
    c.r[2] = check_for_new_triggers(c, AIHighTraffic(c.r[4]));
}

/// `AIHigh_Traffic::~AIHigh_Traffic` (AIHIGH.H, 0x8006718c).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8006_719c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{ARGS, HEAP_OBJS, OBJS, args, pursuit, pursuit_shuffle};
    use crate::test_util::{against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn traf_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn([u32; 7], [u32; 4]) -> Vec<u32>, Port); 7] = [
            ("CheckForCops", nfs4rc::f_8006648c, |_, p| vec![p[2], ARGS + 0x60], |c, a| Some(check_for_cops(c, AIHighTraffic(a[0]), a[1]).0)),
            ("CopCheck", nfs4rc::f_80066560, |_, p| vec![p[2], ARGS + 0x64], |c, a| Some(cop_check(c, AIHighTraffic(a[0]), a[1]))),
            ("HighExecute", nfs4rc::f_8006666c, |_, p| vec![p[2]], |c, a| {
                high_execute(c, AIHighTraffic(a[0]));
                None
            }),
            ("Construct", nfs4rc::f_80066ef8, |a, _| vec![OBJS + 0x700, a[0]], |c, a| Some(construct(c, AIHighTraffic(a[0]), Car(a[1])).0)),
            ("CheckForNewTriggers", nfs4rc::f_80066f38, |_, p| vec![p[2]], |c, a| Some(check_for_new_triggers(c, AIHighTraffic(a[0])))),
            ("Destruct", nfs4rc::f_8006718c, |a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
            ("HighExecuteAgain", nfs4rc::f_8006666c, |_, p| vec![p[2]], |c, a| {
                high_execute(c, AIHighTraffic(a[0]));
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 2_000, 0x4110 + k as u64, pursuit_shuffle, original, &|c| {
                let (a, p) = (args(c), pursuit(c));
                call(a, p)
            }, port);
        }
    }

    /// The stack a traffic car's tick leaves.
    #[test]
    fn traf_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 3] = [
            ("HighExecute", 0x8006_666c, nfs4rc::f_8006666c),
            ("CheckForNewTriggers", 0x8006_6f38, nfs4rc::f_80066f38),
            ("CopCheck", 0x8006_6560, nfs4rc::f_80066560),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 600, 0x4150 + k as u64, pursuit_shuffle, &|c, _| {
                let p = pursuit(c)[2];
                if name == "CopCheck" { vec![p, ARGS + 0x64] } else { vec![p] }
            });
        }
    }
}
