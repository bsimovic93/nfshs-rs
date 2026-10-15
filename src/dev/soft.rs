//! Tiny software rasterizer used to produce preview PNGs of decoded assets (debug tooling).

use glam::{Mat4, Vec2, Vec3, Vec4};

pub struct Tri {
    pub p: [Vec3; 3],
    pub uv: [Vec2; 3],
    pub tex: usize,
    pub color: [u8; 4],
}

pub struct Texture {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub fn render(tris: &[Tri], textures: &[Texture], view_proj: Mat4, w: u32, h: u32, bg: [u8; 4]) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba(bg));
    let mut z = vec![f32::INFINITY; (w * h) as usize];
    for t in tris {
        let clip: Vec<Vec4> = t.p.iter().map(|p| view_proj * p.extend(1.0)).collect();
        if clip.iter().any(|c| c.w <= 0.01) {
            continue;
        }
        let s: Vec<Vec3> = clip
            .iter()
            .map(|c| {
                let n = c.truncate() / c.w;
                Vec3::new((n.x * 0.5 + 0.5) * w as f32, (1.0 - (n.y * 0.5 + 0.5)) * h as f32, n.z)
            })
            .collect();
        let inv_w: Vec<f32> = clip.iter().map(|c| 1.0 / c.w).collect();
        let minx = s.iter().map(|v| v.x).fold(f32::MAX, f32::min).max(0.0) as i32;
        let maxx = s.iter().map(|v| v.x).fold(f32::MIN, f32::max).min(w as f32 - 1.0) as i32;
        let miny = s.iter().map(|v| v.y).fold(f32::MAX, f32::min).max(0.0) as i32;
        let maxy = s.iter().map(|v| v.y).fold(f32::MIN, f32::max).min(h as f32 - 1.0) as i32;
        let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
        if area.abs() < 1e-6 {
            continue;
        }
        for py in miny..=maxy {
            for px in minx..=maxx {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let w0 = ((s[1].x - fx) * (s[2].y - fy) - (s[2].x - fx) * (s[1].y - fy)) / area;
                let w1 = ((s[2].x - fx) * (s[0].y - fy) - (s[0].x - fx) * (s[2].y - fy)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let depth = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
                let zi = (py as u32 * w + px as u32) as usize;
                if depth >= z[zi] {
                    continue;
                }
                let pw = w0 * inv_w[0] + w1 * inv_w[1] + w2 * inv_w[2];
                let uv = (t.uv[0] * w0 * inv_w[0] + t.uv[1] * w1 * inv_w[1] + t.uv[2] * w2 * inv_w[2]) / pw;
                let c = if let Some(tex) = textures.get(t.tex) {
                    let tx = (uv.x as i32).clamp(0, tex.w as i32 - 1) as u32;
                    let ty = (uv.y as i32).clamp(0, tex.h as i32 - 1) as u32;
                    let i = ((ty * tex.w + tx) * 4) as usize;
                    let c = [tex.rgba[i], tex.rgba[i + 1], tex.rgba[i + 2], tex.rgba[i + 3]];
                    if c[3] == 0 {
                        continue;
                    }
                    c
                } else {
                    t.color
                };
                z[zi] = depth;
                img.put_pixel(px as u32, py as u32, image::Rgba(c));
            }
        }
    }
    img
}
