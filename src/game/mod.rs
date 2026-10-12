//! Gameplay: a circuit race between the player and AI opponents.

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
use glam::{Mat4, Quat, Vec3};

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
    /// Analog controls (gamepad); override the digital ones when non-zero.
    pub steer_axis: f32,
    pub throttle_axis: f32,
    pub brake_axis: f32,
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
}

pub struct Game {
    pub track: TrackAssets,
    pub models: Vec<CarModel>,
    pub world: CollisionWorld,
    pub racers: Vec<Racer>,
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
    /// The chunk build list, instead of the simulation's (for comparisons with captures).
    pub chunks_override: Option<Vec<(i16, u16)>>,
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
        // the original game's simulation drives the race unless disabled (NFSHS_OWN_PHYSICS=1)
        // or it cannot run this setup
        let sim = if !cfg.original_sim || std::env::var_os("NFSHS_OWN_PHYSICS").is_some() {
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
        for (k, code) in codes.iter().enumerate() {
            let night = matches!(cfg.variant, "N" | "S");
            // the number plate shows the player's name (GameSetup carInfo + 0x68)
            let plate = crate::ps1::race::player_plate();
            let model = assets::load_car(data, atlas, code, k == 0, night, (k == 0).then_some(plate.as_str()))?;
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
        let shadow_tex = data.read("ZSFX.PSH").ok().and_then(|d| crate::formats::psh::Psh::parse(&d).ok()).and_then(|p| {
            let s = p.shapes.into_iter().find(|s| s.name == "SHAD")?;
            let r = atlas.add(s.width as u32, s.height as u32, &s.rgba);
            let (w, h) = (s.width as f32 - 0.5, s.height as f32 - 0.5);
            let uv = |x: f32, y: f32| atlas.uv(r, glam::Vec2::new(x, y));
            Some([uv(0.5, 0.5), uv(w, 0.5), uv(0.5, h), uv(w, h)])
        });
        let mut game = Self {
            cam_pos: player.pos - player.forward() * 7.0 + Vec3::Y * 2.5,
            cam_target: player.pos,
            track,
            models,
            world,
            racers,
            path,
            walls,
            laps: cfg.laps,
            phase: Phase::Countdown(3.0),
            race_time: 0.0,
            cam_mode: CameraMode::Chase,
            car_facets_override: None,
            chunks_override: None,
            shadow_tex,
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
            ambient: match cfg.variant {
                // linear-light multipliers (output is sRGB)
                "N" => 0.1,
                "W" => 0.65,
                _ => 1.0,
            },
            night: cfg.variant == "N",
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
    fn sync_original(&mut self, frame_dt: f32) {
        let Some(sim) = &self.sim else { return };
        let snap = sim.snapshot();
        let (poses, camera) = sim.poses();
        if let Some(cam) = camera {
            self.cam_pos = cam.pos;
            self.cam_target = cam.pos + cam.forward;
            self.cam_up = cam.up;
            // keep the original horizontal field of view on a 4:3 screen
            self.tan_half_v = cam.tan_half_h * 0.75;
            self.y_scale = cam.y_scale;
            // the bumper view does not show the player's car
            self.cam_mode = match self.racers.first() {
                Some(r) if r.car.pos.distance(cam.pos) < 2.0 => CameraMode::Bumper,
                _ => CameraMode::Chase,
            };
        }
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
        }
        self.phase = match (snap.countdown, self.phase) {
            (_, Phase::Finished) => Phase::Finished,
            (0, _) => Phase::Intro,
            (s @ 1..=3, _) => Phase::Countdown((4 - s) as f32),
            _ => Phase::Racing,
        };
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
                let mut r = Vec::new();
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
            shadows: self.shadows(),
            subtract_late: self.car_reflections().1,
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
            depth_cue: self.sim.as_ref().map(|s| {
                // the GTE far colour / depth-cue values the game set for this view
                let e = s.snapshot().view_env;
                crate::render::DepthCue {
                    far: Vec3::new(e[0] as f32, e[1] as f32, e[2] as f32) / 16.0,
                    dqa: e[3] as f32,
                    dqb: e[4] as f32,
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
        for (i, r) in self.racers.iter().enumerate() {
            if i == 0 && self.cam_mode == CameraMode::Bumper {
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
            for (pi, (part, &on)) in m.parts.iter().zip(visible).enumerate() {
                if on == 0 {
                    continue;
                }
                let tm = match part.wheel {
                    Some(w) => {
                        let steer = if w < 2 { -r.car.steer } else { 0.0 };
                        let spin = Quat::from_rotation_x(r.car.wheel_spin);
                        model * Mat4::from_rotation_translation(Quat::from_rotation_y(steer) * spin, part.hub)
                    }
                    None => model,
                };
                // parts sorted in front of their surroundings (negative bias) are drawn a little
                // towards the camera: the original orders by depth, not a depth buffer
                let pull = (-(assets::CAR_PART_BIAS.get(pi).copied().unwrap_or(0) as f32)).max(0.0) * 0.003;
                for v in &part.verts {
                    let c = v.color;
                    let p = tm.transform_point3(Vec3::from(v.pos));
                    let p = p + (self.cam_pos - p).normalize_or_zero() * pull;
                    out.push(Vertex { pos: p.into(), color: [c[0] * tint, c[1] * tint, c[2] * tint, c[3]], ..*v });
                }
            }
        }
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
            flare::build(&listed, &view, self.white_semi, &mut out);
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
            if i == 0 && self.cam_mode == CameraMode::Bumper {
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
                Vec3::new(r.car.pos.x + q.x, q.y.max(ground) + 0.03, r.car.pos.z + q.z)
            });
            for k in [0, 1, 3, 0, 3, 2] {
                out.push(Vertex { pos: p[k].into(), uv: uv[k], color: [c[0], c[1], c[2], 1.0] });
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
