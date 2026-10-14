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
    memcard: super::memcard::MemCard,
    /// The car DrawCar last showed (type, colour).
    showroom_car: Option<(i8, u8)>,
    videos: super::video::Videos,
    idles: u64,
    /// Which part of the game is running (switched by the hooks on their entry points).
    pub mode: Mode,
    /// PS1_CALLLOG=addr,addr,...: calls the host sees that are logged (diagnostics).
    call_log: Option<Vec<u32>>,
    /// The EA file layer's asynchronous operations (FILE_open, FILE_read, ...): they complete
    /// at once; id -> (result, user data).
    file_ops: HashMap<u32, (u32, u32)>,
    next_op: u32,
    /// Display area from the last PutDispEnv (x, y, w, h).
    pub display: [u16; 4],
    /// GTE environment the game set up for the player view: far colour R, G, B, DQA, DQB,
    /// H, back colour R, G, B.
    pub view_env: [i32; 9],
    /// The cars the game put into the player view this frame (R3DCar_InsertCarFacetII).
    pub car_facets: Vec<CarFacet>,
    /// 2D drawing recorded this frame (see [`OverlayCmd`]).
    pub overlay: Vec<OverlayCmd>,
    /// A screen the game puts straight into the display buffer while it loads (the title,
    /// the race's loading screen, the loading icon): its pictures in the order drawn, and the
    /// clock of the last frame shown of it. Shown until the next frame the game draws.
    loading: Vec<OverlayCmd>,
    loading_shown: u64,
    /// The ordering-table entry (`*(0x1F800000)`, where the front end adds its primitives)
    /// each overlay command went into, and the entry current for the running host call.
    overlay_ot: Vec<u32>,
    current_ot: u32,
    /// Races: the HUD is what the original puts into its views' ordering tables (every
    /// DrawOTag after the frame's first, the world's), recorded as overlay commands; off
    /// with NFSHS_OWN_HUD=1 (the host's own port of the HUD).
    pub hud_prims: bool,
    /// DrawOTag calls since the last frame.
    otags: u32,
    /// This frame's recorded views (each list in drawing order) and the last full frame's
    /// HUD (a paused game draws only its menu's view over the picture it keeps).
    frame_lists: Vec<Vec<OverlayCmd>>,
    last_hud: Vec<OverlayCmd>,
    /// This frame's sky layer (Flare_Sun's primitives), and the last one (kept while paused).
    sky_overlay: Vec<OverlayCmd>,
    /// The weather (rain, snow, the drops on the lens) as Render_StartRenderingWorldView built
    /// it this frame, recorded.
    weather: Vec<OverlayCmd>,
    last_sky: Vec<OverlayCmd>,
    /// The race cars' light halos (DrawC_PrimHalo's primitives), drawn over the 3D.
    car_glows: Vec<Glow>,
    /// The track lights' glares (BWorld_BuildGlareEffects' primitives), over the 3D.
    track_glows: Vec<Glow>,
    /// This frame's player views (one, or two split screen), as each was set up.
    views: Vec<ViewCapture>,
    /// The showroom car's shadow (DrawC_ShadowPrimClip's primitives), for the floor.
    showroom_shadow: Vec<OverlayCmd>,
    /// What each drawing buffer (by its x) shows of the 2D while paused: the last HUD and the
    /// menu views drawn into it since (the game redraws only what changes), and the lists
    /// already in it.
    paused_buffers: HashMap<i32, (Vec<OverlayCmd>, std::collections::HashSet<u64>)>,
    /// The drawing offset of the view being recorded (DR_OFFSET, display-relative).
    prim_offset: [i16; 2],
    area_tl: [i32; 2],
    draw_origin: [i32; 2],
    /// SetDrawEnv's packets: their drawing environment's clip (x, y, w, h) and offset, in VRAM.
    draw_envs: HashMap<u32, ([i16; 4], [i16; 2])>,
    /// The game started from main (its title and front end), not straight into a race.
    pub from_main: bool,
    /// Virtual PS1 clock in 1/1920 s units (one 128 Hz timer tick = 15, one 60 Hz field = 32).
    pub clock: u64,
    pub rcnt: u32,
    pub log: Vec<String>,
    /// 1024x512 16-bit VRAM, filled by the game's LoadImage/ClearImage/MoveImage.
    pub vram: Vec<u16>,
    vram_dirty: bool,
    /// Inside a captured front-end drawing routine (see `capture_prims`).
    capturing: bool,
    /// The primitive packets recorded this frame (front end, PS1_MISSLOG: packets the original
    /// put in its ordering table that were not recorded are reported at DrawOTag).
    recorded_packets: std::collections::HashSet<u32>,
    /// The GPU draw mode (texture page) as the captured primitives left it.
    draw_mode: u16,
    vram_shared: Option<std::sync::Arc<Vec<u16>>>,
    /// Controller receive buffers registered with PadInitDirect (port 1, port 2).
    pub pad_buffers: [u32; 2],
    /// Callbacks the game registered (vsync, draw sync, ...), keyed by kind.
    pub callbacks: HashMap<&'static str, u32>,
    next_event: u32,
    /// Frame driver state while the original main loop runs (see race.rs).
    pub frame: FrameDriver,
}

/// PS1_FILELOG: print the files the game opens.
pub fn file_log(s: String) {
    if std::env::var_os("PS1_FILELOG").is_some() {
        eprintln!("{s}");
    }
}

/// The car model files (`zzz....viv`) the game has opened, lower case: which police car
/// livery it chose for a track, say (`zzzgbmws.viv` for the cop BMW in Germany).
static CAR_FILES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn note_open(name: &str) {
    let n = name.rsplit(['\\', '/', ':']).next().unwrap_or(name).to_ascii_lowercase();
    if n.starts_with("zzz") && n.ends_with(".viv") {
        if let Ok(mut v) = CAR_FILES.lock() {
            if !v.contains(&n) {
                v.push(n);
            }
        }
    }
}

/// The model file the game opened for car `code` whose first letter it replaced (police cars:
/// "CBMW" -> "zzzgbmws.viv"), as the name to read.
pub fn opened_variant(code: &str) -> Option<String> {
    let tail = code.get(1..)?.to_ascii_lowercase();
    let v = CAR_FILES.lock().ok()?;
    v.iter().rev().find(|n| n.len() == 3 + 1 + tail.len() + 5 && n[4..4 + tail.len()] == tail).map(|n| n.to_ascii_uppercase())
}

/// Called at the top of every main-loop iteration (one game frame, locked to a 60 Hz field)
/// with the current field number; returns the PS1 digital buttons to hold during the frame
/// (active-high: bit 14 = Cross, 15 = Square, 4 = Up, ...) or `None` to leave the loop.
pub type FrameFn = Box<dyn FnMut(&mut Cpu, FrameInfo) -> Option<[u16; 2]>>;

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
}

/// 2D drawing the game asked for this frame outside the HUD (pause menu and the like):
/// recorded from its calls, drawn by the host.
#[derive(Clone, Debug)]
pub enum OverlayCmd {
    /// Font_TextXY: text at (x, y) in font colour `colour` (Font_TextColor index); `space`
    /// is the font's space width.
    Text { x: i16, y: i16, colour: u8, space: u8, text: Vec<u8> },
    /// Hud_RenderPauseBox(x, y, w, h).
    PauseBox { x: i16, y: i16, w: i16, h: i16 },
    /// Hud_FBuildF4(semi, x, y, w, h, colour 0xBBGGRR, top-left / top-right x offsets).
    Rect { semi: bool, x: i16, y: i16, w: i16, h: i16, colour: u32, dx0: i8, dx1: i8 },
    /// Hud_FBuildSprite: HudPmx_gShapes[shape] at (x, y) modulated by colour (0xBBGGRR).
    Shape { shape: u8, x: i16, y: i16, colour: u32 },
    /// Hud_BlackThinBox(x, y, w, h).
    ThinBox { x: i16, y: i16, w: i16, h: i16 },
    /// A screen-space quad (front end): corners in order top-left, top-right, bottom-left,
    /// bottom-right with their colours (0xBBGGRR, 0x80 = 1 on textured quads), optionally
    /// textured from VRAM; `abr` is the semi-transparency mode (0 half, 1 add, 2 subtract,
    /// 3 add a quarter).
    Quad { pts: [[i16; 2]; 4], colours: [u32; 4], tex: Option<QuadTex>, semi: bool, abr: u8 },
    /// SetDrawArea in the front end: drawing is clipped to this screen rectangle.
    Clip { x: i16, y: i16, w: i16, h: i16 },
    /// Movie_Play: frame `frame` of the movie file `file` (under MOVIES) fills the screen.
    Movie { file: String, frame: u32, fps: f32 },
    /// The showroom's car (R3DCar_InsertCarFacetMenuII): the car, the camera (the view's
    /// position and rotation rows, 16.16 / 4096 scaled as in races) and the GTE projection
    /// (distance H, screen centre OFX/OFY 16.16); `mirrored` is its reflection in the floor.
    ShowroomCar { car: Box<super::race::CarState>, parts: Vec<u8>, cam_pos: [i32; 3], cam_rot: [i32; 9], h: u32, ofx: i32, ofy: i32, mirrored: bool, level: u32, env: Option<CarEnv> },
    /// The showroom's turntable and spot (R3DCar_Showroom): primitives in the first slot of
    /// the car cache's ordering table, drawn before the cars (in this list's order, as the
    /// overlay: last first).
    ShowroomFloor(Vec<OverlayCmd>),
    /// Draw_DrawDirectScreen: a picture put straight onto the screen at (x, y), opaque.
    Bitmap { x: i16, y: i16, w: u16, h: u16, rgba: std::sync::Arc<Vec<u8>> },
}

/// A quad's texture in VRAM terms: texture page and CLUT ids, texel coordinates per corner.
#[derive(Clone, Copy, Debug)]
pub struct QuadTex {
    pub tpage: u16,
    pub clut: u16,
    pub uv: [[u8; 2]; 4],
}

/// HUD functions whose drawing the host renders from its own port (Hud_RenderMapView,
/// Hud_BlackThinBox's countdown use in Hud_Draw321Num / Hud_Render321Go, Hud_RenderHudView,
/// Hud_RenderTacView): 2D calls made from them are not recorded again.
const NATIVE_HUD: [std::ops::Range<u32>; 4] = [0x800D_840C..0x800D_8690, 0x800D_887C..0x800D_8B84, 0x800D_8EA4..0x800D_981C, 0x800D_981C..0x800D_9938];

fn native_hud(ra: u32) -> bool {
    NATIVE_HUD.iter().any(|r| r.contains(&ra))
}

/// Inner hook for R3DCar_InsertCarFacetII(car): record what the original decided to draw
/// (see [`CarFacet::parse`]) instead of building primitives.
thread_local! {
    /// While the original's R3DCar_InsertCarFacetII runs for its halos (see
    /// `record_car_facet`); and the primitive buffer ranges they were built in.
    static HALO_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HALO_RANGES: std::cell::RefCell<Vec<(u32, u32, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The car body's drawing in R3DCar_InsertCarFacetII / InsertCarFacetMenuII (the host
/// draws the body): the part drawing, the shadow, the environment, and the menu's
/// DrawC_PrimMenu / DrawC_PrimStop.
const CAR_BODY_DRAWING: [u32; 6] = [0x800C_22E0, 0x800C_0AC8, 0x800C_5054, 0x800C_4C68, 0x800C_4034, 0x800C_0A08];

/// A light's glow as the original built it this frame (screen-space primitives) and the
/// world point it belongs to (PS1 16.16), or the car (Car_tObj) it is on.
#[derive(Clone, Debug)]
pub struct Glow {
    pub anchor: [i32; 3],
    pub car: Option<u32>,
    pub cmds: Vec<OverlayCmd>,
    /// A smoke or dust puff (Sfx_BuildSouffleFacet), sorted at its own depth; otherwise a
    /// light's glow.
    pub smoke: bool,
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
    pub glows: Vec<Glow>,
    pub weather: Vec<OverlayCmd>,
    pub sky_overlay: Vec<OverlayCmd>,
    pub night_beam: Option<super::race::NightBeam>,
}

impl OverlayCmd {
    /// The command moved by (dx, dy) on the screen.
    pub fn offset(&self, dx: i16, dy: i16) -> OverlayCmd {
        let mut c = self.clone();
        if dx == 0 && dy == 0 {
            return c;
        }
        match &mut c {
            OverlayCmd::Text { x, y, .. }
            | OverlayCmd::PauseBox { x, y, .. }
            | OverlayCmd::Rect { x, y, .. }
            | OverlayCmd::Shape { x, y, .. }
            | OverlayCmd::ThinBox { x, y, .. }
            | OverlayCmd::Clip { x, y, .. }
            | OverlayCmd::Bitmap { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            OverlayCmd::Quad { pts, .. } => {
                for p in pts {
                    p[0] += dx;
                    p[1] += dy;
                }
            }
            OverlayCmd::ShowroomFloor(v) => {
                for c in v {
                    *c = c.offset(dx, dy);
                }
            }
            OverlayCmd::Movie { .. } | OverlayCmd::ShowroomCar { .. } => {}
        }
        c
    }
}

thread_local! {
    /// While the track's glares are built: where each light's primitives start, and its
    /// position (Flare_Halo / Flare_Halo2's coorddef).
    static GLARE_MARKS: std::cell::RefCell<Vec<(u32, [i32; 3])>> = const { std::cell::RefCell::new(Vec::new()) };
}
const FLARE_HALO: u32 = 0x800C_EBFC;
/// The weather's drawing (rain, snow, lens drops; Render_RenderWorld calls it with the view):
/// skipped in tunnels and when the race has no weather.
const WEATHER_RENDER: u32 = 0x800E_493C;
/// Souffle_InsertFacet(view): the smoke and dust puffs (Sfx_BuildSouffleFacet for each in
/// view), as Render_RenderWorld calls it.
const SOUFFLE_INSERT_FACET: u32 = 0x800B_8ABC;
const SFX_BUILD_SOUFFLE_FACET: u32 = 0x800D_E39C;

fn smoke_marker(c: &mut Cpu, addr: u32) -> bool {
    if addr == SFX_BUILD_SOUFFLE_FACET {
        // Souffle_tISouffle: its position (coorddef) at +12
        let p = c.r[5] + 12;
        let anchor = [c.m.r32(p) as i32, c.m.r32(p + 4) as i32, c.m.r32(p + 8) as i32];
        let at = c.m.r32(0x1F80_0004);
        GLARE_MARKS.with_borrow_mut(|m| m.push((at, anchor)));
    }
    false
}
const FLARE_HALO2: u32 = 0x800C_E290;

fn glare_marker(c: &mut Cpu, addr: u32) -> bool {
    if addr == FLARE_HALO || addr == FLARE_HALO2 {
        let p = c.r[7];
        let anchor = [c.m.r32(p) as i32, c.m.r32(p + 4) as i32, c.m.r32(p + 8) as i32];
        let at = c.m.r32(0x1F80_0004);
        GLARE_MARKS.with_borrow_mut(|m| m.push((at, anchor)));
    }
    false
}

fn record_car_facet(c: &mut Cpu, addr: u32) -> bool {
    if HALO_PASS.get() {
        // the original's car drawing for its halos: the body's primitives are skipped
        if CAR_BODY_DRAWING.contains(&addr) {
            c.r[2] = 0;
            return true;
        }
        return false;
    }
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
    // DrawC_PrimStart sets up the car's drawing (light level, reflection maps chosen from the
    // track position, their colour and scroll) in the car cache on the scratchpad
    let (r, g, b) = (light & 0xFF, (light >> 8) & 0xFF, (light >> 16) & 0xFF);
    let level = ((r + g + b) / 3).max(24);
    c.m.w32(CAR_CACHE + 140, level * 0x01_0101);
    let ok = c.call_nested(DRAWC_PRIM_START, &[R3DCAR_CENTER, car, level, CAR_CACHE]) as i32 >= 0;
    rec.push(ok as u32);
    rec.extend((0..8).map(|i| c.m.r32(CAR_CACHE + 100 + 4 * i)));
    rec.extend([c.m.r32(CAR_CACHE + 132), c.m.r32(CAR_CACHE + 144), c.m.r16(R3DCAR_CENTER + 0x14) as u32]);
    c.capture.extend(rec);
    // then the original itself, which adds the lights' halos (DrawC_PrimHalo: brake lights,
    // headlights) to the primitive buffer, everything else skipped
    let start = c.m.r32(0x1F80_0004);
    HALO_PASS.set(true);
    c.call_nested(R3DCAR_INSERT_CAR_FACET_II, &[car]);
    HALO_PASS.set(false);
    let end = c.m.r32(0x1F80_0004);
    if end > start && end - start < 0x1_0000 {
        HALO_RANGES.with_borrow_mut(|r| r.push((start, end, car)));
    }
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
    /// The reflection maps (DrawC_PrimStart), when the car is drawn.
    pub env: Option<CarEnv>,
}

/// A car's reflection mapping for this frame: two of the track's reflection maps
/// (Track_gReflectionMaps pixmaps: u, v, clut, u, v, tpage, ...), the second one's scroll,
/// their colour (0xBBGGRR) and the camera's yaw around the car (4096 per turn).
#[derive(Clone, Copy, Debug, Default)]
pub struct CarEnv {
    pub a: Option<[u32; 4]>,
    pub b: Option<[u32; 4]>,
    pub scroll: u32,
    pub colour: u32,
    pub yaw: i16,
}

impl CarFacet {
    const WORDS: usize = 3 + 12 + 1 + 57 + 1 + 8 + 3;

    /// Records left by `record_car_facet`.
    fn parse(w: &[u32]) -> Vec<Self> {
        w.chunks_exact(Self::WORDS)
            .map(|r| Self {
                car: r[0],
                light: r[1],
                shadow: (r[2] != 0).then(|| {
                    (std::array::from_fn(|k| std::array::from_fn(|i| r[3 + 3 * k + i] as i32)), std::array::from_fn(|i| (r[15] >> (8 * i)) as u8))
                }),
                parts: r[16..73].iter().map(|&p| p as u8).collect(),
                env: (r[73] != 0).then(|| {
                    let map = |o: usize| (r[o] != 0).then(|| [r[o], r[o + 1], r[o + 2], r[o + 3]]);
                    CarEnv { a: map(74), b: map(78), scroll: r[82], colour: r[83] & 0xFF_FFFF, yaw: r[84] as u16 as i16 }
                }),
            })
            .collect()
    }
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
    last_top: u64,
}

pub const MAIN_LOOP_EXIT_FLAG: u32 = 0x8011_F380;
/// `DRender_tView` the game renders the player view with (id, player, DRender_tCalcView).
pub const RENDER_VIEW: u32 = 0x8011_8234;
/// Draw_gViews: per player view (200 bytes) its OT, size (+12, +14) and a drawing
/// environment per buffer (+8, 92 bytes each).
const DRAW_VIEWS: u32 = 0x8011_FF44;
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
/// The font: CLUT of colour 0 (x in gp + 4864, y in shpfontclut + 14), the current CLUT
/// (gp + 4880) and the space width (gp + 4882).
const FONT_CLUT_X: u32 = 0x8013_D83C + 4864;
const FONT_CLUT_Y: u32 = 0x8012_10AE;
const FONT_CURRENT_CLUT: u32 = 0x8013_D83C + 4880;
const FONT_SPACE: u32 = 0x8013_D83C + 4882;
/// Font_TextXY's blitter (gp+6328) and the two the game uses.
const FONT_BLITTER: u32 = 0x8013_D83C + 6328;
const FONT_BLIT: u32 = 0x800C_C00C;
const FONT_UPSIDE_DOWN_BLIT: u32 = 0x8004_F49C;
/// DrawC_PrimStart(vertex, car, light level, cache) and its arguments' homes.
const DRAWC_PRIM_START: u32 = 0x800B_FAC8;
const R3DCAR_CENTER: u32 = 0x8013_E674;
const CAR_CACHE: u32 = 0x1F80_0000;
/// BWorld_OnyxBuildFacets' set-up: SetupBuildMatrices(view, dcache), then
/// DrawW_WorldSetUpMatrix(gWorldMat, dcache + 0x14), then SetupChunkBuildList(view) fills
/// BWorld_gChunkBuildList (count kept at gp + 608): the chunks drawn this frame.
const SETUP_BUILD_MATRICES: u32 = 0x8007_E2E8;
const HRZ_SET_PSX_MATRIX: u32 = 0x800D_159C;
const BWORLD_BUILD_GLARE_EFFECTS: u32 = 0x8007_E068;
const TRACK_CHUNK_LIST: u32 = 0x8013_E79C;
const DRAWC_SHADOW_PRIM_CLIP: u32 = 0x800C_5054;
const FLARE_SUN: u32 = 0x800C_FF30;
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
/// The showroom turntable: its ring (Fe3D_InitShowroom), texture words and light colours.
const SHOWROOM_RING: u32 = 0x8005_181C;
const SHOWROOM_TEX: u32 = 0x8012_22CC;
const SHOWROOM_RIM: u32 = 0x8005_1754;
const SHOWROOM_LIGHTS: u32 = 0x8012_07A4;
const TICKS: u32 = 0x8013_EF8C;
const GP: u32 = 0x8013_D83C;
/// A string in NFS4.EXE's data to name the video handles' heap blocks ("audio heap").
const VIDEO_BLOCK_NAME: u32 = 0x8005_6F08;
/// Front-end routines that build GPU primitives themselves (the video wall's DrawTV and
/// DrawTVLines): run as they are and their primitives recorded (`capture_prims`).
const CAPTURED_FE_DRAWING: [u32; 9] = [
    0x8002_24AC, // DrawTV
    0x800C_ECD0, // Flare_2DHalo (the glow beside a menu's selected row, DrawLeftFlare)
    0x8002_4C4C, // DrawSlider (car statistics, options sliders)
    0x8002_2158, // DrawTVLines
    0x8001_C25C, // SubtractiveBox
    0x8003_79FC, // tScreenMain::DrawDropShadow
    0x8004_C2E0, // DrawShape_SubtractNFS4RectEdges
    0x8004_F1C4, // PSXDrawGouraudSquare
    0x8004_F2B4, // PSXDrawTransGouraudSquare
];
/// Return address of the engine flip's PutDispEnv call.
const FLIP_PUT_DISP_ENV_RA: u32 = 0x800B_F164;

impl GameHost {
    /// 16 palette entries of CLUT id `clut` from VRAM.
    fn clut(&self, clut: u32) -> [u16; 16] {
        let (x, y) = ((clut & 0x3F) as usize * 16, ((clut >> 6) & 0x1FF) as usize);
        std::array::from_fn(|i| self.vram.get(y * VRAM_W + x + i).copied().unwrap_or(0))
    }

    /// Move the virtual clock forward, queueing the timer interrupts that fall in between.
    fn advance_to(&mut self, c: &mut Cpu, t: u64) {
        c.pending_ticks += (t / TICK - self.clock / TICK) as u32;
        // the sound hardware runs along (44.1 kHz of the virtual clock)
        let samples = t * super::spu::RATE / 1920 - self.clock * super::spu::RATE / 1920;
        self.clock = t;
        if let Some(spu) = c.m.bus.as_mut().and_then(|b| b.as_any().downcast_mut::<super::spu::Spu>()) {
            spu.run(samples as usize);
            crate::audio::push_spu(&spu.out);
            spu.out.clear();
            if std::env::var_os("PS1_VOICELOG").is_some() && t / 1920 != (t - samples * 1920 / super::spu::RATE) / 1920 {
                eprintln!("[{t:>8}] {}", spu.describe());
                if let Some(p) = std::env::var_os("PS1_SPURAM") {
                    std::fs::write(p, &spu.ram).ok();
                }
            }
            if std::env::var_os("PS1_SNDLOG").is_some() && spu.keyons != 0 {
                eprintln!("[{t:>8}] keyon {:06x}", std::mem::take(&mut spu.keyons));
            }
        }
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: HashMap::new(),
            mounts: Vec::new(),
            next_handle: 0x100,
            memcard: super::memcard::MemCard::new(),
            showroom_car: None,
            videos: Default::default(),
            idles: 0,
            mode: Mode::FrontEnd,
            call_log: std::env::var("PS1_CALLLOG").ok().map(|v| v.split(',').filter_map(|a| u32::from_str_radix(a.trim().trim_start_matches("0x"), 16).ok()).collect()),
            file_ops: HashMap::new(),
            next_op: 1,
            clock: 0,
            display: [0; 4],
            view_env: [0; 9],
            car_facets: Vec::new(),
            overlay: Vec::new(),
            loading: Vec::new(),
            loading_shown: 0,
            overlay_ot: Vec::new(),
            current_ot: 0,
            hud_prims: std::env::var_os("NFSHS_OWN_HUD").is_none(),
            otags: 0,
            frame_lists: Vec::new(),
            last_hud: Vec::new(),
            sky_overlay: Vec::new(),
            weather: Vec::new(),
            last_sky: Vec::new(),
            showroom_shadow: Vec::new(),
            car_glows: Vec::new(),
            track_glows: Vec::new(),
            paused_buffers: HashMap::new(),
            views: Vec::new(),
            prim_offset: [0, 0],
            area_tl: [0, 0],
            draw_origin: [0, 0],
            draw_envs: HashMap::new(),
            from_main: false,
            rcnt: 0,
            log: Vec::new(),
            vram: vec![0; VRAM_W * VRAM_H],
            vram_dirty: true,
            capturing: false,
            recorded_packets: Default::default(),
            draw_mode: 0,
            vram_shared: None,
            pad_buffers: [0; 2],
            callbacks: HashMap::new(),
            next_event: 0xF000_0001,
            frame: FrameDriver::default(),
        }
    }

    /// A game frame is done (race loop top or the front end's flip): hand this frame's state
    /// to the frame callback and take the pad input for the next.
    /// Frames of the race's loading screen for the fields the loading has taken since the
    /// last one (the original shows it while it reads the track from the CD).
    fn loading_frames(&mut self, c: &mut Cpu) {
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
            };
            if self.frame.on_frame.as_mut().and_then(|f| f(c, info)).is_none() {
                c.tick_limit = 0;
            }
        }
    }

    /// A new screen put straight into the display buffer: the last one is shown until now.
    fn new_direct_screen(&mut self, c: &mut Cpu) {
        if !self.loading.is_empty() {
            self.loading_frames(c);
        }
        self.loading.clear();
        self.loading_shown = self.clock;
    }

    /// The GTE as DrawC_ShowroomPrims / DrawC_SpotPrims set it for `view` (DRender_tView):
    /// rotation the transpose of its matrix (+68, 16.16) with the second row negated,
    /// translation its coordinates (+56) at 1/256 with y negated.
    fn showroom_gte(c: &mut Cpu, view: u32) {
        let m = |c: &mut Cpu, i: u32| (c.m.r32(view + 68 + 4 * i) as i32 >> 4) as i16;
        let r: [[i16; 3]; 3] = [[m(c, 0), m(c, 3), m(c, 6)], [-m(c, 1), -m(c, 4), -m(c, 7)], [m(c, 2), m(c, 5), m(c, 8)]];
        let pair = |a: i16, b: i16| a as u16 as u32 | (b as u16 as u32) << 16;
        c.gte_write(32, pair(r[0][0], r[0][1]));
        c.gte_write(33, pair(r[0][2], r[1][0]));
        c.gte_write(34, pair(r[1][1], r[1][2]));
        c.gte_write(35, pair(r[2][0], r[2][1]));
        c.gte_write(36, r[2][2] as u16 as u32);
        let t = |c: &mut Cpu, i: u32| c.m.r32(view + 56 + 4 * i) as i32 >> 8;
        let (tx, ty, tz) = (t(c, 0), -t(c, 1), t(c, 2));
        c.gte_write(37, tx as u32);
        c.gte_write(38, ty as u32);
        c.gte_write(39, tz as u32);
    }

    /// RTPS: a vertex to the screen with the GTE as set up.
    fn gte_project(c: &mut Cpu, v: [i16; 3]) -> [i16; 2] {
        c.gte_write(0, v[0] as u16 as u32 | (v[1] as u16 as u32) << 16);
        c.gte_write(1, v[2] as u16 as u32);
        c.gte_command(0x0018_0001);
        let sxy = c.gte_read(14);
        [sxy as i16, (sxy >> 16) as i16]
    }

    /// A shape (shapetbl) in RAM as RGBA, opaque.
    fn shape_rgba(c: &mut Cpu, shape: u32) -> Option<(u16, u16, Vec<u8>)> {
        let (w, h) = (c.m.r16(shape + 4), c.m.r16(shape + 6));
        let len = 16 + w as u32 * h as u32 * 2 + 16 + 512 + 64;
        if w == 0 || h == 0 || len > 0x10_0000 {
            return None;
        }
        let mut d: Vec<u8> = (0..len).map(|i| c.m.r8(shape + i)).collect();
        // flag 0x08 of the type does not change the pixels (Texture_Vramcf masks it)
        d[0] &= 0xF7;
        let s = crate::formats::psh::Psh::parse_shape(&d, 0, String::new()).ok()?;
        if !matches!(s.kind, 0x40..=0x42) {
            log::warn!("direct screen picture of type {:#x} not drawn", s.kind);
            return None;
        }
        let mut rgba = s.rgba;
        for px in rgba.chunks_mut(4) {
            px[3] = 255;
        }
        Some((w, h, rgba))
    }

    /// The race is paused: its menu is drawn by the host (the paused game redraws only what
    /// changes over the picture it keeps).
    fn paused(c: &mut Cpu) -> bool {
        c.m.r32(super::race::addr::SIM_PAUSED) != 0
    }

    /// PS1_MISSLOG: the primitives in the front end's ordering table (from `start`) that no
    /// recording took, by code and first vertex (drawing the original made that is not shown).
    fn report_unrecorded(&mut self, c: &mut Cpu, start: u32) {
        let mut missed = std::collections::BTreeMap::<(u8, i16, i16), u32>::new();
        let (mut a, mut n) = (start, 0);
        while n < 20_000 {
            let w = c.m.r32(a);
            let len = w >> 24;
            if len > 0 && !self.recorded_packets.contains(&a) {
                let code = (c.m.r32(a + 4) >> 24) as u8;
                if !(0xE1..=0xE6).contains(&code) && code != 0 {
                    // (the first vertex: after the colour word)
                    let v = c.m.r32(a + 8);
                    *missed.entry((code, v as i16, (v >> 16) as i16)).or_default() += 1;
                }
            }
            n += 1;
            if w & 0xFF_FFFF == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | (w & 0xFF_FFFF);
        }
        if std::env::var("PS1_MISSLOG").as_deref() == Ok("2") {
            eprintln!("[{:>8}] ordering table {start:#x}: {n} entries, {} recorded packets", self.clock, self.recorded_packets.len());
        }
        if !missed.is_empty() {
            let list: Vec<String> = missed.iter().take(24).map(|((code, x, y), k)| format!("{code:#04x}@{x},{y}x{k}")).collect();
            eprintln!("[{:>8}] unrecorded front-end primitives: {} kinds: {}", self.clock, missed.len(), list.join(" "));
        }
        self.recorded_packets.clear();
    }

    /// Packets in an ordering table.
    fn ot_len(c: &mut Cpu, start: u32) -> u32 {
        let (mut a, mut n) = (start, 0);
        while n < 20_000 {
            let next = c.m.r32(a) & 0xFF_FFFF;
            n += 1;
            if next == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | next;
        }
        n
    }

    /// Walk an ordering table from `start` and record its primitives (a race's HUD view).
    fn record_ot(&mut self, c: &mut Cpu, start: u32) {
        let before = self.overlay.len();
        let mut a = start;
        self.prim_offset = [0, 0];
        // each list starts unclipped (its own drawing environment, if any, clips it)
        self.overlay.push(OverlayCmd::Clip { x: 0, y: 0, w: 1024, h: 512 });
        for _ in 0..20_000 {
            let tag = c.m.r32(a);
            let (len, next) = (tag >> 24, tag & 0xFF_FFFF);
            if len > 0 {
                let code = c.m.r8(a + 7);
                // a view's drawing environment (SetDrawEnv's packet): the offset from its E5
                // word, the clip SetDrawEnv was given for that offset (the packet's own area
                // words are not used by the game)
                let e5 = (code >= 0xE0).then(|| (0..len).map(|k| c.m.r32(a + 4 + 4 * k)).find(|w| w >> 24 == 0xE5)).flatten();
                if let Some(e5) = e5 {
                    let sx = |v: u32| ((v as i32) << 21) >> 21;
                    let ofs = [sx(e5 & 0x7FF), sx((e5 >> 11) & 0x7FF)];
                    let [ox, oy] = self.draw_origin;
                    let clip = self.draw_envs.values().find(|(_, o)| o[0] as i32 == ofs[0] && o[1] as i32 == ofs[1]).map(|e| e.0);
                    match clip {
                        Some(cl) => self.overlay.push(OverlayCmd::Clip { x: (cl[0] as i32 - ox) as i16, y: (cl[1] as i32 - oy) as i16, w: cl[2], h: cl[3] }),
                        None => self.overlay.push(OverlayCmd::Clip { x: 0, y: 0, w: 1024, h: 512 }),
                    }
                    self.prim_offset = [(ofs[0] - ox) as i16, (ofs[1] - oy) as i16];
                    for k in 0..len {
                        let w = c.m.r32(a + 4 + 4 * k);
                        if w >> 24 == 0xE1 {
                            self.draw_mode = w as u16 & 0x1FF;
                        }
                    }
                } else if code >= 0xE0 {
                    // environment commands, several to a packet
                    for k in 0..len {
                        let w = c.m.r32(a + 4 + 4 * k);
                        self.environment(w);
                    }
                } else {
                    self.parse_prim(c, a + 4, len);
                }
            }
            if next == 0xFF_FFFF {
                break;
            }
            a = 0x8000_0000 | next;
        }
        if std::env::var_os("NFSHS_HUDLOG").is_some() {
            eprintln!("hud view {start:#x}: {} commands, display {:?}", self.overlay.len() - before, self.display);
            if std::env::var("NFSHS_HUDLOG").as_deref() == Ok("2") {
                for o in &self.overlay[before..] {
                    eprintln!("    {}", format!("{o:?}").chars().take(160).collect::<String>());
                }
            }
        }
    }

    /// GP0 E1..E6 in a recorded view: draw mode, drawing area (a clip), drawing offset.
    fn environment(&mut self, w: u32) {
        let [ox, oy] = self.draw_origin;
        match w >> 24 {
            0xE1 => self.draw_mode = w as u16 & 0x1FF,
            0xE3 => self.area_tl = [(w & 0x3FF) as i32 - ox, ((w >> 10) & 0x3FF) as i32 - oy],
            0xE4 => {
                let (x1, y1) = ((w & 0x3FF) as i32 - ox, ((w >> 10) & 0x3FF) as i32 - oy);
                let [x0, y0] = self.area_tl;
                self.overlay.push(OverlayCmd::Clip { x: x0 as i16, y: y0 as i16, w: (x1 - x0 + 1) as i16, h: (y1 - y0 + 1) as i16 });
            }
            0xE5 => {
                let sx = |v: u32| ((v as i32) << 21) >> 21;
                self.prim_offset = [(sx(w & 0x7FF) - ox) as i16, (sx((w >> 11) & 0x7FF) - oy) as i16];
            }
            _ => {}
        }
    }

    fn frame_top(&mut self, c: &mut Cpu) -> Option<[u16; 2]> {
        self.otags = 0;
        // the recorded HUD views (in drawing order; overlays are kept last-drawn first): all
        // but the world's, or a paused game's menu view over the last full frame's HUD
        if self.mode == Mode::Race && self.hud_prims && !Self::paused(c) {
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
        // the screen shown while loading, until this frame
        if !self.loading.is_empty() {
            self.loading_frames(c);
            self.loading.clear();
        }
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
                            other => format!("{}", format!("{other:?}").chars().take(60).collect::<String>()),
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
            vram: (self.mode == Mode::FrontEnd || self.hud_prims).then(|| {
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
            Some(buttons) => {
                self.frame.frames_run += 1;
                // each port its own controller (the front end takes either player's)
                for (b, buttons) in self.pad_buffers.into_iter().zip(buttons) {
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
        input
    }

    /// The front end's shape emitter (0x8004E74C) as quads: the shape is cut into strips that
    /// stay inside one texture page, each a gouraud-textured quad.
    #[allow(clippy::too_many_arguments)]
    fn shape_quads(&mut self, c: &mut Cpu, info: u32, flags: u32, x: i32, y: i32, colours: [u32; 4], abr: u32) {
        let clut = c.m.r32(info + 4) as u16;
        let bpp = (c.m.r8(info + 8) as i32).max(1);
        let mode = (c.m.r8(info + 9) & 3) as u16;
        let w = c.m.r16(info + 16) as i16 as i32;
        let h0 = c.m.r16(info + 18) as i16 as i32;
        let vx = c.m.r16(info + 24) as i32;
        let w26 = c.m.r16(info + 26);
        let (mut top, mut h) = (y, h0);
        let mut v0 = (w26 & 0xFF) as i32;
        if flags & 2 != 0 {
            top += h0;
            h = -h0;
            v0 -= 1;
        }
        let bottom = top + h;
        let mut s1 = 0;
        while s1 < w {
            let first = (vx << 4) / bpp;
            let col = vx + ((s1 * bpp) >> 4);
            let page = ((col & !63) << 4) / bpp;
            let u = s1 + first - page;
            let mut width = (255 - u).min(w - s1);
            let tpage = (mode << 7) | (((abr & 3) as u16) << 5) | ((w26 & 0x100) >> 4) | (((col & 0x3C0) >> 6) as u16) | ((w26 & 0x200) << 2);
            let (mut u0, mut extra) = (u, 0);
            if flags & 4 != 0 && h0 < 255 {
                u0 = u - 1;
                extra = 1;
            }
            if width <= 0 {
                width = 1;
            }
            let (xa, xb) = if flags & 4 != 0 { (w + x - s1 + extra - 1, w + x - (s1 + width) + extra - 1) } else { (s1 + x, s1 + x + width) };
            let (va, vb) = (v0 as u8, (h0 + v0) as u8);
            let (ua, ub) = (u0 as u8, (u0 + width) as u8);
            self.overlay.push(OverlayCmd::Quad {
                pts: [[xa as i16, top as i16], [xb as i16, top as i16], [xa as i16, bottom as i16], [xb as i16, bottom as i16]],
                colours,
                tex: Some(QuadTex { tpage, clut, uv: [[ua, va], [ub, va], [ua, vb], [ub, vb]] }),
                semi: flags & 1 != 0,
                abr: ((tpage >> 5) & 3) as u8,
            });
            s1 += width;
        }
    }

    /// Font_TextXY in the front end: the blitter's glyphs (of the current font's 4-bit
    /// picture) as quads. Font_Blit: sprites in CLUT gp+4880 and colour gp+4872, blended;
    /// FontUpsideDownBlit (the menu's reflection in the showroom floor): flipped opaque quads
    /// in its own CLUT and colour.
    fn font_quads(&mut self, c: &mut Cpu, text: u32, x: i32, y: i32) {
        let upside_down = match c.m.r32(FONT_BLITTER) {
            FONT_UPSIDE_DOWN_BLIT => true,
            FONT_BLIT => false,
            b => {
                log::warn!("front-end text with an unknown blitter {b:#x}");
                false
            }
        };
        let table = c.m.r32(0x8013_6F14);
        let count = c.m.r32(0x8013_6F04);
        let shape = c.m.r32(0x8013_6F18);
        if table == 0 || shape == 0 {
            return;
        }
        let pos = c.m.r32(shape + 12);
        let (fx, fy) = ((pos & 0xFFF) as u16, ((pos >> 16) & 0xFFF) as u16);
        let tpage = (fx / 64) | ((fy / 256) << 4);
        let (clut, colour) = if upside_down {
            (c.m.r16(0x8013_EB4C), c.m.r32(0x8013_EB44) & 0xFF_FFFF)
        } else {
            (c.m.r16(FONT_CURRENT_CLUT), c.m.r32(0x8013_D83C + 4872) & 0xFF_FFFF)
        };
        let space = c.m.r8(FONT_SPACE) as i32;
        let code = |c: &mut Cpu, a: u32| c.m.r8(a) as u16 | (c.m.r8(a + 1) as u16) << 8;
        let mut x = x;
        for i in 0..256 {
            let ch = c.m.r8(text + i) as u16;
            if ch == 0 {
                break;
            }
            if ch < 33 {
                x += space;
                continue;
            }
            let k = ch as u32 - 32;
            let direct = table + 11 * k;
            let g = if k < count && code(c, direct) == ch { Some(direct) } else { (0..count).map(|k| table + 11 * k).find(|&a| code(c, a) == ch) };
            let Some(g) = g else { continue };
            let (w, h) = (c.m.r8(g + 2) as i32, c.m.r8(g + 3) as i32);
            let (u, v) = (code(c, g + 4) as i32, code(c, g + 6) as i32);
            let (adv, dx, dy) = (c.m.r8(g + 8) as i8 as i32, c.m.r8(g + 9) as i8 as i32, c.m.r8(g + 10) as i8 as i32);
            if w > 0 && h > 0 && upside_down {
                // the glyph's top row (from one texel above it) at the bottom, around y + 5
                let (u0, v0) = (u as u8, (((fy as i32 + v) & 0xFF) - 1) as u8);
                let (u1, v1) = ((u + w) as u8, (v0 as i32 + h) as u8);
                let (top, bottom) = (y + 5 - h - dy, y + 5 - dy);
                let (l, r) = (x + dx, x + dx + w);
                self.overlay.push(OverlayCmd::Quad {
                    pts: [[l, bottom], [r, bottom], [l, top], [r, top]].map(|[a, b]| [a as i16, b as i16]),
                    colours: [colour; 4],
                    tex: Some(QuadTex { tpage, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }),
                    semi: false,
                    abr: 0,
                });
            } else if w > 0 && h > 0 {
                let (u0, v0) = (u as u8, ((fy as i32 + v) & 0xFF) as u8);
                let (u1, v1) = ((u + w) as u8, (v0 as i32 + h) as u8);
                self.overlay.push(OverlayCmd::Quad {
                    pts: rect_pts(x + dx, y + dy, w, h),
                    colours: [colour; 4],
                    tex: Some(QuadTex { tpage, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }),
                    semi: true,
                    abr: 0,
                });
            }
            x += adv;
        }
    }

    /// Run a front-end drawing routine that builds its own primitives (DrawTV and friends) and
    /// record what it appended to the primitive buffer (scratchpad 0x1F800004) as quads, in the
    /// order it built them.
    fn capture_prims(&mut self, c: &mut Cpu, addr: u32) -> u32 {
        let start = c.m.r32(0x1F80_0004);
        let args = [c.r[4], c.r[5], c.r[6], c.r[7]];
        // arguments past the fourth are on the caller's stack: copy them to where the nested
        // call's frame (see Cpu::call_nested) has them
        let (sp, nested_sp) = (c.r[29], (c.r[29] - 0x100) & !7);
        for i in 0..16 {
            let w = c.m.r32(sp + 16 + 4 * i);
            c.m.w32(nested_sp + 16 + 4 * i, w);
        }
        self.capturing = true;
        let ret = c.call_nested(addr, &args);
        self.capturing = false;
        let end = c.m.r32(0x1F80_0004);
        self.record_packets(c, start, end);
        ret
    }

    /// Record the GPU packets built in the primitive buffer from `start` to `end`.
    fn record_packets(&mut self, c: &mut Cpu, start: u32, end: u32) {
        let mut packets = Vec::new();
        let mut p = start;
        while p < end && end - p < 0x10_0000 {
            let len = c.m.r32(p) >> 24;
            if len == 0 {
                p += 4;
                continue;
            }
            packets.push((p, len));
            self.recorded_packets.insert(p);
            p += 4 + 4 * len;
        }
        // the packets go into the ordering table head first, so they are drawn in reverse:
        // a draw mode (DR_MODE, E1) applies to the packets added before it. Resolve each
        // packet's mode in drawing order, then record in the order added.
        let entry_mode = self.draw_mode;
        let mut modes = vec![entry_mode; packets.len()];
        let mut mode = entry_mode;
        for (i, &(p, _)) in packets.iter().enumerate().rev() {
            let w0 = c.m.r32(p + 4);
            if w0 >> 24 == 0xE1 {
                mode = w0 as u16 & 0x1FF;
            }
            modes[i] = mode;
        }
        for (i, &(p, len)) in packets.iter().enumerate() {
            self.draw_mode = modes[i];
            if c.m.r32(p + 4) >> 24 != 0xE1 {
                self.parse_prim(c, p + 4, len);
            }
        }
        self.draw_mode = entry_mode;
    }

    /// Points moved by the recorded view's drawing offset.
    fn offset_pts(&self, pts: [[i16; 2]; 4]) -> [[i16; 2]; 4] {
        pts.map(|[x, y]| [x + self.prim_offset[0], y + self.prim_offset[1]])
    }

    /// One GPU primitive (its command words at `a`) as a quad, as far as the front end uses them.
    fn parse_prim(&mut self, c: &mut Cpu, a: u32, len: u32) {
        let w = |c: &mut Cpu, k: u32| c.m.r32(a + 4 * k);
        let w0 = w(c, 0);
        let code = (w0 >> 24) as u8;
        let semi = code & 2 != 0;
        let raw = code & 1 != 0;
        let xy = |v: u32| [v as u16 as i16, (v >> 16) as u16 as i16];
        match code {
            0xE1 => self.draw_mode = w0 as u16 & 0x1FF,
            0x20..=0x3F => {
                let textured = code & 4 != 0;
                let gouraud = code & 0x10 != 0;
                let n = if code & 8 != 0 { 4 } else { 3 };
                // words per vertex: colour (gouraud, after the first), xy, uv (textured)
                let per = 1 + u32::from(textured) + u32::from(gouraud);
                let mut pts = [[0i16; 2]; 4];
                let mut colours = [w0 & 0xFF_FFFF; 4];
                let mut uv = [[0u8; 2]; 4];
                let (mut clut, mut tpage) = (0u16, self.draw_mode);
                for k in 0..n {
                    let base = 1 + per * k as u32;
                    if gouraud && k > 0 {
                        colours[k] = w(c, base - 1) & 0xFF_FFFF;
                    }
                    pts[k] = xy(w(c, base));
                    if textured {
                        let t = w(c, base + 1);
                        uv[k] = [t as u8, (t >> 8) as u8];
                        match k {
                            0 => clut = (t >> 16) as u16,
                            1 => tpage = (t >> 16) as u16,
                            _ => {}
                        }
                    }
                }
                if len < per * n as u32 {
                    return;
                }
                if n == 3 {
                    pts[3] = pts[2];
                    colours[3] = colours[2];
                    uv[3] = uv[2];
                }
                if textured {
                    self.draw_mode = tpage & 0x1FF;
                }
                if textured && raw {
                    colours = [0x80_8080; 4];
                }
                let abr = (if textured { tpage } else { self.draw_mode } >> 5) as u8 & 3;
                let tex = textured.then_some(QuadTex { tpage, clut, uv });
                let pts = self.offset_pts(pts);
                self.overlay.push(OverlayCmd::Quad { pts, colours, tex, semi, abr });
            }
            0x60..=0x7F => {
                // TILE / SPRT (variable or 1, 8, 16 pixels): the draw mode's texture page
                let textured = code & 4 != 0;
                let [x, y] = xy(w(c, 1));
                let (mut tw, mut th, mut uv0, mut clut) = (0i32, 0i32, [0u8; 2], 0u16);
                let mut k = 2;
                if textured {
                    let t = w(c, 2);
                    uv0 = [t as u8, (t >> 8) as u8];
                    clut = (t >> 16) as u16;
                    k = 3;
                }
                match (code >> 3) & 3 {
                    0 => {
                        let s = w(c, k);
                        tw = (s & 0xFFFF) as i32;
                        th = (s >> 16) as i32;
                    }
                    1 => (tw, th) = (1, 1),
                    2 => (tw, th) = (8, 8),
                    _ => (tw, th) = (16, 16),
                }
                let (x, y) = (x as i32, y as i32);
                let colours = if textured && raw { [0x80_8080; 4] } else { [w0 & 0xFF_FFFF; 4] };
                let tex = textured.then(|| {
                    let (u0, v0) = (uv0[0], uv0[1]);
                    let (u1, v1) = (u0.wrapping_add(tw as u8), v0.wrapping_add(th as u8));
                    QuadTex { tpage: self.draw_mode, clut, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]] }
                });
                let abr = (self.draw_mode >> 5) as u8 & 3;
                let pts = self.offset_pts(rect_pts(x, y, tw, th));
                self.overlay.push(OverlayCmd::Quad { pts, colours, tex, semi, abr });
            }
            0x40..=0x5F if code & 8 == 0 => {
                // a line (one segment): a pixel-wide quad
                let gouraud = code & 0x10 != 0;
                let (c0, p0) = (w0 & 0xFF_FFFF, xy(w(c, 1)));
                let (c1, p1) = if gouraud { (w(c, 2) & 0xFF_FFFF, xy(w(c, 3))) } else { (c0, xy(w(c, 2))) };
                let (dx, dy) = ((p1[0] - p0[0]) as f32, (p1[1] - p0[1]) as f32);
                let n = (dx * dx + dy * dy).sqrt().max(1.0);
                let (nx, ny) = ((-dy / n * 0.5).round() as i16, (dx / n * 0.5).round() as i16);
                let (nx, ny) = if nx == 0 && ny == 0 { (0, 1) } else { (nx, ny) };
                let pts = [[p0[0], p0[1]], [p1[0], p1[1]], [p0[0] + nx, p0[1] + ny], [p1[0] + nx, p1[1] + ny]];
                let abr = (self.draw_mode >> 5) as u8 & 3;
                let pts = self.offset_pts(pts);
                self.overlay.push(OverlayCmd::Quad { pts, colours: [c0, c1, c0, c1], tex: None, semi, abr });
            }
            _ => {
                if std::env::var_os("PS1_PRIMLOG").is_some() {
                    eprintln!("prim {code:#04x} len {len} not drawn");
                }
            }
        }
    }

    /// Movie_Play: show the movie's frames (at the stream's rate: the CD gives 150 sectors a
    /// second) through the frame callback. Any button stops it; Start (or the game's flag
    /// 0x80052EBE) marks it skipped (0x80052EBC), as the original's loop does.
    fn play_movie(&mut self, c: &mut Cpu, index: u32) {
        let name_at = c.m.r32(0x8005_2E68 + 4 * index);
        if name_at == 0 || self.frame.on_frame.is_none() {
            return;
        }
        let file = c.m.cstr(name_at);
        let Some(data) = self.lookup(&format!("MOVIES\\{file}")) else {
            self.log.push(format!("movie {file}: not found"));
            return;
        };
        let movie = crate::formats::str::Movie::parse(&data);
        if movie.frames.is_empty() {
            return;
        }
        let fps = 150.0 * movie.frames.len() as f32 / (data.len() / 2336) as f32;
        let fields_per_frame = (60.0 / fps).round().max(1.0) as u32;
        'play: for k in 0..movie.frames.len() as u32 {
            for _ in 0..fields_per_frame {
                self.overlay.clear();
                self.overlay.push(OverlayCmd::Movie { file: file.clone(), frame: k, fps });
                let Some(pads) = self.frame_top(c) else { break 'play };
                let buttons = pads[0] | pads[1];
                if buttons != 0 {
                    if buttons == 1 << 3 || c.m.r8(0x8005_2EBE) != 0 {
                        c.m.w16(0x8005_2EBC, 1);
                    }
                    break 'play;
                }
            }
        }
        self.overlay.clear();
    }

    fn new_op(&mut self, result: u32, user: u32) -> u32 {
        let op = self.next_op;
        self.next_op += 1;
        self.file_ops.insert(op, (result, user));
        op
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
    fn idle(&mut self, c: &mut Cpu) {
        // a busy-wait on the timer: let one tick pass
        self.advance_to(c, (self.clock / TICK + 1) * TICK);
        self.idles += 1;
        // PS1_IDLE_DUMP=file: after this many idles the RAM is written there (and the CPU's
        // registers printed) and the program ends: for finding what a hang spins on
        if self.idles == 5000 {
            if let Some(p) = std::env::var_os("PS1_IDLE_DUMP") {
                std::fs::write(&p, &c.m.ram).ok();
                eprintln!("idle dump: regs {:x?} stack {:x?}", c.r, c.stack);
                std::process::exit(4);
            }
        }
        if self.idles % 500 == 0 && std::env::var_os("PS1_IDLELOG").is_some() {
            eprintln!("idle {} at clock {}: stack {:x?} recent {:x?}", self.idles, self.clock, c.stack, &c.recent[..4]);
        }
    }

    fn call(&mut self, c: &mut Cpu, addr: u32) -> bool {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        // the commands recorded since the last host call went into the entry current then
        while self.overlay_ot.len() < self.overlay.len() {
            self.overlay_ot.push(self.current_ot);
        }
        self.overlay_ot.truncate(self.overlay.len());
        self.current_ot = if std::env::var_os("PS1_OTLOG_PRIM").is_some() { c.m.r32(0x1F80_0004) & 0xFFF_0000 } else { c.m.r32(0x1F80_0000) };
        if let Some(log) = &self.call_log {
            if log.contains(&addr) && std::env::var_os("PS1_CALLSTACK").is_some() {
                eprintln!("  stack {:x?}", c.stack);
            }
            if log.contains(&addr) {
                eprintln!("[{:>8}] call {addr:#x}({a0:#x}, {a1:#x}, {a2:#x}, {a3:#x}) from {:#x}", self.clock, c.r[31]);
            }
        }
        // drawing recorded without a frame to take it (should not happen): don't let it grow
        if self.overlay.len() > 200_000 {
            self.overlay.clear();
        }
        if self.mode == Mode::FrontEnd && !self.capturing && addr == 0x8004_22DC && c.r[31] == 0x8004_1E5C {
            // tScreenTrackSelect::DrawVideoWall from DrawBackground, which has just built the
            // spinning track's two video quads (POLY_FT4, frames streamed to VRAM x 512) in the
            // primitive buffer itself: record them, then run the wall as usual
            let end = c.m.r32(0x1F80_0004);
            let quad = |c: &mut Cpu, p: u32| c.m.r32(p) >> 24 == 9 && c.m.r8(p + 7) & 0xFC == 0x2C;
            if std::env::var_os("PS1_VIDLOG").is_some() {
                eprintln!("track video quads at {end:#x}: {:x?}", (0..20).map(|i| c.m.r32(end.wrapping_sub(80) + 4 * i)).collect::<Vec<_>>());
            }
            if quad(c, end.wrapping_sub(80)) && quad(c, end.wrapping_sub(40)) {
                self.record_packets(c, end - 80, end);
            }
        }
        if self.mode == Mode::FrontEnd && !self.capturing && CAPTURED_FE_DRAWING.contains(&addr) {
            c.r[2] = self.capture_prims(c, addr);
            return true;
        }
        if (0x8005_11B0..=0x8005_1528).contains(&addr) {
            let clock = self.clock;
            let file = if addr == 0x8005_1380 { c.m.cstr(a1) } else { String::new() };
            let mut videos = std::mem::take(&mut self.videos);
            let action = videos.call(addr, [a0, a1, a2, a3], |_| file.clone(), |f| self.lookup(f), clock);
            self.videos = videos;
            if std::env::var_os("PS1_VIDLOG").is_some() && addr == 0x8005_1480 {
                let o = c.r[23];
                let f: Vec<i32> = [100u32, 104, 110, 112, 114, 116, 120, 124, 668].iter().map(|&k| if k == 110 || (112..=116).contains(&k) { c.m.r16(o + k) as i16 as i32 } else { c.m.r32(o + k) as i32 }).collect();
                eprintln!("video state {:?} ticks {} fields 100,104,110,112,114,116,120,124,668 = {f:?}", action.as_ref().map(|a| match a { super::video::VideoAction::Return(r) => *r as i32, _ => -1 }), c.m.r32(0x8013_EF8C));
            }
            if std::env::var_os("PS1_VIDLOG").is_some() && addr != 0x8005_1480 && addr != 0x8005_1528 {
                eprintln!("video {addr:#x} handle {a0:#x} {file} -> {}", match &action { Some(super::video::VideoAction::Return(r)) => format!("{r}"), Some(super::video::VideoAction::Create) => "create".into(), _ => "upload".into() });
            }
            match action {
                Some(super::video::VideoAction::Create) => {
                    // a heap block for the handle, as VIDEO_create's VIDEOSTRUCT ("VIDW")
                    let block = c.call_nested(super::race::addr::RESERVE_MEM, &[VIDEO_BLOCK_NAME, 64, 0]);
                    if block != 0 {
                        c.m.w32(block, 0x5744_4956);
                        self.videos.created(block);
                    }
                    c.r[2] = block;
                    return true;
                }
                Some(super::video::VideoAction::Return(r)) => {
                    c.r[2] = r;
                    return true;
                }
                Some(super::video::VideoAction::Upload { x, y, w, h, pixels }) => {
                    self.vram_dirty = true;
                    for yy in 0..h as usize {
                        for xx in 0..w as usize {
                            let (vx, vy) = (x as usize + xx, y as usize + yy);
                            if vx < VRAM_W && vy < VRAM_H {
                                self.vram[vy * VRAM_W + vx] = pixels[yy * w as usize + xx];
                            }
                        }
                    }
                    c.r[2] = 1;
                    return true;
                }
                None => {}
            }
        }
        if let Some(ret) = self.memcard.call(c, addr) {
            c.r[2] = ret;
            return true;
        }
        let ret: u32 = match addr {
            // ---- EA file layer (FILE_*sync)
            0x800E_B204 => {
                let name = c.m.cstr(a0);
                match self.lookup(&name) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        crate::ps1::host::file_log(format!("open {name} ({} bytes)", data.len()));
                        note_open(&name);
                        if std::env::var_os("PS1_FILES").is_some() {
                            eprintln!("open {name}");
                        }
                        self.files.insert(h, data);
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        crate::ps1::host::file_log(format!("open {name}: not found"));
                        c.m.w32(a3, 0);
                        0
                    }
                }
            }
            // the asynchronous operations (asyncload*, streams): done as soon as asked for
            0x800E_CCC8 => {
                // FILE_open(name, mode, prio, user) -> op; its result is the handle (0: failed)
                let name = c.m.cstr(a0);
                let h = match self.lookup(&name) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        crate::ps1::host::file_log(format!("open {name} ({} bytes, async)", data.len()));
                        note_open(&name);
                        self.files.insert(h, data);
                        h
                    }
                    None => {
                        crate::ps1::host::file_log(format!("open {name}: not found"));
                        0
                    }
                };
                self.new_op(h, a3)
            }
            0x800E_CE48 => {
                // FILE_read(handle, offset, address, size, prio, user)
                let data = self.files.get(&a0).cloned().unwrap_or_default();
                let off = (a1 as usize).min(data.len());
                let n = (a3 as usize).min(data.len() - off);
                c.m.write_bytes(a2, &data[off..off + n]);
                let user = c.m.r32(c.r[29] + 20);
                self.new_op(n as u32, user)
            }
            0x800E_CF2C => {
                // FILE_size(handle, prio, user)
                let n = self.files.get(&a0).map_or(0, |d| d.len() as u32);
                self.new_op(n, a2)
            }
            0x800E_CD88 => {
                // FILE_close(handle, prio, user)
                self.files.remove(&a0);
                self.new_op(0, a2)
            }
            0x800E_C720 | 0x800E_CB18 => u32::from(self.file_ops.contains_key(&a0)), // FILE_opstatus / waitop: done
            0x800E_C778 => 0,                                                         // FILE_operror
            0x800E_C828 => 0,                                                         // FILE_priorityop
            0x800E_CC0C => self.file_ops.remove(&a0).map_or(0, |(r, _)| r),         // FILE_completeop
            0x800E_C964 => {
                self.file_ops.remove(&a0);
                0
            }
            0x800E_C7A8 => {
                // FILE_callbackop(op, callback): done, so the callback runs now (op, status, user)
                if let Some(&(_, user)) = self.file_ops.get(&a0) {
                    c.deferred.push((a1, vec![a0, 1, user]));
                }
                0
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
                        crate::ps1::host::file_log(format!("mount {name} ({} entries)", v.entries.len()));
                        self.mounts.push((h, v.entries));
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        crate::ps1::host::file_log(format!("mount {name}: failed"));
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
                self.vram_dirty = true;
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
                self.vram_dirty = true;
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
                self.vram_dirty = true;
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
            0x800E_E610 => {
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("DrawOTag {a0:#x} ra {:#x} gp4704 {}", c.r[31], c.m.r32(GP + 4704));
                }
                if self.mode == Mode::FrontEnd && std::env::var_os("PS1_MISSLOG").is_some() {
                    self.report_unrecorded(c, a0);
                }
                // DrawOTag: rendering is done by the host engine. In races the views after
                // the world's (HUD, tachometer, map) are recorded as 2D
                if self.mode == Mode::Race && self.hud_prims && !Self::paused(c) {
                    // (the world's list, thousands of primitives, is only counted)
                    let saved = std::mem::take(&mut self.overlay);
                    if self.otags > 0 || Self::ot_len(c, a0) < 1500 {
                        self.record_ot(c, a0);
                    }
                    let list = std::mem::replace(&mut self.overlay, saved);
                    self.frame_lists.push(list);
                    self.otags += 1;
                }
                0
            }
            0x800E_E680 => {
                // PutDrawEnv(env): a full-screen one is the frame's drawing buffer (where the
                // recorded HUD views' areas and offsets are)
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("PutDrawEnv {:?} ofs {:?} ra {:#x}", (0..4).map(|i| c.m.r16(a0 + 2 * i)).collect::<Vec<_>>(), (c.m.r16(a0 + 8), c.m.r16(a0 + 10)), c.r[31]);
                }
                // (a split screen's views are full width, half height)
                if c.m.r16(a0 + 4) >= 256 && c.m.r16(a0 + 6) >= 200 {
                    self.draw_origin = [c.m.r16(a0) as i32, c.m.r16(a0 + 2) as i32];
                }
                a0
            }
            0x800E_ED6C => {
                // SetDrawEnv(packet, env): remember the environment the packet sets (the HUD
                // views' clip and offset, for recording them)
                let clip = std::array::from_fn(|i| c.m.r16(a1 + 2 * i as u32) as i16);
                let ofs = [c.m.r16(a1 + 8) as i16, c.m.r16(a1 + 10) as i16];
                self.draw_envs.insert(a0, (clip, ofs));
                if std::env::var_os("NFSHS_ENVDBG").is_some() {
                    eprintln!("SetDrawEnv {a0:#x} {clip:?} ofs {ofs:?} ra {:#x}", c.r[31]);
                }
                // a full-screen one is the drawing buffer the views are in
                // (or the first view's: a split screen has no full-screen one)
                let view0 = |c: &mut Cpu, buf: u32| [0, 1].map(|k| c.m.r16(DRAW_VIEWS + 8 + 92 * buf + 2 * k) as i16);
                let first_view = [view0(c, 0), view0(c, 1)].contains(&[clip[0], clip[1]]) && clip[2] >= 256;
                if (clip[2] >= 256 && clip[3] >= 200) || first_view {
                    self.draw_origin = [clip[0] as i32, clip[1] as i32];
                }
                return false;
            }
            0x800E_EC70 => {
                // SetDrawArea(DR_AREA, rect): the front end clips some drawing (scrolling text)
                if self.mode == Mode::FrontEnd {
                    // in VRAM terms, inside one of the two 256-line drawing buffers
                    let mut r = |o: u32| c.m.r16(a1 + o) as i16;
                    let (x, y) = (r(0), r(2));
                    let oy = if y >= 256 { 256 } else { 0 };
                    self.overlay.push(OverlayCmd::Clip { x, y: y - oy, w: r(4), h: r(6) });
                }
                return false;
            }
            0x800E_E740 => {
                // PutDispEnv: remember the display area (x, y, w, h)
                self.display = [0, 2, 4, 6].map(|o| c.m.r16(a0 + o));
                // the front end's frames end in the engine's flip (Draw_ flip: VSync, PutDispEnv,
                // DrawOTag per view)
                if self.mode == Mode::FrontEnd && self.frame.on_frame.is_some() && c.r[31] == FLIP_PUT_DISP_ENV_RA {
                    let _ = self.frame_top(c);
                }
                a0
            }
            0x8004_E74C => {
                // the front end's shape emitter (DrawShapeExtended and friends): shape info,
                // flags (1 semi-transparent, 2 flip vertically, 4 flip horizontally), position,
                // four corner colours, blend mode
                let sp = c.r[29];
                let (colours_at, abr) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
                let colours: [u32; 4] = std::array::from_fn(|k| c.m.r32(colours_at + 4 * k as u32) & 0xFF_FFFF);
                self.shape_quads(c, a0, a1, a2 as i32, a3 as i32, colours, abr);
                0
            }
            0x8004_F12C | 0x8004_F3B8 => {
                // PSXDrawSquare(colour, x, y, w, h) / PSXDrawTransSquare(.., count): flat quads,
                // the transparent one drawn `count` times over itself
                let sp = c.r[29];
                let h = c.m.r32(sp + 16) as i32;
                let (x, y, w) = (a1 as i32, a2 as i32, a3 as i32);
                let semi = addr == 0x8004_F3B8;
                let n = if semi { c.m.r16(sp + 20) as i16 as i32 } else { 1 };
                for _ in 0..n {
                    self.overlay.push(OverlayCmd::Quad { pts: rect_pts(x, y, w, h), colours: [a0 & 0xFF_FFFF; 4], tex: None, semi, abr: 0 });
                }
                0
            }
            0x8004_D1C4 => {
                // Movie_Play(movie): the host plays it (see `play_movie`); returns whether it
                // was skipped
                self.play_movie(c, a0 & 0xFF);
                c.m.r16(0x8005_2EBC) as i16 as i32 as u32
            }
            0x8003_AC64 => {
                // DrawCar(carInfo, ...): the car the showroom shows (type, colour)
                self.showroom_car = Some((c.m.r8(a0) as i8, c.m.r8(a0 + 1)));
                return false;
            }
            0x800B_3428 if self.mode == Mode::FrontEnd && !self.capturing => {
                // R3DCar_InsertCarFacetMenuII(car, ..): the showroom car, drawn by the host
                let mut car = Box::new(super::race::CarState::read(&mut c.m, a0));
                // its light level (halved for the reflection), left in the car cache as the
                // original does (the floor's light uses it)
                let level = if c.m.r32(GP + 3532) & 0x80 != 0 { a1 >> 1 } else { a1 } & 0xFF;
                c.m.w32(0x1F80_0000 + 140, level * 0x01_0101);
                // the model is the selected one (DrawCar's); the object keeps an older code
                if let Some(code) = self.showroom_car.and_then(|(t, _)| u32::try_from(t).ok()).and_then(super::race::car_code) {
                    car.code = code.to_string();
                }
                // DrawC_PrimStart as the original calls it here: the reflection maps (as in
                // races, see record_car_facet)
                let ok = c.call_nested(DRAWC_PRIM_START, &[R3DCAR_CENTER, a0, level, CAR_CACHE]) as i32 >= 0;
                // its shadow on the floor (DrawC_ShadowPrimClip, not for the reflection): the
                // original's primitives, drawn with the floor before the car
                if ok && c.m.r32(GP + 3532) & 0x80 == 0 {
                    c.r[4] = 0x8011_7BA0;
                    c.r[5] = CAR_CACHE;
                    let saved = std::mem::take(&mut self.overlay);
                    self.capture_prims(c, DRAWC_SHADOW_PRIM_CLIP);
                    self.showroom_shadow = std::mem::replace(&mut self.overlay, saved);
                }
                let env = ok.then(|| {
                    let map = |c: &mut Cpu, o: u32| (c.m.r32(CAR_CACHE + o) != 0).then(|| std::array::from_fn(|i| c.m.r32(CAR_CACHE + o + 4 * i as u32)));
                    CarEnv { a: map(c, 100), b: map(c, 116), scroll: c.m.r32(CAR_CACHE + 132), colour: c.m.r32(CAR_CACHE + 144) & 0xFF_FFFF, yaw: c.m.r16(R3DCAR_CENTER + 0x14) as i16 }
                });
                // the lights' halos: the original itself, its body drawing skipped
                let mut halos = Vec::new();
                if ok {
                    let start = c.m.r32(0x1F80_0004);
                    let args = [a0, a1, a2, a3];
                    HALO_PASS.set(true);
                    c.inner_hook = Some(record_car_facet);
                    let saved_host = self.capturing;
                    self.capturing = true;
                    c.call_nested(0x800B_3428, &args);
                    self.capturing = saved_host;
                    c.inner_hook = None;
                    HALO_PASS.set(false);
                    let end = c.m.r32(0x1F80_0004);
                    if end > start && end - start < 0x1_0000 {
                        let saved = std::mem::take(&mut self.overlay);
                        self.record_packets(c, start, end);
                        halos = std::mem::replace(&mut self.overlay, saved);
                    }
                }
                let view = RENDER_VIEW;
                self.overlay.push(OverlayCmd::ShowroomCar {
                    car,
                    parts: (0..57).map(|i| c.m.r8(R3DCAR_OBJECT_VISIBLE + i)).collect(),
                    cam_pos: std::array::from_fn(|i| c.m.r32(view + 8 + 4 * i as u32) as i32),
                    cam_rot: std::array::from_fn(|i| c.m.r32(view + 0x14 + 4 * i as u32) as i32),
                    h: c.gte_regs[32 + 26] & 0xFFFF,
                    ofx: c.gte_regs[32 + 24] as i32,
                    ofy: c.gte_regs[32 + 25] as i32,
                    mirrored: c.m.r32(0x8013_E608) & 0x80 != 0,
                    level,
                    env,
                });
                // after the car (drawn over it)
                self.overlay.extend(halos);
                0
            }
            0x800B_3850 if self.mode == Mode::FrontEnd => {
                // R3DCar_Showroom(view): the turntable, drawn by the host. DrawC_ShowroomPrims'
                // chasing lights: two runs of five segments from the tick counter (the second
                // one opposite, or turning the other way), each segment taking the dimmest
                // light that covers it
                let t = (c.m.r32(TICKS) as i32 % 256) >> 3;
                let (head, dir): ([i32; 2], [i32; 2]) = if c.m.r32(GP + 4792) == 0 { ([t, (t + 16) & 31], [-1, -1]) } else { ([t, 32 - t], [-1, 1]) };
                let mut seg = [-1i8; 32];
                for k in 0..2 {
                    for i in 0..5 {
                        let s = ((head[k] + i * dir[k]) & 31) as usize;
                        if (seg[s] as i32) < i {
                            seg[s] = i as i8;
                        }
                    }
                }
                // both project with the view's camera (its matrix transposed, y flipped) on the
                // GTE as the game sets it up, into the ordering table's first slot
                Self::showroom_gte(c, a0);
                let mut prims = Vec::new();
                let vertex = |c: &mut Cpu, base: u32, i: u32| -> [i16; 3] { std::array::from_fn(|k| c.m.r16(base + 6 * i + 2 * k as u32) as i16) };
                if c.m.r32(0x8005_199C) != 0 {
                    // the ring: 32 textured quads, added (ChangeTPage(.., 1)), each in the base
                    // colour and again in its light's
                    let tex_at = c.m.r32(SHOWROOM_TEX);
                    let tex: [u32; 4] = std::array::from_fn(|i| c.m.r32(tex_at + 4 * i as u32));
                    let clut = (tex[0] >> 16) as u16;
                    let tpage = ((tex[1] >> 16) as u16 & !0x60) | (1 << 5);
                    let uv = tex.map(|w| [w as u8, (w >> 8) as u8]);
                    for t in 0..32u32 {
                        let (a, b) = (2 * t, (2 * t + 2) % 64);
                        let pts = [a, b, a + 1, b + 1].map(|i| {
                            let v = vertex(c, SHOWROOM_RING, i);
                            Self::gte_project(c, v)
                        });
                        let light = seg[t as usize];
                        for colour in std::iter::once(0x30_0A00).chain((light >= 0).then(|| c.m.r32(SHOWROOM_LIGHTS + 4 * light as u32) & 0xFF_FFFF)) {
                            prims.push(OverlayCmd::Quad { pts, colours: [colour; 4], tex: Some(QuadTex { tpage, clut, uv }), semi: true, abr: 1 });
                        }
                    }
                }
                // DrawC_SpotPrims: the light on the floor, a fan of added gouraud triangles from
                // the centre (the car cache's light level) to a black rim
                let spot = c.m.r32(0x1F80_0000 + 140) & 0xFF_FFFF;
                // the centre follows the rim's 32 vertices
                let centre = vertex(c, SHOWROOM_RIM, 32);
                let centre = Self::gte_project(c, centre);
                for i in 0..32u32 {
                    let (p, q) = (vertex(c, SHOWROOM_RIM, i), vertex(c, SHOWROOM_RIM, (i + 1) % 32));
                    let (p, q) = (Self::gte_project(c, p), Self::gte_project(c, q));
                    prims.push(OverlayCmd::Quad { pts: [centre, p, q, q], colours: [spot, 0, 0, 0], tex: None, semi: true, abr: 1 });
                }
                // the car's shadow last (drawn over the ring and spot; the list is reversed)
                let mut all = std::mem::take(&mut self.showroom_shadow);
                all.reverse();
                all.extend(prims);
                self.overlay.push(OverlayCmd::ShowroomFloor(all));
                0
            }
            0x800B_F1D4 => {
                // Draw_DrawDirectScreen(shape, x, y): a picture loaded straight into the
                // display buffer (the loading screen); recorded, and loaded as the game does
                if let Some((w, h, rgba)) = Self::shape_rgba(c, a0) {
                    self.loading.push(OverlayCmd::Bitmap { x: a1 as i16, y: a2 as i16, w, h, rgba: std::sync::Arc::new(rgba) });
                }
                return false;
            }
            0x8004_E01C => {
                // DoTitleScreen: the title picture while the front end loads
                self.from_main = true;
                self.new_direct_screen(c);
                return false;
            }
            0x800A_512C => {
                // NFS4_LoadingIcon: a cleared screen with the loading icon
                self.new_direct_screen(c);
                return false;
            }
            0x800F_146C => {
                // movfxya(shape, x, y): a picture blitted into the display buffer
                if let Some((w, h, rgba)) = Self::shape_rgba(c, a0) {
                    self.loading.push(OverlayCmd::Bitmap { x: a1 as i16, y: a2 as i16, w, h, rgba: std::sync::Arc::new(rgba) });
                }
                return false;
            }
            0x800D_BCA4 => {
                // Loading_DrawLoadingScreen: a new loading screen (background and track map)
                self.new_direct_screen(c);
                return false;
            }
            0x800D_BDEC => {
                // Loading_UpdateLoadingScreen(step): show the screen for the time loading took
                // so far, then let the game draw the progress bar's next part
                self.loading_frames(c);
                return false;
            }
            0x8002_7B38 => {
                // Front_Menu: the front end runs (also its post-race screens, before
                // Front_BuildStream)
                self.mode = Mode::FrontEnd;
                self.from_main = true;
                return false;
            }
            0x8002_A7B0 => {
                // Front_BuildStream: the front end runs
                self.mode = Mode::FrontEnd;
                self.from_main = true;
                return false;
            }
            0x800B_7A20 => {
                // Sim_MainGameLoop: a race runs (after the rest of the loading screen's time)
                if !self.loading.is_empty() {
                    self.loading_frames(c);
                    self.loading.clear();
                }
                self.mode = Mode::Race;
                return false;
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
                // InterruptCallback(line, handler): the previous handler
                self.callbacks.insert("interrupt", a1);
                let slot = c.irq_callbacks.get_mut(a0 as usize);
                slot.map_or(0, |h| std::mem::replace(h, a1))
            }
            0x800F_3208 => {
                // DMACallback(channel, handler): the previous handler
                self.callbacks.insert("dma", a1);
                let slot = c.dma_callbacks.get_mut(a0 as usize);
                slot.map_or(0, |h| std::mem::replace(h, a1))
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
            0x800F_08BC => u32::from(a1 == 1) * 4, // PadInfoMode: a digital pad (ID 4), no extended modes
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
            0x8009_A5AC => {
                // CopSpeak_Stop waits for the cop speech queue (read gp+2932, write gp+2928)
                // to be played out: the requests still waiting are dropped
                let write = c.m.r32(0x8013_D83C + 2928);
                c.m.w32(0x8013_D83C + 2932, write);
                return false;
            }
            0x800E_77F0 => {
                if self.frame.on_frame.is_some() && c.r[31] == MAIN_LOOP_YIELD_RA {
                    // top of the original race loop
                    let _ = self.frame_top(c);
                } else {
                    // yield inside a wait loop: let time pass until the next timer tick
                    self.advance_to(c, (self.clock / TICK + 1) * TICK);
                    if let Ok(path) = std::env::var("PS1_STALL_DUMP") {
                        if self.clock > self.frame.last_top + 1920 * 30 {
                            std::fs::write(&path, &c.m.ram).ok();
                            eprintln!("stalled: RAM written to {path}; stack {:x?}", c.stack);
                            std::process::exit(3);
                        }
                    }
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

            // the HUD's helpers: in races the original builds its primitives (recorded from
            // the HUD views' ordering tables)
            0x800C_C7F0 | 0x800D_4E9C | 0x800D_8690 | 0x800D_9ABC | 0x800D_3558 if self.mode == Mode::Race && self.hud_prims && !Self::paused(c) => return false,
            0x800C_C7F0 if self.mode == Mode::FrontEnd => {
                self.font_quads(c, a0, a1 as i32, a2 as i32);
                0
            }
            0x800C_C7F0 => {
                // Font_TextXY(text, x, y)
                let mut text = Vec::new();
                while text.len() < 256 {
                    let b = c.m.r8(a0 + text.len() as u32);
                    if b == 0 {
                        break;
                    }
                    text.push(b);
                }
                let colour = ((c.m.r16(FONT_CURRENT_CLUT) as u32 & 0x3F) as i32 - (c.m.r16(FONT_CLUT_X) as i32 >> 4)).clamp(0, 15) as u8;
                self.overlay.push(OverlayCmd::Text { x: a1 as i16, y: a2 as i16, colour, space: c.m.r8(FONT_SPACE), text });
                0
            }
            0x800D_4E9C => {
                // Hud_BuildString(text, x, y, colour, pad, measure only): the HUD's small
                // letters and digits and the pad button symbols; returns the width
                let sp = c.r[29];
                let measure = c.m.r32(sp + 20) != 0;
                let record = !measure && !native_hud(c.r[31]);
                let width = |c: &mut Cpu, s: u32| c.m.r16(super::race::HUD_SHAPES + 20 * s + 16) as i16 as i32;
                let (x0, y) = (a1 as i32, a2 as i32);
                let mut x = x0;
                for i in 0..64 {
                    let ch = c.m.r8(a0 + i) as u32;
                    if ch == 0 {
                        break;
                    }
                    // (shape, y offset); a standard pad's symbols
                    let (shape, dy) = match ch {
                        0x20 => {
                            x += 3;
                            continue;
                        }
                        0x2A => {
                            // '*': the X button, two pixels in
                            x += 2;
                            (170, 0)
                        }
                        0x5E => (170, 0),
                        0x28 => (169, 0),
                        0x29 => (168, 0),
                        0x26 => (174, 0),
                        0x30..=0x39 => (ch + 110, 0),
                        0x2D => (72, 0),
                        0x3A => (73, 0),
                        0xC0..=0xDC => (ch - 118, -1),
                        0xE5 => (103, -1),
                        _ => (ch + 67, 0),
                    };
                    let shape = shape & 0xFF;
                    if record {
                        self.overlay.push(OverlayCmd::Shape { shape: shape as u8, x: x as i16, y: (y + dy) as i16, colour: a3 & 0xFF_FFFF });
                    }
                    x += width(c, shape) + if ch == 0x2A { 3 } else { 1 };
                }
                (x - x0) as u32
            }
            0x800D_8690 => {
                if !native_hud(c.r[31]) {
                    self.overlay.push(OverlayCmd::ThinBox { x: a0 as i16, y: a1 as i16, w: a2 as i16, h: a3 as i16 });
                }
                0
            }
            0x800D_9ABC => {
                self.overlay.push(OverlayCmd::PauseBox { x: a0 as i16, y: a1 as i16, w: a2 as i16, h: a3 as i16 });
                0
            }
            0x800D_3558 if native_hud(c.r[31]) => 0,
            0x800D_3558 => {
                let sp = c.r[29];
                if std::env::var_os("PS1_OVLOG").is_some() {
                    eprintln!("F4 ({a1},{a2}) {a3}x{} from {:#x}", c.m.r32(sp + 16) as i32, c.r[31]);
                }
                self.overlay.push(OverlayCmd::Rect {
                    semi: a0 != 0,
                    x: a1 as i16,
                    y: a2 as i16,
                    w: a3 as i16,
                    h: c.m.r32(sp + 16) as i16,
                    colour: c.m.r32(sp + 20) & 0xFF_FFFF,
                    dx0: c.m.r32(sp + 24) as i8,
                    dx1: c.m.r32(sp + 28) as i8,
                });
                0
            }
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
                if std::env::var_os("NFSHS_ENVLOG").is_some() && self.frame.frames_run % 120 == 0 {
                    eprintln!("view {a0} env (far rgb, dqa, dqb, h, back rgb) {:?}, ofx {} ofy {}", self.view_env, c.gte_read(32 + 24) as i32 as f32 / 65536.0, c.gte_read(32 + 25) as i32 as f32 / 65536.0);
                }
                // which track chunks to draw, and how (detail level)
                c.call_nested(SETUP_BUILD_MATRICES, &[RENDER_VIEW, 0x1F80_0000]);
                c.call_nested(DRAWW_WORLD_SETUP_MATRIX, &[G_WORLD_MAT, 0x1F80_0014]);
                let n = c.call_nested(SETUP_CHUNK_BUILD_LIST, &[RENDER_VIEW]);
                c.m.w32(CHUNK_BUILD_COUNT, n);
                // the track lights' glares as the original builds them for the listed chunks
                // (BWorld_BuildGlareEffects, as BWorld_OnyxBuildFacets calls it), recorded
                if self.hud_prims {
                    let start = c.m.r32(0x1F80_0004);
                    GLARE_MARKS.with_borrow_mut(Vec::clear);
                    c.inner_hook = Some(glare_marker);
                    for i in 0..n.min(64) {
                        let chunk = c.m.r16(0x8011_02B0 + 4 * i) as i16;
                        if chunk < 0 {
                            continue;
                        }
                        let list = c.m.r32(TRACK_CHUNK_LIST);
                        let group = c.m.r32(list + 112 * chunk as u32 + 84);
                        if group != 0 {
                            c.call_nested(BWORLD_BUILD_GLARE_EFFECTS, &[RENDER_VIEW, 0x1F80_0000, group]);
                        }
                    }
                    c.inner_hook = None;
                    let end = c.m.r32(0x1F80_0004);
                    self.track_glows.clear();
                    if end > start && end - start < 0x4_0000 {
                        let marks = GLARE_MARKS.with_borrow(Clone::clone);
                        let saved = std::mem::take(&mut self.overlay);
                        self.prim_offset = [0, 0];
                        for (k, &(from, anchor)) in marks.iter().enumerate() {
                            let to = marks.get(k + 1).map_or(end, |m| m.0);
                            if to > from {
                                self.record_packets(c, from, to);
                                let cmds = std::mem::take(&mut self.overlay);
                                if !cmds.is_empty() {
                                    self.track_glows.push(Glow { anchor, car: None, cmds, smoke: false });
                                }
                            }
                        }
                        self.overlay = saved;
                    }
                }
                // the tyre smoke and dust (Souffle_InsertFacet), each puff recorded with its
                // position
                if self.hud_prims {
                    let start = c.m.r32(0x1F80_0004);
                    GLARE_MARKS.with_borrow_mut(Vec::clear);
                    c.inner_hook = Some(smoke_marker);
                    c.call_nested(SOUFFLE_INSERT_FACET, &[RENDER_VIEW]);
                    c.inner_hook = None;
                    let end = c.m.r32(0x1F80_0004);
                    if std::env::var_os("NFSHS_SMOKELOG").is_some() {
                        let n = c.m.r32(GP + 3828);
                        let arr = c.m.r32(GP + 3824);
                        let live = (0..n.min(64)).filter(|&i| c.m.r8(arr + 68 * i) != 0).count();
                        eprintln!("smoke: {n} souffles ({live} live), {} built, {} bytes", GLARE_MARKS.with_borrow(Vec::len), end.wrapping_sub(start));
                    }
                    if end > start && end - start < 0x4_0000 {
                        let marks = GLARE_MARKS.with_borrow(Clone::clone);
                        let saved = std::mem::take(&mut self.overlay);
                        self.prim_offset = [0, 0];
                        for (k, &(from, anchor)) in marks.iter().enumerate() {
                            let to = marks.get(k + 1).map_or(end, |m| m.0);
                            if to > from {
                                self.record_packets(c, from, to);
                                let cmds = std::mem::take(&mut self.overlay);
                                if std::env::var_os("NFSHS_SMOKELOG").is_some() {
                                    eprintln!("puff at {:?}: {}", anchor.map(|v| v >> 16), format!("{cmds:?}").chars().take(300).collect::<String>());
                                }
                                if !cmds.is_empty() {
                                    self.track_glows.push(Glow { anchor, car: None, cmds, smoke: true });
                                }
                            }
                        }
                        self.overlay = saved;
                    }
                }
                // the weather (rain or snow, and the drops on the lens) as Render_RenderWorld
                // builds it after the world, recorded
                if self.hud_prims {
                    let start = c.m.r32(0x1F80_0004);
                    c.call_nested(WEATHER_RENDER, &[RENDER_VIEW]);
                    let end = c.m.r32(0x1F80_0004);
                    if std::env::var_os("NFSHS_WEATHERLOG").is_some() {
                        eprintln!("weather: prims {start:#x}..{end:#x}");
                    }
                    if end > start && end - start < 0x4_0000 {
                        let saved = std::mem::take(&mut self.overlay);
                        self.prim_offset = [0, 0];
                        self.record_packets(c, start, end);
                        self.weather = std::mem::replace(&mut self.overlay, saved);
                    }
                }
                // the sun's flare (Hrz_BuildSky: Flare_Sun when the sky record's flags have
                // 4): the horizon's rotation (the view's matrix, no translation) on the GTE,
                // its primitives recorded as the sky layer
                let mut view_sky = Vec::new();
                let sky = c.m.r32(GP + 4920);
                if self.hud_prims && sky != 0 && c.m.r32(0x8012_45C8) & 4 != 0 {
                    let (rot, tr) = (std::array::from_fn::<u32, 8, _>(|i| c.gte_read(32 + i)), [c.gte_read(37), c.gte_read(38), c.gte_read(39)]);
                    c.call_nested(HRZ_SET_PSX_MATRIX, &[RENDER_VIEW + 68]);
                    for (i, v) in [37, 38, 39].into_iter().enumerate() {
                        let _ = i;
                        c.gte_write(v, 0);
                    }
                    c.r[4] = 0x8013_F0F8;
                    c.r[5] = 0x1F80_0000;
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    self.capture_prims(c, FLARE_SUN);
                    let sun = std::mem::replace(&mut self.overlay, saved);
                    if std::env::var_os("NFSHS_SUNLOG").is_some() {
                        for q in &sun {
                            eprintln!("sun {}", format!("{q:?}").chars().take(220).collect::<String>());
                        }
                    }
                    view_sky = sun.clone();
                    self.sky_overlay.extend(sun);
                    for (i, v) in rot.into_iter().enumerate() {
                        c.gte_write(32 + i, v);
                    }
                    for (i, v) in tr.into_iter().enumerate() {
                        c.gte_write(37 + i, v);
                    }
                }
                c.call_nested(DASHHUD_CHECK_WRONG_WAY, &[a1]);
                c.call_nested(DASHHUD_HUD_CALC, &[a1]);
                // the cars: the original decides what is drawn of each (the host draws them).
                // Its calls cannot reach the host from here: R3DCar_InsertCarFacetII is
                // replaced by an inner hook that records the decision instead of drawing.
                c.capture.clear();
                HALO_RANGES.with_borrow_mut(Vec::clear);
                c.inner_hook = Some(record_car_facet);
                c.call_nested(R3DCAR_INSERT_ALL_LIST_FACET, &[RENDER_VIEW]);
                c.inner_hook = None;
                self.car_facets = CarFacet::parse(&c.capture);
                // the cars' light halos, as the original built them (screen space)
                if self.hud_prims {
                    let ranges = HALO_RANGES.with_borrow(Clone::clone);
                    let saved = std::mem::take(&mut self.overlay);
                    self.prim_offset = [0, 0];
                    self.car_glows.clear();
                    for (start, end, car) in ranges {
                        self.record_packets(c, start, end);
                        let cmds = std::mem::take(&mut self.overlay);
                        let anchor = [c.m.r32(car + 0xA0) as i32, c.m.r32(car + 0xA4) as i32, c.m.r32(car + 0xA8) as i32];
                        self.car_glows.push(Glow { anchor, car: Some(car), cmds, smoke: false });
                    }
                    self.overlay = saved;
                }
                // this view as set up (Draw_gViews[view]: its drawing environment's clip,
                // relative to the first view's, and size)
                let env = |c: &mut Cpu, v: u32, k: u32| c.m.r16(DRAW_VIEWS + 200 * v + 8 + 2 * k) as i16;
                let rect = [env(c, a0, 0) - env(c, 0, 0), env(c, a0, 1) - env(c, 0, 1), c.m.r16(DRAW_VIEWS + 200 * a0 + 12) as i16, c.m.r16(DRAW_VIEWS + 200 * a0 + 14) as i16];
                self.views.push(ViewCapture {
                    player: a1,
                    rect,
                    camera_pos: std::array::from_fn(|i| c.m.r32(RENDER_VIEW + 8 + 4 * i as u32) as i32),
                    camera_rot: std::array::from_fn(|i| c.m.r32(RENDER_VIEW + 0x14 + 4 * i as u32) as i32),
                    projection: c.m.r32(super::race::addr::CAMERA_ZOOM),
                    view_env: self.view_env,
                    chunks: (0..n.min(64)).map(|i| (c.m.r16(0x8011_02B0 + 4 * i) as i16, c.m.r16(0x8011_02B2 + 4 * i))).collect(),
                    car_facets: self.car_facets.clone(),
                    glows: self.track_glows.iter().chain(&self.car_glows).cloned().collect(),
                    weather: self.weather.clone(),
                    sky_overlay: view_sky,
                    night_beam: super::race::NightBeam::read(c, a1),
                });
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

/// An axis-aligned rectangle's corners in quad order (top-left, top-right, bottom-left,
/// bottom-right).
fn rect_pts(x: i32, y: i32, w: i32, h: i32) -> [[i16; 2]; 4] {
    [[x as i16, y as i16], [(x + w) as i16, y as i16], [x as i16, (y + h) as i16], [(x + w) as i16, (y + h) as i16]]
}
