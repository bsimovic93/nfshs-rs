//! AIINIT.CPP: the AI's start-up, reset and clean-up (the other AI files' in order, the traffic
//! triggers, the AI physics' configuration from `trafcfg.dat`), and an AI car's set-up from its
//! car file: top speeds, acceleration table, curve speeds (for the cars whose files have them),
//! its upgrades.

use crate::abi::{self, Saved};
use crate::aitrigger::init;
use crate::fixed::{fixeddiv, fixedmult};
use crate::globals::{
    AIPHYSIC_CONFIG, AI_RACE_CARS, CARS_LIST, GAME_SETUP, HUMAN_RACE_CARS, INVERSE_LANE_WIDTH_TABLE, LEADER_BOARD, NUM_CARS, PATHS,
};
use crate::types::{AIDataRecord, AITriggerTriggerManager, Car, UdffInfo};
use crate::udff::get_int;
use ps1rt::{Cpu, Mem};

/// `triggerManagerTraffic`: the traffic's trigger manager (`AITrigger_TriggerManager *`).
pub const TRIGGER_MANAGER_TRAFFIC: u32 = 0x8013_D8DC;
/// `AITraffic_rawTriggers`: the traffic trigger file, loaded.
pub const TRAFFIC_RAW_TRIGGERS: u32 = 0x8013_D86C;
/// `AIInit_useSpreadForce`: the cars pushed apart (in every race but type 3).
pub const USE_SPREAD_FORCE: u32 = 0x8013_D870;
/// `AIInit_forceHumanHandBrake`.
pub const FORCE_HUMAN_HAND_BRAKE: u32 = 0x8013_D874;
/// `AI_Info` (AI.CPP), its words the reset clears (all but +0x40).
const AI_INFO: u32 = 0x8010_D94C;
/// `trafcfg`: the AI physics' configuration file, in the executable.
const TRAFCFG: u32 = 0x8010_E1BC;
/// `AITune_accelerationScale`: an AI racer's acceleration by car type.
const ACCELERATION_SCALE: u32 = 0x8010_F1C8;
/// `AIInit_IsNonStandardCarFile`'s table (50 words, by car type).
const NON_STANDARD_CAR_FILES: u32 = 0x8005_5A0C;

/// `Car_tObj.carFlags`: an AI car, an AI racer.
const CAR_AI: i32 = 0x2;
const CAR_AI_RACER: i32 = 0x8;

fn car_at(m: &mut Mem, i: i32) -> Car {
    Car(m.r32(CARS_LIST + 4 * i as u32))
}

/// Each car's script started (the loop's registers in `r`: s0 the count, s1 the next car's
/// slot), the calls returning to `ra`.
fn start_scripts(c: &mut Cpu, r: &mut Saved, ra: u32) {
    r.0[0] = 0;
    let mut i = 0;
    while i < NUM_CARS.get(&mut c.m) {
        let script = car_at(&mut c.m, i).script();
        r.0[1] = CARS_LIST + 4 * (i as u32 + 1);
        r.enter(c, 0x8006_ff3c, ra, |c| crate::aiscript::startup(&mut c.m, script));
        i += 1;
        r.0[0] = i as u32;
    }
}

/// `AIInit_StartUp1` (AIINIT.CPP, 0x800673b0): the traffic set up; the inverse lane widths
/// (1 / (i/4 m)); the tuning and the data records started.
pub fn start_up1(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.enter(c, 0x8006_770c, 0x8006_73c4, traffic_start_up);
        let m = &mut c.m;
        m.w32(INVERSE_LANE_WIDTH_TABLE, 0);
        for i in 1..80u32 {
            m.w32(INVERSE_LANE_WIDTH_TABLE + 4 * i, fixeddiv(0x1_0000, (i << 14) as i32) as u32);
        }
        r.0[0] = 80;
        r.0[1] = INVERSE_LANE_WIDTH_TABLE + 4 * 80;
        r.enter(c, 0x8007_3820, 0x8006_7400, |c| crate::aitune::start_up1(&mut c.m));
        r.enter(c, 0x8006_d73c, 0x8006_7408, crate::aidatarecord::start_up1);
        // (AI_StartUp: nothing)
    })
}

pub fn abi_start_up1(c: &mut Cpu) {
    start_up1(c);
}

/// `AIInit_StartUp2` (AIINIT.CPP, 0x80067424): the speeds, the configuration, each car's
/// script, the personalities and the records' second part started; each car's AI physics and
/// AI set up.
pub fn start_up2(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.enter(c, 0x8006_ddb4, 0x8006_743c, crate::aispeeds::start_up);
        r.0[0] = 0;
        r.enter(c, 0x8006_77f8, 0x8006_7444, load_configs);
        start_scripts(c, &mut r, 0x8006_746c);
        r.0[2] = 0;
        r.enter(c, 0x8006_988c, 0x8006_748c, crate::aiperson::startup);
        r.enter(c, 0x8006_d7a8, 0x8006_7494, crate::aidatarecord::start_up2);
        // (AIPhysic_StartUp, AITune_StartUp2: nothing)
        let mut i = 0;
        while i < NUM_CARS.get(&mut c.m) {
            let car = car_at(&mut c.m, i);
            r.0[0] = car.0;
            r.0[1] = CARS_LIST + 4 * (i as u32 + 1);
            r.0[2] = i as u32 + 1;
            r.enter(c, 0x8006_d460, 0x8006_74d4, |c| crate::aiphysic::init_car(c, car));
            r.enter(c, 0x8006_7d68, 0x8006_74dc, |c| init_ai_car2(c, car));
            i += 1;
        }
    })
}

pub fn abi_start_up2(c: &mut Cpu) {
    start_up2(c);
}

/// `AIInit_Reset1` (AIINIT.CPP, 0x800674fc): no forced hand brake.
pub fn reset1(m: &mut Mem) {
    m.w32(FORCE_HUMAN_HAND_BRAKE, 0);
}

pub fn abi_reset1(c: &mut Cpu) {
    reset1(&mut c.m);
}

/// `AIInit_Reset2` (AIINIT.CPP, 0x80067508): the cars spread apart (not in race type 3); each
/// script started again; the leader board's first human and AI racer; the AI's lane info
/// cleared.
pub fn reset2(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let spread = (GAME_SETUP.race_type(&mut c.m) != 3) as u32;
        c.m.w32(USE_SPREAD_FORCE, spread);
        start_scripts(c, &mut r, 0x8006_7568);
        let m = &mut c.m;
        let (human, ai) = (m.r32(HUMAN_RACE_CARS), m.r32(AI_RACE_CARS));
        m.w32(LEADER_BOARD + 4, human);
        m.w32(LEADER_BOARD, human);
        m.w32(LEADER_BOARD + 12, ai);
        m.w32(LEADER_BOARD + 8, ai);
        // (AIPhysic_Reset: nothing)
        for k in [8, 4, 0, 20, 16, 12, 32, 28, 24, 44, 40, 36, 56, 52, 48, 60, 68] {
            m.w32(AI_INFO + k, 0);
        }
    })
}

pub fn abi_reset2(c: &mut Cpu) {
    reset2(c);
}

/// `AIInit_CleanUp1` (AIINIT.CPP, 0x8006760c): the records' first part and the traffic cleaned
/// up.
pub fn clean_up1(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        // (AI_CleanUp, AITune_CleanUp1: nothing)
        r.enter(c, 0x8006_d814, 0x8006_7624, crate::aidatarecord::clean_up1);
        r.enter(c, 0x8006_77a8, 0x8006_7634, traffic_clean_up);
    })
}

pub fn abi_clean_up1(c: &mut Cpu) {
    clean_up1(c);
}

/// `AIInit_CleanUp2` (AIINIT.CPP, 0x80067644): each car's AI physics freed; the speeds cleaned
/// up (the rest's clean-ups do nothing).
pub fn clean_up2(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = 0;
        let mut i = 0;
        while i < NUM_CARS.get(&mut c.m) {
            let car = car_at(&mut c.m, i);
            i += 1;
            r.0[0] = CARS_LIST + 4 * i as u32;
            r.0[1] = i as u32;
            r.enter(c, 0x8006_d5d4, 0x8006_7684, |c| crate::aiphysic::de_init_car(c, car));
        }
        let n = NUM_CARS.get(&mut c.m);
        r.0[0] = n.max(0) as u32;
        r.enter(c, 0x8006_e070, 0x8006_76f8, crate::aispeeds::clean_up);
    })
}

pub fn abi_clean_up2(c: &mut Cpu) {
    clean_up2(c);
}

/// `AI_TrafficStartUp` (AIINIT.CPP, 0x8006770c): with traffic (any density), its trigger manager made and
/// its triggers loaded (`<track>Tr<nn>.trf`, the name made at sp+16).
pub fn traffic_start_up(c: &mut Cpu) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = GAME_SETUP.0;
        if GAME_SETUP.traffic_density(&mut c.m) == 0 {
            return;
        }
        let p = r.call(c, nfs4rc::f_800a48c0, &[0x34c], 0x8006_7738); // operator new
        c.m.w32(TRIGGER_MANAGER_TRAFFIC, p);
        let (dir, track) = (c.m.r32(PATHS + 0x48), GAME_SETUP.track(&mut c.m) as u32);
        r.call(c, nfs4rc::f_800e50a8, &[sp + 16, 0x8005_59EC, dir, track], 0x8006_775c); // sprintf "%sTr%02d.trf"
        let raw = r.call(c, nfs4rc::f_800e6394, &[sp + 16, 0], 0x8006_7768); // loadfileadrz
        c.m.w32(TRAFFIC_RAW_TRIGGERS, raw);
        let manager = AITriggerTriggerManager(c.m.r32(TRIGGER_MANAGER_TRAFFIC));
        r.enter(c, 0x8007_314c, 0x8006_7798, |c| init(c, manager, raw));
    })
}

pub fn abi_traffic_start_up(c: &mut Cpu) {
    traffic_start_up(c);
}

/// `AI_TrafficCleanUp` (AIINIT.CPP, 0x800677a8): the traffic's trigger manager and file freed.
pub fn traffic_clean_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let manager = c.m.r32(TRIGGER_MANAGER_TRAFFIC);
        if manager != 0 {
            r.call(c, nfs4rc::f_800a4910, &[manager], 0x8006_77c4); // operator delete
            c.m.w32(TRIGGER_MANAGER_TRAFFIC, 0);
        }
        let raw = c.m.r32(TRAFFIC_RAW_TRIGGERS);
        if raw != 0 {
            r.call(c, nfs4rc::f_800e612c, &[raw], 0x8006_77e4); // purgememadr
            c.m.w32(TRAFFIC_RAW_TRIGGERS, 0);
        }
    })
}

pub fn abi_traffic_clean_up(c: &mut Cpu) {
    traffic_clean_up(c);
}

/// `AIInit_LoadConfigs` (AIINIT.CPP, 0x800677f8): the AI physics' configuration read from the
/// executable's copy of `trafcfg.dat` (its name made at sp+16, unused). The scratch registers
/// flow from call to call as in the original (sprintf homes a3).
pub fn load_configs(c: &mut Cpu) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        let dir = c.m.r32(PATHS + 8);
        r.call_leaving(c, nfs4rc::f_800e50a8, &[sp + 16, 0x8005_59FC, dir], 0x8006_781c); // sprintf "%strafcfg.dat"
        let file = r.call_leaving(c, nfs4rc::f_800bc7dc, &[0, TRAFCFG, 1], 0x8006_7830); // Udff_Opena
        r.0[0] = file;
        r.enter(c, 0x8006_7854, 0x8006_783c, |c| load_physics_config(&mut c.m, UdffInfo(file)));
        r.call_leaving(c, nfs4rc::f_800bc8a4, &[file], 0x8006_7844); // Udff_Close
    })
}

pub fn abi_load_configs(c: &mut Cpu) {
    load_configs(c);
}

/// `AIInit_LoadPhysicsConfig` (AIINIT.CPP, 0x80067854): the AI physics' configuration from the
/// file: 5 words, then 11 per physics model (2).
pub fn load_physics_config(m: &mut Mem, file: UdffInfo) {
    let config = AIPHYSIC_CONFIG.0;
    for k in 0..5 {
        let v = get_int(m, file);
        m.w32(config + 4 * k, v as u32);
    }
    for model in 0..2 {
        let at = config + if model == 0 { 0x14 } else { 0x40 };
        for k in 0..11 {
            let v = get_int(m, file);
            m.w32(at + 4 * k, v as u32);
        }
    }
}

pub fn abi_load_physics_config(c: &mut Cpu) {
    load_physics_config(&mut c.m, UdffInfo(c.r[4]));
}

/// `AIInit_ClearAICar` (AIINIT.CPP, 0x80067974): the car not spinning (the zero vector made at
/// sp+16), not flying, skidding, wiping out or fishtailing, its timers off, driving forward.
pub fn clear_ai_car(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 12], 0x8006_7994); // memset
        let m = &mut c.m;
        let v = crate::types::Coord(sp + 16).get(m);
        car.n().angular_vel().set(m, v);
        car.n().set_flight_time(m, 0);
        car.set_front_skid(m, 0);
        car.set_rear_skid(m, 0);
        car.set_tailgate_timer(m, 0);
        car.set_ai_shift_timer(m, 0);
        car.set_drive_direction(m, 1);
        car.set_drive_direction_timer(m, 0);
        car.set_drive_direction_reverse_time(m, 0);
        car.set_wipe_out_end_tick(m, 0);
        car.set_wipe_out_start_tick(m, 0);
        car.set_ai_fishtail_end_tick(m, 0);
    })
}

pub fn abi_clear_ai_car(c: &mut Cpu) {
    clear_ai_car(c, Car(c.r[4]));
}

/// `AIInit_RestartAICar` (AIINIT.CPP, 0x800679ec): the car's AI state from the start: going
/// the track's way, no targets, lanes, speeds, slack, timers or nitrous; its own life basis;
/// no glue, full cop speed, lane 7; an AI car's simulation optimisable, another's not; a
/// parked one inactive; no caravan; full grip and damage.
pub fn restart_ai_car(m: &mut Mem, car: Car) {
    let dir = if GAME_SETUP.reverse_track(m) != 0 { -1 } else { 1 };
    car.set_direction(m, dir);
    car.set_desired_direction(m, dir);
    let target = car.target_pos();
    target.set_z(m, 0);
    target.set_y(m, 0);
    target.set_x(m, 0);
    car.set_target_lat_pos(m, 0);
    car.set_preferred_lateral_position(m, 0);
    car.set_preferred_lateral_position_power(m, 0);
    car.set_car_in_lane(m, 0);
    car.set_drive_direction(m, 1);
    car.set_drive_direction_timer(m, 0);
    car.set_drive_direction_reverse_time(m, 0);
    car.set_barrier_think_harder(m, 0);
    car.set_desired_lat_pos(m, 0);
    car.set_desired_speed(m, 0);
    car.set_original_desired_speed(m, 0);
    car.set_current_speed(m, 0);
    car.set_speed(m, 0);
    car.set_lane_slack(m, 0);
    car.set_road_position(m, 0);
    car.set_ai_flags(m, 0);
    car.set_road_span(m, 0);
    car.set_basis_car(m, car);
    car.set_ai_shift_timer(m, 0);
    car.set_tailgate_timer(m, 0);
    car.set_physics_model_timer(m, 0);
    car.set_traffic_speed_randomizing_factor(m, 0);
    car.set_lane_change_speed(m, 0);
    car.set_time_off_road(m, 0);
    car.set_ramp_desired_lat_pos(m, 0);
    car.set_acc_nitrous(m, 0x1_0000);
    car.set_speed_nitrous(m, 0x1_0000);
    m.w32(car.speech_info().0, 0);
    car.set_wipe_out_end_tick(m, 0);
    car.set_btc_glue_modifier(m, 0x1_0000);
    car.set_cop_acc_mult(m, 0x1_0000);
    car.set_cop_top_speed(m, 0x64_0000);
    car.set_donut_mode(m, 0);
    car.set_lane_index(m, 7);
    car.set_ai_fishtail_end_tick(m, 0);
    car.set_look_ahead_slice(m, 0);
    let ai = car.car_flags(m) & CAR_AI != 0;
    car.set_force_no_sim_optz(m, (!ai) as i32);
    car.set_wipe_out_start_tick(m, 0);
    if car.car_flags(m) & (CAR_AI | CAR_AI_RACER) == CAR_AI {
        car.n().set_active(m, 0);
    }
    car.set_caravan_timer(m, -1);
    car.set_fall_behind_car(m, Car(0));
    car.set_next_ai_racer(m, Car(0));
    car.set_caravan_follow_behind_distance_meters(m, 0);
    car.set_ai_slot(m, 0);
    car.set_grip_factor(m, 0x1_0000);
    car.set_damage_mult(m, 0x1_0000);
    car.set_extra_wall_collision_allowance(m, 0);
}

pub fn abi_restart_ai_car(c: &mut Cpu) {
    restart_ai_car(&mut c.m, Car(c.r[4]));
}

/// `AIInit_InitAICar` (AIINIT.CPP, 0x80067b14): the car's AI data from its car file: red line,
/// 7 gears' top speeds (and the inverses of their differences), acceleration table, shift
/// time, grip limits; its acceleration record (an AI racer's scaled by its car type), its
/// curve speeds (the cars whose files have them), its speed factor and slack chance; no
/// upgrades, full grip and damage.
pub fn init_ai_car(c: &mut Cpu, car: Car, file: UdffInfo) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = car.0;
        r.0[4] = file.0;
        r.0[5] = 0x1_0000;
        let v = get_int(&mut c.m, file);
        car.set_red_line(&mut c.m, v);
        let top = car.0.wrapping_add(0x598);
        r.call(c, nfs4rc::f_800bc910, &[file.0, top, 28], 0x8006_7b5c); // Udff_GetBuffer
        let m = &mut c.m;
        for i in 0..7u32 {
            let inv = if i == 0 {
                fixeddiv(0x1_0000, car.top_speeds(m, 0))
            } else {
                let v = car.top_speeds(m, i);
                if v == 0 { 0 } else { fixeddiv(0x1_0000, v.wrapping_sub(car.top_speeds(m, i - 1))) }
            };
            car.set_inv_top_speeds(m, i, inv);
        }
        r.0[0] = car.0.wrapping_add(0x5d0);
        r.0[1] = 7;
        r.0[3] = 24;
        let table = car.0.wrapping_add(0x5d0);
        r.call(c, nfs4rc::f_800bc910, &[file.0, table, 224], 0x8006_7bd8);
        let m = &mut c.m;
        let v = get_int(m, file);
        car.set_ai_shift_duration(m, v);
        let v = get_int(m, file);
        car.set_max_clacc(m, v);
        let v = get_int(m, file);
        car.set_max_aa(m, v);
        let car_type = |m: &mut Mem| car.car_info(m).car_type(m);
        if car.car_flags(m) & CAR_AI_RACER != 0 {
            let t = car_type(m);
            r.0[5] = m.r32(ACCELERATION_SCALE.wrapping_add((t as u32).wrapping_mul(4)));
        }
        let p = AIDataRecord(r.call(c, nfs4rc::f_800a48c0, &[92], 0x8006_7c30)); // operator new
        let scale = r.0[5] as i32;
        let record = r.enter(c, 0x8006_d970, 0x8006_7c44, |c| crate::aidatarecord::acc_table_construct(c, p, table, scale));
        car.set_acceleration_record(&mut c.m, record.0);
        let t = car_type(&mut c.m);
        if r.enter(c, 0x8006_7dd8, 0x8006_7c58, |c| is_non_standard_car_file(c, t)) != 0 {
            let p = AIDataRecord(r.call(c, nfs4rc::f_800a48c0, &[88], 0x8006_7c68));
            let curves = r.enter(c, 0x8006_dbbc, 0x8006_7c78, |c| crate::aidatarecord::curve_speed_table_construct(c, p, car.0.wrapping_add(0x240), 7));
            car.set_curve_speed_table(&mut c.m, curves.0);
        } else {
            car.set_curve_speed_table(&mut c.m, 0);
        }
        let m = &mut c.m;
        let v = fixedmult(car.max_clacc(m), 0x1_3333);
        car.set_max_clacc(m, v);
        let v = get_int(m, file);
        car.set_speed_factor(m, v);
        let v = get_int(m, file);
        car.set_slack_prob(m, v);
        car.set_grip_factor(m, 0x1_0000);
        car.set_top_speed_upgrade_mult(m, 0);
        car.set_acc_upgrade_mult(m, 0);
        car.set_damage_mult(m, 0x1_0000);
    })
}

pub fn abi_init_ai_car(c: &mut Cpu) {
    init_ai_car(c, Car(c.r[4]), UdffInfo(c.r[5]));
}

/// `AIInit_DeInitAICar` (AIINIT.CPP, 0x80067ce8): the car's curve speeds and acceleration
/// record destroyed (their virtual destructors, freeing them).
pub fn de_init_ai_car(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        for (at, ra) in [(0x6b4u32, 0x8006_7d24u32), (0x6b0, 0x8006_7d54)] {
            let obj = c.m.r32(car.0.wrapping_add(at));
            if obj != 0 {
                let (this, f) = crate::aidatarecord::virtual_fn(&mut c.m, obj, 1);
                r.call_ptr(c, f, &[this, 3], ra);
                c.m.w32(car.0.wrapping_add(at), 0);
            }
        }
    })
}

pub fn abi_de_init_ai_car(c: &mut Cpu) {
    de_init_ai_car(c, Car(c.r[4]));
}

/// `AIInit_InitAICar2` (AIINIT.CPP, 0x80067d68): the car's upgrades: an AI racer's curve speeds
/// by its handling, everyone's acceleration and top speed multipliers.
pub fn init_ai_car2(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car.0;
        if car.car_flags(&mut c.m) & CAR_AI_RACER != 0 {
            let index = car.car_index(&mut c.m);
            let mult = r.enter(c, 0x8006_fa84, 0x8006_7d98, |c| crate::aispeeds::get_upgrade_handling_mult(&mut c.m, index));
            let curves = AIDataRecord(car.curve_speed_table(&mut c.m));
            r.enter(c, 0x8006_dc80, 0x8006_7da4, |c| crate::aidatarecord::curve_speed_table_upgrade(&mut c.m, curves, mult));
        }
        let index = car.car_index(&mut c.m);
        let acc = r.enter(c, 0x8006_f90c, 0x8006_7db0, |c| crate::aispeeds::get_upgrade_acc_mult(&mut c.m, index));
        car.set_acc_upgrade_mult(&mut c.m, acc);
        let index = car.car_index(&mut c.m);
        let top = r.enter(c, 0x8006_fb40, 0x8006_7dbc, |c| crate::aispeeds::get_upgrade_top_speed_mult(&mut c.m, index));
        car.set_top_speed_upgrade_mult(&mut c.m, top);
    })
}

pub fn abi_init_ai_car2(c: &mut Cpu) {
    init_ai_car2(c, Car(c.r[4]));
}

/// `AIInit_DeInitAICar2` (AIINIT.CPP, 0x80067dd0): nothing.
pub fn abi_nothing(_: &mut Cpu) {}

/// `AIInit_IsNonStandardCarFile` (AIINIT.CPP, 0x80067dd8): whether car type `t`'s file has curve
/// speeds (nonzero; the table copied into its frame first; past the 50, no).
pub fn is_non_standard_car_file(c: &mut Cpu, t: i32) -> u32 {
    abi::frame(c, 200, |c, sp| {
        let m = &mut c.m;
        for k in 0..50 {
            let v = m.r32(NON_STANDARD_CAR_FILES + 4 * k);
            m.w32(sp + 4 * k, v);
        }
        if t < 50 { m.r32(sp.wrapping_add((t as u32).wrapping_mul(4))) } else { 0 }
    })
}

pub fn abi_is_non_standard_car_file(c: &mut Cpu) {
    c.r[2] = is_non_standard_car_file(c, c.r[4] as i32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused: a car file's `Udff_tInfo` and its words; the arguments (a car,
    /// a car type).
    const FILE: u32 = 0x8014_3200;
    const WORDS: u32 = 0x8014_4000;
    const ARGS: u32 = 0x8014_3300;

    /// The AI speeds' shuffled race; cars of any kind (AI, AI racer, both, neither), the track
    /// either way, any race type and traffic density; a car file (top speeds rising, some zero,
    /// any other words); a car and a car type to call with.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::aispeeds::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let n = NUM_CARS.get(m) as u32;
        for i in 0..n {
            let car = Car(m.r32(CARS_LIST + 4 * i));
            let kind = [0, CAR_AI, CAR_AI_RACER, CAR_AI | CAR_AI_RACER][pick(4) as usize];
            car.update_car_flags(m, |f| f & !(CAR_AI | CAR_AI_RACER) | kind);
        }
        GAME_SETUP.set_reverse_track(m, pick(2));
        GAME_SETUP.set_race_type(m, pick(6));
        GAME_SETUP.set_traffic_density(m, pick(3));
        UdffInfo(FILE).set_mem_ptr(m, WORDS);
        let mut top = 0;
        for k in 0..100 {
            let v = if (1..8).contains(&k) {
                top += [0, pick(0x20_0000)][pick(4).min(1) as usize];
                top
            } else {
                [pick(0x2_0000), pick(0x100_0000), 0][pick(3) as usize]
            };
            m.w32(WORDS + 4 * k, v as u32);
        }
        let car = m.r32(CARS_LIST + 4 * pick(n) as u32);
        m.w32(ARGS, car);
        m.w32(ARGS + 4, (pick(60) - 5) as u32);
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aiinit_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 15] = [
            ("AIInit_Reset1", nfs4rc::f_800674fc, |c, _| {
                reset1(&mut c.m);
                None
            }),
            ("AIInit_Reset2", nfs4rc::f_80067508, |c, _| {
                reset2(c);
                None
            }),
            ("AIInit_StartUp1", nfs4rc::f_800673b0, |c, _| {
                start_up1(c);
                None
            }),
            ("AIInit_CleanUp2", nfs4rc::f_80067644, |c, _| {
                clean_up2(c);
                None
            }),
            ("AI_TrafficStartUp", nfs4rc::f_8006770c, |c, _| {
                traffic_start_up(c);
                None
            }),
            ("AI_TrafficCleanUp", nfs4rc::f_800677a8, |c, _| {
                traffic_clean_up(c);
                None
            }),
            ("AIInit_LoadPhysicsConfig", nfs4rc::f_80067854, |c, a| {
                load_physics_config(&mut c.m, UdffInfo(a[0]));
                None
            }),
            ("AIInit_LoadConfigs", nfs4rc::f_800677f8, |c, _| {
                load_configs(c);
                None
            }),
            ("AIInit_ClearAICar", nfs4rc::f_80067974, |c, a| {
                clear_ai_car(c, Car(a[1]));
                None
            }),
            ("AIInit_RestartAICar", nfs4rc::f_800679ec, |c, a| {
                restart_ai_car(&mut c.m, Car(a[1]));
                None
            }),
            ("AIInit_InitAICar", nfs4rc::f_80067b14, |c, a| {
                init_ai_car(c, Car(a[0]), UdffInfo(a[1]));
                None
            }),
            ("AIInit_DeInitAICar", nfs4rc::f_80067ce8, |c, a| {
                de_init_ai_car(c, Car(a[1]));
                None
            }),
            ("AIInit_InitAICar2", nfs4rc::f_80067d68, |c, a| {
                init_ai_car2(c, Car(a[1]));
                None
            }),
            ("AIInit_IsNonStandardCarFile", nfs4rc::f_80067dd8, |c, a| Some(is_non_standard_car_file(c, a[0] as i32))),
            ("AIInit_StartUp2", nfs4rc::f_80067424, |c, _| {
                start_up2(c);
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 300, 0x1A10 + k as u64, shuffle, original, &|c| {
                let (car, t) = (c.m.r32(ARGS), c.m.r32(ARGS + 4));
                // (the car first and again second, where the ports take it; the file second
                // for InitAICar)
                match name {
                    "AIInit_LoadPhysicsConfig" => vec![FILE],
                    "AIInit_InitAICar" => vec![car, FILE, t],
                    "AIInit_IsNonStandardCarFile" => vec![t, car, t],
                    _ => vec![car, car, t],
                }
            }, port);
        }
    }

    /// The stack the set-ups and resets leave (their callees' frames, the file names, the
    /// car-type table); a2 and a3 set too (sprintf homes the caller's a3).
    #[test]
    fn init_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 7] = [
            ("AIInit_Reset2", 0x8006_7508, nfs4rc::f_80067508),
            ("AIInit_ClearAICar", 0x8006_7974, nfs4rc::f_80067974),
            ("AIInit_InitAICar", 0x8006_7b14, nfs4rc::f_80067b14),
            ("AIInit_DeInitAICar", 0x8006_7ce8, nfs4rc::f_80067ce8),
            ("AIInit_InitAICar2", 0x8006_7d68, nfs4rc::f_80067d68),
            ("AIInit_LoadConfigs", 0x8006_77f8, nfs4rc::f_800677f8),
            ("AIInit_StartUp1", 0x8006_73b0, nfs4rc::f_800673b0),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, SCRATCH_TOP, 300, 0x1A30 + k as u64, shuffle, &|c, _| vec![c.m.r32(ARGS), FILE, 0, 0]);
        }
    }
}
