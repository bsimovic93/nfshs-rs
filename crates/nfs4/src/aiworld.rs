//! AIWORLD.CPP: the AI's view of the road. Distances along the track (by slices, or exactly
//! along the road near by), the lanes (which can be driven, from each slice's paved profile;
//! which a position is in, by the slice's lane widths), how the road bends ahead, and the cars'
//! speeds along and across it.

use crate::abi::{self, Saved};
use crate::fixed::fixedmult;
use crate::globals::{AITUNE_DRIVE_SIDE, BWORLDSM_SLICES, INVERSE_LANE_WIDTH_TABLE, NUM_SLICES};
use crate::types::{Car, Coord, Matrix, TrkNewSlice};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`: an AI racer, traffic.
const CAR_AI_RACER: i32 = 0x8;
const CAR_TRAFFIC: i32 = 0x10;
/// `Car_tObj.AIFlags`: the car is parked.
const AI_PARKED: i32 = 0x4;
/// A slice's length along the track (6 m, 16.16).
const SLICE_LENGTH: i32 = 3 << 17;

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

fn car_slice(m: &mut Mem, car: Car) -> i32 {
    car.n().sim_road_info().slice(m) as i32
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

/// A slice's direction across the road (`right`, 16.16).
fn right(m: &mut Mem, s: TrkNewSlice) -> Vec3 {
    Vec3::new((s.right(m, 0) as i32) << 9, (s.right(m, 1) as i32) << 9, (s.right(m, 2) as i32) << 9)
}

/// The bit of `lane` (0..15, 7 the centre's right) in a slice's paved profile.
fn lane_bit(lane: i32) -> i32 {
    let v = lane.wrapping_add(1).max(0).min(15);
    1 << (15 - v)
}

/// The distance from `a` to `b` along `forward` (x and z only), the difference kept at sp+16
/// and the direction at sp+32 of the original's 56-byte frame.
fn z_distance(c: &mut Cpu, a: Vec3, b: Vec3, forward: Vec3) -> i32 {
    abi::frame(c, 56, |c, sp| {
        let m = &mut c.m;
        let d = Vec3::new(a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y), a.z.wrapping_sub(b.z));
        Coord(sp + 16).set(m, d);
        Coord(sp + 32).set(m, forward);
        fixedmult(d.x, forward.x).wrapping_add(fixedmult(d.z, forward.z))
    })
}

fn forward(m: &mut Mem, mat: Matrix) -> Vec3 {
    Vec3::new(mat.m(m, 6), mat.m(m, 7), mat.m(m, 8))
}

/// `AIWorld_ZSplineDistance(Car_tObj*, Car_tObj*)` (AIWORLD.CPP, 0x80073870): how far `a` is
/// ahead of `b` along `a`'s road.
pub fn z_spline_distance(c: &mut Cpu, a: Car, b: Car) -> i32 {
    let m = &mut c.m;
    let (pa, pb, f) = (a.n().position().get(m), b.n().position().get(m), forward(m, a.n().road_matrix()));
    z_distance(c, pa, pb, f)
}

pub fn abi_z_spline_distance(c: &mut Cpu) {
    c.r[2] = z_spline_distance(c, Car(c.r[4]), Car(c.r[5])) as u32;
}

/// `AIWorld_ZSplineDistance(coorddef*, coorddef*, matrixtdef*)` (AIWORLD.CPP, 0x80073904): how
/// far `a` is ahead of `b` along the road `road` faces.
pub fn z_spline_distance_points(c: &mut Cpu, a: Coord, b: Coord, road: Matrix) -> i32 {
    let m = &mut c.m;
    let (pa, pb, f) = (a.get(m), b.get(m), forward(m, road));
    z_distance(c, pa, pb, f)
}

pub fn abi_z_spline_distance_points(c: &mut Cpu) {
    c.r[2] = z_spline_distance_points(c, Coord(c.r[4]), Coord(c.r[5]), Matrix(c.r[6])) as u32;
}

/// `AIWorld_ApxSplineDistance(int, int)` (AIWORLD.CPP, 0x80073a9c): how far slice `a` is ahead
/// of slice `b`, the shorter way round the track, in 6 m slices.
pub fn apx_spline_distance_slices(m: &mut Mem, a: i32, b: i32) -> i32 {
    let num = NUM_SLICES.get(m);
    let half = num / 2;
    let mut d = a.wrapping_sub(b);
    if d > 0 && half < d {
        d = d.wrapping_sub(num);
    } else if d < 0 && d < half.wrapping_neg() {
        d = d.wrapping_add(num);
    }
    d.wrapping_mul(SLICE_LENGTH)
}

pub fn abi_apx_spline_distance_slices(c: &mut Cpu) {
    c.r[2] = apx_spline_distance_slices(&mut c.m, c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `AIWorld_ApxSplineDistance(Car_tObj*, Car_tObj*)` (AIWORLD.CPP, 0x80073998).
pub fn apx_spline_distance(m: &mut Mem, a: Car, b: Car) -> i32 {
    let (sa, sb) = (car_slice(m, a), car_slice(m, b));
    apx_spline_distance_slices(m, sa, sb)
}

pub fn abi_apx_spline_distance(c: &mut Cpu) {
    c.r[2] = apx_spline_distance(&mut c.m, Car(c.r[4]), Car(c.r[5])) as u32;
}

/// `AIWorld_ApxSplineDistance(Car_tObj*, int)` (AIWORLD.CPP, 0x80073a08).
pub fn apx_spline_distance_to_slice(m: &mut Mem, a: Car, slice: i32) -> i32 {
    let sa = car_slice(m, a);
    apx_spline_distance_slices(m, sa, slice)
}

pub fn abi_apx_spline_distance_to_slice(c: &mut Cpu) {
    c.r[2] = apx_spline_distance_to_slice(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIWorld_ApxSplineDistance(int, Car_tObj*)` (AIWORLD.CPP, 0x80073a74).
pub fn apx_spline_distance_from_slice(m: &mut Mem, slice: i32, b: Car) -> i32 {
    apx_spline_distance_to_slice(m, b, slice).wrapping_neg()
}

pub fn abi_apx_spline_distance_from_slice(c: &mut Cpu) {
    c.r[2] = apx_spline_distance_from_slice(&mut c.m, c.r[4] as i32, Car(c.r[5])) as u32;
}

/// Within 12 m by the slices: exact.
fn near(d: i32) -> bool {
    (d.wrapping_add(0xC_0000) as u32) <= 0x18_0000
}

/// `AIWorld_SplineDistance(Car_tObj*, Car_tObj*)` (AIWORLD.CPP, 0x80073b04): how far `a` is
/// ahead of `b`: by the slices, exactly along the road within 12 m.
pub fn spline_distance(c: &mut Cpu, a: Car, b: Car) -> i32 {
    abi::frame(c, 32, |c, _| {
        let d = apx_spline_distance(&mut c.m, a, b);
        if !near(d) {
            return d;
        }
        let mut r = Saved::of(c);
        r.0[0] = a.0;
        r.0[1] = b.0;
        r.enter(c, 0x8007_3870, 0x8007_3b48, |c| z_spline_distance(c, a, b))
    })
}

pub fn abi_spline_distance(c: &mut Cpu) {
    c.r[2] = spline_distance(c, Car(c.r[4]), Car(c.r[5])) as u32;
}

/// `AIWorld_SplineDistance(Car_tObj*, int, coorddef*)` (AIWORLD.CPP, 0x80073b5c): how far `a`
/// is ahead of the point `at` on slice `slice`.
pub fn spline_distance_to_point(c: &mut Cpu, a: Car, slice: i32, at: Coord) -> i32 {
    abi::frame(c, 32, |c, _| {
        let d = apx_spline_distance_to_slice(&mut c.m, a, slice);
        if !near(d) {
            return d;
        }
        let mut r = Saved::of(c);
        r.0[0] = a.0;
        r.0[1] = at.0;
        r.enter(c, 0x8007_3904, 0x8007_3ba4, |c| z_spline_distance_points(c, a.n().position(), at, a.n().road_matrix()))
    })
}

pub fn abi_spline_distance_to_point(c: &mut Cpu) {
    c.r[2] = spline_distance_to_point(c, Car(c.r[4]), c.r[5] as i32, Coord(c.r[6])) as u32;
}

/// `AIWorld_GameOdometer` (AIWORLD.CPP, 0x80073bb8): how far the car has driven (its slices).
pub fn game_odometer(m: &mut Mem, car: Car) -> i32 {
    (car.n().total_slice(m) as i32).wrapping_mul(SLICE_LENGTH)
}

pub fn abi_game_odometer(c: &mut Cpu) {
    c.r[2] = game_odometer(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIWorld_IsDriveableLaneInSliceRange` (AIWORLD.CPP, 0x80073bd0): whether `lane` can be
/// driven on `count` slices from `slice`, every `step`th.
pub fn is_driveable_lane_in_slice_range(m: &mut Mem, slice: i32, count: i32, step: i32, lane: i32) -> bool {
    let bit = lane_bit(lane);
    let mut by = 0i32;
    for _ in 0..count.max(0) {
        let s = slice_on(m, slice, by);
        if slice_at(m, s).paved_profile(m) as i32 & bit == 0 {
            return false;
        }
        by = by.wrapping_add(step);
    }
    true
}

pub fn abi_is_driveable_lane_in_slice_range(c: &mut Cpu) {
    c.r[2] = is_driveable_lane_in_slice_range(&mut c.m, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// `AIWorld_IsDriveableLane` (AIWORLD.CPP, 0x80073c88): `lane`'s bit of the slice's paved
/// profile (nonzero: it can be driven).
pub fn is_driveable_lane(m: &mut Mem, slice: i32, lane: i32) -> i32 {
    slice_at(m, slice).paved_profile(m) as i32 & lane_bit(lane)
}

pub fn abi_is_driveable_lane(c: &mut Cpu) {
    c.r[2] = is_driveable_lane(&mut c.m, c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `AIWorld_GetProfileMask` (AIWORLD.CPP, 0x80073cdc): `lane`'s bit in a paved profile.
pub fn get_profile_mask(lane: i32) -> i32 {
    lane_bit(lane)
}

pub fn abi_get_profile_mask(c: &mut Cpu) {
    c.r[2] = get_profile_mask(c.r[4] as i32) as u32;
}

/// `AIWorld_IsDriveableLane_UsingMask` (AIWORLD.CPP, 0x80073d1c).
pub fn is_driveable_lane_using_mask(m: &mut Mem, slice: i32, mask: i32) -> i32 {
    slice_at(m, slice).paved_profile(m) as i32 & mask
}

pub fn abi_is_driveable_lane_using_mask(c: &mut Cpu) {
    c.r[2] = is_driveable_lane_using_mask(&mut c.m, c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `AIWorld_CheckForBarrierBetweenLanes` (AIWORLD.CPP, 0x80073d38): the lanes from `a` to `b`
/// (either order) that can't be driven on the slice, as bits (none: no barrier between).
pub fn check_for_barrier_between_lanes(m: &mut Mem, slice: i32, a: i32, b: i32) -> i32 {
    let not_paved = !(slice_at(m, slice).paved_profile(m) as i32);
    let (a, b) = (14i32.wrapping_sub(a), 14i32.wrapping_sub(b));
    let (lo, hi) = if b < a { (b, a) } else { (a, b) };
    let mask = !(-1i32).wrapping_shl(hi.wrapping_sub(lo).wrapping_add(1) as u32);
    not_paved.wrapping_shr(lo as u32) & mask
}

pub fn abi_check_for_barrier_between_lanes(c: &mut Cpu) {
    c.r[2] = check_for_barrier_between_lanes(&mut c.m, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32) as u32;
}

/// `AIWorld_LaneIndex` (AIWORLD.CPP, 0x80073dac): the lane (0..13, 7 the first right of the
/// centre) at `pos` across the road on `slice`, by the slice's lane widths on that side.
pub fn lane_index(m: &mut Mem, slice: i32, pos: i32) -> i32 {
    let s = slice_at(m, slice);
    let (width, first) = if pos < 0 { (s.avg_paved_width_lf(m), 6) } else { (s.avg_paved_width_rt(m), 7) };
    let inv = m.r32(INVERSE_LANE_WIDTH_TABLE.wrapping_add(((width as u32) << 15 >> 14) * 4)) as i32;
    let lane = first + fixedmult(pos, inv) / 0x1_0000;
    lane.max(0).min(13)
}

pub fn abi_lane_index(c: &mut Cpu) {
    c.r[2] = lane_index(&mut c.m, c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `AIWorld_CalculateLaneInfo` (AIWORLD.CPP, 0x80073e74, each tick): the car's lane, and the
/// lanes its width covers (`carInLane`, a bit each; none parked or off the lanes).
pub fn calculate_lane_info(c: &mut Cpu, car: Car) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        let m = &mut c.m;
        car.set_car_in_lane(m, 0);
        if car.ai_flags(m) & AI_PARKED != 0 {
            return;
        }
        r.0[0] = 0x8000;
        let lane = |c: &mut Cpu, r: &Saved, pos: i32, ra: u32| {
            let slice = car_slice(&mut c.m, car);
            r.enter(c, 0x8007_3dac, ra, |c| lane_index(&mut c.m, slice, pos))
        };
        let pos = car.road_position(&mut c.m);
        let here = lane(c, &r, pos, 0x8007_3eac);
        car.set_lane_index(&mut c.m, here);
        let (pos, span) = (car.road_position(&mut c.m), car.road_span(&mut c.m));
        let low = lane(c, &r, pos.wrapping_sub(span).wrapping_add(0x8000), 0x8007_3ec8);
        let (pos, span) = (car.road_position(&mut c.m), car.road_span(&mut c.m));
        r.0[0] = low as u32;
        let high = lane(c, &r, pos.wrapping_add(span).wrapping_sub(0x8000), 0x8007_3ee4);
        if (high as u32) < 14 && (low as u32) < 14 && high >= low {
            let m = &mut c.m;
            for i in low..=high {
                car.update_car_in_lane(m, |v| v | 1 << i);
            }
        }
    })
}

pub fn abi_calculate_lane_info(c: &mut Cpu) {
    calculate_lane_info(c, Car(c.r[4]));
}

/// `AIWorld_CalculateDeltaRoadYaw` (AIWORLD.CPP, 0x80073f38): for an AI racer, how much the
/// road turns from the car's to the next slice (1024ths of a turn); 0 for the others.
pub fn calculate_delta_road_yaw(c: &mut Cpu, car: Car) -> i32 {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if car.car_flags(m) & CAR_AI_RACER == 0 {
            return 0;
        }
        let (slice, num) = (car_slice(m, car), NUM_SLICES.get(m));
        let yaw = car.n().road_yaw(m);
        let next = if slice + 1 < num { slice + 1 } else { slice.wrapping_sub(num - 1) };
        let mut r = Saved::of(c);
        r.0[0] = yaw as u32;
        let to = r.enter(c, 0x800a_023c, 0x8007_3f88, |c| crate::newton::calculate_slice_yaw(c, next));
        let mut d = to.wrapping_sub(yaw);
        if d >= 513 {
            d -= 1024;
        }
        if d < -512 {
            d += 1024;
        }
        d
    })
}

pub fn abi_calculate_delta_road_yaw(c: &mut Cpu) {
    c.r[2] = calculate_delta_road_yaw(c, Car(c.r[4])) as u32;
}

/// `AIWorld_CalcRoadBend` (AIWORLD.CPP, 0x80073fc0): how much the road bends `ahead` slices on:
/// that slice's direction against this one's across (positive to the right, 16.16).
pub fn calc_road_bend(m: &mut Mem, car: Car, ahead: i32) -> i32 {
    let here = car_slice(m, car);
    let there = slice_on(m, here, ahead);
    let (t, h) = (slice_at(m, there), slice_at(m, here));
    let fwd = |m: &mut Mem, i| ((t.forward(m, i) as i32) << 9) / 256;
    let rgt = |m: &mut Mem, i| ((h.right(m, i) as i32) << 9) / 256;
    let x = fwd(m, 0).wrapping_mul(rgt(m, 0));
    x.wrapping_add(fwd(m, 2).wrapping_mul(rgt(m, 2)))
}

pub fn abi_calc_road_bend(c: &mut Cpu) {
    c.r[2] = calc_road_bend(&mut c.m, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIWorld_CalcFutureLateralVel` (AIWORLD.CPP, 0x8007409c): the car's speed across the road
/// as it will be `ahead` slices on (here for slow traffic), the slice's direction across kept
/// at sp+16.
pub fn calc_future_lateral_vel(c: &mut Cpu, car: Car, ahead: i32) -> i32 {
    abi::frame(c, 48, |c, sp| {
        let m = &mut c.m;
        let mut ahead = ahead;
        if car.car_flags(m) & CAR_TRAFFIC != 0 && car.current_speed(m).wrapping_abs() <= 0x13_FFFF {
            ahead = 0;
        }
        let here = car_slice(m, car);
        let s = slice_on(m, here, ahead);
        let s = slice_at(m, s);
        let across = right(m, s);
        Coord(sp + 16).set(m, across);
        let v = car.n().linear_vel().get(m);
        let a = fixedmult(v.x, across.x).wrapping_add(fixedmult(v.y, across.y));
        a.wrapping_add(fixedmult(v.z, across.z))
    })
}

pub fn abi_calc_future_lateral_vel(c: &mut Cpu) {
    c.r[2] = calc_future_lateral_vel(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIWorld_CalcSpeed` (AIWORLD.CPP, 0x800741b4): the car's speed across the ground,
/// estimated (the larger of x and z and a quarter of the other).
pub fn calc_speed(m: &mut Mem, car: Car) {
    let v = car.n().linear_vel();
    let (x, z) = (v.x(m).wrapping_abs(), v.z(m).wrapping_abs());
    let speed = if z < x { x.wrapping_add(z >> 2) } else { z.wrapping_add(x >> 2) };
    car.set_speed(m, speed);
}

pub fn abi_calc_speed(c: &mut Cpu) {
    calc_speed(&mut c.m, Car(c.r[4]));
}

/// `AIWorld_CalcLateralVelocity` (AIWORLD.CPP, 0x800741fc): the car's speed across the road.
pub fn calc_lateral_velocity(m: &mut Mem, car: Car) -> i32 {
    let (v, r) = (car.n().linear_vel().get(m), car.n().road_matrix());
    let a = fixedmult(v.x, r.m(m, 0)).wrapping_add(fixedmult(v.y, r.m(m, 1)));
    a.wrapping_add(fixedmult(v.z, r.m(m, 2)))
}

pub fn abi_calc_lateral_velocity(c: &mut Cpu) {
    c.r[2] = calc_lateral_velocity(&mut c.m, Car(c.r[4])) as u32;
}

/// `AIWorld_FindBarrierLessLaneAndPosition` (AIWORLD.CPP, 0x80074258): the first lane that can
/// be driven on the side the car keeps to (from the centre out, at most 3 lanes over), and the
/// middle of it across the road (by the slice's lane width on that side).
pub fn find_barrier_less_lane_and_position(m: &mut Mem, car: Car, lane: u32, pos: u32) {
    let side = car.direction(m).wrapping_mul(AITUNE_DRIVE_SIDE.get(m));
    let here = car_slice(m, car);
    let s = slice_at(m, here);
    let width = if side == 1 {
        m.w32(lane, 7);
        (s.avg_paved_width_rt(m) as i32) << 15
    } else {
        m.w32(lane, 6);
        (s.avg_paved_width_lf(m) as i32) << 15
    };
    let mut over = 0;
    while over < 3 {
        let slice = car_slice(m, car);
        let l = m.r32(lane) as i32;
        if is_driveable_lane(m, slice, l) != 0 {
            break;
        }
        over += 1;
        m.w32(lane, l.wrapping_add(side) as u32);
    }
    let at = width.wrapping_mul(over).wrapping_add(((width as u32) >> 1) as i32);
    m.w32(pos, side.wrapping_mul(at) as u32);
}

pub fn abi_find_barrier_less_lane_and_position(c: &mut Cpu) {
    find_barrier_less_lane_and_position(&mut c.m, Car(c.r[4]), c.r[5], c.r[6]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, NUM_CARS};
    use crate::test_util::{Rng, SCRATCH, against_original};

    /// The AI physics' shuffled car (at `SCRATCH`), another car (`SCRATCH+4`), and a slice,
    /// a lane, a position across the road, a slice count and step (`SCRATCH+8..`), some out
    /// of range; the car somewhere along another car often.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aiphysic::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m);
        let other = Car(m.r32(CARS_LIST + 4 * pick(n as u32) as u32));
        m.w32(SCRATCH + 4, other.0);
        let car = Car(m.r32(SCRATCH));
        if pick(2) == 0 {
            let s = car.n().sim_road_info().slice(m) as i32;
            other.n().sim_road_info().set_slice(m, (s + pick(5) - 2).max(0) as i16);
            let p = car.n().position().get(m);
            other.n().position().set(m, Vec3::new(p.x + pick(0x20_0000) - 0x10_0000, p.y, p.z + pick(0x20_0000) - 0x10_0000));
        }
        let slices = NUM_SLICES.get(m);
        let args = [pick(slices as u32), [pick(14), pick(18) - 2, pick(40) - 20][pick(3) as usize], pick(0x40_0000) - 0x20_0000, pick(12) - 2, pick(9) - 4];
        for (k, v) in args.into_iter().enumerate() {
            m.w32(SCRATCH + 8 + 4 * k as u32, v as u32);
        }
        car.set_road_span(m, pick(0x3_0000));
        car.n().set_total_slice(m, pick(0x1_0000) as u16);
    }

    fn arg(c: &mut Cpu, k: u32) -> u32 {
        c.m.r32(SCRATCH + 4 * k)
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aiworld_fuzz() {
        let cases: [(&str, fn(&mut Cpu), &[u32], Port); 22] = [
            ("AIWorld_ZSplineDistance", nfs4rc::f_80073870, &[0, 1], |c, a| Some(z_spline_distance(c, Car(a[0]), Car(a[1])) as u32)),
            ("AIWorld_ApxSplineDistance", nfs4rc::f_80073998, &[0, 1], |c, a| Some(apx_spline_distance(&mut c.m, Car(a[0]), Car(a[1])) as u32)),
            ("AIWorld_ApxSplineDistance_ToSlice", nfs4rc::f_80073a08, &[0, 2], |c, a| Some(apx_spline_distance_to_slice(&mut c.m, Car(a[0]), a[1] as i32) as u32)),
            ("AIWorld_ApxSplineDistance_FromSlice", nfs4rc::f_80073a74, &[2, 0], |c, a| Some(apx_spline_distance_from_slice(&mut c.m, a[0] as i32, Car(a[1])) as u32)),
            ("AIWorld_ApxSplineDistance_Slices", nfs4rc::f_80073a9c, &[2, 3], |c, a| Some(apx_spline_distance_slices(&mut c.m, a[0] as i32, a[1] as i32) as u32)),
            ("AIWorld_SplineDistance", nfs4rc::f_80073b04, &[0, 1], |c, a| Some(spline_distance(c, Car(a[0]), Car(a[1])) as u32)),
            ("AIWorld_GameOdometer", nfs4rc::f_80073bb8, &[0], |c, a| Some(game_odometer(&mut c.m, Car(a[0])) as u32)),
            ("AIWorld_IsDriveableLaneInSliceRange", nfs4rc::f_80073bd0, &[2, 5, 6, 3], |c, a| {
                Some(is_driveable_lane_in_slice_range(&mut c.m, a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32) as u32)
            }),
            ("AIWorld_IsDriveableLane", nfs4rc::f_80073c88, &[2, 3], |c, a| Some(is_driveable_lane(&mut c.m, a[0] as i32, a[1] as i32) as u32)),
            ("AIWorld_GetProfileMask", nfs4rc::f_80073cdc, &[3], |_, a| Some(get_profile_mask(a[0] as i32) as u32)),
            ("AIWorld_IsDriveableLane_UsingMask", nfs4rc::f_80073d1c, &[2, 4], |c, a| Some(is_driveable_lane_using_mask(&mut c.m, a[0] as i32, a[1] as i32) as u32)),
            ("AIWorld_CheckForBarrierBetweenLanes", nfs4rc::f_80073d38, &[2, 3, 5], |c, a| {
                Some(check_for_barrier_between_lanes(&mut c.m, a[0] as i32, a[1] as i32, a[2] as i32) as u32)
            }),
            ("AIWorld_LaneIndex", nfs4rc::f_80073dac, &[2, 4], |c, a| Some(lane_index(&mut c.m, a[0] as i32, a[1] as i32) as u32)),
            ("AIWorld_CalculateLaneInfo", nfs4rc::f_80073e74, &[0], |c, a| {
                calculate_lane_info(c, Car(a[0]));
                None
            }),
            ("AIWorld_CalculateDeltaRoadYaw", nfs4rc::f_80073f38, &[0], |c, a| Some(calculate_delta_road_yaw(c, Car(a[0])) as u32)),
            ("AIWorld_CalcRoadBend", nfs4rc::f_80073fc0, &[0, 6], |c, a| Some(calc_road_bend(&mut c.m, Car(a[0]), a[1] as i32) as u32)),
            ("AIWorld_CalcFutureLateralVel", nfs4rc::f_8007409c, &[0, 5], |c, a| Some(calc_future_lateral_vel(c, Car(a[0]), a[1] as i32) as u32)),
            ("AIWorld_CalcSpeed", nfs4rc::f_800741b4, &[0], |c, a| {
                calc_speed(&mut c.m, Car(a[0]));
                None
            }),
            ("AIWorld_CalcLateralVelocity", nfs4rc::f_800741fc, &[0], |c, a| Some(calc_lateral_velocity(&mut c.m, Car(a[0])) as u32)),
            ("AIWorld_FindBarrierLessLaneAndPosition", nfs4rc::f_80074258, &[0, 100, 101], |c, a| {
                find_barrier_less_lane_and_position(&mut c.m, Car(a[0]), a[1], a[2]);
                None
            }),
            ("AIWorld_ZSplineDistance_Points", nfs4rc::f_80073904, &[200, 201, 202], |c, a| {
                Some(z_spline_distance_points(c, Coord(a[0]), Coord(a[1]), Matrix(a[2])) as u32)
            }),
            ("AIWorld_SplineDistance_ToPoint", nfs4rc::f_80073b5c, &[0, 2, 201], |c, a| Some(spline_distance_to_point(c, Car(a[0]), a[1] as i32, Coord(a[2])) as u32)),
        ];
        for (k, (name, original, which, port)) in cases.into_iter().enumerate() {
            let args = move |c: &mut Cpu| {
                let (car, other) = (Car(arg(c, 0)), Car(arg(c, 1)));
                which
                    .iter()
                    .map(|&w| match w {
                        // (out-parameters: the car's lane and wanted position; points and a road)
                        100 => car.0 + 0x6c4,
                        101 => car.0 + 0x558,
                        200 => car.n().position().0,
                        201 => other.n().position().0,
                        202 => car.n().road_matrix().0,
                        w => arg(c, w),
                    })
                    .collect()
            };
            against_original(name, 3_000, 0xA10 + k as u64, shuffle, original, &args, port);
        }
    }
}
