//! Gameplay: a circuit race between the player and AI opponents.

pub mod fe;
pub mod ai;
pub mod car;
pub mod collision;
pub mod hud;
pub mod original;
pub mod flare;
pub mod ps1hud;

use crate::assets::{self, Atlas, CarModel, GameData, TrackAssets, Vertex};
use crate::render::FrameParams;
use anyhow::Result;
use car::{Car, CarSpec, Controls};
use collision::CollisionWorld;
use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub handbrake: bool,
    pub reset: bool,
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
}

#[derive(Clone, Copy, PartialEq)]
pub enum CameraMode {
    Chase,
    Far,
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
    pub driver: Option<ai::Driver>,
    /// Nearest virtual-road sample.
    pub progress: usize,
    /// Completed laps (-1 while still behind the start line on the grid).
    pub laps_done: i32,
    pub lap_time: f32,
    pub best_lap: Option<f32>,
    pub finish_time: Option<f32>,
    /// Throttle applied last frame (for engine sound).
    pub throttle: f32,
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
    /// Run the recompiled original simulation (false: the engine's own physics, e.g. when
    /// poses come from elsewhere).
    pub original_sim: bool,
    /// Traffic models to load when the simulation does not list them (reference renders).
    pub traffic: Vec<String>,
}

pub struct Game {
    pub track: TrackAssets,
    pub models: Vec<CarModel>,
    pub world: CollisionWorld,
    pub racers: Vec<Racer>,
    /// The original's traffic (cars after the racers in its car list): snapshot index and
    /// model (index into `models`).
    pub traffic: Vec<(usize, usize)>,
    /// Traffic poses and what is drawn of each (reference renders), instead of the sim's.
    pub traffic_override: Option<Vec<(original::Pose, crate::ps1::host::CarFacet)>>,
    /// Virtual-road centre line (world space) used for progress, resets and start grid.
    pub path: Vec<Vec3>,
    /// (left, right) wall distance per path sample.
    pub walls: Vec<(f32, f32)>,
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
    prev_camera_key: bool,
    autopilot: ai::Driver,
    accum: f32,
    font: hud::Font,
    pub fog: (Vec3, f32, f32),
    /// Ambient light multiplier (night < 1).
    pub ambient: f32,
    pub night: bool,
    /// Night driving (the track's night variant): the original draws the world without its
    /// depth cue then (DrawW_DrawQuad takes DrawW_NightColorCalc's colours instead).
    pub night_driving: bool,
    /// The recompiled original simulation, when it drives this race.
    pub sim: Option<original::OriginalSim>,
    /// The original HUD (when the simulation provides its layout).
    pub ps1hud: Option<ps1hud::Ps1Hud>,
    /// The track's glowing lights (from the original's track data).
    glares: Vec<crate::ps1::race::Glare>,
    /// The track's reflection map page (atlas rectangle) and the atlas' size.
    reflections: Option<assets::Rect>,
    atlas_size: f32,
    /// A white texel with the semi-transparency bit, for untextured blended primitives.
    white_semi: [f32; 2],
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

const DT: f32 = 1.0 / 120.0;

fn load_spec(data: &GameData, code: &str, model: &CarModel) -> CarSpec {
    let mut spec = CarSpec { wheel_radius: model.wheel_radius, half_width: model.half_extents.x * 0.9, ..Default::default() };
    match data.read(&format!("Z{code}.QDA")).and_then(crate::formats::compress::maybe_decompress) {
        Ok(qda) => spec.apply_qda(&qda),
        Err(e) => log::warn!("no engine data for {code}: {e}"),
    }
    log::info!("{code}: redline {} rpm, top speed {:.0} km/h", spec.redline, spec.top_speed() * 3.6);
    spec
}

impl Game {
    pub fn load(data: &GameData, atlas: &mut Atlas, cfg: &RaceConfig) -> Result<Self> {
        Self::load_with(data, atlas, cfg, None)
    }

    /// Load the race `cfg` describes; `running` is the original already in this race (started
    /// from its own front end), else one is started for it.
    pub fn load_with(data: &GameData, atlas: &mut Atlas, cfg: &RaceConfig, running: Option<original::OriginalSim>) -> Result<Self> {
        // the original game's simulation drives the race unless disabled (NFSHS_OWN_PHYSICS=1)
        // or it cannot run this setup
        let sim = if running.is_some() {
            running
        } else if !cfg.original_sim || std::env::var_os("NFSHS_OWN_PHYSICS").is_some() {
            None
        } else {
            match original::OriginalSim::start(data, cfg) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("original simulation unavailable, using the engine's physics: {e:#}");
                    None
                }
            }
        };
        // the track's vertex colours come from the original's light table
        let light = sim.as_ref().map(|s| s.snapshot().light_table.clone()).unwrap_or_default();
        if std::env::var_os("NFSHS_LIGHTLOG").is_some() {
            println!("light table: {}", light.iter().step_by(8).map(|c| format!("{:06x}", c & 0xFF_FFFF)).collect::<Vec<_>>().join(" "));
        }
        let sky = sim.as_ref().and_then(|s| s.snapshot().sky.clone());
        let track = assets::load_track(data, atlas, cfg.track, cfg.variant, &light, sky.as_ref())?;
        let font = hud::Font::new(atlas);
        let world = CollisionWorld::new(track.tris.clone());
        let mut path: Vec<Vec3> = track.track.vroad.iter().map(|v| assets::ps1_to_world(v.pos)).collect();
        let mut walls: Vec<(f32, f32)> = track.track.vroad.iter().map(|v| (v.left_wall, v.right_wall)).collect();
        if path.len() < 2 {
            path = track.nodes.clone();
            walls = vec![(15.0, 15.0); path.len()];
        }
        let n = path.len();

        let mut codes = vec![cfg.player_car.to_string()];
        codes.extend(cfg.opponents.iter().cloned());
        let mut models = Vec::new();
        let mut racers = Vec::new();
        let slots = codes.len();
        // the paint of each car, as the original picked it (cars in the order race_of lists them)
        let colours: Vec<u8> = sim
            .as_ref()
            .map(|s| {
                let cars = &s.snapshot().cars;
                cars.iter().take(1).chain(cars.iter().skip(1).filter(|c| crate::ps1::race::car_type(&c.code).is_some())).map(|c| c.colour).collect()
            })
            .unwrap_or_default();
        log::info!("car colours {colours:?}");
        for (k, code) in codes.iter().enumerate() {
            let night = matches!(cfg.variant, "N" | "S");
            // the number plate shows the player's name (GameSetup carInfo + 0x68)
            let plate = crate::ps1::race::player_plate();
            let model = assets::load_car(data, atlas, code, k == 0, night, (k == 0).then_some(plate.as_str()), colours.get(k).copied().unwrap_or(0))?;
            let spec = load_spec(data, code, &model);
            // two-wide grid behind the start line, ~12 m between rows; the player starts last
            let slot = if k == 0 { slots - 1 } else { k - 1 };
            let row = slot / 2;
            let idx = (n * 4 - 2 - row * 2) % n;
            let (p, yaw) = Self::spawn_point(&path, idx);
            let side = if slot % 2 == 0 { -2.5 } else { 2.5 };
            let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            let right = Vec3::new(-fwd.z, 0.0, fwd.x);
            let mut pos = p + right * side;
            if let Some(h) = world.ground(pos + Vec3::Y * 5.0, 10.0) {
                pos.y = h.height;
            }
            racers.push(Racer {
                name: code.clone(),
                car: Car::new(spec, pos, yaw),
                model: models.len(),
                driver: (k > 0).then(|| ai::Driver::new(side * 0.6, 0.92 + 0.03 * (k % 3) as f32)),
                progress: idx,
                laps_done: if idx > n / 2 { -1 } else { 0 },
                lap_time: 0.0,
                best_lap: None,
                finish_time: None,
                throttle: 0.0,
            });
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
        } else if let Some(s) = &sim {
            let night = matches!(cfg.variant, "N" | "S");
            let mut loaded: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for (k, c) in s.snapshot().cars.iter().enumerate().skip(racers.len()) {
                if crate::ps1::race::car_type(&c.code).is_some() {
                    log::warn!("race car {} after the traffic in the car list", c.code);
                    continue;
                }
                let mi = match loaded.get(&c.code) {
                    Some(&mi) => mi,
                    None => match assets::load_car(data, atlas, &c.code, false, night, None, c.colour) {
                        Ok(m) => {
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
        let player = &racers[0].car;
        let glares: Vec<_> = sim.as_ref().map(|s| s.snapshot().glares.clone()).unwrap_or_default();
        let glares: Vec<_> = glares.into_iter().filter(|g| (g.chunk as usize) < track.track.blocks.len()).collect();
        log::info!("{} glowing lights", glares.len());
        let white_semi = {
            let r = atlas.add(1, 1, &[255, 255, 255, 128]);
            atlas.uv(r, glam::Vec2::new(0.5, 0.5))
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
            cam_pos: player.pos - player.forward() * 7.0 + Vec3::Y * 2.5,
            cam_target: player.pos,
            track,
            models,
            world,
            racers,
            traffic,
            traffic_override: None,
            path,
            walls,
            laps: cfg.laps,
            phase: Phase::Countdown(3.0),
            race_time: 0.0,
            cam_mode: CameraMode::Chase,
            view_player: 0,
            view_ratio: 0.75,
            car_facets_override: None,
            chunks_override: None,
            skids_override: None,
            shadow_tex,
            skid_tex,
            glares,
            white_semi,
            reflections,
            atlas_size: atlas.size as f32,
            cam_up: Vec3::Y,
            tan_half_v: (1.05f32 / 2.0).tan(),
            y_scale: 1.0,
            prev_camera_key: false,
            autopilot: ai::Driver::new(0.0, 1.0),
            accum: 0.0,
            font,
            fog,
            // (this engine's own night and wet looks; races the original runs light their
            // tracks as it does: its night/wet track data and depth cue)
            ambient: match cfg.variant {
                // linear-light multipliers (output is sRGB)
                _ if sim.is_some() => 1.0,
                "N" => 0.1,
                "W" => 0.65,
                _ => 1.0,
            },
            night: cfg.variant == "N" && sim.is_none(),
            night_driving: matches!(cfg.variant, "N" | "S"),
            ps1hud: None,
            sim,
        };
        if let Some(setup) = game.sim.as_ref().and_then(|s| s.snapshot().hud.clone()) {
            match ps1hud::Ps1Hud::new(data, atlas, setup, cfg.track) {
                Ok(h) => game.ps1hud = Some(h),
                Err(e) => log::warn!("original HUD unavailable: {e:#}"),
            }
        }
        if game.sim.is_some() {
            game.phase = Phase::Intro;
            game.sync_original(0.0);
            let p = &game.racers[0].car;
            game.cam_pos = p.pos - p.forward() * 7.0 + Vec3::Y * 2.5;
            game.cam_target = p.pos;
            for i in 0..game.racers.len() {
                game.racers[i].progress = Self::nearest_path(&game.path, game.racers[i].car.pos, 0, n / 2 + 1);
                game.racers[i].laps_done = if game.racers[i].progress > n / 2 { -1 } else { 0 };
            }
        }
        Ok(game)
    }

    /// Copy car poses and the start sequence from the original simulation.
    /// The views the original draws: 2 split screen (top and bottom), else 1.
    pub fn view_count(&self) -> usize {
        self.sim.as_ref().map_or(1, |s| s.view_count())
    }

    /// View `k`'s rectangle on the PS1 screen (x, y, w, h) and the screen's size.
    pub fn view_rect(&self, k: usize) -> ([f32; 4], [f32; 2]) {
        let Some(s) = &self.sim else { return ([0.0, 0.0, 1.0, 1.0], [1.0, 1.0]) };
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
        let Some(sim) = &self.sim else { return };
        sim.set_view(k);
        self.view_player = sim.view_player().min(self.racers.len().saturating_sub(1));
        let (rect, screen) = self.view_rect(k);
        self.view_ratio = (rect[3] / screen[1]) / (rect[2] / screen[0]).max(1e-3) * 0.75;
        self.apply_camera();
    }

    /// The current view's camera as the original computed it (interpolated).
    fn apply_camera(&mut self) {
        let Some(sim) = &self.sim else { return };
        let (_, camera) = sim.poses();
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

    fn sync_original(&mut self, frame_dt: f32) {
        let Some(sim) = &self.sim else { return };
        let snap = sim.snapshot();
        let (poses, _) = sim.poses();
        for ((r, c), pose) in self.racers.iter_mut().zip(&snap.cars).zip(poses) {
            let car = &mut r.car;
            car.pos = pose.pos;
            car.vel = pose.vel;
            car.rot = Some(pose.rot);
            car.up = pose.up;
            car.yaw = pose.forward.x.atan2(pose.forward.z);
            car.rpm = c.rpm as f32;
            // control.gear: 0 reverse, 1 neutral, 2.. forward gears
            car.reversing = c.gear == 0;
            car.gear = (c.gear as i32 - 2).max(0) as usize;
            car.steer = (c.steering as f32 / 128.0).clamp(-1.0, 1.0) * 0.5;
            car.wheel_spin += car.vel.dot(pose.forward) / car.spec.wheel_radius.max(0.2) * frame_dt;
            car.wheel_pose = Some((pose.wheel_rot, pose.wheel_steer));
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

    pub fn font(&self) -> &hud::Font {
        &self.font
    }

    pub fn player(&self) -> &Racer {
        &self.racers[0]
    }

    fn spawn_point(path: &[Vec3], i: usize) -> (Vec3, f32) {
        let n = path.len();
        let a = path[i % n];
        let b = path[(i + 2) % n];
        let d = b - a;
        (a, d.x.atan2(d.z))
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

    fn reset_racer(&mut self, i: usize) {
        let (p, yaw) = Self::spawn_point(&self.path, self.racers[i].progress);
        let ground = self.world.ground(p + Vec3::Y * 3.0, 6.0).map(|h| h.height).unwrap_or(p.y);
        self.racers[i].car.reset(Vec3::new(p.x, ground, p.z), yaw);
    }

    pub fn update(&mut self, frame_dt: f32, input: Input) {
        if input.camera && !self.prev_camera_key && self.sim.is_none() {
            self.cam_mode = match self.cam_mode {
                CameraMode::Chase => CameraMode::Far,
                CameraMode::Far => CameraMode::Bumper,
                CameraMode::Bumper => CameraMode::Chase,
            };
        }
        self.prev_camera_key = input.camera;
        if self.sim.is_some() {
            let mut input = input;
            if input.autopilot || self.racers[0].finish_time.is_some() {
                // the engine's driver, through the digital pad
                let r = &self.racers[0];
                let c = self.autopilot.controls(&r.car, &self.path, r.progress, frame_dt);
                input.up = c.throttle > 0.5;
                input.down = c.brake > 0.5;
                input.left = c.steer < -0.2;
                input.right = c.steer > 0.2;
                input.handbrake = c.handbrake;
            }
            for r in &mut self.racers {
                r.throttle = 0.0;
            }
            self.racers[0].throttle = if input.up || input.throttle_axis > 0.3 { 1.0 } else { 0.0 };
            if let Some(sim) = &mut self.sim {
                sim.advance(frame_dt, &input);
            }
            self.sync_original(frame_dt);
            let racing = matches!(self.phase, Phase::Racing | Phase::Finished);
            self.track_progress(frame_dt, racing);
            return;
        }
        if input.reset {
            self.reset_racer(0);
        }
        let racing = match &mut self.phase {
            Phase::Countdown(t) => {
                *t -= frame_dt;
                if *t <= 0.0 {
                    self.phase = Phase::Racing;
                }
                false
            }
            _ => true,
        };

        // nearest car ahead for each racer: (distance, lateral offset, closing speed)
        let traffic: Vec<Option<(f32, f32, f32)>> = (0..self.racers.len())
            .map(|i| {
                let me = &self.racers[i].car;
                let (f, r) = (me.forward(), me.right());
                self.racers
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .filter_map(|(_, o)| {
                        let d = o.car.pos - me.pos;
                        let ahead = d.dot(f);
                        let lat = d.dot(r);
                        (ahead > 0.0 && ahead < 25.0 && lat.abs() < 3.0).then(|| (ahead, lat, (me.vel - o.car.vel).dot(f)))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
            })
            .collect();
        // controls for every racer (frozen during the countdown)
        let mut controls = Vec::with_capacity(self.racers.len());
        for (i, r) in self.racers.iter_mut().enumerate() {
            let c = if !racing {
                Controls { brake: 1.0, ..Default::default() }
            } else if r.finish_time.is_some() && i > 0 {
                Controls { brake: 0.5, ..Default::default() }
            } else if let Some(d) = r.driver.as_mut() {
                let mut c = d.controls(&r.car, &self.path, r.progress, frame_dt);
                if let Some((dist, lat, closing)) = traffic[i] {
                    // move out of the slipstream to pass, and lift if we are about to hit
                    d.nudge = if lat >= 0.0 { -2.5 } else { 2.5 };
                    if dist < 8.0 && closing > 2.0 {
                        c.throttle = 0.0;
                        c.brake = c.brake.max((closing / 10.0).min(0.6));
                    }
                } else {
                    d.nudge *= 0.98;
                }
                c
            } else if input.autopilot || (i == 0 && r.finish_time.is_some()) {
                self.autopilot.controls(&r.car, &self.path, r.progress, frame_dt)
            } else {
                let digital_steer = input.right as u8 as f32 - input.left as u8 as f32;
                let throttle = (input.up as u8 as f32).max(input.throttle_axis);
                let brake = (input.down as u8 as f32).max(input.brake_axis);
                Controls {
                    throttle,
                    brake,
                    reverse: brake > 0.5,
                    steer: if input.steer_axis != 0.0 { input.steer_axis } else { digital_steer },
                    handbrake: input.handbrake,
                }
            };
            r.throttle = if r.car.reversing { c.reverse as u8 as f32 } else { c.throttle };
            controls.push(c);
        }

        self.accum = (self.accum + frame_dt).min(0.25);
        while self.accum >= DT {
            for (r, c) in self.racers.iter_mut().zip(&controls) {
                r.car.step(*c, DT, &self.world);
            }
            self.collide_cars();
            self.accum -= DT;
        }

        for i in 0..self.racers.len() {
            self.enforce_walls(i);
        }
        self.track_progress(frame_dt, racing);
    }

    /// Race time, laps, standings and the camera after the cars moved.
    fn track_progress(&mut self, frame_dt: f32, racing: bool) {
        let n = self.path.len();
        if racing && self.phase == Phase::Racing {
            self.race_time += frame_dt;
        }
        for i in 0..self.racers.len() {
            let r = &mut self.racers[i];
            if r.car.pos.y < self.path[r.progress].y - 40.0 {
                self.reset_racer(i);
            }
            let r = &mut self.racers[i];
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
        if self.sim.is_none() {
            self.update_camera(frame_dt);
        }
    }

    /// Circle-pair contacts between cars (two circles per car along its length).
    fn collide_cars(&mut self) {
        let k = self.racers.len();
        for a in 0..k {
            for b in a + 1..k {
                let (ra, rb) = {
                    let (l, r) = self.racers.split_at_mut(b);
                    (&mut l[a], &mut r[0])
                };
                if ra.car.pos.distance_squared(rb.car.pos) > 64.0 {
                    continue;
                }
                let circles = |c: &Car| {
                    let f = c.forward();
                    [c.pos + f * c.spec.cg_front * 0.8, c.pos - f * c.spec.cg_rear * 0.8]
                };
                let rad = (ra.car.spec.half_width + rb.car.spec.half_width) * 0.95;
                // deepest contact only, so one impact isn't applied once per circle pair
                let mut best: Option<(f32, Vec3)> = None;
                for pa in circles(&ra.car) {
                    for pb in circles(&rb.car) {
                        let mut d = pb - pa;
                        d.y = 0.0;
                        let dist = d.length();
                        if dist < rad && dist > 1e-4 && best.is_none_or(|b| rad - dist > b.0) {
                            best = Some((rad - dist, d / dist));
                        }
                    }
                }
                if let Some((overlap, nrm)) = best {
                    ra.car.pos -= nrm * overlap * 0.5;
                    rb.car.pos += nrm * overlap * 0.5;
                    let rel = (rb.car.vel - ra.car.vel).dot(nrm);
                    if rel < 0.0 {
                        let impulse = -(1.0 + 0.2) * rel * 0.5;
                        ra.car.vel -= nrm * impulse;
                        rb.car.vel += nrm * impulse;
                        ra.car.yaw_rate *= 0.95;
                        rb.car.yaw_rate *= 0.95;
                    }
                }
            }
        }
    }

    /// Keep a car between the virtual-road walls (the original game's track boundary).
    fn enforce_walls(&mut self, idx: usize) {
        let n = self.path.len();
        let r = &mut self.racers[idx];
        let i = Self::nearest_path(&self.path, r.car.pos, r.progress, 8);
        // project onto the segment (i -> i+1) or (i-1 -> i), whichever contains the car
        let (a, b, ia) = {
            let nxt = (i + 1) % n;
            let prv = (i + n - 1) % n;
            let d = self.path[nxt] - self.path[i];
            if (r.car.pos - self.path[i]).dot(d) >= 0.0 { (self.path[i], self.path[nxt], i) } else { (self.path[prv], self.path[i], prv) }
        };
        let seg = (b - a).with_y(0.0);
        let len2 = seg.length_squared().max(1e-6);
        let t = ((r.car.pos - a).with_y(0.0).dot(seg) / len2).clamp(0.0, 1.0);
        let centre = a + (b - a) * t;
        let fwd = seg.normalize_or_zero();
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        let (l0, r0) = self.walls[ia];
        let (l1, r1) = self.walls[(ia + 1) % n];
        let (lw, rw) = (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t);
        let lateral = (r.car.pos - centre).dot(right);
        let margin = r.car.spec.half_width;
        let push = if lateral > rw - margin {
            Some((rw - margin - lateral, right))
        } else if lateral < -(lw - margin) {
            Some((-(lw - margin) - lateral, right))
        } else {
            None
        };
        if let Some((amount, axis)) = push {
            r.car.pos += axis * amount;
            let n_wall = axis * amount.signum();
            let into = r.car.vel.dot(n_wall);
            if into < 0.0 {
                r.car.vel -= n_wall * into * 1.2;
                r.car.vel *= 0.985;
                r.car.yaw_rate *= 0.8;
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let car = &self.racers[0].car;
        let f = car.forward();
        let (dist, height, look) = match self.cam_mode {
            CameraMode::Chase => (6.5, 2.0, 1.0),
            CameraMode::Far => (10.0, 3.5, 1.0),
            CameraMode::Bumper => (-0.4, 1.1, 1.0),
        };
        let desired = car.pos - f * dist + Vec3::Y * height;
        let target = car.pos + Vec3::Y * look + f * 4.0;
        if self.cam_mode == CameraMode::Bumper {
            self.cam_pos = desired;
            self.cam_target = car.pos + f * 20.0 + Vec3::Y * 1.0;
        } else {
            let k = 1.0 - (-dt * 6.0).exp();
            self.cam_pos = self.cam_pos.lerp(desired, k);
            self.cam_target = self.cam_target.lerp(target, 1.0 - (-dt * 12.0).exp());
            // keep the camera above ground
            if let Some(h) = self.world.ground(self.cam_pos + Vec3::Y * 2.0, 4.0) {
                self.cam_pos.y = self.cam_pos.y.max(h.height + 0.5);
            }
        }
    }

    pub fn frame_params(&self, aspect: f32) -> FrameParams {
        let fov = 2.0 * self.tan_half_v.atan();
        // reversed-Z projection (the renderer's depth test is Greater, cleared to 0)
        let squash = Mat4::from_scale(Vec3::new(1.0, self.y_scale, 1.0));
        let proj = squash * Mat4::perspective_infinite_reverse_rh(fov, aspect, 0.1);
        let view = Mat4::look_at_rh(self.cam_pos, self.cam_target, self.cam_up);
        let sky_view = Mat4::look_at_rh(Vec3::ZERO, self.cam_target - self.cam_pos, self.cam_up);
        let p = &self.racers[0].car;
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
            ambient: self.ambient,
            headlight: (if self.night { 2.2 } else { 0.0 }, p.pos + Vec3::Y * 0.7 + p.forward() * 2.0, (p.forward() - Vec3::Y * 0.08).normalize()),
            sky_view_proj: squash * Mat4::perspective_rh(fov, aspect, 1.0, 1000.0) * sky_view,
            sky_top: self.track.sky_colors.0 * if self.night { 0.12 } else { 1.0 },
            view_proj: proj * view,
            cam_pos: self.cam_pos,
            fog_color: self.fog.0,
            fog_start: self.fog.1,
            fog_end: self.fog.2,
            cam_fwd: (self.cam_target - self.cam_pos).normalize_or(Vec3::Z),
            night_beam: self.sim.as_ref().and_then(|s| {
                let snap = s.snapshot();
                let b = snap.night_beam.as_ref().filter(|b| b.flags & 1 != 0)?;
                let car = &self.racers[self.view_player].car;
                Some(crate::render::NightBeam {
                    origin: car.pos,
                    right: car.right(),
                    forward: car.forward(),
                    z0: b.z0 as f32,
                    xs: b.xs as f32,
                    zs: b.zs as f32,
                    data: std::sync::Arc::new(crate::render::NightBeam::pack(&b.map, &b.table, &snap.light_table)),
                })
            }),
            depth_cue: self.sim.as_ref().map(|s| {
                // the GTE far colour / depth-cue values the game set for this view
                let e = s.snapshot().view_env;
                crate::render::DepthCue {
                    far: Vec3::new(e[0] as f32, e[1] as f32, e[2] as f32) / 16.0,
                    dqa: if self.night_driving { 0.0 } else { e[3] as f32 },
                    dqb: if self.night_driving { 0.0 } else { e[4] as f32 },
                    h: e[5].max(1) as f32,
                }
            }),
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
        let fields = match &self.sim {
            Some(s) => s.snapshot().field as f64,
            None => self.race_time as f64 * 60.0,
        };
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
            let model = Mat4::from_rotation_translation(r.car.orientation(), r.car.pos);
            // the parts the game draws (R3DCar_ObjectVisible), once it has put cars into a view,
            // lit by the track's light at the car (DrawC: the average of its channels)
            let facets = self.car_facets();
            let (visible, tint): (&[u8], f32) = match facets {
                Some(f) => match f.get(i).and_then(Option::as_ref) {
                    Some(f) => (&f.parts, car_light(f.light) as f32 / 128.0),
                    None => (&[], 1.0),
                },
                None => (&assets::CAR_PARTS_VISIBLE, 1.0),
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
                        let (turn, steer) = match r.car.wheel_pose {
                            Some((rot, steer)) => (rot[(w >= 2) as usize], -steer),
                            None => (r.car.wheel_spin, -r.car.steer),
                        };
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
                for tri in part.verts.chunks_exact(3) {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    for (v, p) in tri.iter().zip(w) {
                        let c = v.color;
                        let p = p + (self.cam_pos - p).normalize_or_zero() * pull;
                        out.push(Vertex { pos: p.into(), color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]], ..*v });
                    }
                }
            }
        }
        out.extend(self.traffic_geometry());
        out.extend(self.car_reflections().0);
        // the lights' glows last (additive)
        if let Some(sim) = &self.sim {
            let snap = sim.snapshot();

            let forward = (self.cam_target - self.cam_pos).normalize_or(Vec3::Z);
            let right = self.cam_up.cross(forward).normalize_or(Vec3::X) * -1.0;
            let up = forward.cross(right) * -1.0;
            let view = flare::FlareView {
                pos: self.cam_pos,
                right,
                up,
                forward,
                h: snap.projection.max(1) as f32,
                y_scale: self.y_scale,
                counter: snap.sim_ticks,
            };
            // BWorld_BuildGlareEffects goes through the chunks in the build list
            let listed: Vec<_> = match &list {
                Some(l) => self.glares.iter().filter(|g| l.iter().any(|&(c, _, _)| c == g.chunk as usize)).copied().collect(),
                None => self.glares.clone(),
            };
            // with the HUD recorded the original's own glares come with it
            if !sim.hud_recorded() {
                flare::build(&listed, &view, self.white_semi, &mut out);
            }
        }
        out
    }

    /// The original's chunk build list this frame: (chunk, draw flags); None without one.
    fn chunk_list(&self) -> Option<Vec<(usize, u8, u8)>> {
        let chunks = match &self.chunks_override {
            Some(c) => c.as_slice(),
            None => self.sim.as_ref()?.snapshot().chunks.as_slice(),
        };
        // (chunk, draw flags, resolution)
        (!chunks.is_empty()).then(|| chunks.iter().filter(|c| c.0 >= 0).map(|&(c, f)| (c as usize, (f >> 8) as u8, f as u8)).collect())
    }

    /// What the game draws of each car this frame (None before it has put any car into a view).
    fn car_facets(&self) -> Option<&[Option<crate::ps1::host::CarFacet>]> {
        let f = self.car_facets_override.as_deref().or_else(|| self.sim.as_ref().map(|s| s.snapshot().car_facets.as_slice()))?;
        f.iter().any(Option::is_some).then_some(f)
    }

    /// (R3DCar_ObjectVisible values 17 and up are light halos, drawn by the original's
    /// DrawC_PrimHalo as 2D, not the part's polygons.)
    /// The cars' reflections (DrawC_Prim's environment path, flags 9): each polygon of a
    /// reflection-mapped part again with reflection map A and, for polygons flagged 0x3F3,
    /// map B (scrolled), blended as the maps' texture pages say. The map coordinates come
    /// from the vertex normals turned by the camera's yaw around the car (RotMatrix of
    /// R3DCar_yawCam): u = u0 + 64 + x, v = v0 + |z|. Returns (blended, subtractive).
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
            let model = Mat4::from_rotation_translation(r.car.orientation(), r.car.pos);
            let a = env.yaw as f32 / 4096.0 * std::f32::consts::TAU;
            let (sn, cs) = a.sin_cos();
            let rgb = [(env.colour & 0xFF) as f32 / 256.0, ((env.colour >> 8) & 0xFF) as f32 / 256.0, ((env.colour >> 16) & 0xFF) as f32 / 256.0];
            let uv = |u: u8, v: u8| [(page.x as f32 + u as f32 + 0.5) / atlas_size, (page.y as f32 + v as f32 + 0.5) / atlas_size];
            for (part, &on) in m.parts.iter().zip(visible) {
                let (Some((normals, flags)), true) = (&part.env, on != 0) else { continue };
                for (t, &fl) in flags.iter().enumerate() {
                    let tri = &part.verts[t * 3..t * 3 + 3];
                    let coords: Vec<(u8, u8, Vec3)> = (0..3)
                        .map(|k| {
                            let n = normals[t * 3 + k];
                            let (nx, nz) = (n[0] as f32, n[2] as f32);
                            let x = (cs * nx + sn * nz) as i32;
                            let z = (-sn * nx + cs * nz) as i32;
                            let p = model.transform_point3(Vec3::from(tri[k].pos));
                            // a hair towards the camera, over the body it covers
                            let p = p + (self.cam_pos - p).normalize_or_zero() * 0.01;
                            (x as u8, z.unsigned_abs() as u8, p)
                        })
                        .collect();
                    for (map, scroll, needs) in [(env.a, 0u8, 0u16), (env.b, env.scroll as u8, 0x3F3)] {
                        let Some(map) = map else { continue };
                        if needs != 0 && fl & needs == 0 {
                            continue;
                        }
                        let (u0, v0) = (map[0] as u8, (map[0] >> 8) as u8);
                        let abr = (map[1] >> 16 >> 5) & 3;
                        for &(x, z, p) in &coords {
                            let vert = Vertex {
                                pos: p.into(),
                                uv: uv(u0.wrapping_add(64).wrapping_add(x), v0.wrapping_add(scroll).wrapping_add(z)),
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
        let (poses, facets): (Vec<original::Pose>, Vec<Option<crate::ps1::host::CarFacet>>) = match (&self.traffic_override, &self.sim) {
            (Some(o), _) => {
                let n = self.racers.len();
                let mut p = vec![original::Pose::default(); n];
                let mut f = vec![None; n];
                for (pose, facet) in o {
                    p.push(*pose);
                    f.push(Some(facet.clone()));
                }
                (p, f)
            }
            (None, Some(sim)) => (sim.poses().0, sim.snapshot().car_facets.clone()),
            _ => return out,
        };
        for &(k, mi) in &self.traffic {
            let (Some(pose), Some(m)) = (poses.get(k), self.models.get(mi)) else { continue };
            if pose.pos.distance_squared(self.cam_pos) > self.fog.2 * self.fog.2 {
                continue;
            }
            if std::env::var_os("NFSHS_TRAFLOG").is_some() && pose.pos.distance(self.cam_pos) < 30.0 {
                let g = self.world.ground(pose.pos + Vec3::Y * 3.0, 6.0).map(|h| h.height);
                eprintln!("traffic {k}: origin y {:.2} ground {g:?} up {:.2?}", pose.pos.y, pose.up);
            }
            let facet = facets.get(k).and_then(Option::as_ref);
            // not put into the view by the original this frame, or its drawing set-up
            // (DrawC_PrimStart) refused the car (parked traffic waiting to enter): not drawn
            let Some(facet) = facet else { continue };
            let parked = self.sim.as_ref().and_then(|s| s.snapshot().cars.get(k).map(|c| c.parked)).unwrap_or(false);
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
                for tri in part.verts.chunks_exact(3) {
                    let w = [0, 1, 2].map(|k| tm.transform_point3(Vec3::from(tri[k].pos)));
                    if !front_facing(self.cam_pos, w) {
                        continue;
                    }
                    for (v, p) in tri.iter().zip(w) {
                        let c = v.color;
                        out.push(Vertex { pos: p.into(), color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]], ..*v });
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
            let model = Mat4::from_rotation_translation(r.car.orientation(), r.car.pos);
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
        if let (Some(sim), Some(uv)) = (&self.sim, self.shadow_tex) {
            let snap = sim.snapshot();
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
        let snap = self.sim.as_ref().map(|s| s.snapshot());
        let (skids, chunks) = match (&self.skids_override, &snap) {
            (Some(o), _) => (o, self.chunks_override.as_ref()),
            (None, Some(s)) => (&s.skids, Some(&s.chunks)),
            _ => return out,
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
        let sim = self.sim.as_ref()?;
        let s = sim.snapshot();
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
        let sim = self.sim.as_ref()?;
        let s = sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let (rect, _) = self.view_rect(sim.view());
        let ofs = [rect[0] as i16, rect[1] as i16];
        let (atlas, sky, glows, glows_sub) = self.layers(atlas, w, h, vp, ofs)?;
        let weather: Vec<_> = s.weather.iter().map(|c| c.offset(ofs[0], ofs[1])).collect();
        Some((fe::FeFrame::build_in(atlas, &weather, vram, screen, w, h), sky, glows, glows_sub))
    }

    /// The HUD over the whole window (split screen: after the views), added to `atlas`.
    pub fn hud_2d(&self, atlas: crate::assets::Atlas, w: f32, h: f32) -> Option<fe::FeFrame> {
        let s = self.sim.as_ref()?.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let cmds = if s.paused { &[][..] } else { &s.overlay[..] };
        Some(fe::FeFrame::build_in(atlas, cmds, vram, screen, w, h))
    }

    /// The current view's sky layer and light glows for a `w` x `h` window, the view's 3D at
    /// `vp` there and its 2D at `ofs` on the PS1 screen.
    #[allow(clippy::type_complexity)]
    fn layers(&self, atlas: crate::assets::Atlas, w: f32, h: f32, vp: [f32; 4], ofs: [i16; 2]) -> Option<(crate::assets::Atlas, Vec<Vertex>, Vec<Vertex>, Vec<Vertex>)> {
        let sim = self.sim.as_ref()?;
        let s = sim.snapshot();
        let vram = s.vram.as_ref()?;
        let screen = (s.display[2].max(1), s.display[3].max(1));
        let moved = |cmds: &[crate::ps1::host::OverlayCmd]| -> Vec<crate::ps1::host::OverlayCmd> { cmds.iter().map(|c| c.offset(ofs[0], ofs[1])).collect() };
        let sky = fe::FeFrame::build_in(atlas, &moved(&s.sky_overlay), vram, screen, w, h);
        let mut atlas = sky.atlas;
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
        for g in &s.glows {
            let raw = Vec3::new(-(g.anchor[0] as f32), g.anchor[1] as f32, g.anchor[2] as f32) / 65536.0;
            // a car's light follows the drawn (interpolated) car
            let now_p = match g.car.and_then(|a| s.cars.iter().position(|c| c.addr == a)) {
                Some(k) if k < self.racers.len() => raw + (self.racers[k].car.pos - original::pose(&s.cars[k]).pos),
                _ => raw,
            };
            let (Some(orig_vp), Some((p_now, _))) = (orig, to_px(now, now_p)) else { continue };
            let Some((p_orig, _)) = to_px(orig_vp, raw) else { continue };
            let half = self.cam_pos + (now_p - self.cam_pos) * 0.5;
            let Some((_, depth)) = to_px(now, half) else { continue };
            // (a puff is reprojected afterwards: not cut at the original's screen edges)
            let f = fe::FeFrame::build_clipped(atlas, &moved(&g.cmds), vram, screen, w, h, !g.smoke);
            atlas = f.atlas;
            let d = p_now - p_orig;
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
        self.sim.as_ref().is_some_and(|s| s.snapshot().paused)
    }

    pub fn hud(&self, w: f32, h: f32) -> Vec<Vertex> {
        if let (Some(hud), Some(sim)) = (&self.ps1hud, &self.sim) {
            let s = sim.snapshot();
            let frame = ps1hud::HudFrame {
                dash: &s.dash,
                layout: &s.hud_layout,
                map: &s.map,
                blink: (s.sim_ticks >> 4) & 1 != 0,
                countdown: s.countdown,
                countdown_ticks: s.ticks.wrapping_sub(s.countdown_start) as i32,
                overlay: &s.overlay,
            };
            return hud.vertices(&frame, w, h);
        }
        let mut out = Vec::new();
        let s = (h / 240.0).max(1.0).floor();
        let p = &self.racers[0];
        let kmh = (p.car.speed().abs() * 3.6).round() as i32;
        let white = [1.0, 1.0, 1.0, 1.0];
        let yellow = [1.0, 0.85, 0.2, 1.0];
        let gear = if p.car.reversing { "R".to_string() } else { format!("{}", p.car.gear + 1) };
        self.font.text(&mut out, &format!("{kmh:3} KM/H"), w - 80.0 * s, h - 24.0 * s, s * 1.5, white);
        self.font.text(&mut out, &format!("GEAR {gear}  {:4.0} RPM", p.car.rpm), w - 120.0 * s, h - 36.0 * s, s, yellow);
        let fmt = |t: f32| format!("{}:{:05.2}", (t / 60.0) as u32, t % 60.0);
        let order = self.standings();
        let pos = order.iter().position(|&i| i == 0).unwrap_or(0) + 1;
        let lap = (p.laps_done + 1).clamp(1, self.laps);
        self.font.text(&mut out, &format!("POS {pos}/{}", self.racers.len()), 8.0 * s, 8.0 * s, s * 1.5, white);
        self.font.text(&mut out, &format!("LAP {lap}/{}", self.laps), 8.0 * s, 22.0 * s, s, white);
        self.font.text(&mut out, &format!("TIME {}", fmt(self.race_time)), 8.0 * s, 32.0 * s, s, white);
        if let Some(b) = p.best_lap {
            self.font.text(&mut out, &format!("BEST {}", fmt(b)), 8.0 * s, 42.0 * s, s, yellow);
        }
        match self.phase {
            Phase::Countdown(t) => {
                let label = format!("{}", t.ceil().max(1.0) as i32);
                self.font.text(&mut out, &label, w * 0.5 - 15.0 * s, h * 0.35, s * 5.0, yellow);
            }
            Phase::Racing if self.race_time < 1.0 => {
                self.font.text(&mut out, "GO", w * 0.5 - 30.0 * s, h * 0.35, s * 5.0, yellow);
            }
            Phase::Finished => {
                self.font.text(&mut out, "RACE FINISHED", w * 0.5 - 100.0 * s, h * 0.2, s * 2.5, yellow);
                for (k, &i) in order.iter().enumerate() {
                    let r = &self.racers[i];
                    let t = r.finish_time.map(fmt).unwrap_or_else(|| "--:--.--".into());
                    let col = if i == 0 { yellow } else { white };
                    self.font.text(&mut out, &format!("{}. {:6} {}", k + 1, r.name, t), w * 0.5 - 90.0 * s, h * 0.2 + (30.0 + k as f32 * 12.0) * s, s * 1.5, col);
                }
            }
            Phase::Racing | Phase::Intro => {}
        }
        out
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

