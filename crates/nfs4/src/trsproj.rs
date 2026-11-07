//! TRSPROJ.CPP: the GTE's set-up for drawing: the screen's centre and projection distance, the
//! camera's turn (a `matrixtdef`, 16.16, as the GTE's 4.12 matrix), its translation, and points
//! through them. World positions are in 16.16; the GTE works in 1024ths of them (`>> 10`).
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data and the GTE's
//! registers as the original leaves them, the stack and CPU registers free.

use crate::call::{call_addr, with_buffer};
use crate::globals::GAME_SETUP;
use ps1rt::{Cpu, Mem};

/// The translations' precision (`TrsProj_precision`: bits dropped), and its usual value.
const PRECISION: u32 = 0x8013_D83C + 5696;
const PRECISION_DEFAULT: u32 = 10;
/// The projection distance (`Camera_gGeomScreen`), and the menus'.
const GEOM_SCREEN: u32 = 0x8013_DAD0;
const MENU_GEOM_SCREEN: u32 = 512;
/// The screen: 320 wide, 240 high (120 a player with two).
const SCREEN_W: u32 = 320;
const SCREEN_H: u32 = 240;
const SCREEN_H_SPLIT: u32 = 120;
const SPLIT_SCREEN: i32 = 1;
/// A vertex's projection flag in `TransformProjectVertex`'s output.
const VERTEX_FLAG: u16 = 10;

/// The library's `SetGeomScreen(h)`, `transform(in, matrix, out)` (a point through a
/// `matrixtdef`, 16.16).
const SET_GEOM_SCREEN: u32 = 0x800E_B4F8;
const TRANSFORM: u32 = 0x800E_B834;

/// The GTE's registers used: the rotation matrix (control 0..4), the translation (5..7), the
/// screen offset (24, 25) and distance (26); the vector (data 0, 1) and the results (MAC1..3,
/// data 25..27); MVMVA with the rotation, the translation and vector 0, the results >> 12.
const RT: usize = 32;
const TR: usize = 32 + 5;
const OFX: usize = 32 + 24;
const OFY: usize = 32 + 25;
const H: usize = 32 + 26;
const VXY0: usize = 0;
const VZ0: usize = 1;
const MAC1: usize = 25;
const MVMVA_RT_V0_TR: u32 = 0x48_0012;

/// The translation (16.16) in the GTE's units.
fn set_translation(c: &mut Cpu, t: [i32; 3]) {
    for (k, v) in t.into_iter().enumerate() {
        c.gte_write(TR + k, v as u32);
    }
}

/// `v` (shorts) through the rotation and translation: MAC1..3.
fn rotate_translate(c: &mut Cpu, v: [i16; 3]) -> [i32; 3] {
    c.gte_write(VXY0, (v[0] as u16 as u32) | (v[1] as u16 as u32) << 16);
    c.gte_write(VZ0, v[2] as u16 as u32);
    c.gte_command(MVMVA_RT_V0_TR);
    std::array::from_fn(|k| c.gte_read(MAC1 + k) as i32)
}

fn coord(m: &mut Mem, at: u32) -> [i32; 3] {
    std::array::from_fn(|k| m.r32(at.wrapping_add(4 * k as u32)) as i32)
}

/// `TrsProj_SetTransPrecision` (TRSPROJ.CPP, 0x800e2538).
pub fn set_trans_precision(m: &mut Mem, precision: u32) {
    m.w32(PRECISION, precision);
}

pub fn abi_set_trans_precision(c: &mut Cpu) {
    set_trans_precision(&mut c.m, c.r[4]);
}

/// `TrsProj_ResetTransPrecision` (TRSPROJ.CPP, 0x800e2544).
pub fn reset_trans_precision(m: &mut Mem) {
    m.w32(PRECISION, PRECISION_DEFAULT);
}

pub fn abi_reset_trans_precision(c: &mut Cpu) {
    reset_trans_precision(&mut c.m);
}

/// The screen's centre: of the rectangle at (x, y), w by h.
fn set_offset(c: &mut Cpu, x: i32, y: i32, w: i32, h: i32) {
    c.gte_write(OFX, (x.wrapping_add(w / 2) as u32) << 16);
    c.gte_write(OFY, (y.wrapping_add(h / 2) as u32) << 16);
}

/// `TrsProj_SetProjection` (TRSPROJ.CPP, 0x800e2554): the race's projection distance, the
/// screen's centre that of the rectangle (x, y, w, h).
pub fn set_projection(c: &mut Cpu, x: i32, y: i32, w: i32, h: i32) {
    let h_screen = c.m.r32(GEOM_SCREEN);
    call_addr(c, SET_GEOM_SCREEN, &[h_screen]);
    set_offset(c, x, y, w, h);
}

pub fn abi_set_projection(c: &mut Cpu) {
    set_projection(c, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `TrsProj_SetMenuProjection` (TRSPROJ.CPP, 0x800e25d4): the menus' projection distance, the
/// screen's centre that of the rectangle (x, y, w, h).
pub fn set_menu_projection(c: &mut Cpu, x: i32, y: i32, w: i32, h: i32) {
    c.gte_write(H, MENU_GEOM_SCREEN);
    set_offset(c, x, y, w, h);
}

pub fn abi_set_menu_projection(c: &mut Cpu) {
    set_menu_projection(c, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `TrsProj_SetViewTrsProjEnviro` (TRSPROJ.CPP, 0x800e2614): the projection for a view: the
/// whole screen, or the top half with two players (the view itself unused).
pub fn set_view_trs_proj_enviro(c: &mut Cpu, _view: u32) {
    let h = if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN { SCREEN_H_SPLIT } else { SCREEN_H };
    set_projection(c, 0, 0, SCREEN_W as i32, h as i32);
}

pub fn abi_set_view_trs_proj_enviro(c: &mut Cpu) {
    set_view_trs_proj_enviro(c, c.r[4]);
}

/// `TrsProj_SetPsxMatrix` (TRSPROJ.CPP, 0x800e2668): the GTE's rotation from `matrix` (its
/// transpose, 16.16 to 4.12) and its translation from `trans` (none: zero).
pub fn set_psx_matrix(c: &mut Cpu, matrix: u32, trans: u32) {
    let m = &mut c.m;
    // (the GTE's row k is the matrix's column k)
    let r: [i16; 9] = std::array::from_fn(|i| {
        let (row, col) = (i / 3, i % 3);
        (m.r32(matrix + 4 * (3 * col + row) as u32) as i32 >> 4) as i16
    });
    for k in 0..4 {
        c.gte_write(RT + k, (r[2 * k] as u16 as u32) | (r[2 * k + 1] as u16 as u32) << 16);
    }
    c.gte_write(RT + 4, r[8] as u16 as u32);
    if trans != 0 {
        set_psx_trans(c, trans);
    } else {
        set_translation(c, [0; 3]);
    }
}

pub fn abi_set_psx_matrix(c: &mut Cpu) {
    set_psx_matrix(c, c.r[4], c.r[5]);
}

/// `TrsProj_SetPsxTrans` (TRSPROJ.CPP, 0x800e2758): the GTE's translation from `trans` (16.16).
pub fn set_psx_trans(c: &mut Cpu, trans: u32) {
    let t = coord(&mut c.m, trans).map(|v| v >> 10);
    set_translation(c, t);
}

pub fn abi_set_psx_trans(c: &mut Cpu) {
    set_psx_trans(c, c.r[4]);
}

/// `TrsProj_SetPsxTransZero` (TRSPROJ.CPP, 0x800e27a8): no translation.
pub fn set_psx_trans_zero(c: &mut Cpu) {
    set_translation(c, [0; 3]);
}

pub fn abi_set_psx_trans_zero(c: &mut Cpu) {
    set_psx_trans_zero(c);
}

/// `TrsProj_TransPt` (TRSPROJ.CPP, 0x800e27d8): the point at `from` (16.16) through the GTE's
/// rotation and translation into `to` (16.16).
pub fn trans_pt(c: &mut Cpu, from: u32, to: u32) {
    let v = coord(&mut c.m, from).map(|v| (v >> 10) as i16);
    let out = rotate_translate(c, v);
    for (k, v) in out.into_iter().enumerate() {
        c.m.w32(to.wrapping_add(4 * k as u32), (v << 10) as u32);
    }
}

pub fn abi_trans_pt(c: &mut Cpu) {
    trans_pt(c, c.r[4], c.r[5]);
}

/// `TrsProj_TransPtN16` (TRSPROJ.CPP, 0x800e2864): `n` points on the ground (`RelCoord16`: x
/// and z, y 0) through the GTE into `to` (the GTE's results, a `coorddef` each).
pub fn trans_pt_n16(c: &mut Cpu, from: u32, to: u32, n: u32) {
    for k in 0..n {
        let at = from.wrapping_add(4 * k);
        let (x, z) = (c.m.r16(at) as i16, c.m.r16(at.wrapping_add(2)) as i16);
        let out = rotate_translate(c, [x, 0, z]);
        let to = to.wrapping_add(12 * k);
        for (j, v) in out.into_iter().enumerate() {
            c.m.w32(to.wrapping_add(4 * j as u32), v as u32);
        }
    }
}

pub fn abi_trans_pt_n16(c: &mut Cpu) {
    trans_pt_n16(c, c.r[4], c.r[5], c.r[6]);
}

/// `TrsProj_TransformProjectVertex` (TRSPROJ.CPP, 0x800e28ec): `n` points (`coorddef`s at
/// `points`) through `matrix` and moved by `pos`, into `out` (`Draw_tVertex`: x, y, z in the
/// GTE's units, the flag 10).
pub fn transform_project_vertex(c: &mut Cpu, matrix: u32, pos: u32, n: i32, points: u32, out: u32) {
    with_buffer(c, 12, |c, moved| {
        for k in 0..n.max(0) as u32 {
            call_addr(c, TRANSFORM, &[points.wrapping_add(12 * k), matrix, moved]);
            let m = &mut c.m;
            let v = out.wrapping_add(8 * k);
            for j in 0..3 {
                let p = (m.r32(moved + 4 * j) as i32).wrapping_add(m.r32(pos.wrapping_add(4 * j)) as i32) >> 10;
                m.w16(v + 2 * j, p as u16);
                if j == 1 {
                    m.w16(v + 6, VERTEX_FLAG);
                }
            }
        }
    });
}

pub fn abi_transform_project_vertex(c: &mut Cpu) {
    let out = c.m.r32(c.r[29] + 16);
    transform_project_vertex(c, c.r[4], c.r[5], c.r[6] as i32, c.r[7], out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM (the race leaves it zero): the GTE's registers before a run and after it (as
    /// the game reads them), a matrix, a point, a result, points, vertices, ground points.
    const GTE_IN: u32 = 0x8014_0000;
    const GTE_OUT: u32 = 0x8014_0100;
    const MATRIX: u32 = 0x8014_0200;
    const POINT: u32 = 0x8014_0240;
    const RESULT: u32 = 0x8014_0260;
    const POINTS: u32 = 0x8014_0300;
    const VERTICES: u32 = 0x8014_0400;
    const GROUND: u32 = 0x8014_0500;
    const RESULTS: u32 = 0x8014_0600;

    /// The GTE as the shuffle left it in RAM (the tests' CPU keeps one GTE for both runs).
    fn load_gte(c: &mut Cpu) {
        for r in 0..64 {
            c.gte_regs[r] = c.m.r32(GTE_IN + 4 * r as u32);
        }
    }

    /// The GTE's registers as code reads them, into RAM for the comparison.
    fn dump_gte(c: &mut Cpu) {
        for r in 0..64 {
            let v = c.gte_read(r);
            c.m.w32(GTE_OUT + 4 * r as u32, v);
        }
    }

    fn gte_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        load_gte(c);
        let v = via_abi(c, a);
        dump_gte(c);
        v
    }

    /// Any GTE state (the matrices' elements and vectors 16-bit, the screen's set-up sane), a
    /// turn (16.16, around unit length), points (16.16, near and far), ground points.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        for r in 0..64 {
            m.w32(GTE_IN + 4 * r, rng.next() as u32);
        }
        m.w32(GTE_IN + 4 * (32 + 26), 200 + pick(rng, 400));
        for k in 0..9 {
            m.w32(MATRIX + 4 * k, (pick(rng, 0x2_0000) as i32 - 0x1_0000) as u32);
        }
        for k in 0..3 {
            m.w32(POINT + 4 * k, rng.any() as u32);
        }
        for k in 0..24 {
            let v = [rng.any(), pick(rng, 0x200_0000) as i32 - 0x100_0000][pick(rng, 2) as usize];
            m.w32(POINTS + 4 * k, v as u32);
        }
        for k in 0..16 {
            m.w32(GROUND + 4 * k, rng.next() as u32);
        }
        GAME_SETUP.set_comm_mode(m, pick(rng, 2) as i32);
    }

    fn rect(r: &mut Rng) -> Vec<u32> {
        (0..4).map(|_| [0, 320, 240, 120, pick(r, 641), (pick(r, 200) as i32 - 100) as u32 as u32][pick(r, 6) as usize]).collect()
    }

    fn cases() -> Vec<Case> {
        vec![
            ("TrsProj_SetTransPrecision", 0x800e_2538, |c| { load_gte(c); nfs4rc::f_800e2538(c); dump_gte(c) }, |_, r| vec![pick(r, 16)]),
            ("TrsProj_ResetTransPrecision", 0x800e_2544, |c| { load_gte(c); nfs4rc::f_800e2544(c); dump_gte(c) }, |_, _| vec![]),
            ("TrsProj_SetProjection", 0x800e_2554, |c| { load_gte(c); nfs4rc::f_800e2554(c); dump_gte(c) }, |_, r| rect(r)),
            ("TrsProj_SetMenuProjection", 0x800e_25d4, |c| { load_gte(c); nfs4rc::f_800e25d4(c); dump_gte(c) }, |_, r| rect(r)),
            ("TrsProj_SetViewTrsProjEnviro", 0x800e_2614, |c| { load_gte(c); nfs4rc::f_800e2614(c); dump_gte(c) }, |_, _| vec![0]),
            ("TrsProj_SetPsxMatrix", 0x800e_2668, |c| { load_gte(c); nfs4rc::f_800e2668(c); dump_gte(c) }, |_, r| vec![MATRIX, [0, POINT][pick(r, 2) as usize]]),
            ("TrsProj_SetPsxTrans", 0x800e_2758, |c| { load_gte(c); nfs4rc::f_800e2758(c); dump_gte(c) }, |_, _| vec![POINT]),
            ("TrsProj_SetPsxTransZero", 0x800e_27a8, |c| { load_gte(c); nfs4rc::f_800e27a8(c); dump_gte(c) }, |_, _| vec![]),
            ("TrsProj_TransPt", 0x800e_27d8, |c| { load_gte(c); nfs4rc::f_800e27d8(c); dump_gte(c) }, |_, r| vec![[POINT, POINTS][pick(r, 2) as usize], RESULT]),
            ("TrsProj_TransPtN16", 0x800e_2864, |c| { load_gte(c); nfs4rc::f_800e2864(c); dump_gte(c) }, |_, r| vec![GROUND, RESULTS, pick(r, 9)]),
            ("TrsProj_TransformProjectVertex", 0x800e_28ec, |c| { load_gte(c); nfs4rc::f_800e28ec(c); dump_gte(c) }, |_, r| vec![MATRIX, POINT, [0, 1, 8, pick(r, 9)][pick(r, 4) as usize], POINTS, VERTICES]),
        ]
    }

    #[test]
    fn trsproj_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xC10 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0xC11 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), gte_port);
        }
    }
}
