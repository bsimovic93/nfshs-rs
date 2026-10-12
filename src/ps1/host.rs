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
    /// Virtual PS1 clock in 1/1920 s units (one 128 Hz timer tick = 15, one 60 Hz field = 32).
    pub clock: u64,
    pub rcnt: u32,
    pub log: Vec<String>,
    /// 1024x512 16-bit VRAM, filled by the game's LoadImage/ClearImage/MoveImage.
    pub gpu: super::gpu::Gpu,
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
    /// The displayed picture of the previous frame (width, height, RGBA8).
    pub image: std::sync::Arc<(u32, u32, Vec<u8>)>,
    /// 60 Hz field number of the virtual clock.
    pub field: u64,
    /// Display area (x, y, w, h) from the last PutDispEnv.
    pub display: [u16; 4],
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
const CAMERA_GET_VIEW_INFO: u32 = 0x8008_5A2C;
/// Virtual clock units per 128 Hz timer tick and per 60 Hz field.
pub const TICK: u64 = 15;
pub const FIELD: u64 = 32;
const MAIN_LOOP_YIELD_RA: u32 = 0x800B_7AA4;

impl GameHost {
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
            rcnt: 0,
            log: Vec::new(),
            gpu: Default::default(),
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
            0x800E_DFCC => {
                // ResetGraph: libgpu remembers the VRAM size (used to clamp draw areas)
                c.m.w16(0x8012_4990, 1024);
                c.m.w16(0x8012_4992, 512);
                0
            }
            0x800E_E140 => 0,                  // SetDispMask
            0x800E_E1D8 | 0x800F_0318 => 0,    // DrawSync
            0x800E_E35C => {
                // ClearImage(rect, r, g, b)
                let (x, y, w, h) = Self::rect(c, a0);
                let col = ((a1 as u16 >> 3) & 31) | (((a2 as u16 >> 3) & 31) << 5) | (((a3 as u16 >> 3) & 31) << 10);
                for yy in y..(y + h).min(VRAM_H) {
                    for xx in x..(x + w).min(VRAM_W) {
                        self.gpu.vram[yy * VRAM_W + xx] = col;
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
                            self.gpu.vram[yy * VRAM_W + xx] = v;
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
                        let v = if yy < VRAM_H && xx < VRAM_W { self.gpu.vram[yy * VRAM_W + xx] } else { 0 };
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
                    .map(|(sx, sy)| if sy < VRAM_H && sx < VRAM_W { self.gpu.vram[sy * VRAM_W + sx] } else { 0 })
                    .collect();
                for j in 0..h {
                    for i in 0..w {
                        let (tx, ty) = (dx + i, dy + j);
                        if ty < VRAM_H && tx < VRAM_W {
                            self.gpu.vram[ty * VRAM_W + tx] = src[j * w + i];
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
            0x800E_E610 => {
                // DrawOTag(ot)
                let before = self.gpu.prims;
                self.gpu.draw_ot(&mut c.m, a0);
                if std::env::var_os("PS1_GPU").is_some() {
                    eprintln!("DrawOTag({a0:#x}): {} primitives, area {:?} offset {:?} display {:?}", self.gpu.prims - before, self.gpu.area(), self.gpu.offset(), self.gpu.display);
                }
                0
            }
            0x800E_E680 => {
                self.gpu.put_draw_env(&mut c.m, a0);
                a0
            }
            0x800E_E740 => {
                // PutDispEnv: the displayed VRAM area (x, y, w, h)
                self.display = [0, 2, 4, 6].map(|o| c.m.r16(a0 + o));
                self.gpu.put_disp_env(&mut c.m, a0);
                a0
            }
            0x8010_53B8 => {
                // GPU_cw(word): a single GP0 command
                self.gpu.gp0_list(&[a0]);
                0
            }
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
                    match self.frame.on_frame.as_mut().and_then(|f| f(c, FrameInfo { image: std::sync::Arc::new(self.gpu.display_rgba()), field, display: self.display })) {
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
