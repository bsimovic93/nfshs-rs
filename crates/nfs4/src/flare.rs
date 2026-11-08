//! FLARE.CPP: glows and flares. A flare is drawn round a screen place from a shape's points
//! (`Flare_gOct`: 8, `Flare_gHex`: 6, `Flare_gReflectHex`, `Flare_gQuad`: 4, `Flare_gSpikes`: 4)
//! projected by the GTE's rotation (which sizes and turns it): semi-transparent shaded triangles
//! from the centre's colour (`gfrgb`) out to black, spikes in a second colour (`gfrgb2`), light
//! beams as lines. A car's lights (`Flare_CarShapedHalo`) pick their flare by their kind
//! (`Flare_gType`: two colours, a size shift, flags) and distance; the sun's (`Flare_Sun`) is an
//! octagon or its own texture, with the lens flare; the moon's a texture.
//!
//! Nine of the file's functions are the host's hooks (the halos, the spikes, the single-colour
//! pieces, the lens flare: it records them) and stay the original's; these call them through
//! `call_jal`, which passes them to the host. Written readable from the start (step 3 of
//! DECOMP.md's plan): the game's data and the GTE's registers as the original leaves them, the
//! stack and CPU registers free. All of it runs in the race.

use crate::call::{call_jal, with_buffer};
use crate::gpu;
use ps1rt::{Cpu, Mem};

/// The colours (`gfrgb`: a flare's centre, `gfrgb2`: its spikes'), the size (`gscale`).
const RGB: u32 = 0x8013_EB54;
const RGB2: u32 = 0x8013_EB58;
const SCALE: u32 = 0x8013_EB5C;
/// The shapes' points (`SVECTOR`s) and the triangles' corners (shorts, indices of the points
/// projected): the octagon's (`gfOctPt1`, `gfOctPt2`), the hexagon's (`gfHexPt1`, `gfHexPt2`),
/// the spikes' (`gfSpikePt0..2`: their quads, indices among the spikes then the octagon's).
const OCT: u32 = 0x8012_1158;
const HEX: u32 = 0x8012_11D8;
const REFLECT_HEX: u32 = 0x8012_1208;
const QUAD: u32 = 0x8012_1238;
const SPIKES: u32 = 0x8012_1138;
const OCT_PT1: u32 = 0x8012_1100;
const OCT_PT2: u32 = 0x8012_1110;
const HEX_PT1: u32 = 0x8012_1120;
const HEX_PT2: u32 = 0x8012_112C;
const SPIKE_PT0: u32 = 0x8012_10D0;
const SPIKE_PT1: u32 = 0x8012_10E0;
const SPIKE_PT2: u32 = 0x8012_10F0;
/// Where the octagon's places start among the spikes' and its.
const OCT_FIRST: u32 = 4;
/// The lights' kinds (`Flare_gType`: 16 bytes each: the two colours, the size's shift, flags:
/// 1 dimmed as it turns away, 2 more so, 4 turned toward its other end, 8 turned by its screen
/// place, 0x40 dithered), the first of a race's (11; the showroom's 1), the textures
/// (`gFlarePixmap`).
const TYPES: u32 = 0x8012_1258;
const TYPE_SIZE: u32 = 16;
const RACE_TYPES: u32 = 11;
const MENU_TYPES: u32 = 1;
const PIXMAPS: u32 = 0x8011_3E40;
/// The lens flare (`gFlare_LensFlare`: +0x74 its size, +0x78 its pieces, +0x7C, +0x7D its state)
/// and the sun's pieces (`Flare_SunFlarePieces`).
const LENS_FLARE: u32 = 0x8012_1568;
const SUN_FLARE_PIECES: u32 = 0x8012_14D8;
/// The showroom's switch (`R3DCar_InMenu`), the reflections' offset (`DrawC_gReflectOffset`), the
/// two views' set-up (`GameSetup_gData + 0xC`), the projection's distance (`Camera_gGeomScreen`),
/// the ordering table's length (`Draw_gViewOtSize`), the track spec's sky flags (+0x5C: 0x100 the
/// sun its own texture, its colour at +0xBC).
const IN_MENU: u32 = 0x8013_E608;
const REFLECT_OFFSET: u32 = 0x8013_EAFC;
const COMM_MODE: u32 = 0x8011_44B0;
const GEOM_SCREEN: u32 = 0x8013_DAD0;
const VIEW_OT_SIZE: u32 = 0x8013_EA98;
const SKY_FLAGS: u32 = 0x8012_45C8;
const SUN_TEXTURE: u32 = 0x100;
const SUN_COLOR: u32 = 0x8012_4628;

/// The scratchpad's drawing (`dcache`): the ordering table (+0), the next primitive (+4), the
/// buffer's end (+8; a flare wants 1 KB spare).
const DCACHE: u32 = 0x1F80_0000;
const FLARE_ROOM: u32 = 1024;

/// A car light's flags (its first argument): the kind (`& 0x7F`), dimmed (0x80), a reflection
/// (0x100: on the road, below).
const DIMMED: u32 = 0x80;
const REFLECTION: u32 = 0x100;

/// Primitives: a shaded triangle (`POLY_G3`, 28 bytes), a flat quad (`POLY_F4`, 24), a shaded
/// quad (`POLY_G4`, 36), a textured one (`POLY_FT4`, 40), a shaded line (`LINE_G2`, 20), a draw
/// mode (12); their semi-transparent codes, black.
const G3_SIZE: u32 = 28;
const F4_SIZE: u32 = 24;
const G4_SIZE: u32 = 36;
const FT4_SIZE: u32 = 40;
const LINE_SIZE: u32 = 20;
const DRAW_MODE_SIZE: u32 = 12;
const G3_SEMI_BLACK: u32 = 0x3200_0000;
const G4_SEMI_BLACK: u32 = 0x3A00_0000;
const DRAW_MODE_TPAGE: u32 = 288;

/// The GTE: the rotation (control 0..4), the translation (5..7); vector 0 (data 0, 1), the
/// vectors in IR1..3 (data 9..11), the screen place (SXY2, data 14), the results (MAC1..3, data
/// 25..27). RTPS; MVMVA by the rotation of IR (12-bit shift, no translation).
const RT: usize = 32;
const TR: usize = 32 + 5;
const VXY0: usize = 0;
const IR1: usize = 9;
const SXY2: usize = 14;
const MAC1: usize = 25;
const RTPS: u32 = 0x18_0001;
const MVMVA_RT_IR: u32 = 0x49_E012;

/// Functions called: the library's `SetDrawMode(prim, dfe, dtd, tpage, rect)`, `RotMatrixZ(angle,
/// m)`, `VectorNormal(v, out)`, `isqrt`, `fixedatan(y, x)`; the host's hooks
/// `Flare_LensFlare`, `Flare_SingleColorTex(xy, colour, w, h, tex, ot)`.
const SET_DRAW_MODE: u32 = 0x800E_ED18;
const ROT_MATRIX_Z: u32 = 0x800F_3A88;
const VECTOR_NORMAL: u32 = 0x800F_2810;
const ISQRT: u32 = 0x800F_3C28;
const FIXED_ATAN: u32 = 0x800E_DE84;
const LENS_FLARE_HOOK: u32 = 0x800C_F8CC;
const SINGLE_COLOR_TEX: u32 = 0x800C_F458;

fn sh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// The ordering table entry `index`.
fn ot_entry(m: &mut Mem, index: u32) -> u32 {
    m.r32(DCACHE).wrapping_add(index << 2)
}

/// A primitive of `size` linked into the entry `index`, its length and code set.
fn prim(m: &mut Mem, index: u32, size: u32) -> u32 {
    let entry = ot_entry(m, index);
    gpu::add_prim_to(m, entry, size)
}

/// Vector 0 from the words at `v` projected: RTPS; its screen place.
fn project(c: &mut Cpu, v: u32) -> u32 {
    let (xy, z) = (c.m.r32(v), c.m.r32(v.wrapping_add(4)));
    c.gte_write(VXY0, xy);
    c.gte_write(VXY0 + 1, z);
    c.gte_command(RTPS);
    c.gte_read(SXY2)
}

/// The GTE's rotation from a `MATRIX`'s five words at `m`.
fn load_rotation(c: &mut Cpu, m: u32) {
    for k in 0..5 {
        let v = c.m.r32(m + 4 * k);
        c.gte_write(RT + k as usize, v);
    }
}

fn set_translation(c: &mut Cpu, t: [u32; 3]) {
    for (k, v) in t.into_iter().enumerate() {
        c.gte_write(TR + k, v);
    }
}

/// The `MATRIX` at `m` turned by the GTE's rotation, column by column (MVMVA of each column in
/// IR1..3), in place.
fn rotate_columns(c: &mut Cpu, m: u32) {
    for col in 0..3 {
        let at = m + 2 * col;
        for row in 0..3 {
            let v = c.m.r16(at + 6 * row) as u32;
            c.gte_write(IR1 + row as usize, v);
        }
        c.gte_command(MVMVA_RT_IR);
        for row in 0..3 {
            let v = c.gte_read(IR1 + row as usize);
            c.m.w16(at + 6 * row, v as u16);
        }
    }
}

/// `Flare_Tri` (FLARE.CPP, 0x800cca10): a triangle from `a`'s screen place through `centre`'s
/// (the flare's colour) to `b`'s, black at its ends, into the entry `ot` (its corners in the
/// primitive: `b`, the centre, `a`).
pub fn tri(m: &mut Mem, centre: u32, a: u32, b: u32, ot: u32) {
    let p = prim(m, ot, G3_SIZE);
    m.w32(p + 4, G3_SEMI_BLACK);
    let rgb = m.r32(RGB);
    m.w32(p + 20, 0);
    m.w8(p + 3, 6);
    m.w32(p + 12, rgb);
    for (at, from) in [(8, b), (16, centre), (24, a)] {
        let v = m.r32(from);
        m.w32(p + at, v);
    }
}

pub fn abi_tri(c: &mut Cpu) {
    tri(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Flare_SetMatrix` (FLARE.CPP, 0x800ccaac): the GTE's rotation from `matrix` (as
/// `Weather_SetMatrix`: its transpose, 16.16 to 4.12), no translation.
pub fn set_matrix(c: &mut Cpu, matrix: u32) {
    crate::weather::set_matrix(c, matrix);
}

pub fn abi_set_matrix(c: &mut Cpu) {
    set_matrix(c, c.r[4]);
}

/// `Flare_IdentMatrix` (FLARE.CPP, 0x800ccb58): the `MATRIX` at `m` the identity.
pub fn ident_matrix(m: &mut Mem, at: u32) {
    for (k, v) in [4096, 0, 4096, 0, 4096].into_iter().enumerate() {
        m.w32(at + 4 * k as u32, v);
    }
}

pub fn abi_ident_matrix(c: &mut Cpu) {
    ident_matrix(&mut c.m, c.r[4]);
}

/// A shape's points projected into `places` (screen places, a word each).
fn project_points(c: &mut Cpu, points: u32, n: u32, places: u32) {
    for k in 0..n {
        let p = project(c, points + 8 * k);
        c.m.w32(places + 4 * k, p);
    }
}

/// A fan of `n` triangles round `centre` (the last first) between the projected points the
/// corner tables name: their indices count from the start of the places (the octagon's after
/// the spikes' four, as `Flare_OctFlareSpikes` lays them out; the hexagon's from 0).
fn fan(c: &mut Cpu, centre: u32, points: u32, n: u32, first: u32, pt1: u32, pt2: u32, ot: u32) {
    with_buffer(c, 4 * (first + n), |c, places| {
        project_points(c, points, n, places + 4 * first);
        let m = &mut c.m;
        for k in (0..n).rev() {
            let (a, b) = (sh(m, pt1 + 2 * k) as u32, sh(m, pt2 + 2 * k) as u32);
            tri(m, centre, places.wrapping_add(a << 2), places.wrapping_add(b << 2), ot);
        }
    });
}

/// `Flare_OctFlare` (FLARE.CPP, 0x800ccb74): an octagon's flare round `centre`.
pub fn oct_flare(c: &mut Cpu, centre: u32, ot: u32) {
    fan(c, centre, OCT, 8, OCT_FIRST, OCT_PT1, OCT_PT2, ot);
}

pub fn abi_oct_flare(c: &mut Cpu) {
    oct_flare(c, c.r[4], c.r[5]);
}

/// `Flare_OctFlareSpikes` (FLARE.CPP, 0x800ccd94): an octagon's flare with spikes: eight quads
/// in the spikes' colour from the centre out to the spikes, each with its octagon triangle.
pub fn oct_flare_spikes(c: &mut Cpu, centre: u32, ot: u32) {
    with_buffer(c, 48, |c, places| {
        project_points(c, SPIKES, 4, places);
        project_points(c, OCT, 8, places + 4 * OCT_FIRST);
        let m = &mut c.m;
        let (rgb, rgb2, at) = (m.r32(RGB), m.r32(RGB2), m.r32(centre));
        let place = |m: &mut Mem, table: u32, k: u32| {
            let i = sh(m, table + 2 * k) as u32;
            m.r32(places.wrapping_add(i << 2))
        };
        for k in (0..8).rev() {
            let p = prim(m, ot, G4_SIZE);
            m.w32(p + 4, G4_SEMI_BLACK);
            m.w8(p + 3, 8);
            m.w32(p + 12, rgb2);
            m.w32(p + 20, 0);
            m.w32(p + 28, 0);
            m.w32(p + 16, at);
            let (a, b, d) = (place(m, SPIKE_PT0, k), place(m, SPIKE_PT2, k), place(m, SPIKE_PT1, k));
            m.w32(p + 8, a);
            m.w32(p + 24, b);
            m.w32(p + 32, d);
            let p = prim(m, ot, G3_SIZE);
            m.w32(p + 4, G3_SEMI_BLACK);
            m.w8(p + 3, 6);
            m.w32(p + 12, rgb);
            m.w32(p + 20, 0);
            m.w32(p + 16, at);
            let (a, b) = (place(m, OCT_PT1, k), place(m, OCT_PT2, k));
            m.w32(p + 8, a);
            m.w32(p + 24, b);
        }
    });
}

pub fn abi_oct_flare_spikes(c: &mut Cpu) {
    oct_flare_spikes(c, c.r[4], c.r[5]);
}

/// `Flare_HexFlare` (FLARE.CPP, 0x800cd3f4): a hexagon's flare round `centre`.
pub fn hex_flare(c: &mut Cpu, centre: u32, ot: u32) {
    fan(c, centre, HEX, 6, 0, HEX_PT1, HEX_PT2, ot);
}

pub fn abi_hex_flare(c: &mut Cpu) {
    hex_flare(c, c.r[4], c.r[5]);
}

/// `Flare_ReflectHexFlare` (FLARE.CPP, 0x800cd5c8): a reflection's hexagon round `centre`.
pub fn reflect_hex_flare(c: &mut Cpu, centre: u32, ot: u32) {
    fan(c, centre, REFLECT_HEX, 6, 0, HEX_PT1, HEX_PT2, ot);
}

pub fn abi_reflect_hex_flare(c: &mut Cpu) {
    reflect_hex_flare(c, c.r[4], c.r[5]);
}

/// `Flare_QuadFlare` (FLARE.CPP, 0x800cd79c): a quad's flare round `centre`: four triangles
/// between its corners in turn.
pub fn quad_flare(c: &mut Cpu, centre: u32, ot: u32) {
    with_buffer(c, 12, |c, at| {
        let (q2, q1, q0) = (at, at + 4, at + 8);
        let p = project(c, QUAD);
        c.m.w32(q0, p);
        let p = project(c, QUAD + 8);
        c.m.w32(q1, p);
        tri(&mut c.m, centre, q0, q1, ot);
        let p = project(c, QUAD + 16);
        c.m.w32(q2, p);
        tri(&mut c.m, centre, q1, q2, ot);
        // (the fourth corner where the second was)
        let p = project(c, QUAD + 24);
        c.m.w32(q1, p);
        tri(&mut c.m, centre, q2, q1, ot);
        tri(&mut c.m, centre, q1, q0, ot);
    });
}

pub fn abi_quad_flare(c: &mut Cpu) {
    quad_flare(c, c.r[4], c.r[5]);
}

/// `Flare_CarShapedHalo` (FLARE.CPP, 0x800cd8b8): a car light's flare at `pos` (an `SVECTOR`;
/// a reflection's mirrored below the road first) between the light's edge points `p1` and `p2`
/// (`side` picks the one it turns toward and the edge it is sized by), if the view's primitives
/// have room and it is ahead (within 45 degrees, nearer than 128 not): sized by its kind's shift
/// and the edge's length, turned and dimmed as its kind says, a reflection's hexagon, an
/// octagon with spikes near, a hexagon with beams further, a quad far; dithered as its kind says.
#[allow(clippy::too_many_arguments)]
pub fn car_shaped_halo(c: &mut Cpu, flags: u32, pos: u32, p1: u32, p2: u32, side: i16, ot: u32, dcache: u32) {
    let m = &mut c.m;
    if m.r32(dcache + 4) >= m.r32(dcache + 8).wrapping_sub(FLARE_ROOM) {
        return;
    }
    // the edges from the light's place (as unsigned halves' differences), the normal across them
    let diff = |m: &mut Mem, p: u32, k: u32| m.r16(p + 2 * k) as i32 - m.r16(pos + 2 * k) as i32;
    let e1: [i32; 3] = std::array::from_fn(|k| diff(m, p1, k as u32));
    let e2: [i32; 3] = std::array::from_fn(|k| diff(m, p2, k as u32));
    let normal = [
        e2[1].wrapping_mul(e1[2]).wrapping_sub(e2[2].wrapping_mul(e1[1])),
        e2[2].wrapping_mul(e1[0]).wrapping_sub(e2[0].wrapping_mul(e1[2])),
        e2[0].wrapping_mul(e1[1]).wrapping_sub(e2[1].wrapping_mul(e1[0])),
    ];
    let toward = if side < 0 { p1 } else { p2 };
    let base = if m.r32(IN_MENU) != 0 { MENU_TYPES } else { RACE_TYPES };
    let kind = (flags & 0x7F) + base;
    if flags & REFLECTION != 0 {
        let y = m.r16(REFLECT_OFFSET).wrapping_neg().wrapping_sub(m.r16(pos + 2));
        m.w16(pos + 2, y);
    }
    project(c, pos);
    let m = &mut c.m;
    let entry = TYPES + TYPE_SIZE * kind;
    let mut colors: [u32; 2] = [m.r32(entry), m.r32(entry + 4)];
    let (shift, how) = (m.r32(entry + 8), m.r32(entry + 12));
    if flags & DIMMED != 0 {
        let halve = |v: u32| (v & 0xFF00_0000) | ((v & 0xFE_FEFE) >> 1);
        colors = colors.map(halve);
    }
    let mac: [i32; 3] = std::array::from_fn(|k| c.gte_read(MAC1 + k) as i32);
    let (x, depth) = (mac[0], mac[2]);
    if depth < x || depth < x.wrapping_neg() || depth < 128 {
        return;
    }
    let place = c.gte_read(SXY2);
    let (sx, sy) = (place as i16 as i32, (place >> 16) as i16 as i32);
    let roll = if how & 8 != 0 { sx.wrapping_add(sy) << 2 } else { 0 };
    let (mut turn, mut reach) = (0, 0);
    if how & 4 != 0 {
        let other = project(c, toward);
        let (dy, dx) = (((other >> 16) as i16 as i32).wrapping_sub(sy), (other as i16 as i32).wrapping_sub(sx));
        turn = (call_jal(c, FIXED_ATAN, &[dy as u32, dx as u32]) as i32) >> 4;
        let root = call_jal(c, ISQRT, &[dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) as u32]) as i32;
        reach = root.wrapping_mul(depth);
    }
    let m = &mut c.m;
    m.w32(RGB, colors[0]);
    m.w32(RGB2, colors[1]);
    let edge = if side >= 0 { e1 } else { e2 };
    let length = edge.iter().map(|&e| (e as i16 as i32).wrapping_mul(e as i16 as i32)).fold(0i32, |a, b| a.wrapping_add(b));
    m.w32(SCALE, (length >> 1) as u32);
    let size = (m.r32(SCALE) as i32).wrapping_shl(shift);
    m.w32(SCALE, size as u32);
    if how & 1 != 0 {
        // dimmed as the light turns away: its normal's depth (turned, unit length) past 600
        set_translation(c, [0; 3]);
        with_buffer(c, 32, |c, at| {
            c.m.w16(at, normal[0] as u16);
            c.m.w16(at + 2, normal[1] as u16);
            c.m.w16(at + 4, normal[2] as u16);
            c.m.w16(at + 6, 0);
            project(c, at);
            let v = at + 8;
            for k in 0..3 {
                let r = c.gte_read(MAC1 + k);
                c.m.w32(v + 4 * k as u32, r);
            }
            let unit = at + 20;
            call_jal(c, VECTOR_NORMAL, &[v, unit]);
            let mut facing = c.m.r32(unit + 8) as i32;
            if how & 2 != 0 {
                facing = facing.wrapping_sub(3891).wrapping_mul(20);
            }
            let facing = facing.wrapping_sub(600).max(0);
            let scaled = (c.m.r32(SCALE) as i32).wrapping_mul(facing) >> 11;
            c.m.w32(SCALE, scaled as u32);
        });
    }
    if flags & REFLECTION != 0 {
        // a reflection: half as bright, its hexagon squashed flat and turned half round
        let saved = c.m.r32(RGB);
        let v = size.wrapping_add(reach >> 7) as u32;
        let rgb = saved;
        let half = (rgb & 0xFF00_0000) | ((rgb & 0xFE_FEFE) >> 1);
        c.m.w32(RGB, half);
        with_buffer(c, 40, |c, mats| {
            let (scale_m, turn_m) = (mats, mats + 20);
            for (k, w) in [v, 0, v, 0, 0].into_iter().enumerate() {
                c.m.w32(scale_m + 4 * k as u32, w);
            }
            ident_matrix(&mut c.m, turn_m);
            set_translation(c, [mac[0] as u32, mac[1] as u32, mac[2] as u32]);
            call_jal(c, ROT_MATRIX_Z, &[2048, turn_m]);
            load_rotation(c, turn_m);
            rotate_columns_into(c, scale_m, turn_m);
            load_rotation(c, turn_m);
        });
        with_buffer(c, 8, |c, at| {
            c.m.w32(at, place);
            reflect_hex_flare(c, at, ot);
        });
        c.m.w32(RGB, saved);
    } else {
        let scale = c.m.r32(SCALE);
        let wide = scale.wrapping_add((reach >> 5) as u32);
        with_buffer(c, 60, |c, mats| {
            let (roll_m, turn_m, scale_m) = (mats, mats + 20, mats + 40);
            for (k, w) in [wide, 0, scale, 0, 0].into_iter().enumerate() {
                c.m.w32(scale_m + 4 * k as u32, w);
            }
            ident_matrix(&mut c.m, roll_m);
            ident_matrix(&mut c.m, turn_m);
            set_translation(c, [mac[0] as u32, mac[1] as u32, mac[2] as u32]);
            call_jal(c, ROT_MATRIX_Z, &[roll as u32, roll_m]);
            call_jal(c, ROT_MATRIX_Z, &[turn as u32, turn_m]);
            load_rotation(c, turn_m);
            rotate_columns(c, scale_m);
            load_rotation(c, scale_m);
            rotate_columns(c, roll_m);
            load_rotation(c, roll_m);
        });
        with_buffer(c, 8, |c, at| {
            c.m.w32(at, place);
            if depth < 3200 {
                oct_flare_spikes(c, at, ot);
            } else if depth < 7040 {
                hex_flare(c, at, ot);
                pre_calc_hex_light_beam(c, at, ot);
            } else {
                quad_flare(c, at, ot);
            }
        });
    }
    let p = prim(&mut c.m, ot, DRAW_MODE_SIZE);
    call_jal(c, SET_DRAW_MODE, &[p, 0, (how & 0x40 != 0) as u32, DRAW_MODE_TPAGE, 0]);
}

/// The `MATRIX` at `m` turned by the GTE's rotation column by column, the result to `out` (the
/// rotation's own place in memory, its new value).
fn rotate_columns_into(c: &mut Cpu, m: u32, out: u32) {
    for col in 0..3 {
        for row in 0..3 {
            let v = c.m.r16(m + 2 * col + 6 * row) as u32;
            c.gte_write(IR1 + row as usize, v);
        }
        c.gte_command(MVMVA_RT_IR);
        for row in 0..3 {
            let v = c.gte_read(IR1 + row as usize);
            c.m.w16(out + 2 * col + 6 * row, v as u16);
        }
    }
}

pub fn abi_car_shaped_halo(c: &mut Cpu) {
    let sp = c.r[29];
    let (side, ot, dcache) = (c.m.r32(sp + 16) as i16, c.m.r32(sp + 20), c.m.r32(sp + 24));
    car_shaped_halo(c, c.r[4], c.r[5], c.r[6], c.r[7], side, ot, dcache);
}

/// A shaded line from `from`'s screen place (the spikes' colour) to `to` (black).
fn beam(m: &mut Mem, from: u32, to: u32, ot: u32) {
    let p = prim(m, ot, LINE_SIZE);
    let rgb2 = m.r32(RGB2);
    m.w8(p + 3, 4);
    m.w32(p + 12, 0);
    m.w32(p + 4, rgb2);
    m.w8(p + 7, 0x52);
    m.w32(p + 8, from);
    m.w32(p + 16, to);
}

/// `Flare_2DSpike` (FLARE.CPP, 0x800cec24): a spike from `a`'s screen place to `b`'s.
pub fn spike_2d(m: &mut Mem, a: u32, b: u32, ot: u32) {
    let (from, to) = (m.r32(a), m.r32(b));
    beam(m, from, to, ot);
}

pub fn abi_spike_2d(c: &mut Cpu) {
    spike_2d(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `Flare_PreCalcHexLightBeam` (FLARE.CPP, 0x800cf0ac): eight beams from `centre` out to the
/// octagon's points.
pub fn pre_calc_hex_light_beam(c: &mut Cpu, centre: u32, ot: u32) {
    let from = c.m.r32(centre);
    for k in 0..8 {
        let to = project(c, OCT + 8 * k);
        beam(&mut c.m, from, to, ot);
    }
}

pub fn abi_pre_calc_hex_light_beam(c: &mut Cpu) {
    pre_calc_hex_light_beam(c, c.r[4], c.r[5]);
}

/// A flat quad of `colour` (its code `code`) at the four screen places at `places`.
fn flat_quad(m: &mut Mem, places: u32, colour: u32, ot: u32, code: u8) {
    let p = prim(m, ot, F4_SIZE);
    let rgb = m.r32(colour);
    m.w8(p + 3, 5);
    m.w32(p + 4, rgb);
    m.w8(p + 7, code);
    for k in 0..4 {
        let v = m.r32(places + 4 * k);
        m.w32(p + 8 + 4 * k, v);
    }
}

/// `Flare_Quad` (FLARE.CPP, 0x800cf180): a semi-transparent flat quad.
pub fn quad(m: &mut Mem, places: u32, colour: u32, ot: u32) {
    flat_quad(m, places, colour, ot, 0x2A);
}

pub fn abi_quad(c: &mut Cpu) {
    quad(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `Flare_QuadNotTransparent` (FLARE.CPP, 0x800cf224): an opaque flat quad.
pub fn quad_not_transparent(m: &mut Mem, places: u32, colour: u32, ot: u32) {
    flat_quad(m, places, colour, ot, 0x28);
}

pub fn abi_quad_not_transparent(c: &mut Cpu) {
    quad_not_transparent(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `Flare_QuadRing` (FLARE.CPP, 0x800cf2c8): a ring's piece: a shaded quad, `colour` at its first
/// and third corners, black at the others.
pub fn quad_ring(m: &mut Mem, places: u32, colour: u32, ot: u32) {
    let p = prim(m, ot, G4_SIZE);
    let rgb = m.r32(colour);
    m.w32(p + 12, 0);
    m.w32(p + 4, rgb);
    m.w8(p + 3, 8);
    m.w32(p + 28, 0);
    m.w8(p + 7, 0x3A);
    let rgb = m.r32(colour);
    m.w32(p + 20, rgb);
    for k in 0..4 {
        let v = m.r32(places + 4 * k);
        m.w32(p + 8 + 8 * k, v);
    }
}

pub fn abi_quad_ring(c: &mut Cpu) {
    quad_ring(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `Flare_TextureQuad` (FLARE.CPP, 0x800cf37c): a semi-transparent textured quad of `colour`, the
/// texture `tex`'s.
pub fn texture_quad(m: &mut Mem, places: u32, colour: u32, tex: u8, ot: u32) {
    let p = prim(m, ot, FT4_SIZE);
    let rgb = m.r32(colour);
    m.w8(p + 3, 9);
    m.w32(p + 4, rgb);
    m.w8(p + 7, 0x2E);
    for k in 0..4 {
        let v = m.r32(places + 4 * k);
        m.w32(p + 8 + 8 * k, v);
    }
    let pmx = m.r32(PIXMAPS + 4 * tex as u32);
    for k in 0..4 {
        let uv = m.r32(pmx.wrapping_add(4 * k));
        m.w32(p + 12 + 8 * k, uv);
    }
}

pub fn abi_texture_quad(c: &mut Cpu) {
    texture_quad(&mut c.m, c.r[4], c.r[5], c.r[6] as u8, c.r[7]);
}

/// `Flare_InitLensFlare` (FLARE.CPP, 0x800cf8a4): the lens flare at rest, the sun's pieces.
pub fn init_lens_flare(m: &mut Mem) {
    m.w32(LENS_FLARE + 0x74, 80);
    m.w32(LENS_FLARE + 0x78, SUN_FLARE_PIECES);
    m.w8(LENS_FLARE + 0x7D, 0);
    m.w8(LENS_FLARE + 0x7C, 0);
}

pub fn abi_init_lens_flare(c: &mut Cpu) {
    init_lens_flare(&mut c.m);
}

/// The views' screen centre line (one view, two).
fn half_height(m: &mut Mem) -> i32 {
    if m.r32(COMM_MODE) == 1 { 60 } else { 120 }
}

/// A sky body at `pos` projected: its screen place with its height a quarter down from the
/// centre line (in `at`), whether it is ahead (within 45 degrees); MAC1..3.
fn sky_place(c: &mut Cpu, pos: u32, at: u32) -> (bool, [i32; 3]) {
    let centre = half_height(&mut c.m);
    let place = project(c, pos);
    let mac: [i32; 3] = std::array::from_fn(|k| c.gte_read(MAC1 + k) as i32);
    c.m.w32(at, place);
    c.m.w16(at + 2, ((mac[1] >> 2) + centre) as u16);
    (!(mac[2] < mac[0] || mac[2] < mac[0].wrapping_neg()), mac)
}

/// `Flare_Sun` (FLARE.CPP, 0x800cff30): the sun at `pos` (if the view's primitives have room and
/// it is ahead): the lens flare (on a single view, the sun not too far right, unless it is a
/// texture), then the sun grey (as its own texture or an octagon's flare) at the back of the
/// ordering table, between two draw modes, the GTE set to draw it at its place on the screen.
pub fn sun(c: &mut Cpu, pos: u32, dcache: u32) {
    let m = &mut c.m;
    let centre = half_height(m);
    if m.r32(dcache + 4) >= m.r32(dcache + 8).wrapping_sub(FLARE_ROOM) {
        return;
    }
    let place = project(c, pos);
    let mac: [i32; 3] = std::array::from_fn(|k| c.gte_read(MAC1 + k) as i32);
    if mac[2] < mac[0] || mac[2] < mac[0].wrapping_neg() {
        return;
    }
    with_buffer(c, 8, |c, at| {
        c.m.w32(at, place);
        let y = ((mac[1] >> 2) + centre) as i16;
        c.m.w16(at + 2, y as u16);
        let m = &mut c.m;
        let split = m.r32(COMM_MODE) == 1;
        let x = place as i16 as i32;
        let geom = ((m.r32(GEOM_SCREEN) << 2) as i32).min(2896) as u32;
        let lift = (y as i32 - if split { 60 } else { 120 }) << 2;
        set_translation(c, [((x - 160) << 2) as u32, lift as u32, geom]);
        if x < 317 && !split && c.m.r32(SKY_FLAGS) & SUN_TEXTURE == 0 {
            let mode = c.m.r32(COMM_MODE);
            call_jal(c, LENS_FLARE_HOOK, &[at, dcache, x as u32, mode]);
        }
        c.m.w32(RGB, 0x80_8080);
        let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
        let p = prim(&mut c.m, back, DRAW_MODE_SIZE);
        call_jal(c, SET_DRAW_MODE, &[p, 0, 0, DRAW_MODE_TPAGE, 0]);
        // drawn flat at a quarter size
        for (k, v) in [1024u32, 0, 1024, 0, 0].into_iter().enumerate() {
            c.gte_write(RT + k, v);
        }
        if c.m.r32(SKY_FLAGS) & SUN_TEXTURE != 0 {
            call_jal(c, SINGLE_COLOR_TEX, &[at, SUN_COLOR, 16, 16, 0, back]);
        } else {
            oct_flare(c, at, back);
        }
        let p = prim(&mut c.m, back, DRAW_MODE_SIZE);
        call_jal(c, SET_DRAW_MODE, &[p, 0, 0, DRAW_MODE_TPAGE, 0]);
    });
}

pub fn abi_sun(c: &mut Cpu) {
    sun(c, c.r[4], c.r[5]);
}

/// `Flare_Moon` (FLARE.CPP, 0x800d021c): the moon at `pos`, grey, its texture at the back of the
/// ordering table, if it is ahead.
pub fn moon(c: &mut Cpu, pos: u32, _dcache: u32) {
    with_buffer(c, 16, |c, buf| {
        let (at, grey) = (buf, buf + 8);
        c.m.w32(grey, 0x80_8080);
        let (ahead, _) = sky_place(c, pos, at);
        if ahead {
            let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
            call_jal(c, SINGLE_COLOR_TEX, &[at, grey, 16, 16, 0, back]);
        }
    });
}

pub fn abi_moon(c: &mut Cpu) {
    moon(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// The views' primitives' end (`Draw_gMaxPrim`) and the scratchpad's.
    const MAX_PRIM: u32 = 0x8013_EAA8;
    const PRIM_END: u32 = DCACHE + 8;

    /// Free RAM (the race leaves it zero): the GTE's registers before and after a run, the
    /// scratchpad after it, screen places, colours, a matrix, places in the world, a light's
    /// edges; the ordering table and primitives (below the stack).
    const GTE_IN: u32 = 0x8014_4000;
    const GTE_OUT: u32 = 0x8014_4100;
    const SCRATCH_OUT: u32 = 0x8014_4200;
    const SCRATCH_SIZE: u32 = 0x100;
    const PLACES: u32 = 0x8014_0000;
    const COLOUR: u32 = 0x8014_0040;
    const MATRIX: u32 = 0x8014_0080;
    const POS: u32 = 0x8014_0100;
    const P1: u32 = 0x8014_0110;
    const P2: u32 = 0x8014_0120;
    const OT_BASE: u32 = 0x801E_2A00;
    const PRIMS: u32 = 0x801E_3000;

    fn load_gte(c: &mut Cpu) {
        for r in 0..64 {
            c.gte_regs[r] = c.m.r32(GTE_IN + 4 * r as u32);
        }
    }

    fn dump(c: &mut Cpu) {
        for r in 0..64 {
            let v = c.gte_read(r);
            c.m.w32(GTE_OUT + 4 * r as u32, v);
        }
        for k in (0..SCRATCH_SIZE).step_by(4) {
            let v = c.m.r32(DCACHE + k);
            c.m.w32(SCRATCH_OUT + k, v);
        }
    }

    fn gte_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        load_gte(c);
        let v = via_abi(c, a);
        dump(c);
        v
    }

    fn pick_i(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
        lo + pick(rng, (hi - lo) as u32) as i32
    }

    /// The race RAM made any flare's: the colours and size any; screen places, a colour, a
    /// matrix; a light ahead of the camera (near, further, far) or behind or aside, its edges
    /// about it; the showroom or the race, one view or two, the sun its own texture or not, the
    /// projection's distance; the drawing's ordering table at free RAM, its buffer with room or
    /// not; the GTE: a turn near the identity (or any), the camera somewhere before the light,
    /// the screen as the race has it.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        for at in [RGB, RGB2, SCALE] {
            m.w32(at, rng.next() as u32);
        }
        for k in 0..16 {
            m.w32(PLACES + 4 * k, (pick(rng, 400) | pick(rng, 300) << 16) as u32);
            m.w32(COLOUR + 4 * k, rng.next() as u32);
        }
        for k in 0..9 {
            m.w32(MATRIX + 4 * k, pick_i(rng, -0x1_0000, 0x1_0000) as u32);
        }
        for k in 0..3 {
            let p = pick_i(rng, -300, 300);
            m.w16(POS + 2 * k, p as u16);
            m.w16(P1 + 2 * k, (p + pick_i(rng, -200, 200)) as u16);
            m.w16(P2 + 2 * k, (p + pick_i(rng, -200, 200)) as u16);
        }
        m.w32(IN_MENU, pick(rng, 2));
        m.w16(REFLECT_OFFSET, pick_i(rng, -100, 100) as u16);
        m.w32(COMM_MODE, (pick(rng, 3) == 0) as u32);
        let flags = m.r32(SKY_FLAGS) & !SUN_TEXTURE;
        m.w32(SKY_FLAGS, flags | if pick(rng, 3) == 0 { SUN_TEXTURE } else { 0 });
        m.w32(GEOM_SCREEN, [256, 724, 725, pick(rng, 1000)][pick(rng, 4) as usize]);
        // the drawing
        let ot_size = 2 + pick(rng, 60);
        m.w32(VIEW_OT_SIZE, ot_size);
        m.w32(DCACHE, OT_BASE);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        let room = if pick(rng, 6) == 0 { pick(rng, 1100) } else { 0x8000 };
        m.w32(PRIM_END, PRIMS + room);
        m.w32(MAX_PRIM, PRIMS + 0x8000);
        for k in 0..64 {
            m.w32(OT_BASE + 4 * k, rng.next() as u32);
        }
        // the GTE
        for r in 0..64 {
            m.w32(GTE_IN + 4 * r, rng.next() as u32);
        }
        if pick(rng, 4) != 0 {
            let r: [i32; 9] = std::array::from_fn(|i| if i % 4 == 0 { 4096 - pick_i(rng, 0, 600) } else { pick_i(rng, -600, 600) });
            let r = r.map(|v| v as u16 as u32);
            for (k, v) in [r[0] | r[1] << 16, r[2] | r[3] << 16, r[4] | r[5] << 16, r[6] | r[7] << 16, r[8]].into_iter().enumerate() {
                m.w32(GTE_IN + 4 * (32 + k as u32), v);
            }
            let depth = [pick_i(rng, 100, 3200), pick_i(rng, 3200, 7040), pick_i(rng, 7040, 16000), pick_i(rng, -2000, 200)][pick(rng, 4) as usize];
            let side = depth.abs() + 200;
            for (k, v) in [pick_i(rng, -side, side), pick_i(rng, -side, side), depth].into_iter().enumerate() {
                m.w32(GTE_IN + 4 * (37 + k as u32), v as u32);
            }
        }
        // (now and then exactly: no turn, the place's depth on the boundaries, ahead at 45
        // degrees)
        if pick(rng, 4) == 0 {
            for (k, v) in [4096u32, 0, 4096, 0, 4096].into_iter().enumerate() {
                m.w32(GTE_IN + 4 * (32 + k as u32), v);
            }
            let (x, z) = (sh(m, POS), sh(m, POS + 4));
            let depth = [127, 128, 3199, 3200, 7039, 7040, pick_i(rng, 0, 9000)][pick(rng, 7) as usize];
            let tx = if pick(rng, 3) == 0 { depth - x } else { pick_i(rng, -500, 500) };
            for (k, v) in [tx, pick_i(rng, -500, 500), depth - z].into_iter().enumerate() {
                m.w32(GTE_IN + 4 * (37 + k as u32), v as u32);
            }
        }
        m.w32(GTE_IN + 4 * (32 + 24), 160 << 16);
        m.w32(GTE_IN + 4 * (32 + 25), 120 << 16);
        m.w32(GTE_IN + 4 * (32 + 26), 256);
        // (the lights' kinds any now and then: their flags all ways; the lens flare's state)
        if pick(rng, 2) == 0 {
            for k in 0..RACE_TYPES + 16 {
                let e = TYPES + TYPE_SIZE * k;
                m.w32(e, rng.next() as u32);
                m.w32(e + 4, rng.next() as u32);
                m.w32(e + 8, pick(rng, 5));
                m.w32(e + 12, rng.next() as u32 & 0x4F);
            }
        }
        for k in 0x74..0x80 {
            m.w8(LENS_FLARE + k, rng.next() as u8);
        }
    }

    fn ot(r: &mut Rng) -> u32 {
        pick(r, 60)
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Flare_Tri", 0x800c_ca10, |c| { load_gte(c); nfs4rc::f_800cca10(c); dump(c) }, |_, r| vec![PLACES, PLACES + 4, PLACES + 8, ot(r)]),
            ("Flare_SetMatrix", 0x800c_caac, |c| { load_gte(c); nfs4rc::f_800ccaac(c); dump(c) }, |_, _| vec![MATRIX]),
            ("Flare_IdentMatrix", 0x800c_cb58, |c| { load_gte(c); nfs4rc::f_800ccb58(c); dump(c) }, |_, _| vec![MATRIX]),
            ("Flare_OctFlare", 0x800c_cb74, |c| { load_gte(c); nfs4rc::f_800ccb74(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_OctFlareSpikes", 0x800c_cd94, |c| { load_gte(c); nfs4rc::f_800ccd94(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_HexFlare", 0x800c_d3f4, |c| { load_gte(c); nfs4rc::f_800cd3f4(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_ReflectHexFlare", 0x800c_d5c8, |c| { load_gte(c); nfs4rc::f_800cd5c8(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_QuadFlare", 0x800c_d79c, |c| { load_gte(c); nfs4rc::f_800cd79c(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_CarShapedHalo", 0x800c_d8b8, |c| { load_gte(c); nfs4rc::f_800cd8b8(c); dump(c) }, |_, r| {
                let flags = pick(r, 12) | [0, DIMMED, REFLECTION, DIMMED | REFLECTION][pick(r, 4) as usize];
                vec![flags, POS, P1, P2, pick_i(r, -2, 2) as u32, ot(r), DCACHE]
            }),
            ("Flare_2DSpike", 0x800c_ec24, |c| { load_gte(c); nfs4rc::f_800cec24(c); dump(c) }, |_, r| vec![PLACES, PLACES + 4, ot(r)]),
            ("Flare_PreCalcHexLightBeam", 0x800c_f0ac, |c| { load_gte(c); nfs4rc::f_800cf0ac(c); dump(c) }, |_, r| vec![PLACES, ot(r)]),
            ("Flare_Quad", 0x800c_f180, |c| { load_gte(c); nfs4rc::f_800cf180(c); dump(c) }, |_, r| vec![PLACES, COLOUR, ot(r)]),
            ("Flare_QuadNotTransparent", 0x800c_f224, |c| { load_gte(c); nfs4rc::f_800cf224(c); dump(c) }, |_, r| vec![PLACES, COLOUR, ot(r)]),
            ("Flare_QuadRing", 0x800c_f2c8, |c| { load_gte(c); nfs4rc::f_800cf2c8(c); dump(c) }, |_, r| vec![PLACES, COLOUR, ot(r)]),
            ("Flare_TextureQuad", 0x800c_f37c, |c| { load_gte(c); nfs4rc::f_800cf37c(c); dump(c) }, |_, r| vec![PLACES, COLOUR, pick(r, 4), ot(r)]),
            ("Flare_InitLensFlare", 0x800c_f8a4, |c| { load_gte(c); nfs4rc::f_800cf8a4(c); dump(c) }, |_, _| vec![]),
            ("Flare_Sun", 0x800c_ff30, |c| { load_gte(c); nfs4rc::f_800cff30(c); dump(c) }, |_, _| vec![POS, DCACHE]),
            ("Flare_Moon", 0x800d_021c, |c| { load_gte(c); nfs4rc::f_800d021c(c); dump(c) }, |_, _| vec![POS, DCACHE]),
        ]
    }

    #[test]
    fn flare_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x6A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0x6A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), gte_port);
        }
    }
}
