//! Gameplay: a single-player time-trial session on a track with one car.

pub mod car;
pub mod collision;
pub mod hud;

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
}

#[derive(Clone, Copy, PartialEq)]
pub enum CameraMode {
    Chase,
    Far,
    Bumper,
}

pub struct Game {
    pub track: TrackAssets,
    pub car_model: CarModel,
    pub world: CollisionWorld,
    pub car: Car,
    /// Virtual-road centre line (world space) used for progress, resets and start grid.
    pub path: Vec<Vec3>,
    pub progress: usize,
    pub lap: u32,
    pub lap_time: f32,
    pub best_lap: Option<f32>,
    pub cam_pos: Vec3,
    pub cam_target: Vec3,
    pub cam_mode: CameraMode,
    prev_camera_key: bool,
    accum: f32,
    font: hud::Font,
    pub fog: (Vec3, f32, f32),
}

const DT: f32 = 1.0 / 120.0;

impl Game {
    pub fn load(data: &GameData, atlas: &mut Atlas, track_no: u32, variant: &str, car_code: &str) -> Result<Self> {
        let track = assets::load_track(data, atlas, track_no, variant)?;
        let car_model = assets::load_car(data, atlas, &format!("ZZZ{car_code}.VIV"))?;
        let font = hud::Font::new(atlas);
        let world = CollisionWorld::new(track.tris.clone());
        let mut path: Vec<Vec3> = track.track.vroad.iter().map(|v| assets::ps1_to_world(v.pos)).collect();
        if path.len() < 2 {
            path = track.nodes.clone();
        }
        let spec = CarSpec { wheel_radius: car_model.wheel_radius, half_width: car_model.half_extents.x * 0.9, ..Default::default() };
        let (pos, yaw) = Self::spawn_point(&path, 0);
        let mut car = Car::new(spec, pos, yaw);
        if let Some(h) = world.ground(pos + Vec3::Y * 5.0, 10.0) {
            car.pos.y = h.height;
        }
        let f = car.forward();
        let fog = match variant {
            "N" => (Vec3::new(0.02, 0.02, 0.06), 60.0, 260.0),
            "S" => (Vec3::new(0.75, 0.78, 0.82), 40.0, 220.0),
            "W" => (Vec3::new(0.45, 0.48, 0.52), 50.0, 260.0),
            _ => (Vec3::new(0.62, 0.72, 0.85), 120.0, 420.0),
        };
        Ok(Self {
            cam_pos: car.pos - f * 7.0 + Vec3::Y * 2.5,
            cam_target: car.pos,
            track,
            car_model,
            world,
            car,
            path,
            progress: 0,
            lap: 1,
            lap_time: 0.0,
            best_lap: None,
            cam_mode: CameraMode::Chase,
            prev_camera_key: false,
            accum: 0.0,
            font,
            fog,
        })
    }

    fn spawn_point(path: &[Vec3], i: usize) -> (Vec3, f32) {
        let n = path.len();
        let a = path[i % n];
        let b = path[(i + 2) % n];
        let d = b - a;
        (a, d.x.atan2(d.z))
    }

    fn nearest_path(&self, from: usize, window: usize) -> usize {
        let n = self.path.len();
        let mut best = (f32::MAX, from);
        for k in 0..window * 2 {
            let i = (from + n + k - window) % n;
            let d = self.path[i].distance_squared(self.car.pos);
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }

    pub fn update(&mut self, frame_dt: f32, input: Input) {
        if input.camera && !self.prev_camera_key {
            self.cam_mode = match self.cam_mode {
                CameraMode::Chase => CameraMode::Far,
                CameraMode::Far => CameraMode::Bumper,
                CameraMode::Bumper => CameraMode::Chase,
            };
        }
        self.prev_camera_key = input.camera;
        if input.reset {
            let (p, yaw) = Self::spawn_point(&self.path, self.progress);
            let ground = self.world.ground(p + Vec3::Y * 3.0, 6.0).map(|h| h.height).unwrap_or(p.y);
            self.car.reset(Vec3::new(p.x, ground, p.z), yaw);
        }
        let controls = Controls {
            throttle: input.up as u8 as f32,
            brake: input.down as u8 as f32,
            reverse: input.down,
            steer: input.right as u8 as f32 - input.left as u8 as f32,
            handbrake: input.handbrake,
        };
        self.accum = (self.accum + frame_dt).min(0.25);
        while self.accum >= DT {
            self.car.step(controls, DT, &self.world);
            self.accum -= DT;
        }
        // fell out of the world: put back on the road
        if self.car.pos.y < self.path[self.progress].y - 40.0 {
            let (p, yaw) = Self::spawn_point(&self.path, self.progress);
            self.car.reset(p, yaw);
        }
        // progress & laps
        let n = self.path.len();
        let new = self.nearest_path(self.progress, 24);
        if self.progress > n * 3 / 4 && new < n / 4 {
            self.lap += 1;
            self.best_lap = Some(self.best_lap.map_or(self.lap_time, |b: f32| b.min(self.lap_time)));
            self.lap_time = 0.0;
        } else if self.progress < n / 4 && new > n * 3 / 4 {
            self.lap = self.lap.saturating_sub(1).max(1);
        }
        self.progress = new;
        self.lap_time += frame_dt;
        self.update_camera(frame_dt);
    }

    fn update_camera(&mut self, dt: f32) {
        let f = self.car.forward();
        let (dist, height, look) = match self.cam_mode {
            CameraMode::Chase => (6.5, 2.0, 1.0),
            CameraMode::Far => (10.0, 3.5, 1.0),
            CameraMode::Bumper => (-0.4, 1.1, 1.0),
        };
        let desired = self.car.pos - f * dist + Vec3::Y * height;
        let target = self.car.pos + Vec3::Y * look + f * 4.0;
        if self.cam_mode == CameraMode::Bumper {
            self.cam_pos = desired;
            self.cam_target = self.car.pos + f * 20.0 + Vec3::Y * 1.0;
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
        let proj = Mat4::perspective_rh(1.05, aspect, 0.1, 2000.0);
        let view = Mat4::look_at_rh(self.cam_pos, self.cam_target, Vec3::Y);
        FrameParams {
            view_proj: proj * view,
            cam_pos: self.cam_pos,
            fog_color: self.fog.0,
            fog_start: self.fog.1,
            fog_end: self.fog.2,
        }
    }

    /// Car geometry transformed to world space.
    pub fn dynamic_geometry(&self) -> Vec<Vertex> {
        let mut out = Vec::new();
        let rot = self.car.orientation();
        let model = Mat4::from_rotation_translation(rot, self.car.pos);
        let skip_body = self.cam_mode == CameraMode::Bumper;
        if !skip_body {
            for v in &self.car_model.body {
                out.push(Vertex { pos: model.transform_point3(Vec3::from(v.pos)).into(), ..*v });
            }
        }
        for (i, (verts, hub)) in self.car_model.wheels.iter().enumerate() {
            if skip_body {
                break;
            }
            let steer = if i < 2 { -self.car.steer } else { 0.0 };
            let spin = Quat::from_rotation_x(self.car.wheel_spin);
            let wm = model * Mat4::from_rotation_translation(Quat::from_rotation_y(steer) * spin, *hub);
            for v in verts {
                out.push(Vertex { pos: wm.transform_point3(Vec3::from(v.pos)).into(), ..*v });
            }
        }
        out
    }

    pub fn hud(&self, w: f32, h: f32) -> Vec<Vertex> {
        let mut out = Vec::new();
        let s = (h / 240.0).max(1.0).floor();
        let kmh = (self.car.speed().abs() * 3.6).round() as i32;
        let white = [1.0, 1.0, 1.0, 1.0];
        let yellow = [1.0, 0.85, 0.2, 1.0];
        let gear = if self.car.reversing { "R".to_string() } else { format!("{}", self.car.gear + 1) };
        self.font.text(&mut out, &format!("{kmh:3} KM/H"), w - 80.0 * s, h - 24.0 * s, s * 1.5, white);
        self.font.text(&mut out, &format!("GEAR {gear}  {:4.0} RPM", self.car.rpm), w - 120.0 * s, h - 36.0 * s, s, yellow);
        let fmt = |t: f32| format!("{}:{:05.2}", (t / 60.0) as u32, t % 60.0);
        self.font.text(&mut out, &format!("LAP {}", self.lap), 8.0 * s, 8.0 * s, s * 1.5, white);
        self.font.text(&mut out, &format!("TIME {}", fmt(self.lap_time)), 8.0 * s, 22.0 * s, s, white);
        if let Some(b) = self.best_lap {
            self.font.text(&mut out, &format!("BEST {}", fmt(b)), 8.0 * s, 32.0 * s, s, yellow);
        }
        let pct = self.progress as f32 * 100.0 / self.path.len() as f32;
        self.font.text(&mut out, &format!("{pct:3.0}%"), w - 40.0 * s, 8.0 * s, s, white);
        out
    }
}
