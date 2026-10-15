//! The test autopilot: drives the player's car along the virtual road's centre line through
//! the pad (`P` in the window, `a` in scripted runs).

use super::car::Car;
use glam::Vec3;

/// What the autopilot asks for: pedals 0..1, steering -1 (left) ..1.
#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
}

#[derive(Default)]
pub struct Driver {
    stuck_time: f32,
    recover_time: f32,
}

const LAT_ACCEL: f32 = 10.0;
const BRAKE_DECEL: f32 = 9.0;

impl Driver {
    fn heading(path: &[Vec3], i: usize) -> f32 {
        let n = path.len();
        let d = path[(i + 1) % n] - path[i % n];
        d.x.atan2(d.z)
    }

    /// Highest speed we can carry now and still make every corner within braking range.
    fn target_speed(path: &[Vec3], progress: usize, speed: f32) -> f32 {
        let n = path.len();
        let mut limit = f32::MAX;
        let brake_dist = speed * speed / (2.0 * BRAKE_DECEL) + 30.0;
        let mut dist = 0.0;
        let mut j = 1;
        while dist < brake_dist && j < 80 {
            let i = progress + j;
            let seg = path[(i + 1) % n].distance(path[i % n]).max(0.5);
            // curvature from heading change over a short window
            let span = 3;
            let mut dh = Self::heading(path, i + span) - Self::heading(path, i);
            while dh > std::f32::consts::PI {
                dh -= std::f32::consts::TAU;
            }
            while dh < -std::f32::consts::PI {
                dh += std::f32::consts::TAU;
            }
            let radius = (seg * span as f32) / dh.abs().max(1e-3);
            let v_corner = (LAT_ACCEL * radius).sqrt();
            let v_now = (v_corner * v_corner + 2.0 * BRAKE_DECEL * dist).sqrt();
            limit = limit.min(v_now);
            dist += seg;
            j += 1;
        }
        limit
    }

    pub fn controls(&mut self, car: &Car, path: &[Vec3], progress: usize, dt: f32) -> Controls {
        let n = path.len();
        let speed = car.speed();
        let ahead = 2 + (speed.max(0.0) / 8.0) as usize;
        let target = path[(progress + ahead) % n];
        let to = (target - car.pos).with_y(0.0).normalize_or_zero();
        let angle = to.dot(car.right()).atan2(to.dot(car.forward()));

        // stuck: back out for a moment, steering the other way
        if self.recover_time > 0.0 {
            self.recover_time -= dt;
            return Controls { throttle: 0.0, brake: 1.0, steer: -angle.signum(), handbrake: false };
        }
        if speed.abs() < 1.5 {
            self.stuck_time += dt;
            if self.stuck_time > 1.5 {
                self.stuck_time = 0.0;
                self.recover_time = 1.2;
            }
        } else {
            self.stuck_time = 0.0;
        }

        let steer = (angle * 2.2).clamp(-1.0, 1.0);
        let mut vmax = Self::target_speed(path, progress, speed.max(0.0));
        if angle.abs() > 0.6 {
            vmax = vmax.min(12.0);
        }
        let (throttle, brake) = if speed > vmax + 2.0 {
            (0.0, ((speed - vmax) / 6.0).clamp(0.3, 1.0))
        } else if speed > vmax {
            (0.0, 0.0)
        } else {
            (1.0, 0.0)
        };
        Controls { throttle, brake, steer, handbrake: false }
    }
}
