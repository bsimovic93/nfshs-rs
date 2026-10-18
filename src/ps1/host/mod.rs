//! Host replacements (HLE) for the NFS4.EXE functions that drive PS1 hardware.
//! The address list is shared with the recompiler: tools/hle_funcs.txt.
//! Addresses are for the NTSC-U executable (SLUS-00826).
//!
//! [`GameHost`] keeps the machine's state (clock, VRAM, files, pads) and dispatches each
//! replaced call by library: `files` (the EA file layer), `gpu` (libgpu), `system` (BIOS,
//! timers, pads, CD), `game` (the game's front-end drawing and mode changes) and `views`
//! (a race's player views and HUD helpers). What the original draws is recorded by
//! `overlay` (2D) and `cars` (the cars, glows, smoke); `frame` hands each finished frame to
//! the engine.

mod cars;
mod files;
mod frame;
mod game;
mod gpu;
mod objects;
mod overlay;
mod system;
mod views;

pub use cars::{CarEnv, CarFacet, Glow};
pub use files::{file_log, opened_variant};
pub use frame::{FrameDriver, FrameInfo, Mode, Pad, ViewCapture};
pub use objects::ObjectDraw;
pub use overlay::{OverlayCmd, QuadTex};

use cars::{glare_marker, record_car_facet, smoke_marker};
use overlay::rect_pts;
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
    /// The track objects the game drew in the player view this frame (DrawW_DoObjects).
    pub objects: Vec<ObjectDraw>,
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
    /// The players' cameras (`Camera_gInfo`) as the pause found them (see `hold_cameras`).
    paused_cameras: Option<Vec<u8>>,
    /// The drawing offset of the view being recorded (DR_OFFSET, display-relative).
    prim_offset: [i16; 2],
    area_tl: [i32; 2],
    draw_origin: [i32; 2],
    /// SetDrawEnv's packets: their drawing environment's clip (x, y, w, h) and offset, in VRAM.
    draw_envs: HashMap<u32, ([i16; 4], [i16; 2])>,
    /// The game started from main (its title and front end), not straight into a race.
    pub from_main: bool,
    /// The game setup menu's Quit (see `add_quit_item`) was chosen: the app closes.
    pub quit: bool,
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
    /// PS1_MISSLOG: the primitive buffer's position at each host call this frame and the
    /// functions running then (to name what built an unrecorded packet).
    prim_marks: Vec<(u32, Vec<u32>)>,
    /// The GPU draw mode (texture page) as the captured primitives left it.
    draw_mode: u16,
    vram_shared: Option<std::sync::Arc<Vec<u16>>>,
    /// Controller receive buffers registered with PadInitDirect (port 1, port 2).
    pub pad_buffers: [u32; 2],
    /// Each port's actuator data as the game gave it to PadSetAct (its DualShock motors: the
    /// small one on / off, the big one 0..255), 0 before it has.
    pub actuators: [u32; 2],
    /// Callbacks the game registered (vsync, draw sync, ...), keyed by kind.
    pub callbacks: HashMap<&'static str, u32>,
    next_event: u32,
    /// Frame driver state while the original main loop runs (see race.rs).
    pub frame: FrameDriver,
}

/// Called at the top of every main-loop iteration (one game frame, locked to a 60 Hz field)
/// with the current field number; returns the controllers to hold during the frame (port 1,
/// port 2) or `None` to leave the loop.
pub type FrameFn = Box<dyn FnMut(&mut Cpu, FrameInfo) -> Option<[Pad; 2]>>;

/// HUD functions whose drawing the host renders from its own port (Hud_RenderMapView,
/// Hud_BlackThinBox's countdown use in Hud_Draw321Num / Hud_Render321Go, Hud_RenderHudView,
/// Hud_RenderTacView): 2D calls made from them are not recorded again.
const NATIVE_HUD: [std::ops::Range<u32>; 4] = [0x800D_840C..0x800D_8690, 0x800D_887C..0x800D_8B84, 0x800D_8EA4..0x800D_981C, 0x800D_981C..0x800D_9938];

fn native_hud(ra: u32) -> bool {
    NATIVE_HUD.iter().any(|r| r.contains(&ra))
}

thread_local! {
    /// While the original's R3DCar_InsertCarFacetII runs for its halos (see
    /// `record_car_facet`); and the primitive buffer ranges they were built in.
    static HALO_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HALO_RANGES: std::cell::RefCell<Vec<(u32, u32, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// While the original's R3DCar_Visibilty runs inside its widened-view call.
    static VISIBILITY_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// R3DCar_Visibilty(car, view): sets the car's level of detail (`Car_tObj` + 0x8C2), -1 when
/// it is not drawn: beyond about 200 m, or outside a cone of |x| <= z in view space (90
/// degrees, the PS1's 4:3 picture with room to spare).
const R3DCAR_VISIBILITY: u32 = 0x800B_03E4;
const CAR_DETAIL: u32 = 0x8C2;
/// How much wider than the original's the car cone is made: the host draws the picture
/// wider than 4:3 (the PS1's vertical field of view, the window's aspect), up to |x| <= 2z
/// (21:9 with every camera).
const CAR_CONE_WIDTH: i32 = 2;
/// How much further than the original's (about 200) cars are drawn: past every weather's fog.
const CAR_RANGE: i32 = 3;

/// The car body's drawing in R3DCar_InsertCarFacetII / InsertCarFacetMenuII (the host
/// draws the body): the part drawing, the shadow, the environment, and the menu's
/// DrawC_PrimMenu / DrawC_PrimStop.
const CAR_BODY_DRAWING: [u32; 6] = [0x800C_22E0, 0x800C_0AC8, 0x800C_5054, 0x800C_4C68, 0x800C_4034, 0x800C_0A08];

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

const FLARE_HALO2: u32 = 0x800C_E290;

/// The front end's `tCarManager` (cars: count, then the table of `tCarInfo`, 204 bytes each:
/// +0 the model, +2 its class) and `tTrackManager`.
const CAR_MANAGER: u32 = 0x8011_552C;
const TRACK_MANAGER: u32 = 0x8011_5D08;

/// Every track and car available in the front end, as winning everything would leave them:
/// `tTrackManager::SetTrackAvailable` for the eleven track slots and
/// `tCarManager::SetCarAvailable` for the cars of classes 0..7 (the race cars, the bonus
/// cars and the police cars; class 8 is traffic): available (+520) and unlocked (+568, which
/// the car lists go by). Done every front-end frame, since loading a save sets them again.
/// The settings' `locked` or NFSHS_LOCKED=1: as the save has it.
fn unlock_everything(c: &mut Cpu) {
    static LOCKED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *LOCKED.get_or_init(|| crate::settings::get().locked || std::env::var_os("NFSHS_LOCKED").is_some()) {
        return;
    }
    for t in 0..11 {
        c.m.w32(TRACK_MANAGER + 8 + 4 * t, 1);
    }
    let (count, table) = (c.m.r32(CAR_MANAGER), c.m.r32(CAR_MANAGER + 4));
    // (before the front end has set its cars up)
    if !(1..=64).contains(&count) || !(0x8000_0000..0x8020_0000).contains(&table) {
        return;
    }
    for k in 0..count {
        let (model, class) = (c.m.r8(table + 204 * k) as i8, c.m.r8(table + 204 * k + 2));
        if model >= 0 && class <= 7 {
            c.m.w8(CAR_MANAGER + 520 + model as u32, 1);
            c.m.w8(CAR_MANAGER + 568 + model as u32, 1);
        }
    }
}

/// The game setup menu (One Player, Two Player, Game Options, Trophy Room): the front end's
/// pointer to its first item, a `tMenuItemGoToMenuNFS4Button` (44 bytes: +4 its text, +0x14
/// +0xc the icon shown by the title while it is selected, +0x14 the menu it goes to, +0x1c a
/// function called with the menu command when it is chosen).
/// The menu (`tMenuNFS4`, its vtable at +0x68) is allocated right after its four items and
/// lists them from +0x10, null-terminated (room for 16).
const GAME_SETUP_FIRST_ITEM: u32 = 0x8005_1EE0;
const MENU_NFS4_VTABLE: u32 = 0x8001_0798;
const MENU_ITEM_SIZE: u32 = 44;
/// ZTEXT's "ONE PLAYER" and "QUIT".
const ONE_PLAYER_TEXT: u32 = 92;
const QUIT_TEXT: u32 = 156;
/// What the Quit item calls when chosen: no code, the host takes the call.
const QUIT_CALLBACK: u32 = 0x8000_F000;
/// The Quit item itself, in the kernel's RAM below the game (unused: the BIOS is the host's).
/// Not on the heap: the front end never frees it (it deletes its items by its own pointers),
/// and a block left there splits the memory the race loads into (Snowy Ridge's track no
/// longer fits).
const QUIT_ITEM: u32 = 0x8000_F010;

/// A Quit at the bottom of the game setup menu (the console game has none): a copy of its
/// Game Options item (its icon the plain one) that shows "QUIT", goes to no menu and calls
/// [`QUIT_CALLBACK`].
/// Done every front-end frame, since the front end builds its menus again after a race.
fn add_quit_item(c: &mut Cpu) {
    let first = c.m.r32(GAME_SETUP_FIRST_ITEM);
    if !(0x8000_0000..0x8020_0000).contains(&first) {
        return;
    }
    let menu = first + 4 * MENU_ITEM_SIZE;
    if c.m.r32(menu + 0x68) != MENU_NFS4_VTABLE || c.m.r32(menu + 0x10) != first || c.m.r32(first + 4) != ONE_PLAYER_TEXT {
        return;
    }
    // (five: the Quit is there already)
    let items: Vec<u32> = (0..5).map(|k| c.m.r32(menu + 0x10 + 4 * k)).take_while(|&i| i != 0).collect();
    if items.len() != 4 {
        return;
    }
    let item = QUIT_ITEM;
    for i in (0..MENU_ITEM_SIZE).step_by(4) {
        let w = c.m.r32(items[2] + i);
        c.m.w32(item + i, w);
    }
    c.m.w32(item + 4, QUIT_TEXT);
    c.m.w32(item + 0x14, 0);
    c.m.w32(item + 0x1C, QUIT_CALLBACK);
    c.m.w32(menu + 0x20, item);
    c.m.w32(menu + 0x24, 0);
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
/// DrawC_PrimStart's per-texture codes for the car being drawn (52 halfwords): its lights
/// (brake: bROf/bSOf |= 0x81, headlights ...) and damage zones; the low byte for a polygon,
/// the high byte for its mirrored twin.
const LIGHT_CODES: u32 = 0x8012_07F8;
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
const CAPTURED_FE_DRAWING: [u32; 10] = [
    0x8002_24AC, // DrawTV
    0x8004_EC64, // a scaled shape's POLY_GT4 (ScaleShapeExtended: the trophy room's trophies)
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
        // the VSync callback (VSyncCallback: the game's Force_Vbl, the DualShock motors) at each
        // 60 Hz field, run like the interrupt it is once this host call returns
        if let Some(&cb) = self.callbacks.get("vsync").filter(|&&cb| cb != 0) {
            for _ in 0..(t / FIELD - self.clock / FIELD).min(4) {
                c.deferred.push((cb, Vec::new()));
            }
        }
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
        let memcard = super::memcard::MemCard::new(&root);
        Self {
            root,
            files: HashMap::new(),
            mounts: Vec::new(),
            next_handle: 0x100,
            memcard,
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
            objects: Vec::new(),
            overlay: Vec::new(),
            loading: Vec::new(),
            loading_shown: 0,
            overlay_ot: Vec::new(),
            current_ot: 0,
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
            paused_cameras: None,
            views: Vec::new(),
            prim_offset: [0, 0],
            area_tl: [0, 0],
            draw_origin: [0, 0],
            draw_envs: HashMap::new(),
            from_main: false,
            quit: false,
            rcnt: 0,
            log: Vec::new(),
            vram: vec![0; VRAM_W * VRAM_H],
            vram_dirty: true,
            capturing: false,
            recorded_packets: Default::default(),
            prim_marks: Vec::new(),
            draw_mode: 0,
            vram_shared: None,
            pad_buffers: [0; 2],
            actuators: [0; 2],
            callbacks: HashMap::new(),
            next_event: 0xF000_0001,
            frame: FrameDriver::default(),
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
                let buttons = pads[0].buttons | pads[1].buttons;
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
        if self.idles.is_multiple_of(500) && std::env::var_os("PS1_IDLELOG").is_some() {
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
        if self.mode == Mode::FrontEnd && std::env::var_os("PS1_MISSLOG").is_some() && self.prim_marks.len() < 100_000 {
            let ptr = c.m.r32(0x1F80_0004);
            if self.prim_marks.last().is_none_or(|m| m.0 != ptr) {
                self.prim_marks.push((ptr, c.stack.iter().rev().take(4).copied().collect()));
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
        let ret = self
            .file_call(c, addr)
            .or_else(|| self.gpu_call(c, addr))
            .or_else(|| self.game_call(c, addr))
            .or_else(|| self.system_call(c, addr))
            .or_else(|| self.race_call(c, addr));
        let Some(ret) = ret else { return false };
        c.r[2] = ret;
        true
    }
}

