//! DRAWSHP.CPP: the front end's boxes: a rounded rectangle (its corner shapes, its top and body
//! between them, highlighted or shaded, a title over it), the NFS4 rectangle (its four corner
//! shapes, its edges shading in and its body) and a translucent framed one.
//! `DrawShape_SubtractNFS4RectEdges` stays recompiled (the host captures its primitives).
//!
//! Written as FEMENU.CPP is (see there). The squares are the host's (called by address). The
//! corner shapes are the help's (`HELP` in their flags), which read no options: the original
//! passes its frame's leftovers as them, the rewrite none.

use crate::call::{call, call_addr};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SET_ABR_MODE};
use crate::globals::SHAPES;
use crate::types::TextureShapeInfo;
use ps1rt::{Cpu, Mem};

/// `PSXDrawSquare`, `PSXDrawTransSquare`, `PSXDrawTransGouraudSquare`.
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
/// The help's shapes: the rounded end (41), the rectangle's corners (42 top left, 43 top right,
/// 44 bottom left, 45 bottom right).
const ROUND_END: u32 = 41;
const TOP_LEFT: u32 = 42;
const TOP_RIGHT: u32 = 43;
const BOTTOM_LEFT: u32 = 44;
const BOTTOM_RIGHT: u32 = 45;
/// The drawing's flags: semi-transparent, mirrored, the help's shapes.
const SEMI: u32 = 1;
const MIRROR: u32 = 4;
const HELP: u32 = 8;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// A `RECT` (four `short`s), sign-extended.
#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Rect {
    fn read(m: &mut Mem, at: u32) -> Rect {
        let mut f = |k: u32| m.r16(at + k) as i16 as i32 as u32;
        Rect { x: f(0), y: f(2), w: f(4), h: f(6) }
    }
}

/// Help shape `k` and its size.
fn help_shape(m: &mut Mem, k: u32) -> (u32, u32) {
    let shape = TextureShapeInfo(m.r32(SHAPES)).at(k);
    (shape.width(m) as i32 as u32, shape.height(m) as i32 as u32)
}

fn draw_shape(c: &mut Cpu, k: u32, flags: u32, x: u32, y: u32) {
    call(c, DRAW_SHAPE_EXTENDED, &[k, flags, x, y, 0, 0, 0]);
}

fn square(c: &mut Cpu, color: u32, x: u32, y: u32, w: u32, h: u32) {
    call_addr(c, DRAW_SQUARE, &[color, x, y, w, h]);
}

fn trans_square(c: &mut Cpu, color: u32, x: u32, y: u32, w: u32, h: u32, abr: u32) {
    call_addr(c, DRAW_TRANS_SQUARE, &[color, x, y, w, h, abr]);
}

/// `DrawShape_NFS4RoundRectangle` (DRAWSHP.CPP, 0x8004c490): text `title` (none when
/// negative) centred over `rect`; the rounded rectangle's ends (the end shape and its mirror),
/// its top line and the body between them: solid when `highlight`, else translucent.
pub fn round_rectangle(c: &mut Cpu, title: u32, rect: u32, highlight: u32) {
    let r = Rect::read(&mut c.m, rect);
    if (title as i32) >= 0 {
        let half = ((c.m.r16(rect + 4) as i16) >> 1) as i32 as u32;
        let x = sx((c.m.r16(rect) as u32).wrapping_add(half));
        let y = sx((c.m.r16(rect + 2) as u32).wrapping_add(4));
        call(c, crate::fetextrender::menu_text_positioned_justify_entry, &[sx(title), x, y, 2, 1, 3]);
    }
    let solid = sx(highlight) != 0;
    let semi = if solid { 0 } else { SEMI };
    let (end_w, end_h) = help_shape(&mut c.m, ROUND_END);
    draw_shape(c, ROUND_END, semi | HELP, r.x, r.y);
    draw_shape(c, ROUND_END, semi | HELP | MIRROR, r.x.wrapping_add(r.w).wrapping_sub(end_w), r.y);
    let (left, between) = (r.x.wrapping_add(end_w), r.w.wrapping_sub(end_w << 1).wrapping_add(1));
    let top = r.y.wrapping_add(end_h).wrapping_sub(1);
    if solid {
        square(c, TOP_COLOR, left, top, between, 1);
        square(c, 0, left, r.y, between, end_h.wrapping_sub(1));
    } else {
        trans_square(c, TOP_COLOR, left, top, between, 1, 1);
        trans_square(c, 0, left, r.y, between, end_h.wrapping_sub(1), 1);
        call(c, SET_ABR_MODE, &[0]);
    }
}

pub fn abi_round_rectangle(c: &mut Cpu) {
    round_rectangle(c, c.r[4], c.r[5], c.r[6]);
}

/// `DrawShape_NFS4Rectangle` (DRAWSHP.CPP, 0x8004c69c): `rect`'s four corner shapes, its top
/// and bottom edges, its sides shading down (2 wide), the dark between the side corners, its
/// body; all translucent.
pub fn rectangle(c: &mut Cpu, rect: u32) {
    let r = Rect::read(&mut c.m, rect);
    let m = &mut c.m;
    let (tl_w, tl_h) = help_shape(m, TOP_LEFT);
    let (tr_w, tr_h) = help_shape(m, TOP_RIGHT);
    let (bl_w, bl_h) = help_shape(m, BOTTOM_LEFT);
    let (br_w, br_h) = help_shape(m, BOTTOM_RIGHT);
    let (right, bottom) = (r.x.wrapping_add(r.w), r.y.wrapping_add(r.h));
    draw_shape(c, TOP_LEFT, HELP | SEMI, r.x, r.y);
    draw_shape(c, TOP_RIGHT, HELP | SEMI, right.wrapping_sub(tr_w), r.y);
    draw_shape(c, BOTTOM_LEFT, HELP | SEMI, r.x, bottom.wrapping_sub(bl_h));
    draw_shape(c, BOTTOM_RIGHT, HELP | SEMI, right.wrapping_sub(br_w), bottom.wrapping_sub(br_h));
    trans_square(c, RECT_TOP, r.x.wrapping_add(tl_w), r.y, r.w.wrapping_sub(tl_w).wrapping_sub(tr_w), 1, 1);
    trans_square(c, RECT_BOTTOM, r.x.wrapping_add(bl_w), bottom.wrapping_sub(1), r.w.wrapping_sub(bl_w).wrapping_sub(br_w), 1, 1);
    let (a, b) = (RECT_SIDE_TOP, RECT_SIDE_BOTTOM);
    let left_h = r.h.wrapping_sub(tl_h).wrapping_sub(bl_h);
    let right_h = r.h.wrapping_sub(tr_h).wrapping_sub(br_h);
    call_addr(c, DRAW_TRANS_GOURAUD_SQUARE, &[r.x, r.y.wrapping_add(tl_h), 2, left_h, 1, a, a, b, b]);
    call_addr(c, DRAW_TRANS_GOURAUD_SQUARE, &[right.wrapping_sub(2), r.y.wrapping_add(tr_h), 2, right_h, 1, a, a, b, b]);
    trans_square(c, 0, r.x.wrapping_add(2), r.y.wrapping_add(tl_h), tl_w.wrapping_sub(2), left_h, 1);
    // (the right one's width as the original has it: 2 - the corner's)
    trans_square(c, 0, right.wrapping_sub(2), r.y.wrapping_add(tr_h), 2u32.wrapping_sub(tr_w), right_h, 1);
    trans_square(c, 0, r.x.wrapping_add(tl_w), r.y.wrapping_add(1), r.w.wrapping_sub(tl_w).wrapping_sub(tr_w), r.h.wrapping_sub(2), 1);
    call(c, SET_ABR_MODE, &[0]);
}

pub fn abi_rectangle(c: &mut Cpu) {
    rectangle(c, c.r[4]);
}

/// `DrawShape_NFS4TransRectangle` (DRAWSHP.CPP, 0x8004c9bc): `rect`'s frame (the sides 4 wide,
/// the top and bottom 2 high, in blending `abr * 2`) and its inside (in `abr`).
pub fn trans_rectangle(c: &mut Cpu, rect: u32, abr: u32) {
    let r = Rect::read(&mut c.m, rect);
    let edge = sx(abr << 1);
    let (right, bottom) = (r.x.wrapping_add(r.w), r.y.wrapping_add(r.h));
    let inner_w = r.w.wrapping_sub(8);
    trans_square(c, 0, r.x, r.y, 4, r.h, edge);
    trans_square(c, 0, right.wrapping_sub(4), r.y, 4, r.h, edge);
    trans_square(c, 0, r.x.wrapping_add(4), r.y, inner_w, 2, edge);
    trans_square(c, 0, r.x.wrapping_add(4), bottom.wrapping_sub(2), inner_w, 2, edge);
    trans_square(c, 0, r.x.wrapping_add(4), r.y.wrapping_add(2), inner_w, r.h.wrapping_sub(4), sx(abr));
    call(c, SET_ABR_MODE, &[0]);
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
