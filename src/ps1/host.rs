//! Host replacements (HLE) for the NFS4.EXE functions that drive PS1 hardware.
//! The address list is shared with the recompiler: tools/hle_funcs.txt.
//! Addresses are for the NTSC-U executable (SLUS-00826).

use ps1rt::{Cpu, Host};
use std::collections::HashMap;
use std::path::PathBuf;

pub const VRAM_W: usize = 1024;
pub const VRAM_H: usize = 512;

pub struct GameHost {
    pub root: PathBuf,
    files: HashMap<u32, Vec<u8>>,
    /// Mounted BIGF archives (FILE_addbigsync): handle -> entries.
    mounts: Vec<(u32, Vec<(String, Vec<u8>)>)>,
    next_handle: u32,
    /// Display area from the last PutDispEnv (x, y, w, h).
    pub display: [u16; 4],
    /// GTE environment the game set up for the player view: far colour R, G, B, DQA, DQB,
    /// H, back colour R, G, B.
    pub view_env: [i32; 9],
    /// The cars the game put into the player view this frame (R3DCar_InsertCarFacetII).
    pub car_facets: Vec<CarFacet>,
    /// Virtual PS1 clock in 1/1920 s units (one 128 Hz timer tick = 15, one 60 Hz field = 32).
    pub clock: u64,
    pub rcnt: u32,
    pub log: Vec<String>,
    /// 1024x512 16-bit VRAM, filled by the game's LoadImage/ClearImage/MoveImage.
    pub vram: Vec<u16>,
    /// Controller receive buffers registered with PadInitDirect (port 1, port 2).
    pub pad_buffers: [u32; 2],
    /// Callbacks the game registered (vsync, draw sync, ...), keyed by kind.
    pub callbacks: HashMap<&'static str, u32>,
    next_event: u32,
    /// Frame driver state while the original main loop runs (see race.rs).
    pub frame: FrameDriver,
}

/// Called at the top of every main-loop iteration (one game frame, locked to a 60 Hz field)
/// with the current field number; returns the PS1 digital buttons to hold during the frame
/// (active-high: bit 14 = Cross, 15 = Square, 4 = Up, ...) or `None` to leave the loop.
pub type FrameFn = Box<dyn FnMut(&mut Cpu, FrameInfo) -> Option<u16>>;

/// Host-side state handed to the frame callback.
#[derive(Clone)]
pub struct FrameInfo {
    /// 60 Hz field number of the virtual clock.
    pub field: u64,
    /// Display area (x, y, w, h) from the last PutDispEnv.
    pub display: [u16; 4],
    /// See [`GameHost::view_env`].
    pub view_env: [i32; 9],
    /// The HUD font's palette as the game uploaded it (CLUT of the digits' glyphs).
    pub font_clut: [u16; 16],
    /// See [`GameHost::car_facets`].
    pub car_facets: Vec<CarFacet>,
}

/// Inner hook for R3DCar_InsertCarFacetII(car): record what the original decided to draw
/// (see [`CarFacet::parse`]) instead of building primitives.
fn record_car_facet(c: &mut Cpu, addr: u32) -> bool {
    if addr != R3DCAR_INSERT_CAR_FACET_II {
        return false;
    }
    let car = c.r[4];
    let light = c.call_nested(BWORLDSM_QUAD_LIGHT, &[car + 8]) & 0xFF_FFFF;
    let mut rec = vec![car, light, c.m.r32(SHADOW_ON)];
    // Newton_CalcRealShadowCoordinates left the shadow's corners at car + 488; keep them
    // relative to the car's position in x and z (the car is drawn at an interpolated pose)
    let pos = [c.m.r32(car + 0xA0), 0, c.m.r32(car + 0xA8)];
    rec.extend((0..12).map(|i| c.m.r32(car + 488 + 4 * i).wrapping_sub(pos[i as usize % 3])));
    rec.push(c.m.r32(SHADOW_COLOUR) & 0xFF_FFFF);
    rec.extend((0..57).map(|i| c.m.r8(R3DCAR_OBJECT_VISIBLE + i) as u32));
    c.capture.extend(rec);
    c.r[2] = 0;
    true
}

/// What the game decided to draw of one car this frame.
#[derive(Clone, Debug, Default)]
pub struct CarFacet {
    /// `Car_tObj` address.
    pub car: u32,
    /// `R3DCar_ObjectVisible`: the model parts drawn.
    pub parts: Vec<u8>,
    /// The track's light at the car (BWorldSm_QuadLight, 0xBBGGRR).
    pub light: u32,
    /// The ground shadow's corners (16.16: x and z relative to the car's position, y the
    /// world height) and colour, when drawn.
    pub shadow: Option<([[i32; 3]; 4], [u8; 3])>,
}

impl CarFacet {
    const WORDS: usize = 3 + 12 + 1 + 57;

    /// Records left by `record_car_facet`.
    fn parse(w: &[u32]) -> Vec<Self> {
        w.chunks_exact(Self::WORDS)
            .map(|r| Self {
                car: r[0],
                light: r[1],
                shadow: (r[2] != 0).then(|| {
                    (std::array::from_fn(|k| std::array::from_fn(|i| r[3 + 3 * k + i] as i32)), std::array::from_fn(|i| (r[15] >> (8 * i)) as u8))
                }),
                parts: r[16..].iter().map(|&p| p as u8).collect(),
            })
            .collect()
    }
}

/// Paces the original main loop against the virtual clock: game time only passes while the
/// game waits (VSync = until the next 60 Hz field, timer polling = until the next 128 Hz tick)
/// and every main-loop iteration is locked to the next field boundary.
#[derive(Default)]
pub struct FrameDriver {
    pub on_frame: Option<FrameFn>,
    /// Main-loop iterations (game frames) run.
    pub frames_run: u32,
    last_top: u64,
}

pub const MAIN_LOOP_EXIT_FLAG: u32 = 0x8011_F380;
/// `DRender_tView` the game renders the player view with (id, player, DRender_tCalcView).
pub const RENDER_VIEW: u32 = 0x8011_8234;
const RENDER_START_WORLD_VIEW: u32 = 0x800B_3EE4;
const DASHHUD_CHECK_WRONG_WAY: u32 = 0x8009_BAC4;
const DASHHUD_HUD_CALC: u32 = 0x8009_BB34;
/// R3DCar_InsertAllListFacet(view): puts every car into the view (sorting, level of detail,
/// lights and damage state, R3DCar_InsertCarFacet per car).
const R3DCAR_INSERT_ALL_LIST_FACET: u32 = 0x800B_38C0;
/// R3DCar_InsertCarFacetII(car): builds the car's primitives from the parts chosen in
/// `R3DCar_ObjectVisible`.
const R3DCAR_INSERT_CAR_FACET_II: u32 = 0x800B_2E30;
const R3DCAR_OBJECT_VISIBLE: u32 = 0x8011_7B64;
/// BWorld_OnyxBuildFacets' set-up: SetupBuildMatrices(view, dcache), then
/// DrawW_WorldSetUpMatrix(gWorldMat, dcache + 0x14), then SetupChunkBuildList(view) fills
/// BWorld_gChunkBuildList (count kept at gp + 608): the chunks drawn this frame.
const SETUP_BUILD_MATRICES: u32 = 0x8007_E2E8;
const DRAWW_WORLD_SETUP_MATRIX: u32 = 0x800C_829C;
const G_WORLD_MAT: u32 = 0x8011_0104;
const SETUP_CHUNK_BUILD_LIST: u32 = 0x8007_E71C;
const CHUNK_BUILD_COUNT: u32 = 0x8013_D83C + 608;
/// BWorldSm_QuadLight(pos): the track lighting at a position.
const BWORLDSM_QUAD_LIGHT: u32 = 0x8008_0868;
/// R3DCar_InsertCarFacet's shadow switch and colour (gp-relative).
const SHADOW_ON: u32 = 0x8013_D83C + 3648;
const SHADOW_COLOUR: u32 = 0x8013_D83C + 3652;
/// Virtual clock units per 128 Hz timer tick and per 60 Hz field.
pub const TICK: u64 = 15;
pub const FIELD: u64 = 32;
const MAIN_LOOP_YIELD_RA: u32 = 0x800B_7AA4;

impl GameHost {
    /// 16 palette entries of CLUT id `clut` from VRAM.
    fn clut(&self, clut: u32) -> [u16; 16] {
        let (x, y) = ((clut & 0x3F) as usize * 16, ((clut >> 6) & 0x1FF) as usize);
        std::array::from_fn(|i| self.vram.get(y * VRAM_W + x + i).copied().unwrap_or(0))
    }

    /// Move the virtual clock forward, queueing the timer interrupts that fall in between.
    fn advance_to(&mut self, c: &mut Cpu, t: u64) {
        c.pending_ticks += (t / TICK - self.clock / TICK) as u32;
        self.clock = t;
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: HashMap::new(),
            mounts: Vec::new(),
            next_handle: 0x100,
            clock: 0,
            display: [0; 4],
            view_env: [0; 9],
            car_facets: Vec::new(),
            rcnt: 0,
            log: Vec::new(),
            vram: vec![0; VRAM_W * VRAM_H],
            pad_buffers: [0; 2],
            callbacks: HashMap::new(),
            next_event: 0xF000_0001,
            frame: FrameDriver::default(),
        }
    }

    fn resolve(&self, name: &str) -> Option<PathBuf> {
        let n = name.trim_start_matches("cdrom:").trim_start_matches('\\').split(';').next().unwrap_or("");
        let p = self.root.join(n.to_ascii_uppercase());
        p.is_file().then_some(p)
    }

    fn lookup(&self, name: &str) -> Option<Vec<u8>> {
        if std::env::var_os("PS1_FILES").is_some() {
            eprintln!("file: {name}");
        }
        if let Some(p) = self.resolve(name) {
            return std::fs::read(p).ok();
        }
        let base = name.rsplit(['\\', '/', ':']).next().unwrap_or(name);
        for (_, entries) in self.mounts.iter().rev() {
            if let Some((_, d)) = entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(base)) {
                return Some(d.clone());
            }
        }
        None
    }

    fn rect(c: &mut Cpu, p: u32) -> (usize, usize, usize, usize) {
        let x = c.m.r16(p) as i16 as i32;
        let y = c.m.r16(p + 2) as i16 as i32;
        let w = c.m.r16(p + 4) as i16 as i32;
        let h = c.m.r16(p + 6) as i16 as i32;
        (x.max(0) as usize, y.max(0) as usize, w.max(0) as usize, h.max(0) as usize)
    }
}

impl Host for GameHost {
    fn call(&mut self, c: &mut Cpu, addr: u32) -> bool {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        let ret: u32 = match addr {
            // ---- EA file layer (FILE_*sync)
            0x800E_B204 => {
                let name = c.m.cstr(a0);
                match self.lookup(&name) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        self.log.push(format!("open {name} ({} bytes)", data.len()));
                        if std::env::var_os("PS1_FILES").is_some() {
                            eprintln!("open {name}");
                        }
                        self.files.insert(h, data);
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        self.log.push(format!("open {name}: not found"));
                        c.m.w32(a3, 0);
                        0
                    }
                }
            }
            0x800E_B27C => {
                let data = self.files.get(&a0).cloned().unwrap_or_default();
                let off = (a1 as usize).min(data.len());
                let n = (a3 as usize).min(data.len() - off);
                c.m.write_bytes(a2, &data[off..off + n]);
                n as u32
            }
            0x800E_B2AC => {
                self.files.remove(&a0);
                0
            }
            0x800E_B300 => self.files.get(&a0).map_or(0, |d| d.len() as u32),
            0x800E_B354 => {
                // FILE_addbigsync(name, mode, prio, &handle): mount a BIGF archive
                let name = c.m.cstr(a0);
                let viv = self.lookup(&name).and_then(|d| crate::formats::viv::Viv::parse(&d).ok());
                match viv {
                    Some(v) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        self.log.push(format!("mount {name} ({} entries)", v.entries.len()));
                        self.mounts.push((h, v.entries));
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        self.log.push(format!("mount {name}: failed"));
                        c.m.w32(a3, 0);
                        0
                    }
                }
            }
            0x800E_B3CC => {
                // FILE_delbigsync(handle, prio)
                self.mounts.retain(|(h, _)| *h != a0);
                0
            }

            // ---- libgpu
            0x800E_DFCC => 0,                  // ResetGraph
            0x800E_E140 => 0,                  // SetDispMask
            0x800E_E1D8 | 0x800F_0318 => 0,    // DrawSync
            0x800E_E35C => {
                // ClearImage(rect, r, g, b)
                let (x, y, w, h) = Self::rect(c, a0);
                let col = ((a1 as u16 >> 3) & 31) | (((a2 as u16 >> 3) & 31) << 5) | (((a3 as u16 >> 3) & 31) << 10);
                for yy in y..(y + h).min(VRAM_H) {
                    for xx in x..(x + w).min(VRAM_W) {
                        self.vram[yy * VRAM_W + xx] = col;
                    }
                }
                0
            }
            0x800E_E3EC => {
                // LoadImage(rect, data)
                let (x, y, w, h) = Self::rect(c, a0);
                let mut p = a1;
                for yy in y..y + h {
                    for xx in x..x + w {
                        let v = c.m.r16(p);
                        p += 2;
                        if yy < VRAM_H && xx < VRAM_W {
                            self.vram[yy * VRAM_W + xx] = v;
                        }
                    }
                }
                0
            }
            0x800E_E44C => {
                // StoreImage(rect, data)
                let (x, y, w, h) = Self::rect(c, a0);
                let mut p = a1;
                for yy in y..y + h {
                    for xx in x..x + w {
                        let v = if yy < VRAM_H && xx < VRAM_W { self.vram[yy * VRAM_W + xx] } else { 0 };
                        c.m.w16(p, v);
                        p += 2;
                    }
                }
                0
            }
            0x800E_E4AC => {
                // MoveImage(rect, x, y)
                let (x, y, w, h) = Self::rect(c, a0);
                let (dx, dy) = (a1 as usize, a2 as usize);
                let src: Vec<u16> = (0..h)
                    .flat_map(|j| (0..w).map(move |i| (x + i, y + j)))
                    .map(|(sx, sy)| if sy < VRAM_H && sx < VRAM_W { self.vram[sy * VRAM_W + sx] } else { 0 })
                    .collect();
                for j in 0..h {
                    for i in 0..w {
                        let (tx, ty) = (dx + i, dy + j);
                        if ty < VRAM_H && tx < VRAM_W {
                            self.vram[ty * VRAM_W + tx] = src[j * w + i];
                        }
                    }
                }
                0
            }
            0x800E_E564 => {
                // ClearOTagR(ot, n): each entry links to the previous one, ot[0] terminates
                for i in 0..a1 {
                    let v = if i == 0 { 0x00FF_FFFF } else { (a0 + (i - 1) * 4) & 0x00FF_FFFF };
                    c.m.w32(a0 + i * 4, v);
                }
                a0
            }
            0x800E_E610 => 0,                  // DrawOTag: rendering is done by the host engine
            0x800E_E680 => a0, // PutDrawEnv
            0x800E_E740 => {
                // PutDispEnv: remember the display area (x, y, w, h)
                self.display = [0, 2, 4, 6].map(|o| c.m.r16(a0 + o));
                a0
            }
            0x8010_53B8 => 0,                  // GPU_cw
            0x800F_24B8 => 0,                  // GetVideoMode: NTSC

            // ---- libetc / timers / callbacks
            0x800F_2C78 => {
                // VSync(mode): mode 0 waits for the next field; mode 1 just reads the counter
                if a0 == 0 {
                    self.advance_to(c, (self.clock / FIELD + 1) * FIELD);
                }
                (self.clock / FIELD) as u32
            }
            0x800F_31A8 => 0, // ResetCallback
            0x800F_31D8 => {
                self.callbacks.insert("interrupt", a1);
                0
            }
            0x800F_3208 => {
                self.callbacks.insert("dma", a1);
                0
            }
            0x800F_3238 => {
                let old = self.callbacks.insert("vsync", a0).unwrap_or(0);
                old
            }
            0x800E_A7CC | 0x800E_A8A0 => 1, // SetRCnt / StartRCnt
            0x800E_A868 => {
                // GetRCnt
                self.rcnt = self.rcnt.wrapping_add(64);
                self.rcnt & 0xFFFF
            }
            0x8010_7000 | 0x8010_7010 => 0, // ChangeClearPAD / ChangeClearRCnt
            0x800F_4D30 => 0,               // FlushCache
            0x8010_7318 | 0x8010_7338 => 1, // Exit/EnterCriticalSection

            // ---- pad
            0x800F_E6D4 => {
                // PadInitDirect(buf1, buf2)
                self.pad_buffers = [a0, a1];
                for b in [a0, a1] {
                    if b != 0 {
                        c.m.w8(b, 0xFF); // no controller until the host writes one
                    }
                }
                1
            }
            0x800F_07BC => 1, // PadStartCom
            0x800F_07FC => 6, // PadGetState: PadStateStable
            0x800F_0A88 | 0x800F_0B08 => 1, // PadSetActAlign / PadSetAct

            // ---- events (BIOS)
            0x8010_77CC => {
                // OpenEvent(desc, spec, mode, handler): RCNT2 interrupt = the game's 128 Hz timer
                if a0 == 0xF200_0002 && a3 != 0 && !c.timer_handlers.contains(&a3) {
                    c.timer_handlers.push(a3);
                    self.log.push(format!("timer handler {a3:#x}"));
                }
                let e = self.next_event;
                self.next_event += 1;
                e
            }
            0x800E_77F0 => {
                if self.frame.on_frame.is_some() && c.r[31] == MAIN_LOOP_YIELD_RA {
                    // top of the original main loop: game frames are locked to 60 Hz fields
                    if self.clock % FIELD != 0 || self.clock == self.frame.last_top {
                        self.advance_to(c, (self.clock / FIELD + 1) * FIELD);
                    }
                    self.frame.last_top = self.clock;
                    let field = self.clock / FIELD;
                    let info = FrameInfo {
                        field,
                        display: self.display,
                        view_env: self.view_env,
                        font_clut: self.clut(c.m.r32(0x8011_1F34) >> 16),
                        car_facets: self.car_facets.clone(),
                    };
                    match self.frame.on_frame.as_mut().and_then(|f| f(c, info)) {
                        None => c.m.w32(MAIN_LOOP_EXIT_FLAG, 1),
                        Some(buttons) => {
                            self.frame.frames_run += 1;
                            for b in self.pad_buffers {
                                if b != 0 {
                                    c.m.w8(b, 0x00); // status: ok
                                    c.m.w8(b + 1, 0x41); // digital pad
                                    c.m.w16(b + 2, !buttons); // active low
                                }
                            }
                        }
                    }
                    if c.trace || std::env::var("PS1_FRAMES").is_ok() {
                        eprintln!("frame {} at field {field} (ticks pending {}, calls {})", self.frame.frames_run, c.pending_ticks, c.calls);
                    }
                } else {
                    // yield inside a wait loop: let time pass until the next timer tick
                    self.advance_to(c, (self.clock / TICK + 1) * TICK);
                }
                // then run the original systemtask
                return false;
            }
            0x8010_77BC | 0x8010_77DC | 0x8010_D4C0 | 0x8010_D354 => 1, // Enable/Disable/Close/Deliver
            0x8010_D4D0 => 1, // TestEvent: report every event as delivered

            // ---- CD (all game data goes through the FILE_* layer above; streaming is stubbed)
            0x8010_9CC8 => 1,                 // CdInit
            0x800F_81A8 | 0x8010_9924 => 0,   // CdSync / CdReadSync: complete
            0x800F_80DC => 0x02,              // CdStatus: motor on, idle
            0x800F_81C8 => 0,                 // CdReady
            0x800F_8210 | 0x800F_834C | 0x800F_8480 => 1, // CdControl*
            0x8010_9788 => 1,                 // CdRead
            0x8010_9A38 | 0x8010_9B74 => 2,   // CdDiskReady / CdGetDiskType (PS1 disc)
            0x8010_9ED8 | 0x8010_9EFC => 1,   // CdGetToc
            0x800F_80FC | 0x800F_8108 | 0x800F_8174 | 0x800F_8194 | 0x800F_85CC => 0,
            0x800F_81E8 | 0x800F_81FC | 0x800F_860C => 0, // Cd*Callback

            // ---- EA sound driver: SPU DMA and voice registers (audio is produced by the host)
            0x8010_BAC8 => 1,                             // iSNDdmcomplete: transfer done
            0x8010_B8FC | 0x8010_B9F0 => 1,               // iSNDdmqueue / queuesplit: transfer id
            0x8010_B4BC | 0x8010_B87C => 0,               // iSNDdmtransfer / iSNDdmservice
            0x8010_4360 => 0,                             // iSNDpsxzerospu
            0x800F_FAEC | 0x800F_FB14 | 0x800F_FB3C | 0x800F_FB7C | 0x800F_FBBC => 0,
            0x8010_CBDC | 0x8010_CC00 => 0,               // SPU IRQ enable/disable

            0x8009_68E8 => 0, // Speech_Server: commentator streaming (audio is host-side)
            0x8007_6B90 => 0, // AudioClc_SoundCars: positional engine/track audio (host-side)
            0x800B_4024 => {
                // Render_RenderPlayerView(view, player): the world itself is drawn by the host
                // renderer. Run the original view set-up (Render_StartRenderingWorldView:
                // camera, fog keys, environment colours) and keep what it programs into the
                // GTE, but skip Render_RenderWorld.
                c.m.w32(RENDER_VIEW, a0);
                c.m.w32(RENDER_VIEW + 4, a1);
                c.call_nested(RENDER_START_WORLD_VIEW, &[a0]);
                // the HUD's values (speed in display units, gear, times, position): game logic
                // the original runs while rendering the HUD
                let g = |r: usize| c.gte_read(32 + r) as i32;
                self.view_env = [g(21), g(22), g(23), g(27), g(28), g(26), g(13), g(14), g(15)];
                // which track chunks to draw, and how (detail level)
                c.call_nested(SETUP_BUILD_MATRICES, &[RENDER_VIEW, 0x1F80_0000]);
                c.call_nested(DRAWW_WORLD_SETUP_MATRIX, &[G_WORLD_MAT, 0x1F80_0014]);
                let n = c.call_nested(SETUP_CHUNK_BUILD_LIST, &[RENDER_VIEW]);
                c.m.w32(CHUNK_BUILD_COUNT, n);
                c.call_nested(DASHHUD_CHECK_WRONG_WAY, &[a1]);
                c.call_nested(DASHHUD_HUD_CALC, &[a1]);
                // the cars: the original decides what is drawn of each (the host draws them).
                // Its calls cannot reach the host from here: R3DCar_InsertCarFacetII is
                // replaced by an inner hook that records the decision instead of drawing.
                c.capture.clear();
                c.inner_hook = Some(record_car_facet);
                c.call_nested(R3DCAR_INSERT_ALL_LIST_FACET, &[RENDER_VIEW]);
                c.inner_hook = None;
                self.car_facets = CarFacet::parse(&c.capture);
                0
            }

            // ---- EA CD layer (only used for streaming once FILE_* is replaced)
            0x800F_AB04 | 0x800F_AB58 => 1,              // readsectorB / loaddirinfo
            0x800F_AD14 => 0,                            // CD_Restore
            0x800F_AD44 | 0x800F_AE58 => 1,              // CD_Init / CD_Restart
            0x800F_AF04 => 0,                            // CD_Open: streams unavailable
            0x800F_B00C | 0x800F_B2B0 | 0x800F_B2CC => 0, // CD_Close / CD_Stopread / CD_Getinfo
            0x800F_9954 => 0,                            // StSetStream

            // ---- streaming / MDEC / memory card: unavailable
            0x800F_92C4 | 0x800F_9824 | 0x800F_A354 => 0,
            0x800F_9324 | 0x800F_9358 | 0x800F_93F4 => 0,
            0x800F_B458 | 0x800F_B4A4 | 0x800F_B4F4 | 0x800F_C494 => 0,
            0x800F_C234 | 0x800F_C5C4 | 0x800F_C7CC | 0x800F_C988 => 0,
            0x800F_C4A8 => {
                // MemCardSync: report "no card"
                if a1 != 0 {
                    c.m.w32(a1, 0);
                }
                c.m.w32(a2, 0x02);
                1
            }

            _ => return false,
        };
        c.r[2] = ret;
        true
    }
}
