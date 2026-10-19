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
