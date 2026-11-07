//! REPLAY.CPP: the replays. While racing (replay modes 0 and 1) each human's pad as the
//! simulation reads it (`Input_gSim`: steering, gas, brake, flags) goes into a 32-tick record a
//! tick at a time (`controlData`, `tControllerData`: 32 steering bytes, then the gases, the
//! brakes, the flags), and each full record into the replay buffer (`Replay_ReplayBuffer`) as
//! four run-length coded parts. Watching one (mode 2) reads them back the same way and drives
//! the cars with them, under the TV cameras of the track's camera file (`camera.viv`), chosen by
//! the slice the car is on, and the replay's own controls (`Replay_GetInterfaceKey`).
//!
//! The coding (`Replay_Compress`): a part's runs of three or more of a byte become 0xFF, the
//! run's length and the byte; shorter runs stay as they are; a length byte (counting itself)
//! first. A lone 0xFF byte isn't escaped, so the reading takes it and the next two bytes for a
//! run: the original's bug, kept. The pad's bytes never are 0xFF but for the flags.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free. One leftover: reading a part back, the
//! original copies it into a 40-byte buffer in its frame and decodes it there, so a part that
//! ends in a mistaken run (a lone 0xFF flags byte last or last but one) reads the buffer past
//! the part: what the earlier parts left there, and before them the stack's leftovers. The port
//! keeps the buffer across the parts as the original does and starts it zeroed.

use crate::call::{call_addr, with_buffer};
use crate::globals::{GAME_SETUP, HUMAN_RACE_CARS, INPUT_SIM, NUM_HUMAN_RACE_CARS, NUM_SLICES, REPLAY_INTERFACE, REPLAY_MODE, SIM_GLOBAL, SIM_VAR};
use crate::types::{Car, CameraCamSlot, NewtonObj, ReplayBuffer, ReplayCameraModes};
use ps1rt::{Cpu, Mem};

/// The replay modes (`Replay_ReplayMode`): racing and recording; racing, the buffer full;
/// watching a replay; a replay whose recording has run out (no more input).
pub const MODE_RECORDING: i32 = 0;
pub const MODE_FULL: i32 = 1;
pub const MODE_WATCHING: i32 = 2;
pub const MODE_RAN_OUT: i32 = 3;

/// The recording's length in ticks (`Replay_Size`), the TV cameras the track has
/// (`numValidCams`), where the next part goes and the next is read (`Replay_ReplayStorePtr`,
/// `Replay_ReplayGetPtr`, offsets into the buffer's data), each player's tick in its record
/// (`Replay_ReplayCounter`).
const SIZE: u32 = 0x8013_E6D0;
pub const NUM_VALID_CAMS: u32 = 0x8013_E6D4;
const STORE_PTR: u32 = 0x8013_E6DC;
const GET_PTR: u32 = 0x8013_E6E0;
const COUNTERS: u32 = 0x8013_E6E4;

/// The cameras the replay's camera button steps through (`ReplayCameraList`: nine modes, the
/// TV cameras' among them), each player's camera state (`Replay_ReplayCamera`), the records
/// (`controlData`, 128 bytes a player), the buffer (`Replay_ReplayBuffer`: the race's setup, the
/// length, 24576 bytes of parts), the coding's buffers (`compressed_data`,
/// `uncompressed_data`), the track's TV cameras (`gReplayCameraSlots`, 32).
const CAMERA_LIST: u32 = 0x8011_82C0;
const CAMERA_LIST_LEN: i32 = 9;
const CAMERAS: ReplayCameraModes = ReplayCameraModes(0x8011_8304);
const CONTROL_DATA: u32 = 0x8011_8324;
const BUFFER: ReplayBuffer = ReplayBuffer(0x8011_8424);
const DATA: u32 = 0x8011_8424 + 0xA2C;
const DATA_SIZE: i32 = 24576;
const COMPRESSED: u32 = 0x8011_EE50;
const UNCOMPRESSED: u32 = 0x8011_EE74;
pub const SLOTS: CameraCamSlot = CameraCamSlot(0x8011_EE94);
const NUM_SLOTS: u32 = 32;

/// A record: 32 ticks of each of its four parts (steering, gas, brake, flags), 32 bytes each.
const TICKS: u32 = 32;
const PART: u32 = 32;
const RECORD: u32 = 128;
/// A part's run of a byte: the mark, then its length and the byte.
const RUN_MARK: u8 = 0xFF;
const RUN_MIN: u32 = 3;
/// A tick's bit 7 in its steering, gas and brake bytes: the car's ramping of each was on.
const RAMP_BIT: u8 = 0x80;

/// The race's setup (`GameSetup_gData`, its size) and the user's settings in it (kept when a
/// replay's setup is put back); the camera's modes: the TV cameras', the spline camera's, the
/// one shown without TV cameras, the one the stats screen's camera switches to.
const SETUP_SIZE: u32 = 0xA28;
const USER_SETTING: u32 = 0xE0;
const USER_SETTING_SIZE: u32 = 0xCC;
pub const CAMERA_TV: i32 = 19;
const CAMERA_SPLINE: u32 = 11;
const CAMERA_NO_TV: i32 = 4;
const CAMERA_NO_TV_SWITCH: u32 = 10;

/// The players' cameras (`Camera_gInfo`): their bits (+0x74: the zoom's kind in bits 25..26), the
/// spline's mode (+0x7E). A TV camera slot's first word: the zoom's kind (bits 9..10), the
/// spline's mode (bits 11..13).
const CAMERA_BITS: u32 = 0x74;
const CAMERA_ZOOM: u32 = 0x0600_0000;
const SLOT_ZOOM: u32 = 0x0000_0600;
const SLOT_FLAGS_INIT: u8 = 2;

/// A car's flags (`carFlags`): a human playing the police; race types whose human cops
/// always get the spline camera.
const CAR_HUMAN_COP: u32 = 0x200;
const RACE_TYPE_COP_A: i32 = 1;
const RACE_TYPE_COP_B: i32 = 5;
/// Two players (`commMode`).
const SPLIT_SCREEN: i32 = 1;

/// The replay's controls (`tReplayInterface`): the menu's items (the highlight's place,
/// `selection`), how long a press keeps the next away (`depressed`), the stats screen.
const ITEM_INSTANT_REPLAY: i32 = 0;
const ITEM_END: i32 = 1;
const ITEM_PAUSE: i32 = 2;
const ITEM_SPEED: i32 = 3;
const ITEM_CAMERA: i32 = 4;
const ITEMS: i32 = 5;
const PRESS_DELAY: i32 = 8;
const STATS_DELAY: i32 = 24;
/// The speeds: one player's four, two players' three.
const SPEEDS: i32 = 4;
const SPEEDS_SPLIT: i32 = 3;
/// The pad's interface buttons (`Input_Interface(button, edge)`): left, right, accept, back, the
/// stats screen's; the pause sounds (`AudioCmn_PlayPauseSound`): moved, accepted.
const KEY_LEFT: u32 = 3;
const KEY_RIGHT: u32 = 4;
const KEY_ACCEPT: u32 = 5;
const KEY_BACK: u32 = 6;
const KEY_STATS: u32 = 25;
const SOUND_MOVE: u32 = 3;
const SOUND_ACCEPT: u32 = 4;

/// The stats screen's timers (`StatsTimer`, a player each).
const STATS_TIMER: u32 = 0x8013_EC78;
/// The directories (`Paths_Paths`): +0x20 the tracks'.
const TRACKS_PATH: u32 = crate::globals::PATHS + 0x20;
/// The camera file's names: the big file, the track's ("tr%02d.rho", reversed "tr%02dr.rho");
/// the big file's slot and its priority.
const CAMERA_VIV: u32 = 0x8005_6D00;
const TRACK_RHO_REVERSED: u32 = 0x8005_6D10;
const TRACK_RHO: u32 = 0x8005_6D1C;
const BIG_FILE_SLOT: u32 = 16;
const BIG_FILE_PRIORITY: u32 = 100;
/// A camera's place in the file is in 1/64 of the world's units.
const SLOT_POS_SHIFT: u32 = 6;
/// How close (in slices) the car comes to the next or previous camera's slice for a cut to it.
const CUT_DISTANCE: i32 = 200;

/// Functions called: the pad (`Input_Fetch(player)`, `Input_Interface(button, edge)`), the pause
/// sound, the camera (`Camera_SetMode(player, mode)`, `Camera_ReplayUpdate(player, slot)`,
/// `Camera_Update`); the library's `sprintf`, big files (`FILE_addbigsync(name, slot, priority,
/// &handle)`, `FILE_delbigsync(handle, priority)`), `loadfileadrz(name, 0)`, `purgememadr`.
const INPUT_FETCH: u32 = 0x8009_DD58;
const INPUT_INTERFACE: u32 = 0x8009_DDDC;
const PLAY_PAUSE_SOUND: u32 = 0x8007_A16C;
const CAMERA_SET_MODE: u32 = 0x8008_602C;
const CAMERA_REPLAY_UPDATE: u32 = 0x8008_65F4;
const CAMERA_UPDATE: u32 = 0x8008_3DFC;
const SPRINTF: u32 = 0x800E_50A8;
const ADD_BIG_FILE: u32 = 0x800E_B354;
const DEL_BIG_FILE: u32 = 0x800E_B3CC;
const LOAD_FILE: u32 = 0x800E_6394;
const PURGE_MEM: u32 = 0x800E_612C;

fn human(m: &mut Mem, p: u32) -> Car {
    Car(m.r32(HUMAN_RACE_CARS.wrapping_add(p << 2)))
}

/// The slice human `p`'s car is on.
fn slice(m: &mut Mem, p: u32) -> u32 {
    human(m, p).n().sim_road_info().slice(m) as i32 as u32
}

fn counter(p: u32) -> u32 {
    COUNTERS.wrapping_add(p << 2)
}

fn record(p: u32) -> u32 {
    CONTROL_DATA.wrapping_add(p << 7)
}

/// Signed compare of two addresses, as the original's `slt`.
fn before(a: u32, b: u32) -> bool {
    (a as i32) < (b as i32)
}

/// `Replay_Compress` (REPLAY.CPP, 0x800b4920): the 32 bytes at `src` coded into
/// `compressed_data` (the length byte, the runs, a 0 after); `compressed_data`. A run's length
/// is counted to the part's end, though the byte after it is read.
pub fn compress(m: &mut Mem, src: u32) -> u32 {
    let end = src.wrapping_add(PART);
    let mut at = src;
    let mut out = 1u32;
    loop {
        let byte = m.r8(at);
        at = at.wrapping_add(1);
        let mut run = 0;
        loop {
            let next = m.r8(at);
            run += 1;
            if next != byte || !before(at, end) {
                break;
            }
            at = at.wrapping_add(1);
        }
        if run >= RUN_MIN {
            m.w8(COMPRESSED.wrapping_add(out), RUN_MARK);
            m.w8(COMPRESSED.wrapping_add(out + 1), run as u8);
            m.w8(COMPRESSED.wrapping_add(out + 2), byte);
            out += 3;
        } else {
            for _ in 0..run {
                m.w8(COMPRESSED.wrapping_add(out), byte);
                out += 1;
            }
        }
        if !before(at, end) {
            break;
        }
    }
    m.w8(COMPRESSED, out as u8);
    m.w8(COMPRESSED.wrapping_add(out), 0);
    COMPRESSED
}

pub fn abi_compress(c: &mut Cpu) {
    c.r[2] = compress(&mut c.m, c.r[4]);
}

/// A coded part (its byte `i` from `byte`) decoded into `uncompressed_data`; a run's byte is
/// read again for each copy, as the original does.
fn decode(m: &mut Mem, mut byte: impl FnMut(&mut Mem, u32) -> u8) -> u32 {
    let len = byte(m, 0) as i32;
    let mut i = 1;
    let mut out = UNCOMPRESSED;
    if i < len {
        loop {
            let b = byte(m, i as u32);
            if b == RUN_MARK {
                let run = byte(m, i as u32 + 1);
                for _ in 0..run {
                    let v = byte(m, i as u32 + 2);
                    m.w8(out, v);
                    out = out.wrapping_add(1);
                }
                i += 3;
            } else {
                m.w8(out, b);
                out = out.wrapping_add(1);
                i += 1;
            }
            if i >= len {
                break;
            }
        }
    }
    UNCOMPRESSED
}

/// `Replay_Decompress` (REPLAY.CPP, 0x800b49e4): the part coded at `src` decoded into
/// `uncompressed_data`; `uncompressed_data`.
pub fn decompress(m: &mut Mem, src: u32) -> u32 {
    decode(m, |m, i| m.r8(src.wrapping_add(i)))
}

pub fn abi_decompress(c: &mut Cpu) {
    c.r[2] = decompress(&mut c.m, c.r[4]);
}

/// `n` bytes copied from `from` to `to`, a word at a time (the setup's copies).
fn copy(m: &mut Mem, to: u32, from: u32, n: u32) {
    for k in (0..n).step_by(4) {
        let w = m.r32(from + k);
        m.w32(to + k, w);
    }
}

/// `Replay_InitReplay` (REPLAY.CPP, 0x800b4a74): the race's replay mode from its setup, the
/// track's TV cameras loaded; watching a replay, the race set up as it was recorded (but for
/// the user's settings, kept), read from its start; else the race's setup kept with the replay.
pub fn init_replay(c: &mut Cpu) {
    let mode = GAME_SETUP.replay_mode(&mut c.m);
    REPLAY_MODE.set(&mut c.m, mode);
    load_camera_file(c);
    let m = &mut c.m;
    if REPLAY_MODE.get(m) == MODE_WATCHING {
        let settings = GAME_SETUP.0 + USER_SETTING;
        let kept: Vec<u32> = (0..USER_SETTING_SIZE).step_by(4).map(|k| m.r32(settings + k)).collect();
        load_replay(c);
        let m = &mut c.m;
        copy(m, GAME_SETUP.0, BUFFER.game_setup().0, SETUP_SIZE);
        for (k, &w) in kept.iter().enumerate() {
            m.w32(settings + 4 * k as u32, w);
        }
        let size = BUFFER.size(m);
        m.w32(GET_PTR, 0);
        m.w32(SIZE, size as u32);
    } else {
        copy(m, BUFFER.game_setup().0, GAME_SETUP.0, SETUP_SIZE);
    }
}

pub fn abi_init_replay(c: &mut Cpu) {
    init_replay(c);
}

/// `Replay_ResetReplay` (REPLAY.CPP, 0x800b4c18): recording, the buffer emptied (from its end)
/// and both ends back to its start; watching, the controls reset (normal speed, the camera
/// item) and each player's camera: the TV camera nearest the car (only the first player's,
/// but with two players) or, on a track without, the chase camera; then the players' ticks and
/// the stats screen's timers.
pub fn reset_replay(c: &mut Cpu) {
    let mode = REPLAY_MODE.get(&mut c.m);
    if (mode as u32) < 2 {
        // (a tick a byte, the original's loop branching back: the host takes a long enough
        // run of them without a call to it for time passing, as at the race's load)
        for k in (0..DATA_SIZE as u32).rev() {
            c.m.w8(DATA + k, 0);
            c.tick();
        }
        let m = &mut c.m;
        m.w32(STORE_PTR, 0);
        m.w32(GET_PTR, 0);
    } else if mode >= 2 {
        let m = &mut c.m;
        let ui = REPLAY_INTERFACE;
        ui.set_pause(m, 0);
        ui.set_speed(m, 2);
        m.w32(GET_PTR, 0);
        ui.set_end(m, 0);
        ui.set_camera(m, 0);
        ui.set_selection(m, ITEM_CAMERA);
        ui.set_depressed(m, 0);
        ui.set_change_camera(m, 0);
        for p in 0..2 {
            let cam = CAMERAS.at(p);
            let mode = if c.m.r32(NUM_VALID_CAMS) != 0 {
                if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN || p == 0 {
                    let slice = slice(&mut c.m, p);
                    find_closest_camera(c, p, slice);
                }
                CAMERA_TV
            } else {
                CAMERA_NO_TV
            };
            cam.set_camera_mode(&mut c.m, mode);
            cam.set_cut_to_next_camera(&mut c.m, 0);
        }
    }
    let m = &mut c.m;
    m.w32(counter(1), 0);
    m.w32(counter(0), 0);
    m.w32(STATS_TIMER, 0);
    m.w32(STATS_TIMER + 4, 0);
}

pub fn abi_reset_replay(c: &mut Cpu) {
    reset_replay(c);
}

/// `Replay_StoringReplay` (REPLAY.CPP, 0x800b4d70): the race's end: the replay kept (its setup
/// and length), the TV cameras forgotten.
pub fn storing_replay(c: &mut Cpu) {
    save_replay(&mut c.m);
    c.m.w32(NUM_VALID_CAMS, 0);
}

pub fn abi_storing_replay(c: &mut Cpu) {
    storing_replay(c);
}

/// `Replay_StoringControllerData` (REPLAY.CPP, 0x800b4d90): the record at `data` (passed by
/// value) coded into the buffer, its four parts in turn; with no room for a whole record, the
/// recording stops there (the buffer full, its length the ticks so far).
pub fn storing_controller_data(m: &mut Mem, data: u32) {
    if m.r32(STORE_PTR) as i32 + RECORD as i32 >= DATA_SIZE {
        let ticks = SIM_GLOBAL.game_ticks(m);
        REPLAY_MODE.set(m, MODE_FULL);
        m.w32(SIZE, ticks as u32);
        return;
    }
    for part in 0..4 {
        compress(m, data.wrapping_add(part * PART));
        let len = m.r8(COMPRESSED) as u32;
        let at = m.r32(STORE_PTR);
        for k in 0..len {
            let b = m.r8(COMPRESSED + k);
            m.w8(DATA.wrapping_add(at).wrapping_add(k), b);
        }
        m.w32(STORE_PTR, at.wrapping_add(len));
    }
}

/// From the game: the record in a0..a3 and on at sp+16, homed into the caller's argument area as
/// the original does, which makes it whole at sp.
pub fn abi_storing_controller_data(c: &mut Cpu) {
    let sp = c.r[29];
    for k in 0..4 {
        c.m.w32(sp + 4 * k, c.r[4 + k as usize]);
    }
    storing_controller_data(&mut c.m, sp);
}

/// The next record read from the buffer: its four parts decoded in turn (see the module's
/// note on the buffer they are copied into).
fn read_record(m: &mut Mem) -> [u8; RECORD as usize] {
    // (the original's 40 bytes; a coded part is at most 33, the rest for parts that aren't)
    let mut copied = [0u8; 256 + 3];
    let mut data = [0u8; RECORD as usize];
    for part in 0..4 {
        let at = m.r32(GET_PTR);
        let len = m.r8(DATA.wrapping_add(at)) as usize;
        for (k, b) in copied.iter_mut().enumerate().take(len) {
            *b = m.r8(DATA.wrapping_add(at).wrapping_add(k as u32));
        }
        decode(m, |_, i| copied[i as usize]);
        for k in 0..PART {
            data[(part * PART + k) as usize] = m.r8(UNCOMPRESSED + k);
        }
        m.w32(GET_PTR, at.wrapping_add(copied[0] as u32));
    }
    data
}

/// `Replay_RetreivingControllerData` (REPLAY.CPP, 0x800b515c): the next record read from the
/// buffer into `out` (the value returned); `out`.
pub fn retreiving_controller_data(m: &mut Mem, out: u32) -> u32 {
    let data = read_record(m);
    for (k, &b) in data.iter().enumerate() {
        m.w8(out.wrapping_add(k as u32), b);
    }
    out
}

pub fn abi_retreiving_controller_data(c: &mut Cpu) {
    c.r[2] = retreiving_controller_data(&mut c.m, c.r[4]);
}

/// `Replay_SaveInput` (REPLAY.CPP, 0x800b54f8): human `p`'s pad this tick into its record (the
/// steering to 1/4 around 64, the gas and brake to 1/8, the flags, the car's ramping of the
/// first three in their bit 7); a full record into the buffer.
pub fn save_input(c: &mut Cpu, p: u32) {
    call_addr(c, INPUT_FETCH, &[p]);
    let m = &mut c.m;
    let tick = m.r32(counter(p));
    let at = record(p).wrapping_add(tick);
    let steering = INPUT_SIM.steering(m) as i32;
    m.w8(at, (steering / 4 + 64) as u8);
    let gas = INPUT_SIM.gas(m);
    m.w8(at + PART, gas >> 3);
    let brake = INPUT_SIM.brake(m);
    m.w8(at + 2 * PART, brake >> 3);
    let flags = INPUT_SIM.flags(m);
    m.w8(at + 3 * PART, flags);
    let setup = human(m, p).car_info(m);
    for (part, ramping) in [setup.ramp_steering(m), setup.ramp_gas(m), setup.ramp_brake(m)].into_iter().enumerate() {
        if ramping != 0 {
            let b = at + part as u32 * PART;
            let v = m.r8(b) | RAMP_BIT;
            m.w8(b, v);
        }
    }
    let tick = tick.wrapping_add(1);
    m.w32(counter(p), tick);
    if tick == TICKS {
        m.w32(counter(p), 0);
        storing_controller_data(m, record(p));
    }
}

pub fn abi_save_input(c: &mut Cpu) {
    save_input(c, c.r[4]);
}

/// `Replay_GetInput` (REPLAY.CPP, 0x800b580c): human `p`'s pad this tick from the replay: a
/// record read at its first tick (the recording's end, a 0 length, stops the input); watching,
/// the car's ramping and the pad from the tick's bytes (the ramping bits cleared from them),
/// once it has run out no input; then a TV camera chosen by the car's slice.
pub fn get_input(c: &mut Cpu, p: u32) {
    call_addr(c, INPUT_FETCH, &[p]);
    let m = &mut c.m;
    if m.r32(counter(p)) == 0 {
        let at = m.r32(GET_PTR);
        if m.r8(DATA.wrapping_add(at)) == 0 {
            REPLAY_MODE.set(m, MODE_RAN_OUT);
        } else {
            let data = read_record(m);
            for (k, &b) in data.iter().enumerate() {
                m.w8(record(p) + k as u32, b);
            }
        }
    }
    let mode = REPLAY_MODE.get(m);
    if mode == MODE_WATCHING {
        let at = record(p).wrapping_add(m.r32(counter(p)));
        let setup = human(m, p).car_info(m);
        let ramping = |m: &mut Mem, part: u32| (m.r8(at + part * PART) & RAMP_BIT != 0) as i32;
        let v = ramping(m, 0);
        setup.set_ramp_steering(m, v);
        let v = ramping(m, 1);
        setup.set_ramp_gas(m, v);
        let v = ramping(m, 2);
        setup.set_ramp_brake(m, v);
        for part in 0..3 {
            let b = at + part * PART;
            let v = m.r8(b) & !RAMP_BIT;
            m.w8(b, v);
        }
        let steering = (m.r8(at) as i32 - 64) << 2;
        INPUT_SIM.set_steering(m, steering as i8);
        let gas = m.r8(at + PART) << 3;
        INPUT_SIM.set_gas(m, gas);
        let brake = m.r8(at + 2 * PART) << 3;
        INPUT_SIM.set_brake(m, brake);
        let flags = m.r8(at + 3 * PART);
        INPUT_SIM.set_flags(m, flags);
    } else if mode == MODE_RAN_OUT {
        INPUT_SIM.set_steering(m, 0);
        INPUT_SIM.set_gas(m, 0);
        INPUT_SIM.set_brake(m, 0);
        INPUT_SIM.set_flags(m, 0);
    }
    let tick = m.r32(counter(p)).wrapping_add(1);
    m.w32(counter(p), tick);
    if m.r32(NUM_VALID_CAMS) != 0 && CAMERAS.at(p).camera_mode(m) == CAMERA_TV && SIM_GLOBAL.game_started(m) != 0 {
        let slice = slice(m, p);
        choose_camera(c, p, slice);
    }
    let m = &mut c.m;
    if m.r32(counter(p)) == TICKS {
        m.w32(counter(p), 0);
    }
}

pub fn abi_get_input(c: &mut Cpu) {
    get_input(c, c.r[4]);
}

/// `Replay_SaveReplay` (REPLAY.CPP, 0x800b5c6c): the replay kept with the race's setup (to be
/// watched: mode 2) and its length (the ticks so far, unless the buffer filled first).
pub fn save_replay(m: &mut Mem) {
    if REPLAY_MODE.get(m) == MODE_RECORDING {
        let ticks = SIM_GLOBAL.game_ticks(m);
        m.w32(SIZE, ticks as u32);
    }
    copy(m, BUFFER.game_setup().0, GAME_SETUP.0, SETUP_SIZE);
    BUFFER.game_setup().set_replay_mode(m, MODE_WATCHING);
    let size = m.r32(SIZE);
    BUFFER.set_size(m, size as i32);
}

pub fn abi_save_replay(c: &mut Cpu) {
    save_replay(&mut c.m);
}

/// `Replay_LoadReplay` (REPLAY.CPP, 0x800b5cf8): nothing (the replay stays in RAM).
pub fn load_replay(_c: &mut Cpu) {}

pub fn abi_load_replay(c: &mut Cpu) {
    load_replay(c);
}

/// `Replay_DoReplay` (REPLAY.CPP, 0x800b5d00; a car's replay function): its human's pad
/// recorded while racing, replayed while watching.
pub fn do_replay(c: &mut Cpu, car: Car) {
    let p = car.human_index(&mut c.m) as u32;
    if (REPLAY_MODE.get(&mut c.m) as u32) < 2 {
        save_input(c, p);
    } else {
        get_input(c, p);
    }
}

pub fn abi_do_replay(c: &mut Cpu) {
    do_replay(c, Car(c.r[4]));
}

fn pressed(c: &mut Cpu, key: u32) -> bool {
    call_addr(c, INPUT_INTERFACE, &[key, 1]) != 0
}

fn play(c: &mut Cpu, sound: u32) {
    call_addr(c, PLAY_PAUSE_SOUND, &[sound]);
}

/// `Replay_GetInterfaceKey` (REPLAY.CPP, 0x800b5d44): a tick of the replay's controls. A press
/// keeps the next away for a while; on the stats screen accept or back ends the replay and its
/// button leaves it; else left and right move along the menu's items and accept takes one
/// (instant replay, end, pause, the next speed, the next camera: the TV cameras nearest the
/// cars or another mode for each player), back ends the replay and the stats button shows the
/// stats screen. The cameras are updated then while paused quickly.
pub fn get_interface_key(c: &mut Cpu) {
    let players = if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN { 2 } else { 1 };
    let ui = REPLAY_INTERFACE;
    let m = &mut c.m;
    ui.set_change_camera(m, 0);
    let depressed = ui.depressed(m);
    if depressed > 0 {
        ui.set_depressed(m, depressed - 1);
    } else if ui.stats_screen(m) != 0 {
        if pressed(c, KEY_ACCEPT) || pressed(c, KEY_BACK) {
            ui.set_depressed(&mut c.m, STATS_DELAY);
            ui.set_end(&mut c.m, 1);
        } else if pressed(c, KEY_STATS) {
            play(c, SOUND_ACCEPT);
            ui.set_stats_screen(&mut c.m, 0);
        }
    } else if pressed(c, KEY_LEFT) {
        play(c, SOUND_MOVE);
        let m = &mut c.m;
        let item = ui.selection(m);
        ui.set_selection(m, if item == 0 { ITEMS - 1 } else { item - 1 });
    } else if pressed(c, KEY_RIGHT) {
        play(c, SOUND_MOVE);
        let m = &mut c.m;
        let item = ui.selection(m).wrapping_add(1);
        ui.set_selection(m, item.wrapping_rem(ITEMS));
    } else if pressed(c, KEY_ACCEPT) {
        play(c, SOUND_ACCEPT);
        let m = &mut c.m;
        let item = ui.selection(m);
        ui.set_depressed(m, PRESS_DELAY);
        match item {
            ITEM_INSTANT_REPLAY => {
                ui.set_end(m, 1);
                GAME_SETUP.set_instant_replay(m, 1);
            }
            ITEM_END => ui.set_end(m, 1),
            ITEM_PAUSE => {
                let pause = ui.pause(m);
                ui.set_pause(m, pause ^ 1);
            }
            ITEM_SPEED => {
                let speeds = if GAME_SETUP.comm_mode(m) == SPLIT_SCREEN { SPEEDS_SPLIT } else { SPEEDS };
                let speed = ui.speed(m).wrapping_add(1);
                ui.set_speed(m, speed.wrapping_rem(speeds));
            }
            ITEM_CAMERA => next_camera(c, players),
            _ => {}
        }
    } else if pressed(c, KEY_BACK) {
        let m = &mut c.m;
        ui.set_depressed(m, PRESS_DELAY);
        ui.set_end(m, 1);
    } else if pressed(c, KEY_STATS) {
        let m = &mut c.m;
        m.w32(STATS_TIMER, 0);
        m.w32(STATS_TIMER + 4, 0);
        ui.set_stats_screen(m, 1);
        play(c, SOUND_ACCEPT);
    }
    if SIM_VAR.quick_pause_sim(&mut c.m) != 0 {
        call_addr(c, CAMERA_UPDATE, &[]);
    }
}

pub fn abi_get_interface_key(c: &mut Cpu) {
    get_interface_key(c);
}

/// The replay's camera button: the list's next camera for each player. The TV cameras: the
/// one nearest each car (or, on a track without, the chase camera); another mode: that mode,
/// the camera's twist and default cleared.
fn next_camera(c: &mut Cpu, players: u32) {
    let ui = REPLAY_INTERFACE;
    let m = &mut c.m;
    ui.set_change_camera(m, 1);
    let camera = ui.camera(m).wrapping_add(1).wrapping_rem(CAMERA_LIST_LEN);
    ui.set_camera(m, camera);
    let list_mode = |m: &mut Mem| {
        let camera = ui.camera(m) as u32;
        m.r32(CAMERA_LIST.wrapping_add(camera << 2))
    };
    if list_mode(m) as i32 == CAMERA_TV {
        for p in 0..players {
            if c.m.r32(NUM_VALID_CAMS) != 0 {
                let m = &mut c.m;
                let info = crate::camera::info(p);
                let car = human(m, p);
                info.set_anchor(m, NewtonObj(car.0));
                let car = human(m, p);
                info.set_target(m, NewtonObj(car.0));
                let slice = slice(m, p);
                find_closest_camera(c, p, slice);
                CAMERAS.at(p).set_camera_mode(&mut c.m, CAMERA_TV);
            } else {
                call_addr(c, CAMERA_SET_MODE, &[p, CAMERA_NO_TV_SWITCH]);
            }
        }
    } else {
        for p in 0..players {
            let m = &mut c.m;
            let info = crate::camera::info(p);
            CAMERAS.at(p).set_default_camera(m, 0);
            info.set_twist(m, 0);
            let car = human(m, p);
            info.set_anchor(m, NewtonObj(car.0));
            let car = human(m, p);
            info.set_target(m, NewtonObj(car.0));
            let mode = list_mode(m);
            call_addr(c, CAMERA_SET_MODE, &[p, mode]);
            let mode = list_mode(&mut c.m);
            CAMERAS.at(p).set_camera_mode(&mut c.m, mode as i32);
        }
    }
}

/// `Replay_LoadCameraFile` (REPLAY.CPP, 0x800b61d4): once a race, the track's TV cameras from
/// its camera file (the reversed track's own): the slots reset, the file's 32 read in, those
/// with a field of view counted (the rest's slices -1), sorted by slice (the unused last),
/// reversed tracks' slices turned one along, the places scaled up.
pub fn load_camera_file(c: &mut Cpu) {
    if c.m.r32(NUM_VALID_CAMS) != 0 {
        return;
    }
    let data = with_buffer(c, 112 + 8, |c, name| {
        let handle = name + 112;
        c.m.w32(handle, 0);
        let path = c.m.r32(TRACKS_PATH);
        call_addr(c, SPRINTF, &[name, CAMERA_VIV, path]);
        call_addr(c, ADD_BIG_FILE, &[name, BIG_FILE_SLOT, BIG_FILE_PRIORITY, handle]);
        let m = &mut c.m;
        let format = if GAME_SETUP.reverse_track(m) != 0 { TRACK_RHO_REVERSED } else { TRACK_RHO };
        let track = GAME_SETUP.track(m);
        call_addr(c, SPRINTF, &[name, format, track as u32]);
        let data = call_addr(c, LOAD_FILE, &[name, 0]);
        c.m.w32(NUM_VALID_CAMS, 0);
        let handle = c.m.r32(handle);
        call_addr(c, DEL_BIG_FILE, &[handle, BIG_FILE_PRIORITY]);
        data
    });
    install_cameras(c, data);
}

/// [`load_camera_file`]'s cameras from the file loaded at `data` (0: none).
fn install_cameras(c: &mut Cpu, data: u32) {
    let m = &mut c.m;
    for k in 0..NUM_SLOTS {
        let slot = SLOTS.at(k);
        m.w8(slot.0, SLOT_FLAGS_INIT);
        let flags = m.r32(slot.0);
        slot.set_slice(m, -1);
        m.w32(slot.0, flags & !SLOT_ZOOM);
    }
    if data == 0 {
        return;
    }
    copy(m, SLOTS.0, data, NUM_SLOTS * CameraCamSlot::SIZE);
    for k in 0..NUM_SLOTS {
        let slot = SLOTS.at(k);
        if slot.fov(m) != 0 {
            let n = m.r32(NUM_VALID_CAMS);
            m.w32(NUM_VALID_CAMS, n.wrapping_add(1));
        } else {
            slot.set_slice(m, -1);
        }
    }
    call_addr(c, PURGE_MEM, &[data]);
    let m = &mut c.m;
    // (a bubble sort: by slice, the unused, -1, after the rest)
    for _ in 0..NUM_SLOTS - 1 {
        for k in 0..NUM_SLOTS - 1 {
            let (a, b) = (SLOTS.at(k), SLOTS.at(k + 1));
            let (sa, sb) = (a.slice(m), b.slice(m));
            if sb >= 0 && (sb < sa || sa < 0) {
                let size = CameraCamSlot::SIZE;
                let first: Vec<u32> = (0..size).step_by(4).map(|o| m.r32(a.0 + o)).collect();
                copy(m, a.0, b.0, size);
                for (o, &w) in first.iter().enumerate() {
                    m.w32(b.0 + 4 * o as u32, w);
                }
            }
        }
    }
    let n = m.r32(NUM_VALID_CAMS) as i32;
    if GAME_SETUP.reverse_track(m) != 0 {
        // (each camera takes the slice of the one before it, the first the last's)
        let last = SLOTS.at((n - 1) as u32).slice(m);
        for k in (0..n).rev() {
            let slice = if k == 0 { last } else { SLOTS.at(k as u32 - 1).slice(m) };
            SLOTS.at(k as u32).set_slice(m, slice);
        }
    }
    let n = m.r32(NUM_VALID_CAMS) as i32;
    for k in 0..n.max(0) as u32 {
        let pos = SLOTS.at(k).pos();
        let (x, z) = (pos.x(m), pos.z(m));
        pos.set_x(m, x << SLOT_POS_SHIFT);
        let y = pos.y(m);
        pos.set_z(m, z << SLOT_POS_SHIFT);
        pos.set_y(m, y << SLOT_POS_SHIFT);
    }
}

pub fn abi_load_camera_file(c: &mut Cpu) {
    load_camera_file(c);
}

/// Player `p`'s camera to TV camera slot `cut` (its state's `cutToNextCamera`): a spline
/// camera slot sets the spline camera up (its zoom and spline mode from the slot), the others
/// are shown as they are (`Camera_ReplayUpdate`).
fn cut_to(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let cam = CAMERAS.at(p);
    let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
    if m.r8(slot.0) as u32 == CAMERA_SPLINE {
        let info = crate::camera::info(p);
        cam.set_default_camera(m, 0);
        let bits_at = info.0 + CAMERA_BITS;
        let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
        let zoom = (m.r32(slot.0) >> 9) & 3;
        let bits = (m.r32(bits_at) & !CAMERA_ZOOM) | (zoom << 25);
        m.w32(bits_at, bits);
        let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
        let spline = (m.r32(slot.0) >> 11) & 7;
        info.set_spline_mode(m, spline as i8);
        call_addr(c, CAMERA_SET_MODE, &[p, CAMERA_SPLINE]);
    } else {
        let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
        cam.set_default_camera(m, 1);
        call_addr(c, CAMERA_REPLAY_UPDATE, &[p, slot.0]);
    }
}

/// `Replay_ReplayChooseCamera` (REPLAY.CPP, 0x800b6540): player `p`'s TV camera as its car
/// (on `slice`) goes: the previous one once the car is back before the current one's slice
/// (and near it), the next once it reaches the next's (and is near it); cut to a new one,
/// unless the car is going the wrong way (a human cop in the police's races always), which the
/// spline camera slots alone show.
pub fn choose_camera(c: &mut Cpu, p: u32, slice: u32) {
    let m = &mut c.m;
    let slice = slice as i32;
    let cam = CAMERAS.at(p);
    let current = cam.cut_to_next_camera(m);
    let n = m.r32(NUM_VALID_CAMS) as i32;
    let previous = if current.wrapping_sub(1) < 0 { n.wrapping_sub(1) } else { current.wrapping_sub(1) };
    let next = if current.wrapping_add(1) < n { current.wrapping_add(1) } else { 0 };
    let here = SLOTS.at(current as u32).slice(m) as i32;
    let ahead = SLOTS.at(next as u32).slice(m) as i32;
    if slice < here && here.wrapping_sub(slice) < CUT_DISTANCE {
        cam.set_cut_to_next_camera(m, previous);
    } else if slice >= ahead && slice.wrapping_sub(ahead) < CUT_DISTANCE {
        cam.set_cut_to_next_camera(m, next);
    }
    if cam.cut_to_next_camera(m) == current {
        return;
    }
    let race = GAME_SETUP.race_type(m);
    let cop = |m: &mut Mem, k: u32| human(m, k).car_flags(m) as u32 & CAR_HUMAN_COP != 0;
    let any_cop = (race == RACE_TYPE_COP_A || race == RACE_TYPE_COP_B)
        && (cop(m, 0) || (NUM_HUMAN_RACE_CARS.get(m) == 2 && cop(m, 1)));
    let spline_ok = any_cop || human(m, p).wrongway(m) != 0;
    let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
    if spline_ok && m.r8(slot.0) as u32 == CAMERA_SPLINE {
        cut_to(c, p);
    } else {
        let slot = SLOTS.at(cam.cut_to_next_camera(m) as u32);
        cam.set_default_camera(m, 1);
        call_addr(c, CAMERA_REPLAY_UPDATE, &[p, slot.0]);
    }
}

pub fn abi_choose_camera(c: &mut Cpu) {
    choose_camera(c, c.r[4], c.r[5]);
}

/// How far `a` is from `b` along a track of `n` slices that loops (`half` its half), as the
/// original works it out.
fn distance(a: i32, b: i32, n: i32, half: i32) -> i32 {
    let d = a.wrapping_sub(b);
    let ahead = if half < d { n.wrapping_sub(d) > 0 } else { d > 0 };
    match (ahead, half < d) {
        (true, true) => n.wrapping_sub(d),
        (true, false) => d,
        (false, true) => d.wrapping_sub(n),
        (false, false) => b.wrapping_sub(a),
    }
}

/// `Replay_ReplayFindClosestCamera` (REPLAY.CPP, 0x800b67e8): player `p`'s TV camera for a car
/// on `slice`: the first whose stretch to the next camera has the car in it (else the first),
/// cut to (a spline camera slot sets the spline camera up).
pub fn find_closest_camera(c: &mut Cpu, p: u32, slice: u32) {
    let m = &mut c.m;
    let slice = slice as i32;
    let n = NUM_SLICES.get(m);
    let half = n / 2;
    let cams = m.r32(NUM_VALID_CAMS) as i32;
    let mut best = 0;
    for i in 0..cams.max(0) {
        let next = if i + 1 < cams { i + 1 } else { 0 };
        let here = SLOTS.at(i as u32).slice(m) as i32;
        let ahead = SLOTS.at(next as u32).slice(m) as i32;
        let to_here = distance(slice, here, n, half);
        let to_ahead = distance(slice, ahead, n, half);
        if distance(here, ahead, n, half) >= to_here.wrapping_add(to_ahead) {
            best = i;
            break;
        }
    }
    CAMERAS.at(p).set_cut_to_next_camera(m, best);
    cut_to(c, p);
}

pub fn abi_find_closest_camera(c: &mut Cpu) {
    find_closest_camera(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// The pad's buttons by tick (`Input_gInterfaceResults`, 32), the tick (`simGlobal` +8).
    const INPUT_RESULTS: u32 = 0x8011_51CC;
    const SIM_TICK: u32 = 0x8011_F36C;
    /// Free RAM (the race leaves it zero): a camera file; a record to code, a coded part, a
    /// record read back.
    const FILE: u32 = 0x8014_0000;
    const RECORD_IN: u32 = 0x8014_1000;
    const PART_IN: u32 = 0x8014_1100;
    const RECORD_OUT: u32 = 0x8014_1200;

    /// A record as the pad gives them: steering around 64 (bit 7 the ramping), the gas and
    /// brake to 1/8 (the same), flags (never 0xFF, the coding's blind spot); in runs.
    fn fill_record(m: &mut Mem, at: u32, rng: &mut Rng) {
        for part in 0..4 {
            let ramp = if part < 3 && pick(rng, 2) == 0 { RAMP_BIT } else { 0 };
            let kind = pick(rng, 4);
            let mut b = 0u8;
            for k in 0..PART {
                if k == 0 || pick(rng, [32, 8, 2, 1][kind as usize]) == 0 {
                    b = match part {
                        0 => 32 + pick(rng, 64) as u8,
                        1 | 2 => [0, 31, pick(rng, 32) as u8][pick(rng, 3) as usize],
                        _ => [0, 1, 4, 0x40, 0x7F, pick(rng, 255) as u8][pick(rng, 6) as usize],
                    } | ramp;
                }
                m.w8(at + part * PART + k, b);
            }
        }
    }

    /// Coded records from the buffer offset `at` on (coded by the port: only the input);
    /// where they end.
    fn write_records(m: &mut Mem, rng: &mut Rng, mut at: u32, n: u32) -> u32 {
        for _ in 0..n {
            fill_record(m, RECORD_OUT, rng);
            for part in 0..4 {
                compress(m, RECORD_OUT + part * PART);
                let len = m.r8(COMPRESSED) as u32;
                for k in 0..len {
                    let b = m.r8(COMPRESSED + k);
                    m.w8(DATA + at + k, b);
                }
                at += len;
            }
        }
        at
    }

    /// The race RAM, the replay in any state the game can be in: recording or watching (or
    /// run out), each player at any tick of its record, the buffer nearly full or not, coded
    /// records to read (the recording ending after them or not), the pad, the car's ramping,
    /// flags, way and slice; the TV cameras (the track's 18 or fewer, other modes and zooms;
    /// none), each player's camera state, the controls, the buttons this tick and the last;
    /// the setup (two players, the race type, a reversed track); a camera file to load.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        // (the race has one human: the second player drives the second car, an opponent)
        let second = m.r32(crate::globals::CARS_LIST + 4);
        m.w32(HUMAN_RACE_CARS + 4, second);
        NUM_HUMAN_RACE_CARS.set(m, 1 + pick(rng, 2) as i32);
        REPLAY_MODE.set(m, [0, 1, 2, 2, 2, 3, -1][pick(rng, 7) as usize]);
        GAME_SETUP.set_comm_mode(m, pick(rng, 2) as i32);
        GAME_SETUP.set_race_type(m, pick(rng, 8) as i32);
        GAME_SETUP.set_reverse_track(m, pick(rng, 2) as i32);
        GAME_SETUP.set_replay_mode(m, [0, 1, 2, 2, 3][pick(rng, 5) as usize]);
        GAME_SETUP.set_instant_replay(m, pick(rng, 2) as i32);
        for p in 0..2 {
            m.w32(counter(p), [0, 0, 31, pick(rng, 32)][pick(rng, 4) as usize]);
        }
        let full = DATA_SIZE as u32 - RECORD;
        m.w32(STORE_PTR, [0, 131, pick(rng, full), full, full - 1, full + 1][pick(rng, 6) as usize]);
        let start = pick(rng, 4000);
        let n = 1 + pick(rng, 3);
        let end = write_records(m, rng, start, n);
        if pick(rng, 2) == 0 {
            m.w8(DATA + end, 0);
        }
        m.w32(GET_PTR, if pick(rng, 6) == 0 { end } else { start });
        m.w32(SIZE, pick(rng, 5000));
        fill_record(m, RECORD_IN, rng);
        for k in 0..2 * RECORD {
            m.w8(CONTROL_DATA + k, rng.next() as u8);
        }
        // (the pad comes from the input queue's output, `Input_Fetch` copies it)
        for k in 0..8 {
            m.w8(crate::globals::QUEUE_OUTPUT.0 + k, rng.next() as u8);
            m.w8(INPUT_SIM.0 + k % 4, rng.next() as u8);
        }
        let slices = NUM_SLICES.get(m) as u32;
        for p in 0..2 {
            let car = human(m, p);
            let setup = car.car_info(m);
            setup.set_ramp_steering(m, pick(rng, 2) as i32);
            setup.set_ramp_gas(m, pick(rng, 2) as i32);
            setup.set_ramp_brake(m, pick(rng, 2) as i32);
            let flags = car.car_flags(m) as u32 & !CAR_HUMAN_COP | if pick(rng, 2) == 0 { CAR_HUMAN_COP } else { 0 };
            car.set_car_flags(m, flags as i32);
            car.set_wrongway(m, pick(rng, 2) as i32);
            car.n().sim_road_info().set_slice(m, pick(rng, slices) as i16);
        }
        // the TV cameras: the track's, some other modes and zooms; fewer or none
        for k in 0..NUM_SLOTS {
            let slot = SLOTS.at(k);
            if pick(rng, 4) == 0 {
                m.w8(slot.0, [11, 12, 4, 8][pick(rng, 4) as usize]);
            }
            let flags = m.r32(slot.0) & !0x3E00 | (pick(rng, 32) << 9);
            m.w32(slot.0, flags);
        }
        let cams = [18, 18, 0, 1, 2, pick(rng, 19)][pick(rng, 6) as usize];
        m.w32(NUM_VALID_CAMS, cams);
        for p in 0..2 {
            let cam = CAMERAS.at(p);
            cam.set_default_camera(m, pick(rng, 2) as i32);
            cam.set_time_counter(m, pick(rng, 100) as i32);
            cam.set_camera_mode(m, [CAMERA_TV, CAMERA_TV, 4, pick(rng, 21) as i32][pick(rng, 4) as usize]);
            cam.set_cut_to_next_camera(m, pick(rng, cams.max(1)) as i32);
        }
        SIM_GLOBAL.set_game_started(m, pick(rng, 2) as i32);
        SIM_VAR.set_quick_pause_sim(m, (pick(rng, 4) == 0) as i32);
        let ui = REPLAY_INTERFACE;
        ui.set_pause(m, pick(rng, 2) as i32);
        ui.set_speed(m, pick(rng, 4) as i32);
        ui.set_end(m, pick(rng, 2) as i32);
        ui.set_camera(m, pick(rng, 9) as i32);
        ui.set_selection(m, [pick(rng, 5), pick(rng, 7)][pick(rng, 2) as usize] as i32);
        ui.set_depressed(m, [0, 0, 0, 1, 2][pick(rng, 5) as usize]);
        ui.set_change_camera(m, pick(rng, 2) as i32);
        ui.set_stats_screen(m, (pick(rng, 3) == 0) as i32);
        let tick = m.r32(SIM_TICK);
        for t in [tick, tick.wrapping_sub(1)] {
            let mut bits = 0;
            for b in [KEY_LEFT, KEY_RIGHT, KEY_ACCEPT, KEY_BACK, KEY_STATS] {
                if pick(rng, 4) == 0 {
                    bits |= 1 << b;
                }
            }
            m.w32(INPUT_RESULTS + 4 * (t & 31), bits);
        }
        m.w32(STATS_TIMER, rng.next() as u32);
        m.w32(STATS_TIMER + 4, rng.next() as u32);
        // a camera file: 32 slots, some unused (no field of view)
        for k in 0..NUM_SLOTS {
            let slot = CameraCamSlot(FILE + k * CameraCamSlot::SIZE);
            for o in (0..CameraCamSlot::SIZE).step_by(4) {
                m.w32(slot.0 + o, rng.next() as u32 & 0x00FF_FFFF);
            }
            m.w8(slot.0, [11, 12, 4, 9][pick(rng, 4) as usize]);
            slot.set_fov(m, [0, 190, 466][pick(rng, 3) as usize]);
            slot.set_slice(m, [pick(rng, slices) as i16, -1][(pick(rng, 4) == 0) as usize]);
        }
        // a part to decode: well coded, or any bytes
        for k in 0..64 {
            m.w8(PART_IN + k, rng.next() as u8);
        }
        m.w8(PART_IN, pick(rng, 41) as u8);
        if pick(rng, 2) == 0 {
            compress(m, RECORD_IN + PART * pick(rng, 4));
            for k in 0..40 {
                let b = m.r8(COMPRESSED + k);
                m.w8(PART_IN + k, b);
            }
        }
    }

    /// A slice near one of the TV cameras (for a cut), or anywhere.
    fn near_camera(c: &mut Cpu, rng: &mut Rng) -> u32 {
        let m = &mut c.m;
        let cams = m.r32(NUM_VALID_CAMS).max(1);
        let slice = SLOTS.at(pick(rng, cams)).slice(m) as i32;
        let slices = NUM_SLICES.get(m) as u32;
        match pick(rng, 4) {
            0 => pick(rng, slices),
            1 => slice as u32,
            _ => slice.wrapping_add(pick(rng, 501) as i32 - 250) as u32,
        }
    }

    /// The controls taking a press: none held off, not on the stats screen, one key pressed
    /// (alone), any item and speed.
    fn one_key(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let ui = REPLAY_INTERFACE;
        ui.set_depressed(m, 0);
        ui.set_stats_screen(m, 0);
        ui.set_selection(m, pick(rng, ITEMS as u32) as i32);
        ui.set_speed(m, pick(rng, 4) as i32);
        let tick = m.r32(SIM_TICK);
        let key = [KEY_LEFT, KEY_RIGHT, KEY_ACCEPT, KEY_ACCEPT, KEY_BACK, KEY_STATS][pick(rng, 6) as usize];
        m.w32(INPUT_RESULTS + 4 * (tick & 31), 1 << key);
        m.w32(INPUT_RESULTS + 4 * (tick.wrapping_sub(1) & 31), 0);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Replay_Compress", 0x800b_4920, nfs4rc::f_800b4920, |_, r| vec![RECORD_IN + PART * pick(r, 4) + [0, 0, pick(r, 32)][pick(r, 3) as usize]]),
            ("Replay_Decompress", 0x800b_49e4, nfs4rc::f_800b49e4, |c, r| vec![[PART_IN, DATA + c.m.r32(GET_PTR)][pick(r, 2) as usize]]),
            ("Replay_InitReplay", 0x800b_4a74, nfs4rc::f_800b4a74, |_, _| vec![]),
            ("Replay_ResetReplay", 0x800b_4c18, nfs4rc::f_800b4c18, |_, _| vec![]),
            ("Replay_StoringReplay", 0x800b_4d70, nfs4rc::f_800b4d70, |_, _| vec![]),
            ("Replay_StoringControllerData", 0x800b_4d90, nfs4rc::f_800b4d90, |c, _| (0..32).map(|k| c.m.r32(RECORD_IN + 4 * k)).collect()),
            ("Replay_RetreivingControllerData", 0x800b_515c, nfs4rc::f_800b515c, |_, _| vec![RECORD_OUT]),
            ("Replay_SaveInput", 0x800b_54f8, nfs4rc::f_800b54f8, |_, r| vec![pick(r, 2)]),
            ("Replay_GetInput", 0x800b_580c, nfs4rc::f_800b580c, |_, r| vec![pick(r, 2)]),
            ("Replay_SaveReplay", 0x800b_5c6c, nfs4rc::f_800b5c6c, |_, _| vec![]),
            ("Replay_LoadReplay", 0x800b_5cf8, nfs4rc::f_800b5cf8, |_, _| vec![]),
            ("Replay_DoReplay", 0x800b_5d00, nfs4rc::f_800b5d00, |c, _| vec![c.m.r32(HUMAN_RACE_CARS)]),
            ("Replay_GetInterfaceKey", 0x800b_5d44, nfs4rc::f_800b5d44, |c, r| {
                if pick(r, 2) == 0 {
                    one_key(c, r);
                }
                vec![]
            }),
            ("Replay_LoadCameraFile", 0x800b_61d4, nfs4rc::f_800b61d4, |c, r| {
                if pick(r, 3) != 0 {
                    c.m.w32(NUM_VALID_CAMS, 0);
                }
                vec![]
            }),
            ("Replay_ReplayChooseCamera", 0x800b_6540, nfs4rc::f_800b6540, |c, r| {
                if pick(r, 2) == 0 {
                    GAME_SETUP.set_race_type(&mut c.m, [RACE_TYPE_COP_A, RACE_TYPE_COP_B][pick(r, 2) as usize]);
                }
                vec![pick(r, 2), near_camera(c, r)]
            }),
            ("Replay_ReplayFindClosestCamera", 0x800b_67e8, nfs4rc::f_800b67e8, |c, r| vec![pick(r, 2), near_camera(c, r)]),
        ]
    }

    /// `Replay_LoadCameraFile` from its file loaded at a0 (the test CPUs find no file): the
    /// original entered at the instruction after `loadfileadrz`, its frame and v0 as it has them
    /// there; the port's [`install_cameras`] after the same big file closed.
    fn loaded_original(c: &mut Cpu) {
        let sp = c.r[29].wrapping_sub(144);
        c.r[29] = sp;
        c.m.w32(sp + 140, c.r[31]);
        c.m.w32(sp + 136, c.r[16]);
        c.m.w32(sp + 128, 0);
        c.r[2] = c.r[4];
        nfs4rc::f_800b61d4_at(c, 0x800b_6260);
    }

    fn loaded_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        c.m.w32(NUM_VALID_CAMS, 0);
        call_addr(c, DEL_BIG_FILE, &[0, BIG_FILE_PRIORITY]);
        install_cameras(c, a[0]);
        None
    }

    #[test]
    fn replay_camera_file_fuzz() {
        against_original("Replay_LoadCameraFile (loaded)", 300, 0xAF1, shuffle, loaded_original, &|_| vec![FILE], loaded_port);
    }

    #[test]
    fn replay_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xAE0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let n = match name {
                "Replay_GetInterfaceKey" => 3000,
                "Replay_ReplayChooseCamera" => 2000,
                _ => 300,
            };
            against_original(name, n, 0xAE1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
