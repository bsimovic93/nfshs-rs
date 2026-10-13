//! Races driven by the recompiled original simulation (`crate::ps1::race`): the original
//! NFS4.EXE physics, AI and race logic run on their own thread, one 60 Hz frame at a time,
//! and the engine only renders the resulting car poses.

use super::{Input, RaceConfig};
use crate::assets::GameData;
use crate::ps1::race::{CarState, FrameInput, RaceSetup, RaceThread, Snapshot, pad};
use anyhow::Result;
use glam::{Mat3, Quat, Vec3};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI16, Ordering};

/// The game's camera modes, in the order Triangle cycles them (14 = intro fly-by).
const CAMERA_MODES: [i16; 3] = [0, 3, 6]; // bumper, close chase, far chase
/// Camera mode to switch to once the race starts; the last one the player chose.
static PREFERRED_CAMERA: AtomicI16 = AtomicI16::new(3);

const FRAME: f32 = 1.0 / 60.0;

pub struct OriginalSim {
    thread: RaceThread,
    accum: f32,
    /// The original main loop has ended (race over or an error): the last state stays frozen.
    pub stopped: bool,
    /// Recent car poses stamped with simulation time (s). Physics runs at 64 Hz but frames at
    /// 60 Hz, so every 15th frame advances two steps; rendering interpolates on this timeline.
    history: VecDeque<(f64, State)>,
    /// Time the rendered poses show, a little behind the newest simulation state.
    render_t: f64,
    /// Camera mode still to reach once the race starts (None when done).
    camera_target: Option<i16>,
    camera_wait: u32,
}

/// The game's camera for the player view, in engine world space.
#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    /// tan of half the horizontal field of view on the PS1's 320-pixel-wide screen.
    pub tan_half_h: f32,
    /// The view matrix's vertical scale: the game squashes the picture vertically by
    /// scaling the camera's up row (about 0.85).
    pub y_scale: f32,
}

#[derive(Clone)]
struct State {
    cars: Vec<Pose>,
    camera: Camera,
}

/// Pose of a simulated car in engine world space.
#[derive(Clone, Copy, Default)]
pub struct Pose {
    pub pos: Vec3,
    pub rot: Quat,
    pub forward: Vec3,
    pub up: Vec3,
    pub vel: Vec3,
    /// The wheels' turn (front, rear) and the front wheels' steering, radians.
    pub wheel_rot: [f32; 2],
    pub wheel_steer: f32,
}

impl OriginalSim {
    /// Power the original on: its front end first.
    pub fn start_main(data: &GameData) -> Result<Self> {
        let thread = RaceThread::start_main(data.root.clone())?;
        Ok(Self {
            thread,
            accum: 0.0,
            stopped: false,
            history: VecDeque::new(),
            render_t: 0.0,
            camera_target: Some(PREFERRED_CAMERA.load(Ordering::Relaxed)),
            camera_wait: 0,
        })
    }

    /// The original is in its front end.
    pub fn in_front_end(&self) -> bool {
        self.thread.last.front_end
    }

    /// Forget the last race (its poses and camera set-up) before the next one starts.
    pub fn reset_race(&mut self) {
        self.history.clear();
        self.render_t = 0.0;
        self.camera_target = Some(PREFERRED_CAMERA.load(Ordering::Relaxed));
        self.camera_wait = 0;
    }

    /// The front end's pad: d-pad, Cross accept, Triangle back, Circle, Square, Start.
    fn front_end_buttons(input: &Input) -> u16 {
        let mut b = 0;
        for (on, bit) in [
            (input.up, pad::UP),
            (input.down, pad::DOWN),
            (input.left, pad::LEFT),
            (input.right, pad::RIGHT),
            (input.accept, pad::CROSS),
            (input.back, pad::TRIANGLE),
            (input.circle, pad::CIRCLE),
            (input.square, pad::SQUARE),
            (input.start, pad::START),
            (input.select, pad::SELECT),
        ] {
            if on {
                b |= bit;
            }
        }
        b
    }

    pub fn start(data: &GameData, cfg: &RaceConfig) -> Result<Self> {
        let mut cars = vec![cfg.player_car.to_string()];
        cars.extend(cfg.opponents.iter().cloned());
        let setup = RaceSetup { track: cfg.track, variant: cfg.variant.to_string(), cars, laps: cfg.laps.max(1) as u32 };
        let thread = RaceThread::start(data.root.clone(), setup.game_setup()?)?;
        let mut sim = Self {
            thread,
            accum: 0.0,
            stopped: false,
            history: VecDeque::new(),
            render_t: 0.0,
            camera_target: Some(PREFERRED_CAMERA.load(Ordering::Relaxed)),
            camera_wait: 0,
        };
        sim.record();
        sim.render_t = sim.history.back().map_or(0.0, |h| h.0);
        Ok(sim)
    }

    fn record(&mut self) {
        let s = &self.thread.last;
        let t = s.sim_ticks as f64 / 64.0;
        let raw = |i: usize| v3([s.camera_rot[i * 3], s.camera_rot[i * 3 + 1], s.camera_rot[i * 3 + 2]]);
        let row = |i: usize| raw(i).normalize_or_zero();
        let y_scale = (raw(1).length() / raw(2).length().max(1e-6)).clamp(0.5, 1.5);
        let camera = Camera {
            pos: v3(s.camera_pos),
            forward: row(2),
            up: row(1),
            tan_half_h: (s.display[2].max(1) as f32 / 2.0) / s.projection.max(1) as f32,
            y_scale: if y_scale.is_finite() { y_scale } else { 1.0 },
        };
        let state = State { cars: s.cars.iter().map(pose).collect(), camera };
        match self.history.back_mut() {
            // no physics step this frame (start sequence): keep the newest state
            Some(h) if h.0 >= t => h.1 = state,
            _ => self.history.push_back((t, state)),
        }
        while self.history.len() > 8 {
            self.history.pop_front();
        }
    }

    /// Car poses and camera at the render time, interpolated between simulation steps.
    pub fn poses(&self) -> (Vec<Pose>, Option<Camera>) {
        let Some(newest) = self.history.back() else { return (Vec::new(), None) };
        let t = self.render_t;
        let (a, b, f) = match self.history.iter().position(|h| h.0 > t) {
            None => (&newest.1, &newest.1, 0.0),
            Some(0) => (&self.history[0].1, &self.history[0].1, 0.0),
            Some(i) => {
                let (a, b) = (&self.history[i - 1], &self.history[i]);
                (&a.1, &b.1, ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) as f32)
            }
        };
        let cars = a
            .cars
            .iter()
            .zip(&b.cars)
            .map(|(p, q)| Pose {
                pos: p.pos.lerp(q.pos, f),
                rot: p.rot.slerp(q.rot, f),
                forward: p.forward.lerp(q.forward, f).normalize_or(q.forward),
                up: p.up.lerp(q.up, f).normalize_or(q.up),
                vel: p.vel.lerp(q.vel, f),
                // (the turn wraps: the short way round, it moves less than half a turn a frame)
                wheel_rot: std::array::from_fn(|i| {
                    let d = (q.wheel_rot[i] - p.wheel_rot[i] + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    p.wheel_rot[i] + d * f
                }),
                wheel_steer: p.wheel_steer + (q.wheel_steer - p.wheel_steer) * f,
            })
            .collect();
        let (p, q) = (&a.camera, &b.camera);
        // a camera cut (new mode or intro shot) is not interpolated
        let cut = p.pos.distance(q.pos) > 3.0 || p.forward.dot(q.forward) < 0.9;
        let camera = if cut {
            *q
        } else {
            Camera {
                pos: p.pos.lerp(q.pos, f),
                forward: p.forward.lerp(q.forward, f).normalize_or(q.forward),
                up: p.up.lerp(q.up, f).normalize_or(q.up),
                tan_half_h: q.tan_half_h,
                y_scale: q.y_scale,
            }
        };
        (cars, Some(camera))
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.thread.last
    }

    /// PS1 controller buttons for the engine's input (the game's default layout:
    /// Cross accelerate, Square brake/reverse, Circle handbrake, d-pad steer).
    fn buttons(input: &Input, menu: bool) -> u16 {
        let mut b = 0;
        if input.start {
            b |= pad::START;
        }
        if menu {
            // the pause menu: d-pad and Cross
            for (on, bit) in [(input.up, pad::UP), (input.down, pad::DOWN), (input.left, pad::LEFT), (input.right, pad::RIGHT), (input.accept, pad::CROSS)] {
                if on {
                    b |= bit;
                }
            }
            return b;
        }
        if input.up || input.throttle_axis > 0.3 {
            b |= pad::CROSS;
        }
        if input.down || input.brake_axis > 0.3 {
            b |= pad::SQUARE;
        }
        if input.handbrake {
            b |= pad::CIRCLE;
        }
        if input.camera {
            b |= pad::TRIANGLE;
        }
        if input.left || input.steer_axis < -0.3 {
            b |= pad::LEFT;
        }
        if input.right || input.steer_axis > 0.3 {
            b |= pad::RIGHT;
        }
        b
    }

    /// The camera of the newest simulated frame (the one the original's 2D was built for).
    pub fn sim_camera(&self) -> Option<Camera> {
        self.history.back().map(|h| h.1.camera)
    }

    /// The 2D the original draws over the race (HUD, glares) is recorded (see
    /// `GameHost::hud_prims`).
    pub fn hud_recorded(&self) -> bool {
        self.thread.last.vram.is_some()
    }

    /// The original's main loop has ended (the race was quit from its menu).
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Run the 60 Hz frames that fall into `dt` seconds of real time.
    pub fn advance(&mut self, dt: f32, input: &Input) {
        if self.stopped {
            return;
        }
        self.accum = (self.accum + dt).min(FRAME * 6.0);
        let paused = self.thread.last.paused;
        let user_buttons = Self::buttons(input, paused);
        while self.accum >= FRAME {
            if self.thread.last.front_end {
                // the front end: its own buttons, no race bookkeeping
                if let Err(e) = self.thread.step(FrameInput { buttons: Self::front_end_buttons(input), want_ram: false }) {
                    log::warn!("original game ended: {e:#}");
                    self.stopped = true;
                    return;
                }
                self.accum -= FRAME;
                continue;
            }
            let mut buttons = user_buttons;
            let snap = &self.thread.last;
            let mode = snap.camera_mode;
            match self.camera_target {
                // the game starts every race in the bumper view: after the start, press
                // Triangle (like the player would) until the preferred view is up
                Some(target) if snap.countdown >= 4 && CAMERA_MODES.contains(&mode) => {
                    if mode == target {
                        self.camera_target = None;
                    } else if self.camera_wait == 0 {
                        buttons |= pad::TRIANGLE;
                        self.camera_wait = 10;
                    } else {
                        self.camera_wait -= 1;
                    }
                }
                Some(_) => {}
                None if CAMERA_MODES.contains(&mode) => PREFERRED_CAMERA.store(mode, Ordering::Relaxed),
                None => {}
            }
            if let Err(e) = self.thread.step(FrameInput { buttons, want_ram: false }) {
                log::warn!("original simulation ended: {e:#}");
                self.stopped = true;
                return;
            }
            self.record();
            self.accum -= FRAME;
        }
        // simulation time runs at real speed (16 steps of 1/64 s per 15 frames), so the render
        // clock just advances by dt, kept about two physics steps behind the newest state;
        // it is only corrected when it drifts beyond that step jitter
        let newest = self.history.back().map_or(0.0, |h| h.0);
        let target = newest - 2.0 / 64.0;
        self.render_t += dt as f64;
        let err = target - self.render_t;
        if err.abs() > 0.25 {
            self.render_t = target;
        } else if err.abs() > 1.5 / 64.0 {
            self.render_t += err * 0.05;
        }
        self.render_t = self.render_t.min(newest);
    }
}

/// PS1 world space is the engine's with X mirrored; fixed point is 16.16.
pub fn v3(v: [i32; 3]) -> Vec3 {
    Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0
}

pub fn pose(c: &CarState) -> Pose {
    let row = |i: usize| v3([c.orient[i * 3], c.orient[i * 3 + 1], c.orient[i * 3 + 2]]);
    let (right, up, forward) = (row(0), row(1), row(2));
    // model space: +Y up, +Z forward and (mirrored) +X to the car's left
    let m = Mat3::from_cols(-right, up, forward);
    let rot = Quat::from_mat3(&orthonormal(m));
    // the car models' origin is at ground level, the simulation's position is the centre of
    // the body box (dimension.y above the wheels' contact points)
    let lift = c.dimension[1] as f32 / 65536.0;
    let up_n = up.normalize_or(Vec3::Y);
    let angle = |a: i32| a as f32 * (std::f32::consts::TAU / 65536.0);
    Pose {
        pos: v3(c.position) - up_n * lift,
        rot,
        forward: forward.normalize_or(Vec3::Z),
        up: up.normalize_or(Vec3::Y),
        vel: v3(c.velocity),
        wheel_rot: c.wheel_rot.map(|r| angle(r as i32)),
        wheel_steer: angle(c.wheel_steer),
    }
}

/// Gram-Schmidt on the fixed-point matrix so it is a proper rotation.
fn orthonormal(m: Mat3) -> Mat3 {
    let z = m.z_axis.normalize_or(Vec3::Z);
    let y = (m.y_axis - z * m.y_axis.dot(z)).normalize_or(Vec3::Y);
    let x = y.cross(z);
    Mat3::from_cols(x, y, z)
}
