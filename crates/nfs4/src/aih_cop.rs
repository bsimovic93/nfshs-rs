//! AIH_COP.CPP: `AIHigh_Cop`, an AI police car of a pursuit race: parked in purgatory until a
//! police trigger puts it on the road ahead of a speeder (driving, idle at the roadside, off
//! the road, or blocking); chasing the racer its level sends it after, harder when close
//! behind, cutting it off, laying a spike belt or springing a blockade; around barriers;
//! back to purgatory once out of the race's life.

use crate::abi::{self, Saved};
use crate::aih_basiccop::{self, ROAD_BLOCK_STATE, fast_random_step};
use crate::aih_basicperp::{self, speech};
use crate::aihigh::{self, HIGH_LEVEL_AI_OBJS, obj};
use crate::aiworld::{apx_spline_distance, apx_spline_distance_to_slice};
use crate::fixed::{self, fixeddiv, fixedmult};
use crate::globals::{
    AI_ELAPSED_TIME, AITUNE_ROUGH_LAP_TIME, BWORLDSM_SLICES, GAME_SETUP, HUMAN_RACE_CARS, NUM_CARS, NUM_HUMAN_RACE_CARS, NUM_RACE_CARS, NUM_SLICES, RACE_CARS,
    SIM_COUNTER, SPIKE_BELT, STACK_SPEED_UP_ENABLED, TOTAL_SORTED_LIST, W_SAVE_PTR,
};
use crate::types::{AIHighCop, AIHighPlayer, AIStateBase, AIStateChase, AIStateGotoSlice, AITriggerTriggerManager, Car, Coord};
use ps1rt::{Cpu, Mem};

/// The class's vtable.
pub const VT_COP: u32 = 0x8005_5924;
/// `copTuningInfo`: each police car type's top speed caps and acceleration, by its car (from
/// 22 on).
const COP_TUNING_INFO: u32 = 0x8010_F220;
/// `AIHigh_Cop_AggressionData` (`._144[3]`, by the aggression): the ticks in the chase region,
/// the distances across and along for murder mode, its ticks, the nitrous ticks.
const AGGRESSION_DATA: u32 = 0x8010_DB00;
/// `NitroDistanceMeters`: the nitrous's fore and aft distances by police type.
const NITRO_DISTANCE: u32 = 0x8010_DC20;
/// `AICop_skillDelay`: the chase's delay factor by the skill.
const SKILL_DELAY: u32 = 0x8010_DB3C;
/// `AIH_Cop_chasePositions`: where a chaser drives, relative to the speeder, by its aggression
/// and place.
const CHASE_POSITIONS: u32 = 0x8010_DB48;
/// `triggerManagerCops`.
const TRIGGER_MANAGER_COPS: u32 = 0x8013_D8D8;

/// The state kinds (`stateType_`).
const STATE_PURGATORY: i32 = 1;
const STATE_NORMAL: i32 = 2;
const STATE_IDLE: i32 = 3;
const STATE_CHASE: i32 = 4;
const STATE_OFFROAD: i32 = 5;
const STATE_GOTO_SLICE: i32 = 9;
const VT_STATE_IDLE: u32 = 0x8005_5DD0;

/// `Car_tObj.AIFlags`: on duty; available.
const AI_ON_DUTY: i32 = 0x2;
const AI_AVAILABLE: i32 = 0x8;
/// `Car_tObj.carFlags`: out of the race.
const CAR_OUT: i32 = 0x400;

fn car_of(m: &mut Mem, this: AIHighCop) -> Car {
    Car(m.r32(this.0))
}

fn perp_car(m: &mut Mem, this: AIHighCop) -> Car {
    let perp = this.perp_target_(m).0;
    Car(m.r32(perp))
}

fn aggression(m: &mut Mem, this: AIHighCop, field: u32) -> i32 {
    let level = this.aggression_level_(m) as u32;
    m.r32(AGGRESSION_DATA.wrapping_add(level.wrapping_mul(20)).wrapping_add(field)) as i32
}

/// `AIHigh_Cop::AIHigh_Cop(Car_tObj*, int)` (AIH_COP.CPP, 0x800643f0): no speeder, no spike
/// belt asked for, the first aggression.
pub fn construct(c: &mut Cpu, this: AIHighCop, car: Car, index: i32) -> AIHighCop {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8005_c954, 0x8006_4404, |c| aih_basiccop::construct(c, this.ai_high_basic_cop(), car, index));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_COP);
        this.set_perp_target_(m, AIHighPlayer(0));
        this.set_chase_index_(m, 0);
        this.set_request_spike_belt_at_slice_(m, -1);
        this.set_force_purgatory_(m, 0);
        this.set_aggression_level_(m, 0);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighCop(c.r[4]), Car(c.r[5]), c.r[6] as i32).0;
}

/// `AIHigh_Cop::SetTuningLevers` (AIH_COP.CPP, 0x8006443c): its car's top speed and
/// acceleration, by its type and car.
pub fn set_tuning_levers(m: &mut Mem, this: AIHighCop) {
    let car = car_of(m, this);
    let info = car.car_info(m).0;
    let kind = m.r32(info) as i32;
    let e = COP_TUNING_INFO.wrapping_add((kind.wrapping_sub(22) << 4) as u32);
    let (top, acc) = if this.ai_high_basic_cop().type_(m) == 1 { (12, 4) } else { (8, 0) };
    let v = m.r32(e + top) as i32;
    car.set_cop_top_speed(m, v);
    let v = m.r32(e + acc) as i32;
    car.set_cop_acc_mult(m, v);
}

pub fn abi_set_tuning_levers(c: &mut Cpu) {
    set_tuning_levers(&mut c.m, AIHighCop(c.r[4]));
}

/// A new chase after the speeder from its place (`rel`, on the stack at sp+40): the aggression's
/// nitrous, the type's nitrous distances, the aggression, the skill's delay; its arguments
/// past the fourth at sp+16.
fn new_chase(c: &mut Cpu, r: &Saved, this: AIHighCop, sp: u32, ra: u32) -> u32 {
    let m = &mut c.m;
    let s = crate::aistate::pool_new(m);
    let kind = this.ai_high_basic_cop().type_(m) as u32;
    let nitro = NITRO_DISTANCE.wrapping_add(kind.wrapping_mul(8));
    let skill = GAME_SETUP.skill(m) as u32;
    let args = [
        aggression(m, this, 16),
        m.r32(nitro) as i32,
        m.r32(nitro + 4) as i32,
        this.aggression_level_(m),
        m.r32(SKILL_DELAY.wrapping_add(skill.wrapping_mul(4))) as i32,
    ];
    for (k, v) in args.iter().enumerate() {
        m.w32(sp + 16 + 4 * k as u32, *v as u32);
    }
    let (car, target) = (car_of(m, this), perp_car(m, this));
    r.enter(c, 0x8007_04dc, ra, |c| crate::aistate::chase_construct(c, AIStateChase(s), car, target, Coord(sp + 40), args[0], args[1], args[2], args[3], args[4]));
    s
}

fn new_normal(c: &mut Cpu, r: &Saved, car: Car, ra: u32) -> u32 {
    let s = crate::aistate::pool_new(&mut c.m);
    r.enter(c, 0x8007_03bc, ra, |c| crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car));
    s
}

fn new_purgatory(c: &mut Cpu, r: &Saved, car: Car, ra: u32) -> u32 {
    let s = crate::aistate::pool_new(&mut c.m);
    r.enter(c, 0x8007_1d6c, ra, |c| crate::aistate::purgatory_construct(c, AIStateBase(s), car));
    s
}

fn new_idle(m: &mut Mem, car: Car, in_place: u32) -> u32 {
    let s = crate::aistate::pool_new(m);
    crate::aistate::base_construct(m, AIStateBase(s), car);
    m.w32(s.wrapping_add(4), VT_STATE_IDLE);
    m.w32(s.wrapping_add(12), in_place);
    s
}

/// The speeder's engagement for its level (its time and the engagement each tick, by the
/// race's laps).
fn set_engagement(m: &mut Mem, perp: AIHighPlayer) {
    let info = perp.perp_chase_info_();
    let level = info.chase_level_(m);
    let lap = (m.r32(level + 0x24) as i32).wrapping_mul(AITUNE_ROUGH_LAP_TIME.get(m)) / 0x1_0000;
    let per_tick = fixed::div(0x1_0000, lap << 5);
    info.set_engagement_time_(m, lap << 21);
    info.set_engagement_percent_increase_per_tick_(m, per_tick);
    match GAME_SETUP.num_laps(m) {
        2 => info.set_engagement_percent_increase_per_tick_(m, fixedmult(per_tick, 0x1_3333)),
        4 => info.set_engagement_percent_increase_per_tick_(m, fixedmult(per_tick, 0xA8F5)),
        _ => {}
    }
}

/// The track's way back (a police car turning round to drive away).
fn away(m: &mut Mem) -> i32 {
    if GAME_SETUP.reverse_track(m) != 0 { 1 } else { -1 }
}

/// `AIHigh_Cop::HighExecute` (AIH_COP.CPP, 0x800644b4, virtual): each AI tick (lapless), the spike
/// belt, a wipe-out, its tuning; by its state: none, purgatory (or idle when out of the race);
/// purgatory: a blockade's place when blocking, else a police trigger for a speeder puts it on
/// the road (driving or idle, all-out with two humans close; off the road); driving, after its
/// speeder; chasing: around barriers, a needy player's slow-down, harder when close, cutting
/// it off, a new speeder, back when the speeder is done; idle: blocking (a spike belt, the
/// blockade's elements, chasing once the speeder arrives), or chasing a new speeder; off the
/// road, let go when a racer with a crime comes, back on the road; going to a slice. A car
/// out of the race's life (or sent) goes back to purgatory. Then its state.
pub fn high_execute(c: &mut Cpu, this: AIHighCop) {
    abi::frame(c, 136, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let basic = this.ai_high_basic_cop();
        let b = basic.blockade_();
        let m = &mut c.m;
        let mut car = car_of(m, this);
        car.set_unlap(m, 0);
        car.set_lap(m, 0);
        r.enter(c, 0x8005_c9dc, 0x8006_44e8, aih_basiccop::check_spike_belt);
        r.enter(c, 0x8006_5c30, 0x8006_44f0, |c| check_for_wipe_out(&mut c.m, this));
        r.enter(c, 0x8006_443c, 0x8006_44f8, |c| set_tuning_levers(&mut c.m, this));
        let assign = |c: &mut Cpu, r: &Saved, ra: u32, perp: AIHighPlayer| r.enter(c, 0x8006_6008, ra, |c| assign_to_player(c, this, perp));
        let none = AIHighPlayer(0);
        // (back to purgatory: the radio, nobody chased, the purgatory state)
        let to_purgatory = |c: &mut Cpu, r: &mut Saved, ra: [u32; 4]| {
            this.set_force_purgatory_(&mut c.m, 0);
            let car = car_of(&mut c.m, this);
            speech(c, r, car, 16, ra[0], ra[1], &[]);
            assign(c, r, ra[2], none);
            let car = car_of(&mut c.m, this);
            let s = new_purgatory(c, r, car, ra[3]);
            r.0[0] = s;
            aihigh::set_state(c, r, this.0, s, STATE_PURGATORY, ra[3] + 0x2c);
        };
        // (driving away: back the track's way, nobody chased, driving)
        let drive_away = |c: &mut Cpu, r: &mut Saved| {
            let m = &mut c.m;
            let car = car_of(m, this);
            let dir = away(m);
            car.set_desired_direction(m, dir);
            r.0[2] = 1;
            car.set_drive_direction(m, 1);
            assign(c, r, 0x8006_523c, none);
            let car = car_of(&mut c.m, this);
            let s = new_normal(c, r, car, 0x8006_5250);
            r.0[0] = s;
            aihigh::set_state(c, r, this.0, s, STATE_NORMAL, 0x8006_527c);
            let car = car_of(&mut c.m, this);
            if this.ai_high_basic_cop().drive_away_(&mut c.m) == 1 {
                r.enter(c, 0x8008_7088, 0x8006_52a0, |c| crate::cars::reset_collided_cars(c, car, 1));
            }
            this.ai_high_basic_cop().set_drive_away_(&mut c.m, 0);
        };
        let kind = c.m.r32(this.0 + 8);
        match kind {
            0 => {
                assign(c, &r, 0x8006_4534, none);
                car = car_of(&mut c.m, this);
                let m = &mut c.m;
                car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                if car.car_flags(m) & CAR_OUT != 0 {
                    let s = new_idle(m, car, 1);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_45c0);
                } else {
                    let s = new_purgatory(c, &r, car, 0x8006_45dc);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_PURGATORY, 0x8006_4608);
                }
                return;
            }
            1 => {
                let m = &mut c.m;
                car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                this.set_request_spike_belt_at_slice_(m, -1);
                basic.set_drive_away_(m, 0);
                if b.mode(m) == 4 {
                    b.set_mode(m, 0);
                }
                let mode = b.mode(m);
                if mode == 2 {
                    let target = b.target(m);
                    assign(c, &r, 0x8006_4668, target);
                    car = car_of(&mut c.m, this);
                    let s = new_idle(&mut c.m, car, 1);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_46c0);
                    let m = &mut c.m;
                    let (slice, dir, lat, yaw) = (b.slice(m), b.direction(m), b.lat_pos(m), b.rotation(m));
                    m.w32(sp + 16, yaw as u32);
                    r.enter(c, 0x8006_8cd4, 0x8006_46ec, |c| crate::ailife::reencarnate_cop_by_lat_pos_and_rotation(c, car, slice, dir, lat, yaw));
                    let m = &mut c.m;
                    let at = b.request_spike_belt_at_slice(m);
                    this.set_request_spike_belt_at_slice_(m, at);
                } else if car.ai_flags(m) & AI_AVAILABLE != 0 && mode != 1 {
                    let trigger = r.enter(c, 0x8006_6144, 0x8006_4724, |c| check_for_new_triggers(c, this));
                    r.0[0] = 0;
                    if trigger != 0 {
                        let m = &mut c.m;
                        for w in (0..72).step_by(4) {
                            let v = m.r32(trigger + w);
                            m.w32(sp + 40 + w, v);
                        }
                        let mut close = false;
                        if NUM_HUMAN_RACE_CARS.get(m) == 2 {
                            let (a, b2) = (Car(m.r32(HUMAN_RACE_CARS)), Car(m.r32(HUMAN_RACE_CARS + 4)));
                            close = apx_spline_distance(m, a, b2).wrapping_abs() <= 0x12B_FFFF;
                        }
                        r.0[0] = close as u32;
                        if close {
                            m.w32(sp + 40, 1);
                            m.w32(sp + 56, 1);
                            m.w32(sp + 48, 1);
                            m.w32(sp + 52, 1);
                        }
                        let kind = m.r32(sp + 40) as i32;
                        r.0[2] = kind as u32;
                        let slice = m.r32(sp + 44) as i32;
                        if kind == 1 {
                            fast_random_step(m);
                            let dir = if GAME_SETUP.reverse_track(m) != 0 { -1 } else { 1 };
                            r.0[3] = dir as u32;
                            if m.r32(sp + 56) != 0 {
                                assign(c, &r, 0x8006_4944, none);
                                car = car_of(&mut c.m, this);
                                let s = new_normal(c, &r, car, 0x8006_4958);
                                r.0[0] = s;
                                aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8006_4984);
                            } else {
                                assign(c, &r, 0x8006_4998, none);
                                car = car_of(&mut c.m, this);
                                let s = new_idle(&mut c.m, car, kind as u32);
                                r.0[0] = s;
                                aihigh::set_state(c, &r, this.0, s, STATE_IDLE, 0x8006_49ec);
                            }
                            let moving = c.m.r32(sp + 56);
                            c.m.w32(sp + 16, moving);
                            r.enter(c, 0x8006_89a0, 0x8006_4a14, |c| crate::ailife::reencarnate_cop_by_slice(c, car, slice, dir, dir, moving != 0));
                        } else if kind == 3 {
                            let way = m.r32(sp + 48) as i32;
                            let ours = if GAME_SETUP.reverse_track(m) != 0 { way == -1 } else { way == 1 };
                            if ours || way == 0 {
                                assign(c, &r, 0x8006_4848, none);
                                car = car_of(&mut c.m, this);
                                let m = &mut c.m;
                                let s = crate::aistate::pool_new(m);
                                (r.0[2], r.0[3]) = (sp + 52, sp + 64);
                                let (mph, release, target) = (m.r32(sp + 100) as i32, m.r32(sp + 104) as i32, m.r32(sp + 108) as i32);
                                m.w32(sp + 16, sp + 64);
                                m.w32(sp + 20, mph as u32);
                                m.w32(sp + 24, release as u32);
                                m.w32(sp + 28, target as u32);
                                r.enter(c, 0x8007_1a00, 0x8006_488c, |c| {
                                    crate::aistate::offroad_construct(&mut c.m, crate::types::AIStateOffroad(s), car, slice, Coord(sp + 52), crate::types::Matrix(sp + 64), mph, release, target)
                                });
                                r.0[0] = s;
                                aihigh::set_state(c, &r, this.0, s, STATE_OFFROAD, 0x8006_48b8);
                                c.m.w32(sp + 16, sp + 64);
                                r.enter(c, 0x8006_8b24, 0x8006_48dc, |c| crate::ailife::reencarnate_cop_by_position(c, car, slice, 1, Coord(sp + 52), crate::types::Matrix(sp + 64)));
                            }
                        }
                    }
                }
            }
            2 => {
                let m = &mut c.m;
                this.set_request_spike_belt_at_slice_(m, -1);
                let mode = b.mode(m);
                if mode != 1 {
                    r.0[2] = STATE_CHASE as u32;
                    if mode != 4 {
                        car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                    }
                    let mode = b.mode(m);
                    if mode != 1 && mode != 4 && r.enter(c, 0x8006_5da8, 0x8006_4a78, |c| check_for_new_target(c, this)) {
                        r.enter(c, 0x8006_6094, 0x8006_4a8c, |c| get_check_chase_position(c, this, Coord(sp + 40)));
                        car = car_of(&mut c.m, this);
                        let s = new_chase(c, &r, this, sp, 0x8006_4b34);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_CHASE, 0x8006_4b60);
                        let target = perp_car(&mut c.m, this);
                        speech(c, &r, car, 6, 0x8006_4b70, 0x8006_4b90, &[target.0]);
                    }
                }
                car = car_of(&mut c.m, this);
                let alive = this.force_purgatory_(&mut c.m) == 0 && !r.enter(c, 0x8006_7e50, 0x8006_4bac, |c| crate::ailife::evaluate_life(c, car));
                if !alive {
                    to_purgatory(c, &mut r, [0x8006_4bc0, 0x8006_4bdc, 0x8006_4be8, 0x8006_4bfc]);
                }
            }
            4 => {
                let m = &mut c.m;
                car.update_ai_flags(m, |f| f | AI_ON_DUTY);
                let chase = AIStateChase(m.r32(this.0 + 4));
                r.0[2] = chase.0;
                if this.perp_target_(m).0 == 0 {
                    drive_away(c, &mut r);
                } else {
                    if chase.barrier_ticks32_(m) >= 161 {
                        let end = r.enter(c, 0x8007_15f4, 0x8006_4c74, |c| crate::aistate::find_barrier_end_slice(c, chase));
                        r.0[0] = end as u32;
                        let s = crate::aistate::pool_new(&mut c.m);
                        r.enter(c, 0x8007_2c4c, 0x8006_4c94, |c| crate::aistate::goto_slice_construct(c, AIStateGotoSlice(s), car, end, 0));
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_GOTO_SLICE, 0x8006_4cc0);
                        speech(c, &r, car, 7, 0x8006_4cd4, 0x8006_4cf0, &[]);
                    }
                    let m = &mut c.m;
                    let perp = this.perp_target_(m);
                    let level = perp.perp_chase_info_().chase_level_(m);
                    let k = basic.type_(m) as u32;
                    if m.r32(level.wrapping_add(4 * k)) == 0 && m.r32(level.wrapping_add(4 * k + 12)) == 0 {
                        chase.set_nitrous_ticks_(m, 0);
                    }
                    this.set_request_spike_belt_at_slice_(m, -1);
                    r.0[0] = crate::globals::SIM_GLOBAL.0;
                    let needy = SIM_COUNTER.get(m) >= chase.slow_down_end_time_(m) && r.enter(c, 0x8006_5b94, 0x8006_4d54, |c| check_for_needy_players(&mut c.m)) != -1;
                    if needy {
                        let m = &mut c.m;
                        let now = SIM_COUNTER.get(m);
                        chase.set_slow_down_end_time_(m, now.wrapping_add(960));
                    }
                    r.enter(c, 0x8005_d218, 0x8006_4d7c, |c| aih_basiccop::handle_blockade_speech(c, basic));
                    r.0[0] = 0;
                    let moved = r.enter(c, 0x8006_6094, 0x8006_4d8c, |c| get_check_chase_position(c, this, Coord(sp + 40))) && chase.murder_mode_(&mut c.m) == 0;
                    r.0[0] = moved as u32;
                    if moved {
                        let target = perp_car(&mut c.m, this);
                        r.enter(c, 0x8007_066c, 0x8006_4dbc, |c| crate::aistate::set_target(c, chase, target, Coord(sp + 40)));
                    }
                    let m = &mut c.m;
                    let murder = aggression(m, this, 0) < chase.in_target_region_(m)
                        && chase.lat_meters_between_(m).wrapping_abs() < aggression(m, this, 4)
                        && chase.long_meters_between_(m).wrapping_abs() < aggression(m, this, 8);
                    if murder {
                        let ticks = aggression(m, this, 12);
                        crate::aistate::set_murder_mode(m, chase, 1, ticks);
                    }
                    r.0[0] = 0;
                    let target = perp_car(&mut c.m, this);
                    let cut = r.enter(c, 0x8005_caa4, 0x8006_4e78, |c| aih_basiccop::should_i_perform_cut_off_block(c, basic, 16384, target)) && chase.murder_mode_(&mut c.m) == 0;
                    r.0[0] = cut as u32;
                    if cut {
                        r.0[0] = sp + 56;
                        r.call(c, nfs4rc::f_800e4f04, &[sp + 56, 0, 12], 0x8006_4ea4); // memset
                        let target = perp_car(&mut c.m, this);
                        r.enter(c, 0x8007_066c, 0x8006_4eb8, |c| crate::aistate::set_target(c, chase, target, Coord(sp + 56)));
                        crate::aistate::set_murder_mode(&mut c.m, chase, 1, 32);
                    }
                    if r.enter(c, 0x8006_5da8, 0x8006_4ed0, |c| check_for_new_target(c, this)) {
                        r.0[0] = sp + 56;
                        r.enter(c, 0x8006_6094, 0x8006_4ee4, |c| get_check_chase_position(c, this, Coord(sp + 56)));
                        let target = perp_car(&mut c.m, this);
                        r.enter(c, 0x8007_066c, 0x8006_4ef8, |c| crate::aistate::set_target(c, chase, target, Coord(sp + 56)));
                    }
                    let m = &mut c.m;
                    car = car_of(m, this);
                    let mode = b.mode(m);
                    let done = if mode == 1 || mode == 4 {
                        let dir = away(m);
                        car.set_desired_direction(m, dir);
                        car.set_drive_direction(m, 1);
                        true
                    } else {
                        let perp = this.perp_target_(m);
                        perp.0 == 0 || Car(m.r32(perp.0)).stats().finish_type(m) >= 2
                    };
                    if done {
                        assign(c, &r, 0x8006_4f84, none);
                        car = car_of(&mut c.m, this);
                        let s = new_normal(c, &r, car, 0x8006_4f98);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8006_4fc4);
                    }
                    let alive = this.force_purgatory_(&mut c.m) == 0 && !r.enter(c, 0x8006_7e50, 0x8006_4fec, |c| crate::ailife::evaluate_life(c, car));
                    if alive {
                        if basic.drive_away_(&mut c.m) != 0 {
                            drive_away(c, &mut r);
                        }
                    } else {
                        let life = r.enter(c, 0x8006_7e50, 0x8006_5000, |c| crate::ailife::evaluate_life(c, car));
                        if life && basic.drive_away_(&mut c.m) == 0 {
                            speech(c, &r, car, 7, 0x8006_5024, 0x8006_5040, &[]);
                        }
                        let mut rr = r;
                        rr.0[0] = r.0[0];
                        let m = &mut c.m;
                        this.set_force_purgatory_(m, 0);
                        speech(c, &rr, car, 16, 0x8006_504c, 0x8006_5068, &[]);
                        assign(c, &rr, 0x8006_5078, none);
                        car = car_of(&mut c.m, this);
                        let s = new_purgatory(c, &rr, car, 0x8006_508c);
                        rr.0[0] = s;
                        aihigh::set_state(c, &rr, this.0, s, STATE_PURGATORY, 0x8006_50b8);
                        r = rr;
                    }
                }
            }
            3 => {
                let m = &mut c.m;
                let mode = b.mode(m);
                if mode == 1 || mode == 4 {
                    let dir = away(m);
                    car.set_desired_direction(m, dir);
                    car.set_drive_direction(m, 1);
                    assign(c, &r, 0x8006_5120, none);
                    car = car_of(&mut c.m, this);
                    let s = new_normal(c, &r, car, 0x8006_5134);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8006_5160);
                }
                if this.force_purgatory_(&mut c.m) != 0 {
                    to_purgatory(c, &mut r, [0x8006_5188, 0x8006_51a4, 0x8006_51b0, 0x8006_51c4]);
                } else if basic.drive_away_(&mut c.m) != 0 {
                    drive_away(c, &mut r);
                } else if b.mode(&mut c.m) == 2 {
                    idle_blocking(c, &mut r, this, sp);
                } else {
                    let m = &mut c.m;
                    car.update_ai_flags(m, |f| f & !AI_ON_DUTY);
                    let found = r.enter(c, 0x8006_5da8, 0x8006_5780, |c| check_for_new_target(c, this));
                    car = car_of(&mut c.m, this);
                    let mode = b.mode(&mut c.m);
                    if found && mode != 1 && mode != 4 {
                        r.0[2] = STATE_CHASE as u32;
                        r.enter(c, 0x8006_6094, 0x8006_57a8, |c| get_check_chase_position(c, this, Coord(sp + 40)));
                        car = car_of(&mut c.m, this);
                        let s = new_chase(c, &r, this, sp, 0x8006_5850);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_CHASE, 0x8006_587c);
                        let target = perp_car(&mut c.m, this);
                        speech(c, &r, car, 6, 0x8006_588c, 0x8006_58ac, &[target.0]);
                    } else if r.enter(c, 0x8006_7e50, 0x8006_58c0, |c| crate::ailife::evaluate_life(c, car)) {
                        assign(c, &r, 0x8006_58d0, none);
                        car = car_of(&mut c.m, this);
                        let s = new_purgatory(c, &r, car, 0x8006_58e4);
                        r.0[0] = s;
                        aihigh::set_state(c, &r, this.0, s, STATE_PURGATORY, 0x8006_5910);
                    }
                }
            }
            5 => {
                let m = &mut c.m;
                let allowance = if car.n().sim_optz(m) != 0 { 0x1_8000 } else { 0 };
                car.set_extra_wall_collision_allowance(m, allowance);
                this.set_request_spike_belt_at_slice_(m, -1);
                // (s2 the index, s0 its place in the list, past the last one after)
                (r.0[0], r.0[2], r.0[3]) = (RACE_CARS, 0, HIGH_LEVEL_AI_OBJS);
                let mut i = 0;
                while i < NUM_RACE_CARS.get(&mut c.m) {
                    let m = &mut c.m;
                    let racer = Car(m.r32(RACE_CARS + 4 * i as u32));
                    let index = racer.car_index(m);
                    let o = obj(m, index);
                    if m.r32(o + 0x78) != 0 {
                        let state = crate::types::AIStateOffroad(m.r32(this.0 + 4));
                        r.enter(c, 0x8007_1b48, 0x8006_59b0, |c| crate::aistate::unleash_if_in_range(c, state, racer));
                    }
                    i += 1;
                    (r.0[0], r.0[2]) = (RACE_CARS + 4 * i as u32, i as u32);
                }
                let m = &mut c.m;
                let slice = car.n().sim_road_info().slice(m) as i16 as i32;
                let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
                let b29 = m.r8(s + 29) as i32;
                let left = ((m.r8(s + 30) as i32) << 15).wrapping_mul(b29 >> 4);
                let at = car.road_position(m);
                let on_road = at >= left.wrapping_neg() && ((m.r8(s + 31) as i32) << 15).wrapping_mul(b29 & 15) >= at;
                if on_road {
                    car.set_extra_wall_collision_allowance(m, 0);
                    assign(c, &r, 0x8006_5a30, none);
                    car = car_of(&mut c.m, this);
                    let s = new_normal(c, &r, car, 0x8006_5a44);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8006_5a70);
                }
                let gone = this.force_purgatory_(&mut c.m) != 0 || r.enter(c, 0x8006_7e50, 0x8006_5a98, |c| crate::ailife::evaluate_life(c, car));
                if gone {
                    let m = &mut c.m;
                    car.set_extra_wall_collision_allowance(m, 0);
                    this.set_force_purgatory_(m, 0);
                    assign(c, &r, 0x8006_5ab4, none);
                    car = car_of(&mut c.m, this);
                    let s = new_purgatory(c, &r, car, 0x8006_5ac8);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_PURGATORY, 0x8006_5af4);
                }
            }
            9 => {
                let goto = AIStateGotoSlice(c.m.r32(this.0 + 4));
                r.0[0] = goto.0;
                assign(c, &r, 0x8006_5b0c, none);
                car = car_of(&mut c.m, this);
                if r.enter(c, 0x8007_2db8, 0x8006_5b18, |c| crate::aistate::in_target_slice_range(c, goto, 0xA_0000)) {
                    let s = new_normal(c, &r, car, 0x8006_5b34);
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, STATE_NORMAL, 0x8006_5b60);
                }
            }
            _ => {}
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8006_5b78, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_high_execute(c: &mut Cpu) {
    high_execute(c, AIHighCop(c.r[4]));
}

/// An idle police car blocking: the spike belt asked for laid (across the lanes by the skill)
/// and the road block up; the blockade's elements placed (off the scratchpad stack when on
/// it); once its speeder arrives (within 50 m, or before the blockade's release time), its
/// engagement anew and a chase (turning round when the blockade faces back).
fn idle_blocking(c: &mut Cpu, r: &mut Saved, this: AIHighCop, sp: u32) {
    let basic = this.ai_high_basic_cop();
    let b = basic.blockade_();
    let m = &mut c.m;
    let car = car_of(m, this);
    car.update_ai_flags(m, |f| f | AI_ON_DUTY);
    let at = this.request_spike_belt_at_slice_(m);
    r.0[2] = 0;
    if at != -1 && SPIKE_BELT.active_(m) == 0 {
        let share: i32 = match GAME_SETUP.skill(m) {
            0 => 0xB333,
            1 => 0xCCCC,
            _ => 0xE666,
        };
        r.0[0] = share as u32;
        let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((this.request_spike_belt_at_slice_(m) as u32) << 5);
        let b29 = m.r8(s + 29) as i32;
        let left = fixedmult(((m.r8(s + 30) as i32) << 15).wrapping_mul(b29 >> 4), share);
        let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((this.request_spike_belt_at_slice_(m) as u32) << 5);
        let b29 = m.r8(s + 29) as i32;
        let right = fixedmult(((m.r8(s + 31) as i32) << 15).wrapping_mul(b29 & 15), share);
        r.0[0] = left as u32;
        let slice = this.request_spike_belt_at_slice_(m);
        SPIKE_BELT.set_left_lat_pos_(m, left.wrapping_neg());
        SPIKE_BELT.set_right_lat_pos_(m, right);
        let now = SIM_COUNTER.get(m);
        SPIKE_BELT.set_active_(m, 1);
        SPIKE_BELT.set_slice_(m, slice);
        SPIKE_BELT.set_freshen_time_(m, now);
        let slice = this.request_spike_belt_at_slice_(m);
        r.call(c, nfs4rc::f_8007e26c, &[slice as u32, left.wrapping_neg() as u32, left.wrapping_add(right) as u32], 0x8006_53c4); // BWorld_SetSpikeBelt
    }
    let m = &mut c.m;
    ROAD_BLOCK_STATE.set(m, 1);
    let at = this.request_spike_belt_at_slice_(m);
    if at != -1 && SPIKE_BELT.slice_(m) == at {
        let now = SIM_COUNTER.get(m);
        SPIKE_BELT.set_freshen_time_(m, now);
    }
    if b.flags(m) != 0 {
        if STACK_SPEED_UP_ENABLED.get(m) != 0 {
            // (SetSp: off the scratchpad stack and back)
            let swap_sp = |c: &mut Cpu| {
                let sp = c.r[29];
                c.r[29] = W_SAVE_PTR.get(&mut c.m);
                W_SAVE_PTR.set(&mut c.m, sp);
            };
            swap_sp(c);
            STACK_SPEED_UP_ENABLED.set(&mut c.m, 0);
            r.enter(c, 0x8005_cf80, 0x8006_5450, |c| aih_basiccop::setup_blockade_elements(c, b));
            swap_sp(c);
            STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
        } else {
            r.enter(c, 0x8005_cf80, 0x8006_5484, |c| aih_basiccop::setup_blockade_elements(c, b));
        }
    }
    let m = &mut c.m;
    let perp = this.perp_target_(m);
    r.0[0] = perp.0 + 0x8C;
    set_engagement(m, perp);
    r.enter(c, 0x8006_6094, 0x8006_5540, |c| get_check_chase_position(c, this, Coord(sp + 40)));
    let m = &mut c.m;
    let target = perp_car(m, this);
    let d = apx_spline_distance(m, car, target);
    let far = d.wrapping_abs();
    let arrived = if far <= 0x31_FFFF {
        true
    } else if far > 0x12B_FFFF {
        false
    } else {
        let speed = target.current_speed(m);
        if speed.wrapping_abs() > 0x4_71C7 {
            let t = fixeddiv(d, speed);
            t > 0 && t < b.release_time(m)
        } else {
            false
        }
    };
    r.0[2] = arrived as u32;
    if !arrived {
        let slice = b.slice(m);
        let d = apx_spline_distance_to_slice(m, target, slice) / 0x1_0000;
        if b.initial_player_distance_meters_int(m).wrapping_mul(d) <= 0 {
            return;
        }
    }
    let m = &mut c.m;
    this.set_request_spike_belt_at_slice_(m, -1);
    let perp = this.perp_target_(m);
    if b.chase_level(m) == perp.perp_chase_info_().chase_level_index_(m) {
        perp.perp_chase_info_().set_engagement_time_(m, 0);
    }
    b.set_mode(m, 0);
    let s = new_chase(c, r, this, sp, 0x8006_5710);
    r.0[0] = s;
    aihigh::set_state(c, r, this.0, s, STATE_CHASE, 0x8006_573c);
    if b.reverse(&mut c.m) != 0 {
        crate::aiphysic::change_direction(&mut c.m, car, 64);
    }
}

/// `AIHigh_Cop::CheckForNeedyPlayers` (AIH_COP.CPP, 0x80065b94): the last human free of the
/// police for over 12.5 s and going over 20 m/s, or none (-1).
pub fn check_for_needy_players(m: &mut Mem) -> i32 {
    let mut needy = -1;
    for i in 0..NUM_HUMAN_RACE_CARS.get(m) {
        let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
        let index = human.car_index(m);
        if AIHighPlayer(obj(m, index)).perp_chase_info_().cop_free_ticks_(m) >= 801 && human.current_speed(m).wrapping_abs() > 0x14_0000 {
            needy = index;
        }
    }
    needy
}

pub fn abi_check_for_needy_players(c: &mut Cpu) {
    c.r[2] = check_for_needy_players(&mut c.m) as u32;
}

/// `AIHigh_Cop::CheckForWipeOut` (AIH_COP.CPP, 0x80065c30): chasing an AI opponent engaged for
/// 2 s or more (and not wiping out already), a human at a higher level makes it wipe out for
/// 10 s by chance.
pub fn check_for_wipe_out(m: &mut Mem, this: AIHighCop) {
    if m.r32(this.0 + 8) != STATE_CHASE as u32 {
        return;
    }
    let perp = this.perp_target_(m);
    if perp.0 == 0 {
        return;
    }
    let pc = Car(m.r32(perp.0));
    let car = car_of(m, this);
    if pc.car_flags(m) & aihigh::CAR_OPPONENT == 0 || SIM_COUNTER.get(m) < car.wipe_out_end_tick(m) {
        return;
    }
    if perp.perp_chase_info_().engagement_time_(m) / 0x1_0000 >= 2 {
        return;
    }
    let t = fast_random_step(m);
    let e = AI_ELAPSED_TIME.get(m);
    let chance = (t >> 8 & 0xFFFF) as i32;
    let odds = e.wrapping_mul(11) << 3;
    let level = perp.perp_chase_info_().chase_level_index_(m);
    for i in 0..NUM_HUMAN_RACE_CARS.get(m) {
        let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
        let index = human.car_index(m);
        if level < AIHighPlayer(obj(m, index)).perp_chase_info_().chase_level_index_(m) && chance < odds.wrapping_add(e) {
            let now = SIM_COUNTER.get(m);
            car.set_wipe_out_end_tick(m, now.wrapping_add(640));
        }
    }
}

pub fn abi_check_for_wipe_out(c: &mut Cpu) {
    check_for_wipe_out(&mut c.m, AIHighCop(c.r[4]));
}

/// `AIHigh_Cop::CheckForNewTarget` (AIH_COP.CPP, 0x80065da8): unless blocking, the closest racer
/// with a crime whose level wants more police cars of its type; else the closest with a
/// crime; chased (its level's aggression); nobody when none. Whether it changed to a new
/// speeder.
pub fn check_for_new_target(c: &mut Cpu, this: AIHighCop) -> bool {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let basic = this.ai_high_basic_cop();
        let old = this.perp_target_(m).0;
        (r.0[3], r.0[4], r.0[5], r.0[7]) = (0, this.0, 0x2710_0000, old);
        let mode = basic.blockade_().mode(m);
        if mode == 1 || mode == 4 || mode == 2 {
            return false;
        }
        let me = car_of(m, this);
        let k = basic.type_(m) as u32;
        let (mut best, mut best_d) = (0u32, 0x2710_0000);
        let n = NUM_RACE_CARS.get(m);
        r.0[6] = HIGH_LEVEL_AI_OBJS;
        for i in 0..n {
            let m = &mut c.m;
            let racer = Car(m.r32(RACE_CARS + 4 * i as u32));
            let index = racer.car_index(m);
            let o = AIHighPlayer(obj(m, index));
            r.0[0] = o.0;
            let crime = m.r32(o.0 + 0x78);
            let level = o.perp_chase_info_().chase_level_(m);
            let want = if crime != 0 { m.r32(level.wrapping_add(4 * k)) as i32 } else { 0 };
            let mut have = m.r32(o.0 + 0x70 + 4 * k) as i32;
            if old != 0 && old == o.0 {
                have -= 1;
            }
            if have < want {
                let other = Car(m.r32(o.0));
                let d = apx_spline_distance(m, me, other).wrapping_abs();
                if d < best_d {
                    (best_d, best) = (d, o.0);
                }
            }
        }
        // (s2 cleared in the delay slot either way: the second loop's count only when it runs)
        let second = best == 0;
        if second {
            for i in 0..NUM_RACE_CARS.get(&mut c.m) {
                let m = &mut c.m;
                let racer = Car(m.r32(RACE_CARS + 4 * i as u32));
                let index = racer.car_index(m);
                let o = AIHighPlayer(obj(m, index));
                r.0[0] = o.0;
                let other = Car(m.r32(o.0));
                let d = apx_spline_distance(m, me, other).wrapping_abs();
                if d < best_d && m.r32(o.0 + 0x78) != 0 {
                    (best_d, best) = (d, o.0);
                }
            }
        }
        let n = NUM_RACE_CARS.get(&mut c.m).max(0) as u32;
        (r.0[1], r.0[2], r.0[3], r.0[5]) = (RACE_CARS + 4 * n, if second { n } else { 0 }, best, best_d as u32);
        if best != 0 && best != old {
            r.enter(c, 0x8006_6008, 0x8006_5fa0, |c| assign_to_player(c, this, AIHighPlayer(best)));
            let m = &mut c.m;
            let level = AIHighPlayer(best).perp_chase_info_().chase_level_(m);
            // (its type read again: the speeder's chasers list can reach it)
            let k = basic.type_(m) as u32;
            let v = m.r32(level.wrapping_add(0x18 + 4 * k)) as i32;
            this.set_aggression_level_(m, v);
            return true;
        }
        if old != 0 && best == 0 {
            r.enter(c, 0x8006_6008, 0x8006_5fd8, |c| assign_to_player(c, this, AIHighPlayer(0)));
        }
        false
    })
}

pub fn abi_check_for_new_target(c: &mut Cpu) {
    c.r[2] = check_for_new_target(c, AIHighCop(c.r[4])) as u32;
}

/// `AIHigh_Cop::AssignToPlayer` (AIH_COP.CPP, 0x80066008): chase `perp` (or nobody, ending a
/// blockade unless one is being set up): off the last one's chasers, onto its.
pub fn assign_to_player(c: &mut Cpu, this: AIHighCop, perp: AIHighPlayer) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this.0, perp.0);
        let m = &mut c.m;
        let basic = this.ai_high_basic_cop();
        let (cop, car, kind) = (basic.cop_index_(m), car_of(m, this).car_index(m), basic.type_(m));
        let old = this.perp_target_(m);
        if old.0 != 0 {
            aih_basicperp::remove_chaser(m, old.ai_high_basic_perp(), cop, kind);
        }
        if perp.0 == 0 && basic.blockade_().mode(m) != 1 {
            basic.blockade_().set_mode(m, 0);
        }
        this.set_perp_target_(m, perp);
        if perp.0 != 0 {
            r.enter(c, 0x8005_c6f8, 0x8006_6080, |c| aih_basicperp::add_chaser(c, perp.ai_high_basic_perp(), cop, car, kind));
        }
    })
}

pub fn abi_assign_to_player(c: &mut Cpu) {
    assign_to_player(c, AIHighCop(c.r[4]), AIHighPlayer(c.r[5]));
}

/// `AIHigh_Cop::GetCheckChasePosition` (AIH_COP.CPP, 0x80066094): its place among the speeder's
/// chasers brought up to date, and where that place drives (by the aggression; `out`);
/// whether the place changed.
pub fn get_check_chase_position(c: &mut Cpu, this: AIHighCop, out: Coord) -> bool {
    let at = abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1], r.0[2]) = (this.0, 0, out.0);
        let m = &mut c.m;
        let basic = this.ai_high_basic_cop();
        let (perp, cop, car) = (this.perp_target_(m), basic.cop_index_(m), car_of(m, this).car_index(m));
        r.enter(c, 0x8005_c74c, 0x8006_60c8, |c| aih_basicperp::check_chaser_position(c, perp.ai_high_basic_perp(), cop, car))
    });
    let m = &mut c.m;
    let changed = at != this.chase_index_(m);
    if changed {
        this.set_chase_index_(m, at);
    }
    let place = (this.chase_index_(m) as u32).wrapping_mul(12).wrapping_add((this.aggression_level_(m) as u32).wrapping_mul(72));
    let p = Coord(CHASE_POSITIONS.wrapping_add(place)).get(m);
    out.set(m, p);
    changed
}

pub fn abi_get_check_chase_position(c: &mut Cpu) {
    c.r[2] = get_check_chase_position(c, AIHighCop(c.r[4]), Coord(c.r[5])) as u32;
}

/// `AIHigh_Cop::CheckForNewTriggers` (AIH_COP.CPP, 0x80066144): after 23 s of the race, from
/// the last racer up, one that wants more police cars of its type (by its level; with no
/// crime only where no police are within 500 m) 7 s after its last pull-over: the police
/// triggers on its way since it was last checked (up to 31 slices ahead, at most 5 at a
/// time), the first out of the humans' sight taken by the chance (by its level's trigger
/// chance: doubled without a crime, always at hard), or any when it has none of its type;
/// the trigger, or none (0).
pub fn check_for_new_triggers(c: &mut Cpu, this: AIHighCop) -> u32 {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        m.w32(sp + 64, this.0);
        if SIM_COUNTER.get(m) < 1472 {
            return 0;
        }
        let manager = AITriggerTriggerManager(m.r32(TRIGGER_MANAGER_COPS));
        let k = this.ai_high_basic_cop().type_(m) as u32;
        let mut i = NUM_CARS.get(m) - 1;
        while i >= 0 {
            let m = &mut c.m;
            r.0[8] = i as u32;
            let racer = Car(m.r32(TOTAL_SORTED_LIST.wrapping_add(4 * i as u32)));
            r.0[5] = racer.0;
            i -= 1;
            if racer.car_flags(m) & 1 == 0 {
                continue;
            }
            let index = racer.car_index(m);
            let o = AIHighPlayer(obj(m, index));
            r.0[0] = o.0;
            let level = o.perp_chase_info_().chase_level_(m);
            let mut prob = o.new_trigger_prob_(m);
            let have = m.r32(o.0 + 0x70 + 4 * k) as i32;
            m.w32(sp + 20, have as u32);
            let want = if m.r32(o.0 + 0x78) != 0 {
                m.r32(level.wrapping_add(4 * k)) as i32
            } else {
                prob <<= 1;
                if m.r32(level.wrapping_add(4 * k)) as i32 <= 0 {
                    0
                } else {
                    let slice = Car(m.r32(o.0)).n().sim_road_info().slice(m) as i16 as i32;
                    r.0[2] = prob as u32;
                    let none = r.enter(c, 0x8006_7308, 0x8006_623c, |c| crate::aicop::no_cops_in_area(&mut c.m, slice, 0x1F4_0000));
                    none as i32
                }
            };
            let m = &mut c.m;
            if GAME_SETUP.skill(m) == 2 {
                prob = 0x1_0000;
            }
            r.0[2] = prob as u32;
            if SIM_COUNTER.get(m).wrapping_sub(o.ai_high_basic_perp().last_pull_over_time_(m)) < 448 || have >= want {
                continue;
            }
            let dir: i32 = if racer.current_speed(m) < 0 { -1 } else { 1 };
            let slice = racer.n().sim_road_info().slice(m) as i16 as i32;
            let ahead = {
                let by = dir.wrapping_mul(31);
                let s = slice.wrapping_add(by);
                let n = NUM_SLICES.get(m);
                if by >= 0 {
                    if s >= n { s - n } else { s }
                } else if s < 0 {
                    s + n
                } else {
                    s
                }
            };
            let last = o.last_trigger_check_slice_(m);
            o.set_last_trigger_check_slice_(m, ahead);
            let (mut from, to) = if last < ahead { (last, ahead) } else { (ahead, last) };
            let few = to - from < 6;
            let odds = prob.wrapping_mul(25);
            (r.0[1], r.0[4], r.0[6], r.0[7]) = (from as u32, to as u32, odds as u32, few as u32);
            while from < to && few {
                r.0[1] = from as u32;
                let found = r.enter(c, 0x8007_3404, 0x8006_6378, |c| crate::aitrigger::check_for_trigger_at_slice(c, manager, index, from));
                r.0[3] = found as u32;
                if found != -1 {
                    let m = &mut c.m;
                    let chance_odds = (odds << 2) / 0x1_0000;
                    let t = fast_random_step(m);
                    let roll = ((t >> 8 & 0xFFFF).wrapping_mul(25) >> 14) as i32;
                    (r.0[0], r.0[2]) = (roll as u32, chance_odds as u32);
                    let seen = r.enter(c, 0x8006_900c, 0x8006_63ec, |c| crate::ailife::is_slice_in_any_visible_area(&mut c.m, from)) != 0;
                    if !seen && (c.m.r32(sp + 20) == 0 || roll < chance_odds) {
                        return crate::aitrigger::get_trigger(&mut c.m, manager, found, sp + 16);
                    }
                }
                from += 1;
            }
        }
        0
    })
}

pub fn abi_check_for_new_triggers(c: &mut Cpu) {
    c.r[2] = check_for_new_triggers(c, AIHighCop(c.r[4]));
}

/// `AIHigh_Cop::~AIHigh_Cop` (AIHIGH.H, 0x8006646c).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], None, 0x8006_647c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{ARGS, HEAP_OBJS, OBJS, args, pursuit, pursuit_shuffle};
    use crate::test_util::{against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn cop_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn([u32; 7], [u32; 4]) -> Vec<u32>, Port); 11] = [
            ("Construct", nfs4rc::f_800643f0, |a, _| vec![OBJS + 0x700, a[0], a[2]], |c, a| Some(construct(c, AIHighCop(a[0]), Car(a[1]), a[2] as i32).0)),
            ("SetTuningLevers", nfs4rc::f_8006443c, |_, p| vec![p[1]], |c, a| {
                set_tuning_levers(&mut c.m, AIHighCop(a[0]));
                None
            }),
            ("HighExecute", nfs4rc::f_800644b4, |_, p| vec![p[1]], |c, a| {
                high_execute(c, AIHighCop(a[0]));
                None
            }),
            ("CheckForNeedyPlayers", nfs4rc::f_80065b94, |_, p| vec![p[1]], |c, _| Some(check_for_needy_players(&mut c.m) as u32)),
            ("CheckForWipeOut", nfs4rc::f_80065c30, |_, p| vec![p[1]], |c, a| {
                check_for_wipe_out(&mut c.m, AIHighCop(a[0]));
                None
            }),
            ("CheckForNewTarget", nfs4rc::f_80065da8, |_, p| vec![p[1]], |c, a| Some(check_for_new_target(c, AIHighCop(a[0])) as u32)),
            ("AssignToPlayer", nfs4rc::f_80066008, |a, p| vec![p[1], [0, p[0]][a[4] as usize]], |c, a| {
                assign_to_player(c, AIHighCop(a[0]), AIHighPlayer(a[1]));
                None
            }),
            ("GetCheckChasePosition", nfs4rc::f_80066094, |_, p| vec![p[1], ARGS + 0x60], |c, a| Some(get_check_chase_position(c, AIHighCop(a[0]), Coord(a[1])) as u32)),
            ("CheckForNewTriggers", nfs4rc::f_80066144, |_, p| vec![p[1]], |c, a| Some(check_for_new_triggers(c, AIHighCop(a[0])))),
            ("Destruct", nfs4rc::f_8006646c, |a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
            ("HighExecuteAgain", nfs4rc::f_800644b4, |_, p| vec![p[1]], |c, a| {
                high_execute(c, AIHighCop(a[0]));
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 2_000, 0x4010 + k as u64, pursuit_shuffle, original, &|c| {
                let (a, p) = (args(c), pursuit(c));
                if name == "GetCheckChasePosition" && c.m.r32(p[1] + 0x58) == 0 {
                    c.m.w32(p[1] + 0x58, p[0]);
                }
                call(a, p)
            }, port);
        }
    }

    /// The stack a police car's tick leaves (its states', the blockades', the triggers').
    #[test]
    fn cop_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 3] = [
            ("HighExecute", 0x8006_44b4, nfs4rc::f_800644b4),
            ("CheckForNewTriggers", 0x8006_6144, nfs4rc::f_80066144),
            ("CheckForNewTarget", 0x8006_5da8, nfs4rc::f_80065da8),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 600, 0x4050 + k as u64, pursuit_shuffle, &|c, _| vec![pursuit(c)[1]]);
        }
    }
}
