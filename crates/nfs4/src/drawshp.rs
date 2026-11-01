//! DRAWSHP.CPP: the front end's boxes: a rounded rectangle (its corner shapes, its top and body
//! between them, highlighted or shaded, a title over it), the NFS4 rectangle (its four corner
//! shapes, its edges shading in and its body) and a translucent framed one.
//! `DrawShape_SubtractNFS4RectEdges` stays recompiled (the host captures its primitives).

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE};
use crate::globals::SHAPES;
use ps1rt::{Cpu, Mem};

/// `PSXDrawSquare`, `PSXDrawTransSquare`, `PSXDrawTransGouraudSquare` (the host captures their
/// primitives: called by address).
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
const DRAW_TRANS_GOURAUD_SQUARE: u32 = 0x8004_F2B4;
/// The rounded rectangle's top's colour; the NFS4 rectangle's colours (its top, bottom, its
/// sides shading from one to the other).
const TOP_COLOR: u32 = 0x0084_1D08;
const RECT_TOP: u32 = 0x0015_0800;
const RECT_BOTTOM: u32 = 0x007B_2908;
const RECT_SIDE_TOP: u32 = 0x0029_1000;
const RECT_SIDE_BOTTOM: u32 = 0x004A_1900;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// A shape's width and height (its +16, +18).
fn size(m: &mut Mem, shape: u32) -> (u32, u32) {
    (lh(m, shape + 16), lh(m, shape + 18))
}

/// `DrawShape_NFS4RoundRectangle` (DRAWSHP.CPP, 0x8004c490): text `title` (none for -1)
/// centred over `rect`; the rounded rectangle's ends (shape 41 and its mirror), its top line and
/// body between them, solid when `highlight` (else shaded).
pub fn round_rectangle(c: &mut Cpu, title: u32, rect: u32, highlight: u32) {
    let corner = c.m.r32(SHAPES).wrapping_add(1312);
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = rect;
        r.0[0] = highlight;
        r.0[3] = 0;
        r.0[2] = corner;
        if (title as i32) >= 0 {
            let m = &mut c.m;
            let half = ((m.r16(rect + 4) as i16) >> 1) as i32 as u32;
            let x = sx((m.r16(rect) as u32).wrapping_add(half));
            let y = sx((m.r16(rect + 2) as u32).wrapping_add(4));
            r.call_leaving(c, crate::fetextrender::menu_text_positioned_justify_entry, &[sx(title), x, y, 2, 1, 3], 0x8004_c514);
        }
        let on = sx(highlight);
        r.0[4] = on;
        if on == 0 {
            r.0[3] = 1;
        }
        let place = sp + 32;
        r.0[0] = place;
        let m = &mut c.m;
        let (x, y) = (lh(m, rect), lh(m, rect + 2));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[41, r.0[3] | 8, x, y, 0, 0, place], 0x8004_c548);
        let m = &mut c.m;
        let (x, w, y) = (lh(m, rect), lh(m, rect + 4), lh(m, rect + 2));
        let (cw, ch) = size(m, corner);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[41, r.0[3] | 0xC, x.wrapping_add(w).wrapping_sub(cw), y, 0, 0, place], 0x8004_c578);
        let m = &mut c.m;
        let (x, y, w) = (lh(m, rect), lh(m, rect + 2), lh(m, rect + 4));
        let between = w.wrapping_sub(cw << 1).wrapping_add(1);
        let (left, top) = (x.wrapping_add(cw), y.wrapping_add(ch).wrapping_sub(1));
        if on != 0 {
            r.call_ptr_leaving(c, DRAW_SQUARE, &[TOP_COLOR, left, top, between, 1], 0x8004_c5bc);
            let m = &mut c.m;
            let (x, y, w) = (lh(m, rect), lh(m, rect + 2), lh(m, rect + 4));
            let (cw, ch) = size(m, corner);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0, x.wrapping_add(cw), y, w.wrapping_sub(cw << 1).wrapping_add(1), ch.wrapping_sub(1)], 0x8004_c5f0);
            return;
        }
        r.0[0] = 1;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[TOP_COLOR, left, top, between, 1, 1], 0x8004_c63c);
        let m = &mut c.m;
        let (x, y, w) = (lh(m, rect), lh(m, rect + 2), lh(m, rect + 4));
        let (cw, ch) = size(m, corner);
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(cw), y, w.wrapping_sub(cw << 1).wrapping_add(1), ch.wrapping_sub(1), 1], 0x8004_c674);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8004_c67c);
    })
}

pub fn abi_round_rectangle(c: &mut Cpu) {
    round_rectangle(c, c.r[4], c.r[5], c.r[6]);
}

/// `DrawShape_NFS4Rectangle` (DRAWSHP.CPP, 0x8004c69c): `rect`'s four corner shapes (42..45),
/// its top and bottom edges, its sides shading down, the dark between the corners, its body.
pub fn rectangle(c: &mut Cpu, rect: u32) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = rect;
        let m = &mut c.m;
        m.w32(sp + 48, RECT_TOP);
        m.w32(sp + 56, RECT_SIDE_BOTTOM);
        m.w32(sp + 44, RECT_BOTTOM);
        m.w32(sp + 52, RECT_SIDE_TOP);
        let shapes = m.r32(SHAPES);
        let place = sp + 40;
        let (tl, tr, bl, br) = (shapes + 1344, shapes + 1376, shapes + 1408, shapes + 1440);
        r.0[..6].copy_from_slice(&[rect, place, br, tr, bl, tl]);
        let g = |m: &mut Mem, k: u32| lh(m, rect + k);
        let (x, y) = (g(m, 0), g(m, 2));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[42, 9, x, y, 0, 0, place], 0x8004_c72c);
        let m = &mut c.m;
        let (x, w, tr_w, y) = (g(m, 0), g(m, 4), lh(m, tr + 16), g(m, 2));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[43, 9, x.wrapping_add(w).wrapping_sub(tr_w), y, 0, 0, place], 0x8004_c75c);
        let m = &mut c.m;
        let (x, y, h, bl_h) = (g(m, 0), g(m, 2), g(m, 6), lh(m, bl + 18));
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[44, 9, x, y.wrapping_add(h).wrapping_sub(bl_h), 0, 0, place], 0x8004_c78c);
        let m = &mut c.m;
        let (x, w, y, h) = (g(m, 0), g(m, 4), g(m, 2), g(m, 6));
        let (br_w, br_h) = size(m, br);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[45, 9, x.wrapping_add(w).wrapping_sub(br_w), y.wrapping_add(h).wrapping_sub(br_h), 0, 0, place], 0x8004_c7cc);
        r.0[1] = 1;
        let m = &mut c.m;
        let (x, y, w) = (g(m, 0), g(m, 2), g(m, 4));
        let (tl_w, tr_w, top) = (lh(m, tl + 16), lh(m, tr + 16), m.r32(sp + 48));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[top, x.wrapping_add(tl_w), y, w.wrapping_sub(tl_w).wrapping_sub(tr_w), 1, 1], 0x8004_c800);
        let m = &mut c.m;
        let (x, y, h, w) = (g(m, 0), g(m, 2), g(m, 6), g(m, 4));
        let (bl_w, br_w, bottom) = (lh(m, bl + 16), lh(m, br + 16), m.r32(sp + 44));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[bottom, x.wrapping_add(bl_w), y.wrapping_add(h).wrapping_sub(1), w.wrapping_sub(bl_w).wrapping_sub(br_w), 1, 1], 0x8004_c83c);
        let m = &mut c.m;
        let (x, y, h) = (g(m, 0), g(m, 2), g(m, 6));
        let (tl_h, bl_h, a, b) = (lh(m, tl + 18), lh(m, bl + 18), m.r32(sp + 52), m.r32(sp + 56));
        r.call_ptr_leaving(c, DRAW_TRANS_GOURAUD_SQUARE, &[x, y.wrapping_add(tl_h), 2, h.wrapping_sub(tl_h).wrapping_sub(bl_h), 1, a, a, b, b], 0x8004_c880);
        let m = &mut c.m;
        let (x, w, y, h) = (g(m, 0), g(m, 4), g(m, 2), g(m, 6));
        let (tr_h, br_h, a, b) = (lh(m, tr + 18), lh(m, br + 18), m.r32(sp + 52), m.r32(sp + 56));
        let args = [x.wrapping_add(w).wrapping_sub(2), y.wrapping_add(tr_h), 2, h.wrapping_sub(tr_h).wrapping_sub(br_h), 1, a, a, b, b];
        r.call_ptr_leaving(c, DRAW_TRANS_GOURAUD_SQUARE, &args, 0x8004_c8d0);
        let m = &mut c.m;
        let (x, y, h) = (g(m, 0), g(m, 2), g(m, 6));
        let (tl_w, tl_h, bl_h) = (lh(m, tl + 16), lh(m, tl + 18), lh(m, bl + 18));
        let args = [0, x.wrapping_add(2), y.wrapping_add(tl_h), tl_w.wrapping_sub(2), h.wrapping_sub(tl_h).wrapping_sub(bl_h), 1];
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &args, 0x8004_c90c);
        let m = &mut c.m;
        let (x, w, y, h) = (g(m, 0), g(m, 4), g(m, 2), g(m, 6));
        let (tr_w, tr_h, br_h) = (lh(m, tr + 16), lh(m, tr + 18), lh(m, br + 18));
        let args = [0, x.wrapping_add(w).wrapping_sub(2), y.wrapping_add(tr_h), 2u32.wrapping_sub(tr_w), h.wrapping_sub(tr_h).wrapping_sub(br_h), 1];
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &args, 0x8004_c954);
        let m = &mut c.m;
        let (x, y, w, h) = (g(m, 0), g(m, 2), g(m, 4), g(m, 6));
        let (tl_w, tr_w) = (lh(m, tl + 16), lh(m, tr + 16));
        let args = [0, x.wrapping_add(tl_w), y.wrapping_add(1), w.wrapping_sub(tl_w).wrapping_sub(tr_w), h.wrapping_sub(2), 1];
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &args, 0x8004_c990);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8004_c998);
    })
}

pub fn abi_rectangle(c: &mut Cpu) {
    rectangle(c, c.r[4]);
}

/// `DrawShape_NFS4TransRectangle` (DRAWSHP.CPP, 0x8004c9bc): `rect`'s frame (the sides 4 wide,
/// the top and bottom 2 high, in blending `abr * 2`) and its inside (in `abr`).
pub fn trans_rectangle(c: &mut Cpu, rect: u32, abr: u32) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let edge = sx(abr << 1);
        r.0[..3].copy_from_slice(&[rect, edge, abr]);
        let g = |m: &mut Mem, k: u32| lh(m, rect + k);
        let m = &mut c.m;
        let (x, y, h) = (g(m, 0), g(m, 2), g(m, 6));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x, y, 4, h, edge], 0x8004_ca04);
        let m = &mut c.m;
        let (x, w, y, h) = (g(m, 0), g(m, 4), g(m, 2), g(m, 6));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(w).wrapping_sub(4), y, 4, h, edge], 0x8004_ca30);
        r.0[3] = 2;
        let m = &mut c.m;
        let (x, y, w) = (g(m, 0), g(m, 2), g(m, 4));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y, w.wrapping_sub(8), 2, edge], 0x8004_ca58);
        let m = &mut c.m;
        let (x, y, h, w) = (g(m, 0), g(m, 2), g(m, 6), g(m, 4));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y.wrapping_add(h).wrapping_sub(2), w.wrapping_sub(8), 2, edge], 0x8004_ca88);
        r.0[2] = sx(abr);
        let m = &mut c.m;
        let (x, y, w, h) = (g(m, 0), g(m, 2), g(m, 4), g(m, 6));
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, x.wrapping_add(4), y.wrapping_add(2), w.wrapping_sub(8), h.wrapping_sub(4), sx(abr)], 0x8004_cac0);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8004_cac8);
    })
}

pub fn abi_trans_rectangle(c: &mut Cpu) {
    trans_rectangle(c, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A rectangle (x, y, w, h).
    const RECT: u32 = 0x8014_7000;

    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for (k, v) in [pick(rng, 300), pick(rng, 240), [0, 8, pick(rng, 300)][pick(rng, 3) as usize], [0, 4, pick(rng, 200)][pick(rng, 3) as usize]].into_iter().enumerate() {
            m.w16(RECT + 2 * k as u32, v as u16);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("DrawShape_NFS4RoundRectangle", 0x8004_c490, nfs4rc::f_8004c490, |_, r| vec![[u32::MAX, pick(r, 300)][pick(r, 2) as usize], RECT, [0, 1, 0x1_0000][pick(r, 3) as usize]]),
            ("DrawShape_NFS4Rectangle", 0x8004_c69c, nfs4rc::f_8004c69c, |_, _| vec![RECT]),
            ("DrawShape_NFS4TransRectangle", 0x8004_c9bc, nfs4rc::f_8004c9bc, |_, r| vec![RECT, [0, 1, 2, 3, 0x8001][pick(r, 5) as usize]]),
        ]
    }

    #[test]
    fn drawshp_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xD5A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xD5A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
