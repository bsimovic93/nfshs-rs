//! FE3DMENU.CPP: the front end's 3D: the showroom's tables (a circle and a ring of points), a
//! camera looking at a point, and a car (or no car) drawn in a menu's view, as the car select
//! shows it.

use crate::abi::{self, Saved};
use ps1rt::Cpu;

/// The showroom's circle (33 points of `(x, 0, z)`, the last at the centre), its ring (32 pairs
/// of points, inner and outer), each view's spin (an angle added to each frame), the camera's
/// mode (looking at the car or not).
const CIRCLE: u32 = 0x8005_1754;
const RING: u32 = 0x8005_181C;
const SPINS: u32 = 0x8005_174C;
const LOOK_AT: u32 = 0x8005_1748;
/// The camera's up vector.
const UP: u32 = 0x8001_002C;
/// The frame counter, the next frame the showroom's lights flicker and whether they do now.
const FRAME: u32 = 0x8013_DB1C;
const NEXT_FLICKER: u32 = 0x8013_E690;
const FLICKER: u32 = 0x8013_E68C;
/// The renderer's flags (0x80 drawing the reflection), the scratchpad's buffer toggle, the
/// stack saved while on the scratchpad and the flag saying so, the scratchpad's stack, the
/// shadow's colour.
const RENDER_FLAGS: u32 = 0x8013_E608;
const BUFFER: u32 = 0x1F80_000C;
const SAVED_SP: u32 = 0x8013_EB18;
const ON_SCRATCHPAD: u32 = 0x8013_EB04;
const SCRATCH_STACK: u32 = 0x1F80_03FC;
const SHADOW: u32 = 0x1F80_008C;

/// The libraries' `csin`, `ccos`, `fastintsin`, `fastintcos`; the geometry's
/// `TrsProj_ResetTransPrecision`, its offset and its screen set (0x800e25d4), the projection's
/// start (0x800e12a4); `Math_NormalizeVector`, `crossproduct`, `fixedxformy`, `xformy`,
/// `Math_fasttransmult`, `transpose`, `transform`; the car's set up in the view (0x800b1db8),
/// `R3DCar_InsertCarFacetMenuII`, `R3DCar_Showroom`; `SetSp`; the soft floats' `__mulsf3`,
/// `__fixsfsi`, `__extendsfdf2`, `__divdf3`, `__adddf3`, `__truncdfsf2`.
const CSIN: u32 = 0x800F_4154;
const CCOS: u32 = 0x800F_4084;
const FAST_SIN: u32 = 0x800F_261C;
const FAST_COS: u32 = 0x800F_2618;
const RESET_PRECISION: u32 = 0x800E_2544;
const SET_SCREEN: u32 = 0x800E_25D4;
const START_PROJECTION: u32 = 0x800E_12A4;
const NORMALIZE: u32 = 0x8009_E4E0;
const CROSS: u32 = 0x800E_B440;
const FIXED_XFORM_Y: u32 = 0x800E_B56C;
const XFORM_Y: u32 = 0x800E_66B0;
const TRANS_MULT: u32 = 0x8009_E850;
const TRANSPOSE: u32 = 0x800E_4F44;
const TRANSFORM: u32 = 0x800E_B834;
const SET_UP_CAR: u32 = 0x800B_1DB8;
const INSERT_CAR: u32 = 0x800B_3428;
const SHOWROOM: u32 = 0x800B_3850;
const SET_SP: u32 = 0x800E_5098;
const MUL_SF: u32 = 0x800F_3E0C;
const FIX_SF: u32 = 0x800F_4294;
const EXTEND_SF: u32 = 0x800F_6120;
const DIV_DF: u32 = 0x800F_6730;
const ADD_DF: u32 = 0x800F_63B0;
const TRUNC_DF: u32 = 0x800F_6280;
/// One half and 65536, as floats; 2.5's high word as a double.
const HALF: u32 = 0x3F00_0000;
const ONE_16: u32 = 0x4780_0000;
const TWO_AND_A_HALF_HI: u32 = 0x4004_0000;

/// `v * 48 / 256`, as the circle's points are.
fn by_48(v: u32) -> u32 {
    ((v << 4).wrapping_add(v << 5) as i32 >> 8) as u32
}

/// `Fe3D_InitShowroom` (FE3DMENU.CPP, 0x800128e8): the circle's 32 points (radius 48, every
/// 128th of the turn's 4096; `csin`, `ccos`) and its centre; the ring's 32 pairs (every 32nd,
/// `fastintsin`, `fastintcos` an eighth down): the inner at 3/32, the outer at 21/256.
pub fn abi_init_showroom(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        for k in 0..32u32 {
            let at = CIRCLE + 6 * k;
            (r.0[0], r.0[1], r.0[2]) = (at, 128 * k, k);
            let v = r.call_ptr_leaving(c, CSIN, &[128 * k], 0x8001_291c);
            c.m.w16(at, by_48(v) as u16);
            c.m.w16(at + 2, 0);
            let v = r.call_ptr_leaving(c, CCOS, &[128 * k], 0x8001_293c);
            c.m.w16(at + 4, by_48(v) as u16);
        }
        let centre = CIRCLE + 192;
        for at in [0, 2, 4] {
            c.m.w16(centre + at, 0);
        }
        (r.0[0], r.0[4]) = (centre, RING);
        let (mut a0, mut v1) = (0, 0);
        for k in 0..32u32 {
            (r.0[1], r.0[2], r.0[3]) = (32 * k, k, 2 * k);
            let s = r.call_ptr_leaving(c, FAST_SIN, &[32 * k], 0x8001_29b0) as i32 >> 3;
            r.0[0] = s as u32;
            let v = r.call_ptr_leaving(c, FAST_COS, &[32 * k], 0x8001_29bc) as i32 >> 3;
            let (inner, outer) = (RING + 12 * k, RING + 12 * k + 6);
            let m = &mut c.m;
            m.w16(inner, (s.wrapping_mul(3) as u32 >> 5) as u16);
            m.w16(inner + 2, 0);
            m.w16(inner + 4, (v.wrapping_mul(3) as u32 >> 5) as u16);
            m.w16(outer, (s.wrapping_mul(21) >> 8) as u16);
            v1 = (v.wrapping_mul(21) >> 8) as u32;
            m.w16(outer + 2, 0);
            m.w16(outer + 4, v1 as u16);
            a0 = outer;
        }
        // the scratch registers as the original leaves them
        (c.r[2], c.r[3], c.r[4], c.r[5], c.r[6]) = (0, v1, a0, 63, 62);
    })
}

/// `Fe3D_InitShowroom` entered as the original's callers `jal` it.
pub(crate) fn init_showroom_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_28e8, None, &[]);
    abi_init_showroom(c);
}

/// `CameraLookAt` (FE3DMENU.CPP, 0x80012a94): matrix `out` of a camera at `at` looking at the
/// origin: forward the normalised `-at`, right the up vector across it, up forward across
/// right; its rows right, up, forward.
pub fn abi_camera_look_at(c: &mut Cpu) {
    let (out, at) = (c.r[4], c.r[5]);
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let up = [m.r32(UP), m.r32(UP + 4), m.r32(UP + 8)];
        for k in 0..3 {
            m.w32(sp + 48 + 4 * k, up[k as usize]);
        }
        let mut z = 0;
        for k in 0..3 {
            z = 0u32.wrapping_sub(m.r32(at + 4 * k));
            m.w32(sp + 16 + 4 * k, z);
        }
        r.0[1] = out;
        // the scratch registers the up vector's copy leaves
        (c.r[2], c.r[3], c.r[7], c.r[8], c.r[9]) = (z, up[0], up[1], up[2], UP);
        r.call_ptr_leaving(c, NORMALIZE, &[sp + 16], 0x8001_2afc);
        (r.0[0], r.0[2]) = (sp + 32, sp + 48);
        r.call_ptr_leaving(c, CROSS, &[sp + 48, sp + 16, sp + 32], 0x8001_2b14);
        r.call_ptr_leaving(c, CROSS, &[sp + 16, sp + 32, sp + 48], 0x8001_2b24);
        let m = &mut c.m;
        let mut v = 0;
        for (k, row) in [32, 48, 16].into_iter().enumerate() {
            for j in 0..3 {
                v = m.r32(sp + row + 4 * j);
                m.w32(out + 12 * k as u32 + 4 * j, v);
            }
        }
        c.r[2] = v;
    })
}

/// A float multiplied (the soft floats') then made an int.
fn fixed(c: &mut Cpu, r: &Saved, v: u32, by: u32, ras: (u32, u32)) -> u32 {
    let v = r.call_ptr_leaving(c, MUL_SF, &[v, by], ras.0);
    r.call_ptr_leaving(c, FIX_SF, &[v], ras.1)
}

/// The view's matrices from `turn` (at sp+16): its world one (the turn squashed by 0x8980/65536
/// in y, at `view`+20) and that transposed (+68); the camera's position (`x`, `y`, 0 in
/// 65536ths) through each into +8 and (negated) +56. `ras` the calls' return addresses.
fn place_view(c: &mut Cpu, r: &mut Saved, sp: u32, view: u32, (x, y): (u32, u32), at: (u32, u32), ras: &[u32; 7]) {
    let (id, pos) = at;
    let m = &mut c.m;
    for k in 0..9 {
        m.w32(id + 4 * k, if k % 4 == 0 { 0x1_0000 } else { 0 });
    }
    m.w32(id + 16, 0x8980);
    r.0[2] = view + 20;
    r.call_ptr_leaving(c, TRANS_MULT, &[id, sp + 16, view + 20], ras[0]);
    r.0[3] = view + 68;
    r.call_ptr_leaving(c, TRANSPOSE, &[view + 20, view + 68], ras[1]);
    r.0[0] = ONE_16;
    c.m.w32(pos, 0);
    let v = fixed(c, r, x, ONE_16, (ras[2], ras[3]));
    c.m.w32(pos + 4, v);
    let v = fixed(c, r, y, ONE_16, (ras[4], ras[5]));
    r.0[0] = pos;
    c.m.w32(pos + 8, v);
    r.call_ptr_leaving(c, TRANSFORM, &[pos, view + 20, view + 8], ras[6]);
    let m = &mut c.m;
    for k in 0..3 {
        let v = m.r32(view + 8 + 4 * k);
        m.w32(pos + 4 * k, 0u32.wrapping_sub(v));
    }
}

/// `Draw_MenuRenderingView` (FE3DMENU.CPP, 0x80012ba8): `car` drawn in `view` at (`x`, `y`)
/// on the screen (in the camera's mode, 140 down), for player `k` (its spin turned on by
/// `spin`; the first player's lights flickering every 5 frames); turned by `angle` (a 64th of
/// the turn's); the camera `distance` back and `height` up (floats; in the camera's mode it
/// looks at the car from half the distance, then sits on the floor that far ahead); the car's
/// own matrix plain, its facets drawn with `facet`, the showroom around it; with `reflection`,
/// the car again on the scratchpad's stack as the reflection.
pub fn abi_draw_menu_rendering_view(c: &mut Cpu) {
    let (car, view, x, y) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let args = c.r[29] + 16;
    let arg = |c: &mut Cpu, k: u32| c.m.r32(args + 4 * k);
    let (k, spin, angle, distance, height, facet, reflection) =
        (arg(c, 0), arg(c, 1), arg(c, 2), arg(c, 3), arg(c, 4), arg(c, 5), arg(c, 6));
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[2], r.0[3], r.0[4], r.0[5], r.0[6], r.0[7], r.0[8]) = (y, x, angle, car, distance, height, view, k);
        r.call_ptr_leaving(c, RESET_PRECISION, &[], 0x8001_2bf8);
        r.0[1] = 0x8005_0000;
        if c.m.r32(LOOK_AT) != 0 {
            r.0[0] = 140;
        }
        r.call_ptr_leaving(c, SET_SCREEN, &[x.wrapping_sub(256), r.0[0].wrapping_sub(128), 512, 256], 0x8001_2c20);
        let slot = SPINS.wrapping_add(k << 2);
        let turned = c.m.r32(slot).wrapping_add(spin);
        let look = c.m.r32(LOOK_AT);
        c.m.w32(slot, turned);
        let (mut distance, mut height) = (distance, height);
        if look != 0 {
            distance = r.call_ptr_leaving(c, MUL_SF, &[distance, HALF], 0x8001_2c54);
            (r.0[5], r.0[0]) = (distance, ONE_16);
            c.m.w32(sp + 152, 0);
            let v = fixed(c, &r, distance, ONE_16, (0x8001_2c6c, 0x8001_2c74));
            c.m.w32(sp + 156, v);
            let v = fixed(c, &r, height, ONE_16, (0x8001_2c84, 0x8001_2c8c));
            r.0[1] = sp + 56;
            c.m.w32(sp + 160, v);
            r.call_ptr_leaving(c, 0x8001_2a94, &[sp + 56, sp + 152], 0x8001_2ca0);
            r.0[0] = sp + 96;
            r.call_ptr_leaving(c, FIXED_XFORM_Y, &[sp + 96, angle << 6], 0x8001_2cb0);
            r.call_ptr_leaving(c, TRANS_MULT, &[sp + 56, sp + 96, sp + 16], 0x8001_2cc0);
            let lo = r.call_ptr_leaving(c, EXTEND_SF, &[height], 0x8001_2cc8);
            (r.0[0], r.0[1]) = (lo, c.r[3]);
            let lo = r.call_ptr_leaving(c, EXTEND_SF, &[distance], 0x8001_2cd8);
            let lo = r.call_ptr_leaving(c, DIV_DF, &[lo, c.r[3], 0, TWO_AND_A_HALF_HI], 0x8001_2cec);
            let lo = r.call_ptr_leaving(c, ADD_DF, &[r.0[0], r.0[1], lo, c.r[3]], 0x8001_2d00);
            height = r.call_ptr_leaving(c, TRUNC_DF, &[lo, c.r[3]], 0x8001_2d0c);
            distance = 0;
            (r.0[6], r.0[5]) = (height, distance);
        } else {
            r.call_ptr_leaving(c, FIXED_XFORM_Y, &[sp + 16, angle << 6], 0x8001_2d24);
        }
        r.0[1] = 0x1_0000;
        let ras = [0x8001_2d68, 0x8001_2d78, 0x8001_2d8c, 0x8001_2d94, 0x8001_2da4, 0x8001_2dac, 0x8001_2dc4];
        place_view(c, &mut r, sp, view, (distance, height), (sp + 168, sp + 136), &ras);
        r.call_ptr_leaving(c, TRANSFORM, &[sp + 136, view + 68, view + 56], 0x8001_2df8);
        let m = &mut c.m;
        for at in [240, 256, 272] {
            m.w32(car + at, 0x1_0000);
        }
        for at in [244, 248, 252, 260, 264, 268, 160, 164, 168] {
            m.w32(car + at, 0);
        }
        if k == 0 {
            let now = m.r32(FRAME);
            if (m.r32(NEXT_FLICKER) as i32) < now as i32 {
                let now = m.r32(FRAME);
                m.w32(FLICKER, 1);
                m.w32(NEXT_FLICKER, now.wrapping_add(5));
            } else {
                m.w32(FLICKER, 0);
            }
        }
        m.w32(view + 4, k);
        r.call_ptr_leaving(c, START_PROJECTION, &[], 0x8001_2e70);
        r.call_ptr_leaving(c, SET_UP_CAR, &[car, view], 0x8001_2e7c);
        r.call_ptr_leaving(c, INSERT_CAR, &[car, facet], 0x8001_2e88);
        r.call_ptr_leaving(c, SHOWROOM, &[view], 0x8001_2e90);
        c.r[8] = reflection;
        if reflection == 0 {
            c.r[4] = 0x1F80_0000;
            return;
        }
        (r.0[2], r.0[1]) = (0x1F80_0000, 0x8014_0000);
        let m = &mut c.m;
        let flags = m.r32(RENDER_FLAGS) | 0x80;
        let buffer = m.r32(BUFFER) ^ 1;
        m.w32(RENDER_FLAGS, flags);
        m.w32(BUFFER, buffer);
        let old = r.call_ptr_leaving(c, SET_SP, &[SCRATCH_STACK], 0x8001_2ec8);
        c.r[29] = SCRATCH_STACK;
        (r.0[0], r.0[3]) = (0x8014_0000, 0x8014_0000);
        c.m.w32(SAVED_SP, old);
        c.m.w32(ON_SCRATCHPAD, 1);
        r.call_ptr_leaving(c, SET_UP_CAR, &[car, view], 0x8001_2ee8);
        let back = c.m.r32(SAVED_SP);
        let v = r.call_ptr_leaving(c, SET_SP, &[back], 0x8001_2ef4);
        c.r[29] = back;
        c.m.w32(SAVED_SP, v);
        c.m.w32(ON_SCRATCHPAD, 0);
        r.call_ptr_leaving(c, INSERT_CAR, &[car, facet], 0x8001_2f08);
        let m = &mut c.m;
        let flags = m.r32(RENDER_FLAGS) & 0x7F;
        let buffer = m.r32(BUFFER) ^ 1;
        m.w32(RENDER_FLAGS, flags);
        m.w32(BUFFER, buffer);
        (c.r[2], c.r[3], c.r[4]) = (flags, buffer, car);
    })
}

/// `Draw_MenuRenderingView` entered as the original's callers `jal` it.
pub(crate) fn draw_menu_rendering_view_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_2ba8, None, &[]);
    abi_draw_menu_rendering_view(c);
}

/// The car select's empty view (FE3DMENU.CPP, 0x80012f50, a static function): `view` at
/// (`x`, `y`) for player `k` (its spin turned by `spin`), turned by `angle` (the turn's
/// 4096ths), the camera `distance` back and `height` up; the shadow's colour grey, only the
/// showroom drawn.
pub fn abi_draw_empty_view(c: &mut Cpu) {
    let (view, x, y, k) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let args = c.r[29] + 16;
    let arg = |c: &mut Cpu, n: u32| c.m.r32(args + 4 * n);
    let (spin, angle, distance, height) = (arg(c, 0), arg(c, 1), arg(c, 2), arg(c, 3));
    abi::frame(c, 152, |c, sp| {
        let mut r = Saved::of(c);
        r.0[..8].copy_from_slice(&[k, view, x, y, angle, spin, distance, height]);
        r.call_ptr_leaving(c, RESET_PRECISION, &[], 0x8001_2f9c);
        r.call_ptr_leaving(c, SET_SCREEN, &[x.wrapping_sub(256), y.wrapping_sub(128), 512, 256], 0x8001_2fb0);
        let slot = SPINS.wrapping_add(k << 2);
        r.0[0] = slot;
        let turned = c.m.r32(slot).wrapping_add(spin);
        c.m.w32(slot, turned);
        r.call_ptr_leaving(c, XFORM_Y, &[sp + 16, angle], 0x8001_2fd8);
        let ras = [0x8001_301c, 0x8001_302c, 0x8001_3040, 0x8001_3048, 0x8001_3058, 0x8001_3060, 0x8001_3078];
        place_view(c, &mut r, sp, view, (distance, height), (sp + 72, sp + 56), &ras);
        r.call_ptr_leaving(c, TRANSFORM, &[sp + 56, view + 68, view + 56], 0x8001_30ac);
        r.call_ptr_leaving(c, START_PROJECTION, &[], 0x8001_30b4);
        c.m.w32(SHADOW, 0x40_4040);
        (c.r[1], c.r[2]) = (0x1F80_0000, 0x40_4040);
        r.call_ptr_leaving(c, SHOWROOM, &[view], 0x8001_30cc);
    })
}

/// The empty view entered as the original's callers `jal` it.
pub(crate) fn draw_empty_view_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_2f50, None, &[]);
    abi_draw_empty_view(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The car select's car objects and its view; a vector and a matrix.
    const CAR_OBJECTS: u32 = 0x8005_2EF8;
    const CAR_WORLD: u32 = 0x8011_8234;
    const VEC: u32 = 0x8014_7000;
    const OUT: u32 = 0x8014_7100;

    /// FEMENU's structures, then: the camera's mode, the views' spins, the frame and the next
    /// flicker (either side), the renderer's flags, the scratchpad's buffer; a vector of any
    /// direction (never zero); the car's own matrix and position any words.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        m.w32(LOOK_AT, pick(rng, 2));
        for k in 0..2 {
            m.w32(SPINS + 4 * k, pick(rng, 8192));
        }
        let now = pick(rng, 100_000);
        m.w32(FRAME, now);
        m.w32(NEXT_FLICKER, now.wrapping_add(pick(rng, 11)).wrapping_sub(5));
        m.w32(RENDER_FLAGS, rng.next() as u32 & 0xFF);
        m.w32(BUFFER, pick(rng, 2));
        for k in 0..3 {
            m.w32(VEC + 4 * k, (pick(rng, 0x40_0000) as i32 - 0x20_0000) as u32 | 1);
        }
        let o = m.r32(CAR_OBJECTS);
        for at in [160, 164, 168, 240, 244, 248, 252, 256, 260, 264, 268, 272] {
            m.w32(o + at, rng.next() as u32);
        }
    }

    const DISTANCES: [u32; 4] = [0x4080_0000, 0x4000_0000, 0x40D0_0000, 0xBF80_0000];
    const HEIGHTS: [u32; 4] = [0xC0F0_0000, 0xC040_0000, 0, 0x4010_0000];

    fn cases() -> Vec<Case> {
        vec![
            ("Fe3D_InitShowroom", 0x8001_28e8, nfs4rc::f_800128e8, |_, _| vec![]),
            ("CameraLookAt", 0x8001_2a94, nfs4rc::f_80012a94, |_, _| vec![OUT, VEC]),
            ("Draw_MenuRenderingView", 0x8001_2ba8, nfs4rc::f_80012ba8, |c, r| {
                let o = c.m.r32(CAR_OBJECTS);
                let (d, h) = (DISTANCES[pick(r, 4) as usize], HEIGHTS[pick(r, 4) as usize]);
                vec![o, CAR_WORLD, pick(r, 320), pick(r, 240), pick(r, 2), pick(r, 64), pick(r, 64), d, h, pick(r, 256), pick(r, 2)]
            }),
            ("Draw_EmptyView", 0x8001_2f50, nfs4rc::f_80012f50, |_, r| {
                let (d, h) = (DISTANCES[pick(r, 4) as usize], HEIGHTS[pick(r, 4) as usize]);
                vec![CAR_WORLD, pick(r, 320), pick(r, 240), pick(r, 2), pick(r, 64), pick(r, 4096), d, h]
            }),
        ]
    }

    #[test]
    fn fe3dmenu_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x3D0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 100, 0x3D1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn fe3dmenu_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 60, 0x3E1 + k as u64, shuffle, &args);
        }
    }
}
