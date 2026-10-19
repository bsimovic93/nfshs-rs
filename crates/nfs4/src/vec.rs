//! `coorddef` and `matrixtdef` by value: a vector and a 3x3 matrix of 16.16 fixed-point
//! numbers, read from and written to their views in PS1 RAM.

use crate::types::{Coord, Matrix};
use ps1rt::Mem;

/// A `coorddef` (1.0 = 0x10000).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Vec3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Vec3 {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

impl From<[i32; 3]> for Vec3 {
    fn from(v: [i32; 3]) -> Self {
        Self { x: v[0], y: v[1], z: v[2] }
    }
}

impl From<Vec3> for [i32; 3] {
    fn from(v: Vec3) -> Self {
        [v.x, v.y, v.z]
    }
}

/// A `matrixtdef`: nine 16.16 numbers, row by row.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mat3 {
    pub m: [i32; 9],
}

impl Coord {
    pub fn get(self, m: &mut Mem) -> Vec3 {
        Vec3 { x: self.x(m), y: self.y(m), z: self.z(m) }
    }
    pub fn set(self, m: &mut Mem, v: Vec3) {
        self.set_x(m, v.x);
        self.set_y(m, v.y);
        self.set_z(m, v.z);
    }
}

impl Matrix {
    pub fn get(self, m: &mut Mem) -> Mat3 {
        Mat3 { m: std::array::from_fn(|i| self.m(m, i as u32)) }
    }
    pub fn set(self, m: &mut Mem, v: &Mat3) {
        for (i, &e) in v.m.iter().enumerate() {
            self.set_m(m, i as u32, e);
        }
    }
}

impl Coord {
    /// The addresses of its components, for the original's functions that take `int *`s.
    pub fn x_addr(self) -> u32 {
        self.0
    }
    pub fn y_addr(self) -> u32 {
        self.0 + 4
    }
    pub fn z_addr(self) -> u32 {
        self.0 + 8
    }
}
