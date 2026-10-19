//! COLLIDE.CPP: collisions between objects (cars, traffic, the fixed and infinitely heavy
//! objects), with the ground's plane, and the registry of what can collide.

use crate::abi;
use crate::globals::*;
use crate::fixed::{fixeddiv, fixedmult};
use crate::types::{Car, Coord, NewtonObj, TrkNewSlice};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

/// `InfiniteMassNewton`: the infinitely heavy objects (`BO_tNewtonObj[5]`).
const INFINITE_MASS_OBJS: u32 = 0x8011_13A8;
/// `Collide_gRegistry`: the objects that can collide (`BO_tNewtonObj *[16]`).
const REGISTRY: u32 = 0x8011_1EF4;
/// `Collide_gNumRegistered`.
const NUM_REGISTERED: I32 = I32(0x8013_DB28);

/// The fastest a collision leaves an object spinning (1.5 per tick about each axis).
const MAX_ANGULAR_VEL: i32 = 0x1_8000;

/// `Collide_LimitAngularVel` (COLLIDE.CPP, 0x8008e3a4): the object's spin held within
/// [`MAX_ANGULAR_VEL`] about each axis.
pub fn limit_angular_vel(c: &mut Cpu, obj: NewtonObj) {
    let w = obj.angular_vel();
    let limit = |v: i32| v.clamp(-MAX_ANGULAR_VEL, MAX_ANGULAR_VEL);
    w.update_x(&mut c.m, limit);
    w.update_y(&mut c.m, limit);
    w.update_z(&mut c.m, limit);
}

pub fn abi_limit_angular_vel(c: &mut Cpu) {
    limit_angular_vel(c, NewtonObj(c.r[4]));
}

/// `Collide_ClearCollisionRegistry` (COLLIDE.CPP, 0x80091fc4): the registry started again
/// with the infinitely heavy objects, each moved where its motion (`Object_*`, still
/// recompiled) puts it, not spinning, on the road (its slice found, and how far it is
/// across the road: the original measures it from the slice centre's x in all three axes).
pub fn clear_collision_registry(c: &mut Cpu) {
    abi::frame(c, 80, |c, sp| {
        NUM_REGISTERED.set(&mut c.m, 0);
        let mut i = 0u32;
        while (i as i32) < abi::call(c, nfs4rc::f_800a6b7c, &[]) as i32 {
            let obj = NewtonObj(INFINITE_MASS_OBJS + i * 0x240);
            abi::call(c, nfs4rc::f_800a6bb4, &[i, obj.position().0, obj.orient_mat().0, obj.linear_vel().0]);
            obj.angular_vel().set(&mut c.m, Vec3::default());
            c.m.w32(REGISTRY + 4 * i, obj.0);
            let n = NUM_REGISTERED.get(&mut c.m);
            NUM_REGISTERED.set(&mut c.m, n.wrapping_add(1));
            crate::bworldsm::find_closest_slice(c, obj.position(), obj.sim_road_info());
            let s = TrkNewSlice(BWORLDSM_SLICES.get(&mut c.m) as u32).at(obj.sim_road_info().slice(&mut c.m) as i32 as u32);
            let (p, x) = (obj.position().get(&mut c.m), s.center(&mut c.m, 0));
            let d = Vec3::new(p.x.wrapping_sub(x), p.y.wrapping_sub(x), p.z.wrapping_sub(x));
            Coord(sp + 16).set(&mut c.m, d);
            let r: [i32; 3] = std::array::from_fn(|k| (s.right(&mut c.m, k as u32) as i32) << 9);
            Coord(sp + 32).set(&mut c.m, r.into());
            let m88 = |a: i32, b: i32| (a / 256).wrapping_mul(b / 256);
            obj.set_x_rel_road_center(&mut c.m, m88(d.x, r[0]).wrapping_add(m88(d.y, r[1])).wrapping_add(m88(d.z, r[2])));
            i += 1;
        }
    });
}

pub fn abi_clear_collision_registry(c: &mut Cpu) {
    clear_collision_registry(c);
}

/// The collision test's state, kept in globals between its steps: the two objects, the
/// first's basis dotted with the second's (scaled; and the other way round), the
/// distance between them in the first's frame (`[3..6]`: in the second's), and what a
/// vertex test leaves (the point, the normal there and the point in the frame it was tested
/// in).
const OBJ0: U32 = U32(0x8013_DB30);
const OBJ1: U32 = U32(0x8013_DB34);
const P_NORMAL: U32 = U32(0x8013_DB38);
const P_P: U32 = U32(0x8013_DB3C);
const SCALED_BASIS_DOTS: I32s = I32s(0x8013_F340, 9);
const SCALED_BASIS_DOTS_OTHER: I32s = I32s(0x8013_F370, 9);
const BASIS_DOT_RELATIVE: I32s = I32s(0x8013_F3A0, 6);
const X_RANGE: I32 = I32(0x8013_F0A0);
const Y_RANGE: I32 = I32(0x8013_F0A4);
const Z_RANGE: I32 = I32(0x8013_F0A8);

/// The `CHECK_*VERTEX_WITH_DIRS*` macros (made functions): whether the corner of one object
/// in the directions `d` (each ±1) is inside the other's box (its position in the box's
/// frame left in `x/y/zRange`). If so the corner is left in `pP` and, for the centre
/// versions, its face's outward normal (the corner's object's axes along `d`) in `pNormal`
/// (turned inward for the other object's corner). `other`: the second object's corner in
/// the first's box, rather than the first's in the second's.
fn check_vertex_with_dirs(c: &mut Cpu, d: [i32; 3], other: bool, normal: bool) -> i32 {
    let m = &mut c.m;
    let (dots, rel, boxed, corner) = if other {
        (SCALED_BASIS_DOTS_OTHER, 3, OBJ1, OBJ0)
    } else {
        (SCALED_BASIS_DOTS, 0, OBJ0, OBJ1)
    };
    // the dots, row by row (the other object's: column by column)
    let dot = |m: &mut Mem, r: u32, k: u32| if other { dots.get(m, 3 * k + r) } else { dots.get(m, 3 * r + k) };
    let range: [i32; 3] = std::array::from_fn(|r| {
        let r = r as u32;
        let s = d[0].wrapping_mul(dot(m, r, 0)).wrapping_add(d[1].wrapping_mul(dot(m, r, 1))).wrapping_add(d[2].wrapping_mul(dot(m, r, 2)));
        let off = BASIS_DOT_RELATIVE.get(m, rel + r);
        if other { s.wrapping_sub(off) } else { s.wrapping_add(off) }
    });
    X_RANGE.set(m, range[0]);
    Y_RANGE.set(m, range[1]);
    Z_RANGE.set(m, range[2]);
    let b = NewtonObj(boxed.get(m));
    let size = b.dimension().get(m);
    let inside = |v: i32, half: i32| v <= half && v >= half.wrapping_neg();
    if !inside(range[2], size.z) || !inside(range[0], size.x) || !inside(range[1], size.y) {
        return 0;
    }
    let o = NewtonObj(corner.get(m));
    let a = o.orient_mat().get(m).m;
    if normal {
        let n: [i32; 3] = std::array::from_fn(|k| {
            let v = d[0].wrapping_mul(a[k]).wrapping_add(d[1].wrapping_mul(a[3 + k])).wrapping_add(d[2].wrapping_mul(a[6 + k]));
            if other { v.wrapping_neg() } else { v }
        });
        Coord(P_NORMAL.get(m)).set(m, n.into());
    }
    let (p, dim) = (o.position().get(m), o.dimension().get(m));
    let m88 = |x: i32, y: i32| (x / 256).wrapping_mul(y / 256);
    let at: [i32; 3] = std::array::from_fn(|k| {
        [p.x, p.y, p.z][k]
            .wrapping_add(d[0].wrapping_mul(m88(a[k], dim.x)))
            .wrapping_add(d[1].wrapping_mul(m88(a[3 + k], dim.y)))
            .wrapping_add(d[2].wrapping_mul(m88(a[6 + k], dim.z)))
    });
    Coord(P_P.get(m)).set(m, at.into());
    1
}

/// `CHECK_CENTER_VERTEX_WITH_DIRS` (COLLIDE.CPP, 0x8008c560).
pub fn check_center_vertex_with_dirs(c: &mut Cpu, d: [i32; 3]) -> i32 {
    check_vertex_with_dirs(c, d, false, true)
}

/// `CHECK_CENTER_VERTEX_WITH_DIRS_OTHER` (COLLIDE.CPP, 0x8008c9e0).
pub fn check_center_vertex_with_dirs_other(c: &mut Cpu, d: [i32; 3]) -> i32 {
    check_vertex_with_dirs(c, d, true, true)
}

/// `CHECK_VERTEX_WITH_DIRS` (COLLIDE.CPP, 0x8008ce6c).
pub fn check_vertex_with_dirs_one(c: &mut Cpu, d: [i32; 3]) -> i32 {
    check_vertex_with_dirs(c, d, false, false)
}

/// `CHECK_VERTEX_WITH_DIRS_OTHER` (COLLIDE.CPP, 0x8008d234).
pub fn check_vertex_with_dirs_other(c: &mut Cpu, d: [i32; 3]) -> i32 {
    check_vertex_with_dirs(c, d, true, false)
}

fn dirs(c: &Cpu) -> [i32; 3] {
    [c.r[4] as i32, c.r[5] as i32, c.r[6] as i32]
}

pub fn abi_check_center_vertex_with_dirs(c: &mut Cpu) {
    c.r[2] = check_center_vertex_with_dirs(c, dirs(c)) as u32;
}

pub fn abi_check_center_vertex_with_dirs_other(c: &mut Cpu) {
    c.r[2] = check_center_vertex_with_dirs_other(c, dirs(c)) as u32;
}

pub fn abi_check_vertex_with_dirs(c: &mut Cpu) {
    c.r[2] = check_vertex_with_dirs_one(c, dirs(c)) as u32;
}

pub fn abi_check_vertex_with_dirs_other(c: &mut Cpu) {
    c.r[2] = check_vertex_with_dirs_other(c, dirs(c)) as u32;
}

/// `Collide_CheckAccuratePointRadiusCollision` (COLLIDE.CPP, 0x800921c4): whether a circle
/// of `radius` at `pt` (on the ground: its height ignored) touches the object's box.
pub fn check_accurate_point_radius_collision(c: &mut Cpu, obj: NewtonObj, pt: Coord, radius: i32) -> i32 {
    abi::frame(c, 56, |c, sp| {
        let (p, q) = (pt.get(&mut c.m), obj.position().get(&mut c.m));
        let d = Vec3::new(p.x.wrapping_sub(q.x), 0, p.z.wrapping_sub(q.z));
        Coord(sp + 16).set(&mut c.m, d);
        let o = obj.orient_mat().get(&mut c.m).m;
        let along = |row: usize| fixedmult(d.x, o[3 * row]).wrapping_add(fixedmult(d.y, o[3 * row + 1])).wrapping_add(fixedmult(d.z, o[3 * row + 2])).wrapping_abs();
        let dim = obj.dimension().get(&mut c.m);
        if dim.x.wrapping_add(radius) < along(0) {
            return 0;
        }
        (along(2) <= dim.z.wrapping_add(radius)) as i32
    })
}

pub fn abi_check_accurate_point_radius_collision(c: &mut Cpu) {
    c.r[2] = check_accurate_point_radius_collision(c, NewtonObj(c.r[4]), Coord(c.r[5]), c.r[6] as i32) as u32;
}

/// How many times two objects are pushed apart in a frame, at most.
const MAX_SEPARATIONS: i32 = 8;

/// `Collide_CheckForCollisionBetween` (COLLIDE.CPP, 0x80091e64): whether two objects
/// collide (a corner of one in the other: `Collide_TestObjectVertices`); if so they are
/// pushed apart (`Collide_DoObjectObjectCollision`) and kept off the barriers until they
/// are, at most [`MAX_SEPARATIONS`] times. Cars still caught (crashing at speed) after
/// that are damaged.
pub fn check_for_collision_between(c: &mut Cpu, a: NewtonObj, b: NewtonObj) -> i32 {
    abi::frame(c, 72, |c, sp| {
        let (at, normal) = (sp + 16, sp + 32);
        let touching = |c: &mut Cpu| abi::call(c, nfs4rc::f_80090c34, &[a.0, b.0, at, normal]) != 0;
        let separate = |c: &mut Cpu| abi::call(c, nfs4rc::f_8008f0c4, &[a.0, b.0, at, normal]) != 0;
        let off_barriers = |c: &mut Cpu| {
            crate::physics::test_for_barrier_collision(c, Car(a.0));
            crate::physics::test_for_barrier_collision(c, Car(b.0));
        };
        a.collision().set_impulse(&mut c.m, 0);
        b.collision().set_impulse(&mut c.m, 0);
        if !touching(c) {
            return 0;
        }
        separate(c);
        off_barriers(c);
        let mut left = MAX_SEPARATIONS;
        while touching(c) && left > 0 {
            left -= 1;
            if !separate(c) {
                break;
            }
            off_barriers(c);
            if left == 0 {
                for obj in [a, b] {
                    if Car(obj.0).crash(&mut c.m) != 0 && 0xF_0000 < obj.speed_xz(&mut c.m) {
                        crate::newton::add_damage_zone(c, Car(obj.0), 0x64_0000, 8, 2);
                    }
                }
            }
        }
        1
    })
}

pub fn abi_check_for_collision_between(c: &mut Cpu) {
    c.r[2] = check_for_collision_between(c, NewtonObj(c.r[4]), NewtonObj(c.r[5])) as u32;
}

/// `frictionLess`: the next fixed-object collision barely grips (set when the object is
/// tipped over; cleared by the collision).
const FRICTION_LESS: I32 = I32(0x8013_DB24);
/// The axis an object counts as tipped against (a constant vector in the executable).
const TIP_AXIS: u32 = 0x8005_61B0;

/// `a` x `b`, each product of 8.8 numbers.
fn cross_8_8(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        mul_8_8(a.y, b.z).wrapping_sub(mul_8_8(a.z, b.y)),
        mul_8_8(a.z, b.x).wrapping_sub(mul_8_8(a.x, b.z)),
        mul_8_8(a.x, b.y).wrapping_sub(mul_8_8(a.y, b.x)),
    )
}

fn scaled_by(k: i32, v: Vec3) -> Vec3 {
    Vec3::new(fixedmult(k, v.x), fixedmult(k, v.y), fixedmult(k, v.z))
}

fn plus(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x.wrapping_add(b.x), a.y.wrapping_add(b.y), a.z.wrapping_add(b.z))
}

/// `Collide_DoObjectFixedObjectCollision` (COLLIDE.CPP, 0x8008d5fc): an object's point at
/// `at`, moving at `vel`, hitting something immovable whose surface faces `normal`. The
/// impulse along the normal (from the closing speed of the point, through the object's mass
/// and moment of inertia, with a restitution of 0.4) is recorded as the collision; the
/// friction impulse acts against the point's sliding (half as hard, or barely when the object
/// is tipped over and slow), no more than stops it; both change its motion and its spin.
pub fn do_object_fixed_object_collision(c: &mut Cpu, obj: NewtonObj, at: Coord, vel: Coord, normal: Coord) {
    abi::frame(c, 192, |c, sp| {
        let slot = |off: u32| Coord(sp + off);
        let (n_at, dv_at, j_at, r_at, arm_at, slide_at, f_at, t_at, tip_at) =
            (slot(16), slot(32), slot(48), slot(64), slot(80), slot(96), slot(112), slot(128), slot(144));
        let v = vel.get(&mut c.m);
        slide_at.set(&mut c.m, v);
        let n = normal.get(&mut c.m);
        n_at.set(&mut c.m, n);
        let lv = obj.linear_vel().get(&mut c.m);
        let closing = dot_8_8(n, [lv.x, lv.y, lv.z]);
        let (p, q) = (at.get(&mut c.m), obj.position().get(&mut c.m));
        let r = Vec3::new(p.x.wrapping_sub(q.x), p.y.wrapping_sub(q.y), p.z.wrapping_sub(q.z));
        r_at.set(&mut c.m, r);
        let arm = cross_8_8(r, n);
        arm_at.set(&mut c.m, arm);
        let w = obj.angular_vel().get(&mut c.m);
        let spin = dot_8_8(w, [arm.x, arm.y, arm.z]);
        let mass_inv = obj.mass_inv(&mut c.m);
        let len2 = crate::mathnfs::vector_length2(c, arm_at);
        let turn = fixedmult(len2, obj.mo_inertia_inv(&mut c.m) << 1);
        let j = fixeddiv(closing.wrapping_add(spin).wrapping_neg(), (mass_inv / 4).wrapping_add(turn / 2));
        let j = fixedmult(j, 26214);
        let hit = obj.collision();
        hit.set_impulse(&mut c.m, closing.wrapping_abs() << 2);
        hit.set_other_obj(&mut c.m, NewtonObj(0));
        let surface = obj.drive_surface_type(&mut c.m);
        hit.set_sfx_type(&mut c.m, surface | 0x3_0000);
        hit.collision_point().set(&mut c.m, p);
        // the sliding direction, reversed (made a unit vector): the friction's
        let mut slide_len = 0;
        if v != Vec3::default() {
            let along = dot_8_8(v, [n.x, n.y, n.z]) / 256;
            let t = Vec3::new(v.x.wrapping_sub(along.wrapping_mul(n.x / 256)), v.y.wrapping_sub(along.wrapping_mul(n.y / 256)), v.z.wrapping_sub(along.wrapping_mul(n.z / 256)));
            slide_at.set(&mut c.m, t);
            slide_len = crate::fixed::fixedsqrt(&mut c.m, dot_8_8(t, [t.x, t.y, t.z]));
            let half = slide_len / 2;
            let k = if half != 0 { fixeddiv(0x8000, half).wrapping_neg() } else { -0x1_0000 };
            slide_at.set(&mut c.m, scaled_by(k, t));
        }
        if obj.speed_xz(&mut c.m).wrapping_abs() <= 0x9_FFFF {
            // tipped over (the axis far off the object's own) and slow: barely any grip
            let tip = Vec3::new(c.m.r32(TIP_AXIS) as i32, c.m.r32(TIP_AXIS + 4) as i32, c.m.r32(TIP_AXIS + 8) as i32);
            tip_at.set(&mut c.m, tip);
            let o = obj.orient_mat().get(&mut c.m).m;
            let along = |row: usize| dot_8_8(tip, [o[3 * row], o[3 * row + 1], o[3 * row + 2]]);
            let (across, up, ahead) = (along(0), along(1), along(2));
            if 0xF5C2 < across.wrapping_abs() || 0xF5C2 < ahead.wrapping_abs() || up < -0xF5C2 {
                FRICTION_LESS.set(&mut c.m, 1);
            }
        }
        let grip = if FRICTION_LESS.get(&mut c.m) == 1 { 655 } else { 0x8000 };
        let mut friction = fixedmult(grip, j);
        let t = slide_at.get(&mut c.m);
        let per_mass = |c: &mut Cpu, x: i32| fixedmult(fixedmult(friction, obj.mass_inv(&mut c.m) / 2), x);
        let fx = per_mass(c, t.x);
        let fy = per_mass(c, t.y);
        let fz = per_mass(c, t.z);
        f_at.set(&mut c.m, Vec3::new(fx, fy, fz));
        FRICTION_LESS.set(&mut c.m, 0);
        let spin_k = fixedmult(friction, obj.mo_inertia_inv(&mut c.m) << 1);
        // the friction's turn at the point: (t x r) x r, the second product in place
        let tr = cross_8_8(t, r);
        let x = mul_8_8(tr.y, r.z).wrapping_sub(mul_8_8(tr.z, r.y));
        let y = mul_8_8(tr.z, r.x).wrapping_sub(mul_8_8(x, r.z));
        let z = mul_8_8(x, r.y).wrapping_sub(mul_8_8(y, r.x));
        let turn = scaled_by(spin_k, Vec3::new(x, y, z));
        t_at.set(&mut c.m, turn);
        let f = plus(f_at.get(&mut c.m), turn);
        f_at.set(&mut c.m, f);
        // no more than stops the sliding
        let stop = dot_8_8(f, [t.x, t.y, t.z]);
        if slide_len < stop {
            friction = fixedmult(friction, fixeddiv(slide_len, stop));
        }
        let t = scaled_by(friction, t);
        slide_at.set(&mut c.m, t);
        if j <= 0 {
            return;
        }
        let impulse = plus(scaled_by(j, n), t);
        j_at.set(&mut c.m, impulse);
        let dv = scaled_by(obj.mass_inv(&mut c.m) / 2, impulse);
        dv_at.set(&mut c.m, dv);
        let lv = obj.linear_vel().get(&mut c.m);
        obj.linear_vel().set(&mut c.m, plus(lv, dv));
        let dw = cross_8_8(r, impulse);
        dv_at.set(&mut c.m, dw);
        let dw = scaled_by(obj.mo_inertia_inv(&mut c.m) << 1, dw);
        dv_at.set(&mut c.m, dw);
        let w = obj.angular_vel().get(&mut c.m);
        obj.angular_vel().set(&mut c.m, plus(w, dw));
    });
}

pub fn abi_do_object_fixed_object_collision(c: &mut Cpu) {
    do_object_fixed_object_collision(c, NewtonObj(c.r[4]), Coord(c.r[5]), Coord(c.r[6]), Coord(c.r[7]));
}

/// `Collide_gRaiseUp`: objects sunk into a plane are lifted out of it.
const RAISE_UP: I32 = I32(0x8013_DB20);

/// The angular velocity in the units the collision response works in (times 2 pi), and back.
const TO_RADIANS: i32 = 0x6_487E;
const FROM_RADIANS: i32 = 10430;

/// `a` . `b`, each product of 8.8 numbers.
fn dot_8_8(a: Vec3, b: [i32; 3]) -> i32 {
    mul_8_8(a.x, b[0]).wrapping_add(mul_8_8(a.y, b[1])).wrapping_add(mul_8_8(a.z, b[2]))
}

fn mul_8_8(a: i32, b: i32) -> i32 {
    (a / 256).wrapping_mul(b / 256)
}

/// `Collide_TestWithPlane` (COLLIDE.CPP, 0x8008e498): an object against the plane through
/// `point` with `normal` (straight up when it is zero). When its lowest corner is below
/// the plane it is lifted out (by half the depth, when objects are raised), the corner hits
/// the plane as a fixed object (`Collide_DoObjectFixedObjectCollision`)
/// if it is moving into it, and a lift slows its climb as gravity would. Its spin is then
/// held in bounds, and a hard enough blow damages it: on the roof, and on the corner the
/// plane faces.
pub fn test_with_plane(c: &mut Cpu, obj: NewtonObj, normal: Coord, point: Coord) {
    abi::frame(c, 144, |c, sp| {
        let w = obj.angular_vel();
        let scale = |c: &mut Cpu, k: i32| {
            w.update_x(&mut c.m, |v| fixedmult(k, v));
            w.update_y(&mut c.m, |v| fixedmult(k, v));
            w.update_z(&mut c.m, |v| fixedmult(k, v));
        };
        scale(c, TO_RADIANS);
        if normal.get(&mut c.m) == Vec3::default() {
            normal.set_y(&mut c.m, 0x1_0000);
        }
        let n = normal.get(&mut c.m);
        let o = obj.orient_mat().get(&mut c.m).m;
        let dim = obj.dimension().get(&mut c.m);
        let size = [dim.x, dim.y, dim.z];
        // how far each half extent reaches along the normal; the lowest corner is on the
        // side each reaches down
        let reach: [i32; 3] = std::array::from_fn(|k| mul_8_8(dot_8_8(n, [o[3 * k], o[3 * k + 1], o[3 * k + 2]]), size[k]));
        Coord(sp + 16).set(&mut c.m, reach.into());
        let side: [i32; 3] = reach.map(|r| if r >= 0 { -1 } else { 1 });
        let p = obj.position().get(&mut c.m);
        let q = point.get(&mut c.m);
        let d = Vec3::new(p.x.wrapping_sub(q.x), p.y.wrapping_sub(q.y), p.z.wrapping_sub(q.z));
        Coord(sp + 32).set(&mut c.m, d);
        let height = side[0]
            .wrapping_mul(reach[0])
            .wrapping_add(side[1].wrapping_mul(reach[1]))
            .wrapping_add(side[2].wrapping_mul(reach[2]))
            .wrapping_add(dot_8_8(n, [d.x, d.y, d.z]));
        if height < 0 {
            let corner: [i32; 3] = std::array::from_fn(|i| {
                [p.x, p.y, p.z][i]
                    .wrapping_add(side[0].wrapping_mul(mul_8_8(o[i], size[0])))
                    .wrapping_add(side[1].wrapping_mul(mul_8_8(o[3 + i], size[1])))
                    .wrapping_add(side[2].wrapping_mul(mul_8_8(o[6 + i], size[2])))
            });
            let corner_at = Coord(sp + 96);
            corner_at.set(&mut c.m, corner.into());
            let depth = height.wrapping_neg().max(0);
            let raise = RAISE_UP.get(&mut c.m) != 0 && depth != 0;
            let lift = Coord(sp + 48);
            if raise {
                let half = depth / 2;
                let by = Vec3::new(fixedmult(half, n.x), fixedmult(half, n.y), fixedmult(half, n.z));
                lift.set(&mut c.m, by);
                let p = obj.position().get(&mut c.m);
                obj.position().set(&mut c.m, Vec3::new(p.x.wrapping_add(by.x), p.y.wrapping_add(by.y), p.z.wrapping_add(by.z)));
            }
            // the corner's velocity: the spin about the centre and the motion
            let p = obj.position().get(&mut c.m);
            let r = Vec3::new(corner[0].wrapping_sub(p.x), corner[1].wrapping_sub(p.y), corner[2].wrapping_sub(p.z));
            Coord(sp + 80).set(&mut c.m, r);
            let s = w.get(&mut c.m);
            let v = obj.linear_vel().get(&mut c.m);
            let vel = Vec3::new(
                mul_8_8(s.y, r.z).wrapping_sub(mul_8_8(s.z, r.y)).wrapping_add(v.x),
                mul_8_8(s.z, r.x).wrapping_sub(mul_8_8(s.x, r.z)).wrapping_add(v.y),
                mul_8_8(s.x, r.y).wrapping_sub(mul_8_8(s.y, r.x)).wrapping_add(v.z),
            );
            let vel_at = Coord(sp + 64);
            vel_at.set(&mut c.m, vel);
            if dot_8_8(n, [vel.x, vel.y, vel.z]) < 0 {
                do_object_fixed_object_collision(c, obj, corner_at, vel_at, normal);
            }
            let up = lift.y(&mut c.m);
            if raise && up > 0 {
                // climbing out of the plane costs the speed gravity would take
                let cost = fixedmult(0x9_CCCC, up) << 1;
                let vy = obj.linear_vel().y(&mut c.m);
                let v2 = fixedmult(vy, vy);
                let vy = if v2 < cost {
                    0
                } else {
                    let left = crate::fixed::fixedsqrt(&mut c.m, v2.wrapping_sub(cost));
                    if vy > 0 { left } else { left.wrapping_neg() }
                };
                obj.linear_vel().set_y(&mut c.m, vy);
            }
        }
        scale(c, FROM_RADIANS);
        limit_angular_vel(c, obj);
        let v = obj.linear_vel().get(&mut c.m);
        let into = fixedmult(n.x, v.x).wrapping_add(fixedmult(n.y, v.y)).wrapping_add(fixedmult(n.z, v.z));
        let blow = into.wrapping_abs() << 2;
        if 0xA_0000 < blow {
            let along = |row: usize| fixedmult(n.x, o[3 * row]).wrapping_add(fixedmult(n.y, o[3 * row + 1])).wrapping_add(fixedmult(n.z, o[3 * row + 2]));
            let (across, up, ahead) = (along(0), along(1), along(2));
            let car = Car(obj.0);
            let mut zone = 9;
            if up < -0x8000 {
                // on the roof
                zone = 8;
                crate::newton::add_damage_zone(c, car, blow, zone, 0);
            }
            let corner_zone = |if_right: i32, if_left: i32| {
                if across >= 6554 {
                    Some(if_right)
                } else if across < -6553 {
                    Some(if_left)
                } else {
                    None
                }
            };
            if ahead < -6553 {
                zone = corner_zone(0, 2).unwrap_or(zone);
            } else if ahead >= 6554 {
                zone = corner_zone(4, 6).unwrap_or(zone);
            }
            if zone < 8 {
                crate::newton::add_damage_zone(c, car, blow, zone, 0);
            }
        }
    });
}

pub fn abi_test_with_plane(c: &mut Cpu) {
    test_with_plane(c, NewtonObj(c.r[4]), Coord(c.r[5]), Coord(c.r[6]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, cpu_with, race_ram, ram_diff, report, run};

    /// The first two cars of the race.
    fn cars(ram: &[u8]) -> (NewtonObj, NewtonObj) {
        let at = |a: usize| NewtonObj(u32::from_le_bytes(ram[a..a + 4].try_into().unwrap()));
        (at(0x11_0CA0), at(0x11_0CA4))
    }

    /// The second car put against the first: up to 4 m away across the ground and half a
    /// metre up or down, turned any way about the vertical, both moving and spinning.
    fn against(c: &mut Cpu, a: NewtonObj, b: NewtonObj, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let p = a.position().get(m);
        let near = Vec3::new(p.x + pick(0x8_0000) - 0x4_0000, p.y + pick(0x1_0000) - 0x8000, p.z + pick(0x8_0000) - 0x4_0000);
        b.position().set(m, near);
        let o = a.orient_mat().get(m).m;
        let (s, k) = crate::fixed::fixedsincos(m, pick(0x1_0000));
        let turn = |x: i32, y: i32| fixedmult(k, x).wrapping_sub(fixedmult(s, y));
        let mut t = o;
        for i in 0..3 {
            t[i] = turn(o[i], o[6 + i]);
            t[6 + i] = fixedmult(s, o[i]).wrapping_add(fixedmult(k, o[6 + i]));
        }
        b.orient_mat().set(m, &crate::vec::Mat3 { m: t });
        for obj in [a, b] {
            obj.linear_vel().set(m, Vec3::new(pick(0x10_0000) - 0x8_0000, pick(0x4_0000) - 0x2_0000, pick(0x10_0000) - 0x8_0000));
            obj.angular_vel().set(m, Vec3::new(pick(0x4_0000) - 0x2_0000, pick(0x4_0000) - 0x2_0000, pick(0x4_0000) - 0x2_0000));
            obj.set_speed_xz(m, pick(0x20_0000));
            Car(obj.0).set_crash(m, pick(2));
        }
        let p = a.position().get(m);
        let normal = if pick(16) == 0 { Vec3::default() } else { Vec3::new(pick(0x1_6000) - 0xB000, 0xB000 + pick(0x5000), pick(0x1_6000) - 0xB000) };
        Coord(PLANE_NORMAL).set(m, normal);
        Coord(PLANE_POINT).set(m, Vec3::new(p.x + pick(0x4_0000) - 0x2_0000, p.y + pick(0x2_0000) - 0x1_8000, p.z + pick(0x4_0000) - 0x2_0000));
        RAISE_UP.set(m, pick(2));
    }

    /// Where the plane tests keep their plane.
    const PLANE_NORMAL: u32 = crate::test_util::SCRATCH;
    const PLANE_POINT: u32 = crate::test_util::SCRATCH + 12;
    const HIT_VEL: u32 = crate::test_util::SCRATCH + 24;

    /// `port` against the recompiled `original` (`args` its arguments) on the race's RAM
    /// with the cars put against each other; the port runs with the decompiled code routed
    /// (so the recompiled code it calls uses the decompiled helpers), the original without.
    /// `more` sets up the case further; `port` returns what it returns (None when void).
    fn against_original(name: &str, cases: u32, seed: u64, more: fn(&mut Cpu, NewtonObj, &mut Rng), original: fn(&mut Cpu), args: fn(NewtonObj, NewtonObj) -> Vec<u32>, port: fn(&mut Cpu, NewtonObj, NewtonObj) -> Option<u32>) {
        let Some(ram) = race_ram() else { return };
        let (a, b) = cars(&ram);
        let mut rng = Rng(seed);
        let mut c = cpu_with(&ram);
        let plain = c.decomp;
        let (mut mismatches, mut hits, mut busy) = (0, 0, 0);
        for i in 0..cases {
            c.m.ram.copy_from_slice(&ram);
            against(&mut c, a, b, &mut rng);
            more(&mut c, a, &mut rng);
            let state = c.m.ram.clone();
            c.decomp = plain;
            let want_v = run(&mut c, original, &args(a, b));
            let want = c.m.ram.clone();
            c.m.ram.copy_from_slice(&state);
            c.decomp = crate::decomp;
            let got_v = port(&mut c, a, b);
            hits += (want_v != 0) as u32;
            // more changed than the spin (scaled and back) and the stack
            busy += (ram_diff(&state, &want).len() > 3) as u32;
            let diff = ram_diff(&want, &c.m.ram);
            if !diff.is_empty() || got_v.is_some_and(|v| v != want_v) {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("{name} case {i}: returned {got_v:x?}, the original {want_v:#x}");
                    report(name, i, &want, &c.m.ram, &diff);
                }
            }
        }
        eprintln!("{name}: {hits} of {cases} cases returned nonzero, {busy} changed more than 3 words");
        assert_eq!(mismatches, 0, "{name} differs from the original");
    }

    #[test]
    fn collision_between_fuzz() {
        against_original("Collide_CheckForCollisionBetween", 20_000, 0xC011, |_, _, _| {}, nfs4rc::f_80091e64, |a, b| vec![a.0, b.0], |c, a, b| Some(check_for_collision_between(c, a, b) as u32));
    }

    #[test]
    fn point_radius_fuzz() {
        against_original("Collide_CheckAccuratePointRadiusCollision", 20_000, 0x9AD1, |_, _, _| {}, nfs4rc::f_800921c4, |a, b| vec![a.0, b.position().0, 0x1_8000], |c, a, b| Some(check_accurate_point_radius_collision(c, a, b.position(), 0x1_8000) as u32));
        against_original("Collide_LimitAngularVel", 2_000, 0x11A, |_, _, _| {}, nfs4rc::f_8008e3a4, |a, _| vec![a.0], |c, a, _| {
            limit_angular_vel(c, a);
            None
        });
    }

    /// The first car against a plane through a point near its underside (tilted up to 45
    /// degrees, or zero), moving and spinning, raised or not.
    #[test]
    fn plane_fuzz() {
        against_original("Collide_TestWithPlane", 20_000, 0x91A4E, |_, _, _| {}, nfs4rc::f_8008e498, |a, _| vec![a.0, PLANE_NORMAL, PLANE_POINT], |c, a, _| {
            test_with_plane(c, a, Coord(PLANE_NORMAL), Coord(PLANE_POINT));
            None
        });
    }

    /// A point of the first car (any orientation: sometimes on its side or roof) hitting a
    /// fixed object, the point moving any way.
    #[test]
    fn fixed_object_fuzz() {
        let tipped = |c: &mut Cpu, a: NewtonObj, rng: &mut Rng| {
            let m = &mut c.m;
            let mut pick = |n: u32| (rng.next() % n as u64) as i32;
            Coord(HIT_VEL).set(m, Vec3::new(pick(0x10_0000) - 0x8_0000, pick(0x10_0000) - 0x8_0000, pick(0x10_0000) - 0x8_0000));
            FRICTION_LESS.set(m, pick(2));
            let o = a.orient_mat().get(m).m;
            let rows = [[0, 1, 2], [1, 0, 2], [2, 1, 0], [0, 2, 1]][pick(4) as usize];
            let sign = if pick(2) == 0 { 1 } else { -1 };
            let t: [i32; 9] = std::array::from_fn(|k| sign * o[3 * rows[k / 3] + k % 3]);
            a.orient_mat().set(m, &crate::vec::Mat3 { m: t });
        };
        against_original("Collide_DoObjectFixedObjectCollision", 20_000, 0xF1BED, tipped, nfs4rc::f_8008d5fc, |a, _| vec![a.0, PLANE_POINT, HIT_VEL, PLANE_NORMAL], |c, a, _| {
            do_object_fixed_object_collision(c, a, Coord(PLANE_POINT), Coord(HIT_VEL), Coord(PLANE_NORMAL));
            None
        });
    }
}
