//! HUD.CPP: the race's head-up display: its views (the map, the dashboard, the tachometer, the
//! stats), the primitives it builds from the HUD's shapes (`HudPmx_gShapes`) and the font, the
//! map, the radar, the CD player, the countdown, the pause box, the "busted" overlay.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free. Calls to the HUD's own functions not yet
//! decompiled, and to the helpers the host hooks (`Hud_FBuildF4`, `Hud_BuildString`,
//! `Hud_BlackThinBox`, `Hud_RenderPauseBox`, `Font_TextXY`), go by address.

use crate::call::{call, call_addr, with_buffer};
use crate::fixed::{fixedmult, fixedsincos};
use crate::globals::GAME_SETUP;
use crate::gpu::{self, xy};
use ps1rt::{Cpu, Mem};

/// The HUD's shapes (`HudPmx_tShape`, 20 bytes: four texture words (u, v and the CLUT, then
/// the page...), the width and height as halves).
const SHAPES: u32 = 0x8011_1F5C;
const SHAPE_SIZE: u32 = 20;
const SHAPE_W: u32 = 16;
const SHAPE_H: u32 = 18;

/// The sprites' colour and transparency (`currentSpriteColor`, `currentSpriteTransparent`);
/// the font's CLUT.
const SPRITE_COLOR: u32 = 0x8013_F11C;
const SPRITE_TRANSPARENT: u32 = 0x8013_F118;
const FONT_CLUT: u32 = 0x8013_EB4C;
/// The two players' sprite buffers (`gSprite0`, `gSprite1`, 2060 bytes each) and their names.
const SPRITES: [u32; 2] = [0x8013_F110, 0x8013_F114];
const SPRITES_NAME: [u32; 2] = [0x8013_EBAC, 0x8013_EBB4];
const SPRITES_SIZE: u32 = 2060;
/// The views (`Draw_SetView`'s): the map's and the dashboard's per player, the tachometer's
/// per player, the stats'; the map's offset down; "be the cop" (a human cop in the race).
const MAP_VIEW: u32 = 0x8013_EC38;
const HUD_VIEW: u32 = 0x8013_EC30;
const TAC_VIEW: u32 = 0x8013_EC40;
const STATS_VIEW: u32 = 0x8013_EC48;
const MAP_OFFSET_Y: u32 = 0x8013_EC4C;
const BE_THE_COP: u32 = 0x8013_EC1C;
/// A car's class bit: a human cop.
const CLASS_COP: i32 = 0x40;

/// Primitive sizes (bytes) and codes the HUD writes itself.
const SPRT_SIZE: u32 = 20;
const SPRT_CODE: u32 = 0x66 << 24;
const POLY_GT4_SIZE: u32 = 52;
const POLY_GT4_CODE: u32 = 0x3E << 24;
const POLY_FT4_SIZE: u32 = 40;
const POLY_F4_SIZE: u32 = 24;
const DR_MODE_SIZE: u32 = 12;

const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const SET_DRAW_MODE: fn(&mut Cpu) = nfs4rc::f_800eed18;
/// `Font_GetUVWH`, `HudPmx_Kill`, `Draw_SetView` (by address: the host sees the views).
const FONT_GET_UVWH: fn(&mut Cpu) = nfs4rc::f_800cc948;
const HUD_PMX_KILL: fn(&mut Cpu) = nfs4rc::f_80093a20;
const DRAW_SET_VIEW: u32 = 0x800B_E8A8;

fn shape(k: u32) -> u32 {
    SHAPES.wrapping_add(k.wrapping_mul(SHAPE_SIZE))
}

/// A shape's width and height (halves, sign-extended).
fn shape_wh(m: &mut Mem, shape: u32) -> (u32, u32) {
    (m.r16(shape + SHAPE_W) as i16 as i32 as u32, m.r16(shape + SHAPE_H) as i16 as i32 as u32)
}

/// `Hud_CreateHudViews` (HUD.CPP, 0x800d27e0): the HUD's tables, whether a human cop races
/// ("be the cop"), the map's offset down (up with two players, down alone or as the cop), and
/// the views: with two players (split screen) a map, dashboard and tachometer per half, else
/// one of each; the stats' view.
pub fn create_hud_views(c: &mut Cpu) {
    init_tables(&mut c.m);
    let m = &mut c.m;
    m.w32(BE_THE_COP, 0);
    let gs = GAME_SETUP;
    for k in 0..gs.num_cars(m).max(0) as u32 {
        if gs.car_info(k).car_class(m) & CLASS_COP != 0 {
            m.w32(BE_THE_COP, 1);
        }
    }
    let players = gs.num_player_race_cars(m);
    let offset = if players >= 2 {
        -6
    } else if players.wrapping_add(gs.num_opponent_race_cars(m)) == 1 || m.r32(BE_THE_COP) != 0 {
        8
    } else {
        0
    };
    m.w32(MAP_OFFSET_Y, offset as u32);
    let view = |c: &mut Cpu, x0: u32, y: u32, x1: u32, w: u32, h: u32| call_addr(c, DRAW_SET_VIEW, &[x0, y, x1, y, w, h, 0, 0, 1]);
    let off = |c: &mut Cpu| c.m.r32(MAP_OFFSET_Y);
    if gs.comm_mode(&mut c.m) == 1 {
        let o = off(c);
        let v = view(c, 261, o.wrapping_add(318), 581, 45, 48);
        c.m.w32(MAP_VIEW, v);
        let o = off(c);
        let v = view(c, 261, o.wrapping_add(425), 581, 45, 48);
        c.m.w32(MAP_VIEW + 4, v);
        let v = view(c, 0, 256, 320, 320, 120);
        c.m.w32(HUD_VIEW, v);
        let v = view(c, 0, 376, 320, 320, 120);
        c.m.w32(HUD_VIEW + 4, v);
        let v = view(c, 277, 275, 597, 28, 28);
        c.m.w32(TAC_VIEW, v);
        let v = view(c, 277, 380, 597, 28, 28);
        c.m.w32(TAC_VIEW + 4, v);
    } else {
        let o = off(c);
        let v = view(c, 255, o.wrapping_add(420), 575, 45, 48);
        c.m.w32(MAP_VIEW, v);
        let v = view(c, 0, 256, 320, 320, 240);
        c.m.w32(HUD_VIEW, v);
        let v = view(c, 184, 277, 504, 28, 28);
        c.m.w32(TAC_VIEW, v);
    }
    let v = view(c, 0, 256, 320, 320, 240);
    c.m.w32(STATS_VIEW, v);
}

pub fn abi_create_hud_views(c: &mut Cpu) {
    create_hud_views(c);
}

/// `Hud_GoTpage` (HUD.CPP, 0x800d2acc): a drawing mode primitive selecting texture page
/// `k + 2` (of 16).
pub fn go_tpage(c: &mut Cpu, k: u32) {
    let prim = gpu::add_prim(&mut c.m, DR_MODE_SIZE);
    let page = ((k << 6).wrapping_add(128) & 0x3FF) >> 6;
    call(c, SET_DRAW_MODE, &[prim, 0, 0, page, 0]);
}

pub fn abi_go_tpage(c: &mut Cpu) {
    go_tpage(c, c.r[4]);
}

/// `Hud_DebugInfo` and `Hud_DebugCrap` (HUD.CPP, 0x800d2b5c, 0x800d2b64): nothing in the
/// retail game.
pub fn abi_nothing(_c: &mut Cpu) {}

/// `Hud_BuildSprite` (HUD.CPP, 0x800d2b6c): `prim` a sprite of shape `k` at `x`, `y` in
/// `colour` (always semi-transparent: the code written after `SetSemiTrans` has the bit).
pub fn build_sprite(m: &mut Mem, prim: u32, k: u32, x: u32, y: u32, colour: u32, semi: bool) {
    let shape = shape(k);
    gpu::set_semi_trans(m, prim, semi);
    m.w8(prim + 3, 4);
    m.w32(prim + 4, colour | SPRT_CODE);
    m.w32(prim + 8, xy(x, y));
    let uv = m.r32(shape);
    m.w32(prim + 12, uv);
    let (w, h) = shape_wh(m, shape);
    m.w32(prim + 16, xy(w, h));
}

pub fn abi_build_sprite(c: &mut Cpu) {
    let (colour, semi) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    build_sprite(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour, semi != 0);
}

/// `Hud_BuildSprite2` (HUD.CPP, 0x800d2c18): [`build_sprite`] in the current sprite colour
/// and transparency.
pub fn build_sprite2(m: &mut Mem, prim: u32, k: u32, x: u32, y: u32) {
    let (colour, semi) = (m.r32(SPRITE_COLOR), m.r8(SPRITE_TRANSPARENT) != 0);
    build_sprite(m, prim, k, x, y, colour, semi);
}

pub fn abi_build_sprite2(c: &mut Cpu) {
    build_sprite2(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hud_FBuildSprite` (HUD.CPP, 0x800d2c44): [`build_sprite`] into a new primitive.
pub fn f_build_sprite(m: &mut Mem, k: u32, x: u32, y: u32, colour: u32, semi: bool) {
    let prim = gpu::add_prim(m, SPRT_SIZE);
    build_sprite(m, prim, k, x, y, colour, semi);
}

pub fn abi_f_build_sprite(c: &mut Cpu) {
    let semi = crate::abi::arg(c, 4);
    f_build_sprite(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], semi != 0);
}

/// `Hud_BuildSpriteFromFont` (HUD.CPP, 0x800d2cd8): `prim` a sprite of the font's character
/// `ch` (its place and size in the font, its drop below the line) at `x`, `y` in the current
/// sprite colour.
pub fn build_sprite_from_font(c: &mut Cpu, prim: u32, ch: u32, x: u32, y: u32) {
    with_buffer(c, 20, |c, uvwh| {
        let (u, v, w, h, drop) = (uvwh, uvwh + 4, uvwh + 8, uvwh + 12, uvwh + 16);
        call(c, FONT_GET_UVWH, &[ch & 0xFF, u, v, w, h, drop]);
        let m = &mut c.m;
        m.w8(prim + 3, 4);
        let colour = m.r32(SPRITE_COLOR) | SPRT_CODE;
        m.w32(prim + 4, colour);
        let down = y.wrapping_add(m.r32(drop));
        m.w32(prim + 8, xy(x, down));
        let (u, v) = (m.r8(u), m.r8(v));
        m.w8(prim + 12, u);
        m.w8(prim + 13, v);
        let clut = m.r16(FONT_CLUT);
        let (w, h) = (m.r16(w), m.r16(h));
        m.w16(prim + 14, clut);
        m.w16(prim + 16, w);
        m.w16(prim + 18, h);
    });
}

pub fn abi_build_sprite_from_font(c: &mut Cpu) {
    build_sprite_from_font(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hud_BuildF3` (HUD.CPP, 0x800d2d98): `prim` a flat triangle pointing left from `x`, `y`,
/// as wide and high as `shape` (centred on `y`), in `colour`.
pub fn build_f3(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32, colour: u32) {
    m.w32(prim + 4, colour);
    gpu::set_poly_f3(m, prim);
    m.w32(prim + 8, xy(x, y));
    let (w, h) = shape_wh(m, shape);
    let half = (h as i32 / 2) as u32;
    m.w32(prim + 12, xy(x.wrapping_add(w), y.wrapping_sub(half)));
    let (w, h) = shape_wh(m, shape);
    let half = (h as i32 / 2) as u32;
    m.w32(prim + 16, xy(x.wrapping_add(w), y.wrapping_add(half)));
}

pub fn abi_build_f3(c: &mut Cpu) {
    let colour = crate::abi::arg(c, 4);
    build_f3(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour);
}

/// `shape`'s four texture words into a textured quad's (`at`: its vertices' offsets, each
/// texture word 4 bytes after).
fn texture(m: &mut Mem, prim: u32, shape: u32, at: [u32; 4]) {
    for (k, off) in at.into_iter().enumerate() {
        let v = m.r32(shape + 4 * k as u32);
        m.w32(prim + off + 4, v);
    }
}

/// `Hud_BuildGT4` (HUD.CPP, 0x800d2e5c): `prim` a gouraud-shaded textured quad of `shape` at
/// `x`, `y`, every corner in `colour`.
pub fn build_gt4(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32, colour: u32) {
    m.w8(prim + 3, 12);
    m.w32(prim + 8, xy(x, y));
    m.w32(prim + 4, colour | POLY_GT4_CODE);
    for at in [16, 28, 40] {
        m.w32(prim + at, colour);
    }
    let (w, h) = shape_wh(m, shape);
    m.w32(prim + 20, xy(x.wrapping_add(w), y));
    m.w32(prim + 32, xy(x, y.wrapping_add(h)));
    m.w32(prim + 44, xy(x.wrapping_add(w), y.wrapping_add(h)));
    texture(m, prim, shape, [8, 20, 32, 44]);
}

pub fn abi_build_gt4(c: &mut Cpu) {
    let colour = crate::abi::arg(c, 4);
    build_gt4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour);
}

/// A textured quad's set-up: its colour, the code, transparency.
fn ft4_header(m: &mut Mem, prim: u32, colour: u32, semi: bool) {
    m.w32(prim + 4, colour);
    gpu::set_poly_ft4(m, prim);
    gpu::set_semi_trans(m, prim, semi);
}

/// `Hud_BuildFT4` (HUD.CPP, 0x800d2f08): `prim` a textured quad of `shape` at `x`, `y`.
pub fn build_ft4(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32, colour: u32, semi: bool) {
    ft4_header(m, prim, colour, semi);
    let (w, h) = shape_wh(m, shape);
    m.w32(prim + 8, xy(x, y));
    m.w32(prim + 16, xy(x.wrapping_add(w), y));
    m.w32(prim + 24, xy(x, y.wrapping_add(h)));
    m.w32(prim + 32, xy(x.wrapping_add(w), y.wrapping_add(h)));
    texture(m, prim, shape, [8, 16, 24, 32]);
}

pub fn abi_build_ft4(c: &mut Cpu) {
    let (colour, semi) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    build_ft4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour, semi != 0);
}

/// `shape`'s quad mirrored left to right at `x`, `y` (its corners' x swapped).
fn mirrored_ft4(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32) {
    let (w, h) = shape_wh(m, shape);
    m.w32(prim + 8, xy(x.wrapping_add(w), y));
    m.w32(prim + 16, xy(x, y));
    m.w32(prim + 24, xy(x.wrapping_add(w), y.wrapping_add(h)));
    m.w32(prim + 32, xy(x, y.wrapping_add(h)));
    texture(m, prim, shape, [8, 16, 24, 32]);
}

/// A half's value less `by`.
fn sub_half(m: &mut Mem, at: u32, by: u16) {
    let v = m.r16(at).wrapping_sub(by);
    m.w16(at, v);
}

/// A byte's value less one.
fn dec_byte(m: &mut Mem, at: u32) {
    let v = m.r8(at).wrapping_sub(1);
    m.w8(at, v);
}

/// `Hud_BuildMirrorFT4` (HUD.CPP, 0x800d2ff8): `prim` [`build_ft4`]'s quad mirrored, its
/// right-hand texture column a texel in.
pub fn build_mirror_ft4(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32, colour: u32, semi: bool) {
    ft4_header(m, prim, colour, semi);
    mirrored_ft4(m, prim, shape, x, y);
    dec_byte(m, prim + 20);
    dec_byte(m, prim + 36);
}

pub fn abi_build_mirror_ft4(c: &mut Cpu) {
    let (colour, semi) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    build_mirror_ft4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour, semi != 0);
}

/// `Hud_BuildMapMirrorFT4` (HUD.CPP, 0x800d30fc): `prim` [`build_ft4`]'s quad mirrored, three
/// pixels left (the map's frame).
pub fn build_map_mirror_ft4(m: &mut Mem, prim: u32, shape: u32, x: u32, y: u32, colour: u32, semi: bool) {
    ft4_header(m, prim, colour, semi);
    mirrored_ft4(m, prim, shape, x, y);
    for at in [8, 16, 24, 32] {
        sub_half(m, prim + at, 3);
    }
}

pub fn abi_build_map_mirror_ft4(c: &mut Cpu) {
    let (colour, semi) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    build_map_mirror_ft4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], colour, semi != 0);
}

/// A quad's four corners: `x`, `y` the top left, `w` wide, `h` high, the top edge's ends moved
/// by `dx0` and `dx1`; at `prim + first`, `step` bytes apart.
#[allow(clippy::too_many_arguments)]
fn corners(m: &mut Mem, prim: u32, first: u32, step: u32, x: u32, y: u32, w: u32, h: u32, dx: (u32, u32)) {
    let (right, bottom) = (x.wrapping_add(w), y.wrapping_add(h));
    m.w32(prim + first, xy(x.wrapping_add(dx.0), y));
    m.w32(prim + first + step, xy(right.wrapping_add(dx.1), y));
    m.w32(prim + first + 2 * step, xy(x, bottom));
    m.w32(prim + first + 3 * step, xy(right, bottom));
}

/// `Hud_BuildF4` (HUD.CPP, 0x800d3208): `prim` a flat rectangle.
#[allow(clippy::too_many_arguments)]
pub fn build_f4(m: &mut Mem, prim: u32, semi: bool, x: u32, y: u32, w: u32, h: u32, colour: u32) {
    m.w32(prim + 4, colour);
    gpu::set_poly_f4(m, prim);
    gpu::set_semi_trans(m, prim, semi);
    corners(m, prim, 8, 4, x, y, w, h, (0, 0));
}

pub fn abi_build_f4(c: &mut Cpu) {
    let a: Vec<u32> = (4..7).map(|k| crate::abi::arg(c, k)).collect();
    build_f4(&mut c.m, c.r[4], c.r[5] != 0, c.r[6], c.r[7], a[0], a[1], a[2]);
}

/// `Hud_BuildG4` (HUD.CPP, 0x800d32ac): `prim` a gouraud-shaded rectangle, a colour per corner.
#[allow(clippy::too_many_arguments)]
pub fn build_g4(m: &mut Mem, prim: u32, semi: bool, x: u32, y: u32, w: u32, h: u32, colours: [u32; 4]) {
    for (k, colour) in colours.into_iter().enumerate() {
        m.w32(prim + 4 + 8 * k as u32, colour);
    }
    gpu::set_poly_g4(m, prim);
    gpu::set_semi_trans(m, prim, semi);
    corners(m, prim, 8, 8, x, y, w, h, (0, 0));
}

pub fn abi_build_g4(c: &mut Cpu) {
    let a: Vec<u32> = (4..10).map(|k| crate::abi::arg(c, k)).collect();
    build_g4(&mut c.m, c.r[4], c.r[5] != 0, c.r[6], c.r[7], a[0], a[1], [a[2], a[3], a[4], a[5]]);
}

/// `Hud_BuildF4o` (HUD.CPP, 0x800d3368): [`build_f4`] with its top edge's ends moved by `dx0`
/// and `dx1` (a slanted box).
#[allow(clippy::too_many_arguments)]
pub fn build_f4o(m: &mut Mem, prim: u32, semi: bool, x: u32, y: u32, w: u32, h: u32, colour: u32, dx0: i8, dx1: i8) {
    m.w32(prim + 4, colour);
    gpu::set_poly_f4(m, prim);
    gpu::set_semi_trans(m, prim, semi);
    corners(m, prim, 8, 4, x, y, w, h, (dx0 as i32 as u32, dx1 as i32 as u32));
}

pub fn abi_build_f4o(c: &mut Cpu) {
    let a: Vec<u32> = (4..9).map(|k| crate::abi::arg(c, k)).collect();
    build_f4o(&mut c.m, c.r[4], c.r[5] != 0, c.r[6], c.r[7], a[0], a[1], a[2], a[3] as i8, a[4] as i8);
}

/// `Hud_FBuildGT4` (HUD.CPP, 0x800d343c): [`build_gt4`] into a new primitive.
pub fn f_build_gt4(m: &mut Mem, shape: u32, x: u32, y: u32, colour: u32) {
    let prim = gpu::add_prim(m, POLY_GT4_SIZE);
    build_gt4(m, prim, shape, x, y, colour);
}

pub fn abi_f_build_gt4(c: &mut Cpu) {
    f_build_gt4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hud_FBuildFT4` (HUD.CPP, 0x800d34c8): [`build_ft4`] into a new primitive, opaque.
pub fn f_build_ft4(m: &mut Mem, shape: u32, x: u32, y: u32, colour: u32) {
    let prim = gpu::add_prim(m, POLY_FT4_SIZE);
    build_ft4(m, prim, shape, x, y, colour, false);
}

pub fn abi_f_build_ft4(c: &mut Cpu) {
    f_build_ft4(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hud_FBuildF4` (HUD.CPP, 0x800d3558; the host hooks it: called by address): [`build_f4o`]
/// into a new primitive.
#[allow(clippy::too_many_arguments)]
pub fn f_build_f4(m: &mut Mem, semi: bool, x: u32, y: u32, w: u32, h: u32, colour: u32, dx0: i8, dx1: i8) {
    let prim = gpu::add_prim(m, POLY_F4_SIZE);
    build_f4o(m, prim, semi, x, y, w, h, colour, dx0, dx1);
}

pub fn abi_f_build_f4(c: &mut Cpu) {
    let a: Vec<u32> = (4..8).map(|k| crate::abi::arg(c, k)).collect();
    f_build_f4(&mut c.m, c.r[4] != 0, c.r[5], c.r[6], c.r[7], a[0], a[1], a[2] as i8, a[3] as i8);
}

/// `uppercase` (HUD.CPP, 0x800d3618): `s`'s lower-case letters made capitals, in place.
pub fn uppercase(c: &mut Cpu, s: u32) {
    let n = call(c, STRLEN, &[s]) as i32;
    let m = &mut c.m;
    for k in 0..n.max(0) as u32 {
        let ch = m.r8(s + k);
        if ch.wrapping_sub(b'a') < 26 {
            m.w8(s + k, ch.wrapping_sub(32));
        }
    }
}

pub fn abi_uppercase(c: &mut Cpu) {
    uppercase(c, c.r[4]);
}

/// `Hud_Kill` (HUD.CPP, 0x800d3678): the HUD's shapes freed, both sprite buffers.
pub fn kill(c: &mut Cpu) {
    call(c, HUD_PMX_KILL, &[]);
    for at in SPRITES {
        let buffer = c.m.r32(at);
        if buffer != 0 {
            call(c, PURGE, &[buffer]);
        }
        c.m.w32(at, 0);
    }
}

pub fn abi_kill(c: &mut Cpu) {
    kill(c);
}

/// `Hud_Init0` (HUD.CPP, 0x800d36c8): the first player's sprite buffer, and with two players
/// (split screen) the second's.
pub fn init0(c: &mut Cpu) {
    let v = call(c, RESERVE, &[SPRITES_NAME[0], SPRITES_SIZE, 0]);
    c.m.w32(SPRITES[0], v);
    if GAME_SETUP.comm_mode(&mut c.m) == 1 {
        let v = call(c, RESERVE, &[SPRITES_NAME[1], SPRITES_SIZE, 0]);
        c.m.w32(SPRITES[1], v);
    }
}

pub fn abi_init0(c: &mut Cpu) {
    init0(c);
}

/// The HUD's elements' places (halves x, y per element; the second table for two players), the
/// one in use (`g1Player`).
const ELEMENT_POSITIONS: u32 = 0x8012_1C14;
const ELEMENT_POSITIONS_2P: u32 = ELEMENT_POSITIONS + 76;
const POSITIONS: u32 = 0x8013_F10C;

/// `Hud_InitTables` (HUD.CPP, 0x800d45c0): the elements' places for one player or two.
pub fn init_tables(m: &mut Mem) {
    let two = GAME_SETUP.num_player_race_cars(m) >= 2;
    m.w32(POSITIONS, if two { ELEMENT_POSITIONS_2P } else { ELEMENT_POSITIONS });
}

pub fn abi_init_tables(c: &mut Cpu) {
    init_tables(&mut c.m);
}

/// The digits' texture words (`HudPmx_gHudNumberUV`), 0 to 9.
const DIGITS: u32 = 0x8011_1F34;
/// A sprite's texture word.
const SPRT_UV: u32 = 12;

/// Sprite `k` of `sprites` shows `digit`.
fn set_digit(m: &mut Mem, sprites: u32, k: u32, digit: i32) {
    let uv = m.r32(DIGITS.wrapping_add((digit as u32) << 2));
    m.w32(sprites + SPRT_SIZE * k + SPRT_UV, uv);
}

/// A time in 64ths of a second as minutes, seconds (of the minute) and hundredths.
fn split_time(t: i32) -> (i32, i32, i32) {
    let seconds = t / 64;
    let hundredths = (t - seconds * 64) * 100 / 64;
    (seconds / 60, seconds % 60, hundredths)
}

/// `Hud_BuildETimeString` (HUD.CPP, 0x800d45ec): the elapsed time `t` (64ths of a second,
/// none below 0) in `sprites` as "mm:ss.hh" (the separators' sprites left alone).
pub fn build_e_time_string(m: &mut Mem, sprites: u32, t: i32) {
    let (minutes, seconds, hundredths) = split_time(t.max(0));
    let minutes = minutes % 60;
    for (k, digit) in [(0, minutes / 10), (1, minutes % 10), (3, seconds / 10), (4, seconds % 10), (6, hundredths / 10), (7, hundredths % 10)] {
        set_digit(m, sprites, k, digit);
    }
}

pub fn abi_build_e_time_string(c: &mut Cpu) {
    build_e_time_string(&mut c.m, c.r[4], c.r[5] as i32);
}

/// A car's distance along the track (`Car_tObj` +920, in sixths of a metre).
const CAR_DISTANCE: u32 = 920;
/// The players' HUD settings (`checkpointHUD`, a word each): 2 miles.
const CHECKPOINT_HUD: u32 = 0xC0;
const HUD_MILES: i32 = 2;

/// `Hud_BuildDistanceString` (HUD.CPP, 0x800d4778): player `p`'s car's distance (metres, or
/// thousandths of a mile, the game's being 1607 m) in `sprites` as "d.ddd"; whether it fits
/// (under 10000).
pub fn build_distance_string(m: &mut Mem, sprites: u32, p: u32) -> bool {
    let car = m.r32(crate::globals::HUMAN_RACE_CARS.wrapping_add(p << 2));
    let mut d = (m.r32(car + CAR_DISTANCE) as i32).wrapping_mul(6).wrapping_abs();
    if m.r32(GAME_SETUP.0.wrapping_add(CHECKPOINT_HUD).wrapping_add(p << 2)) as i32 == HUD_MILES {
        d = d.wrapping_mul(1000) / 1607;
    }
    if d >= 10000 {
        return false;
    }
    for (k, digit) in [(0, d / 1000), (2, d % 1000 / 100), (3, d % 100 / 10), (4, d % 10)] {
        set_digit(m, sprites, k, digit);
    }
    true
}

pub fn abi_build_distance_string(c: &mut Cpu) {
    c.r[2] = build_distance_string(&mut c.m, c.r[4], c.r[5]) as u32;
}

/// `Hud_BuildTimeString` (HUD.CPP, 0x800d4918): the time `t` (64ths of a second, its size) in
/// `sprites` as "m:ss.hh".
pub fn build_time_string(m: &mut Mem, sprites: u32, t: i32) {
    let (minutes, seconds, hundredths) = split_time(t.wrapping_abs());
    for (k, digit) in [(0, minutes % 10), (2, seconds / 10), (3, seconds % 10), (5, hundredths / 10), (6, hundredths % 10)] {
        set_digit(m, sprites, k, digit);
    }
}

pub fn abi_build_time_string(c: &mut Cpu) {
    build_time_string(&mut c.m, c.r[4], c.r[5] as i32);
}

/// The minutes' and the seconds' marks by language (six each).
const MINUTE_MARKS: u32 = 0x8013_EBBC;
const SECOND_MARKS: u32 = 0x8013_EBC4;
/// `Font_Getcharacter` (its +8: the character's width).
const FONT_GET_CHARACTER: fn(&mut Cpu) = nfs4rc::f_800cc4a0;

/// `Hud_BuildTimeSprites` (HUD.CPP, 0x800d3acc): `text` as the font's sprites from `x`, `y`,
/// a pixel apart; its M and S the language's minutes' and seconds' marks.
pub fn build_time_sprites(c: &mut Cpu, sprites: u32, text: u32, x: u32, y: u32) {
    let m = &mut c.m;
    let language = GAME_SETUP.user_setting().language(m) as u32;
    let minute = m.r8(MINUTE_MARKS.wrapping_add(language)) as u32;
    let second = m.r8(SECOND_MARKS.wrapping_add(language)) as u32;
    let (mut sprite, mut x) = (sprites, x);
    let mut k = 0;
    loop {
        let mut ch = c.m.r8(text.wrapping_add(k)) as u32;
        if ch == 0 {
            return;
        }
        if ch == b'M' as u32 {
            ch = minute;
        }
        if ch == b'S' as u32 {
            ch = second;
        }
        let glyph = call(c, FONT_GET_CHARACTER, &[ch]);
        let advance = (c.m.r8(glyph + 8) as i8 as i32 + 1) as u32;
        build_sprite_from_font(c, sprite, ch, x, y);
        sprite = sprite.wrapping_add(SPRT_SIZE);
        x = x.wrapping_add(advance);
        k += 1;
    }
}

pub fn abi_build_time_sprites(c: &mut Cpu) {
    build_time_sprites(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The HUD's primitives kept between frames: the flat quads (`gHudF4`, 168 bytes a player)
/// and the textured ones (`gHudFT4`, 200 a player).
const HUD_F4: u32 = 0x8013_F758;
const HUD_FT4: u32 = 0x8013_F8A8;
/// The map frame's colour and shapes: corners 14 to 17, the sides from 20 by kind, the
/// joins 24; the map's background and its rim's shapes; its element in the places' table.
const MAP_FRAME_COLOUR: u32 = 0xFF_5721;
const MAP_BACKGROUND: u32 = 120;
const MAP_RIM: [u32; 2] = [112, 113];
const GREY: u32 = 0x80_8080;
const MAP_ELEMENT: u32 = 14;

/// `Hud_InitMapFrame` (HUD.CPP, 0x800d3720): player `p`'s map frame (`kind` 1: 30 pixels
/// further left; its sides' shapes by kind) as sprites in the player's sprite buffer: the four
/// corners, the top and bottom sides, the joins; the map's black back, its background
/// (mirrored on a mirrored track) and its rim.
pub fn init_map_frame(c: &mut Cpu, p: u32, kind: u32) {
    let m = &mut c.m;
    let sprites = m.r32(SPRITES[(p != 0) as usize]);
    let f4 = HUD_F4 + if p != 0 { 168 } else { 0 };
    let ft4 = HUD_FT4 + if p != 0 { 200 } else { 0 };
    let dy = if p != 0 { (-13i32) as u32 } else { 0 };
    let (corner_w, corner_h) = shape_wh(m, shape(14));
    let (side_w, side_h) = shape_wh(m, shape(20));
    let (join_w, join_h) = shape_wh(m, shape(24));
    // the frame's place (the left as moved for kind 1; the right side's from the place itself)
    let place = |m: &mut Mem| {
        let at = m.r32(POSITIONS) + 4 * MAP_ELEMENT;
        let x = m.r16(at) as i16 as i32 as u32;
        let y = (m.r16(at + 2) as i16 as i32 as u32).wrapping_add(m.r32(MAP_OFFSET_Y)).wrapping_add(dy);
        (x, y)
    };
    let (left, _) = place(m);
    let x = if kind == 1 { left.wrapping_sub(30) } else { left };
    m.w32(SPRITE_COLOR, MAP_FRAME_COLOUR);
    m.w8(SPRITE_TRANSPARENT, 0);
    let right = |x0: u32| x0.wrapping_add(corner_w).wrapping_add(side_w);
    let bottom = corner_h.wrapping_add(join_h);
    let sprite = |k: u32| sprites.wrapping_add(1260 + SPRT_SIZE * k);
    let (x0, y) = place(m);
    build_sprite2(m, sprite(0), 14, x, y);
    build_sprite2(m, sprite(1), 15, right(x0), y);
    build_sprite2(m, sprite(2), 16, x, y.wrapping_add(bottom));
    build_sprite2(m, sprite(3), 17, right(x0), y.wrapping_add(bottom));
    let side = kind + 20;
    let mid = x.wrapping_add(corner_w);
    build_sprite2(m, sprite(4), side, mid, y);
    build_sprite2(m, sprite(5), side, mid, y.wrapping_add(bottom).wrapping_add(corner_h.wrapping_sub(side_h)));
    build_sprite2(m, sprite(6), 24, x, y.wrapping_add(corner_h));
    build_sprite2(m, sprite(7), 24, right(x0).wrapping_add(corner_w.wrapping_sub(join_w)), y.wrapping_add(corner_h));
    build_f4(m, f4 + 96, true, 0, 0, 45, 48, 0);
    if GAME_SETUP.mirror_track(m) != 0 {
        build_map_mirror_ft4(m, ft4, shape(MAP_BACKGROUND), 0, 0, GREY, true);
    } else {
        build_ft4(m, ft4, shape(MAP_BACKGROUND), 0, 0, GREY, true);
    }
    for (k, rim) in MAP_RIM.into_iter().enumerate() {
        let at = ft4 + 40 + 80 * k as u32;
        build_ft4(m, at, shape(rim), 0, 0, GREY, true);
        let (w, _) = shape_wh(m, shape(rim));
        build_mirror_ft4(m, at + 40, shape(rim), w, 0, GREY, true);
    }
}

pub fn abi_init_map_frame(c: &mut Cpu) {
    init_map_frame(c, c.r[4], c.r[5]);
}

/// The pads' library state (`gPadinfo`, 32 bytes a pad: +5 its type, 35 a NeGcon).
const PAD_INFO: u32 = 0x8013_FBA4;
const PAD_TYPE: u32 = 5;
const NEGCON: u8 = 35;
/// The pad buttons' shapes: the cross (a NeGcon's A), the triangle and the circle (the
/// NeGcon's B and its I), the square (the NeGcon's II), the select-like `&`.
const BUTTON_CROSS: [u32; 2] = [170, 173];
const BUTTON_LEFT: [u32; 2] = [169, 171];
const BUTTON_RIGHT: [u32; 2] = [168, 172];
const BUTTON_AND: u32 = 174;

fn pad_is_negcon(m: &mut Mem, pad: u32) -> bool {
    m.r8(PAD_INFO.wrapping_add(pad << 5).wrapping_add(PAD_TYPE)) == NEGCON
}

/// The shape of text character `ch` (and how far down it sits: -1 for the accented capitals):
/// digits, `-` and `:`, the accented capitals (0xC0..0xDC, `å`), the letters.
fn char_shape(ch: u8, negcon: bool) -> (u32, u32) {
    let pad = negcon as usize;
    match ch {
        b'^' => (BUTTON_CROSS[pad], 0),
        b'(' => (BUTTON_LEFT[pad], 0),
        b')' => (BUTTON_RIGHT[pad], 0),
        b'&' => (BUTTON_AND, 0),
        b'0'..=b'9' => ((ch as u32 + 110) & 0xFF, 0),
        b'-' => (72, 0),
        b':' => (73, 0),
        0xC0..=0xDC => ((ch as u32).wrapping_sub(118) & 0xFF, u32::MAX),
        0xE5 => (103, u32::MAX),
        _ => ((ch as u32 + 67) & 0xFF, 0),
    }
}

/// `Hud_BuildString` (HUD.CPP, 0x800d4e9c; the host hooks it, called by address): `text` in
/// the HUD's letters from `x`, `y` in `colour` (only measured when `measure`), a space 3 pixels,
/// each character a pixel apart; `*` the accept button (the first pad's, and with two players
/// the second's after it when the pads differ), `^`, `(`, `)` and `&` the buttons of pad `pad`.
/// The width; on the HUD's texture page meanwhile.
#[allow(clippy::too_many_arguments)]
pub fn build_string(c: &mut Cpu, text: u32, x: u32, y: u32, colour: u32, pad: u32, measure: bool) -> u32 {
    go_tpage(c, 1);
    let n = call(c, STRLEN, &[text]) as i32;
    let m = &mut c.m;
    let comm_mode = GAME_SETUP.comm_mode(m);
    let start = x;
    let mut x = x;
    let button = |m: &mut Mem, x: &mut u32, negcon: bool| {
        let shape = BUTTON_CROSS[negcon as usize];
        if !measure {
            f_build_sprite(m, shape, *x, y, colour, false);
        }
        let (w, _) = shape_wh(m, self::shape(shape));
        *x = x.wrapping_add(3).wrapping_add(w);
    };
    for k in 0..n.max(0) as u32 {
        let ch = m.r8(text.wrapping_add(k));
        match ch {
            b' ' => x = x.wrapping_add(3),
            b'*' => {
                x = x.wrapping_add(2);
                let first = pad_is_negcon(m, 0);
                button(m, &mut x, first);
                let second = pad_is_negcon(m, 1);
                if comm_mode == 1 && first != second {
                    button(m, &mut x, second);
                }
            }
            _ => {
                let (shape, dy) = char_shape(ch, pad_is_negcon(m, pad));
                if !measure {
                    f_build_sprite(m, shape, x, y.wrapping_add(dy), colour, false);
                }
                let (w, _) = shape_wh(m, self::shape(shape));
                x = x.wrapping_add(1).wrapping_add(w);
            }
        }
    }
    go_tpage(c, 0);
    x.wrapping_sub(start)
}

pub fn abi_build_string(c: &mut Cpu) {
    let (pad, measure) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    c.r[2] = build_string(c, c.r[4], c.r[5], c.r[6], c.r[7], pad, measure != 0);
}

/// The tachometer's needle colours by car (30, by day and by night), the engine's reading
/// (`DashHUD_gInfo` +0x40, its revs) and the needle's angle at rest and at most (of a turn's
/// 0x10000).
const DAY_NEEDLE: u32 = 0x8012_1CAC;
const NIGHT_NEEDLE: u32 = 0x8012_1D24;
const ENGINE_READING: u32 = 0x8011_3A28;
const NEEDLE_REST: i32 = 0x5999;
const NEEDLE_MAX: i32 = 0x1_3334;
/// The dial's sprite (the third of the player's buffer), its shapes (the second player's on the
/// texture's right: its u from 117, not 29), the needle tip's shape; the dial's centre.
const DIAL_SPRITE: u32 = 40;
const DIAL_SHAPE: [u32; 2] = [129, 131];
const DIAL_U: [i32; 2] = [29, 117];
const DIAL_V: i32 = 157;
const NEEDLE_SHAPE: u32 = 130;
const DIAL_CENTRE: i32 = 14;
/// A flat line (`LINE_F2`, 16 bytes) and a flat triangle (`POLY_F3`, 20).
const LINE_F2_SIZE: u32 = 16;
const LINE_F2_CODE: u32 = 0x42 << 24;
const POLY_F3_SIZE: u32 = 20;

/// `Hud_BuildTach` (HUD.CPP, 0x800d4a68): player `p`'s tachometer: the needle's angle from the
/// engine's revs (from at rest to its limit); the dial's sprite's texture turned with it; the
/// needle a line in the car's needle colour (by night or by day) brightened, its tip a triangle
/// in that colour over a black one 2 pixels lower, round the dial's centre; the dial's sprite
/// linked last (drawn under them).
pub fn build_tach(m: &mut Mem, p: u32) {
    let second = (p != 0) as usize;
    let sprites = m.r32(SPRITES[second]);
    let car = GAME_SETUP.car_info(p).car_type(m).min(29);
    let needles = if GAME_SETUP.time(m) != 0 { NIGHT_NEEDLE } else { DAY_NEEDLE };
    let colour = m.r32(needles.wrapping_add((car as u32) << 2));
    let revs = (m.r32(ENGINE_READING) << 16) as i32;
    let angle = (revs / 10800 + NEEDLE_REST).clamp(NEEDLE_REST, NEEDLE_MAX);
    let (sin, cos) = fixedsincos(m, angle);
    let page = m.r32(shape(DIAL_SHAPE[second])) & 0xFFFF_0000;
    let (dx, dy) = (fixedmult(cos, 29), fixedmult(sin, 29));
    let uv = page | ((dy + DIAL_V) as u32) << 8 | (dx + DIAL_U[second]) as u32;
    m.w32(sprites + DIAL_SPRITE + SPRT_UV, uv);
    let tip_x = (fixedmult(cos, 10) + DIAL_CENTRE) as u32;
    let tip_y = (fixedmult(sin, 10) + DIAL_CENTRE) as u32;
    let tip = gpu::add_prim(m, POLY_F3_SIZE);
    let line = gpu::add_prim(m, LINE_F2_SIZE);
    m.w8(line + 3, 3);
    m.w32(line + 4, colour.wrapping_add(0x48_4848) | LINE_F2_CODE);
    m.w16(line + 8, (DIAL_CENTRE - dx) as u16);
    m.w16(line + 10, (DIAL_CENTRE - dy) as u16);
    m.w16(line + 12, tip_x as u16);
    m.w16(line + 14, tip_y as u16);
    let shadow = gpu::add_prim(m, POLY_F3_SIZE);
    let needle = shape(NEEDLE_SHAPE);
    build_f3(m, tip, needle, tip_x, tip_y, colour);
    build_f3(m, shadow, needle, tip_x, tip_y, 0);
    gpu::set_semi_trans(m, tip, false);
    // the tip's back corners a quarter-turn (of 2048) either side, 32 from the centre
    for (vertex, turn) in [(12, -512), (16, 512)] {
        let (sin, cos) = fixedsincos(m, angle + turn);
        let x = (DIAL_CENTRE - fixedmult(cos, 32)) as u16;
        m.w16(tip + vertex, x);
        m.w16(shadow + vertex, x);
        let y = (DIAL_CENTRE - fixedmult(sin, 32)) as u16;
        m.w16(tip + vertex + 2, y);
        m.w16(shadow + vertex + 2, y);
    }
    for at in [10, 18, 14] {
        let y = m.r16(shadow + at).wrapping_add(2);
        m.w16(shadow + at, y);
    }
    link(m, sprites + DIAL_SPRITE);
}

pub fn abi_build_tach(c: &mut Cpu) {
    build_tach(&mut c.m, c.r[4]);
}

/// A primitive kept elsewhere linked into the ordering table (`addPrim`).
fn link(m: &mut Mem, prim: u32) {
    let ot = m.r32(gpu::OT);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    let entry = m.r32(ot);
    m.w32(ot, (entry & 0xFF00_0000) | (prim & 0x00FF_FFFF));
}

/// The texture pages' drawing modes (`gTPage0`, `gTPage1`: 4 a player, 12 bytes each, 48 a
/// player), pages 2 and 3.
const TPAGE: [u32; 2] = [0x8013_F698, 0x8013_F6F8];
/// The gauges' bars (`gHudG4`, 144 bytes a player).
const HUD_G4: u32 = 0x8013_FA38;
/// The players on the HUD less one (`DashHUD_gInfo` +0: 1 split screen).
const HUD_LAST_PLAYER: u32 = 0x8011_39E8;
/// The time sprites' texts: the race's time, the laps, the lap's time, the split's distance.
const RACE_TIME_TEXT: u32 = 0x8005_7090;
const LAPS_TEXT: u32 = 0x8013_EBCC;
const LAP_TIME_TEXT: u32 = 0x8013_EBD0;
const DISTANCE_TEXT: u32 = 0x8013_EBD8;
/// The split's marks' heights against its first digit's (`HudSplitTimeDiff1`, 2: a word a
/// player).
const SPLIT_TIME_DIFF: [u32; 2] = [0x8013_F120, 0x8013_F128];
/// The pursuit's state the HUD shows: the bonus time, the countdowns, whether the user has the
/// controls, the "busted" overlay, the perp's overlay and its message (a word a player), the
/// song to turn off next.
const BTC_BONUS_TIME: u32 = 0x8013_F130;
const BTC_COUNTDOWN: u32 = 0x8013_EC14;
const FINAL_BTC_COUNTDOWN: u32 = 0x8013_EC18;
const BTC_USER_HAS_CONTROL: u32 = 0x8013_F138;
const BUSTED_OVERLAY: u32 = 0x8013_EC08;
const PERP_OVERLAY_ON: u32 = 0x8013_F140;
const PERP_OVERLAY_MESSAGE: u32 = 0x8013_F148;
const TURN_SONG_OFF_NEXT: u32 = 0x8013_EC20;
const INIT_CD_PLAYER: fn(&mut Cpu) = nfs4rc::f_800d6f8c;
const HUD_RESET: fn(&mut Cpu) = nfs4rc::f_800da3dc;

/// The dashboard's colours: its text, its sprites, its bars' grey.
const TEXT_COLOUR: u32 = 0xA0_A0A0;
const BAR_GREY: u32 = 0x70_7070;
/// A gauge's bar's end (and its underline's place below it).
const BAR_END: u32 = 125;

/// Element `e`'s place on the HUD (`g1Player`'s table).
fn element(m: &mut Mem, e: u32) -> (u32, u32) {
    let at = m.r32(POSITIONS).wrapping_add(4 * e);
    (m.r16(at) as i16 as i32 as u32, m.r16(at + 2) as i16 as i32 as u32)
}

/// Two places added (an element's and its part's).
fn plus((x, y): (u32, u32), (dx, dy): (u32, u32)) -> (u32, u32) {
    (x.wrapping_add(dx), y.wrapping_add(dy))
}

/// A gauge on the dashboard from `x`, `y`: its label (shape `label`, `label_w` wide) as sprite
/// `label_prim`; a bar `w` wide and 10 high after it fading from black to grey (`g4`); the
/// bar's end as sprite `end_prim` (in the current sprite colour, or `end_colour` opaque) and
/// the end's underline (`f4`, 7 by 3, 7 down).
#[allow(clippy::too_many_arguments)]
fn gauge(m: &mut Mem, label_prim: u32, label: u32, label_w: u32, (x, y): (u32, u32), g4: u32, w: u32, end_prim: u32, f4: u32, end_colour: Option<u32>) {
    build_sprite2(m, label_prim, label, x, y);
    let bar = x.wrapping_add(label_w);
    build_g4(m, g4, true, bar, y, w, 10, [0, BAR_GREY, 0, BAR_GREY]);
    let end = bar.wrapping_add(w);
    build_f4(m, f4, true, end, y.wrapping_add(7), 7, 3, BAR_GREY);
    match end_colour {
        None => build_sprite2(m, end_prim, BAR_END, end, y),
        Some(colour) => build_sprite(m, end_prim, BAR_END, end, y, colour, false),
    }
}

/// `Hud_Init` (HUD.CPP, 0x800d3c00): the HUD's primitives kept between frames: the texture
/// pages' drawing modes; per player ([`init_player`]) the dashboard's sprites, gauges, time
/// sprites and map frame; the replay's controls (the first player's sprites, half-transparent);
/// the CD player; the HUD reset; no pursuit's countdowns, overlays or bonus time.
pub fn init(c: &mut Cpu) {
    for p in 0..2 {
        for k in 0..4 {
            for (page, modes) in [(2, TPAGE[0]), (3, TPAGE[1])] {
                call(c, SET_DRAW_MODE, &[modes + 48 * p + 12 * k, 0, 0, page, 0]);
            }
        }
    }
    let mut p = 0;
    while p as i32 <= c.m.r32(HUD_LAST_PLAYER) as i32 {
        init_player(c, p);
        p += 1;
    }
    let m = &mut c.m;
    m.w32(SPRITE_COLOR, GREY);
    m.w8(SPRITE_TRANSPARENT, 1);
    let sprites = m.r32(SPRITES[0]);
    let (x, y) = element(m, 13);
    for (prim, k, dx, dy) in [(1100, 111, 0, 0), (1020, 108, 18, 0), (1040, 110, 37, 0), (1060, 63, 58, 0), (1120, 114, 74, 0), (1080, 0, 109, -7)] {
        build_sprite2(m, sprites + prim, k, x.wrapping_add(dx), y.wrapping_add(dy as u32));
    }
    build_sprite2(m, sprites + 1140, 3, 0, y.wrapping_add(4));
    m.w8(SPRITE_TRANSPARENT, 0);
    call(c, INIT_CD_PLAYER, &[]);
    call(c, HUD_RESET, &[]);
    let m = &mut c.m;
    for at in [BTC_BONUS_TIME, BTC_COUNTDOWN, FINAL_BTC_COUNTDOWN, BTC_USER_HAS_CONTROL, BUSTED_OVERLAY] {
        m.w32(at, 0);
    }
    for p in 0..2 {
        m.w32(PERP_OVERLAY_ON + 4 * p, 0);
        m.w32(PERP_OVERLAY_MESSAGE + 4 * p, 0);
    }
    m.w32(TURN_SONG_OFF_NEXT, 0);
}

pub fn abi_init(c: &mut Cpu) {
    init(c);
}

/// [`init`]'s player `p`: in the player's sprite buffer and bars (the second player's 15
/// higher), from the HUD's elements' places: element 0's back (up 16 for each of the time and
/// the lap count hidden), element 1's, the tachometer's dial; four gauges (the race's time,
/// the laps (up 16 with the time hidden), the lap's time, the split: as the cop by the race
/// time's, its bar shorter); their time sprites in the text colour (the split's a distance with
/// the HUD's checkpoints so set); the split's marks' heights; the sprites by the map and the
/// gear's; the map's frame.
fn init_player(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let second = (p != 0) as usize;
    let sprites = m.r32(SPRITES[second]);
    let f4 = HUD_F4 + if p != 0 { 168 } else { 0 };
    let g4 = HUD_G4 + if p != 0 { 144 } else { 0 };
    let dy = if p != 0 { (-15i32) as u32 } else { 0 };
    let car = GAME_SETUP.car_info(p);
    let no_time = if car.hud_time(m) == 0 { 16 } else { 0 };
    let no_laps = if car.hud_lapnum(m) == 0 { 16 } else { 0 };
    let at = |m: &mut Mem, e: u32| plus(element(m, e), (0, dy));
    let cop = m.r32(BE_THE_COP) != 0;
    let distance = m.r32(GAME_SETUP.0.wrapping_add(CHECKPOINT_HUD).wrapping_add(4 * p)) != 0;

    let (x, y) = at(m, 0);
    build_sprite(m, sprites, 104, x, y.wrapping_sub(no_time).wrapping_sub(no_laps), 0xBEBE, false);
    m.w32(SPRITE_COLOR, GREY);
    let (x, y) = at(m, 1);
    build_sprite2(m, sprites + 20, 128, x, y);
    build_sprite2(m, sprites + DIAL_SPRITE, DIAL_SHAPE[second], 0, 0);
    m.w32(sprites + DIAL_SPRITE + 16, xy(28, 28));

    let (label_w, _) = shape_wh(m, shape(107));
    let race_time = at(m, 2);
    gauge(m, sprites + 80, 105, label_w, race_time, g4, if cop { 46 } else { 70 }, sprites + 100, f4, None);
    let laps = plus(at(m, 3), (0, no_time.wrapping_neg()));
    gauge(m, sprites + 120, 107, label_w, laps, g4 + 36, 29, sprites + 140, f4 + 24, None);
    let lap_time = at(m, 4);
    gauge(m, sprites + 160, 105, label_w, lap_time, g4 + 108, 60, sprites + 180, f4 + 48, None);
    let (split_w, _) = shape_wh(m, shape(118));
    let w = if cop { 38 } else if distance { 50 } else { 61 };
    let split = if cop { plus(race_time, (14, 14)) } else { at(m, 5) };
    gauge(m, sprites + 200, 118, split_w, split, g4 + 72, w, sprites + 220, f4 + 72, Some(GREY));

    m.w32(SPRITE_COLOR, TEXT_COLOUR);
    let race_time_text = plus(race_time, element(m, 6));
    build_time_sprites(c, sprites + 240, RACE_TIME_TEXT, race_time_text.0, race_time_text.1);
    let laps_text = plus(laps, element(&mut c.m, 7));
    build_time_sprites(c, sprites + 400, LAPS_TEXT, laps_text.0, laps_text.1);
    c.m.w32(SPRITE_COLOR, TEXT_COLOUR);
    let lap_time_text = plus(lap_time, element(&mut c.m, 8));
    build_time_sprites(c, sprites + 460, LAP_TIME_TEXT, lap_time_text.0, lap_time_text.1.wrapping_add(1));
    let m = &mut c.m;
    // (element 9 only across: the split's text on its gauge's line)
    let (x, y) = if cop { plus(race_time, (16, 12)) } else { let (x, y) = at(m, 5); (x.wrapping_add(element(m, 9).0), y.wrapping_add(1)) };
    let x = x.wrapping_add(split_w);
    let text = if distance { DISTANCE_TEXT } else { LAP_TIME_TEXT };
    build_time_sprites(c, sprites + 600, text, x, y);

    // the split's marks' heights against its first digit's (its second and fifth characters)
    let m = &mut c.m;
    let char_y = |m: &mut Mem, k: u32| m.r16(sprites + 600 + SPRT_SIZE * k + 10) as i16 as i32;
    let first = char_y(m, 0);
    let mark = char_y(m, 1);
    m.w32(SPLIT_TIME_DIFF[0] + 4 * p, mark.wrapping_sub(first) as u32);
    let mark = char_y(m, 4);
    m.w32(SPRITE_COLOR, TEXT_COLOUR);
    m.w32(SPLIT_TIME_DIFF[1] + 4 * p, mark.wrapping_sub(first) as u32);

    // elements 14 (the map's) and 10's sprites (2 higher for the first of two players)
    let (map_x, _) = element(m, 14);
    let (x, mut y) = plus(plus(element(m, 14), element(m, 10)), (0, m.r32(MAP_OFFSET_Y).wrapping_add(dy)));
    if p == 0 && m.r32(HUD_LAST_PLAYER) != 0 {
        y = y.wrapping_sub(2);
    }
    let (w44, _) = shape_wh(m, shape(44));
    let (w71, _) = shape_wh(m, shape(71));
    build_sprite(m, sprites + 800, 62, map_x.wrapping_add(19), y.wrapping_sub(1), GREY, false);
    build_sprite2(m, sprites + 740, 44, x, y);
    build_sprite2(m, sprites + 760, 71, x.wrapping_add(w44), y);
    build_sprite2(m, sprites + 780, 54, x.wrapping_add(w44).wrapping_add(w71), y);

    // the gear's (R, N, 1 to 6) by element 1
    m.w32(SPRITE_COLOR, GREY);
    let (x, y) = plus(at(m, 1), element(m, 11));
    build_sprite2(m, sprites + 980, 26, x.wrapping_sub(1), y);
    build_sprite2(m, sprites + 1000, 27, x.wrapping_sub(2), y.wrapping_sub(1));
    for k in 0..8 {
        build_sprite2(m, sprites + 820 + SPRT_SIZE * k, 28 + k, x.wrapping_add(2), y.wrapping_add(6));
    }
    init_map_frame(c, p, 0);
}

/// The dashboard's state (`DashHUD_gInfo`): the race's time flashing (+0x14), the race's time
/// (+0x34), a player's warning (+0x2C, a word a player: element 0's sprite flashing, no split).
const RACE_TIME_FLASHING: u32 = 0x8011_39FC;
const RACE_TIME: u32 = 0x8011_3A1C;
const HUD_WARNING: u32 = 0x8011_3A14;
/// The split gauge's label by its sign: behind (or a distance ahead) and ahead (or behind).
const SPLIT_BEHIND: u32 = 118;
const SPLIT_AHEAD: u32 = 119;
/// A car's split at the last checkpoint against the record (64ths of a second, +916) and the
/// checkpoints it has passed (+924); a split shown only within ten minutes.
const CAR_SPLIT: u32 = 916;
const CAR_CHECKPOINTS: u32 = 924;
const SPLIT_LIMIT: i32 = 600 * 64;
/// The split gauge's primitives: its label and end (sprites 10, 11), its bar and underline (in
/// the player's bars), its text (from sprite 30).
const SPLIT_LABEL: u32 = 10;
const SPLIT_END: u32 = 11;
const SPLIT_BAR: u32 = 72;
const SPLIT_TEXT: u32 = 30;

/// `Hud_BuildNumbers0` (HUD.CPP, 0x800d51f8): player `p`'s race time and split gauges into the
/// ordering table: the race's time (the countdown as the cop; flashing when so) with its gauge;
/// the split's gauge moved to its line (not as the cop); the split: the bonus time as the cop,
/// else with checkpoints on the car's last split (within ten minutes, no warning on) or its
/// distance, the label by its sign.
pub fn build_numbers0(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let sprites = m.r32(SPRITES[(p != 0) as usize]);
    let sprite = |k: u32| sprites.wrapping_add(SPRT_SIZE * k);
    let f4 = HUD_F4 + if p != 0 { 168 } else { 0 };
    let g4 = HUD_G4 + if p != 0 { 144 } else { 0 };
    let dy = if p != 0 { (-15i32) as u32 } else { 0 };
    let car = GAME_SETUP.car_info(p);
    let cop = m.r32(BE_THE_COP) != 0;
    if car.hud_time(m) != 0 {
        let flashed_off = m.r32(RACE_TIME_FLASHING) != 0 && crate::globals::SIM_COUNTER.get(m) & 0x10 != 0;
        if !flashed_off {
            let t = m.r32(if cop { BTC_COUNTDOWN } else { RACE_TIME });
            build_e_time_string(m, sprite(12), t as i32);
        }
        let digits = if cop { 5 } else { 8 };
        for k in 12..12 + digits {
            link(m, sprite(k));
        }
        for k in 4..6 {
            link(m, sprite(k));
        }
        link(m, g4);
        link(m, f4);
    }
    if !cop {
        // (on element 5's line moved by element 9's height)
        let y = element(m, 5).1.wrapping_add(dy).wrapping_add(element(m, 9).1) as u16;
        let (bar, underline) = (g4 + SPLIT_BAR, f4 + SPLIT_BAR);
        m.w16(sprite(SPLIT_LABEL) + 10, y);
        for (vertex, y) in [(10, y), (18, y), (26, y + 10), (34, y + 10)] {
            m.w16(bar + vertex, y);
        }
        m.w16(sprite(SPLIT_END) + 10, y);
        for (vertex, y) in [(10, y + 7), (14, y + 7), (18, y + 10), (22, y + 10)] {
            m.w16(underline + vertex, y);
        }
        // the text a pixel lower, its marks (second and fifth characters) by their heights
        let y = y.wrapping_add(1);
        for k in 0..7 {
            let mark = match k {
                1 => m.r16(SPLIT_TIME_DIFF[0] + 4 * p),
                4 => m.r16(SPLIT_TIME_DIFF[1] + 4 * p),
                _ => 0,
            };
            m.w16(sprite(SPLIT_TEXT + k) + 10, mark.wrapping_add(y));
        }
    }
    let label = sprite(SPLIT_LABEL) + SPRT_UV;
    let bonus = m.r32(BTC_BONUS_TIME) as i32;
    let characters = if bonus != 0 && cop {
        if car.hud_time(m) == 0 {
            return;
        }
        let uv = m.r32(shape(SPLIT_BEHIND));
        m.w32(label, uv);
        build_time_string(m, sprite(SPLIT_TEXT), bonus.max(0));
        4
    } else {
        if GAME_SETUP.checkpoint_type(m) == 0 {
            return;
        }
        let human = m.r32(crate::globals::HUMAN_RACE_CARS.wrapping_add(4 * p));
        if GAME_SETUP.checkpoint_hud(m, p) == 0 {
            let split = m.r32(human + CAR_SPLIT) as i32;
            let shown = m.r32(human + CAR_CHECKPOINTS) as i32 > 0 && split > -SPLIT_LIMIT && split < SPLIT_LIMIT;
            if !shown || cop || m.r32(HUD_WARNING + 4 * p) != 0 {
                return;
            }
            let uv = m.r32(shape(if split < 0 { SPLIT_AHEAD } else { SPLIT_BEHIND }));
            m.w32(label, uv);
            build_time_string(m, sprite(SPLIT_TEXT), split);
            7
        } else {
            if !build_distance_string(m, sprite(SPLIT_TEXT), p) {
                return;
            }
            let distance = m.r32(human + CAR_DISTANCE) as i32;
            let uv = m.r32(shape(if distance < 0 { SPLIT_AHEAD } else { SPLIT_BEHIND }));
            m.w32(label, uv);
            5
        }
    };
    for k in 0..characters {
        link(&mut c.m, sprite(SPLIT_TEXT + k));
    }
    // the gauge's label and end on the HUD's first texture page
    go_tpage(c, 0);
    link(&mut c.m, sprite(SPLIT_LABEL));
    go_tpage(c, 1);
    let m = &mut c.m;
    link(m, sprite(SPLIT_END));
    link(m, g4 + SPLIT_BAR);
    link(m, f4 + SPLIT_BAR);
}

pub fn abi_build_numbers0(c: &mut Cpu) {
    build_numbers0(c, c.r[4]);
}

/// More of the dashboard's state (`DashHUD_gInfo`): the lap and the laps (+0x38, +0x3C), the
/// gear (+0x48: 0 reverse, 1 neutral), the speed (+0x4C), the position and the racers (+0x54,
/// +0x58), the lap's time (+0x5C).
const LAP: u32 = 0x8011_3A20;
const LAPS: u32 = 0x8011_3A24;
const GEAR: u32 = 0x8011_3A30;
const SPEED: u32 = 0x8011_3A34;
const POSITION: u32 = 0x8011_3A3C;
const RACERS: u32 = 0x8011_3A40;
const LAP_TIME: u32 = 0x8011_3A44;
/// Whether the CD player has been shown (`Hud_gShowedCDPlayer`: no lap time then).
const SHOWED_CD_PLAYER: u32 = 0x8013_F150;
/// The big digits' shapes (0 to 9 from 44; the position's and racers' from 44 and 53).
const BIG_DIGITS: u32 = 44;
const RACERS_DIGITS: u32 = 53;
/// The speed's digits' colours: top and bottom.
const SPEED_TOP: u32 = 0xC8_C8C8;
const SPEED_BOTTOM: u32 = 0x50_5050;

/// The speed's digits for `speed` (16.16, whole units toward zero): units, tens, hundreds.
fn speed_digits(speed: i32) -> (i32, i32, i32) {
    let whole = (if speed < 0 { speed + 0xFFFF } else { speed }) >> 16;
    (whole % 10, whole / 10 % 10, whole / 100)
}

/// One of the speed's digits at `x`, `y`: shape `BIG_DIGITS + digit`, lighter at the top.
fn speed_digit(m: &mut Mem, digit: i32, x: u32, y: u32) {
    let prim = gpu::add_prim(m, POLY_GT4_SIZE);
    build_gt4(m, prim, shape(BIG_DIGITS.wrapping_add(digit as u32)), x, y, SPEED_TOP);
    m.w32(prim + 40, SPEED_BOTTOM);
    m.w32(prim + 28, SPEED_BOTTOM);
}

/// `Hud_BuildNumbers` (HUD.CPP, 0x800d5a44): player `p`'s other numbers into the ordering
/// table, each as its HUD setting has it (and not as the cop): the lap of the laps (not in a
/// one-lap race) with its gauge; the lap's time (within ten minutes; flashing when the race's
/// time does; none once the CD player showed) with its gauge; the position of the racers (the
/// map's mark with the map); the gear and the speed's units with the tachometer, the speed in
/// big digits (the ones and sevens narrower); element 0's sprite flashing with a warning.
pub fn build_numbers(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let sprites = m.r32(SPRITES[(p != 0) as usize]);
    let sprite = |k: u32| sprites.wrapping_add(SPRT_SIZE * k);
    let f4 = HUD_F4 + if p != 0 { 168 } else { 0 };
    let g4 = HUD_G4 + if p != 0 { 144 } else { 0 };
    let dy = if p != 0 { (-15i32) as u32 } else { 0 };
    let car = GAME_SETUP.car_info(p);
    let cop = m.r32(BE_THE_COP) != 0;
    let one_lap = m.r32(LAPS) == 1;
    if car.hud_lapnum(m) != 0 && !cop && !one_lap {
        // "0/0": sprites 20 to 22
        let digit = |m: &mut Mem, at: u32| {
            let n = m.r32(at);
            m.r32(DIGITS.wrapping_add(n << 2))
        };
        let lap = digit(m, LAP);
        m.w32(sprite(20) + SPRT_UV, lap);
        let laps = digit(m, LAPS);
        m.w32(sprite(22) + SPRT_UV, laps);
        for k in (20..23).chain(6..8) {
            link(m, sprite(k));
        }
        link(m, g4 + 36);
        link(m, f4 + 24);
    }
    let lap_time = m.r32(LAP_TIME) as i32;
    if car.hud_time(m) != 0 && lap_time != 0 && lap_time <= SPLIT_LIMIT - 1 && !cop && m.r32(SHOWED_CD_PLAYER) == 0 && !one_lap {
        let flashed_off = m.r32(RACE_TIME_FLASHING) != 0 && crate::globals::SIM_COUNTER.get(m) & 0x10 != 0;
        if !flashed_off {
            build_time_string(m, sprite(23), lap_time);
        }
        for k in (23..30).chain(8..10) {
            link(m, sprite(k));
        }
        link(m, g4 + 108);
        link(m, f4 + 48);
    }
    if car.hud_position(m) != 0 && !cop && m.r32(RACERS) as i32 >= 2 {
        // the position right-aligned on the first digit's place, the racers' count after it
        let position = shape(BIG_DIGITS.wrapping_add(m.r32(POSITION)));
        let racers = shape(RACERS_DIGITS.wrapping_add(m.r32(RACERS)));
        let (x, _) = plus(element(m, 14), element(m, 10));
        let (w0, _) = shape_wh(m, shape(BIG_DIGITS));
        let (w, _) = shape_wh(m, position);
        m.w16(sprite(37) + 8, x.wrapping_add(w0).wrapping_sub(2).wrapping_sub(w) as u16);
        let uv = m.r32(position);
        m.w32(sprite(37) + SPRT_UV, uv);
        m.w16(sprite(37) + 16, w as u16);
        let uv = m.r32(racers);
        m.w32(sprite(39) + SPRT_UV, uv);
        let (w, _) = shape_wh(m, racers);
        m.w16(sprite(39) + 16, w as u16);
        for k in 37..40 {
            link(m, sprite(k));
        }
        if car.hud_map(m) != 0 {
            go_tpage(c, 0);
            link(&mut c.m, sprite(40));
            go_tpage(c, 1);
        }
    }
    go_tpage(c, 0);
    let m = &mut c.m;
    if car.hud_tach(m) != 0 {
        // the gear's sprites: 41 on for the gears, reverse and neutral last
        let gear = match m.r32(GEAR) {
            0 => 48,
            1 => 47,
            g => 39u32.wrapping_add(g),
        };
        link(m, sprite(gear));
        let speed = if car.hud_speed(m) == 0 { 50 } else { 49 };
        link(m, sprite(speed));
    }
    go_tpage(c, 1);
    let m = &mut c.m;
    if car.hud_tach(m) != 0 {
        let speed = fixedmult(car.hud_speed_mult(m), m.r32(SPEED) as i32);
        let (units, tens, hundreds) = speed_digits(speed);
        let width = |m: &mut Mem, digit: i32| shape_wh(m, shape(BIG_DIGITS.wrapping_add(digit as u32))).0 as i32;
        // a digit's step: the zero's width and a pixel; half that and a one's or a seven's
        let step = width(m, 0) + 1;
        let one = (step + width(m, 1)) >> 1;
        let seven = (step + width(m, 7)) >> 1;
        let (x, y) = plus(element(m, 1), element(m, 12));
        let y = y.wrapping_add(dy);
        let mut x = x.wrapping_add(4).wrapping_add(2 * step as u32);
        speed_digit(m, units, x, y);
        x = x.wrapping_sub(match tens {
            1 => one,
            7 => seven,
            _ => step,
        } as u32);
        if hundreds != 0 || tens != 0 {
            speed_digit(m, tens, x, y);
        }
        if tens == 1 {
            x = x.wrapping_sub((step - one) as u32);
        }
        x = x.wrapping_sub(1).wrapping_sub(width(m, hundreds) as u32);
        if hundreds != 0 {
            speed_digit(m, hundreds, x, y);
        }
    }
    let m = &mut c.m;
    if m.r32(HUD_WARNING + 4 * p) != 0 && (crate::globals::SIM_COUNTER.get(m) >> 5) & 1 != 0 {
        link(m, sprite(0));
    }
}

pub fn abi_build_numbers(c: &mut Cpu) {
    build_numbers(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM in the race image (compared, below the stack): an ordering table entry, the
    /// primitives' buffer, a string.
    const OT_ENTRY: u32 = 0x801E_2A00;
    const PRIMS: u32 = 0x801E_3000;
    const TEXT: u32 = 0x801E_2B00;

    /// The scratchpad's ordering table and primitive buffer pointed at free RAM full of any
    /// bytes (the tags' top bytes are kept); the sprite colour and transparency, the HUD's
    /// views' globals any; a string of any case.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(gpu::OT, OT_ENTRY);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        for k in (0..0x1000).step_by(4) {
            m.w32(PRIMS + k, rng.next() as u32);
        }
        m.w32(OT_ENTRY, rng.next() as u32);
        m.w32(SPRITE_COLOR, rng.next() as u32);
        m.w8(SPRITE_TRANSPARENT, pick(rng, 3) as u8);
        let n = pick(rng, 24);
        for k in 0..32 {
            let ch = if k < n { [b'a', b'z', b'A', b'0', b' ', 0xE9, b'{', b'`'][pick(rng, 8) as usize] } else { 0 };
            m.w8(TEXT + k, ch);
        }
    }

    fn coord(r: &mut Rng) -> u32 {
        [0, 1, 255, 319, 512, (-3i32) as u32, 0x8000, r.next() as u32 & 0x3FF][pick(r, 8) as usize]
    }

    fn time(r: &mut Rng) -> u32 {
        [0, 63, 64, 3839, 3840, 383_999, 384_000, (-1i32) as u32, (-3840i32) as u32, i32::MIN as u32, r.next() as u32 & 0xFF_FFFF][pick(r, 11) as usize]
    }

    fn shape_arg(r: &mut Rng) -> u32 {
        shape(pick(r, 200))
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Hud_GoTpage", 0x800d_2acc, nfs4rc::f_800d2acc, |_, r| vec![pick(r, 40)]),
            ("Hud_BuildSprite", 0x800d_2b6c, nfs4rc::f_800d2b6c, |_, r| vec![PRIMS + 0x100, pick(r, 200), coord(r), coord(r), r.next() as u32, pick(r, 3)]),
            ("Hud_BuildSprite2", 0x800d_2c18, nfs4rc::f_800d2c18, |_, r| vec![PRIMS + 0x100, pick(r, 200), coord(r), coord(r)]),
            ("Hud_FBuildSprite", 0x800d_2c44, nfs4rc::f_800d2c44, |_, r| vec![pick(r, 200), coord(r), coord(r), r.next() as u32, pick(r, 3)]),
            ("Hud_BuildSpriteFromFont", 0x800d_2cd8, nfs4rc::f_800d2cd8, |_, r| vec![PRIMS + 0x100, r.next() as u32, coord(r), coord(r)]),
            ("Hud_BuildF3", 0x800d_2d98, nfs4rc::f_800d2d98, |_, r| vec![PRIMS + 0x100, shape_arg(r), coord(r), coord(r), r.next() as u32]),
            ("Hud_BuildGT4", 0x800d_2e5c, nfs4rc::f_800d2e5c, |_, r| vec![PRIMS + 0x100, shape_arg(r), coord(r), coord(r), r.next() as u32]),
            ("Hud_BuildFT4", 0x800d_2f08, nfs4rc::f_800d2f08, |_, r| vec![PRIMS + 0x100, shape_arg(r), coord(r), coord(r), r.next() as u32, pick(r, 3)]),
            ("Hud_BuildMirrorFT4", 0x800d_2ff8, nfs4rc::f_800d2ff8, |_, r| vec![PRIMS + 0x100, shape_arg(r), coord(r), coord(r), r.next() as u32, pick(r, 3)]),
            ("Hud_BuildMapMirrorFT4", 0x800d_30fc, nfs4rc::f_800d30fc, |_, r| vec![PRIMS + 0x100, shape_arg(r), coord(r), coord(r), r.next() as u32, pick(r, 3)]),
            ("Hud_BuildF4", 0x800d_3208, nfs4rc::f_800d3208, |_, r| vec![PRIMS + 0x100, pick(r, 3), coord(r), coord(r), coord(r), coord(r), r.next() as u32]),
            ("Hud_BuildG4", 0x800d_32ac, nfs4rc::f_800d32ac, |_, r| {
                vec![PRIMS + 0x100, pick(r, 3), coord(r), coord(r), coord(r), coord(r), r.next() as u32, r.next() as u32, r.next() as u32, r.next() as u32]
            }),
            ("Hud_BuildF4o", 0x800d_3368, nfs4rc::f_800d3368, |_, r| {
                vec![PRIMS + 0x100, pick(r, 3), coord(r), coord(r), coord(r), coord(r), r.next() as u32, r.next() as u32, r.next() as u32]
            }),
            ("Hud_FBuildGT4", 0x800d_343c, nfs4rc::f_800d343c, |_, r| vec![shape_arg(r), coord(r), coord(r), r.next() as u32]),
            ("Hud_FBuildFT4", 0x800d_34c8, nfs4rc::f_800d34c8, |_, r| vec![shape_arg(r), coord(r), coord(r), r.next() as u32]),
            ("Hud_FBuildF4", 0x800d_3558, nfs4rc::f_800d3558, |_, r| {
                vec![pick(r, 3), coord(r), coord(r), coord(r), coord(r), r.next() as u32, r.next() as u32, r.next() as u32]
            }),
            ("uppercase", 0x800d_3618, nfs4rc::f_800d3618, |_, _| vec![TEXT]),
            ("Hud_InitTables", 0x800d_45c0, nfs4rc::f_800d45c0, |c, r| {
                GAME_SETUP.set_num_player_race_cars(&mut c.m, pick(r, 3) as i32);
                vec![]
            }),
            ("Hud_BuildETimeString", 0x800d_45ec, nfs4rc::f_800d45ec, |_, r| vec![PRIMS + 0x100, time(r)]),
            ("Hud_BuildTimeString", 0x800d_4918, nfs4rc::f_800d4918, |_, r| vec![PRIMS + 0x100, time(r)]),
            ("Hud_BuildDistanceString", 0x800d_4778, nfs4rc::f_800d4778, |c, r| {
                let p = pick(r, 2);
                let m = &mut c.m;
                let car = m.r32(crate::globals::HUMAN_RACE_CARS + 4 * p);
                if car != 0 {
                    m.w32(car + CAR_DISTANCE, [0, 1666, 1667, 16_666, (-5000i32) as u32, r.next() as u32 & 0xFFFF][pick(r, 6) as usize]);
                }
                m.w32(GAME_SETUP.0 + CHECKPOINT_HUD + 4 * p, pick(r, 4));
                vec![PRIMS + 0x100, p]
            }),
            ("Hud_BuildTimeSprites", 0x800d_3acc, nfs4rc::f_800d3acc, |c, r| {
                // (the marks' tables have six languages: the original copies them to its stack)
                GAME_SETUP.user_setting().set_language(&mut c.m, pick(r, 6) as i32);
                let n = pick(r, 10);
                for k in 0..12 {
                    let ch = if k < n { [b'M', b'S', b'0', b'9', b':', b' ', b'a'][pick(r, 7) as usize] } else { 0 };
                    c.m.w8(TEXT + k, ch);
                }
                vec![PRIMS + 0x100, TEXT, coord(r), coord(r)]
            }),
            ("Hud_InitMapFrame", 0x800d_3720, nfs4rc::f_800d3720, |c, r| {
                let m = &mut c.m;
                m.w32(MAP_OFFSET_Y, [0, 8, (-6i32) as u32][pick(r, 3) as usize]);
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                // (both players' sprite buffers somewhere: the second's is none alone)
                m.w32(SPRITES[0], PRIMS);
                m.w32(SPRITES[1], PRIMS + 0x800);
                vec![pick(r, 2), pick(r, 3)]
            }),
            ("Hud_BuildString", 0x800d_4e9c, nfs4rc::f_800d4e9c, |c, r| {
                // the pads' types (a NeGcon or not), split screen or not, any of the characters
                let m = &mut c.m;
                for pad in 0..3 {
                    m.w8(PAD_INFO + 32 * pad + PAD_TYPE, [NEGCON, 4, 7][pick(r, 3) as usize]);
                }
                GAME_SETUP.set_comm_mode(m, pick(r, 3) as i32);
                let n = pick(r, 16);
                for k in 0..20 {
                    const CHARS: &[u8] = b"*^()&09-: aZ\xc0\xdc\xdd\xe5\xe9\xff";
                    let ch = if k < n { CHARS[pick(r, CHARS.len() as u32) as usize] } else { 0 };
                    m.w8(TEXT + k, ch);
                }
                vec![TEXT, coord(r), coord(r), r.next() as u32, pick(r, 3), pick(r, 3)]
            }),
            ("Hud_BuildTach", 0x800d_4a68, nfs4rc::f_800d4a68, |c, r| {
                // the car (past the tables' 30 too), day or night, any revs
                let p = pick(r, 2);
                let m = &mut c.m;
                m.w32(SPRITES[0], PRIMS + 0x400);
                m.w32(SPRITES[1], PRIMS + 0x800);
                GAME_SETUP.car_info(p).set_car_type(m, [0, 29, 30, 45, -1, pick(r, 30) as i32][pick(r, 6) as usize]);
                GAME_SETUP.set_time(m, pick(r, 2) as i32);
                let revs = [0, 1, 8000, 10_000, 0x2_0000, 0xFFFF, r.next() as u32 & 0x3FFF, r.next() as u32][pick(r, 8) as usize];
                m.w32(ENGINE_READING, revs);
                vec![p]
            }),
            ("Hud_Init", 0x800d_3c00, nfs4rc::f_800d3c00, |c, r| {
                // one player or two, the cop or not, the times and laps shown or not, the split
                // a time or a distance, the places' tables, the map's offset, a mirrored track
                let m = &mut c.m;
                m.w32(SPRITES[0], 0x801E_5000);
                m.w32(SPRITES[1], 0x801E_5800);
                m.w32(HUD_LAST_PLAYER, pick(r, 2));
                m.w32(BE_THE_COP, pick(r, 2));
                for p in 0..2 {
                    let car = GAME_SETUP.car_info(p);
                    car.set_hud_time(m, pick(r, 2) as i32);
                    car.set_hud_lapnum(m, pick(r, 2) as i32);
                    m.w32(GAME_SETUP.0 + CHECKPOINT_HUD + 4 * p, pick(r, 3));
                }
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                m.w32(MAP_OFFSET_Y, [0, 8, (-6i32) as u32][pick(r, 3) as usize]);
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                m.w32(SPRITE_COLOR, r.next() as u32);
                vec![]
            }),
            ("Hud_BuildNumbers0", 0x800d_51f8, nfs4rc::f_800d51f8, |c, r| {
                // the time shown, the cop, the flashing, the bonus time, checkpoints and the HUD's
                // kind, the car's split, checkpoints and distance, the split hidden
                let p = (pick(r, 4) == 0) as u32;
                let m = &mut c.m;
                m.w32(SPRITES[0], PRIMS + 0x400);
                m.w32(SPRITES[1], PRIMS + 0x800);
                GAME_SETUP.car_info(p).set_hud_time(m, pick(r, 2) as i32);
                // (mostly the split's paths: not the cop, checkpoints on, the split shown)
                m.w32(BE_THE_COP, (pick(r, 4) == 0) as u32);
                m.w32(RACE_TIME_FLASHING, pick(r, 2));
                crate::globals::SIM_COUNTER.set(m, r.next() as i32);
                m.w32(BTC_COUNTDOWN, time(r));
                m.w32(RACE_TIME, time(r));
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                m.w32(BTC_BONUS_TIME, [0, 0, 0, 1, (-5i32) as u32, 3000][pick(r, 6) as usize]);
                GAME_SETUP.set_checkpoint_type(m, [0, 1, 2, 1][pick(r, 4) as usize]);
                m.w32(GAME_SETUP.0 + CHECKPOINT_HUD + 4 * p, pick(r, 3));
                let car = m.r32(crate::globals::HUMAN_RACE_CARS + 4 * p);
                if car != 0 {
                    m.w32(car + CAR_SPLIT, [0, 1, (-1i32) as u32, 38_399, 38_400, (-38_399i32) as u32, (-38_400i32) as u32, (-38_400i32) as u32, i32::MIN as u32, time(r)][pick(r, 10) as usize]);
                    m.w32(car + CAR_CHECKPOINTS, [0, 1, 3, 3, (-1i32) as u32][pick(r, 5) as usize]);
                    m.w32(car + CAR_DISTANCE, [0, 1666, (-5000i32) as u32, (-1i32) as u32, 16_666, r.next() as u32 & 0xFFFF][pick(r, 6) as usize]);
                }
                m.w32(HUD_WARNING + 4 * p, (pick(r, 4) == 0) as u32);
                m.w32(SPLIT_TIME_DIFF[0] + 4 * p, r.next() as u32);
                m.w32(SPLIT_TIME_DIFF[1] + 4 * p, r.next() as u32);
                vec![p]
            }),
            ("Hud_BuildNumbers", 0x800d_5a44, nfs4rc::f_800d5a44, |c, r| {
                // the HUD's settings, the cop, the laps, the lap's time, the CD player shown, the
                // flashing, the position and racers, the gear, the speed, a warning
                let p = pick(r, 2);
                let m = &mut c.m;
                m.w32(SPRITES[0], PRIMS + 0x400);
                m.w32(SPRITES[1], PRIMS + 0x800);
                let car = GAME_SETUP.car_info(p);
                for at in [0x70, 0x78, 0x7C, 0x84, 0x88, 0x8C] {
                    m.w32(car.0 + at, (pick(r, 4) != 0) as u32);
                }
                car.set_hud_speed_mult(m, [0x1_0000, 0x2_3333, 0x9000, r.next() as u32 as i32][pick(r, 4) as usize]);
                m.w32(BE_THE_COP, (pick(r, 4) == 0) as u32);
                m.w32(LAP, pick(r, 10));
                m.w32(LAPS, [1, 2, 3, 8][pick(r, 4) as usize]);
                m.w32(LAP_TIME, [0, 1, 38_399, 38_400, (-5i32) as u32, time(r)][pick(r, 6) as usize]);
                m.w32(SHOWED_CD_PLAYER, (pick(r, 4) == 0) as u32);
                m.w32(RACE_TIME_FLASHING, pick(r, 2));
                crate::globals::SIM_COUNTER.set(m, r.next() as i32);
                m.w32(POSITION, pick(r, 9));
                m.w32(RACERS, pick(r, 9));
                m.w32(GEAR, pick(r, 8));
                let speed = [0, 1, 0xFFFF, 0x1_0000, 0x63_8000, 0xC8_0000, 0x3E7_0000, 0x7B_0000, 0x11_0000, 0x47_0000, (-1i32) as u32, (-0x1_0000i32) as u32, (-0x12_3456i32) as u32, r.next() as u32 & 0xFFF_FFFF];
                m.w32(SPEED, speed[pick(r, speed.len() as u32) as usize]);
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                m.w32(HUD_WARNING + 4 * p, pick(r, 2));
                vec![p]
            }),
            ("Hud_CreateHudViews", 0x800d_27e0, nfs4rc::f_800d27e0, |c, r| {
                // the setup's split screen, players, opponents and cars' classes
                let gs = GAME_SETUP;
                let m = &mut c.m;
                gs.set_comm_mode(m, pick(r, 3) as i32);
                gs.set_num_player_race_cars(m, pick(r, 3) as i32);
                gs.set_num_opponent_race_cars(m, pick(r, 4) as i32);
                gs.set_num_cars(m, pick(r, 10) as i32);
                for k in 0..9 {
                    let class = [1, 2, 4, 0x40, 0x41, r.next() as i32][pick(r, 6) as usize];
                    gs.car_info(k).set_car_class(m, class);
                }
                vec![]
            }),
        ]
    }

    #[test]
    fn hud_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x4D0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 300, 0x4D1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
