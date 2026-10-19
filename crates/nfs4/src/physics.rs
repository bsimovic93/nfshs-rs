//! PHYSICS.CPP: the cars' handling (engine, gears, tyres, steering) on top of the rigid-body
//! motion of NEWTON.CPP.

use crate::abi;
use crate::fixed::{div, fixedatan, fixeddiv, fixedmult};
use crate::mathnfs::resolve_rotated_vector;
use crate::globals::*;
use crate::types::{Car, WheelAcc};
use ps1rt::Cpu;

/// Tyre types (`GameSetup_tCarData.TireType`).
const TIRE_RACING: i32 = 1;
const TIRE_RAIN: i32 = 2;

/// The length of (x, z) as the physics estimates it: the longer side and a quarter of the
/// shorter.
fn quick_length(x: i32, z: i32) -> i32 {
    let (x, z) = (x.wrapping_abs(), z.wrapping_abs());
    if z < x { x.wrapping_add(z >> 2) } else { z.wrapping_add(x >> 2) }
}

/// The tyre's grip loss when sliding: its tyre type's divisor, wet or dry.
fn grip_loss(c: &mut Cpu, car: Car) -> i32 {
    let tire = car.car_info(&mut c.m).tire_type(&mut c.m) as u32;
    if SLIPPERY.get(&mut c.m) != 0 { GRIP_LOSS_TABLE_WET.get(&mut c.m, tire) } else { GRIP_LOSS_TABLE.get(&mut c.m, tire) }
}

/// `Physics_GetTorque` (PHYSICS.CPP, 0x800a9f78): the engine's torque at step `rpm` of its
/// curve (0..40).
pub fn get_torque(c: &mut Cpu, car: Car, rpm: i32) -> i32 {
    let specs = car.specs(&mut c.m);
    specs.torque_curve(&mut c.m, rpm.clamp(0, 40) as u32)
}

pub fn abi_get_torque(c: &mut Cpu) {
    c.r[2] = get_torque(c, Car(c.r[4]), c.r[5] as i32) as u32;
}

/// `Physics_StopCar` (PHYSICS.CPP, 0x800ab36c): the car slows by 245/256 a frame; on its
/// side or roof (less than 0.2 upright) its spin slows too.
pub fn stop_car(c: &mut Cpu, car: Car) {
    let n = car.n();
    let vel = n.linear_vel();
    let slow = |v: i32| v.wrapping_mul(245) / 256;
    vel.update_x(&mut c.m, slow);
    vel.update_y(&mut c.m, slow);
    vel.update_z(&mut c.m, slow);
    if n.orientation_to_ground().y(&mut c.m) < 13107 {
        let spin = n.angular_vel();
        spin.update_y(&mut c.m, |v| (v / 256).wrapping_mul(250));
    }
}

pub fn abi_stop_car(c: &mut Cpu) {
    stop_car(c, Car(c.r[4]));
}

/// `Physics_CalculateRoadGripModifiers` (PHYSICS.CPP, 0x800ab49c): the grip of the road under
/// each wheel by its surface and the car's tyres (one class slipperier on a wet track),
/// averaged per axle and side, with the aerodynamic downforce at speed on the axles.
pub fn calculate_road_grip_modifiers(c: &mut Cpu, car: Car) {
    let (mut front, mut rear, mut left, mut right) = (0i32, 0i32, 0i32, 0i32);
    for i in 0..4 {
        let tire = car.car_info(&mut c.m).tire_type(&mut c.m);
        let surface = car.wheel(i).road_surface_type(&mut c.m) & 0xF;
        let mut class = ROAD_SURFACE_INDEX.get(&mut c.m, tire.wrapping_mul(20).wrapping_add(surface) as u32) as u32;
        if SLIPPERY.get(&mut c.m) != 0 {
            class += 1;
        }
        let grip = ROAD_SURFACE_FRICTION_COEFF.get(&mut c.m, class);
        // wheels 0, 1 at the front; 0, 2 on the left
        if i < 2 {
            front = front.wrapping_add(grip);
        } else {
            rear = rear.wrapping_add(grip);
        }
        if i == 0 || i == 2 {
            left = left.wrapping_add(grip);
        } else {
            right = right.wrapping_add(grip);
        }
    }
    let (front, rear) = (front >> 1, rear >> 1);
    FRONT_MULT.set(&mut c.m, front);
    REAR_MULT.set(&mut c.m, rear);
    LEFT_MULT.set(&mut c.m, left >> 1);
    RIGHT_MULT.set(&mut c.m, right >> 1);
    let slope = car.n().road_gravity_modifier(&mut c.m);
    ROAD_MULT.set(&mut c.m, (front.wrapping_add(rear) >> 1).wrapping_add(slope));
    // downforce above 5 m/s
    let speed = car.linear_vel_ch().z(&mut c.m);
    if speed > 0x5_0000 {
        let specs = car.specs(&mut c.m);
        let down = fixedmult(speed, specs.front_aero_down_force(&mut c.m));
        FRONT_MULT.update(&mut c.m, |v| v.wrapping_add(down));
        let down = fixedmult(car.linear_vel_ch().z(&mut c.m), specs.rear_aero_down_force(&mut c.m));
        REAR_MULT.update(&mut c.m, |v| v.wrapping_add(down));
    }
}

pub fn abi_calculate_road_grip_modifiers(c: &mut Cpu) {
    calculate_road_grip_modifiers(c, Car(c.r[4]));
}

/// `Physics_CalcWheelLockAcc` (PHYSICS.CPP, 0x800ac130): a locked wheel's acceleration: its
/// sliding velocity scaled by the grip left, and its skid.
pub fn calc_wheel_lock_acc(c: &mut Cpu, car: Car, w: WheelAcc) {
    let front = w.front_tire(&mut c.m) != 0;
    let skid = if front { car.front_skid(&mut c.m) } else { car.rear_skid(&mut c.m) };
    w.set_skid(&mut c.m, skid);
    let axle = if front { FRONT_MULT.get(&mut c.m) } else { REAR_MULT.get(&mut c.m) };
    let grip = (w.road_grip(&mut c.m) / 256).wrapping_mul(axle / 256);
    let cap = w.vel_cap();
    let (cap_x, cap_z) = (cap.x(&mut c.m), cap.z(&mut c.m));
    let slide = quick_length(cap_x, cap_z);
    let held = grip.wrapping_sub(div(grip, grip_loss(c, car)));
    if grip < slide {
        let skid = w.skid(&mut c.m);
        w.set_skid(&mut c.m, skid.wrapping_mul(3).wrapping_add(slide.wrapping_sub(grip)) >> 2);
    } else {
        w.set_skid(&mut c.m, 0);
    }
    let most = if car.car_info(&mut c.m).tire_type(&mut c.m) == TIRE_RAIN { 0x8_0000 } else { 0xA_0000 };
    let skid = w.skid(&mut c.m);
    w.set_skid(&mut c.m, if most < skid { most } else { skid });
    // the velocity scaled by grip / sliding speed (a slow slide keeps |cap.x| as the scale:
    // the original's register left over)
    let scale = if slide.wrapping_abs() < 257 { cap_x.wrapping_abs() } else { fixeddiv(held, slide) >> 8 };
    let acc = w.final_acc();
    acc.set_x(&mut c.m, cap_x.wrapping_mul(scale) >> 8);
    acc.set_z(&mut c.m, cap_z.wrapping_mul(scale) >> 8);
}

pub fn abi_calc_wheel_lock_acc(c: &mut Cpu) {
    calc_wheel_lock_acc(c, Car(c.r[4]), WheelAcc(c.r[5]));
}

/// `Physics_CalcTractionCircleAcc` (PHYSICS.CPP, 0x800ac32c): a rolling wheel's acceleration
/// held within its grip (the traction circle), the wheel spin it causes and its skid.
pub fn calc_traction_circle_acc(c: &mut Cpu, car: Car, w: WheelAcc) {
    let acc = w.final_acc();
    let len = quick_length(acc.x(&mut c.m), acc.z(&mut c.m));
    let grip = w.road_grip(&mut c.m);
    let front = w.front_tire(&mut c.m) != 0;
    if front {
        let skid = car.front_skid(&mut c.m);
        w.set_skid(&mut c.m, skid);
    } else {
        let skid = car.rear_skid(&mut c.m);
        w.set_skid(&mut c.m, skid);
        // the driven wheels spin under more throttle than they can put down
        let over = grip < acc.z(&mut c.m).wrapping_abs() && car.control().gas_level(&mut c.m) as u8 >= 129;
        let spin = (over || car.wheel_spin(&mut c.m) == 2) as i32;
        car.set_wheel_spin(&mut c.m, spin);
    }
    let loss = grip_loss(c, car);
    let info = car.car_info(&mut c.m);
    if grip < len {
        let over = len.wrapping_sub(grip);
        if info.traction(&mut c.m) != 0 && !front && car.slide(&mut c.m).wrapping_abs() < 9830 {
            // traction control: the acceleration held to the grip, without a skid (its
            // sideways part only from third gear up)
            let scale = fixeddiv(grip, len);
            w.set_skid(&mut c.m, 0);
            if car.control().gear(&mut c.m) as u8 >= 3 {
                acc.update_x(&mut c.m, |a| fixedmult(a, scale));
            }
            acc.update_z(&mut c.m, |a| fixedmult(a, scale));
        } else {
            // beyond its grip a tyre loses some: of the excess or of the grip, the smaller
            let lost = div(over, loss).min(div(grip, loss));
            let scale = fixeddiv(grip.wrapping_sub(lost), len);
            let skid = w.skid(&mut c.m);
            let skid = if info.tire_type(&mut c.m) == TIRE_RAIN { skid.wrapping_mul(15).wrapping_add(over) / 16 } else { skid.wrapping_mul(3).wrapping_add(over) / 4 };
            w.set_skid(&mut c.m, skid);
            acc.update_x(&mut c.m, |a| fixedmult(a, scale));
            acc.update_z(&mut c.m, |a| fixedmult(a, scale));
        }
    } else {
        w.set_skid(&mut c.m, 0);
    }
    if !front && car.wheel_spin(&mut c.m) == 1 && info.traction(&mut c.m) == 0 {
        let gear = car.control().gear(&mut c.m) as u8;
        w.set_skid(&mut c.m, if gear < 4 { len << 2 } else { len });
    }
    let most = match info.tire_type(&mut c.m) {
        TIRE_RACING => Some(0x8_0000),
        TIRE_RAIN => Some(0x4_0000),
        _ => None,
    };
    if let Some(most) = most {
        let skid = w.skid(&mut c.m);
        w.set_skid(&mut c.m, if most < skid { most } else { skid });
    }
    if info.traction(&mut c.m) != 0 {
        let skid = w.skid(&mut c.m);
        w.set_skid(&mut c.m, skid.wrapping_mul(3) / 4);
    }
}

pub fn abi_calc_traction_circle_acc(c: &mut Cpu) {
    calc_traction_circle_acc(c, Car(c.r[4]), WheelAcc(c.r[5]));
}

/// `Physics_CalculateTireForces` (PHYSICS.CPP, 0x800ac6d0): one wheel's force on the road.
/// The road's velocity under the wheel is turned into the wheel's frame (by its steering);
/// the drive or brake can't turn the wheel past its rolling speed; a wheel braked beyond its
/// grip (or the rear ones on the handbrake) locks, unless ABS holds it; otherwise the slip
/// angle gives a lateral force (with gravity's pull along the slope), and the traction circle
/// holds the total within the grip. The result is turned back into the car's frame.
pub fn calculate_tire_forces(c: &mut Cpu, car: Car, w: WheelAcc) {
    let grip = w.road_grip(&mut c.m);
    w.set_skid(&mut c.m, 0);
    let cap = w.vel_cap();
    let angle = w.steering_angle(&mut c.m);
    if angle != 0 {
        let (x, z) = (cap.x(&mut c.m), cap.z(&mut c.m));
        resolve_rotated_vector(c, x, z, angle, cap.x_addr(), cap.z_addr());
    }

    // drive and brake no faster than the wheel rolls (in reverse gear and first and second
    // only when the player's throttle is past a quarter): the wheel may lock
    let acc = w.acc(&mut c.m);
    let rolling = cap.z(&mut c.m);
    let full_throttle = GAS_RATIO.get(&mut c.m) >= 16385;
    let gear = car.control().gear(&mut c.m) as u8;
    let mut limited = false;
    if acc < 0 {
        if rolling < 0 && (!full_throttle || gear != 0) {
            w.set_acc(&mut c.m, acc.max(rolling));
            limited = true;
        }
    } else if acc > 0 && rolling > 0 && (!full_throttle || gear < 2) {
        w.set_acc(&mut c.m, acc.min(rolling));
        limited = true;
    }
    let specs = car.specs(&mut c.m);
    let acc = fixedmult(w.acc(&mut c.m), specs.lateral_grip_mult(&mut c.m));
    w.set_acc(&mut c.m, acc);

    let front = w.front_tire(&mut c.m) != 0;
    let handbrake = car.control().hand_brake(&mut c.m) != 0;
    let speed = car.linear_vel_ch().z(&mut c.m);
    if (limited && grip < acc.wrapping_abs()) || (handbrake && !front && 0x8000 < speed.wrapping_abs()) {
        // hard braking without ABS locks the wheels below 25 m/s, from 5 m/s up or with the
        // wheels spinning
        let info = car.car_info(&mut c.m);
        let locked = handbrake
            || (info.abs(&mut c.m) == 0
                && speed <= 0x19_0000
                && car.control().brake_level(&mut c.m) as u8 >= 236
                && (0x4_FFFF < speed.wrapping_abs() || car.wheel_spin(&mut c.m) != 0));
        if locked {
            let lock = car.wheel_lock(&mut c.m);
            car.set_wheel_lock(&mut c.m, lock.wrapping_add(if front { 1 } else { 2 }));
            calc_wheel_lock_acc(c, car, w);
            store_skid(c, car, w);
            return;
        }
        // the brake held to the grip; ABS pulses (skid sounds) below 25 m/s
        let acc = w.acc(&mut c.m);
        if grip < acc {
            w.set_acc(&mut c.m, grip);
        } else if acc < grip.wrapping_neg() {
            w.set_acc(&mut c.m, grip.wrapping_neg());
        }
        if info.abs(&mut c.m) != 0 && car.linear_vel_ch().z(&mut c.m) <= 0x18_FFFF {
            let pulse = SIM_COUNTER.get(&mut c.m) & 3 == 0;
            w.set_skid(&mut c.m, if pulse { 0x8_0000 } else { 0 });
        }
    }

    // the slip angle: of the road's velocity under the wheel against its heading
    let rolling = cap.z(&mut c.m);
    let slip = if rolling == 0 {
        0
    } else {
        let side = cap.x(&mut c.m);
        let a = fixedatan(&mut c.m, side, rolling.wrapping_neg() / 2);
        let a = if cap.z(&mut c.m) > 0 {
            let side = cap.x(&mut c.m);
            if side > 0 {
                0x8000 - a
            } else if side < 0 {
                -0x8000 - a
            } else {
                a
            }
        } else {
            a
        };
        a << 8
    };
    // the lateral force: grip by the slip angle, up to a limit, at most what stops the
    // sliding; the front by 1/12 of it up to 16, the rear by half of it from 1/2 to 2
    let acc = w.final_acc();
    let half_gravity = |c: &mut Cpu| GRAVITY_CH.x(&mut c.m) / 2;
    let toward = |lat: i32, side: i32| if side > 0 { lat.wrapping_abs().min(side) } else { lat.wrapping_abs().wrapping_neg().max(side) };
    if front {
        let lat = fixedmult(fixedmult(slip.wrapping_abs().min(0x10_0000), grip), 5461);
        let lat = if slip < 0 { lat.wrapping_neg() } else { lat };
        acc.set_x(&mut c.m, lat);
        let g = half_gravity(c);
        acc.set_x(&mut c.m, lat.wrapping_add(g));
        let (side, rolling) = (cap.x(&mut c.m), cap.z(&mut c.m));
        // (not when sliding fast)
        if side.wrapping_abs().wrapping_add(rolling.wrapping_abs()) <= 0x1F_FFFF {
            acc.set_x(&mut c.m, toward(lat, side));
        }
    } else {
        let lat = fixedmult(slip.wrapping_abs().clamp(0x8000, 0x2_0000), grip) / 2;
        let lat = if slip < 0 { lat.wrapping_neg() } else { lat };
        let lat = lat.wrapping_add(half_gravity(c));
        let side = cap.x(&mut c.m);
        acc.set_x(&mut c.m, toward(lat, side));
    }
    acc.set_y(&mut c.m, 0);
    let along = w.acc(&mut c.m);
    acc.set_z(&mut c.m, along);
    calc_traction_circle_acc(c, car, w);

    let angle = w.steering_angle(&mut c.m);
    if angle != 0 {
        let (x, z) = (acc.x(&mut c.m), acc.z(&mut c.m));
        resolve_rotated_vector(c, x, z, angle.wrapping_neg(), acc.x_addr(), acc.z_addr());
    }
    store_skid(c, car, w);
}

/// The wheel's skid kept on the car, per axle.
fn store_skid(c: &mut Cpu, car: Car, w: WheelAcc) {
    let skid = w.skid(&mut c.m);
    if w.front_tire(&mut c.m) != 0 {
        car.set_front_skid(&mut c.m, skid);
    } else {
        car.set_rear_skid(&mut c.m, skid);
    }
}

pub fn abi_calculate_tire_forces(c: &mut Cpu) {
    calculate_tire_forces(c, Car(c.r[4]), WheelAcc(c.r[5]));
}

/// `AIWorld_IsDriveableLane` (still recompiled): lane `lane` of slice `slice` can be driven.
fn is_driveable_lane(c: &mut Cpu, slice: i32, lane: i32) -> bool {
    abi::call(c, nfs4rc::f_80073c88, &[slice as u32, lane as u32]) != 0
}

/// `AIWorld_IsDriveableLaneInSliceRange` (still recompiled).
fn is_driveable_lane_in_slice_range(c: &mut Cpu, slice: i32, range: i32, side: i32, lane: i32) -> bool {
    abi::call(c, nfs4rc::f_80073bd0, &[slice as u32, range as u32, side as u32, lane as u32]) != 0
}

/// `Physics_CalculateRSControlDesiredPosition` (PHYSICS.CPP, 0x800acc38): where across the road
/// (16.16 metres from the centre) a car steered by road-side control wants to be, on the
/// side `RSControl` asks for (the other one when there is no lane there): the outermost
/// driveable lane within `range` slices of `slice`, by the slice's paved width.
pub fn calculate_rs_control_desired_position(c: &mut Cpu, car: Car, slice: i32, range: i32) -> i32 {
    let mut side = car.rs_control(&mut c.m).wrapping_mul(AITUNE_DRIVE_SIDE.get(&mut c.m));
    let here = car.n().sim_road_info().slice(&mut c.m) as i32;
    let road_position = car.road_position(&mut c.m);
    let lane = car.lane_index(&mut c.m);
    let blocked = if road_position < 0 && side > 0 {
        !is_driveable_lane(c, here, lane.wrapping_add(1))
    } else if road_position > 0 && side < 0 {
        !is_driveable_lane(c, here, lane.wrapping_sub(1))
    } else {
        false
    };
    if blocked {
        side = side.wrapping_neg();
    }
    let slices = BWORLDSM_SLICES.get(&mut c.m) as u32;
    let rs = car.rs_control(&mut c.m);
    if side > 0 {
        // lanes 7 (the centre) to 10 to the right
        let mut lane = 7;
        while !is_driveable_lane_in_slice_range(c, here, range, rs, lane) && lane < 10 {
            lane += 1;
        }
        let width = (crate::types::TrkNewSlice(slices).at(slice as u32).avg_paved_width_rt(&mut c.m) as i32) << 15;
        let out = lane - 7;
        let at = out.wrapping_mul(width).wrapping_add((width as u32 >> 1) as i32);
        if out > 0 { at.wrapping_add(0x1_8000) } else { at }
    } else {
        // lanes 6 to 3 to the left
        let mut lane = 6;
        while !is_driveable_lane_in_slice_range(c, here, range, rs, lane) && lane >= 4 {
            lane -= 1;
        }
        let width = (crate::types::TrkNewSlice(slices).at(slice as u32).avg_paved_width_lf(&mut c.m) as i32) << 15;
        let out = 6 - lane;
        let at = out.wrapping_mul(width).wrapping_add((width as u32 >> 1) as i32);
        if out > 0 { at.wrapping_add(0x1_8000).wrapping_neg() } else { at.wrapping_neg() }
    }
}

pub fn abi_calculate_rs_control_desired_position(c: &mut Cpu) {
    c.r[2] = calculate_rs_control_desired_position(c, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32) as u32;
}

/// The engine speed (rpm) at road speed `speed` in `gear`.
fn rpm_in_gear(c: &mut Cpu, car: Car, speed: i32, gear: i32) -> i32 {
    let ratio = car.specs(&mut c.m).vel_to_rpm_ratio(&mut c.m, gear as u32);
    fixedmult(speed, ratio) / 0x1_0000
}

/// Put the gearbox in `gear` (`down` when shifting down), the shift taking the car's shift
/// delay; `last` keeps the gear left.
fn shift_to(c: &mut Cpu, car: Car, gear: i32, down: bool, last: bool) {
    let control = car.control();
    let from = control.gear(&mut c.m);
    let delay = car.specs(&mut c.m).gear_shift_delay(&mut c.m) as i8;
    control.set_down_shifting(&mut c.m, down as i8);
    control.set_gear(&mut c.m, gear as i8);
    if last {
        control.set_last_gear(&mut c.m, from);
    }
    control.set_gear_shift_timer(&mut c.m, delay);
}

/// `Physics_AutoShift` (PHYSICS.CPP, 0x800aa6e4): the automatic gearbox. Up a gear past 5/6
/// of the redline (with 500 rpm to spare); down when the gear below would stay under it,
/// and off the throttle only when not sliding. Road-side control shifts at half the redline;
/// the top gear is left out when the car can't reach its top speed in it.
pub fn auto_shift(c: &mut Cpu, car: Car) {
    let specs = car.specs(&mut c.m);
    let redline = specs.redline(&mut c.m);
    let mut shift_rpm = redline - redline / 6;
    let gear = car.control().gear(&mut c.m) as u8 as i32;
    // gears 0 and 1 are reverse and neutral
    if gear < 2 {
        return;
    }
    if car.rs_control(&mut c.m) != 0 {
        shift_rpm = redline - redline / 2;
    }
    let gears = specs.num_gears(&mut c.m);
    let sliding = car.slide(&mut c.m).wrapping_abs() >= 6553;
    let top_reach = fixedmult(specs.vel_to_rpm_ratio_inv(&mut c.m, (gears - 2) as u32), redline << 16);
    let skip_top = (specs.max_speed(&mut c.m) < top_reach) as i32;
    let up = if gear < gears - (skip_top + 1) { gear + 1 } else { gear };
    let down = if gear >= 3 { gear - 1 } else { gear };
    let mut speed = car.n().speed_xz(&mut c.m);
    if car.linear_vel_ch().z(&mut c.m) < 0 {
        speed = speed.wrapping_neg();
    }
    let rpm = rpm_in_gear(c, car, speed, gear);
    let rpm_down = rpm_in_gear(c, car, speed, down);

    let pressing = 0x8000 < GAS_RATIO.get(&mut c.m);
    if pressing || specs.redline(&mut c.m) < rpm.wrapping_abs() {
        if shift_rpm + 500 < rpm && gear < gears - (skip_top + 1) {
            if up != gear {
                shift_to(c, car, up, false, true);
            }
            return;
        }
        if rpm_down < shift_rpm && down != gear {
            shift_to(c, car, down, true, true);
        }
    } else if rpm_down < shift_rpm && down >= 2 && !sliding && down != gear {
        shift_to(c, car, down, true, false);
    }
}

pub fn abi_auto_shift(c: &mut Cpu) {
    auto_shift(c, Car(c.r[4]));
}

/// `GameSetup_tCarData.Transmission`: the automatic gearbox.
const TRANSMISSION_AUTOMATIC: i32 = 1;
/// `GameSetup_tData.raceType`s whose start holds the human cars until both are ready.
const RACE_TYPE_HELD_STARTS: [i32; 2] = [1, 5];
/// `Car_tObj.carFlags`: the car is not ready to start.
const CAR_NOT_READY: i32 = 0x200;
/// Gears: reverse, neutral, first.
const GEAR_REVERSE: i32 = 0;
const GEAR_FIRST: i32 = 2;

/// `cur` moved toward `to` by at most `step`.
fn ramp(cur: i32, to: i32, step: i32) -> i32 {
    let d = to.wrapping_sub(cur);
    if d >= 0 {
        cur.wrapping_add(if d < step { d } else { step })
    } else {
        let d = d.wrapping_neg();
        cur.wrapping_sub(if d < step { d } else { step })
    }
}

/// A pedal (0..255) as a 16.16 ratio up to 1.
fn pedal_ratio(level: u8) -> i32 {
    (((level as i32 + 1) << 16) / 248).min(0x1_0000)
}

/// The race start holds the cars: in held-start races until no human car is still getting
/// ready, in the others always.
fn start_held(c: &mut Cpu) -> bool {
    if !RACE_TYPE_HELD_STARTS.contains(&GAME_SETUP.race_type(&mut c.m)) {
        return true;
    }
    let not_ready = |c: &mut Cpu, k: u32| {
        let car = Car(c.m.r32(HUMAN_RACE_CARS + 4 * k));
        car.car_flags(&mut c.m) & CAR_NOT_READY != 0
    };
    !(not_ready(c, 0) || (NUM_HUMAN_RACE_CARS.get(&mut c.m) == 2 && not_ready(c, 1)))
}

/// `Physics_RampCarControlValues` (PHYSICS.CPP, 0x800aa9a0): the driver's controls as the car
/// gets them. Throttle, brake and steering move toward what is asked (gradually when the
/// player chose ramped controls); gears change, the automatic gearbox engaging reverse when
/// the brake is held at a standstill (`hanno`) and picking the gear when leaving reverse or
/// neutral; a car that has finished brakes to a stop; the start holds the cars on the
/// brake. Last the pedals and steering as ratios for the player's car (`gGasRatio`...).
pub fn ramp_car_control_values(c: &mut Cpu, car: Car) {
    let control = car.control();
    if car.rs_control(&mut c.m) != 0 && (control.gear(&mut c.m) as u8) < 2 {
        control.set_desired_gear(&mut c.m, GEAR_FIRST as i8);
        control.set_gear(&mut c.m, GEAR_FIRST as i8);
    }
    let setup = car.car_info(&mut c.m);
    if car.stats().finish_type(&mut c.m) >= 2 {
        // finished: off the throttle, on the brakes, coasting to a stop
        control.set_steering(&mut c.m, 0);
        control.set_gas_level(&mut c.m, 0);
        control.set_brake_level(&mut c.m, -1);
        control.set_down_shifting(&mut c.m, 0);
        control.set_hanno(&mut c.m, 0);
        let vel = car.n().linear_vel();
        let slow = |v: i32| v.wrapping_mul(254) / 256;
        vel.update_x(&mut c.m, slow);
        vel.update_y(&mut c.m, slow);
        vel.update_z(&mut c.m, slow);
    } else {
        let step = if setup.ramp_gas(&mut c.m) != 0 { 36 } else { 48 };
        let gas = ramp(control.gas_level(&mut c.m) as u8 as i32, control.desired_gas_level(&mut c.m) as u8 as i32, step);
        control.set_gas_level(&mut c.m, gas as i8);
        let brake = if setup.ramp_brake(&mut c.m) != 0 {
            ramp(control.brake_level(&mut c.m) as u8 as i32, control.desired_brake_level(&mut c.m) as u8 as i32, 16)
        } else {
            control.desired_brake_level(&mut c.m) as u8 as i32
        };
        control.set_brake_level(&mut c.m, brake as i8);
        let timer = control.gear_shift_timer(&mut c.m);
        if timer != 0 {
            control.set_gear_shift_timer(&mut c.m, timer.wrapping_sub(1));
        }
        if SIM_COUNTER.get(&mut c.m) >= 513 {
            change_gear(c, car);
        }
        let steering = if setup.ramp_steering(&mut c.m) != 0 {
            let ramp_by = car.specs(&mut c.m).steering_ramp(&mut c.m);
            ramp(control.steering(&mut c.m), control.desired_steering(&mut c.m), ramp_by)
        } else {
            control.desired_steering(&mut c.m)
        };
        control.set_steering(&mut c.m, steering);
    }

    // held at the start on the brake; a blown tyre or pulled over: rolling to a stop
    let held = SIM_COUNTER.get(&mut c.m) < 512 && start_held(c);
    if held {
        control.set_brake_level(&mut c.m, -1);
    } else if car.blowout(&mut c.m) != 0 || car.pull_over(&mut c.m) != 0 {
        let reversing = control.hanno(&mut c.m) != 0;
        control.set_gas_level(&mut c.m, if reversing { 128u8 as i8 } else { 0 });
        control.set_brake_level(&mut c.m, if reversing { 0 } else { 128u8 as i8 });
        control.set_down_shifting(&mut c.m, 0);
    }
    if car.crash(&mut c.m) == 2 {
        control.set_gas_level(&mut c.m, 0);
    }
    if FORCE_HUMAN_HAND_BRAKE.get(&mut c.m) != 0 && car.rs_control(&mut c.m) != 0 {
        // a handbrake turn toward the road
        let side = car.road_position(&mut c.m).wrapping_mul(car.direction(&mut c.m));
        control.set_steering(&mut c.m, if side > 0 { -124 } else { 124 });
        control.set_hand_brake(&mut c.m, 1);
    }

    let gas = pedal_ratio(control.gas_level(&mut c.m) as u8);
    GAS_RATIO.set(&mut c.m, gas);
    let brake = pedal_ratio(control.brake_level(&mut c.m) as u8);
    let steer = control.steering(&mut c.m).wrapping_abs() << 9;
    STEER_RATIO.set(&mut c.m, steer);
    BRAKE_RATIO.set(&mut c.m, brake);
    let index = car.car_index(&mut c.m) as u32;
    if GAME_SETUP.car_info(index).transmission(&mut c.m) == TRANSMISSION_AUTOMATIC
        && control.gear(&mut c.m) as u8 as i32 == GEAR_REVERSE
        && control.hanno(&mut c.m) == 1
    {
        // reversing on the brake pedal: the pedals swap
        let gas = pedal_ratio(control.brake_level(&mut c.m) as u8);
        GAS_RATIO.set(&mut c.m, gas);
        let brake = pedal_ratio(control.gas_level(&mut c.m) as u8);
        BRAKE_RATIO.set(&mut c.m, brake);
    }
}

/// The gear change part of [`ramp_car_control_values`].
fn change_gear(c: &mut Cpu, car: Car) {
    let control = car.control();
    let index = car.car_index(&mut c.m) as u32;
    let automatic = GAME_SETUP.car_info(index).transmission(&mut c.m) == TRANSMISSION_AUTOMATIC;
    if automatic {
        let counter = SIM_COUNTER.get(&mut c.m);
        if counter < 520 && start_held(c) {
            // just after the start: into first
            control.set_desired_gear(&mut c.m, GEAR_FIRST as i8);
        } else if control.desired_gear(&mut c.m) != control.gear(&mut c.m) || car.stats().finish_type(&mut c.m) != 0 {
            control.set_hanno(&mut c.m, 0);
        } else if car.pull_over(&mut c.m) == 0 && car.blowout(&mut c.m) == 0 && car.n().speed_xz(&mut c.m).wrapping_abs() < 13107 {
            // at a standstill: the brake held engages reverse, the throttle first again
            let desired_gas = control.desired_gas_level(&mut c.m) as u8;
            let desired_brake = control.desired_brake_level(&mut c.m) as u8;
            let desired_gear = control.desired_gear(&mut c.m) as u8;
            if desired_brake >= 129 && desired_gas == 0 && desired_gear >= 2 && control.hanno(&mut c.m) == 0 {
                control.set_desired_gear(&mut c.m, GEAR_REVERSE as i8);
                control.set_hanno(&mut c.m, 1);
            } else if desired_gas >= 129 && control.gear(&mut c.m) == 0 && control.hanno(&mut c.m) != 0 {
                control.set_desired_gear(&mut c.m, GEAR_FIRST as i8);
                control.set_hanno(&mut c.m, 0);
            }
        }
    }

    let gear = control.gear(&mut c.m) as u8 as i32;
    let desired = control.desired_gear(&mut c.m) as u8 as i32;
    if desired == gear {
        return;
    }
    let specs = car.specs(&mut c.m);
    if automatic || car.rs_control(&mut c.m) != 0 {
        if desired < 2 {
            control.set_down_shifting(&mut c.m, 0);
        } else {
            if desired != GEAR_FIRST || gear >= 2 {
                return;
            }
            // out of reverse or neutral: the highest gear whose top speed is below the car's
            control.set_last_gear(&mut c.m, gear as i8);
            let mut pick = GEAR_FIRST;
            let mut g = GEAR_FIRST;
            while g < specs.num_gears(&mut c.m) {
                let redline = specs.redline(&mut c.m);
                let top = fixedmult(specs.vel_to_rpm_ratio_inv(&mut c.m, g as u32), redline << 16);
                if top < car.linear_vel_ch().z(&mut c.m) {
                    pick = g;
                }
                g += 1;
            }
            control.set_down_shifting(&mut c.m, 0);
            control.set_gear(&mut c.m, pick as i8);
            let delay = specs.gear_shift_delay(&mut c.m) as i8;
            control.set_gear_shift_timer(&mut c.m, delay);
            return;
        }
    } else {
        control.set_down_shifting(&mut c.m, (desired < gear && desired >= 2) as i8);
    }
    let delay = specs.gear_shift_delay(&mut c.m) as i8;
    control.set_last_gear(&mut c.m, gear as i8);
    control.set_gear(&mut c.m, desired as i8);
    control.set_gear_shift_timer(&mut c.m, delay);
}

pub fn abi_ramp_car_control_values(c: &mut Cpu) {
    ramp_car_control_values(c, Car(c.r[4]));
}

/// The engine's rise in rpm on a downshift (the throttle blip), by the gear shifted into:
/// off the brake, and on it.
const DOWNSHIFT_BLIP: [i32; 8] = [0, 0, 250, 200, 175, 150, 125, 0];
const DOWNSHIFT_BLIP_BRAKING: [i32; 8] = [0, 0, 200, 175, 150, 125, 100, 0];
/// `GameSetup_tData.sgge`: the cheat giving more grip under power.
const SGGE_GRIP: i32 = 8;
const GEAR_NEUTRAL: i32 = 1;

/// The `fastRandom` macro: the next of the game's quick random numbers (16 bits).
fn fast_random(c: &mut Cpu) -> u32 {
    let t = FAST_RANDOM.get(&mut c.m).wrapping_mul(RAND_SEED.get(&mut c.m));
    RAND_TEMP.set(&mut c.m, t);
    FAST_RANDOM.set(&mut c.m, t & 0xFFFF);
    (t >> 8) & 0xFFFF
}

/// `Physics_CalculateCarAcceleration` (PHYSICS.CPP, 0x800ab618): the engine. Its rpm follows
/// the throttle when it is out of gear (in neutral, mid-shift or without power), with a rev
/// limiter at the redline and a damaged engine misfiring at random; in gear it is tied to the
/// wheels, but for wheelspin (too much slip, or driving against the car's motion) and engine
/// braking off the throttle. Returns the drive acceleration (the torque through the gear,
/// within the tyres' grip under power) less the aerodynamic drag.
pub fn calculate_car_acceleration(c: &mut Cpu, car: Car) -> i32 {
    let specs = car.specs(&mut c.m);
    let control = car.control();
    let redline = specs.redline(&mut c.m);
    let spin_slip = redline / 8;
    let mut acc = 0;
    let damage = car.n().damage(&mut c.m, 1).wrapping_add(car.n().damage(&mut c.m, 5)) / 256;
    let misfire = fast_random(c) < damage as u32;
    let over = redline.wrapping_add(500) < car.flywheel_rpm(&mut c.m);
    EXCEED_REDLINE.set(&mut c.m, over as i32);

    // drag by the cube of the speed (more with ground effects)
    let mut drag_factor = 0x1_0000;
    if car.car_info(&mut c.m).ground_effects(&mut c.m) != 0 {
        let down = specs.front_aero_down_force(&mut c.m).wrapping_add(specs.rear_aero_down_force(&mut c.m)) / 2;
        drag_factor = down.wrapping_add(0x1_0000);
    }
    let v = fixedmult(car.linear_vel_ch().z(&mut c.m), drag_factor) / 0x1_0000;
    let drag = fixedmult(specs.drag_coeff(&mut c.m), v.wrapping_mul(v).wrapping_mul(v));

    // the rpm the throttle asks for
    let gear = control.gear(&mut c.m) as u8 as i32;
    let power = POWER_CONTROL.get(&mut c.m) != 0;
    let free = gear == GEAR_NEUTRAL || !power;
    let top = redline.wrapping_add(if free { 250 } else { 100 });
    let asked = fixedmult(top, GAS_RATIO.get(&mut c.m));
    let mut target = if asked < top { asked } else { top };
    // the rev limiter: at the redline the engine is held below it for a few frames
    if car.flywheel_rpm(&mut c.m) >= redline {
        car.set_flywheel_rpm(&mut c.m, redline.wrapping_add(50));
        car.set_rev_limit(&mut c.m, if free { 3 } else { 4 });
    }
    if car.rev_limit(&mut c.m) > 0 {
        let cut = redline.wrapping_sub(if free { 800 } else { 400 });
        target = cut.min(target);
        car.update_rev_limit(&mut c.m, |n| n - 1);
    }

    let timer = control.gear_shift_timer(&mut c.m) as u8;
    if gear == GEAR_NEUTRAL || timer != 0 || !power {
        free_rev(c, car, target, misfire);
        car.set_front_wheel_spin(&mut c.m, 0);
        car.set_wheel_spin(&mut c.m, 0);
        return wet_first_gear(c, car, acc).wrapping_sub(drag);
    }

    let index = car.car_index(&mut c.m) as u32;
    if GAME_SETUP.car_info(index).transmission(&mut c.m) == TRANSMISSION_AUTOMATIC || car.rs_control(&mut c.m) != 0 {
        auto_shift(c, car);
    }
    // the rpm the wheels turn the engine at (mid-shift up, the gear left)
    let timer = control.gear_shift_timer(&mut c.m) as u8;
    let geared = if timer != 0 && control.down_shifting(&mut c.m) == 0 { control.last_gear(&mut c.m) } else { control.gear(&mut c.m) } as u8;
    let ratio = specs.vel_to_rpm_ratio(&mut c.m, geared as u32);
    let wheel_rpm = fixedmult(car.linear_vel_ch().z(&mut c.m), ratio) / 0x1_0000;

    // the torque through the gear (at the redline's when over it)
    let gear = control.gear(&mut c.m) as u8;
    let coeff = specs.gear_acc_coeff(&mut c.m, gear as u32);
    acc = if EXCEED_REDLINE.get(&mut c.m) != 0 || car.rev_limit(&mut c.m) > 0 {
        let step = specs.redline(&mut c.m) / 256;
        let torque = specs.torque_curve(&mut c.m, step as u32);
        fixedmult(torque, coeff) << 1
    } else {
        let step = car.flywheel_rpm(&mut c.m) / 256;
        let torque = get_torque(c, car, step);
        fixedmult(torque, coeff)
    };
    let mut demand = target.wrapping_sub(wheel_rpm);
    if demand.wrapping_abs() < 125 && target < specs.redline(&mut c.m).wrapping_sub(300) {
        demand = 0;
    }
    let slip = car.flywheel_rpm(&mut c.m).wrapping_sub(wheel_rpm);

    let gear = gear as i32;
    let speed = car.linear_vel_ch().z(&mut c.m);
    let throttle = 0x8000 < GAS_RATIO.get(&mut c.m);
    let spinning = (slip >= 251 && control.gear_shift_timer(&mut c.m) == 0 && gear < 5)
        || (gear >= 2 && speed < -6553 && throttle)
        || (gear == GEAR_REVERSE && speed >= 6554 && throttle)
        || car.wheel_spin(&mut c.m) == 1;
    if spinning && car.rev_limit(&mut c.m) == 0 {
        // wheelspin: the engine drops slowly toward the wheels' rpm
        let car_type = car.car_info(&mut c.m).car_type(&mut c.m);
        let drop = if target < 2000 || (control.desired_gas_level(&mut c.m) as u8) < 64 || misfire || (car_type == 19 && gear >= 3) {
            200
        } else if gear == GEAR_FIRST || gear == GEAR_REVERSE {
            10
        } else if gear < 3 {
            0
        } else {
            50
        };
        if spin_slip < slip {
            car.set_wheel_spin(&mut c.m, 2);
        }
        car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_sub(if drop < slip { drop } else { slip }));
    } else if demand < 0 {
        // off the throttle: engine braking, halved going downhill in first, or uphill in
        // reverse
        acc = fixedmult(acc, specs.gas_off_factor(&mut c.m)).wrapping_neg();
        let slope = GRAVITY_CH.z(&mut c.m);
        if (slope > 0 && gear == GEAR_FIRST && acc < 0) || (slope < 0 && gear == GEAR_REVERSE && acc > 0) {
            acc /= 2;
        }
        let gear = control.gear(&mut c.m) as u8;
        let inv = specs.vel_to_rpm_ratio_inv(&mut c.m, gear as u32);
        let fall = fixedmult(inv << 3, 0x2800_0000) / 0x1_0000;
        let back = slip.wrapping_neg();
        car.update_flywheel_rpm(&mut c.m, |rpm| {
            if gear != 0 {
                if fall < back { rpm.wrapping_add(fall) } else { rpm.wrapping_sub(slip) }
            } else {
                rpm.wrapping_add(fall.max(back))
            }
        });
        if EXCEED_REDLINE.get(&mut c.m) == 0 {
            car.update_flywheel_rpm(&mut c.m, |rpm| if target < rpm { rpm } else { target });
        }
    } else if demand == 0 {
        // cruising: the engine just holds the speed
        car.set_flywheel_rpm(&mut c.m, wheel_rpm);
        acc = drag;
    } else {
        if misfire {
            acc = 0;
            car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_sub(100));
        } else {
            if slip >= 201 {
                car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_sub(200));
            } else if slip < -200 {
                car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_add(200));
            } else {
                car.set_flywheel_rpm(&mut c.m, wheel_rpm);
            }
            acc = fixedmult(acc, GAS_RATIO.get(&mut c.m));
        }
        car.update_flywheel_rpm(&mut c.m, |rpm| if target < rpm { target } else { rpm });
        // under power the drive is held to the tyres' grip, more when sliding
        let most = if GAME_SETUP.sgge(&mut c.m) & SGGE_GRIP != 0 { 0x3_0000 } else { 0x2_0000 };
        let grip = car.slide(&mut c.m).wrapping_abs().wrapping_add(0x1_0000).min(most);
        acc = (acc / 256).wrapping_mul(grip / 256);
    }

    // below 0 rpm (rolling against the gear) the engine stops the car
    if car.flywheel_rpm(&mut c.m) < 0 {
        let stop = (car.linear_vel_ch().z(&mut c.m) << 5).wrapping_neg();
        let beyond = acc.wrapping_sub(stop);
        if !((acc > 0 && stop > 0 && beyond > 0) || (acc < 0 && stop < 0 && beyond < 0)) {
            acc = stop;
            car.set_flywheel_rpm(&mut c.m, 0);
        }
    }
    wet_first_gear(c, car, acc).wrapping_sub(drag)
}

/// The engine revving free of the wheels: toward the rpm asked (dropping while shifting up,
/// blipping on a downshift).
fn free_rev(c: &mut Cpu, car: Car, target: i32, misfire: bool) {
    let control = car.control();
    let timer = control.gear_shift_timer(&mut c.m);
    let rpm = car.flywheel_rpm(&mut c.m);
    if misfire {
        car.set_flywheel_rpm(&mut c.m, rpm.wrapping_sub(100).max(0));
        return;
    }
    if rpm < target && timer == 0 {
        let up = rpm.wrapping_add(250);
        car.set_flywheel_rpm(&mut c.m, if target < up { target } else { up });
        return;
    }
    let shifting = (rpm < target || timer != 0) && control.last_gear(&mut c.m) as u8 as i32 != GEAR_NEUTRAL;
    if shifting {
        if control.down_shifting(&mut c.m) != 0 {
            let to = control.desired_gear(&mut c.m) as u8 as usize;
            let blip = if (control.brake_level(&mut c.m) as u8) < 65 { DOWNSHIFT_BLIP[to] } else { DOWNSHIFT_BLIP_BRAKING[to] };
            let redline = car.specs(&mut c.m).redline(&mut c.m);
            car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_add(blip).min(redline));
        } else {
            let gear = control.gear(&mut c.m) as u8;
            let fall = if gear >= 4 { 200 } else { 100 };
            car.update_flywheel_rpm(&mut c.m, |rpm| rpm.wrapping_sub(fall).max(0));
        }
        return;
    }
    // falling back toward the rpm asked
    if rpm >= target {
        let down = rpm.wrapping_sub(200);
        car.set_flywheel_rpm(&mut c.m, if target < down { down } else { target });
    }
}

/// The handling cars (types 12..15) in first gear on a slippery track: a quarter less drive.
fn wet_first_gear(c: &mut Cpu, car: Car, acc: i32) -> i32 {
    let car_type = car.car_info(&mut c.m).car_type(&mut c.m);
    if (car_type.wrapping_sub(12) as u32) < 4 && SLIPPERY.get(&mut c.m) != 0 && car.control().gear(&mut c.m) as u8 as i32 == GEAR_FIRST && acc > 0 {
        acc.wrapping_mul(3) >> 2
    } else {
        acc
    }
}

pub fn abi_calculate_car_acceleration(c: &mut Cpu) {
    c.r[2] = calculate_car_acceleration(c, Car(c.r[4])) as u32;
}

/// `GameSetup_tData.sgge` cheat: the horn boosts the car.
const SGGE_HORN_BOOST: i32 = 0x80;
/// `BO_tNewtonObj.driveSurfaceType`: the car is off the road (it slows at speed).
const SURFACE_OFF_ROAD: i32 = 3;

/// `m * v` as the physics computes it (each term rounded by `fixedmult`).
fn mat_vec(m: &[i32; 9], v: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|r| fixedmult(v[0], m[3 * r]).wrapping_add(fixedmult(v[1], m[3 * r + 1])).wrapping_add(fixedmult(v[2], m[3 * r + 2])))
}

fn transposed(m: &[i32; 9]) -> [i32; 9] {
    std::array::from_fn(|i| m[3 * (i % 3) + i / 3])
}

/// `a * b` as two 8.8 numbers multiply (both truncated to 1/256 first).
fn mul_8_8(a: i32, b: i32) -> i32 {
    (a / 256).wrapping_mul(b / 256)
}

/// `Physics_Real` (PHYSICS.CPP, 0x800acdf8): one simulation step of a car on its wheels. In
/// the car's frame (`shadowMat`, the car on the road): the controls, the steering angle
/// (counter-steered by gravity on a camber, less with damage, at speed and sliding), the
/// engine's drive and the brakes split between the axles, the road's grip with the weight
/// transfer, each axle's tyre forces, and from them the acceleration and the yaw; then the
/// world velocities and spin, with drag and the damping of off-road, neutral and handbrake.
/// Cars under road-side control steer and drive themselves toward their lane.
pub fn real(c: &mut Cpu, car: Car) {
    let n = car.n();
    let acc_ch = car.linear_acc_ch();
    acc_ch.set(&mut c.m, crate::vec::Vec3::default());
    let specs = car.specs(&mut c.m);
    STEERING_CONTROL.set(&mut c.m, 1);
    POWER_CONTROL.set(&mut c.m, 1);
    // rain and snow make the road slippery, but not in tunnels
    let wet = GAME_SETUP.weather(&mut c.m) != 0 && tunnel_flag(c, car) != 1;
    SLIPPERY.set(&mut c.m, wet as i32);
    // the steering or the drive is lost with both its wheels in the air
    let airborne = |c: &mut Cpu, a: u32, b: u32| {
        car.wheel(a).wheel_in_air(&mut c.m) != 0 && car.wheel(b).wheel_in_air(&mut c.m) != 0 && n.obj_altitude(&mut c.m) >= 3277
    };
    if airborne(c, 0, 1) {
        STEERING_CONTROL.set(&mut c.m, 0);
    }
    if airborne(c, 2, 3) {
        POWER_CONTROL.set(&mut c.m, 0);
    }

    // velocities into the car's frame
    let frame = n.shadow_mat().get(&mut c.m).m;
    let vel = mat_vec(&frame, n.linear_vel().get(&mut c.m).into());
    car.linear_vel_ch().set(&mut c.m, vel.into());
    let spin = mat_vec(&frame, n.angular_vel().get(&mut c.m).into());
    car.angular_vel_ch().set(&mut c.m, spin.into());
    let vel_ch = car.linear_vel_ch();
    let forward = vel_ch.z(&mut c.m);
    let slide = if 0x5_0000 < forward { fixeddiv(vel_ch.x(&mut c.m), forward) } else { 0 };
    car.set_slide(&mut c.m, slide);
    // the road's velocity under the axles (in 1/32 m/s halved), the yaw adding to it at the
    // front and taking it away at the rear
    let ground = vel.map(|v| (v << 5).wrapping_neg() / 2);
    let yaw = (fixedmult(n.angular_vel().y(&mut c.m) << 5, specs.alpha_to_acc_rot_inertia(&mut c.m))).wrapping_neg() / 2;
    let g = mat_vec(&frame, [0, -0xA_0000, 0]);
    GRAVITY_CH.set(&mut c.m, g.into());

    abi::with_stack(c, 2 * WheelAcc::SIZE, |c, at| {
        let (front, rear) = (WheelAcc(at), WheelAcc(at + WheelAcc::SIZE));
        front.vel_cap().set(&mut c.m, crate::vec::Vec3::new(ground[0].wrapping_add(yaw), ground[1], ground[2]));
        rear.vel_cap().set(&mut c.m, crate::vec::Vec3::new(ground[0].wrapping_sub(yaw), ground[1], ground[2]));
        ramp_car_control_values(c, car);
        let angle = steering_angle(c, car);
        front.set_steering_angle(&mut c.m, angle);
        rear.set_steering_angle(&mut c.m, 0);
        front.set_front_tire(&mut c.m, 1);
        rear.set_front_tire(&mut c.m, 0);
        real_on_wheels(c, car, front, rear);
    });
}

/// `BWorldSm_TunnelFlagSm` (still recompiled): 1 where the car's road quad is in a tunnel.
fn tunnel_flag(c: &mut Cpu, car: Car) -> u32 {
    abi::call(c, nfs4rc::f_800809f8, &[car.n().sim_road_info().0])
}

/// The front wheels' steering angle, from the controls.
fn steering_angle(c: &mut Cpu, car: Car) -> i32 {
    let n = car.n();
    let specs = car.specs(&mut c.m);
    // a camber pulls the steering
    let pull = (((GRAVITY_CH.x(&mut c.m) / 256).wrapping_mul(3)) << 8) / 0x1_0000;
    let wheel = car.control().steering(&mut c.m).wrapping_add(pull).clamp(-127, 127);
    let angle = wheel.wrapping_mul(specs.max_steering_acc(&mut c.m)) / 128;
    // less with front damage
    let damage = [0, 1, 2, 9].iter().fold(0i32, |s, &i| s.wrapping_add(n.damage(&mut c.m, i))) / 512;
    let mut angle = mul_8_8(angle, 0x1_0000 - damage);
    // less at speed (above 60 m/s)
    let forward = car.linear_vel_ch().z(&mut c.m);
    if 0x3C_0000 < forward {
        angle = fixeddiv(angle, fixedmult(forward, 1092));
    }
    // steering into a slide (past 0.05 above 26.7 m/s, 0.15 above 13.4) is cut by the slide
    let limit = if 0x1A_B333 < forward {
        Some((3277, -3276))
    } else if 0xD_6666 < forward {
        Some((9831, -9830))
    } else {
        None
    };
    if let Some((right, left)) = limit {
        let slide = car.slide(&mut c.m);
        if (slide >= right && angle > 0) || (slide < left && angle < 0) {
            let cut = (slide.wrapping_abs() << 1).min(0x1_0000);
            angle = fixedmult(angle, cut);
        }
    }
    angle
}

/// The rest of [`real`], with the two axles' wheel structures.
fn real_on_wheels(c: &mut Cpu, car: Car, front: WheelAcc, rear: WheelAcc) {
    let n = car.n();
    let specs = car.specs(&mut c.m);
    let control = car.control();
    // the drive, split between the axles
    let drive = calculate_car_acceleration(c, car);
    let front_drive = mul_8_8(drive, specs.front_drive_ratio(&mut c.m));
    front.set_acc(&mut c.m, front_drive);
    rear.set_acc(&mut c.m, drive.wrapping_sub(front_drive));
    if STEERING_CONTROL.get(&mut c.m) == 0 && POWER_CONTROL.get(&mut c.m) == 0 {
        car.set_front_skid(&mut c.m, 0);
        car.set_rear_skid(&mut c.m, 0);
        return;
    }
    car.set_crash(&mut c.m, 0);

    // the brakes, no harder than stops the car this frame, weaker with damage
    let forward = car.linear_vel_ch().z(&mut c.m);
    let mut brake = mul_8_8(BRAKE_RATIO.get(&mut c.m), specs.max_brake_acc(&mut c.m)).min(forward.wrapping_abs() << 5);
    let wear = n.damage(&mut c.m, 9);
    if wear != 0 {
        brake = mul_8_8(brake, 0x1_0000 - wear / 128);
    }
    if car.linear_vel_ch().z(&mut c.m) > 0 {
        brake = brake.wrapping_neg();
    }
    let front_brake = fixedmult(brake, specs.front_brake_ratio(&mut c.m));
    front.update_acc(&mut c.m, |a| a.wrapping_add(front_brake));
    rear.update_acc(&mut c.m, |a| a.wrapping_add(brake.wrapping_sub(front_brake)));

    // the grip: the car's weight on the road (none when it is upside down), split between
    // the axles by the grip bias and the weight transfer
    let weight = fixedmult(GRAVITY_CH.y(&mut c.m).wrapping_neg(), specs.lateral_grip_mult(&mut c.m));
    calculate_road_grip_modifiers(c, car);
    let weight = mul_8_8(weight, ROAD_MULT.get(&mut c.m));
    if weight < 0 {
        return;
    }
    let bias = specs.front_grip_bias(&mut c.m).wrapping_add(if SLIPPERY.get(&mut c.m) != 0 { 655 } else { 0 });
    let on_front = fixedmult(weight, bias);
    let damage = [4, 5, 6, 9].iter().fold(0i32, |s, &i| s.wrapping_add(n.damage(&mut c.m, i))) / 512;
    let keep = damage.wrapping_add(0xC000);
    let transfer = fixedmult(car.g_transfer_front(&mut c.m), keep);
    car.set_g_transfer_front(&mut c.m, transfer);
    if transfer < 0 {
        car.set_g_transfer_front(&mut c.m, mul_8_8(transfer, keep));
    } else if transfer > 0 {
        car.set_g_transfer_front(&mut c.m, transfer.wrapping_mul(3) / 4);
    }
    let transfer = car.g_transfer_front(&mut c.m);
    let grip = mul_8_8(FRONT_MULT.get(&mut c.m), on_front.wrapping_sub(transfer));
    front.set_road_grip(&mut c.m, grip);
    let grip = mul_8_8(REAR_MULT.get(&mut c.m), weight.wrapping_sub(on_front).wrapping_add(transfer));
    rear.set_road_grip(&mut c.m, grip);

    // the tyres
    car.set_wheel_lock(&mut c.m, 0);
    for w in [front, rear] {
        w.final_acc().set_x(&mut c.m, 0);
        w.final_acc().set_z(&mut c.m, 0);
    }
    if STEERING_CONTROL.get(&mut c.m) != 0 {
        calculate_tire_forces(c, car, front);
    }
    if POWER_CONTROL.get(&mut c.m) != 0 {
        calculate_tire_forces(c, car, rear);
    }
    let (fa, ra) = (front.final_acc().get(&mut c.m), rear.final_acc().get(&mut c.m));
    let acc_ch = car.linear_acc_ch();
    acc_ch.set_z(&mut c.m, fa.z.wrapping_add(ra.z));
    acc_ch.set_x(&mut c.m, fa.x.wrapping_add(ra.x));
    let mut along = fixedmult(acc_ch.z(&mut c.m), specs.lateral_grip_mult_inv(&mut c.m));
    acc_ch.set_z(&mut c.m, along);
    if GAME_SETUP.sgge(&mut c.m) & SGGE_HORN_BOOST != 0 && along > 0 && control.horn(&mut c.m) != 0 {
        along <<= 2;
        acc_ch.set_z(&mut c.m, along);
    }
    if SLIPPERY.get(&mut c.m) == 0 {
        // dry roads grip better (rain tyres in the low gears more so)
        let rain_low = car.car_info(&mut c.m).tire_type(&mut c.m) == TIRE_RAIN && (control.gear(&mut c.m) as u8) < 4;
        acc_ch.update_z(&mut c.m, |a| fixedmult(a, if rain_low { 0x1_2666 } else { 0x1_1999 }));
    }
    acc_ch.set_y(&mut c.m, 0);
    // the weight transfer for the next frame
    let slope = fixedmult(GRAVITY_CH.z(&mut c.m), 6553).wrapping_neg();
    let factor = specs.g_transfer_factor(&mut c.m);
    let (side, along) = (acc_ch.x(&mut c.m), acc_ch.z(&mut c.m));
    car.set_g_transfer_right(&mut c.m, fixedmult(side / 8, factor).wrapping_neg());
    car.set_g_transfer_front(&mut c.m, fixedmult(along, factor).wrapping_add(slope));

    // the yaw: from the axles' sideways forces and the sides' grip difference
    let (fa, ra) = (front.final_acc().get(&mut c.m), rear.final_acc().get(&mut c.m));
    let inertia = specs.acc_to_alpha_rot_inertia(&mut c.m);
    let sides = fixedmult(fa.z.wrapping_add(ra.z), LEFT_MULT.get(&mut c.m).wrapping_sub(RIGHT_MULT.get(&mut c.m)));
    let mut yaw = fixedmult(fa.x.wrapping_sub(ra.x), inertia).wrapping_add(fixedmult(sides, inertia) << 1);
    let spin = n.angular_vel().y(&mut c.m);
    if (spin > 0 && yaw > 0) || (spin < 0 && yaw < 0) {
        // spinning further: halved on the handbrake, stopped when steering against it with
        // the wheels spinning, else capped
        let steering = control.desired_steering(&mut c.m);
        yaw = if control.hand_brake(&mut c.m) != 0 && GAME_SETUP.sgge(&mut c.m) & SGGE_GRIP == 0 {
            yaw / 2
        } else if ((steering < 0 && spin >= 13108) || (steering > 0 && spin < -13107)) && car.wheel_spin(&mut c.m) == 1 {
            spin.wrapping_neg()
        } else {
            fixedmult(yaw, specs.spin_vel_cap(&mut c.m))
        };
    }

    // back into the world: accelerations, then velocities (a 32nd of them a step; the yaw a
    // 64th)
    let to_world = transposed(&n.shadow_mat().get(&mut c.m).m);
    crate::cars::do_gravity_effects_on_acc(c, car);
    let acc = mat_vec(&to_world, car.linear_acc_ch().get(&mut c.m).into());
    car.linear_acc().set(&mut c.m, acc.into());
    let alpha = mat_vec(&to_world, [0, yaw, 0]);
    car.angular_acc().set(&mut c.m, alpha.into());
    let vel = n.linear_vel();
    vel.update_x(&mut c.m, |v| v.wrapping_add(acc[0] / 32));
    vel.update_y(&mut c.m, |v| v.wrapping_add(acc[1] / 32));
    vel.update_z(&mut c.m, |v| v.wrapping_add(acc[2] / 32));
    let spin = n.angular_vel();
    spin.update_x(&mut c.m, |v| v.wrapping_add(alpha[0] / 32));
    spin.update_y(&mut c.m, |v| v.wrapping_add(alpha[1] / 64));
    spin.update_z(&mut c.m, |v| v.wrapping_add(alpha[2] / 32));

    if car.rs_control(&mut c.m) != 0 {
        rs_control_drive(c, car);
    }

    // drag off the throttle or sliding (unless the cheat), off-road slowing at speed, the
    // coasting losses in neutral, and the handbrake stopping a slow car
    let slow_all = |c: &mut Cpu, f: i32, spin_too: bool| {
        let vel = n.linear_vel();
        vel.update_x(&mut c.m, |v| fixedmult(f, v));
        vel.update_y(&mut c.m, |v| fixedmult(f, v));
        vel.update_z(&mut c.m, |v| fixedmult(f, v));
        if spin_too {
            let spin = n.angular_vel();
            spin.update_x(&mut c.m, |v| fixedmult(f, v));
            spin.update_y(&mut c.m, |v| fixedmult(f, v));
            spin.update_z(&mut c.m, |v| fixedmult(f, v));
        }
    };
    if GAME_SETUP.sgge(&mut c.m) & SGGE_GRIP == 0 && (control.gas_level(&mut c.m) == 0 || car.slide(&mut c.m).wrapping_abs() >= 6554) {
        let drag = specs.drag_factor(&mut c.m);
        slow_all(c, drag, false);
    }
    if n.drive_surface_type(&mut c.m) == SURFACE_OFF_ROAD && n.speed_xz(&mut c.m) > 0x2C_A3D7 {
        slow_all(c, 0xFEB8, false);
    } else if control.gear(&mut c.m) as u8 as i32 == GEAR_NEUTRAL && GRAVITY_CH.z(&mut c.m).wrapping_abs() <= 0x7FFF {
        let straight = car.linear_vel_ch().z(&mut c.m).wrapping_abs() > 0x13_FFFF && control.steering(&mut c.m).wrapping_abs() < 33;
        slow_all(c, if straight { 0xFF7C } else { 0xFD70 }, true);
    } else if control.hand_brake(&mut c.m) != 0 && n.speed_xz(&mut c.m) <= 0xFFFF {
        slow_all(c, 0x8000, false);
    }
}

/// Road-side control driving the car: throttle and brake toward its desired speed, steering
/// toward its lane a few slices ahead.
fn rs_control_drive(c: &mut Cpu, car: Car) {
    let n = car.n();
    let specs = car.specs(&mut c.m);
    let gear = |c: &mut Cpu| (car.control().gear(&mut c.m) as u8).max(2) as u32;
    let desired = car.desired_speed(&mut c.m);
    let g = gear(c);
    let ratio = specs.vel_to_rpm_ratio(&mut c.m, g);
    let want = fixedmult(if 0x4_71C6 < desired { desired } else { 0x18_8000 }, ratio) / 0x1_0000;
    let g = gear(c);
    let ratio = specs.vel_to_rpm_ratio(&mut c.m, g);
    let now = fixedmult(car.linear_vel_ch().z(&mut c.m), ratio) / 0x1_0000;
    let redline = specs.redline(&mut c.m);
    let base = div(want << 8, redline);
    let short = want.wrapping_sub(now);
    if short >= 201 {
        let gas = base.wrapping_add(div(short << 7, want)).min(224);
        car.set_rs_gas_level(&mut c.m, gas as i8);
    } else if short < 200 {
        let gas = base.wrapping_add(div(short << 7, want));
        car.set_rs_gas_level(&mut c.m, if gas < 0 { 0 } else { gas as i8 });
        if short < 0 {
            let brake = div((short << 9).wrapping_abs(), redline);
            let brake = if brake < 256 { brake } else { 255 };
            car.set_rs_brake_level(&mut c.m, brake as i8);
            if (brake & 0xFF) >= 129 {
                car.set_rs_gas_level(&mut c.m, 0);
            }
        }
    }

    // the lane a few slices ahead (one per 6 m/s, at least 3)
    let speed = car.current_speed(&mut c.m).wrapping_abs() / (6 * 0x1_0000);
    let rs = car.rs_control(&mut c.m);
    let ahead = if speed < 3 { rs.wrapping_mul(3) } else { rs.wrapping_mul(speed) };
    let slices = NUM_SLICES.get(&mut c.m);
    let mut slice = (n.sim_road_info().slice(&mut c.m) as i32).wrapping_add(ahead);
    if ahead >= 0 {
        if slice >= slices {
            slice -= slices;
        }
    } else if slice < 0 {
        slice += slices;
    }
    let range = ahead.wrapping_mul(3).wrapping_abs();
    let pos = n.position().get(&mut c.m);
    let at = |c: &mut Cpu| crate::types::TrkNewSlice(BWORLDSM_SLICES.get(&mut c.m) as u32).at(slice as u32);
    let s = at(c);
    let center = [0, 1, 2].map(|i| s.center(&mut c.m, i));
    let across = calculate_rs_control_desired_position(c, car, slice, range);
    let s = at(c);
    let off = [0, 1, 2].map(|i| fixedmult((s.right(&mut c.m, i) as i32) << 9, across));
    let to = crate::vec::Vec3::new(
        center[0].wrapping_add(off[0]).wrapping_sub(pos.x),
        center[1].wrapping_add(off[1]).wrapping_sub(pos.y),
        center[2].wrapping_add(off[2]).wrapping_sub(pos.z),
    );
    let dir = abi::with_stack(c, 12, |c, at| {
        let v = crate::types::Coord(at);
        v.set(&mut c.m, to);
        crate::mathnfs::normalize_vector(c, v);
        v.get(&mut c.m)
    });
    // steer by how far the lane is to the right
    let orient = n.orient_mat();
    let right = fixedmult(dir.x, orient.m(&mut c.m, 0))
        .wrapping_add(fixedmult(dir.y, orient.m(&mut c.m, 1)))
        .wrapping_add(fixedmult(dir.z, orient.m(&mut c.m, 2)));
    car.set_rs_steering(&mut c.m, (right / 256).clamp(-127, 127));
}

pub fn abi_real(c: &mut Cpu) {
    real(c, Car(c.r[4]));
}

/// `Physics_SimCar` (PHYSICS.CPP, 0x800ae1dc): a car's physics step. Upright enough it drives
/// ([`real`]); on its side or roof the engine winds down and the car slides to a stop.
pub fn sim_car(c: &mut Cpu, car: Car) {
    if car.n().orientation_to_ground().y(&mut c.m) >= 6553 {
        real(c, car);
        return;
    }
    let rpm = car.flywheel_rpm(&mut c.m);
    car.set_wheel_spin(&mut c.m, 0);
    car.set_slide(&mut c.m, 0);
    car.set_front_skid(&mut c.m, 0);
    car.set_rear_skid(&mut c.m, 0);
    car.set_flywheel_rpm(&mut c.m, if rpm >= 501 { rpm - 500 } else { 0 });
    if car.n().obj_altitude(&mut c.m) <= 32767 {
        stop_car(c, car);
    }
}

pub fn abi_sim_car(c: &mut Cpu) {
    sim_car(c, Car(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::Rng;
    use ps1rt::Mem;

    /// A race's RAM (`BOOT_RAM=1 nfshs boot-race ...` writes it), or None.
    fn race_ram() -> Option<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../re/boot/ram.bin");
        let ram = std::fs::read(&path).ok();
        if ram.is_none() {
            eprintln!("{} not found (BOOT_RAM=1 nfshs boot-race): skipped", path.display());
        }
        ram
    }

    fn cpu_with(ram: &[u8]) -> Cpu {
        let mut m = Mem::default();
        m.ram.copy_from_slice(ram);
        let mut c = Cpu::new(m);
        c.dispatch = nfs4rc::dispatch;
        c.dispatch_at = nfs4rc::dispatch_at;
        c.r[28] = crate::test_util::GP;
        c.r[29] = 0x801F_FF00;
        c
    }

    /// Shake the player's car into the states races rarely reach: road-side control, upside
    /// down or airborne, any controls and gear, damage, every car setup and cheat.
    fn perturb(c: &mut Cpu, car: Car, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u64| (rng.next() % n) as i32;
        let n = car.n();
        if pick(3) == 0 {
            car.set_rs_control(m, [-1, 1][pick(2) as usize]);
            car.set_desired_speed(m, pick(0x40_0000));
            car.set_current_speed(m, pick(0x80_0000) - 0x40_0000);
            car.set_road_position(m, pick(3) - 1);
            car.set_lane_index(m, pick(12));
            car.set_direction(m, [-1, 1][pick(2) as usize]);
            FORCE_HUMAN_HAND_BRAKE.set(m, pick(2));
        }
        if pick(4) == 0 {
            n.orientation_to_ground().set_y(m, pick(0x2_0000) - 0x1_0000);
            n.set_obj_altitude(m, pick(0x2_0000));
        }
        for w in 0..4 {
            car.wheel(w).set_wheel_in_air(m, (pick(4) == 0) as i32);
            car.wheel(w).set_road_surface_type(m, pick(16));
        }
        let control = car.control();
        control.set_desired_gas_level(m, pick(256) as i8);
        control.set_desired_brake_level(m, pick(256) as i8);
        control.set_gas_level(m, pick(256) as i8);
        control.set_brake_level(m, pick(256) as i8);
        control.set_gear(m, pick(7) as i8);
        control.set_desired_gear(m, pick(7) as i8);
        control.set_last_gear(m, pick(7) as i8);
        control.set_gear_shift_timer(m, pick(6) as i8);
        control.set_down_shifting(m, pick(2) as i8);
        control.set_hand_brake(m, (pick(3) == 0) as i8);
        control.set_horn(m, pick(2) as i8);
        control.set_steering(m, pick(256) - 128);
        control.set_desired_steering(m, pick(256) - 128);
        control.set_hanno(m, pick(2));
        car.set_flywheel_rpm(m, pick(12000) - 1000);
        car.set_rev_limit(m, pick(5));
        car.set_wheel_spin(m, pick(3));
        car.set_slide(m, pick(0x2_0000) - 0x1_0000);
        car.set_crash(m, pick(3));
        car.set_blowout(m, (pick(6) == 0) as i32);
        car.set_pull_over(m, (pick(6) == 0) as i32);
        car.stats().set_finish_type(m, [0, 0, 0, 1, 2, 3][pick(6) as usize]);
        car.set_g_transfer_front(m, pick(0x4_0000) - 0x2_0000);
        let vel = n.linear_vel();
        for k in 0..3 {
            let v = pick(0x80_0000) - 0x40_0000;
            match k {
                0 => vel.set_x(m, v),
                1 => vel.set_y(m, v / 8),
                _ => vel.set_z(m, v),
            }
        }
        n.angular_vel().set_y(m, pick(0x4_0000) - 0x2_0000);
        for i in 0..10 {
            n.set_damage(m, i, if pick(2) == 0 { 0 } else { pick(0x1_0000) });
        }
        n.set_drive_surface_type(m, pick(5));
        let info = car.car_info(m);
        info.set_transmission(m, pick(2));
        info.set_traction(m, pick(2));
        info.set_abs(m, pick(2));
        info.set_tire_type(m, pick(3));
        info.set_ramp_gas(m, pick(2));
        info.set_ramp_brake(m, pick(2));
        info.set_ramp_steering(m, pick(2));
        info.set_ground_effects(m, pick(2));
        GAME_SETUP.set_sgge(m, [0, 8, 0x80, 0x88][pick(4) as usize]);
        GAME_SETUP.set_weather(m, pick(2));
        GAME_SETUP.set_race_type(m, pick(7));
        SIM_COUNTER.set(m, [400, 515, 518, 2000][pick(4) as usize]);
    }

    /// `Physics_SimCar` on perturbed real race states, against its recompiled original.
    #[test]
    fn sim_car_fuzz() {
        let Some(ram) = race_ram() else { return };
        let mut rng = Rng(0xC0FFEE);
        let car = Car(u32::from_le_bytes(ram[0x11_0CA0..0x11_0CA4].try_into().unwrap()));
        let mut c = cpu_with(&ram);
        let mut mismatches = 0;
        for i in 0..20_000 {
            c.m.ram.copy_from_slice(&ram);
            perturb(&mut c, car, &mut rng);
            let state = c.m.ram.clone();
            crate::test_util::run(&mut c, nfs4rc::f_800ae1dc, &[car.0]);
            let want = c.m.ram.clone();
            c.m.ram.copy_from_slice(&state);
            sim_car(&mut c, car);
            // the stack below 0x801FFF00 is dead
            let dead = 0x1F_0000..0x1F_FF00;
            let diff: Vec<usize> = (0..want.len()).step_by(4).filter(|a| !dead.contains(a) && want[*a..a + 4] != c.m.ram[*a..a + 4]).collect();
            if !diff.is_empty() {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("case {i}: {} words differ", diff.len());
                    for &a in diff.iter().take(10) {
                        let w = |r: &[u8]| u32::from_le_bytes(r[a..a + 4].try_into().unwrap());
                        let addr = 0x8000_0000 | a as u32;
                        eprintln!("  {addr:#010x} {}: expected {:#x} got {:#x}", crate::check::describe_in(&want, addr), w(&want), w(&c.m.ram));
                    }
                }
            }
        }
        assert_eq!(mismatches, 0, "Physics_SimCar differs from the original");
    }
}
