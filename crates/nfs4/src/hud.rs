//! HUD.CPP: the race's head-up display: its views (the map, the dashboard, the tachometer, the
//! stats), the primitives it builds from the HUD's shapes (`HudPmx_gShapes`) and the font, the
//! map, the radar, the CD player, the countdown, the pause box, the "busted" overlay.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free. Calls to the HUD's own functions not yet
//! decompiled, and to the helpers the host hooks (`Hud_FBuildF4`, `Hud_BuildString`,
//! `Hud_BlackThinBox`, `Hud_RenderPauseBox`, `Font_TextXY`), go by address.

use crate::call::{call, call_addr, with_buffer};
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
/// The HUD's functions not decompiled yet.
const INIT_TABLES: u32 = 0x800D_45C0;

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
    call_addr(c, INIT_TABLES, &[]);
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
        for k in (0..0x400).step_by(4) {
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
