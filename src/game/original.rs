//! Races driven by the recompiled original simulation (`crate::ps1::race`): the original
//! NFS4.EXE physics, AI and race logic run on their own thread, one 60 Hz frame at a time,
//! and the engine only renders the resulting car poses.

use super::{Input, RaceConfig};
use crate::assets::GameData;
use crate::ps1::race::{CarState, FrameInput, RaceSetup, RaceThread, Snapshot, pad};
use anyhow::Result;
use glam::{Mat3, Quat, Vec3};

const FRAME: f32 = 1.0 / 60.0;

pub struct OriginalSim {
    thread: RaceThread,
    accum: f32,
    /// The original main loop has ended (race over or an error): the last state stays frozen.
    pub stopped: bool,
}

/// Pose of a simulated car in engine world space.
pub struct Pose {
    pub pos: Vec3,
    pub rot: Quat,
    pub forward: Vec3,
    pub up: Vec3,
    pub vel: Vec3,
}

impl OriginalSim {
    pub fn start(data: &GameData, cfg: &RaceConfig) -> Result<Self> {
        let mut cars = vec![cfg.player_car.to_string()];
        cars.extend(cfg.opponents.iter().cloned());
        let setup = RaceSetup { track: cfg.track, variant: cfg.variant.to_string(), cars, laps: cfg.laps.max(1) as u32 };
        let thread = RaceThread::start(data.root.clone(), setup.game_setup()?)?;
        Ok(Self { thread, accum: 0.0, stopped: false })
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.thread.last
    }

    /// PS1 controller buttons for the engine's input (the game's default layout:
    /// Cross accelerate, Square brake/reverse, Circle handbrake, d-pad steer).
    fn buttons(input: &Input) -> u16 {
        let mut b = 0;
        if input.up || input.throttle_axis > 0.3 {
            b |= pad::CROSS;
        }
        if input.down || input.brake_axis > 0.3 {
            b |= pad::SQUARE;
        }
        if input.handbrake {
            b |= pad::CIRCLE;
        }
        if input.left || input.steer_axis < -0.3 {
            b |= pad::LEFT;
        }
        if input.right || input.steer_axis > 0.3 {
            b |= pad::RIGHT;
        }
        b
    }

    /// Run the 60 Hz frames that fall into `dt` seconds of real time.
    pub fn advance(&mut self, dt: f32, input: &Input) {
        if self.stopped {
            return;
        }
        self.accum = (self.accum + dt).min(FRAME * 6.0);
        let buttons = Self::buttons(input);
        while self.accum >= FRAME {
            if let Err(e) = self.thread.step(FrameInput { buttons, want_ram: false }) {
                log::warn!("original simulation ended: {e:#}");
                self.stopped = true;
                return;
            }
            self.accum -= FRAME;
        }
    }
}

/// PS1 world space is the engine's with X mirrored; fixed point is 16.16.
fn v3(v: [i32; 3]) -> Vec3 {
    Vec3::new(-(v[0] as f32), v[1] as f32, v[2] as f32) / 65536.0
}

pub fn pose(c: &CarState) -> Pose {
    let row = |i: usize| v3([c.orient[i * 3], c.orient[i * 3 + 1], c.orient[i * 3 + 2]]);
    let (right, up, forward) = (row(0), row(1), row(2));
    // model space: +Y up, +Z forward and (mirrored) +X to the car's left
    let m = Mat3::from_cols(-right, up, forward);
    let rot = Quat::from_mat3(&orthonormal(m));
    Pose { pos: v3(c.position), rot, forward: forward.normalize_or(Vec3::Z), up: up.normalize_or(Vec3::Y), vel: v3(c.velocity) }
}

/// Gram-Schmidt on the fixed-point matrix so it is a proper rotation.
fn orthonormal(m: Mat3) -> Mat3 {
    let z = m.z_axis.normalize_or(Vec3::Z);
    let y = (m.y_axis - z * m.y_axis.dot(z)).normalize_or(Vec3::Y);
    let x = y.cross(z);
    Mat3::from_cols(x, y, z)
}
