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

// CARS.CPP, STATS.CPP, SIM.CPP, COPSPEAK.CPP

/// `Cars_gRaceCarList`, `Cars_gAIRaceCarList`: the cars racing, and those of them the AI
/// drives (`Car_tObj *[9]`), with `Cars_gNumRaceCars`, `Cars_gNumAIRaceCars`.
pub const RACE_CARS: u32 = 0x8011_0CC4;
pub const AI_RACE_CARS: u32 = 0x8011_0D30;
pub const NUM_RACE_CARS: I32 = I32(0x8013_DAEC);
pub const NUM_AI_RACE_CARS: I32 = I32(0x8013_DAF8);
/// `Cars_topSpeedCap`: the top speed of the cars with a capped one, by car type (`int[22]`).
pub const TOP_SPEED_CAP: u32 = 0x8011_0AEC;
/// `Stats_racePosition`: the cars in race order (`Stats_tPosition[6]`).
pub const RACE_POSITION: u32 = 0x8011_F398;
/// `simVar.endSimGame`: the race is over.
pub const SIM_END_GAME: I32 = I32(0x8011_F380);
/// `CopSpeak_gQueuePlay`, `CopSpeak_gQueueHead`: the police radio's queue (the next to play,
/// the end); `CopSpeak_gSpchHandle`: what plays (-1 nothing).
pub const COPSPEAK_QUEUE_PLAY: I32 = I32(0x8013_E3A0);
pub const COPSPEAK_QUEUE_HEAD: I32 = I32(0x8013_E39C);
pub const COPSPEAK_SPCH_HANDLE: I32 = I32(0x8013_E3A4);
/// `Cars_gList`: every car (`Car_tObj *[9]`), with `Cars_gNumCars`; `Cars_gNumAICars`.
pub const CARS_LIST: u32 = 0x8011_0CA0;
pub const NUM_CARS: I32 = I32(0x8013_DAE8);
pub const NUM_AI_CARS: I32 = I32(0x8013_DAF0);
/// `Cars_gSortedList`: the cars by the slice they are on; `Cars_gTotalSortedList`: by the
/// slices they have driven.
pub const SORTED_LIST: u32 = 0x8011_0DC0;
pub const TOTAL_SORTED_LIST: u32 = 0x8011_0DE4;
/// `SceneLoaded`: the accident scene's objects are on the track; `accidentSlice`: where.
pub const SCENE_LOADED: I32 = I32(0x8013_DA4C);
pub const ACCIDENT_SLICE: I32 = I32(0x8013_DB08);
/// `frontLimit`, `rearLimit`: how hard an axle skids before its skid marks are the heaviest
/// kind.
pub const FRONT_LIMIT: I32 = I32(0x8013_DAD8);
pub const REAR_LIMIT: I32 = I32(0x8013_DADC);
/// `gTAddCarWheelDelay`: how long a wheel's sound effect lasts (doubled for the big vehicles).
pub const ADD_CAR_WHEEL_DELAY: I32 = I32(0x8013_E7D8);
/// `gLeafPixmap`: the track's leaves (`Draw_tPixMap *`; none on most tracks).
pub const LEAF_PIXMAP: U32 = U32(0x8013_E4F0);

// CONTROL.CPP, INPUT.CPP, HUD.CPP

/// `Input_gSim`: the player's pad as the simulation reads it this tick.
pub const INPUT_SIM: crate::types::InputResults = crate::types::InputResults(0x8013_E504);
/// `HudBustedOverlay`: the "busted" screen is up.
pub const HUD_BUSTED_OVERLAY: U32 = U32(0x8013_EC08);

// SCHEDULE.CPP, SIMQUEUE.CPP, SIM.CPP

/// `Sched_gExecuteInfo`: the simulation rate of each kind of object (rows of 20 bytes) by its
/// distance in 16 m steps, an index into the four tables after it (7 rates each):
/// `Sched_ExecuteTimes`, `Sched_ExecuteiTimes` and `Sched_ExecuteElapsedTimes` (the time
/// steps), `Sched_ExecuteMasks` (the ticks it runs on).
pub const SCHED_EXECUTE_INFO: u32 = 0x8011_F304;
pub const SCHED_EXECUTE_TIMES: u32 = 0x8011_F294;
pub const SCHED_EXECUTE_ELAPSED_TIMES: u32 = 0x8011_F2B0;
pub const SCHED_EXECUTE_I_TIMES: u32 = 0x8011_F2CC;
pub const SCHED_EXECUTE_MASKS: u32 = 0x8011_F2E8;
/// `inputQueue`: the players' input queued for the ticks; `output`: this tick's
/// (`Input_tResults[2]`); `maxTicksPerFrame`: how far ahead the queue may run.
pub const INPUT_QUEUE: crate::types::SimQueue = crate::types::SimQueue(0x8013_F3FC);
pub const QUEUE_OUTPUT: crate::types::InputResults = crate::types::InputResults(0x8013_F0D0);
pub const MAX_TICKS_PER_FRAME: I32 = I32(0x8013_F0D8);
/// `gSimQueue_Ticker`, `gSimQueue_BlockSelf`, `gSimQueue_BlockOther`.
pub const SIM_QUEUE_TICKER: I32 = I32(0x8013_E718);
pub const SIM_QUEUE_BLOCK_SELF: I32 = I32(0x8013_E71C);
pub const SIM_QUEUE_BLOCK_OTHER: I32 = I32(0x8013_E720);
/// `simGlobal`: the race started, its ticks ([`SIM_COUNTER`]) and its schedules; `simVar`:
/// restarting, ending, paused.
pub const SIM_GLOBAL: crate::types::SimGlobal = crate::types::SimGlobal(0x8011_F364);
pub const SIM_VAR: crate::types::SimVar = crate::types::SimVar(0x8011_F37C);
/// `countdown`: the start's countdown beeps played; `counter`: the tick of each (`int[4]`).
pub const COUNTDOWN: u32 = 0x8013_D930;
pub const COUNTDOWN_TICKS: u32 = 0x8011_F354;
/// `gMasterSFXLevel`: the sound effects' volume now (faded in at the start).
pub const MASTER_SFX_LEVEL: I32 = I32(0x8013_D948);
/// `quitType`: how the race was left (1: as usual); `skipRender`: no frame drawn this time
/// round (the simulation is ahead); `InBetween`: a slowed replay's frame between two ticks;
/// `unPauseDelay`: the clock time the pause menu takes input from.
pub const QUIT_TYPE: I32 = I32(0x8013_E714);
pub const SKIP_RENDER: I32 = I32(0x8013_E710);
pub const IN_BETWEEN: I32 = I32(0x8013_E6F0);
pub const UNPAUSE_DELAY: I32 = I32(0x8013_E70C);
/// `clock_realTime`: the clocks the timer interrupt runs (+4 in 1/x s ticks, +8 in frames).
pub const CLOCK_TICKS: I32 = I32(0x8011_1EEC);
pub const CLOCK_FRAMES: I32 = I32(0x8011_1EF0);
/// `Input_gTime`: the 32 Hz ticks the pad has been read for; `Input_gLookBehind`: each
/// player looks behind (`int[2]`).
pub const INPUT_TIME: I32 = I32(0x8013_E4F8);
pub const LOOK_BEHIND: u32 = 0x8013_E508;
/// `Replay_ReplayMode` (0, 1 racing; 2 watching a replay; 3 the attract mode),
/// `Replay_ReplayInterface`: the replay's controls.
pub const REPLAY_MODE: I32 = I32(0x8013_E6D8);
pub const REPLAY_INTERFACE: crate::types::ReplayInterface = crate::types::ReplayInterface(0x8011_82E4);
/// `Device_gForcePause`: pause (the pad was pulled out...); `Device_gPausePortIndex`: the
/// pad that paused (a byte).
pub const FORCE_PAUSE: I32 = I32(0x8013_EA60);
pub const PAUSE_PORT_INDEX: u32 = 0x8013_EA7C;
/// `Hud_ActivateCDPlayer`: show the music player's song name.
pub const HUD_ACTIVATE_CD_PLAYER: I32 = I32(0x8013_EC10);

// CARS.CPP's set-up

/// The car lists by kind (`Car_tObj *[9]`) and their counts: `Cars_gAICarList` (driven by
/// the AI), `Cars_gTrafficCarList`, `Cars_gCopCarList`, `Cars_gLifeBasisCarList` (the cars
/// the AI's world knows).
pub const AI_CARS: u32 = 0x8011_0CE8;
pub const TRAFFIC_CARS: u32 = 0x8011_0D54;
pub const COP_CARS: u32 = 0x8011_0D78;
pub const LIFE_BASIS_CARS: u32 = 0x8011_0D9C;
pub const NUM_TRAFFIC_CARS: I32 = I32(0x8013_DAFC);
pub const NUM_COP_CARS: I32 = I32(0x8013_DB00);
pub const NUM_LIFE_BASIS_CARS: I32 = I32(0x8013_DB04);
/// `GameSetup_gCarNames` (`char[51][5]`), `GameSetup_gPersonalityNames` (`char[15][8]`).
pub const CAR_NAMES: u32 = 0x8011_42FC;
pub const PERSONALITY_NAMES: u32 = 0x8011_442C;
/// `frontEnd`: the front end's state (+3 the mode, +0x384 the players' licence plates,
/// `char[2][8]`).
pub const FRONT_END: u32 = 0x8011_58B8;
/// `Paths_Paths`: the game's directories (`char *[]`: +0xC the cars', +0x10 the AI's).
pub const PATHS: u32 = 0x8011_7720;
/// `InfiniteMassNewton`: the heavy track objects' Newton objects (`BO_tNewtonObj[5]`);
/// `Object_IMassObjInst`: their instances (`Object_tIMassObjInfo *`, 32 bytes each).
pub const INFINITE_MASS_NEWTON: u32 = 0x8011_13A8;
pub const IMASS_OBJ_INST: u32 = 0x8013_E5B4;
/// `R3DCar_LicenseShapeFile`: the licence plates' shapes, loaded for the set-up.
pub const R3DCAR_LICENSE_SHAPE_FILE: U32 = U32(0x8013_E698);
/// Strings in the executable: the heaps' names ("Car_tObj", "carSpecs"), the AI packs'
/// ("%s%s.qda", "%sSTDR.qda"), the specs' ("p%s.dat", "ptram.dat"), "%sdusty.viv", and the
/// local name of the cars past the 29 proper.
pub const CAR_HEAP_NAME: u32 = 0x8005_6174;
pub const SPECS_HEAP_NAME: u32 = 0x8005_6168;
pub const QDA_NAME: u32 = 0x8005_6138;
pub const STD_QDA_NAME: u32 = 0x8005_6144;
pub const SPECS_NAME: u32 = 0x8013_DAE0;
pub const TRAM_SPECS_NAME: u32 = 0x8005_6150;
pub const VIV_NAME: u32 = 0x8005_615C;
pub const OTHER_CAR_NAME: u32 = 0x8013_DAD4;
/// The force feedback's per-tick function (FORCE.CPP), on the 32 Hz schedule.
pub const FORCE_TICK: u32 = 0x800c_b920;

// AIPHYSIC.CPP

/// The AI physics' time step this tick, as `Sched_ExecuteCheck` sets it by the car's rate:
/// `AIPhysic_time`, `AIPhysic_elapsedTime` (in 64 Hz ticks) and `AIPhysic_iTime` (16.16).
pub const AIPHYSIC_TIME: I32 = I32(0x8013_D888);
pub const AIPHYSIC_ELAPSED_TIME: I32 = I32(0x8013_D88C);
pub const AIPHYSIC_I_TIME: I32 = I32(0x8013_D884);
/// `AIPhysicConfig`: the look-ahead and the cool physics' models.
pub const AIPHYSIC_CONFIG: crate::types::AIPhysicConfig = crate::types::AIPhysicConfig(0x8010_E8D0);
/// `AIDataRecord_TrackCurve`: the track's curvature by slice.
pub const AIDATARECORD_TRACK_CURVE: U32 = U32(0x8013_D894);
/// `AITune_BTCPerpAccMults`: an AI racer's acceleration against a human cop, by the skill.
pub const AITUNE_BTC_PERP_ACC_MULTS: I32s = I32s(0x8010_F12C, 3);
/// A car just hit by a human cop, by the skill: for how many ticks (bytes), its grip then and
/// its braking.
pub const HUMAN_COP_HIT_TICKS: u32 = 0x8013_D880;
pub const HUMAN_COP_HIT_GRIP: u32 = 0x8010_E8B0;
pub const HUMAN_COP_HIT_BRAKE: u32 = 0x8010_E8C0;

// AIWORLD.CPP

/// `inverseLaneWidthTable`: 1 / a lane's width, by the width in half metres.
pub const INVERSE_LANE_WIDTH_TABLE: u32 = 0x8010_F2D0;

// AISPEEDS.CPP

/// `AI_elapsedTime`: the AI's time step this tick (64 Hz ticks).
pub const AI_ELAPSED_TIME: I32 = I32(0x8013_D844);
/// `AISpeeds_WeatherMultFactors`: the rain's speed factors by curvature (65, allocated).
pub const AISPEEDS_WEATHER_MULT_FACTORS: U32 = U32(0x8013_D8A0);
/// `AISpeeds_trackAndNightMult`: the AI racers' speed on this track (at night).
pub const AISPEEDS_TRACK_AND_NIGHT_MULT: I32 = I32(0x8013_D8A4);
/// `AISpeeds_TrackSpeeds`: each track's speed limits (slice, speed pairs).
pub const AISPEEDS_TRACK_SPEEDS: u32 = 0x8010_E9D4;
/// `AIPerson_glueTable`: the catch-up glue by the gap (21, one per 60 m).
pub const GLUE_TABLE: u32 = 0x8010_E370;
/// `leaderBoard` (`AISpeeds_tLeaderBoard`): the leading racer, human and AI racer, the last AI.
pub const LEADER_BOARD: u32 = 0x8010_EA00;
/// `CaravanInfo` (`AISpeeds_tSlotInfo[6]`): each AI slot's caravan.
pub const CARAVAN_INFO: u32 = 0x8010_EA10;
/// `engineUpgrade`, `suspensionUpgrade`, `aeroUpgrade` (`AISpeeds_Upgrade_t`).
pub const ENGINE_UPGRADE: u32 = 0x8010_E93C;
pub const SUSPENSION_UPGRADE: u32 = 0x8010_E94C;
pub const AERO_UPGRADE: u32 = 0x8010_E95C;
