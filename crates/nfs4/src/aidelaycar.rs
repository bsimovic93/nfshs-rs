//! AIDELAYCAR.CPP: a target car followed with a delay (a police car chasing): where the target
//! was, relative to the chaser, eased toward where it is by the delay factor each tick.

use crate::abi::{self, Saved};
use crate::aiworld::{apx_spline_distance, lane_index};
use crate::fixed::fixedmult;
use crate::globals::NUM_SLICES;
use crate::types::{AIDelayCar, Car, Coord};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

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

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y), a.z.wrapping_sub(b.z))
}

/// `AIDelayCar::AIDelayCar(Car_tObj*, Car_tObj*, int)` (AIDELAYCAR.CPP, 0x8005b218): a delayed
/// copy of `target` seen from `basis`, eased by `factor`.
pub fn construct(c: &mut Cpu, this: AIDelayCar, basis: Car, target: Car, factor: i32) -> AIDelayCar {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        this.set_basis_car_(&mut c.m, basis);
        this.set_delay_factor_(&mut c.m, factor);
        r.enter(c, 0x8005_b24c, 0x8005_b238, |c| set_new_target_car(&mut c.m, this, target));
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIDelayCar(c.r[4]), Car(c.r[5]), Car(c.r[6]), c.r[7] as i32).0;
}

/// `AIDelayCar::SetNewTargetCar` (AIDELAYCAR.CPP, 0x8005b24c): the copy at `target` now.
pub fn set_new_target_car(m: &mut Mem, this: AIDelayCar, target: Car) {
    this.set_target_car_(m, target);
    let basis = this.basis_car_(m);
    let d = apx_spline_distance(m, basis, target);
    let t = this.target_car_(m);
    this.set_delta_meters_(m, d);
    let slice = t.n().sim_road_info().slice(m) as i32;
    this.set_slice_(m, slice);
    let (bp, tp) = (this.basis_car_(m).n().position().get(m), t.n().position().get(m));
    this.delta_position_().set(m, sub(bp, tp));
    let tp = this.target_car_(m).n().position().get(m);
    this.position_().set(m, tp);
    let (b, t) = (this.basis_car_(m), this.target_car_(m));
    let d = b.road_position(m).wrapping_sub(t.road_position(m));
    this.set_delta_road_position_(m, d);
    let t = this.target_car_(m);
    let v = t.road_position(m);
    this.set_road_position_(m, v);
    let v = t.lane_index(m);
    this.set_lane_index_(m, v);
    let v = t.current_speed(m);
    this.set_current_speed_(m, v);
}

pub fn abi_set_new_target_car(c: &mut Cpu) {
    set_new_target_car(&mut c.m, AIDelayCar(c.r[4]), Car(c.r[5]));
}

/// `AIDelayCar::Update` (AIDELAYCAR.CPP, 0x8005b334, each tick): the gap along the road, the
/// offset in the world and across the road and the speed each moved by the factor toward the
/// target's; the copy's slice, position and lane from the chaser's and them (the differences
/// kept at sp+16 and sp+32).
pub fn update(c: &mut Cpu, this: AIDelayCar) {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let (t, b) = (this.target_car_(m), this.basis_car_(m));
        let d = apx_spline_distance(m, t, b);
        let factor = this.delay_factor_(m);
        let step = fixedmult(d.wrapping_sub(this.delta_meters_(m)), factor);
        let meters = this.delta_meters_(m).wrapping_add(step);
        this.set_delta_meters_(m, meters);
        let hi = ((meters as i64 * 0x2AAA_AAAB) >> 32) as i32;
        let by = (hi >> 16).wrapping_sub(meters >> 31);
        let here = this.basis_car_(m).n().sim_road_info().slice(m) as i32;
        let slice = slice_on(m, here, by);
        this.set_slice_(m, slice);
        let (tp, bp) = (this.target_car_(m).n().position().get(m), this.basis_car_(m).n().position().get(m));
        let off = sub(tp, bp);
        Coord(sp + 16).set(m, off);
        let dp = this.delta_position_().get(m);
        let err = sub(off, dp);
        Coord(sp + 32).set(m, err);
        let dp = this.delta_position_();
        let f = this.delay_factor_(m);
        dp.update_x(m, |v| v.wrapping_add(fixedmult(err.x, f)));
        let f = this.delay_factor_(m);
        dp.update_y(m, |v| v.wrapping_add(fixedmult(err.y, f)));
        let f = this.delay_factor_(m);
        dp.update_z(m, |v| v.wrapping_add(fixedmult(err.z, f)));
        let (bp, dp) = (this.basis_car_(m).n().position().get(m), this.delta_position_().get(m));
        let v = Vec3::new(bp.x.wrapping_add(dp.x), bp.y.wrapping_add(dp.y), bp.z.wrapping_add(dp.z));
        this.position_().set(m, v);
        let (t, b) = (this.target_car_(m), this.basis_car_(m));
        let across = t.road_position(m).wrapping_sub(b.road_position(m)).wrapping_sub(this.delta_road_position_(m));
        let f = this.delay_factor_(m);
        let delta = this.delta_road_position_(m).wrapping_add(fixedmult(across, f));
        this.set_delta_road_position_(m, delta);
        let at = this.basis_car_(m).road_position(m).wrapping_add(delta);
        this.set_road_position_(m, at);
        let slice = this.slice_(m);
        let lane = r.enter(c, 0x8007_3dac, 0x8005_b548, |c| lane_index(&mut c.m, slice, at));
        let m = &mut c.m;
        this.set_lane_index_(m, lane);
        let target = this.target_car_(m).current_speed(m);
        let f = this.delay_factor_(m);
        let speed = this.current_speed_(m);
        this.set_current_speed_(m, speed.wrapping_add(fixedmult(target.wrapping_sub(speed), f)));
    })
}

pub fn abi_update(c: &mut Cpu) {
    update(c, AIDelayCar(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, NUM_CARS};
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused.
    const DELAY_CAR: u32 = 0x8014_3100;

    /// The AI speeds' shuffled race (cars on any slices and lanes, at any speeds); a delay car
    /// following any car from any other (or itself), its factor anything to a whole, its gap,
    /// offsets and speed anything near.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        let this = AIDelayCar(DELAY_CAR);
        let basis = Car(m.r32(CARS_LIST + 4 * pick(n) as u32));
        let target = Car(m.r32(CARS_LIST + 4 * pick(n) as u32));
        this.set_basis_car_(m, basis);
        this.set_target_car_(m, target);
        this.set_delay_factor_(m, [0, 0x1_0000, pick(0x1_0001), pick(0x2000)][pick(4) as usize]);
        this.set_delta_meters_(m, pick(0x200_0000) - 0x100_0000);
        let slices = NUM_SLICES.get(m) as u32;
        this.set_slice_(m, pick(slices));
        for k in 0..3 {
            m.w32(DELAY_CAR + 0x14 + 4 * k, (pick(0x100_0000) - 0x80_0000) as u32);
        }
        this.set_delta_road_position_(m, pick(0x20_0000) - 0x10_0000);
        this.set_current_speed_(m, pick(0x50_0000));
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aidelaycar_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 3] = [
            ("AIDelayCar", nfs4rc::f_8005b218, |c, a| Some(construct(c, AIDelayCar(a[0]), Car(a[1]), Car(a[2]), a[3] as i32).0)),
            ("SetNewTargetCar", nfs4rc::f_8005b24c, |c, a| {
                set_new_target_car(&mut c.m, AIDelayCar(a[0]), Car(a[1]));
                None
            }),
            ("Update", nfs4rc::f_8005b334, |c, a| {
                update(c, AIDelayCar(a[0]));
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 2_000, 0xDE1 + k as u64, shuffle, original, &|c| {
                let this = AIDelayCar(DELAY_CAR);
                let (b, t) = (this.basis_car_(&mut c.m), this.target_car_(&mut c.m));
                // (SetNewTargetCar's a1 is the new target)
                if name == "SetNewTargetCar" { vec![DELAY_CAR, t.0] } else { vec![DELAY_CAR, b.0, t.0, this.delay_factor_(&mut c.m) as u32] }
            }, port);
        }
    }

    /// The stack the constructor (through `SetNewTargetCar`) and `Update` (through
    /// `LaneIndex`) leave.
    #[test]
    fn delay_car_stack_fuzz() {
        same_stack("AIDelayCar", 0x8005_b218, nfs4rc::f_8005b218, 0x8007_0550, SCRATCH_TOP, 1_000, 0xDE5, shuffle, &|c, _| {
            let this = AIDelayCar(DELAY_CAR);
            let (b, t) = (this.basis_car_(&mut c.m), this.target_car_(&mut c.m));
            vec![DELAY_CAR, b.0, t.0, this.delay_factor_(&mut c.m) as u32]
        });
        same_stack("Update", 0x8005_b334, nfs4rc::f_8005b334, 0x8007_0798, SCRATCH_TOP, 1_000, 0xDE6, shuffle, &|_, _| vec![DELAY_CAR]);
    }
}
