//! AIPERSON.CPP: the AI drivers' personalities (9, from `prsonal.bin`: each a pick from the
//! tables below), their scripted reactions (`scripts.bin`) and the catch-up glue (`glue.bin`,
//! `hhglue.bin` with one AI racer, `btcglue.bin` when a human drives a police car); each car
//! gets the personality its grid slot has, by its index mod 5.

use crate::abi::{self, Saved};
use crate::fixed::fixedmult;
use crate::globals::{CARS_LIST, GAME_SETUP, GLUE_TABLE, HUMAN_RACE_CARS, NUM_AI_RACE_CARS, NUM_CARS, NUM_COP_CARS, NUM_HUMAN_RACE_CARS, PATHS, PERSONALITY_NAMES};
use crate::types::{AIPerson, Car, UdffInfo};
use crate::udff::get_int;
use ps1rt::{Cpu, Mem};

/// `AIPerson_PersonalityData`: the 9 personalities (`AIPerson_t`).
pub const PERSONALITY_DATA: u32 = 0x8010_E3C4;
/// `AIPerson_ScriptData`: each personality's reactions (`AIScript_tReactionDetails[7]`, 56
/// bytes: per action 4 reactions and their half seconds).
pub const SCRIPT_DATA: u32 = 0x8010_E6B8;

// The tables a personality picks from (the file gives an index into each).
const BLOCK_MAX_DISTANCE: u32 = 0x8010_E228;
const BLOCK_MIN_DISTANCE: u32 = 0x8010_E238;
const ONCOMING_LOOK_AHEAD: u32 = 0x8010_E248;
/// 4 words each.
const LANE_SWERVE: u32 = 0x8010_E258;
const SWERVE_CHANGE_PROB: u32 = 0x8010_E298;
const BRAKE_MULTIPLIER: u32 = 0x8010_E2A8;
const ATTACK_ACTIVATION_HITS: u32 = 0x8010_E2B8;
const ATTACK_TIMES: u32 = 0x8010_E2C8;
const FISHTAIL_ANGLES: u32 = 0x8010_E2D8;
const GRIP_LOSS_PROB_PER_SECOND: u32 = 0x8010_E2E8;
const GRIP_LOSS_MIN_FACTOR: u32 = 0x8010_E2F8;
const GRIP_LOSS_RECOVERY_PER_TICK: u32 = 0x8010_E308;
/// By the file's index plus the weather.
const MINIMUM_WIPE_OUT_TICKS: u32 = 0x8010_E318;
const RANDOM_WIPE_OUT_TICKS: u32 = 0x8010_E32C;
const REAR_END_PROB_MASK: u32 = 0x8010_E340;
const SMACK_PROB_MASK: u32 = 0x8010_E350;
const BEST_LINE_ABILITIES: u32 = 0x8010_E360;

/// `Car_tObj.carFlags`: the car is a human's.
const HUMAN: i32 = 4;
/// `Car_tObj.carFlags`: the car is a police car.
const COP: i32 = 0x200;

fn table(m: &mut Mem, at: u32, i: i32) -> u32 {
    m.r32(at.wrapping_add((i as u32).wrapping_mul(4)))
}

/// `AIPerson_LoadGridAndSetPersonalityIndexes` (AIPERSON.CPP, 0x800691e8): each car's
/// personality index from its grid slot's setup.
pub fn load_grid_and_set_personality_indexes(m: &mut Mem) {
    for i in 0..NUM_CARS.get(m).max(0) as u32 {
        let p = GAME_SETUP.car_info(i).personality(m);
        Car(m.r32(CARS_LIST + 4 * i)).set_personality_index(m, p);
    }
}

pub fn abi_load_grid_and_set_personality_indexes(c: &mut Cpu) {
    load_grid_and_set_personality_indexes(&mut c.m);
}

/// `AIPerson_SetPersonality` (AIPERSON.CPP, 0x80069234): `car` drives with personality `index`
/// mod 5 (an AI car's driver named after personality `index`), its script that one's.
pub fn set_personality(c: &mut Cpu, car: Car, index: i32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = car.0;
        r.0[0] = index as u32;
        if car.car_flags(&mut c.m) & HUMAN == 0 {
            let driver = car.car_info(&mut c.m).0.wrapping_add(0x5c);
            let name = PERSONALITY_NAMES.wrapping_add((index as u32).wrapping_mul(8));
            r.call(c, nfs4rc::f_800e6714, &[driver, name], 0x8006_9278); // strcpy
        }
        let index = index.wrapping_rem(5);
        r.0[0] = index as u32;
        let m = &mut c.m;
        car.set_personality_index(m, index);
        car.set_personality(m, AIPerson(PERSONALITY_DATA.wrapping_add((index as u32).wrapping_mul(84))));
        let script = SCRIPT_DATA.wrapping_add((index as u32).wrapping_mul(56));
        r.enter(c, 0x8006_fef8, 0x8006_92e8, |c| crate::aiscript::assign(c, car.script(), script));
    })
}

pub fn abi_set_personality(c: &mut Cpu) {
    set_personality(c, Car(c.r[4]), c.r[5] as i32);
}

/// `AIPerson_SetPersonalityPointers` (AIPERSON.CPP, 0x800692fc): each car's personality set
/// from its index.
pub fn set_personality_pointers(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let mut i = 0;
        while i < NUM_CARS.get(&mut c.m) {
            let car = Car(c.m.r32(CARS_LIST + 4 * i as u32));
            i += 1;
            r.0[0] = CARS_LIST + 4 * i as u32;
            r.0[1] = i as u32;
            let index = car.personality_index(&mut c.m);
            r.enter(c, 0x8006_9234, 0x8006_9344, |c| set_personality(c, car, index));
        }
    })
}

pub fn abi_set_personality_pointers(c: &mut Cpu) {
    set_personality_pointers(c);
}

/// `AIPerson_LoadPersonalityData` (AIPERSON.CPP, 0x80069360): the 9 personalities from the
/// file (after a count): per personality 14 indexes into the tables, the eighth unused, the
/// last the police collisions' firmness itself (the loop's count and some indexes kept at
/// sp+16..36, the file at sp+80).
pub fn load_personality_data(c: &mut Cpu, file: UdffInfo) {
    abi::frame(c, 80, |c, sp| {
        let m = &mut c.m;
        m.w32(sp + 80, file.0);
        get_int(m, file);
        m.w32(sp + 16, 0);
        let mut p = AIPerson(PERSONALITY_DATA);
        while (m.r32(sp + 16) as i32) < 9 {
            let block_max = get_int(m, file);
            let block_min = get_int(m, file);
            let oncoming = get_int(m, file);
            let swerve = get_int(m, file);
            let brake = get_int(m, file);
            let attack = get_int(m, file);
            let fishtail = get_int(m, file);
            m.w32(sp + 20, fishtail as u32);
            get_int(m, file);
            let wipe_out = get_int(m, file);
            let grip = get_int(m, file);
            let best_line = get_int(m, file);
            m.w32(sp + 24, best_line as u32);
            let rear_end = get_int(m, file);
            m.w32(sp + 32, rear_end as u32);
            let smack = get_int(m, file);
            m.w32(sp + 36, smack as u32);
            let firmness = get_int(m, file);
            let v = table(m, BLOCK_MAX_DISTANCE, block_max);
            p.set_block_max_distance(m, v as i32);
            let v = table(m, BLOCK_MIN_DISTANCE, block_min);
            p.set_block_min_distance(m, v as i32);
            let v = table(m, ONCOMING_LOOK_AHEAD, oncoming);
            p.set_oncoming_look_ahead(m, v as i32);
            for k in 0..4 {
                let v = table(m, LANE_SWERVE, swerve.wrapping_mul(4).wrapping_add(k as i32));
                p.set_lane_swerve(m, k, v as i32);
            }
            let v = table(m, SWERVE_CHANGE_PROB, swerve);
            p.set_swerve_change_prob(m, v as i32);
            let v = table(m, BRAKE_MULTIPLIER, brake);
            p.set_brake_multiplier(m, v as i32);
            let v = table(m, ATTACK_ACTIVATION_HITS, attack);
            p.set_attack_activation_hits(m, v as i32);
            let v = table(m, ATTACK_TIMES, attack);
            p.set_attack_time(m, v as i32);
            let v = table(m, FISHTAIL_ANGLES, fishtail);
            p.set_fishtail_angle(m, v as i32);
            let at = wipe_out.wrapping_add(GAME_SETUP.weather(m));
            let v = table(m, MINIMUM_WIPE_OUT_TICKS, at);
            p.set_minimum_between_wipeout_ticks(m, v as i32);
            let at = wipe_out.wrapping_add(GAME_SETUP.weather(m));
            let v = table(m, RANDOM_WIPE_OUT_TICKS, at);
            p.set_random_between_wipeout_ticks(m, v as i32);
            let v = table(m, GRIP_LOSS_PROB_PER_SECOND, grip);
            p.set_grip_loss_prob_per_second(m, v as i32);
            let v = table(m, GRIP_LOSS_MIN_FACTOR, grip);
            p.set_grip_loss_min_factor(m, v as i32);
            let v = table(m, GRIP_LOSS_RECOVERY_PER_TICK, grip);
            p.set_grip_loss_recovery_per_tick(m, v as i32);
            let v = table(m, BEST_LINE_ABILITIES, best_line);
            p.set_best_line_ability(m, v as i32);
            let v = table(m, REAR_END_PROB_MASK, rear_end);
            p.set_rear_bump_prob_mask(m, v);
            let v = table(m, SMACK_PROB_MASK, smack);
            let i = m.r32(sp + 16).wrapping_add(1);
            m.w32(sp + 16, i);
            p.set_cop_collision_firmness(m, firmness as u32);
            p.set_smack_prob_mask(m, v);
            p = p.at(1);
        }
    })
}

pub fn abi_load_personality_data(c: &mut Cpu) {
    load_personality_data(c, UdffInfo(c.r[4]));
}

/// `AIPerson_LoadScriptData` (AIPERSON.CPP, 0x800696a4): the 9 personalities' reactions from
/// the file (after a count): per action, per reaction, its kind and its half seconds.
pub fn load_script_data(m: &mut Mem, file: UdffInfo) {
    get_int(m, file);
    for i in 0..9u32 {
        for action in 0..7u32 {
            for k in 0..4u32 {
                let at = SCRIPT_DATA + i * 56 + action * 8 + k;
                let reaction = get_int(m, file);
                m.w8(at, reaction as u8);
                let half_seconds = get_int(m, file);
                m.w8(at + 4, half_seconds as u8);
            }
        }
    }
}

pub fn abi_load_script_data(c: &mut Cpu) {
    load_script_data(&mut c.m, UdffInfo(c.r[4]));
}

/// Whether a human drives a police car, in a race with the police chasing (types 1 and 5).
fn human_cop(m: &mut Mem) -> bool {
    let kind = GAME_SETUP.race_type(m);
    if kind != 1 && kind != 5 {
        return false;
    }
    let first = Car(m.r32(HUMAN_RACE_CARS));
    if first.car_flags(m) & COP != 0 {
        return true;
    }
    NUM_HUMAN_RACE_CARS.get(m) == 2 && Car(m.r32(HUMAN_RACE_CARS + 4)).car_flags(m) & COP != 0
}

/// `AIPerson_LoadGlue` (AIPERSON.CPP, 0x80069780): the glue table from the file (after a
/// count); with police cars about but no human in one, each factor 15% further from 1.
pub fn load_glue(c: &mut Cpu, file: UdffInfo) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = file.0;
        get_int(&mut c.m, file);
        r.call(c, nfs4rc::f_800bc910, &[file.0, GLUE_TABLE, 84], 0x8006_97ac); // Udff_GetBuffer
        let m = &mut c.m;
        if NUM_COP_CARS.get(m) == 0 || human_cop(m) {
            return;
        }
        for i in 0..21 {
            let at = GLUE_TABLE + 4 * i;
            let v = m.r32(at) as i32;
            if v != 0x1_0000 {
                m.w32(at, fixedmult(v, if v > 0x1_0000 { 0x1_2666 } else { 0xD999 }) as u32);
            }
        }
    })
}

pub fn abi_load_glue(c: &mut Cpu) {
    load_glue(c, UdffInfo(c.r[4]));
}

/// `AIPerson_Startup` (AIPERSON.CPP, 0x8006988c): the grid's personality indexes; the
/// personalities, scripts and glue loaded (the file names made at sp+16); each car's
/// personality set.
pub fn startup(c: &mut Cpu) {
    abi::frame(c, 144, |c, sp| {
        let mut r = Saved::of(c);
        load_grid_and_set_personality_indexes(&mut c.m);
        r.0[1] = PATHS;
        let dir = c.m.r32(PATHS + 8);
        let name = sp + 16;
        // (the file `format` names, made, opened, read by `port` (at `addr`) and closed; the
        // calls returning to `at`. The scratch registers flow from call to call as in the
        // original: sprintf homes a3, the last one's what Close left.)
        let load = |c: &mut Cpu, r: &mut Saved, format: u32, at: [u32; 4], port: fn(&mut Cpu, UdffInfo), addr: u32| {
            r.call_leaving(c, nfs4rc::f_800e50a8, &[name, format, dir], at[0]); // sprintf
            let file = r.call_leaving(c, nfs4rc::f_800bc7dc, &[name, 0, 1], at[1]); // Udff_Opena
            r.0[0] = file;
            r.enter(c, addr, at[2], |c| port(c, UdffInfo(file)));
            r.call_leaving(c, nfs4rc::f_800bc8a4, &[file], at[3]); // Udff_Close
        };
        load(c, &mut r, 0x8005_5B24, [0x8006_98bc, 0x8006_98cc, 0x8006_98d8, 0x8006_98e0], load_personality_data, 0x8006_9360);
        load(c, &mut r, 0x8005_5B34, [0x8006_98f4, 0x8006_9904, 0x8006_9910, 0x8006_9918], |c, f| load_script_data(&mut c.m, f), 0x8006_96a4);
        let m = &mut c.m;
        let glue = if human_cop(m) {
            0x8005_5B44 // "%sbtcglue.bin"
        } else if (GAME_SETUP.race_type(m) as u32) < 2 && NUM_AI_RACE_CARS.get(m) == 1 {
            0x8005_5B54 // "%shhglue.bin"
        } else {
            0x8005_5B64 // "%sglue.bin"
        };
        load(c, &mut r, glue, [0x8006_99f0, 0x8006_9a00, 0x8006_9a0c, 0x8006_9a14], load_glue, 0x8006_9780);
        r.enter(c, 0x8006_92fc, 0x8006_9a1c, set_personality_pointers);
    })
}

pub fn abi_startup(c: &mut Cpu) {
    startup(c);
}

/// `AIPerson_Cleanup` (AIPERSON.CPP, 0x80069a30): nothing.
pub fn abi_cleanup(_: &mut Cpu) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused: a data file's `Udff_tInfo` and its words.
    const FILE: u32 = 0x8014_3200;
    const WORDS: u32 = 0x8014_4000;

    /// The AI speeds' shuffled race; each grid slot's personality and each car's index anything
    /// near, cars a human's or not; a file of small indexes (some not) or of glue about 1, the
    /// police on or not, humans in police cars, any race type and weather.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        for i in 0..9 {
            GAME_SETUP.car_info(i).set_personality(m, [pick(9), pick(5), pick(40) - 20][pick(3) as usize]);
        }
        let n = NUM_CARS.get(m) as u32;
        for i in 0..n {
            let car = Car(m.r32(CARS_LIST + 4 * i));
            car.set_personality_index(m, [pick(9), pick(30) - 10][pick(2) as usize]);
            let flags = [0, HUMAN][pick(2) as usize] | if pick(4) == 0 { COP } else { 0 };
            car.update_car_flags(m, |f| f & !(HUMAN | COP) | flags);
        }
        UdffInfo(FILE).set_mem_ptr(m, WORDS);
        for k in 0..800 {
            m.w32(WORDS + 4 * k, [pick(4), pick(3), pick(256), pick(0x1_0000)][pick(4) as usize] as u32);
        }
        // (the glue file's table, after its count)
        if pick(2) == 0 {
            for i in 0..21 {
                m.w32(WORDS + 4 + 4 * i, [0x1_0000, 0xC000 + pick(0x8000), pick(0x4_0000)][pick(3) as usize] as u32);
            }
        }
        NUM_COP_CARS.set(m, pick(3));
        GAME_SETUP.set_race_type(m, pick(7));
        GAME_SETUP.set_weather(m, pick(2));
        NUM_HUMAN_RACE_CARS.set(m, 1 + pick(2));
        NUM_AI_RACE_CARS.set(m, pick(3));
        for k in 0..2 {
            let car = Car(m.r32(CARS_LIST + 4 * pick(n) as u32));
            m.w32(HUMAN_RACE_CARS + 4 * k, car.0);
        }
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aiperson_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 6] = [
            ("LoadGridAndSetPersonalityIndexes", nfs4rc::f_800691e8, |c, _| {
                load_grid_and_set_personality_indexes(&mut c.m);
                None
            }),
            ("SetPersonality", nfs4rc::f_80069234, |c, a| {
                set_personality(c, Car(a[0]), a[1] as i32);
                None
            }),
            ("SetPersonalityPointers", nfs4rc::f_800692fc, |c, _| {
                set_personality_pointers(c);
                None
            }),
            ("LoadPersonalityData", nfs4rc::f_80069360, |c, a| {
                load_personality_data(c, UdffInfo(a[0]));
                None
            }),
            ("LoadScriptData", nfs4rc::f_800696a4, |c, a| {
                load_script_data(&mut c.m, UdffInfo(a[0]));
                None
            }),
            ("LoadGlue", nfs4rc::f_80069780, |c, a| {
                load_glue(c, UdffInfo(a[0]));
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x9E1 + k as u64, shuffle, original, &|c| {
                if name == "SetPersonality" {
                    let car = Car(c.m.r32(CARS_LIST));
                    vec![car.0, car.personality_index(&mut c.m) as u32]
                } else {
                    vec![FILE]
                }
            }, port);
        }
    }

    /// The stack the start-up (the files made, opened, read and closed) leaves.
    #[test]
    fn person_stack_fuzz() {
        same_stack("AIPerson_Startup", 0x8006_988c, nfs4rc::f_8006988c, 0x8006_0000, SCRATCH_TOP, 300, 0x9F1, shuffle, &|_, _| vec![]);
        same_stack("AIPerson_SetPersonalityPointers", 0x8006_92fc, nfs4rc::f_800692fc, 0x8006_0000, SCRATCH_TOP, 500, 0x9F2, shuffle, &|_, _| vec![]);
        same_stack("AIPerson_LoadPersonalityData", 0x8006_9360, nfs4rc::f_80069360, 0x8006_0000, SCRATCH_TOP, 500, 0x9F3, shuffle, &|_, _| vec![FILE]);
        same_stack("AIPerson_LoadGlue", 0x8006_9780, nfs4rc::f_80069780, 0x8006_0000, SCRATCH_TOP, 500, 0x9F4, shuffle, &|_, _| vec![FILE]);
    }
}
