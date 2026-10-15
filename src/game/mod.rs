//! A race as the original runs it: its cars, camera and 2D, drawn by this engine.

pub mod fe;
pub mod ai;
pub mod car;
pub mod original;
pub mod ps1hud;

use crate::assets::{self, Atlas, CarModel, GameData, TrackAssets, Vertex};
use crate::render::FrameParams;
use anyhow::Result;
use car::Car;
use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub handbrake: bool,
    pub camera: bool,
    pub autopilot: bool,
    /// Start (pause) and the menus' accept button.
    pub start: bool,
    pub accept: bool,
    /// The front end's other buttons: back (Triangle), Circle and Square.
    pub back: bool,
    pub circle: bool,
    pub square: bool,
    pub select: bool,
    /// Analog controls (gamepad); override the digital ones when non-zero.
    pub steer_axis: f32,
    pub throttle_axis: f32,
    pub brake_axis: f32,
    /// The second controller's PS1 buttons (player 2), as they are.
    pub pad2: u16,
    /// The controllers' sticks (RX, RY, LX, LY) when they are gamepads: the original then
    /// sees DualShocks in analog mode. `None`: a digital pad (keyboard).
    pub sticks: Option<[u8; 4]>,
    pub pad2_sticks: Option<[u8; 4]>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum CameraMode {
    Chase,
    /// The bumper view: the followed car itself is not drawn.
    Bumper,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    /// The original game's intro fly-by before the start lights.
    Intro,
    Countdown(f32),
    Racing,
    Finished,
}

pub struct Racer {
    pub name: String,
    pub car: Car,
    pub model: usize,
    /// Nearest virtual-road sample.
    pub progress: usize,
    /// Completed laps (-1 while still behind the start line on the grid).
    pub laps_done: i32,
    pub lap_time: f32,
    pub best_lap: Option<f32>,
    pub finish_time: Option<f32>,
}

impl Racer {
    fn distance(&self, n: usize) -> i64 {
        self.laps_done as i64 * n as i64 + self.progress as i64
    }
}

pub struct RaceConfig<'a> {
    pub track: u32,
    pub variant: &'a str,
    pub player_car: &'a str,
    pub opponents: Vec<String>,
    pub laps: i32,
    /// Traffic models to load when the simulation does not list them (reference renders).
    pub traffic: Vec<String>,
}

/// A light glow from one frame to the next: its world anchor, its car and the view's place in
/// the window (split screen).
type GlowKey = ([i32; 3], u32, [i32; 2]);

/// The world anchor standing for the sun in [`Game::glow_offsets`].
const SUN_KEY: [i32; 3] = [i32::MIN; 3];

/// The centre of the quads among `cmds` (PS1 screen pixels).
fn quads_centre<'a>(cmds: impl IntoIterator<Item = &'a crate::ps1::host::OverlayCmd>) -> Option<Vec2> {
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for c in cmds {
        if let crate::ps1::host::OverlayCmd::Quad { pts, .. } = c {
            for p in pts {
                let v = Vec2::new(p[0] as f32, p[1] as f32);
                (lo, hi) = (lo.min(v), hi.max(v));
            }
        }
    }
    (lo.x <= hi.x).then(|| (lo + hi) * 0.5)
}

pub struct Game {
    pub track: TrackAssets,
    pub models: Vec<CarModel>,
    pub racers: Vec<Racer>,
    /// The original's traffic (cars after the racers in its car list): snapshot index and
    /// model (index into `models`).
    pub traffic: Vec<(usize, usize)>,
    /// Traffic poses and what is drawn of each (reference renders), instead of the sim's.
    pub traffic_override: Option<Vec<(original::Pose, crate::ps1::host::CarFacet)>>,
    /// Virtual-road centre line (world space): laps and positions, the autopilot's line.
    pub path: Vec<Vec3>,
    pub laps: i32,
    pub phase: Phase,
    pub race_time: f32,
    pub cam_pos: Vec3,
    pub cam_target: Vec3,
    pub cam_mode: CameraMode,
    /// The racer the current view follows (split screen: player 2's view follows racer 1).
    pub view_player: usize,
    /// The current view's height to width as shown (3/4 for the whole 4:3 screen, 3/8 for
    /// half of it).
    view_ratio: f32,
    pub cam_up: Vec3,
    /// tan of half the vertical field of view.
    pub tan_half_v: f32,
    /// Vertical squash of the picture (the original's view matrix scales Y), 1 = none.
    pub y_scale: f32,
    autopilot: ai::Driver,
    /// Fog colour and distances (the draw distance; the original's depth cue does the
    /// colouring).
    pub fog: (Vec3, f32, f32),
    /// Night driving (the track's night variant): the original draws the world without its
    /// depth cue then (DrawW_DrawQuad takes DrawW_NightColorCalc's colours instead).
    pub night_driving: bool,
    /// The recompiled original, running this race.
    pub sim: original::OriginalSim,
    /// The original HUD (when the simulation provides its layout).
    pub ps1hud: Option<ps1hud::Ps1Hud>,
    /// Per light glow (and the sun) as last drawn: how far the PS1 put it from the light's
    /// exact projection (PS1 pixels, smoothed) and the simulated frame it was seen in.
    glow_offsets: std::cell::RefCell<std::collections::HashMap<GlowKey, (Vec2, u32)>>,
    /// The track's reflection map page (atlas rectangle) and the atlas' size.
    reflections: Option<assets::Rect>,
    atlas_size: f32,
    /// The cars' ground shadow texture (ZSFX.PSH "SHAD", gShadowPixmap), corners in
    /// POLY_FT4 order.
    shadow_tex: Option<[[f32; 2]; 4]>,
    /// The tyre mark textures (ZSFX.PSH "SKD0", "SKD1"; Skidmark pixmaps), corners as
    /// `shadow_tex`.
    skid_tex: [Option<[[f32; 2]; 4]>; 2],
    /// The chunk build list, instead of the simulation's (for comparisons with captures).
    pub chunks_override: Option<Vec<(i16, u16)>>,
    /// Tyre marks (reference renders), instead of the sim's.
    pub skids_override: Option<Vec<crate::ps1::race::SkidQuad>>,
    /// What is drawn of each car, instead of the simulation's (for comparisons with
    /// captures).
    pub car_facets_override: Option<Vec<Option<crate::ps1::host::CarFacet>>>,
}

impl Game {
    /// Load the race `cfg` describes, starting the original in it.
    pub fn load(data: &GameData, atlas: &mut Atlas, cfg: &RaceConfig) -> Result<Self> {
        let sim = original::OriginalSim::start(data, cfg)?;
        Self::load_with(data, atlas, cfg, sim)
    }

    /// Load the race the original `sim` is in (started from its own front end).
    pub fn load_with(data: &GameData, atlas: &mut Atlas, cfg: &RaceConfig, sim: original::OriginalSim) -> Result<Self> {
        // the track's vertex colours come from the original's light table
        let light = sim.snapshot().light_table.clone();
        if std::env::var_os("NFSHS_LIGHTLOG").is_some() {
            println!("light table: {}", light.iter().step_by(8).map(|c| format!("{:06x}", c & 0xFF_FFFF)).collect::<Vec<_>>().join(" "));
        }
        let sky = sim.snapshot().sky.clone();
        let track = assets::load_track(data, atlas, cfg.track, cfg.variant, &light, sky.as_ref())?;
        let mut path: Vec<Vec3> = track.track.vroad.iter().map(|v| assets::ps1_to_world(v.pos)).collect();
        if path.len() < 2 {
            path = track.nodes.clone();
        }
        let n = path.len();

        let mut codes = vec![cfg.player_car.to_string()];
        codes.extend(cfg.opponents.iter().cloned());
        let mut models = Vec::new();
        let mut racers = Vec::new();
        let snap = sim.snapshot();
        // the race cars (the player first, then the opponents in the car list), as the original
        // set them up: paint, reflection normals, texture substitutes (lit lights, dents)
        let race_car = |k: usize| k == 0 || snap.cars.get(k).is_some_and(|c| crate::ps1::race::car_type(&c.code).is_some());
        let colours: Vec<u8> = snap.cars.iter().enumerate().filter(|(k, _)| race_car(*k)).map(|(_, c)| c.colour).collect();
        log::info!("car colours {colours:?}");
        let env_normals: Vec<_> = snap.env_normals.iter().enumerate().filter(|(k, _)| race_car(*k)).map(|(_, n)| n.clone()).collect();
        let texture_subs: Vec<_> = snap.texture_subs.iter().enumerate().filter(|(k, _)| race_car(*k)).map(|(_, t)| t.clone()).collect();
        let part_modes = snap.part_modes.clone();
        for (k, code) in codes.iter().enumerate() {
            let night = matches!(cfg.variant, "N" | "S");
            // the number plate shows the player's name (GameSetup carInfo + 0x68)
            let plate = crate::ps1::race::player_plate();
            let mut model = assets::load_car(data, atlas, code, k == 0, night, (k == 0).then_some(plate.as_str()), colours.get(k).copied().unwrap_or(0))?;
            for (pi, normals) in env_normals.get(k).into_iter().flatten() {
                model.set_env_normals(*pi, normals);
            }
            model.texture_subs = texture_subs.get(k).cloned().unwrap_or_default();
            model.part_modes = part_modes.clone();
            // (placed by the first sync with the original)
            racers.push(Racer { name: code.clone(), car: Car::default(), model: models.len(), progress: 0, laps_done: 0, lap_time: 0.0, best_lap: None, finish_time: None });
            models.push(model);
        }
        // the traffic the original drives (test drives and most races): its cars come after
        // the racers in the car list, their models loaded here (one per code)
        let mut traffic = Vec::new();
        if !cfg.traffic.is_empty() {
            let night = matches!(cfg.variant, "N" | "S");
            for (i, code) in cfg.traffic.iter().enumerate() {
                if let Ok(m) = assets::load_car(data, atlas, code, false, night, None, 0) {
                    models.push(m);
                    traffic.push((racers.len() + i, models.len() - 1));
                }
            }
        } else {
            let night = matches!(cfg.variant, "N" | "S");
            let mut loaded: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for (k, c) in snap.cars.iter().enumerate().skip(racers.len()) {
                if crate::ps1::race::car_type(&c.code).is_some() {
                    log::warn!("race car {} after the traffic in the car list", c.code);
                    continue;
                }
                let mi = match loaded.get(&c.code) {
                    Some(&mi) => mi,
                    None => match assets::load_car(data, atlas, &c.code, false, night, None, c.colour) {
                        Ok(mut m) => {
                            m.texture_subs = snap.texture_subs.get(k).cloned().unwrap_or_default();
                            m.part_modes = snap.part_modes.clone();
                            models.push(m);
                            loaded.insert(c.code.clone(), models.len() - 1);
                            models.len() - 1
                        }
                        Err(e) => {
                            log::warn!("traffic car {}: {e:#}", c.code);
                            continue;
                        }
                    },
                };
                traffic.push((k, mi));
            }
            log::info!("{} traffic cars ({} models)", traffic.len(), loaded.len());
        }
        let fog = match cfg.variant {
            "N" => (Vec3::new(0.03, 0.04, 0.09), 50.0, 240.0),
            "S" => (Vec3::new(0.75, 0.78, 0.82), 40.0, 220.0),
            "W" => (Vec3::new(0.45, 0.48, 0.52), 50.0, 260.0),
            _ => (track.sky_colors.1, 120.0, 420.0),
        };
        let reflections = assets::load_reflection_maps(data, atlas, cfg.track);
        // the ground shadow texture (gShadowPixmap: ZSFX.PSH "SHAD", 32x64)
        let zsfx = data.read("ZSFX.PSH").ok().and_then(|d| crate::formats::psh::Psh::parse(&d).ok());
        let mut zsfx_tex = |name: &str| {
            let s = zsfx.as_ref()?.shapes.iter().find(|s| s.name == name)?;
            let r = atlas.add(s.width as u32, s.height as u32, &s.rgba);
            let (w, h) = (s.width as f32 - 0.5, s.height as f32 - 0.5);
            let uv = |x: f32, y: f32| atlas.uv(r, glam::Vec2::new(x, y));
            Some([uv(0.5, 0.5), uv(w, 0.5), uv(0.5, h), uv(w, h)])
        };
        let shadow_tex = zsfx_tex("SHAD");
        let skid_tex = [zsfx_tex("SKD0"), zsfx_tex("SKD1")];
        let mut game = Self {
            cam_pos: Vec3::ZERO,
            cam_target: Vec3::Z,
            track,
            models,
            racers,
            traffic,
            traffic_override: None,
            path,
            laps: cfg.laps,
            phase: Phase::Intro,
            race_time: 0.0,
            cam_mode: CameraMode::Chase,
            view_player: 0,
            view_ratio: 0.75,
            car_facets_override: None,
            chunks_override: None,
            skids_override: None,
            shadow_tex,
            skid_tex,
            glow_offsets: Default::default(),
            reflections,
            atlas_size: atlas.size as f32,
            cam_up: Vec3::Y,
            tan_half_v: (1.05f32 / 2.0).tan(),
            y_scale: 1.0,
            autopilot: ai::Driver::default(),
            fog,
            night_driving: matches!(cfg.variant, "N" | "S"),
            ps1hud: None,
            sim,
        };
        if let Some(setup) = game.sim.snapshot().hud.clone() {
            match ps1hud::Ps1Hud::new(data, atlas, setup, cfg.track) {
                Ok(h) => game.ps1hud = Some(h),
                Err(e) => log::warn!("original HUD unavailable: {e:#}"),
            }
        }
        game.sync_original();
        for r in &mut game.racers {
            r.progress = Self::nearest_path(&game.path, r.car.pos, 0, n / 2 + 1);
            r.laps_done = if r.progress > n / 2 { -1 } else { 0 };
        }
        Ok(game)
    }

    /// The views the original draws: 2 split screen (top and bottom), else 1.
    pub fn view_count(&self) -> usize {
        self.sim.view_count()
    }

    /// View `k`'s rectangle on the PS1 screen (x, y, w, h) and the screen's size.
    pub fn view_rect(&self, k: usize) -> ([f32; 4], [f32; 2]) {
        let s = &self.sim;
        let last = s.snapshot();
        let screen = [last.display[2].max(1) as f32, last.display[3].max(1) as f32];
        match s.view_snapshot_rect(k) {
            Some(r) => (r.map(|v| v as f32), screen),
            None => ([0.0, 0.0, screen[0], screen[1]], screen),
        }
    }

    /// View `k`'s place in a `w` x `h` window: its band of the 4:3 picture, the window's
    /// full width (the 3D is as wide as the window, as with one view).
    pub fn view_viewport(&self, k: usize, w: f32, h: f32) -> [f32; 4] {
        let (rect, screen) = self.view_rect(k);
        let ah = if w * 3.0 > h * 4.0 { h } else { w * 0.75 };
        let oy = (h - ah) / 2.0;
        [0.0, oy + rect[1] / screen[1] * ah, w, rect[3] / screen[1] * ah]
    }

    /// Look through view `k` (split screen: the player it follows, its camera and what the
    /// original put into it).
    pub fn set_view(&mut self, k: usize) {
        let sim = &self.sim;
        sim.set_view(k);
        self.view_player = sim.view_player().min(self.racers.len().saturating_sub(1));
        let (rect, screen) = self.view_rect(k);
        self.view_ratio = (rect[3] / screen[1]) / (rect[2] / screen[0]).max(1e-3) * 0.75;
        self.apply_camera();
    }

    /// The current view's camera as the original computed it (interpolated).
    fn apply_camera(&mut self) {
        let (_, camera) = self.sim.poses();
        if let Some(cam) = camera {
            self.cam_pos = cam.pos;
            self.cam_target = cam.pos + cam.forward;
            self.cam_up = cam.up;
            // keep the original horizontal field of view on a 4:3 screen
            self.tan_half_v = cam.tan_half_h * self.view_ratio;
            self.y_scale = cam.y_scale;
            // the bumper view does not show the player's car
            self.cam_mode = match self.racers.get(self.view_player) {
                Some(r) if r.car.pos.distance(cam.pos) < 2.0 => CameraMode::Bumper,
                _ => CameraMode::Chase,
            };
        }
    }

    /// Copy car poses and the start sequence from the original simulation.
    fn sync_original(&mut self) {
        let snap = self.sim.snapshot();
        let (poses, _) = self.sim.poses();
        for (r, pose) in self.racers.iter_mut().zip(poses) {
            r.car = Car { pos: pose.pos, vel: pose.vel, yaw: pose.forward.x.atan2(pose.forward.z), up: pose.up, rot: pose.rot, wheel_pose: (pose.wheel_rot, pose.wheel_steer) };
        }
        self.phase = match (snap.countdown, self.phase) {
            (_, Phase::Finished) => Phase::Finished,
            (0, _) => Phase::Intro,
            (s @ 1..=3, _) => Phase::Countdown((4 - s) as f32),
            _ => Phase::Racing,
        };
        // (after the cars: the bumper view is told by the player's car)
        self.apply_camera();
    }

    pub fn player(&self) -> &Racer {
        &self.racers[0]
    }

    fn nearest_path(path: &[Vec3], pos: Vec3, from: usize, window: usize) -> usize {
        let n = path.len();
        let mut best = (f32::MAX, from);
        for k in 0..window * 2 {
            let i = (from + n + k - window) % n;
            let d = path[i].distance_squared(pos);
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }

    pub fn update(&mut self, frame_dt: f32, input: Input) {
        let mut input = input;
        // (after the finish the original drives the car itself and its results screens
        // take the pad: no autopilot then, even when switched on)
        if input.autopilot && self.racers[0].finish_time.is_none() {
            // the autopilot, through the digital pad
            let r = &self.racers[0];
            let c = self.autopilot.controls(&r.car, &self.path, r.progress, frame_dt);
            input.up = c.throttle > 0.5;
            input.down = c.brake > 0.5;
            match &mut input.sticks {
                // an analog pad: steering in proportion
                Some(s) => s[2] = ((c.steer.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8,
                None => {
                    input.left = c.steer < -0.2;
                    input.right = c.steer > 0.2;
                }
            }
            input.handbrake = c.handbrake;
        }
        self.sim.advance(frame_dt, &input);
        self.sync_original();
        let racing = matches!(self.phase, Phase::Racing | Phase::Finished);
        self.track_progress(frame_dt, racing);
    }

    /// Race time, laps and standings after the cars moved.
    fn track_progress(&mut self, frame_dt: f32, racing: bool) {
        let n = self.path.len();
        if racing && self.phase == Phase::Racing {
            self.race_time += frame_dt;
        }
        for r in &mut self.racers {
            let new = Self::nearest_path(&self.path, r.car.pos, r.progress, 24);
            if r.progress > n * 3 / 4 && new < n / 4 {
                r.laps_done += 1;
                if r.laps_done >= 1 {
                    r.best_lap = Some(r.best_lap.map_or(r.lap_time, |b: f32| b.min(r.lap_time)));
                }
                r.lap_time = 0.0;
                if r.laps_done >= self.laps && r.finish_time.is_none() {
                    r.finish_time = Some(self.race_time);
                }
            } else if r.progress < n / 4 && new > n * 3 / 4 {
                r.laps_done -= 1;
            }
            r.progress = new;
            if racing {
                r.lap_time += frame_dt;
            }
        }
        if self.racers[0].finish_time.is_some() {
            self.phase = Phase::Finished;
        }
    }

    pub fn frame_params(&self, aspect: f32) -> FrameParams {
        let fov = 2.0 * self.tan_half_v.atan();
        // reversed-Z projection (the renderer's depth test is Greater, cleared to 0)
        let squash = Mat4::from_scale(Vec3::new(1.0, self.y_scale, 1.0));
        let proj = squash * Mat4::perspective_infinite_reverse_rh(fov, aspect, 0.1);
        let view = Mat4::look_at_rh(self.cam_pos, self.cam_target, self.cam_up);
        let sky_view = Mat4::look_at_rh(Vec3::ZERO, self.cam_target - self.cam_pos, self.cam_up);
        FrameParams {
            world_ranges: self.chunk_list().map(|list| {
                let mut r = vec![self.track.global_range.clone()];
                for (chunk, flags, rez) in list {
                    let Some(b) = self.track.block_ranges.get(chunk) else { continue };
                    // flags: 1 ground (DrawW_DoTrough: full or low detail by the chunk's
                    // resolution, plus the detail surfaces), 2 objects (DrawW_DoObjects),
                    // 4 road lines (DrawW_DoLines)
                    if flags & 1 != 0 {
                        r.push(if rez != 0 { b[0].clone() } else { b[1].clone() });
                        r.push(b[2].clone());
                    }
                    if flags & 2 != 0 {
                        r.push(b[3].clone());
                    }
                    if flags & 4 != 0 {
                        r.push(b[4].clone());
                    }
                }
                r
            }),
            shadows: {
                let mut v = self.skids();
                v.extend(self.shadows());
                v
            },
            subtract_late: {
                let mut v = self.car_reflections().1;
                v.extend(self.car_arch_shadows());
                v
            },
            sky_dome: self.track.sky_dome as u32,
            sky_view_proj: squash * Mat4::perspective_rh(fov, aspect, 1.0, 1000.0) * sky_view,
            sky_top: self.track.sky_colors.0,
            view_proj: proj * view,
            cam_pos: self.cam_pos,
            fog_color: self.fog.0,
            fog_start: self.fog.1,
            fog_end: self.fog.2,
            cam_fwd: (self.cam_target - self.cam_pos).normalize_or(Vec3::Z),
            night_beam: {
                let snap = self.sim.snapshot();
                snap.night_beam.as_ref().filter(|b| b.flags & 1 != 0).map(|b| {
                let car = &self.racers[self.view_player].car;
                    crate::render::NightBeam {
                        origin: car.pos,
                        right: car.right(),
                        forward: car.forward(),
                        z0: b.z0 as f32,
                        xs: b.xs as f32,
                        zs: b.zs as f32,
                        data: std::sync::Arc::new(crate::render::NightBeam::pack(&b.map, &b.table, &snap.light_table)),
                    }
                })
            },
            depth_cue: {
                // the GTE far colour / depth-cue values the game set for this view
                let e = self.sim.snapshot().view_env;
                Some(crate::render::DepthCue {
                    far: Vec3::new(e[0] as f32, e[1] as f32, e[2] as f32) / 16.0,
                    dqa: if self.night_driving { 0.0 } else { e[3] as f32 },
                    dqb: if self.night_driving { 0.0 } else { e[4] as f32 },
                    h: e[5].max(1) as f32,
                })
            },
        }
    }

    /// Cars and camera-facing sprites transformed to world space.
    pub fn dynamic_geometry(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        // chunks whose objects are drawn
        let list = self.chunk_list();
        let objects_on = |block: usize| list.as_ref().is_none_or(|l| l.iter().any(|&(c, f, _)| c == block && f & 2 != 0));
        for s in self.track.sprites.iter().filter(|s| objects_on(s.block)) {
            let to_cam = (self.cam_pos - s.pos).with_y(0.0);
            if to_cam.length_squared() > self.fog.2 * self.fog.2 {
                continue;
            }
            let rot = Quat::from_rotation_y(to_cam.x.atan2(to_cam.z));
            for v in &s.verts {
                out.push(Vertex { pos: (s.pos + rot * Vec3::from(v.pos)).into(), ..*v });
            }
        }
        // keyframed scenery (windmill sails), looping through its frames (ticks per frame
        // counted in 60 Hz fields)
        let fields = self.sim.snapshot().field as f64;
        for a in &self.track.animated {
            let n = a.frames.len();
            let t = (fields / a.ticks as f64).rem_euclid(n as f64);
            let (k, f) = (t as usize % n, t.fract() as f32);
            let (p0, q0) = a.frames[k];
            let (p1, q1) = a.frames[(k + 1) % n];
            let (pos, rot) = (p0.lerp(p1, f), q0.slerp(q1, f));
            for v in &a.verts {
                out.push(Vertex { pos: (pos + rot * Vec3::from(v.pos)).into(), ..*v });
            }
        }
        for (i, r) in self.racers.iter().enumerate() {
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            if r.car.pos.distance_squared(self.cam_pos) > self.fog.2 * self.fog.2 {
                continue;
            }
            let m = &self.models[r.model];
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            // the parts the game draws (R3DCar_ObjectVisible), once it has put cars into a view,
            // lit by the track's light at the car (DrawC: the average of its channels)
            let facets = self.car_facets();
            let (visible, tint, codes): (&[u8], f32, &[u16]) = match facets {
                Some(f) => match f.get(i).and_then(Option::as_ref) {
                    Some(f) => (&f.parts, car_light(f.light) as f32 / 128.0, &f.codes),
                    None => (&[], 1.0, &[]),
                },
                None => (&assets::CAR_PARTS_VISIBLE, 1.0, &[]),
            };
            let hide: Vec<usize> = std::env::var("NFSHS_HIDE_PART").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            for (pi, (part, &on)) in m.parts.iter().zip(visible).enumerate() {
                if on == 0 || on >= 17 || hide.contains(&pi) {
                    continue;
                }
                let tm = match part.wheel {
                    Some(w) => {
                        // the original: front wheels turned by wheelRot[0] and steered, rear by
                        // wheelRot[1] (R3DCar_InsertCarFacet)
                        let (rot, steer) = r.car.wheel_pose;
                        let (turn, steer) = (rot[(w >= 2) as usize], -steer);
                        let steer = if w < 2 { steer } else { 0.0 };
                        let spin = Quat::from_rotation_x(turn);
                        model * Mat4::from_rotation_translation(Quat::from_rotation_y(steer) * spin, part.hub)
                    }
                    None => model,
                };
                // parts sorted in front of their surroundings (negative bias) are drawn a little
                // towards the camera: the original orders by depth, not a depth buffer
                let pull = (-(assets::CAR_PART_BIAS.get(pi).copied().unwrap_or(0) as f32)).max(0.0) * 0.003;
                // the wheel arches' shadow is subtracted (drawn in the late subtract pass)
                if part.subtract {
                    continue;
                }
                for (t, tri) in part.verts.chunks_exact(3).enumerate() {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    // a lit light or a dent: another texture, a lamp at full brightness
                    let sub = m.substitute(pi, t, codes);
                    let tint = if sub.is_some_and(|s| s.1) { 1.0 } else { tint };
                    for (k, (v, p)) in tri.iter().zip(w).enumerate() {
                        let c = v.color;
                        let p = p + (self.cam_pos - p).normalize_or_zero() * pull;
                        let uv = sub.map_or(v.uv, |s| s.0[k]);
                        out.push(Vertex { pos: p.into(), uv, color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]] });
                    }
                }
            }
        }
        out.extend(self.traffic_geometry());
        out.extend(self.car_reflections().0);
        out
    }

    /// The original's chunk build list this frame: (chunk, draw flags); None without one.
    fn chunk_list(&self) -> Option<Vec<(usize, u8, u8)>> {
        let chunks = match &self.chunks_override {
            Some(c) => c.as_slice(),
            None => self.sim.snapshot().chunks.as_slice(),
        };
        // (chunk, draw flags, resolution)
        (!chunks.is_empty()).then(|| chunks.iter().filter(|c| c.0 >= 0).map(|&(c, f)| (c as usize, (f >> 8) as u8, f as u8)).collect())
    }

    /// What the game draws of each car this frame (None before it has put any car into a view).
    fn car_facets(&self) -> Option<&[Option<crate::ps1::host::CarFacet>]> {
        let f = self.car_facets_override.as_deref().unwrap_or(&self.sim.snapshot().car_facets);
        f.iter().any(Option::is_some).then_some(f)
    }

    /// (R3DCar_ObjectVisible values 17 and up are light halos, drawn by the original's
    /// DrawC_PrimHalo as 2D, not the part's polygons.)
    /// The cars' reflections (DrawC_Prim's environment path, flags 9): each polygon of a
    /// reflection-mapped part again with reflection map A and, for polygons flagged 0x3F3,
    /// map B (scrolled), blended as the maps' texture pages say. Map A's coordinates come
    /// from the vertex normals turned by the camera's yaw around the car (RotMatrix of
    /// R3DCar_yawCam): u = u0 + 64 + x, v = v0 + |z|. Map B's from the normals as they are:
    /// u = u0 + 64 + y, v = v0 + scroll + z (low bytes), inside a texture window (GP0 E2,
    /// the car cache's packet at +72) that keeps them on the 128 x 64 map at (u0, v0).
    /// Returns (blended, subtractive).
    fn car_reflections(&self) -> (Vec<Vertex>, Vec<Vertex>) {
        let (mut add, mut sub) = (Vec::new(), Vec::new());
        let (Some(facets), Some(page)) = (self.car_facets(), self.reflections) else { return (add, sub) };
        let atlas_size = self.atlas_size;
        for (i, (r, f)) in self.racers.iter().zip(facets).enumerate() {
            let Some(env) = f.as_ref().and_then(|f| f.env) else { continue };
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            let m = &self.models[r.model];
            let visible = f.as_ref().map(|f| f.parts.as_slice()).unwrap_or(&[]);
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            let a = env.yaw as f32 / 4096.0 * std::f32::consts::TAU;
            let (sn, cs) = a.sin_cos();
            let rgb = [(env.colour & 0xFF) as f32 / 256.0, ((env.colour >> 8) & 0xFF) as f32 / 256.0, ((env.colour >> 16) & 0xFF) as f32 / 256.0];
            let uv = |u: u8, v: u8| [(page.x as f32 + u as f32 + 0.5) / atlas_size, (page.y as f32 + v as f32 + 0.5) / atlas_size];
            for (part, &on) in m.parts.iter().zip(visible) {
                let (Some((normals, flags)), true) = (&part.env, on != 0) else { continue };
                for (t, &fl) in flags.iter().enumerate() {
                    let tri = &part.verts[t * 3..t * 3 + 3];
                    // per corner: map A's (x, |z|), map B's (y, z), the position
                    let coords: Vec<([u8; 2], [u8; 2], Vec3)> = (0..3)
                        .map(|k| {
                            let n = normals[t * 3 + k];
                            let (nx, nz) = (n[0] as f32, n[2] as f32);
                            let x = (cs * nx + sn * nz) as i32;
                            let z = (-sn * nx + cs * nz) as i32;
                            let p = model.transform_point3(Vec3::from(tri[k].pos));
                            // a hair towards the camera, over the body it covers
                            let p = p + (self.cam_pos - p).normalize_or_zero() * 0.01;
                            ([x as u8, z.unsigned_abs() as u8], [n[1] as u8, n[2] as u8], p)
                        })
                        .collect();
                    for (map, scroll, needs) in [(env.a, 0u8, 0u16), (env.b, env.scroll as u8, 0x3F3)] {
                        let Some(map) = map else { continue };
                        if needs != 0 && fl & needs == 0 {
                            continue;
                        }
                        let (u0, v0) = (map[0] as u8, (map[0] >> 8) as u8);
                        let abr = (map[1] >> 16 >> 5) & 3;
                        for &(a, b, p) in &coords {
                            let (u, v) = if needs == 0 {
                                (u0.wrapping_add(64).wrapping_add(a[0]), v0.wrapping_add(a[1]))
                            } else {
                                let (u, v) = (u0.wrapping_add(64).wrapping_add(b[0]), v0.wrapping_add(scroll).wrapping_add(b[1]));
                                ((u & 0x7F) | (u0 & 0x80), (v & 0x3F) | (v0 & 0xC0))
                            };
                            let vert = Vertex {
                                pos: p.into(),
                                uv: uv(u, v),
                                color: [rgb[0], rgb[1], rgb[2], if abr == 1 { 0.0 } else { 0.5 }],
                            };
                            if abr == 2 { sub.push(vert) } else { add.push(vert) }
                        }
                    }
                }
            }
        }
        (add, sub)
    }

    /// The traffic cars as the original places them (its poses), the parts it draws.
    fn traffic_geometry(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let (poses, facets): (Vec<original::Pose>, Vec<Option<crate::ps1::host::CarFacet>>) = match &self.traffic_override {
            Some(o) => {
                let n = self.racers.len();
                let mut p = vec![original::Pose::default(); n];
                let mut f = vec![None; n];
                for (pose, facet) in o {
                    p.push(*pose);
                    f.push(Some(facet.clone()));
                }
                (p, f)
            }
            None => (self.sim.poses().0, self.sim.snapshot().car_facets.clone()),
        };
        for &(k, mi) in &self.traffic {
            let (Some(pose), Some(m)) = (poses.get(k), self.models.get(mi)) else { continue };
            if pose.pos.distance_squared(self.cam_pos) > self.fog.2 * self.fog.2 {
                continue;
            }
            if std::env::var_os("NFSHS_TRAFLOG").is_some() && pose.pos.distance(self.cam_pos) < 30.0 {
                eprintln!("traffic {k}: origin y {:.2} up {:.2?}", pose.pos.y, pose.up);
            }
            let facet = facets.get(k).and_then(Option::as_ref);
            // not put into the view by the original this frame, or its drawing set-up
            // (DrawC_PrimStart) refused the car (parked traffic waiting to enter): not drawn
            let Some(facet) = facet else { continue };
            let parked = self.sim.snapshot().cars.get(k).is_some_and(|c| c.parked);
            if self.traffic_override.is_none() && (facet.env.is_none() || parked) {
                continue;
            }
            let tint = car_light(facet.light) as f32 / 128.0;
            let model = Mat4::from_rotation_translation(pose.rot, pose.pos);
            for (pi, (part, &on)) in m.parts.iter().zip(&facet.parts).enumerate() {
                if on == 0 || on >= 17 || part.subtract {
                    continue;
                }
                let _ = pi;
                let tm = if part.wheel.is_some() { model * Mat4::from_translation(part.hub) } else { model };
                for (t, tri) in part.verts.chunks_exact(3).enumerate() {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    let sub = m.substitute(pi, t, &facet.codes);
                    let tint = if sub.is_some_and(|s| s.1) { 1.0 } else { tint };
                    for (k, (v, p)) in tri.iter().zip(w).enumerate() {
                        let c = v.color;
                        let uv = sub.map_or(v.uv, |s| s.0[k]);
                        out.push(Vertex { pos: p.into(), uv, color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]] });
                    }
                }
            }
        }
        out
    }

    /// The cars' wheel-arch shadows (parts textured "shdw"), subtracted like the PS1 does.
    fn car_arch_shadows(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let facets = self.car_facets();
        for (i, r) in self.racers.iter().enumerate() {
            if i == self.view_player && self.cam_mode == CameraMode::Bumper {
                continue;
            }
            let m = &self.models[r.model];
            let visible: &[u8] = match facets {
                Some(f) => f.get(i).and_then(Option::as_ref).map(|f| f.parts.as_slice()).unwrap_or(&[]),
                None => &assets::CAR_PARTS_VISIBLE,
            };
            let model = Mat4::from_rotation_translation(r.car.rot, r.car.pos);
            let hide: Vec<usize> = std::env::var("NFSHS_HIDE_PART").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
            for (pi, (part, &on)) in m.parts.iter().zip(visible).enumerate() {
                if on == 0 || on >= 17 || !part.subtract || hide.contains(&pi) {
                    continue;
                }
                for v in &part.verts {
                    out.push(Vertex { pos: model.transform_point3(Vec3::from(v.pos)).into(), ..*v });
                }
            }
        }
        out
    }

    /// A point on the road moved towards the camera along its view ray: the same place on
    /// screen, in front of the road (the PS1 draws the shadow over the road it lies on, sorted
    /// by depth, without a depth buffer). Far enough for the road's camber and slopes under a
    /// flat quad, not so far that kerbs and walls in front stop hiding it.
    fn over_road(&self, p: Vec3) -> Vec3 {
        let to_cam = self.cam_pos - p;
        let d = to_cam.length();
        p + to_cam / d.max(1e-3) * (0.25 + d * 0.06).min(1.2).min(d * 0.5)
    }

    /// The cars' ground shadows (DrawC_ShadowPrim): the "shdw" texture on the quad
    /// Newton_CalcRealShadowCoordinates computed, coloured by the light, subtracted.
    fn shadows(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let Some(facets) = self.car_facets() else { return out };
        for (r, f) in self.racers.iter().zip(facets) {
            let (Some(f), Some(uv)) = (f, self.shadow_tex) else { continue };
            let Some((corners, rgb)) = f.shadow else { continue };
            let s = car_light(f.light);
            let c = rgb.map(|k| ((s * k as u32) >> 8) as f32 / 256.0);
            if corners.iter().all(|c| *c == [0; 3]) {
                continue;
            }
            // (the original has no depth buffer: keep the quad from sinking under the road)
            let ground = r.car.pos.y;
            let p = corners.map(|v| {
                let q = Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0;
                self.over_road(Vec3::new(r.car.pos.x + q.x, q.y.max(ground) + 0.03, r.car.pos.z + q.z))
            });
            for k in [0, 1, 3, 0, 3, 2] {
                out.push(Vertex { pos: p[k].into(), uv: uv[k], color: [c[0], c[1], c[2], 1.0] });
            }
        }
        // the traffic's shadows (their corners are relative to the car's simulated position)
        if let Some(uv) = self.shadow_tex {
            let snap = self.sim.snapshot();
            for &(k, _) in &self.traffic {
                let Some(f) = snap.car_facets.get(k).and_then(Option::as_ref) else { continue };
                if f.env.is_none() || snap.cars.get(k).is_some_and(|c| c.parked) {
                    continue;
                }
                let Some((corners, rgb)) = f.shadow else { continue };
                let Some(car) = snap.cars.get(k) else { continue };
                if corners.iter().all(|c| *c == [0; 3]) {
                    continue;
                }
                let s = car_light(f.light);
                let c = rgb.map(|k| ((s * k as u32) >> 8) as f32 / 256.0);
                let (cx, cz) = (-(car.position[0] as f32) / 65536.0, car.position[2] as f32 / 65536.0);
                let ground = car.position[1] as f32 / 65536.0 - car.dimension[1] as f32 / 65536.0;
                let p = corners.map(|v| {
                    let q = Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0;
                    self.over_road(Vec3::new(cx + q.x, q.y.max(ground) + 0.03, cz + q.z))
                });
                for k in [0, 1, 3, 0, 3, 2] {
                    out.push(Vertex { pos: p[k].into(), uv: uv[k], color: [c[0], c[1], c[2], 1.0] });
                }
            }
        }
        out
    }

    /// The tyre marks (Draw_kCtrlSkidmark): each entry's quad to the previous one, textured
    /// SKD0/SKD1 and subtracted, gouraud between the two entries' colours. Drawn for the
    /// track slices in view within 50 m (BWorld_IsSliceInBuildList, Draw_CircleClip).
    fn skids(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let snap = self.sim.snapshot();
        let (skids, chunks) = match &self.skids_override {
            Some(o) => (o, self.chunks_override.as_ref()),
            None => (&snap.skids, Some(&snap.chunks)),
        };
        let world = |p: [i32; 3]| Vec3::new(-(p[0] as f32), p[1] as f32, p[2] as f32) / 65536.0;
        // BWorld_IsSliceInBuildList: a slice is a track node, its chunk the node / 8
        let in_view = |slice: i16| chunks.is_none_or(|c| c.is_empty() || c.iter().any(|c| c.0 == slice / 8));
        if std::env::var_os("NFSHS_SKIDLOG").is_some() {
            let near = skids.iter().filter(|q| { let d = world(q.corners[0]) - self.cam_pos; d.x * d.x + d.z * d.z < 3600.0 }).count();
            let listed = skids.iter().filter(|q| in_view(q.slice)).count();
            let slices: std::collections::BTreeSet<i16> = skids.iter().map(|q| q.slice).collect();
            eprintln!("skids: {} quads, {near} near, {listed} in listed chunks; slices {slices:?}; chunks {:?}", skids.len(), chunks.map(|c| c.iter().map(|c| c.0).collect::<Vec<_>>()));
        }
        for q in skids {
            let Some(uv) = self.skid_tex[q.tex as usize & 1] else { continue };
            if !in_view(q.slice) {
                continue;
            }
            let p = q.corners.map(world);
            // Draw_CircleClip: within 50 m (its estimate: the larger of |dx|, |dz| plus a
            // quarter of the smaller)
            let d = (p[0] - self.cam_pos).abs();
            if d.x.max(d.z) + d.x.min(d.z) / 4.0 >= 50.0 {
                continue;
            }
            let p = p.map(|v| self.over_road(v + Vec3::Y * 0.02));
            let col = |c: u32| [(c & 0xFF) as f32 / 256.0, ((c >> 8) & 0xFF) as f32 / 256.0, ((c >> 16) & 0xFF) as f32 / 256.0, 1.0];
            // this entry's edge on the texture's top row, the previous entry's on its bottom
            let c = [col(q.colour[0]), col(q.colour[0]), col(q.colour[1]), col(q.colour[1])];
            for k in [0, 1, 3, 0, 3, 2] {
                out.push(Vertex { pos: p[k].into(), uv: uv[k], color: c[k] });
            }
        }
        out
    }

    /// Race order: index into `racers`, leader first.
    pub fn standings(&self) -> Vec<usize> {
        let n = self.path.len();
        let mut order: Vec<usize> = (0..self.racers.len()).collect();
        order.sort_by(|&a, &b| {
            let (ra, rb) = (&self.racers[a], &self.racers[b]);
            match (ra.finish_time, rb.finish_time) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => rb.distance(n).cmp(&ra.distance(n)),
            }
        });
        order
    }

    /// The HUD as the original drew it this frame (its views' primitives, recorded), when it
    /// was recorded: 2D for a `w` x `h` window with its own atlas.
    pub fn hud_frame(&self, w: f32, h: f32) -> Option<(fe::FeFrame, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        // the sky layer first (its own vertices), the light glows, then the HUD, in one
        // atlas; paused, the HUD is the host's (empty here)
        let (atlas, sky, glows, glows_sub) = self.layers(crate::assets::Atlas::new(1024), w, h, [0.0, 0.0, w, h], [0, 0])?;
        // the weather (rain streaks, drops on the lens) under the HUD
        let mut cmds = s.weather.clone();
        if !s.paused {
            cmds.extend(s.overlay.iter().cloned());
        }
        let hud = fe::FeFrame::build_in(atlas, &cmds, vram, screen, w, h);
        Some((hud, sky, glows, glows_sub))
    }

    /// The current view's 2D (split screen) in a `w` x `h` window, the view at `vp` there:
    /// its sky layer, glows (normal and subtracted) and weather, added to `atlas`.
    #[allow(clippy::type_complexity)]
    pub fn view_2d(&self, atlas: crate::assets::Atlas, w: f32, h: f32, vp: [f32; 4]) -> Option<(fe::FeFrame, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let (rect, _) = self.view_rect(self.sim.view());
        let ofs = [rect[0] as i16, rect[1] as i16];
        let (atlas, sky, glows, glows_sub) = self.layers(atlas, w, h, vp, ofs)?;
        let weather: Vec<_> = s.weather.iter().map(|c| c.offset(ofs[0], ofs[1])).collect();
        Some((fe::FeFrame::build_in(atlas, &weather, vram, screen, w, h), sky, glows, glows_sub))
    }

    /// The HUD over the whole window (split screen: after the views), added to `atlas`.
    pub fn hud_2d(&self, atlas: crate::assets::Atlas, w: f32, h: f32) -> Option<fe::FeFrame> {
        let s = self.sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let cmds = if s.paused { &[][..] } else { &s.overlay[..] };
        Some(fe::FeFrame::build_in(atlas, cmds, vram, screen, w, h))
    }

    /// The current view's sky layer and light glows for a `w` x `h` window, the view's 3D at
    /// `vp` there and its 2D at `ofs` on the PS1 screen.
    #[allow(clippy::type_complexity)]
    fn layers(&self, atlas: crate::assets::Atlas, w: f32, h: f32, vp: [f32; 4], ofs: [i16; 2]) -> Option<(crate::assets::Atlas, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let sim = &self.sim;
        let s = sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let moved = |cmds: &[crate::ps1::host::OverlayCmd]| -> Vec<crate::ps1::host::OverlayCmd> { cmds.iter().map(|c| c.offset(ofs[0], ofs[1])).collect() };
        let mut sky = fe::FeFrame::build_in(atlas, &moved(&s.sky_overlay), vram, screen, w, h);
        let mut atlas = std::mem::replace(&mut sky.atlas, crate::assets::Atlas::new(1));
        let mut glows = Vec::new();
        // (the subtracted ones: PS1 semi-transparency mode 2, the smoke's dark puffs)
        let mut glows_sub = Vec::new();
        // each glow moved from where the original's camera saw its light to where this
        // frame's (interpolated) camera sees it, and set at the depth the original sorts it
        // at (half way to the light: drawn over what is further, behind what is nearer)
        let aspect = vp[2] / vp[3].max(1.0);
        let now = self.frame_params(aspect).view_proj;
        let orig = sim.sim_camera().map(|c| self.view_proj_of(&c, aspect));
        let to_px = |m: Mat4, p: Vec3| -> Option<(Vec2, f32)> {
            let c = m * p.extend(1.0);
            (c.w > 0.05).then(|| (Vec2::new(vp[0] + (c.x / c.w + 1.0) * 0.5 * vp[2], vp[1] + (1.0 - c.y / c.w) * 0.5 * vp[3]), c.z / c.w))
        };
        let poses = sim.poses().0;
        // the PS1 puts its 2D on whole pixels of its 320 x 240 screen: a light's glow is up to
        // a pixel off its light, differently every frame, which this picture's finer pixels
        // show as a wobble. Each glow keeps a smoothed offset from its light's exact
        // projection and is drawn at that, never more than a PS1 pixel from where the PS1 drew
        // it (a car's halos move with its lights, which the smoothing must not lag behind).
        let (aw, ah) = if w * 3.0 > h * 4.0 { (h * 4.0 / 3.0, h) } else { (w, w * 0.75) };
        let scale = Vec2::new(aw / screen.0 as f32, ah / screen.1 as f32);
        let origin = Vec2::new((w - aw) / 2.0, (h - ah) / 2.0);
        let view_at = [vp[0] as i32, vp[1] as i32];
        let mut offsets = self.glow_offsets.borrow_mut();
        let mut seen = Vec::new();
        let mut steadied = |key: GlowKey, centre: Option<Vec2>, p_orig: Vec2| -> Vec2 {
            let Some(centre) = centre else { return Vec2::ZERO };
            seen.push(key);
            let off = (origin + centre * scale - p_orig) / scale;
            let e = offsets.entry(key).or_insert((off, s.frame));
            // (another shape, or the glow came back: start over)
            if (off - e.0).length() > 4.0 {
                e.0 = off;
            } else if e.1 != s.frame {
                e.0 += (off - e.0) * 0.15;
            }
            e.1 = s.frame;
            (off - e.0).clamp(Vec2::splat(-1.0), Vec2::splat(1.0)) * scale
        };
        // the sun as seen by this frame's camera: its flare moved from where the original's
        // camera saw the sun's direction to where this one does (the sky is drawn with it)
        if let (Some(orig_vp), Some(cam)) = (orig, sim.sim_camera()) {
            let dir = Vec3::new(-(s.sun_dir[0] as f32), s.sun_dir[1] as f32, s.sun_dir[2] as f32).normalize_or_zero() * 500.0;
            if let (Some((p_now, _)), Some((p_orig, _))) = (to_px(now, self.cam_pos + dir), to_px(orig_vp, cam.pos + dir)) {
                let first = s.sky_overlay.iter().find(|c| matches!(c, crate::ps1::host::OverlayCmd::Quad { .. }));
                let centre = quads_centre(first.map(|c| c.offset(ofs[0], ofs[1])).as_ref());
                let d = p_now - p_orig - steadied((SUN_KEY, 0, view_at), centre, p_orig);
                for v in &mut sky.vertices {
                    v.pos[0] += d.x;
                    v.pos[1] += d.y;
                }
            }
        }
        for g in &s.glows {
            let raw = Vec3::new(-(g.anchor[0] as f32), g.anchor[1] as f32, g.anchor[2] as f32) / 65536.0;
            // a car's light follows the drawn (interpolated) car; its size keeps the glow in
            // front of the car's own body
            let car = g.car.and_then(|a| s.cars.iter().position(|c| c.addr == a));
            let (now_p, radius) = match car {
                Some(k) if k < self.racers.len() => (raw + (self.racers[k].car.pos - original::pose(&s.cars[k]).pos), self.models[self.racers[k].model].half_extents.length()),
                Some(k) => {
                    let drawn = poses.get(k).map_or(raw, |p| raw + (p.pos - original::pose(&s.cars[k]).pos));
                    let size = self.traffic.iter().find(|t| t.0 == k).and_then(|t| self.models.get(t.1)).map_or(2.5, |m| m.half_extents.length());
                    (drawn, size)
                }
                None => (raw, 0.0),
            };
            let (Some(orig_vp), Some((p_now, _))) = (orig, to_px(now, now_p)) else { continue };
            let Some((p_orig, _)) = to_px(orig_vp, raw) else { continue };
            // the depth it is tested at: a light on a car just in front of that car (the PS1
            // draws the halos over the body), any other half way to it (over what is further,
            // behind what is nearer)
            let dist = now_p.distance(self.cam_pos);
            let toward = if radius > 0.0 { (radius + 0.2).min(dist * 0.9) } else { dist * 0.5 };
            let front = now_p + (self.cam_pos - now_p).normalize_or_zero() * toward;
            let Some((_, depth)) = to_px(now, front) else { continue };
            // (a puff is reprojected afterwards: not cut at the original's screen edges)
            let f = fe::FeFrame::build_clipped(atlas, &moved(&g.cmds), vram, screen, w, h, !g.smoke);
            atlas = f.atlas;
            let d = if g.smoke { p_now - p_orig } else { p_now - p_orig - steadied((g.anchor, g.car.unwrap_or(0), view_at), quads_centre(&moved(&g.cmds)), p_orig) };
            if g.smoke && std::env::var_os("NFSHS_NOSMOKE").is_some() {
                continue;
            }
            let is_sub = |i: usize| f.subtract.iter().any(|r| r.contains(&(i as u32)));
            if !g.smoke {
                for (i, v) in f.vertices.iter().enumerate() {
                    let v = Vertex { pos: [v.pos[0] + d.x, v.pos[1] + d.y, 1.0 - 2.0 * depth], ..*v };
                    if is_sub(i) { glows_sub.push(v) } else { glows.push(v) }
                }
                continue;
            }
            // a smoke puff, close to the camera: each corner taken back into the world at the
            // puff's depth as the original's camera saw it, then seen by this frame's camera
            let Some((_, z_orig)) = to_px(orig_vp, raw) else { continue };
            let (orig_inv, inv) = (orig_vp.inverse(), now.inverse());
            let ndc = |x: f32, y: f32| Vec2::new((x - vp[0]) / vp[2] * 2.0 - 1.0, 1.0 - (y - vp[1]) / vp[3] * 2.0);
            // the puff is a flat picture standing on the ground: below its anchor it covers the
            // road nearer the camera, which must not hide it (the PS1 draws it whole, sorted by
            // its anchor), so there it takes the depth of the road it covers
            let road_depth = |x: f32, y: f32| -> Option<f32> {
                let n = ndc(x, y);
                let near = inv.project_point3(n.extend(1.0));
                let dir = (inv.project_point3(n.extend(0.5)) - near).normalize_or_zero();
                let t = (raw.y + 0.05 - near.y) / dir.y;
                (dir.y < -1e-4 && t > 0.0).then(|| to_px(now, near + dir * t).map(|(_, z)| z)).flatten()
            };
            for (i, &v) in f.vertices.iter().enumerate() {
                let world = orig_inv.project_point3(ndc(v.pos[0], v.pos[1]).extend(z_orig));
                // (a vertex is never dropped: the list is triangles)
                let p = to_px(now, world).map_or(Vec2::new(v.pos[0] + d.x, v.pos[1] + d.y), |(p, _)| p);
                let pulled = world + (self.cam_pos - world).normalize_or_zero() * 0.5;
                let z = to_px(now, pulled).map_or(depth, |(_, z)| z);
                let z = match road_depth(p.x, p.y) {
                    Some(r) if r > z => r,
                    _ => z,
                };
                let v = Vertex { pos: [p.x, p.y, 1.0 - 2.0 * z], ..v };
                if is_sub(i) { glows_sub.push(v) } else { glows.push(v) }
            }
        }
        offsets.retain(|k, _| k.2 != view_at || seen.contains(k));
        Some((atlas, sky.vertices, glows, glows_sub))
    }

    /// The view-projection of a camera the original computed (as `frame_params` builds it).
    fn view_proj_of(&self, c: &original::Camera, aspect: f32) -> Mat4 {
        let tan_half_v = c.tan_half_h * self.view_ratio;
        let fov = 2.0 * tan_half_v.atan();
        let squash = Mat4::from_scale(Vec3::new(1.0, c.y_scale, 1.0));
        let proj = squash * Mat4::perspective_infinite_reverse_rh(fov, aspect, 0.1);
        proj * Mat4::look_at_rh(c.pos, c.pos + c.forward, c.up)
    }

    /// The original's race is paused (its menu up).
    pub fn paused(&self) -> bool {
        self.sim.snapshot().paused
    }

    /// The host's port of the original HUD (shown while the race is paused, under the pause
    /// menu, when the recorded 2D is kept).
    pub fn hud(&self, w: f32, h: f32) -> Vec<Vertex> {
        let Some(hud) = &self.ps1hud else { return Vec::new() };
        let s = self.sim.snapshot();
        let frame = ps1hud::HudFrame {
            dash: &s.dash,
            layout: &s.hud_layout,
            map: &s.map,
            blink: (s.sim_ticks >> 4) & 1 != 0,
            countdown: s.countdown,
            countdown_ticks: s.ticks.wrapping_sub(s.countdown_start) as i32,
            overlay: &s.overlay,
        };
        hud.vertices(&frame, w, h)
    }
}

/// A car's flat lighting level from the track light (R3DCar_InsertCarFacetII: the average
/// of the light's channels, at least 24).
fn car_light(light: u32) -> u32 {
    let (r, g, b) = (light & 0xFF, (light >> 8) & 0xFF, (light >> 16) & 0xFF);
    ((r + g + b) / 3).max(24)
}

/// A car polygon facing the camera: the PS1 draws the cars' polygons one-sided (DrawC_Prim
/// skips those whose NCLIP is not positive), so the underside and the far side are not seen.
pub(crate) fn front_facing(cam: Vec3, p: [Vec3; 3]) -> bool {
    (p[1] - p[0]).cross(p[2] - p[0]).dot(cam - p[0]) * CAR_WINDING > 0.0
}

/// The car models' winding in world space (their x is mirrored from the PS1's).
const CAR_WINDING: f32 = -1.0;

