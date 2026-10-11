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
    pub max_torque: f32,
    pub redline: f32,
    pub idle: f32,
    pub gears: Vec<f32>,
    pub final_drive: f32,
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
            max_torque: 500.0,
            redline: 7000.0,
            idle: 900.0,
            gears: vec![3.5, 2.3, 1.6, 1.2, 0.95, 0.78],
            final_drive: 3.4,
            drag: 0.42,
            grip: 1.25,
            cornering_stiffness: 9.0,
            max_steer: 0.55,
            brake: 1.3,
        }
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
            let wheel_rpm = vf.abs() / s.wheel_radius * 60.0 / std::f32::consts::TAU;
            let ratio = if self.reversing { 3.2 } else { s.gears[self.gear] } * s.final_drive;
            self.rpm = (wheel_rpm * ratio).max(s.idle);
            // automatic gearbox
            if !self.reversing {
                if self.rpm > s.redline * 0.95 && self.gear + 1 < s.gears.len() {
                    self.gear += 1;
                } else if self.gear > 0 {
                    let lower = wheel_rpm * s.gears[self.gear - 1] * s.final_drive;
                    if lower < s.redline * 0.6 {
                        self.gear -= 1;
                    }
                }
            }
            self.rpm = self.rpm.min(s.redline);
            // flat-ish torque curve peaking at 60% of redline
            let x = self.rpm / s.redline;
            let torque_curve = (0.55 + 1.2 * x - 1.0 * x * x).clamp(0.3, 1.0);
            let throttle = if self.reversing { c.reverse as u8 as f32 } else { c.throttle };
            let mut drive = throttle * s.max_torque * torque_curve * ratio * 0.85 / s.wheel_radius;
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
            force -= self.vel * 12.0;
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
