//! CARS.CPP: the car objects and what the simulation does to them besides the physics.

use crate::abi;
use crate::fixed::fixedmult;
use crate::globals::{
    ACCIDENT_SLICE, ADD_CAR_WHEEL_DELAY, AITUNE_DRIVE_SIDE, FRONT_LIMIT, LEAF_PIXMAP, REAR_LIMIT, BWORLDSM_SLICES, CARS_LIST, FAST_RANDOM, GAME_SETUP, HUMAN_RACE_CARS, NUM_AI_CARS, NUM_CARS, NUM_SLICES, RAND_SEED,
    RAND_TEMP, SCENE_LOADED, SIM_COUNTER, SORTED_LIST, TOTAL_SORTED_LIST,
};
use crate::types::{Car, Coord, Matrix, TrkNewSlice};
use crate::{bworldsm, newton};
use ps1rt::{Cpu, Mem};

/// `Car_tObj.carFlags`: a human player's car.
const CAR_HUMAN: i32 = 0x4;
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

/// The slice the car is on, counted the way the race goes (from the far end on a reversed
/// track).
fn race_slice(m: &mut Mem, slice: i32) -> i32 {
    if GAME_SETUP.reverse_track(m) != 0 { NUM_SLICES.get(m).wrapping_sub(slice).wrapping_sub(1) } else { slice }
}

/// `Cars_FindTotalSlice` (CARS.CPP, 0x8008a184): the slices the car has driven (its laps and
/// the slice it is on); 0 while it is behind the start line.
pub fn find_total_slice(m: &mut Mem, car: Car) {
    let n = car.n();
    if car.unlap(m) > 0 {
        n.set_total_slice(m, 0);
        return;
    }
    let laps = car.lap(m).wrapping_mul(NUM_SLICES.get(m));
    let slice = n.sim_road_info().slice(m) as u16 as i32;
    let slice = race_slice(m, slice);
    n.set_total_slice(m, slice.wrapping_add(laps) as u16);
}

/// `Cars_FindCurrentLap` (CARS.CPP, 0x8008b654): count the laps as the car crosses the start
/// line: from slice 0 back past it (to 501 or more) is a lap unwound, and from 501 or more on
/// to under 51 a lap done (or one unwound made up).
pub fn find_current_lap(m: &mut Mem, car: Car) {
    let n = car.n();
    let slice = n.sim_road_info().slice(m) as i32;
    let slice = race_slice(m, slice);
    let old = n.old_slice(m);
    if old as i32 == slice {
        return;
    }
    if slice >= 501 {
        if old == 0 {
            car.update_unlap(m, |u| u.wrapping_add(1));
        }
    } else if (slice as u32) < 51 && old >= 501 {
        if car.unlap(m) > 0 {
            car.update_unlap(m, |u| u - 1);
        } else {
            car.update_lap(m, |l| l.wrapping_add(1));
        }
    }
    n.set_old_slice(m, slice as u16);
}

/// The original's dot products of 16.16 vectors: both sides `/ 256` first.
fn dot_256(a: [i32; 3], b: [i32; 3]) -> i32 {
    (0..3).fold(0i32, |s, k| s.wrapping_add((a[k] / 256).wrapping_mul(b[k] / 256)))
}

fn row(m: &mut Mem, mat: Matrix, r: u32) -> [i32; 3] {
    [0, 1, 2].map(|k| mat.m(m, 3 * r + k))
}

/// `Cars_CalculateRoadSpan` (CARS.CPP, 0x8008b70c): how wide the car is across the road: its
/// box's half sizes along the road's sideways axis, each by how much it lies that way.
pub fn calculate_road_span(m: &mut Mem, car: Car) -> i32 {
    let n = car.n();
    let across = row(m, n.road_matrix(), 0);
    let dim = n.dimension();
    let size = [dim.x(m), dim.y(m), dim.z(m)];
    (0..3).fold(0i32, |s, r| {
        let axis = row(m, n.orient_mat(), r);
        let along = dot_256(across, axis).wrapping_abs();
        s.wrapping_add((along / 256).wrapping_mul(size[r as usize] / 256))
    })
}

/// `Cars_CalculateRoadPosition` (CARS.CPP, 0x8008b954): `Newton_CalculateRoadPosition`.
pub fn calculate_road_position(c: &mut Cpu, car: Car) -> i32 {
    newton::calculate_road_position(c, car.n())
}

/// `Cars_CalcVelDownRoad` (CARS.CPP, 0x8008b974): the car's speed along the road.
pub fn calc_vel_down_road(m: &mut Mem, car: Car) -> i32 {
    let n = car.n();
    let v = n.linear_vel();
    dot_256([v.x(m), v.y(m), v.z(m)], row(m, n.road_matrix(), 2))
}

/// `Cars_Randomize` (CARS.CPP, 0x8008ba10): with AI cars about, step the quick random numbers
/// on by 0 to 3 by bits 8-9 of the first human's road position, so they follow the driving.
pub fn randomize(m: &mut Mem) {
    if NUM_AI_CARS.get(m) == 0 {
        return;
    }
    let human = Car(m.r32(HUMAN_RACE_CARS));
    let steps = (human.road_position(m) & 0x300) >> 8;
    let seed = RAND_SEED.get(m);
    for _ in 0..steps {
        let t = FAST_RANDOM.get(m).wrapping_mul(seed);
        RAND_TEMP.set(m, t);
        FAST_RANDOM.set(m, t & 0xFFFF);
    }
}

/// `Cars_ManageBureaucracy` (CARS.CPP, 0x8008ba88, every tick): for each active car its span
/// across the road, its position on it, its lane, its speed down the road (for the AI's
/// physics), a human's speed and which way it goes along the road, its laps and the slices
/// it has driven; then the cars sorted, the random numbers stirred, the AI's leader board
/// and the track's animations.
pub fn manage_bureaucracy(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut i = 0;
        while i < NUM_CARS.get(&mut c.m) {
            let car = Car(c.m.r32(CARS_LIST + 4 * i as u32));
            i += 1;
            if car.n().active(&mut c.m) == 0 {
                continue;
            }
            let span = calculate_road_span(&mut c.m, car);
            car.set_road_span(&mut c.m, span);
            let pos = calculate_road_position(c, car);
            car.set_road_position(&mut c.m, pos);
            abi::call(c, nfs4rc::f_80073e74, &[car.0]); // AIWorld_CalculateLaneInfo
            if abi::call(c, nfs4rc::f_8006a6ec, &[car.0]) != 0 {
                // AIPhysics_UseCoolPhysics
                let v = calc_vel_down_road(&mut c.m, car);
                car.set_current_speed(&mut c.m, v);
            }
            let m = &mut c.m;
            if car.car_flags(m) & CAR_HUMAN != 0 {
                let speed = car.current_speed(m);
                let xz = car.n().speed_xz(m);
                car.set_speed(m, xz);
                let dir = if (speed.wrapping_add(0x2_FFFF) as u32) > 0x5_FFFE {
                    if speed < 0 { -1 } else { 1 }
                } else {
                    // under 3 m/s down the road: the way the car points along the slice
                    let slice = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((car.n().sim_road_info().slice(m) as i32 as u32) << 5);
                    let fwd = car.n().orient_mat();
                    let along = (0..3u32).fold(0i32, |s, k| {
                        let f = fwd.m(m, 6 + k);
                        s.wrapping_add(fixedmult(f, m.r8(slice + 15 + k) as i8 as i32))
                    });
                    if along > 0 { 1 } else { -1 }
                };
                car.set_direction(m, dir);
            }
            if car.n().sim_road_info().slice_changed(m) != 0 {
                find_current_lap(m, car);
                find_total_slice(m, car);
            }
        }
        sort_cars(&mut c.m);
        randomize(&mut c.m);
        abi::call(c, nfs4rc::f_8006f7a4, &[]); // AISpeeds_MaintainLeaderBoard
        abi::call(c, nfs4rc::f_800c8568, &[]); // DrawW_DoObjectAnimations
    })
}

/// `Cars_CheckForAccidentScenes` (CARS.CPP, 0x8008bc50): in a one-player race (not a time
/// trial or knockout), the accident scene's objects put on the track when the first human
/// starts the scene's first lap and taken off when it starts its last.
pub fn check_for_accident_scenes(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if GAME_SETUP.comm_mode(m) == 1 {
            return;
        }
        let race_type = GAME_SETUP.race_type(m);
        if race_type == 1 || race_type == 5 {
            return;
        }
        let lap = Car(m.r32(HUMAN_RACE_CARS)).stats().lap(m);
        if SCENE_LOADED.get(m) != 0 {
            if lap == GAME_SETUP.scene_end_lap(m) {
                abi::call(c, nfs4rc::f_800a61e4, &[]); // Object_ClearCustomObjects
                SCENE_LOADED.set(&mut c.m, 0);
                ACCIDENT_SLICE.set(&mut c.m, 0);
            }
        } else if lap == GAME_SETUP.scene_start_lap(m) {
            let slice = abi::call(c, nfs4rc::f_800b6cb8, &[]); // Scene_BuildCustomSceneList
            ACCIDENT_SLICE.set(&mut c.m, slice as i32);
            SCENE_LOADED.set(&mut c.m, 1);
        }
    })
}

/// Bubble-sort the cars in `list` by `before` (one goes before the other), `swapped` told of
/// each pair swapped, until a pass swaps none.
fn bubble_sort(m: &mut Mem, list: u32, before: fn(&mut Mem, Car, Car) -> bool, swapped: fn(&mut Mem, Car, Car)) {
    let slot = |i: i32| list.wrapping_add((i as u32) << 2);
    if NUM_CARS.get(m) - 1 <= 0 {
        return;
    }
    loop {
        let mut any = false;
        let last = NUM_CARS.get(m) - 1;
        let mut i = 0;
        loop {
            let (a, b) = (Car(m.r32(slot(i))), Car(m.r32(slot(i + 1))));
            if before(m, b, a) {
                swapped(m, a, b);
                m.w32(slot(i), b.0);
                m.w32(slot(i + 1), a.0);
                any = true;
            }
            i += 1;
            if i >= last {
                break;
            }
        }
        if !any {
            return;
        }
    }
}

/// `Cars_SortCars` (CARS.CPP, 0x8008bd18): the cars sorted by the slice they are on (their
/// `sortIndex` the place), and by the slices they have driven, cars passing each other
/// noting it (`swapCar`, `swapTime`).
pub fn sort_cars(m: &mut Mem) {
    bubble_sort(m, SORTED_LIST, |m, a, b| a.n().sim_road_info().slice(m) < b.n().sim_road_info().slice(m), |_, _, _| {});
    for i in 0..NUM_CARS.get(m) {
        Car(m.r32(SORTED_LIST + 4 * i as u32)).set_sort_index(m, i);
    }
    bubble_sort(
        m,
        TOTAL_SORTED_LIST,
        |m, a, b| a.n().total_slice(m) < b.n().total_slice(m),
        |m, a, b| {
            a.set_swap_car(m, b);
            b.set_swap_car(m, a);
            let now = SIM_COUNTER.get(m);
            b.set_swap_time(m, now);
            a.set_swap_time(m, now);
        },
    );
}

/// `Cars_SetAudioCalls` (CARS.CPP, 0x800869b0): queue a sound for the car this frame
/// (`Cars_tAudio`: what, on which channel, the surfaces, how hard, panned where). The queue
/// has room for 7; the original doesn't check.
#[allow(clippy::too_many_arguments)]
pub fn set_audio_calls(m: &mut Mem, car: Car, kind: i32, channel: i32, surface1: i32, surface2: i32, force: i32, pan: i32) {
    let at = car.0.wrapping_add(0x798).wrapping_add((car.audio_count(m) as u32).wrapping_mul(0x18));
    for (k, v) in [kind, channel, surface1, surface2, force, pan].into_iter().enumerate() {
        m.w32(at + 4 * k as u32, v as u32);
    }
    car.update_audio_count(m, |n| n.wrapping_add(1));
}

/// [`set_audio_calls`] called from a frame at `sp`, its last three arguments put on the
/// stack as the original passes them.
#[allow(clippy::too_many_arguments)]
fn set_audio_calls_from(m: &mut Mem, sp: u32, car: Car, kind: i32, channel: i32, surface1: i32, surface2: i32, force: i32, pan: i32) {
    for (k, v) in [surface2, force, pan].into_iter().enumerate() {
        m.w32(sp + 16 + 4 * k as u32, v as u32);
    }
    set_audio_calls(m, car, kind, channel, surface1, surface2, force, pan);
}

/// `Cars_ResetVariablesAfterACollision` (CARS.CPP, 0x80087020): a car put back on the road
/// still: no spin, no acceleration, on the ground, no collision, not smoking; its controls
/// and engine reset (`Physics_ResetCar`).
pub fn reset_variables_after_a_collision(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let n = car.n();
        for v in [n.angular_vel(), car.angular_vel_ch(), car.linear_acc_ch()] {
            v.set(m, Default::default());
        }
        n.set_flight_time(m, 0);
        let hit = n.collision();
        hit.set_collided(m, 0);
        hit.set_last_collision(m, 0);
        hit.set_impulse(m, 0);
        hit.set_other_obj(m, crate::types::NewtonObj(0));
        hit.set_disable_collision_timer(m, 0);
        n.set_ground_vel(m, 0);
        n.set_speed_xz(m, 0);
        car.collision().set_smoking(m, 0);
        n.set_road_gravity_modifier(m, 0);
        crate::physics::reset_car(c, car);
    })
}

/// `Cars_ResetCollidedCars` (CARS.CPP, 0x80087088): put a crashed car back on the road (`how`
/// 0: only if it isn't upright and settled on the road; 1: always; 2: where it is, not 4
/// slices on the way it goes), never with a tyre blown. Moved on past an accident scene; to
/// the side of the road its grid place puts it (odd places one side, even the other, by the
/// way it goes and the side traffic keeps to), its own width in from the edge; tried there
/// again while that is off the road (41 more times), then in the middle of the road; still.
pub fn reset_collided_cars(c: &mut Cpu, car: Car, how: i32) {
    abi::frame(c, 56, |c, sp| {
        let mut r = abi::Saved::of(c);
        r.0[1] = how as u32;
        r.0[2] = car.0;
        let offset = Coord(sp + 16);
        r.call(c, nfs4rc::f_800e4f04, &[offset.0, 0, 12], 0x8008_70bc); // memset
        let m = &mut c.m;
        let n = car.n();
        let blowout = car.blowout(m);
        let dir = car.desired_direction(m);
        r.0[3] = dir as u32;
        car.collision().set_smoking(m, 0);
        if blowout != 0 {
            return;
        }
        if how == 0 {
            let up = dot_256(row(m, n.road_matrix(), 1), row(m, n.orient_mat(), 1));
            let spin = n.angular_vel();
            let surface = n.drive_surface_type(m);
            if up > 0xC000
                && spin.x(m) <= 0xFFFF
                && spin.z(m) <= 0xFFFF
                && surface != 14
                && surface != 0
                && n.sim_road_info().sim_quad(m).0 != 0
                && n.flight_time(m) < 6
                && n.obj_altitude(m) <= 0xFFFF
            {
                car.collision().set_smoking(m, 0);
                return;
            }
        }
        let slices = |m: &mut Mem| NUM_SLICES.get(m);
        let wrap = |m: &mut Mem, s: i32, by: i32| {
            let s = s.wrapping_add(by);
            if by < 0 {
                if s < 0 { s.wrapping_add(slices(m)) } else { s }
            } else if s < slices(m) {
                s
            } else {
                s.wrapping_sub(slices(m))
            }
        };
        let here = n.sim_road_info().slice(m) as i32;
        let mut slice = wrap(m, here, dir.wrapping_shl(2));
        if how == 2 {
            slice = here;
        }
        // clear of an accident scene (within 5 slices, as the original reckons it)
        let accident = ACCIDENT_SLICE.get(m);
        if accident > 0 {
            let near = if accident + 5 < slices(m) {
                slice < accident + 5 && {
                    let back = accident - 5;
                    if back < 0 { back.wrapping_add(slices(m)) < slice } else { back < slice }
                }
            } else {
                slice < accident - (slices(m) - 5) && {
                    let back = accident - 5;
                    if back < 0 { accident + (slices(m) - 5) < slice } else { back < slice }
                }
            };
            if near {
                slice = wrap(m, slice, dir.wrapping_mul(5));
            }
        }
        r.0[0] = slice as u32;
        r.0[4] = 0;
        // the side: by the grid place, the way the car goes and the side traffic keeps to
        let mut left = car.car_index(m) % 2 != 0;
        if dir == -1 {
            left = !left;
        }
        if AITUNE_DRIVE_SIDE.get(m) == -1 {
            left = !left;
        }
        let road = TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(slice as u32);
        let width = (n.dimension().x(m) / 256).wrapping_mul(384);
        let x = if left {
            let edge = ((road.avg_paved_width_lf(m) as i32) << 15).wrapping_mul((road.lane_count(m) >> 4) as i32);
            edge.wrapping_neg().wrapping_add(width)
        } else {
            let edge = ((road.avg_paved_width_rt(m) as i32) << 15).wrapping_mul((road.lane_count(m) & 0xF) as i32);
            edge.wrapping_sub(width)
        };
        offset.set_x(m, x);
        let place = |c: &mut Cpu, r: &abi::Saved, ra: u32| {
            r.enter(c, 0x800a_22a8, ra, |c| newton::set_initial_slice_position_orientation_etc(c, n, slice, offset, dir));
        };
        place(c, &r, 0x8008_744c);
        let mut tries = 0;
        r.0[1] = 0;
        loop {
            let m = &mut c.m;
            let surface = n.drive_surface_type(m);
            if surface != 0 && surface != 14 && n.sim_road_info().sim_quad(m).0 != 0 {
                break;
            }
            tries += 1;
            r.0[1] = tries;
            // (moved on by nothing)
            offset.update_x(m, |x| x.wrapping_add(0));
            place(c, &r, 0x8008_747c);
            if tries >= 41 {
                offset.set_x(&mut c.m, 0);
                place(c, &r, 0x8008_73e0);
                break;
            }
        }
        n.linear_vel().set(&mut c.m, Default::default());
        r.enter(c, 0x8008_7020, 0x8008_74c8, |c| reset_variables_after_a_collision(c, car));
    })
}

/// `Cars_SetCarUpForHiRezSim` (CARS.CPP, 0x800874e8): a car coming back into the full
/// simulation: not smoking, its road found again, and put back on the road if off it.
pub fn set_car_up_for_hi_rez_sim(c: &mut Cpu, car: Car) {
    abi::frame(c, 24, |c, _| {
        let n = car.n();
        let m = &mut c.m;
        car.collision().set_smoking(m, 0);
        if n.sim_optz(m) >= 2 {
            return;
        }
        let mut r = abi::Saved::of(c);
        r.0[0] = car.0;
        r.enter(c, 0x8008_04e8, 0x8008_751c, |c| bworldsm::find_closest_quad_rez(c, n.position(), n.sim_road_info(), 1));
        let q = n.sim_road_info().sim_quad(&mut c.m);
        if q.0 != 0 && q.surface(&mut c.m) & 0xF == 0 {
            r.enter(c, 0x8008_7088, 0x8008_754c, |c| reset_collided_cars(c, car, 1));
        }
    })
}

/// `Car_tObj.carFlags`: the car is out of the race (nothing more happens to it).
const CAR_OUT: i32 = 0x400;
/// The kinds of collision (`BO_tNewtonCollisionInfo.sfxType`'s high half, but for its
/// 0x10000000 bit; the low byte is the surface): the road (scraping it), a wall, a car.
const HIT_ROAD: u32 = 0x3_0000;
const HIT_WALL: u32 = 0x4_0000;
const HIT_CAR: u32 = 0x5_0000;
/// `Cars_kSkidMarkSurface`, `Cars_kSFXWallSurfaceInterface`, `Cars_kAudioWallSurfaceInterface`,
/// `Cars_kAudioRoadSurfaceInterface` (by surface), `Cars_kAudioCollisoinTypeInterface` (by kind).
const SKID_MARK_SURFACE: u32 = 0x8011_0B84;
const SFX_WALL_SURFACE: u32 = 0x8011_0C44;
const AUDIO_WALL_SURFACE: u32 = 0x8011_0C64;
const AUDIO_ROAD_SURFACE: u32 = 0x8011_0C04;
const AUDIO_COLLISION_TYPE: u32 = 0x8011_0C84;
/// `Camera_gInfo`: the two players' cameras (`camera_info`, 0x110 bytes; `relpos` at 0x14).
const CAMERA_INFO: u32 = 0x8011_0570;
/// Damage parts that fall off past a zone's damage: (bit, zone, damage).
const DAMAGE_PARTS: [(u16, u32, i32); 7] = [(1, 7, 0x1E_0000), (2, 3, 0x1E_0000), (4, 8, 0x3_0000), (8, 0, 0x1E_0000), (0x10, 2, 0x1E_0000), (0x20, 6, 0x1E_0000), (0x40, 4, 0x1E_0000)];

fn table(m: &mut Mem, at: u32, i: u32) -> i32 {
    m.r32(at.wrapping_add(i << 2)) as i32
}

/// `Cars_DoExtraCarCollisionProcessing` (CARS.CPP, 0x8008755c): what follows a car's
/// collisions. A blown tyre at a standstill for 320 ticks, or the player's reset (`abort`,
/// after the first 833 ticks, slower than 100 km/h) puts the car back on the road; sparks
/// from a scraping body; parts falling off with damage; a heavy hit crashes the car (smoke,
/// the reset timer); the hit's sound effects (sparks off walls, a car's crunch) and sound;
/// and a car on its side or roof, or barely moving off the road, reset after a while.
pub fn do_extra_car_collision_processing(c: &mut Cpu, car: Car) {
    abi::frame(c, 120, |c, sp| {
        let n = car.n();
        // s0: the scraping wheel, then whether parts fell off, then the hit's surface; s2 the
        // hit's kind; s3 its volume
        let mut r = abi::Saved::of(c);
        r.0[1] = car.0;
        if car.car_flags(&mut c.m) & CAR_OUT != 0 {
            return;
        }
        let m = &mut c.m;
        let blowout = car.blowout(m);
        let check_abort = if blowout != 0 {
            let vel = car.linear_vel_ch();
            if vel.z(m).wrapping_abs() < 6553 && vel.x(m).wrapping_abs() < 6553 {
                car.set_blowout(m, blowout.wrapping_add(1));
            }
            if car.blowout(m) >= 321 {
                car.set_blowout(m, 0);
                r.enter(c, 0x8008_7088, 0x8008_7604, |c| reset_collided_cars(c, car, 1));
            }
            car.blowout(&mut c.m) == 0
        } else {
            true
        };
        let m = &mut c.m;
        let stats = car.stats();
        if check_abort
            && car.pull_over(m) == 0
            && car.control().abort(m) != 0
            && stats.finish_type(m) == 0
            && SIM_COUNTER.get(m) >= 833
            && stats.fatal_crashes(m) == 0
            && n.speed_xz(m) <= 0x1B_9998
        {
            r.enter(c, 0x8008_7088, 0x8008_768c, |c| reset_collided_cars(c, car, 2));
            let m = &mut c.m;
            stats.set_fatal_crashes(m, 160);
            let second = (car.0 == m.r32(HUMAN_RACE_CARS + 4)) as u32;
            let relpos = crate::types::Coord(CAMERA_INFO + second * 0x110 + 0x14);
            let fwd = n.orient_mat();
            let back = [6, 7, 8].map(|k| fwd.m(m, k).wrapping_neg().wrapping_shl(1));
            relpos.set(m, crate::vec::Vec3::new(back[0], back[1], back[2]));
        }
        let m = &mut c.m;
        if stats.fatal_crashes(m) > 0 {
            stats.update_fatal_crashes(m, |t| t - 1);
        }

        if car.collision().smoking(m) != 0 {
            r.call(c, nfs4rc::f_800bc144, &[n.position().0], 0x8008_7720); // TrgSfx_CrashCar
            if n.flight_time(&mut c.m) == 0 {
                car.collision().set_smoking(&mut c.m, 0);
            }
        }

        // sparks from the body scraping the road, behind a rear wheel on the side the car leans
        if car.audio_damage_scrape(&mut c.m) != 0 && n.sim_optz(&mut c.m) == 0 {
            let m = &mut c.m;
            let wheel = if car.render().current_roll(m) >= 0 { 2 } else { 3 };
            r.0[0] = wheel;
            let surface = car.wheel(wheel).road_surface_type(m) as u32 & 0xF;
            if table(m, SKID_MARK_SURFACE, surface) == 1 && r.call(c, nfs4rc::f_800e8394, &[], 0x8008_77a8) & 3 == 0 {
                // random
                let m = &mut c.m;
                let back = n.wheel_back_x(m) / 256;
                let side = [0, 1, 2].map(|k| back.wrapping_mul(n.orient_mat().m(m, k)) / 256);
                for (k, v) in side.into_iter().enumerate() {
                    m.w32(sp + 64 + 4 * k as u32, v as u32);
                }
                let zero = sp + 80;
                for k in 0..3 {
                    m.w32(zero + 4 * k, 0);
                }
                let p = n.position().get(m);
                let at = [p.x, n.ground_elevation(m), p.z];
                for (k, v) in at.into_iter().enumerate() {
                    m.w32(sp + 32 + 4 * k as u32, v as u32);
                }
                for k in 0..3 {
                    let v = if wheel & 1 == 0 { at[k].wrapping_sub(side[k]) } else { at[k].wrapping_add(side[k]) };
                    let v = v.wrapping_sub(m.r32(zero + 4 * k as u32) as i32);
                    m.w32(sp + 48 + 4 * k as u32, v as u32);
                }
                let id = n.obj_id(m) as u32;
                r.call(c, nfs4rc::f_800bbea4, &[id, sp + 48, 4, n.linear_vel().0], 0x8008_7910); // TrgSfx_AddCarSfx
            }
            car.set_audio_damage_scrape(&mut c.m, 0);
        }

        let hit = n.collision();
        r.0[0] = 0;
        if hit.impulse(&mut c.m) != 0 {
            let m = &mut c.m;
            let mut parts_off = false;
            let kind = hit.sfx_type(m) as u32 & 0xEFFF_0000;
            r.0[2] = kind;
            if car.car_info(m).car_type(m) < 28 {
                let render = car.render();
                for (bit, zone, limit) in DAMAGE_PARTS {
                    let parts = render.damage_parts(m) as u16;
                    if parts & bit == 0 && n.damage(m, zone) > limit {
                        render.set_damage_parts(m, (parts | bit) as i16);
                        parts_off = true;
                    }
                }
            }
            r.0[0] = parts_off as u32;
            if hit.other_obj(m).0 == 0 && hit.impulse(m) > 0x4B_0000 {
                // a heavy hit with no car: crashed
                if car.car_flags(m) & CAR_HUMAN != 0 {
                    r.enter(c, 0x800a_b178, 0x8008_7ad4, |c| crate::physics::fix_engine_rpm(c, car));
                }
                r.call(c, nfs4rc::f_800bc144, &[n.position().0], 0x8008_7adc); // TrgSfx_CrashCar
                let m = &mut c.m;
                car.collision().set_smoking(m, 1);
                car.set_front_skid(m, 0);
                car.set_rear_skid(m, 0);
                if car.collision().reset_timer(m) <= 0 {
                    car.collision().set_reset_timer(m, 320);
                }
            }
            let point = sp + 32;
            let at_point = |m: &mut Mem| {
                let p = hit.collision_point().get(m);
                m.w32(point, p.x as u32);
                m.w32(point + 4, p.y.wrapping_add(0x8000) as u32);
                m.w32(point + 8, p.z as u32);
            };
            let id = n.obj_id(&mut c.m) as u32;
            let vel = n.linear_vel().0;
            if kind == HIT_WALL {
                at_point(&mut c.m);
                let m = &mut c.m;
                if n.speed_xz(m) > 0x18_0000 {
                    // (the table's address half made, in a delay slot)
                    r.0[0] = SFX_WALL_SURFACE & 0xFFFF_0000;
                    if n.obj_altitude(m) > 0x9998 {
                        r.call(c, nfs4rc::f_800bbea4, &[id, point, 6, vel], 0x8008_7c0c); // TrgSfx_AddCarSfx
                    } else {
                        r.0[0] = SFX_WALL_SURFACE;
                        let surface = hit.sfx_type(m) as u8 as u32;
                        let sfx = table(m, SFX_WALL_SURFACE, surface);
                        r.call(c, nfs4rc::f_800bbea4, &[id, point, sfx as u32, vel], 0x8008_7b78);
                        let m = &mut c.m;
                        let surface = hit.sfx_type(m) as u8 as u32;
                        if table(m, SFX_WALL_SURFACE, surface) == 4 {
                            set_audio_calls_from(m, sp, car, 1, -1, 1, 15, 0, 0);
                        }
                    }
                } else if n.speed_xz(m) > 0xF_0000 || hit.impulse(m) > 0xF_0000 || parts_off {
                    r.call(c, nfs4rc::f_800bbea4, &[id, point, 1, vel], 0x8008_7c0c);
                }
            }
            let m = &mut c.m;
            let impulse = hit.impulse(m);
            if impulse > 0xA_0000 {
                let mut volume = m.r32(AUDIO_COLLISION_TYPE.wrapping_add(((kind as i32) >> 14) as u32)) as i32;
                r.0[3] = volume as u32;
                let on = hit.sfx_type(m) as u8 as u32;
                let surface = if kind == HIT_WALL {
                    table(m, AUDIO_WALL_SURFACE, on)
                } else if kind as i32 <= HIT_ROAD as i32 {
                    let surface = table(m, AUDIO_ROAD_SURFACE, on);
                    r.0[0] = surface as u32;
                    if kind == HIT_ROAD {
                        if n.orientation_to_ground().y(m) <= 0xE665 {
                            volume = 1;
                            r.0[3] = 1;
                        }
                        r.call(c, nfs4rc::f_800bc144, &[hit.collision_point().0], 0x8008_7ca8); // TrgSfx_CrashCar
                    }
                    surface
                } else {
                    let surface = hit.sfx_type(m) as u8 as i32;
                    r.0[0] = surface as u32;
                    if kind == HIT_CAR {
                        at_point(m);
                        r.call(c, nfs4rc::f_800bbea4, &[id, point, 4, vel], 0x8008_7cf4); // TrgSfx_AddCarSfx
                    }
                    surface
                };
                r.0[0] = surface as u32;
                let m = &mut c.m;
                let force = hit.impulse(m) / 8;
                set_audio_calls_from(m, sp, car, 1, -1, volume, surface, force, 0);
            }
            let m = &mut c.m;
            hit.set_impulse(m, 0);
            hit.set_other_obj(m, crate::types::NewtonObj(0));
            hit.set_sfx_type(m, 0);
        }

        // the reset timer: down by 1 a tick, by 9 when slow or off the road
        let m = &mut c.m;
        let timer = car.collision().reset_timer(m);
        if timer > 0 && car.pull_over(m) == 0 {
            car.collision().set_reset_timer(m, timer - 1);
            let moving = n.speed_xz(m).wrapping_add(n.linear_vel().y(m).wrapping_abs());
            let surface = n.drive_surface_type(m);
            if moving <= 0xFFFF || surface == 0 || surface == 14 {
                car.collision().update_reset_timer(m, |t| t - 8);
            }
            if car.collision().reset_timer(m) <= 0 {
                r.enter(c, 0x8008_7088, 0x8008_7dc4, |c| reset_collided_cars(c, car, 0));
                car.collision().set_reset_timer(&mut c.m, 0);
            }
        }
        // on its side or roof, or off the road: the timer starts
        let m = &mut c.m;
        let up = dot_256(row(m, n.road_matrix(), 1), row(m, n.orient_mat(), 1));
        if car.collision().reset_timer(m) <= 0 {
            let surface = n.drive_surface_type(m);
            if up < 6553 || surface == 0 || surface == 14 {
                car.collision().set_reset_timer(m, 320);
            }
        }
    })
}

/// `Cars_kSmokingSurface` (the speed above which a wheel throws up its surface, 0 for none),
/// `Cars_kConvertFromRoadToSfxType` (the effect), by surface.
const SMOKING_SURFACE: u32 = 0x8011_0B44;
const ROAD_TO_SFX_TYPE: u32 = 0x8011_0BC4;
/// The effect of a wet surface: a splash.
const SFX_SPLASH: i32 = 8;
/// `Car_tObj.render.currentCarType`: the police helicopter (no wheels on the road), and the
/// first of the big vehicles (their wheels' effects last twice as long, and leave no marks).
const CAR_TYPE_HELICOPTER: i16 = 28;
const CAR_TYPE_BIG: i16 = 29;

/// `Cars_AddCarSfx` (CARS.CPP, 0x80087eb0): a wheel's skid mark and the effect it throws up
/// at `pos`. Skidding (`kind` 1 to 3, `on`), the mark goes on from where the wheel last left
/// it, as dark as the axle's skid and the load on the wheel (`TrgSfx_AddSkidmark`); not
/// skidding, a mark is ended. The surface's effect (dust, a splash...) comes faster than its
/// speed, or skidding hard (`TrgSfx_AddCarWheelSfx`); a heavy skid (kind 3) throws none of
/// its own.
pub fn add_car_sfx(c: &mut Cpu, car: Car, wheel: u32, pos: Coord, surface: i32, kind: i32, on: i32) {
    abi::frame(c, 72, |c, _| {
        let m = &mut c.m;
        let surface = surface as u32 & 0xF;
        let mut smoking = table(m, SMOKING_SURFACE, surface);
        let sfx = table(m, ROAD_TO_SFX_TYPE, surface);
        let bit = 1u32.wrapping_shl(wheel) as i32;
        if kind == 3 {
            smoking = 0;
        }
        let id = car.n().obj_id(m) as u32;
        let car_type = car.render().current_car_type(m);
        let big = car_type >= CAR_TYPE_BIG;
        let mut delay = ADD_CAR_WHEEL_DELAY.get(m);
        if big {
            delay = delay.wrapping_shl(1);
        }
        if car_type == CAR_TYPE_HELICOPTER {
            return;
        }
        let regs = abi::Saved([car.0, wheel, id, pos.0, bit as u32, delay as u32, smoking as u32, sfx as u32, c.r[30]]);
        // the front wheels by the front skid, the rear by the rear; the left wheel (0 or 2)
        // unloaded by the weight thrown right, the right loaded
        let front = (wheel as i32) < 2;
        let skid = |m: &mut Mem| if front { car.front_skid(m) } else { car.rear_skid(m) };
        let (left, light, ra_on, ra_off) = if front { (0, 0x1_8000, 0x8008_81b8, 0x8008_8230) } else { (2, 0x1_0000, 0x8008_802c, 0x8008_80a4) };
        let last = car.old_skid_point(wheel);
        if on != 0 && kind != 0 && !big {
            let (p, l) = (pos.get(m), last.get(m));
            if p.x != l.x || p.z != l.z {
                let right = car.g_transfer_right(m);
                let load = if wheel == left { skid(m).wrapping_sub(right) } else { skid(m).wrapping_add(right) };
                let dark = load.wrapping_sub(light).clamp(0, 0x6_0000);
                let args = [id, wheel, pos.0, 0, dark as u32, car.0, kind.wrapping_sub(1) as u32];
                regs.call(c, nfs4rc::f_800bc1f0, &args, ra_on); // TrgSfx_AddSkidmark
                // (where the mark went, moved on with the car)
                let p = pos.get(&mut c.m);
                last.set(&mut c.m, p);
                car.update_old_skid_state(&mut c.m, |s| s | bit);
            }
        } else if car.old_skid_state(m) & bit != 0 {
            let dark = skid(m);
            regs.call(c, nfs4rc::f_800bc1f0, &[id, wheel, last.0, 1, dark as u32, car.0, 0], ra_off);
            car.update_old_skid_state(&mut c.m, |s| s.wrapping_sub(bit));
        }
        let m = &mut c.m;
        let vel = car.n().linear_vel().0;
        let speed = car.n().speed_xz(m);
        if sfx == SFX_SPLASH {
            let args = [id, wheel, pos.0, SFX_SPLASH as u32, vel, delay as u32, speed as u32];
            regs.call(c, nfs4rc::f_800bc090, &args, 0x8008_8270); // TrgSfx_AddCarSplash
        } else if (smoking < speed && smoking > 0) || skid(m) > 0x4_0000 {
            regs.call(c, nfs4rc::f_800bbf10, &[id, wheel, pos.0, sfx as u32, vel, delay as u32], 0x8008_82c4); // TrgSfx_AddCarWheelSfx
        }
    })
}

/// The return addresses of `Car_TireSkiddingStuff`'s calls for each wheel: `Cars_AddCarSfx`
/// not skidding and skidding, `TrgSfx_AddSkidmark`, `TrgSfx_AddCarSplash`,
/// `TrgSfx_AddCarExtraCheck`, `random`, `TrgSfx_AddCarExtraSfx`.
const TIRE_SKIDDING_RA: [[u32; 7]; 4] = [
    [0x8008_8754, 0x8008_8734, 0x8008_8798, 0x8008_896c, 0x8008_89bc, 0x8008_89cc, 0x8008_8bbc],
    [0x8008_8e08, 0x8008_8de8, 0x8008_8e4c, 0x8008_9020, 0x8008_9070, 0x8008_9080, 0x8008_9270],
    [0x8008_94bc, 0x8008_949c, 0x8008_94f8, 0x8008_96dc, 0x8008_972c, 0x8008_973c, 0x8008_992c],
    [0x8008_9b78, 0x8008_9b58, 0x8008_9bb4, 0x8008_9d98, 0x8008_9de8, 0x8008_9df8, 0x8008_9fe8],
];

/// A wheel's place on the road for `Car_TireSkiddingStuff`, at sp+48: the car's position at
/// ground level (sp+32) out to the side by the wheel's offset (`spread` more, along the
/// body's sideways axis: sp+64) and forward or back (along its forward axis: at `fwd_at`).
fn wheel_point(m: &mut Mem, sp: u32, car: Car, wheel: u32, spread: i32, fwd_at: u32) {
    let n = car.n();
    let front = wheel < 2;
    let (x, z) = if front { (n.wheel_front_x(m), n.wheel_front_z(m)) } else { (n.wheel_back_x(m), n.wheel_back_z(m)) };
    let axis = |m: &mut Mem, r: u32, d: i32| [0, 1, 2].map(|k| (d / 256).wrapping_mul(n.orient_mat().m(m, 3 * r + k)) / 256);
    let side = axis(m, 0, x.wrapping_add(spread));
    let fwd = axis(m, 2, z);
    for k in 0..3 {
        m.w32(sp + 64 + 4 * k as u32, side[k] as u32);
        m.w32(sp + fwd_at + 4 * k as u32, fwd[k] as u32);
    }
    for k in 0..3 {
        let at = m.r32(sp + 32 + 4 * k as u32) as i32;
        let at = if wheel & 1 == 0 { at.wrapping_sub(side[k]) } else { at.wrapping_add(side[k]) };
        let at = if front { at.wrapping_add(fwd[k]) } else { at.wrapping_sub(fwd[k]) };
        m.w32(sp + 48 + 4 * k as u32, at as u32);
    }
}

/// `Car_TireSkiddingStuff` (CARS.CPP, 0x800882f0): what the tyres leave and throw up, for
/// a car near a player and on the ground. The axles' skids (at most 10, the front less for
/// a car sliding fast along) make marks where each wheel skids (or the surface itself marks,
/// moving), ended where it stops; in the rain (outside tunnels) the wheels on a wet surface
/// splash, and on a leafy one, fast, throw up leaves. Then the skids' sounds: a human's
/// always, on the surface under the car (rain on the road, for the tyre type on dirt).
pub fn tire_skidding_stuff(c: &mut Cpu, car: Car) {
    abi::frame(c, 168, |c, sp| {
        let n = car.n();
        let mut r = abi::Saved::of(c);
        let m = &mut c.m;
        // (the speed in whole m/s)
        let moving = n.speed_xz(m) >> 16;
        r.0[0] = car.0;
        r.0[5] = 0;
        r.0[7] = 0;
        r.0[8] = moving as u32;
        let weather = GAME_SETUP.weather(m);
        let rain = weather != 0 && !bworldsm::tunnel_flag_sm(c, n.sim_road_info());
        r.0[7] = rain as u32;
        let m = &mut c.m;
        let near = n.dist_to_player(m) <= 0x3B_FFFF && n.obj_altitude(m) < 26214;
        r.0[5] = near as u32;
        // the skids: s3 (at sp+116), s6 (the front's for the marks), s4 (sp+120)
        let (mut front, front_marks, mut rear);
        if near {
            let p = n.position().get(m);
            for (k, v) in [p.x, n.ground_elevation(m), p.z].into_iter().enumerate() {
                m.w32(sp + 32 + 4 * k as u32, v as u32);
            }
            front = 0;
            let skid = car.front_skid(m);
            front_marks = if skid > 0 {
                front = skid.min(0xA_0000);
                m.w32(sp + 116, front as u32);
                car.set_front_skid(m, front);
                let along = car.linear_vel_ch().z(m);
                if along.wrapping_abs() > 0x14_0000 { front.wrapping_sub((along / 8).wrapping_abs()) } else { front }
            } else {
                m.w32(sp + 116, 0);
                0
            };
            let skid = car.rear_skid(m);
            rear = if skid > 0 { skid.min(0xA_0000) } else { 0 };
            if skid > 0 {
                car.set_rear_skid(m, rear);
            }
            m.w32(sp + 120, rear as u32);
        } else {
            (front, front_marks, rear) = (0, 0, 0);
            m.w32(sp + 116, 0);
            m.w32(sp + 120, 0);
        }
        (r.0[3], r.0[4], r.0[6]) = (front as u32, rear as u32, front_marks as u32);

        // the skids' sound's surface: in the rain, the rain's (outside tunnels) where the
        // ground has none; else on dirt by the tyre type
        let on = n.drive_surface_type(m) as u32;
        let audio = table(m, AUDIO_ROAD_SURFACE, on);
        m.w32(sp + 112, audio as u32);
        if weather > 0 {
            if audio == 0 && !bworldsm::tunnel_flag_sm(c, n.sim_road_info()) {
                c.m.w32(sp + 112, 16);
            }
        } else if audio == 0 {
            match car.car_info(m).tire_type(m) {
                0 => m.w32(sp + 112, 18),
                2 => m.w32(sp + 112, 17),
                _ => {}
            }
        }

        for w in 0..4u32 {
            let ra = &TIRE_SKIDDING_RA[w as usize];
            let is_front = w < 2;
            let m = &mut c.m;
            let surface = car.wheel(w).road_surface_type(m);
            let mut kind = table(m, SKID_MARK_SURFACE, surface as u32 & 0xF);
            if car.wheel(w).wheel_in_air(m) != 0 {
                kind = 0;
            }
            (r.0[1], r.0[2]) = (kind as u32, surface as u32);
            let skid = if is_front { front } else { rear };
            if near && ((skid != 0 && !rain) || (kind >= 2 && moving != 0)) {
                wheel_point(m, sp, car, w, 0, 80);
                let (limit, over) = if is_front { (FRONT_LIMIT.get(m), front_marks) } else { (REAR_LIMIT.get(m), rear) };
                if kind >= 2 && limit < over {
                    kind = 3;
                    r.0[1] = 3;
                }
                let on = (kind != 0) as i32;
                m.w32(sp + 16, kind as u32);
                m.w32(sp + 20, on as u32);
                r.enter(c, 0x8008_7eb0, ra[on as usize], |c| add_car_sfx(c, car, w, Coord(sp + 48), surface, kind, on));
            } else if car.old_skid_state(m) & (1 << w) != 0 {
                // the mark ends
                let dark = if is_front { car.front_skid(m) } else { car.rear_skid(m) };
                if is_front {
                    car.update_old_skid_state(m, |s| s.wrapping_sub(1 << w));
                }
                let id = n.obj_id(m) as u32;
                r.call(c, nfs4rc::f_800bc1f0, &[id, w, car.old_skid_point(w).0, 1, dark as u32, car.0, 0], ra[2]); // TrgSfx_AddSkidmark
                if !is_front {
                    car.update_old_skid_state(&mut c.m, |s| s.wrapping_sub(1 << w));
                }
            }

            // a splash in the rain
            let m = &mut c.m;
            if rain && kind == 1 && moving != 0 && near {
                wheel_point(m, sp, car, w, 0, if w == 0 { 96 } else { 80 });
                let (id, speed) = (n.obj_id(m) as u32, n.speed_xz(m) as u32);
                r.call(c, nfs4rc::f_800bc090, &[id, w, sp + 48, 10, n.linear_vel().0, 0, speed], ra[3]); // TrgSfx_AddCarSplash
                // (a skid that sounds)
                if is_front {
                    front = front.max(1);
                    r.0[3] = front as u32;
                } else {
                    rear = rear.max(1);
                    r.0[4] = rear as u32;
                }
            }

            // leaves
            let m = &mut c.m;
            if surface & 0x20 != 0 && LEAF_PIXMAP.get(m) != 0 && n.speed_xz(m) > 0xA_0000 {
                let id = n.obj_id(m) as u32;
                if r.call(c, nfs4rc::f_800bbfc0, &[id, w], ra[4]) != 0 {
                    // TrgSfx_AddCarExtraCheck
                    let spread = ((r.call(c, nfs4rc::f_800e8394, &[], ra[5]) & 7) as i32 - 4) * 0x3333; // random
                    let m = &mut c.m;
                    wheel_point(m, sp, car, w, spread, 80);
                    let (speed, y, light) = (n.speed_xz(m), m.r32(sp + 52), car.render().light(m));
                    let id = n.obj_id(m) as u32;
                    let args = [id, w, sp + 48, 13, n.linear_vel().0, speed as u32, y, light as u32];
                    r.call(c, nfs4rc::f_800bbffc, &args, ra[6]); // TrgSfx_AddCarExtraSfx
                }
            }
        }

        // the skids' sounds: front (channel 18, sounding while on), rear (20, at half)
        let m = &mut c.m;
        let audio = m.r32(sp + 112) as i32;
        let human = car.car_flags(m) & CAR_HUMAN != 0;
        for (skid, at, bit, start, channel) in [(front, 116, 1, 2, 18), (rear, 120, 2, 4, 20)] {
            if skid != 0 {
                if (m.r32(sp + at) as i32) < 0 {
                    m.w32(sp + at, 0);
                }
                if m.r32(sp + at) == 0 && !human {
                    continue;
                }
                let force = if bit == 1 { skid } else { skid / 2 };
                set_audio_calls_from(m, sp, car, start, channel, 2, audio, force, 0);
                car.update_old_audio_skid_state(m, |s| s | bit);
            } else if car.old_audio_skid_state(m) & bit != 0 {
                set_audio_calls_from(m, sp, car, start + 1, channel, 2, audio, 0, 0);
                car.update_old_audio_skid_state(m, |s| s.wrapping_sub(bit));
            }
        }
    })
}

/// `Car_DoSkiddingStuff` (CARS.CPP, 0x8008a1f0): on the wheels, the tyres' skids
/// (`Car_TireSkiddingStuff`) and the body's scraping sound stopped; on its side or roof, the
/// body scraping along the road (its sound on the surface, faster than 2 m/s and on the
/// ground) or not.
pub fn do_skidding_stuff(c: &mut Cpu, car: Car) {
    abi::frame(c, 40, |c, sp| {
        let n = car.n();
        let m = &mut c.m;
        if n.orientation_to_ground().y(m) >= 13108 {
            if car.old_audio_skid_state(m) & 4 != 0 {
                set_audio_calls_from(m, sp, car, 5, 20, 1, 0, 0, 0);
                car.update_old_audio_skid_state(m, |s| s - 4);
            }
            let mut regs = abi::Saved::of(c);
            regs.0[0] = car.0;
            regs.enter(c, 0x8008_82f0, 0x8008_a258, |c| tire_skidding_stuff(c, car));
            return;
        }
        let on = n.drive_surface_type(m) as u32;
        let surface = table(m, AUDIO_ROAD_SURFACE, on);
        if n.obj_altitude(m) < 13107 && n.speed_xz(m) > 0x2_0000 {
            set_audio_calls_from(m, sp, car, 4, 20, 1, surface, 0xA_0000, 0);
            car.update_old_audio_skid_state(m, |s| s | 4);
        } else {
            set_audio_calls_from(m, sp, car, 5, 20, 1, 0, 0, 0);
            car.update_old_audio_skid_state(m, |s| s.wrapping_sub(4));
        }
    })
}

/// `Car_DoPostCollisionStuff` (CARS.CPP, 0x8008a300, every other tick per car): after the
/// collisions, the last hit remembered; for an active car near a player the AI's reaction,
/// the extra processing, the skids; and the body's pitch and roll on its springs for the
/// drawing (following the acceleration, quicker for a human on the ground, within limits but
/// for the police helicopter), its height bobbing with the ground; a car far off just level.
pub fn do_post_collision_stuff(c: &mut Cpu, car: Car) {
    abi::frame(c, 32, |c, _| {
        let n = car.n();
        let m = &mut c.m;
        car.set_audio_count(m, 0);
        let hit = n.collision();
        let impulse = hit.impulse(m);
        if impulse != 0 {
            hit.set_last_impulse(m, impulse);
            let now = SIM_COUNTER.get(m);
            hit.set_last_time(m, now);
            let other = hit.other_obj(m);
            hit.set_last_other_obj(m, other);
        }
        let render = car.render();
        if n.sim_optz(m) != 0 || n.active(m) == 0 {
            render.set_body_pitch(m, 0);
            render.set_body_roll(m, 0);
            let ride = car.specs(m).ride_offset(m);
            render.set_current_height(m, ride.wrapping_neg());
            do_extra_car_collision_processing(c, car);
            return;
        }
        abi::call(c, nfs4rc::f_8006d174, &[car.0]); // AIPhysic_ProcessCollision
        do_extra_car_collision_processing(c, car);
        do_skidding_stuff(c, car);
        let m = &mut c.m;
        let acc = car.linear_acc_ch();
        let (pitch, roll) = (render.current_pitch(m), render.current_roll(m));
        let (fwd, side) = (acc.z(m), acc.x(m));
        if car.car_flags(m) & CAR_HUMAN != 0 && n.flight_time(m) == 0 {
            render.set_current_pitch(m, pitch.wrapping_mul(7).wrapping_add(fwd) >> 3);
            render.set_current_roll(m, roll.wrapping_mul(7).wrapping_add(side) >> 3);
        } else {
            render.set_current_pitch(m, pitch.wrapping_mul(15).wrapping_add(fwd) >> 4);
            render.set_current_roll(m, roll.wrapping_mul(15).wrapping_add(side) >> 4);
        }
        if car.car_info(m).car_type(m) != 28 {
            render.update_current_roll(m, |r| r.clamp(-0x14_0000, 0x14_0000));
            render.update_current_pitch(m, |p| p.clamp(-0xA_0000, 0xA_0000));
        }
        let bob = (n.ground_vel(m).wrapping_neg() / 32).clamp(-2621, 1310);
        let roll = render.current_roll(m);
        let specs = car.specs(m);
        let lean = fixedmult(roll.wrapping_mul(3) / 2, specs.body_roll_factor(m)).wrapping_abs();
        let ride = specs.ride_offset(m);
        render.set_current_height(m, bob.wrapping_sub(lean).wrapping_sub(ride));
        render.set_body_roll(m, roll);
        let v = fixedmult(roll, render.roll_factor(m));
        render.set_body_roll(m, v);
        let pitch = render.current_pitch(m).wrapping_neg();
        render.set_body_pitch(m, pitch);
        let v = fixedmult(pitch, render.pitch_factor(m));
        render.set_body_pitch(m, v);
        if car.car_info(m).weight_transfer(m) == 1 || car.car_flags(m) & CAR_HUMAN == 0 {
            render.update_body_pitch(m, |p| p / 2);
            render.update_body_roll(m, |r| r / 2);
        }
    })
}

pub fn abi_set_audio_calls(c: &mut Cpu) {
    let a = [4, 5, 6].map(|k| abi::arg(c, k) as i32);
    set_audio_calls(&mut c.m, Car(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32, a[0], a[1], a[2]);
}

pub fn abi_do_extra_car_collision_processing(c: &mut Cpu) {
    do_extra_car_collision_processing(c, Car(c.r[4]));
}

pub fn abi_reset_variables_after_a_collision(c: &mut Cpu) {
    reset_variables_after_a_collision(c, Car(c.r[4]));
}

pub fn abi_reset_collided_cars(c: &mut Cpu) {
    reset_collided_cars(c, Car(c.r[4]), c.r[5] as i32);
}

pub fn abi_set_car_up_for_hi_rez_sim(c: &mut Cpu) {
    set_car_up_for_hi_rez_sim(c, Car(c.r[4]));
}

pub fn abi_add_car_sfx(c: &mut Cpu) {
    let (kind, on) = (abi::arg(c, 4) as i32, abi::arg(c, 5) as i32);
    add_car_sfx(c, Car(c.r[4]), c.r[5], Coord(c.r[6]), c.r[7] as i32, kind, on);
}

pub fn abi_tire_skidding_stuff(c: &mut Cpu) {
    tire_skidding_stuff(c, Car(c.r[4]));
}

pub fn abi_do_skidding_stuff(c: &mut Cpu) {
    do_skidding_stuff(c, Car(c.r[4]));
}

pub fn abi_do_post_collision_stuff(c: &mut Cpu) {
    do_post_collision_stuff(c, Car(c.r[4]));
}

pub fn abi_do_gravity_effects_on_acc(c: &mut Cpu) {
    do_gravity_effects_on_acc(c, Car(c.r[4]));
}

pub fn abi_find_total_slice(c: &mut Cpu) {
    find_total_slice(&mut c.m, Car(c.r[4]));
}

pub fn abi_find_current_lap(c: &mut Cpu) {
    find_current_lap(&mut c.m, Car(c.r[4]));
}

pub fn abi_calculate_road_span(c: &mut Cpu) {
    c.r[2] = calculate_road_span(&mut c.m, Car(c.r[4])) as u32;
}

pub fn abi_calculate_road_position(c: &mut Cpu) {
    c.r[2] = calculate_road_position(c, Car(c.r[4])) as u32;
}

pub fn abi_calc_vel_down_road(c: &mut Cpu) {
    c.r[2] = calc_vel_down_road(&mut c.m, Car(c.r[4])) as u32;
}

pub fn abi_randomize(c: &mut Cpu) {
    randomize(&mut c.m);
}

pub fn abi_manage_bureaucracy(c: &mut Cpu) {
    manage_bureaucracy(c);
}

pub fn abi_check_for_accident_scenes(c: &mut Cpu) {
    check_for_accident_scenes(c);
}

pub fn abi_sort_cars(c: &mut Cpu) {
    sort_cars(&mut c.m);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original};

    /// The race's cars moved about: on any slice (near the start line more often), with laps
    /// and laps unwound, having crossed slices or not, facing and moving any way, slowly or
    /// not, human or not, active or not; the lists in any order; the track reversed or not.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let slices = NUM_SLICES.get(m);
        GAME_SETUP.set_reverse_track(m, pick(2));
        let n = NUM_CARS.get(m);
        let near_line = |p: &mut dyn FnMut(u32) -> i32| [p(slices as u32), p(60), slices - 1 - p(60), 0, 500 + p(10)][p(5) as usize];
        for i in 0..n {
            let car = Car(m.r32(CARS_LIST + 4 * i as u32));
            let nobj = car.n();
            nobj.sim_road_info().set_slice(m, near_line(&mut pick) as i16);
            nobj.set_old_slice(m, near_line(&mut pick) as u16);
            nobj.set_total_slice(m, pick(3000) as u16);
            nobj.sim_road_info().set_slice_changed(m, pick(2) as i8);
            if pick(8) == 0 {
                nobj.set_active(m, 0);
            }
            car.set_lap(m, pick(4));
            car.set_unlap(m, [0, 0, 1, 2][pick(4) as usize]);
            car.update_car_flags(m, |f| f & !CAR_HUMAN | [0, CAR_HUMAN][pick(2) as usize]);
            car.set_current_speed(m, [pick(0x8_0000) - 0x4_0000, pick(0x80_0000) - 0x40_0000, 0x3_0000, -0x3_0000, 0x2_FFFF][pick(5) as usize]);
            car.set_road_position(m, pick(0x1000));
            let v = nobj.linear_vel();
            v.set_x(m, pick(0x80_0000) - 0x40_0000);
            v.set_z(m, pick(0x80_0000) - 0x40_0000);
            if pick(2) == 0 {
                // turned round
                for k in [0, 2, 6, 8] {
                    nobj.orient_mat().update_m(m, k, |x| x.wrapping_neg());
                }
            }
        }
        for list in [SORTED_LIST, TOTAL_SORTED_LIST] {
            for i in (1..n).rev() {
                let j = pick(i as u32 + 1);
                let (a, b) = (m.r32(list + 4 * i as u32), m.r32(list + 4 * j as u32));
                m.w32(list + 4 * i as u32, b);
                m.w32(list + 4 * j as u32, a);
            }
        }
        NUM_AI_CARS.set(m, pick(3));
        SIM_COUNTER.set(m, pick(100_000));
    }

    pub(crate) fn car(c: &mut Cpu, i: u32) -> u32 {
        c.m.r32(CARS_LIST + 4 * i)
    }

    #[test]
    fn laps_fuzz() {
        for k in 0..3 {
            against_original("Cars_FindCurrentLap", 5_000, 0x1A9 + k as u64, shuffle, nfs4rc::f_8008b654, &|c| vec![car(c, k)], |c, a| {
                find_current_lap(&mut c.m, Car(a[0]));
                None
            });
            against_original("Cars_FindTotalSlice", 2_000, 0x7071 + k as u64, shuffle, nfs4rc::f_8008a184, &|c| vec![car(c, k)], |c, a| {
                find_total_slice(&mut c.m, Car(a[0]));
                None
            });
        }
    }

    #[test]
    fn road_fuzz() {
        for k in 0..3 {
            against_original("Cars_CalculateRoadSpan", 2_000, 0x5BA + k as u64, shuffle, nfs4rc::f_8008b70c, &|c| vec![car(c, k)], |c, a| Some(calculate_road_span(&mut c.m, Car(a[0])) as u32));
            against_original("Cars_CalculateRoadPosition", 1_000, 0x9051 + k as u64, shuffle, nfs4rc::f_8008b954, &|c| vec![car(c, k)], |c, a| Some(calculate_road_position(c, Car(a[0])) as u32));
            against_original("Cars_CalcVelDownRoad", 1_000, 0xDD + k as u64, shuffle, nfs4rc::f_8008b974, &|c| vec![car(c, k)], |c, a| Some(calc_vel_down_road(&mut c.m, Car(a[0])) as u32));
        }
    }

    #[test]
    fn bureaucracy_fuzz() {
        against_original("Cars_SortCars", 5_000, 0x5027, shuffle, nfs4rc::f_8008bd18, &|_| vec![], |c, _| {
            sort_cars(&mut c.m);
            None
        });
        against_original("Cars_Randomize", 2_000, 0x2A4D, shuffle, nfs4rc::f_8008ba10, &|_| vec![], |c, _| {
            randomize(&mut c.m);
            None
        });
        against_original("Cars_ManageBureaucracy", 5_000, 0xB0BE, shuffle, nfs4rc::f_8008ba88, &|_| vec![], |c, _| {
            manage_bureaucracy(c);
            None
        });
    }

    /// A car of the race after a collision: hit by something (the road, a wall, a car; hard
    /// or not; with another car or not), damaged, a tyre blown, the player resetting, on its
    /// side or roof, scraping, smoking, in the air, far off; as the human or not.
    pub(crate) fn crash(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        for i in 0..NUM_CARS.get(m) {
            let car = Car(m.r32(CARS_LIST + 4 * i as u32));
            let n = car.n();
            let hit = n.collision();
            hit.set_impulse(m, [0, pick(0x10_0000), pick(0x100_0000), 0x4B_0001 + pick(0x10_0000)][pick(4) as usize]);
            let kind = [0x1_0000, 0x3_0000, 0x4_0000, 0x5_0000, 0x2_0000][pick(5) as usize] | [0, 0x1000_0000][pick(2) as usize] | pick(16);
            hit.set_sfx_type(m, kind);
            let other = if pick(2) == 0 { 0 } else { m.r32(CARS_LIST + 4 * ((i as u32 + 1) % 3)) };
            hit.set_other_obj(m, crate::types::NewtonObj(other));
            let p = n.position().get(m);
            hit.collision_point().set(m, p);
            for k in 0..10 {
                n.set_damage(m, k, [0, pick(0x40_0000)][pick(2) as usize]);
            }
            car.render().set_damage_parts(m, pick(0x80) as i16);
            car.set_blowout(m, [0, 0, 1 + pick(400), 320][pick(4) as usize]);
            car.set_pull_over(m, (pick(6) == 0) as i32);
            car.control().set_abort(m, (pick(3) == 0) as i8);
            car.stats().set_finish_type(m, (pick(5) == 0) as i32);
            car.stats().set_fatal_crashes(m, [0, 0, pick(200)][pick(3) as usize]);
            car.collision().set_smoking(m, pick(2));
            car.collision().set_reset_timer(m, [0, 0, -pick(10), 1 + pick(400)][pick(4) as usize]);
            car.set_audio_damage_scrape(m, pick(2));
            car.render().set_current_roll(m, pick(0x40_0000) - 0x20_0000);
            car.render().set_current_pitch(m, pick(0x20_0000) - 0x10_0000);
            for w in 0..4 {
                car.wheel(w).set_road_surface_type(m, pick(32));
            }
            n.orientation_to_ground().set_y(m, [0x1_0000, pick(0x2_0000) - 0x1_0000, 0xE665, 13108, 13107][pick(5) as usize]);
            n.set_obj_altitude(m, pick(0x2_0000));
            n.set_speed_xz(m, [pick(0x4_0000), pick(0x40_0000), 0x1B_9998, 0xF_0001][pick(4) as usize]);
            n.set_drive_surface_type(m, [0, 14, pick(16), 1][pick(4) as usize]);
            n.set_sim_optz(m, (pick(6) == 0) as u8);
            n.set_flight_time(m, [0, 0, 1 + pick(8)][pick(3) as usize] as u16);
            n.set_ground_vel(m, pick(0x20_0000) - 0x10_0000);
            car.linear_acc_ch().set_x(m, pick(0x40_0000) - 0x20_0000);
            car.linear_acc_ch().set_z(m, pick(0x40_0000) - 0x20_0000);
            car.linear_vel_ch().set_x(m, [0, pick(0x2000), pick(0x10_0000)][pick(3) as usize]);
            car.linear_vel_ch().set_z(m, [0, pick(0x2000), pick(0x10_0000)][pick(3) as usize]);
            car.set_old_audio_skid_state(m, pick(8));
            car.set_audio_count(m, pick(3));
            if pick(4) == 0 {
                // on its side or roof
                for k in [3, 4, 5] {
                    n.orient_mat().update_m(m, k, |x| x.wrapping_neg());
                }
            }
            car.car_info(m).set_car_type(m, [pick(28), 28, 27, 0][pick(4) as usize]);
            car.car_info(m).set_weight_transfer(m, pick(2));
        }
        SIM_COUNTER.set(m, [pick(1000), 832, 833, 5000][pick(4) as usize]);
    }

    #[test]
    fn collision_aftermath_fuzz() {
        for k in 0..3 {
            against_original("Cars_DoExtraCarCollisionProcessing", 5_000, 0xE7C0 + k as u64, crash, nfs4rc::f_8008755c, &|c| vec![car(c, k)], |c, a| {
                do_extra_car_collision_processing(c, Car(a[0]));
                None
            });
            against_original("Car_DoSkiddingStuff", 3_000, 0x5C1D + k as u64, crash, nfs4rc::f_8008a1f0, &|c| vec![car(c, k)], |c, a| {
                do_skidding_stuff(c, Car(a[0]));
                None
            });
            against_original("Car_DoPostCollisionStuff", 5_000, 0x9057 + k as u64, crash, nfs4rc::f_8008a300, &|c| vec![car(c, k)], |c, a| {
                do_post_collision_stuff(c, Car(a[0]));
                None
            });
        }
    }

    /// The race's cars skidding or not: either axle's skid (none, light, past the limits,
    /// beyond 10), thrown sideways, sliding fast along; near a player or far, on the ground or
    /// in the air; each wheel in the air or on any surface (leafy, wet); old marks going on or
    /// ended; rain or not, any tyres; the helicopter and the big vehicles; leaves on the track.
    pub(crate) fn skidding(c: &mut Cpu, rng: &mut Rng) {
        crash(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        GAME_SETUP.set_weather(m, [0, 0, 1, -1][pick(4) as usize]);
        LEAF_PIXMAP.set(m, [0, 0, 0x8010_0000][pick(3) as usize]);
        for i in 0..NUM_CARS.get(m) {
            let car = Car(m.r32(CARS_LIST + 4 * i as u32));
            let n = car.n();
            let skid = |p: &mut dyn FnMut(u32) -> i32| [0, 0, p(0x4_0000), p(0x10_0000), 0xA_0000, -p(0x1000), 0x1_0000 + p(0x6_0000)][p(7) as usize];
            car.set_front_skid(m, skid(&mut pick));
            car.set_rear_skid(m, skid(&mut pick));
            car.set_g_transfer_right(m, pick(0x8_0000) - 0x4_0000);
            car.linear_vel_ch().set_z(m, [0, pick(0x40_0000) - 0x20_0000, 0x14_0001, -0x14_0001][pick(4) as usize]);
            n.set_dist_to_player(m, [0, pick(0x40_0000), 0x3B_FFFF, 0x3C_0000][pick(4) as usize]);
            n.set_obj_altitude(m, [0, pick(0x8000), 26213, 26214][pick(4) as usize]);
            n.set_speed_xz(m, [0, pick(0x2_0000), pick(0x40_0000), 0xA_0001, 0xA_0000][pick(5) as usize]);
            n.set_ground_elevation(m, pick(0x100_0000) - 0x80_0000);
            for w in 0..4 {
                let wheel = car.wheel(w);
                wheel.set_wheel_in_air(m, (pick(4) == 0) as i32);
                wheel.set_road_surface_type(m, pick(16) | [0, 0x20, 0x10, 0x40][pick(4) as usize]);
                let p = n.position().get(m);
                let near = crate::vec::Vec3::new(p.x + pick(0x4_0000) - 0x2_0000, p.y, p.z + pick(0x4_0000) - 0x2_0000);
                car.old_skid_point(w).set(m, near);
            }
            car.set_old_skid_state(m, pick(16));
            car.set_old_audio_skid_state(m, pick(4));
            car.set_audio_count(m, pick(2));
            car.render().set_current_car_type(m, [pick(28) as i16, 28, 29, 30, 0][pick(5) as usize]);
            car.car_info(m).set_tire_type(m, pick(3));
        }
    }

    #[test]
    fn skid_fuzz() {
        // Cars_AddCarSfx on its own: a wheel at a point near or at the old mark's, any surface,
        // kind and switch (put at SCRATCH for the arguments)
        fn sfx_args(c: &mut Cpu, rng: &mut Rng) {
            skidding(c, rng);
            let m = &mut c.m;
            let mut pick = |n: u32| (rng.next() % n as u64) as i32;
            let car = Car(m.r32(CARS_LIST + 4 * pick(3) as u32));
            let w = pick(4) as u32;
            let mut p = car.old_skid_point(w).get(m);
            if pick(3) != 0 {
                p.x += pick(0x1000) - 0x800;
            }
            if pick(3) != 0 {
                p.z += pick(0x1000) - 0x800;
            }
            let at = crate::test_util::SCRATCH;
            Coord(at).set(m, p);
            for (k, v) in [car.0, w, pick(64) as u32, pick(4) as u32, [0, 1, pick(3)][pick(3) as usize] as u32].into_iter().enumerate() {
                m.w32(at + 16 + 4 * k as u32, v);
            }
        }
        let args = |c: &mut Cpu| {
            let at = crate::test_util::SCRATCH;
            let a = [0, 1, 2, 3, 4].map(|k| c.m.r32(at + 16 + 4 * k));
            vec![a[0], a[1], at, a[2], a[3], a[4]]
        };
        against_original("Cars_AddCarSfx", 10_000, 0xADD5, sfx_args, nfs4rc::f_80087eb0, &args, |c, a| {
            // (the caller's stack arguments)
            c.m.w32(c.r[29] + 16, a[4]);
            c.m.w32(c.r[29] + 20, a[5]);
            add_car_sfx(c, Car(a[0]), a[1], Coord(a[2]), a[3] as i32, a[4] as i32, a[5] as i32);
            None
        });
        for k in 0..3 {
            against_original("Car_TireSkiddingStuff", 5_000, 0x71BE + k as u64, skidding, nfs4rc::f_800882f0, &|c| vec![car(c, k)], |c, a| {
                tire_skidding_stuff(c, Car(a[0]));
                None
            });
            against_original("Car_DoSkiddingStuff", 3_000, 0xD051 + k as u64, skidding, nfs4rc::f_8008a1f0, &|c| vec![car(c, k)], |c, a| {
                do_skidding_stuff(c, Car(a[0]));
                None
            });
        }
    }

    /// [`crate::test_util::same_stack`] on one of the race's first three cars.
    #[allow(clippy::too_many_arguments)]
    fn same_stack(name: &str, addr: u32, original: fn(&mut Cpu), ra: u32, top: u32, cases: u32, seed: u64, shuffle: fn(&mut Cpu, &mut Rng)) {
        crate::test_util::same_stack(name, addr, original, ra, top, cases, seed, shuffle, &|c, rng| vec![car(c, (rng.next() % 3) as u32)]);
    }

    /// The skids leave the stack as the original: the skid marks' colours take a byte the
    /// stack held before.
    #[test]
    fn skid_stack_fuzz() {
        same_stack("Car_DoSkiddingStuff", 0x8008_a1f0, nfs4rc::f_8008a1f0, 0x8008_a3ac, crate::test_util::SCRATCH_TOP, 3000, 0x57AC, skidding);
    }

    /// The collisions' aftermath (the cars put back on the road) leaves the stack and RAM as
    /// the original (on the main stack: putting a car on the road can come back round to do it
    /// again, deeper than the scratchpad goes).
    #[test]
    fn reset_stack_fuzz() {
        same_stack("Cars_DoExtraCarCollisionProcessing", 0x8008_755c, nfs4rc::f_8008755c, 0x8008_a3a4, 0x801F_FE00, 3000, 0xE5AC, stranded);
        same_stack("Cars_SetCarUpForHiRezSim", 0x8008_74e8, nfs4rc::f_800874e8, 0x800a_1f98, 0x801F_FE00, 1000, 0x5E7C, stranded);
    }

    /// The race's cars to be put back on the road: crashed (as [`crash`]), tilted, spinning,
    /// off the road or in the air or not, going either way, any grid place and width, the
    /// road off its surface sometimes, near an accident scene (at either end of the track
    /// too), traffic on either side, simulated fully, roughly or not.
    pub(crate) fn stranded(c: &mut Cpu, rng: &mut Rng) {
        crash(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let slices = NUM_SLICES.get(m);
        ACCIDENT_SLICE.set(m, [0, 0, -3, pick(slices as u32), 2, slices - 3][pick(6) as usize]);
        AITUNE_DRIVE_SIDE.set(m, [1, -1][pick(2) as usize]);
        for i in 0..NUM_CARS.get(m) {
            let car = Car(m.r32(CARS_LIST + 4 * i as u32));
            let n = car.n();
            if pick(2) == 0 {
                // near the accident scene
                let at = (ACCIDENT_SLICE.get(m) + pick(16) - 8).rem_euclid(slices);
                n.sim_road_info().set_slice(m, at as i16);
            }
            car.set_blowout(m, [0, 0, 0, 1][pick(4) as usize]);
            car.set_desired_direction(m, [1, -1][pick(2) as usize]);
            car.set_car_index(m, pick(8) - 1);
            let spin = n.angular_vel();
            spin.set_x(m, [0, pick(0x2_0000), 0xFFFF, 0x1_0000][pick(4) as usize]);
            spin.set_z(m, [0, pick(0x2_0000), 0xFFFF, 0x1_0000][pick(4) as usize]);
            n.set_flight_time(m, [0, 5, 6, pick(10)][pick(4) as usize] as u16);
            n.set_obj_altitude(m, [0, 0xFFFF, 0x1_0000, pick(0x2_0000)][pick(4) as usize]);
            n.set_sim_optz(m, pick(3) as u8);
            if pick(4) == 0 {
                n.dimension().set_x(m, pick(0x40_0000));
            }
            if pick(8) == 0 {
                n.sim_road_info().set_sim_quad(m, crate::types::TrkNewSimQuad(0));
            }
        }
    }

    #[test]
    fn reset_fuzz() {
        for k in 0..3 {
            for how in 0..3u32 {
                against_original("Cars_ResetCollidedCars", 300, 0x2E5E + 3 * k as u64 + how as u64, stranded, nfs4rc::f_80087088, &|c| vec![car(c, k), how, 0], |c, a| {
                    reset_collided_cars(c, Car(a[0]), a[1] as i32);
                    None
                });
            }
            against_original("Cars_SetCarUpForHiRezSim", 2_000, 0x41E2 + k as u64, stranded, nfs4rc::f_800874e8, &|c| vec![car(c, k)], |c, a| {
                set_car_up_for_hi_rez_sim(c, Car(a[0]));
                None
            });
            against_original("Cars_ResetVariablesAfterACollision", 500, 0x7A2 + k as u64, stranded, nfs4rc::f_80087020, &|c| vec![car(c, k)], |c, a| {
                reset_variables_after_a_collision(c, Car(a[0]));
                None
            });
        }
        against_original("Cars_DoExtraCarCollisionProcessing", 5_000, 0xE7D0, stranded, nfs4rc::f_8008755c, &|c| vec![car(c, 0)], |c, a| {
            do_extra_car_collision_processing(c, Car(a[0]));
            None
        });
    }
}
