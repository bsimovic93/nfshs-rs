//! AICOP.CPP: the police's set-up: their triggers (`<track>Tr<nn>.cop`, where roadblocks and
//! waiting police cars are) in a trigger manager, the spike belt, roadblocks and arrests reset;
//! and whether a police car is near a slice.

use crate::abi::{self, Saved};
use crate::aitrigger::init;
use crate::aiworld::apx_spline_distance_to_slice;
use crate::globals::{COP_CARS, GAME_SETUP, NUM_COP_CARS, PATHS, SPIKE_BELT};
use crate::types::{AITriggerTriggerManager, Car};
use ps1rt::{Cpu, Mem};

/// `triggerManagerCops`: the police's trigger manager (`AITrigger_TriggerManager *`).
pub const TRIGGER_MANAGER_COPS: u32 = 0x8013_D8D8;
/// `AICop_rawTriggers`: the trigger file, loaded.
pub const RAW_TRIGGERS: u32 = 0x8013_D860;
/// `AICop_gRoadBlockState`.
pub const ROAD_BLOCK_STATE: u32 = 0x8013_D864;
/// `AICop_numArrestedHumans`.
pub const NUM_ARRESTED_HUMANS: u32 = 0x8013_D868;

/// `Car_tObj.AIFlags`: the car is parked.
const AI_PARKED: i32 = 0x4;

/// No spike belt down, no roadblock, nobody arrested.
fn reset(m: &mut Mem) {
    m.w32(SPIKE_BELT.0, 0);
    m.w32(NUM_ARRESTED_HUMANS, 0);
    m.w32(ROAD_BLOCK_STATE, 0);
}

/// `AICop_StartUp` (AICOP.CPP, 0x800671ac): with the police on, their trigger manager made and
/// its triggers loaded (the file name made at sp+16; none when the file isn't there); reset.
pub fn start_up(c: &mut Cpu) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = GAME_SETUP.0;
        if GAME_SETUP.cops(&mut c.m) != 0 {
            let p = r.call(c, nfs4rc::f_800a48c0, &[0x34c], 0x8006_71d8); // operator new
            c.m.w32(TRIGGER_MANAGER_COPS, p);
            let (dir, track) = (c.m.r32(PATHS + 0x58), GAME_SETUP.track(&mut c.m) as u32);
            r.call(c, nfs4rc::f_800e50a8, &[sp + 16, 0x8005_59D0, dir, track], 0x8006_71fc); // sprintf "%sTr%02d.cop"
            let raw = r.call(c, nfs4rc::f_800e6394, &[sp + 16, 0], 0x8006_7208); // loadfileadrz
            c.m.w32(RAW_TRIGGERS, raw);
            let manager = AITriggerTriggerManager(c.m.r32(TRIGGER_MANAGER_COPS));
            r.enter(c, 0x8007_314c, 0x8006_7238, |c| init(c, manager, raw));
        }
        reset(&mut c.m);
    })
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(c);
}

/// `AICop_Restart` (AICOP.CPP, 0x80067258): the triggers loaded again (with the police on and
/// the file there); reset.
pub fn restart(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let raw = c.m.r32(RAW_TRIGGERS);
        if raw != 0 && GAME_SETUP.cops(&mut c.m) != 0 {
            let manager = AITriggerTriggerManager(c.m.r32(TRIGGER_MANAGER_COPS));
            Saved::of(c).enter(c, 0x8007_314c, 0x8006_728c, |c| init(c, manager, raw));
        }
        reset(&mut c.m);
    })
}

pub fn abi_restart(c: &mut Cpu) {
    restart(c);
}

/// `AICop_CleanUp` (AICOP.CPP, 0x800672a8): the trigger manager freed, and the file (with the
/// police on).
pub fn clean_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let manager = c.m.r32(TRIGGER_MANAGER_COPS);
        if manager != 0 {
            r.call(c, nfs4rc::f_800a4910, &[manager], 0x8006_72c4); // operator delete
            c.m.w32(TRIGGER_MANAGER_COPS, 0);
        }
        let raw = c.m.r32(RAW_TRIGGERS);
        if raw != 0 && GAME_SETUP.cops(&mut c.m) != 0 {
            r.call(c, nfs4rc::f_800e612c, &[raw], 0x8006_72f4); // purgememadr
            c.m.w32(RAW_TRIGGERS, 0);
        }
    })
}

pub fn abi_clean_up(c: &mut Cpu) {
    clean_up(c);
}

/// `AICop_NoCopsInArea` (AICOP.CPP, 0x80067308): no police car but parked ones within `dist`
/// of `slice` along the road.
pub fn no_cops_in_area(m: &mut Mem, slice: i32, dist: i32) -> bool {
    for i in 0..NUM_COP_CARS.get(m).max(0) as u32 {
        let cop = Car(m.r32(COP_CARS + 4 * i));
        if cop.ai_flags(m) & AI_PARKED == 0 && apx_spline_distance_to_slice(m, cop, slice).wrapping_abs() < dist {
            return false;
        }
    }
    true
}

pub fn abi_no_cops_in_area(c: &mut Cpu) {
    c.r[2] = no_cops_in_area(&mut c.m, c.r[4] as i32, c.r[5] as i32) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::{CARS_LIST, NUM_CARS};
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// The AI speeds' shuffled race with 0 to 3 police cars (any of the cars, parked or not);
    /// the police on or not, a trigger manager and file or none, any track; a spike belt,
    /// roadblock and arrests.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        NUM_COP_CARS.set(m, pick(4));
        for k in 0..3 {
            let car = Car(m.r32(CARS_LIST + 4 * pick(n) as u32));
            car.update_ai_flags(m, |f| f & !AI_PARKED | [0, AI_PARKED][pick(2) as usize]);
            m.w32(COP_CARS + 4 * k, car.0);
        }
        GAME_SETUP.set_cops(m, pick(2));
        GAME_SETUP.set_track(m, pick(20));
        m.w32(TRIGGER_MANAGER_COPS, [0, 0x8014_0000][pick(2) as usize]);
        m.w32(RAW_TRIGGERS, [0, 0x8014_2000][pick(2) as usize]);
        m.w32(0x8014_2000, pick(3) as u32);
        m.w32(SPIKE_BELT.0, pick(2) as u32);
        m.w32(ROAD_BLOCK_STATE, pick(3) as u32);
        m.w32(NUM_ARRESTED_HUMANS, pick(3) as u32);
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aicop_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 4] = [
            ("AICop_StartUp", nfs4rc::f_800671ac, |c, _| {
                start_up(c);
                None
            }),
            ("AICop_Restart", nfs4rc::f_80067258, |c, _| {
                restart(c);
                None
            }),
            ("AICop_CleanUp", nfs4rc::f_800672a8, |c, _| {
                clean_up(c);
                None
            }),
            ("AICop_NoCopsInArea", nfs4rc::f_80067308, |c, a| Some(no_cops_in_area(&mut c.m, a[0] as i32, a[1] as i32) as u32)),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0xC01 + k as u64, shuffle, original, &|c| {
                let r = crate::globals::SIM_COUNTER.get(&mut c.m) as u32;
                let slice = Car(c.m.r32(CARS_LIST)).n().sim_road_info().slice(&mut c.m) as u32;
                vec![slice.wrapping_add(r % 40).wrapping_sub(20), (r >> 8) % 0x100_0000]
            }, port);
        }
    }

    #[test]
    fn cop_stack_fuzz() {
        for (name, addr, original) in [("AICop_StartUp", 0x8006_71acu32, nfs4rc::f_800671ac as fn(&mut Cpu)), ("AICop_Restart", 0x8006_7258, nfs4rc::f_80067258), ("AICop_CleanUp", 0x8006_72a8, nfs4rc::f_800672a8)] {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 500, 0xC11, shuffle, &|_, _| vec![]);
        }
    }
}
