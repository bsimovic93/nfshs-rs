//! WEATHER.CPP: rain and snow. 152 particles (76 a player with two) live in a box ahead of the
//! camera (`Weather_gSys`: its width, its height from bottom to top, from near to far), in the
//! camera's frame: each frame (but every second tick at most) they are turned and moved by the
//! camera's turn and motion since the last, fall by the wind's velocity (with a twelve-way jitter)
//! and come back in at the far side when they leave the box. Rain is drawn as streaks from each
//! drop's last screen place, snow as small textured squares; in the bumper camera drops splat on
//! the lens. The density (how many particles) and the intensity (how fast they fall: rain below
//! -32, else snow) wander between four levels each, on their own timers.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data and the GTE's
//! registers as the original leaves them, the stack and CPU registers free. What runs while the
//! race loads (the set-up, the restart) ticks as the original's loops do (see `call::ticks`).

use crate::call::{call_jal, ticks, with_buffer};
use crate::fixed::fixedmult;
use crate::globals::{GAME_SETUP, SIM_GLOBAL};
use crate::gpu;
use ps1rt::{Cpu, Mem};

/// The particle system (`Weather_gSys`): the particles each player's view has (+0, +4), the
/// wind's velocity (+8: x, y, z shorts; y the intensity), the box (+0x14 near, +0x16 far, +0x18
/// its depth for new particles, +0x1A bottom, +0x1C top, +0x1E its height for new ones, +0x20
/// its half width).
const SYS: u32 = 0x8012_46F8;
const VELOCITY: u32 = SYS + 8;
const INTENSITY: u32 = SYS + 0xA;
const HDIST: u32 = SYS + 0x10;
const ZNEAR: u32 = SYS + 0x14;
const ZFAR: u32 = SYS + 0x16;
const LENGTH: u32 = SYS + 0x18;
const BOTTOM: u32 = SYS + 0x1A;
const TOP: u32 = SYS + 0x1C;
const HEIGHT: u32 = SYS + 0x1E;
const WIDTH: u32 = SYS + 0x20;
/// The particles a player's view shows at most, and the margin a particle comes back in by.
const MAX_PARTICLES: u32 = 152;
const SPLIT_PARTICLES: u32 = 76;
const REENTER: u16 = 64;

/// The density's and intensity's levels (`Weather_gDensityTbl`, `IntensityTbl`: particles, the
/// fall's velocity), each track's highest intensity (`Weather_gTrackIntensityLimitTbl`), the
/// jitter (`Weather_gRandomVelocityVectors`: 12 of x, y, z bytes).
const DENSITY_TBL: u32 = 0x8012_4674;
const INTENSITY_TBL: u32 = 0x8012_4684;
const LIMIT_TBL: u32 = 0x8012_4694;
const JITTER: u32 = 0x8012_46D4;
const JITTERS: u32 = 12;
const LEVELS: i32 = 4;
const TOP_LEVEL: i32 = 3;
/// How long a level holds before a new goal (ticks), how often the levels move (every 33 ticks).
const HOLD: i32 = 1024;
const STEP_TICKS: i32 = 33;
/// The intensity below which it rains (a faster fall), else it snows.
const RAIN_BELOW: i32 = -32;

/// The state (`Weather_gType`: 1 rain, 0 snow), the levels' goals, steps and timers, the snow
/// tracks (0 and 4) and the track's highest intensity, when the levels last moved
/// (`timechange`).
const TYPE: u32 = 0x8013_EECC;
const RAIN: u32 = 1;
const DENSITY_GOAL: u32 = 0x8013_EED0;
const INTENSITY_GOAL: u32 = 0x8013_EED4;
const DENSITY_STEP: u32 = 0x8013_EED8;
const INTENSITY_STEP: u32 = 0x8013_EEDC;
const DENSITY_TIMER: u32 = 0x8013_EEE0;
const INTENSITY_TIMER: u32 = 0x8013_EEE4;
const SNOW_TRACK: u32 = 0x8013_EEE8;
const TRACK_LIMIT: u32 = 0x8013_EEEC;
const TIME_CHANGE: u32 = 0x8013_F154;

/// The particles' blocks (`Weather_gPos`: an `SVECTOR` each; `Weather_gPrevPos`: their last
/// screen places, a `DVECTOR` each; `Weather_gWasDrawn`: a byte each; `Weather_gSplatInfo`: the
/// lens's drops, 8 bytes each), each player's share of them (`...Server`, the second's from the
/// middle with two players), the splats shown (`gCurrentNumSplats`), the track's weather spec
/// (`Weather_gTrackSpec`: the spec's +0x20, its type first).
const POS: u32 = 0x8013_EE88;
const PREV_POS: u32 = 0x8013_EE90;
const WAS_DRAWN: u32 = 0x8013_EE84;
const SPLAT_INFO: u32 = 0x8013_EE8C;
const POS_SERVER: u32 = 0x8013_EEB0;
const PREV_SERVER: u32 = 0x8013_EEB8;
const DRAWN_SERVER: u32 = 0x8013_EEC0;
const SPLAT_SERVER: u32 = 0x8013_EEA8;
const NUM_SPLATS: u32 = 0x8013_EE80;
const TRACK_SPEC: u32 = 0x8013_EEA4;
const SPEC_WEATHER: u32 = 0x8012_456C + 0x20;
const SPLATS: u32 = 19;
const SPLAT_SIZE: u32 = 8;
const SPLAT_SPLIT: u32 = 9;

/// Each player's last look behind and camera mode (a change clears the streaks), when its
/// particles last moved; the camera's last place and turn (`prevCamPos`, `prevCamMat`), a
/// player's camera (`Camera_gInfo`: its place +8, turn +0x30, bits +0x74 with the bumper camera's
/// bit 27, road position +0x8C), the pad's look behind (`Input_gLookBehind`).
const LAST_LOOK: u32 = 0x8013_EE94;
const LAST_MODE: u32 = 0x8013_EE9C;
const LAST_PROCESS: u32 = 0x8013_F15C;
const PREV_CAM_POS: u32 = 0x8012_471C;
const PREV_CAM_MAT: u32 = 0x8012_4734;
const MATRIX_SIZE: u32 = 36;
const CAMERA_INFO: u32 = 0x8011_0570;
const CAMERA_SIZE: u32 = 0x110;
const CAMERA_PLACE: u32 = 8;
const CAMERA_TURN: u32 = 0x30;
const CAMERA_BITS: u32 = 0x74;
const BUMPER: u32 = 1 << 27;
const CAMERA_POS: u32 = 0x8C;
const LOOK_BEHIND: u32 = 0x8013_E508;
/// A view (`DRender_tView`): its player (+4), place (+8), turn (+0x68).
const VIEW_PLAYER: u32 = 4;
const VIEW_PLACE: u32 = 8;
const VIEW_TURN: u32 = 0x68;
/// The up vector the turn is squared to (`0x800572d0`), or the camera's own up past this.
const UP: u32 = 0x8005_72D0;
const NEAR_VERTICAL: i32 = 0xFD70;

/// The textures (`gWeatherPixmap`: two snowflakes, the lens's drop); the primitives: snow and
/// splats a semi-transparent textured square (`POLY_FT4`, 40 bytes), rain a semi-transparent
/// shaded line (`LINE_G2`, 20 bytes) from black to grey, the draw mode after them (12 bytes).
const PIXMAPS: u32 = 0x8011_3E34;
const FT4_SIZE: u32 = 40;
const FT4_LEN: u8 = 9;
const FT4_SEMI: u32 = 0x2E20_2020;
const FT4_SEMI_CODE: u8 = 0x2E;
const LINE_SIZE: u32 = 20;
const LINE_LEN: u8 = 4;
const LINE_SEMI_BLACK: u32 = 0x5200_0000;
const LINE_GREY: u32 = 0x40_2020;
const DRAW_MODE_SIZE: u32 = 12;
const SPLIT_SCREEN: i32 = 1;

/// The GTE: the rotation (control 0..4), the translation (5..7); the vectors (data 0..5), the
/// screen places (SXY0..2, data 12..14), the results (MAC1..3, data 25..27). MVMVA with the
/// rotation, the translation and vector 0 (results >> 12), RTPS, RTPT.
const RT: usize = 32;
const TR: usize = 32 + 5;
const VXY0: usize = 0;
const SXY0: usize = 12;
const MAC1: usize = 25;
const MVMVA: u32 = 0x48_0012;
const RTPS: u32 = 0x18_0001;
const RTPT: u32 = 0x28_0030;

/// Functions called: the library's `random`, `reservememadr(name, size, 0)`, `purgememadr`,
/// `transpose(from, to)`, `SetDrawMode(prim, dfe, dtd, tpage, rect)`; `Math_fasttransmult(a, b,
/// out)`, `Math_NormalizeVector`, `Camera_GetMode`, `BWorldSm_TunnelFlagSm(pos)`.
const RANDOM: u32 = 0x800E_8394;
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const TRANSPOSE: u32 = 0x800E_4F44;
const SET_DRAW_MODE: u32 = 0x800E_ED18;
const FAST_TRANS_MULT: u32 = 0x8009_E850;
const NORMALIZE: u32 = 0x8009_E4E0;
const CAMERA_GET_MODE: u32 = 0x8008_5F8C;
const TUNNEL_FLAG: u32 = 0x8008_09F8;
/// The blocks' names: "raindrop", "weather1", "weather2", "weather3".
const SPLAT_NAME: u32 = 0x8005_72A0;
const POS_NAME: u32 = 0x8005_72AC;
const PREV_NAME: u32 = 0x8005_72B8;
const DRAWN_NAME: u32 = 0x8005_72C4;

fn random(c: &mut Cpu) -> u32 {
    call_jal(c, RANDOM, &[])
}

/// `a % b` as the R3000's `divu` leaves it (by 0, `a`).
fn remu(a: u32, b: u32) -> u32 {
    a.checked_rem(b).unwrap_or(a)
}

/// A short of the system's, sign-extended as the original's `divu` takes it.
fn sys_short(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn game_ticks(m: &mut Mem) -> i32 {
    SIM_GLOBAL.game_ticks(m)
}

fn short(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// `Weather_GetNumParticles` (WEATHER.CPP, 0x800e29cc): the first view's particles.
pub fn get_num_particles(m: &mut Mem) -> u32 {
    m.r32(SYS)
}

pub fn abi_get_num_particles(c: &mut Cpu) {
    c.r[2] = get_num_particles(&mut c.m);
}

/// The GTE's rotation from the shorts of `r` (row by row), its translation zero.
fn set_rotation(c: &mut Cpu, r: [i16; 9]) {
    for k in 0..4 {
        c.gte_write(RT + k, (r[2 * k] as u16 as u32) | (r[2 * k + 1] as u16 as u32) << 16);
    }
    c.gte_write(RT + 4, r[8] as u16 as u32);
    for k in 0..3 {
        c.gte_write(TR + k, 0);
    }
}

/// `Weather_SetMatrix` (WEATHER.CPP, 0x800e29dc): the GTE's rotation from `matrix` (its
/// transpose, 16.16 to 4.12), no translation.
pub fn set_matrix(c: &mut Cpu, matrix: u32) {
    let m = &mut c.m;
    let r: [i16; 9] = std::array::from_fn(|i| (m.r32(matrix.wrapping_add(4 * (3 * (i % 3) + i / 3) as u32)) as i32 >> 4) as i16);
    set_rotation(c, r);
}

pub fn abi_set_matrix(c: &mut Cpu) {
    set_matrix(c, c.r[4]);
}

/// `Weather_SetIdentMatrix` (WEATHER.CPP, 0x800e2aa0): the GTE's rotation the identity with y
/// turned over (the screen's down), no translation.
pub fn set_ident_matrix(c: &mut Cpu) {
    set_rotation(c, [4096, 0, 0, 0, -4096, 0, 0, 0, 4096]);
}

pub fn abi_set_ident_matrix(c: &mut Cpu) {
    set_ident_matrix(c);
}

/// The box and the wind for snow or rain: (half width, near, far, depth, bottom, top, height,
/// the fall).
fn init_box(m: &mut Mem, b: [i16; 8]) {
    let [width, near, far, length, bottom, top, height, fall] = b;
    m.w32(SYS, 150);
    m.w16(WIDTH, width as u16);
    m.w16(ZNEAR, near as u16);
    m.w16(ZFAR, far as u16);
    m.w16(BOTTOM, bottom as u16);
    m.w16(TOP, top as u16);
    m.w16(INTENSITY, fall as u16);
    m.w16(LENGTH, length as u16);
    m.w16(HEIGHT, height as u16);
    m.w16(VELOCITY, 0);
    m.w16(VELOCITY + 4, 0);
    m.w32(HDIST, 160);
}

/// `Weather_InitSnow` (WEATHER.CPP, 0x800e2b24): 150 flakes in a box 1280 wide, 320 to 1600
/// ahead, from -320 up to 960, falling slowly.
pub fn init_snow(m: &mut Mem) {
    init_box(m, [640, 320, 1600, 1280, -320, 960, 1280, -18]);
}

pub fn abi_init_snow(c: &mut Cpu) {
    init_snow(&mut c.m);
}

/// `Weather_InitRain` (WEATHER.CPP, 0x800e2b84): 150 drops in a box 2176 wide, 384 to 2560
/// ahead, from -320 up to 1088, falling fast.
pub fn init_rain(m: &mut Mem) {
    init_box(m, [1088, 384, 2560, 2176, -320, 1088, 1408, -104]);
}

pub fn abi_init_rain(c: &mut Cpu) {
    init_rain(&mut c.m);
}

/// `Weather_InitSplats` (WEATHER.CPP, 0x800e2be4): the lens's 19 drops at random places on the
/// screen (the top half with two players), each starting within 300 ticks.
pub fn init_splats(c: &mut Cpu) {
    for k in 0..SPLATS {
        let height = if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN { 120 } else { 240 };
        let x = random(c) % 320;
        let at = c.m.r32(SPLAT_INFO).wrapping_add(SPLAT_SIZE * k);
        c.m.w16(at, x as u16);
        let y = remu(random(c), height);
        let at = c.m.r32(SPLAT_INFO).wrapping_add(SPLAT_SIZE * k);
        c.m.w16(at + 2, y as u16);
        let start = random(c) % 300;
        let at = c.m.r32(SPLAT_INFO).wrapping_add(SPLAT_SIZE * k);
        c.m.w32(at + 4, start);
        c.tick();
    }
}

pub fn abi_init_splats(c: &mut Cpu) {
    init_splats(c);
}

/// `Weather_GetNewState` (WEATHER.CPP, 0x800e2cf8): a level's next step: hold (half the time),
/// up or down.
pub fn get_new_state(c: &mut Cpu) -> i32 {
    match random(c) & 3 {
        0 | 1 => 0,
        2 => 1,
        _ => -1,
    }
}

pub fn abi_get_new_state(c: &mut Cpu) {
    c.r[2] = get_new_state(c) as u32;
}

/// A level's new goal: a step from the old one, turned back at the ends (`top` the highest),
/// held for a while when it holds.
fn new_goal(c: &mut Cpu, goal: u32, step: u32, timer: u32, top: i32) {
    let mut s = get_new_state(c);
    let m = &mut c.m;
    let next = (m.r32(goal) as i32).wrapping_add(s);
    if next > top {
        s = -1;
    } else if next < 0 {
        s = 1;
    }
    m.w32(step, s as u32);
    let g = m.r32(goal) as i32;
    m.w32(goal, g.wrapping_add(s) as u32);
    if s == 0 {
        let hold = game_ticks(m).wrapping_add(HOLD);
        m.w32(timer, hold as u32);
    }
}

/// `Weather_ChangeDensityState` (WEATHER.CPP, 0x800e2d38): the density's new goal.
pub fn change_density_state(c: &mut Cpu) {
    new_goal(c, DENSITY_GOAL, DENSITY_STEP, DENSITY_TIMER, LEVELS - 1);
}

pub fn abi_change_density_state(c: &mut Cpu) {
    change_density_state(c);
}

/// `Weather_ChangeIntensityState` (WEATHER.CPP, 0x800e2dac): the intensity's new goal (up to the
/// track's highest).
pub fn change_intensity_state(c: &mut Cpu) {
    let top = c.m.r32(TRACK_LIMIT) as i32;
    new_goal(c, INTENSITY_GOAL, INTENSITY_STEP, INTENSITY_TIMER, top);
}

pub fn abi_change_intensity_state(c: &mut Cpu) {
    change_intensity_state(c);
}

/// Whether a level moving by `step` toward `goal` from `now` needs a new goal: once it passed
/// the goal, or once a hold's time is up.
fn goal_reached(m: &mut Mem, step: i32, now: i32, goal: i32, timer: u32) -> bool {
    if step > 0 {
        now >= goal
    } else if step < 0 {
        goal >= now
    } else {
        (m.r32(timer) as i32) < game_ticks(m)
    }
}

/// `Weather_ChangeDensityBasedOnTime` (WEATHER.CPP, 0x800e2e24): the particles a step toward the
/// density's goal (a new goal when it's reached), never below none.
pub fn change_density_based_on_time(c: &mut Cpu) {
    let m = &mut c.m;
    let step = m.r32(DENSITY_STEP) as i32;
    let level = m.r32(DENSITY_GOAL);
    let goal = m.r32(DENSITY_TBL.wrapping_add(level << 2)) as i32;
    let now = m.r32(SYS) as i32;
    if goal_reached(m, step, now, goal, DENSITY_TIMER) {
        change_density_state(c);
    }
    let m = &mut c.m;
    let n = (m.r32(SYS) as i32).wrapping_add(m.r32(DENSITY_STEP) as i32);
    m.w32(SYS, n as u32);
    if n <= 0 {
        m.w32(SYS, 0);
    }
}

pub fn abi_change_density_based_on_time(c: &mut Cpu) {
    change_density_based_on_time(c);
}

/// `Weather_ChangeIntensityBasedOnTime` (WEATHER.CPP, 0x800e2efc): the fall a step toward the
/// intensity's goal (a new goal when it's reached); rain while it falls faster than 32.
pub fn change_intensity_based_on_time(c: &mut Cpu) {
    let m = &mut c.m;
    let step = m.r32(INTENSITY_STEP) as i32;
    let level = m.r32(INTENSITY_GOAL);
    let goal = m.r32(INTENSITY_TBL.wrapping_add(level << 2)) as i32;
    let now = short(m, INTENSITY);
    if goal_reached(m, step, now, goal, INTENSITY_TIMER) {
        change_intensity_state(c);
    }
    let m = &mut c.m;
    let fall = m.r16(INTENSITY).wrapping_add(m.r16(INTENSITY_STEP));
    m.w16(INTENSITY, fall);
    m.w32(TYPE, ((fall as i16 as i32) < RAIN_BELOW) as u32);
}

pub fn abi_change_intensity_based_on_time(c: &mut Cpu) {
    change_intensity_based_on_time(c);
}

/// `Weather_InitStateControls` (WEATHER.CPP, 0x800e2ff4): the levels at the start: the track's
/// highest intensity, snow tracks (0, 4) at the top intensity, the rest at the lowest; the
/// density at the top; both holding for 1024 ticks.
pub fn init_state_controls(m: &mut Mem) {
    let track = GAME_SETUP.track(m);
    let limit = m.r32(LIMIT_TBL.wrapping_add((track as u32) << 2));
    m.w32(TRACK_LIMIT, limit);
    let snow = track == 0 || track == 4;
    m.w32(SNOW_TRACK, snow as u32);
    m.w32(INTENSITY_GOAL, if snow { TOP_LEVEL as u32 } else { 0 });
    let goal = m.r32(INTENSITY_GOAL);
    let fall = m.r16(INTENSITY_TBL.wrapping_add(goal << 2));
    m.w16(INTENSITY, fall);
    m.w32(TYPE, (goal != TOP_LEVEL as u32) as u32);
    let now = game_ticks(m);
    m.w32(DENSITY_GOAL, TOP_LEVEL as u32);
    let density = m.r32(DENSITY_TBL + 4 * TOP_LEVEL as u32);
    m.w32(INTENSITY_STEP, 0);
    m.w32(DENSITY_STEP, 0);
    m.w32(INTENSITY_TIMER, now.wrapping_add(HOLD) as u32);
    m.w32(SYS, density);
    m.w32(DENSITY_TIMER, now.wrapping_add(HOLD) as u32);
    m.w32(TIME_CHANGE, now as u32);
}

pub fn abi_init_state_controls(c: &mut Cpu) {
    init_state_controls(&mut c.m);
}

/// Every particle's streak forgotten (`Weather_gWasDrawn`), as the original's loop (a tick a
/// pass but the last).
fn forget_drawn(c: &mut Cpu) {
    for k in 0..MAX_PARTICLES {
        let at = c.m.r32(WAS_DRAWN).wrapping_add(k);
        c.m.w8(at, 0);
    }
    ticks(c, MAX_PARTICLES - 1);
}

/// `Weather_Restart` (WEATHER.CPP, 0x800e30c0): in weather, the levels set again (one player
/// only), the particles moved from now, their streaks forgotten.
pub fn restart(c: &mut Cpu) {
    let m = &mut c.m;
    if GAME_SETUP.weather(m) == 0 {
        return;
    }
    if GAME_SETUP.comm_mode(m) != SPLIT_SCREEN {
        init_state_controls(m);
    }
    let now = game_ticks(m) as u32;
    m.w32(LAST_PROCESS + 4, now);
    m.w32(LAST_PROCESS, now);
    forget_drawn(c);
}

pub fn abi_restart(c: &mut Cpu) {
    restart(c);
}

/// A block of `size` reserved (once) at `at`.
fn reserve_once(c: &mut Cpu, at: u32, name: u32, size: u32) {
    if c.m.r32(at) == 0 {
        let block = call_jal(c, RESERVE, &[name, size, 0]);
        c.m.w32(at, block);
    }
}

/// `Weather_Init` (WEATHER.CPP, 0x800e313c): the race's weather (by the track's spec: rain or
/// snow): the blocks (once), each player's share, the camera's place and turn as the last, the
/// particles at random in the box, no streaks, the lens's drops.
pub fn init(c: &mut Cpu) {
    let m = &mut c.m;
    m.w32(TRACK_SPEC, SPEC_WEATHER);
    if GAME_SETUP.weather(m) == 0 {
        return;
    }
    let kind = m.r32(SPEC_WEATHER);
    m.w32(TYPE, kind);
    if kind == RAIN {
        init_rain(m);
    } else if kind == 0 {
        init_snow(m);
    }
    reserve_once(c, SPLAT_INFO, SPLAT_NAME, SPLAT_SIZE * (SPLATS + 2));
    reserve_once(c, POS, POS_NAME, 8 * MAX_PARTICLES);
    reserve_once(c, PREV_POS, PREV_NAME, 4 * MAX_PARTICLES);
    reserve_once(c, WAS_DRAWN, DRAWN_NAME, MAX_PARTICLES);
    let m = &mut c.m;
    let now = game_ticks(m) as u32;
    let (pos, prev, drawn, splats) = (m.r32(POS), m.r32(PREV_POS), m.r32(WAS_DRAWN), m.r32(SPLAT_INFO));
    m.w32(LAST_PROCESS + 4, now);
    m.w32(LAST_PROCESS, now);
    m.w32(POS_SERVER, pos);
    m.w32(PREV_SERVER, prev);
    m.w32(DRAWN_SERVER, drawn);
    m.w32(SPLAT_SERVER, splats);
    if GAME_SETUP.comm_mode(m) == SPLIT_SCREEN {
        m.w32(POS_SERVER + 4, pos + 8 * SPLIT_PARTICLES);
        m.w32(PREV_SERVER + 4, prev + 4 * SPLIT_PARTICLES);
        m.w32(DRAWN_SERVER + 4, drawn + SPLIT_PARTICLES);
        m.w32(SYS, SPLIT_PARTICLES);
        m.w32(SYS + 4, SPLIT_PARTICLES);
        m.w32(SPLAT_SERVER + 4, splats + SPLAT_SIZE * SPLAT_SPLIT);
    }
    // the camera's place and turn as the last (both players': the first's)
    for k in 0..3 {
        let v = m.r32(CAMERA_INFO + CAMERA_PLACE + 4 * k);
        m.w32(PREV_CAM_POS + 12 + 4 * k, v);
    }
    for k in 0..3 {
        let v = m.r32(PREV_CAM_POS + 12 + 4 * k);
        m.w32(PREV_CAM_POS + 4 * k, v);
    }
    for k in 0..MATRIX_SIZE / 4 {
        let v = m.r32(CAMERA_INFO + CAMERA_TURN + 4 * k);
        m.w32(PREV_CAM_MAT + 4 * k, v);
    }
    for k in 0..MATRIX_SIZE / 4 {
        let v = m.r32(PREV_CAM_MAT + 4 * k);
        m.w32(PREV_CAM_MAT + MATRIX_SIZE + 4 * k, v);
    }
    // (the two copies' loops: 16 bytes a pass)
    ticks(c, 2);
    for k in 0..MAX_PARTICLES {
        let at = c.m.r32(POS).wrapping_add(8 * k);
        for axis in 0..3 {
            scatter(c, at, axis);
        }
    }
    ticks(c, MAX_PARTICLES - 1);
    forget_drawn(c);
    c.m.w32(NUM_SPLATS, 0);
    init_splats(c);
}

pub fn abi_init(c: &mut Cpu) {
    init(c);
}

/// `Weather_DeInit` (WEATHER.CPP, 0x800e3488): in weather, the blocks freed.
pub fn de_init(c: &mut Cpu) {
    if GAME_SETUP.weather(&mut c.m) == 0 {
        return;
    }
    for at in [SPLAT_INFO, POS, PREV_POS, WAS_DRAWN] {
        let block = c.m.r32(at);
        if block != 0 {
            call_jal(c, PURGE, &[block]);
        }
    }
    for at in [SPLAT_INFO, POS, PREV_POS, WAS_DRAWN] {
        c.m.w32(at, 0);
    }
}

pub fn abi_de_init(c: &mut Cpu) {
    de_init(c);
}

/// A vector (data 0, 1) through the GTE's rotation and translation: MAC1..3.
fn rotate(c: &mut Cpu, v: [u16; 3]) -> [u32; 3] {
    c.gte_write(VXY0, v[0] as u32 | (v[1] as u32) << 16);
    c.gte_write(VXY0 + 1, v[2] as u32);
    c.gte_command(MVMVA);
    std::array::from_fn(|k| c.gte_read(MAC1 + k))
}

fn svector(m: &mut Mem, at: u32) -> [u16; 3] {
    std::array::from_fn(|k| m.r16(at.wrapping_add(2 * k as u32)))
}

/// `Weather_TransformVertex` (WEATHER.CPP, 0x800e351c): `n` particles at `v` turned by `matrix`
/// in place (none is no particle: the original's loop then runs on and on).
pub fn transform_vertex(c: &mut Cpu, matrix: u32, n: i32, v: u32) {
    set_matrix(c, matrix);
    for k in 0..n.max(1) as u32 {
        let at = v.wrapping_add(8 * k);
        let p = svector(&mut c.m, at);
        let r = rotate(c, p);
        for (j, w) in r.into_iter().enumerate() {
            c.m.w16(at + 2 * j as u32, w as u16);
        }
    }
}

pub fn abi_transform_vertex(c: &mut Cpu) {
    transform_vertex(c, c.r[4], c.r[5] as i32, c.r[6]);
}

/// A particle put back in the box, across `axis`'s range (0 x: the width, 1 y: the height,
/// 2 z: the depth), at random.
fn scatter(c: &mut Cpu, p: u32, axis: u32) {
    let r = random(c);
    let m = &mut c.m;
    let v = match axis {
        0 => (remu(r, sys_short(m, WIDTH)) << 1).wrapping_sub(m.r16(WIDTH) as u32),
        1 => (m.r16(BOTTOM) as u32).wrapping_add(remu(r, sys_short(m, HEIGHT))),
        _ => (m.r16(ZNEAR) as u32).wrapping_add(remu(r, sys_short(m, LENGTH))),
    };
    m.w16(p.wrapping_add(2 * axis), v as u16);
}

/// `Weather_CheckAndResetParticles` (WEATHER.CPP, 0x800e35e0): particle `p` brought back into
/// the box where it left it: past a side (1 right, 2 left), the far or near end (3, 4: from the
/// near half), the top or bottom (5, 6); the other two axes at random. The last that applied, or
/// 0.
pub fn check_and_reset_particles(c: &mut Cpu, p: u32) -> u32 {
    let mut side = 0;
    let m = &mut c.m;
    let (x, width, w) = (short(m, p), short(m, WIDTH), m.r16(WIDTH));
    if width < x || x < width.wrapping_neg() {
        // (in at the other side)
        let (x, from) = if width < x { (REENTER.wrapping_sub(w), 1) } else { (w.wrapping_sub(REENTER), 2) };
        m.w16(p, x);
        scatter(c, p, 1);
        scatter(c, p, 2);
        side = from;
    }
    let m = &mut c.m;
    let z = short(m, p + 4);
    if short(m, ZFAR) < z {
        let near = m.r16(ZNEAR).wrapping_add(REENTER);
        m.w16(p + 4, near);
        scatter(c, p, 0);
        scatter(c, p, 1);
        side = 3;
    } else if z < short(m, ZNEAR) {
        let r = random(c);
        let m = &mut c.m;
        let half = short(m, ZFAR) / 2;
        let z = (m.r16(ZNEAR) as u32).wrapping_add(remu(r, half as u32));
        m.w16(p + 4, z as u16);
        scatter(c, p, 0);
        scatter(c, p, 1);
        side = 4;
    }
    let m = &mut c.m;
    let y = short(m, p + 2);
    if short(m, TOP) < y {
        let bottom = m.r16(BOTTOM).wrapping_add(REENTER);
        m.w16(p + 2, bottom);
        scatter(c, p, 0);
        scatter(c, p, 2);
        side = 5;
    } else if y < short(m, BOTTOM) {
        let top = m.r16(TOP).wrapping_sub(REENTER);
        m.w16(p + 2, top);
        scatter(c, p, 0);
        scatter(c, p, 2);
        side = 6;
    }
    side
}

pub fn abi_check_and_reset_particles(c: &mut Cpu) {
    c.r[2] = check_and_reset_particles(c, c.r[4]);
}

/// `Weather_QuickReOrthogonalize` (WEATHER.CPP, 0x800e395c): `out` the turn `from` made square:
/// its forward row normalized, its right row the up vector (the turn's own up when it looks
/// nearly straight up or down) across it, normalized, its up row forward across right.
pub fn quick_re_orthogonalize(c: &mut Cpu, from: u32, out: u32) {
    let m = &mut c.m;
    let mut up: [i32; 3] = std::array::from_fn(|k| m.r32(UP + 4 * k as u32) as i32);
    for k in 0..3 {
        let v = m.r32(from.wrapping_add(24 + 4 * k));
        m.w32(out.wrapping_add(24 + 4 * k), v);
    }
    call_jal(c, NORMALIZE, &[out.wrapping_add(24)]);
    let m = &mut c.m;
    let f: [i32; 3] = std::array::from_fn(|k| m.r32(from.wrapping_add(24 + 4 * k as u32)) as i32);
    let dot = fixedmult(up[0], f[0]).wrapping_add(fixedmult(up[1], f[1])).wrapping_add(fixedmult(up[2], f[2]));
    if NEAR_VERTICAL < if dot > 0 { dot } else { dot.wrapping_neg() } {
        up = std::array::from_fn(|k| m.r32(from.wrapping_add(12 + 4 * k as u32)) as i32);
    }
    let fwd: [i32; 3] = std::array::from_fn(|k| m.r32(out.wrapping_add(24 + 4 * k as u32)) as i32);
    let cross = |a: [i32; 3], b: [i32; 3], k: usize| {
        let (i, j) = ((k + 1) % 3, (k + 2) % 3);
        fixedmult(a[i], b[j]).wrapping_sub(fixedmult(a[j], b[i]))
    };
    for k in 0..3 {
        let v = cross(up, fwd, k);
        m.w32(out.wrapping_add(4 * k as u32), v as u32);
    }
    call_jal(c, NORMALIZE, &[out]);
    let m = &mut c.m;
    let right: [i32; 3] = std::array::from_fn(|k| m.r32(out.wrapping_add(4 * k as u32)) as i32);
    let fwd: [i32; 3] = std::array::from_fn(|k| m.r32(out.wrapping_add(24 + 4 * k as u32)) as i32);
    for k in 0..3 {
        let v = cross(fwd, right, k);
        m.w32(out.wrapping_add(12 + 4 * k as u32), v as u32);
    }
}

pub fn abi_quick_re_orthogonalize(c: &mut Cpu) {
    quick_re_orthogonalize(c, c.r[4], c.r[5]);
}

/// `Weather_ProcessParticles` (WEATHER.CPP, 0x800e3bdc): a step of `view`'s `n` particles at `v`
/// (`drawn` their streaks' flags): turned by the camera's turn since the last (kept squared),
/// moved against the camera's motion (in the view's frame) and by the wind, each with one of the
/// twelve jitters, brought back into the box when out (its streak then forgotten).
pub fn process_particles(c: &mut Cpu, view: u32, n: i32, v: u32, drawn: u32) {
    with_buffer(c, 2 * MATRIX_SIZE + 8 * JITTERS, |c, buf| {
        let (turn, delta, jitters) = (buf, buf + MATRIX_SIZE, buf + 2 * MATRIX_SIZE);
        quick_re_orthogonalize(c, view.wrapping_add(VIEW_TURN), turn);
        let p = c.m.r32(view.wrapping_add(VIEW_PLAYER));
        let last = PREV_CAM_MAT.wrapping_add(p.wrapping_mul(MATRIX_SIZE));
        call_jal(c, FAST_TRANS_MULT, &[last, turn, delta]);
        let p = c.m.r32(view.wrapping_add(VIEW_PLAYER));
        let last = PREV_CAM_MAT.wrapping_add(p.wrapping_mul(MATRIX_SIZE));
        call_jal(c, TRANSPOSE, &[turn, last]);
        transform_vertex(c, delta, n, v);
        set_matrix(c, view.wrapping_add(VIEW_TURN));
        // the camera's motion since the last, in 1024ths, into the view's frame
        let moved: [u16; 3] = std::array::from_fn(|k| {
            let m = &mut c.m;
            let p = m.r32(view.wrapping_add(VIEW_PLAYER));
            let last = m.r32(PREV_CAM_POS.wrapping_add(p.wrapping_mul(12)).wrapping_add(4 * k as u32)) as i32;
            let d = (m.r32(view.wrapping_add(VIEW_PLACE + 4 * k as u32)) as i32).wrapping_sub(last);
            (d / 1024) as u16
        });
        c.gte_write(VXY0, moved[0] as u32 | (moved[1] as u32) << 16);
        c.gte_write(VXY0 + 1, moved[2] as u32);
        c.gte_command(MVMVA);
        let p = c.m.r32(view.wrapping_add(VIEW_PLAYER));
        for k in 0..3 {
            let w = c.m.r32(view.wrapping_add(VIEW_PLACE + 4 * k));
            c.m.w32(PREV_CAM_POS.wrapping_add(p.wrapping_mul(12)).wrapping_add(4 * k), w);
        }
        let away: [u16; 3] = std::array::from_fn(|k| (c.gte_read(MAC1 + k) as u16).wrapping_neg());
        let wind = svector(&mut c.m, VELOCITY);
        let blown = rotate(c, wind);
        let step: [u16; 3] = std::array::from_fn(|k| away[k].wrapping_add(blown[k] as u16));
        for k in 0..JITTERS {
            let j: [u16; 3] = std::array::from_fn(|i| c.m.r8(JITTER + 3 * k + i as u32) as i8 as i16 as u16);
            let r = rotate(c, j);
            for (i, w) in r.into_iter().enumerate() {
                c.m.w16(jitters + 8 * k + 2 * i as u32, w as u16);
            }
        }
        with_buffer(c, 8, |c, moved| {
            for k in 0..n.max(0) as u32 {
                let at = v.wrapping_add(8 * k);
                let jitter = jitters + 8 * (k % JITTERS);
                for i in 0..3 {
                    let w = c.m.r16(jitter + 2 * i).wrapping_add(c.m.r16(at + 2 * i).wrapping_add(step[i as usize]));
                    c.m.w16(moved + 2 * i, w);
                }
                let side = check_and_reset_particles(c, moved);
                let m = &mut c.m;
                let xy = m.r32(moved);
                m.w32(at, xy);
                let z = m.r16(moved + 4);
                m.w16(at + 4, z);
                if side as u16 != 0 {
                    m.w8(drawn.wrapping_add(k), 0);
                }
            }
        });
    });
}

pub fn abi_process_particles(c: &mut Cpu) {
    process_particles(c, c.r[4], c.r[5] as i32, c.r[6], c.r[7]);
}

/// A `POLY_FT4`'s texture from the pixmap at `pixmap` (its four words of uv, clut and page).
fn texture(m: &mut Mem, prim: u32, pixmap: u32) {
    for k in 0..4 {
        let w = m.r32(pixmap.wrapping_add(4 * k));
        m.w32(prim + 12 + 8 * k, w);
    }
}

/// `Weather_CreateSnow` (WEATHER.CPP, 0x800e3fc8): a flake at `p`: a 10-unit square projected
/// (one of two flakes by the particle's place in memory).
pub fn create_snow(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let v0 = svector(m, p);
    let pad = m.r16(p.wrapping_add(6));
    let v1 = [v0[0].wrapping_add(10), v0[1], v0[2]];
    let v2 = [v0[0], v0[1].wrapping_sub(10), v0[2]];
    let v3 = [v1[0], v2[1], v0[2]];
    for (k, v) in [v0, v1, v2].into_iter().enumerate() {
        c.gte_write(VXY0 + 2 * k, v[0] as u32 | (v[1] as u32) << 16);
        c.gte_write(VXY0 + 2 * k + 1, v[2] as u32 | (pad as u32) << 16);
    }
    c.gte_command(RTPT);
    let prim = gpu::add_prim(&mut c.m, FT4_SIZE);
    c.m.w8(prim + 3, FT4_LEN);
    for k in 0..3 {
        let sxy = c.gte_read(SXY0 + k);
        c.m.w32(prim + 8 + 8 * k as u32, sxy);
    }
    c.gte_write(VXY0, v3[0] as u32 | (v3[1] as u32) << 16);
    c.gte_write(VXY0 + 1, v3[2] as u32 | (pad as u32) << 16);
    c.gte_command(RTPS);
    let m = &mut c.m;
    m.w32(prim + 4, FT4_SEMI);
    let sxy = c.gte_read(SXY0 + 2);
    let m = &mut c.m;
    m.w32(prim + 32, sxy);
    let pixmap = m.r32(PIXMAPS + (p & 4));
    texture(m, prim, pixmap);
}

pub fn abi_create_snow(c: &mut Cpu) {
    create_snow(c, c.r[4]);
}

/// `Weather_CreateRain` (WEATHER.CPP, 0x800e4178): a drop at `p`: a streak to its screen place
/// from three quarters of the way there from its last (`prev`), across and half way down, when
/// it was drawn last frame (`drawn`), else a dot; its place kept as the last.
pub fn create_rain(c: &mut Cpu, p: u32, prev: u32, drawn: u32) {
    let m = &mut c.m;
    let v = svector(m, p);
    let pad = m.r16(p.wrapping_add(6));
    let was = m.r8(drawn) != 0;
    let (px, py) = (short(m, prev), short(m, prev.wrapping_add(2)));
    c.gte_write(VXY0, v[0] as u32 | (v[1] as u32) << 16);
    c.gte_write(VXY0 + 1, v[2] as u32 | (pad as u32) << 16);
    c.gte_command(RTPS);
    let prim = gpu::add_prim(&mut c.m, LINE_SIZE);
    let m = &mut c.m;
    m.w8(prim + 3, LINE_LEN);
    m.w32(prim + 4, LINE_SEMI_BLACK);
    m.w32(prim + 12, LINE_GREY);
    let sxy = c.gte_read(SXY0 + 2);
    let m = &mut c.m;
    m.w32(prim + 16, sxy);
    if was {
        let (sx, sy) = (short(m, prim + 16), short(m, prim + 18));
        m.w16(prim + 8, (px.wrapping_add(sx.wrapping_mul(3)) / 4) as u16);
        m.w16(prim + 10, (py.wrapping_add(sy) / 2) as u16);
    } else {
        let xy = m.r32(prim + 16);
        m.w32(prim + 8, xy);
    }
    m.w8(drawn, 1);
    let xy = m.r32(prim + 16);
    m.w32(prev, xy);
}

pub fn abi_create_rain(c: &mut Cpu) {
    create_rain(c, c.r[4], c.r[5], c.r[6]);
}

/// `Weather_CreateSplat` (WEATHER.CPP, 0x800e433c): the lens's drop `splat` (`Weather_tSplatInfo`:
/// its place, when it started): a textured square (12 or 18 across by its place's bit 0) that
/// grows and fades as it ages.
pub fn create_splat(m: &mut Mem, splat: u32) {
    let prim = gpu::add_prim(m, FT4_SIZE);
    let (x, y) = (m.r16(splat), m.r16(splat.wrapping_add(2)));
    m.w8(prim + 3, FT4_LEN);
    m.w8(prim + 7, FT4_SEMI_CODE);
    let size: u16 = if m.r16(splat) & 1 != 0 { 12 } else { 18 };
    let age = game_ticks(m).wrapping_sub(m.r32(splat.wrapping_add(4)) as i32);
    let shade = (-128i32).wrapping_sub(age.wrapping_mul(4)) as u8;
    let grow = (age >> 3) as u16;
    let left = x.wrapping_sub(grow);
    let right = x.wrapping_add(size).wrapping_add(grow);
    let bottom = y.wrapping_add(grow).wrapping_add(size).wrapping_add(grow.wrapping_mul(2));
    m.w8(prim + 6, shade);
    m.w8(prim + 5, shade);
    m.w8(prim + 4, shade);
    m.w16(prim + 26, bottom);
    m.w16(prim + 34, bottom);
    m.w16(prim + 8, left);
    m.w16(prim + 10, y);
    m.w16(prim + 16, right);
    m.w16(prim + 18, y);
    m.w16(prim + 24, left);
    m.w16(prim + 32, right);
    let pixmap = m.r32(PIXMAPS + 8);
    texture(m, prim, pixmap);
}

pub fn abi_create_splat(c: &mut Cpu) {
    create_splat(&mut c.m, c.r[4]);
}

/// `Weather_DoSplats` (WEATHER.CPP, 0x800e4464): `n` of the lens's drops (`splats`) shown: those
/// started and under 32 ticks old drawn, those older started again elsewhere within 100 ticks
/// (or, past `n`, the last of them dropped).
pub fn do_splats(c: &mut Cpu, n: i32, splats: u32) {
    if (c.m.r32(NUM_SPLATS) as i32) < n {
        c.m.w32(NUM_SPLATS, n as u32);
    }
    let mut k = 0;
    while k < c.m.r32(NUM_SPLATS) as i32 {
        let at = splats.wrapping_add(SPLAT_SIZE * k as u32);
        let m = &mut c.m;
        let (now, start) = (game_ticks(m), m.r32(at + 4) as i32);
        let count = m.r32(NUM_SPLATS) as i32;
        if now < start {
        } else if start.wrapping_add(32) < now {
            if n < count && k == count - 1 {
                m.w32(NUM_SPLATS, k as u32);
            } else {
                let x = random(c) % 320;
                c.m.w16(at, x as u16);
                let r = random(c) % 240;
                let y = if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN { r >> 1 } else { r };
                c.m.w16(at + 2, y as u16);
                let r = random(c) % 100;
                let now = game_ticks(&mut c.m);
                c.m.w32(at + 4, now.wrapping_add(r as i32) as u32);
            }
        } else {
            create_splat(m, at);
        }
        k += 1;
    }
}

pub fn abi_do_splats(c: &mut Cpu) {
    do_splats(c, c.r[4] as i32, c.r[5]);
}

/// `Weather_DoWeather` (WEATHER.CPP, 0x800e4628): `view`'s weather this frame: with one player
/// the levels moved every 33 ticks (the intensity only off the snow tracks); with particles,
/// the streaks forgotten when the player looks behind or the camera changes, the particles moved
/// (each second tick), the lens's drops in the bumper camera, each particle drawn (a drop or a
/// flake), then a draw mode.
pub fn do_weather(c: &mut Cpu, view: u32) {
    let m = &mut c.m;
    let p = m.r32(view.wrapping_add(VIEW_PLAYER));
    let at = |base: u32| base.wrapping_add(p << 2);
    let (pos, prev, drawn) = (m.r32(at(POS_SERVER)), m.r32(at(PREV_SERVER)), m.r32(at(DRAWN_SERVER)));
    if GAME_SETUP.comm_mode(m) != SPLIT_SCREEN {
        let now = game_ticks(m);
        if now.wrapping_sub(m.r32(TIME_CHANGE) as i32) >= STEP_TICKS {
            m.w32(TIME_CHANGE, now as u32);
            if m.r32(SNOW_TRACK) == 0 {
                change_intensity_based_on_time(c);
            }
            change_density_based_on_time(c);
        }
    }
    if c.m.r32(SYS) == 0 {
        return;
    }
    let m = &mut c.m;
    let look = m.r32(at(LOOK_BEHIND));
    let mut forget = look != m.r32(at(LAST_LOOK)) && look == 1;
    m.w32(at(LAST_LOOK), look);
    let mode = call_jal(c, CAMERA_GET_MODE, &[p]);
    let m = &mut c.m;
    if mode != m.r32(at(LAST_MODE)) {
        forget = true;
    }
    m.w32(at(LAST_MODE), mode);
    let count = SYS.wrapping_add(p << 2);
    if forget {
        let mut k = 0;
        while (k as i32) < m.r32(count) as i32 {
            m.w8(drawn.wrapping_add(k), 0);
            k += 1;
        }
    }
    let now = game_ticks(m);
    if now.wrapping_sub(m.r32(at(LAST_PROCESS)) as i32) >= 2 {
        m.w32(at(LAST_PROCESS), now as u32);
        let n = m.r32(count) as i32;
        process_particles(c, view, n, pos, drawn);
    }
    set_ident_matrix(c);
    let m = &mut c.m;
    let bits = m.r32(CAMERA_INFO.wrapping_add(p.wrapping_mul(CAMERA_SIZE)) + CAMERA_BITS);
    if bits & BUMPER != 0 {
        let (n, splats) = (m.r32(count) as i32, m.r32(at(SPLAT_SERVER)));
        do_splats(c, n >> 3, splats);
    }
    let mut k = 0;
    while (k as i32) < c.m.r32(count) as i32 {
        let particle = pos.wrapping_add(8 * k);
        if c.m.r32(TYPE) == RAIN {
            create_rain(c, particle, prev.wrapping_add(4 * k), drawn.wrapping_add(k));
        } else {
            create_snow(c, particle);
        }
        k += 1;
    }
    let prim = gpu::add_prim(&mut c.m, DRAW_MODE_SIZE);
    call_jal(c, SET_DRAW_MODE, &[prim, 0, 0, 32, 0]);
}

pub fn abi_do_weather(c: &mut Cpu) {
    do_weather(c, c.r[4]);
}

/// `Weather_BuildWeather` (WEATHER.CPP, 0x800e493c; the host calls it for the views it draws):
/// `view`'s weather, unless its camera is in a tunnel.
pub fn build_weather(c: &mut Cpu, view: u32) {
    if GAME_SETUP.weather(&mut c.m) == 0 {
        return;
    }
    let p = c.m.r32(view.wrapping_add(VIEW_PLAYER));
    let pos = CAMERA_INFO.wrapping_add(p.wrapping_mul(CAMERA_SIZE)).wrapping_add(CAMERA_POS);
    if call_jal(c, TUNNEL_FLAG, &[pos]) == 0 {
        do_weather(c, view);
    }
}

pub fn abi_build_weather(c: &mut Cpu) {
    build_weather(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{COMPARE_TICKS, Rng, against_original};

    /// `random`'s state (six words).
    const RANDOM_STATE: u32 = 0x8012_478C;

    /// Free RAM (the race leaves it zero): the GTE's registers before and after a run, the
    /// particles' blocks, the textures, a matrix, a particle; the drawing's ordering table entry
    /// and primitives (below the stack).
    const GTE_IN: u32 = 0x8014_4000;
    const GTE_OUT: u32 = 0x8014_4100;
    const BLOCK_POS: u32 = 0x8014_0000;
    const BLOCK_PREV: u32 = 0x8014_0500;
    const BLOCK_DRAWN: u32 = 0x8014_0800;
    const BLOCK_SPLATS: u32 = 0x8014_0900;
    const PIXMAP: u32 = 0x8014_0A00;
    const MATRIX: u32 = 0x8014_0B00;
    const PARTICLE: u32 = 0x8014_0B40;
    const OT_ENTRY: u32 = 0x801E_2A00;
    const PRIMS: u32 = 0x801E_3000;
    const PRIM_END: u32 = 0x1F80_0008;
    /// The race's camera view (`gCView`).
    const VIEW: u32 = 0x8011_8234;

    fn load_gte(c: &mut Cpu) {
        for r in 0..64 {
            c.gte_regs[r] = c.m.r32(GTE_IN + 4 * r as u32);
        }
    }

    fn dump_gte(c: &mut Cpu) {
        for r in 0..64 {
            let v = c.gte_read(r);
            c.m.w32(GTE_OUT + 4 * r as u32, v);
        }
    }

    fn gte_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        load_gte(c);
        let v = via_abi(c, a);
        dump_gte(c);
        v
    }

    fn pick_i(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
        lo + pick(rng, (hi - lo) as u32) as i32
    }

    /// The race RAM made a rainy or snowy one's: the blocks at free RAM (each player's share,
    /// two players or one), the box (rain's or snow's, now and then any), the particles inside
    /// it and out, their streaks, the lens's drops around now; the levels, their goals, steps and
    /// timers, the snow tracks; the camera turned and moved a little since the last; the look
    /// behind, the camera's mode, the bumper camera; the GTE any; the drawing at free RAM.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let split = pick(rng, 3) == 0;
        GAME_SETUP.set_comm_mode(m, split as i32);
        GAME_SETUP.set_weather(m, (pick(rng, 6) != 0) as i32);
        GAME_SETUP.set_track(m, pick(rng, 16) as i32);
        m.w32(SPEC_WEATHER, pick(rng, 2));
        for (at, block) in [(POS, BLOCK_POS), (PREV_POS, BLOCK_PREV), (WAS_DRAWN, BLOCK_DRAWN), (SPLAT_INFO, BLOCK_SPLATS)] {
            m.w32(at, block);
        }
        let share = if split { SPLIT_PARTICLES } else { 0 };
        m.w32(POS_SERVER, BLOCK_POS);
        m.w32(POS_SERVER + 4, BLOCK_POS + 8 * share);
        m.w32(PREV_SERVER, BLOCK_PREV);
        m.w32(PREV_SERVER + 4, BLOCK_PREV + 4 * share);
        m.w32(DRAWN_SERVER, BLOCK_DRAWN);
        m.w32(DRAWN_SERVER + 4, BLOCK_DRAWN + share);
        m.w32(SPLAT_SERVER, BLOCK_SPLATS);
        m.w32(SPLAT_SERVER + 4, BLOCK_SPLATS + SPLAT_SIZE * if split { SPLAT_SPLIT } else { 0 });
        // the box: rain's or snow's
        if pick(rng, 2) == 0 { init_rain(m) } else { init_snow(m) }
        let max = if split { SPLIT_PARTICLES } else { MAX_PARTICLES };
        m.w32(SYS, [max, pick(rng, max + 1), 0, 1][pick(rng, 4) as usize]);
        m.w32(SYS + 4, if split { SPLIT_PARTICLES } else { pick(rng, MAX_PARTICLES + 1) });
        m.w16(VELOCITY, pick_i(rng, -40, 40) as u16);
        m.w16(INTENSITY, [-104, -18, -32, -33, pick_i(rng, -120, 0)][pick(rng, 5) as usize] as u16);
        m.w16(VELOCITY + 4, pick_i(rng, -40, 40) as u16);
        let (w, near, far, bottom, top) = (short(m, WIDTH), short(m, ZNEAR), short(m, ZFAR), short(m, BOTTOM), short(m, TOP));
        for k in 0..MAX_PARTICLES {
            let at = BLOCK_POS + 8 * k;
            let out = pick(rng, 4) == 0;
            let x = if out { [w + 1, -w - 1, w, -w][pick(rng, 4) as usize] } else { pick_i(rng, -w, w + 1) };
            let y = if out && pick(rng, 2) == 0 { [top + 1, bottom - 1, top, bottom][pick(rng, 4) as usize] } else { pick_i(rng, bottom, top + 1) };
            let z = if out && pick(rng, 2) == 0 { [far + 1, near - 1, far, near][pick(rng, 4) as usize] } else { pick_i(rng, near, far + 1) };
            m.w16(at, x as u16);
            m.w16(at + 2, y as u16);
            m.w16(at + 4, z as u16);
            m.w16(at + 6, rng.next() as u16);
            m.w32(BLOCK_PREV + 4 * k, rng.next() as u32 & 0x00FF_01FF);
            m.w8(BLOCK_DRAWN + k, pick(rng, 2) as u8);
        }
        let now = game_ticks(m);
        for k in 0..SPLATS + 2 {
            let at = BLOCK_SPLATS + SPLAT_SIZE * k;
            m.w16(at, pick(rng, 320) as u16);
            m.w16(at + 2, pick(rng, 240) as u16);
            m.w32(at + 4, now.wrapping_add(pick_i(rng, -60, 20)) as u32);
        }
        m.w32(NUM_SPLATS, pick(rng, SPLATS + 1));
        // the levels
        m.w32(TYPE, pick(rng, 2));
        m.w32(DENSITY_GOAL, pick(rng, 4));
        m.w32(INTENSITY_GOAL, pick(rng, 4));
        m.w32(DENSITY_STEP, pick_i(rng, -1, 2) as u32);
        m.w32(INTENSITY_STEP, pick_i(rng, -1, 2) as u32);
        m.w32(DENSITY_TIMER, now.wrapping_add(pick_i(rng, -3, 3)) as u32);
        m.w32(INTENSITY_TIMER, now.wrapping_add(pick_i(rng, -3, 3)) as u32);
        m.w32(SNOW_TRACK, pick(rng, 2));
        m.w32(TRACK_LIMIT, pick(rng, 4));
        m.w32(TIME_CHANGE, now.wrapping_sub([0, 32, 33, 40][pick(rng, 4) as usize]) as u32);
        // the cameras: each player's last place, turn, look behind, mode, last step
        for p in 0..2 {
            let cam = CAMERA_INFO + p * CAMERA_SIZE;
            for k in 0..3 {
                let v = (m.r32(VIEW + VIEW_PLACE + 4 * k) as i32).wrapping_sub(pick_i(rng, -0x4_0000, 0x4_0000));
                m.w32(PREV_CAM_POS + 12 * p + 4 * k, v as u32);
            }
            for k in 0..9 {
                let v = (m.r32(VIEW + VIEW_TURN + 4 * k) as i32).wrapping_add(pick_i(rng, -0x800, 0x800));
                m.w32(PREV_CAM_MAT + MATRIX_SIZE * p + 4 * k, v as u32);
            }
            m.w32(LOOK_BEHIND + 4 * p, pick(rng, 2));
            m.w32(LAST_LOOK + 4 * p, pick(rng, 2));
            m.w32(LAST_MODE + 4 * p, pick(rng, 4));
            m.w32(LAST_PROCESS + 4 * p, now.wrapping_sub(pick(rng, 4) as i32) as u32);
            let bits = m.r32(cam + CAMERA_BITS) & !BUMPER | if pick(rng, 2) == 0 { BUMPER } else { 0 };
            m.w32(cam + CAMERA_BITS, bits);
        }
        m.w32(VIEW + VIEW_PLAYER, if split { pick(rng, 2) } else { 0 });
        // the textures
        for k in 0..3 {
            m.w32(PIXMAPS + 4 * k, PIXMAP + 16 * k);
        }
        for k in 0..12 {
            m.w32(PIXMAP + 4 * k, rng.next() as u32);
        }
        for k in 0..9 {
            m.w32(MATRIX + 4 * k, pick_i(rng, -0x1_0000, 0x1_0000) as u32);
        }
        for k in 0..4 {
            m.w16(PARTICLE + 2 * k, pick_i(rng, -3000, 3000) as u16);
        }
        for r in 0..64 {
            m.w32(GTE_IN + 4 * r, rng.next() as u32);
        }
        // the screen's set-up (as the race has it) for the projections
        m.w32(GTE_IN + 4 * (32 + 24), 160 << 16);
        m.w32(GTE_IN + 4 * (32 + 25), 120 << 16);
        m.w32(GTE_IN + 4 * (32 + 26), 256);
        m.w32(gpu::OT, OT_ENTRY);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        m.w32(PRIM_END, PRIMS + 0x8000);
        m.w32(OT_ENTRY, rng.next() as u32);
        // (the library's `random` state: else every case draws the same numbers)
        for k in 0..6 {
            m.w32(RANDOM_STATE + 4 * k, rng.next() as u32);
        }
    }

    /// The camera's slice in a tunnel now and then (its acoustics).
    fn tunnel(c: &mut Cpu, r: &mut Rng) {
        let p = c.m.r32(VIEW + VIEW_PLAYER);
        let slice = c.m.r16(CAMERA_INFO + p * CAMERA_SIZE + CAMERA_POS) as i16 as i32 as u32;
        let slices = crate::globals::BWORLDSM_SLICES.get(&mut c.m) as u32;
        let at = slices.wrapping_add(slice.wrapping_mul(crate::types::TrkNewSlice::SIZE)) + 0x15;
        c.m.w8(at, [0, 0, 0x44, 0x40][pick(r, 4) as usize]);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Weather_GetNumParticles", 0x800e_29cc, |c| { load_gte(c); nfs4rc::f_800e29cc(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_SetMatrix", 0x800e_29dc, |c| { load_gte(c); nfs4rc::f_800e29dc(c); dump_gte(c) }, |_, _| vec![MATRIX]),
            ("Weather_SetIdentMatrix", 0x800e_2aa0, |c| { load_gte(c); nfs4rc::f_800e2aa0(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_InitSnow", 0x800e_2b24, |c| { load_gte(c); nfs4rc::f_800e2b24(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_InitRain", 0x800e_2b84, |c| { load_gte(c); nfs4rc::f_800e2b84(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_InitSplats", 0x800e_2be4, |c| { load_gte(c); nfs4rc::f_800e2be4(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_GetNewState", 0x800e_2cf8, |c| { load_gte(c); nfs4rc::f_800e2cf8(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_ChangeDensityState", 0x800e_2d38, |c| { load_gte(c); nfs4rc::f_800e2d38(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_ChangeIntensityState", 0x800e_2dac, |c| { load_gte(c); nfs4rc::f_800e2dac(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_ChangeDensityBasedOnTime", 0x800e_2e24, |c| { load_gte(c); nfs4rc::f_800e2e24(c); dump_gte(c) }, |c, r| {
                // (now and then the density at its goal)
                if pick(r, 2) == 0 {
                    let goal = c.m.r32(DENSITY_GOAL);
                    let v = c.m.r32(DENSITY_TBL + 4 * goal);
                    c.m.w32(SYS, v.wrapping_add(pick(r, 3)).wrapping_sub(1));
                }
                vec![]
            }),
            ("Weather_ChangeIntensityBasedOnTime", 0x800e_2efc, |c| { load_gte(c); nfs4rc::f_800e2efc(c); dump_gte(c) }, |c, r| {
                if pick(r, 2) == 0 {
                    let goal = c.m.r32(INTENSITY_GOAL);
                    let v = c.m.r32(INTENSITY_TBL + 4 * goal);
                    c.m.w16(INTENSITY, v.wrapping_add(pick(r, 3)).wrapping_sub(1) as u16);
                }
                vec![]
            }),
            ("Weather_InitStateControls", 0x800e_2ff4, |c| { load_gte(c); nfs4rc::f_800e2ff4(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_Restart", 0x800e_30c0, |c| { load_gte(c); nfs4rc::f_800e30c0(c); dump_gte(c) }, |_, _| vec![]),
            ("Weather_Init", 0x800e_313c, |c| { load_gte(c); nfs4rc::f_800e313c(c); dump_gte(c) }, |c, r| {
                // (the blocks reserved here unless the race has them)
                for at in [POS, PREV_POS, WAS_DRAWN, SPLAT_INFO] {
                    if pick(r, 2) == 0 {
                        c.m.w32(at, 0);
                    }
                }
                vec![]
            }),
            ("Weather_DeInit", 0x800e_3488, |c| { load_gte(c); nfs4rc::f_800e3488(c); dump_gte(c) }, |c, r| {
                for at in [POS, PREV_POS, WAS_DRAWN, SPLAT_INFO] {
                    let block = if pick(r, 2) == 0 { crate::abi::call(c, nfs4rc::f_800e5f28, &[POS_NAME, 64, 0]) } else { 0 };
                    c.m.w32(at, block);
                }
                vec![]
            }),
            ("Weather_TransformVertex", 0x800e_351c, |c| { load_gte(c); nfs4rc::f_800e351c(c); dump_gte(c) }, |_, r| vec![MATRIX, 1 + pick(r, 20), BLOCK_POS]),
            ("Weather_CheckAndResetParticles", 0x800e_35e0, |c| { load_gte(c); nfs4rc::f_800e35e0(c); dump_gte(c) }, |_, r| vec![BLOCK_POS + 8 * pick(r, MAX_PARTICLES)]),
            ("Weather_QuickReOrthogonalize", 0x800e_395c, |c| { load_gte(c); nfs4rc::f_800e395c(c); dump_gte(c) }, |c, r| {
                // (the view's turn, now and then looking nearly straight up)
                if pick(r, 4) == 0 {
                    c.m.w32(VIEW + VIEW_TURN + 24, 0);
                    c.m.w32(VIEW + VIEW_TURN + 28, [0xFFFF, -0xFFFF_i32 as u32, 0xFD70, 0xFD71][pick(r, 4) as usize]);
                    c.m.w32(VIEW + VIEW_TURN + 32, pick(r, 0x100));
                }
                vec![VIEW + VIEW_TURN, MATRIX]
            }),
            ("Weather_ProcessParticles", 0x800e_3bdc, |c| { load_gte(c); nfs4rc::f_800e3bdc(c); dump_gte(c) }, |c, r| {
                let p = c.m.r32(VIEW + VIEW_PLAYER);
                let n = c.m.r32(SYS + 4 * p).max(1);
                vec![VIEW, [n, 1 + pick(r, MAX_PARTICLES)][pick(r, 2) as usize], BLOCK_POS, BLOCK_DRAWN]
            }),
            ("Weather_CreateSnow", 0x800e_3fc8, |c| { load_gte(c); nfs4rc::f_800e3fc8(c); dump_gte(c) }, |_, r| vec![PARTICLE + 4 * pick(r, 2)]),
            ("Weather_CreateRain", 0x800e_4178, |c| { load_gte(c); nfs4rc::f_800e4178(c); dump_gte(c) }, |_, r| {
                let k = pick(r, MAX_PARTICLES);
                vec![BLOCK_POS + 8 * k, BLOCK_PREV + 4 * k, BLOCK_DRAWN + k]
            }),
            ("Weather_CreateSplat", 0x800e_433c, |c| { load_gte(c); nfs4rc::f_800e433c(c); dump_gte(c) }, |_, r| vec![BLOCK_SPLATS + SPLAT_SIZE * pick(r, SPLATS)]),
            ("Weather_DoSplats", 0x800e_4464, |c| { load_gte(c); nfs4rc::f_800e4464(c); dump_gte(c) }, |_, r| vec![pick(r, SPLATS + 1), BLOCK_SPLATS]),
            ("Weather_DoWeather", 0x800e_4628, |c| { load_gte(c); nfs4rc::f_800e4628(c); dump_gte(c) }, |_, _| vec![VIEW]),
            ("Weather_BuildWeather", 0x800e_493c, |c| { load_gte(c); nfs4rc::f_800e493c(c); dump_gte(c) }, |c, r| {
                tunnel(c, r);
                vec![VIEW]
            }),
        ]
    }

    #[test]
    fn weather_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            // (the ticks too in what runs while the race loads)
            COMPARE_TICKS.set(matches!(name, "Weather_Init" | "Weather_InitSplats" | "Weather_Restart" | "Weather_InitStateControls" | "Weather_InitSnow" | "Weather_InitRain" | "Weather_DeInit"));
            let args_rng = std::cell::RefCell::new(Rng(0xF10 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0xF11 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), gte_port);
        }
    }
}
