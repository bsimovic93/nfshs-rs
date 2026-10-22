//! AILIFE.CPP: traffic and police cars' lives. A traffic car lives while it is near a life basis
//! car (a human's, around which the world is alive); out of every live area it is reborn ahead of
//! or behind one, out of the humans' sight, in a lane of its direction at its speed. The police
//! are put on the road by slice, by position or by lateral position and yaw. What a human can
//! see: within a distance (the larger axis plus a quarter of the smaller) or along the road.

use crate::abi::{self, Saved};
use crate::aiworld::{apx_spline_distance, apx_spline_distance_slices};
use crate::fixed::fixedmult;
use crate::globals::{
    AITUNE_DRIVE_SIDE, AITUNE_ONE_WAY, BWORLDSM_SLICES, COP_CARS, FAST_RANDOM, GAME_SETUP, HUMAN_RACE_CARS, LIFE_BASIS_CARS, NUM_CARS, NUM_COP_CARS,
    NUM_HUMAN_RACE_CARS, NUM_LIFE_BASIS_CARS, NUM_SLICES, RAND_SEED, RAND_TEMP, SORTED_LIST, STACK_SPEED_UP_ENABLED, W_SAVE_PTR,
};
use crate::types::{Car, Coord, Matrix, TrkNewSlice};
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`: a human's car.
const CAR_HUMAN: i32 = 0x4;
/// A traffic car.
const CAR_TRAFFIC: i32 = 0x10;
/// The car others' lives go by.
const CAR_LIFE_BASIS: i32 = 0x100;
/// Out of the race.
const CAR_OUT: i32 = 0x400;

/// A car lives within this of a life basis car (216 m).
const LIVE_DISTANCE: i32 = 0xD8_0000;
/// A human sees this far (172 m).
const VISIBLE_DISTANCE: i32 = 0xAC_0000;
/// Along the road a human sees a slice this near (just under 172 m), a police car this near
/// (just under 50 m).
const VISIBLE_ALONG: i32 = 0xAB_FFFF;
const COP_CLOSE_ALONG: i32 = 0x31_FFFF;
/// A car is reborn this many slices on from where its basis car looks (36).
const LOOK_AHEAD_SLICES: i32 = 36;
/// A basis car faster than this (30 m/s) keeps its direction for the reborn car.
const FAST: i32 = 0x1E_0000;

/// `AILife_Debug`'s format strings.
const DEBUG_GROUP: u32 = 0x8005_5AE0;
const DEBUG_DIST: u32 = 0x8013_D878;

/// One step of the game's quick random numbers: the product it keeps (`randtemp`).
fn fast_random_step(m: &mut Mem) -> u32 {
    let t = FAST_RANDOM.get(m).wrapping_mul(RAND_SEED.get(m));
    RAND_TEMP.set(m, t);
    FAST_RANDOM.set(m, t & 0xFFFF);
    t
}

/// Of 1000, the random number `t`'s (from its middle 16 bits).
fn per_mille(t: u32) -> u32 {
    ((t >> 8) & 0xFFFF).wrapping_mul(1000) >> 16
}

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

/// The track's direction on a one-way track (backward when reversed), else `dir`.
fn one_way_direction(m: &mut Mem, dir: i32) -> i32 {
    if AITUNE_ONE_WAY.get(m) == 0 {
        return dir;
    }
    if GAME_SETUP.reverse_track(m) != 0 { -1 } else { 1 }
}

/// The game's distance between two points on the ground: the larger axis plus a quarter of
/// the smaller; and the z distance (the first the original compares).
fn ground_distance(m: &mut Mem, pos: Coord, car: Car) -> (i32, i32) {
    let p = car.n().position();
    let dz = pos.z(m).wrapping_sub(p.z(m)).wrapping_abs();
    let dx = pos.x(m).wrapping_sub(p.x(m)).wrapping_abs();
    let d = if dz < dx { dx.wrapping_add(dz >> 2) } else { dz.wrapping_add(dx >> 2) };
    (d, dz)
}

/// `AILife_Debug` (AILIFE.CPP, 0x800691d0): nothing (a printf in the development build), but
/// its arguments homed in the caller's frame at `sp`.
fn debug(m: &mut Mem, sp: u32, args: [u32; 4]) {
    for (k, a) in args.into_iter().enumerate() {
        m.w32(sp + 4 * k as u32, a);
    }
}

pub fn abi_debug(c: &mut Cpu) {
    let sp = c.r[29];
    debug(&mut c.m, sp, [c.r[4], c.r[5], c.r[6], c.r[7]]);
}

/// The car's position, frame, speed and road position set up after it is put down: the
/// frame turned by `yaw`'s matrix at `turn` (in the caller's frame), the shadow's frame the
/// same, its velocity `vel`, its AI cleared, its road position, span and lanes found.
fn settle(c: &mut Cpu, r: &mut Saved, car: Car, ra: [u32; 3]) {
    let orient = car.n().orient_mat();
    copy_matrix(&mut c.m, orient, car.n().shadow_mat());
    r.0[0] = orient.0.wrapping_add(0x20);
    r.call(c, nfs4rc::f_80067974, &[car.0], ra[0]); // AIInit_ClearAICar
    find_road_position(c, r, car, [ra[1], ra[2]]);
}

/// The car's road position (its wanted lateral positions with it), span and lanes.
fn find_road_position(c: &mut Cpu, r: &Saved, car: Car, ra: [u32; 2]) {
    let at = r.enter(c, 0x8008_b954, ra[0], |c| crate::cars::calculate_road_position(c, car));
    let m = &mut c.m;
    car.set_ramp_desired_lat_pos(m, at);
    car.set_desired_lat_pos(m, at);
    car.set_road_position(m, at);
    let span = crate::cars::calculate_road_span(m, car);
    car.set_road_span(m, span);
    r.enter(c, 0x8007_3e74, ra[1], |c| crate::aiworld::calculate_lane_info(c, car));
}

/// A frame's 9 words copied.
fn copy_matrix(m: &mut Mem, from: Matrix, to: Matrix) {
    for k in 0..9 {
        let v = m.r32(from.0 + 4 * k);
        m.w32(to.0 + 4 * k, v);
    }
}

/// `AILife_EvaluateLife` (AILIFE.CPP, 0x80067e50): whether the traffic car must be reborn: out
/// of its basis car's live area and every other's (it takes the first it is in as its basis).
pub fn evaluate_life(c: &mut Cpu, car: Car) -> bool {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        if car.car_flags(&mut c.m) & CAR_OUT != 0 {
            return false;
        }
        let basis = car.basis_car(&mut c.m);
        if r.enter(c, 0x8006_8e58, 0x8006_7e80, |c| is_coord_in_this_live_area(c, car.n().position(), basis)) {
            return false;
        }
        let basis = r.enter(c, 0x8006_8f04, 0x8006_7e90, |c| is_traffic_car_in_any_live_area(c, car));
        car.set_basis_car(&mut c.m, Car(basis));
        basis == 0
    })
}

pub fn abi_evaluate_life(c: &mut Cpu) {
    c.r[2] = evaluate_life(c, Car(c.r[4])) as u32;
}

/// `AILife_RCSetSpeeds` (AILIFE.CPP, 0x80067eb4): the car at the speed it wants.
pub fn rc_set_speeds(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8006_f5dc, 0x8006_7ec8, |c| crate::aispeeds::calc_desired_speed(c, car));
        let v = car.desired_speed(&mut c.m);
        car.set_current_speed(&mut c.m, v);
    })
}

pub fn abi_rc_set_speeds(c: &mut Cpu) {
    rc_set_speeds(c, Car(c.r[4]));
}

/// `AILife_RCPickSliceAndDirection` (AILIFE.CPP, 0x80067ee4): where the car is reborn. A random
/// basis car and side (ahead or behind; the basis car's direction when it is fast); then on
/// along the race order that way, while the next car (a basis car, nor this one) sees the
/// slice 36 on from the current basis car and is further that way, it is the basis (the slice's
/// centre kept at sp+16); a random direction (the track's on a one-way track), and the slice
/// 28 to 35 on from the basis car that side.
pub fn rc_pick_slice_and_direction(c: &mut Cpu, car: Car) {
    abi::frame(c, 72, |c, sp| {
        let m = &mut c.m;
        let t = fast_random_step(m);
        let pick = NUM_LIFE_BASIS_CARS.get(m).wrapping_mul(((t >> 8) & 0xFFFF) as i32) as u32 >> 16;
        let t = fast_random_step(m);
        let basis = Car(m.r32(LIFE_BASIS_CARS.wrapping_add(pick.wrapping_mul(4))));
        car.set_basis_car(m, basis);
        let mut side = if per_mille(t) < 500 { -1 } else { 1 };
        let basis = car.basis_car(m);
        if basis.current_speed(m).wrapping_abs() > FAST {
            side = basis.direction(m);
        }
        let mut order = basis.sort_index(m);
        let ahead = side.wrapping_mul(LOOK_AHEAD_SLICES);
        let mut i = 0;
        while i < NUM_CARS.get(m) {
            let n = NUM_CARS.get(m);
            let other = Car(m.r32(SORTED_LIST.wrapping_add((order.wrapping_add(n).wrapping_rem(n) as u32).wrapping_mul(4))));
            let basis = car.basis_car(m);
            if other != car && other != basis && other.car_flags(m) & CAR_LIFE_BASIS != 0 {
                let s = (basis.n().sim_road_info().slice(m) as i32).wrapping_add(ahead);
                let s = if ahead >= 0 {
                    if s < NUM_SLICES.get(m) { s } else { s.wrapping_sub(NUM_SLICES.get(m)) }
                } else if s >= 0 {
                    s
                } else {
                    s.wrapping_add(NUM_SLICES.get(m))
                };
                let center = slice_at(m, s).0;
                for k in 0..3 {
                    let v = m.r32(center + 4 * k);
                    m.w32(sp + 16 + 4 * k, v);
                }
                let further = is_coord_in_this_visible_area(m, Coord(sp + 16), other) && {
                    let basis = car.basis_car(m);
                    let d = apx_spline_distance(m, other, basis);
                    side.wrapping_mul(d / 0x1_0000) > 0
                };
                if !further {
                    break;
                }
                car.set_basis_car(m, other);
            }
            order = order.wrapping_add(side);
            i += 1;
        }
        let t = fast_random_step(m);
        car.set_direction(m, if per_mille(t) < 500 { -1 } else { 1 });
        if AITUNE_ONE_WAY.get(m) != 0 {
            let dir = one_way_direction(m, 0);
            car.set_direction(m, dir);
        }
        let t = fast_random_step(m);
        let on = (((t >> 21) & 7) as i32 + 28).wrapping_mul(side);
        let dir = car.direction(m);
        car.set_desired_direction(m, dir);
        let from = car.basis_car(m).n().sim_road_info().slice(m) as i32;
        let s = from.wrapping_add(on);
        let slices = NUM_SLICES.get(m) & 0xFFFF;
        let s = if on >= 0 {
            if s < NUM_SLICES.get(m) { s } else { s.wrapping_sub(slices) }
        } else if s >= 0 {
            s
        } else {
            s.wrapping_add(slices)
        };
        car.n().sim_road_info().set_slice(m, s as i16);
        let basis = car.basis_car(m);
        let args = [DEBUG_GROUP, basis.car_index(m) as u32, basis.n().sim_road_info().slice(m) as i32 as u32, car.n().sim_road_info().slice(m) as i32 as u32];
        debug(m, sp, args);
    })
}

pub fn abi_rc_pick_slice_and_direction(c: &mut Cpu) {
    rc_pick_slice_and_direction(c, Car(c.r[4]));
}

/// `AILife_RCPickDesiredLatPosition` (AILIFE.CPP, 0x8006831c): a random lane of the car's
/// side of the road (the drive side's lanes right of the centre, the others left), its centre
/// plus the car's slack.
pub fn rc_pick_desired_lat_position(m: &mut Mem, car: Car) {
    let at = car.n().sim_road_info().slice(m) as i32;
    let slice = slice_at(m, at);
    let at = if car.direction(m) == AITUNE_DRIVE_SIDE.get(m) {
        let t = fast_random_step(m);
        let width = (slice.avg_paved_width_rt(m) as i32) << 15;
        let lanes = (slice.lane_count(m) & 0xF) as u32;
        let lane = (lanes.wrapping_mul((t >> 8) & 0xFFFF) >> 16) as i32 + 1;
        width.wrapping_mul(lane).wrapping_sub(((width as u32) >> 1) as i32)
    } else {
        let t = fast_random_step(m);
        let width = (slice.avg_paved_width_lf(m) as i32) << 15;
        let lanes = (slice.lane_count(m) >> 4) as u32;
        let lane = (lanes.wrapping_mul((t >> 8) & 0xFFFF) >> 16) as i32 + 1;
        width.wrapping_neg().wrapping_mul(lane).wrapping_add(((width as u32) >> 1) as i32)
    };
    car.set_desired_lat_pos(m, at);
    let at = car.desired_lat_pos(m).wrapping_add(car.lane_slack(m));
    car.set_desired_lat_pos(m, at);
    car.set_ramp_desired_lat_pos(m, at);
}

pub fn abi_rc_pick_desired_lat_position(c: &mut Cpu) {
    rc_pick_desired_lat_position(&mut c.m, Car(c.r[4]));
}

/// `AILife_PlaceCarAtLocation(Car_tObj*, int, int, int, int, int)` (AILIFE.CPP, 0x80068450):
/// the car put at `slice`, `lat` across, going `dir` at `speed`, turned by `yaw`.
pub fn place_car_at_location_at(c: &mut Cpu, car: Car, slice: i32, lat: i32, dir: i32, speed: i32, yaw: i32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        car.set_desired_lat_pos(m, lat);
        car.set_direction(m, dir);
        car.set_desired_direction(m, dir);
        car.n().sim_road_info().set_slice(m, slice as i16);
        car.set_current_speed(m, speed);
        Saved::of(c).enter(c, 0x8006_84e0, 0x8006_847c, |c| place_car_at_location(c, car, yaw));
    })
}

pub fn abi_place_car_at_location_at(c: &mut Cpu) {
    let (speed, yaw) = (abi::arg(c, 4) as i32, abi::arg(c, 5) as i32);
    place_car_at_location_at(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, speed, yaw);
}

/// `AILife_SetInitialSlicePositionOrientationEtc` (AILIFE.CPP, 0x8006848c): the car put on the
/// road at its slice and wanted lateral position, facing its direction (the offset at sp+16).
pub fn set_initial_slice_position_orientation_etc(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_84ac); // memset
        let m = &mut c.m;
        m.w32(sp + 20, 0x1_0000);
        let lat = car.desired_lat_pos(m);
        m.w32(sp + 16, lat as u32);
        let (slice, dir) = (car.n().sim_road_info().slice(m) as i32, car.direction(m));
        r.enter(c, 0x800a_22a8, 0x8006_84d0, |c| crate::newton::set_initial_slice_position_orientation_etc(c, car.n(), slice, Coord(sp + 16), dir));
    })
}

pub fn abi_set_initial_slice_position_orientation_etc(c: &mut Cpu) {
    set_initial_slice_position_orientation_etc(c, Car(c.r[4]));
}

/// `AILife_PlaceCarAtLocation(Car_tObj*, int)` (AILIFE.CPP, 0x800684e0): the car active on the
/// road at its slice, lateral position and direction (put down on the saved stack when the
/// race runs on the scratchpad one), its AI physics reset, moving along its frame at its speed
/// (the velocity at sp+16), a human's physics reset, turned by `yaw` (its matrix at sp+32),
/// settled.
pub fn place_car_at_location(c: &mut Cpu, car: Car, yaw: i32) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        r.0[3] = yaw as u32;
        let m = &mut c.m;
        car.n().set_active(m, 1);
        let lat = car.desired_lat_pos(m);
        car.set_ramp_desired_lat_pos(m, lat);
        let speed = car.current_speed(m);
        car.set_desired_speed(m, speed);
        crate::aiphysic::reset_car(m, car);
        if STACK_SPEED_UP_ENABLED.get(m) != 0 {
            // (SetSp: off the scratchpad stack and back)
            let swap_sp = |c: &mut Cpu| {
                let sp = c.r[29];
                c.r[29] = W_SAVE_PTR.get(&mut c.m);
                W_SAVE_PTR.set(&mut c.m, sp);
            };
            swap_sp(c);
            STACK_SPEED_UP_ENABLED.set(&mut c.m, 0);
            r.enter(c, 0x8006_848c, 0x8006_8558, |c| set_initial_slice_position_orientation_etc(c, car));
            swap_sp(c);
            STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
        } else {
            r.enter(c, 0x8006_848c, 0x8006_858c, |c| set_initial_slice_position_orientation_etc(c, car));
        }
        let vel = Coord(sp + 16);
        if car.current_speed(&mut c.m) != 0 {
            let m = &mut c.m;
            let forward = car.n().orient_mat().0.wrapping_add(0x18);
            for k in 0..3 {
                let v = m.r32(forward + 4 * k);
                m.w32(sp + 16 + 4 * k, v);
            }
            let speed = car.current_speed(m).wrapping_abs();
            r.0[0] = speed as u32;
            for k in 0..3 {
                let v = fixedmult(speed, m.r32(sp + 16 + 4 * k) as i32);
                m.w32(sp + 16 + 4 * k, v as u32);
            }
        } else {
            r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_8604); // memset
        }
        let v = vel.get(&mut c.m);
        car.n().linear_vel().set(&mut c.m, v);
        r.0[1] = sp + 32;
        if car.car_flags(&mut c.m) & CAR_HUMAN != 0 {
            r.enter(c, 0x800a_b2d0, 0x8006_8638, |c| crate::physics::reset_car(c, car));
        }
        turn(c, &mut r, car, sp + 32, yaw, [0x8006_8648, 0x8006_865c]);
        settle(c, &mut r, car, [0x8006_86a0, 0x8006_86a8, 0x8006_86c8]);
    })
}

/// The car's frame turned by `yaw` (its matrix made at `turn` by the library's `xformy`).
fn turn(c: &mut Cpu, r: &mut Saved, car: Car, turn: u32, yaw: i32, ra: [u32; 2]) {
    r.call(c, nfs4rc::f_800e66b0, &[turn, yaw as u32], ra[0]); // xformy
    let orient = car.n().orient_mat();
    r.0[0] = orient.0;
    r.enter(c, 0x8009_e850, ra[1], |c| crate::mathnfs::fasttransmult(c, orient, Matrix(turn), orient));
}

pub fn abi_place_car_at_location(c: &mut Cpu) {
    place_car_at_location(c, Car(c.r[4]), c.r[5] as i32);
}

/// A traffic car's new colour (random), lane slack and speed factor.
fn renew_traffic(c: &mut Cpu, r: &Saved, car: Car, ra: [u32; 2]) {
    let m = &mut c.m;
    let t = fast_random_step(m);
    let colour = ((t >> 8) & 0xFFFF).wrapping_mul(3) >> 16;
    if car.car_flags(m) & CAR_TRAFFIC != 0 {
        r.call(c, nfs4rc::f_800ae8c4, &[car.0, colour], ra[0]); // R3DCar_ChangeTrafficColor
    }
    crate::ai::choose_new_lane_slack(&mut c.m, car);
    r.enter(c, 0x8006_f728, ra[1], |c| crate::aispeeds::set_traffic_speed_random_factor(c, car));
}

/// `AILife_ReencarnateTraffic` (AILIFE.CPP, 0x800686e4): the traffic car reborn: a new colour,
/// slack and speed factor; a slice, direction, lane and speed picked; put there.
pub fn reencarnate_traffic(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        renew_traffic(c, &r, car, [0x8006_8750, 0x8006_8760]);
        r.enter(c, 0x8006_7ee4, 0x8006_8768, |c| rc_pick_slice_and_direction(c, car));
        rc_pick_desired_lat_position(&mut c.m, car);
        r.enter(c, 0x8006_7eb4, 0x8006_8778, |c| rc_set_speeds(c, car));
        r.enter(c, 0x8006_84e0, 0x8006_8784, |c| place_car_at_location(c, car, 0));
    })
}

pub fn abi_reencarnate_traffic(c: &mut Cpu) {
    reencarnate_traffic(c, Car(c.r[4]));
}

/// The car's position and frame (the shadow's too) `pos` and `mat`; its velocity `vel`.
fn put(m: &mut Mem, car: Car, pos: Coord, mat: Matrix, vel: Coord) {
    let p = pos.get(m);
    car.n().position().set(m, p);
    copy_matrix(m, mat, car.n().orient_mat());
    copy_matrix(m, mat, car.n().shadow_mat());
    let v = vel.get(m);
    car.n().linear_vel().set(m, v);
}

/// `AILife_ReencarnateTrafficByPosition` (AILIFE.CPP, 0x80068794): the traffic car reborn at
/// `pos` with frame `mat`, on `slice` going `dir`, no basis car, still (on the road's centre
/// first: the offset at sp+32, the velocity at sp+16); a lane picked.
pub fn reencarnate_traffic_by_position(c: &mut Cpu, car: Car, slice: i32, dir: i32, pos: Coord, mat: Matrix) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0 = [car.0, r.0[1], mat.0, dir as u32, slice as u32, pos.0, r.0[6], r.0[7], r.0[8]];
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_87d8); // memset
        r.0[1] = sp + 32;
        r.call(c, nfs4rc::f_800e4f04, &[sp + 32, 0, 12], 0x8006_87ec);
        renew_traffic(c, &r, car, [0x8006_8848, 0x8006_8858]);
        let m = &mut c.m;
        car.set_basis_car(m, Car(0));
        car.set_direction(m, dir);
        car.set_desired_direction(m, dir);
        car.n().sim_road_info().set_slice(m, slice as i16);
        r.enter(c, 0x8006_7eb4, 0x8006_8870, |c| rc_set_speeds(c, car));
        let m = &mut c.m;
        let (slice, dir) = (car.n().sim_road_info().slice(m) as i32, car.direction(m));
        car.set_current_speed(m, 0);
        r.enter(c, 0x800a_22a8, 0x8006_8888, |c| crate::newton::set_initial_slice_position_orientation_etc(c, car.n(), slice, Coord(sp + 32), dir));
        let m = &mut c.m;
        put(m, car, pos, mat, Coord(sp + 16));
        car.n().set_speed_xz(m, 0);
        r.call(c, nfs4rc::f_80067974, &[car.0], 0x8006_894c); // AIInit_ClearAICar
        find_road_position(c, &r, car, [0x8006_8954, 0x8006_8974]);
        rc_pick_desired_lat_position(&mut c.m, car);
    })
}

pub fn abi_reencarnate_traffic_by_position(c: &mut Cpu) {
    let mat = Matrix(abi::arg(c, 4));
    reencarnate_traffic_by_position(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, Coord(c.r[7]), mat);
}

/// `AILife_ReencarnateCopBySlice` (AILIFE.CPP, 0x800689a0): the police car put at `slice` going
/// `dir` (the track's on a one-way track): moving, in the middle of the lanes of its side
/// (the drive side's when going forward); still, on the shoulder of side `side` (-1 the left).
pub fn reencarnate_cop_by_slice(c: &mut Cpu, car: Car, slice: i32, dir: i32, side: i32, moving: bool) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        let m = &mut c.m;
        car.n().sim_road_info().set_slice(m, slice as i16);
        let dir = one_way_direction(m, dir);
        car.set_direction(m, dir);
        car.set_desired_direction(m, dir);
        let s = slice_at(m, slice);
        let (lf, rt, lanes) = ((s.avg_paved_width_lf(m) as i32) << 15, (s.avg_paved_width_rt(m) as i32) << 15, s.lane_count(m) as i32);
        let at = if !moving {
            if side == -1 {
                (-0x2_0000i32).wrapping_sub(lf.wrapping_mul(lanes >> 4))
            } else {
                rt.wrapping_mul(lanes & 0xF).wrapping_add(0x2_0000)
            }
        } else if car.direction(m) == 1 || lanes >> 4 == 0 {
            rt.wrapping_mul(lanes & 0xF).wrapping_sub(((rt as u32) >> 1) as i32)
        } else {
            lf.wrapping_neg().wrapping_mul(lanes >> 4).wrapping_add(((lf as u32) >> 1) as i32)
        };
        car.set_desired_lat_pos(m, at);
        if moving {
            r.enter(c, 0x8006_7eb4, 0x8006_8aec, |c| rc_set_speeds(c, car));
        } else {
            car.set_desired_speed(&mut c.m, 0);
            car.set_current_speed(&mut c.m, 0);
        }
        let m = &mut c.m;
        let at = car.desired_lat_pos(m);
        car.set_ramp_desired_lat_pos(m, at);
        car.set_road_position(m, at);
        r.enter(c, 0x8006_84e0, 0x8006_8b14, |c| place_car_at_location(c, car, 0));
    })
}

pub fn abi_reencarnate_cop_by_slice(c: &mut Cpu) {
    let moving = abi::arg(c, 4) != 0;
    reencarnate_cop_by_slice(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, moving);
}

/// `AILife_ReencarnateCopByPosition` (AILIFE.CPP, 0x80068b24): the police car put still at
/// `pos` with frame `mat`, on `slice` going `dir` (the track's on a one-way track; on the
/// road's centre first: the offset at sp+32, the velocity at sp+16), settled.
pub fn reencarnate_cop_by_position(c: &mut Cpu, car: Car, slice: i32, dir: i32, pos: Coord, mat: Matrix) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0 = [slice as u32, car.0, dir as u32, mat.0, pos.0, r.0[5], r.0[6], r.0[7], r.0[8]];
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_8b64); // memset
        r.call(c, nfs4rc::f_800e4f04, &[sp + 32, 0, 12], 0x8006_8b74);
        let m = &mut c.m;
        car.n().sim_road_info().set_slice(m, slice as i16);
        let dir = one_way_direction(m, dir);
        r.0[2] = dir as u32;
        stop(m, car, dir);
        let (slice, dir) = (car.n().sim_road_info().slice(m) as i32, car.direction(m));
        r.enter(c, 0x800a_22a8, 0x8006_8bcc, |c| crate::newton::set_initial_slice_position_orientation_etc(c, car.n(), slice, Coord(sp + 32), dir));
        put(&mut c.m, car, pos, mat, Coord(sp + 16));
        r.call(c, nfs4rc::f_80067974, &[car.0], 0x8006_8c8c); // AIInit_ClearAICar
        find_road_position(c, &r, car, [0x8006_8c94, 0x8006_8cb4]);
    })
}

/// The car going `dir`, still, its AI physics reset.
fn stop(m: &mut Mem, car: Car, dir: i32) {
    car.set_direction(m, dir);
    car.set_desired_direction(m, dir);
    car.set_desired_speed(m, 0);
    car.set_current_speed(m, 0);
    crate::aiphysic::reset_car(m, car);
}

pub fn abi_reencarnate_cop_by_position(c: &mut Cpu) {
    let mat = Matrix(abi::arg(c, 4));
    reencarnate_cop_by_position(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, Coord(c.r[7]), mat);
}

/// `AILife_ReencarnateCopByLatPosAndRotation` (AILIFE.CPP, 0x80068cd4): the police car put
/// still at `slice`, `lat` across its way (the offset at sp+32, the velocity at sp+16), going
/// `dir` (the track's on a one-way track), turned by `yaw` (its matrix at sp+48), settled.
pub fn reencarnate_cop_by_lat_pos_and_rotation(c: &mut Cpu, car: Car, slice: i32, dir: i32, lat: i32, yaw: i32) {
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = slice as u32;
        r.0[1] = dir as u32;
        r.0[2] = car.0;
        r.0[3] = lat as u32;
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_8d0c); // memset
        r.call(c, nfs4rc::f_800e4f04, &[sp + 32, 0, 12], 0x8006_8d1c);
        let m = &mut c.m;
        car.n().set_active(m, 1);
        car.n().sim_road_info().set_slice(m, slice as i16);
        let dir = one_way_direction(m, dir);
        r.0[1] = dir as u32;
        stop(m, car, dir);
        let across = lat.wrapping_mul(car.direction(m));
        m.w32(sp + 32, across as u32);
        let (slice, dir) = (car.n().sim_road_info().slice(m) as i32, car.direction(m));
        r.enter(c, 0x800a_22a8, 0x8006_8d90, |c| crate::newton::set_initial_slice_position_orientation_etc(c, car.n(), slice, Coord(sp + 32), dir));
        r.0[1] = sp + 48;
        turn(c, &mut r, car, sp + 48, yaw, [0x8006_8da0, 0x8006_8db4]);
        let orient = car.n().orient_mat();
        copy_matrix(&mut c.m, orient, car.n().shadow_mat());
        let v = Coord(sp + 16).get(&mut c.m);
        car.n().linear_vel().set(&mut c.m, v);
        r.0[0] = orient.0.wrapping_add(0x20);
        r.call(c, nfs4rc::f_80067974, &[car.0], 0x8006_8e14); // AIInit_ClearAICar
        find_road_position(c, &r, car, [0x8006_8e1c, 0x8006_8e3c]);
    })
}

pub fn abi_reencarnate_cop_by_lat_pos_and_rotation(c: &mut Cpu) {
    let yaw = abi::arg(c, 4) as i32;
    reencarnate_cop_by_lat_pos_and_rotation(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, yaw);
}

/// `AILife_IsCoordInThisLiveArea` (AILIFE.CPP, 0x80068e58): `pos` within 216 m of `car` (else
/// the distance logged).
pub fn is_coord_in_this_live_area(c: &mut Cpu, pos: Coord, car: Car) -> bool {
    abi::frame(c, 24, |c, sp| {
        let m = &mut c.m;
        let (d, dz) = ground_distance(m, pos, car);
        if d <= LIVE_DISTANCE {
            return true;
        }
        let hi = ((d as i64 * 0x8000_8001u32 as i32 as i64) >> 32) as i32;
        let metres = (hi.wrapping_add(d) >> 15).wrapping_sub(d >> 31);
        debug(m, sp, [DEBUG_DIST, metres as u32, dz as u32, hi as u32]);
        false
    })
}

pub fn abi_is_coord_in_this_live_area(c: &mut Cpu) {
    c.r[2] = is_coord_in_this_live_area(c, Coord(c.r[4]), Car(c.r[5])) as u32;
}

/// `AILife_IsTrafficCarInAnyLiveArea` (AILIFE.CPP, 0x80068f04): the first basis car whose live
/// area the car is in, or none (0).
pub fn is_traffic_car_in_any_live_area(c: &mut Cpu, car: Car) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = car.0.wrapping_add(0xa0);
        let mut i = 0;
        while i < NUM_LIFE_BASIS_CARS.get(&mut c.m) {
            let at = LIFE_BASIS_CARS + 4 * i as u32;
            r.0[0] = at;
            r.0[1] = i as u32;
            let basis = Car(c.m.r32(at));
            if r.enter(c, 0x8006_8e58, 0x8006_8f40, |c| is_coord_in_this_live_area(c, car.n().position(), basis)) {
                return c.m.r32(at);
            }
            i += 1;
        }
        0
    })
}

pub fn abi_is_traffic_car_in_any_live_area(c: &mut Cpu) {
    c.r[2] = is_traffic_car_in_any_live_area(c, Car(c.r[4]));
}

/// `AILife_IsCoordInThisVisibleArea` (AILIFE.CPP, 0x80068f88): `pos` within 172 m of `car`.
pub fn is_coord_in_this_visible_area(m: &mut Mem, pos: Coord, car: Car) -> bool {
    ground_distance(m, pos, car).0 <= VISIBLE_DISTANCE
}

pub fn abi_is_coord_in_this_visible_area(c: &mut Cpu) {
    c.r[2] = is_coord_in_this_visible_area(&mut c.m, Coord(c.r[4]), Car(c.r[5])) as u32;
}

/// `AILife_IsCarInAnyVisibleArea` (AILIFE.CPP, 0x80068fec): a human sees the car (the human,
/// or none: 0).
pub fn is_car_in_any_visible_area(c: &mut Cpu, car: Car) -> u32 {
    abi::frame(c, 24, |c, _| Saved::of(c).enter(c, 0x8006_914c, 0x8006_8ffc, |c| is_position_in_any_visible_area(c, car.n().position())))
}

pub fn abi_is_car_in_any_visible_area(c: &mut Cpu) {
    c.r[2] = is_car_in_any_visible_area(c, Car(c.r[4]));
}

/// The first car of `list` (`n` of them) within `within` of `slice` along the road, or none.
fn near_along(m: &mut Mem, list: u32, n: fn(&mut Mem) -> i32, slice: i32, within: i32) -> u32 {
    let mut i = 0;
    while i < n(m) {
        let car = Car(m.r32(list + 4 * i as u32));
        let at = car.n().sim_road_info().slice(m) as i32;
        if apx_spline_distance_slices(m, at, slice).wrapping_abs() <= within {
            return car.0;
        }
        i += 1;
    }
    0
}

/// `AILife_IsSliceInAnyVisibleArea` (AILIFE.CPP, 0x8006900c): the first human within sight of
/// `slice` along the road, or none (0).
pub fn is_slice_in_any_visible_area(m: &mut Mem, slice: i32) -> u32 {
    near_along(m, HUMAN_RACE_CARS, |m| NUM_HUMAN_RACE_CARS.get(m), slice, VISIBLE_ALONG)
}

pub fn abi_is_slice_in_any_visible_area(c: &mut Cpu) {
    c.r[2] = is_slice_in_any_visible_area(&mut c.m, c.r[4] as i32);
}

/// `AILife_IsSliceCloseToAnyCopCar` (AILIFE.CPP, 0x800690ac): the first police car within 50 m
/// of `slice` along the road, or none (0).
pub fn is_slice_close_to_any_cop_car(m: &mut Mem, slice: i32) -> u32 {
    near_along(m, COP_CARS, |m| NUM_COP_CARS.get(m), slice, COP_CLOSE_ALONG)
}

pub fn abi_is_slice_close_to_any_cop_car(c: &mut Cpu) {
    c.r[2] = is_slice_close_to_any_cop_car(&mut c.m, c.r[4] as i32);
}

/// `AILife_IsPositionInAnyVisibleArea` (AILIFE.CPP, 0x8006914c): the first human who sees
/// `pos`, or none (0).
pub fn is_position_in_any_visible_area(c: &mut Cpu, pos: Coord) -> u32 {
    abi::frame(c, 32, |c, _| {
        let m = &mut c.m;
        let mut i = 0;
        while i < NUM_HUMAN_RACE_CARS.get(m) {
            let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
            if is_coord_in_this_visible_area(m, pos, human) {
                return human.0;
            }
            i += 1;
        }
        0
    })
}

pub fn abi_is_position_in_any_visible_area(c: &mut Cpu) {
    c.r[2] = is_position_in_any_visible_area(c, Coord(c.r[4]));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::globals::CARS_LIST;
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused: the arguments (a car, another, a slice, a direction, a side,
    /// moving or not, a yaw) at `ARGS`.
    const ARGS: u32 = 0x8014_3300;

    /// The AI speeds' shuffled race; 1 to 3 life basis cars and 0 to 3 police cars of the race's,
    /// cars anywhere near (or far from) one another, out of the race, traffic or basis cars or
    /// not, the random state anywhere, a one-way track, either drive side, the race on the
    /// scratchpad stack or not; arguments for the calls.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        let cars: Vec<Car> = (0..n).map(|i| Car(m.r32(CARS_LIST + 4 * i))).collect();
        NUM_LIFE_BASIS_CARS.set(m, 1 + pick(3));
        NUM_COP_CARS.set(m, pick(4));
        for k in 0..3 {
            m.w32(LIFE_BASIS_CARS + 4 * k, cars[pick(n) as usize].0);
            m.w32(COP_CARS + 4 * k, cars[pick(n) as usize].0);
        }
        let near = cars[pick(n) as usize].n().position().get(m);
        for &car in &cars {
            let far = [0x10_0000, 0xB0_0000, 0xE0_0000, 0x400_0000][pick(4) as usize];
            let p = car.n().position();
            p.set_x(m, near.x.wrapping_add(pick(2 * far as u32) - far));
            p.set_z(m, near.z.wrapping_add(pick(2 * far as u32) - far));
            let flags = [0, CAR_OUT, CAR_TRAFFIC, CAR_LIFE_BASIS, CAR_LIFE_BASIS | CAR_TRAFFIC][pick(5) as usize];
            car.update_car_flags(m, |f| f & !(CAR_OUT | CAR_TRAFFIC | CAR_LIFE_BASIS) | flags);
            car.set_basis_car(m, cars[pick(n) as usize]);
            car.set_current_speed(m, [0, pick(0x40_0000) - 0x20_0000][pick(2) as usize]);
            car.set_direction(m, [1, -1][pick(2) as usize]);
        }
        FAST_RANDOM.set(m, pick(0x1_0000) as u32);
        AITUNE_ONE_WAY.set(m, (pick(3) == 0) as i32);
        GAME_SETUP.set_reverse_track(m, pick(2));
        AITUNE_DRIVE_SIDE.set(m, [1, -1][pick(2) as usize]);
        STACK_SPEED_UP_ENABLED.set(m, pick(2));
        W_SAVE_PTR.set(m, 0x801F_E000);
        let slices = NUM_SLICES.get(m) as u32;
        let args = [
            cars[pick(n) as usize].0,
            cars[pick(n) as usize].0,
            pick(slices) as u32,
            [1, -1][pick(2) as usize] as u32,
            [1, -1, 0][pick(3) as usize] as u32,
            pick(2) as u32,
            pick(0x1000) as u32,
        ];
        for (k, a) in args.into_iter().enumerate() {
            m.w32(ARGS + 4 * k as u32, a);
        }
    }

    /// The shuffle's arguments.
    fn args(c: &mut Cpu) -> [u32; 7] {
        std::array::from_fn(|k| c.m.r32(ARGS + 4 * k as u32))
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    /// The arguments from the fifth on where the caller puts them (sp+16), as for the original.
    fn home(c: &mut Cpu, a: &[u32]) {
        let sp = c.r[29];
        for (k, v) in a.iter().skip(4).enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, *v);
        }
    }

    #[test]
    fn ailife_fuzz() {
        // (each call's arguments from the shuffle's: a car, another, a slice, a direction, a
        // side, moving, a yaw; the port takes them as the original does)
        let cases: [(&str, fn(&mut Cpu), fn([u32; 7]) -> Vec<u32>, Port); 19] = [
            ("EvaluateLife", nfs4rc::f_80067e50, |a| vec![a[0]], |c, a| Some(evaluate_life(c, Car(a[0])) as u32)),
            ("RCSetSpeeds", nfs4rc::f_80067eb4, |a| vec![a[0]], |c, a| {
                rc_set_speeds(c, Car(a[0]));
                None
            }),
            ("RCPickSliceAndDirection", nfs4rc::f_80067ee4, |a| vec![a[0]], |c, a| {
                rc_pick_slice_and_direction(c, Car(a[0]));
                None
            }),
            ("RCPickDesiredLatPosition", nfs4rc::f_8006831c, |a| vec![a[0]], |c, a| {
                rc_pick_desired_lat_position(&mut c.m, Car(a[0]));
                None
            }),
            ("PlaceCarAtLocation6", nfs4rc::f_80068450, |a| vec![a[0], a[2], a[1] & 0x3F_FFFF, a[3], a[4].wrapping_mul(0x10_0000), a[6]], |c, a| {
                home(c, a);
                place_car_at_location_at(c, Car(a[0]), a[1] as i32, a[2] as i32, a[3] as i32, a[4] as i32, a[5] as i32);
                None
            }),
            ("SetInitialSlicePositionOrientationEtc", nfs4rc::f_8006848c, |a| vec![a[0]], |c, a| {
                set_initial_slice_position_orientation_etc(c, Car(a[0]));
                None
            }),
            ("PlaceCarAtLocation", nfs4rc::f_800684e0, |a| vec![a[0], a[6]], |c, a| {
                place_car_at_location(c, Car(a[0]), a[1] as i32);
                None
            }),
            ("ReencarnateTraffic", nfs4rc::f_800686e4, |a| vec![a[0]], |c, a| {
                reencarnate_traffic(c, Car(a[0]));
                None
            }),
            ("ReencarnateTrafficByPosition", nfs4rc::f_80068794, |a| vec![a[0], a[2], a[3], a[1] + 0xa0, a[1] + 0xf0], |c, a| {
                home(c, a);
                reencarnate_traffic_by_position(c, Car(a[0]), a[1] as i32, a[2] as i32, Coord(a[3]), Matrix(a[4]));
                None
            }),
            ("ReencarnateCopBySlice", nfs4rc::f_800689a0, |a| vec![a[0], a[2], a[3], a[4], a[5]], |c, a| {
                home(c, a);
                reencarnate_cop_by_slice(c, Car(a[0]), a[1] as i32, a[2] as i32, a[3] as i32, a[4] != 0);
                None
            }),
            ("ReencarnateCopByPosition", nfs4rc::f_80068b24, |a| vec![a[0], a[2], a[3], a[1] + 0xa0, a[1] + 0xf0], |c, a| {
                home(c, a);
                reencarnate_cop_by_position(c, Car(a[0]), a[1] as i32, a[2] as i32, Coord(a[3]), Matrix(a[4]));
                None
            }),
            ("ReencarnateCopByLatPosAndRotation", nfs4rc::f_80068cd4, |a| vec![a[0], a[2], a[3], a[1] & 0x3F_FFFF, a[6]], |c, a| {
                home(c, a);
                reencarnate_cop_by_lat_pos_and_rotation(c, Car(a[0]), a[1] as i32, a[2] as i32, a[3] as i32, a[4] as i32);
                None
            }),
            ("IsCoordInThisLiveArea", nfs4rc::f_80068e58, |a| vec![a[0] + 0xa0, a[1]], |c, a| Some(is_coord_in_this_live_area(c, Coord(a[0]), Car(a[1])) as u32)),
            ("IsTrafficCarInAnyLiveArea", nfs4rc::f_80068f04, |a| vec![a[0]], |c, a| Some(is_traffic_car_in_any_live_area(c, Car(a[0])))),
            ("IsCoordInThisVisibleArea", nfs4rc::f_80068f88, |a| vec![a[0] + 0xa0, a[1]], |c, a| Some(is_coord_in_this_visible_area(&mut c.m, Coord(a[0]), Car(a[1])) as u32)),
            ("IsCarInAnyVisibleArea", nfs4rc::f_80068fec, |a| vec![a[0]], |c, a| Some(is_car_in_any_visible_area(c, Car(a[0])))),
            ("IsSliceInAnyVisibleArea", nfs4rc::f_8006900c, |a| vec![a[2]], |c, a| Some(is_slice_in_any_visible_area(&mut c.m, a[0] as i32))),
            ("IsSliceCloseToAnyCopCar", nfs4rc::f_800690ac, |a| vec![a[2]], |c, a| Some(is_slice_close_to_any_cop_car(&mut c.m, a[0] as i32))),
            ("IsPositionInAnyVisibleArea", nfs4rc::f_8006914c, |a| vec![a[0] + 0xa0], |c, a| Some(is_position_in_any_visible_area(c, Coord(a[0])))),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_500, 0x11F0 + k as u64, shuffle, original, &|c| call(args(c)), port);
        }
    }

    /// The shuffle as on the scratchpad stack, where the race always runs with the flag set (the
    /// cars put down on the saved stack: the scratchpad's 1 KB doesn't hold it).
    fn on_scratchpad(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
    }

    /// The stack the rebirths leave (their callees' frames, the debug output's arguments, the
    /// velocities, offsets and turns); the fifth argument is whatever the stack holds.
    #[test]
    fn life_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 9] = [
            ("RCSetSpeeds", 0x8006_7eb4, nfs4rc::f_80067eb4),
            ("RCPickSliceAndDirection", 0x8006_7ee4, nfs4rc::f_80067ee4),
            ("PlaceCarAtLocation", 0x8006_84e0, nfs4rc::f_800684e0),
            ("ReencarnateTraffic", 0x8006_86e4, nfs4rc::f_800686e4),
            ("EvaluateLife", 0x8006_7e50, nfs4rc::f_80067e50),
            ("ReencarnateCopBySlice", 0x8006_89a0, nfs4rc::f_800689a0),
            ("ReencarnateCopByPosition", 0x8006_8b24, nfs4rc::f_80068b24),
            ("ReencarnateCopByLatPosAndRotation", 0x8006_8cd4, nfs4rc::f_80068cd4),
            ("ReencarnateTrafficByPosition", 0x8006_8794, nfs4rc::f_80068794),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 500, 0x1210 + k as u64, on_scratchpad, &|c, _| {
                let a = args(c);
                match name {
                    "ReencarnateCopBySlice" => vec![a[0], a[2], a[3], a[4]],
                    "ReencarnateCopByPosition" | "ReencarnateTrafficByPosition" => vec![a[0], a[2], a[3], a[1] + 0xa0],
                    "ReencarnateCopByLatPosAndRotation" => vec![a[0], a[2], a[3], a[1] & 0x3F_FFFF],
                    "PlaceCarAtLocation" => vec![a[0], a[6]],
                    _ => vec![a[0]],
                }
            });
        }
    }
}
