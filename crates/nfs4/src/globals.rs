//! The original's global data the decompiled code reads and writes, at their retail
//! addresses (names and types from the development build: tools/globmatch.py).

use ps1rt::Mem;

/// An `int` global.
#[derive(Clone, Copy, Debug)]
pub struct I32(pub u32);
impl I32 {
    pub fn get(self, m: &mut Mem) -> i32 {
        m.r32(self.0) as i32
    }
    pub fn set(self, m: &mut Mem, v: i32) {
        m.w32(self.0, v as u32)
    }
    pub fn update(self, m: &mut Mem, f: impl FnOnce(i32) -> i32) {
        let v = f(self.get(m));
        self.set(m, v)
    }
}

/// A `uint` global.
#[derive(Clone, Copy, Debug)]
pub struct U32(pub u32);
impl U32 {
    pub fn get(self, m: &mut Mem) -> u32 {
        m.r32(self.0)
    }
    pub fn set(self, m: &mut Mem, v: u32) {
        m.w32(self.0, v)
    }
    pub fn update(self, m: &mut Mem, f: impl FnOnce(u32) -> u32) {
        let v = f(self.get(m));
        self.set(m, v)
    }
}

/// An `int[]` global.
#[derive(Clone, Copy, Debug)]
pub struct I32s(pub u32, pub u32);
impl I32s {
    pub fn get(self, m: &mut Mem, i: u32) -> i32 {
        debug_assert!(i < self.1);
        m.r32(self.0 + 4 * i) as i32
    }
}

/// A `short[]` global.
#[derive(Clone, Copy, Debug)]
pub struct I16s(pub u32, pub u32);
impl I16s {
    pub fn get(self, m: &mut Mem, i: u32) -> i16 {
        debug_assert!(i < self.1);
        m.r16(self.0 + 2 * i) as i16
    }
}

/// A `uchar[]` global.
#[derive(Clone, Copy, Debug)]
pub struct U8s(pub u32, pub u32);
impl U8s {
    pub fn get(self, m: &mut Mem, i: u32) -> u8 {
        debug_assert!(i < self.1);
        m.r8(self.0 + i)
    }
}

// fixed-point library tables

/// `fixedsqrt`'s table, by the argument's leading zeros: the slope for 0 leading zeros (the
/// others below it, 4 bytes each), and 132 bytes on the offsets.
pub const SQRT_SLOPES: u32 = 0x8012_4858;
/// The quarter-wave sine table of `fastintsin` (257 entries, 16.16).
pub const SINE_TABLE: u32 = 0x8013_8754;
/// `atantbl`: the arctangent table of `intatan` (bytes, 1024ths of a turn).
pub const ATAN_BYTES: u32 = 0x8013_7FD8;
/// The arctangent table of `fixedatan` (257 entries over the first octant).
pub const ATAN_TABLE: u32 = 0x8013_8B58;

// PHYSICS.CPP

/// `roadSurfaceFrictionCoeff`: grip per road surface class.
pub const ROAD_SURFACE_FRICTION_COEFF: I32s = I32s(0x8011_77E8, 10);
/// `roadSurfaceIndex[3][20]`: the friction class of each road surface, per tyre type.
pub const ROAD_SURFACE_INDEX: U8s = U8s(0x8011_7838, 60);
/// `gripLossTable`, `gripLossTableWet`: per tyre type, how much grip a sliding tyre loses
/// (a divisor).
pub const GRIP_LOSS_TABLE: I32s = I32s(0x8011_7874, 3);
pub const GRIP_LOSS_TABLE_WET: I32s = I32s(0x8011_7880, 3);
/// The road's grip under the car this frame (`Physics_CalculateRoadGripModifiers`): overall,
/// per axle and per side.
pub const ROAD_MULT: I32 = I32(0x8013_E5E8);
pub const FRONT_MULT: I32 = I32(0x8013_E5EC);
pub const REAR_MULT: I32 = I32(0x8013_E5F0);
pub const LEFT_MULT: I32 = I32(0x8013_E5F4);
pub const RIGHT_MULT: I32 = I32(0x8013_E5F8);
/// `slippery`: the track is wet or snowy.
pub const SLIPPERY: I32 = I32(0x8013_E5FC);
/// `currentWallType`: the kind of barrier last hit (its sound).
pub const CURRENT_WALL_TYPE: I32 = I32(0x8013_E5E0);
/// `exceedRedline`: the engine is past the redline (by 500 rpm) this frame.
pub const EXCEED_REDLINE: I32 = I32(0x8013_E5E4);
/// `powerControl`: the engine drives the wheels (they are on the ground).
pub const POWER_CONTROL: I32 = I32(0x8013_E604);
/// `steeringControl`: the steered wheels touch the ground.
pub const STEERING_CONTROL: I32 = I32(0x8013_E600);
/// `gGasRatio`: how far the player's throttle is pressed (16.16).
pub const GAS_RATIO: I32 = I32(0x8013_E5D8);
/// `gravity_ch`: gravity in the car's frame this frame.
pub const GRAVITY_CH: crate::types::Coord = crate::types::Coord(0x8011_788C);

// the fastRandom macro's state: the last number, the product it came from, the multiplier

pub const FAST_RANDOM: U32 = U32(0x8013_E3D4);
pub const RAND_TEMP: U32 = U32(0x8013_E3D8);
pub const RAND_SEED: U32 = U32(0x8013_E3DC);

// SIM.CPP

/// `simGlobal.counter`: simulation time in 128 Hz ticks.
pub const SIM_COUNTER: I32 = I32(0x8011_F368);

// AITUNE.CPP, BWORLDSM.CPP

/// `AITune_driveSide`: which side of the road traffic keeps to (1 right, -1 left).
pub const AITUNE_DRIVE_SIDE: I32 = I32(0x8013_D8E4);
/// `gMaxFindQuadSliceIterations`: how many slices around the quad search tries.
pub const MAX_FIND_QUAD_SLICE_ITERATIONS: I32 = I32(0x8013_DAC8);
/// The slices the quad search tries next, relative to the last (+1, -1, +2, -2...).
pub const SLICE_SEARCH_STEPS: I32s = I32s(0x8011_0340, 8);
/// `BWSM_NormalCache`: road normals and forwards per quad (16 `tNormalCacheEntry`), and
/// `BWSM_NormalCacheSysTime`, the clock its entries' last use is kept by.
pub const NORMAL_CACHE: u32 = 0x8011_0370;
pub const NORMAL_CACHE_TIME: U32 = U32(0x8013_DACC);
/// `GlobalSimQuad`: the road surface where a quad has no sim quad.
pub const GLOBAL_SIM_QUAD: u32 = 0x8013_DAB0;
/// `corrPt`: `FindClosestQuad`'s point walked toward the road.
pub const CORR_PT: crate::types::Coord = crate::types::Coord(0x8013_F2CC);
/// `Track_chunkList` (`Chunk *`), `Chunk_chunkCenters` (`coorddef *`), `Chunk_lightTable`
/// (colours).
pub const TRACK_CHUNK_LIST: I32 = I32(0x8013_E79C);
pub const CHUNK_CENTERS: I32 = I32(0x8013_DB10);
pub const CHUNK_LIGHT_TABLE: I32 = I32(0x8013_DB0C);
/// `gNumSlices`: how many slices the track has.
pub const NUM_SLICES: I32 = I32(0x8013_DABC);
/// `BWorldSm_slices`: the track's slices (`Trk_NewSlice *`).
pub const BWORLDSM_SLICES: I32 = I32(0x8013_DAB4);

/// `gBrakeRatio`, `gSteerRatio`: the player's brake and steering as ratios (16.16), with
/// [`GAS_RATIO`].
pub const BRAKE_RATIO: I32 = I32(0x8013_E5D4);
pub const STEER_RATIO: I32 = I32(0x8013_E5DC);

// GAMESETUP.CPP, CARS.CPP, AIINIT.CPP

/// `GameSetup_gData`: the race as set up in the front end.
pub const GAME_SETUP: crate::types::GameSetupData = crate::types::GameSetupData(0x8011_44A4);
/// `Cars_gHumanRaceCarList`: the human players' cars (`Car_tObj *[2]`).
pub const HUMAN_RACE_CARS: u32 = 0x8011_0D0C;
/// `Cars_gNumHumanRaceCars`.
pub const NUM_HUMAN_RACE_CARS: I32 = I32(0x8013_DAF4);
/// `AIInit_forceHumanHandBrake`.
pub const FORCE_HUMAN_HAND_BRAKE: I32 = I32(0x8013_D874);

// NEWTON.CPP, AICOP.CPP

/// `divTable`: 1 / y for road normals near level (by (1 - y) in 1/128ths).
pub const DIV_TABLE: I32s = I32s(0x8011_52D4, 50);
/// `fudgeTable`: the bumps of rough ground.
pub const FUDGE_TABLE: I16s = I16s(0x8011_539C, 32);
/// `AICop_spikeBelt`: the police's spike belt.
pub const SPIKE_BELT: crate::types::AICopSpikeBelt = crate::types::AICopSpikeBelt(0x8010_E1A8);

/// `swap`: the wheel that takes each wheel's tyre compression (the same wheel).
pub const WHEEL_SWAP: u32 = 0x8011_53DC;
/// `stackSpeedUpEnbabledFlag`: the race runs on the scratchpad stack.
pub const STACK_SPEED_UP_ENABLED: I32 = I32(0x8013_EB04);
/// `gWSavePtr`: the stack pointer `SetSp` swapped out.
pub const W_SAVE_PTR: U32 = U32(0x8013_EB18);
/// `dummy.124`: where `Newton_CalcDistToClosestPlayerCar` has the ground's normal put.
pub const DUMMY_NORMAL: u32 = 0x8013_F3E0;
/// `dummy.133`: where `Newton_SetInitialSlicePositionOrientationEtc` has the ground's normal put.
pub const DUMMY_NORMAL_INIT: u32 = 0x8013_F3F0;
