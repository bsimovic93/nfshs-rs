//! Uniform-grid spatial index over track triangles for ground and wall queries.

use crate::assets::CollTri;
use glam::{Vec2, Vec3};
use std::collections::HashMap;

const CELL: f32 = 8.0;

pub struct CollisionWorld {
    pub tris: Vec<CollTri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

pub struct GroundHit {
    pub height: f32,
    pub normal: Vec3,
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

impl CollisionWorld {
    pub fn new(tris: Vec<CollTri>) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, t) in tris.iter().enumerate() {
            let min = t.p.iter().fold(Vec2::splat(f32::MAX), |m, p| m.min(Vec2::new(p.x, p.z)));
            let max = t.p.iter().fold(Vec2::splat(f32::MIN), |m, p| m.max(Vec2::new(p.x, p.z)));
            let (a, b) = (cell(min), cell(max));
            if (b.0 - a.0) * (b.1 - a.1) > 4096 {
                continue; // degenerate giant triangle
            }
            for x in a.0..=b.0 {
                for z in a.1..=b.1 {
                    grid.entry((x, z)).or_default().push(i as u32);
                }
            }
        }
        Self { tris, grid }
    }

    fn near(&self, p: Vec2) -> impl Iterator<Item = &CollTri> {
        self.grid.get(&cell(p)).into_iter().flatten().map(|&i| &self.tris[i as usize])
    }

    /// Highest walkable surface below `pos.y + step`.
    pub fn ground(&self, pos: Vec3, step: f32) -> Option<GroundHit> {
        let p2 = Vec2::new(pos.x, pos.z);
        let mut best: Option<GroundHit> = None;
        for t in self.near(p2) {
            let n = if t.n.y < 0.0 { -t.n } else { t.n };
            if n.y < 0.35 {
                continue;
            }
            // barycentric test in XZ
            let (a, b, c) = (Vec2::new(t.p[0].x, t.p[0].z), Vec2::new(t.p[1].x, t.p[1].z), Vec2::new(t.p[2].x, t.p[2].z));
            let d = (b - a).perp_dot(c - a);
            if d.abs() < 1e-9 {
                continue;
            }
            let u = (p2 - a).perp_dot(c - a) / d;
            let v = (b - a).perp_dot(p2 - a) / d;
            if u < -1e-4 || v < -1e-4 || u + v > 1.0001 {
                continue;
            }
            let h = t.p[0].y + u * (t.p[1].y - t.p[0].y) + v * (t.p[2].y - t.p[0].y);
            if h <= pos.y + step && best.as_ref().is_none_or(|b| h > b.height) {
                best = Some(GroundHit { height: h, normal: n });
            }
        }
        best
    }

    /// Push a vertical capsule-ish circle out of steep triangles. Returns (correction, wall normal).
    pub fn walls(&self, pos: Vec3, radius: f32, y_lo: f32, y_hi: f32) -> Option<(Vec3, Vec3)> {
        let p2 = Vec2::new(pos.x, pos.z);
        let mut total = Vec3::ZERO;
        let mut normal = Vec3::ZERO;
        for t in self.near(p2) {
            if t.n.y.abs() > 0.5 {
                continue;
            }
            // triangle must span the car's vertical extent
            let tmin = t.p.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let tmax = t.p.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            if tmax < y_lo || tmin > y_hi {
                continue;
            }
            let probe = Vec3::new(pos.x, pos.y.clamp(tmin, tmax), pos.z);
            let q = closest_point_on_triangle(probe, &t.p);
            let mut d = probe - q;
            d.y = 0.0;
            let dist = d.length();
            if dist < radius && dist > 1e-5 {
                let n = d / dist;
                total += n * (radius - dist);
                normal += n;
            }
        }
        (total.length_squared() > 0.0).then(|| (total, normal.normalize_or_zero()))
    }
}

fn closest_point_on_triangle(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}
