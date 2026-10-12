//! Track light glows as the original draws them (BWorld_BuildGlareEffects, Flare_Halo2,
//! Flare_OctFlareSpikes): around each light, in view space, an octagon fading from the
//! type's centre colour to black and a four-pointed star in its second colour, scaled by
//! the type's size, turned by the light's screen position, added to the picture.

use crate::assets::Vertex;
use crate::ps1::race::Glare;
use glam::Vec3;

/// Flare_gType: centre colour, star colour (0xBBGGRR), size (4.12), flags.
const TYPES: [(u32, u32, i32, u32); 34] = [
    (0x808080, 0x808080, 1024, 0x0),
    (0x303030, 0x808080, 0, 0x3),
    (0x080868, 0x060628, 1, 0xd),
    (0x808080, 0x303030, 0, 0xd),
    (0x406fc0, 0x0808c0, 0, 0xd),
    (0x406fc0, 0x0808c0, 0, 0xd),
    (0x707070, 0x101018, 0, 0xd),
    (0x080880, 0x080828, 0, 0xd),
    (0x080880, 0x080828, 0, 0xc),
    (0x088080, 0x082828, 0, 0xc),
    (0x800808, 0x280808, 0, 0xc),
    (0x303030, 0x808080, 0, 0x3),
    (0x080868, 0x060628, 2, 0xd),
    (0x808080, 0x303030, 1, 0xd),
    (0x406fc0, 0x0808c0, 1, 0xd),
    (0x406fc0, 0x0808c0, 1, 0xd),
    (0x707070, 0x101018, 1, 0xd),
    (0x080880, 0x080828, 1, 0xd),
    (0x080880, 0x080828, 1, 0xc),
    (0x088080, 0x082828, 1, 0xc),
    (0x800808, 0x280808, 1, 0xc),
    (0x080880, 0x080828, 0, 0x0),
    (0x0a1c82, 0x020414, 675, 0x88),
    (0x00bebe, 0x004646, 0, 0x0),
    (0xc0c0c0, 0x000000, 770, 0x4),
    (0x384868, 0x202020, 1795, 0xc),
    (0x605838, 0x181818, 3594, 0xc),
    (0x206890, 0x182020, 3594, 0xc),
    (0x206890, 0x182020, 1281, 0x8),
    (0x6068a8, 0x202020, 734, 0x8),
    (0xc8c8c8, 0x202020, 16384, 0x9),
    (0x206890, 0x182020, 1281, 0x18),
    (0x738b00, 0x000000, 1024, 0xc),
    (0xff0000, 0x000000, 256, 0x8),
];
/// Flare_gOct and Flare_gSpikes (x, y per 4096 of the size).
const OCT: [(f32, f32); 8] = [(0.0, -256.0), (180.0, -180.0), (256.0, 0.0), (180.0, 180.0), (0.0, 256.0), (-180.0, 180.0), (-256.0, 0.0), (-180.0, -180.0)];
const SPIKES: [(f32, f32); 4] = [(0.0, -45.0), (45.0, 0.0), (0.0, 45.0), (-45.0, 0.0)];
/// Flare_OctFlareSpikes' quads: spike, centre, octagon corner, spike.
const SPIKE_A: [usize; 8] = [3, 3, 0, 0, 1, 1, 2, 2];
const SPIKE_B: [usize; 8] = [1, 2, 2, 3, 3, 0, 0, 1];

/// The view the flares are built for: camera basis, the original's projection distance H
/// (320-pixel screen) and its vertical squash.
pub struct FlareView {
    pub pos: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub forward: Vec3,
    pub h: f32,
    pub y_scale: f32,
    /// simGlobal.counter, for the blinking types.
    pub counter: u32,
}

fn rgb(c: u32) -> [f32; 3] {
    // untextured PS1 colour on the white texel (the shader doubles)
    [(c & 0xFF) as f32 / 510.0, ((c >> 8) & 0xFF) as f32 / 510.0, ((c >> 16) & 0xFF) as f32 / 510.0]
}

/// Glow geometry (additive) for the track's lights.
pub fn build(glares: &[Glare], view: &FlareView, white: [f32; 2], out: &mut Vec<Vertex>) {
    for g in glares {
        if g.kind == 100 || g.kind < 0 {
            continue;
        }
        let Some(&(c0, c1, size, mut flags)) = TYPES.get(g.kind as usize & 0xFF) else { continue };
        // two-point flares (beams between a pair) are drawn by Flare_Halo2 with both ends;
        // single lights drop those modes
        flags &= !5;
        if flags & 0x10 != 0 && (view.counter >> 6) & 1 != 0 || flags & 0x20 != 0 && ((view.counter + 27) >> 5) & 1 != 0 {
            continue;
        }
        let p = Vec3::new(-(g.pos[0] as f32), g.pos[1] as f32, g.pos[2] as f32) / 65536.0;
        // view space in GTE units (1/64 m): x right, y down, z ahead
        let v = p - view.pos;
        let (x, y, z) = (v.dot(view.right) * 64.0, -v.dot(view.up) * 64.0 * view.y_scale, v.dot(view.forward) * 64.0);
        if z < 128.0 || x.abs() > z {
            continue;
        }
        let (sx, sy) = (view.h * x / z + 160.0, view.h * y / z + 120.0);
        // turned by its screen position (RotMatrixZ: 4096 per turn)
        let angle = if flags & 0x8 != 0 { (sx as i32 + sy as i32) as f32 * 4.0 / 4096.0 * std::f32::consts::TAU } else { 0.0 };
        let (sn, cs) = angle.sin_cos();
        let s = size as f32 / 4096.0;
        // a point of the flare: view-space offset (x right, y down) to world space; the
        // original's squash is in its world matrix, so undo the renderer's on these offsets
        let at = |(ox, oy): (f32, f32)| -> Vec3 {
            let (rx, ry) = ((ox * cs - oy * sn) * s, (ox * sn + oy * cs) * s);
            p + (view.right * rx - view.up * ry / view.y_scale) / 64.0
        };
        let (c0, c1, black) = (rgb(c0), rgb(c1), [0.0; 3]);
        // the original sorts the flare at half its depth (OT index z / 2): it is drawn over
        // anything further than half way. Move the points half way to the camera (same
        // picture, that depth).
        let near = |q: Vec3| view.pos + (q - view.pos) * 0.5;
        let mut tri = |a: Vec3, ca: [f32; 3], b: Vec3, cb: [f32; 3], c: Vec3, cc: [f32; 3]| {
            for (q, col) in [(near(a), ca), (near(b), cb), (near(c), cc)] {
                out.push(Vertex { pos: q.into(), uv: white, color: [col[0], col[1], col[2], 0.0] });
            }
        };
        let oct: Vec<Vec3> = OCT.iter().map(|&o| at(o)).collect();
        // nearer than 50 m: the octagon with the star (Flare_OctFlareSpikes); further away
        // the original switches to a hexagon, then a quad
        if z < 3200.0 {
            let spikes: Vec<Vec3> = SPIKES.iter().map(|&o| at(o)).collect();
            for k in 0..8 {
                let (a, b) = (spikes[SPIKE_A[k]], spikes[SPIKE_B[k]]);
                tri(a, black, p, c1, oct[k], black);
                tri(a, black, oct[k], black, b, black);
            }
        }
        for k in 0..8 {
            tri(oct[k], black, p, c0, oct[(k + 1) % 8], black);
        }
    }
}
