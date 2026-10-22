//! AITUNE.CPP: the AI's tuning by track: whether the road is one way, the side traffic keeps
//! to, a rough lap time.

use crate::globals::{AITUNE_DRIVE_SIDE, AITUNE_ONE_WAY, AITUNE_ROUGH_LAP_TIME, AITUNE_TRACK_INFO, BWORLDSM_SLICES, GAME_SETUP};
use ps1rt::{Cpu, Mem};

/// The tracks `AITune_trackInfo` has (others are a custom track's).
const TRACKS: i32 = 12;

/// `AITune_trackInfo[track]`'s word `k` (one way, the drive side, the lap time).
fn track_info(m: &mut Mem, track: i32, k: u32) -> i32 {
    m.r32(AITUNE_TRACK_INFO.wrapping_add((track as u32).wrapping_mul(12)).wrapping_add(4 * k)) as i32
}

/// `AITune_GetOneWay` (AITUNE.CPP, 0x80073748): whether the track is one way (another track's:
/// when its second slice has no lanes on the left).
pub fn get_one_way(m: &mut Mem) -> i32 {
    let track = GAME_SETUP.track(m);
    if track >= TRACKS {
        let slices = BWORLDSM_SLICES.get(m) as u32;
        let lanes = m.r8(slices + 32 + 29);
        return (lanes >> 4 == 0) as i32;
    }
    track_info(m, track, 0)
}

pub fn abi_get_one_way(c: &mut Cpu) {
    c.r[2] = get_one_way(&mut c.m) as u32;
}

/// `AITune_GetDriveSide` (AITUNE.CPP, 0x800737a0): the side traffic keeps to (1 the right).
pub fn get_drive_side(m: &mut Mem) -> i32 {
    let track = GAME_SETUP.track(m);
    if track >= TRACKS { 1 } else { track_info(m, track, 1) }
}

pub fn abi_get_drive_side(c: &mut Cpu) {
    c.r[2] = get_drive_side(&mut c.m) as u32;
}

/// `AITune_GetRoughLapTimes` (AITUNE.CPP, 0x800737e0): about how long a lap takes (s).
pub fn get_rough_lap_times(m: &mut Mem) -> i32 {
    let track = GAME_SETUP.track(m);
    if track >= TRACKS { 190 } else { track_info(m, track, 2) }
}

pub fn abi_get_rough_lap_times(c: &mut Cpu) {
    c.r[2] = get_rough_lap_times(&mut c.m) as u32;
}

/// `AITune_StartUp1` (AITUNE.CPP, 0x80073820): the track's tuning.
pub fn start_up1(m: &mut Mem) {
    let v = get_one_way(m);
    AITUNE_ONE_WAY.set(m, v);
    let v = get_drive_side(m);
    AITUNE_DRIVE_SIDE.set(m, v);
    let v = get_rough_lap_times(m);
    AITUNE_ROUGH_LAP_TIME.set(m, v);
}

pub fn abi_start_up1(c: &mut Cpu) {
    start_up1(&mut c.m);
}

/// `AITune_StartUp2`, `AITune_CleanUp1`, `AITune_CleanUp2` (AITUNE.CPP, 0x80073858,
/// 0x80073860, 0x80073868): nothing.
pub fn abi_nothing(_: &mut Cpu) {}
