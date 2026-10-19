//! A race as the original runs it: its cars, camera and 2D, drawn by this engine.

pub mod ai;
pub mod car;
pub mod fe;
mod layers;
pub mod original;
pub mod ps1hud;
mod world;

use crate::assets::{self, Atlas, CarModel, GameData, TrackAssets, Vertex};
use crate::render::FrameParams;
use anyhow::Result;
use car::Car;
use glam::{Mat4, Quat, Vec2, Vec3};
use layers::GlowKey;

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
    /// The gamepads' PS1 buttons (controller 1 besides the keyboard above, and player 2's),
    /// as they are.
    pub pad1: u16,
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
    /// Where the sun was drawn this frame for the original's lens flare to be tested (its
    /// centre in window pixels and a PS1 pixel's size there), see [`Game::sun_probe`].
    sun_probe: std::cell::Cell<Option<([f32; 2], [f32; 2])>>,
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

        let snap = sim.snapshot();
        // (the player's car as the original runs it: GS_SET can make it a police car)
        let player = snap.cars.first().map(|c| c.code.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| cfg.player_car.to_string());
        let mut codes = vec![player];
        codes.extend(cfg.opponents.iter().cloned());
        let mut models = Vec::new();
        let mut racers = Vec::new();
        // the race cars (the player first, then the opponents in the car list), as the original
        // set them up: paint, reflection normals, texture substitutes (lit lights, dents)
        let race_car = |k: usize| k == 0 || snap.cars.get(k).is_some_and(|c| crate::ps1::race::car_type(&c.code).is_some());
        let mut colours: Vec<u8> = snap.cars.iter().enumerate().filter(|(k, _)| race_car(*k)).map(|(_, c)| c.colour).collect();
        // NFSHS_PAINT=n (debug): the player's car's textures in paint n (CarState::colour)
        if let (Some(c), Some(p)) = (colours.first_mut(), std::env::var("NFSHS_PAINT").ok().and_then(|v| v.parse().ok())) {
            *c = p;
        }
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
            sun_probe: Default::default(),
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

    /// The original's race is paused (its menu up).
    pub fn paused(&self) -> bool {
        self.sim.snapshot().paused
    }

}

