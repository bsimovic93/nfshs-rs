//! A car as drawn: its pose as the original simulation leaves it (interpolated).

use glam::{Quat, Vec3};

#[derive(Clone, Copy)]
pub struct Car {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Heading in radians (0 = +Z).
    pub yaw: f32,
    pub up: Vec3,
    pub rot: Quat,
    /// The wheels' turn as the original draws them (front, rear) and the front wheels'
    /// steering, radians.
    pub wheel_pose: ([f32; 2], f32),
}

impl Default for Car {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, vel: Vec3::ZERO, yaw: 0.0, up: Vec3::Y, rot: Quat::IDENTITY, wheel_pose: ([0.0; 2], 0.0) }
    }
}

impl Car {
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
}
