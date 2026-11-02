//! FE3DMENU.CPP: the front end's 3D: the showroom's tables (a circle and a ring of points), a
//! camera looking at a point, and a car (or no car) drawn in a menu's view, as the car select
//! shows it.
//!
//! Written as FEMENU.CPP is (see there). The library's math and the soft floats are called as
//! the original calls them (by address); `R3DCar_InsertCarFacetMenuII` and `R3DCar_Showroom`
//! are the host's.

use crate::call::{call_addr, call_addr64, with_buffer};
use ps1rt::{Cpu, Mem};

/// The showroom's circle (33 points of `(x, 0, z)`, the last at the centre), its ring (32 pairs
/// of points, inner and outer), each view's spin (an angle added to each frame), the camera's
/// mode (looking at the car or not).
const CIRCLE: u32 = 0x8005_1754;
const RING: u32 = 0x8005_181C;
const SPINS: u32 = 0x8005_174C;
const LOOK_AT: u32 = 0x8005_1748;
const POINTS: u32 = 32;
/// The camera's up vector.
const UP: u32 = 0x8001_002C;
/// The frame counter, the next frame the showroom's lights flicker and whether they do now
/// (every 5 frames).
const FRAME: u32 = 0x8013_DB1C;
const NEXT_FLICKER: u32 = 0x8013_E690;
const FLICKER: u32 = 0x8013_E68C;
const FLICKER_EVERY: u32 = 5;
/// The renderer's flags (0x80 drawing the reflection), the scratchpad's buffer toggle, the
/// stack saved while on the scratchpad and the flag saying so, the scratchpad's stack, the
/// shadow's colour.
const RENDER_FLAGS: u32 = 0x8013_E608;
const REFLECTION: u32 = 0x80;
const BUFFER: u32 = 0x1F80_000C;
const SAVED_SP: u32 = 0x8013_EB18;
const ON_SCRATCHPAD: u32 = 0x8013_EB04;
const SCRATCH_STACK: u32 = 0x1F80_03FC;
const SHADOW: u32 = 0x1F80_008C;
const SHADOW_GREY: u32 = 0x40_4040;

/// The libraries' `csin`, `ccos`, `fastintsin`, `fastintcos`; the geometry's
/// `TrsProj_ResetTransPrecision`, its offset and its screen set, the projection's start;
/// `Math_NormalizeVector`, `crossproduct`, `fixedxformy`, `xformy`, `Math_fasttransmult`,
/// `transpose`, `transform`; the car's set up in the view, `R3DCar_InsertCarFacetMenuII`,
/// `R3DCar_Showroom`; the soft floats' `__mulsf3`, `__fixsfsi`, `__extendsfdf2`,
/// `__divdf3`, `__adddf3`, `__truncdfsf2`.
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

/// A view (`tMenuRenderingView`): +4 its player, +8 the camera's position, +20 its world matrix,
/// +56 the camera's position in it, +68 the matrix transposed.
const VIEW_PLAYER: u32 = 4;
const VIEW_POSITION: u32 = 8;
const VIEW_MATRIX: u32 = 20;
const VIEW_EYE: u32 = 56;
const VIEW_TRANSPOSED: u32 = 68;
/// A matrix (9 words, 16.16) and a vector (3 words).
const MATRIX: u32 = 36;
const VECTOR: u32 = 12;
/// The view's squash in y (16.16).
const SQUASH: u32 = 0x8980;
const ONE: u32 = 0x1_0000;
/// The car object's matrix (+240, 9 words) and its offset (+160, 3 words).
const CAR_MATRIX: u32 = 240;
const CAR_OFFSET: u32 = 160;

/// `v * 48 / 256`, as the circle's points are.
fn by_48(v: u32) -> u32 {
    ((v << 4).wrapping_add(v << 5) as i32 >> 8) as u32
}

/// `Fe3D_InitShowroom` (FE3DMENU.CPP, 0x800128e8): the circle's 32 points (radius 48, every
/// 128th of the turn's 4096; `csin`, `ccos`) and its centre; the ring's 32 pairs (every 32nd,
/// `fastintsin`, `fastintcos` an eighth down): the inner at 3/32, the outer at 21/256.
pub fn init_showroom(c: &mut Cpu) {
    for k in 0..POINTS {
        let at = CIRCLE + 6 * k;
        let v = call_addr(c, CSIN, &[128 * k]);
        c.m.w16(at, by_48(v) as u16);
        c.m.w16(at + 2, 0);
        let v = call_addr(c, CCOS, &[128 * k]);
        c.m.w16(at + 4, by_48(v) as u16);
    }
    let centre = CIRCLE + 6 * POINTS;
    for at in [0, 2, 4] {
        c.m.w16(centre + at, 0);
    }
    for k in 0..POINTS {
        let s = call_addr(c, FAST_SIN, &[32 * k]) as i32 >> 3;
        let v = call_addr(c, FAST_COS, &[32 * k]) as i32 >> 3;
        let (inner, outer) = (RING + 12 * k, RING + 12 * k + 6);
        let m = &mut c.m;
        m.w16(inner, (s.wrapping_mul(3) as u32 >> 5) as u16);
        m.w16(inner + 2, 0);
        m.w16(inner + 4, (v.wrapping_mul(3) as u32 >> 5) as u16);
        m.w16(outer, (s.wrapping_mul(21) >> 8) as u16);
        m.w16(outer + 2, 0);
        m.w16(outer + 4, (v.wrapping_mul(21) >> 8) as u16);
    }
}

pub fn abi_init_showroom(c: &mut Cpu) {
    init_showroom(c);
}

/// `CameraLookAt` (FE3DMENU.CPP, 0x80012a94): matrix `out` of a camera at `at` looking at the
/// origin: forward the normalised `-at`, right the up vector across it, up forward across
/// right; its rows right, up, forward.
pub fn camera_look_at(c: &mut Cpu, out: u32, at: u32) {
    with_buffer(c, 3 * VECTOR, |c, buffer| {
        let (forward, right, up) = (buffer, buffer + VECTOR, buffer + 2 * VECTOR);
        let m = &mut c.m;
        for k in 0..3 {
            let v = m.r32(UP + 4 * k);
            m.w32(up + 4 * k, v);
        }
        for k in 0..3 {
            let v = 0u32.wrapping_sub(m.r32(at + 4 * k));
            m.w32(forward + 4 * k, v);
        }
        call_addr(c, NORMALIZE, &[forward]);
        call_addr(c, CROSS, &[up, forward, right]);
        call_addr(c, CROSS, &[forward, right, up]);
        let m = &mut c.m;
        for (k, row) in [right, up, forward].into_iter().enumerate() {
            for j in 0..3 {
                let v = m.r32(row + 4 * j);
                m.w32(out + VECTOR * k as u32 + 4 * j, v);
            }
        }
    })
}

pub fn abi_camera_look_at(c: &mut Cpu) {
    camera_look_at(c, c.r[4], c.r[5]);
}

/// A float multiplied, then made an int (the soft floats').
fn fixed(c: &mut Cpu, v: u32, by: u32) -> u32 {
    let v = call_addr(c, MUL_SF, &[v, by]);
    call_addr(c, FIX_SF, &[v])
}

/// The view's matrices from `turn`: its world one (the turn squashed in y) and that transposed;
/// the camera's position (`distance`, `height` floats, 0 in 65536ths) through it, negated at
/// `pos`. `id` is room for a matrix.
fn place_view(c: &mut Cpu, view: u32, turn: u32, distance: u32, height: u32, id: u32, pos: u32) {
    let m = &mut c.m;
    for k in 0..9 {
        m.w32(id + 4 * k, if k % 4 == 0 { ONE } else { 0 });
    }
    m.w32(id + 16, SQUASH);
    call_addr(c, TRANS_MULT, &[id, turn, view + VIEW_MATRIX]);
    call_addr(c, TRANSPOSE, &[view + VIEW_MATRIX, view + VIEW_TRANSPOSED]);
    c.m.w32(pos, 0);
    let v = fixed(c, distance, ONE_16);
    c.m.w32(pos + 4, v);
    let v = fixed(c, height, ONE_16);
    c.m.w32(pos + 8, v);
    call_addr(c, TRANSFORM, &[pos, view + VIEW_MATRIX, view + VIEW_POSITION]);
    let m = &mut c.m;
    for k in 0..3 {
        let v = m.r32(view + VIEW_POSITION + 4 * k);
        m.w32(pos + 4 * k, 0u32.wrapping_sub(v));
    }
    call_addr(c, TRANSFORM, &[pos, view + VIEW_TRANSPOSED, view + VIEW_EYE]);
}

/// The view's screen (512 by 256, centred at `x`, `y`) and player `k`'s spin turned by `spin`.
fn start_view(c: &mut Cpu, x: u32, y: u32, k: u32, spin: u32) {
    call_addr(c, RESET_PRECISION, &[]);
    call_addr(c, SET_SCREEN, &[x.wrapping_sub(256), y.wrapping_sub(128), 512, 256]);
    let slot = SPINS.wrapping_add(k << 2);
    let turned = c.m.r32(slot).wrapping_add(spin);
    c.m.w32(slot, turned);
}

/// The menu's car drawn (`Draw_MenuRenderingView`'s arguments past the view and its place).
pub struct CarView {
    pub player: u32,
    pub spin: u32,
    pub angle: u32,
    pub distance: u32,
    pub height: u32,
    pub facet: u32,
    pub reflection: bool,
}

/// `Draw_MenuRenderingView` (FE3DMENU.CPP, 0x80012ba8): `car` drawn in `view` at (`x`, `y`)
/// on the screen (in the camera's mode, 140 down), for the player (its spin turned on; the
/// first player's lights flickering every 5 frames); turned by `angle` (a 64th of the turn's);
/// the camera `distance` back and `height` up (floats; in the camera's mode it looks at the car
/// from half the distance, then sits on the floor, `height + distance / 2.5` up); the car's own
/// matrix plain, its facets drawn with `facet`, the showroom around it; with `reflection`, the
/// car set up again on the scratchpad's stack as the reflection.
pub fn draw_menu_rendering_view(c: &mut Cpu, car: u32, view: u32, x: u32, y: u32, v: &CarView) {
    let look = c.m.r32(LOOK_AT) != 0;
    let (k, angle) = (v.player, v.angle);
    start_view(c, x, if look { 140 } else { y }, k, v.spin);
    let (mut distance, mut height) = (v.distance, v.height);
    with_buffer(c, 4 * MATRIX + 2 * VECTOR, |c, buffer| {
        let (turn, look_at, around, id) = (buffer, buffer + MATRIX, buffer + 2 * MATRIX, buffer + 3 * MATRIX);
        let (pos, eye) = (buffer + 4 * MATRIX, buffer + 4 * MATRIX + VECTOR);
        if look {
            distance = call_addr(c, MUL_SF, &[distance, HALF]);
            c.m.w32(eye, 0);
            let v = fixed(c, distance, ONE_16);
            c.m.w32(eye + 4, v);
            let v = fixed(c, height, ONE_16);
            c.m.w32(eye + 8, v);
            camera_look_at(c, look_at, eye);
            call_addr(c, FIXED_XFORM_Y, &[around, angle << 6]);
            call_addr(c, TRANS_MULT, &[look_at, around, turn]);
            let h = call_addr64(c, EXTEND_SF, &[height]);
            let d = call_addr64(c, EXTEND_SF, &[distance]);
            let d = call_addr64(c, DIV_DF, &[d.0, d.1, 0, TWO_AND_A_HALF_HI]);
            let sum = call_addr64(c, ADD_DF, &[h.0, h.1, d.0, d.1]);
            height = call_addr(c, TRUNC_DF, &[sum.0, sum.1]);
            distance = 0;
        } else {
            call_addr(c, FIXED_XFORM_Y, &[turn, angle << 6]);
        }
        place_view(c, view, turn, distance, height, id, pos);
    });
    let m = &mut c.m;
    for k in 0..9 {
        m.w32(car + CAR_MATRIX + 4 * k, if k % 4 == 0 { ONE } else { 0 });
    }
    for k in 0..3 {
        m.w32(car + CAR_OFFSET + 4 * k, 0);
    }
    if k == 0 {
        flicker(m);
    }
    m.w32(view + VIEW_PLAYER, k);
    call_addr(c, START_PROJECTION, &[]);
    call_addr(c, SET_UP_CAR, &[car, view]);
    call_addr(c, INSERT_CAR, &[car, v.facet]);
    call_addr(c, SHOWROOM, &[view]);
    if v.reflection {
        reflect(c, car, view, v.facet);
    }
}

/// The showroom's lights flicker every 5 frames.
fn flicker(m: &mut Mem) {
    let now = m.r32(FRAME);
    if (m.r32(NEXT_FLICKER) as i32) < now as i32 {
        let now = m.r32(FRAME);
        m.w32(FLICKER, 1);
        m.w32(NEXT_FLICKER, now.wrapping_add(FLICKER_EVERY));
    } else {
        m.w32(FLICKER, 0);
    }
}

/// The car's reflection: set up on the scratchpad's stack (the renderer drawing a reflection,
/// the other scratchpad buffer), then its facets drawn.
fn reflect(c: &mut Cpu, car: u32, view: u32, facet: u32) {
    let m = &mut c.m;
    let flags = m.r32(RENDER_FLAGS) | REFLECTION;
    let buffer = m.r32(BUFFER) ^ 1;
    m.w32(RENDER_FLAGS, flags);
    m.w32(BUFFER, buffer);
    // (the original's `SetSp` puts it on the scratchpad's stack, its top; `SetSp` back gives
    // that top, which stays the saved stack)
    c.m.w32(SAVED_SP, c.r[29]);
    c.m.w32(ON_SCRATCHPAD, 1);
    call_on_stack(c, SCRATCH_STACK, SET_UP_CAR, &[car, view]);
    c.m.w32(SAVED_SP, SCRATCH_STACK);
    c.m.w32(ON_SCRATCHPAD, 0);
    call_addr(c, INSERT_CAR, &[car, facet]);
    let m = &mut c.m;
    let flags = m.r32(RENDER_FLAGS) & !REFLECTION & 0xFF;
    let buffer = m.r32(BUFFER) ^ 1;
    m.w32(RENDER_FLAGS, flags);
    m.w32(BUFFER, buffer);
}

/// The function at `addr` called with `args` (four at most) with the stack pointer exactly at
/// `sp` (no argument area below it: as the original calls it right after switching stacks),
/// every register put back after.
fn call_on_stack(c: &mut Cpu, sp: u32, addr: u32, args: &[u32]) {
    let (regs, hi, lo) = (c.r, c.hi, c.lo);
    c.r[29] = sp;
    for (k, &a) in args.iter().enumerate() {
        c.r[4 + k] = a;
    }
    c.r[31] = 0;
    c.call(addr);
    if c.cont != 0 {
        c.run_cont();
    }
    (c.r, c.hi, c.lo) = (regs, hi, lo);
}

pub fn abi_draw_menu_rendering_view(c: &mut Cpu) {
    let a: Vec<u32> = (4..11).map(|k| crate::abi::arg(c, k)).collect();
    let v = CarView { player: a[0], spin: a[1], angle: a[2], distance: a[3], height: a[4], facet: a[5], reflection: a[6] != 0 };
    draw_menu_rendering_view(c, c.r[4], c.r[5], c.r[6], c.r[7], &v);
}

/// `Draw_EmptyView` (FE3DMENU.CPP, 0x80012f50, a static function): the car select's empty
/// `view` at (`x`, `y`) for player `k` (its spin turned by `spin`), turned by `angle` (the
/// turn's 4096ths), the camera `distance` back and `height` up; the shadow's colour grey, only
/// the showroom drawn.
#[allow(clippy::too_many_arguments)]
pub fn draw_empty_view(c: &mut Cpu, view: u32, x: u32, y: u32, k: u32, spin: u32, angle: u32, distance: u32, height: u32) {
    start_view(c, x, y, k, spin);
    with_buffer(c, 2 * MATRIX + VECTOR, |c, buffer| {
        let (turn, id, pos) = (buffer, buffer + MATRIX, buffer + 2 * MATRIX);
        call_addr(c, XFORM_Y, &[turn, angle]);
        place_view(c, view, turn, distance, height, id, pos);
    });
    call_addr(c, START_PROJECTION, &[]);
    c.m.w32(SHADOW, SHADOW_GREY);
    call_addr(c, SHOWROOM, &[view]);
}

pub fn abi_draw_empty_view(c: &mut Cpu) {
    let a: Vec<u32> = (4..8).map(|k| crate::abi::arg(c, k)).collect();
    draw_empty_view(c, c.r[4], c.r[5], c.r[6], c.r[7], a[0], a[1], a[2], a[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

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

}
