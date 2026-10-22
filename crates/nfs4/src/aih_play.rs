//! AIH_PLAY.CPP: `AIHigh_Player`, a racer the police chase (a human or an AI opponent): its
//! chase level (by its crimes and the time it has been chased; the police cars, blockades and
//! spike belts the level brings), the police cars kept available for it, blockades set up
//! ahead of it on a police trigger, and being pulled over: warnings, fines (busts), arrests.

use crate::abi::{self, Saved};
use crate::aih_basiccop::{ROAD_BLOCK_STATE, fast_random_step, per_mille};
use crate::aih_basicperp::{self, speech, speech_call};
use crate::aihigh::{self, HIGH_LEVEL_AI_OBJS, VT_BASIC_PERP, VT_PLAYER, obj};
use crate::aiworld::apx_spline_distance_to_slice;
use crate::fixed::{self, fixedmult};
use crate::globals::{
    AI_ELAPSED_TIME, AITUNE_ROUGH_LAP_TIME, BWORLDSM_SLICES, CARS_LIST, COP_CARS, GAME_SETUP, NUM_COP_CARS, NUM_HUMAN_RACE_CARS, NUM_RACE_CARS, NUM_SLICES, RACE_CARS,
    SIM_COUNTER, SIM_END_GAME,
};
use crate::types::{AICopPerpChaseInfo, AIHighBasicCop, AIHighPlayer, Car};
use ps1rt::{Cpu, Mem};

/// `copGame`: the police's levels for each kind of race (`copGame_t[8]`: the number of
/// levels and their `copLevel_t`s).
const COP_GAME: u32 = 0x8010_E178;
/// `triggerManagerCops`: the police's trigger manager (its triggers per lap at +4).
const TRIGGER_MANAGER_COPS: u32 = 0x8013_D8D8;
/// `AICop_numArrestedHumans`: the humans arrested (the race ends when all are).
pub const NUM_ARRESTED_HUMANS: crate::globals::I32 = crate::globals::I32(0x8013_D868);
/// `AIHigh_Player_kNumArrestsByLap`: the busts before an arrest, by the race's laps (2, 4,
/// other).
const ARRESTS_BY_LAP: u32 = 0x8010_DAF4;
/// `gBlockadeTypes`: the blockades' elements, five kinds.
const BLOCKADE_TYPES: u32 = 0x8013_D858;
/// `DashHUD_gInfo`: the dashboard's state per player (a word at +0x1c per car index).
const DASH_HUD: u32 = 0x8011_39E8;

/// `copLevel_t` fields: the chasers and blockaders by police type, blockaders in all, a spike
/// belt, police per lap, the lap fraction of engagement, the warning and beating ticks, the
/// warnings a level adds.
const LEVEL_CHASERS: u32 = 0x0;
const LEVEL_NUM_BLOCKADERS: u32 = 0x8;
const LEVEL_BLOCKADERS: u32 = 0xC;
const LEVEL_SPIKE_BELT: u32 = 0x14;
const LEVEL_COPS_PER_LAP: u32 = 0x20;
const LEVEL_ENGAGEMENT_LAP_FRACTION: u32 = 0x24;
const LEVEL_WARNING_TICKS: u32 = 0x28;
const LEVEL_BEATING_TICKS: u32 = 0x2C;
const LEVEL_NUM_WARNINGS_ADDED: u32 = 0x30;

/// `Car_tObj.AIFlags`: parked (a police car waiting); available to chase.
const AI_PARKED: i32 = 0x4;
const AI_AVAILABLE: i32 = 0x8;

fn info(this: AIHighPlayer) -> AICopPerpChaseInfo {
    this.perp_chase_info_()
}

fn car_of(m: &mut Mem, this: AIHighPlayer) -> Car {
    this.ai_high_basic_perp().ai_high_base().car_obj_(m)
}

fn level_field(m: &mut Mem, level: u32, field: u32) -> i32 {
    m.r32(level.wrapping_add(field)) as i32
}

/// A 16.16 value's whole part, rounded toward zero (the original's `x / 65536`).
fn whole(x: i32) -> i32 {
    x / 0x1_0000
}

/// The chase level `chaseLevelIndex_` taken up: its `copLevel_t`, the engagement time (a lap
/// fraction of the rough lap time) and the engagement it adds each tick (more over 2 laps,
/// less over 4); no blockade done.
fn set_level(c: &mut Cpu, info: AICopPerpChaseInfo, this: AIHighPlayer) {
    let m = &mut c.m;
    let index = info.chase_level_index_(m) as u32;
    let game = info.cop_game_info_(m);
    let levels = m.r32(game.wrapping_add(4));
    let level = levels.wrapping_add(index.wrapping_mul(0x34));
    m.w32(info.0 + 0x10, level);
    let lap = whole(level_field(m, level, LEVEL_ENGAGEMENT_LAP_FRACTION).wrapping_mul(AITUNE_ROUGH_LAP_TIME.get(m)));
    let per_tick = fixed::div(0x1_0000, lap << 5);
    this.set_engagement_time(m, lap << 21);
    info.set_engagement_percent_increase_per_tick_(m, per_tick);
    match GAME_SETUP.num_laps(m) {
        2 => info.set_engagement_percent_increase_per_tick_(m, fixedmult(per_tick, 0x1_3333)),
        4 => info.set_engagement_percent_increase_per_tick_(m, fixedmult(per_tick, 0xA8F5)),
        _ => {}
    }
    info.set_blockade_done_(m, 0);
}

trait Engagement {
    fn set_engagement_time(self, m: &mut Mem, v: i32);
    fn engagement_time(self, m: &mut Mem) -> i32;
}

impl Engagement for AIHighPlayer {
    fn set_engagement_time(self, m: &mut Mem, v: i32) {
        self.perp_chase_info_().set_engagement_time_(m, v);
    }
    fn engagement_time(self, m: &mut Mem) -> i32 {
        self.perp_chase_info_().engagement_time_(m)
    }
}

/// Back to the first chase level (the best kept, at least 0).
fn reset_level(c: &mut Cpu, info: AICopPerpChaseInfo, this: AIHighPlayer) {
    let m = &mut c.m;
    info.set_chase_level_index_(m, 0);
    if info.best_chase_level_index_(m) < 0 {
        info.set_best_chase_level_index_(m, 0);
    }
    set_level(c, info, this);
}

/// The chance of a police trigger per slice, by the level: the triggers per lap times its
/// police per lap.
fn set_trigger_prob(m: &mut Mem, this: AIHighPlayer) {
    let manager = m.r32(TRIGGER_MANAGER_COPS);
    let per_lap = m.r32(manager.wrapping_add(4)) as i32;
    let level = info(this).chase_level_(m);
    let prob = per_lap.wrapping_mul(level_field(m, level, LEVEL_COPS_PER_LAP));
    this.set_new_trigger_prob_(m, prob);
}

/// `AIHigh_Player::CheckIfABlockadeCanBeSetup` (AIH_PLAY.CPP, 0x80061bf0): a level with
/// blockaders, none done, its chasers all assigned (or two humans): the parked police cars
/// available, by type, up to the level's blockaders, set to block it (then any other
/// available one not blocking yet; with two humans a first type-0 car as a spike belt when
/// type 1 runs short); whether enough of type 0 and type 1 block.
pub fn check_if_a_blockade_can_be_setup(c: &mut Cpu, this: AIHighPlayer) -> bool {
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        let info = info(this);
        (r.0[1], r.0[2]) = (info.0, this.0);
        let level = info.chase_level_(&mut c.m);
        r.0[0] = level;
        r.call(c, nfs4rc::f_800e4f04, &[sp + 24, 0, 8], 0x8006_1c20); // memset
        r.call(c, nfs4rc::f_800e4f04, &[sp + 32, 0, 8], 0x8006_1c30);
        let m = &mut c.m;
        let two = NUM_HUMAN_RACE_CARS.get(m) == 2;
        let assigned = |m: &mut Mem, k: u32| m.r32(this.0 + 0x70 + 4 * k) as i32;
        let short = |m: &mut Mem, k: u32| assigned(m, k) < level_field(m, level, LEVEL_CHASERS + 4 * k) && !two;
        if level_field(m, level, LEVEL_NUM_BLOCKADERS) == 0 || info.blockade_done_(m) != 0 || short(m, 0) || short(m, 1) {
            return false;
        }
        let want = |m: &mut Mem, k: u32| m.r32(sp + 16 + 4 * k) as i32;
        let v = level_field(m, level, LEVEL_BLOCKADERS);
        m.w32(sp + 16, v as u32);
        let v = level_field(m, level, LEVEL_BLOCKADERS + 4);
        m.w32(sp + 20, v as u32);
        let cops: Vec<(Car, AIHighBasicCop)> = (0..NUM_COP_CARS.get(m).max(0) as u32)
            .map(|i| {
                let cop = Car(m.r32(COP_CARS + 4 * i));
                let index = cop.car_index(m);
                (cop, AIHighBasicCop(obj(m, index)))
            })
            .collect();
        let block = |m: &mut Mem, o: AIHighBasicCop, mode: i32| {
            o.blockade_().set_mode(m, mode);
            o.blockade_().set_target(m, this);
        };
        for &(cop, o) in &cops {
            if cop.ai_flags(m) & (AI_PARKED | AI_AVAILABLE) != AI_PARKED | AI_AVAILABLE {
                continue;
            }
            let k = o.type_(m) as u32;
            let have = m.r32(sp + 32 + 4 * k) as i32;
            if have >= want(m, k) {
                continue;
            }
            let ready = m.r32(sp + 24 + 4 * k);
            m.w32(sp + 24 + 4 * k, ready.wrapping_add(1));
            m.w32(sp + 32 + 4 * k, (have + 1) as u32);
            block(m, o, 1);
        }
        let got = |m: &mut Mem, k: u32| m.r32(sp + 32 + 4 * k) as i32;
        if got(m, 0) < want(m, 0) || got(m, 1) < want(m, 1) {
            for &(cop, o) in &cops {
                if cop.ai_flags(m) & (AI_PARKED | AI_AVAILABLE) != AI_AVAILABLE || o.blockade_().mode(m) == 2 {
                    continue;
                }
                let k = o.type_(m) as u32;
                let have = got(m, k);
                if have < want(m, k) {
                    m.w32(sp + 32 + 4 * k, (have + 1) as u32);
                    block(m, o, 1);
                }
            }
        }
        if NUM_HUMAN_RACE_CARS.get(m) != 1 && got(m, 1) < want(m, 1) {
            for &(cop, o) in &cops {
                if cop.ai_flags(m) & (AI_PARKED | AI_AVAILABLE) != AI_AVAILABLE || o.type_(m) != 0 {
                    continue;
                }
                let have = got(m, 1);
                if have < want(m, 1) && have == 0 {
                    m.w32(sp + 36, 1);
                    block(m, o, 4);
                }
            }
        }
        let ready = |m: &mut Mem, k: u32| m.r32(sp + 24 + 4 * k) as i32;
        ready(m, 0) >= want(m, 0) && ready(m, 1) >= want(m, 1)
    })
}

pub fn abi_check_if_a_blockade_can_be_setup(c: &mut Cpu) {
    c.r[2] = check_if_a_blockade_can_be_setup(c, AIHighPlayer(c.r[4])) as u32;
}

/// `slice + by` wrapped round the track (by one lap at most).
fn slice_plus(m: &mut Mem, slice: i32, by: i32) -> i32 {
    let s = slice.wrapping_add(by);
    let n = NUM_SLICES.get(m);
    if by >= 0 {
        if s >= n { s - n } else { s }
    } else if s < 0 {
        s + n
    } else {
        s
    }
}

/// A blockader's place across the road on its slice: the middle (the only one of its type
/// with no other), else a quarter in from either side by turns; its rotation.
fn blockade_lane(m: &mut Mem, b: crate::types::Blockade, alone: bool, k: i32) {
    let s = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((b.slice(m) as u32) << 5);
    let b29 = m.r8(s + 29) as i32;
    let left = ((m.r8(s + 30) as i32) << 15).wrapping_mul(b29 >> 4);
    let right = ((m.r8(s + 31) as i32) << 15).wrapping_mul(b29 & 15);
    let width = left.wrapping_add(right);
    let (lat, rotation) = if alone {
        (((width as u32) >> 1) as i32 - left, 255)
    } else if k & 1 == 0 {
        (left.wrapping_neg().wrapping_add(width / 4), 190)
    } else {
        (left.wrapping_neg().wrapping_add((width / 4).wrapping_mul(3)), -190)
    };
    b.set_lat_pos(m, lat);
    b.set_rotation(m, rotation);
}

/// A blockade's release time: random, 0.85 to 2.15 s on.
fn release_time(m: &mut Mem) -> i32 {
    let t = fast_random_step(m);
    ((t >> 8 & 0xFFFF).wrapping_mul(85197) >> 16).wrapping_add(0xD999) as i32
}

/// `AIHigh_Player::SetupBlockade` (AIH_PLAY.CPP, 0x80061f74): at the first police blockade
/// trigger 83 slices ahead out of the humans' sight and away from the police (or the race's
/// level steps up when there is none), the cars set to block it make blockades 3 and 5
/// slices on by pairs, of a kind by chance, across the road (type 1 first, then type 0 which
/// may face the other way), the first with the level's spike belt; the radio and dispatcher
/// say so.
pub fn setup_blockade(c: &mut Cpu, this: AIHighPlayer) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        let info = info(this);
        let m = &mut c.m;
        let car = car_of(m, this);
        let level = info.chase_level_(m);
        (r.0[3], r.0[4]) = (level, this.0);
        let dir = car.direction(m);
        let slice = car.n().sim_road_info().slice(m) as i16 as i32;
        let ahead = slice_plus(m, slice, dir.wrapping_mul(83));
        r.0[2] = ahead as u32;
        let v = level_field(m, level, LEVEL_BLOCKADERS);
        m.w32(sp + 16, v as u32);
        let v = level_field(m, level, LEVEL_BLOCKADERS + 4);
        m.w32(sp + 20, v as u32);
        let manager = crate::types::AITriggerTriggerManager(m.r32(TRIGGER_MANAGER_COPS));
        let mut at = crate::aitrigger::check_for_closest_trigger_of_type(m, manager, ahead, 2, dir);
        let mut tries = 0;
        let new_level = |c: &mut Cpu, r: &Saved| r.enter(c, 0x8006_29fc, 0x8006_292c, |c| check_for_new_level(c, this, true));
        if at == -1 {
            new_level(c, &r);
            return;
        }
        let mut trigger = crate::aitrigger::get_trigger(&mut c.m, manager, at, sp + 40);
        r.0[2] = 0;
        loop {
            r.0[0] = trigger;
            r.0[1] = 0;
            let ts = c.m.r32(trigger + 4) as i32;
            let mut seen = r.enter(c, 0x8006_900c, 0x8006_207c, |c| crate::ailife::is_slice_in_any_visible_area(&mut c.m, ts)) != 0;
            if !seen {
                seen = r.enter(c, 0x8006_90ac, 0x8006_2090, |c| crate::ailife::is_slice_close_to_any_cop_car(&mut c.m, ts)) != 0;
            }
            if !seen {
                break;
            }
            r.0[1] = 1;
            let m = &mut c.m;
            let n = NUM_SLICES.get(m);
            let next = if ts + 1 < n { ts + 1 } else { ts - (n - 1) };
            let dir = car_of(m, this).direction(m);
            at = crate::aitrigger::check_for_closest_trigger_of_type(m, manager, next, 2, dir);
            if at == -1 {
                new_level(c, &r);
                return;
            }
            tries += 1;
            r.0[2] = tries;
            trigger = crate::aitrigger::get_trigger(&mut c.m, manager, at, sp + 40);
            if tries >= 4 {
                break;
            }
        }
        let m = &mut c.m;
        r.0[0] = trigger;
        let base = m.r32(trigger + 4) as i32;
        r.0[2] = base as u32;
        let mut spike = if level_field(m, level, LEVEL_SPIKE_BELT) != 0 { base } else { -1 };
        r.0[6] = spike as u32;
        // (the cars set to block, by type; counted down from the level's blockaders)
        m.w32(sp + 24, 0);
        m.w32(sp + 28, 0);
        let v = m.r32(sp + 16);
        m.w32(sp + 32, v);
        let v = m.r32(sp + 20);
        m.w32(sp + 36, v);
        let n = NUM_COP_CARS.get(m);
        for i in 0..n.max(0) as u32 {
            let cop = Car(m.r32(COP_CARS + 4 * i));
            let index = cop.car_index(m);
            let o = AIHighBasicCop(obj(m, index));
            if cop.ai_flags(m) & AI_PARKED == 0 || o.blockade_().mode(m) != 1 {
                continue;
            }
            let k = o.type_(m) as u32;
            let left = m.r32(sp + 32 + 4 * k);
            if left == 0 {
                continue;
            }
            m.w32(sp + 32 + 4 * k, left.wrapping_sub(1));
            let k_ready = m.r32(sp + 24 + 4 * k);
            m.w32(sp + 24 + 4 * k, k_ready.wrapping_add(1));
        }
        let t = fast_random_step(m);
        let mut kind = m.r8(BLOCKADE_TYPES + (t >> 8 & 0xFFFF) % 5) as i32;
        r.0[7] = kind as u32;
        r.0[8] = info.0;
        (r.0[1], r.0[3], r.0[5]) = (0, 0, 0);
        m.w32(sp + 44, 0);
        let mut first = 0u32;
        let mut count = 0i32;
        let mut i = 0;
        while i < NUM_COP_CARS.get(&mut c.m) {
            let m = &mut c.m;
            r.0[3] = i as u32;
            let cop = Car(m.r32(COP_CARS + 4 * i as u32));
            let index = cop.car_index(m);
            let o = AIHighBasicCop(obj(m, index));
            i += 1;
            if cop.ai_flags(m) & AI_PARKED == 0 || o.blockade_().mode(m) != 1 {
                continue;
            }
            let b = o.blockade_();
            let type1 = o.type_(m) == 1 && m.r32(sp + 20) != 0;
            let budget = if type1 { sp + 20 } else { sp + 16 };
            let left = m.r32(budget);
            if left == 0 {
                continue;
            }
            if first == 0 {
                first = o.0;
                r.0[5] = first;
            }
            r.0[0] = b.0;
            m.w32(budget, left.wrapping_sub(1));
            b.set_blockade_speech_flags(m, 0);
            b.set_flags(m, kind);
            let lvl = info.chase_level_index_(m);
            b.set_chase_level(m, lvl);
            o.blockade_().set_mode(m, 2);
            let by = (count / 2 * 2 + 3).wrapping_mul(car_of(m, this).direction(m));
            kind = 0;
            r.0[7] = 0;
            let at = slice_plus(m, base, by);
            b.set_slice(m, at);
            // (the distance from the racer to the blockade, in whole metres ahead; none when
            // behind)
            let distance = |m: &mut Mem| {
                m.w32(sp + 48, 1);
                let me = car_of(m, this);
                let slice = b.slice(m);
                let d = apx_spline_distance_to_slice(m, me, slice);
                let metres = whole(d).wrapping_neg();
                b.set_initial_player_distance_meters_int(m, metres);
                if metres.wrapping_mul(car_of(m, this).direction(m)) < 0 {
                    b.set_initial_player_distance_meters_int(m, 0);
                }
            };
            if type1 {
                let dir = car_of(m, this).direction(m);
                b.set_direction(m, dir);
                let alone = m.r32(sp + 28) == 1 && m.r32(sp + 24) == 0;
                blockade_lane(m, b, alone, count);
                b.set_target(m, this);
                b.set_reverse(m, 0);
                let release = release_time(m);
                b.set_release_time(m, release);
                distance(m);
            } else {
                let chance = per_mille(m);
                let dir = car_of(m, this).direction(m);
                b.set_direction(m, dir);
                b.set_reverse(m, (chance < 300) as i32);
                let release = release_time(m);
                b.set_release_time(m, release);
                distance(m);
                let alone = m.r32(sp + 24) == 1 && m.r32(sp + 28) == 0;
                blockade_lane(m, b, alone, count);
                b.set_target(m, this);
            }
            if spike != -1 {
                m.w32(sp + 44, 1);
                b.set_request_spike_belt_at_slice(m, spike);
                spike = -1;
                r.0[6] = spike as u32;
            }
            info.set_blockade_done_(m, 1);
            count += 1;
            r.0[1] = count as u32;
        }
        r.0[3] = i as u32;
        if first == 0 {
            return;
        }
        let o = AIHighBasicCop(first);
        let m = &mut c.m;
        o.blockade_().set_blockade_speech_flags(m, 1);
        let cop_car = o.ai_high_base().car_obj_(m);
        let me = car_of(m, this);
        if m.r32(sp + 44) != 0 {
            let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[cop_car.0], 0x8006_2908);
            speech_call(c, &r, mobile, 11, 0x8006_295c, &[]);
        } else {
            let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[cop_car.0], 0x8006_2940);
            speech_call(c, &r, mobile, 10, 0x8006_295c, &[]);
        }
        let mobile = r.call_leaving(c, nfs4rc::f_8009834c, &[cop_car.0], 0x8006_2968);
        speech_call(c, &r, mobile, 6, 0x8006_2984, &[me.0]);
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8006_298c); // Speech::Dispatch
        speech_call(c, &r, d, 4, 0x8006_29a8, &[]);
        let d = r.call_leaving(c, nfs4rc::f_800969e0, &[], 0x8006_29b0);
        speech_call(c, &r, d, 5, 0x8006_29cc, &[cop_car.0]);
    })
}

pub fn abi_setup_blockade(c: &mut Cpu) {
    setup_blockade(c, AIHighPlayer(c.r[4]));
}

/// `AIHigh_Player::CheckForNewLevel` (AIH_PLAY.CPP, 0x800629fc): finished, back to the first
/// level, no crime; else its crimes looked for (the virtual `CheckForCrimes`), and with one,
/// a level up (at most the second last) when the engagement time has run out or `force`; a
/// new level changes the chance of police triggers.
pub fn check_for_new_level(c: &mut Cpu, this: AIHighPlayer, force: bool) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        let info = info(this);
        let m = &mut c.m;
        let old = info.chase_level_index_(m);
        (r.0[0], r.0[1], r.0[2], r.0[3]) = (old as u32, force as u32, this.0, old as u32);
        if car_of(m, this).stats().finish_type(m) >= 2 {
            reset_level(c, info, this);
            this.ai_high_basic_perp().basic_perp_info_().set_crime_(&mut c.m, 0);
            return;
        }
        let (o, f) = aihigh::virtual_fn(&mut c.m, this.0, 3);
        r.call_ptr(c, f, &[o], 0x8006_2b40);
        let m = &mut c.m;
        if this.ai_high_basic_perp().basic_perp_info_().crime_(m) != 0 && (force || whole(this.engagement_time(m)) <= 0) {
            let game = info.cop_game_info_(m);
            let levels = m.r32(game) as i32;
            let mut next = old.wrapping_add(1);
            if next >= levels {
                next = levels - 2;
            }
            info.set_chase_level_index_(m, next);
            if info.best_chase_level_index_(m) < next {
                info.set_best_chase_level_index_(m, next);
            }
            set_level(c, info, this);
        }
        let m = &mut c.m;
        if old != info.chase_level_index_(m) {
            set_trigger_prob(m, this);
        }
    })
}

pub fn abi_check_for_new_level(c: &mut Cpu) {
    check_for_new_level(c, AIHighPlayer(c.r[4]), c.r[5] != 0);
}

/// `AIHigh_Player::HandleSpeech` (AIH_PLAY.CPP, 0x80062cdc): the closest chaser (the lowest
/// car index of the first three places) radios the pull-over: a warning, a fine (the HUD's
/// notice; the busts said), an arrest; a road block up again.
pub fn handle_speech(c: &mut Cpu, this: AIHighPlayer) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        let m = &mut c.m;
        let perp = this.ai_high_basic_perp();
        let mut chaser = perp.position_vs_cop_list_(0).car_index(m);
        if chaser == -1 {
            chaser = Car(m.r32(COP_CARS)).car_index(m);
        }
        for p in 1..3 {
            let k = perp.position_vs_cop_list_(p).car_index(m);
            if k != -1 && k < chaser {
                chaser = k;
            }
        }
        r.0[1] = chaser as u32;
        let index = car_of(m, this).car_index(m);
        let what = match perp.pull_over_mode_(m) {
            1 => {
                r.call(c, nfs4rc::f_800da510, &[index as u32, 0], 0x8006_2d88); // the HUD's notice
                2
            }
            2 => {
                r.call(c, nfs4rc::f_800da510, &[index as u32, 1], 0x8006_2d98);
                let busts = this.num_busts_(&mut c.m);
                if busts < 3 { 8 } else { busts + 6 }
            }
            3 => {
                r.call(c, nfs4rc::f_800da510, &[index as u32, 2], 0x8006_2dbc);
                1
            }
            _ => 4,
        };
        r.0[0] = what as u32;
        let m = &mut c.m;
        if ROAD_BLOCK_STATE.get(m) != 0 {
            ROAD_BLOCK_STATE.set(m, 2);
        }
        let cop = Car(m.r32(CARS_LIST.wrapping_add((chaser as u32).wrapping_mul(4))));
        speech(c, &r, cop, 9, 0x8006_2df8, 0x8006_2e14, &[what as u32]);
    })
}

pub fn abi_handle_speech(c: &mut Cpu) {
    handle_speech(c, AIHighPlayer(c.r[4]));
}

/// `AIHigh_Player::MaintainAvailableCops` (AIH_PLAY.CPP, 0x80062e28): the police cars
/// available: those driving or blocking, then parked ones up to what the racers' levels ask
/// for by type (three with one racer, two with two humans, four otherwise, in all).
pub fn maintain_available_cops(c: &mut Cpu) {
    abi::frame(c, 40, |c, sp| {
        let r = Saved::of(c);
        r.call(c, nfs4rc::f_800e4f04, &[sp + 16, 0, 8], 0x8006_2e40); // memset
        r.call(c, nfs4rc::f_800e4f04, &[sp + 24, 0, 8], 0x8006_2e50);
        let m = &mut c.m;
        let mut spare = if NUM_RACE_CARS.get(m) == 1 {
            3
        } else if NUM_HUMAN_RACE_CARS.get(m) == 2 {
            2
        } else {
            4
        };
        for i in 0..NUM_RACE_CARS.get(m).max(0) as u32 {
            let car = Car(m.r32(RACE_CARS + 4 * i));
            let index = car.car_index(m);
            let o = obj(m, index);
            let level = m.r32(o.wrapping_add(0x8C + 0x10));
            for (k, f) in [(0, LEVEL_BLOCKADERS), (1, LEVEL_BLOCKADERS + 4), (0, LEVEL_CHASERS), (1, LEVEL_CHASERS + 4)] {
                let v = m.r32(sp + 16 + 4 * k).wrapping_add(m.r32(level.wrapping_add(f)));
                m.w32(sp + 16 + 4 * k, v);
            }
        }
        let n = NUM_COP_CARS.get(m);
        for i in 0..n.max(0) as u32 {
            let cop = Car(m.r32(COP_CARS + 4 * i));
            let index = cop.car_index(m);
            let o = AIHighBasicCop(obj(m, index));
            let mode = o.blockade_().mode(m);
            let busy = cop.ai_flags(m) & AI_PARKED == 0 || mode == 1 || mode == 2;
            if busy {
                let k = o.type_(m) as u32;
                let v = m.r32(sp + 24 + 4 * k);
                m.w32(sp + 24 + 4 * k, v.wrapping_add(1));
                spare -= 1;
                cop.update_ai_flags(m, |f| f | AI_AVAILABLE);
            } else {
                cop.update_ai_flags(m, |f| f & !AI_AVAILABLE);
            }
        }
        let mut i = 0;
        while spare > 0 && i < NUM_COP_CARS.get(m) {
            let cop = Car(m.r32(COP_CARS + 4 * i as u32));
            i += 1;
            if cop.ai_flags(m) & AI_AVAILABLE != 0 {
                continue;
            }
            let index = cop.car_index(m);
            let k = AIHighBasicCop(obj(m, index)).type_(m) as u32;
            let have = m.r32(sp + 24 + 4 * k) as i32;
            if have < m.r32(sp + 16 + 4 * k) as i32 {
                m.w32(sp + 24 + 4 * k, (have + 1) as u32);
                spare -= 1;
                cop.update_ai_flags(m, |f| f | AI_AVAILABLE);
            }
        }
    })
}

pub fn abi_maintain_available_cops(c: &mut Cpu) {
    maintain_available_cops(c);
}

/// `AIHigh_Player::AIHigh_Player(Car_tObj*)` (AIH_PLAY.CPP, 0x800630b0): its police game by the
/// race (one-on-one, with AI racers or not; two laps or not), at the first level, no
/// warnings or busts; with police, the chance of their triggers and the first level's
/// engagement.
pub fn construct(c: &mut Cpu, this: AIHighPlayer, car: Car) -> AIHighPlayer {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        r.enter(c, 0x8005_c8e4, 0x8006_30c8, |c| aih_basicperp::construct(c, this.ai_high_basic_perp(), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_PLAYER);
        let laps = (GAME_SETUP.num_laps(m) != 2) as u32;
        let race = if GAME_SETUP.comm_mode(m) == 1 { 2 } else { ((crate::globals::NUM_AI_RACE_CARS.get(m) > 0) as u32) << 2 };
        let info = info(this);
        let game = COP_GAME.wrapping_add(race.wrapping_add(laps).wrapping_mul(8));
        m.w32(info.0 + 4, game);
        info.set_chase_level_index_(m, 0);
        let index = info.chase_level_index_(m) as u32;
        info.set_engagement_time_(m, 0);
        info.set_best_chase_level_index_(m, 0);
        let levels = m.r32(game.wrapping_add(4));
        info.set_blockade_done_(m, 0);
        info.set_cop_free_ticks_(m, 0);
        info.set_total_engagement_percent_(m, 0);
        info.set_engagement_percent_increase_per_tick_(m, 0);
        m.w32(info.0 + 0x10, levels.wrapping_add(index.wrapping_mul(0x34)));
        this.set_num_warnings_(m, 0);
        this.set_num_busts_(m, 0);
        if GAME_SETUP.cops(m) != 0 {
            set_trigger_prob(m, this);
            let slice = car_of(m, this).n().sim_road_info().slice(m) as i16 as i32;
            this.set_last_trigger_check_slice_(m, slice);
            reset_level(c, info, this);
        }
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighPlayer(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_Player::HandleCops` (AIH_PLAY.CPP, 0x800632b4): with police, the cars kept
/// available, a blockade set up when it can be, blockaders sent off at a level without
/// them; chased, the engagement time runs (half as fast going the wrong way) and over a lap's
/// worth the engagement grows; else the police-free time; the level, the pull-over.
pub fn handle_cops(c: &mut Cpu, this: AIHighPlayer) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let info = info(this);
        let m = &mut c.m;
        let level = info.chase_level_(m);
        (r.0[0], r.0[1], r.0[2]) = (info.0, this.0, level);
        if NUM_COP_CARS.get(m) == 0 {
            return;
        }
        r.enter(c, 0x8006_2e28, 0x8006_32ec, maintain_available_cops);
        if r.enter(c, 0x8006_1bf0, 0x8006_32f4, |c| check_if_a_blockade_can_be_setup(c, this)) {
            r.enter(c, 0x8006_1f74, 0x8006_3304, |c| setup_blockade(c, this));
        }
        if level_field(&mut c.m, level, LEVEL_NUM_BLOCKADERS) == 0 {
            r.enter(c, 0x8006_3454, 0x8006_3320, |c| cleanup_blockaders(c, this, false));
        }
        let m = &mut c.m;
        let car = car_of(m, this);
        let backwards = car.current_speed(m).wrapping_mul(car.direction(m)) < 0;
        let e = AI_ELAPSED_TIME.get(m);
        let chasers = (m.r32(this.0 + 0x70) as i32).wrapping_add(m.r32(this.0 + 0x74) as i32);
        if chasers > 0 {
            info.set_cop_free_ticks_(m, 0);
            let t = this.engagement_time(m);
            if whole(t) >= -1 {
                this.set_engagement_time(m, t.wrapping_sub(if backwards { e << 15 } else { e << 16 }));
                let left = whole(info.engagement_time_(m));
                let level = info.chase_level_(m);
                let lap = whole(level_field(m, level, LEVEL_ENGAGEMENT_LAP_FRACTION).wrapping_mul(AITUNE_ROUGH_LAP_TIME.get(m)));
                if left < (lap << 5) - 128 {
                    let add = info.engagement_percent_increase_per_tick_(m).wrapping_mul(e);
                    info.update_total_engagement_percent_(m, |p| p.wrapping_add(add));
                }
            }
        } else {
            info.update_cop_free_ticks_(m, |t| t.wrapping_add(e));
        }
        r.enter(c, 0x8006_29fc, 0x8006_3434, |c| check_for_new_level(c, this, false));
        r.enter(c, 0x8006_356c, 0x8006_343c, |c| handle_pull_over(c, this));
    })
}

pub fn abi_handle_cops(c: &mut Cpu) {
    handle_cops(c, AIHighPlayer(c.r[4]));
}

/// `AIHigh_Player::CleanupBlockaders` (AIH_PLAY.CPP, 0x80063454): the police cars set to block
/// it (and blocking, once it is arrested, finished or `force`) stop and chase nobody.
pub fn cleanup_blockaders(c: &mut Cpu, this: AIHighPlayer, force: bool) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let car = car_of(m, this);
        let all = car.stats().num_arrests(m) > 0 || car.stats().finish_type(m) >= 2 || force;
        (r.0[2], r.0[3], r.0[4]) = (all as u32, this.0, HIGH_LEVEL_AI_OBJS);
        for i in 0..NUM_COP_CARS.get(m) {
            let m = &mut c.m;
            (r.0[0], r.0[1]) = (COP_CARS + 4 * i as u32, i as u32);
            let cop = Car(m.r32(COP_CARS + 4 * i as u32));
            let index = cop.car_index(m);
            let o = AIHighBasicCop(obj(m, index));
            let b = o.blockade_();
            let mode = b.mode(m);
            if !(mode == 1 || mode == 4 || (mode == 2 && all)) || b.target(m) != this {
                continue;
            }
            b.set_mode(m, 0);
            r.enter(c, 0x8006_6008, 0x8006_3540, |c| crate::aih_cop::assign_to_player(c, crate::types::AIHighCop(o.0), AIHighPlayer(0)));
        }
    })
}

pub fn abi_cleanup_blockaders(c: &mut Cpu) {
    cleanup_blockaders(c, AIHighPlayer(c.r[4]), c.r[5] != 0);
}

/// `AIHigh_Player::HandlePullOver` (AIH_PLAY.CPP, 0x8006356c): pulled over, once the beating
/// is done: arrested (a human's arrest counted; all humans arrested end the race), back to
/// the first level, no crime, the close police sent away, the car put back on the road; else
/// a police car catching it pulls it over (the blockaders let go): a warning while the level
/// gives them (twice at most), a fine, and after enough fines (by the laps) at the top level
/// with one human an arrest; the radio says so.
pub fn handle_pull_over(c: &mut Cpu, this: AIHighPlayer) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this.0;
        let perp = this.ai_high_basic_perp();
        let info = info(this);
        if perp.pull_over_mode_(&mut c.m) != 0 {
            let m = &mut c.m;
            let e = AI_ELAPSED_TIME.get(m);
            perp.update_beating_ticks_left_(m, |t| t.wrapping_sub(e));
            if perp.beating_ticks_left_(m) > 0 {
                return;
            }
            let car = car_of(m, this);
            let index = car.car_index(m);
            if index < 2 {
                r.call(c, nfs4rc::f_800da53c, &[index as u32], 0x8006_35d0); // the HUD's notice off
            }
            let m = &mut c.m;
            let now = SIM_COUNTER.get(m);
            perp.set_last_pull_over_time_(m, now);
            let mode = perp.pull_over_mode_(m);
            if mode == 3 {
                if car.car_flags(m) & aihigh::CAR_HUMAN != 0 {
                    NUM_ARRESTED_HUMANS.update(m, |n| n.wrapping_add(1));
                }
                car.stats().set_finish_type(m, 3);
            }
            r.0[0] = info.0;
            if NUM_ARRESTED_HUMANS.get(m) == NUM_HUMAN_RACE_CARS.get(m) {
                SIM_END_GAME.set(m, 1);
                r.enter(c, 0x800b_92d8, 0x8006_365c, |c| crate::stats::extrapolate_opponent_times(c, 2));
            }
            reset_level(c, info, this);
            perp.basic_perp_info_().set_crime_(&mut c.m, 0);
            r.enter(c, 0x8005_c56c, 0x8006_3748, |c| aih_basicperp::remove_close_cops(c, perp));
            let m = &mut c.m;
            let mode = perp.pull_over_mode_(m);
            let keep = mode == 3 && NUM_HUMAN_RACE_CARS.get(m) == 1 && car.car_flags(m) & aihigh::CAR_OPPONENT == 0;
            if !keep {
                r.enter(c, 0x8008_7088, 0x8006_3798, |c| crate::cars::reset_collided_cars(c, car, 1));
            }
            let m = &mut c.m;
            if perp.pull_over_mode_(m) != 3 {
                car.set_pull_over(m, 0);
            } else if NUM_HUMAN_RACE_CARS.get(m) == 2 && NUM_ARRESTED_HUMANS.get(m) != 2 {
                let index = car.car_index(m) as u32;
                m.w32(DASH_HUD.wrapping_add(index.wrapping_mul(4)).wrapping_add(0x1C), 0);
            }
            perp.set_pull_over_mode_(m, 0);
            return;
        }
        if !r.enter(c, 0x8005_bf7c, 0x8006_3808, |c| aih_basicperp::check_if_caught(c, perp)) {
            return;
        }
        let m = &mut c.m;
        let car = car_of(m, this);
        car.set_pull_over(m, 1);
        r.enter(c, 0x8006_3454, 0x8006_3824, |c| cleanup_blockaders(c, this, true));
        let m = &mut c.m;
        let level = info.chase_level_(m);
        let lap = whole(level_field(m, level, LEVEL_ENGAGEMENT_LAP_FRACTION).wrapping_mul(AITUNE_ROUGH_LAP_TIME.get(m))) << 5;
        let engaged = this.engagement_time(m);
        let beating = level_field(m, level, LEVEL_BEATING_TICKS);
        perp.set_beating_ticks_left_(m, beating);
        let now = SIM_COUNTER.get(m);
        perp.set_last_pull_over_time_(m, now);
        let chased = lap.wrapping_sub(whole(engaged));
        let mut warn = false;
        if perp.basic_perp_info_().crime_(m) != 4 && car.stats().num_fines(m) == 0 {
            let best = info.best_chase_level_index_(m) as u32;
            let game = info.cop_game_info_(m);
    let levels = m.r32(game.wrapping_add(4));
            if level_field(m, levels.wrapping_add(best.wrapping_mul(0x34)), LEVEL_NUM_WARNINGS_ADDED) != 0 {
                let level = info.chase_level_(m);
                warn = chased < level_field(m, level, LEVEL_WARNING_TICKS);
            }
        }
        let mode = if warn && this.num_warnings_(m) < 2 {
            let level = info.chase_level_(m);
            let added = level_field(m, level, LEVEL_NUM_WARNINGS_ADDED);
            this.update_num_warnings_(m, |w| w.wrapping_add(added));
            car.stats().update_num_warnings(m, |w| w.wrapping_add(1));
            1
        } else {
            this.update_num_busts_(m, |b| b.wrapping_add(1));
            car.stats().update_num_fines(m, |f| f.wrapping_add(1));
            let laps = match GAME_SETUP.num_laps(m) {
                2 => 0,
                4 => 1,
                _ => 2,
            };
            let limit = m.r32(ARRESTS_BY_LAP + 4 * laps) as i32;
            let game = info.cop_game_info_(m);
            let top = info.best_chase_level_index_(m) == m.r32(game) as i32 - 1;
            if this.num_busts_(m) < limit && !(top && NUM_HUMAN_RACE_CARS.get(m) == 1) {
                2
            } else {
                perp.update_beating_ticks_left_(m, |t| t.wrapping_add(192));
                car.stats().update_num_arrests(m, |a| a.wrapping_add(1));
                3
            }
        };
        perp.set_pull_over_mode_(m, mode);
        r.enter(c, 0x8006_2cdc, 0x8006_3a24, |c| handle_speech(c, this));
    })
}

pub fn abi_handle_pull_over(c: &mut Cpu) {
    handle_pull_over(c, AIHighPlayer(c.r[4]));
}

/// `AIHigh_Player::~AIHigh_Player` (AIHIGH.H, 0x80063a38).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8006_3a50);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{HEAP_OBJS, OBJS, args, pursuit, pursuit_shuffle};
    use crate::test_util::{against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn play_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, [u32; 7], [u32; 4]) -> Vec<u32>, Port); 11] = [
            ("CheckIfABlockadeCanBeSetup", nfs4rc::f_80061bf0, |_, _, p| vec![p[0]], |c, a| Some(check_if_a_blockade_can_be_setup(c, AIHighPlayer(a[0])) as u32)),
            ("SetupBlockade", nfs4rc::f_80061f74, |_, _, p| vec![p[0]], |c, a| {
                setup_blockade(c, AIHighPlayer(a[0]));
                None
            }),
            ("CheckForNewLevel", nfs4rc::f_800629fc, |_, a, p| vec![p[0], a[4]], |c, a| {
                check_for_new_level(c, AIHighPlayer(a[0]), a[1] != 0);
                None
            }),
            ("HandleSpeech", nfs4rc::f_80062cdc, |_, _, p| vec![p[0]], |c, a| {
                handle_speech(c, AIHighPlayer(a[0]));
                None
            }),
            ("MaintainAvailableCops", nfs4rc::f_80062e28, |_, _, p| vec![p[0]], |c, _| {
                maintain_available_cops(c);
                None
            }),
            ("Construct", nfs4rc::f_800630b0, |_, a, _| vec![OBJS + 0x700, a[0]], |c, a| Some(construct(c, AIHighPlayer(a[0]), Car(a[1])).0)),
            ("HandleCops", nfs4rc::f_800632b4, |_, _, p| vec![p[0]], |c, a| {
                handle_cops(c, AIHighPlayer(a[0]));
                None
            }),
            ("CleanupBlockaders", nfs4rc::f_80063454, |_, a, p| vec![p[0], a[4]], |c, a| {
                cleanup_blockaders(c, AIHighPlayer(a[0]), a[1] != 0);
                None
            }),
            ("HandlePullOver", nfs4rc::f_8006356c, |_, _, p| vec![p[0]], |c, a| {
                handle_pull_over(c, AIHighPlayer(a[0]));
                None
            }),
            ("Destruct", nfs4rc::f_80063a38, |_, a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
            ("HandlePullOverOpponent", nfs4rc::f_8006356c, |_, _, p| vec![p[3]], |c, a| {
                handle_pull_over(c, AIHighPlayer(a[0]));
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3E10 + k as u64, pursuit_shuffle, original, &|c| {
                let (a, p) = (args(c), pursuit(c));
                call(c, a, p)
            }, port);
        }
    }

    /// The race's human (an object of the class, else the first), its objects of any role.
    fn race_human(c: &mut Cpu) -> u32 {
        let n = crate::globals::NUM_CARS.get(&mut c.m) as u32;
        (0..n).map(|i| OBJS + 0x100 * i).find(|&o| c.m.r32(o + 0x14) == crate::aihigh::VT_HUMAN).unwrap_or(OBJS)
    }

    /// The police's handling of a race's human (the race shuffle: any objects round it).
    #[test]
    fn play_race_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 7] = [
            ("MaintainAvailableCops", nfs4rc::f_80062e28, |c, _| {
                maintain_available_cops(c);
                None
            }),
            ("CheckIfABlockadeCanBeSetup", nfs4rc::f_80061bf0, |c, a| Some(check_if_a_blockade_can_be_setup(c, AIHighPlayer(a[0])) as u32)),
            ("SetupBlockade", nfs4rc::f_80061f74, |c, a| {
                setup_blockade(c, AIHighPlayer(a[0]));
                None
            }),
            ("CleanupBlockaders", nfs4rc::f_80063454, |c, a| {
                cleanup_blockaders(c, AIHighPlayer(a[0]), false);
                None
            }),
            ("CheckForNewLevel", nfs4rc::f_800629fc, |c, a| {
                check_for_new_level(c, AIHighPlayer(a[0]), false);
                None
            }),
            ("HandlePullOver", nfs4rc::f_8006356c, |c, a| {
                handle_pull_over(c, AIHighPlayer(a[0]));
                None
            }),
            ("HandleCops", nfs4rc::f_800632b4, |c, a| {
                handle_cops(c, AIHighPlayer(a[0]));
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3E80 + k as u64, crate::aihigh::tests::SHUFFLES[0].1, original, &|c| {
                let o = race_human(c);
                if name == "CleanupBlockaders" { vec![o, 0] } else { vec![o] }
            }, port);
        }
    }

    /// The stack the police's handling of a player leaves (the blockades, the levels).
    #[test]
    fn play_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 5] = [
            ("HandleCops", 0x8006_32b4, nfs4rc::f_800632b4),
            ("SetupBlockade", 0x8006_1f74, nfs4rc::f_80061f74),
            ("HandlePullOver", 0x8006_356c, nfs4rc::f_8006356c),
            ("CheckIfABlockadeCanBeSetup", 0x8006_1bf0, nfs4rc::f_80061bf0),
            ("Construct", 0x8006_30b0, nfs4rc::f_800630b0),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 300, 0x3E50 + k as u64, pursuit_shuffle, &|c, _| {
                let (a, p) = (args(c), pursuit(c));
                if name == "Construct" { vec![OBJS + 0x700, a[0]] } else { vec![p[0]] }
            });
        }
    }
}
