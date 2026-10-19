//! NEWTON.CPP: the rigid-body side of the simulation: where an object is on the road (its
//! slice, quad and road frame), the ground under it, its motion, and what happens at the
//! road's edges.

use crate::abi;
use crate::bworldsm::{find_closest_quad_rez, u_forward, u_normal};
use crate::fixed::{fixeddiv, fixedmult, intatan};
use crate::globals::*;
use crate::mathnfs::{longer_and_quarter, normalize_short_vector};
use crate::types::{BWorldSmPos, Car, Coord, Matrix, NewtonObj, TrkNewSimSlice, TrkNewSlice};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

fn center(m: &mut Mem, slice: i32) -> Vec3 {
    let s = slice_at(m, slice);
    Vec3::new(s.center(m, 0), s.center(m, 1), s.center(m, 2))
}

/// `a * b` as two 8.8 numbers multiply.
fn mul_8_8(a: i32, b: i32) -> i32 {
    (a / 256).wrapping_mul(b / 256)
}

/// Road surface 14: not road.
const SURFACE_NONE: i32 = 14;

/// `Newton_FindClosestQuad` (NEWTON.CPP, 0x800a0210): the road quad under the object.
pub fn find_closest_quad(c: &mut Cpu, obj: NewtonObj) -> i32 {
    abi::frame(c, 24, |c, _| find_closest_quad_rez(c, obj.position(), obj.sim_road_info(), 1))
}

pub fn abi_find_closest_quad(c: &mut Cpu) {
    c.r[2] = find_closest_quad(c, NewtonObj(c.r[4])) as u32;
}

/// The slice after `slice` (round the track).
fn next_slice(m: &mut Mem, slice: i32) -> i32 {
    let count = NUM_SLICES.get(m);
    let next = slice.wrapping_add(1);
    if next < count { next } else { slice.wrapping_sub(count.wrapping_sub(1)) }
}

/// The road's heading at `slice` (to the next one), in 1024ths of a turn.
fn slice_yaw(c: &mut Cpu, slice: i32) -> i32 {
    let next = next_slice(&mut c.m, slice);
    let (a, b) = (center(&mut c.m, slice), center(&mut c.m, next));
    intatan(&mut c.m, b.x.wrapping_sub(a.x), b.z.wrapping_sub(a.z))
}

/// `Newton_CalculateSliceYaw` (NEWTON.CPP, 0x800a023c).
pub fn calculate_slice_yaw(c: &mut Cpu, slice: i32) -> i32 {
    slice_yaw(c, slice)
}

pub fn abi_calculate_slice_yaw(c: &mut Cpu) {
    c.r[2] = calculate_slice_yaw(c, c.r[4] as i32) as u32;
}

/// `Newton_UpdateRoadGeometry` (NEWTON.CPP, 0x800a02ac): the road under an active object: its
/// centre point (the quad's middle, or the slice's centre), its frame (from the quad's
/// normal and forward; the slice's own in low detail, `simOptz` 2 and up) and heading.
pub fn update_road_geometry(c: &mut Cpu, obj: NewtonObj) {
    if obj.active(&mut c.m) == 0 {
        return;
    }
    let road = obj.sim_road_info();
    let slice = road.slice(&mut c.m) as i32;
    let detailed = obj.sim_optz(&mut c.m) < 2;
    let point = obj.road_center_point();
    if detailed {
        point.set(&mut c.m, Vec3::default());
        for i in 0..4 {
            let p = if road.sim_quad(&mut c.m).0 != 0 { road.quad_pts(i).get(&mut c.m) } else { center(&mut c.m, slice) };
            point.update_x(&mut c.m, |v| v.wrapping_add(p.x));
            point.update_y(&mut c.m, |v| v.wrapping_add(p.y));
            point.update_z(&mut c.m, |v| v.wrapping_add(p.z));
        }
        point.update_x(&mut c.m, |v| v / 4);
        point.update_y(&mut c.m, |v| v / 4);
        point.update_z(&mut c.m, |v| v / 4);
    } else {
        let p = if road.sim_quad(&mut c.m).0 != 0 { road.quad_pts(0).get(&mut c.m) } else { center(&mut c.m, slice) };
        point.set(&mut c.m, p);
    }
    let frame = obj.road_matrix();
    if detailed {
        if road.quad_changed(&mut c.m) != 0 {
            // up: the road's normal; ahead: its forward; across: up x ahead
            let n = u_normal(c, road).get(&mut c.m);
            set_row(&mut c.m, frame, 1, n);
            let f = u_forward(c, road).get(&mut c.m);
            set_row(&mut c.m, frame, 2, f);
            let across = Vec3::new(
                fixedmult(n.y, f.z).wrapping_sub(fixedmult(n.z, f.y)),
                fixedmult(n.z, f.x).wrapping_sub(fixedmult(n.x, f.z)),
                fixedmult(n.x, f.y).wrapping_sub(fixedmult(n.y, f.x)),
            );
            set_row(&mut c.m, frame, 0, across);
        }
    } else {
        let s = slice_at(&mut c.m, slice);
        let v = |m: &mut Mem, off: u32| Vec3::new((m.r8(s.0 + off) as i8 as i32) << 9, (m.r8(s.0 + off + 1) as i8 as i32) << 9, (m.r8(s.0 + off + 2) as i8 as i32) << 9);
        let (right, normal, forward) = (v(&mut c.m, 18), v(&mut c.m, 12), v(&mut c.m, 15));
        set_row(&mut c.m, frame, 0, right);
        set_row(&mut c.m, frame, 1, normal);
        set_row(&mut c.m, frame, 2, forward);
    }
    let yaw = slice_yaw(c, slice);
    obj.set_road_yaw(&mut c.m, yaw);
}

fn set_row(m: &mut Mem, frame: Matrix, row: u32, v: Vec3) {
    frame.set_m(m, 3 * row, v.x);
    frame.set_m(m, 3 * row + 1, v.y);
    frame.set_m(m, 3 * row + 2, v.z);
}

pub fn abi_update_road_geometry(c: &mut Cpu) {
    update_road_geometry(c, NewtonObj(c.r[4]));
}

/// `-(normal . (pt - origin))` across the ground, in 8.8.
fn plane_offset(m: &mut Mem, pt: Coord, normal: Coord, origin: Coord) -> i32 {
    let (p, n, o) = (pt.get(m), normal.get(m), origin.get(m));
    mul_8_8(n.x, p.x.wrapping_sub(o.x)).wrapping_neg().wrapping_sub(mul_8_8(n.z, p.z.wrapping_sub(o.z)))
}

/// `offset` over the normal's height (by its reciprocal from `divTable` when the road is near
/// level, above 0.62).
fn over_height(m: &mut Mem, offset: i32, ny: i32) -> i32 {
    if 0x9EB8 < ny {
        fixedmult(offset, DIV_TABLE.get(m, ((0x1_0000 - ny) >> 9) as u32))
    } else {
        fixeddiv(offset, ny)
    }
}

/// `Newton_FindGroundElevationGeneral` (NEWTON.CPP, 0x800a063c): the height of the plane
/// through `origin` with `normal` at `pt` (across the ground).
pub fn find_ground_elevation_general(c: &mut Cpu, pt: Coord, normal: Coord, origin: Coord) -> i32 {
    let offset = plane_offset(&mut c.m, pt, normal, origin);
    let ny = normal.y(&mut c.m);
    over_height(&mut c.m, offset, ny).wrapping_add(origin.y(&mut c.m))
}

pub fn abi_find_ground_elevation_general(c: &mut Cpu) {
    c.r[2] = find_ground_elevation_general(c, Coord(c.r[4]), Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// `Newton_FindGroundElevationRough` (NEWTON.CPP, 0x800a0798): [`find_ground_elevation_general`]
/// with a bump: two values of `fudgeTable` picked by where `pt` is.
pub fn find_ground_elevation_rough(c: &mut Cpu, pt: Coord, normal: Coord, origin: Coord) -> i32 {
    let offset = plane_offset(&mut c.m, pt, normal, origin);
    let p = pt.get(&mut c.m);
    let mut bump = 0i32;
    for k in 0..2 {
        let len = if k == 0 {
            longer_and_quarter(p.x.wrapping_abs(), (p.z >> 1).wrapping_abs())
        } else {
            longer_and_quarter((p.x >> 2).wrapping_abs(), p.z.wrapping_abs())
        };
        let v = (len >> 15).wrapping_abs();
        // v mod 32, as the original divides
        let i = v.wrapping_sub((v.wrapping_add(if v < 0 { 31 } else { 0 }) >> 5) << 5);
        bump = bump.wrapping_add((FUDGE_TABLE.get(&mut c.m, i as u32) as i32) << 7);
    }
    let ny = normal.y(&mut c.m);
    over_height(&mut c.m, offset, ny).wrapping_add(origin.y(&mut c.m)).wrapping_add(bump)
}

pub fn abi_find_ground_elevation_rough(c: &mut Cpu) {
    c.r[2] = find_ground_elevation_rough(c, Coord(c.r[4]), Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// No ground (the car is upside down or the road too steep).
const NO_GROUND: i32 = 0x8300_0000u32 as i32;

/// `Newton_FindGroundElevationAndNormalFast` (NEWTON.CPP, 0x800a096c): the ground under a car
/// on the road's plane (its normal into `normal`), all four wheels on it; none where the
/// road leans past 84 degrees.
pub fn find_ground_elevation_and_normal_fast(c: &mut Cpu, car: Car, normal: Coord) -> i32 {
    abi::frame(c, 32, |c, _| {
        let n = car.n();
        let up = Vec3::new(n.road_matrix().m(&mut c.m, 3), n.road_matrix().m(&mut c.m, 4), n.road_matrix().m(&mut c.m, 5));
        normal.set_y(&mut c.m, up.y);
        normal.set_x(&mut c.m, up.x);
        normal.set_z(&mut c.m, up.z);
        if up.y < 6554 {
            return NO_GROUND;
        }
        let height = find_ground_elevation_general(c, n.position(), normal, n.road_center_point());
        let pos = n.position().get(&mut c.m);
        let up = normal.get(&mut c.m);
        for i in 0..4 {
            let w = car.wheel(i);
            w.set_wheel_in_air(&mut c.m, 0);
            w.set_rebound(&mut c.m, 0);
            w.current_pos().set(&mut c.m, Vec3::new(pos.x, height, pos.z));
            w.set_actual_height(&mut c.m, height);
            w.set_road_surface_type(&mut c.m, 1);
            w.road_normal().set(&mut c.m, up);
        }
        height
    })
}

pub fn abi_find_ground_elevation_and_normal_fast(c: &mut Cpu) {
    c.r[2] = find_ground_elevation_and_normal_fast(c, Car(c.r[4]), Coord(c.r[5])) as u32;
}

/// `Newton_LimitCarsToDrivableDist` (NEWTON.CPP, 0x800a18b0): a car past the road's edge is
/// put back (and its road found again).
pub fn limit_cars_to_drivable_dist(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        if crate::physics::do_barrier_check(c, car) != 0 {
            let hit = car.n().collision();
            hit.set_impulse(&mut c.m, 0);
            hit.set_other_obj(&mut c.m, NewtonObj(0));
            find_closest_quad(c, car.n());
            update_road_geometry(c, car.n());
        }
    });
}

pub fn abi_limit_cars_to_drivable_dist(c: &mut Cpu) {
    limit_cars_to_drivable_dist(c, Car(c.r[4]));
}

/// `Newton_CalcPerpenHeightOfCenterPointFromGround` (NEWTON.CPP, 0x800a1c78): how far the
/// object's underside is above the plane through `point` with `normal`: its centre's
/// height less half its height (upright or upside down) or width (on its side).
pub fn calc_perpen_height_of_center_point_from_ground(c: &mut Cpu, obj: NewtonObj, normal: Coord, point: Coord) -> i32 {
    let (p, q, n) = (obj.position().get(&mut c.m), point.get(&mut c.m), normal.get(&mut c.m));
    let d = Vec3::new(p.x.wrapping_sub(q.x), p.y.wrapping_sub(q.y), p.z.wrapping_sub(q.z));
    let h = fixedmult(n.x, d.x).wrapping_add(fixedmult(n.y, d.y)).wrapping_add(fixedmult(n.z, d.z));
    let dim = obj.dimension();
    if 0xB333 < obj.orientation_to_ground().y(&mut c.m).wrapping_abs() { h.wrapping_sub(dim.y(&mut c.m)) } else { h.wrapping_sub(dim.x(&mut c.m)) }
}

pub fn abi_calc_perpen_height_of_center_point_from_ground(c: &mut Cpu) {
    c.r[2] = calc_perpen_height_of_center_point_from_ground(c, NewtonObj(c.r[4]), Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// `Newton_UpdateRoadInfo` (NEWTON.CPP, 0x800a2074): an active object's road: its distance to
/// the players, its quad and (when that changed) the road's geometry, and the surface it is
/// on.
pub fn update_road_info(c: &mut Cpu, obj: NewtonObj) {
    abi::frame(c, 24, |c, _| {
        if obj.active(&mut c.m) == 0 {
            return;
        }
        calc_dist_to_closest_player_car(c, obj);
        find_closest_quad(c, obj);
        if obj.sim_road_info().quad_changed(&mut c.m) != 0 {
            update_road_geometry(c, obj);
        }
        if obj.sim_optz(&mut c.m) == 0 {
            let q = obj.sim_road_info().sim_quad(&mut c.m);
            let surface = if q.0 == 0 { SURFACE_NONE } else { q.surface(&mut c.m) as i32 };
            obj.set_ground_surface_type(&mut c.m, surface);
            obj.set_drive_surface_type(&mut c.m, surface & 0xF);
        }
    });
}

pub fn abi_update_road_info(c: &mut Cpu) {
    update_road_info(c, NewtonObj(c.r[4]));
}

/// The road matrix copied into `to`, turned round (rows across and ahead negated) when
/// `backward`.
fn copy_road_matrix(m: &mut Mem, obj: NewtonObj, to: Matrix, backward: bool) {
    let mut r = obj.road_matrix().get(m);
    if backward {
        for i in [0, 1, 2, 6, 7, 8] {
            r.m[i] = r.m[i].wrapping_neg();
        }
    }
    to.set(m, &r);
}

/// `Newton_CopyRoadMatrixToOrientMat` (NEWTON.CPP, 0x800a2100).
pub fn copy_road_matrix_to_orient_mat(c: &mut Cpu, obj: NewtonObj, backward: i32) {
    copy_road_matrix(&mut c.m, obj, obj.orient_mat(), backward != 0);
}

pub fn abi_copy_road_matrix_to_orient_mat(c: &mut Cpu) {
    copy_road_matrix_to_orient_mat(c, NewtonObj(c.r[4]), c.r[5] as i32);
}

/// `Newton_CopyRoadMatrixToShadowMat` (NEWTON.CPP, 0x800a21d4).
pub fn copy_road_matrix_to_shadow_mat(c: &mut Cpu, obj: NewtonObj, backward: i32) {
    copy_road_matrix(&mut c.m, obj, obj.shadow_mat(), backward != 0);
}

pub fn abi_copy_road_matrix_to_shadow_mat(c: &mut Cpu) {
    copy_road_matrix_to_shadow_mat(c, NewtonObj(c.r[4]), c.r[5] as i32);
}

/// `GameSetup_tData.sgge`: the slow-motion cheat.
const SGGE_SLOW_MOTION: i32 = 4;

/// `Newton_QDUpdateVel` (NEWTON.CPP, 0x800a2974): an active object moved by its velocity (a
/// 64th of it a step; 0.8 of that across the ground in slow motion).
pub fn qd_update_vel(c: &mut Cpu, obj: NewtonObj) {
    if obj.active(&mut c.m) == 0 {
        return;
    }
    let v = obj.linear_vel().get(&mut c.m);
    let step = Vec3::new(v.x >> 6, v.y >> 6, v.z >> 6);
    let step = if GAME_SETUP.sgge(&mut c.m) & SGGE_SLOW_MOTION != 0 { Vec3::new(fixedmult(step.x, 0xCCCC), step.y, fixedmult(step.z, 0xCCCC)) } else { step };
    let pos = obj.position();
    pos.update_x(&mut c.m, |p| p.wrapping_add(step.x));
    pos.update_z(&mut c.m, |p| p.wrapping_add(step.z));
    pos.update_y(&mut c.m, |p| p.wrapping_add(step.y));
}

pub fn abi_qd_update_vel(c: &mut Cpu) {
    qd_update_vel(c, NewtonObj(c.r[4]));
}

/// `Car_tObj.carFlags` that keep a car off a spike belt (the police's own, and others).
const CAR_IMMUNE_TO_SPIKES: i32 = 0x230;

/// `Newton_CheckForSpikeBelts` (NEWTON.CPP, 0x800a3538): a car running over the police's spike
/// belt (on its slice, between its ends across the road) has a blowout; the first time, the
/// hit and its sound.
pub fn check_for_spike_belts(c: &mut Cpu, car: Car) {
    let m = &mut c.m;
    let belt = SPIKE_BELT;
    if belt.active_(m) == 0 {
        return;
    }
    if car.n().sim_road_info().slice(m) as i32 != belt.slice_(m) || car.car_flags(m) & CAR_IMMUNE_TO_SPIKES != 0 {
        return;
    }
    let across = car.road_position(m);
    if !(belt.left_lat_pos_(m) < across && across < belt.right_lat_pos_(m)) {
        return;
    }
    let blowout = car.blowout(m).wrapping_add(1);
    car.set_blowout(m, blowout);
    if blowout == 1 {
        let hit = car.n().collision();
        hit.set_impulse(m, 0xF_0000);
        hit.set_sfx_type(m, 0x5_0007);
        let pos = car.n().position().get(m);
        hit.collision_point().set(m, pos);
    }
}

pub fn abi_check_for_spike_belts(c: &mut Cpu) {
    check_for_spike_belts(c, Car(c.r[4]));
}

/// `Newton_DoPostBarrierCollisionHandling` (NEWTON.CPP, 0x800a35fc): an object that hit a
/// barrier with normal `normal`: pushed out along it (by the speed into it, at least 1/32 of
/// a metre) and turned along it, with the barrier's sound.
pub fn do_post_barrier_collision_handling(c: &mut Cpu, obj: NewtonObj, normal: Vec3) {
    // the normal is passed in a1..a3; the original keeps them in their home slots
    Coord(c.r[29] + 4).set(&mut c.m, normal);
    abi::frame(c, 112, |c, sp| {
        Coord(sp + 40).set(&mut c.m, Vec3::new(0, 0x1_0000, 0));
        let flat_z = ((normal.z / 256) << 8).wrapping_neg();
        let flat_x = (normal.x / 256) << 8;
        Coord(sp + 24).set(&mut c.m, Vec3::new(flat_z, 0, flat_x));
        let v = obj.linear_vel().get(&mut c.m);
        let into = mul_8_8(normal.x, v.x).wrapping_add(mul_8_8(normal.y, v.y)).wrapping_add(mul_8_8(normal.z, v.z));
        let push = (into.wrapping_abs().wrapping_neg() / 16).min(-1966);
        // the barrier's frame: across (the normal), up, along
        let frame = Matrix(sp + 56);
        frame.set(&mut c.m, &crate::vec::Mat3 { m: [normal.x, normal.y, normal.z, 0, 0x1_0000, 0, flat_z, 0, flat_x] });
        let impact = crate::physics::attenuate_velocity(c, obj_car(obj), push, frame);
        crate::physics::set_current_wall_type(c, 4);
        crate::physics::correct_post_collision_yaw(c, obj_car(obj), impact, normal);
    });
}

/// The car whose Newton object `obj` is (the object starts the car).
fn obj_car(obj: NewtonObj) -> Car {
    Car(obj.0)
}

pub fn abi_do_post_barrier_collision_handling(c: &mut Cpu) {
    do_post_barrier_collision_handling(c, NewtonObj(c.r[4]), Vec3::new(c.r[5] as i32, c.r[6] as i32, c.r[7] as i32));
}

/// `Newton_GenerateVector` (NEWTON.CPP, 0x800a37a4): the outward normal (across the ground)
/// of the quad's side `edge` (1, 2, 4, 8 as [`crate::bworldsm::find_edge_off`]). For another
/// `edge` the original normalizes whatever its stack held there.
pub fn generate_vector(c: &mut Cpu, edge: i32, out: Coord, pos: BWorldSmPos) {
    abi::frame(c, 80, |c, sp| {
        let up = Coord(sp + 32);
        up.set(&mut c.m, Vec3::new(0, 0x1_0000, 0));
        let v = Coord(sp + 16);
        v.set_y(&mut c.m, 0);
        let q = |m: &mut Mem, i: u32| pos.quad_pts(i).get(m);
        let side = match edge {
            1 => Some((3, 2)),
            4 => Some((2, 1)),
            2 => Some((1, 0)),
            8 => Some((0, 3)),
            _ => None,
        };
        if let Some((a, b)) = side {
            let (a, b) = (q(&mut c.m, a), q(&mut c.m, b));
            v.set_z(&mut c.m, a.z.wrapping_sub(b.z));
            v.set_x(&mut c.m, a.x.wrapping_sub(b.x));
        }
        normalize_short_vector(c, v);
        let (d, u) = (v.get(&mut c.m), up.get(&mut c.m));
        let n = Vec3::new(
            fixedmult(d.y, u.z).wrapping_sub(fixedmult(d.z, u.y)),
            fixedmult(d.z, u.x).wrapping_sub(fixedmult(d.x, u.z)),
            fixedmult(d.x, u.y).wrapping_sub(fixedmult(d.y, u.x)),
        );
        Coord(sp + 48).set(&mut c.m, n);
        out.set(&mut c.m, n);
    });
}

pub fn abi_generate_vector(c: &mut Cpu) {
    generate_vector(c, c.r[4] as i32, Coord(c.r[5]), BWorldSmPos(c.r[6]));
}

/// `Netwon_CheckForBadQuad` (NEWTON.CPP, 0x800a3930): whether car wheel `wheel` is on a bad
/// quad: surface 0, or the wheel more than 2 m above the ground under it.
pub fn check_for_bad_quad(c: &mut Cpu, car: Car, pos: BWorldSmPos, wheel: i32) -> bool {
    let q = pos.sim_quad(&mut c.m);
    if q.0 != 0 && q.surface(&mut c.m) & 0xF == 0 {
        return true;
    }
    let w = car.wheel(wheel as u32);
    0x2_0000 < w.actual_height(&mut c.m).wrapping_sub(w.current_pos().y(&mut c.m))
}

pub fn abi_check_for_bad_quad(c: &mut Cpu) {
    c.r[2] = check_for_bad_quad(c, Car(c.r[4]), BWorldSmPos(c.r[5]), c.r[6] as i32) as u32;
}

/// `Newton_LimitAngularVelocity` (NEWTON.CPP, 0x800a4140): the spin held within 1.5 a step.
pub fn limit_angular_velocity(c: &mut Cpu, obj: NewtonObj) {
    let spin = obj.angular_vel();
    let limit = |v: i32| v.clamp(-0x1_8000, 0x1_8000);
    spin.update_x(&mut c.m, limit);
    spin.update_y(&mut c.m, limit);
    spin.update_z(&mut c.m, limit);
}

pub fn abi_limit_angular_velocity(c: &mut Cpu) {
    limit_angular_velocity(c, NewtonObj(c.r[4]));
}

/// `Newton_CalculateRoadPositionFromSliceAndPosition` (NEWTON.CPP, 0x800a46c4): how far
/// `pos` is from `slice`'s centre along `across` (a direction).
pub fn calculate_road_position_from_slice_and_position(c: &mut Cpu, slice: i32, pos: Coord, across: Coord) -> i32 {
    let (cen, p, a) = (center(&mut c.m, slice), pos.get(&mut c.m), across.get(&mut c.m));
    mul_8_8(a.x, p.x.wrapping_sub(cen.x)).wrapping_add(mul_8_8(a.y, p.y.wrapping_sub(cen.y))).wrapping_add(mul_8_8(a.z, p.z.wrapping_sub(cen.z)))
}

pub fn abi_calculate_road_position_from_slice_and_position(c: &mut Cpu) {
    c.r[2] = calculate_road_position_from_slice_and_position(c, c.r[4] as i32, Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// `Newton_CalculateRoadPosition` (NEWTON.CPP, 0x800a47b8): how far the object is across the
/// road from its slice's centre.
pub fn calculate_road_position(c: &mut Cpu, obj: NewtonObj) -> i32 {
    let slice = obj.sim_road_info().slice(&mut c.m) as i32;
    calculate_road_position_from_slice_and_position(c, slice, obj.position(), Coord(obj.road_matrix().0))
}

pub fn abi_calculate_road_position(c: &mut Cpu) {
    c.r[2] = calculate_road_position(c, NewtonObj(c.r[4])) as u32;
}

/// `Car_tObj.carFlags`: damage counts half.
const CAR_TOUGH: i32 = 0x200;

/// `Newton_AddDamageZone` (NEWTON.CPP, 0x8009fa38): a hit of `amount` on damage zone `zone`
/// (0..7 round the car, 8 and 9 the engine and brakes) when damage is on: the zone takes the
/// larger of its damage and half the hit (at most 100), and its neighbours the average of
/// theirs. With `kind` 1 or 2 (2 harder) the hit also throws the car: up (and off the
/// ground) when hard, and spinning about the axes by the side hit, with some randomness.
pub fn add_damage_zone(c: &mut Cpu, car: Car, amount: i32, zone: i32, kind: i32) {
    abi::frame(c, 96, |c, sp| {
        let n = car.n();
        if abi::call(c, nfs4rc::f_800cbeb8, &[car.0]) != 0 {
            let impulse = n.collision().impulse(&mut c.m);
            abi::call(c, nfs4rc::f_800cbf64, &[impulse as u32]);
        }
        if GAME_SETUP.damage(&mut c.m) != 0 {
            let mut hit = (amount / 2).min(0x64_0000);
            if car.car_flags(&mut c.m) & CAR_TOUGH != 0 {
                hit /= 2;
            }
            let d = |m: &mut Mem, i: i32| n.damage(m, i as u32);
            let worst = hit.max(d(&mut c.m, zone));
            n.set_damage(&mut c.m, zone as u32, worst);
            if zone < 8 {
                // a zone takes the average of the two either side of it, if more
                let spread = |m: &mut Mem, target: i32, a: i32, b: i32| {
                    let avg = d(m, a).wrapping_add(d(m, b)) / 2;
                    let v = avg.max(d(m, target));
                    n.set_damage(m, target as u32, v);
                };
                let m = &mut c.m;
                match zone {
                    0 => {
                        spread(m, 1, 0, 2);
                        spread(m, 7, 0, 6);
                    }
                    1 => {
                        spread(m, 0, 7, 1);
                        spread(m, 2, 1, 3);
                    }
                    6 => {
                        spread(m, 5, 4, 6);
                        spread(m, 7, 0, 6);
                    }
                    7 => {
                        spread(m, 0, 7, 1);
                        spread(m, 6, 5, 7);
                    }
                    _ => {
                        spread(m, zone + 1, zone, zone + 2);
                        spread(m, zone - 1, zone, zone - 2);
                    }
                }
            }
        }
        if kind == 0 {
            return;
        }
        let (mut spin_x, mut spin_y, mut spin_z) = (0x2_0000, 0, 0x2_0000);
        let strength;
        if amount > 0x5A_0000 {
            let quarter = amount / 4;
            n.set_flight_time(&mut c.m, 1);
            n.collision().set_last_collision(&mut c.m, 0);
            car.collision().set_smoking(&mut c.m, 1);
            if n.obj_altitude(&mut c.m) <= 0x1_FFFF {
                n.position().update_y(&mut c.m, |y| y.wrapping_add(0x1_0000));
            }
            let up = (quarter / 3).wrapping_add(n.linear_vel().y(&mut c.m)).min(0xC_0000);
            n.linear_vel().set_y(&mut c.m, up);
            strength = (quarter / 32).min(0x9999);
            if crate::bworldsm::tunnel_flag_sm(c, n.sim_road_info()) && 0x9_0000 < n.linear_vel().y(&mut c.m) {
                n.linear_vel().set_y(&mut c.m, 0x9_0000);
            }
            let mut strength = strength;
            if quarter > 0x14_0000 {
                let r1 = crate::physics::fast_random(c);
                spin_y = (crate::physics::fast_random(c) * 3) as i32;
                spin_z = (crate::physics::fast_random(c) * 3) as i32;
                if crate::physics::fast_random(c) < 13107 {
                    strength = strength.wrapping_neg();
                }
                spin_x = (r1 << 2) as i32;
                if spin_x.wrapping_add(spin_y).wrapping_add(spin_z) <= 0x3_FFFF {
                    if spin_x <= 0xFFFF {
                        spin_x = (r1 << 3) as i32;
                    }
                    if spin_y <= 0xFFFF {
                        spin_y <<= 1;
                    }
                    if spin_z <= 0xFFFF {
                        spin_z <<= 1;
                    }
                }
            }
            if kind == 2 {
                spin_x <<= 1;
                spin_y <<= 1;
                spin_z <<= 1;
            }
            spin(c, car, sp, zone, strength, [spin_x, spin_y, spin_z]);
        } else {
            strength = (amount / 64).min(0x8000);
            spin(c, car, sp, zone, strength, [spin_x, spin_y, spin_z]);
        }
    });
}

/// The spin of a hit: the car's spin in its own frame changed about each axis by the side
/// hit, then back into the world.
fn spin(c: &mut Cpu, car: Car, sp: u32, zone: i32, strength: i32, by: [i32; 3]) {
    let n = car.n();
    let orient = n.orient_mat().get(&mut c.m).m;
    let own: [i32; 3] = {
        let w = n.angular_vel().get(&mut c.m);
        std::array::from_fn(|r| fixedmult(w.x, orient[3 * r]).wrapping_add(fixedmult(w.y, orient[3 * r + 1])).wrapping_add(fixedmult(w.z, orient[3 * r + 2])))
    };
    let ch = car.angular_vel_ch();
    ch.set(&mut c.m, own.into());
    let turn = |k: usize| fixedmult(strength, by[k]) / 2;
    if (zone as u32) < 3 {
        ch.update_x(&mut c.m, |v| v.wrapping_sub(turn(0)));
    } else {
        ch.update_x(&mut c.m, |v| v.wrapping_add(turn(0)));
    }
    if (zone.wrapping_sub(2) as u32) < 3 {
        ch.update_y(&mut c.m, |v| v.wrapping_add(turn(1)));
        ch.update_z(&mut c.m, |v| v.wrapping_add(turn(2)));
    } else {
        ch.update_y(&mut c.m, |v| v.wrapping_sub(turn(1)));
        ch.update_z(&mut c.m, |v| v.wrapping_sub(turn(2)));
    }
    let back = crate::physics::transposed_matrix(&orient);
    Matrix(sp + 16).set(&mut c.m, &crate::vec::Mat3 { m: back });
    let w = ch.get(&mut c.m);
    let world: [i32; 3] = std::array::from_fn(|r| fixedmult(w.x, back[3 * r]).wrapping_add(fixedmult(w.y, back[3 * r + 1])).wrapping_add(fixedmult(w.z, back[3 * r + 2])));
    n.angular_vel().set(&mut c.m, world.into());
}

pub fn abi_add_damage_zone(c: &mut Cpu) {
    add_damage_zone(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// Where `Newton_FindGroundElevationAndNormal` keeps its work on the stack (the original's
/// frame, 448 bytes; its locals are passed by address).
mod ground {
    pub const FRAME: u32 = 448;
    /// The sum of the wheels' ground points (a quarter of it, the average, at the end).
    pub const SUM: u32 = 16;
    /// The wheels' points under the body (4 x 12).
    pub const WHEELS: u32 = 32;
    /// The car's up axis.
    pub const UP: u32 = 80;
    /// The car's half length along its axis; then the wheels' ground points (4 x 12).
    pub const AHEAD: u32 = 96;
    pub const GROUND: u32 = 96;
    /// The car's half width along its axis.
    pub const ACROSS: u32 = 112;
    /// The underside's centre and the offset to it from the car's centre.
    pub const BASE: u32 = 128;
    pub const DOWN: u32 = 144;
    /// A copy of the car's road position (for the wheels' searches).
    pub const ROAD: u32 = 160;
    /// A wheel's road normal; then the ground's right axis.
    pub const NORMAL: u32 = 296;
    /// The road's origin under a wheel; then the ground's normal.
    pub const ORIGIN: u32 = 312;
    /// The wheels' suspension compression.
    pub const TRAVEL: u32 = 328;
    /// The ground's forward axis.
    pub const FORWARD: u32 = 344;
    /// The car's frame transposed.
    pub const BACK: u32 = 360;
    /// How many wheels rebound.
    pub const REBOUNDS: u32 = 400;
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x.wrapping_add(b.x), a.y.wrapping_add(b.y), a.z.wrapping_add(b.z))
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y), a.z.wrapping_sub(b.z))
}

/// `v` scaled by the 8.8 number `k` (each component's high 24 bits by it).
fn scaled(k: i32, v: [i32; 3]) -> Vec3 {
    Vec3::new(k.wrapping_mul(v[0] >> 8), k.wrapping_mul(v[1] >> 8), k.wrapping_mul(v[2] >> 8))
}

/// `v` held within `limit` either way.
fn capped(v: i32, limit: i32) -> i32 {
    if v > 0 { v.min(limit) } else { v.max(limit.wrapping_neg()) }
}

/// `m` times `v` (fixed point).
fn xform(m: &[i32; 9], v: Vec3) -> Vec3 {
    let row = |r: usize| fixedmult(v.x, m[3 * r]).wrapping_add(fixedmult(v.y, m[3 * r + 1])).wrapping_add(fixedmult(v.z, m[3 * r + 2]));
    Vec3::new(row(0), row(1), row(2))
}

/// `Newton_FindGroundElevationAndNormal` (NEWTON.CPP, 0x800a0a8c): the ground under a car,
/// wheel by wheel. Each wheel's point under the body finds its quad and the road's height
/// there (rough on surfaces 2 and 3; none off the road, or 2 m above the wheel: then the
/// wheel hangs); then the wheels' suspensions and rebound, the ground's normal from the four
/// points (straight up when they say the car is upside down), and from the suspensions the
/// car's pitch and roll. The normal into `normal`; returns the average ground height, and
/// the car's height above the ground into `objAltitude`.
pub fn find_ground_elevation_and_normal(c: &mut Cpu, car: Car, normal: Coord) -> i32 {
    abi::frame(c, ground::FRAME, |c, sp| find_ground_in_frame(c, car, normal, sp))
}

fn find_ground_in_frame(c: &mut Cpu, car: Car, normal_out: Coord, sp: u32) -> i32 {
    use ground::*;
    let n = car.n();
    let at = |off: u32| Coord(sp + off);
    let o = n.orient_mat().get(&mut c.m).m;
    let dim = n.dimension().get(&mut c.m);
    at(SUM).set(&mut c.m, Vec3::default());
    at(UP).set(&mut c.m, Vec3::new(o[3], o[4], o[5]));
    // the wheels: 13/16 of the half length ahead and behind, 7/8 of the half width either
    // side, the half height below the centre
    let ahead = scaled(dim.z.wrapping_mul(13) >> 12, [o[6], o[7], o[8]]);
    at(AHEAD).set(&mut c.m, ahead);
    let across = scaled(dim.x.wrapping_mul(7) >> 11, [o[0], o[1], o[2]]);
    at(ACROSS).set(&mut c.m, across);
    let down = scaled(dim.y.wrapping_neg() >> 8, [o[3], o[4], o[5]]);
    at(DOWN).set(&mut c.m, down);
    let base = add(n.position().get(&mut c.m), down);
    at(BASE).set(&mut c.m, base);
    // front left, front right, rear left, rear right
    let (front, rear) = (add(base, ahead), sub(base, ahead));
    let wheels = [sub(front, across), add(front, across), sub(rear, across), add(rear, across)];
    for (i, w) in wheels.iter().enumerate() {
        at(WHEELS + 12 * i as u32).set(&mut c.m, *w);
    }
    let road = BWorldSmPos(sp + ROAD);
    for k in 0..BWorldSmPos::SIZE / 4 {
        let v = c.m.r32(n.sim_road_info().0 + 4 * k);
        c.m.w32(road.0 + 4 * k, v);
    }

    let up = Vec3::new(0, 0x1_0000, 0);
    let (normal, origin, sum) = (at(NORMAL), at(ORIGIN), at(SUM));
    for i in 0..4u32 {
        let wheel = car.wheel(i);
        let (w, g) = (at(WHEELS + 12 * i), at(GROUND + 12 * i));
        let wp = w.get(&mut c.m);
        g.set(&mut c.m, wp);
        wheel.set_actual_height(&mut c.m, wp.y);
        crate::bworldsm::find_closest_triangle_rez(c, w, road, 1);
        let rn = u_normal(c, road).get(&mut c.m);
        normal.set(&mut c.m, rn);
        let q = road.sim_quad(&mut c.m);
        let surface = if q.0 == 0 { SURFACE_NONE } else { q.surface(&mut c.m) as i32 };
        wheel.set_road_surface_type(&mut c.m, surface);
        let kind = surface & 0xF;
        let hang = |c: &mut Cpu, y: i32| {
            normal.set(&mut c.m, up);
            sum.update_x(&mut c.m, |v| v.wrapping_add(wp.x));
            sum.update_y(&mut c.m, |v| v.wrapping_add(y));
            sum.update_z(&mut c.m, |v| v.wrapping_add(wp.z));
        };
        if rn.y < 6553 || kind == SURFACE_NONE || kind == 0 {
            // no road under it: the wheel hangs, the car's height above the ground kept
            let alt = n.obj_altitude(&mut c.m);
            hang(c, wp.y.wrapping_sub(alt));
        } else {
            let from = if q.0 != 0 { road.quad_pts(0).get(&mut c.m) } else {
                let s = road.slice(&mut c.m) as i32;
                center(&mut c.m, s)
            };
            origin.set(&mut c.m, from);
            let y = if (kind as u32).wrapping_sub(2) < 2 { find_ground_elevation_rough(c, w, normal, origin) } else { find_ground_elevation_general(c, w, normal, origin) };
            g.set_y(&mut c.m, y);
            wheel.set_actual_height(&mut c.m, y);
            if 0x2_0000 < y.wrapping_sub(wp.y) {
                // the road far above the wheel (a wall): the wheel hangs
                g.set_y(&mut c.m, wp.y);
                hang(c, wp.y);
            } else {
                let gp = g.get(&mut c.m);
                sum.update_x(&mut c.m, |v| v.wrapping_add(gp.x));
                sum.update_y(&mut c.m, |v| v.wrapping_add(y));
                sum.update_z(&mut c.m, |v| v.wrapping_add(gp.z));
            }
        }
        let gp = g.get(&mut c.m);
        wheel.current_pos().set(&mut c.m, gp);
        let rn = normal.get(&mut c.m);
        wheel.road_normal().set(&mut c.m, rn);
    }

    // the suspensions: each wheel's travel; a wheel hangs when the ground drops away
    let (mut hanging, mut rebounding) = (0, 0);
    c.m.w32(sp + REBOUNDS, 0);
    normal.set_x(&mut c.m, 0);
    normal.set_z(&mut c.m, 0);
    let specs = car.specs(&mut c.m);
    for i in 0..4u32 {
        let wheel = car.wheel(i);
        let w_y = at(WHEELS + 12 * i).y(&mut c.m);
        let travel = wheel.current_pos().y(&mut c.m).wrapping_sub(w_y) / 2;
        normal.set_y(&mut c.m, travel);
        wheel.set_rebound(&mut c.m, 0);
        if travel < -9829 {
            wheel.set_wheel_in_air(&mut c.m, 1);
            hanging += 1;
        } else if wheel.wheel_in_air(&mut c.m) == 1 {
            // a wheel in the air lands: pushed up it rebounds; otherwise it is down once the
            // car is low, upright and not falling fast
            if travel > 0 {
                rebounding += 1;
                wheel.set_wheel_in_air(&mut c.m, 0);
                wheel.set_rebound(&mut c.m, 1);
            } else if n.obj_altitude(&mut c.m) < 13107 && n.linear_vel().y(&mut c.m) >= -6552 && 0xF333 < n.orientation_to_ground().y(&mut c.m) {
                wheel.set_wheel_in_air(&mut c.m, 0);
            }
        }
        let floor = if 0x35_8000 < n.speed_xz(&mut c.m) { -9830 } else { -13107 };
        let travel = travel.max(floor);
        normal.set_y(&mut c.m, travel);
        let compression = mul_8_8(travel, at(UP).y(&mut c.m));
        c.m.w32(sp + TRAVEL + 4 * i, compression as u32);
        let force = mul_8_8(compression, specs.suspension_stiffness(&mut c.m));
        wheel.set_wheel_acc(&mut c.m, force);
        if car.car_info(&mut c.m).weight_transfer(&mut c.m) == 1 {
            wheel.set_wheel_acc(&mut c.m, force << 1);
        }
    }
    if rebounding != 0 {
        let count = (0..4).fold(0i32, |s, i| s.wrapping_add(car.wheel(i).rebound(&mut c.m)));
        c.m.w32(sp + REBOUNDS, count as u32);
    }
    for i in 0..4u32 {
        let wheel = car.wheel(i);
        if rebounding == 0 {
            // all on the ground or hanging: the suspensions pull down half as hard upright
            let a = wheel.wheel_acc(&mut c.m);
            if a < 0 && 0xDC28 < n.orientation_to_ground().y(&mut c.m) {
                wheel.set_wheel_acc(&mut c.m, a >> 1);
            }
            continue;
        }
        let vy = n.linear_vel().y(&mut c.m);
        if wheel.rebound(&mut c.m) == 0 || vy >= 0 {
            continue;
        }
        // a rebounding wheel takes its share of the fall
        let fall = vy.wrapping_abs().wrapping_add(n.ground_vel(&mut c.m));
        if fall < 0 {
            continue;
        }
        let count = c.m.r32(sp + REBOUNDS) as i32;
        let mut push = crate::fixed::div(fall, count) >> 1;
        if fall <= 0xF_FFFF {
            push = fixedmult(fixedmult(fall, 4096).max(19660), push);
        }
        wheel.update_wheel_acc(&mut c.m, |a| a.wrapping_add(push));
    }
    // the tyres' compression from the wheels' drop (into the wheels `swap` says)
    for i in 0..4u32 {
        let dy = at(WHEELS + 12 * i).y(&mut c.m).wrapping_sub(at(GROUND + 12 * i).y(&mut c.m)) >> 1;
        let v = capped(fixedmult(dy, n.orient_mat().m(&mut c.m, 4)), specs.tire_range(&mut c.m));
        let to = c.m.r32(WHEEL_SWAP + 4 * i);
        car.wheel(to).set_impact_compression(&mut c.m, v);
    }

    // the ground: the average point, and its frame from the wheels' points
    sum.update_x(&mut c.m, |v| v >> 2);
    sum.update_z(&mut c.m, |v| v >> 2);
    sum.update_y(&mut c.m, |v| v >> 2);
    let g: [Vec3; 4] = std::array::from_fn(|i| at(GROUND + 12 * i as u32).get(&mut c.m));
    let half = |a: Vec3, b: Vec3| {
        let d = sub(a, b);
        Vec3::new(d.x >> 1, d.y >> 1, d.z >> 1)
    };
    let forward = at(FORWARD);
    forward.set(&mut c.m, half(add(g[0], g[1]), add(g[2], g[3])));
    normalize_short_vector(c, forward);
    let right = at(NORMAL);
    right.set(&mut c.m, half(add(g[1], g[3]), add(g[0], g[2])));
    normalize_short_vector(c, right);
    let (f, r) = (forward.get(&mut c.m), right.get(&mut c.m));
    let ground_up = at(ORIGIN);
    ground_up.set(&mut c.m, Vec3::new(
        fixedmult(f.y, r.z).wrapping_sub(fixedmult(f.z, r.y)),
        fixedmult(f.z, r.x).wrapping_sub(fixedmult(f.x, r.z)),
        fixedmult(f.x, r.y).wrapping_sub(fixedmult(f.y, r.x)),
    ));
    normalize_short_vector(c, ground_up);
    let gu = ground_up.get(&mut c.m);
    if gu.y >= 0 {
        normal_out.set(&mut c.m, gu);
        if gu.y <= 32767 {
            hanging = 4;
        }
    } else {
        hanging = 4;
        normal_out.set(&mut c.m, up);
    }

    if n.orientation_to_ground().y(&mut c.m) < -0xE666 {
        // upside down: the spin dies away
        let slow = |v: i32| v.wrapping_mul(253) / 256;
        n.angular_vel().update_x(&mut c.m, slow);
        n.angular_vel().update_y(&mut c.m, slow);
        n.angular_vel().update_z(&mut c.m, slow);
    } else if hanging < 4 {
        // pitch and roll from the suspensions, damped (harder when fast)
        let ch = car.angular_vel_ch();
        let w = n.angular_vel().get(&mut c.m);
        ch.set(&mut c.m, xform(&o, w));
        let acc: [i32; 4] = std::array::from_fn(|i| car.wheel(i as u32).wheel_acc(&mut c.m));
        let pitch = capped(acc[2].wrapping_add(acc[3]).wrapping_sub(acc[0].wrapping_add(acc[1])) >> 2, specs.pitch_angular_vel_cap(&mut c.m));
        let roll = capped(acc[0].wrapping_add(acc[2]).wrapping_sub(acc[1].wrapping_add(acc[3])) >> 2, specs.roll_angular_vel_cap(&mut c.m));
        let damp = |v: i32, rate: i32| fixedmult(v, if 0x1_3332 < v.wrapping_abs() { 0xD999 } else { rate });
        let x = damp(ch.x(&mut c.m), specs.damping_pitch_rate(&mut c.m));
        ch.set_x(&mut c.m, x.wrapping_add(pitch));
        let z = damp(ch.z(&mut c.m), specs.damping_roll_rate(&mut c.m));
        ch.set_z(&mut c.m, z.wrapping_sub(roll));
        let back = crate::physics::transposed_matrix(&o);
        Matrix(sp + BACK).set(&mut c.m, &crate::vec::Mat3 { m: back });
        let w = ch.get(&mut c.m);
        n.angular_vel().set(&mut c.m, xform(&back, w));
    }
    // the car's height above the ground (in the first second from the road's centre)
    let from = if SIM_COUNTER.get(&mut c.m) < 64 { n.road_center_point() } else { sum };
    let alt = calc_perpen_height_of_center_point_from_ground(c, n, normal_out, from);
    n.set_obj_altitude(&mut c.m, alt);
    sum.y(&mut c.m)
}

pub fn abi_find_ground_elevation_and_normal(c: &mut Cpu) {
    c.r[2] = find_ground_elevation_and_normal(c, Car(c.r[4]), Coord(c.r[5])) as u32;
}

/// `Newton_CalcPerpenHeightOfLowestPointFromGround` (NEWTON.CPP, 0x800a18f4): how far the
/// object's lowest corner is above the plane through `point` with `normal`: its centre's
/// height less its half extents along each axis, as far as they reach down the normal.
pub fn calc_perpen_height_of_lowest_point_from_ground(c: &mut Cpu, obj: NewtonObj, normal: Coord, point: Coord) -> i32 {
    abi::frame(c, 32, |c, sp| {
        let (nv, o, dim) = (normal.get(&mut c.m), obj.orient_mat().get(&mut c.m).m, obj.dimension().get(&mut c.m));
        let dot = |a: Vec3, b: [i32; 3]| mul_8_8(a.x, b[0]).wrapping_add(mul_8_8(a.y, b[1])).wrapping_add(mul_8_8(a.z, b[2]));
        // each half extent along the normal
        let reach: [i32; 3] = std::array::from_fn(|k| mul_8_8(dot(nv, [o[3 * k], o[3 * k + 1], o[3 * k + 2]]), [dim.x, dim.y, dim.z][k]));
        for (k, r) in reach.iter().enumerate() {
            c.m.w32(sp + 4 * k as u32, *r as u32);
        }
        let (p, q) = (obj.position().get(&mut c.m), point.get(&mut c.m));
        let d = Vec3::new(p.x.wrapping_sub(q.x), p.y.wrapping_sub(q.y), p.z.wrapping_sub(q.z));
        Coord(sp + 16).set(&mut c.m, d);
        let down = |r: i32| if r >= 0 { r.wrapping_neg() } else { r };
        let h = dot(nv, [d.x, d.y, d.z]);
        down(reach[0]).wrapping_add(down(reach[1])).wrapping_add(down(reach[2])).wrapping_add(h)
    })
}

pub fn abi_calc_perpen_height_of_lowest_point_from_ground(c: &mut Cpu) {
    c.r[2] = calc_perpen_height_of_lowest_point_from_ground(c, NewtonObj(c.r[4]), Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// The distance from `a` to `b` on the ground: the longer of the x and z gaps and a quarter
/// of the shorter.
fn ground_dist(m: &mut Mem, a: NewtonObj, b: NewtonObj) -> i32 {
    let gap = |p: i32, q: i32| {
        let d = p.wrapping_sub(q);
        if d > 0 { d } else { q.wrapping_sub(p) }
    };
    let (p, q) = (a.position().get(m), b.position().get(m));
    let (dx, dz) = (gap(p.x, q.x), gap(p.z, q.z));
    if dz < dx { dx.wrapping_add(dz >> 2) } else { dz.wrapping_add(dx >> 2) }
}

fn human_car(m: &mut Mem, i: i32) -> Car {
    Car(m.r32(HUMAN_RACE_CARS.wrapping_add((i as u32) << 2)))
}

/// Far enough from the players to simulate roughly (90 m), or not at all (over 96 m, or out
/// of their sight).
const ROUGH_SIM_DIST: i32 = 0x48_0000;
const NO_SIM_DIST: i32 = 0x60_0000;

/// `Newton_CalcDistToClosestPlayerCar` (NEWTON.CPP, 0x800a1d54): how far the object is from
/// the nearest player, and from that how much of it to simulate: nothing (out of sight or
/// far away: it is held on the road), roughly (`simOptz` 1) or fully. A car coming back from
/// not being simulated is put back on the road (its quad, its geometry and the ground under
/// its wheels found again, the ground's search on the scratchpad stack when that is in use).
pub fn calc_dist_to_closest_player_car(c: &mut Cpu, obj: NewtonObj) {
    abi::frame(c, 32, |c, _| {
        let car = obj_car(obj);
        let local = GAME_SETUP.local_car(&mut c.m);
        let me = human_car(&mut c.m, local);
        let d = ground_dist(&mut c.m, obj, me.n());
        obj.set_dist_to_player(&mut c.m, d);
        let mut closest = 0;
        if NUM_HUMAN_RACE_CARS.get(&mut c.m) == 2 {
            let other = human_car(&mut c.m, 1 - local);
            let d = ground_dist(&mut c.m, obj, other.n());
            if d < obj.dist_to_player(&mut c.m) {
                // (the second player's car, whichever is local)
                closest = 1;
                obj.set_dist_to_player(&mut c.m, d);
            }
        }
        let mut hidden = false;
        if obj.dist_to_player(&mut c.m) <= NO_SIM_DIST {
            let seer = human_car(&mut c.m, closest);
            hidden = abi::call(c, nfs4rc::f_8007e5bc, &[seer.n().sim_road_info().0, obj.sim_road_info().0]) == 0;
        }
        // a car told to stay simulated is, after its first frames
        let kept = car.force_no_sim_optz(&mut c.m) != 0 && 3 <= SIM_COUNTER.get(&mut c.m);
        if (NO_SIM_DIST < obj.dist_to_player(&mut c.m) || hidden) && !kept {
            if obj.sim_optz(&mut c.m) != 2 {
                obj.set_ground_surface_type(&mut c.m, 1);
                obj.set_drive_surface_type(&mut c.m, 1);
            }
            obj.set_sim_optz(&mut c.m, 2);
            obj.angular_vel().set_x(&mut c.m, 0);
            obj.angular_vel().set_z(&mut c.m, 0);
            return;
        }
        let was = obj.sim_optz(&mut c.m);
        let rough = car.car_flags(&mut c.m) & 0x30 == 0 && !kept && ROUGH_SIM_DIST < obj.dist_to_player(&mut c.m);
        obj.set_sim_optz(&mut c.m, rough as u8);
        if was < 2 {
            return;
        }
        abi::call(c, nfs4rc::f_800874e8, &[car.0]);
        find_closest_quad(c, obj);
        update_road_geometry(c, obj);
        limit_cars_to_drivable_dist(c, car);
        let height = find_ground_off_scratchpad(c, car, Coord(DUMMY_NORMAL));
        obj.set_ground_elevation(&mut c.m, height);
        obj.set_ground_surface_type(&mut c.m, 1);
        obj.set_drive_surface_type(&mut c.m, 1);
        obj.set_ground_vel(&mut c.m, 0);
        obj.collision().set_impulse(&mut c.m, 0);
        obj.collision().set_other_obj(&mut c.m, NewtonObj(0));
        obj.set_obj_altitude(&mut c.m, 0);
        let y = obj.ground_elevation(&mut c.m).wrapping_add(obj.dimension().y(&mut c.m));
        obj.position().set_y(&mut c.m, y);
    });
}

pub fn abi_calc_dist_to_closest_player_car(c: &mut Cpu) {
    calc_dist_to_closest_player_car(c, NewtonObj(c.r[4]));
}

/// The ground under a car found with its wheels (`Newton_FindGroundElevationAndNormal`, its
/// normal into `normal`), on the saved stack when the race runs on the scratchpad one (as
/// `SetSp` swaps them).
fn find_ground_off_scratchpad(c: &mut Cpu, car: Car, normal: Coord) -> i32 {
    if STACK_SPEED_UP_ENABLED.get(&mut c.m) == 0 {
        return find_ground_elevation_and_normal(c, car, normal);
    }
    let swap_sp = |c: &mut Cpu| {
        let sp = c.r[29];
        c.r[29] = W_SAVE_PTR.get(&mut c.m);
        W_SAVE_PTR.set(&mut c.m, sp);
    };
    swap_sp(c);
    STACK_SPEED_UP_ENABLED.set(&mut c.m, 0);
    let height = find_ground_elevation_and_normal(c, car, normal);
    swap_sp(c);
    STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
    height
}

/// `Newton_SetInitialSlicePositionOrientationEtc` (NEWTON.CPP, 0x800a22a8): an object put on
/// the road at `slice`, `offset` from its centre in the road's frame, facing along the road
/// (or back along it unless `forward` is 1): its road position, frame and height on the
/// ground (on the road's centre line when not simulated, its quad's corners when roughly),
/// its lowest point on the road and its surface.
pub fn set_initial_slice_position_orientation_etc(c: &mut Cpu, obj: NewtonObj, slice: i32, offset: Coord, forward: i32) {
    abi::frame(c, 120, |c, sp| {
        let backward = (forward != 1) as i32;
        crate::bworldsm::set_slice(c, slice, obj.sim_road_info());
        obj.set_total_slice(&mut c.m, slice as u16);
        // the road's frame from the slice's (signed byte) axes
        let s = slice_at(&mut c.m, slice);
        let road = obj.road_matrix();
        for k in 0..3 {
            let (r, u, f) = (s.right(&mut c.m, k), s.normal(&mut c.m, k), s.forward(&mut c.m, k));
            road.set_m(&mut c.m, k, (r as i32) << 9);
            road.set_m(&mut c.m, 3 + k, (u as i32) << 9);
            road.set_m(&mut c.m, 6 + k, (f as i32) << 9);
        }
        for row in 0..3 {
            normalize_short_vector(c, Coord(road.0 + 12 * row));
        }
        let back = sp + 32;
        crate::fixed::transpose(&mut c.m, road.0, back);
        copy_road_matrix_to_orient_mat(c, obj, backward);
        copy_road_matrix_to_shadow_mat(c, obj, backward);
        // the offset into the world, from the slice's centre
        let (o, t) = (offset.get(&mut c.m), Matrix(back).get(&mut c.m).m);
        let world = xform(&t, o);
        Coord(sp + 16).set(&mut c.m, world);
        let mid = center(&mut c.m, slice);
        obj.position().set(&mut c.m, add(world, mid));
        let car = obj_car(obj);
        let optz = if car.car_flags(&mut c.m) & 4 != 0 { 0 } else { 2 };
        obj.set_sim_optz(&mut c.m, optz);
        find_closest_quad(c, obj);
        update_road_info(c, obj);
        update_road_geometry(c, obj);
        copy_road_matrix_to_orient_mat(c, obj, backward);
        copy_road_matrix_to_shadow_mat(c, obj, backward);
        match obj.sim_optz(&mut c.m) {
            2 => {
                let y = slice_at(&mut c.m, slice).center(&mut c.m, 1);
                obj.set_ground_elevation(&mut c.m, y);
            }
            1 => {
                // the average of the quad's corners (or the slice's centre four times)
                let pos = obj.sim_road_info();
                let mut sum = 0i32;
                for i in 0..4 {
                    let p = if pos.sim_quad(&mut c.m).0 != 0 {
                        pos.quad_pts(i).get(&mut c.m)
                    } else {
                        let s = pos.slice(&mut c.m) as i32;
                        center(&mut c.m, s)
                    };
                    Coord(sp + 72).set(&mut c.m, p);
                    sum = sum.wrapping_add(p.y);
                }
                obj.set_ground_elevation(&mut c.m, sum / 4);
            }
            _ => {
                let height = find_ground_off_scratchpad(c, car, Coord(DUMMY_NORMAL_INIT));
                obj.set_ground_elevation(&mut c.m, height);
                let y = height.wrapping_add(obj.dimension().y(&mut c.m));
                obj.position().set_y(&mut c.m, y);
            }
        }
        let lowest = calc_perpen_height_of_lowest_point_from_ground(c, obj, Coord(road.0 + 12), obj.road_center_point());
        obj.set_obj_altitude(&mut c.m, 0);
        obj.position().update_y(&mut c.m, |y| y.wrapping_sub(lowest));
        let surface = if obj.sim_optz(&mut c.m) != 0 {
            1
        } else {
            let q = obj.sim_road_info().sim_quad(&mut c.m);
            if q.0 == 0 { SURFACE_NONE } else { q.surface(&mut c.m) as i32 }
        };
        obj.set_ground_surface_type(&mut c.m, surface);
        obj.set_drive_surface_type(&mut c.m, surface & 0xF);
    });
}

pub fn abi_set_initial_slice_position_orientation_etc(c: &mut Cpu) {
    set_initial_slice_position_orientation_etc(c, NewtonObj(c.r[4]), c.r[5] as i32, Coord(c.r[6]), c.r[7] as i32);
}

/// `Newton_InitBaseNewtonObj` (NEWTON.CPP, 0x800a26f4): a Newton object at rest: its ID, its
/// mass (five times as heavy for cars flagged 4 in games with `sgge` bit 2; cars flagged
/// 0x20 half as heavy again, or twice as heavy in a linked game where a player drives one of
/// the first 22 car types), its moment of inertia, size and radius, upright, undamaged and
/// active.
pub fn init_base_newton_obj(c: &mut Cpu, obj: NewtonObj, id: i32, mass: i32, size: Vec3) {
    abi::frame(c, 40, |c, _| {
        let m = &mut c.m;
        obj.set_obj_id(m, id);
        obj.sim_road_info().set_sim_slice(m, TrkNewSimSlice(0));
        obj.set_sim_optz(m, 0);
        obj.position().set(m, Vec3::default());
        obj.linear_vel().set(m, Vec3::default());
        obj.set_mass(m, mass);
        let flags = obj_car(obj).car_flags(m);
        if GAME_SETUP.sgge(m) & 2 != 0 && flags & 4 != 0 {
            obj.set_mass(m, mass.wrapping_mul(5));
        }
        if flags & 0x20 != 0 {
            let early = |m: &mut Mem, i: i32| human_car(m, i).car_info(m).car_type(m) < 22;
            if GAME_SETUP.comm_mode(m) == 1 && (early(m, 0) || early(m, 1)) {
                obj.update_mass(m, |v| v << 1);
            } else {
                obj.update_mass(m, |v| v.wrapping_mul(3) / 2);
            }
        }
        let mass = obj.mass(m);
        obj.set_mass_inv(m, fixeddiv(0x1_0000, mass));
        obj.set_mo_inertia(m, mass << 3);
        obj.set_mo_inertia_inv(m, fixeddiv(0x1_0000, mass << 3));
        obj.angular_vel().set(m, Vec3::default());
        obj.orientation_to_ground().set(m, Vec3::new(0, 0x1_0000, 0));
        obj.dimension().set(m, size);
        let sq = |v: i32| (v / 256).wrapping_mul(v / 256);
        let r = crate::fixed::fixedsqrt(m, sq(size.x).wrapping_add(sq(size.y)));
        obj.set_dimension_radius(m, r);
        let r = crate::fixed::fixedsqrt(m, sq(r).wrapping_add(sq(size.z)));
        obj.set_dimension_radius(m, r);
        obj.set_flight_time(m, 0);
        obj.set_ground_surface_type(m, 1);
        obj.set_drive_surface_type(m, 1);
        obj.set_ground_vel(m, 0);
        obj.set_obj_altitude(m, 0);
        obj.set_gravity_mult(m, 0x1_0000);
        obj.set_x_rel_road_center(m, 0);
        obj.set_road_gravity_modifier(m, 0);
        let hit = obj.collision();
        hit.set_collided(m, 0);
        hit.set_impulse(m, 0);
        hit.set_other_obj(m, NewtonObj(0));
        hit.set_sfx_type(m, 0);
        hit.set_disable_collision_timer(m, 0);
        hit.set_last_other_obj(m, NewtonObj(0));
        hit.set_last_impulse(m, 0);
        hit.set_last_time(m, 0);
        for i in 0..10 {
            obj.set_damage(m, i, 0);
        }
        obj.set_re_ortho_counter(m, 0);
        obj.set_dead_timer(m, 0);
        obj.set_active(m, 1);
        obj.set_cumulated_rot(m, 0);
    });
}

pub fn abi_init_base_newton_obj(c: &mut Cpu) {
    let size = Vec3::new(abi::arg(c, 4) as i32, abi::arg(c, 5) as i32, abi::arg(c, 6) as i32);
    init_base_newton_obj(c, NewtonObj(c.r[4]), c.r[5] as i32, c.r[6] as i32, size);
}

/// `Newton_OptzRotxform` (NEWTON.CPP, 0x800a2a58): the rotation by `y` about y, then by `x`
/// about x and `z` about z where those are large enough to matter (14 units), into `out`;
/// the turn added to `cumulated` and `flag` cleared. Returns whether it is a turn worth
/// applying (any of them large enough: 20 units about y).
pub fn optz_rotxform(c: &mut Cpu, out: Matrix, x: i32, y: i32, z: i32, flag: u32, cumulated: u32) -> i32 {
    abi::frame(c, 208, |c, sp| {
        let (ax, ay, az) = (x.wrapping_abs(), y.wrapping_abs(), z.wrapping_abs());
        c.m.w32(flag, 0);
        let total = c.m.r32(cumulated).wrapping_add(ax.wrapping_add(ay).wrapping_add(az) as u32);
        c.m.w32(cumulated, total);
        let (rx, ry, rz, xy) = (Matrix(sp + 16), Matrix(sp + 56), Matrix(sp + 96), Matrix(sp + 136));
        crate::fixed::fixedxformy(&mut c.m, ry.0, y);
        let mut turned = (ay >= 20) as i32;
        if ax >= 14 {
            crate::fixed::fixedxformx(&mut c.m, rx.0, x);
            crate::mathnfs::fasttransmult(c, rx, ry, xy);
            turned = 1;
        } else {
            let m = ry.get(&mut c.m);
            xy.set(&mut c.m, &m);
        }
        if az >= 14 {
            crate::fixed::fixedxformz(&mut c.m, rz.0, z);
            crate::mathnfs::fasttransmult(c, xy, rz, out);
            turned = 1;
        } else {
            let m = xy.get(&mut c.m);
            out.set(&mut c.m, &m);
        }
        turned
    })
}

pub fn abi_optz_rotxform(c: &mut Cpu) {
    let (flag, cumulated) = (abi::arg(c, 4), abi::arg(c, 6));
    c.r[2] = optz_rotxform(c, Matrix(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, flag, cumulated) as u32;
}

/// One step of an object's rotation: its angular velocity divided by `steps` (a frame's
/// worth) turned into a rotation and applied to its frame, which is reorthogonalized
/// (`reorthogonalize`, 0x800f1018, still recompiled) every `every` steps.
fn qd_update_rot(c: &mut Cpu, obj: NewtonObj, steps: i32, scale: u32, every: u8, flag_first: bool) {
    abi::frame(c, 112, |c, sp| {
        let w = obj.angular_vel().get(&mut c.m);
        let w = Vec3::new(w.x / steps, w.y / steps, w.z / steps);
        Coord(sp + 72).set(&mut c.m, w);
        let (rot, flag) = (Matrix(sp + 32), sp + 88);
        for (k, v) in [flag, scale, obj.0 + 0x98].into_iter().enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, v);
        }
        if optz_rotxform(c, rot, w.x, w.y, w.z, flag, obj.0 + 0x98) == 0 {
            return;
        }
        let orient = obj.orient_mat();
        crate::mathnfs::fasttransmult(c, orient, rot, orient);
        let flagged = c.m.r32(flag) != 0;
        let due = if flag_first {
            flagged || {
                obj.update_re_ortho_counter(&mut c.m, |n| n.wrapping_sub(1));
                obj.re_ortho_counter(&mut c.m) == 0
            }
        } else {
            obj.update_re_ortho_counter(&mut c.m, |n| n.wrapping_sub(1));
            obj.re_ortho_counter(&mut c.m) == 0 || flagged
        };
        if due {
            abi::call(c, nfs4rc::f_800f1018, &[orient.0]);
            obj.set_re_ortho_counter(&mut c.m, every);
            obj.set_cumulated_rot(&mut c.m, 0);
        }
    });
}

/// `Newton_QDUpdateRot64Hz` (NEWTON.CPP, 0x800a2bf4): an active object's rotation over a
/// 64th of a second.
pub fn qd_update_rot_64hz(c: &mut Cpu, obj: NewtonObj) {
    if obj.active(&mut c.m) != 0 {
        qd_update_rot(c, obj, 64, 4096, 32, false);
    }
}

pub fn abi_qd_update_rot_64hz(c: &mut Cpu) {
    qd_update_rot_64hz(c, NewtonObj(c.r[4]));
}

/// `Newton_QDUpdateRot32Hz` (NEWTON.CPP, 0x800a2d04): an active, fully simulated object's
/// rotation over a 16th of its angular velocity.
pub fn qd_update_rot_32hz(c: &mut Cpu, obj: NewtonObj) {
    if obj.active(&mut c.m) != 0 && obj.sim_optz(&mut c.m) == 0 {
        qd_update_rot(c, obj, 16, 8192, 64, true);
    }
}

pub fn abi_qd_update_rot_32hz(c: &mut Cpu) {
    qd_update_rot_32hz(c, NewtonObj(c.r[4]));
}

/// `a` x `b` (fixed point).
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        fixedmult(a.y, b.z).wrapping_sub(fixedmult(a.z, b.y)),
        fixedmult(a.z, b.x).wrapping_sub(fixedmult(a.x, b.z)),
        fixedmult(a.x, b.y).wrapping_sub(fixedmult(a.y, b.x)),
    )
}

/// `Newton_CalculateGroundShadowMatrix` (NEWTON.CPP, 0x800a2e24): the frame the object's
/// shadow is drawn in: up along the ground's `normal`, turned to the object's heading. A
/// nearly upright object (`up`, its frame's height, over 0.9) not flagged 4 takes its own
/// frame. Otherwise right is found from the object's forward axis while that is not
/// along the normal (the object leaning past 45 degrees), else forward from its right axis.
pub fn calculate_ground_shadow_matrix(c: &mut Cpu, obj: NewtonObj, normal: Coord, up: i32) {
    abi::frame(c, 32, |c, _| {
        let shadow = obj.shadow_mat();
        let (right, ground, ahead) = (Coord(shadow.0), Coord(shadow.0 + 12), Coord(shadow.0 + 24));
        let n = normal.get(&mut c.m);
        let car = obj_car(obj);
        if car.car_flags(&mut c.m) & 4 == 0 && 0xE666 < up {
            let o = obj.orient_mat().get(&mut c.m);
            shadow.set(&mut c.m, &o);
            ground.set(&mut c.m, n);
            return;
        }
        ground.set(&mut c.m, n);
        let o = obj.orient_mat().get(&mut c.m).m;
        // (the original tests the dot product as a double, against 0.5)
        let along = up <= 32767 && {
            let d = mul_8_8(o[0], n.x).wrapping_add(mul_8_8(o[1], n.y)).wrapping_add(mul_8_8(o[2], n.z));
            d.wrapping_abs() > 0
        };
        if along {
            right.set(&mut c.m, cross(n, Vec3::new(o[6], o[7], o[8])));
            normalize_short_vector(c, right);
            let r = right.get(&mut c.m);
            ahead.set(&mut c.m, cross(r, n));
            normalize_short_vector(c, ahead);
        } else {
            ahead.set(&mut c.m, cross(Vec3::new(o[0], o[1], o[2]), n));
            normalize_short_vector(c, ahead);
            let f = ahead.get(&mut c.m);
            right.set(&mut c.m, cross(n, f));
            normalize_short_vector(c, right);
        }
    });
}

pub fn abi_calculate_ground_shadow_matrix(c: &mut Cpu) {
    calculate_ground_shadow_matrix(c, NewtonObj(c.r[4]), Coord(c.r[5]), c.r[6] as i32);
}

/// `Newton_CalcRealShadowCoordinates` (NEWTON.CPP, 0x800a3198): the corners of a car's
/// shadow (its footprint and a margin, wider at the front for car type 20), on the ground
/// under it (below its underside on the road while upright, else at the ground's height),
/// each at its wheel's height carried on by the car's fall since it was last updated, at
/// `time`.
pub fn calc_real_shadow_coordinates(c: &mut Cpu, car: Car, time: i32) {
    abi::frame(c, 144, |c, sp| {
        let n = car.n();
        let at = |off: u32| Coord(sp + off);
        let since = time.wrapping_sub(n.last_updated(&mut c.m));
        let dim = n.dimension().get(&mut c.m);
        at(112).set(&mut c.m, dim);
        let half = Vec3::new(dim.x.wrapping_add(3276), dim.y, dim.z.wrapping_add(3276));
        at(112).set_x(&mut c.m, half.x);
        at(112).set_z(&mut c.m, half.z);
        let o = n.orient_mat().get(&mut c.m).m;
        let along = |k: i32, row: usize| Vec3::new(fixedmult(k, o[3 * row]), fixedmult(k, o[3 * row + 1]), fixedmult(k, o[3 * row + 2]));
        let ahead = along(half.z, 2);
        at(16).set(&mut c.m, ahead);
        let across = along(half.x, 0);
        at(32).set(&mut c.m, across);
        let front = if car.render().current_car_type(&mut c.m) == 20 {
            along((half.x.wrapping_mul(3) << 6) >> 8, 0)
        } else {
            across
        };
        at(48).set(&mut c.m, front);
        let centre = if 0xE666 < n.orientation_to_ground().y(&mut c.m) {
            let down = half.y.wrapping_neg().wrapping_sub(n.obj_altitude(&mut c.m));
            let r = n.road_matrix().get(&mut c.m).m;
            let off = Vec3::new(fixedmult(down, r[3]), fixedmult(down, r[4]), fixedmult(down, r[5]));
            at(96).set(&mut c.m, off);
            add(n.position().get(&mut c.m), off)
        } else {
            let mut p = n.position().get(&mut c.m);
            p.y = n.ground_elevation(&mut c.m);
            p
        };
        at(64).set(&mut c.m, centre);
        let fore = add(centre, ahead);
        at(80).set(&mut c.m, fore);
        n.shadow_coord(0).set(&mut c.m, sub(fore, front));
        n.shadow_coord(1).set(&mut c.m, add(fore, front));
        let aft = sub(centre, ahead);
        at(80).set(&mut c.m, aft);
        n.shadow_coord(2).set(&mut c.m, sub(aft, across));
        n.shadow_coord(3).set(&mut c.m, add(aft, across));
        for i in 0..4 {
            let y = car.wheel(i).current_pos().y(&mut c.m);
            let fall = (n.linear_vel().y(&mut c.m) / 64).wrapping_mul(since);
            n.shadow_coord(i).set_y(&mut c.m, y.wrapping_add(fall));
        }
    });
}

pub fn abi_calc_real_shadow_coordinates(c: &mut Cpu) {
    calc_real_shadow_coordinates(c, Car(c.r[4]), c.r[5] as i32);
}

/// Gravity per tick (before the object's `gravityMult`).
const GRAVITY: i32 = 0x4800;

/// `Newton_ApplyTheLawOfGravity` (NEWTON.CPP, 0x800a41d8): an active object's fall and its
/// meeting with the ground, for the ticks due to it (asked of 0x800b6e90, still recompiled).
/// A car not simulated sits on the road. Otherwise the ground under its wheels gives its
/// tilt, its shadow's frame and the ground's rise (its velocity); gravity pulls it down
/// (twice while in the air, which counts its flight time). Landing: tilted, it strikes the
/// ground as a plane (`Collide_TestWithPlane`), a hard enough blow counting
/// at least 20 (and landing on its side, it smokes); upright, the suspensions throw it up,
/// and the blow (twice the change in its fall) can damage it and shake the pad. On the ground it skids slower over a steep rise
/// and follows the ground's rise (less of it at low speed); in a tunnel it can't fly up
/// into the roof.
pub fn apply_the_law_of_gravity(c: &mut Cpu, obj: NewtonObj) {
    abi::frame(c, 128, |c, sp| {
        if obj.active(&mut c.m) == 0 {
            return;
        }
        let car = obj_car(obj);
        let (rate, ticks) = (sp + 80, sp + 84);
        let keep = car.force_no_sim_optz(&mut c.m) as u32;
        let (dist, id) = (obj.dist_to_player(&mut c.m) as u32, obj.obj_id(&mut c.m) as u32);
        let due = abi::call(c, nfs4rc::f_800b6e90, &[1, 3, dist, id, rate, ticks, sp + 88, keep]);
        if due == 0 {
            return;
        }
        let normal = Coord(sp + 32);
        if 2 <= obj.sim_optz(&mut c.m) {
            obj.linear_vel().set_y(&mut c.m, 0);
            obj.set_flight_time(&mut c.m, 0);
            let h = find_ground_elevation_and_normal_fast(c, car, normal);
            let y = h.wrapping_add(obj.dimension().y(&mut c.m));
            obj.position().set_y(&mut c.m, y);
            return;
        }
        let h = find_ground_elevation_and_normal(c, car, normal);
        let ticks_n = c.m.r32(ticks) as i32;
        let n = normal.get(&mut c.m);
        let shadow_up = Coord(sp + 48);
        shadow_up.set(&mut c.m, n);
        let was = obj.ground_elevation(&mut c.m);
        obj.set_ground_elevation(&mut c.m, h);
        let rise = h.wrapping_sub(was).wrapping_mul(c.m.r32(rate) as i32);
        let alt = obj.obj_altitude(&mut c.m);
        let pull = |c: &mut Cpu| {
            let g = fixedmult(ticks_n.wrapping_mul(GRAVITY), obj.gravity_mult(&mut c.m));
            obj.linear_vel().update_y(&mut c.m, |v| v.wrapping_sub(g));
        };
        pull(c);
        let o = obj.orient_mat().get(&mut c.m).m;
        let upright = fixedmult(o[3], n.x).wrapping_add(fixedmult(o[4], n.y)).wrapping_add(fixedmult(o[5], n.z));
        obj.orientation_to_ground().set_y(&mut c.m, upright);
        calculate_ground_shadow_matrix(c, obj, shadow_up, upright);
        let v = obj.linear_vel().get(&mut c.m);
        obj.set_speed_xz(&mut c.m, crate::mathnfs::better_dist(v.x.wrapping_abs(), v.z.wrapping_abs()));
        let modifier = obj.road_gravity_modifier(&mut c.m);
        let ground_vel = obj.ground_vel(&mut c.m);
        obj.set_road_gravity_modifier(&mut c.m, modifier.wrapping_mul(7).wrapping_add(rise.wrapping_sub(ground_vel)) / 8);
        obj.set_ground_vel(&mut c.m, rise);
        let mut alt = alt;
        if 13107 <= alt {
            // in the air
            obj.set_road_gravity_modifier(&mut c.m, 0);
            obj.update_flight_time(&mut c.m, |t| t.wrapping_add(ticks_n as u16));
            pull(c);
        } else {
            if obj.flight_time(&mut c.m) == 0 {
                check_for_spike_belts(c, car);
            } else {
                // landing
                if car.car_flags(&mut c.m) & 4 != 0 {
                    crate::physics::fix_engine_rpm(c, car);
                }
                let change = rise.wrapping_sub(obj.linear_vel().y(&mut c.m));
                if obj.orientation_to_ground().y(&mut c.m) <= 0xB333 {
                    let at = Coord(sp + 64);
                    let p = obj.road_center_point().get(&mut c.m);
                    at.set(&mut c.m, p);
                    at.set_y(&mut c.m, p.y.wrapping_sub(6553));
                    crate::collide::test_with_plane(c, obj, normal, at);
                    let hit = obj.collision();
                    let blow = hit.impulse(&mut c.m);
                    if 0x5_0000 < blow {
                        hit.set_impulse(&mut c.m, blow.max(0x14_0000));
                    }
                    if obj.orientation_to_ground().y(&mut c.m) < 13107 {
                        car.collision().set_smoking(&mut c.m, 1);
                    }
                } else {
                    let push = (0..4).fold(0i32, |s, i| s.wrapping_add(car.wheel(i).wheel_acc(&mut c.m)));
                    if push > 0 {
                        obj.linear_vel().set_y(&mut c.m, rise.wrapping_add(push.wrapping_mul(3) / 4));
                        let hit = obj.collision();
                        hit.set_impulse(&mut c.m, change << 1);
                        obj.set_flight_time(&mut c.m, 0);
                        obj.position().update_y(&mut c.m, |y| y.wrapping_sub(alt));
                        alt = 3276;
                        obj.set_obj_altitude(&mut c.m, alt);
                        let blow = hit.impulse(&mut c.m);
                        if 0x14_0000 < blow {
                            hit.set_sfx_type(&mut c.m, 0x1_0000);
                            hit.set_other_obj(&mut c.m, NewtonObj(0));
                            let p = obj.position().get(&mut c.m);
                            hit.collision_point().set(&mut c.m, p);
                            add_damage_zone(c, car, blow, 9, 0);
                        }
                        if abi::call(c, nfs4rc::f_800cbeb8, &[car.0]) != 0 {
                            let blow = hit.impulse(&mut c.m);
                            abi::call(c, nfs4rc::f_800cbf64, &[blow as u32]);
                        }
                    }
                }
                limit_angular_velocity(c, obj);
            }
            if alt < 2621 {
                // on the ground
                obj.set_obj_altitude(&mut c.m, 0);
                obj.position().update_y(&mut c.m, |y| y.wrapping_sub(alt));
                if car.car_flags(&mut c.m) & 0x400 == 0 && 0xE666 < obj.orientation_to_ground().y(&mut c.m) {
                    obj.set_flight_time(&mut c.m, 0);
                }
                if 0x5_FFFF < rise {
                    let k = if 0xC_0000 < rise { 0xFFBE } else { 0xFFDF };
                    obj.linear_vel().update_x(&mut c.m, |v| fixedmult(v, k));
                    obj.linear_vel().update_z(&mut c.m, |v| fixedmult(v, k));
                }
                let speed = obj.speed_xz(&mut c.m);
                let follow = if speed <= 0x4_FFFF { rise / 4 } else if speed <= 0x9_FFFF { rise / 2 } else { rise };
                obj.linear_vel().set_y(&mut c.m, follow);
            }
        }
        let now = SIM_COUNTER.get(&mut c.m);
        obj.set_last_updated(&mut c.m, now);
        let tunnel = abi::call(c, nfs4rc::f_800809f8, &[obj.sim_road_info().0]);
        if tunnel != 0 {
            let vy = obj.linear_vel().y(&mut c.m);
            if vy > 0 && 0x8_0000 < obj.position().y(&mut c.m).wrapping_sub(obj.road_center_point().y(&mut c.m)) {
                obj.linear_vel().set_y(&mut c.m, vy.wrapping_neg());
            }
        }
    });
}

pub fn abi_apply_the_law_of_gravity(c: &mut Cpu) {
    apply_the_law_of_gravity(c, NewtonObj(c.r[4]));
}

/// `testSimRoadInfo` and `newtestSimRoadInfo`: the road positions
/// `Newton_TestForUndrivableSurfaces` searches with.
const TEST_ROAD: BWorldSmPos = BWorldSmPos(0x8011_53EC);
const NEW_TEST_ROAD: BWorldSmPos = BWorldSmPos(0x8011_5470);

fn copy_road_pos(m: &mut Mem, from: BWorldSmPos, to: BWorldSmPos) {
    for k in 0..BWorldSmPos::SIZE / 4 {
        let v = m.r32(from.0 + 4 * k);
        m.w32(to.0 + 4 * k, v);
    }
}

/// The damage zone a wheel's side of the car takes in a wall.
const WHEEL_ZONES: [i32; 4] = [0, 2, 6, 4];

/// `Newton_TestForUndrivableSurfaces` (NEWTON.CPP, 0x800a3990): whether a wheel (a little
/// ahead along the car's velocity when fast) is off the road's edge or on a quad it
/// can't drive on; the first that is makes the car bounce off a wall facing it. The wall's
/// normal comes from the road's edge, or the quad's edges the wheel crossed (checked by
/// stepping over each in turn), or else points from the quad's middle back to the road's
/// centre (or the car); then the car bounces, its collision is handled (and the AI told)
/// and a hard enough blow damages its side.
pub fn test_for_undrivable_surfaces(c: &mut Cpu, obj: NewtonObj) {
    abi::frame(c, 176, |c, sp| {
        let at = |off: u32| Coord(sp + off);
        let (wall, centre, ahead, wheel_pt, step, quad_mid, corner) = (at(16), at(32), at(64), at(80), at(96), at(48), at(112));
        let car = obj_car(obj);
        let road = obj.sim_road_info();
        wall.set(&mut c.m, Vec3::default());
        c.m.w32(sp + 128, 0);
        let rc = obj.road_center_point().get(&mut c.m);
        centre.set(&mut c.m, rc);
        ahead.set(&mut c.m, Vec3::default());
        copy_road_pos(&mut c.m, road, TEST_ROAD);
        if 0x28_0000 < obj.speed_xz(&mut c.m) {
            let v = obj.linear_vel().get(&mut c.m);
            let lead = |x: i32| x.wrapping_mul(5) / 256;
            ahead.set(&mut c.m, Vec3::new(lead(v.x), lead(v.y), lead(v.z)));
        }
        // the average of the test position's quad corners (or the slice's centre)
        let quad_middle = |c: &mut Cpu| {
            quad_mid.set(&mut c.m, Vec3::default());
            for i in 0..4 {
                let p = if TEST_ROAD.sim_quad(&mut c.m).0 != 0 {
                    TEST_ROAD.quad_pts(i).get(&mut c.m)
                } else {
                    let s = TEST_ROAD.slice(&mut c.m) as i32;
                    center(&mut c.m, s)
                };
                corner.set(&mut c.m, p);
                let sum = add(quad_mid.get(&mut c.m), p);
                quad_mid.set(&mut c.m, sum);
            }
            let s = quad_mid.get(&mut c.m);
            let mid = Vec3::new(s.x / 4, s.y / 4, s.z / 4);
            quad_mid.set(&mut c.m, mid);
            mid
        };
        // a step from the wheel along the wall's normal so far, onto a fresh copy of the test
        // position: whether the wheel is still on a bad quad there
        let step_still_bad = |c: &mut Cpu, wheel: i32| {
            let (p, w) = (step.get(&mut c.m), wall.get(&mut c.m));
            step.set(&mut c.m, add(p, w));
            find_closest_quad_rez(c, step, NEW_TEST_ROAD, 1);
            check_for_bad_quad(c, car, NEW_TEST_ROAD, wheel)
        };
        for i in 0..4u32 {
            let wheel = car.wheel(i);
            let p = add(wheel.current_pos().get(&mut c.m), ahead.get(&mut c.m));
            wheel_pt.set(&mut c.m, p);
            find_closest_quad_rez(c, wheel_pt, TEST_ROAD, 1);
            let off = TEST_ROAD.off_edge(&mut c.m) as i32;
            if off != 0 {
                generate_vector(c, off, wall, road);
                wall.set_y(&mut c.m, 0);
                normalize_short_vector(c, wall);
            } else {
                if !check_for_bad_quad(c, car, TEST_ROAD, i as i32) {
                    continue;
                }
                copy_road_pos(&mut c.m, TEST_ROAD, NEW_TEST_ROAD);
                step.set(&mut c.m, p);
                let edges = crate::bworldsm::find_edge_off(c, wheel_pt, road, TEST_ROAD, sp + 128);
                match edges {
                    1 | 2 | 4 | 8 => generate_vector(c, edges, wall, road),
                    0 | 12 => {
                        let v = obj.linear_vel().get(&mut c.m);
                        wall.set(&mut c.m, Vec3::new(v.x.wrapping_neg(), v.y.wrapping_neg(), v.z.wrapping_neg()));
                    }
                    _ => {}
                }
                if edges & 3 != 0 {
                    if edges & 1 != 0 {
                        generate_vector(c, 1, wall, road);
                    } else if edges & 2 != 0 {
                        generate_vector(c, 2, wall, road);
                    }
                    if step_still_bad(c, i as i32) {
                        let to_mid = if edges & 0xC != 0 {
                            if edges & 4 != 0 {
                                generate_vector(c, 4, wall, road);
                            } else if edges & 8 != 0 {
                                generate_vector(c, 8, wall, road);
                            }
                            copy_road_pos(&mut c.m, TEST_ROAD, NEW_TEST_ROAD);
                            step.set(&mut c.m, p);
                            // back towards the road's centre, past both edges
                            step_still_bad(c, i as i32).then_some(rc)
                        } else {
                            // back towards the car
                            Some(obj.position().get(&mut c.m))
                        };
                        if let Some(to) = to_mid {
                            let mid = quad_middle(c);
                            wall.set(&mut c.m, Vec3::new(to.x.wrapping_sub(mid.x), 0, to.z.wrapping_sub(mid.z)));
                        }
                    }
                }
                wall.set_y(&mut c.m, 0);
                normalize_short_vector(c, wall);
            }
            // a wall: bounce off it
            let w = wall.get(&mut c.m);
            do_post_barrier_collision_handling(c, obj, w);
            let p = wheel.current_pos().get(&mut c.m);
            obj.collision().collision_point().set(&mut c.m, p);
            abi::call(c, nfs4rc::f_8006d120, &[car.0]);
            if car.car_flags(&mut c.m) & 4 != 0 {
                crate::physics::fix_engine_rpm(c, car);
            }
            let blow = obj.collision().impulse(&mut c.m);
            if 0xA_0000 < blow {
                add_damage_zone(c, car, blow, WHEEL_ZONES[i as usize], 1);
            }
            return;
        }
    });
}

pub fn abi_test_for_undrivable_surfaces(c: &mut Cpu) {
    test_for_undrivable_surfaces(c, NewtonObj(c.r[4]));
}
