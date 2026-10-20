//! CONTROL.CPP: what drives the cars' controls each tick (the AI's are set elsewhere).

use crate::abi;
use crate::globals::{GAME_SETUP, HUD_BUSTED_OVERLAY, INPUT_SIM, SIM_COUNTER};
use crate::types::Car;
use ps1rt::{Cpu, Mem};

/// `Car_tObj.AIFlags`: the police car's lights and siren are on.
const AI_SIREN: i32 = 0x2;
/// `Car_tControl.lights`: the headlights (1 and 2 on, with the high beams 4), the indicators
/// (8 left, 0x10 right, both the hazard lights).
const LIGHTS_LEFT: u8 = 0x8;
const LIGHTS_RIGHT: u8 = 0x10;
/// `Cars_tRenderInfo.signalLight`: an indicator blinks (its phase in the low bits).
const SIGNAL_ON: i16 = 0x80;

/// The police cars (car types 22..27): their indicator button works their siren.
fn is_police(m: &mut Mem, car: Car) -> bool {
    (car.car_info(m).car_type(m) as u32).wrapping_sub(22) < 6
}

/// `Control_AI` (CONTROL.CPP, 0x80092acc): nothing (the AI sets its controls itself).
pub fn control_ai(_c: &mut Cpu, _car: Car) {}

pub fn abi_control_ai(c: &mut Cpu) {
    control_ai(c, Car(c.r[4]));
}

/// `Control_Human` (CONTROL.CPP, 0x80092ad4, every fourth tick): a human's controls from the
/// pad (`Input_gSim`) or from the replay's road-side control: the throttle and brake to 1/32,
/// the steering to 1/4 (mirrored on a mirrored track), the handbrake, horn and the event
/// button; that event (the wingman calls, the indicators, a police car's siren, the
/// headlights and high beams); then the gear the pad asks for, unless that is a downshift
/// over the redline.
pub fn control_human(c: &mut Cpu, car: Car) {
    if SIM_COUNTER.get(&mut c.m) & 3 != 0 {
        return;
    }
    abi::frame(c, 24, |c, _| {
        let mut r = abi::Saved::of(c);
        r.0[0] = car.0;
        // (AI_Main_OverallSetup, called here for the first car when the AI races, is empty)
        r.call(c, nfs4rc::f_800dd948, &[], 0x8009_2b28); // InGame_SetRamp
        let replay = car.func_replay(&mut c.m);
        r.call_ptr(c, replay, &[car.0], 0x8009_2b38);
        let m = &mut c.m;
        let control = car.control();
        if car.rs_control(m) != 0 {
            let gas = car.rs_gas_level(m) as u8 & 0xF8;
            control.set_desired_gas_level(m, gas as i8);
            let steering = car.rs_steering(m) / 4 * 4;
            control.set_desired_steering(m, steering);
            control.set_desired_brake_level(m, 0);
            control.set_hand_brake(m, 0);
            control.set_horn(m, 0);
            control.set_abort(m, 0);
            control.set_event(m, 0);
        } else {
            let pad = INPUT_SIM;
            let gas = pad.gas(m) & 0xF8;
            control.set_desired_gas_level(m, gas as i8);
            let brake = pad.brake(m) & 0xF8;
            control.set_desired_brake_level(m, brake as i8);
            let steering = pad.steering(m) as i32 / 4 * 4;
            let mirrored = GAME_SETUP.mirror_track(m) == 1;
            control.set_desired_steering(m, if mirrored { steering.wrapping_neg() } else { steering });
            let flags = pad.flags(m);
            control.set_hand_brake(m, (flags & 1) as i8);
            control.set_horn(m, (flags >> 1 & 1) as i8);
            control.set_abort(m, (flags >> 3 == 3) as i8);
            control.set_event(m, (flags >> 3) as i8);
        }
        let event = control.event(m) as u8;
        let lights = control.lights(m) as u8;
        let signal = car.render();
        let police = matches!(event, 7 | 8 | 11) && is_police(m, car);
        let set_signals = |m: &mut Mem, lights: u8, left: Option<i16>, right: Option<i16>| {
            control.set_lights(m, lights as i8);
            for (i, s) in [left, right].into_iter().enumerate() {
                match s {
                    Some(v) => signal.update_signal_light(m, i as u32, |x| x | v),
                    None => signal.set_signal_light(m, i as u32, 0),
                }
            }
        };
        match event {
            // the wingman calls (not while busted)
            4..=6 => {
                if HUD_BUSTED_OVERLAY.get(m) == 0 {
                    control.set_queued_event(m, event as i8);
                    let index = car.car_index(m) as u32;
                    r.call(c, nfs4rc::f_800d6b8c, &[index, event as u32 - 4], 0x8009_2d08); // Hud_WingmanFlash
                }
            }
            // the left indicator, the right one (off when on), not on a police car
            7 if !police => {
                let m = &mut c.m;
                if lights & LIGHTS_LEFT != 0 {
                    set_signals(m, lights & !(LIGHTS_LEFT | LIGHTS_RIGHT), None, None);
                } else {
                    set_signals(m, (lights | LIGHTS_LEFT) & !LIGHTS_RIGHT, Some(SIGNAL_ON), None);
                }
            }
            8 if !police => {
                let m = &mut c.m;
                if lights & LIGHTS_RIGHT != 0 {
                    set_signals(m, lights & !(LIGHTS_LEFT | LIGHTS_RIGHT), None, None);
                } else {
                    set_signals(m, (lights | LIGHTS_RIGHT) & !LIGHTS_LEFT, None, Some(SIGNAL_ON));
                }
            }
            // the hazard lights, or a police car's siren
            11 => {
                if police {
                    car.update_ai_flags(&mut c.m, |f| f ^ AI_SIREN);
                } else if lights & (LIGHTS_LEFT | LIGHTS_RIGHT) != 0 {
                    set_signals(&mut c.m, lights & !(LIGHTS_LEFT | LIGHTS_RIGHT), None, None);
                } else {
                    set_signals(&mut c.m, lights | LIGHTS_LEFT | LIGHTS_RIGHT, Some(SIGNAL_ON), Some(SIGNAL_ON));
                }
            }
            // the headlights on or off (not with the siren on)
            12 => {
                if car.ai_flags(m) & AI_SIREN == 0 {
                    let lights = lights ^ 3;
                    control.set_lights(m, lights as i8);
                    if lights & 2 != 0 {
                        r.call(c, nfs4rc::f_800b078c, &[car.0, 1], 0x8009_2e94); // R3DCar_TurnHeadLightOn
                    } else {
                        r.call(c, nfs4rc::f_800b07ec, &[car.0, (lights & 4 == 0) as u32], 0x8009_2efc); // R3DCar_TurnHeadLightOff
                    }
                }
            }
            // the high beams (with the headlights on, not with the siren)
            14 => {
                let lights = lights ^ 4;
                control.set_lights(m, lights as i8);
                if lights & 2 == 0 && car.ai_flags(m) & AI_SIREN == 0 {
                    if lights & 4 != 0 {
                        r.call(c, nfs4rc::f_800b078c, &[car.0, 0], 0x8009_2ee8); // R3DCar_TurnHeadLightOn
                    } else {
                        r.call(c, nfs4rc::f_800b07ec, &[car.0, 1], 0x8009_2efc); // R3DCar_TurnHeadLightOff
                    }
                }
            }
            _ => {}
        }
        let m = &mut c.m;
        let (gear, specs) = (control.gear(m) as u8, car.specs(m));
        let gears = specs.num_gears(m);
        let want = r.call(c, nfs4rc::f_8009dd80, &[gear as u32, gears as u32], 0x8009_2f10) as u8; // Input_Gear
        let m = &mut c.m;
        let over_redline = specs.redline(m) < car.flywheel_rpm(m);
        if !(over_redline && want < gear) {
            control.set_desired_gear(m, want as i8);
        }
    });
}

pub fn abi_control_human(c: &mut Cpu) {
    control_human(c, Car(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::HUMAN_RACE_CARS;
    use crate::test_util::{Rng, against_original};

    /// The player's car with any pad input (each event), or under road-side control; its
    /// lights, indicators, siren, gear and revs; a police car or not; busted or not; on any
    /// tick, the track mirrored or not.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let car = Car(m.r32(HUMAN_RACE_CARS));
        SIM_COUNTER.set(m, pick(1000) & ![0, 0, 0, 3][pick(4) as usize]);
        GAME_SETUP.set_mirror_track(m, pick(3));
        HUD_BUSTED_OVERLAY.set(m, [0, 0, 0, 1][pick(4) as usize]);
        let pad = INPUT_SIM;
        pad.set_steering(m, pick(256) as i8);
        pad.set_gas(m, pick(256) as u8);
        pad.set_brake(m, pick(256) as u8);
        pad.set_flags(m, ((pick(16) << 3) | pick(8)) as u8);
        car.set_rs_control(m, [0, 0, 1][pick(3) as usize]);
        car.set_rs_gas_level(m, pick(256) as i8);
        car.set_rs_steering(m, pick(0x2_0000) - 0x1_0000);
        let control = car.control();
        control.set_lights(m, pick(256) as i8);
        control.set_gear(m, pick(8) as i8);
        for i in 0..2 {
            car.render().set_signal_light(m, i, pick(0x200) as i16);
        }
        car.update_ai_flags(m, |f| f ^ (pick(2) << 1));
        car.car_info(m).set_car_type(m, [pick(30), 22, 27, 21, 28][pick(5) as usize]);
        let redline = car.specs(m).redline(m);
        car.set_flywheel_rpm(m, redline + pick(2000) - 1000);
    }

    #[test]
    fn human_fuzz() {
        against_original("Control_Human", 10_000, 0xC0, shuffle, nfs4rc::f_80092ad4, &|c| vec![c.m.r32(HUMAN_RACE_CARS)], |c, a| {
            control_human(c, Car(a[0]));
            None
        });
    }
}
