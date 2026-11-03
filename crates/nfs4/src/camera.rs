//! CAMERA.CPP: the race's cameras, one a player (`Camera_gInfo`): the views from the car (the
//! bumper, the tail, the helicopter), the replays' (the TV cameras along the track, the
//! circling, the blimp, the splines, the animations), the finish's and the police's, kept off
//! the walls and above the ground; the view handed to the drawing and the sound.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call, call_addr, with_buffer};
use crate::fixed::fixedmult;
use crate::vec::Vec3;
use crate::globals::{GAME_SETUP, HUMAN_RACE_CARS};
use crate::types::{CameraInfo, Coord, Matrix};
use ps1rt::{Cpu, Mem};

/// The players' cameras (`Camera_gInfo`, 272 bytes each).
pub const CAMERA_INFO: CameraInfo = CameraInfo(0x8011_0570);
/// The camera's bit fields (a word at +0x74; its second and third bytes the transition's count
/// and the crash's, `TRANSITION` and `TUMBLING`): bit 0 back from a special camera, bits 1 to
/// 6 the mode's (1 aimed above the car, 2 pitch and roll, 3 aimed at the car, 4 off the walls,
/// 5 no looking behind, 6 the crash camera), bit 7 split screen (zoomed out), bit 24 the arm
/// flipped, bits 25-26 the zoom's kind, bit 27 a bumper camera.
const CAMERA_BITS: u32 = 0x74;
/// The projection's distance (`Camera_gGeomScreen`).
pub const GEOM_SCREEN: u32 = 0x8013_DAD0;

/// Whether the simulation is paused (`simVar.quickPauseSim`).
const QUICK_PAUSE_SIM: u32 = 0x8011_F38C;

const FIXED_XFORM: [fn(&mut Mem, u32, i32); 3] = [crate::fixed::fixedxformx, crate::fixed::fixedxformy, crate::fixed::fixedxformz];
const MATH_FASTTRANSMULT: u32 = 0x8009_e850;
const MATRIX_SIZE: u32 = 36;

/// Player `p`'s camera.
pub fn info(p: u32) -> CameraInfo {
    CAMERA_INFO.at(p)
}

/// `EulerToMat` (CAMERA.CPP, 0x80081030): `out` = the turns about z, y and x (in that order)
/// combined, its second and third rows swapped and the new second negated (the camera's frame:
/// y down).
pub fn euler_to_mat(c: &mut Cpu, out: Matrix, x: i32, y: i32, z: i32) {
    with_buffer(c, 4 * MATRIX_SIZE, |c, buf| {
        let turn = |k: u32| buf + MATRIX_SIZE * k;
        for (k, angle) in [x, y, z].into_iter().enumerate() {
            FIXED_XFORM[k](&mut c.m, turn(k as u32), angle);
        }
        call_addr(c, MATH_FASTTRANSMULT, &[turn(2), turn(1), turn(3)]);
        call_addr(c, MATH_FASTTRANSMULT, &[turn(3), turn(0), out.0]);
        let m = &mut c.m;
        let row = |k: u32| out.0 + 12 * k;
        let second = [m.r32(row(1)), m.r32(row(1) + 4), m.r32(row(1) + 8)];
        for k in [0, 2, 1] {
            let v = m.r32(row(2) + 4 * k).wrapping_neg();
            m.w32(row(1) + 4 * k, v);
        }
        for (k, v) in second.into_iter().enumerate() {
            m.w32(row(2) + 4 * k as u32, v);
        }
    });
}

pub fn abi_euler_to_mat(c: &mut Cpu) {
    euler_to_mat(c, Matrix(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// A view (`DRender_tView`: +0 its kind, +4 its player, +8 its view's maths).
const CAMERA_GET_VIEW_INFO: u32 = 0x8008_5a2c;
const TRS_PROJ_SET_VIEW_ENVIRO: fn(&mut Cpu) = nfs4rc::f_800e2614;

/// `Camera_SetCamera` (CAMERA.CPP, 0x80081128): view `view`'s maths from its camera, then the
/// projection's set-up for it.
pub fn set_camera(c: &mut Cpu, view: u32) {
    let (player, kind) = (c.m.r32(view + 4), c.m.r32(view));
    call_addr(c, CAMERA_GET_VIEW_INFO, &[player, view + 8, kind]);
    call(c, TRS_PROJ_SET_VIEW_ENVIRO, &[view]);
}

pub fn abi_set_camera(c: &mut Cpu) {
    set_camera(c, c.r[4]);
}

/// `Camera_LookBack` (CAMERA.CPP, 0x80081160): `out` = `m` looking back (its first and last
/// rows negated).
pub fn look_back(m: &mut Mem, from: Matrix, out: Matrix) {
    for k in 0..9 {
        let v = m.r32(from.0 + 4 * k);
        m.w32(out.0 + 4 * k, if (3..6).contains(&k) { v } else { v.wrapping_neg() });
    }
}

pub fn abi_look_back(c: &mut Cpu) {
    look_back(&mut c.m, Matrix(c.r[4]), Matrix(c.r[5]));
}

/// The camera's place on the track (`slicePos`, a `BWorldSm_Pos`) and an anchor's (its
/// `simRoadInfo`); a Newton object's place (+0xA0).
const SLICE_POS: u32 = 0x8C;
const ANCHOR_ROAD_INFO: u32 = 8;
const OBJ_POSITION: u32 = 0xA0;
/// The tunnels' camera heights by track (the tracks from 16 seven down).
const TUNNEL_CAM_HEIGHT: u32 = 0x8011_0790;
const TUNNEL_FLAG_SM: u32 = 0x8008_09f8;
const U_NORMAL: u32 = 0x8008_0f44;
const FIND_GROUND_ELEVATION_GENERAL: u32 = 0x800a_063c;

/// `Camera_TunnelLimit` (CAMERA.CPP, 0x800811e8): in a tunnel (the camera's place or its
/// anchor's), `*limit` lowered to the tunnel's camera height over the anchor (the ground under
/// the camera's place along the road's normal).
pub fn tunnel_limit(c: &mut Cpu, p: u32, limit: u32) {
    let cam = info(p);
    let slice_pos = cam.0 + SLICE_POS;
    let in_tunnel = call_addr(c, TUNNEL_FLAG_SM, &[slice_pos]) != 0 || {
        let anchor = cam.anchor(&mut c.m).0;
        call_addr(c, TUNNEL_FLAG_SM, &[anchor + ANCHOR_ROAD_INFO]) != 0
    };
    if !in_tunnel {
        return;
    }
    let normal = call_addr(c, U_NORMAL, &[slice_pos]);
    let ground = with_buffer(c, 24, |c, buf| {
        let (n, pt) = (Coord(buf), Coord(buf + 12));
        let m = &mut c.m;
        let up = Coord(normal).get(m);
        n.set(m, up);
        let pos = cam.position().get(m);
        pt.set(m, pos);
        call_addr(c, FIND_GROUND_ELEVATION_GENERAL, &[pt.0, n.0, slice_pos + 8]) as i32
    });
    let m = &mut c.m;
    let track = GAME_SETUP.track(m);
    let track = if track >= 16 { track - 7 } else { track };
    let height = m.r32(TUNNEL_CAM_HEIGHT.wrapping_add((track as u32) << 2)) as i32;
    let anchor = cam.anchor(m).0;
    let anchor_y = m.r32(anchor + OBJ_POSITION + 4) as i32;
    let top = height.wrapping_sub(anchor_y).wrapping_add(ground);
    if top < m.r32(limit) as i32 {
        m.w32(limit, top as u32);
    }
}

pub fn abi_tunnel_limit(c: &mut Cpu) {
    tunnel_limit(c, c.r[4], c.r[5]);
}

/// `Camera_IslandProfile` (CAMERA.CPP, 0x80083660): whether the set bits of `bits` make more
/// than one run (an island of set bits between clear ones).
pub fn island_profile(bits: u16) -> bool {
    let mut v = bits;
    let mut lowest_run = v.wrapping_sub(v & v.wrapping_sub(1));
    // (each step: the lowest set bit removed; one run only while each next is the one above)
    let mut rest = v & v.wrapping_sub(1);
    while v != 0 {
        if v.wrapping_sub(rest) != lowest_run {
            return true;
        }
        v = rest;
        rest = v & v.wrapping_sub(1);
        lowest_run <<= 1;
    }
    false
}

pub fn abi_island_profile(c: &mut Cpu) {
    c.r[2] = island_profile(c.r[4] as u16) as u32;
}

/// The cameras' animations' handles (none: -1) and `0x8007464c` freeing one.
const ANIM_FREE: fn(&mut Cpu) = nfs4rc::f_8007464c;

/// `Camera_Kill` (CAMERA.CPP, 0x80084610): each player's camera's animation freed (both with
/// split screen linked: the setup's `commMode` 1).
pub fn kill(c: &mut Cpu) {
    let last = (GAME_SETUP.comm_mode(&mut c.m) == 1) as u32;
    for p in 0..=last {
        let cam = info(p);
        let handle = cam.anim_handle(&mut c.m);
        if handle != -1 {
            call(c, ANIM_FREE, &[handle as i32 as u32]);
        }
        cam.set_anim_handle(&mut c.m, -1);
    }
}

pub fn abi_kill(c: &mut Cpu) {
    kill(c);
}

/// A car's flags (`carFlags`: 1 racing), being pulled over (`pullOver`), how it finished
/// (`stats.finishType`: 2 out, 3 arrested).
const CAR_FLAGS: u32 = 0x260;
const CAR_RACING: u32 = 0x1;
const CAR_PULL_OVER: u32 = 0x278;
const CAR_FINISH_TYPE: u32 = 0x390;
/// The camera mode a player's out of the race or pulled over shows.
pub const MODE_PULLOVER: i32 = 20;

/// `Camera_GetMode` (CAMERA.CPP, 0x80085f8c): player `p`'s camera mode: the pull-over's when
/// the car is out of the race or being pulled over (not once arrested), else its own.
pub fn get_mode(m: &mut Mem, p: u32) -> i32 {
    let car = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
    let finish = m.r32(car + CAR_FINISH_TYPE) as i32;
    if m.r32(car + CAR_FLAGS) & CAR_RACING != 0 && finish == 2 {
        return MODE_PULLOVER;
    }
    if m.r32(car + CAR_PULL_OVER) != 0 && finish != 3 {
        return MODE_PULLOVER;
    }
    info(p).mode(m) as i32
}

pub fn abi_get_mode(c: &mut Cpu) {
    c.r[2] = get_mode(&mut c.m, c.r[4]) as u32;
}

const SET_MODE: u32 = 0x8008_602c;

/// `Camera_ReplayUpdate` (CAMERA.CPP, 0x800865f4): player `p`'s camera from a replay's camera
/// slot (`Camera_tCamSlot`), unless one already holds it (bit 0): its mode, its bits (in
/// transition, the zoom's kind, the spline's mode from the slot's first word), its place and
/// height, the projection, its turn from the slot's angles.
pub fn replay_update(c: &mut Cpu, p: u32, slot: u32) {
    let cam = info(p);
    let bits_at = cam.0 + CAMERA_BITS;
    if c.m.r8(bits_at) & 1 != 0 {
        return;
    }
    let mode = c.m.r8(slot) as u32;
    call_addr(c, SET_MODE, &[p, mode]);
    let m = &mut c.m;
    let flags = m.r32(slot);
    let bits = (m.r32(bits_at) & !0x8) | ((flags >> 5) & 0x8);
    m.w32(bits_at, bits);
    let flags = m.r32(slot);
    let bits = (bits & 0xF9FF_FFFF) | (((flags >> 9) & 3) << 25);
    m.w32(bits_at, bits);
    let flags = m.r32(slot);
    cam.set_spline_mode(m, ((flags >> 11) & 7) as i8);
    let fov = m.r16(slot + 2) as i16 as i32;
    let pos = Coord(slot + 4).get(m);
    cam.position().set(m, pos);
    let height = m.r32(slot + 16);
    cam.set_tv_height(m, height as i32);
    let (x, y, z) = (m.r16(slot + 24) as i16 as i32, m.r16(slot + 26) as i16 as i32, m.r16(slot + 28) as i16 as i32);
    m.w32(GEOM_SCREEN, fov as u32);
    euler_to_mat(c, cam.rotation(), x, y, z);
}

pub fn abi_replay_update(c: &mut Cpu) {
    replay_update(c, c.r[4], c.r[5]);
}

/// A Newton object's turn (`orientMat`, +0xF0; its third row the way it faces).
const OBJ_ORIENT_FORWARD: u32 = 0xF0 + 24;

/// `Camera_ResetRelPos` (CAMERA.CPP, 0x800866f8): the cameras of `players` (bit 0 the first,
/// bit 1 the second) behind their anchors: four times the way the anchor faces, backwards.
pub fn reset_rel_pos(m: &mut Mem, players: u32) {
    for p in 0..2 {
        if players & (1 << p) == 0 {
            continue;
        }
        let cam = info(p);
        let anchor = cam.anchor(m).0;
        for k in 0..3 {
            let v = (m.r32(anchor + OBJ_ORIENT_FORWARD + 4 * k) << 2).wrapping_neg();
            m.w32(cam.relpos().0 + 4 * k, v);
        }
    }
}

pub fn abi_reset_rel_pos(c: &mut Cpu) {
    reset_rel_pos(&mut c.m, c.r[4]);
}

/// The finish's zoomed-out projection; `SetGeomScreen`.
const FINISH_GEOM_SCREEN: u32 = 190;
const SET_GEOM_SCREEN: fn(&mut Cpu) = nfs4rc::f_800eb4f8;

/// `SetCameraZoom` (CAMERA.CPP, 0x8008275c): player `p`'s camera's projection: the finish's
/// zoomed out (bit 7), else from `distance` past 12288 by the zoom's kind (1: distance * 190 /
/// 12288; 2: the same, past 1200 a quarter of 6000 less it, at least 750); the projection set.
pub fn set_camera_zoom(c: &mut Cpu, p: u32, distance: i32) {
    let m = &mut c.m;
    let bits = m.r32(info(p).0 + CAMERA_BITS);
    let zoom = distance.wrapping_mul(190) / 12288;
    if (bits >> 7) & 1 != 0 {
        m.w32(GEOM_SCREEN, FINISH_GEOM_SCREEN);
    } else if distance > 12288 {
        match (bits >> 25) & 3 {
            1 => m.w32(GEOM_SCREEN, zoom as u32),
            2 => {
                m.w32(GEOM_SCREEN, zoom as u32);
                let zoom = if zoom > 1200 { ((6000 - zoom) >> 2).max(750) } else { zoom };
                m.w32(GEOM_SCREEN, zoom as u32);
            }
            _ => {}
        }
    }
    let screen = c.m.r32(GEOM_SCREEN);
    call(c, SET_GEOM_SCREEN, &[screen]);
}

pub fn abi_set_camera_zoom(c: &mut Cpu) {
    set_camera_zoom(c, c.r[4], c.r[5] as i32);
}

/// The cameras' arms by mode (`Camera_gFlags`, 16 bytes each: the camera's place off its anchor
/// in the anchor's frame); the crash camera's arm.
const CAMERA_FLAGS: u32 = 0x8011_0814;
const COLLISION_ARM: u32 = 0x8005_6054;
/// A Newton object's turn (`orientMat`, +0xF0) and the road's frame under it (`roadMatrix`,
/// +0x144).
const OBJ_ORIENT: u32 = 0xF0;
const OBJ_ROAD_MATRIX: u32 = 0x144;
/// The camera's bytes in the bits word: whether its arm is flipped behind (+0x77 bit 0), the
/// transition's countdown (+0x75), whether it tumbles (+0x76); the bits a crash sets (1, 3 to
/// 6) and clears (2); bit 5 the look-behind held off.
const ARM_FLIPPED: u32 = 0x77;
const TRANSITION: u32 = 0x75;
const TUMBLING: u32 = 0x76;
const TRANSFORM: fn(&mut Cpu) = nfs4rc::f_800eb834;
/// How fast the crash camera's arm follows (16.16: about 2%).
const COLLISION_EASE: u32 = 1310;

/// `arm` (a coord) turned by `matrix` into `out`, the camera's place off its anchor.
fn turn_arm(c: &mut Cpu, arm: u32, matrix: u32, out: u32) {
    call(c, TRANSFORM, &[arm, matrix, out]);
}

/// The camera's place: its anchor's plus `off`.
fn place_off_anchor(m: &mut Mem, cam: CameraInfo, at: Coord, off: [i32; 3]) {
    for k in 0..3 {
        let anchor = cam.anchor(m).0;
        let v = (m.r32(anchor + OBJ_POSITION + 4 * k) as i32).wrapping_add(off[k as usize]);
        m.w32(at.0 + 4 * k, v as u32);
    }
}

fn coord(m: &mut Mem, at: u32) -> [i32; 3] {
    [m.r32(at) as i32, m.r32(at + 4) as i32, m.r32(at + 8) as i32]
}

/// `Camera_UpdateCollisionCam` (CAMERA.CPP, 0x80081330): player `p`'s camera in a crash (not
/// while paused unless the camera's being changed, nor between races): the crash arm (flipped
/// behind when so) turned with the road under the car, kept under a tunnel's roof, eased toward
/// 2% a frame; the camera there; in transition 50 frames unless tumbling; the crash's bits.
pub fn update_collision_cam(c: &mut Cpu, p: u32) {
    use crate::globals::{IN_BETWEEN, REPLAY_INTERFACE};
    let m = &mut c.m;
    let paused = m.r32(QUICK_PAUSE_SIM) != 0 && REPLAY_INTERFACE.change_camera(m) == 0;
    if paused || IN_BETWEEN.get(m) != 0 {
        return;
    }
    let cam = info(p);
    with_buffer(c, 24, |c, buf| {
        let (arm, out) = (buf, buf + 12);
        let m = &mut c.m;
        let a = coord(m, COLLISION_ARM);
        let z = if m.r8(cam.0 + ARM_FLIPPED) & 1 != 0 { a[2].wrapping_neg() } else { a[2] };
        Coord(arm).set(m, Vec3::new(a[0], a[1], z));
        let anchor = cam.anchor(m).0;
        turn_arm(c, arm, anchor + OBJ_ROAD_MATRIX, out);
        tunnel_limit(c, p, out + 4);
        let m = &mut c.m;
        let old = coord(m, cam.relpos().0);
        for k in 0..3u32 {
            let target = m.r32(out + 4 * k) as i32;
            let eased = old[k as usize].wrapping_add(fixedmult(target.wrapping_sub(old[k as usize]), COLLISION_EASE as i32));
            m.w32(cam.relpos().0 + 4 * k, eased as u32);
        }
        let rel = coord(m, cam.relpos().0);
        place_off_anchor(m, cam, cam.position(), rel);
        if m.r8(cam.0 + TUMBLING) == 0 {
            m.w8(cam.0 + TRANSITION, 50);
        }
        let bits = ((m.r32(cam.0 + CAMERA_BITS) | 0x2) & !0x4) | 0x78;
        m.w32(cam.0 + CAMERA_BITS, bits);
    });
}

pub fn abi_update_collision_cam(c: &mut Cpu) {
    update_collision_cam(c, c.r[4]);
}

/// The arm of camera mode `mode`.
fn mode_arm(m: &mut Mem, mode: i16) -> [i32; 3] {
    coord(m, CAMERA_FLAGS.wrapping_add((mode as i32 as u32) << 4))
}

/// `Camera_UpdateSimpleCam` (CAMERA.CPP, 0x800814ec): player `p`'s camera on its mode's arm
/// turned with the car (under a tunnel's roof), looking back at the car.
pub fn update_simple_cam(c: &mut Cpu, p: u32) {
    let cam = info(p);
    with_buffer(c, 24, |c, buf| {
        let (arm, out) = (buf, buf + 12);
        let m = &mut c.m;
        let mode = cam.mode(m);
        let a = mode_arm(m, mode);
        Coord(arm).set(m, Vec3::from(a));
        let anchor = cam.anchor(m).0;
        turn_arm(c, arm, anchor + OBJ_ORIENT, out);
        tunnel_limit(c, p, out + 4);
        let m = &mut c.m;
        let off = coord(m, out);
        place_off_anchor(m, cam, cam.position(), off);
        let anchor = cam.anchor(m).0;
        look_back(m, Matrix(anchor + OBJ_ORIENT), cam.rotation());
    });
}

pub fn abi_update_simple_cam(c: &mut Cpu) {
    update_simple_cam(c, c.r[4]);
}

/// `Camera_UpdateBumperCam` (CAMERA.CPP, 0x800815d0): player `p`'s camera on its mode's arm
/// turned with the car, facing as the car; looking behind (unless held off, bit 5) the sound
/// heard from the arm and the camera on the arm flipped behind, looking back.
pub fn update_bumper_cam(c: &mut Cpu, p: u32) {
    use crate::globals::LOOK_BEHIND;
    let cam = info(p);
    with_buffer(c, 24, |c, buf| {
        let (arm, out) = (buf, buf + 12);
        let m = &mut c.m;
        let mode = cam.mode(m);
        let a = mode_arm(m, mode);
        Coord(arm).set(m, Vec3::from(a));
        let held_off = (m.r32(cam.0 + CAMERA_BITS) >> 5) & 1 != 0;
        let behind = !held_off && m.r32(LOOK_BEHIND.wrapping_add(p << 2)) != 0;
        if behind {
            let anchor = cam.anchor(m).0;
            turn_arm(c, arm, anchor + OBJ_ORIENT, out);
            let m = &mut c.m;
            let off = coord(m, out);
            place_off_anchor(m, cam, cam.audio_pos(), off);
            let z = m.r32(arm + 8).wrapping_neg();
            m.w32(arm + 8, z);
        }
        let anchor = cam.anchor(&mut c.m).0;
        turn_arm(c, arm, anchor + OBJ_ORIENT, out);
        let m = &mut c.m;
        let off = coord(m, out);
        place_off_anchor(m, cam, cam.position(), off);
        let anchor = cam.anchor(m).0;
        if behind {
            look_back(m, Matrix(anchor + OBJ_ORIENT), cam.rotation());
        } else {
            for k in 0..9 {
                let v = m.r32(anchor + OBJ_ORIENT + 4 * k);
                m.w32(cam.rotation().0 + 4 * k, v);
            }
        }
    });
}

pub fn abi_update_bumper_cam(c: &mut Cpu) {
    update_bumper_cam(c, c.r[4]);
}

/// A car going the wrong way (`wrongway`), its velocity in its own frame (`linearVel_ch`: x
/// across, z ahead).
const CAR_WRONG_WAY: u32 = 0x3F0;
const CAR_VEL_X: u32 = 0x418;
const CAR_VEL_Z: u32 = 0x420;
/// A slice's height (`TrkNewSlice` +4).
const SLICE_Y: u32 = 4;
/// The tail cameras' eases (16.16): across and along by kind (the closest 10%, the farthest
/// 5%), up and down 5%; the least with the speed (1310: 2%).
const TAIL_EASE: [i32; 3] = [6553, 6553, 3276];
const TAIL_EASE_Y: i32 = 3276;
const TAIL_EASE_MIN: i32 = 1310;
const MATH_VECTOR_LENGTH: u32 = 0x8009_e1b0;

/// Slice `s` stepped by `step` round the track.
fn wrap_slice(m: &mut Mem, s: i32, step: i32, ahead_first: bool) -> i32 {
    let n = crate::globals::NUM_SLICES.get(m);
    let s = s.wrapping_add(step);
    if ahead_first {
        if s >= n { s - n } else { s }
    } else if s < 0 {
        s + n
    } else {
        s
    }
}

/// A chase camera's numbers: the road's rise taken (divided by), its bounds for the middle and
/// far kinds, whether it pulls back with speed (a 20th of it, at most 2.0), its ease in replays,
/// the closest it comes out of transition.
struct Chase {
    rise_divisor: i32,
    lift: [(i32, i32); 2],
    pulls_back: bool,
    replay_ease: Option<i32>,
    closest: i32,
}

/// The tail camera's and the helicopter's.
const TAIL: Chase = Chase { rise_divisor: 3, lift: [(0x4000, 0x1_0000), (-0x4000, 0x2_0000)], pulls_back: false, replay_ease: None, closest: 0x4_F0A3 };
const HELI: Chase = Chase { rise_divisor: 2, lift: [(0x4000, 0x1_4000), (-0xC000, 0x3_0000)], pulls_back: true, replay_ease: Some(655), closest: 0x6_51EB };

/// A car's speed, roughly: the larger of its speeds across and along and a quarter of the other.
fn rough_speed(m: &mut Mem, car: u32) -> i32 {
    let (across, along) = ((m.r32(car + CAR_VEL_X) as i32).wrapping_abs(), (m.r32(car + CAR_VEL_Z) as i32).wrapping_abs());
    if along < across { across.wrapping_add(along >> 2) } else { along.wrapping_add(across >> 2) }
}

/// [`update_tail_cam`] and [`update_heli_cam`]: player `p`'s chase camera of kind `kind` (0
/// near, 1 middle, 2 far; not while paused unless being changed, nor between races): its mode's
/// arm (pulled back with speed, for the helicopter), raised by part of the road's rise over the
/// next three slices (ahead of the way the car goes; bounded for the middle and far kinds),
/// turned with the car; looking behind, the sound from the arm and the camera flipped behind
/// it; else under a tunnel's roof, eased toward (across and along faster with speed, between 2%
/// and the kind's, the helicopter's 1% in replays; 10% in transition, counting it down), and out
/// of transition kept a least distance from the car.
fn chase_cam(c: &mut Cpu, p: u32, kind: u32, chase: &Chase) {
    use crate::globals::{BWORLDSM_SLICES, IN_BETWEEN, LOOK_BEHIND, REPLAY_INTERFACE, REPLAY_MODE};
    let cam = info(p);
    with_buffer(c, 24, |c, buf| {
        let (arm, out) = (buf, buf + 12);
        let m = &mut c.m;
        let mode = cam.mode(m);
        let a = mode_arm(m, mode);
        Coord(arm).set(m, Vec3::from(a));
        let car = cam.anchor(m).0;
        let reverse = GAME_SETUP.reverse_track(m);
        let wrong_way = m.r32(car + CAR_WRONG_WAY) as i32 > 0;
        let step = if wrong_way { if reverse == 1 { -3 } else { 3 } } else if reverse != 0 { 3 } else { -3 };
        let paused = m.r32(QUICK_PAUSE_SIM) != 0 && REPLAY_INTERFACE.change_camera(m) == 0;
        if paused || IN_BETWEEN.get(m) != 0 {
            return;
        }
        let mut ease = TAIL_EASE.get(kind as usize).copied().unwrap_or(6553);
        let mut ease_y = TAIL_EASE_Y;
        if kind != 0 {
            ease = (rough_speed(m, car) / 900).min(ease).max(TAIL_EASE_MIN);
        }
        if let Some(e) = chase.replay_ease {
            if REPLAY_MODE.get(m) >= 2 {
                ease = e;
            }
        }
        let behind = m.r32(LOOK_BEHIND.wrapping_add(p << 2)) != 0;
        if behind {
            turn_arm(c, arm, car + OBJ_ORIENT, cam.relpos().0);
        }
        let m = &mut c.m;
        if chase.pulls_back {
            let pull = (rough_speed(m, car) / 20).min(0x2_0000);
            let z = (m.r32(arm + 8) as i32).wrapping_sub(pull);
            m.w32(arm + 8, z as u32);
        }
        // the road's rise over three slices the way the car goes
        let slice = m.r16(car + ANCHOR_ROAD_INFO) as i16 as i32;
        let ahead = wrap_slice(m, slice, step, step >= 0);
        let back = wrap_slice(m, ahead, -step, step <= 0);
        let slices = BWORLDSM_SLICES.get(m) as u32;
        let y = |m: &mut Mem, s: i32| m.r32(slices.wrapping_add((s as u32) << 5) + SLICE_Y) as i32;
        let rise = (y(m, ahead).wrapping_sub(y(m, back))) / chase.rise_divisor;
        let lift = match kind {
            0 => 0,
            1 | 2 => {
                let (low, high) = chase.lift[kind as usize - 1];
                rise.min(high).max(low)
            }
            _ => rise,
        };
        let v = (m.r32(arm + 4) as i32).wrapping_add(lift);
        m.w32(arm + 4, v as u32);
        if behind {
            turn_arm(c, arm, car + OBJ_ORIENT, out);
            let m = &mut c.m;
            let off = coord(m, out);
            place_off_anchor(m, cam, cam.audio_pos(), off);
            let z = m.r32(arm + 8).wrapping_neg();
            m.w32(arm + 8, z);
            turn_arm(c, arm, car + OBJ_ORIENT, out);
            tunnel_limit(c, p, out + 4);
            let m = &mut c.m;
            let off = coord(m, out);
            place_off_anchor(m, cam, cam.position(), off);
            return;
        }
        turn_arm(c, arm, car + OBJ_ORIENT, out);
        tunnel_limit(c, p, out + 4);
        let m = &mut c.m;
        let old = coord(m, cam.relpos().0);
        let transition = m.r8(cam.0 + TRANSITION);
        if transition != 0 {
            m.w8(cam.0 + TRANSITION, transition - 1);
            ease_y = 6553;
            ease = 6553;
        }
        for (k, e) in [(0u32, ease), (1, ease_y), (2, ease)] {
            let target = m.r32(out + 4 * k) as i32;
            let eased = old[k as usize].wrapping_add(fixedmult(target.wrapping_sub(old[k as usize]), e));
            m.w32(cam.relpos().0 + 4 * k, eased as u32);
        }
        if m.r8(cam.0 + TRANSITION) == 0 {
            let len = call_addr(c, MATH_VECTOR_LENGTH, &[cam.relpos().0]) as i32;
            if len < chase.closest {
                let scale = crate::fixed::fixeddiv(chase.closest, len);
                let m = &mut c.m;
                for k in 0..3 {
                    let v = fixedmult(scale, m.r32(cam.relpos().0 + 4 * k) as i32);
                    m.w32(cam.relpos().0 + 4 * k, v as u32);
                }
            }
        }
        let m = &mut c.m;
        let rel = coord(m, cam.relpos().0);
        place_off_anchor(m, cam, cam.position(), rel);
    });
}

/// `Camera_UpdateTailCam` (CAMERA.CPP, 0x800817a8): [`chase_cam`] behind the car (a third of
/// the road's rise; at least about 4.9 off).
pub fn update_tail_cam(c: &mut Cpu, p: u32, kind: u32) {
    chase_cam(c, p, kind, &TAIL);
}

pub fn abi_update_tail_cam(c: &mut Cpu) {
    update_tail_cam(c, c.r[4], c.r[5]);
}

/// `Camera_UpdateHeliCam` (CAMERA.CPP, 0x80081df0): [`chase_cam`] from above (half the road's
/// rise; pulled back with speed; slower in replays; at least about 6.3 off).
pub fn update_heli_cam(c: &mut Cpu, p: u32, kind: u32) {
    chase_cam(c, p, kind, &HELI);
}

pub fn abi_update_heli_cam(c: &mut Cpu) {
    update_heli_cam(c, c.r[4], c.r[5]);
}

/// The circling camera's heights (three, a turn each) and its radii across and along.
const CIRCLE_HEIGHTS: u32 = 0x8005_6060;
const CIRCLE_RADIUS_X: i32 = 0x4_8000;
const CIRCLE_RADIUS_Z: i32 = 0x6_0000;
/// Whether the simulation is paused from the menu (`simVar.pauseSim`).
const PAUSE_SIM: u32 = 0x8011_F384;

/// Paused (unless the camera's being changed) or between races: the cameras stand still.
fn cameras_held(m: &mut Mem) -> bool {
    use crate::globals::{IN_BETWEEN, REPLAY_INTERFACE};
    (m.r32(QUICK_PAUSE_SIM) != 0 && REPLAY_INTERFACE.change_camera(m) == 0) || IN_BETWEEN.get(m) != 0
}

/// `Camera_UpdateCircleCam` (CAMERA.CPP, 0x800824dc): player `p`'s camera circling the car (not
/// held, nor paused): a step round each frame (1024 a turn), on an ellipse 4.5 across and 6
/// along, at the turn's height of three (easing to the next over the quarter turn after each
/// half), turned with the car, under a tunnel's roof.
pub fn update_circle_cam(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    if cameras_held(m) || m.r32(PAUSE_SIM) != 0 {
        return;
    }
    let cam = info(p);
    let heights = coord(m, CIRCLE_HEIGHTS);
    let angle = cam.circle_angle(m).wrapping_add(1);
    cam.set_circle_angle(m, angle);
    let (sin, cos) = crate::fixed::intsincos(m, angle as i32);
    let (x, z) = (fixedmult(CIRCLE_RADIUS_X, cos), fixedmult(CIRCLE_RADIUS_Z, sin));
    let angle = cam.circle_angle(m) as i32;
    if (angle + 256) & 0x1FF == 0 {
        cam.update_circle_counter(m, |n| n.wrapping_add(1));
    }
    let lap = cam.circle_counter(m) as i32;
    // (the turns counted up from 0: never below)
    let height = |k: i32| heights[(k % 3).max(0) as usize];
    let phase = angle - 128;
    let y = if phase & 0x1FF < 128 {
        let (from, to) = (height(lap), height(lap + 1));
        (to.wrapping_sub(from).wrapping_mul(phase & 0x7F) >> 7).wrapping_add(from)
    } else {
        height(lap)
    };
    with_buffer(c, 24, |c, buf| {
        let (arm, out) = (buf, buf + 12);
        Coord(arm).set(&mut c.m, Vec3::new(x, y, z));
        let anchor = cam.anchor(&mut c.m).0;
        turn_arm(c, arm, anchor + OBJ_ORIENT, out);
        tunnel_limit(c, p, out + 4);
        let m = &mut c.m;
        let off = coord(m, out);
        place_off_anchor(m, cam, cam.position(), off);
    });
}

pub fn abi_update_circle_cam(c: &mut Cpu) {
    update_circle_cam(c, c.r[4]);
}

/// Where each player's TV camera last stood (`lastX`, `lastY`: a word a player).
const TV_LAST_X: u32 = 0x8013_F090;
const TV_LAST_Y: u32 = 0x8013_F098;
const MATH_DIST_3D: u32 = 0x8009_defc;

/// `Camera_UpdateTVCam` (CAMERA.CPP, 0x8008286c): player `p`'s TV camera by the track: its
/// height above where it was set (re-read when it moves) rising with the distance to the car it
/// films (a quarter of its height per 16 units past 1, at most its height), zoomed by distance.
pub fn update_tv_cam(c: &mut Cpu, p: u32) {
    let cam = info(p);
    let d = target_distance(c, p);
    let m = &mut c.m;
    let (last_x, last_y) = (TV_LAST_X.wrapping_add(p << 2), TV_LAST_Y.wrapping_add(p << 2));
    let x = cam.position().x(m);
    if m.r32(last_x) as i32 != x {
        m.w32(last_x, x as u32);
        let y = cam.position().y(m);
        m.w32(last_y, y as u32);
    }
    let rise = fixedmult(cam.tv_height(m) >> 2, d.wrapping_sub(0x4000));
    let lift = rise.min(cam.tv_height(m)).max(0);
    let y = (m.r32(last_y) as i32).wrapping_add(lift);
    cam.position().set_y(m, y);
    set_camera_zoom(c, p, d);
}

pub fn abi_update_tv_cam(c: &mut Cpu) {
    update_tv_cam(c, c.r[4]);
}

/// The cameras' animations (ANIM): one opened by number (its handle), its data by handle, the
/// place and turn at the time (`GetTimedAnimPosRot`: > 0 while it runs), the finish's restarted,
/// one freed.
const ANIM_OPEN: fn(&mut Cpu) = nfs4rc::f_800745c8;
const ANIM_DATA: fn(&mut Cpu) = nfs4rc::f_800746cc;
const ANIM_POS_ROT: fn(&mut Cpu) = nfs4rc::f_80074ed8;
const ANIM_RESTART: fn(&mut Cpu) = nfs4rc::f_80074760;
/// The tracks' animated cameras (four a track, by number) and which of them are in the world's
/// frame (a byte a track, a bit a number); the finish's animation.
const ANIM_CAMS: u32 = 0x8011_07E0;
const ANIM_CAMS_IN_WORLD: u32 = 0x8011_07D0;
const FINISH_ANIM: u32 = 1;
/// Replays' TV cameras: how many are set, one chosen by the slice (`0x800b67e8`); the spline
/// camera's mode and its fallback's.
const NUM_VALID_CAMS: u32 = 0x8013_E6D4;
const REPLAY_CHOOSE_CAMERA: fn(&mut Cpu) = nfs4rc::f_800b67e8;
const MODE_SPLINE: u32 = 11;
/// A setup car's first camera (`camera[0]`).
const SETUP_CAMERA: u32 = 0xA4;
const ANIM_POS: u32 = 0;
const ANIM_ROT: u32 = 12;
const ANIM_BUFFERS: u32 = 12 + 36 + 12;

/// The anim's place (at `buf`) and turn at the time from handle `handle`; whether it runs on.
fn anim_pos_rot(c: &mut Cpu, handle: i8, buf: u32) -> bool {
    let data = call(c, ANIM_DATA, &[handle as i32 as u32]);
    call(c, ANIM_POS_ROT, &[data, buf + ANIM_POS, buf + ANIM_ROT]) as i32 > 0
}

/// Player `p`'s camera on the animation's place and turn: in the world's frame, or the car's
/// (the place turned with it and added to its place, the turn combined with its turn).
fn anim_place(c: &mut Cpu, p: u32, buf: u32, in_world: bool) {
    let cam = info(p);
    let m = &mut c.m;
    if in_world {
        let pos = Coord(buf + ANIM_POS).get(m);
        cam.position().set(m, pos);
        for k in 0..9 {
            let v = m.r32(buf + ANIM_ROT + 4 * k);
            m.w32(cam.rotation().0 + 4 * k, v);
        }
        return;
    }
    let out = buf + ANIM_ROT + 36;
    let anchor = cam.anchor(m).0;
    turn_arm(c, buf + ANIM_POS, anchor + OBJ_ORIENT, out);
    let m = &mut c.m;
    let off = coord(m, out);
    place_off_anchor(m, cam, cam.position(), off);
    let anchor = cam.anchor(m).0;
    call_addr(c, MATH_FASTTRANSMULT, &[buf + ANIM_ROT, anchor + OBJ_ORIENT, cam.rotation().0]);
}

/// The next of player `p`'s track's animated cameras opened (counting the number down).
fn open_next_anim(c: &mut Cpu, p: u32) {
    let cam = info(p);
    let m = &mut c.m;
    let num = cam.anim_num(m);
    cam.set_anim_num(m, num.wrapping_sub(1));
    let track = GAME_SETUP.track(m) as u32;
    let anim = m.r8(ANIM_CAMS.wrapping_add((num as i32 as u32).wrapping_add(track << 2))) as u32;
    let handle = call(c, ANIM_OPEN, &[anim]);
    cam.set_anim_handle(&mut c.m, handle as i8);
}

/// `Camera_UpdateAnimCam` (CAMERA.CPP, 0x800829b8): player `p`'s animated camera: the track's
/// animations played one after another down the count (each in the world's or the car's frame
/// as the track has it); after the last, in a replay a TV camera by the car's slice (or the
/// spline camera without any), else the player's own camera.
pub fn update_anim_cam(c: &mut Cpu, p: u32) {
    use crate::globals::REPLAY_MODE;
    let cam = info(p);
    if cam.anim_handle(&mut c.m) == -1 {
        open_next_anim(c, p);
    }
    with_buffer(c, ANIM_BUFFERS, |c, buf| {
        let handle = cam.anim_handle(&mut c.m);
        if !anim_pos_rot(c, handle, buf) {
            let m = &mut c.m;
            if cam.anim_num(m) > 0 {
                let handle = cam.anim_handle(m);
                call(c, ANIM_FREE, &[handle as i32 as u32]);
                open_next_anim(c, p);
                let handle = cam.anim_handle(&mut c.m);
                anim_pos_rot(c, handle, buf);
            } else {
                if REPLAY_MODE.get(m) == 2 {
                    if m.r32(NUM_VALID_CAMS) != 0 {
                        let slice = anchor_slice(m, cam) as i16 as i32 as u32;
                        call(c, REPLAY_CHOOSE_CAMERA, &[p, slice]);
                    } else {
                        call_addr(c, SET_MODE, &[p, MODE_SPLINE]);
                        cam.set_spline_mode(&mut c.m, 3);
                    }
                } else {
                    let mode = m.r32(GAME_SETUP.car_info(p).0 + SETUP_CAMERA);
                    call_addr(c, SET_MODE, &[p, mode]);
                }
                return;
            }
        }
        let m = &mut c.m;
        let track = GAME_SETUP.track(m) as u32;
        let in_world = (m.r8(ANIM_CAMS_IN_WORLD.wrapping_add(track)) as u32).wrapping_shr(cam.anim_num(m) as i32 as u32) & 1 != 0;
        anim_place(c, p, buf, in_world);
    });
}

pub fn abi_update_anim_cam(c: &mut Cpu) {
    update_anim_cam(c, c.r[4]);
}

/// `Camera_UpdateFinishCam` (CAMERA.CPP, 0x80082c78): player `p`'s finish camera: the finish's
/// animation (opened once, restarted when done) in the car's frame; the finish's bits (1 to 3
/// off, 4 to 6 on).
pub fn update_finish_cam(c: &mut Cpu, p: u32) {
    let cam = info(p);
    if cam.anim_handle(&mut c.m) == -1 {
        let handle = call(c, ANIM_OPEN, &[FINISH_ANIM]);
        cam.set_anim_handle(&mut c.m, handle as i8);
    }
    with_buffer(c, ANIM_BUFFERS, |c, buf| {
        let handle = cam.anim_handle(&mut c.m);
        if !anim_pos_rot(c, handle, buf) {
            call(c, ANIM_RESTART, &[FINISH_ANIM, 6, buf + ANIM_POS, buf + ANIM_ROT]);
        }
        anim_place(c, p, buf, false);
    });
    let m = &mut c.m;
    let bits = (m.r32(cam.0 + CAMERA_BITS) & !0x2 & !0x4 & !0x8) | 0x70;
    m.w32(cam.0 + CAMERA_BITS, bits);
}

pub fn abi_update_finish_cam(c: &mut Cpu) {
    update_finish_cam(c, c.r[4]);
}

/// `Camera_UpdateBlimpCam` (CAMERA.CPP, 0x80082da0): player `p`'s blimp camera: its mode's arm
/// (under a tunnel's roof) in the world's frame, its offset from the car eased toward it 10% a
/// frame.
pub fn update_blimp_cam(c: &mut Cpu, p: u32) {
    let cam = info(p);
    with_buffer(c, 12, |c, arm| {
        let m = &mut c.m;
        let mode = cam.mode(m);
        let a = mode_arm(m, mode);
        Coord(arm).set(m, Vec3::from(a));
        tunnel_limit(c, p, arm + 4);
        let m = &mut c.m;
        let a = coord(m, arm);
        let mut off = [0i32; 3];
        for k in 0..3 {
            let anchor = cam.anchor(m).0;
            off[k] = (m.r32(cam.position().0 + 4 * k as u32) as i32).wrapping_sub(m.r32(anchor + OBJ_POSITION + 4 * k as u32) as i32);
        }
        for k in 0..3 {
            off[k] = off[k].wrapping_add(fixedmult(a[k].wrapping_sub(off[k]), 6553));
        }
        place_off_anchor(m, cam, cam.position(), off);
    });
}

pub fn abi_update_blimp_cam(c: &mut Cpu) {
    update_blimp_cam(c, c.r[4]);
}

/// Whether human player `k`'s car is a human cop.
fn human_cop(m: &mut Mem, k: u32) -> bool {
    let car = m.r32(HUMAN_RACE_CARS + 4 * k);
    m.r32(car + CAR_FLAGS) & CAR_HUMAN_COP != 0
}

/// The slice under player camera `cam`'s car (a half), the car's place, the spline camera's
/// speed by its mode.
fn anchor_slice(m: &mut Mem, cam: CameraInfo) -> u16 {
    let anchor = cam.anchor(m).0;
    m.r16(anchor + ANCHOR_ROAD_INFO)
}

fn anchor_pos(m: &mut Mem, cam: CameraInfo) -> [i32; 3] {
    let anchor = cam.anchor(m).0;
    coord(m, anchor + OBJ_POSITION)
}

fn spline_speed(m: &mut Mem, cam: CameraInfo) -> i32 {
    let mode = cam.spline_mode(m) as u8 as u32;
    m.r32(CAM_SPEED_TABLE.wrapping_add(mode << 2)) as i32
}

/// The spline camera's speeds by mode (16.16, of a slice a frame), the replays' cameras (16
/// bytes a player: set when one is chosen).
const CAM_SPEED_TABLE: u32 = 0x8011_0A94;
const REPLAY_CAMERA: u32 = 0x8011_8304;
/// The road's frame under a car (`roadMatrix`; its third row the road's way).
const OBJ_ROAD_FORWARD: u32 = OBJ_ROAD_MATRIX + 24;
const FIND_CLOSEST_QUAD_REZ: u32 = 0x8008_04e8;

/// `Camera_SetSplineCam` (CAMERA.CPP, 0x80082ee4): player `p`'s spline camera placed (unless
/// a replay chose a camera): slices ahead of the car (fewer the faster the mode; at most 8), or
/// behind when it faces against the road or the car backs, at the slice's centre on the road.
pub fn set_spline_cam(c: &mut Cpu, p: u32) {
    use crate::globals::{BWORLDSM_SLICES, NUM_SLICES};
    let cam = info(p);
    let m = &mut c.m;
    let speed = spline_speed(m, cam);
    let car = cam.anchor(m).0;
    let mut ahead = ((0x1_0000i32.wrapping_sub(speed)).wrapping_mul(15) >> 16).wrapping_add(1).min(8);
    if m.r32(REPLAY_CAMERA.wrapping_add(p << 4)) != 0 {
        return;
    }
    let facing = (0..3).fold(0i32, |sum, k| {
        let (a, b) = (m.r32(cam.rotation().0 + 24 + 4 * k) as i32, m.r32(car + OBJ_ROAD_FORWARD + 4 * k) as i32);
        sum.wrapping_add(fixedmult(a, b))
    });
    if facing < 0 {
        ahead = ahead.wrapping_neg();
    }
    if (m.r32(car + CAR_VEL_Z) as i32) < 0 {
        ahead = ahead.wrapping_neg();
    }
    let slice = m.r16(car + ANCHOR_ROAD_INFO);
    let n = NUM_SLICES.get(m);
    let to = slice.wrapping_add(ahead as u16);
    let at = if ahead >= 0 {
        if (slice as i16 as i32).wrapping_add(ahead) >= n { to.wrapping_sub(n as u16) } else { to }
    } else if (slice as i16 as i32).wrapping_add(ahead) < 0 {
        (n as u16).wrapping_add(to)
    } else {
        to
    };
    let slice_pos = cam.0 + SLICE_POS;
    m.w16(slice_pos, at);
    let centre = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((at as i16 as i32 as u32) << 5);
    let pos = Coord(centre).get(m);
    cam.position().set(m, pos);
    call_addr(c, FIND_CLOSEST_QUAD_REZ, &[cam.position().0, slice_pos, 1]);
}

pub fn abi_set_spline_cam(c: &mut Cpu) {
    set_spline_cam(c, c.r[4]);
}

/// A Newton object's velocity (`linearVel`, +0xAC); the setup's `sgge` (bit 2: the spline
/// camera at 80%).
const OBJ_VELOCITY: u32 = 0xAC;
const SETUP_SGGE: u32 = 0x38;
const MATH_NORMALIZE_VECTOR: u32 = 0x8009_e4e0;

/// The distance from player `p`'s camera to the car it films, over 16 (its size).
fn target_distance(c: &mut Cpu, p: u32) -> i32 {
    let cam = info(p);
    let dist = |c: &mut Cpu| {
        let target = cam.target(&mut c.m).0;
        call_addr(c, MATH_DIST_3D, &[target + OBJ_POSITION, cam.position().0]) as i32
    };
    if dist(c) > 0 { dist(c) >> 4 } else { dist(c).wrapping_neg() >> 4 }
}

/// `Camera_UpdateSplineCam` (CAMERA.CPP, 0x800830e4): player `p`'s spline camera (not held):
/// re-placed ([`set_spline_cam`]) when the car is further round the track than the mode allows
/// (a slow mode only), else kept on the road; moved along the road's way by the car's speed
/// along it (80% so set) times the mode's speed, a 64th a frame; zoomed by distance.
pub fn update_spline_cam(c: &mut Cpu, p: u32) {
    use crate::globals::{BWORLDSM_SLICES, NUM_SLICES};
    let m = &mut c.m;
    if cameras_held(m) {
        return;
    }
    let cam = info(p);
    let car = cam.anchor(m).0;
    let n = NUM_SLICES.get(m);
    let (car_slice, cam_slice) = (m.r16(car + ANCHOR_ROAD_INFO) as i16 as i32, m.r16(cam.0 + SLICE_POS) as i16 as i32);
    // the slices between them round the track (either way, the shorter)
    let apart = car_slice.wrapping_sub(cam_slice).wrapping_abs();
    let gap = if apart > n / 2 { n.wrapping_sub(apart) } else { apart };
    let speed = spline_speed(m, cam);
    let reach = (0x1_0000i32.wrapping_sub(speed)).wrapping_mul(15) >> 16;
    let replace = reach.wrapping_add(2) < gap && speed <= 0xFFFF;
    if replace && m.r32(REPLAY_CAMERA.wrapping_add(p << 4)) == 0 {
        set_spline_cam(c, p);
    } else {
        call_addr(c, FIND_CLOSEST_QUAD_REZ, &[cam.position().0, cam.0 + SLICE_POS, 1]);
    }
    with_buffer(c, 24, |c, buf| {
        let (way, next) = (buf, buf + 12);
        let m = &mut c.m;
        let slices = BWORLDSM_SLICES.get(m) as u32;
        let s = m.r16(cam.0 + SLICE_POS) as i16 as i32;
        let here = Coord(slices.wrapping_add((s as u32) << 5)).get(m);
        let s_next = m.r16(cam.0 + SLICE_POS) as i16 as i32 + 1;
        let there = Coord(if s_next < NUM_SLICES.get(m) { slices.wrapping_add((s_next as u32) << 5) } else { slices }).get(m);
        Coord(next).set(m, there);
        Coord(way).set(m, Vec3::new(there.x.wrapping_sub(here.x), there.y.wrapping_sub(here.y), there.z.wrapping_sub(here.z)));
        call_addr(c, MATH_NORMALIZE_VECTOR, &[way]);
        let m = &mut c.m;
        let dir = coord(m, way);
        let mut along = (0..3).fold(0i32, |sum, k| sum.wrapping_add(fixedmult(m.r32(car + OBJ_VELOCITY + 4 * k as u32) as i32, dir[k])));
        if m.r32(GAME_SETUP.0 + SETUP_SGGE) & 4 != 0 {
            along = fixedmult(along, 0xCCCC);
        }
        let speed = spline_speed(m, cam);
        let along = fixedmult(along, speed);
        let step = [fixedmult(along, dir[0]), fixedmult(along, dir[1]), fixedmult(along, dir[2])];
        for k in 0..3 {
            let at = cam.position().0 + 4 * k as u32;
            let v = (m.r32(at) as i32).wrapping_add(step[k] >> 6);
            m.w32(at, v as u32);
        }
    });
    let d = target_distance(c, p);
    set_camera_zoom(c, p, d);
}

pub fn abi_update_spline_cam(c: &mut Cpu) {
    update_spline_cam(c, c.r[4]);
}

/// A camera's pull-over hold-off (`POInhibitor`: the race tick until which it films the cop).
const PO_INHIBITOR: u32 = 0x88;
/// A slice's ways (`forward`, `right`: bytes, of 128ths... scaled up) and its paved profile (the
/// lanes' bits).
const SLICE_FORWARD: u32 = 0x0F;
const SLICE_RIGHT: u32 = 0x12;
const SLICE_PAVED_PROFILE: u32 = 0x16;
/// The pull-over camera's height limit and ease (16.16: 6 up, 3%).
const PULLOVER_HEIGHT: i32 = 0x6_0000;
const PULLOVER_EASE: i32 = 1966;

/// A slice's byte vector at `at` scaled up by `shift`.
fn slice_bytes(m: &mut Mem, at: u32, shift: u32) -> [i32; 3] {
    [0, 1, 2].map(|k| (m.r8(at + k) as i8 as i32) << shift)
}

/// `Camera_UpdatePulloverCam` (CAMERA.CPP, 0x800836b8): player `p`'s camera as it's pulled
/// over (not held): zoomed out; filming the cop until the hold-off passes; then beside the
/// road at the car's slice, on the side away from the car (the other with the lanes split),
/// up to 6 high under a tunnel's roof, eased toward 3% a frame; the pull-over's bits.
pub fn update_pullover_cam(c: &mut Cpu, p: u32) {
    use crate::globals::{BWORLDSM_SLICES, SIM_COUNTER};
    if cameras_held(&mut c.m) {
        return;
    }
    call(c, SET_GEOM_SCREEN, &[FINISH_GEOM_SCREEN]);
    let cam = info(p);
    let m = &mut c.m;
    if SIM_COUNTER.get(m) < m.r32(cam.0 + PO_INHIBITOR) as i32 {
        update_cop_cam2(c, p);
        return;
    }
    let car = cam.anchor(m).0;
    let slice = (BWORLDSM_SLICES.get(m) as u32).wrapping_add((m.r16(car + ANCHOR_ROAD_INFO) as i16 as i32 as u32) << 5);
    let centre = coord(m, slice);
    let forward = slice_bytes(m, slice + SLICE_FORWARD, 9);
    let car_pos = coord(m, car + OBJ_POSITION);
    let off = [0, 1, 2].map(|k| car_pos[k].wrapping_sub(centre[k]));
    let mut right = slice_bytes(m, slice + SLICE_RIGHT, 11);
    // which side of the road's way the car is on (the cross product's up)
    let side = fixedmult(off[2], forward[0]).wrapping_sub(fixedmult(off[0], forward[2]));
    let profile = m.r16(slice + SLICE_PAVED_PROFILE);
    let split = island_profile(profile);
    if split != (side < 0) {
        right = [right[0].wrapping_neg(), right[1].wrapping_neg(), right[2].wrapping_neg()];
    }
    let height = with_buffer(c, 4, |c, limit| {
        c.m.w32(limit, PULLOVER_HEIGHT as u32);
        tunnel_limit(c, p, limit);
        c.m.r32(limit) as i32
    });
    let target = [centre[0].wrapping_add(right[0]), height.wrapping_add(centre[1].wrapping_add(right[1])), centre[2].wrapping_add(right[2])];
    let m = &mut c.m;
    for k in 0..3 {
        let at = cam.position().0 + 4 * k as u32;
        let now = m.r32(at) as i32;
        m.w32(at, now.wrapping_add(fixedmult(target[k].wrapping_sub(now), PULLOVER_EASE)) as u32);
    }
    let bits = (m.r32(cam.0 + CAMERA_BITS) & !0x2 & !0x4) | 0x78;
    m.w32(cam.0 + CAMERA_BITS, bits);
}

pub fn abi_update_pullover_cam(c: &mut Cpu) {
    update_pullover_cam(c, c.r[4]);
}

/// Where the police camera looks (`gCop1Target`, 12 bytes a player).
const COP1_TARGET: u32 = 0x8013_F2F8;

/// `Camera_UpdateCopCam1` (CAMERA.CPP, 0x80083a34): player `p`'s camera on the cop: looking at
/// the middle of the car and the cop, from 4 out from the car away from the cop and 4 to the
/// side, 1.5 up.
pub fn update_cop_cam1(c: &mut Cpu, p: u32) {
    let cam = info(p);
    with_buffer(c, 12, |c, way| {
        let m = &mut c.m;
        let (anchor, target) = (cam.anchor(m).0, cam.target(m).0);
        let (a, t) = (coord(m, anchor + OBJ_POSITION), coord(m, target + OBJ_POSITION));
        let at = COP1_TARGET.wrapping_add(12 * p);
        for k in 0..3 {
            m.w32(at + 4 * k, (a[k as usize].wrapping_add(t[k as usize]) >> 1) as u32);
        }
        let (a, t) = (coord(m, anchor + OBJ_POSITION), coord(m, target + OBJ_POSITION));
        Coord(way).set(m, Vec3::new(a[0].wrapping_sub(t[0]), a[1].wrapping_sub(t[1]), a[2].wrapping_sub(t[2])));
        call_addr(c, MATH_NORMALIZE_VECTOR, &[way]);
        let m = &mut c.m;
        let d = coord(m, way).map(|v| fixedmult(0x4_0000, v));
        let a = anchor_pos(m, cam);
        cam.position().set(m, Vec3::new(a[0].wrapping_add(d[0]).wrapping_add(d[2]), a[1].wrapping_add(0x1_8000), a[2].wrapping_sub(d[0]).wrapping_add(d[2])));
    });
}

pub fn abi_update_cop_cam1(c: &mut Cpu) {
    update_cop_cam1(c, c.r[4]);
}

/// `Camera_UpdateCopCam2` (CAMERA.CPP, 0x80083bcc): player `p`'s camera 2 behind the car from
/// the cop, 1.2 up.
pub fn update_cop_cam2(c: &mut Cpu, p: u32) {
    let cam = info(p);
    with_buffer(c, 12, |c, way| {
        let m = &mut c.m;
        let (anchor, target) = (cam.anchor(m).0, cam.target(m).0);
        let (a, t) = (coord(m, anchor + OBJ_POSITION), coord(m, target + OBJ_POSITION));
        let d = [0, 1, 2].map(|k| t[k].wrapping_sub(a[k]) / 4);
        Coord(way).set(m, Vec3::from(d));
        call_addr(c, MATH_NORMALIZE_VECTOR, &[way]);
        let m = &mut c.m;
        let d = coord(m, way);
        let a = anchor_pos(m, cam);
        cam.position().set(m, Vec3::new(a[0].wrapping_sub(d[0] << 1), a[1].wrapping_add(0x1_3333), a[2].wrapping_sub(d[2] << 1)));
    });
}

pub fn abi_update_cop_cam2(c: &mut Cpu) {
    update_cop_cam2(c, c.r[4]);
}

/// The cars (`Cars_gList`); a camera's forced focus (1 circling an AI car, 2 behind the
/// player's car looking at one) and that car's index.
const FORCE_CIRCLE: i32 = 1;
const FORCE_BEHIND: i32 = 2;

/// `Camera_UpdateBTCopCam` (CAMERA.CPP, 0x80083cd4): player `p`'s camera as the cop before the
/// controls: the forced focus's (circling the AI car, or behind the player looking at it);
/// zoomed out, the pull-over held off 320 ticks (2.5 s), its bits.
pub fn update_bt_cop_cam(c: &mut Cpu, p: u32) {
    use crate::globals::{CARS_LIST, SIM_COUNTER};
    let cam = info(p);
    let m = &mut c.m;
    let focus = |m: &mut Mem| {
        let k = cam.focus_on_ai_car(m) as i32 as u32;
        m.r32(CARS_LIST.wrapping_add(k << 2))
    };
    match cam.force_focus(m) {
        FORCE_CIRCLE => {
            let car = focus(m);
            cam.set_anchor(m, crate::types::NewtonObj(car));
            let car = focus(m);
            cam.set_target(m, crate::types::NewtonObj(car));
            update_circle_cam(c, p);
        }
        FORCE_BEHIND => {
            let own = m.r32(CARS_LIST.wrapping_add(p << 2));
            cam.set_anchor(m, crate::types::NewtonObj(own));
            let car = focus(m);
            cam.set_target(m, crate::types::NewtonObj(car));
            update_cop_cam2(c, p);
        }
        _ => {}
    }
    let m = &mut c.m;
    m.w32(GEOM_SCREEN, FINISH_GEOM_SCREEN);
    let until = SIM_COUNTER.get(m).wrapping_add(320);
    m.w32(cam.0 + PO_INHIBITOR, until as u32);
    let bits = ((m.r32(cam.0 + CAMERA_BITS) & !0x2 & !0x4) | 0x38) & !0x40;
    m.w32(cam.0 + CAMERA_BITS, bits);
}

pub fn abi_update_bt_cop_cam(c: &mut Cpu) {
    update_bt_cop_cam(c, c.r[4]);
}

/// A Newton object's up against the ground (`orientationToGround`'s y: under a half, it's
/// turning over).
const OBJ_UP_TO_GROUND: u32 = 0x128 + 4;
/// A mode's flags (`Camera_gFlags` +12: bits 0 to 5 the camera's bits 1 to 6).
const MODE_FLAGS: u32 = 12;
/// The camera's bits: 0 back from a special camera, 6 the crash camera allowed, 7 split screen
/// (the first camera's), 24 the arm flipped, 27 the bumper's (modes 0 and 1).
const BIT_FROM_SPECIAL: u32 = 1 << 0;
const BIT_CRASH_CAM: u32 = 1 << 6;
const BIT_FLIPPED: u32 = 1 << 24;
const BIT_BUMPER: u32 = 1 << 27;
/// A replay's camera slot's mode (+8: 19 a TV camera to choose).
const REPLAY_CAMERA_MODE: u32 = 8;
const MODE_REPLAY_TV: u32 = 19;
/// How long a crash camera lasts (frames).
const TUMBLE_FRAMES: u8 = 100;

/// `Camera_Update` (CAMERA.CPP, 0x80083dfc): each player's camera (both with split screen) for
/// the frame. A special camera when one applies: the crash camera (when allowed, as the car
/// turns over: 100 frames, the arm behind it as the camera faced), the finish's (out of the
/// race), the cop's before the controls, the pull-over's; else, back from one, the mode's bits
/// again and the camera on the player's car (in a replay a TV camera chosen when its slot asks);
/// then the mode's camera.
pub fn update(c: &mut Cpu) {
    use crate::globals::{REPLAY_MODE};
    let mut p = 0;
    while p <= (c.m.r8(info(0).0 + CAMERA_BITS) >> 7) as u32 {
        let cam = info(p);
        let bits_at = cam.0 + CAMERA_BITS;
        let m = &mut c.m;
        let anchor = cam.anchor(m).0;
        let mut special = None::<fn(&mut Cpu, u32)>;
        if m.r32(bits_at) & BIT_CRASH_CAM != 0 {
            let tumbling = m.r8(cam.0 + TUMBLING);
            if tumbling != 0 {
                m.w8(cam.0 + TUMBLING, tumbling - 1);
                special = Some(update_collision_cam);
            } else if (m.r32(anchor + OBJ_UP_TO_GROUND) as i32) <= 0x7FFF {
                let facing = (0..3).fold(0i32, |sum, k| {
                    let (a, b) = (m.r32(cam.rotation().0 + 24 + 4 * k) as i32, m.r32(anchor + OBJ_ROAD_FORWARD + 4 * k) as i32);
                    sum.wrapping_add(fixedmult(a, b))
                });
                let bits = (m.r32(bits_at) & !BIT_FLIPPED) | (((facing < 0) as u32) << 24);
                m.w32(bits_at, bits);
                m.w8(cam.0 + TUMBLING, TUMBLE_FRAMES);
            }
        }
        if special.is_none() {
            let human = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
            if m.r32(anchor + CAR_FLAGS) & CAR_RACING != 0 && m.r32(anchor + CAR_FINISH_TYPE) == 2 {
                special = Some(update_finish_cam);
            } else if cam.force_focus(m) > 0 {
                special = Some(update_bt_cop_cam);
            } else if m.r32(human + CAR_PULL_OVER) != 0 && m.r32(human + CAR_FINISH_TYPE) != 3 {
                special = Some(update_pullover_cam);
            }
        }
        if let Some(f) = special {
            f(c, p);
            let m = &mut c.m;
            let bits = (m.r32(bits_at) & !BIT_BUMPER) | BIT_FROM_SPECIAL;
            m.w32(bits_at, bits);
            p += 1;
            continue;
        }
        if m.r8(bits_at) & 1 != 0 {
            let mode = cam.mode(m);
            let flags = m.r32(CAMERA_FLAGS.wrapping_add((mode as i32 as u32) << 4) + MODE_FLAGS);
            let bits = (m.r32(bits_at) & !0x7E) | ((flags << 1) & 0x7E);
            let bits = ((bits & !BIT_BUMPER) | (((mode < 2) as u32) << 27)) & !BIT_FROM_SPECIAL;
            m.w32(bits_at, bits);
            let human = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
            cam.set_anchor(m, crate::types::NewtonObj(human));
            let human = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
            cam.set_target(m, crate::types::NewtonObj(human));
            if REPLAY_MODE.get(m) >= 2 && m.r32(REPLAY_CAMERA.wrapping_add(p << 4) + REPLAY_CAMERA_MODE) == MODE_REPLAY_TV {
                let slice = anchor_slice(m, cam) as i16 as i32 as u32;
                call(c, REPLAY_CHOOSE_CAMERA, &[p, slice]);
            }
        }
        let mode = cam.mode(&mut c.m);
        match mode {
            0 | 1 => update_bumper_cam(c, p),
            2..=4 => update_tail_cam(c, p, (mode - 2) as u32),
            5..=7 => update_heli_cam(c, p, (mode - 5) as u32),
            8 | 9 | 15 => update_simple_cam(c, p),
            10 => update_circle_cam(c, p),
            11 => update_spline_cam(c, p),
            12 => update_tv_cam(c, p),
            13 => update_blimp_cam(c, p),
            14 => update_anim_cam(c, p),
            16 => update_cop_cam1(c, p),
            17 | 18 => update_cop_cam2(c, p),
            _ => {}
        }
        p += 1;
    }
}

pub fn abi_update(c: &mut Cpu) {
    update(c);
}

/// The setup's human player (`GameSetup_gData` +0x1C) and race kinds with a cop's intro (1 and
/// 5); a car's flags' human cop.
const SETUP_PLAYER: u32 = 0x1C;
const CAR_HUMAN_COP: u32 = 0x200;
/// The drivers' cameras by car (`gDriverCam`, 12 bytes a car: the in-car arm), for the first
/// 28; the setup's `sgge` bit 8 (the driver's camera to start).
const DRIVER_CAM: u32 = 0x8011_0944;
const DRIVER_CAM_CARS: i32 = 28;
const SGGE_DRIVER_CAM: u32 = 0x100;
/// A camera's starting mode (the animated intro) and pull-over hold-off.
const MODE_ANIM: i16 = 14;
const START_PO_INHIBITOR: u32 = 64;
/// A car's type (its setup record's first word).
const CAR_INFO_PTR: u32 = 0x288;

/// `Camera_Init` (CAMERA.CPP, 0x8008427c): each player's camera (both with split screen) on its
/// car: at the car's place (the sound's and the walls' too), turned as it, no height, twist or
/// counts; the intro's animated camera (its count of animations by track, one for a cop's
/// intro); the bits (crash camera off, split screen); no animation, focus or road position,
/// the pull-over held 64 ticks; behind their cars. Zoomed out; the driver's camera to start
/// when so set (one player, a car with one): the first player's camera setting and arm.
pub fn init(c: &mut Cpu) {
    use crate::globals::{NUM_HUMAN_RACE_CARS};
    let m = &mut c.m;
    let two = GAME_SETUP.comm_mode(m) == 1;
    for p in 0..=two as u32 {
        let cam = info(p);
        let m = &mut c.m;
        let player = if two { p } else { m.r32(GAME_SETUP.0 + SETUP_PLAYER) };
        let car = m.r32(HUMAN_RACE_CARS.wrapping_add(player << 2));
        cam.set_anchor(m, crate::types::NewtonObj(car));
        let car = m.r32(HUMAN_RACE_CARS.wrapping_add(player << 2));
        cam.set_target(m, crate::types::NewtonObj(car));
        let at_car = |m: &mut Mem| Coord(cam.anchor(m).0 + OBJ_POSITION).get(m);
        let pos = at_car(m);
        cam.position().set(m, pos);
        let pos = at_car(m);
        cam.audio_pos().set(m, pos);
        cam.set_tv_height(m, 0);
        let anchor = cam.anchor(m).0;
        for k in 0..9 {
            let v = m.r32(anchor + OBJ_ORIENT + 4 * k);
            m.w32(cam.rotation().0 + 4 * k, v);
        }
        cam.set_twist(m, 0);
        let pos = at_car(m);
        cam.wall_left().set(m, pos);
        let pos = at_car(m);
        cam.wall_right().set(m, pos);
        cam.set_cam_num(m, 0);
        cam.set_circle_counter(m, 0);
        cam.set_circle_angle(m, 0);
        let bits_at = cam.0 + CAMERA_BITS;
        let bits = (m.r32(bits_at) & !0xDF | 0x20) | ((two as u32) << 7);
        m.w32(bits_at, bits);
        m.w8(cam.0 + TRANSITION, 0);
        m.w8(cam.0 + TUMBLING, 0);
        let bits = m.r32(bits_at);
        cam.set_mode(m, MODE_ANIM);
        m.w32(bits_at, bits & !0x0F00_0000);
        let track = GAME_SETUP.track(m) as u32;
        let count = m.r8(ANIM_CAMS.wrapping_add(track << 2));
        cam.set_anim_num(m, count as i8);
        let race = GAME_SETUP.race_type(m);
        if race == 1 || race == 5 {
            let cop = |m: &mut Mem, k: u32| human_cop(m, k);
            if cop(m, 0) || (NUM_HUMAN_RACE_CARS.get(m) == 2 && cop(m, 1)) {
                cam.set_anim_num(m, 1);
            }
        }
        cam.set_anim_handle(m, -1);
        cam.set_spline_mode(m, 0);
        cam.set_force_focus(m, 0);
        cam.set_focus_on_ai_car(m, -1);
        m.w32(cam.0 + PO_INHIBITOR, START_PO_INHIBITOR);
        for k in 0..0x84 / 4 {
            m.w32(cam.0 + SLICE_POS + 4 * k, 0);
        }
    }
    reset_rel_pos(&mut c.m, 3);
    let m = &mut c.m;
    let car = m.r32(HUMAN_RACE_CARS);
    let info_ptr = m.r32(car + CAR_INFO_PTR);
    let car_type = m.r32(info_ptr) as i32;
    m.w32(GEOM_SCREEN, FINISH_GEOM_SCREEN);
    if car_type < DRIVER_CAM_CARS && m.r32(GAME_SETUP.0 + SETUP_SGGE) & SGGE_DRIVER_CAM != 0 && !two {
        m.w32(GAME_SETUP.car_info(0).0 + SETUP_CAMERA, 1);
        let arm = coord(m, DRIVER_CAM.wrapping_add((car_type as u32).wrapping_mul(12)));
        Coord(CAMERA_FLAGS + 16).set(m, Vec3::from(arm));
    }
}

pub fn abi_init(c: &mut Cpu) {
    init(c);
}

/// A car's body's pitch and roll as drawn (`render.bodyPitch`, `bodyRoll`).
const CAR_BODY_PITCH: u32 = 0x840 + 8;
const CAR_BODY_ROLL: u32 = 0x840 + 12;
/// How long an object's been flying (`flightTime`).
const OBJ_FLIGHT_TIME: u32 = 0x17C;
/// Normals closer than this (16.16 cosine: about 45 degrees) aren't too steep apart.
const STEEP_COS: i32 = 0xB4FC;

/// `Camera_PitchAndRoll` (CAMERA.CPP, 0x80084698): player `p`'s camera turned with its car's
/// body's roll and twice its pitch (the camera's bit 1 in its lowest bit).
pub fn pitch_and_roll(c: &mut Cpu, p: u32) {
    let cam = info(p);
    with_buffer(c, 3 * MATRIX_SIZE, |c, buf| {
        let (roll, pitch, both) = (buf, buf + MATRIX_SIZE, buf + 2 * MATRIX_SIZE);
        let m = &mut c.m;
        let car = cam.anchor(m).0;
        let twice_pitch = (m.r32(car + CAR_BODY_PITCH) << 1) as i32;
        let body_roll = m.r32(car + CAR_BODY_ROLL) as i32;
        crate::fixed::fixedxformz(m, roll, body_roll);
        let low = ((m.r32(cam.0 + CAMERA_BITS) >> 1) & 1) as i32;
        crate::fixed::fixedxformx(m, pitch, twice_pitch | low);
        call_addr(c, MATH_FASTTRANSMULT, &[roll, pitch, both]);
        call_addr(c, MATH_FASTTRANSMULT, &[both, cam.rotation().0, cam.rotation().0]);
    });
}

pub fn abi_pitch_and_roll(c: &mut Cpu) {
    pitch_and_roll(c, c.r[4]);
}

/// The road's normal at `pos` (a `BWorldSm_Pos`).
fn road_normal(c: &mut Cpu, pos: u32) -> [i32; 3] {
    let n = call_addr(c, U_NORMAL, &[pos]);
    coord(&mut c.m, n)
}

fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    fixedmult(a[0], b[0]).wrapping_add(fixedmult(a[1], b[1])).wrapping_add(fixedmult(a[2], b[2]))
}

/// `Camera_TooSteep` (CAMERA.CPP, 0x8008474c): whether the road at `pos` leans more than about
/// 45 degrees from the road under player `p`'s car, the car above it from the camera, and the
/// car on the ground.
pub fn too_steep(c: &mut Cpu, p: u32, pos: u32) -> bool {
    let cam = info(p);
    let car = cam.anchor(&mut c.m).0;
    let here = road_normal(c, pos);
    let under_car = road_normal(c, car + ANCHOR_ROAD_INFO);
    if dot(here, under_car) > STEEP_COS {
        return false;
    }
    let m = &mut c.m;
    let car_pos = anchor_pos(m, cam);
    let cam_pos = coord(m, cam.position().0);
    let to_car = [0, 1, 2].map(|k| car_pos[k].wrapping_sub(cam_pos[k]));
    let anchor = cam.anchor(m).0;
    dot(here, to_car) > 0 && m.r16(anchor + OBJ_FLIGHT_TIME) == 0
}

pub fn abi_too_steep(c: &mut Cpu) {
    c.r[2] = too_steep(c, c.r[4], c.r[5]) as u32;
}

/// `Camera_SetAboveGround` (CAMERA.CPP, 0x80085230): `pos` raised to a unit above the ground
/// under it along the road's normal at player `p`'s camera.
pub fn set_above_ground(c: &mut Cpu, p: u32, pos: u32) {
    let slice_pos = info(p).0 + SLICE_POS;
    let normal = road_normal(c, slice_pos);
    let ground = with_buffer(c, 12, |c, n| {
        Coord(n).set(&mut c.m, Vec3::from(normal));
        call_addr(c, FIND_GROUND_ELEVATION_GENERAL, &[pos, n, slice_pos + 8]) as i32
    });
    let least = ground.wrapping_add(0x1_0000);
    let m = &mut c.m;
    if (m.r32(pos + 4) as i32) < least {
        m.w32(pos + 4, least as u32);
    }
}

pub fn abi_set_above_ground(c: &mut Cpu) {
    set_above_ground(c, c.r[4], c.r[5]);
}

/// The world's up (the cameras' first guess of their up).
const WORLD_UP: u32 = 0x8005_606C;
/// Up taken from the matrix's own second row when nearly along the view (16.16 cosine).
const UP_ALONG_VIEW: i32 = 0xFD70;
/// How far above the car the chase cameras look (16.16): the tail's and the others'.
const AIM_ABOVE_TAIL: i32 = 21299;
const AIM_ABOVE: i32 = 26214;

fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [
        fixedmult(a[1], b[2]).wrapping_sub(fixedmult(a[2], b[1])),
        fixedmult(a[2], b[0]).wrapping_sub(fixedmult(a[0], b[2])),
        fixedmult(a[0], b[1]).wrapping_sub(fixedmult(a[1], b[0])),
    ]
}

/// `Camera_AcquireTarget` (CAMERA.CPP, 0x800852bc): `mat` (rows: right, up, forward) looking from
/// `from` at `aim` (none: player `p`'s camera's target car): forward toward it (a quarter of the
/// way, raised above the car when the camera's bit 1 says, less for the tail cameras),
/// normalized; up the world's (its x the camera's twist in replays), or the matrix's own when
/// nearly along the view; right and up from cross products, normalized.
pub fn acquire_target(c: &mut Cpu, p: u32, aim: u32, from: u32, mat: u32) {
    use crate::globals::REPLAY_MODE;
    let cam = info(p);
    let (right_at, up_at, fwd_at) = (mat, mat + 12, mat + 24);
    let m = &mut c.m;
    let mut up = coord(m, WORLD_UP);
    let target = if aim != 0 {
        coord(m, aim)
    } else {
        let target = cam.target(m).0;
        coord(m, target + OBJ_POSITION)
    };
    for k in 0..3 {
        let v = target[k as usize].wrapping_sub(m.r32(from + 4 * k) as i32) / 4;
        m.w32(fwd_at + 4 * k, v as u32);
    }
    if (m.r32(cam.0 + CAMERA_BITS) >> 1) & 1 != 0 {
        let tail = (cam.mode(m) as u16).wrapping_sub(2) < 3;
        let y = (m.r32(fwd_at + 4) as i32).wrapping_add(if tail { AIM_ABOVE_TAIL } else { AIM_ABOVE });
        m.w32(fwd_at + 4, y as u32);
    }
    if REPLAY_MODE.get(m) == 2 {
        up[0] = cam.twist(m);
    }
    call_addr(c, MATH_NORMALIZE_VECTOR, &[fwd_at]);
    let m = &mut c.m;
    let fwd = coord(m, fwd_at);
    if dot(up, fwd).wrapping_abs() > UP_ALONG_VIEW {
        up = coord(m, up_at);
    }
    let right = cross(up, fwd);
    for k in 0..3 {
        m.w32(right_at + 4 * k as u32, right[k] as u32);
    }
    call_addr(c, MATH_NORMALIZE_VECTOR, &[right_at]);
    let m = &mut c.m;
    let (fwd, right) = (coord(m, fwd_at), coord(m, right_at));
    let up = cross(fwd, right);
    for k in 0..3 {
        m.w32(up_at + 4 * k as u32, up[k] as u32);
    }
    call_addr(c, MATH_NORMALIZE_VECTOR, &[up_at]);
}

pub fn abi_acquire_target(c: &mut Cpu) {
    acquire_target(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The way each player's look-behind last looked (`lastOppVector`, 12 bytes a player).
const LAST_OPP_VECTOR: u32 = 0x8013_F2D8;
/// A Newton object's way (`orientMat`'s third row).
const OBJ_FORWARD: u32 = OBJ_ORIENT + 24;
const INTARCCOS: fn(&mut Cpu) = nfs4rc::f_800eb6f4;
/// The look-behind's lowering by mode (2 to 7: the tail's, the helicopter's, the far
/// helicopter's more).
fn look_behind_drop(mode: i16) -> i32 {
    match mode {
        2..=4 => -0x1_4CCC,
        5 | 6 => -0x1_9999,
        7 => -26214 - 0x1_9999,
        _ => 0,
    }
}

/// `Camera_OpponentLookBehind` (CAMERA.CPP, 0x80085658): player `p`'s camera looking behind
/// from `from` (`reset`: the look's way set to the car's own and nothing else): the nearest car
/// within 10 slices (round the track) within 128 (of 1024) of straight behind, its way eased
/// into the last look's 30%; the camera aimed (as [`acquire_target`] takes it) by the way from
/// the look, lowered by mode.
pub fn opponent_look_behind(c: &mut Cpu, p: u32, from: u32, reset: bool) {
    use crate::globals::{CARS_LIST, NUM_CARS, NUM_SLICES};
    let m = &mut c.m;
    let human = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
    let last = LAST_OPP_VECTOR.wrapping_add(12 * p);
    if reset {
        let way = coord(m, human + OBJ_FORWARD);
        Coord(last).set(m, Vec3::from(way));
        return;
    }
    let mut best = coord(m, human + OBJ_FORWARD);
    let mut best_gap = 11;
    let mut k = 0;
    while (k as i32) < NUM_CARS.get(&mut c.m) {
        let m = &mut c.m;
        let car = m.r32(CARS_LIST.wrapping_add(k << 2));
        let human = m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
        k += 1;
        if car == human {
            continue;
        }
        let apart = (m.r16(car + ANCHOR_ROAD_INFO) as i16 as i32).wrapping_sub(m.r16(human + ANCHOR_ROAD_INFO) as i16 as i32).wrapping_abs();
        let n = NUM_SLICES.get(m);
        let gap = if apart > n / 2 { n.wrapping_sub(apart) } else { apart };
        if gap >= 11 {
            continue;
        }
        let way = with_buffer(c, 12, |c, v| {
            let m = &mut c.m;
            let (f, cp) = (coord(m, from), coord(m, car + OBJ_POSITION));
            Coord(v).set(m, Vec3::new(f[0].wrapping_sub(cp[0]), f[1].wrapping_sub(cp[1]), f[2].wrapping_sub(cp[2])));
            call_addr(c, MATH_NORMALIZE_VECTOR, &[v]);
            coord(&mut c.m, v)
        });
        let human = c.m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2));
        let facing = dot(coord(&mut c.m, human + OBJ_FORWARD), way);
        let angle = call(c, INTARCCOS, &[facing as u32]) as i32;
        let angle = if angle > 0 { call(c, INTARCCOS, &[facing as u32]) as i32 } else { (call(c, INTARCCOS, &[facing as u32]) as i32).wrapping_neg() };
        if angle < 128 && gap < best_gap {
            best = way;
            best_gap = gap;
        }
    }
    let m = &mut c.m;
    for k in 0..3u32 {
        let old = m.r32(last + 4 * k) as i32;
        m.w32(last + 4 * k, old.wrapping_add(fixedmult(best[k as usize].wrapping_sub(old), 19660)) as u32);
    }
    let (f, l) = (coord(m, from), coord(m, last));
    let mut way = [0, 1, 2].map(|k| f[k].wrapping_sub(l[k]));
    way[1] = way[1].wrapping_add(look_behind_drop(info(p).mode(m)));
    with_buffer(c, 12, |c, aim| {
        Coord(aim).set(&mut c.m, Vec3::from(way));
        acquire_target(c, p, aim, from, info(p).rotation().0);
    });
}

pub fn abi_opponent_look_behind(c: &mut Cpu) {
    opponent_look_behind(c, c.r[4], c.r[5], c.r[6] != 0);
}

/// Whether the camera's road position is off the road's edge (`slicePos.offEdge`).
const SLICE_OFF_EDGE: u32 = SLICE_POS + 0x5A;
/// A view's maths (`DRender_tCalcView`): its place, its turn, the translation, the turn
/// transposed (and a copy, unsquashed).
const VIEW_POS: u32 = 0;
const VIEW_MATRIX: u32 = 12;
const VIEW_TRANSLATION: u32 = 48;
const VIEW_TRANSPOSED: u32 = 60;
const VIEW_TRANSPOSED_COPY: u32 = 96;
/// How much the views squash up and down (16.16: 0.86).
const VIEW_SQUASH: i32 = 0xDC00;
/// The police camera's mode.
const MODE_COP1: i16 = 16;

/// `Camera_GetViewInfo` (CAMERA.CPP, 0x80085a2c): `view`'s maths from player `p`'s camera: the
/// camera turned with the car's body (bit 2); its place, kept off the walls (bit 4), on the
/// road, above the ground (on the road, past the bumper modes); aimed at its car (bit 3) or the
/// police camera's target; looking behind unless held off (bit 5); its turn (mirrored across
/// with a mirrored track) transposed, the up axis squashed a little, transposed back; the
/// translation from its place.
pub fn get_view_info(c: &mut Cpu, p: u32, view: u32) {
    use crate::globals::LOOK_BEHIND;
    let cam = info(p);
    let bit = |c: &mut Cpu, k: u32| (c.m.r32(cam.0 + CAMERA_BITS) >> k) & 1 != 0;
    if bit(c, 2) {
        pitch_and_roll(c, p);
    }
    let m = &mut c.m;
    let pos = cam.position().get(m);
    Coord(view + VIEW_POS).set(m, pos);
    if bit(c, 4) {
        check_wall_collisions(c, p, view);
    }
    call_addr(c, FIND_CLOSEST_QUAD_REZ, &[view + VIEW_POS, cam.0 + SLICE_POS, 1]);
    let m = &mut c.m;
    if m.r8(cam.0 + SLICE_OFF_EDGE) == 0 && cam.mode(m) >= 2 {
        set_above_ground(c, p, view + VIEW_POS);
    }
    if bit(c, 3) {
        acquire_target(c, p, 0, view + VIEW_POS, cam.rotation().0);
    }
    if cam.mode(&mut c.m) == MODE_COP1 {
        acquire_target(c, p, COP1_TARGET.wrapping_add(12 * p), view + VIEW_POS, cam.rotation().0);
    }
    if !bit(c, 5) {
        let behind = c.m.r32(LOOK_BEHIND.wrapping_add(p << 2)) != 0;
        opponent_look_behind(c, p, view + VIEW_POS, !behind);
    }
    let m = &mut c.m;
    let mat = view + VIEW_MATRIX;
    for k in 0..9 {
        let v = m.r32(cam.rotation().0 + 4 * k);
        m.w32(mat + 4 * k, v);
    }
    if GAME_SETUP.mirror_track(m) != 0 {
        for k in [0, 2, 1] {
            let v = m.r32(mat + 4 * k).wrapping_neg();
            m.w32(mat + 4 * k, v);
        }
    }
    crate::fixed::transpose(m, mat, view + VIEW_TRANSPOSED);
    for k in 0..9 {
        let v = m.r32(view + VIEW_TRANSPOSED + 4 * k);
        m.w32(view + VIEW_TRANSPOSED_COPY + 4 * k, v);
    }
    let up = [0, 1, 2].map(|k| fixedmult(m.r32(mat + 12 + 4 * k) as i32, VIEW_SQUASH));
    for k in 0..3 {
        m.w32(mat + 12 + 4 * k as u32, up[k] as u32);
    }
    crate::fixed::transpose(m, mat, view + VIEW_TRANSPOSED);
    with_buffer(c, 12, |c, back| {
        let pos = coord(&mut c.m, view + VIEW_POS);
        Coord(back).set(&mut c.m, Vec3::new(pos[0].wrapping_neg(), pos[1].wrapping_neg(), pos[2].wrapping_neg()));
        turn_arm(c, back, view + VIEW_TRANSPOSED, view + VIEW_TRANSLATION);
    });
}

pub fn abi_get_view_info(c: &mut Cpu) {
    get_view_info(c, c.r[4], c.r[5]);
}

/// Modes playing an animation or a TV camera (no velocity to hear: 12 and 14).
fn hears_still(mode: i16) -> bool {
    mode == 12 || mode == 14
}

/// `Camera_GetAudioViewInfo` (CAMERA.CPP, 0x80085d60): `view`'s place and turn for the sound
/// from player `p`'s camera (looking behind unless held off: from the sound's place, aimed at
/// the car (bit 3) or looking back); `*velocity` the car's velocity (none for the TV and
/// animated cameras).
pub fn get_audio_view_info(c: &mut Cpu, p: u32, view: u32, velocity: u32) {
    use crate::globals::LOOK_BEHIND;
    let cam = info(p);
    let m = &mut c.m;
    let bits = m.r32(cam.0 + CAMERA_BITS);
    let behind = m.r32(LOOK_BEHIND.wrapping_add(p << 2)) != 0 && (bits >> 5) & 1 == 0;
    if !behind {
        let pos = cam.position().get(m);
        Coord(view).set(m, pos);
        for k in 0..9 {
            let v = m.r32(cam.rotation().0 + 4 * k);
            m.w32(view + VIEW_MATRIX + 4 * k, v);
        }
    } else {
        let pos = cam.audio_pos().get(m);
        Coord(view).set(m, pos);
        if (bits >> 3) & 1 != 0 {
            acquire_target(c, p, 0, view, view + VIEW_MATRIX);
        } else {
            look_back(m, cam.rotation(), Matrix(view + VIEW_MATRIX));
        }
    }
    let m = &mut c.m;
    let heard = if hears_still(cam.mode(m)) { 0 } else { cam.anchor(m).0 + OBJ_VELOCITY };
    m.w32(velocity, heard);
}

pub fn abi_get_audio_view_info(c: &mut Cpu) {
    get_audio_view_info(c, c.r[4], c.r[5], c.r[6]);
}

/// The split screen's cameras (`gSplitCameras`: three, a word each).
const SPLIT_CAMERAS: u32 = 0x8011_07C4;
const TRS_PROJ_SET_PROJECTION: fn(&mut Cpu) = nfs4rc::f_800e2554;

/// Leaving player `p`'s current mode: the projection reset after the spline and TV cameras,
/// the camera put behind the car after any but the chase cameras, its animation freed.
fn leave_mode(c: &mut Cpu, p: u32) {
    let cam = info(p);
    let old = cam.mode(&mut c.m) as u16;
    if old.wrapping_sub(11) < 2 {
        c.m.w32(GEOM_SCREEN, FINISH_GEOM_SCREEN);
        call(c, TRS_PROJ_SET_PROJECTION, &[0, 0, 320, 240]);
    }
    if (cam.mode(&mut c.m) as u16).wrapping_sub(2) >= 6 {
        reset_rel_pos(&mut c.m, p + 1);
    }
    let handle = cam.anim_handle(&mut c.m);
    if handle >= 0 {
        call(c, ANIM_FREE, &[handle as i32 as u32]);
        cam.set_anim_handle(&mut c.m, -1);
    }
}

/// Entering player `p`'s mode (20 and past: the player's first camera setting): the spline
/// camera placed, 50 frames of transition, the mode's bits.
fn enter_mode(c: &mut Cpu, p: u32) {
    let cam = info(p);
    let m = &mut c.m;
    if cam.mode(m) >= 20 {
        let first = m.r16(GAME_SETUP.car_info(p).0 + SETUP_CAMERA);
        cam.set_mode(m, first as i16);
    }
    if cam.mode(m) == MODE_SPLINE as i16 {
        set_spline_cam(c, p);
    }
    let m = &mut c.m;
    m.w8(cam.0 + TRANSITION, 50);
    let mode = cam.mode(m);
    let flags = m.r32(CAMERA_FLAGS.wrapping_add((mode as i32 as u32) << 4) + MODE_FLAGS);
    let bits = (m.r32(cam.0 + CAMERA_BITS) & !0x7E) | ((flags << 1) & 0x7E);
    m.w32(cam.0 + CAMERA_BITS, bits);
    let bits = (bits & !BIT_BUMPER) | (((cam.mode(m) < 2) as u32) << 27);
    m.w32(cam.0 + CAMERA_BITS, bits);
}

/// `Camera_SetMode` (CAMERA.CPP, 0x8008602c): between races over; player `p`'s camera to mode
/// `mode` (not while back from a special camera, bit 0).
pub fn set_mode(c: &mut Cpu, p: u32, mode: u32) {
    use crate::globals::IN_BETWEEN;
    IN_BETWEEN.set(&mut c.m, 0);
    let cam = info(p);
    if c.m.r8(cam.0 + CAMERA_BITS) & 1 != 0 {
        return;
    }
    leave_mode(c, p);
    cam.set_mode(&mut c.m, mode as i16);
    enter_mode(c, p);
}

pub fn abi_set_mode(c: &mut Cpu) {
    set_mode(c, c.r[4], c.r[5]);
}

/// `Camera_NextMode` (CAMERA.CPP, 0x80086240): player `p`'s camera to its next (not during the
/// intro's animation, nor back from a special camera): the split screen's three in turn, a cop
/// intro's race's four settings, else the player's three settings.
pub fn next_mode(c: &mut Cpu, p: u32) {
    use crate::globals::NUM_HUMAN_RACE_CARS;
    let cam = info(p);
    let m = &mut c.m;
    if cam.mode(m) == MODE_ANIM || m.r8(cam.0 + CAMERA_BITS) & 1 != 0 {
        return;
    }
    leave_mode(c, p);
    let m = &mut c.m;
    let next = cam.cam_num(m).wrapping_add(1);
    cam.set_cam_num(m, next);
    let setting = |m: &mut Mem, k: u32| m.r16((GAME_SETUP.car_info(p).0 + SETUP_CAMERA).wrapping_add(k.wrapping_mul(4)));
    let mode = if m.r8(cam.0 + CAMERA_BITS) >> 7 != 0 {
        m.r16(SPLIT_CAMERAS.wrapping_add(((next as i32 % 3) as u32).wrapping_mul(4)))
    } else {
        let race = GAME_SETUP.race_type(m);
        let cop = |m: &mut Mem, k: u32| human_cop(m, k);
        if (race == 1 || race == 5) && (cop(m, 0) || (NUM_HUMAN_RACE_CARS.get(m) == 2 && cop(m, 1))) {
            setting(m, next as u16 as u32 & 3)
        } else {
            setting(m, (next as i32 % 3) as u32)
        }
    };
    cam.set_mode(m, mode as i16);
    enter_mode(c, p);
}

pub fn abi_next_mode(c: &mut Cpu) {
    next_mode(c, c.r[4]);
}

/// The wall check's feelers (`feeler3`: three arms in the car's frame).
const FEELERS: u32 = 0x8011_0AB0;
/// A road position's quad (`simQuad`, +0x78: its first byte the surface (low 4 bits; 0 none)
/// and a wall flag (0x80)) and its point on the road (+8).
const POS_QUAD: u32 = 0x78;
const POS_POINT: u32 = 8;
const POS_SIZE: u32 = 0x84;
/// A wall too high above the car (16.16: 3.5).
const WALL_HEIGHT: i32 = 0x3_8000;
const FIND_CLOSEST_QUAD_MAX_ITERATIONS: u32 = 0x8008_06b4;

/// Whether `pt` is where player `p`'s camera can't be (found on the road from `pos`, a road
/// position kept between the probes): off the road (no surface, off its edge), too steep from
/// the car's road, or on a wall more than 3.5 above the car.
fn blocked(c: &mut Cpu, p: u32, pt: u32, pos: u32) -> bool {
    call_addr(c, FIND_CLOSEST_QUAD_MAX_ITERATIONS, &[pt, pos, 3]);
    let m = &mut c.m;
    let quad = m.r32(pos + POS_QUAD);
    let ground = if quad != 0 {
        coord(m, pos + POS_POINT)
    } else {
        let slice = m.r16(pos) as i16 as i32 as u32;
        let slices = crate::globals::BWORLDSM_SLICES.get(m) as u32;
        coord(m, slices.wrapping_add(slice << 5))
    };
    if quad != 0 && m.r8(quad) & 0xF == 0 {
        return true;
    }
    if m.r8(pos + SLICE_OFF_EDGE - SLICE_POS) != 0 || too_steep(c, p, pos) {
        return true;
    }
    let m = &mut c.m;
    if quad != 0 && m.r8(quad) & 0x80 != 0 {
        let car_y = anchor_pos(m, info(p))[1];
        return ground[1].wrapping_sub(car_y) > WALL_HEIGHT;
    }
    false
}

/// `v` turned about the up axis by `angle` (1024 a turn).
fn turn_about_y(m: &mut Mem, v: [i32; 3], angle: i32) -> [i32; 3] {
    let (sin, cos) = crate::fixed::intsincos(m, angle);
    [fixedmult(cos, v[0]).wrapping_sub(fixedmult(sin, v[2])), v[1], fixedmult(sin, v[0]).wrapping_add(fixedmult(cos, v[2]))]
}

/// `Camera_CheckWallCollisions` (CAMERA.CPP, 0x800848bc): `view`'s place kept off the walls by
/// player `p`'s camera. Three feelers from the camera turned with the car: when one is blocked,
/// the wall found walking from the car toward it (128 steps); the wall's ends searched round
/// it from half way back to the car (to the left, then to the right: a 256th of a turn a step,
/// half a turn each way; the camera's last ends kept when not found); the camera pushed out
/// along the wall's normal (the way across between its ends) by how far it and the feeler are
/// behind it, plus one.
pub fn check_wall_collisions(c: &mut Cpu, p: u32, view: u32) {
    let cam = info(p);
    with_buffer(c, POS_SIZE + 3 * 12, |c, buf| {
        let (pos, pt, out) = (buf, buf + POS_SIZE, buf + POS_SIZE + 12);
        let m = &mut c.m;
        for k in 0..POS_SIZE / 4 {
            let v = m.r32(cam.0 + SLICE_POS + 4 * k);
            m.w32(pos + 4 * k, v);
        }
        let mut hit = false;
        for k in 0..3 {
            let anchor = cam.anchor(&mut c.m).0;
            turn_arm(c, FEELERS + 12 * k, anchor + OBJ_ORIENT, out);
            let m = &mut c.m;
            let (v, o) = (coord(m, view + VIEW_POS), coord(m, out));
            Coord(pt).set(m, Vec3::new(v[0].wrapping_add(o[0]), v[1].wrapping_add(o[1]), v[2].wrapping_add(o[2])));
            if blocked(c, p, pt, pos) {
                hit = true;
                break;
            }
        }
        if !hit {
            return;
        }
        // walk from the car toward the feeler until blocked
        let m = &mut c.m;
        let (car, feeler) = (anchor_pos(m, cam), coord(m, pt));
        let step = [0, 1, 2].map(|k| car[k].wrapping_sub(feeler[k]) >> 7);
        let wall = with_buffer(c, 12, |c, at| {
            Coord(at).set(&mut c.m, Vec3::from(car));
            for _ in 0..128 {
                let m = &mut c.m;
                let now = coord(m, at);
                Coord(at).set(m, Vec3::new(now[0].wrapping_sub(step[0]), now[1].wrapping_sub(step[1]), now[2].wrapping_sub(step[2])));
                if blocked(c, p, at, pos) {
                    break;
                }
            }
            coord(&mut c.m, at)
        });
        let car = anchor_pos(&mut c.m, cam);
        let back = [0, 1, 2].map(|k| car[k].wrapping_sub(wall[k]) >> 1);
        // the wall's ends: round from the wall half way back to the car, left then right
        for (end, angles) in [(cam.wall_left(), (1..1024).step_by(4).collect::<Vec<i32>>()), (cam.wall_right(), (-1023..=-1).rev().step_by(4).collect())] {
            for angle in angles {
                let turned = turn_about_y(&mut c.m, back, angle);
                let q = [0, 1, 2].map(|k| wall[k].wrapping_add(turned[k]));
                let found = with_buffer(c, 12, |c, at| {
                    Coord(at).set(&mut c.m, Vec3::from(q));
                    blocked(c, p, at, pos)
                });
                if found {
                    end.set(&mut c.m, Vec3::from(q));
                    break;
                }
            }
        }
        let m = &mut c.m;
        let (left, right) = (coord(m, cam.wall_left().0), coord(m, cam.wall_right().0));
        let along = with_buffer(c, 12, |c, w| {
            Coord(w).set(&mut c.m, Vec3::new(right[0].wrapping_sub(left[0]), right[1].wrapping_sub(left[1]).wrapping_add(0x8000), right[2].wrapping_sub(left[2])));
            call_addr(c, MATH_NORMALIZE_VECTOR, &[w]);
            coord(&mut c.m, w)
        });
        let m = &mut c.m;
        let normal = [along[2].wrapping_neg(), along[1], along[0]];
        let feeler = coord(m, pt);
        let behind = dot([0, 1, 2].map(|k| wall[k].wrapping_sub(feeler[k])), normal);
        let push = behind.wrapping_add(dot(coord(m, out), normal)).wrapping_add(0x1_0000);
        if push > 0 {
            for k in 0..3u32 {
                let v = (m.r32(view + 4 * k) as i32).wrapping_add(fixedmult(push, normal[k as usize]));
                m.w32(view + 4 * k, v as u32);
            }
        }
    });
}

pub fn abi_check_wall_collisions(c: &mut Cpu) {
    check_wall_collisions(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM in the race image (compared): a matrix's or a slot's room, a word.
    const FREE: u32 = 0x801E_2A00;

    /// The cameras' bits, modes, projection and place any; the players' cars' state.
    fn shuffle(c: &mut Cpu, r: &mut Rng) {
        let m = &mut c.m;
        for p in 0..2 {
            let cam = info(p);
            m.w32(cam.0 + CAMERA_BITS, r.next() as u32);
            cam.set_mode(m, pick(r, 20) as i16);
            let pos = cam.position().0;
            for k in 0..3 {
                let v = m.r32(pos + 4 * k).wrapping_add(r.next() as u32 & 0x3_FFFF).wrapping_sub(0x2_0000);
                m.w32(pos + 4 * k, v);
            }
        }
        m.w32(GEOM_SCREEN, r.next() as u32 & 0x3FF);
        for k in 0..0x100 {
            m.w32(FREE + 4 * k, r.next() as u32);
        }
    }

    /// A camera's state for its update: paused (and being changed) or not, between races, its
    /// arm flipped, tumbling, in transition, looking behind (held off or not), the car's wrong
    /// way, speed and the track reversed, a replay; the camera's offset; player 0 or 1.
    fn cam_state(c: &mut Cpu, r: &mut Rng) -> Vec<u32> {
        use crate::globals::{IN_BETWEEN, LOOK_BEHIND, REPLAY_INTERFACE, REPLAY_MODE};
        let p = pick(r, 2);
        let m = &mut c.m;
        m.w32(QUICK_PAUSE_SIM, (pick(r, 4) == 0) as u32);
        REPLAY_INTERFACE.set_change_camera(m, pick(r, 2) as i32);
        IN_BETWEEN.set(m, (pick(r, 6) == 0) as i32);
        REPLAY_MODE.set(m, pick(r, 4) as i32);
        GAME_SETUP.set_reverse_track(m, pick(r, 3) as i32);
        let cam = info(p);
        m.w8(cam.0 + ARM_FLIPPED, r.next() as u8);
        m.w8(cam.0 + TUMBLING, (pick(r, 2) == 0) as u8);
        m.w8(cam.0 + TRANSITION, [0, 0, 1, 2, 50][pick(r, 5) as usize]);
        m.w32(LOOK_BEHIND + 4 * p, (pick(r, 3) == 0) as u32);
        for k in 0..3 {
            m.w32(cam.relpos().0 + 4 * k, [0, 0x1_0000, (-0x4_0000i32) as u32, r.next() as u32 & 0x7_FFFF][pick(r, 4) as usize]);
        }
        let car = cam.anchor(m).0;
        m.w32(car + CAR_WRONG_WAY, [0, 1, (-1i32) as u32][pick(r, 3) as usize]);
        for at in [CAR_VEL_X, CAR_VEL_Z] {
            m.w32(car + at, [0, 1_000_000, (-3_000_000i32) as u32, r.next() as u32 & 0xFF_FFFF][pick(r, 4) as usize]);
        }
        vec![p]
    }

    /// [`cam_state`] with the spline camera's mode, a replay's camera chosen or not, the
    /// camera's road position near the car's or far, `sgge`.
    fn spline_state(c: &mut Cpu, r: &mut Rng) -> Vec<u32> {
        let a = cam_state(c, r);
        let m = &mut c.m;
        let cam = info(a[0]);
        cam.set_spline_mode(m, pick(r, 8) as i8);
        m.w32(REPLAY_CAMERA + 16 * a[0], (pick(r, 4) == 0) as u32);
        let slice = anchor_slice(m, cam);
        m.w16(cam.0 + SLICE_POS, slice.wrapping_add([0, 1, 5, 10, 40, (-3i16) as u16][pick(r, 6) as usize]));
        m.w32(GAME_SETUP.0 + SETUP_SGGE, r.next() as u32 & 7);
        a
    }

    /// [`cam_state`] with the camera's target another car.
    fn target_state(c: &mut Cpu, r: &mut Rng) -> Vec<u32> {
        let a = cam_state(c, r);
        let m = &mut c.m;
        let car = m.r32(crate::globals::RACE_CARS + 4 * pick(r, 3));
        if car != 0 {
            info(a[0]).set_target(m, crate::types::NewtonObj(car));
        }
        a
    }

    fn angle(r: &mut Rng) -> u32 {
        [0, 0x4000, 0x8000, (-0x2000i32) as u32, r.next() as u32 & 0xFFFF, r.next() as u32][pick(r, 6) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("EulerToMat", 0x8008_1030, nfs4rc::f_80081030, |_, r| vec![FREE + 0x200, angle(r), angle(r), angle(r)]),
            ("Camera_SetCamera", 0x8008_1128, nfs4rc::f_80081128, |_, r| vec![0x8011_FF44 + 200 * pick(r, 4)]),
            ("Camera_LookBack", 0x8008_1160, nfs4rc::f_80081160, |_, r| vec![FREE, FREE + 0x100 + 4 * pick(r, 2)]),
            ("Camera_TunnelLimit", 0x8008_11e8, nfs4rc::f_800811e8, |c, r| {
                // (in a tunnel mostly: the camera's slice's acoustics or the anchor's; the limit
                // above or below the tunnel's height)
                let p = pick(r, 2);
                let m = &mut c.m;
                GAME_SETUP.set_track(m, pick(r, 20) as i32);
                let cam = info(p);
                let anchor = cam.anchor(m).0;
                for pos in [cam.0 + SLICE_POS, anchor + ANCHOR_ROAD_INFO] {
                    let slice = m.r16(pos) as i16 as i32 as u32;
                    let at = (crate::globals::BWORLDSM_SLICES.get(m) as u32).wrapping_add(slice.wrapping_mul(crate::types::TrkNewSlice::SIZE)) + 0x15;
                    m.w8(at, [0, 0x44, 0x04, 0x40][pick(r, 4) as usize]);
                }
                m.w32(FREE, [i32::MAX as u32, 0, 0x8000, (-0x4000i32) as u32, r.next() as u32][pick(r, 5) as usize]);
                vec![p, FREE]
            }),
            ("Camera_IslandProfile", 0x8008_3660, nfs4rc::f_80083660, |_, r| {
                let bits = [0, 1, 0x8000, 0xF0, 0x0F0F, 0xFFFF, 0x7FFE, 0x1_00F0, r.next() as u32 & 0xFFFF, r.next() as u32][pick(r, 10) as usize];
                vec![bits]
            }),
            ("Camera_Kill", 0x8008_4610, nfs4rc::f_80084610, |c, r| {
                let m = &mut c.m;
                GAME_SETUP.set_comm_mode(m, pick(r, 3) as i32);
                for p in 0..2 {
                    info(p).set_anim_handle(m, -1);
                }
                vec![]
            }),
            ("Camera_GetMode", 0x8008_5f8c, nfs4rc::f_80085f8c, |c, r| {
                let p = pick(r, 2);
                let m = &mut c.m;
                let car = m.r32(HUMAN_RACE_CARS + 4 * p);
                if car != 0 {
                    m.w32(car + CAR_FLAGS, r.next() as u32 & 0x3FF);
                    m.w32(car + CAR_PULL_OVER, pick(r, 2));
                    m.w32(car + CAR_FINISH_TYPE, pick(r, 5));
                }
                vec![p]
            }),
            ("Camera_ReplayUpdate", 0x8008_65f4, nfs4rc::f_800865f4, |c, r| {
                let m = &mut c.m;
                m.w8(FREE, pick(r, 20) as u8);
                for k in 0..3 {
                    m.w16(FREE + 24 + 2 * k, angle(r) as u16);
                }
                vec![pick(r, 2), FREE]
            }),
            ("Camera_ResetRelPos", 0x8008_66f8, nfs4rc::f_800866f8, |_, r| vec![pick(r, 4)]),
            ("Camera_UpdateCollisionCam", 0x8008_1330, nfs4rc::f_80081330, |c, r| cam_state(c, r)),
            ("Camera_UpdateSimpleCam", 0x8008_14ec, nfs4rc::f_800814ec, |c, r| cam_state(c, r)),
            ("Camera_UpdateBumperCam", 0x8008_15d0, nfs4rc::f_800815d0, |c, r| cam_state(c, r)),
            ("Camera_UpdateTailCam", 0x8008_17a8, nfs4rc::f_800817a8, |c, r| {
                let mut a = cam_state(c, r);
                a.push(pick(r, 4));
                a
            }),
            ("Camera_UpdateHeliCam", 0x8008_1df0, nfs4rc::f_80081df0, |c, r| {
                let mut a = cam_state(c, r);
                a.push(pick(r, 4));
                a
            }),
            ("Camera_UpdateCircleCam", 0x8008_24dc, nfs4rc::f_800824dc, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                m.w32(PAUSE_SIM, (pick(r, 5) == 0) as u32);
                let cam = info(a[0]);
                cam.set_circle_angle(m, [0, 127, 255, 383, 511, r.next() as i16][pick(r, 6) as usize]);
                cam.set_circle_counter(m, pick(r, 7) as i16);
                a
            }),
            ("Camera_UpdateTVCam", 0x8008_286c, nfs4rc::f_8008286c, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let cam = info(a[0]);
                cam.set_tv_height(m, [0, 0x1_0000, 0x8_0000, r.next() as u32 as i32 & 0xF_FFFF][pick(r, 4) as usize]);
                let far = [0, 0x40_0000, 0x200_0000][pick(r, 3) as usize];
                let x = cam.position().x(m).wrapping_add(far);
                cam.position().set_x(m, x);
                let x = if pick(r, 2) == 0 { cam.position().x(m) as u32 } else { r.next() as u32 };
                m.w32(TV_LAST_X + 4 * a[0], x);
                a
            }),
            ("Camera_UpdateAnimCam", 0x8008_29b8, nfs4rc::f_800829b8, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let cam = info(a[0]);
                cam.set_anim_handle(m, -1);
                cam.set_anim_num(m, pick(r, 4) as i8);
                m.w32(NUM_VALID_CAMS, pick(r, 2));
                a
            }),
            ("Camera_UpdateFinishCam", 0x8008_2c78, nfs4rc::f_80082c78, |c, r| {
                let a = cam_state(c, r);
                info(a[0]).set_anim_handle(&mut c.m, -1);
                a
            }),
            ("Camera_UpdateBlimpCam", 0x8008_2da0, nfs4rc::f_80082da0, |c, r| cam_state(c, r)),
            ("Camera_SetSplineCam", 0x8008_2ee4, nfs4rc::f_80082ee4, |c, r| spline_state(c, r)),
            ("Camera_UpdateSplineCam", 0x8008_30e4, nfs4rc::f_800830e4, |c, r| spline_state(c, r)),
            ("Camera_UpdatePulloverCam", 0x8008_36b8, nfs4rc::f_800836b8, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let now = crate::globals::SIM_COUNTER.get(m);
                m.w32(info(a[0]).0 + PO_INHIBITOR, now.wrapping_add([-5, 0, 1, 100][pick(r, 4) as usize]) as u32);
                let slice = anchor_slice(m, info(a[0])) as i16 as i32 as u32;
                let at = (crate::globals::BWORLDSM_SLICES.get(m) as u32).wrapping_add(slice << 5);
                m.w16(at + SLICE_PAVED_PROFILE, [0, 0xF, 0x33, 0x81, r.next() as u16][pick(r, 5) as usize]);
                a
            }),
            ("Camera_UpdateCopCam1", 0x8008_3a34, nfs4rc::f_80083a34, |c, r| target_state(c, r)),
            ("Camera_UpdateCopCam2", 0x8008_3bcc, nfs4rc::f_80083bcc, |c, r| target_state(c, r)),
            ("Camera_UpdateBTCopCam", 0x8008_3cd4, nfs4rc::f_80083cd4, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let cam = info(a[0]);
                cam.set_force_focus(m, pick(r, 4) as i32);
                cam.set_focus_on_ai_car(m, pick(r, 4) as i8);
                a
            }),
            ("Camera_Update", 0x8008_3dfc, nfs4rc::f_80083dfc, |c, r| {
                for p in 0..2 {
                    cam_state(c, r);
                    let m = &mut c.m;
                    let cam = info(p);
                    cam.set_anim_handle(m, -1);
                    cam.set_force_focus(m, (pick(r, 6) == 0) as i32);
                    let car = cam.anchor(m).0;
                    m.w32(car + OBJ_UP_TO_GROUND, [0x1_0000, 0x7FFF, 0x8000, 0][pick(r, 4) as usize]);
                    m.w32(car + CAR_FINISH_TYPE, pick(r, 4));
                    m.w32(car + CAR_PULL_OVER, (pick(r, 4) == 0) as u32);
                }
                vec![]
            }),
            ("Camera_Init", 0x8008_427c, nfs4rc::f_8008427c, |c, r| {
                let m = &mut c.m;
                GAME_SETUP.set_comm_mode(m, pick(r, 3) as i32);
                GAME_SETUP.set_race_type(m, pick(r, 7) as i32);
                m.w32(GAME_SETUP.0 + SETUP_SGGE, r.next() as u32 & 0x1FF);
                m.w32(GAME_SETUP.0 + SETUP_PLAYER, pick(r, 2));
                crate::globals::NUM_HUMAN_RACE_CARS.set(m, 1 + pick(r, 2) as i32);
                for p in 0..2 {
                    let car = m.r32(HUMAN_RACE_CARS + 4 * p);
                    let flags = m.r32(car + CAR_FLAGS) & !CAR_HUMAN_COP;
                    m.w32(car + CAR_FLAGS, flags | if pick(r, 3) == 0 { CAR_HUMAN_COP } else { 0 });
                }
                vec![]
            }),
            ("Camera_PitchAndRoll", 0x8008_4698, nfs4rc::f_80084698, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let car = info(a[0]).anchor(m).0;
                m.w32(car + CAR_BODY_PITCH, angle(r));
                m.w32(car + CAR_BODY_ROLL, angle(r));
                a
            }),
            ("Camera_TooSteep", 0x8008_474c, nfs4rc::f_8008474c, |c, r| {
                let mut a = cam_state(c, r);
                let m = &mut c.m;
                let car = info(pick(r, 2)).anchor(m).0;
                m.w16(car + OBJ_FLIGHT_TIME, (pick(r, 3) == 0) as u16);
                a.push([info(a[0]).0 + SLICE_POS, car + ANCHOR_ROAD_INFO][pick(r, 2) as usize]);
                a
            }),
            ("Camera_SetAboveGround", 0x8008_5230, nfs4rc::f_80085230, |c, r| {
                let mut a = cam_state(c, r);
                let pos = info(a[0]).position().get(&mut c.m);
                Coord(FREE).set(&mut c.m, Vec3::new(pos.x, pos.y.wrapping_add([0, -0x2_0000, 0x2_0000, -0x8000][pick(r, 4) as usize]), pos.z));
                a.push(FREE);
                a
            }),
            ("Camera_AcquireTarget", 0x8008_52bc, nfs4rc::f_800852bc, |c, r| {
                let mut a = cam_state(c, r);
                let pos = info(a[0]).position().get(&mut c.m);
                Coord(FREE + 16).set(&mut c.m, pos);
                a.extend([[0, FREE][pick(r, 2) as usize], FREE + 16, FREE + 32]);
                a
            }),
            ("Camera_OpponentLookBehind", 0x8008_5658, nfs4rc::f_80085658, |c, r| {
                let mut a = cam_state(c, r);
                let pos = info(a[0]).position().get(&mut c.m);
                Coord(FREE).set(&mut c.m, pos);
                for k in 0..3 {
                    c.m.w32(LAST_OPP_VECTOR + 12 * a[0] + 4 * k, [0, 0x1_0000, (-0x1_0000i32) as u32, r.next() as u32 & 0x1_FFFF][pick(r, 4) as usize]);
                }
                a.extend([FREE, (pick(r, 4) == 0) as u32]);
                a
            }),
            ("Camera_GetViewInfo", 0x8008_5a2c, nfs4rc::f_80085a2c, |c, r| {
                let mut a = cam_state(c, r);
                let m = &mut c.m;
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                let cam = info(a[0]);
                let off = [0, 0x8_0000, 0x40_0000][pick(r, 3) as usize];
                let x = cam.position().x(m).wrapping_add(off);
                cam.position().set_x(m, x);
                a.extend([FREE, 0]);
                a
            }),
            ("Camera_GetAudioViewInfo", 0x8008_5d60, nfs4rc::f_80085d60, |c, r| {
                let mut a = cam_state(c, r);
                a.extend([FREE, FREE + 0x100]);
                a
            }),
            ("Camera_SetMode", 0x8008_602c, nfs4rc::f_8008602c, |c, r| {
                let mut a = cam_state(c, r);
                info(a[0]).set_anim_handle(&mut c.m, -1);
                a.push(pick(r, 23));
                a
            }),
            ("Camera_NextMode", 0x8008_6240, nfs4rc::f_80086240, |c, r| {
                let a = cam_state(c, r);
                let m = &mut c.m;
                let cam = info(a[0]);
                cam.set_anim_handle(m, -1);
                cam.set_cam_num(m, pick(r, 9) as i16);
                GAME_SETUP.set_race_type(m, pick(r, 7) as i32);
                crate::globals::NUM_HUMAN_RACE_CARS.set(m, 1 + pick(r, 2) as i32);
                for k in 0..2 {
                    let car = m.r32(HUMAN_RACE_CARS + 4 * k);
                    let flags = m.r32(car + CAR_FLAGS) & !CAR_HUMAN_COP;
                    m.w32(car + CAR_FLAGS, flags | if pick(r, 2) == 0 { CAR_HUMAN_COP } else { 0 });
                }
                for k in 0..4 {
                    m.w32(GAME_SETUP.car_info(a[0]).0 + SETUP_CAMERA + 4 * k, pick(r, 22));
                }
                a
            }),
            ("Camera_CheckWallCollisions", 0x8008_48bc, nfs4rc::f_800848bc, |c, r| {
                // the view by the car or off to a side (into the walls and off the road)
                let mut a = cam_state(c, r);
                let m = &mut c.m;
                let car = anchor_pos(m, info(a[0]));
                let side = |r: &mut Rng| [0, 0x2_0000, (-0x6_0000i32), 0x10_0000, (-0x30_0000i32), (r.next() as i32) >> 9][pick(r, 6) as usize];
                Coord(FREE).set(m, Vec3::new(car[0].wrapping_add(side(r)), car[1].wrapping_add(side(r) >> 2), car[2].wrapping_add(side(r))));
                for k in 0..6 {
                    m.w32(info(a[0]).wall_left().0 + 4 * k, car[k as usize % 3].wrapping_add(side(r)) as u32);
                }
                a.push(FREE);
                a
            }),
            ("SetCameraZoom", 0x8008_275c, nfs4rc::f_8008275c, |_, r| {
                vec![pick(r, 2), [0, 12288, 12289, 77_000, 200_000, (-5i32) as u32, r.next() as u32 & 0x7_FFFF][pick(r, 7) as usize]]
            }),
        ]
    }

    #[test]
    fn camera_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xCA0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 300, 0xCA1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
