//! Run the recompiled NFS4.EXE race simulation: boot straight into a race (no front end) by
//! loading the executable image, running the system start-up `main` performs, filling
//! `GameSetup_gData` and calling `Nfs2_GameModuleStartUp`; then run the original main loop on
//! its own thread, one 60 Hz frame per [`RaceThread::step`].

use super::host::GameHost;
use super::{Cpu, Mem, new_cpu};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Retail addresses (SLUS-00826).
pub mod addr {
    pub const GP: u32 = 0x8013_D83C;
    pub const STACK_TOP: u32 = 0x801F_FFF0;
    pub const BSS_START: u32 = 0x8013_F084;
    pub const BSS_END: u32 = 0x8014_9E40;
    /// `GameSetup_gData` (GameSetup_tData, 0xA28 bytes).
    pub const GAMESETUP: u32 = 0x8011_44A4;
    pub const GAMESETUP_SIZE: usize = 0xA28;
    pub const INIT_A: u32 = 0x800E_4C30;
    pub const INIT_B: u32 = 0x800D_D05C;
    pub const NFS2_SYSTEM_N_LIB_STARTUP: u32 = 0x800A_4950;
    pub const LOAD_FRONTEND_OVERLAY: u32 = 0x800A_506C;
    pub const NFS2_GAME_MODULE_STARTUP: u32 = 0x800A_4D1C;
    pub const RACE_RESTART: u32 = 0x800A_4A0C;
    /// `Sim_MainGameLoop`.
    pub const GAME_LOOP: u32 = 0x800B_7A20;
    /// `Cars_gList`: every simulated Car_tObj*, in `carInfo` order (player first).
    pub const CAR_LIST: u32 = 0x8011_0CA0;
    /// Start sequence stage read by `Hud_Render321Go`: 0 = intro, 1..3 = lights, 4 = go.
    pub const COUNTDOWN_STAGE: u32 = 0x8013_D930;
    /// 128 Hz timer tick counter.
    pub const TICKS: u32 = 0x8013_EF8C;
    /// `Camera_gInfo`: per-player camera state (0x110 bytes each).
    pub const CAMERA_INFO: u32 = 0x8011_0570;
    /// Camera projection distance set by `SetCameraZoom` (gp + 0x294).
    pub const CAMERA_ZOOM: u32 = 0x8013_DAD0;
    /// `simGlobal.counter`: simulation time in 128 Hz ticks (physics runs on even counts).
    pub const SIM_COUNTER: u32 = 0x8011_F368;
}

/// `GameSetup_tData` field offsets.
mod gs {
    pub const NUM_LAPS: usize = 0x004;
    pub const TRAFFIC_DENSITY: usize = 0x018;
    pub const REVERSE_TRACK: usize = 0x030;
    pub const TRACK: usize = 0x03C;
    pub const WEATHER: usize = 0x048;
    pub const TIME: usize = 0x054;
    pub const NUM_CARS: usize = 0x3C4;
    pub const NUM_PLAYER_RACE_CARS: usize = 0x3C8;
    pub const NUM_OPPONENT_RACE_CARS: usize = 0x3CC;
    pub const CAR_INFO: usize = 0x3D4;
    pub const CAR_INFO_SIZE: usize = 0xB4;
    pub const CAR_TYPE: usize = 0x00;
    /// Bit mask: 1 = human race car, 2 = AI race car (4 = traffic).
    pub const CAR_CLASS: usize = 0x04;
    pub const STARTING_POS: usize = 0x54;
}

/// A console single-race GameSetup (Durham Road, one player car) used as the base for every
/// race; [`RaceSetup::game_setup`] patches the track, conditions and cars.
const GAMESETUP_TEMPLATE: &[u8] = include_bytes!("gamesetup.bin");

/// Car file codes indexed by the game's car type (`GameSetup_tCarData::carType`).
const CAR_TYPES: &[&str] = &[
    "MSLK", "BMWZ", "", "", "CMRO", "TRAM", "ADB7", "JXKR", "BMWM", "CORV", "F550", "P993", "F375", "DIAB", "MCLK", "MCF1", "X993", "", "XCF1",
    "BNS1", "BNS2",
];

pub fn car_type(code: &str) -> Option<u32> {
    CAR_TYPES.iter().position(|c| !c.is_empty() && c.eq_ignore_ascii_case(code)).map(|i| i as u32)
}

/// What to race: track number (ZTRnn), conditions variant ("" day, "N" night, "W" wet,
/// "S" snow), cars by file code (player first) and lap count.
#[derive(Clone, Debug)]
pub struct RaceSetup {
    pub track: u32,
    pub variant: String,
    pub cars: Vec<String>,
    pub laps: u32,
}

impl RaceSetup {
    pub fn game_setup(&self) -> Result<Vec<u8>> {
        let mut d = GAMESETUP_TEMPLATE.to_vec();
        let put = |d: &mut Vec<u8>, o: usize, v: i32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut d, gs::TRACK, self.track as i32);
        put(&mut d, gs::NUM_LAPS, self.laps.max(1) as i32);
        put(&mut d, gs::REVERSE_TRACK, 0);
        put(&mut d, gs::TRAFFIC_DENSITY, 0);
        let (weather, time) = match self.variant.as_str() {
            "N" => (0, 1),
            "W" => (1, 0),
            "S" => (1, 1),
            _ => (0, 0),
        };
        put(&mut d, gs::WEATHER, weather);
        put(&mut d, gs::TIME, time);
        let n = self.cars.len();
        if n == 0 || n > 8 {
            bail!("need 1..8 cars, got {n}");
        }
        put(&mut d, gs::NUM_CARS, n as i32);
        put(&mut d, gs::NUM_PLAYER_RACE_CARS, 1);
        put(&mut d, gs::NUM_OPPONENT_RACE_CARS, n as i32 - 1);
        for (k, code) in self.cars.iter().enumerate() {
            let Some(t) = car_type(code) else { bail!("car {code} is not a race car of the original game") };
            let o = gs::CAR_INFO + k * gs::CAR_INFO_SIZE;
            if k > 0 {
                // opponents: the player's car settings with another car
                d.copy_within(gs::CAR_INFO..gs::CAR_INFO + gs::CAR_INFO_SIZE, o);
            }
            put(&mut d, o + gs::CAR_TYPE, t as i32);
            put(&mut d, o + gs::CAR_CLASS, if k == 0 { 1 } else { 2 });
            // grid slot (10 track slices apart); the player starts at the back
            put(&mut d, o + gs::STARTING_POS, if k == 0 { n as i32 - 1 } else { k as i32 - 1 });
        }
        Ok(d)
    }
}

/// Run `f` with a fresh stack frame; returns Err with the call stack on a panic.
pub fn call(cpu: &mut Cpu, f: u32, args: &[u32]) -> Result<u32> {
    for (i, a) in args.iter().enumerate() {
        cpu.r[4 + i] = *a;
    }
    cpu.r[29] = addr::STACK_TOP - 0x100;
    cpu.r[30] = cpu.r[29];
    cpu.r[28] = addr::GP;
    cpu.r[31] = 0;
    cpu.stack.clear();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cpu.call(f)));
    match res {
        Ok(()) => Ok(cpu.r[2]),
        Err(e) if e.is::<super::host::QuitGame>() => Ok(0),
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            let names = names();
            let stack: Vec<String> =
                cpu.stack.iter().rev().take(14).map(|a| format!("{a:#x} {}", names.get(a).map(String::as_str).unwrap_or("?"))).collect();
            bail!("{f:#x} failed: {msg}; stack {}", stack.join(" <- "))
        }
    }
}

/// Optional developer names (re/retail_syms.tsv, produced by tools/symmatch.py) for diagnostics.
pub fn names() -> std::collections::HashMap<u32, String> {
    std::fs::read_to_string("re/retail_syms.tsv")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((u32::from_str_radix(it.next()?, 16).ok()?, it.next()?.to_string()))
        })
        .collect()
}

/// A fresh 2 MB RAM image with NFS4.EXE loaded and the host attached.
fn load(data_root: &Path) -> Result<Cpu> {
    let exe = std::fs::read(data_root.join("NFS4.EXE"))?;
    let mut mem = Mem::default();
    mem.load_exe(&exe);
    for a in (addr::BSS_START..addr::BSS_END).step_by(4) {
        mem.w32(a, 0);
    }
    let mut cpu = new_cpu(mem);
    cpu.host = Some(Box::new(GameHost::new(data_root.to_path_buf())));
    cpu.trace = std::env::var("PS1_TRACE").is_ok();
    Ok(cpu)
}

/// Boot NFS4.EXE into a race: the start-up `main` performs, then the race module.
pub fn boot(data_root: &Path, gamesetup: &[u8]) -> Result<Cpu> {
    let mut cpu = load(data_root)?;
    cpu.tick_limit = 300_000_000;
    call(&mut cpu, addr::INIT_A, &[])?;
    call(&mut cpu, addr::INIT_B, &[])?;
    call(&mut cpu, addr::NFS2_SYSTEM_N_LIB_STARTUP, &[])?;
    call(&mut cpu, addr::LOAD_FRONTEND_OVERLAY, &[])?;
    cpu.m.write_bytes(addr::GAMESETUP, &gamesetup[..addr::GAMESETUP_SIZE.min(gamesetup.len())]);
    call(&mut cpu, addr::NFS2_GAME_MODULE_STARTUP, &[addr::GAMESETUP])?;
    call(&mut cpu, addr::RACE_RESTART, &[])?;
    Ok(cpu)
}

/// One car as the original simulation sees it (PS1 world space, 16.16 fixed point, metres).
#[derive(Clone, Debug, Default)]
pub struct CarState {
    pub addr: u32,
    pub code: String,
    pub position: [i32; 3],
    pub velocity: [i32; 3],
    /// Rows: right, up, forward (1.0 = 0x10000).
    pub orient: [i32; 9],
    /// Ground speed (m/s, 16.16).
    pub speed: i32,
    pub rpm: i32,
    pub gear: i8,
    /// `control.steering`.
    pub steering: i32,
    pub lap: i32,
}

impl CarState {
    fn read(m: &mut Mem, car: u32) -> Self {
        let mut v3 = |o: u32| [0, 1, 2].map(|i| m.r32(car + o + 4 * i) as i32);
        let (position, velocity) = (v3(0xA0), v3(0xAC));
        let code: String = (0..4).map(|i| m.r8(car + 0x240 + i)).take_while(|&b| b != 0).map(char::from).collect();
        Self {
            addr: car,
            code,
            position,
            velocity,
            orient: std::array::from_fn(|i| m.r32(car + 0xF0 + 4 * i as u32) as i32),
            speed: m.r32(car + 0xC0) as i32,
            rpm: m.r32(car + 0x468) as i32,
            gear: m.r8(car + 0x442) as i8,
            steering: m.r32(car + 0x450) as i32,
            lap: m.r32(car + 0x264) as i32,
        }
    }
}

/// Simulation state at the top of a frame.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Frames run so far.
    pub frame: u32,
    /// 60 Hz field number of the virtual clock.
    pub field: u64,
    pub cars: Vec<CarState>,
    /// See [`addr::COUNTDOWN_STAGE`].
    pub countdown: u8,
    pub ticks: u32,
    /// Simulation time of the car states, see [`addr::SIM_COUNTER`].
    pub sim_ticks: u32,
    /// The player view's camera as the game computed it for the last rendered frame
    /// (`DRender_tCalcView` translation and rotation).
    pub camera_pos: [i32; 3],
    pub camera_rot: [i32; 9],
    /// Player 0 camera mode (`Camera_gInfo[0].mode`, cycled by Triangle).
    pub camera_mode: i16,
    /// Projection distance H of the camera (`SetCameraZoom`), in screen pixels.
    pub projection: u32,
    /// Display area (x, y, w, h) from PutDispEnv.
    pub display: [u16; 4],
    /// Full RAM copy, when requested.
    pub ram: Option<Vec<u8>>,
    /// The picture the PS1 displays (width, height, RGBA8).
    pub image: std::sync::Arc<(u32, u32, Vec<u8>)>,
}

/// Input for one frame.
#[derive(Clone, Copy, Default)]
pub struct FrameInput {
    pub buttons: u16,
    /// Include a RAM copy in the resulting snapshot.
    pub want_ram: bool,
}

fn snapshot(c: &mut Cpu, frame: u32, info: super::host::FrameInfo, want_ram: bool) -> Snapshot {
    let mut cars = Vec::new();
    for k in 0..9 {
        let car = c.m.r32(addr::CAR_LIST + 4 * k);
        if car == 0 {
            break;
        }
        cars.push(CarState::read(&mut c.m, car));
    }
    Snapshot {
        frame,
        field: info.field,
        cars,
        countdown: c.m.r8(addr::COUNTDOWN_STAGE),
        ticks: c.m.r32(addr::TICKS),
        sim_ticks: c.m.r32(addr::SIM_COUNTER),
        camera_pos: std::array::from_fn(|i| c.m.r32(super::host::RENDER_VIEW + 8 + 4 * i as u32) as i32),
        projection: c.m.r32(addr::CAMERA_ZOOM),
        camera_mode: c.m.r16(addr::CAMERA_INFO + 0x70) as i16,
        display: info.display,
        image: info.image.clone(),
        camera_rot: std::array::from_fn(|i| c.m.r32(super::host::RENDER_VIEW + 0x14 + 4 * i as u32) as i32),
        ram: want_ram.then(|| c.m.ram.to_vec()),
    }
}

/// How to start the original game.
pub enum Boot {
    /// Straight into a race described by a GameSetup (see [`RaceSetup::game_setup`]).
    Race(Vec<u8>),
    /// The whole game from `main`: front end, races and everything in between.
    Main,
}

/// `main`.
const MAIN: u32 = 0x800A_52A4;

/// The original game running on its own thread in lockstep with the caller: each
/// [`step`](Self::step) runs exactly one game frame with the given input.
pub struct RaceThread {
    input: mpsc::SyncSender<Option<FrameInput>>,
    output: mpsc::Receiver<Result<Snapshot, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// State after boot / the last step.
    pub last: Snapshot,
}

impl RaceThread {
    /// Boot the race described by `gamesetup` and wait until it is ready for its first frame.
    pub fn start(data_root: PathBuf, gamesetup: Vec<u8>) -> Result<Self> {
        Self::start_with(data_root, Boot::Race(gamesetup))
    }

    /// Start the game and wait until its first frame is ready.
    pub fn start_with(data_root: PathBuf, how: Boot) -> Result<Self> {
        let (in_tx, in_rx) = mpsc::sync_channel::<Option<FrameInput>>(1);
        let (out_tx, out_rx) = mpsc::sync_channel::<Result<Snapshot, String>>(1);
        let thread = std::thread::Builder::new().name("nfs4-sim".into()).stack_size(256 << 20).spawn(move || {
            let booted = match &how {
                Boot::Race(gs) => boot(&data_root, gs),
                Boot::Main => load(&data_root),
            };
            let mut cpu = match booted {
                Ok(c) => c,
                Err(e) => {
                    let _ = out_tx.send(Err(format!("{e:#}")));
                    return;
                }
            };
            let out = out_tx.clone();
            let mut frame = 0u32;
            let mut want_ram = false;
            let mut quit = false;
            let on_frame: super::host::FrameFn = Box::new(move |c: &mut Cpu, info: super::host::FrameInfo| {
                // report the state after the previous frame (or boot), then wait for input
                if quit || out.send(Ok(snapshot(c, frame, info, want_ram))).is_err() {
                    return None;
                }
                let Some(input) = in_rx.recv().ok().flatten() else {
                    quit = true;
                    return None;
                };
                frame += 1;
                want_ram = input.want_ram;
                Some(input.buttons)
            });
            let host = cpu.host.as_mut().expect("host");
            // the host is always our GameHost
            let host = unsafe { &mut *(host.as_mut() as *mut dyn super::Host as *mut GameHost) };
            host.frame.on_frame = Some(on_frame);
            cpu.m.w32(super::host::MAIN_LOOP_EXIT_FLAG, 0);
            cpu.tick_limit = u64::MAX;
            let entry = match how {
                Boot::Race(_) => addr::GAME_LOOP,
                Boot::Main => MAIN,
            };
            if let Err(e) = call(&mut cpu, entry, &[]) {
                let _ = out_tx.send(Err(format!("{e:#}")));
            }
        })?;
        let last = match out_rx.recv() {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => bail!("{e}"),
            Err(_) => bail!("simulation thread ended during boot"),
        };
        Ok(Self { input: in_tx, output: out_rx, thread: Some(thread), last })
    }

    /// Run one 60 Hz frame with `input` held.
    pub fn step(&mut self, input: FrameInput) -> Result<&Snapshot> {
        if self.input.send(Some(input)).is_err() {
            bail!("simulation stopped");
        }
        match self.output.recv() {
            Ok(Ok(s)) => {
                self.last = s;
                Ok(&self.last)
            }
            Ok(Err(e)) => bail!("{e}"),
            Err(_) => bail!("simulation stopped"),
        }
    }
}

impl Drop for RaceThread {
    fn drop(&mut self) {
        // tell the game loop to exit, then wait for the thread
        let _ = self.input.send(None);
        while self.output.try_recv().is_ok() {}
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// PS1 pad buttons (active-high masks for [`FrameInput::buttons`]).
pub mod pad {
    pub const SELECT: u16 = 1 << 0;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;
}
