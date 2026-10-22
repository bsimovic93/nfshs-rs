//! AIH_BASICPERP.CPP: `AIHigh_BasicPerp`, a car the police may chase (a racer, a human): its
//! crimes (speeding, driving the wrong way, hitting a police car), whether a police car has
//! caught it, and the police cars chasing it, ordered by how close they are.

use crate::abi::{self, Saved};
use crate::aihigh::{self, CAR_COP, CAR_HUMAN_COP, VT_BASIC_PERP, is_btc, obj};
use crate::aiworld::{apx_spline_distance, apx_spline_distance_to_slice, check_for_barrier_between_lanes};
use crate::fixed::fixedmult;
use crate::globals::{AITUNE_DRIVE_SIDE, AITUNE_ONE_WAY, BWORLDSM_SLICES, CARS_LIST, COP_CARS, GAME_SETUP, NUM_CARS, NUM_COP_CARS, SIM_COUNTER};
use crate::types::{AIHighBase, AIHighBasicCop, AIHighBasicPerp, Car};
use ps1rt::{Cpu, Mem};

/// `AIHigh_BasicPerp_PlayerCaughtSpeed`, `..._PlayerCaughtDeltaY`, `..._MinDeltaSpeed`,
/// `..._CopCaughtSpeed`, `..._CaughtDistance`: by the skill, the speed under which a car can be
/// caught, the heights it can be caught within, how much slower than the police car it must
/// be, the police car's speed, the distance along the road.
const PLAYER_CAUGHT_SPEED: u32 = 0x8010_D9B8;
const PLAYER_CAUGHT_DELTA_Y: u32 = 0x8010_D9C4;
const MIN_DELTA_SPEED: u32 = 0x8010_D9D0;
const COP_CAUGHT_SPEED: u32 = 0x8010_D9DC;
const CAUGHT_DISTANCE: u32 = 0x8010_D9E8;

/// `Car_tObj.carFlags`: in the race's "be the cop" sense, a human.
const CAR_PLAYER: i32 = 0x2;
/// `Car_tObj.AIFlags`: a police car on duty; parked.
const AI_ON_DUTY: i32 = 0x2;
const AI_PARKED: i32 = 0x4;

/// The crimes (`crime_`): none, speeding, the wrong way, hitting a police car, hard.
const CRIME_SPEEDING: i32 = 1;
const CRIME_WRONG_WAY: i32 = 2;
const CRIME_HIT_COP: i32 = 3;
const CRIME_HIT_COP_HARD: i32 = 4;

/// The car's orientation, element `k`.
pub(crate) fn orient(m: &mut Mem, car: Car, k: u32) -> i32 {
    m.r32(car.0 + 0xF0 + 4 * k) as i32
}

/// Upright: its up vector's y over 0.6.
pub(crate) fn upright(m: &mut Mem, car: Car) -> bool {
    orient(m, car, 4) > 0x9999
}

fn skill_table(m: &mut Mem, table: u32, skill: i32) -> i32 {
    m.r32(table.wrapping_add((skill as u32).wrapping_mul(4))) as i32
}

/// `AIHigh_BasicPerp::CheckForCrimes` (AIH_BASICPERP.CPP, 0x8005bcf0, virtual): after 8 s of
/// the race and 10 s after the last pull-over, a car not finished has hit a police car in the
/// last 15 ticks (hard over 20, any crime but a hard hit is kept), goes over the speed limit,
/// or the wrong way (a one-way track backwards over 4 m/s; else on the other side's lanes);
/// the crime is taken only while a police car (or a human cop) is active.
pub fn check_for_crimes(m: &mut Mem, this: AIHighBasicPerp) {
    let car = this.ai_high_base().car_obj_(m);
    let had = this.basic_perp_info_().crime_(m);
    let mut crime = had;
    let slice = car.n().sim_road_info().slice(m) as i16 as i32;
    let legal = crate::aispeeds::get_legal_speed(m, slice);
    let now = SIM_COUNTER.get(m);
    if now.wrapping_sub(this.last_pull_over_time_(m)) < 640 || car.stats().finish_type(m) >= 2 {
        return;
    }
    let hit = car.n().collision();
    if now.wrapping_sub(hit.last_time(m)) < 15 && hit.last_other_obj(m).0 != 0 && Car(hit.last_other_obj(m).0).car_flags(m) & CAR_COP != 0 {
        if hit.last_impulse(m) > 0x14_0000 {
            crime = CRIME_HIT_COP_HARD;
        } else if crime == 0 {
            crime = CRIME_HIT_COP;
        }
    }
    if had != 0 && crime != CRIME_HIT_COP_HARD {
        return;
    }
    let speed = car.current_speed(m);
    if legal.wrapping_abs() < speed.wrapping_abs() && crime == 0 {
        crime = CRIME_SPEEDING;
    }
    let wrong_way = if AITUNE_ONE_WAY.get(m) != 0 {
        let backwards = if GAME_SETUP.reverse_track(m) != 0 { speed.wrapping_neg() < 0 } else { speed < 0 };
        backwards
    } else {
        let side = speed.wrapping_mul(AITUNE_DRIVE_SIDE.get(m));
        let lane = car.lane_index(m);
        if side >= 0 { lane < 7 } else { lane >= 7 }
    };
    if wrong_way && speed.wrapping_abs() > 0x4_0000 && crime == 0 {
        crime = CRIME_WRONG_WAY;
    }
    if SIM_COUNTER.get(m) < 512 {
        crime = 0;
    }
    if crime == 0 {
        return;
    }
    for i in 0..NUM_CARS.get(m) {
        let other = Car(m.r32(CARS_LIST + 4 * i as u32));
        if other.car_flags(m) & (CAR_COP | CAR_HUMAN_COP) != 0 && other.n().active(m) != 0 {
            this.basic_perp_info_().set_crime_(m, crime);
            return;
        }
    }
}

pub fn abi_check_for_crimes(c: &mut Cpu) {
    check_for_crimes(&mut c.m, AIHighBasicPerp(c.r[4]));
}

/// The two cars' lanes: either outside the road's paved lanes at its slice, or a barrier
/// between them at either's slice.
fn barrier_between(m: &mut Mem, car: Car, other: Car) -> bool {
    let slices = BWORLDSM_SLICES.get(m) as u32;
    let lanes = |m: &mut Mem, s: Car| {
        let slice = s.n().sim_road_info().slice(m) as i16 as i32 as u32;
        m.r8(slices.wrapping_add(slice << 5).wrapping_add(29))
    };
    let paved = |b: u8, lane: i32| lane >= 7 - (b >> 4) as i32 && lane <= (b & 15) as i32 + 6;
    let (b, lane) = (lanes(m, car), car.lane_index(m));
    if !paved(b, lane) {
        return false;
    }
    let (b, other_lane) = (lanes(m, other), other.lane_index(m));
    if !paved(b, other_lane) {
        return false;
    }
    let other_slice = other.n().sim_road_info().slice(m) as i16 as i32;
    if check_for_barrier_between_lanes(m, other_slice, other_lane, lane) == 0 {
        return false;
    }
    let slice = car.n().sim_road_info().slice(m) as i16 as i32;
    check_for_barrier_between_lanes(m, slice, other_lane, lane) != 0
}

/// `AIHigh_BasicPerp::CheckIfCaught` (AIH_BASICPERP.CPP, 0x8005bf7c): a car with a crime,
/// slow enough (its speed down the road, a human's along its heading), 15 s after the last
/// pull-over and not finished, is caught by an active police car (in "be the cop", a human
/// cop) on duty, on the ground, upright, about as slow, no slower than it by the skill's
/// margin, near in height and along the road, no barrier between their lanes; and when both
/// are upright and either is faster than 4.4 m/s, the police car must be ahead within 9 m and
/// beside it within 9 m. The first catching police car is kept (`lastArrestingCop_`).
pub fn check_if_caught(c: &mut Cpu, this: AIHighBasicPerp) -> bool {
    abi::frame(c, 80, |c, sp| {
        let m = &mut c.m;
        let car = this.ai_high_base().car_obj_(m);
        let skill = GAME_SETUP.skill(m);
        let speed = if car.car_flags(m) & CAR_PLAYER != 0 { car.current_speed(m).wrapping_abs() } else { car.n().speed_xz(m) };
        if this.basic_perp_info_().crime_(m) == 0 || skill_table(m, PLAYER_CAUGHT_SPEED, skill) < speed {
            return false;
        }
        if SIM_COUNTER.get(m).wrapping_sub(this.last_pull_over_time_(m)) < 960 || car.stats().finish_type(m) >= 2 {
            return false;
        }
        let up = upright(m, car);
        m.w32(sp + 32, up as u32);
        for i in 0..NUM_CARS.get(m) {
            let m = &mut c.m;
            let cop = Car(m.r32(CARS_LIST + 4 * i as u32));
            let slower = car.current_speed(m).wrapping_sub(cop.current_speed(m));
            let chaser = if is_btc(m) { cop.car_flags(m) & CAR_HUMAN_COP != 0 } else { cop.car_flags(m) & CAR_COP != 0 };
            if !chaser {
                continue;
            }
            let flags = cop.ai_flags(m);
            if flags & AI_ON_DUTY == 0 || cop.n().flight_time(m) != 0 || cop.n().active(m) == 0 || flags & AI_PARKED != 0 || !upright(m, cop) {
                continue;
            }
            if slower >= skill_table(m, MIN_DELTA_SPEED, skill) || cop.current_speed(m).wrapping_abs() >= skill_table(m, COP_CAUGHT_SPEED, skill) {
                continue;
            }
            let (y, cop_y) = (car.n().position().y(m), cop.n().position().y(m));
            let dy = cop_y.wrapping_sub(y);
            let dy = if dy > 0 { dy } else { y.wrapping_sub(cop_y) };
            if dy >= skill_table(m, PLAYER_CAUGHT_DELTA_Y, skill) {
                continue;
            }
            let along = apx_spline_distance(m, car, cop).wrapping_abs();
            let barrier = barrier_between(m, car, cop);
            if along >= skill_table(m, CAUGHT_DISTANCE, skill) || barrier {
                continue;
            }
            let (mut side, mut ahead) = (0, 0x1_0000);
            if is_btc(m) {
                let (p, q) = (car.n().position().get(m), cop.n().position().get(m));
                let d = [q.x.wrapping_sub(p.x), q.y.wrapping_sub(p.y), q.z.wrapping_sub(p.z)];
                for (k, v) in d.iter().enumerate() {
                    m.w32(sp + 16 + 4 * k as u32, *v as u32);
                }
                let o: [i32; 9] = std::array::from_fn(|k| orient(m, car, k as u32));
                side = fixedmult(d[0], o[0]).wrapping_add(fixedmult(d[1], o[1])).wrapping_add(fixedmult(d[2], o[2]));
                ahead = fixedmult(d[0], o[6]).wrapping_add(fixedmult(d[1], o[7])).wrapping_add(fixedmult(d[2], o[8]));
            }
            let m = &mut c.m;
            let near_needed = m.r32(sp + 32) != 0 && (cop.speed(m) > 0x4_71C6 || car.speed(m) > 0x4_71C6);
            if near_needed && (side.wrapping_abs() > 0x8_FFFF || ahead < 0 || ahead > 0x8_FFFF) {
                continue;
            }
            if this.last_arresting_cop_(m).0 == 0 {
                this.set_last_arresting_cop_(m, cop);
            }
            return true;
        }
        false
    })
}

pub fn abi_check_if_caught(c: &mut Cpu) {
    c.r[2] = check_if_caught(c, AIHighBasicPerp(c.r[4])) as u32;
}

/// `AIHigh_BasicPerp::RemoveCloseCops` (AIH_BASICPERP.CPP, 0x8005c56c): the police cars on the
/// road within 150 m along it turn back (the track's way), driving away (slowly within 30 m);
/// each says so on the radio.
pub fn remove_close_cops(c: &mut Cpu, this: AIHighBasicPerp) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = 0;
        r.0[3] = 1;
        r.0[4] = GAME_SETUP.0;
        r.0[5] = aihigh::HIGH_LEVEL_AI_OBJS;
        r.0[6] = this.0;
        for i in 0..NUM_COP_CARS.get(&mut c.m) {
            let m = &mut c.m;
            r.0[1] = COP_CARS + 4 * i as u32;
            r.0[2] = i as u32;
            let cop = Car(m.r32(COP_CARS + 4 * i as u32));
            r.0[0] = cop.0;
            if cop.ai_flags(m) & AI_PARKED != 0 {
                continue;
            }
            let car = this.ai_high_base().car_obj_(m);
            let d = apx_spline_distance(m, cop, car).wrapping_abs();
            if d > 0x95_FFFF {
                continue;
            }
            let index = cop.car_index(m);
            let o = AIHighBasicCop(obj(m, index));
            let dir = if GAME_SETUP.reverse_track(m) != 0 { 1 } else { -1 };
            cop.set_direction(m, dir);
            cop.set_desired_direction(m, dir);
            cop.set_drive_direction(m, 1);
            o.set_drive_away_(m, if d > 0x1D_FFFF { 2 } else { 1 });
            let car = o.ai_high_base().car_obj_(m);
            speech(c, &r, car, 16, 0x8005_c66c, 0x8005_c688, &[]);
        }
    })
}

pub fn abi_remove_close_cops(c: &mut Cpu) {
    remove_close_cops(c, AIHighBasicPerp(c.r[4]));
}

/// The speech's `Speech::Mobile(car)` (the car's radio, or the speech's stand-in when it is
/// off), then its virtual function `k` with `args` after the object (the registers it isn't
/// given as `Mobile` leaves them); the calls return to `ra_mobile` and `ra`.
pub(crate) fn speech(c: &mut Cpu, r: &Saved, car: Car, k: u32, ra_mobile: u32, ra: u32, args: &[u32]) -> u32 {
    let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[car.0], ra_mobile);
    speech_call(c, r, mobile, k, ra, args)
}

/// The speech object `obj`'s virtual function `k` (its vtable at +0x4c), with `args` after it.
pub(crate) fn speech_call(c: &mut Cpu, r: &Saved, obj: u32, k: u32, ra: u32, args: &[u32]) -> u32 {
    let m = &mut c.m;
    let vt = m.r32(obj.wrapping_add(0x4C));
    let adjust = m.r16(vt.wrapping_add(8 * k)) as i16 as i32;
    let f = m.r32(vt.wrapping_add(8 * k + 4));
    let mut a = vec![obj.wrapping_add(adjust as u32)];
    a.extend_from_slice(args);
    r.call_ptr(c, f, &a, ra)
}

/// `AIHigh_BasicPerp::RemoveChaser` (AIH_BASICPERP.CPP, 0x8005c6bc): the police car `cop` (by
/// its cop index) of `type` no longer chases this car.
pub fn remove_chaser(m: &mut Mem, this: AIHighBasicPerp, cop: i32, kind: i32) {
    let assigned = this.0.wrapping_add(0x70).wrapping_add((kind as u32).wrapping_mul(4));
    let n = m.r32(assigned);
    m.w32(assigned, n.wrapping_sub(1));
    let at = m.r32(this.0.wrapping_add(0x4C).wrapping_add((cop as u32).wrapping_mul(4)));
    let e = this.0.wrapping_add(at.wrapping_mul(8));
    m.w32(e.wrapping_add(0x1C), u32::MAX);
    m.w32(e.wrapping_add(0x20), u32::MAX);
}

pub fn abi_remove_chaser(c: &mut Cpu) {
    remove_chaser(&mut c.m, AIHighBasicPerp(c.r[4]), c.r[5] as i32, c.r[7] as i32);
}

/// `AIHigh_BasicPerp::AddChaser` (AIH_BASICPERP.CPP, 0x8005c6f8): the police car `cop` (its
/// cop index; car index `car`) of `type` chases this car, put last then moved up by distance.
pub fn add_chaser(c: &mut Cpu, this: AIHighBasicPerp, cop: i32, car: i32, kind: i32) {
    let m = &mut c.m;
    let assigned = this.0.wrapping_add(0x70).wrapping_add((kind as u32).wrapping_mul(4));
    let n = m.r32(assigned);
    m.w32(assigned, n.wrapping_add(1));
    m.w32(this.0 + 0x44, cop as u32);
    m.w32(this.0 + 0x48, car as u32);
    m.w32(this.0.wrapping_add(0x4C).wrapping_add((cop as u32).wrapping_mul(4)), 5);
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8005_c74c, 0x8005_c73c, |c| check_chaser_position(c, this, cop, car));
    });
}

pub fn abi_add_chaser(c: &mut Cpu) {
    add_chaser(c, AIHighBasicPerp(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `AIHigh_BasicPerp::CheckChaserPosition` (AIH_BASICPERP.CPP, 0x8005c74c): the police car
/// `cop` (car index `car`) moved up the chasers' order past each one not closer to this car
/// (an empty place, or one less than 12 m ahead along the car's way); its place.
pub fn check_chaser_position(c: &mut Cpu, this: AIHighBasicPerp, cop: i32, car: i32) -> i32 {
    let m = &mut c.m;
    let slice = AIHighBase(obj(m, car)).car_obj_(m).n().sim_road_info().slice(m) as i16 as i32;
    let at_of = this.0.wrapping_add(0x4C).wrapping_add((cop as u32).wrapping_mul(4));
    let mut at = m.r32(at_of) as i32;
    let entry = |p: i32| this.0.wrapping_add((p as u32).wrapping_mul(8));
    while at > 0 {
        let e = entry(at - 1);
        let (other_cop, other_car) = (m.r32(e + 0x1C) as i32, m.r32(e + 0x20) as i32);
        if other_cop != -1 {
            if other_car != -1 {
                let other = AIHighBase(obj(m, other_car)).car_obj_(m);
                let d = apx_spline_distance_to_slice(m, other, slice);
                if d.wrapping_mul(this.ai_high_base().car_obj_(m).direction(m)) >= -0xC_0000 {
                    return at;
                }
            }
            m.w32(this.0.wrapping_add(0x4C).wrapping_add((other_cop as u32).wrapping_mul(4)), at as u32);
        }
        m.w32(at_of, (at - 1) as u32);
        let (from, to) = (e, entry(at));
        let v = m.r32(from + 0x1C);
        m.w32(to + 0x1C, v);
        let v = m.r32(from + 0x20);
        m.w32(to + 0x20, v);
        at -= 1;
        m.w32(from + 0x1C, cop as u32);
        m.w32(from + 0x20, car as u32);
    }
    at
}

pub fn abi_check_chaser_position(c: &mut Cpu) {
    c.r[2] = check_chaser_position(c, AIHighBasicPerp(c.r[4]), c.r[5] as i32, c.r[6] as i32) as u32;
}

/// `AIHigh_BasicPerp::Clear` (AIH_BASICPERP.CPP, 0x8005c8a8): no chasers, not pulled over, the
/// last pull-over long ago, no arresting police car.
pub fn clear(m: &mut Mem, this: AIHighBasicPerp) {
    for p in 0..6 {
        let e = this.position_vs_cop_list_(p);
        e.set_cop_index(m, -1);
        e.set_car_index(m, -1);
    }
    this.set_pull_over_mode_(m, 0);
    this.set_beating_ticks_left_(m, 0);
    this.set_last_pull_over_time_(m, -640);
    this.set_last_arresting_cop_(m, Car(0));
}

pub fn abi_clear(c: &mut Cpu) {
    clear(&mut c.m, AIHighBasicPerp(c.r[4]));
}

/// `AIHigh_BasicPerp::AIHigh_BasicPerp(Car_tObj*)` (AIH_BASICPERP.CPP, 0x8005c8e4): no crime,
/// no chasers.
pub fn construct(c: &mut Cpu, this: AIHighBasicPerp, car: Car) -> AIHighBasicPerp {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8005_bb08, 0x8005_c8f8, |c| aihigh::base_construct(c, this.ai_high_base(), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_BASIC_PERP);
        this.basic_perp_info_().set_cops_assigned_(m, 0, 0);
        this.basic_perp_info_().set_cops_assigned_(m, 1, 0);
        this.basic_perp_info_().set_crime_(m, 0);
        clear(m, this);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighBasicPerp(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_BasicPerp::~AIHigh_BasicPerp` (AIHIGH.H, 0x8005c92c).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8005_c944);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{HEAP_OBJS, OBJS, args, shuffle};
    use crate::test_util::{SCRATCH_TOP, against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn basicperp_fuzz() {
        // (each call's arguments from the shuffle's: cars, a cop index, a car index, a cop type,
        // a small number, an object)
        let cases: [(&str, fn(&mut Cpu), fn([u32; 7]) -> Vec<u32>, Port); 10] = [
            ("CheckForCrimes", nfs4rc::f_8005bcf0, |a| vec![a[6]], |c, a| {
                check_for_crimes(&mut c.m, AIHighBasicPerp(a[0]));
                None
            }),
            ("CheckIfCaught", nfs4rc::f_8005bf7c, |a| vec![a[6]], |c, a| Some(check_if_caught(c, AIHighBasicPerp(a[0])) as u32)),
            ("RemoveCloseCops", nfs4rc::f_8005c56c, |a| vec![a[6]], |c, a| {
                remove_close_cops(c, AIHighBasicPerp(a[0]));
                None
            }),
            ("RemoveChaser", nfs4rc::f_8005c6bc, |a| vec![a[6], a[2], a[3], a[4]], |c, a| {
                remove_chaser(&mut c.m, AIHighBasicPerp(a[0]), a[1] as i32, a[3] as i32);
                None
            }),
            ("AddChaser", nfs4rc::f_8005c6f8, |a| vec![a[6], a[2], a[3], a[4]], |c, a| {
                add_chaser(c, AIHighBasicPerp(a[0]), a[1] as i32, a[2] as i32, a[3] as i32);
                None
            }),
            ("CheckChaserPosition", nfs4rc::f_8005c74c, |a| vec![a[6], a[2], a[3]], |c, a| Some(check_chaser_position(c, AIHighBasicPerp(a[0]), a[1] as i32, a[2] as i32) as u32)),
            ("Clear", nfs4rc::f_8005c8a8, |a| vec![a[6]], |c, a| {
                clear(&mut c.m, AIHighBasicPerp(a[0]));
                None
            }),
            ("Construct", nfs4rc::f_8005c8e4, |a| vec![OBJS + 0x700, a[0]], |c, a| Some(construct(c, AIHighBasicPerp(a[0]), Car(a[1])).0)),
            ("Destruct", nfs4rc::f_8005c92c, |a| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
            ("BaseConstruct", nfs4rc::f_8005bb08, |a| vec![OBJS + 0x700, a[0]], |c, a| Some(crate::aihigh::base_construct(c, AIHighBase(a[0]), Car(a[1])).0)),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3B10 + k as u64, shuffle, original, &|c| call(args(c)), port);
        }
    }

    /// The stack the chasers' order, the arrest check and the radio calls leave.
    #[test]
    fn basicperp_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 4] = [
            ("CheckIfCaught", 0x8005_bf7c, nfs4rc::f_8005bf7c),
            ("RemoveCloseCops", 0x8005_c56c, nfs4rc::f_8005c56c),
            ("AddChaser", 0x8005_c6f8, nfs4rc::f_8005c6f8),
            ("Construct", 0x8005_c8e4, nfs4rc::f_8005c8e4),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 500, 0x3B50 + k as u64, shuffle, &|c, _| {
                let a = args(c);
                if name == "Construct" { vec![OBJS + 0x700, a[0]] } else { vec![a[6], a[2], a[3], a[4]] }
            });
        }
    }
}
