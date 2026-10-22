//! AIH_OPP.CPP: `AIHigh_Opponent`, an AI racer: chased by the police it may wipe out (a short
//! loss of control) when a human leads it by levels or fines; it attacks a human (a chase in
//! murder mode) when its script reacts, when it can rear-end or smack one just ahead, or when
//! a human has hit it often enough.

use crate::abi::{self, Saved};
use crate::aih_basiccop::fast_random_step;
use crate::aihigh::{self, VT_BASIC_PERP, VT_OPPONENT, obj};
use crate::aiscript::do_re_action;
use crate::aiworld::spline_distance;
use crate::globals::{AI_ELAPSED_TIME, BWORLDSM_SLICES, CARS_LIST, GAME_SETUP, HUMAN_RACE_CARS, NUM_COP_CARS, NUM_HUMAN_RACE_CARS, SIM_COUNTER};
use crate::types::{AIHighOpponent, AIHighPlayer, AIScript, AIStateBase, AIStateChase, Car, Coord};
use ps1rt::{Cpu, Mem};

/// `attackMode_`: none, rear-ending, its script's reaction, provoked by hits.
const ATTACK_REAR_END: i32 = 1;
const ATTACK_SCRIPT: i32 = 2;
const ATTACK_PROVOKED: i32 = 3;

/// The script's reactions (`AIScript_DoReAction`): an attack; a rear end.
const REACTION_ATTACK: i32 = 64;
const REACTION_REAR_END: i32 = 256;

fn player(this: AIHighOpponent) -> AIHighPlayer {
    this.ai_high_player()
}

fn car_of(m: &mut Mem, this: AIHighOpponent) -> Car {
    Car(m.r32(this.0))
}

fn script(m: &mut Mem, this: AIHighOpponent) -> AIScript {
    AIScript(car_of(m, this).0 + 0x4F4)
}

/// `AIHigh_Opponent::CheckForWipeOut` (AIH_OPP.CPP, 0x80063af8): chased (with a crime and
/// chasers), 10 s after its last wipe-out, by chance (by the time step) it wipes out for 3 s;
/// below the top level, more likely (by four) when a fast human is ahead of it by levels (a
/// low one while it is high) or fines.
pub fn check_for_wipe_out(m: &mut Mem, this: AIHighOpponent) {
    let p = player(this);
    let level = p.perp_chase_info_().chase_level_index_(m);
    let car = car_of(m, this);
    let fines = car.stats().num_fines(m);
    if NUM_COP_CARS.get(m) == 0 || SIM_COUNTER.get(m) < car.wipe_out_end_tick(m) {
        return;
    }
    let b = p.ai_high_basic_perp();
    let info = b.basic_perp_info_();
    if info.crime_(m) == 0 || info.cops_assigned_(m, 0).wrapping_add(info.cops_assigned_(m, 1)) == 0 {
        return;
    }
    let now = SIM_COUNTER.get(m);
    if now.wrapping_sub(car.wipe_out_end_tick(m)) < 640 {
        return;
    }
    let t = fast_random_step(m);
    let e = AI_ELAPSED_TIME.get(m);
    let chance = (t >> 8 & 0xFFFF) as i32;
    if chance < e.wrapping_mul(3) {
        car.set_wipe_out_end_tick(m, now.wrapping_add(192));
    }
    let chase = p.perp_chase_info_();
    let game = chase.cop_game_info_(m);
    let levels = m.r32(game) as i32;
    if chase.best_chase_level_index_(m) == levels - 1 {
        return;
    }
    let odds = e.wrapping_mul(29) << 2;
    for i in 0..NUM_HUMAN_RACE_CARS.get(m) {
        let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
        let index = human.car_index(m);
        let fast = human.current_speed(m).wrapping_abs() > 0xD_5554;
        let human_level = AIHighPlayer(obj(m, index)).perp_chase_info_().chase_level_index_(m);
        let ahead_by_fines = human.stats().num_fines(m).wrapping_sub(fines) >= 2;
        if fast && ((human_level < 2 && level >= 3) || ahead_by_fines) && chance < odds {
            let now = SIM_COUNTER.get(m);
            car.set_wipe_out_end_tick(m, now.wrapping_add(192));
        }
    }
}

pub fn abi_check_for_wipe_out(c: &mut Cpu) {
    check_for_wipe_out(&mut c.m, AIHighOpponent(c.r[4]));
}

/// Just ahead of `car` along its way (1 to 40 m) and closer across than twice that.
fn just_ahead(m: &mut Mem, car: Car, other: Car, d: i32) -> (bool, i32) {
    let ahead = d.wrapping_mul(car.direction(m));
    let across = car.road_position(m).wrapping_sub(other.road_position(m)).wrapping_abs();
    (ahead.wrapping_add(-0x1_0001) as u32 <= 0x26_FFFE && across < ahead << 1, ahead)
}

/// The racer's chance this tick by a probability mask of its personality's (`field`).
fn by_mask(m: &mut Mem, car: Car, field: u32) -> bool {
    let index = car.car_index(m);
    let person = car.personality(m).0;
    let mask = m.r32(person.wrapping_add(field));
    let t = (SIM_COUNTER.get(m).wrapping_add(index.wrapping_mul(123)) as u32) & mask;
    t == mask
}

/// `AIHigh_Opponent::DoRearEnder` (AIH_OPP.CPP, 0x80063cd8): the car its script says to rear-end
/// when it is just ahead and fast (over 11 m/s); else, simulated fully and going over 20 m/s,
/// a human on the road just ahead (rear-ended by the personality's chance) or beside within
/// 4 m (smacked by chance); the car's index, or none (-1).
pub fn do_rear_ender(c: &mut Cpu, this: AIHighOpponent) -> i32 {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = this.0;
        let m = &mut c.m;
        let s = script(m, this);
        let target = do_re_action(m, s, REACTION_REAR_END);
        r.0[1] = target as u32;
        if target != -1 {
            let other = Car(m.r32(CARS_LIST.wrapping_add((target as u32).wrapping_mul(4))));
            r.0[0] = other.0;
            let car = car_of(m, this);
            let d = r.enter(c, 0x8007_3b04, 0x8006_3d34, |c| spline_distance(c, other, car));
            let m = &mut c.m;
            let car = car_of(m, this);
            let (near, _) = just_ahead(m, car, other, d);
            if near && other.current_speed(m).wrapping_abs() > 0xB_1C71 {
                return target;
            }
        }
        let m = &mut c.m;
        let car = car_of(m, this);
        if car.n().sim_optz(m) != 0 || car.current_speed(m).wrapping_abs() <= 0x14_0000 {
            return -1;
        }
        (r.0[1], r.0[2], r.0[4]) = (HUMAN_RACE_CARS, 0, crate::globals::SIM_GLOBAL.0);
        for i in 0..NUM_HUMAN_RACE_CARS.get(m) {
            let m = &mut c.m;
            (r.0[1], r.0[2]) = (HUMAN_RACE_CARS + 4 * i as u32, i as u32);
            let human = Car(m.r32(HUMAN_RACE_CARS + 4 * i as u32));
            r.0[0] = human.0;
            let slice = human.n().sim_road_info().slice(m) as i16 as i32;
            let sl = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((slice as u32) << 5);
            let b29 = m.r8(sl + 29) as i32;
            let left = ((m.r8(sl + 30) as i32) << 15).wrapping_mul(b29 >> 4);
            let at = human.road_position(m);
            if at < left.wrapping_neg() {
                continue;
            }
            let right = ((m.r8(sl + 31) as i32) << 15).wrapping_mul(b29 & 15);
            if right < at {
                continue;
            }
            let car = car_of(m, this);
            let d = r.enter(c, 0x8007_3b04, 0x8006_3e98, |c| spline_distance(c, human, car));
            let m = &mut c.m;
            let car = car_of(m, this);
            let (behind, ahead) = just_ahead(m, car, human, d);
            if behind && by_mask(m, car, 0x48) {
                return human.car_index(m);
            }
            if (ahead.wrapping_add(0x3_FFFF) as u32) <= 0x7_FFFE && by_mask(m, car, 0x4C) {
                return human.car_index(m);
            }
        }
        -1
    })
}

pub fn abi_do_rear_ender(c: &mut Cpu) {
    c.r[2] = do_rear_ender(c, AIHighOpponent(c.r[4])) as u32;
}

/// `AIHigh_Opponent::HighExecute` (AIH_OPP.CPP, 0x80063fac, virtual): with no state, driving
/// (the state not run); driving: the police, a wipe-out, then an attack (its script's, a
/// rear end, provoked) chases its target harder; attacking: the police, a wipe-out, murder
/// mode, until the attack is over (the script or rear end gone, the provoked time out), then
/// driving the track's way again. Then its state.
pub fn high_execute(c: &mut Cpu, this: AIHighOpponent) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this.0;
        let p = player(this);
        let kind = c.m.r32(this.0 + 8);
        let normal = |c: &mut Cpu, r: &mut Saved, ra: [u32; 2]| {
            let car = car_of(&mut c.m, this);
            let s = crate::aistate::pool_new(&mut c.m);
            r.enter(c, 0x8007_03bc, ra[0], |c| crate::aistate::normal_construct(&mut c.m, AIStateBase(s), car));
            r.0[0] = s;
            aihigh::set_state(c, r, this.0, s, 2, ra[1]);
        };
        match kind {
            0 => {
                normal(c, &mut r, [0x8006_400c, 0x8006_4038]);
                return;
            }
            2 => {
                r.enter(c, 0x8006_32b4, 0x8006_4050, |c| crate::aih_play::handle_cops(c, p));
                r.enter(c, 0x8006_3af8, 0x8006_4058, |c| check_for_wipe_out(&mut c.m, this));
                let m = &mut c.m;
                let s = script(m, this);
                let mut target = do_re_action(m, s, REACTION_ATTACK);
                (r.0[1], r.0[3]) = (target as u32, target as u32);
                if target != -1 {
                    this.set_attack_mode_(m, ATTACK_SCRIPT);
                } else {
                    target = r.enter(c, 0x8006_3cd8, 0x8006_4088, |c| do_rear_ender(c, this));
                    (r.0[0], r.0[1]) = (target as u32, target as u32);
                    if target != -1 {
                        this.set_attack_mode_(&mut c.m, ATTACK_REAR_END);
                    } else {
                        target = r.enter(c, 0x8006_431c, 0x8006_40a4, |c| do_provoked_attack(&mut c.m, this));
                        r.0[1] = target as u32;
                        if target != -1 {
                            this.set_attack_mode_(&mut c.m, ATTACK_PROVOKED);
                        }
                    }
                }
                if this.attack_mode_(&mut c.m) != 0 {
                    r.call(c, nfs4rc::f_800e4f04, &[sp + 40, 0, 12], 0x8006_40d0); // memset
                    let m = &mut c.m;
                    let aggression = (this.attack_mode_(m) - 1).max(2);
                    r.0[0] = aggression as u32;
                    let s = crate::aistate::pool_new(m);
                    let args = [32, 0x96_0000, 0x96_0000, aggression, 0x1_0000];
                    for (k, v) in args.iter().enumerate() {
                        m.w32(sp + 16 + 4 * k as u32, *v as u32);
                    }
                    let car = car_of(m, this);
                    let other = Car(m.r32(CARS_LIST.wrapping_add((target as u32).wrapping_mul(4))));
                    r.enter(c, 0x8007_04dc, 0x8006_413c, |c| {
                        crate::aistate::chase_construct(c, AIStateChase(s), car, other, Coord(sp + 40), args[0], args[1], args[2], args[3], args[4])
                    });
                    r.0[0] = s;
                    aihigh::set_state(c, &r, this.0, s, 4, 0x8006_4168);
                }
            }
            4 => {
                let chase = AIStateChase(c.m.r32(this.0 + 4));
                r.0[0] = chase.0;
                r.enter(c, 0x8006_32b4, 0x8006_4184, |c| crate::aih_play::handle_cops(c, p));
                r.enter(c, 0x8006_3af8, 0x8006_418c, |c| check_for_wipe_out(&mut c.m, this));
                crate::aistate::set_murder_mode(&mut c.m, chase, 1, 15);
                let m = &mut c.m;
                match this.attack_mode_(m) {
                    ATTACK_SCRIPT => {
                        let s = script(m, this);
                        if do_re_action(m, s, REACTION_ATTACK) == -1 {
                            this.set_attack_mode_(m, 0);
                        }
                    }
                    ATTACK_REAR_END => {
                        if r.enter(c, 0x8006_3cd8, 0x8006_4204, |c| do_rear_ender(c, this)) == -1 {
                            this.set_attack_mode_(&mut c.m, 0);
                        }
                    }
                    ATTACK_PROVOKED => {
                        let e = AI_ELAPSED_TIME.get(m);
                        this.update_attack_ticks_left_(m, |t| t.wrapping_sub(e));
                        if this.attack_ticks_left_(m) <= 0 {
                            this.set_attack_mode_(m, 0);
                        }
                    }
                    _ => {}
                }
                let m = &mut c.m;
                if this.attack_mode_(m) == 0 {
                    let dir = if GAME_SETUP.reverse_track(m) != 0 { -1 } else { 1 };
                    car_of(m, this).set_desired_direction(m, dir);
                    normal(c, &mut r, [0x8006_4274, 0x8006_42a0]);
                    this.set_attack_mode_(&mut c.m, 0);
                }
            }
            _ => {}
        }
        let state = AIStateBase(c.m.r32(this.0 + 4));
        r.enter(c, 0x8007_0244, 0x8006_42bc, |c| crate::aistate::state_execute(c, state));
    })
}

pub fn abi_high_execute(c: &mut Cpu) {
    high_execute(c, AIHighOpponent(c.r[4]));
}

/// `AIHigh_Opponent::AIHigh_Opponent(Car_tObj*)` (AIH_OPP.CPP, 0x800642d8): not attacking.
pub fn construct(c: &mut Cpu, this: AIHighOpponent, car: Car) -> AIHighOpponent {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this.0;
        r.enter(c, 0x8006_30b0, 0x8006_42ec, |c| crate::aih_play::construct(c, player(this), car));
        let m = &mut c.m;
        aihigh::set_vt(m, this.0, VT_OPPONENT);
        this.set_attack_mode_(m, 0);
        this.set_last_human_hitter_(m, Car(0));
        this.set_hit_count_(m, 0);
        this.set_attack_ticks_left_(m, 0);
        this
    })
}

pub fn abi_construct(c: &mut Cpu) {
    c.r[2] = construct(c, AIHighOpponent(c.r[4]), Car(c.r[5])).0;
}

/// `AIHigh_Opponent::DoProvokedAttack` (AIH_OPP.CPP, 0x8006431c): just hit by a human (in the
/// last 15 ticks), the hits by that human counted; enough of them (its personality's) start
/// an attack for its attack time; the human's index, or none (-1).
pub fn do_provoked_attack(m: &mut Mem, this: AIHighOpponent) -> i32 {
    let car = car_of(m, this);
    let hit = car.n().collision();
    if SIM_COUNTER.get(m).wrapping_sub(hit.last_time(m)) >= 15 {
        return -1;
    }
    let by = Car(hit.last_other_obj(m).0);
    if by.0 == 0 || by.car_flags(m) & aihigh::CAR_HUMAN == 0 {
        return -1;
    }
    if this.last_human_hitter_(m) != by {
        this.set_last_human_hitter_(m, by);
        this.set_hit_count_(m, 0);
    }
    this.update_hit_count_(m, |n| n.wrapping_add(1));
    let person = car.personality(m).0;
    if m.r32(person + 0x24) as i32 >= this.hit_count_(m) {
        return -1;
    }
    this.set_hit_count_(m, 0);
    let time = m.r32(person + 0x28) as i32;
    this.set_attack_ticks_left_(m, time);
    by.car_index(m)
}

pub fn abi_do_provoked_attack(c: &mut Cpu) {
    c.r[2] = do_provoked_attack(&mut c.m, AIHighOpponent(c.r[4])) as u32;
}

/// `AIHigh_Opponent::~AIHigh_Opponent` (AIHIGH.H, 0x800643c8).
pub fn abi_destruct(c: &mut Cpu) {
    aihigh::destruct_to_base(c, c.r[4], c.r[5], Some(VT_BASIC_PERP), 0x8006_43e0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aihigh::tests::{HEAP_OBJS, OBJS, args, pursuit, pursuit_shuffle};
    use crate::test_util::{against_original, same_stack};

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn opp_fuzz() {
        let cases: [(&str, fn(&mut Cpu), fn([u32; 7], [u32; 4]) -> Vec<u32>, Port); 9] = [
            ("CheckForWipeOut", nfs4rc::f_80063af8, |_, p| vec![p[3]], |c, a| {
                check_for_wipe_out(&mut c.m, AIHighOpponent(a[0]));
                None
            }),
            ("DoRearEnder", nfs4rc::f_80063cd8, |_, p| vec![p[3]], |c, a| Some(do_rear_ender(c, AIHighOpponent(a[0])) as u32)),
            ("HighExecute", nfs4rc::f_80063fac, |_, p| vec![p[3]], |c, a| {
                high_execute(c, AIHighOpponent(a[0]));
                None
            }),
            ("Construct", nfs4rc::f_800642d8, |a, _| vec![OBJS + 0x700, a[0]], |c, a| Some(construct(c, AIHighOpponent(a[0]), Car(a[1])).0)),
            ("DoProvokedAttack", nfs4rc::f_8006431c, |_, p| vec![p[3]], |c, a| Some(do_provoked_attack(&mut c.m, AIHighOpponent(a[0])) as u32)),
            ("Destruct", nfs4rc::f_800643c8, |a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                abi_destruct(c);
                None
            }),
            ("HumanHighExecute", nfs4rc::f_80063a60, |_, p| vec![p[0]], |c, a| {
                crate::aih_hum::high_execute(c, crate::types::AIHighHuman(a[0]));
                None
            }),
            ("HumanConstruct", nfs4rc::f_80063a9c, |a, _| vec![OBJS + 0x700, a[0]], |c, a| Some(crate::aih_hum::construct(c, crate::types::AIHighHuman(a[0]), Car(a[1])).0)),
            ("HumanDestruct", nfs4rc::f_80063ad0, |a, _| vec![HEAP_OBJS[a[3] as usize % 3], a[4] * 3], |c, a| {
                c.r[4..6].copy_from_slice(&a[..2]);
                crate::aih_hum::abi_destruct(c);
                None
            }),
        ];
        for (k, (name, original, call, port)) in cases.into_iter().enumerate() {
            against_original(name, 1_000, 0x3F10 + k as u64, pursuit_shuffle, original, &|c| {
                let (a, p) = (args(c), pursuit(c));
                call(a, p)
            }, port);
        }
    }

    /// The stack an opponent's and a human's ticks leave.
    #[test]
    fn opp_stack_fuzz() {
        let cases: [(&str, u32, fn(&mut Cpu)); 3] = [
            ("HighExecute", 0x8006_3fac, nfs4rc::f_80063fac),
            ("DoRearEnder", 0x8006_3cd8, nfs4rc::f_80063cd8),
            ("HumanHighExecute", 0x8006_3a60, nfs4rc::f_80063a60),
        ];
        for (k, (name, addr, original)) in cases.into_iter().enumerate() {
            same_stack(name, addr, original, 0x8006_0000, 0x801F_FF00, 300, 0x3F50 + k as u64, pursuit_shuffle, &|c, _| {
                let p = pursuit(c);
                vec![if name == "HumanHighExecute" { p[0] } else { p[3] }]
            });
        }
    }
}
