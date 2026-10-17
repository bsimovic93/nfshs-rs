//! Run the recompiled NFS4.EXE race simulation: boot straight into a race (no front end) by
//! loading the executable image, running the system start-up `main` performs, filling
//! `GameSetup_gData` and calling `Nfs2_GameModuleStartUp`; then run the original main loop on
//! its own thread, one 60 Hz frame per [`RaceThread::step`].

use super::host::GameHost;
use super::{Cpu, Mem, new_cpu};
use anyhow::{Context, Result, bail};
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
    /// `reservememadr(name, size, pool)`: the game's heap allocator.
    pub const RESERVE_MEM: u32 = 0x800E_5F28;
    /// A string in the executable, for naming allocations.
    pub const SOME_NAME: u32 = 0x8001_1550;
    /// `Sim_MainGameLoop`.
    pub const GAME_LOOP: u32 = 0x800B_7A20;
    /// `main`: start-up, then front end and races for ever.
    pub const MAIN: u32 = 0x800A_52A4;
    /// `simVar`: set when the race is to be restarted (pause menu "Restart"); main then
    /// resets the game and runs the loop again.
    pub const SIM_VAR: u32 = 0x8011_F37C;
    /// `gSimQueue_BlockSelf`: the simulation is held (paused).
    pub const SIM_PAUSED: u32 = 0x8013_E71C;
    /// `Cars_gList`: every simulated Car_tObj*, in `carInfo` order (player first).
    pub const CAR_LIST: u32 = 0x8011_0CA0;
    /// The human players' cars (`Car_tObj *[2]`).
    pub const HUMAN_CARS: u32 = 0x8011_0D0C;
    /// Start sequence stage read by `Hud_Render321Go`: 0 = intro, 1..3 = lights, 4 = go.
    pub const COUNTDOWN_STAGE: u32 = 0x8013_D930;
    /// 128 Hz timer tick counter.
    pub const TICKS: u32 = 0x8013_EF8C;
    /// `Camera_gInfo`: per-player camera state (0x110 bytes each).
    pub const CAMERA_INFO: u32 = 0x8011_0570;
    /// Camera projection distance set by `SetCameraZoom` (gp + 0x294).
    pub const CAMERA_ZOOM: u32 = 0x8013_DAD0;
    /// `Chunk_lightTable`: the track's vertex colours (u32 RGB, indexed by the vertex "shade").
    pub const LIGHT_TABLE: u32 = 0x8013_DB0C;
    /// `DashHUD_gInfo`: the HUD's values for this frame (computed by DashHUD_HUDCalc).
    pub const DASHHUD: u32 = 0x8011_39E8;
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
    /// The paint (0..15; R3DCar_Instantiate3DCar keeps the low 4 bits at Car_tObj+0x840).
    pub const CAR_COLOUR: usize = 0x0C;
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

/// The police cars' codes, car types 22 on (after the race cars in the game's table): the hot
/// pursuit car select shows them. CHSV has no model on the disc.
const POLICE_TYPES: &[&str] = &["CCPR", "", "CBMW", "CCOV", "CCMR", "CDBL"];

/// The player's licence plate text (GameSetup carInfo[0] + 0x68) of the race setup.
pub fn player_plate() -> String {
    let o = gs::CAR_INFO + 0x68;
    GAMESETUP_TEMPLATE[o..o + 12].iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}

/// The car file code of a car type (the inverse of [`car_type`], which knows only the race
/// cars), police cars included.
pub fn car_code(car_type: u32) -> Option<&'static str> {
    let t = car_type as usize;
    CAR_TYPES.get(t).or_else(|| POLICE_TYPES.get(t.checked_sub(22)?)).copied().filter(|c| !c.is_empty())
}

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
        // NFSHS_TRAFFIC=N: traffic for headless runs at density N (the test drive's 2): the
        // template's traffic cars follow the race cars (the front end sets up its own)
        let traffic: i32 = std::env::var("NFSHS_TRAFFIC").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        put(&mut d, gs::TRAFFIC_DENSITY, traffic);
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
            // NFSHS_COLOUR=n: the player's paint in headless races
            if let (0, Some(c)) = (k, std::env::var("NFSHS_COLOUR").ok().and_then(|v| v.parse::<i32>().ok())) {
                put(&mut d, o + gs::CAR_COLOUR, c);
            }
            put(&mut d, o + gs::CAR_CLASS, if k == 0 { 1 } else { 2 });
            // grid slot (10 track slices apart); the player starts at the back
            put(&mut d, o + gs::STARTING_POS, if k == 0 { n as i32 - 1 } else { k as i32 - 1 });
        }
        if traffic > 0 {
            let template_cars = i32::from_le_bytes(GAMESETUP_TEMPLATE[gs::NUM_CARS..gs::NUM_CARS + 4].try_into()?) as usize;
            let mut total = n;
            for k in 1..template_cars {
                let src = gs::CAR_INFO + k * gs::CAR_INFO_SIZE;
                let class = i32::from_le_bytes(GAMESETUP_TEMPLATE[src + gs::CAR_CLASS..src + gs::CAR_CLASS + 4].try_into()?);
                if class != 4 || total >= 8 {
                    continue;
                }
                let dst = gs::CAR_INFO + total * gs::CAR_INFO_SIZE;
                d[dst..dst + gs::CAR_INFO_SIZE].copy_from_slice(&GAMESETUP_TEMPLATE[src..src + gs::CAR_INFO_SIZE]);
                total += 1;
            }
            put(&mut d, gs::NUM_CARS, total as i32);
        }
        // GS_SET="hexoff=value,..." (debug): poke GameSetup words (a police car as the player:
        // 3d4=27; the next car's type is at 488)
        for kv in std::env::var("GS_SET").unwrap_or_default().split(',').filter(|s| !s.is_empty()) {
            let (o, v) = kv.split_once('=').context("GS_SET: off=value")?;
            let o = usize::from_str_radix(o, 16)?;
            d[o..o + 4].copy_from_slice(&v.parse::<i32>()?.to_le_bytes());
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

/// Boot NFS4.EXE into a race: a fresh 2 MB RAM image and the start-up `main` performs.
pub fn boot(data_root: &Path, gamesetup: &[u8]) -> Result<Cpu> {
    let exe = std::fs::read(data_root.join("NFS4.EXE"))?;
    let mut mem = Mem::default();
    mem.load_exe(&exe);
    for a in (addr::BSS_START..addr::BSS_END).step_by(4) {
        mem.w32(a, 0);
    }
    let mut cpu = new_cpu(mem);
    cpu.host = Some(Box::new(GameHost::new(data_root.to_path_buf())));
    cpu.trace = std::env::var("PS1_TRACE").is_ok();
    cpu.tick_limit = 300_000_000;
    // busy-waits on the timer (a race and its loading stay far below this between host
    // calls: the race runs exactly as it always has)
    cpu.spin_limit = 1_000_000;
    call(&mut cpu, addr::INIT_A, &[])?;
    call(&mut cpu, addr::INIT_B, &[])?;
    call(&mut cpu, addr::NFS2_SYSTEM_N_LIB_STARTUP, &[])?;
    call(&mut cpu, addr::LOAD_FRONTEND_OVERLAY, &[])?;
    cpu.m.write_bytes(addr::GAMESETUP, &gamesetup[..addr::GAMESETUP_SIZE.min(gamesetup.len())]);
    // the module takes a list of setting changes (field, player, value; 0-terminated) that
    // GameSetup_StartUp applies to GameSetup_gData and then frees: the setup is already in
    // place, so pass an empty list from the game's heap
    let changes = call(&mut cpu, addr::RESERVE_MEM, &[addr::SOME_NAME, 16, 0])?;
    if changes == 0 {
        bail!("reservememadr failed");
    }
    for i in 0..4 {
        cpu.m.w32(changes + 4 * i, 0);
    }
    if std::env::var_os("PS1_HEAPLOG").is_some() {
        let free = call(&mut cpu, 0x800E_5E70, &[])?;
        eprintln!("largest free block before the game module: {free:#x}");
    }
    call(&mut cpu, addr::NFS2_GAME_MODULE_STARTUP, &[changes])?;
    call(&mut cpu, addr::RACE_RESTART, &[])?;
    // main clears the restart request before entering the game loop (it blocks pausing)
    cpu.m.w32(addr::SIM_VAR, 0);
    Ok(cpu)
}

/// Power on: NFS4.EXE in a fresh 2 MB RAM image, ready to run `main`.
pub fn power_on(data_root: &Path) -> Result<Cpu> {
    let exe = std::fs::read(data_root.join("NFS4.EXE"))?;
    let mut mem = Mem::default();
    mem.load_exe(&exe);
    for a in (addr::BSS_START..addr::BSS_END).step_by(4) {
        mem.w32(a, 0);
    }
    let mut cpu = new_cpu(mem);
    cpu.host = Some(Box::new(GameHost::new(data_root.to_path_buf())));
    cpu.trace = std::env::var("PS1_TRACE").is_ok();
    // busy-waits on the timer (race frames stay below 3k backward branches between host
    // calls, loading below 70k: a few ticks then pass early, as interrupts would)
    cpu.spin_limit = 20_000;
    Ok(cpu)
}

/// The headlight beam of night driving (Night_SetEnviroment's parameters, kept in the draw
/// cache): a vertex at (x, z) in the beam's space (the player car's axes, 1/64 m) with
/// z0 <= z < z0 + 2^(zs+6) and |x| < 2^(xs+5) takes the level map[(z - z0) >> zs][(x + 2^(xs+5)) >> xs]
/// (0..15) and its colour index s becomes table[s][level].
#[derive(Clone, Debug, Default)]
pub struct NightBeam {
    /// Bit 0: the player's headlights are on.
    pub flags: u8,
    pub z0: i32,
    pub xs: u32,
    pub zs: u32,
    /// 64 x 64 levels (Night's beam map, 0x8013ED20).
    pub map: Vec<u8>,
    /// 256 x 16 colour indices (Night_GenerateAllLightTables, 0x8013ED28).
    pub table: Vec<u8>,
}

impl NightBeam {
    /// The beam for `player`'s view.
    pub(crate) fn read(c: &mut Cpu, player: u32) -> Option<Self> {
        let (map, table) = (c.m.r32(0x8013_ED20), c.m.r32(0x8013_ED28));
        if map == 0 || table == 0 || c.m.r32(0x8011_44F8) == 0 {
            return None;
        }
        let bytes = |c: &mut Cpu, a: u32, n: u32| (0..n).map(|i| c.m.r8(a + i)).collect::<Vec<u8>>();
        // (DrawW sets the draw cache's copies per chunk: headlights on when the player's car
        // has them on, Car_tObj+1095 bits 1..2; the shape from Night_SetEnviroment)
        let player = c.m.r32(addr::CAR_LIST + 4 * player);
        Some(Self {
            flags: u8::from(player != 0 && c.m.r8(player + 1095) & 6 != 0),
            z0: c.m.r16(0x8013_ED10) as i16 as i32,
            xs: c.m.r8(0x8013_ED18) as u32,
            zs: c.m.r8(0x8013_ED1C) as u32,
            map: bytes(c, map, 4096),
            table: bytes(c, table, 4096),
        })
    }
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
    /// `N.dimension`: half extents of the body box (x, y, z); `position` is its centre.
    pub dimension: [i32; 3],
    /// Ground speed (m/s, 16.16).
    pub speed: i32,
    pub rpm: i32,
    pub gear: i8,
    /// `control.steering`.
    pub steering: i32,
    pub lap: i32,
    /// The paint as `R3DCar_Instantiate3DCar` reads it: the colour (Car_tObj+0x840, 0..15:
    /// bit 3 picks the "d" body sheet) in the low 4 bits, the car's palette flags
    /// (Car_tObj+0x8B3 bits 0 and 1: the second half of the body sheet's and the other
    /// sheets' palettes) in bits 4 and 5. See [`crate::assets::load_car`].
    pub colour: u8,
    /// Car_tObj+0x570 bit 4: a traffic car parked out of the race (waiting to enter).
    pub parked: bool,
    /// `wheelRot` (front, rear): the wheels' turn as R3DCar_InsertCarFacet keeps it (65536 =
    /// a full turn; it adds at most 6144 a frame), drawn with fixedxformx.
    pub wheel_rot: [u16; 2],
    /// The front wheels' steering as drawn (fixedxformy angle, 65536 = a full turn):
    /// `control.steering` * 32, reduced in proportion above 30 (Car_tObj+0x420).
    pub wheel_steer: i32,
}

impl CarState {
    pub fn read(m: &mut Mem, car: u32) -> Self {
        let mut v3 = |o: u32| [0, 1, 2].map(|i| m.r32(car + o + 4 * i) as i32);
        let (position, velocity, dimension) = (v3(0xA0), v3(0xAC), v3(0x134));
        let code: String = (0..4).map(|i| m.r8(car + 0x240 + i)).take_while(|&b| b != 0).map(char::from).collect();
        Self {
            addr: car,
            code,
            position,
            velocity,
            dimension,
            orient: std::array::from_fn(|i| m.r32(car + 0xF0 + 4 * i as u32) as i32),
            speed: m.r32(car + 0xC0) as i32,
            rpm: m.r32(car + 0x468) as i32,
            gear: m.r8(car + 0x442) as i8,
            steering: m.r32(car + 0x450) as i32,
            lap: m.r32(car + 0x264) as i32,
            colour: (m.r16(car + 0x840) & 15) as u8 | (m.r8(car + 0x8B3) & 3) << 4,
            parked: m.r32(car + 0x570) & 4 != 0,
            wheel_rot: [m.r32(car + 0xD0) as u16, m.r32(car + 0xD4) as u16],
            wheel_steer: {
                let (mut s, speed) = (m.r32(car + 0x450) as i32 as i64, m.r32(car + 0x420) as i32 as i64);
                if speed > 0x1E_0000 {
                    // fixeddiv(s << 16, fixedmult(speed, 2184)) >> 16
                    let d = (speed * 2184) >> 16;
                    if d != 0 {
                        s = ((s << 32) / d) >> 16;
                    }
                }
                (s * 32) as i32
            },
        }
    }
}

/// Sky and horizon as the original set them up for the track (`Hrz_InitSky`,
/// `Hrz_InitSkyColor`, `Hrz_InitHorizon`), in PS1 view units.
#[derive(Clone, Debug, Default)]
pub struct SkyData {
    /// `gSkyMesh`: 5 rings x 17 points of a radius-1000 dome (x, y, z), horizon first.
    pub dome: Vec<[i16; 3]>,
    /// `gSkyColor`: RGB per dome point.
    pub dome_colors: Vec<[u8; 3]>,
    /// Horizon ring: 17 points (x, z) of radius 32000.
    pub ring: Vec<[i16; 2]>,
    /// Horizon band bottom height and height (ring units).
    pub bottom: i16,
    pub height: i16,
    /// Vertex colours of the band's top and bottom edge, per ring point.
    pub top_colors: Vec<[u8; 3]>,
    pub bottom_colors: Vec<[u8; 3]>,
    /// Horizon texture (`& 7`: HZ01..HZ08) per segment.
    pub segment_tex: Vec<u8>,
    /// The horizon band is drawn at all (the halfword Hrz's draw checks first; off on the
    /// raceways).
    pub band: bool,
    /// Hrz_BuildSky textures the dome (the sky record's first word is 1): per quad (64, ring
    /// by ring) the pixmap index (0..11 HZ01..HZ12, 12..15 HZ09..HZ12 mirrored).
    pub dome_tex: Option<Vec<u8>>,
}

impl SkyData {
    fn read(m: &mut Mem) -> Option<Self> {
        let gp = addr::GP;
        let (hrec, ring) = (m.r32(gp + 4916), m.r32(gp + 4924));
        if hrec == 0 || ring == 0 {
            return None;
        }
        let rgb = |m: &mut Mem, a: u32| [m.r8(a), m.r8(a + 1), m.r8(a + 2)];
        Some(Self {
            dome: (0..85).map(|i| [0, 2, 4].map(|o| m.r16(0x8012_1668 + 8 * i + o) as i16)).collect(),
            dome_colors: (0..85).map(|i| rgb(m, 0x8012_1910 + 4 * i)).collect(),
            ring: (0..17).map(|i| [m.r16(ring + 8 * i) as i16, m.r16(ring + 8 * i + 4) as i16]).collect(),
            bottom: m.r16(hrec + 8) as i16,
            height: m.r16(hrec + 12) as i16,
            top_colors: (0..17).map(|i| rgb(m, 0x8012_1B28 + 4 * i)).collect(),
            bottom_colors: (0..17).map(|i| rgb(m, 0x8012_1AE8 + 4 * i)).collect(),
            segment_tex: (0..16).map(|i| m.r8(hrec + 32 + i)).collect(),
            band: m.r16(0x8012_4570) != 0,
            dome_tex: {
                let rec = m.r32(gp + 4920);
                (rec != 0 && m.r32(rec) == 1).then(|| (0..64).map(|i| m.r8(0x8012_1A64 + i)).collect())
            },
        })
    }
}

/// The HUD's pre-built data for player 1: sprite slots, panel quads, the HUD shape table,
/// the digit glyph table and the car's HUD flags (`GameSetup_tCarData` +0x70..+0x8C).
#[derive(Clone, Debug, Default)]
pub struct HudSetup {
    /// `*(gp + 6356)`: 64 SPRT slots of 20 bytes.
    pub slots: Vec<u8>,
    /// Gouraud panel quads (POLY_G4, 36 bytes each) and their flat end caps (POLY_F4, 24 bytes).
    pub panels: Vec<u8>,
    pub caps: Vec<u8>,
    /// `HudPmx_gShapes`: 175 entries (Draw_tPixMap + width, height).
    pub shapes: Vec<u8>,
    /// `HudPmx_gHudNumberUV`: uv+clut word of the digits 0..9.
    pub number_uv: [u32; 10],
    /// The font's palette (the PFN picture has none; the game builds it).
    pub font_clut: [u16; 16],
    /// GameSetup carInfo[0].
    pub car_info: Vec<u8>,
    /// The font's colour palettes (Font_TextColor 0..15).
    pub font_palettes: Vec<[u16; 16]>,
    /// Night race (GameSetup +0x54): the tachometer uses the night dial and needle colours.
    pub night: bool,
    /// Tachometer needle colour per car (Hud_BuildTach's day or night table).
    pub needle_colors: [u32; 30],
}

/// `HudPmx_gShapes`.
pub const HUD_SHAPES: u32 = 0x8011_1F5C;

impl HudSetup {
    pub fn read(m: &mut Mem, font_clut: [u16; 16], font_palettes: Vec<[u16; 16]>) -> Option<Self> {
        let slots = m.r32(addr::GP + 6356);
        if slots == 0 {
            return None;
        }
        let bytes = |m: &mut Mem, a: u32, n: u32| (0..n).map(|i| m.r8(a + i)).collect::<Vec<u8>>();
        let night = m.r32(addr::GAMESETUP + 0x54) != 0;
        Some(Self {
            slots: bytes(m, slots, 72 * 20),
            panels: bytes(m, 0x8013_FA38, 4 * 36),
            caps: bytes(m, 0x8013_F758, 4 * 24),
            shapes: bytes(m, HUD_SHAPES, 175 * 20),
            number_uv: std::array::from_fn(|i| m.r32(0x8011_1F34 + 4 * i as u32)),
            car_info: bytes(m, addr::GAMESETUP + 0x3D4, 0xB4),
            font_clut,
            font_palettes,
            night,
            needle_colors: std::array::from_fn(|i| m.r32(if night { 0x8012_1D24 } else { 0x8012_1CAC } + 4 * i as u32)),
        })
    }
}

/// One car on the track map: a HudPmx_gShapes sprite at (x, y) in the map view.
#[derive(Clone, Copy, Debug)]
pub struct MapMarker {
    pub shape: u8,
    pub x: i32,
    pub y: i32,
    /// 0xBBGGRR
    pub color: u32,
}

/// The track map view of player 1 (Hud_RenderMapView): the map polygon (POLY_FT4) and the
/// car markers (Hud_BuildMapMarkers), in the order they are linked.
#[derive(Clone, Debug, Default)]
pub struct MapView {
    pub prim: Vec<u8>,
    pub markers: Vec<MapMarker>,
}

impl MapView {
    /// Read through `r32` (word reads of main RAM), so a RAM dump works as well as the
    /// running game.
    pub fn read(r32: &mut dyn FnMut(u32) -> u32) -> Self {
        const GP: u32 = addr::GP;
        let prim = (0..10).flat_map(|i| r32(0x8013_F8A8 + 4 * i).to_le_bytes()).collect();
        let slices = r32(0x8013_DAB4);
        let (scale_x, scale_z) = (r32(GP + 5144) as i32, r32(GP + 5148) as i32);
        let (cos, sin) = (r32(GP + 5100) as i32, r32(GP + 5104) as i32);
        let (ox, oy) = (r32(GP + 5152) as i32, r32(GP + 5156) as i32 + 2);
        let mirror = r32(addr::GAMESETUP + 0x2C) != 0;
        let rmult = |a: i32, b: i32| ((a as i64 * b as i64 + 0x8000) >> 16) as i32;
        let flash = |r32: &mut dyn FnMut(u32) -> u32| if r32(0x8013_EA9C) != 0 || r32(0x8011_F38C) != 0 { 0xFF } else { 0xFF_0000 };
        let mut markers = Vec::new();
        // position on the map: the car's track slice, scaled and turned
        let place = |r32: &mut dyn FnMut(u32) -> u32, car: u32| -> Option<(i32, i32)> {
            if (r32(car + 144) >> 8) & 0xFF == 0 || scale_x == 0 || scale_z == 0 || slices == 0 {
                return None;
            }
            let slice = slices.wrapping_add(((r32(car + 8) as i16 as i32) << 5) as u32);
            let x = r32(slice) as i32 / scale_x;
            let z = r32(slice + 8) as i32 / scale_z;
            let mut mx = ox + rmult(cos, x) - rmult(sin, z);
            let my = oy + rmult(sin, x) + rmult(cos, z);
            if mirror {
                mx = -mx;
            }
            Some((22 + mx, 24 - my))
        };
        // police first, then the race cars
        for i in 0..r32(0x8013_DB00).min(16) {
            let car = r32(0x8011_0D78 + 4 * i);
            if let Some((x, y)) = place(r32, car) {
                let color = if r32(car + 1392) & 2 != 0 { flash(r32) } else { r32(0x8012_1E80 + 4 * i) };
                markers.push(MapMarker { shape: 122, x: x - 2, y, color });
            }
        }
        for i in 0..r32(0x8013_DAEC).min(16) {
            let car = r32(0x8011_0CC4 + 4 * i);
            if let Some((x, y)) = place(r32, car) {
                let flags = r32(car + 608);
                let mut color = r32(0x8012_1E50 + 4 * i);
                let (shape, dx) = if flags & 0x200 != 0 {
                    if r32(car + 1392) & 2 != 0 {
                        color = flash(r32);
                    }
                    (121, -3)
                } else if flags & 4 != 0 {
                    (121, -3)
                } else {
                    (122, -2)
                };
                markers.push(MapMarker { shape, x: x + dx, y, color });
            }
        }
        Self { prim, markers }
    }
}

/// Every chunk's glare list (`Track_chunkList[i] + 84`: count, then 16-byte entries). The
/// chunk count is not kept in a variable: callers keep the chunks the track has.
/// A car's reflection normals as the original loaded them (R3DCar's part objects for its
/// model slot and level of detail, `0x80117BC0`): per reflection-mapped part (0, 8, 10, 41,
/// 42, 45, 46) its vertices' normals, about 64 long (the GEO's are 4096 and differ).
/// A texture's substitutes (DrawC_Prim, drawing modes 8 and 9): when the polygon's light or
/// damage code (the car's [`super::host::CarFacet::codes`]) has a value 1..3 it is drawn with
/// texture `index + offset` and its UVs moved by (du, dv): the lit brake lights and
/// headlights, the dented panels.
pub type TextureSubs = [(i16, u8, u8); 3];

/// A car's texture substitutes as its model was loaded (after the 57 part pointers of
/// `0x80117BC0`): for the GEO's first 30 textures.
pub fn read_texture_subs(m: &mut Mem, car: u32) -> Vec<TextureSubs> {
    let slot = m.r16(car + 0x8BC) as i16;
    if !(0..50).contains(&slot) {
        return Vec::new();
    }
    let lod = (m.r8(car + 0x842) >> 7) as u32;
    let parts = m.r32(0x8011_7BC0 + lod * 200 + slot as u32 * 4);
    if !(0x8000_0000..0x8020_0000).contains(&parts) {
        return Vec::new();
    }
    (0..30u32)
        .map(|t| std::array::from_fn(|k| {
            let e = parts + 228 + (t * 3 + k as u32) * 4;
            (m.r16(e) as i16, m.r8(e + 2), m.r8(e + 3))
        }))
        .collect()
}

/// The track's scrolling textures as Track_AnimateTextures moved them this frame: per material
/// (index into the track's materials, the `Track_tMaterial` array at gp+3960) of a type 0x80
/// controller (16 bytes each at gp+3988, count gp+3984), how many texels its pixmap's V
/// coordinates are past the controller's base ones (`Track_gPixMaps` 0x8011F454: 16 bytes a
/// pixmap, V0 at +1). DrawW_DrawQuad draws those quads inside a texture window the size of
/// the shape, so the scroll wraps round within it. (The cycling type 4 is used by no track.)
pub fn read_texture_scroll(m: &mut Mem) -> Vec<(u16, u8)> {
    let (n, ctrl, mats, pixmaps) = (m.r32(addr::GP + 3984).min(64), m.r32(addr::GP + 3988), m.r32(addr::GP + 3960), m.r32(0x8011_F454));
    if ctrl == 0 || mats == 0 || pixmaps == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| ctrl + 16 * i)
        .filter_map(|c| {
            if m.r16(c) != 0x80 {
                return None;
            }
            let mat = m.r32(c + 12);
            let pixmap = m.r16(mat + 2) as i16;
            (mat >= mats && pixmap >= 0).then(|| (((mat - mats) / 4) as u16, m.r8(pixmaps + 16 * pixmap as u32 + 1).wrapping_sub(m.r8(c + 4))))
        })
        .collect()
}

/// Each part's drawing mode (R3DCar's part table `0x80117898`, 6 bytes a part: the flags byte,
/// & 9): 9 the reflection path, 8 the light path, both applying the texture substitutes.
pub fn read_part_modes(m: &mut Mem) -> Vec<u8> {
    (0..57).map(|i| m.r8(0x8011_7898 + 6 * i + 1) & 9).collect()
}

pub fn read_env_normals(m: &mut Mem, car: u32) -> Vec<(usize, Vec<[i16; 3]>)> {
    let slot = m.r16(car + 0x8BC) as i16;
    if !(0..50).contains(&slot) {
        return Vec::new();
    }
    let lod = (m.r8(car + 0x842) >> 7) as u32;
    let parts = m.r32(0x8011_7BC0 + lod * 200 + slot as u32 * 4);
    let in_ram = |a: u32| (0x8000_0000..0x8020_0000).contains(&a);
    if !in_ram(parts) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for pi in [0usize, 8, 10, 41, 42, 45, 46] {
        let part = m.r32(parts + 4 * pi as u32);
        if !in_ram(part) {
            continue;
        }
        let (nv, normals) = (m.r16(part) as u32, m.r32(part + 20));
        if nv == 0 || nv > 512 || !in_ram(normals) {
            continue;
        }
        out.push((pi, (0..nv).map(|i| std::array::from_fn(|k| m.r16(normals + 6 * i + 2 * k as u32) as i16)).collect()));
    }
    out
}

/// One quad of a tyre mark (Draw_kCtrlSkidmark's POLY_GT4): this entry's edge and the
/// previous entry's, PS1 world coordinates (16.16).
#[derive(Clone, Debug)]
pub struct SkidQuad {
    /// This entry's p0, p1, then the previous entry's p0, p1.
    pub corners: [[i32; 3]; 4],
    /// Colour (0xBBGGRR) of this entry and of the previous one.
    pub colour: [u32; 2],
    /// Texture: 0 = SKD0, 1 = SKD1 (Skidmark pixmaps, flags bit 0).
    pub tex: u8,
    /// Track slice the segment belongs to.
    pub slice: i16,
}

/// The tyre marks (Skidmark segments at gp+5484, count at gp+5488): segments of 688
/// bytes, origin (16.16) and up to 23 entries of two edge points (relative, 1/1024 m),
/// colour, flags and the previous entry the quad joins.
pub fn read_skids(r32: &mut dyn FnMut(u32) -> u32) -> Vec<SkidQuad> {
    fn r16(r32: &mut dyn FnMut(u32) -> u32, a: u32) -> i16 {
        (r32(a & !3) >> ((a & 2) * 8)) as u16 as i16
    }
    // an entry's edge point, relative to the segment's origin
    fn pt(r32: &mut dyn FnMut(u32) -> u32, origin: [i32; 3], a: u32) -> [i32; 3] {
        let mut p = origin;
        for (i, v) in p.iter_mut().enumerate() {
            *v += (r16(r32, a + 2 * i as u32) as i32) << 6;
        }
        p
    }
    let mut out = Vec::new();
    let n = r32(addr::GP + 5488).min(64);
    let base = r32(addr::GP + 5484);
    if base == 0 {
        return out;
    }
    for s in 0..n {
        let seg = base + 688 * s;
        let origin = [r32(seg) as i32, r32(seg + 4) as i32, r32(seg + 8) as i32];
        let count = r16(r32, seg + 12).clamp(0, 23) as u32;
        let slice = r16(r32, seg + 14);
        for e in 0..count {
            let ent = seg + 16 + 28 * e;
            let prev = r32(ent + 24);
            if prev == 0 {
                continue;
            }
            let flags = r32(ent + 20);
            let colour = if flags != 0 { [0x40_4040; 2] } else { [r32(ent + 16) & 0xFF_FFFF, r32(prev + 16) & 0xFF_FFFF] };
            if colour == [0, 0] {
                continue;
            }
            out.push(SkidQuad {
                corners: [pt(r32, origin, ent), pt(r32, origin, ent + 8), pt(r32, origin, prev), pt(r32, origin, prev + 8)],
                colour,
                tex: (flags & 1) as u8,
                slice,
            });
        }
    }
    out
}

/// Simulation state at the top of a frame.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Frames run so far.
    pub frame: u32,
    /// 60 Hz field number of the virtual clock.
    pub field: u64,
    pub cars: Vec<CarState>,
    /// The human players' cars (`Car_tObj` addresses, 0 when there is none): player 1, 2.
    pub player_cars: [u32; 2],
    /// See [`addr::COUNTDOWN_STAGE`].
    pub countdown: u8,
    pub ticks: u32,
    /// `ticks` when the countdown last changed stage (Hud_Render321Go's timer).
    pub countdown_start: u32,
    /// Simulation time of the car states, see [`addr::SIM_COUNTER`].
    pub sim_ticks: u32,
    /// The player view's camera as the game computed it for the last rendered frame
    /// (`DRender_tCalcView` translation and rotation).
    pub camera_pos: [i32; 3],
    pub camera_rot: [i32; 9],
    /// Player 0 camera mode (`Camera_gInfo[0].mode`, cycled by Triangle).
    pub camera_mode: i16,
    /// Sky and horizon set-up.
    pub sky: Option<SkyData>,
    /// The sun's direction (Flare_Sun's SVECTOR, PS1 world axes), for moving its flare with
    /// the drawn camera.
    pub sun_dir: [i16; 3],
    /// HUD inputs: `DashHUD_gInfo` (0x80 bytes) and the HUD layout record (0x60 bytes).
    pub dash: Vec<u8>,
    pub hud_layout: Vec<u8>,
    /// The track map (when the HUD shows it).
    pub map: MapView,
    /// Per car (as in `cars`): what the game draws of it this frame (parts, light,
    /// shadow); `None` when the car is not in the view.
    pub car_facets: Vec<Option<super::host::CarFacet>>,
    /// The track objects the game drew this frame (empty before it has drawn a view).
    pub objects: Vec<super::host::ObjectDraw>,
    /// The 2D drawing (pause menu and the like) the game recorded this frame.
    pub overlay: Vec<super::host::OverlayCmd>,
    /// The race's sky layer (the sun's flare) as recorded.
    pub sky_overlay: Vec<super::host::OverlayCmd>,
    /// See [`super::host::GameHost::weather`].
    pub weather: Vec<super::host::OverlayCmd>,
    /// Light glows with their world anchors (see `host::Glow`).
    pub glows: Vec<super::host::Glow>,
    /// The race is paused (`gSimQueue_BlockSelf`).
    pub paused: bool,
    /// The game drew nothing this frame (the first ones after the pause menu closes): the
    /// 2D and views are the last drawn frame's, and so is the camera (the cars have moved on).
    pub held: bool,
    /// The HUD as Hud_Init built it (read once, see [`HudSetup`]).
    pub hud: Option<std::sync::Arc<HudSetup>>,
    /// The track's vertex colour table (`Chunk_lightTable`, 256 entries of 0xBBGGRR).
    pub light_table: Vec<u32>,
    /// Night driving: the player's headlight beam as Night_NightCalc lights the world with it.
    pub night_beam: Option<NightBeam>,
    /// Per car (first frame of a race only): its reflection normals ([`read_env_normals`]).
    pub env_normals: Vec<Vec<(usize, Vec<[i16; 3]>)>>,
    /// Per car (first frame only): its texture substitutes ([`read_texture_subs`]).
    pub texture_subs: Vec<Vec<TextureSubs>>,
    /// First frame only: the parts' drawing modes ([`read_part_modes`]).
    pub part_modes: Vec<u8>,
    /// The scrolling track textures' offsets ([`read_texture_scroll`]).
    pub texture_scroll: Vec<(u16, u8)>,
    /// The DualShock motors per controller (small 0 / 1, big 0..255) as the game drives them.
    pub rumble: [[u8; 2]; 2],
    /// BWorld_gChunkBuildList: the track chunks the original draws this frame and how
    /// (detail flags).
    pub chunks: Vec<(i16, u16)>,
    /// The tyre marks on the road.
    pub skids: Vec<SkidQuad>,
    /// The view's GTE environment (far colour, depth cue, H, back colour); see
    /// `GameHost::view_env`.
    pub view_env: [i32; 9],
    /// Projection distance H of the camera (`SetCameraZoom`), in screen pixels.
    pub projection: u32,
    /// Display area (x, y, w, h) from PutDispEnv.
    pub display: [u16; 4],
    /// Full RAM copy, when requested.
    pub ram: Option<Vec<u8>>,
    /// A front-end frame (only `frame`, `field`, `overlay`, `vram` and `display` are set).
    pub front_end: bool,
    /// VRAM, for front-end frames.
    pub vram: Option<std::sync::Arc<Vec<u16>>>,
    /// The race GameSetup describes: track number and variant ("", "N", "W", "S").
    pub track: u32,
    pub variant: String,
    pub laps: u32,
    /// The player views drawn this frame: two split screen (each with its own camera, chunks,
    /// cars and 2D; the fields above are the first's), else one or none.
    pub views: Vec<ViewSnap>,
}

/// One player's view of a frame (see [`super::host::ViewCapture`]), the cars' facets by car.
#[derive(Clone, Debug, Default)]
pub struct ViewSnap {
    pub player: u32,
    /// Screen rectangle (x, y, w, h) on the PS1's display.
    pub rect: [i16; 4],
    pub camera_pos: [i32; 3],
    pub camera_rot: [i32; 9],
    pub projection: u32,
    pub view_env: [i32; 9],
    pub chunks: Vec<(i16, u16)>,
    pub car_facets: Vec<Option<super::host::CarFacet>>,
    pub objects: Vec<super::host::ObjectDraw>,
    pub glows: Vec<super::host::Glow>,
    pub weather: Vec<super::host::OverlayCmd>,
    pub sky_overlay: Vec<super::host::OverlayCmd>,
    pub night_beam: Option<NightBeam>,
}

impl Snapshot {
    fn with_views(self, views: Vec<ViewSnap>) -> Self {
        Self { views, ..self }
    }

    /// The frame as view `k` sees it: its per-view fields in place of the first view's.
    pub fn for_view(&self, k: usize) -> Snapshot {
        let mut s = Snapshot { views: Vec::new(), ..self.clone() };
        if let Some(v) = self.views.get(k) {
            s.camera_pos = v.camera_pos;
            s.camera_rot = v.camera_rot;
            s.projection = v.projection;
            s.view_env = v.view_env;
            s.chunks = v.chunks.clone();
            s.car_facets = v.car_facets.clone();
            s.objects = v.objects.clone();
            s.glows = v.glows.clone();
            s.weather = v.weather.clone();
            s.sky_overlay = v.sky_overlay.clone();
            s.night_beam = v.night_beam.clone();
        }
        s
    }
}

/// Input for one frame.
#[derive(Clone, Copy, Default)]
pub struct FrameInput {
    pub buttons: u16,
    /// The second controller (player 2).
    pub buttons2: u16,
    /// The controllers' sticks (RX, RY, LX, LY) when they are DualShocks in analog mode;
    /// `None`: a digital pad.
    pub sticks: [Option<[u8; 4]>; 2],
    /// Include a RAM copy in the resulting snapshot.
    pub want_ram: bool,
}

/// `first`: the first frame of a race (one-off reads such as the track's lights).
fn snapshot(c: &mut Cpu, frame: u32, info: super::host::FrameInfo, want_ram: bool, first: bool) -> Snapshot {
    if info.front_end {
        return Snapshot { frame, field: info.field, overlay: info.overlay, vram: info.vram, display: info.display, front_end: true, rumble: info.rumble, ..Default::default() };
    }
    let mut cars = Vec::new();
    for k in 0..9 {
        let car = c.m.r32(addr::CAR_LIST + 4 * k);
        if car == 0 {
            break;
        }
        cars.push(CarState::read(&mut c.m, car));
    }
    let by_car = |facets: &[super::host::CarFacet]| {
        let mut out = vec![None; cars.len()];
        for f in facets {
            if let Some(k) = cars.iter().position(|c| c.addr == f.car) {
                out[k] = Some(f.clone());
            }
        }
        out
    };
    let car_facets = by_car(&info.car_facets);
    let env_normals = if first { cars.iter().map(|car| read_env_normals(&mut c.m, car.addr)).collect() } else { Vec::new() };
    let texture_subs = if first { cars.iter().map(|car| read_texture_subs(&mut c.m, car.addr)).collect() } else { Vec::new() };
    let part_modes = if first { read_part_modes(&mut c.m) } else { Vec::new() };
    let views: Vec<ViewSnap> = info
        .views
        .iter()
        .map(|v| ViewSnap {
            player: v.player,
            rect: v.rect,
            camera_pos: v.camera_pos,
            camera_rot: v.camera_rot,
            projection: v.projection,
            view_env: v.view_env,
            chunks: v.chunks.clone(),
            car_facets: by_car(&v.car_facets),
            objects: v.objects.clone(),
            glows: v.glows.clone(),
            weather: v.weather.clone(),
            sky_overlay: v.sky_overlay.clone(),
            night_beam: v.night_beam.clone(),
        })
        .collect();
    let snap = Snapshot {
        frame,
        field: info.field,
        player_cars: [c.m.r32(addr::HUMAN_CARS), c.m.r32(addr::HUMAN_CARS + 4)],
        cars,
        countdown: c.m.r8(addr::COUNTDOWN_STAGE),
        ticks: c.m.r32(addr::TICKS),
        countdown_start: c.m.r32(addr::GP + 6348),
        sim_ticks: c.m.r32(addr::SIM_COUNTER),
        camera_pos: std::array::from_fn(|i| c.m.r32(super::host::RENDER_VIEW + 8 + 4 * i as u32) as i32),
        projection: c.m.r32(addr::CAMERA_ZOOM),
        view_env: info.view_env,
        sky: SkyData::read(&mut c.m),
        sun_dir: std::array::from_fn(|i| c.m.r16(0x8013_F0F8 + 2 * i as u32) as i16),
        hud: HudSetup::read(&mut c.m, info.font_clut, info.font_palettes.clone()).map(std::sync::Arc::new),
        map: MapView::read(&mut |a| c.m.r32(a)),
        car_facets,
        objects: info.objects.clone(),
        overlay: info.overlay.clone(),
        sky_overlay: info.sky_overlay.clone(),
        weather: info.weather.clone(),
        glows: info.glows.clone(),
        paused: c.m.r32(addr::SIM_PAUSED) != 0,
        held: false,
        dash: (0..0x80).map(|i| c.m.r8(addr::DASHHUD + i)).collect(),
        hud_layout: {
            let l = c.m.r32(addr::GP + 6352);
            if l == 0 { Vec::new() } else { (0..0x60).map(|i| c.m.r8(l + i)).collect() }
        },
        env_normals,
        texture_subs,
        part_modes,
        texture_scroll: read_texture_scroll(&mut c.m),
        rumble: info.rumble,
        chunks: {
            let n = c.m.r32(addr::GP + 608).min(64);
            (0..n).map(|i| (c.m.r16(0x8011_02B0 + 4 * i) as i16, c.m.r16(0x8011_02B2 + 4 * i))).collect()
        },
        skids: read_skids(&mut |a| c.m.r32(a)),
        night_beam: NightBeam::read(c, 0),
        light_table: {
            let t = c.m.r32(addr::LIGHT_TABLE);
            if t == 0 { Vec::new() } else { (0..256).map(|i| c.m.r32(t + 4 * i)).collect() }
        },
        camera_mode: c.m.r16(addr::CAMERA_INFO + 0x70) as i16,
        display: info.display,
        camera_rot: std::array::from_fn(|i| c.m.r32(super::host::RENDER_VIEW + 0x14 + 4 * i as u32) as i32),
        ram: want_ram.then(|| c.m.ram.to_vec()),
        front_end: false,
        vram: info.vram.clone(),
        track: c.m.r32(addr::GAMESETUP + 0x3C),
        laps: c.m.r32(addr::GAMESETUP + gs::NUM_LAPS as u32),
        variant: match (c.m.r32(addr::GAMESETUP + 0x48), c.m.r32(addr::GAMESETUP + 0x54)) {
            (1, 1) => "S",
            (1, _) => "W",
            (_, 1) => "N",
            _ => "",
        }
        .to_string(),
        views,
    };
    if std::env::var_os("NFSHS_GSLOG").is_some() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let w: Vec<i32> = (0..24).map(|i| c.m.r32(addr::GAMESETUP + 4 * i) as i32).collect();
            eprintln!("GameSetup words 0..0x60: {w:?}, cars {} players {}", c.m.r32(addr::GAMESETUP + 0x3C4), c.m.r32(addr::GAMESETUP + 0x3C8));
        });
    }
    // split screen: the fields that are per view are the first view's
    if snap.views.len() > 1 { snap.for_view(0).with_views(snap.views) } else { snap }
}

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
    /// Power on: run the original's main (front end, races, back to the front end) and wait
    /// for its first frame.
    pub fn start_main(data_root: PathBuf) -> Result<Self> {
        Self::spawn(data_root, None)
    }

    /// Boot the race described by `gamesetup` and wait until it is ready for its first frame.
    pub fn start(data_root: PathBuf, gamesetup: Vec<u8>) -> Result<Self> {
        Self::spawn(data_root, Some(gamesetup))
    }

    fn spawn(data_root: PathBuf, gamesetup: Option<Vec<u8>>) -> Result<Self> {
        let (in_tx, in_rx) = mpsc::sync_channel::<Option<FrameInput>>(1);
        let (out_tx, out_rx) = mpsc::sync_channel::<Result<Snapshot, String>>(1);
        let thread = std::thread::Builder::new().name("nfs4-sim".into()).stack_size(256 << 20).spawn(move || {
            let booted = match &gamesetup {
                Some(gs) => boot(&data_root, gs),
                None => power_on(&data_root),
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
            // the pause menu as last drawn: the game only redraws it when it changes and
            // shows the copied picture in between
            let mut menu: Vec<super::host::OverlayCmd> = Vec::new();
            // the last frame that drew something, shown again over frames that draw nothing
            let mut drawn: Option<Snapshot> = None;
            let mut in_race = false;
            let on_frame: super::host::FrameFn = Box::new(move |c: &mut Cpu, info: super::host::FrameInfo| {
                // report the state after the previous frame (or boot), then wait for input
                let first = !info.front_end && !in_race;
                in_race = !info.front_end;
                let mut s = snapshot(c, frame, info, want_ram, first);
                if !s.paused {
                    menu.clear();
                } else if s.overlay.iter().any(|o| !matches!(o, super::host::OverlayCmd::Rect { .. })) {
                    menu = s.overlay.clone();
                } else {
                    s.overlay = menu.clone();
                }
                if s.front_end {
                    drawn = None;
                } else if !s.paused {
                    if !s.views.is_empty() || !s.overlay.is_empty() {
                        drawn = Some(Snapshot { overlay: s.overlay.clone(), sky_overlay: s.sky_overlay.clone(), weather: s.weather.clone(), glows: s.glows.clone(), views: s.views.clone(), ..Default::default() });
                    } else if let Some(d) = &drawn {
                        s.held = true;
                        s.overlay = d.overlay.clone();
                        s.sky_overlay = d.sky_overlay.clone();
                        s.weather = d.weather.clone();
                        s.glows = d.glows.clone();
                        s.views = d.views.clone();
                    }
                }
                if quit || out.send(Ok(s)).is_err() {
                    return None;
                }
                let Some(input) = in_rx.recv().ok().flatten() else {
                    quit = true;
                    return None;
                };
                frame += 1;
                want_ram = input.want_ram;
                Some([super::host::Pad { buttons: input.buttons, sticks: input.sticks[0] }, super::host::Pad { buttons: input.buttons2, sticks: input.sticks[1] }])
            });
            let host = cpu.host.as_mut().expect("host");
            // the host is always our GameHost
            let host = unsafe { &mut *(host.as_mut() as *mut dyn super::Host as *mut GameHost) };
            host.frame.on_frame = Some(on_frame);
            host.from_main = gamesetup.is_none();
            cpu.m.w32(super::host::MAIN_LOOP_EXIT_FLAG, 0);
            cpu.tick_limit = u64::MAX;
            if gamesetup.is_none() {
                if let Err(e) = call(&mut cpu, addr::MAIN, &[]) {
                    let _ = out_tx.send(Err(format!("{e:#}")));
                }
                return;
            }
            // main's loop: run the race, and again after a reset while a restart is requested
            loop {
                if let Err(e) = call(&mut cpu, addr::GAME_LOOP, &[]) {
                    let _ = out_tx.send(Err(format!("{e:#}")));
                    break;
                }
                if cpu.m.r32(addr::SIM_VAR) == 0 {
                    break;
                }
                if let Err(e) = call(&mut cpu, addr::RACE_RESTART, &[]) {
                    let _ = out_tx.send(Err(format!("{e:#}")));
                    break;
                }
                cpu.m.w32(addr::SIM_VAR, 0);
                cpu.m.w32(super::host::MAIN_LOOP_EXIT_FLAG, 0);
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
