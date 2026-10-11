//! Vehicle dynamics: planar bicycle model with slip-angle tyre forces, an automatic
//! gearbox, and ground following against the track collision mesh.

use super::collision::CollisionWorld;
use glam::{Quat, Vec3};

#[derive(Clone, Debug)]
pub struct CarSpec {
    pub mass: f32,
    /// Distance from CG to front / rear axle (m).
    pub cg_front: f32,
    pub cg_rear: f32,
    pub half_width: f32,
    pub wheel_radius: f32,
    pub redline: f32,
    pub idle: f32,
    /// Road speed (m/s) at redline in each forward gear (from the car's .QDA file).
    pub gear_speeds: Vec<f32>,
    /// Net full-throttle acceleration (m/s^2) sampled every 1 m/s of road speed (from .QDA).
    pub accel_curve: Vec<f32>,
    pub drag: f32,
    pub grip: f32,
    pub cornering_stiffness: f32,
    pub max_steer: f32,
    pub brake: f32,
}

impl Default for CarSpec {
    fn default() -> Self {
        Self {
            mass: 1500.0,
            cg_front: 1.4,
            cg_rear: 1.5,
            half_width: 0.9,
            wheel_radius: 0.33,
            redline: 7000.0,
            idle: 900.0,
            gear_speeds: vec![17.7, 29.5, 44.9, 66.7, 74.5, 89.7],
            accel_curve: (0..70).map(|i| 5.5 * (1.0 - i as f32 / 70.0)).collect(),
            drag: 0.42,
            grip: 1.25,
            cornering_stiffness: 9.0,
            max_steer: 0.55,
            brake: 1.3,
        }
    }
}

impl CarSpec {
    /// Full-throttle net acceleration at road speed `v` (m/s), linearly interpolated.
    pub fn accel_at(&self, v: f32) -> f32 {
        let i = (v.max(0.0).floor() as usize).min(self.accel_curve.len().saturating_sub(1));
        let t = (v - i as f32).clamp(0.0, 1.0);
        let a = self.accel_curve.get(i).copied().unwrap_or(0.0);
        let b = self.accel_curve.get(i + 1).copied().unwrap_or(0.0);
        a + (b - a) * t
    }

    pub fn top_speed(&self) -> f32 {
        self.accel_curve.iter().position(|&a| a <= 0.0).unwrap_or(self.accel_curve.len()) as f32
    }

    /// Engine data from a decompressed `.QDA` file (276 bytes):
    /// `+0x00` redline (16.16 rpm), `+0x04..0x1C` m/s at redline per gear (16.16, 0 = unused),
    /// `+0x20` i16[112] net full-throttle acceleration in 1/256 m/s^2, one entry per m/s of speed.
    pub fn apply_qda(&mut self, d: &[u8]) {
        if d.len() < 0x100 {
            return;
        }
        let fx = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as f32 / 65536.0;
        self.redline = fx(0);
        self.gear_speeds = (1..8).map(|i| fx(i * 4)).take_while(|&v| v > 0.0).collect();
        let raw: Vec<i16> = (0..112).map(|i| i16::from_le_bytes([d[0x20 + i * 2], d[0x21 + i * 2]])).collect();
        let n = raw.iter().rposition(|&v| v > 0).map_or(0, |p| p + 2).min(raw.len());
        self.accel_curve = raw[..n].iter().map(|&v| v.max(0) as f32 / 256.0).collect();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
    pub reverse: bool,
}

pub struct Car {
    pub spec: CarSpec,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub yaw_rate: f32,
    pub up: Vec3,
    pub on_ground: bool,
    pub gear: usize,
    pub rpm: f32,
    pub steer: f32,
    pub wheel_spin: f32,
    pub reversing: bool,
}

const G: f32 = 9.81;

impl Car {
    pub fn new(spec: CarSpec, pos: Vec3, yaw: f32) -> Self {
        Self {
            spec,
            pos,
            vel: Vec3::ZERO,
            yaw,
            yaw_rate: 0.0,
            up: Vec3::Y,
            on_ground: true,
            gear: 0,
            rpm: 900.0,
            steer: 0.0,
            wheel_spin: 0.0,
            reversing: false,
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Right-hand vector (yaw increases counter-clockwise seen from above, i.e. to the left).
    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        Vec3::new(-f.z, 0.0, f.x)
    }

    pub fn speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }

    pub fn orientation(&self) -> Quat {
        let f = self.forward();
        let up = self.up.normalize();
        let fwd = (f - up * f.dot(up)).normalize_or(f);
        let right = fwd.cross(up).normalize();
        let x = -right; // model space +X is the car's left
        Quat::from_mat3(&glam::Mat3::from_cols(x, up, fwd))
    }

    pub fn reset(&mut self, pos: Vec3, yaw: f32) {
        self.pos = pos + Vec3::Y * 0.5;
        self.yaw = yaw;
        self.vel = Vec3::ZERO;
        self.yaw_rate = 0.0;
        self.gear = 0;
        self.reversing = false;
    }

    pub fn step(&mut self, c: Controls, dt: f32, world: &CollisionWorld) {
        let s = self.spec.clone();
        // steering eases in and reduces with speed
        let speed_abs = self.vel.length();
        let steer_limit = s.max_steer / (1.0 + speed_abs * 0.035);
        let target = c.steer * steer_limit;
        self.steer += (target - self.steer) * (dt * 8.0).min(1.0);

        let f = self.forward();
        let r = self.right();
        let vf = self.vel.dot(f);
        let vr = self.vel.dot(r);

        // drive direction (reverse when stopped and holding brake)
        if c.reverse && vf < 1.0 {
            self.reversing = true;
        } else if c.throttle > 0.0 && vf > -1.0 {
            self.reversing = false;
        }

        let mut force = Vec3::ZERO;
        let mut torque = 0.0;

        if self.on_ground {
            // gear + rpm are derived from road speed (display / sound only)
            let speed = vf.abs();
            if self.reversing {
                self.gear = 0;
            } else {
                let mut g = 0;
                while g + 1 < s.gear_speeds.len() && speed > s.gear_speeds[g] * 0.92 {
                    g += 1;
                }
                // hysteresis on downshifts
                if g < self.gear && speed > s.gear_speeds[g] * 0.8 {
                    g = self.gear;
                }
                self.gear = g;
            }
            let gear_speed = s.gear_speeds[self.gear];
            self.rpm = (speed / gear_speed * s.redline).clamp(s.idle, s.redline);
            let throttle = if self.reversing { c.reverse as u8 as f32 } else { c.throttle };
            // the table is net of aerodynamic drag, so add drag back to get tractive force
            let tractive = s.accel_at(speed) * s.mass + s.drag * speed * speed;
            let mut drive = throttle * if self.reversing { tractive.min(s.mass * 3.0) } else { tractive };
            if self.reversing {
                drive = -drive;
            }
            let normal_load = s.mass * G;
            drive = drive.clamp(-normal_load * s.grip, normal_load * s.grip);
            force += f * drive;

            // braking
            let brake_input = if self.reversing { c.throttle } else { c.brake };
            if brake_input > 0.0 && vf.abs() > 0.1 {
                force -= f * vf.signum() * brake_input * s.brake * normal_load;
            }

            // lateral tyre forces (bicycle model)
            let a = s.cg_front;
            let b = s.cg_rear;
            let wb = a + b;
            let load_f = normal_load * b / wb;
            let load_r = normal_load * a / wb;
            let vlong = vf.abs().max(0.5);
            // lateral velocity at an axle: v_r -/+ yaw_rate * distance (positive yaw = turning left)
            let slip_f = (vr - self.yaw_rate * a).atan2(vlong) - self.steer * vf.signum();
            let slip_r = (vr + self.yaw_rate * b).atan2(vlong);
            let rear_grip = if c.handbrake { 0.35 } else { 1.0 };
            let ff = (-s.cornering_stiffness * slip_f).clamp(-1.0, 1.0) * load_f * s.grip;
            let fr = (-s.cornering_stiffness * slip_r).clamp(-1.0, 1.0) * load_r * s.grip * rear_grip;
            force += r * (ff * self.steer.cos() + fr);
            force -= f * ff * self.steer.sin();
            torque += -ff * self.steer.cos() * a + fr * b;
            if c.handbrake && vf.abs() > 0.1 {
                force -= f * vf.signum() * 0.5 * load_r;
            }
            // rolling resistance
            force -= self.vel * if c.throttle > 0.0 { 0.0 } else { 15.0 };
        }
        // aerodynamic drag
        force -= self.vel * self.vel.length() * s.drag;

        let inertia = s.mass * (s.cg_front * s.cg_front + s.cg_rear * s.cg_rear) * 0.5;
        let accel = force / s.mass;
        self.vel += accel * dt;
        self.vel.y -= G * dt;
        self.yaw_rate += torque / inertia * dt;
        if self.on_ground && vf.abs() < 0.5 && c.throttle == 0.0 && !c.reverse {
            // settle when nearly stopped
            self.vel.x *= 0.9;
            self.vel.z *= 0.9;
            self.yaw_rate *= 0.8;
        }
        if !self.on_ground {
            self.yaw_rate *= 0.995;
        }
        self.yaw += self.yaw_rate * dt;
        self.pos += self.vel * dt;
        self.wheel_spin += vf / s.wheel_radius * dt;

        self.collide(world, dt);
    }

    fn collide(&mut self, world: &CollisionWorld, dt: f32) {
        // ground: sample under the centre and both axles, take the highest
        let f = self.forward();
        let probes = [self.pos, self.pos + f * self.spec.cg_front, self.pos - f * self.spec.cg_rear];
        let mut best: Option<(f32, Vec3)> = None;
        let mut n_acc = Vec3::ZERO;
        for p in probes {
            if let Some(h) = world.ground(p, 1.2) {
                n_acc += h.normal;
                if best.is_none_or(|b| h.height > b.0) {
                    best = Some((h.height, h.normal));
                }
            }
        }
        self.on_ground = false;
        if let Some((h, _)) = best {
            let n = n_acc.normalize_or(Vec3::Y);
            if self.pos.y <= h + 0.05 {
                self.pos.y = h;
                let into = self.vel.dot(n);
                if into < 0.0 {
                    self.vel -= n * into;
                }
                self.on_ground = true;
            }
            self.up = self.up.lerp(if self.on_ground { n } else { Vec3::Y }, (dt * 10.0).min(1.0)).normalize();
        }
        // walls: two circles along the body
        let radius = self.spec.half_width;
        for k in [-1.0f32, 0.0, 1.0] {
            let c = self.pos + f * (k * self.spec.cg_rear * 0.9) + Vec3::Y * 0.5;
            if let Some((push, n)) = world.walls(c, radius, self.pos.y + 0.2, self.pos.y + 1.2) {
                self.pos += push;
                let into = self.vel.dot(n);
                if into < 0.0 {
                    self.vel -= n * into * 1.3;
                    self.vel *= 0.97;
                    self.yaw_rate *= 0.7;
                }
            }
        }
    }
}
