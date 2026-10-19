//! CARS.CPP: the car objects and what the simulation does to them besides the physics.

use crate::fixed::fixedmult;
use crate::types::Car;
use ps1rt::Cpu;

/// `Car_tObj.carFlags`: gravity doesn't act on the car (it is held in place).
const CAR_NO_GRAVITY: i32 = 0x10;
/// Gravity, -10 m/s^2.
const GRAVITY: i32 = -0xA_0000;

/// `Cars_DoGravityEffectsOnAcc` (CARS.CPP, 0x800867a8): gravity's pull in the car's frame.
/// Tilted by more than 45 degrees it all applies; on the wheels only the slope's pull along
/// the car, half of it downhill the way the car accelerates, an eighth against, and none
/// on the brakes or on gentle slopes.
pub fn do_gravity_effects_on_acc(c: &mut Cpu, car: Car) {
    if car.car_flags(&mut c.m) & CAR_NO_GRAVITY != 0 {
        return;
    }
    let orient = car.n().orient_mat();
    let acc = car.linear_acc_ch();
    let up = fixedmult(0x1_0000, orient.m(&mut c.m, 4));
    if up <= 0xB5C1 {
        let g = [1, 4, 7].map(|i| fixedmult(GRAVITY, orient.m(&mut c.m, i)));
        acc.update_x(&mut c.m, |a| a.wrapping_add(g[0]));
        acc.update_y(&mut c.m, |a| a.wrapping_add(g[1]));
        acc.update_z(&mut c.m, |a| a.wrapping_add(g[2]));
        return;
    }
    if car.control().brake_level(&mut c.m) as u8 >= 64 {
        return;
    }
    let along = fixedmult(GRAVITY, orient.m(&mut c.m, 7));
    let along = fixedmult(along, car.n().gravity_mult(&mut c.m));
    if along.wrapping_abs() < 3277 {
        return;
    }
    acc.update_z(&mut c.m, |a| {
        let with = (along > 0) == (a > 0);
        a.wrapping_add(if with { along >> 1 } else { along >> 3 })
    });
}

pub fn abi_do_gravity_effects_on_acc(c: &mut Cpu) {
    do_gravity_effects_on_acc(c, Car(c.r[4]));
}
