//! The frames the original finishes: what is handed to the engine (pads in, the picture's
//! makings out), and the loading screen's frames.

use super::*;

/// One controller during a frame: its buttons (active-high: bit 14 = Cross, 15 = Square,
/// 4 = Up, ...) and, for a DualShock in analog mode, its sticks (RX, RY, LX, LY; 0x80 is
/// centred, 0 left/up). Without sticks it is a digital pad.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Pad {
    pub buttons: u16,
    pub sticks: Option<[u8; 4]>,
}

impl From<u16> for Pad {
    fn from(buttons: u16) -> Self {
        Self { buttons, sticks: None }
    }
}

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
    /// See [`GameHost::objects`].
    pub objects: Vec<ObjectDraw>,
    /// See [`GameHost::overlay`].
    pub overlay: Vec<OverlayCmd>,
    /// 2D drawn over the sky and under the world (the sun's flare), as recorded.
    pub sky_overlay: Vec<OverlayCmd>,
    /// See [`GameHost::weather`].
    pub weather: Vec<OverlayCmd>,
    /// Light glows (track lights, car lights) with the world point each belongs to.
    pub glows: Vec<Glow>,
    /// The font's 16 colour palettes (Font_TextColor 0..15).
    pub font_palettes: Vec<[u16; 16]>,
    /// A front-end frame (else a race frame).
    pub front_end: bool,
    /// VRAM (1024x512) as of this front-end frame: the textures its quads use.
    pub vram: Option<std::sync::Arc<Vec<u16>>>,
    /// The player views drawn this frame (none in the front end or paused).
    pub views: Vec<ViewCapture>,
    /// The DualShock motors per port (small 0 / 1, big 0..255), as the game drives them.
    pub rumble: [[u8; 2]; 2],
}

/// One player's view as the original set it up this frame (Render_RenderPlayerView): two
/// split screen, each with its own camera, chunks, cars and 2D.
#[derive(Clone, Debug, Default)]
pub struct ViewCapture {
    /// The player it shows (0 or 1).
    pub player: u32,
    /// Where it is on the screen (x, y, w, h; its drawing environment, Draw_gViews).
    pub rect: [i16; 4],
    /// The camera (`DRender_tCalcView` translation and rotation) and its projection distance.
    pub camera_pos: [i32; 3],
    pub camera_rot: [i32; 9],
    pub projection: u32,
    /// See [`GameHost::view_env`].
    pub view_env: [i32; 9],
    /// BWorld_gChunkBuildList for this view: (chunk, detail flags).
    pub chunks: Vec<(i16, u16)>,
    pub car_facets: Vec<CarFacet>,
    pub objects: Vec<ObjectDraw>,
    pub glows: Vec<Glow>,
    pub weather: Vec<OverlayCmd>,
    pub sky_overlay: Vec<OverlayCmd>,
    pub night_beam: Option<crate::ps1::race::NightBeam>,
}

/// Which part of the original is running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    FrontEnd,
    Race,
}

/// Paces the original main loop against the virtual clock: game time only passes while the
/// game waits (VSync = until the next 60 Hz field, timer polling = until the next 128 Hz tick)
/// and every main-loop iteration is locked to the next field boundary.
#[derive(Default)]
pub struct FrameDriver {
    pub on_frame: Option<FrameFn>,
    /// Main-loop iterations (game frames) run.
    pub frames_run: u32,
    pub(super) last_top: u64,
}

impl GameHost {
    /// Frames of the race's loading screen for the fields the loading has taken since the
    /// last one (the original shows it while it reads the track from the CD).
    pub(super) fn loading_frames(&mut self, c: &mut Cpu) {
        while self.from_main && self.loading_shown + FIELD <= self.clock && self.frame.on_frame.is_some() && c.tick_limit != 0 {
            self.loading_shown += FIELD;
            let mut overlay = self.loading.clone();
            // drawn last asked first (as an ordering table)
            overlay.reverse();
            let info = FrameInfo {
                field: self.loading_shown / FIELD,
                display: [0, 256, 512, 240],
                view_env: self.view_env,
                font_clut: [0; 16],
                car_facets: Vec::new(),
                objects: Vec::new(),
                overlay,
                front_end: true,
                vram: Some({
                    if self.vram_dirty || self.vram_shared.is_none() {
                        self.vram_shared = Some(std::sync::Arc::new(self.vram.clone()));
                        self.vram_dirty = false;
                    }
                    self.vram_shared.clone().unwrap()
                }),
                font_palettes: Vec::new(),
                sky_overlay: Vec::new(),
                weather: Vec::new(),
                glows: Vec::new(),
                views: Vec::new(),
                rumble: self.rumble(c),
            };
            if self.frame.on_frame.as_mut().and_then(|f| f(c, info)).is_none() {
                c.tick_limit = 0;
            }
        }
    }

    /// A game frame is done (race loop top or the front end's flip): hand this frame's state
    /// to the frame callback and take the pad input for the next.
    pub(super) fn frame_top(&mut self, c: &mut Cpu) -> Option<[Pad; 2]> {
        self.otags = 0;
        // the recorded HUD views (in drawing order; overlays are kept last-drawn first): all
        // but the world's, or a paused game's menu view over the last full frame's HUD
        if self.mode == Mode::Race && !Self::paused(c) {
            let mut lists = std::mem::take(&mut self.frame_lists);
            let hud: Vec<OverlayCmd> = match lists.len() {
                0 => Vec::new(),
                1 => {
                    let list = lists.remove(0);
                    let key = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        format!("{list:?}").hash(&mut h);
                        h.finish()
                    };
                    let base = &self.last_hud;
                    let (acc, seen) = self.paused_buffers.entry(self.draw_origin[0]).or_insert_with(|| (base.clone(), Default::default()));
                    if seen.insert(key) && acc.len() < 50_000 {
                        acc.extend(list);
                    }
                    acc.clone()
                }
                _ => {
                    // (each player view's own list first: the world's)
                    let views = self.views.len().clamp(1, lists.len() - 1);
                    let hud: Vec<OverlayCmd> = lists.drain(views..).flatten().collect();
                    self.last_hud = hud.clone();
                    self.paused_buffers.clear();
                    hud
                }
            };
            self.overlay.extend(hud);
            self.overlay.reverse();
        }
        self.hold_cameras(c);
        // the screen shown while loading, until this frame
        if !self.loading.is_empty() {
            self.loading_frames(c);
            self.loading.clear();
        }
        if self.mode == Mode::FrontEnd {
            unlock_everything(c);
        }
        // top of the original main loop: game frames are locked to 60 Hz fields
        if !self.clock.is_multiple_of(FIELD) || self.clock == self.frame.last_top {
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
            objects: self.objects.clone(),
            glows: {
                let mut g = std::mem::take(&mut self.track_glows);
                g.append(&mut self.car_glows);
                g
            },
            weather: std::mem::take(&mut self.weather),
            views: std::mem::take(&mut self.views),
            sky_overlay: {
                if self.mode == Mode::Race && Self::paused(c) && self.sky_overlay.is_empty() {
                    self.last_sky.clone()
                } else {
                    self.last_sky = self.sky_overlay.clone();
                    std::mem::take(&mut self.sky_overlay)
                }
            },
            overlay: {
                while self.overlay_ot.len() < self.overlay.len() {
                    self.overlay_ot.push(self.current_ot);
                }
                if std::env::var_os("PS1_CMDLOG").is_some() && self.mode == Mode::FrontEnd {
                    for (i, cmd) in self.overlay.iter().enumerate() {
                        let d = match cmd {
                            OverlayCmd::Quad { pts, tex, semi, abr, colours } => format!("quad {:?} tex {:x?} semi {semi} abr {abr} pts {pts:?} col {colours:x?}", pts[0], tex.as_ref().map(|t| (t.tpage, t.clut, t.uv[0], t.uv[3]))),
                            OverlayCmd::ShowroomCar { car, cam_pos, h, ofx, ofy, mirrored, level, .. } => format!("showroom car {} pos {:?} cam {cam_pos:?} h {h} of {ofx},{ofy} mirrored {mirrored} level {level}", car.code, car.position),
                            other => format!("{other:?}").chars().take(60).collect::<String>().to_string(),
                        };
                        eprintln!("cmd {i} {d}");
                    }
                }
                if std::env::var_os("PS1_OTLOG").is_some() && self.mode == Mode::FrontEnd {
                    let mut runs: Vec<(u32, usize, String)> = Vec::new();
                    for (cmd, &ot) in self.overlay.iter().zip(&self.overlay_ot) {
                        let kind = format!("{cmd:?}").chars().take_while(|c| c.is_alphanumeric()).collect::<String>();
                        match runs.last_mut() {
                            Some(r) if r.0 == ot && r.2 == kind => r.1 += 1,
                            _ => runs.push((ot, 1, kind)),
                        }
                    }
                    eprintln!("ot runs: {}", runs.iter().map(|(o, n, k)| format!("{o:x}:{k}x{n}")).collect::<Vec<_>>().join(" "));
                }
                self.overlay_ot.clear();
                std::mem::take(&mut self.overlay)
            },
            front_end: self.mode == Mode::FrontEnd,
            rumble: self.rumble(c),
            vram: Some({
                if self.vram_dirty || self.vram_shared.is_none() {
                    self.vram_shared = Some(std::sync::Arc::new(self.vram.clone()));
                    self.vram_dirty = false;
                }
                self.vram_shared.clone().unwrap()
            }),
            font_palettes: {
                let (x, y) = (c.m.r16(FONT_CLUT_X) as u32, c.m.r16(FONT_CLUT_Y) as u32);
                (0..16).map(|i| self.clut((y << 6) | ((x + 16 * i) >> 4))).collect()
            },
        };
        // polling is detected within a frame
        c.m.io_accesses = 0;
        if std::env::var_os("PS1_SPINSTAT").is_some() {
            eprintln!("frame: longest stretch without host calls {} backward branches", c.max_since_host);
            c.max_since_host = 0;
        }
        let input = self.frame.on_frame.as_mut().and_then(|f| f(c, info));
        match input {
            // the host is done with the program: stop it where it is
            None => c.tick_limit = 0,
            Some(pads) => {
                self.frame.frames_run += 1;
                // each port its own controller (the front end takes either player's)
                for (b, pad) in self.pad_buffers.into_iter().zip(pads) {
                    if b != 0 {
                        c.m.w8(b, 0x00); // status: ok
                        c.m.w16(b + 2, !pad.buttons); // active low
                        match pad.sticks {
                            // a DualShock in analog mode: the game then steers with the
                            // left stick, unramped (Physics_RampCarControlValues)
                            Some(sticks) => {
                                c.m.w8(b + 1, 0x73);
                                for (i, v) in sticks.into_iter().enumerate() {
                                    c.m.w8(b + 4 + i as u32, v);
                                }
                            }
                            None => c.m.w8(b + 1, 0x41), // digital pad
                        }
                    }
                }
            }
        }
        if c.trace || std::env::var("PS1_FRAMES").is_ok() {
            eprintln!("frame {} at field {field} (ticks pending {}, calls {})", self.frame.frames_run, c.pending_ticks, c.calls);
        }
        input
    }

    /// Paused, the cameras stand still. The original's Camera_Update runs every main loop
    /// iteration, paused or not: its chase cameras would go on easing towards their place
    /// behind the (stopped) cars, unseen under the kept picture, and jump there on resuming.
    /// Each paused frame puts `Camera_gInfo` back as the pause found it (unless a camera's
    /// mode has changed meanwhile).
    fn hold_cameras(&mut self, c: &mut Cpu) {
        const SIZE: u32 = 2 * 0x110;
        const MODE: usize = 0x70;
        let base = crate::ps1::race::addr::CAMERA_INFO;
        if self.mode != Mode::Race || !Self::paused(c) {
            self.paused_cameras = None;
            return;
        }
        let now: Vec<u8> = (0..SIZE).map(|i| c.m.r8(base + i)).collect();
        let Some(held) = &self.paused_cameras else {
            self.paused_cameras = Some(now);
            return;
        };
        let modes = |b: &[u8]| [0, 0x110].map(|k| u16::from_le_bytes([b[k + MODE], b[k + MODE + 1]]));
        if modes(held) != modes(&now) {
            self.paused_cameras = Some(now);
            return;
        }
        for (i, &b) in held.iter().enumerate() {
            c.m.w8(base + i as u32, b);
        }
    }

    /// The motors' state per port from the game's actuator data (see `actuators`).
    pub(super) fn rumble(&self, c: &mut Cpu) -> [[u8; 2]; 2] {
        self.actuators.map(|a| if a == 0 { [0; 2] } else { [c.m.r8(a), c.m.r8(a + 1)] })
    }
}
