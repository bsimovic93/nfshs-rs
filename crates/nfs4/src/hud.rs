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
const INIT_CD_PLAYER: u32 = 0x800d_6f8c;
const HUD_RESET: u32 = 0x800d_a3dc;

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
    call_addr(c, INIT_CD_PLAYER, &[]);
    call_addr(c, HUD_RESET, &[]);
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

/// The map markers' colours: the racers' and the police's (a word a car).
const MARKER_COLOUR: u32 = 0x8012_1E50;
const COP_MARKER_COLOUR: u32 = 0x8012_1E80;
/// A car's setup record (`carInfo`) and its HUD colour (`HudColour`).
const CAR_INFO: u32 = 0x288;
const HUD_COLOUR: u32 = 0x94;

/// `Hud_InitMap` (HUD.CPP, 0x800d661c): the racers' and the police's map markers in their
/// setups' HUD colours.
pub fn init_map(m: &mut Mem) {
    use crate::globals::{COP_CARS, NUM_COP_CARS, NUM_RACE_CARS, RACE_CARS};
    for (cars, count, colours) in [(RACE_CARS, NUM_RACE_CARS, MARKER_COLOUR), (COP_CARS, NUM_COP_CARS, COP_MARKER_COLOUR)] {
        for k in 0..count.get(m).max(0) as u32 {
            let car = m.r32(cars + 4 * k);
            let info = m.r32(car + CAR_INFO);
            let colour = m.r32(info + HUD_COLOUR);
            m.w32(colours + 4 * k, colour);
        }
    }
}

pub fn abi_init_map(c: &mut Cpu) {
    init_map(&mut c.m);
}

/// The map's place on its view, its scale (world units a pixel), its turn (cosine and sine).
const MAP_OFF_X: u32 = 0x8013_EC5C;
const MAP_OFF_Y: u32 = 0x8013_EC60;
const MAP_SCALE_X: u32 = 0x8013_EC54;
const MAP_SCALE_Y: u32 = 0x8013_EC58;
const MAP_COS: u32 = 0x8013_EC28;
const MAP_SIN: u32 = 0x8013_EC2C;
/// A car's slice (`N.simRoadInfo`: a half), whether it is in the world (`N.active`), its
/// flags (`carFlags`: a human, a human cop), its AI's flags (`AIFlags`: lights on).
const CAR_SLICE: u32 = 8;
const CAR_ACTIVE: u32 = 0x91;
const CAR_FLAGS: u32 = 0x260;
const CAR_HUMAN: u32 = 0x4;
const CAR_HUMAN_COP: u32 = 0x200;
const CAR_AI_FLAGS: u32 = 0x570;
const AI_LIGHTS: u32 = 0x2;
/// A slice's centre (`TrkNewSlice`, 32 bytes: x at +0, z at +8).
const SLICE_SIZE: u32 = 32;
/// The markers: a car's dot, a human's bigger one; the lights' flashing colours (red, blue).
const MARKER: u32 = 122;
const HUMAN_MARKER: u32 = 121;
const LIGHTS_RED: u32 = 0xFF;
const LIGHTS_BLUE: u32 = 0xFF_0000;
/// Whether the simulation is paused (`simVar.quickPauseSim`).
const QUICK_PAUSE_SIM: u32 = 0x8011_F38C;

/// Where on the map car `car` is: its slice's centre scaled, turned with the map and moved to
/// its place (mirrored across with a mirrored track); `y` from the top.
fn map_place(m: &mut Mem, car: u32) -> (i32, i32) {
    let slice = m.r16(car + CAR_SLICE) as i16 as i32 as u32;
    let centre = (crate::globals::BWORLDSM_SLICES.get(m) as u32).wrapping_add(slice.wrapping_mul(SLICE_SIZE));
    let x = crate::fixed::div(m.r32(centre) as i32, m.r32(MAP_SCALE_X) as i32);
    let z = crate::fixed::div(m.r32(centre + 8) as i32, m.r32(MAP_SCALE_Y) as i32);
    let (cos, sin) = (m.r32(MAP_COS) as i32, m.r32(MAP_SIN) as i32);
    let mut across = (m.r32(MAP_OFF_X) as i32).wrapping_add(fixedmult(cos, x)).wrapping_sub(fixedmult(sin, z));
    let up = (m.r32(MAP_OFF_Y) as i32 + 2).wrapping_add(fixedmult(sin, x)).wrapping_add(fixedmult(cos, z));
    if GAME_SETUP.mirror_track(m) != 0 {
        across = across.wrapping_neg();
    }
    (across, 24i32.wrapping_sub(up))
}

/// A car with its lights on: red and blue by turns (red while paused).
fn lights_colour(m: &mut Mem) -> u32 {
    if m.r32(crate::globals::FLIP) != 0 || m.r32(QUICK_PAUSE_SIM) != 0 { LIGHTS_RED } else { LIGHTS_BLUE }
}

/// `Hud_BuildMapMarkers` (HUD.CPP, 0x800d66bc): the cars in the world as markers on the map:
/// the police's (flashing with their lights on), then the racers' (a human's bigger; a human
/// cop's flashing with the lights on).
pub fn build_map_markers(m: &mut Mem) {
    use crate::globals::{COP_CARS, NUM_COP_CARS, NUM_RACE_CARS, RACE_CARS};
    let mut k = 0;
    while (k as i32) < NUM_COP_CARS.get(m) {
        let car = m.r32(COP_CARS + 4 * k);
        if m.r8(car + CAR_ACTIVE) != 0 {
            let (x, y) = map_place(m, car);
            let prim = gpu::add_prim(m, SPRT_SIZE);
            let colour = if m.r32(car + CAR_AI_FLAGS) & AI_LIGHTS != 0 { lights_colour(m) } else { m.r32(COP_MARKER_COLOUR + 4 * k) };
            m.w32(SPRITE_COLOR, colour);
            let x = x.wrapping_add(22 - 2) as u32 & 0xFFFF;
            build_sprite(m, prim, MARKER, x, y as u32 & 0xFFFF, colour, false);
        }
        k += 1;
    }
    let mut k = 0;
    while (k as i32) < NUM_RACE_CARS.get(m) {
        let car = m.r32(RACE_CARS + 4 * k);
        if m.r8(car + CAR_ACTIVE) != 0 {
            let (x, y) = map_place(m, car);
            let prim = gpu::add_prim(m, SPRT_SIZE);
            let flags = m.r32(car + CAR_FLAGS);
            let colour = m.r32(MARKER_COLOUR + 4 * k);
            let (shape, x, colour) = if flags & CAR_HUMAN_COP != 0 {
                let colour = if m.r32(car + CAR_AI_FLAGS) & AI_LIGHTS != 0 { lights_colour(m) } else { colour };
                m.w32(SPRITE_COLOR, colour);
                (HUMAN_MARKER, x.wrapping_add(22 - 3), colour)
            } else if flags & CAR_HUMAN != 0 {
                (HUMAN_MARKER, x.wrapping_add(22 - 3), colour)
            } else {
                (MARKER, x.wrapping_add(22 - 2), colour)
            };
            build_sprite(m, prim, shape, x as u32 & 0xFFFF, y as u32 & 0xFFFF, colour, false);
        }
        k += 1;
    }
}

pub fn abi_build_map_markers(c: &mut Cpu) {
    build_map_markers(&mut c.m);
}

/// The replay's mode (`Replay_ReplayMode`: 2 and up playing one back).
const REPLAY_MODE: u32 = 0x8013_E6D8;
/// The wingman's interface per player (`Hud_gWingmanInterface`, a byte each: shown on the
/// map), the icon flashing on it (`Hud_gWingmanFlashIcon`, a byte each) and until when
/// (`Hud_gWingmanFlashTicks`, a word each: game ticks).
const WINGMAN_INTERFACE: u32 = 0x8013_EB98;
const WINGMAN_FLASH_ICON: u32 = 0x8013_EB9C;
const WINGMAN_FLASH_TICKS: u32 = 0x8013_EBA0;
const TEXT_SYS_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const BUILD_STRING: u32 = 0x800D_4E9C;
/// The wingman's orders' words (the title, then four orders).
const WINGMAN_WORDS: u32 = 41;

/// `Hud_WingmanFlash` (HUD.CPP, 0x800d6b8c): player `p`'s wingman order `icon` flashed for
/// 100 ticks, the map's frame made the wingman's (once), unless a replay plays or the player
/// is busted.
pub fn wingman_flash(c: &mut Cpu, p: u32, icon: u32) {
    let m = &mut c.m;
    if m.r32(REPLAY_MODE) as i32 >= 2 || m.r32(BUSTED_OVERLAY) != 0 {
        return;
    }
    if m.r8(WINGMAN_INTERFACE.wrapping_add(p)) != 1 {
        init_map_frame(c, p, 1);
        c.m.w8(WINGMAN_INTERFACE.wrapping_add(p), 1);
    }
    let m = &mut c.m;
    m.w8(WINGMAN_FLASH_ICON.wrapping_add(p), icon as u8);
    let until = m.r32(crate::globals::GAME_TICKS).wrapping_add(100);
    m.w32(WINGMAN_FLASH_TICKS.wrapping_add(p << 2), until);
}

pub fn abi_wingman_flash(c: &mut Cpu) {
    wingman_flash(c, c.r[4], c.r[5]);
}

/// `Hud_BuildWingmanInterface` (HUD.CPP, 0x800d6c40): player `p`'s wingman orders by the map
/// (its title, the four orders with the player's pad's buttons) on black rows, the flashed
/// order's row in red pulsing as its ticks run out, a half-transparent black panel behind.
pub fn build_wingman_interface(c: &mut Cpu, p: u32) {
    let m = &mut c.m;
    let dy = if p != 0 { (-15i32) as u32 } else { 0 };
    let left = m.r32(WINGMAN_FLASH_TICKS.wrapping_add(p << 2)).wrapping_sub(m.r32(crate::globals::GAME_TICKS)) as i32;
    let (map_x, map_y) = element(m, 14);
    let top = map_y.wrapping_add(m.r32(MAP_OFFSET_Y)).wrapping_add(dy).wrapping_add(2);
    for k in 0..5 {
        let text = call(c, TEXT_SYS_WORD, &[WINGMAN_WORDS + k]);
        let pad = if k == 0 { 0 } else { p };
        call_addr(c, BUILD_STRING, &[text, map_x.wrapping_sub(27), top.wrapping_add(3 + 9 * k), GREY, pad, 0]);
    }
    let m = &mut c.m;
    let rows_x = map_x.wrapping_sub(28);
    if left > 0 {
        let row = m.r8(WINGMAN_FLASH_ICON.wrapping_add(p)) as u32 + 1;
        let prim = gpu::add_prim(m, POLY_F4_SIZE);
        build_f4(m, prim, false, map_x.wrapping_sub(16), top.wrapping_add(9 * row + 2), 63, 8, (left % 20 * 10) as u32);
    }
    for k in 0..5 {
        let prim = gpu::add_prim(m, POLY_F4_SIZE);
        build_f4(m, prim, false, rows_x, top.wrapping_add(2 + 9 * k), 75, 7, 0);
    }
    let prim = gpu::add_prim(m, POLY_F4_SIZE);
    build_f4(m, prim, true, rows_x, top, 75, 48, 0);
}

pub fn abi_build_wingman_interface(c: &mut Cpu) {
    build_wingman_interface(c, c.r[4]);
}

/// The CD player's state: the last tick it moved on, its title's scroll, whether it is open,
/// whether it can be (`Hud_ActivateCDPlayer`).
const CD_LAST_TICK: u32 = 0x8013_EC64;
const CD_SCROLL_TITLE: u32 = 0x8013_EC68;
const CD_ACTIVE: u32 = 0x8013_EC0C;
const CD_PLAYER_ALLOWED: u32 = 0x8013_EC10;

/// `Hud_InitCdPlayer` (HUD.CPP, 0x800d6f8c): the CD player closed, allowed unless a replay
/// plays.
pub fn init_cd_player(m: &mut Mem) {
    let replay = m.r32(REPLAY_MODE) as i32 >= 2;
    for at in [CD_LAST_TICK, CD_SCROLL_TITLE, CD_ACTIVE] {
        m.w32(at, 0);
    }
    m.w32(CD_PLAYER_ALLOWED, !replay as u32);
}

pub fn abi_init_cd_player(c: &mut Cpu) {
    init_cd_player(&mut c.m);
}

/// The playing song's record (`0x8007aa94`'s: none without music): +4 its track (0 none, -2
/// and below the CD's states), +8 its time (ms), +16 its title, +20 its artist.
const SONG_INFO: u32 = 0x8007_AA94;
const SONG_TRACK: u32 = 4;
const SONG_TIME: u32 = 8;
const SONG_TITLE: u32 = 16;
const SONG_ARTIST: u32 = 20;
/// The pads' buttons (`0x800e4da4`, by pad: 0 the first, 4 the second) and the CD player's
/// button; a NeGcon's I (+11 in the pad's state) pressed past three quarters instead.
const PAD_BUTTONS: u32 = 0x800E_4DA4;
const CD_BUTTON: u32 = 0x400;
const NEGCON_I: u32 = 11;
/// The race's start countdown (a byte: 4 and up counting, below 4 going).
const COUNTDOWN: u32 = 0x8013_D930;
/// Whether the CD player keeps its intro style (it opened so).
const CD_INTRO: u32 = 0x8013_EBE0;
/// The formats: a string, the track, no track, the time (minutes, the language's minute mark,
/// seconds); the minute marks by language.
const STRING_FORMAT: u32 = 0x8013_EBE4;
const TRACK_FORMAT: u32 = 0x8013_EBE8;
const NO_TRACK: u32 = 0x8013_EBF0;
const TIME_FORMAT: u32 = 0x8005_709C;
const MINUTE_CHAR: u32 = 0x8013_EB84;
/// The words when no title: no CD, the CD's state, its error.
const NO_CD_WORD: u32 = 68;
const CD_STATE_WORD: u32 = 69;
const CD_ERROR_WORD: u32 = 70;
const SPRINTF: u32 = 0x800E_50A8;
const TEXTPIXELS: u32 = 0x800E_4A2C;
const FONT_TEXT_COLOR: u32 = 0x800C_BF6C;
const FONT_TEXT_XY: u32 = 0x800C_C7F0;
const BLACK_THIN_BOX: u32 = 0x800D_8690;
const F_BUILD_F4: u32 = 0x800D_3558;
/// The title's window (pixels) and how far into the title it may look (characters).
const TITLE_WINDOW: i32 = 76;
const TITLE_MAX: u32 = 63;
/// The texts' buffers (on the stack, as the original lays them out: a title or an artist too
/// long runs into the next): the track, the title's visible part, the artist, the title, the
/// time.
const CD_TRACK: u32 = 0;
const CD_WINDOW: u32 = 8;
const CD_ARTIST: u32 = 40;
const CD_TITLE: u32 = 72;
const CD_TIME: u32 = 104;
const CD_BUFFERS: u32 = 136;

/// Whether the CD player should open: in the race's first 4.5 seconds, while the start counts
/// down (not as the cop), on the CD button (the second pad's with two players; the first's
/// alone or as a racer) or a NeGcon's I pressed.
fn cd_player_wanted(c: &mut Cpu) -> bool {
    let m = &mut c.m;
    let cop = m.r32(BE_THE_COP) != 0;
    let two = m.r32(HUD_LAST_PLAYER) != 0;
    if crate::globals::SIM_COUNTER.get(m) < 576 || (m.r8(COUNTDOWN) < 4 && !cop) {
        return true;
    }
    if call_addr(c, PAD_BUTTONS, &[4]) & CD_BUTTON != 0 && two {
        return true;
    }
    if call_addr(c, PAD_BUTTONS, &[0]) & CD_BUTTON != 0 && (!cop || two) {
        return true;
    }
    let m = &mut c.m;
    let negcon_i = |m: &mut Mem, pad: u32| pad_is_negcon(m, pad) && m.r8(PAD_INFO + 32 * pad + NEGCON_I) >= 192;
    (negcon_i(m, 0) && (!cop || two)) || (negcon_i(m, 1) && two)
}

/// The width the title's window gives `ch` (a space 3, else its shape and a pixel).
fn window_width(m: &mut Mem, ch: u8) -> i32 {
    if ch == b' ' {
        return 3;
    }
    let k = if ch.wrapping_sub(b'0') < 10 { ch as u32 + 110 } else { ch as u32 + 67 };
    shape_wh(m, shape(k)).0 as i32 + 1
}

/// The part of `title` showing in the window scrolled by `scroll` into `out`; the width of the
/// characters scrolled out on the left.
fn title_window(m: &mut Mem, title: u32, scroll: i32, out: u32) -> i32 {
    let (mut at, mut out, mut hidden) = (TITLE_WINDOW.wrapping_sub(scroll), out, 0i32);
    let mut k = 0;
    while m.r8(title.wrapping_add(k)) != 0 && k < TITLE_MAX {
        let ch = m.r8(title.wrapping_add(k));
        let w = window_width(m, ch);
        if at.wrapping_add(w) >= TITLE_WINDOW {
            break;
        }
        if at >= 0 {
            m.w8(out, ch);
            out = out.wrapping_add(1);
        } else {
            hidden = hidden.wrapping_add(w);
        }
        k += 1;
        at = at.wrapping_add(w);
    }
    m.w8(out, 0);
    hidden
}

/// `Hud_BuildCdPlayer` (HUD.CPP, 0x800d6fb0): the CD player's box at element 15, `style` 0 the
/// intro's (kept while the player stays open from the race's start), else the compact one:
/// opened by [`cd_player_wanted`] (as the cop, always once allowed); the title scrolled through
/// its window, four ticks a pixel, the player closing two seconds after (the song turned off
/// next when closed); the intro's the title and the artist, the compact one the track, the
/// title and the song's time. (The original's second argument, the player, goes unused.)
pub fn build_cd_player(c: &mut Cpu, style: u32) {
    let m = &mut c.m;
    let (x, y) = element(m, 15);
    let (x0, y0) = (x.wrapping_add(2), y.wrapping_add(2));
    let mut intro = style == 0;
    if intro {
        m.w32(CD_INTRO, 1);
    }
    let song = call_addr(c, SONG_INFO, &[]);
    if song == 0 {
        return;
    }
    if c.m.r32(CD_ACTIVE) == 0 && cd_player_wanted(c) {
        c.m.w32(CD_ACTIVE, 1);
        c.m.w32(CD_PLAYER_ALLOWED, 1);
    }
    let m = &mut c.m;
    if m.r32(CD_PLAYER_ALLOWED) != 0 {
        if m.r32(BE_THE_COP) != 0 {
            m.w32(CD_ACTIVE, 1);
        }
        let now = m.r32(crate::globals::GAME_TICKS);
        m.w32(CD_PLAYER_ALLOWED, 0);
        m.w32(CD_SCROLL_TITLE, 1);
        m.w32(CD_LAST_TICK, now);
        m.w32(CD_INTRO, intro as u32);
    }
    if m.r32(CD_INTRO) != 0 {
        intro = true;
    }
    let (time, track) = (m.r32(song + SONG_TIME) as i32, m.r32(song + SONG_TRACK) as i32);
    with_buffer(c, CD_BUFFERS, |c, buf| {
        let (title, artist) = (c.m.r32(song + SONG_TITLE), c.m.r32(song + SONG_ARTIST));
        let mut title = if title != 0 {
            call_addr(c, SPRINTF, &[buf + CD_TITLE, STRING_FORMAT, title]);
            buf + CD_TITLE
        } else {
            0
        };
        let mut artist = if artist != 0 {
            call_addr(c, SPRINTF, &[buf + CD_ARTIST, STRING_FORMAT, artist]);
            buf + CD_ARTIST
        } else {
            0
        };
        uppercase(c, title);
        if intro && artist != 0 {
            uppercase(c, artist);
        }
        if c.m.r32(CD_ACTIVE) == 0 {
            c.m.w32(TURN_SONG_OFF_NEXT, 1);
            return;
        }
        // the track: its number, or "- -" and (without a title) a word for the CD's state
        if track > 0 {
            call_addr(c, SPRINTF, &[buf + CD_TRACK, TRACK_FORMAT, track as u32]);
            if title == 0 {
                let now = c.m.r32(crate::globals::GAME_TICKS);
                c.m.w32(CD_SCROLL_TITLE, 1);
                c.m.w32(CD_LAST_TICK, now);
            }
        } else {
            call_addr(c, SPRINTF, &[buf + CD_TRACK, NO_TRACK]);
            artist = 0;
            let word = match track {
                0 => Some(NO_CD_WORD),
                -2 => Some(CD_STATE_WORD),
                _ if title == 0 => Some(CD_ERROR_WORD),
                _ => None,
            };
            if let Some(word) = word {
                title = call(c, TEXT_SYS_WORD, &[word]);
            }
        }
        let mut hidden = 0;
        if title == 0 {
            let now = c.m.r32(crate::globals::GAME_TICKS);
            c.m.w32(CD_INTRO, 0);
            c.m.w32(CD_LAST_TICK, now);
        } else {
            // scrolled a pixel each four ticks across the window, closing 128 ticks after
            let width = call_addr(c, BUILD_STRING, &[title, 0, 0, 0, 0, 1]) as i32;
            let m = &mut c.m;
            if (m.r32(CD_SCROLL_TITLE) as i32) < width.wrapping_add(TITLE_WINDOW) {
                while (m.r32(CD_LAST_TICK) as i32) < m.r32(crate::globals::GAME_TICKS) as i32 {
                    let (last, scroll) = (m.r32(CD_LAST_TICK), m.r32(CD_SCROLL_TITLE));
                    m.w32(CD_LAST_TICK, last.wrapping_add(4));
                    m.w32(CD_SCROLL_TITLE, scroll.wrapping_add(1));
                }
            } else if (m.r32(CD_LAST_TICK).wrapping_add(128) as i32) < m.r32(crate::globals::GAME_TICKS) as i32 {
                m.w32(CD_ACTIVE, 0);
            }
            let scroll = m.r32(CD_SCROLL_TITLE) as i32;
            hidden = title_window(m, title, scroll, buf + CD_WINDOW);
        }
        let m = &mut c.m;
        let scrolled = (m.r32(CD_SCROLL_TITLE) as i32).wrapping_sub(TITLE_WINDOW);
        let (bx, by) = element(m, 15);
        if intro {
            if title == 0 {
                return;
            }
            m.w32(SHOWED_CD_PLAYER, 1);
            let tx = x0.wrapping_add((hidden as u32).wrapping_add(10)).wrapping_sub(scrolled as u32);
            call_addr(c, BUILD_STRING, &[buf + CD_WINDOW, tx, y0.wrapping_add(10), 0xBEBE, 0, 0]);
            if artist != 0 {
                call_addr(c, BUILD_STRING, &[artist, x0.wrapping_add(10), y0.wrapping_add(19), GREY, 0, 0]);
            }
            go_tpage(c, 0);
            call_addr(c, BLACK_THIN_BOX, &[bx.wrapping_add(10), by.wrapping_add(10), 80, 18]);
            call_addr(c, F_BUILD_F4, &[0, bx.wrapping_add(10), by.wrapping_add(10), 80, 18, 0, 0, 0]);
            return;
        }
        m.w32(SHOWED_CD_PLAYER, 1);
        call_addr(c, FONT_TEXT_COLOR, &[4]);
        call_addr(c, FONT_TEXT_XY, &[buf + CD_TRACK, x0, y0.wrapping_sub(2)]);
        if title != 0 {
            call_addr(c, FONT_TEXT_COLOR, &[3]);
            let tx = x0.wrapping_add((hidden as u32).wrapping_add(22)).wrapping_sub(scrolled as u32);
            call_addr(c, BUILD_STRING, &[buf + CD_WINDOW, tx, y0.wrapping_add(3), 0xBEBE, 0, 0]);
            go_tpage(c, 0);
            if track != 0 {
                let language = GAME_SETUP.user_setting().language(&mut c.m) as u32;
                let mark = c.m.r8(MINUTE_CHAR.wrapping_add(language)) as u32;
                let (minutes, seconds) = (time / 60000, time % 60000 / 1000);
                call_addr(c, SPRINTF, &[buf + CD_TIME, TIME_FORMAT, minutes as u32, mark, seconds as u32]);
                call_addr(c, FONT_TEXT_COLOR, &[4]);
                let w = call_addr(c, TEXTPIXELS, &[buf + CD_TIME]);
                call_addr(c, FONT_TEXT_XY, &[buf + CD_TIME, x0.wrapping_sub(w).wrapping_add(92), y0.wrapping_add(12)]);
            }
        }
        let (bx, by) = element(&mut c.m, 15);
        call_addr(c, BLACK_THIN_BOX, &[bx, by, 102, 28]);
        for (semi, dy, h) in [(0, 0, 14), (0, 27, 1), (1, 0, 28)] {
            call_addr(c, F_BUILD_F4, &[semi, bx, by.wrapping_add(dy), 102, h, 0, 0, 0]);
        }
    });
}

pub fn abi_build_cd_player(c: &mut Cpu) {
    build_cd_player(c, c.r[4]);
}

/// The views' cameras (`Camera_gInfo`, 272 bytes a player: +0 the camera object) and a
/// camera's place (+160 x, +168 z, as a car's) and turn (+240.. a 3x3 matrix of words).
const CAMERA_INFO: u32 = 0x8011_0570;
const CAMERA_INFO_SIZE: u32 = 272;
const CAMERA_MATRIX: u32 = 240;
/// A car's place (+160 x, +168 z: the same as a camera's) and index (`carIndex`).
const CAR_X: u32 = 160;
const CAR_Z: u32 = 168;
const CAR_INDEX: u32 = 0x254;
/// The radar's half-size (pixels across and down from its centre).
const RADAR_HALF_W: i32 = 22;
const RADAR_HALF_H: i32 = 24;
/// The radar's range: none, cars near, cars at four times the distance.
pub const RADAR_NONE: u32 = 0;
pub const RADAR_NEAR: u32 = 1;
pub const RADAR_FAR: u32 = 2;
/// The radar's places (on the stack where the original keeps them, 12 bytes a car: +0
/// across, +8 down), the racers' then the police's.
const RADAR_FRAME: u32 = 248;
const RADAR_PLACES: u32 = 24;
const RADAR_PLACE_SIZE: u32 = 12;

/// Whether a radar place is on the radar.
fn on_radar(x: i32, y: i32) -> bool {
    -RADAR_HALF_W < x && x < RADAR_HALF_W && -RADAR_HALF_H < y && y < RADAR_HALF_H
}

/// `Hud_BuildRadar` (HUD.CPP, 0x800d771c): the cars round player `p`'s camera on the radar
/// (in the camera's frame, mirrored with a mirrored track): when none is near (the player's own
/// car aside), at a quarter of the scale; the police flashing red and blue, the racers in their
/// colours (a human's marker bigger). How far it looks ([`RADAR_NONE`] when nobody shows).
///
/// (The quarter-scale pass reads the police not in the world at places it never worked out:
/// whatever the stack held. Only [`RADAR_FAR`] can come of it, which the map treats as
/// [`RADAR_NONE`]; the places stay where the original has them, so the leftovers match.)
pub fn build_radar(c: &mut Cpu, p: u32) -> u32 {
    use crate::globals::{COP_CARS, FLIP, NUM_COP_CARS, NUM_RACE_CARS, RACE_CARS};
    with_buffer(c, RADAR_FRAME, |c, frame| {
        let m = &mut c.m;
        let place = |k: u32| frame + RADAR_PLACES + RADAR_PLACE_SIZE * k;
        let camera = m.r32(CAMERA_INFO + CAMERA_INFO_SIZE * p);
        let turn = |m: &mut Mem, k: u32| (m.r32(camera + CAMERA_MATRIX + 4 * k) as i32) >> 8;
        let (m00, m20, m02, m22) = (turn(m, 0), turn(m, 6), turn(m, 2), turn(m, 8));
        let world = |m: &mut Mem, at: u32| ((m.r32(at + CAR_X) as i32) >> 8, (m.r32(at + CAR_Z) as i32).wrapping_neg() >> 8);
        let (cx, cz) = world(m, camera);
        let mirrored = GAME_SETUP.mirror_track(m) != 0;
        // a car's place on the radar, near scale
        let place_car = |m: &mut Mem, car: u32, at: u32| {
            let (x, z) = world(m, car);
            let (dx, dz) = (x.wrapping_sub(cx), z.wrapping_sub(cz));
            let across = m00.wrapping_mul(dx).wrapping_add(m20.wrapping_mul(dz)) >> 16;
            m.w32(at, across as u32);
            let down = m02.wrapping_mul(dx).wrapping_add(m22.wrapping_mul(dz)) >> 17;
            m.w32(at + 8, down as u32);
            if mirrored {
                let x = m.r32(at);
                m.w32(at, x.wrapping_neg());
            }
        };
        let shown = |m: &mut Mem, at: u32| on_radar(m.r32(at) as i32, m.r32(at + 8) as i32);
        let mut range = RADAR_NONE;
        let mut k = 0;
        while (k as i32) < NUM_RACE_CARS.get(m) {
            let car = m.r32(RACE_CARS + 4 * k);
            place_car(m, car, place(k));
            if m.r8(car + CAR_ACTIVE) != 0 && m.r32(car + CAR_INDEX) != p && shown(m, place(k)) {
                range = RADAR_NEAR;
            }
            k += 1;
        }
        let racers = NUM_RACE_CARS.get(m) as u32;
        let mut k = 0;
        while (k as i32) < NUM_COP_CARS.get(m) {
            let car = m.r32(COP_CARS + 4 * k);
            if m.r8(car + CAR_ACTIVE) != 0 {
                let at = place(racers.wrapping_add(k));
                place_car(m, car, at);
                if shown(m, at) {
                    range = RADAR_NEAR;
                }
            }
            k += 1;
        }
        if range == RADAR_NONE {
            let quarter = |m: &mut Mem, at: u32| {
                for half in [at, at + 8] {
                    let v = m.r32(half) as i32 >> 2;
                    m.w32(half, v as u32);
                }
            };
            for k in 0..NUM_RACE_CARS.get(m).max(0) as u32 {
                let car = m.r32(RACE_CARS + 4 * k);
                if m.r8(car + CAR_ACTIVE) != 0 {
                    quarter(m, place(k));
                    if m.r32(car + CAR_INDEX) != p && shown(m, place(k)) {
                        range = RADAR_FAR;
                    }
                }
            }
            for k in 0..NUM_COP_CARS.get(m).max(0) as u32 {
                let at = place(racers.wrapping_add(k));
                quarter(m, at);
                if shown(m, at) {
                    range = RADAR_FAR;
                }
            }
        }
        let radar_xy = |m: &mut Mem, at: u32, dx: i32| {
            let x = (m.r32(at) as i32).wrapping_add(RADAR_HALF_W).wrapping_sub(dx);
            let y = (m.r32(at + 8) as i32).wrapping_add(RADAR_HALF_H);
            (x as u32 & 0xFFFF, y as u32 & 0xFFFF)
        };
        let mut k = 0;
        while (k as i32) < NUM_COP_CARS.get(m) {
            let car = m.r32(COP_CARS + 4 * k);
            if m.r8(car + CAR_ACTIVE) != 0 {
                let prim = gpu::add_prim(m, SPRT_SIZE);
                let colour = if m.r32(FLIP) != 0 || m.r32(QUICK_PAUSE_SIM) != 0 { LIGHTS_RED } else { LIGHTS_BLUE };
                m.w32(SPRITE_COLOR, colour);
                let racers = NUM_RACE_CARS.get(m) as u32;
                let (x, y) = radar_xy(m, place(racers.wrapping_add(k)), 2);
                build_sprite(m, prim, MARKER, x, y, colour, false);
            }
            k += 1;
        }
        let mut k = 0;
        while (k as i32) < NUM_RACE_CARS.get(m) {
            let car = m.r32(RACE_CARS + 4 * k);
            if k != p && m.r8(car + CAR_ACTIVE) != 0 {
                let prim = gpu::add_prim(m, SPRT_SIZE);
                let human = m.r32(car + CAR_FLAGS) & CAR_HUMAN != 0;
                let colour = m.r32(MARKER_COLOUR + 4 * k);
                let (shape, dx) = if human { (HUMAN_MARKER, 3) } else { (MARKER, 2) };
                let (x, y) = radar_xy(m, place(k), dx);
                build_sprite(m, prim, shape, x, y, colour, false);
            }
            k += 1;
        }
        range
    })
}

pub fn abi_build_radar(c: &mut Cpu) {
    c.r[2] = build_radar(c, c.r[4]);
}

/// The replay's buttons' sprites (the first player's 51 to 57: the five buttons from 51, the
/// speed 56, the camera 57), their colours (grey, the chosen one's cyan), the button sprite for
/// each choice (`0x8013ebf4`: the camera's choice the fifth).
const REPLAY_SPRITES: u32 = 51;
const REPLAY_SPEED_SPRITE: u32 = 56;
const REPLAY_CAMERA_SPRITE: u32 = 57;
const REPLAY_GREY: u32 = 0x6680_8080;
const REPLAY_CHOSEN: u32 = 0x6600_BEBE;
const REPLAY_BUTTON_OF: u32 = 0x8013_EBF4;
/// The play/pause button's shape (110 playing, 109 paused), the speeds' (114 to 117), the
/// cameras' (from 2).
const REPLAY_PLAY: u32 = 110;
const REPLAY_SPEEDS: [u32; 3] = [114, 115, 117];
const REPLAY_OTHER_SPEED: u32 = 116;
const REPLAY_CAMERAS: u32 = 2;

/// `Hud_BuildReplay` (HUD.CPP, 0x800d7e24): the replay's controls into the ordering table: the
/// buttons grey, the chosen one cyan (the speed's too with its choice); the play button by the
/// pause, the speed's and the camera's icons, the camera's beside the controls; with the first
/// player's texture pages.
pub fn build_replay(m: &mut Mem) {
    let sprites = m.r32(SPRITES[0]);
    let sprite = |k: u32| sprites.wrapping_add(SPRT_SIZE * k);
    let replay = crate::globals::REPLAY_INTERFACE;
    for k in REPLAY_SPRITES..REPLAY_SPRITES + 12 {
        m.w32(sprite(k) + 4, REPLAY_GREY);
    }
    let chosen = replay.selection(m) as u32;
    let button = m.r8(REPLAY_BUTTON_OF.wrapping_add(chosen)) as u32;
    m.w32(sprite(REPLAY_SPRITES + button) + 4, REPLAY_CHOSEN);
    if replay.selection(m) == 3 {
        m.w32(sprite(REPLAY_SPEED_SPRITE) + 4, REPLAY_CHOSEN);
    }
    let pause = replay.pause(m) as u32;
    let play = m.r32(shape(REPLAY_PLAY.wrapping_sub(pause)));
    m.w32(sprite(REPLAY_SPRITES + 1) + SPRT_UV, play);
    let speed = match replay.speed(m) {
        s @ 0..=2 => REPLAY_SPEEDS[s as usize],
        _ => REPLAY_OTHER_SPEED,
    };
    let (u, v) = (m.r8(shape(speed)), m.r8(shape(speed) + 1));
    m.w8(sprite(REPLAY_SPEED_SPRITE) + SPRT_UV, u);
    m.w8(sprite(REPLAY_SPEED_SPRITE) + SPRT_UV + 1, v);
    let camera = sprite(REPLAY_CAMERA_SPRITE);
    let camera_kind = replay.camera(m) as u32;
    let icon = m.r32(shape(camera_kind.wrapping_add(REPLAY_CAMERAS)));
    m.w32(camera + SPRT_UV, icon);
    let (controls_x, _) = element(m, 13);
    let x = (controls_x as u16).wrapping_add(117);
    m.w16(camera + 8, x);
    let colour = if replay.selection(m) == 4 { REPLAY_CHOSEN } else { REPLAY_GREY };
    m.w32(camera + 4, colour);
    link(m, camera);
    for k in REPLAY_SPRITES..REPLAY_SPRITES + 5 {
        link(m, sprite(k));
    }
    link(m, sprite(REPLAY_SPEED_SPRITE));
    link(m, TPAGE[1] + 36);
    link(m, TPAGE[0] + 36);
}

pub fn abi_build_replay(c: &mut Cpu) {
    build_replay(&mut c.m);
}

/// Whether the player looks behind (`Input_gLookBehind`, a word a player).
const LOOK_BEHIND: u32 = 0x8013_E508;
/// The cars by race order (`Cars_gSortedList`) and a car's place in it (+604), its driver's
/// voice (+1260); the opponent's name the HUD shows (`HudOpponentID`: 2 the car's) and its
/// name and driver's (+585, the setup's +92).
const SORTED_LIST: u32 = 0x8011_0DC0;
const CAR_SORTED_INDEX: u32 = 604;
const CAR_VOICE: u32 = 1260;
const CAR_NAME: u32 = 0x249;
const DRIVER_NAME: u32 = 0x5C;
const OPPONENT_BY_CAR: i32 = 2;
const CAR_AI_RACER: u32 = 0x8;
/// No racer (an empty string).
const NO_NAME: u32 = 0x8013_EBFC;
const STATS_GET_POSITION: u32 = 0x800b_8e28;
/// The other human's voices: the second player's for the first, the first's for the second.
const HUMAN_VOICE: [u32; 2] = [8, 7];

/// The racer next to player `p`'s car in the race order (ahead, or behind looking back or
/// going the wrong way; the other way round on a reversed track), a human or an AI racer.
fn next_racer(c: &mut Cpu, p: u32) -> Option<u32> {
    use crate::globals::{HUMAN_RACE_CARS, NUM_CARS, NUM_RACE_CARS};
    let m = &mut c.m;
    let looks_back = (m.r32(LOOK_BEHIND + 4 * p) as i32 > 0) != (m.r32(HUD_WARNING + 4 * p) as i32 > 0);
    let car = m.r32(HUMAN_RACE_CARS + 4 * p);
    if NUM_RACE_CARS.get(m) < 2 {
        return None;
    }
    let first = call_addr(c, STATS_GET_POSITION, &[car]) == 1;
    if first && !looks_back {
        return None;
    }
    let m = &mut c.m;
    let mut at = m.r32(car + CAR_SORTED_INDEX) as i32;
    let back = looks_back != (GAME_SETUP.reverse_track(m) != 0);
    let n = NUM_CARS.get(m);
    for _ in 0..n - 1 {
        at = if back { at - 1 } else { at + 1 };
        if at < 0 {
            at += n;
        }
        if at >= n {
            at = 0;
        }
        let other = m.r32(SORTED_LIST.wrapping_add((at as u32) << 2));
        if m.r32(other + CAR_FLAGS) & (CAR_HUMAN | CAR_AI_RACER) != 0 {
            return Some(other);
        }
    }
    None
}

/// `Hud_NextPlayer` (HUD.CPP, 0x800d8120): the voice of the racer next to player `p` (the other
/// human's own); -1 none.
pub fn next_player(c: &mut Cpu, p: u32) -> u32 {
    match next_racer(c, p) {
        None => u32::MAX,
        Some(other) if c.m.r32(other + CAR_FLAGS) & CAR_HUMAN != 0 => HUMAN_VOICE[(p != 0) as usize],
        Some(other) => c.m.r32(other + CAR_VOICE),
    }
}

pub fn abi_next_player(c: &mut Cpu) {
    c.r[2] = next_player(c, c.r[4]);
}

/// `Hud_NextPlayerNameOrCarOrTime` (HUD.CPP, 0x800d8284): the name of the racer next to player
/// `p`, its car's or its driver's as the HUD is set; an empty string none.
pub fn next_player_name(c: &mut Cpu, p: u32) -> u32 {
    match next_racer(c, p) {
        None => NO_NAME,
        Some(other) if GAME_SETUP.car_info(p).hud_opponent_id(&mut c.m) == OPPONENT_BY_CAR => other + CAR_NAME,
        Some(other) => c.m.r32(other + CAR_INFO).wrapping_add(DRIVER_NAME),
    }
}

pub fn abi_next_player_name(c: &mut Cpu) {
    c.r[2] = next_player_name(c, c.r[4]);
}

/// `Draw_StartRenderingView`, `Draw_StopRenderingView`: drawing into a view's ordering table.
const DRAW_START_RENDERING_VIEW: u32 = 0x800B_EDAC;
const DRAW_STOP_RENDERING_VIEW: u32 = 0x800B_EE9C;
/// Whether a player's HUD shows (`DashHUD_gInfo` +0x1C, a word a player).
const HUD_SHOWN: u32 = 0x8011_3A04;
/// The setup's map setting: 1 the track's map, else the radar.
const HUD_MAP_TRACK: i32 = 1;

/// `Hud_RenderMapView` (HUD.CPP, 0x800d840c; the host draws it itself): per player with a map
/// and the HUD shown (not while the wingman's orders show), into the player's map view: the track's map with the
/// cars' markers, or the radar and its rim (near or far); on the map's texture page.
pub fn render_map_view(c: &mut Cpu) {
    let mut p = 0;
    while p as i32 <= c.m.r32(HUD_LAST_PLAYER) as i32 {
        let m = &mut c.m;
        let car = GAME_SETUP.car_info(p);
        if car.hud_map(m) != 0 && m.r32(HUD_SHOWN + 4 * p) != 0 && m.r8(WINGMAN_INTERFACE + p) == 0 {
            let ft4 = HUD_FT4 + if p != 0 { 200 } else { 0 };
            let view = m.r32(MAP_VIEW + 4 * p);
            call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
            if car.hud_map(&mut c.m) == HUD_MAP_TRACK {
                call_addr(c, 0x800d_66bc, &[p]);
                link(&mut c.m, ft4);
            } else {
                let rim = if call_addr(c, 0x800d_771c, &[p]) == RADAR_NEAR { [ft4 + 40, ft4 + 80] } else { [ft4 + 120, ft4 + 160] };
                for half in rim {
                    link(&mut c.m, half);
                }
            }
            link(&mut c.m, TPAGE[1] + 48 * p + 12);
            let view = c.m.r32(MAP_VIEW + 4 * p);
            call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
        }
        p += 1;
    }
}

pub fn abi_render_map_view(c: &mut Cpu) {
    render_map_view(c);
}

/// The thin box's corners (14 to 17; the last's size for all) and its edges' width.
const BOX_CORNERS: [u32; 4] = [14, 15, 16, 17];

/// `Hud_BlackThinBox` (HUD.CPP, 0x800d8690; the host hooks it): a black frame round `x`, `y`,
/// `w` by `h`: the edges 2 pixels thick between the corners, the four corners.
pub fn black_thin_box(c: &mut Cpu, x: u32, y: u32, w: u32, h: u32) {
    let (cw, ch) = shape_wh(&mut c.m, shape(BOX_CORNERS[3]));
    let edge = |c: &mut Cpu, x: u32, y: u32, w: u32, h: u32| {
        call_addr(c, F_BUILD_F4, &[0, x, y, w, h, 0, 0, 0]);
    };
    let (across_x, across_w) = (x.wrapping_add(cw).wrapping_sub(2), w.wrapping_sub(2 * cw).wrapping_add(2));
    let (down_y, down_h) = (y.wrapping_add(ch).wrapping_sub(2), h.wrapping_sub(2 * ch).wrapping_add(4));
    edge(c, across_x, y.wrapping_sub(2), across_w, 2);
    edge(c, x.wrapping_sub(2), down_y, 2, down_h);
    edge(c, x.wrapping_add(w).wrapping_sub(2), down_y, 2, down_h);
    edge(c, across_x, y.wrapping_add(h), across_w, 2);
    let (right, bottom) = (x.wrapping_add(w).wrapping_sub(cw), y.wrapping_add(h).wrapping_sub(ch.wrapping_sub(2)));
    let m = &mut c.m;
    for (corner, cx, cy) in [(0, x.wrapping_sub(2), y.wrapping_sub(2)), (1, right, y.wrapping_sub(2)), (2, x.wrapping_sub(2), bottom), (3, right, bottom)] {
        f_build_gt4(m, shape(BOX_CORNERS[corner]), cx, cy, 0);
    }
}

pub fn abi_black_thin_box(c: &mut Cpu) {
    black_thin_box(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The countdown's digits as 5 by 5 dots (`Hud_Character`, a word a digit: bit 5 * row + col
/// on), the dots' shapes (off, on), the halos behind the lit ones.
const COUNTDOWN_DIGITS: u32 = 0x8012_1D9C;
const DOT: u32 = 60;
const FLARE_2D_HALO: u32 = 0x800C_ECD0;

/// `Hud_Draw321Num` (HUD.CPP, 0x800d887c; the host draws it itself): countdown digit `digit` at
/// `x`, `y` as 5 by 5 dots 10 across and 9 down, the lit ones haloed `size` big (none at 0), in
/// a black box. (The original's last two arguments, the left and the right digits, go unused.)
pub fn draw_321_num(c: &mut Cpu, x: u32, y: u32, digit: u32, size: u32) {
    let lit = |c: &mut Cpu, bit: u32| c.m.r32(COUNTDOWN_DIGITS.wrapping_add(digit << 2)) & (1 << bit) != 0;
    let dot = |row: u32, col: u32| (x.wrapping_add(10 * col), y.wrapping_add(9 * row));
    if size != 0 {
        for bit in 0..25 {
            let (dx, dy) = dot(bit / 5, bit % 5);
            if lit(c, bit) {
                call_addr(c, FLARE_2D_HALO, &[dx.wrapping_add(4), dy.wrapping_add(4), size, size, 6]);
            }
        }
    }
    call_addr(c, BLACK_THIN_BOX, &[x.wrapping_sub(3), y.wrapping_sub(2), 56, 49]);
    for bit in 0..25 {
        let (dx, dy) = dot(bit / 5, bit % 5);
        let on = lit(c, bit) as u32;
        f_build_sprite(&mut c.m, DOT | on, dx.wrapping_add(1), dy.wrapping_add(1), GREY, false);
    }
    call_addr(c, F_BUILD_F4, &[0, x.wrapping_sub(3), y.wrapping_sub(2), 56, 49, 0, 0, 0]);
    go_tpage(c, 1);
}

pub fn abi_draw_321_num(c: &mut Cpu) {
    draw_321_num(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The view drawn into (`gCView`), the countdown as last seen (a signed byte) and since when.
const CURRENT_VIEW: u32 = 0x8011_8234;
const LAST_COUNTDOWN: u32 = 0x8013_EBFD;
const COUNTDOWN_TICK: u32 = 0x8013_F108;
/// The countdown's halos: how big they start (the "go" bigger) and how fast they shrink, over
/// 100 ticks.
const HALO_START: [i32; 2] = [6000, 8000];
const HALO_SHRINK: [i32; 2] = [60, 80];

/// `Hud_Render321Go` (HUD.CPP, 0x800d8a38; the host draws it itself): the start's countdown
/// in the stats view at element 17's height: the digit (3, 2, 1, then 0 for "go": none from
/// 4 on) three times across the screen, haloed as it changes, the halos shrinking over 100
/// ticks.
pub fn render_321_go(c: &mut Cpu) {
    let m = &mut c.m;
    let view = m.r32(STATS_VIEW);
    m.w32(CURRENT_VIEW, view);
    call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
    let m = &mut c.m;
    let (_, y) = element(m, 17);
    let countdown = m.r8(COUNTDOWN);
    if m.r8(LAST_COUNTDOWN) as i8 as i32 != countdown as i32 {
        m.w8(LAST_COUNTDOWN, countdown);
        let now = m.r32(crate::globals::GAME_TICKS);
        m.w32(COUNTDOWN_TICK, now);
    }
    let since = m.r32(crate::globals::GAME_TICKS).wrapping_sub(m.r32(COUNTDOWN_TICK));
    let go = (countdown == 4) as usize;
    let size = if since < 100 { HALO_START[go] - HALO_SHRINK[go] * since as i32 } else { 0 };
    let countdown = m.r8(COUNTDOWN);
    if countdown != 4 {
        let digit = 4u32.wrapping_sub(countdown as u32);
        for (x, left, right) in [(160 - 88, 1, 0), (160 - 25, 0, 0), (160 + 38, 0, 1)] {
            call_addr(c, 0x800d_887c, &[x, y, digit, size as u32, left, right]);
        }
    }
    let view = c.m.r32(STATS_VIEW);
    call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
}

pub fn abi_render_321_go(c: &mut Cpu) {
    render_321_go(c);
}

/// The pursuit's time as last seen, since when, and whether its warning beep has played.
const LAST_BTC_TIME: u32 = 0x8013_EC00;
const LAST_BTC_TICK: u32 = 0x8013_EC04;
const BTC_BEEPED: u32 = 0x8013_EB94;
/// The beep (`0x8007a134`).
const BTC_BEEP: fn(&mut Cpu) = nfs4rc::f_8007a134;
/// The big time's colours (green, and red under 11 seconds), top and bottom.
const BTC_GREEN: [u32; 2] = [0xC800, 0x6400];
const BTC_RED: [u32; 2] = [200, 100];

/// `Hud_BigBTCTime` (HUD.CPP, 0x800d8b84; the host draws it itself): the pursuit's seconds `t`
/// in big digits at element 15, in a black box: green, or under 11 seconds red and flashing
/// (blank for 64 ticks as each second starts) with a beep as it shows; nothing with a bonus
/// time or busted.
pub fn big_btc_time(c: &mut Cpu, t: i32) {
    let m = &mut c.m;
    if m.r32(BTC_BONUS_TIME) != 0 || m.r32(BUSTED_OVERLAY) != 0 || t < 0 {
        return;
    }
    if t != m.r32(LAST_BTC_TIME) as i32 {
        m.w32(LAST_BTC_TIME, t as u32);
        let now = m.r32(crate::globals::GAME_TICKS);
        m.w32(LAST_BTC_TICK, now);
    }
    let (x, y) = element(m, 15);
    let x = x.wrapping_add(2);
    let w0 = shape_wh(m, shape(BIG_DIGITS)).0.wrapping_add(1);
    let w1 = shape_wh(m, shape(BIG_DIGITS + 1)).0.wrapping_add(1);
    let boxed = |c: &mut Cpu| {
        call_addr(c, BLACK_THIN_BOX, &[x, y, 2 * w0, 14]);
        call_addr(c, F_BUILD_F4, &[0, x, y, 2 * w0, 14, 0, 0, 0]);
    };
    let [top, bottom] = if t >= 11 {
        m.w32(BTC_BEEPED, 0);
        BTC_GREEN
    } else if (m.r32(crate::globals::GAME_TICKS).wrapping_sub(m.r32(LAST_BTC_TICK)) as i32) < 64 {
        boxed(c);
        c.m.w32(BTC_BEEPED, 0);
        return;
    } else {
        if m.r32(BTC_BEEPED) == 0 {
            call(c, BTC_BEEP, &[]);
            c.m.w32(BTC_BEEPED, 1);
        }
        BTC_RED
    };
    let m = &mut c.m;
    let digit = |m: &mut Mem, d: i32, x: u32| {
        let prim = gpu::add_prim(m, POLY_GT4_SIZE);
        build_gt4(m, prim, shape(BIG_DIGITS.wrapping_add(d as u32)), x, y, top);
        m.w32(prim + 40, bottom);
        m.w32(prim + 28, bottom);
    };
    let units_x = x.wrapping_add(w0);
    digit(m, t % 10, units_x.wrapping_sub(1));
    let tens = t / 10;
    if tens != 0 {
        digit(m, tens, units_x.wrapping_sub(if tens == 1 { w1 } else { w0 }));
    }
    boxed(c);
}

pub fn abi_big_btc_time(c: &mut Cpu) {
    big_btc_time(c, c.r[4] as i32);
}

/// The race's kind (`GameSetup_gData.raceType`): 1 a pursuit (the perp's overlay shows).
const RACE_PURSUIT: i32 = 1;
/// The perp overlay's messages' words (from 65), its lights' shape (61), its back's colour.
const PERP_WORDS: u32 = 65;
const PERP_LIGHT: u32 = 61;
const PERP_BACK: u32 = 0x46_1414;
/// The dashboard's own (DASHHUD.CPP): the speedometer and the tachometer's needles' parts.
const DASH_HUD_TACH: fn(&mut Cpu) = nfs4rc::f_8009bb34;
/// The pursuit's time on the HUD: the countdown in 64ths (seconds 1 to 30 shown big).
const BTC_SECONDS_SHOWN: u32 = 30;

/// The perp overlay on player `p`'s view at element 18 (`dy` the view's offset): its message in
/// the font, between two pairs of police lights flashing red and blue by turns (halos shrinking
/// every 15 ticks), in a black box on dark red.
fn perp_overlay(c: &mut Cpu, p: u32, dy: u32) {
    let message = c.m.r32(PERP_OVERLAY_MESSAGE + 4 * p);
    let text = call(c, TEXT_SYS_WORD, &[message.wrapping_add(PERP_WORDS)]);
    let w = call_addr(c, TEXTPIXELS, &[text]);
    let y = element(&mut c.m, 18).1.wrapping_add(dy);
    call_addr(c, FONT_TEXT_COLOR, &[4]);
    let text = call(c, TEXT_SYS_WORD, &[message.wrapping_add(PERP_WORDS)]);
    let half = w >> 1;
    call_addr(c, FONT_TEXT_XY, &[text, 160u32.wrapping_sub(half + 1), y]);
    let ticks = crate::globals::SIM_COUNTER.get(&mut c.m);
    let size = (4000 - 250 * (ticks % 15)) as u32;
    let left = 160u32.wrapping_sub(half);
    // a pair of lights: the halos (8 or 10) and the lamps (blue or red) as the phase has them
    let lights = |c: &mut Cpu, halo_x: [u32; 2], lamp_x: [u32; 2], flip: bool| {
        let on = (crate::globals::SIM_COUNTER.get(&mut c.m) >> 4) & 1 != 0;
        let red = on != flip;
        let (lamp, halo) = if red { (128, 8) } else { (0x80_0000, 10) };
        for hx in halo_x {
            call_addr(c, FLARE_2D_HALO, &[hx, y.wrapping_add(8), size, size, halo]);
        }
        for lx in lamp_x {
            f_build_gt4(&mut c.m, shape(PERP_LIGHT), lx, y.wrapping_add(5), lamp);
        }
    };
    lights(c, [left.wrapping_sub(35), left.wrapping_sub(45)], [left.wrapping_sub(38), left.wrapping_sub(48)], false);
    lights(c, [half + 192, half + 202], [half + 189, half + 199], true);
    call_addr(c, BLACK_THIN_BOX, &[left.wrapping_sub(49), y, w.wrapping_add(98), 17]);
    for (semi, x, dy, w, h, colour) in [
        (0, left.wrapping_sub(49), 0, 23, 17, 0),
        (0, half + 186, 0, 21, 17, 0),
        (0, half + 186, 3, 22, 11, 0),
        (1, left.wrapping_sub(26), 0, w.wrapping_add(54), 17, PERP_BACK),
    ] {
        call_addr(c, F_BUILD_F4, &[semi, x, y.wrapping_add(dy), w, h, colour, 0, 0]);
    }
}

/// The next racer's name over the map (player `p`, `dy` the view's offset): uppercased and
/// centred on the map's background, between the frame's top corners on a black band, the
/// band's ends down the sides (shorter with the wingman's orders up).
fn next_racer_label(c: &mut Cpu, p: u32, dy: u32) {
    let m = &mut c.m;
    let back_w = shape_wh(m, shape(MAP_BACKGROUND)).0.wrapping_add(2);
    let (corner_w, corner_h) = shape_wh(m, shape(14));
    let side_w = shape_wh(m, shape(20)).0;
    let name = call_addr(c, 0x800d_8284, &[p]);
    with_buffer(c, 64, |c, buf| {
        call_addr(c, SPRINTF, &[buf, STRING_FORMAT, name]);
        go_tpage(c, 1);
        uppercase(c, buf);
        let w = call_addr(c, BUILD_STRING, &[buf, 0, 0, 0, 0, 1]);
        let m = &mut c.m;
        let (map_x, map_y) = element(m, 14);
        let top = map_y.wrapping_add(m.r32(MAP_OFFSET_Y)).wrapping_add(dy);
        let x = map_x.wrapping_add((back_w.wrapping_sub(w) as i32 >> 1) as u32);
        call_addr(c, BUILD_STRING, &[buf, x, top.wrapping_sub(corner_h).wrapping_add(1), GREY, 0, 0]);
        go_tpage(c, 0);
        let m = &mut c.m;
        f_build_gt4(m, shape(14), map_x, top.wrapping_sub(corner_h), GREY);
        let right_h = shape_wh(m, shape(15)).1;
        f_build_gt4(m, shape(15), map_x.wrapping_add(corner_w).wrapping_add(side_w), top.wrapping_sub(right_h), GREY);
        let band_h = shape_wh(m, shape(14)).1;
        call_addr(c, F_BUILD_F4, &[0, map_x.wrapping_add(2), top.wrapping_sub(band_h), corner_w + side_w + 3, band_h, 0, 0, 0]);
        let side = if c.m.r8(WINGMAN_INTERFACE + p) != 0 { 2 } else { 3 };
        call_addr(c, F_BUILD_F4, &[0, map_x, top, 3, side, 0, 0, 0]);
        let right_w = shape_wh(&mut c.m, shape(15)).0;
        let right = map_x.wrapping_add(corner_w).wrapping_add(side_w).wrapping_add(right_w).wrapping_sub(3);
        call_addr(c, F_BUILD_F4, &[0, right, top, 3, 3, 0, 0, 0]);
    });
}

/// `Hud_RenderHudView` (HUD.CPP, 0x800d8ea4; the host draws it itself): per player, into the
/// player's HUD view: the perp's overlay in a pursuit; for the last player the pursuit's big
/// time (its last 30 seconds) or the CD player (its intro at the race's start); with the HUD
/// shown the dashboard (the speedometer, the tachometer's needle, the numbers), the next
/// racer's name over the map (by the setting; not as the cop or split screen linked) and the
/// map's frame; the replay's controls (the first player's, a replay playing, not on the stats
/// screen); the wingman's orders; the texture pages between.
pub fn render_hud_view(c: &mut Cpu) {
    let mut p = 0;
    while p as i32 <= c.m.r32(HUD_LAST_PLAYER) as i32 {
        let dy = if p != 0 { (-15i32) as u32 } else { 0 };
        let view = c.m.r32(HUD_VIEW + 4 * p);
        call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
        let m = &mut c.m;
        if GAME_SETUP.race_type(m) == RACE_PURSUIT && m.r32(PERP_OVERLAY_ON + 4 * p) != 0 {
            perp_overlay(c, p, dy);
        }
        let m = &mut c.m;
        if p == m.r32(HUD_LAST_PLAYER) {
            let seconds = m.r32(BTC_COUNTDOWN) as i32 >> 6;
            if (seconds as u32).wrapping_sub(1) < BTC_SECONDS_SHOWN {
                call_addr(c, 0x800d_8b84, &[seconds as u32]);
            } else {
                let intro = crate::globals::SIM_COUNTER.get(m) < 576 || (m.r8(COUNTDOWN) < 4 && m.r32(BE_THE_COP) == 0);
                call_addr(c, 0x800d_6fb0, &[!intro as u32, p]);
            }
        }
        let m = &mut c.m;
        let car = GAME_SETUP.car_info(p);
        if m.r32(HUD_SHOWN + 4 * p) != 0 {
            let sprites = m.r32(SPRITES[(p != 0) as usize]);
            // DashHud_Speedo, DashHud_Tach, Hud_BuildNumbers0, Hud_BuildNumbers
            for f in [0x8009_bac4, 0x8009_bb34, 0x800d_51f8, 0x800d_5a44] {
                call_addr(c, f, &[p]);
            }
            let m = &mut c.m;
            link(m, TPAGE[0] + 48 * p);
            if car.hud_tach(m) != 0 {
                link(m, sprites + SPRT_SIZE);
            }
            if car.hud_map(m) != 0 {
                let label = car.hud_opponent_id(m) != 0 && m.r32(BE_THE_COP) == 0 && GAME_SETUP.comm_mode(m) != 1;
                if label {
                    let voice = call_addr(c, 0x800d_8120, &[p]) as i32;
                    if (0..9).contains(&voice) && c.m.r32(BE_THE_COP) == 0 {
                        next_racer_label(c, p, dy);
                    }
                }
                let m = &mut c.m;
                for k in 63..71 {
                    link(m, sprites + SPRT_SIZE * k);
                }
            }
        }
        let m = &mut c.m;
        let replay = crate::globals::REPLAY_INTERFACE;
        if p == 0 && m.r32(REPLAY_MODE) as i32 >= 2 && replay.stats_screen(m) == 0 {
            call_addr(c, 0x800d_7e24, &[]);
        }
        let m = &mut c.m;
        link(m, TPAGE[1] + 48 * p);
        if m.r32(HUD_SHOWN + 4 * p) != 0 && m.r8(WINGMAN_INTERFACE + p) != 0 && (m.r32(REPLAY_MODE) as i32) < 2 {
            call_addr(c, 0x800d_6c40, &[p]);
        }
        let m = &mut c.m;
        link(m, TPAGE[0] + 48 * p + 12);
        let view = m.r32(HUD_VIEW + 4 * p);
        call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
        p += 1;
    }
}

pub fn abi_render_hud_view(c: &mut Cpu) {
    render_hud_view(c);
}

/// `Hud_RenderTacView` (HUD.CPP, 0x800d981c; the host draws it itself): per player with the
/// tachometer and the HUD shown, into the player's tachometer view: its needle's parts and the
/// tachometer, on its texture page.
pub fn render_tac_view(c: &mut Cpu) {
    let mut p = 0;
    while p as i32 <= c.m.r32(HUD_LAST_PLAYER) as i32 {
        let m = &mut c.m;
        if GAME_SETUP.car_info(p).hud_tach(m) != 0 && m.r32(HUD_SHOWN + 4 * p) != 0 {
            let view = m.r32(TAC_VIEW + 4 * p);
            call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
            call(c, DASH_HUD_TACH, &[p]);
            call_addr(c, 0x800d_4a68, &[p]);
            link(&mut c.m, TPAGE[1] + 48 * p + 24);
            let view = c.m.r32(TAC_VIEW + 4 * p);
            call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
        }
        p += 1;
    }
}

pub fn abi_render_tac_view(c: &mut Cpu) {
    render_tac_view(c);
}

/// The time formats: "m:ss.hh" (the language's marks), and dashes for none; the seconds' marks
/// by language.
const TIME_FULL_FORMAT: u32 = 0x8005_70A8;
const TIME_NONE_FORMAT: u32 = 0x8005_70BC;
const SECOND_CHAR: u32 = 0x8013_EB8C;

/// `Hud_ParseTime` (HUD.CPP, 0x800d9938): the time `t` (64ths of a second, none below 0) into
/// `out` as minutes, seconds and hundredths with the language's marks; dashes for no time or
/// an hour and over. (Its hundredths come of `t * 25600 / 16384`, wrapping past 83886 64ths;
/// sprintf's result.)
pub fn parse_time(c: &mut Cpu, t: i32, out: u32) -> u32 {
    let t = t.max(0);
    let scaled = t.wrapping_mul(25600);
    let hundredths_all = (if scaled < 0 { scaled + 16383 } else { scaled }) >> 14;
    let minutes = (hundredths_all / 6000) as i16 as i32;
    let rest = hundredths_all.wrapping_sub(minutes * 6000);
    let seconds = (rest / 100) as i16 as i32;
    let hundredths = rest - seconds * 100;
    let shown = t != 0 && minutes < 60 && hundredths >= 0 && minutes >= 0;
    let language = GAME_SETUP.user_setting().language(&mut c.m) as u32;
    let minute_mark = c.m.r8(MINUTE_CHAR.wrapping_add(language)) as u32;
    let second_mark = c.m.r8(SECOND_CHAR.wrapping_add(language)) as u32;
    if shown {
        let args = [out, TIME_FULL_FORMAT, minutes as u32, minute_mark, seconds as u32, second_mark, hundredths as i16 as i32 as u32];
        call_addr(c, SPRINTF, &args)
    } else {
        call_addr(c, SPRINTF, &[out, TIME_NONE_FORMAT, minute_mark, second_mark])
    }
}

pub fn abi_parse_time(c: &mut Cpu) {
    c.r[2] = parse_time(c, c.r[4] as i32, c.r[5]);
}

/// `Hud_RenderPauseBox` (HUD.CPP, 0x800d9abc; the host hooks it): the pause menu's box at `x`,
/// `y`, `w` by `h`: textured corners below its title bar, a black line at the bottom, the thin
/// black frame, a black title bar 12 high, dark red below it.
pub fn render_pause_box(c: &mut Cpu, x: u32, y: u32, w: u32, h: u32) {
    let m = &mut c.m;
    let (cw, ch) = shape_wh(m, shape(BOX_CORNERS[3]));
    let (right, bottom) = (x.wrapping_add(w).wrapping_sub(cw), y.wrapping_add(h).wrapping_sub(ch));
    for (corner, cx, cy) in [(0, x.wrapping_sub(2), y.wrapping_add(10)), (1, right, y.wrapping_add(10)), (2, x.wrapping_sub(2), bottom), (3, right, bottom)] {
        f_build_ft4(m, shape(BOX_CORNERS[corner]), cx, cy, 0);
    }
    let foot = y.wrapping_add(h).wrapping_sub(2);
    call_addr(c, F_BUILD_F4, &[0, x, foot, w, 2, 0, 0, 0]);
    call_addr(c, BLACK_THIN_BOX, &[x, y, w, h]);
    call_addr(c, F_BUILD_F4, &[0, x, y, w, 12, 0, 0, 0]);
    call_addr(c, F_BUILD_F4, &[1, x, y.wrapping_add(12), w, h.wrapping_sub(12), PERP_BACK, 0, 0]);
}

pub fn abi_render_pause_box(c: &mut Cpu) {
    render_pause_box(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// When the bonus time was given (`BTC_BonusTimeTick`), the busted overlay's player (a half),
/// the stats screens' timers (a word a player).
const BTC_BONUS_TIME_TICK: u32 = 0x8013_F134;
const BUSTED_OVERLAY_PLAYER: u32 = 0x8013_EC24;
const STATS_TIMER: u32 = 0x8013_EC78;
/// The stats screen (a replay's) and the busted one (`0x800db314`), the wingman's command mode
/// (`Input_WingCommandMode`).
const STATS_SCREEN: fn(&mut Cpu) = nfs4rc::f_800dba78;
const BUSTED_SCREEN: fn(&mut Cpu) = nfs4rc::f_800db314;
const WING_COMMAND_MODE: fn(&mut Cpu) = nfs4rc::f_8009cecc;
/// The bonus time's pace into the pursuit's countdown (64ths a frame): faster with two
/// players unless both are cops; after 250 ticks.
const BONUS_STEP: [u32; 2] = [50, 250];
const BONUS_DELAY: i32 = 250;

/// `Hud_Render` (HUD.CPP, 0x800d9c44): the HUD's frame. On a replay's stats screen only that
/// (the HUD hidden at the race's start). Else: the bonus time counted into the pursuit's time;
/// before the cop has the controls just the replay's controls, the CD player and the race
/// time; the busted screen; the wingman's orders' interface following the command mode (off in
/// replays and once busted); the countdown (not as the cop); the HUD's, the map's and the
/// tachometer's views.
pub fn render(c: &mut Cpu) {
    use crate::globals::{GAME_TICKS, RACE_CARS, REPLAY_INTERFACE, SIM_COUNTER};
    let m = &mut c.m;
    if REPLAY_INTERFACE.stats_screen(m) != 0 {
        if SIM_COUNTER.get(m) < 576 {
            m.w32(HUD_SHOWN, 0);
            m.w32(HUD_SHOWN + 4, 0);
        }
        let view = m.r32(STATS_VIEW);
        m.w32(CURRENT_VIEW, view);
        call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
        call(c, STATS_SCREEN, &[]);
        let view = c.m.r32(STATS_VIEW);
        call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
        return;
    }
    let bonus = m.r32(BTC_BONUS_TIME) as i32;
    let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(BTC_BONUS_TIME_TICK)) as i32;
    if m.r32(BUSTED_OVERLAY) == 0 && bonus != 0 && since > BONUS_DELAY {
        let human_cop = |m: &mut Mem, k: u32| {
            let car = m.r32(RACE_CARS + 4 * k);
            m.r32(car + CAR_FLAGS) & CAR_HUMAN_COP != 0
        };
        let fast = m.r32(HUD_LAST_PLAYER) != 0 && !(human_cop(m, 0) && human_cop(m, 1));
        let step = BONUS_STEP[fast as usize] as i32;
        let bonus = m.r32(BTC_BONUS_TIME) as i32;
        let countdown = (m.r32(BTC_COUNTDOWN) as i32).wrapping_add(bonus.min(step));
        m.w32(BTC_COUNTDOWN, countdown as u32);
        m.w32(BTC_BONUS_TIME, bonus.wrapping_sub(step).max(0) as u32);
    }
    let m = &mut c.m;
    if m.r32(BE_THE_COP) != 0 && m.r32(BTC_USER_HAS_CONTROL) == 0 {
        let mut p = 0;
        while p as i32 <= c.m.r32(HUD_LAST_PLAYER) as i32 {
            let view = c.m.r32(HUD_VIEW + 4 * p);
            call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
            go_tpage(c, 1);
            let m = &mut c.m;
            if p == 0 && m.r32(REPLAY_MODE) as i32 >= 2 && REPLAY_INTERFACE.stats_screen(m) == 0 {
                call_addr(c, 0x800d_7e24, &[]);
            }
            let m = &mut c.m;
            if p == m.r32(HUD_LAST_PLAYER) {
                let intro = SIM_COUNTER.get(m) < 576 && m.r8(COUNTDOWN) == 0;
                call_addr(c, 0x800d_6fb0, &[!intro as u32, p]);
            }
            if c.m.r32(HUD_SHOWN + 4 * p) != 0 {
                call_addr(c, 0x800d_51f8, &[p]);
            }
            go_tpage(c, 0);
            let view = c.m.r32(HUD_VIEW + 4 * p);
            call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
            p += 1;
        }
        return;
    }
    let m = &mut c.m;
    if m.r32(BUSTED_OVERLAY) != 0 && (m.r32(REPLAY_MODE) as i32) < 2 {
        let view = m.r32(STATS_VIEW);
        m.w32(CURRENT_VIEW, view);
        call_addr(c, DRAW_START_RENDERING_VIEW, &[view]);
        let m = &mut c.m;
        let player = m.r16(BUSTED_OVERLAY_PLAYER) as i16 as i32 as u32;
        let timer = STATS_TIMER.wrapping_add(player << 2);
        let t = m.r32(timer).wrapping_add(1);
        m.w32(timer, t);
        call(c, BUSTED_SCREEN, &[player, 0]);
        let view = c.m.r32(STATS_VIEW);
        call_addr(c, DRAW_STOP_RENDERING_VIEW, &[view]);
    }
    for p in 0..2 {
        let mode = call(c, WING_COMMAND_MODE, &[p]);
        let m = &mut c.m;
        let shown = m.r8(WINGMAN_INTERFACE + p) as u32;
        let replay = m.r32(REPLAY_MODE) as i32 >= 2;
        let busted = m.r32(BUSTED_OVERLAY) != 0;
        // (off in a replay, else following the mode once its flash is over; off once busted
        // unless the mode moved meanwhile)
        let off = if shown == mode && busted {
            true
        } else if (m.r32(WINGMAN_FLASH_TICKS + 4 * p) as i32) < m.r32(GAME_TICKS) as i32 {
            if shown != mode && !replay && !busted {
                init_map_frame(c, p, mode);
                c.m.w8(WINGMAN_INTERFACE + p, mode as u8);
            }
            replay
        } else {
            false
        };
        if off {
            c.m.w8(WINGMAN_INTERFACE + p, 0);
            init_map_frame(c, p, 0);
        }
    }
    let m = &mut c.m;
    m.w32(SHOWED_CD_PLAYER, 0);
    if SIM_COUNTER.get(m) < 576 && m.r8(COUNTDOWN) != 0 && m.r32(BE_THE_COP) == 0 {
        call_addr(c, 0x800d_8a38, &[]);
    }
    // Hud_RenderHudView, Hud_RenderMapView, Hud_RenderTacView
    for f in [0x800d_8ea4, 0x800d_840c, 0x800d_981c] {
        call_addr(c, f, &[]);
    }
}

pub fn abi_render(c: &mut Cpu) {
    render(c);
}

/// The tracks' map placings (by track: the scale across and down as floats, the offset across
/// and down and the turn as halves); 65536.0 as a float.
const MAP_SCALE_X_BY_TRACK: u32 = 0x8012_1DB0;
const MAP_SCALE_Y_BY_TRACK: u32 = 0x8012_1DDC;
const MAP_OFF_X_BY_TRACK: u32 = 0x8012_1E08;
const MAP_OFF_Y_BY_TRACK: u32 = 0x8012_1E20;
const MAP_ROTATE_BY_TRACK: u32 = 0x8012_1E38;
const FLOAT_65536: u32 = 0x4780_0000;
const MAP_ROTATE: u32 = 0x8013_EC50;
const FMUL: fn(&mut Cpu) = nfs4rc::f_800f3e0c;
const FTOI: fn(&mut Cpu) = nfs4rc::f_800f4294;
const CCOS: fn(&mut Cpu) = nfs4rc::f_800f4084;
const CSIN: fn(&mut Cpu) = nfs4rc::f_800f4154;

/// `Hud_PositionMap` (HUD.CPP, 0x800da0ac): the map placed for the race's track: its scale
/// (16.16), offset (2 to the left mirrored) and turn, the turn's cosine and sine (4096 a turn,
/// shifted to 16.16).
pub fn position_map(c: &mut Cpu) {
    let m = &mut c.m;
    let track = GAME_SETUP.track(m) as u32;
    let scale = |c: &mut Cpu, table: u32| {
        let f = c.m.r32(table.wrapping_add(track << 2));
        let f = call(c, FMUL, &[f, FLOAT_65536]);
        call(c, FTOI, &[f])
    };
    let x = scale(c, MAP_SCALE_X_BY_TRACK);
    c.m.w32(MAP_SCALE_X, x);
    let y = scale(c, MAP_SCALE_Y_BY_TRACK);
    let m = &mut c.m;
    let half = |m: &mut Mem, table: u32| m.r16(table.wrapping_add(track << 1)) as i16 as i32 as u32;
    let off_x = half(m, MAP_OFF_X_BY_TRACK);
    m.w32(MAP_SCALE_Y, y);
    m.w32(MAP_OFF_X, off_x);
    if GAME_SETUP.mirror_track(m) != 0 {
        m.w32(MAP_OFF_X, off_x.wrapping_sub(2));
    }
    let (off_y, rotate) = (half(m, MAP_OFF_Y_BY_TRACK), half(m, MAP_ROTATE_BY_TRACK));
    m.w32(MAP_OFF_Y, off_y);
    m.w32(MAP_ROTATE, rotate);
    let cos = call(c, CCOS, &[rotate]);
    c.m.w32(MAP_COS, cos << 4);
    let sin = call(c, CSIN, &[rotate]);
    c.m.w32(MAP_SIN, sin << 4);
}

pub fn abi_position_map(c: &mut Cpu) {
    position_map(c);
}

/// The perp being chased's name, the next perp's slot per player (a half each) and the perps'
/// records (`BTCPerpInfo`: 10 a player, 16 bytes: its name, +8 its time, +12 whether caught).
const BTC_CURRENT_PERP_NAME: u32 = 0x8013_FB58;
const NEXT_PERP: u32 = 0x8013_EC74;
const BTC_PERP_INFO: u32 = 0x8012_1EB0;
const PERP_INFO_SIZE: u32 = 16;
const PERP_INFOS: u32 = 160;
const PERP_TIME: u32 = 8;
const PERP_CAUGHT: u32 = 12;

/// `Hud_BTC_Update` (HUD.CPP, 0x800da1b4): the perp's name (as a format: copied), whether the
/// user has the controls; the pursuit's time to `seconds_half` (in 32nds? doubled into 64ths)
/// unless a bonus or the busted overlay holds it.
pub fn btc_update(c: &mut Cpu, name: u32, countdown: u32, has_control: u32) {
    call_addr(c, SPRINTF, &[BTC_CURRENT_PERP_NAME, name]);
    let m = &mut c.m;
    m.w32(BTC_USER_HAS_CONTROL, has_control);
    if m.r32(BTC_BONUS_TIME) == 0 && m.r32(BUSTED_OVERLAY) == 0 {
        m.w32(BTC_COUNTDOWN, countdown << 1);
    }
}

pub fn abi_btc_update(c: &mut Cpu) {
    btc_update(c, c.r[4], c.r[5], c.r[6]);
}

/// Player `p`'s next perp record.
fn next_perp(m: &mut Mem, p: u32) -> u32 {
    let k = m.r16(NEXT_PERP + 2 * p) as i16 as i32 as u32;
    BTC_PERP_INFO.wrapping_add(PERP_INFOS * p).wrapping_add(k.wrapping_mul(PERP_INFO_SIZE))
}

fn advance_perp(m: &mut Mem, p: u32) {
    let k = m.r16(NEXT_PERP + 2 * p).wrapping_add(1);
    m.w16(NEXT_PERP + 2 * p, k);
}

/// A perp got away from player `p`: its record named after the perp chased (the name copied
/// before the record is cleared, or after: a name of 8 or more runs into its time), not caught.
fn perp_escaped(c: &mut Cpu, p: u32, name_first: bool) {
    let name = |c: &mut Cpu| {
        let record = next_perp(&mut c.m, p);
        call_addr(c, SPRINTF, &[record, BTC_CURRENT_PERP_NAME]);
    };
    if name_first {
        name(c);
    }
    let record = next_perp(&mut c.m, p);
    c.m.w32(record + PERP_CAUGHT, 0);
    let record = next_perp(&mut c.m, p);
    c.m.w32(record + PERP_TIME, 0);
    if !name_first {
        name(c);
    }
    advance_perp(&mut c.m, p);
}

/// `Hud_BustedOverlayOn` (HUD.CPP, 0x800da218): player `player`'s stats timer reset; outside
/// replays the busted overlay up for them, the pursuit's time kept, the perp's record named
/// by `format` (sprintf'd with the original's leftovers: `5 * player`, `player`) and, if
/// `caught`, marked so with time `time`; else a record for each player whose last perp was
/// caught (or who has none) marked escaped.
pub fn busted_overlay_on(c: &mut Cpu, time: u32, format: u32, caught: u32, player: u32) {
    let m = &mut c.m;
    let p = player as i16 as i32 as u32;
    m.w32(STATS_TIMER.wrapping_add(p << 2), 0);
    if m.r32(REPLAY_MODE) as i32 >= 2 {
        return;
    }
    let countdown = m.r32(BTC_COUNTDOWN);
    let record = next_perp(m, p);
    m.w32(FINAL_BTC_COUNTDOWN, countdown);
    call_addr(c, SPRINTF, &[record, format, p.wrapping_mul(5), player]);
    if caught != 0 {
        let m = &mut c.m;
        let record = next_perp(m, p);
        m.w32(record + PERP_CAUGHT, 1);
        let record = next_perp(m, p);
        m.w32(record + PERP_TIME, time);
        advance_perp(m, p);
    } else {
        for k in 0..2 {
            let m = &mut c.m;
            let n = m.r16(NEXT_PERP + 2 * k) as i16 as i32;
            let last_caught = n == 0 || {
                let last = next_perp(m, k).wrapping_sub(PERP_INFO_SIZE);
                m.r32(last + PERP_CAUGHT) != 0
            };
            if last_caught {
                perp_escaped(c, k, false);
            }
        }
    }
    let m = &mut c.m;
    m.w32(BUSTED_OVERLAY, 1);
    m.w16(BUSTED_OVERLAY_PLAYER, player as u16);
}

pub fn abi_busted_overlay_on(c: &mut Cpu) {
    busted_overlay_on(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hud_BustedOverlayOff` (HUD.CPP, 0x800da3d0).
pub fn busted_overlay_off(m: &mut Mem) {
    m.w32(BUSTED_OVERLAY, 0);
}

pub fn abi_busted_overlay_off(c: &mut Cpu) {
    busted_overlay_off(&mut c.m);
}

/// `Hud_Reset` (HUD.CPP, 0x800da3dc): outside replays the perps' records from the first and no
/// perp's name; no bonus time, busted overlay, perp overlays or user control.
pub fn reset(m: &mut Mem) {
    if (m.r32(REPLAY_MODE) as i32) < 2 {
        for p in 0..2 {
            m.w16(NEXT_PERP + 2 * p, 0);
            m.w8(BTC_CURRENT_PERP_NAME, 0);
        }
    }
    for at in [BTC_BONUS_TIME, BUSTED_OVERLAY, PERP_OVERLAY_ON, PERP_OVERLAY_ON + 4, BTC_USER_HAS_CONTROL] {
        m.w32(at, 0);
    }
}

pub fn abi_reset(c: &mut Cpu) {
    reset(&mut c.m);
}

/// `Hud_BTC_QuitOut` (HUD.CPP, 0x800da434): quitting a pursuit: the perp escaped from both
/// players (unless already busted).
pub fn btc_quit_out(c: &mut Cpu) {
    if c.m.r32(BUSTED_OVERLAY) != 0 {
        return;
    }
    for p in 0..2 {
        perp_escaped(c, p, true);
    }
}

pub fn abi_btc_quit_out(c: &mut Cpu) {
    btc_quit_out(c);
}

/// `Hud_BTC_BonusTime` (HUD.CPP, 0x800da4f8): bonus time `t` (64ths) given now.
pub fn btc_bonus_time(m: &mut Mem, t: u32) {
    let now = m.r32(crate::globals::GAME_TICKS);
    m.w32(BTC_BONUS_TIME, t);
    m.w32(BTC_BONUS_TIME_TICK, now);
}

pub fn abi_btc_bonus_time(c: &mut Cpu) {
    btc_bonus_time(&mut c.m, c.r[4]);
}

/// `Hud_Perp_OverlayOn` (HUD.CPP, 0x800da510): player `p`'s perp overlay up with message
/// `message`.
pub fn perp_overlay_on(m: &mut Mem, p: u32, message: u32) {
    m.w32(PERP_OVERLAY_ON.wrapping_add(p << 2), 1);
    m.w32(PERP_OVERLAY_MESSAGE.wrapping_add(p << 2), message);
}

pub fn abi_perp_overlay_on(c: &mut Cpu) {
    perp_overlay_on(&mut c.m, c.r[4], c.r[5]);
}

/// `Hud_Perp_OverlayOff` (HUD.CPP, 0x800da53c).
pub fn perp_overlay_off(m: &mut Mem, p: u32) {
    m.w32(PERP_OVERLAY_ON.wrapping_add(p << 2), 0);
}

pub fn abi_perp_overlay_off(c: &mut Cpu) {
    perp_overlay_off(&mut c.m, c.r[4]);
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
    const PRIM_END: u32 = 0x1F80_0008;
    const MAX_PRIM: u32 = 0x8013_EAA8;
    const TEXT: u32 = 0x801E_2B00;

    /// The scratchpad's ordering table and primitive buffer pointed at free RAM full of any
    /// bytes (the tags' top bytes are kept); the sprite colour and transparency, the HUD's
    /// views' globals any; a string of any case.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(gpu::OT, OT_ENTRY);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        // (the buffer's end far enough on, and the views' (`Draw_gMaxPrim`'s): the halos want
        // 4 KB to spare)
        m.w32(PRIM_END, PRIMS + 0x8000);
        m.w32(MAX_PRIM, PRIMS + 0x8000);
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
            ("Hud_InitMap", 0x800d_661c, nfs4rc::f_800d661c, |c, r| {
                // fewer cars of each list, any HUD colours
                let m = &mut c.m;
                for (cars, count) in [(crate::globals::RACE_CARS, crate::globals::NUM_RACE_CARS), (crate::globals::COP_CARS, crate::globals::NUM_COP_CARS)] {
                    let n = count.get(m).max(0) as u32;
                    for k in 0..n {
                        let car = m.r32(cars + 4 * k);
                        let info = m.r32(car + CAR_INFO);
                        m.w32(info + HUD_COLOUR, r.next() as u32);
                    }
                    count.set(m, pick(r, n + 1) as i32);
                }
                vec![]
            }),
            ("Hud_BuildMapMarkers", 0x800d_66bc, nfs4rc::f_800d66bc, |c, r| {
                // the map's place, scale and turn, mirrored or not, the lights' phase, the cars
                // in the world or not, their flags and lights
                let m = &mut c.m;
                m.w32(MAP_OFF_X, coord(r));
                m.w32(MAP_OFF_Y, coord(r));
                m.w32(MAP_SCALE_X, [1, 7, 40, 0x100, (-3i32) as u32][pick(r, 5) as usize]);
                m.w32(MAP_SCALE_Y, [1, 7, 40, 0x100, (-3i32) as u32][pick(r, 5) as usize]);
                m.w32(MAP_COS, [0x1_0000, 0, 0xB505, (-0x1_0000i32) as u32, r.next() as u32 & 0x1_FFFF][pick(r, 5) as usize]);
                m.w32(MAP_SIN, [0x1_0000, 0, 0xB505, (-0x1_0000i32) as u32, r.next() as u32 & 0x1_FFFF][pick(r, 5) as usize]);
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                m.w32(crate::globals::FLIP, pick(r, 2));
                m.w32(QUICK_PAUSE_SIM, pick(r, 2));
                for (cars, count) in [(crate::globals::RACE_CARS, crate::globals::NUM_RACE_CARS), (crate::globals::COP_CARS, crate::globals::NUM_COP_CARS)] {
                    for k in 0..count.get(m).max(0) as u32 {
                        let car = m.r32(cars + 4 * k);
                        m.w8(car + CAR_ACTIVE, (pick(r, 4) != 0) as u8);
                        m.w32(car + CAR_FLAGS, [0, CAR_HUMAN, CAR_HUMAN_COP, CAR_HUMAN | CAR_HUMAN_COP, r.next() as u32][pick(r, 5) as usize]);
                        m.w32(car + CAR_AI_FLAGS, pick(r, 4));
                    }
                }
                vec![]
            }),
            ("Hud_WingmanFlash", 0x800d_6b8c, nfs4rc::f_800d6b8c, |c, r| {
                // a replay or not, busted or not, the interface shown or not
                let m = &mut c.m;
                m.w32(REPLAY_MODE, pick(r, 4));
                m.w32(BUSTED_OVERLAY, (pick(r, 4) == 0) as u32);
                for p in 0..2 {
                    m.w8(WINGMAN_INTERFACE + p, pick(r, 3) as u8);
                }
                m.w32(SPRITES[0], PRIMS);
                m.w32(SPRITES[1], PRIMS + 0x800);
                vec![pick(r, 2), r.next() as u32]
            }),
            ("Hud_BuildWingmanInterface", 0x800d_6c40, nfs4rc::f_800d6c40, |c, r| {
                // the flash's ticks left (none, some, past), the icon, the places
                let p = pick(r, 2);
                let m = &mut c.m;
                let now = m.r32(crate::globals::GAME_TICKS);
                m.w32(WINGMAN_FLASH_TICKS + 4 * p, now.wrapping_add([0, 1, 9, 10, 57, 100, (-5i32) as u32][pick(r, 7) as usize]));
                m.w8(WINGMAN_FLASH_ICON + p, [0, 1, 3, 255][pick(r, 4) as usize]);
                for pad in 0..2 {
                    m.w8(PAD_INFO + 32 * pad + PAD_TYPE, [NEGCON, 4][pick(r, 2) as usize]);
                }
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                m.w32(MAP_OFFSET_Y, [0, 8, (-6i32) as u32][pick(r, 3) as usize]);
                vec![p]
            }),
            ("Hud_InitCdPlayer", 0x800d_6f8c, nfs4rc::f_800d6f8c, |c, r| {
                c.m.w32(REPLAY_MODE, [0, 1, 2, 3, (-1i32) as u32][pick(r, 5) as usize]);
                vec![]
            }),
            ("Hud_BuildCdPlayer", 0x800d_6fb0, nfs4rc::f_800d6fb0, |c, r| {
                // the music's state (none, a track, the CD's states), the song's title and
                // artist, the player open or not and its scroll, the start, the pads
                let m = &mut c.m;
                const AUDIO_MUS: u32 = 0x8013_DA14;
                let mus = m.r32(AUDIO_MUS);
                if pick(r, 8) == 0 {
                    m.w32(AUDIO_MUS, 0);
                } else if mus != 0 {
                    m.w32(mus + 28, [0, 0, 0, (-2i32) as u32, (-3i32) as u32, (-4i32) as u32, (-5i32) as u32, 3][pick(r, 8) as usize]);
                    m.w32(mus + 12, [(-1i32) as u32, 0, 4, 11][pick(r, 4) as usize]);
                    m.w32(mus + 160, [0, 1000, 59_999, 60_000, 61_500, 119_999, 754_321, r.next() as u32 & 0xF_FFFF][pick(r, 8) as usize]);
                    m.w32(mus + 24, pick(r, 2));
                    m.w32(mus + 264 + 16, [0, TEXT, TEXT][pick(r, 3) as usize]);
                    m.w32(mus + 264 + 20, [0, TEXT + 0x40][pick(r, 2) as usize]);
                }
                for (at, n) in [(TEXT, pick(r, 40)), (TEXT + 0x40, pick(r, 20))] {
                    for k in 0..48 {
                        const CHARS: &[u8] = b"aZ 09m.,'-\xe9";
                        let ch = if k < n { CHARS[pick(r, CHARS.len() as u32) as usize] } else { 0 };
                        m.w8(at + k, ch);
                    }
                }
                m.w32(CD_ACTIVE, (pick(r, 4) == 0) as u32);
                m.w32(TURN_SONG_OFF_NEXT, 0);
                m.w32(CD_PLAYER_ALLOWED, pick(r, 2));
                m.w32(CD_INTRO, pick(r, 2));
                m.w32(CD_SCROLL_TITLE, [0, 1, 40, 76, 150, 400, r.next() as u32 & 0x1FF][pick(r, 7) as usize]);
                let now = m.r32(crate::globals::GAME_TICKS);
                m.w32(CD_LAST_TICK, now.wrapping_add([0, 1, (-1i32) as u32, (-3i32) as u32, (-40i32) as u32, (-128i32) as u32, (-129i32) as u32, (-500i32) as u32][pick(r, 8) as usize]));
                crate::globals::SIM_COUNTER.set(m, [0, 575, 576, 10_000, 10_000, 10_000][pick(r, 6) as usize]);
                m.w8(COUNTDOWN, [0, 3, 4, 9][pick(r, 4) as usize]);
                m.w32(BE_THE_COP, pick(r, 2));
                m.w32(HUD_LAST_PLAYER, pick(r, 2));
                for k in 0..64 {
                    m.w8(PAD_INFO + k, r.next() as u8);
                }
                for pad in 0..2 {
                    m.w8(PAD_INFO + 32 * pad, 1);
                    m.w8(PAD_INFO + 32 * pad + PAD_TYPE, [NEGCON, 4][pick(r, 2) as usize]);
                }
                GAME_SETUP.user_setting().set_language(m, pick(r, 6) as i32);
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                vec![pick(r, 2)]
            }),
            ("Hud_BuildRadar", 0x800d_771c, nfs4rc::f_800d771c, |c, r| {
                // the camera's turn, the cars round it in the world or not, near or far, their
                // flags; mirrored or not; the lights' phase
                let p = pick(r, 2);
                let m = &mut c.m;
                let camera = m.r32(CAMERA_INFO + CAMERA_INFO_SIZE * p);
                // (the identity a third of the time: places land on the radar's edges exactly)
                let identity = pick(r, 3) == 0;
                for k in 0..9 {
                    let v = if identity { [0x1_0000, 0, 0, 0][(k % 4 != 0) as usize] } else { [0x1_0000, 0, (-0x1_0000i32) as u32, 0xB505, r.next() as u32 & 0x1_FFFF][pick(r, 5) as usize] };
                    m.w32(camera + CAMERA_MATRIX + 4 * k, v);
                }
                let (cx, cz) = (m.r32(camera + CAR_X), m.r32(camera + CAR_Z));
                for (cars, count) in [(crate::globals::RACE_CARS, crate::globals::NUM_RACE_CARS), (crate::globals::COP_CARS, crate::globals::NUM_COP_CARS)] {
                    for k in 0..count.get(m).max(0) as u32 {
                        let car = m.r32(cars + 4 * k);
                        m.w8(car + CAR_ACTIVE, (pick(r, 4) != 0) as u8);
                        let near = |r: &mut Rng| ([0, 0x1000, 0x5000, 0x14000, 0x30000, 0x60000, 0x15_0000, 0x16_0000, 0x2F_0000, 0x30_0000, 0x58_0000][pick(r, 11) as usize] as u32).wrapping_mul([1, (-1i32) as u32][pick(r, 2) as usize]);
                        m.w32(car + CAR_X, cx.wrapping_add(near(r)));
                        m.w32(car + CAR_Z, cz.wrapping_add(near(r)));
                        m.w32(car + CAR_FLAGS, [0, CAR_HUMAN, CAR_AI_RACER, r.next() as u32][pick(r, 4) as usize]);
                        m.w32(car + CAR_INDEX, pick(r, 4));
                    }
                }
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                m.w32(crate::globals::FLIP, pick(r, 2));
                m.w32(QUICK_PAUSE_SIM, pick(r, 2));
                vec![p]
            }),
            ("Hud_BuildReplay", 0x800d_7e24, nfs4rc::f_800d7e24, |c, r| {
                let m = &mut c.m;
                let replay = crate::globals::REPLAY_INTERFACE;
                m.w32(SPRITES[0], PRIMS + 0x400);
                m.w32(replay.0, pick(r, 2));
                m.w32(replay.0 + 4, [(-1i32) as u32, 0, 1, 2, 3, 4][pick(r, 6) as usize]);
                m.w32(replay.0 + 0xC, pick(r, 6));
                m.w32(replay.0 + 0x10, pick(r, 5));
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                vec![]
            }),
            ("Hud_NextPlayer", 0x800d_8120, nfs4rc::f_800d8120, next_racer_case),
            ("Hud_NextPlayerNameOrCarOrTime", 0x800d_8284, nfs4rc::f_800d8284, next_racer_case),
            ("Hud_RenderMapView", 0x800d_840c, nfs4rc::f_800d840c, |c, r| {
                // one player or two, the map setting (none, the track's, the radar), shown or
                // not, the wingman's orders up or not
                let m = &mut c.m;
                m.w32(HUD_LAST_PLAYER, pick(r, 2));
                for p in 0..2 {
                    GAME_SETUP.car_info(p).set_hud_map(m, pick(r, 3) as i32);
                    m.w32(HUD_SHOWN + 4 * p, (pick(r, 4) != 0) as u32);
                    m.w8(WINGMAN_INTERFACE + p, (pick(r, 4) == 0) as u8);
                }
                vec![]
            }),
            ("Hud_BlackThinBox", 0x800d_8690, nfs4rc::f_800d8690, |_, r| vec![coord(r), coord(r), coord(r), coord(r)]),
            ("Hud_Draw321Num", 0x800d_887c, nfs4rc::f_800d887c, |_, r| {
                vec![coord(r), coord(r), pick(r, 5), [0, 0, 6000, 40, r.next() as u32 & 0x1FFF][pick(r, 5) as usize], pick(r, 2), pick(r, 2)]
            }),
            ("Hud_Render321Go", 0x800d_8a38, nfs4rc::f_800d8a38, |c, r| {
                // the countdown (as last seen or not), the ticks since it changed
                let m = &mut c.m;
                let countdown = [0, 1, 2, 3, 4, 5, 200][pick(r, 7) as usize];
                m.w8(COUNTDOWN, countdown);
                m.w8(LAST_COUNTDOWN, if pick(r, 2) == 0 { countdown } else { r.next() as u8 });
                let now = m.r32(crate::globals::GAME_TICKS);
                m.w32(COUNTDOWN_TICK, now.wrapping_sub([0, 1, 50, 99, 100, 5000, (-3i32) as u32][pick(r, 7) as usize]));
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                vec![]
            }),
            ("Hud_BigBTCTime", 0x800d_8b84, nfs4rc::f_800d8b84, |c, r| {
                // a bonus time or busted (mostly not), the time as last seen or not and since
                // when, the beep played or not
                let m = &mut c.m;
                m.w32(BTC_BONUS_TIME, (pick(r, 6) == 0) as u32);
                m.w32(BUSTED_OVERLAY, (pick(r, 6) == 0) as u32);
                let t = [(-1i32) as u32, 0, 1, 7, 10, 11, 12, 45, 99, 130][pick(r, 10) as usize];
                m.w32(LAST_BTC_TIME, if pick(r, 2) == 0 { t } else { r.next() as u32 & 0x3F });
                let now = m.r32(crate::globals::GAME_TICKS);
                m.w32(LAST_BTC_TICK, now.wrapping_sub([0, 1, 63, 64, 65, 1000][pick(r, 6) as usize]));
                m.w32(BTC_BEEPED, pick(r, 2));
                m.w32(POSITIONS, [ELEMENT_POSITIONS, ELEMENT_POSITIONS_2P][pick(r, 2) as usize]);
                vec![t]
            }),
            ("Hud_RenderHudView", 0x800d_8ea4, nfs4rc::f_800d8ea4, |c, r| {
                hud_frame_state(c, r);
                vec![]
            }),
            ("Hud_RenderTacView", 0x800d_981c, nfs4rc::f_800d981c, |c, r| {
                hud_frame_state(c, r);
                vec![]
            }),
            ("Hud_ParseTime", 0x800d_9938, nfs4rc::f_800d9938, |c, r| {
                GAME_SETUP.user_setting().set_language(&mut c.m, pick(r, 6) as i32);
                // (1006569 wraps to just below 0 hundredths: a minus second)
                let t = [0, 1, 63, 64, 3839, 3840, 230_399, 230_400, 83_886, 83_887, 167_773, 1_006_569, 1_006_569, (-1i32) as u32, i32::MAX as u32, r.next() as u32 & 0xF_FFFF, r.next() as u32];
                vec![t[pick(r, t.len() as u32) as usize], TEXT]
            }),
            ("Hud_RenderPauseBox", 0x800d_9abc, nfs4rc::f_800d9abc, |_, r| vec![coord(r), coord(r), coord(r), coord(r)]),
            ("Hud_Render", 0x800d_9c44, nfs4rc::f_800d9c44, |c, r| {
                hud_frame_state(c, r);
                let m = &mut c.m;
                m.w32(BTC_BONUS_TIME, [0, 0, 40, 300, (-7i32) as u32][pick(r, 5) as usize]);
                let now = m.r32(crate::globals::GAME_TICKS);
                m.w32(BTC_BONUS_TIME_TICK, now.wrapping_sub([0, 250, 251, 1000][pick(r, 4) as usize]));
                m.w32(BUSTED_OVERLAY, (pick(r, 5) == 0) as u32);
                m.w16(BUSTED_OVERLAY_PLAYER, pick(r, 2) as u16);
                m.w32(BTC_USER_HAS_CONTROL, pick(r, 2));
                for k in 0..2 {
                    let car = m.r32(crate::globals::RACE_CARS + 4 * k);
                    let flags = m.r32(car + CAR_FLAGS) & !CAR_HUMAN_COP;
                    m.w32(car + CAR_FLAGS, flags | if pick(r, 3) != 0 { CAR_HUMAN_COP } else { 0 });
                }
                for p in 0..2 {
                    m.w32(WINGMAN_FLASH_TICKS + 4 * p, now.wrapping_add([0, 1, (-1i32) as u32, 50][pick(r, 4) as usize]));
                }
                vec![]
            }),
            ("Hud_PositionMap", 0x800d_a0ac, nfs4rc::f_800da0ac, |c, r| {
                let m = &mut c.m;
                GAME_SETUP.set_track(m, pick(r, 11) as i32);
                crate::globals::I32(GAME_SETUP.0 + 0x2C).set(m, pick(r, 2) as i32);
                vec![]
            }),
            ("Hud_BTC_Update", 0x800d_a1b4, nfs4rc::f_800da1b4, |c, r| {
                let m = &mut c.m;
                m.w32(BTC_BONUS_TIME, (pick(r, 3) == 0) as u32);
                m.w32(BUSTED_OVERLAY, (pick(r, 3) == 0) as u32);
                vec![TEXT, r.next() as u32 & 0xFFFF, pick(r, 2)]
            }),
            ("Hud_BustedOverlayOn", 0x800d_a218, nfs4rc::f_800da218, |c, r| {
                perp_records(c, r);
                vec![r.next() as u32, TEXT, pick(r, 2), pick(r, 2)]
            }),
            ("Hud_BustedOverlayOff", 0x800d_a3d0, nfs4rc::f_800da3d0, |_, _| vec![]),
            ("Hud_Reset", 0x800d_a3dc, nfs4rc::f_800da3dc, |c, r| {
                c.m.w32(REPLAY_MODE, pick(r, 4));
                c.m.w8(BTC_CURRENT_PERP_NAME, b'a' + pick(r, 26) as u8);
                vec![]
            }),
            ("Hud_BTC_QuitOut", 0x800d_a434, nfs4rc::f_800da434, |c, r| {
                perp_records(c, r);
                vec![]
            }),
            ("Hud_BTC_BonusTime", 0x800d_a4f8, nfs4rc::f_800da4f8, |_, r| vec![r.next() as u32]),
            ("Hud_Perp_OverlayOn", 0x800d_a510, nfs4rc::f_800da510, |_, r| vec![pick(r, 2), r.next() as u32]),
            ("Hud_Perp_OverlayOff", 0x800d_a53c, nfs4rc::f_800da53c, |_, r| vec![pick(r, 2)]),
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

    /// Looking back or not, going the wrong way or not, a reversed track or not, the cars' kinds
    /// and the opponent's name's setting.
    fn next_racer_case(c: &mut Cpu, r: &mut Rng) -> Vec<u32> {
        let p = pick(r, 2);
        let m = &mut c.m;
        m.w32(LOOK_BEHIND + 4 * p, [0, 1, (-1i32) as u32][pick(r, 3) as usize]);
        m.w32(HUD_WARNING + 4 * p, [0, 1, (-1i32) as u32][pick(r, 3) as usize]);
        crate::globals::I32(GAME_SETUP.0 + 0x30).set(m, pick(r, 2) as i32);
        GAME_SETUP.car_info(p).set_hud_opponent_id(m, pick(r, 4) as i32);
        // the player first in the race order half the time
        if pick(r, 2) == 0 {
            let car = m.r32(crate::globals::HUMAN_RACE_CARS + 4 * p);
            let index = m.r32(car + CAR_INDEX);
            m.w32(crate::globals::RACE_POSITION, index);
        }
        for k in 0..crate::globals::NUM_CARS.get(m).max(0) as u32 {
            let car = m.r32(SORTED_LIST + 4 * k);
            if car != 0 {
                m.w32(car + CAR_FLAGS, [0, CAR_HUMAN, CAR_AI_RACER, 0x10, 0x20][pick(r, 5) as usize]);
            }
        }
        vec![p]
    }

    /// A frame's HUD state: one player or two, the HUD's settings and shown or not, the cop, a
    /// pursuit with the perp's overlay, the pursuit's time, the start, a replay and its stats
    /// screen, the wingman's orders; the sprite buffers in free RAM.
    fn hud_frame_state(c: &mut Cpu, r: &mut Rng) {
        let m = &mut c.m;
        m.w32(SPRITES[0], PRIMS + 0x400);
        m.w32(SPRITES[1], 0x801E_5000);
        m.w32(HUD_LAST_PLAYER, pick(r, 2));
        m.w32(BE_THE_COP, (pick(r, 4) == 0) as u32);
        GAME_SETUP.set_race_type(m, pick(r, 3) as i32);
        GAME_SETUP.set_comm_mode(m, pick(r, 3) as i32);
        m.w32(BTC_COUNTDOWN, [0, 64, 640, 64 * 30, 64 * 31, (-64i32) as u32][pick(r, 6) as usize]);
        crate::globals::SIM_COUNTER.set(m, [0, 575, 576, 10_000, 10_000][pick(r, 5) as usize]);
        m.w8(COUNTDOWN, [0, 2, 4, 9][pick(r, 4) as usize]);
        m.w32(REPLAY_MODE, [0, 1, 2, 3][pick(r, 4) as usize]);
        crate::globals::REPLAY_INTERFACE.set_stats_screen(m, (pick(r, 4) == 0) as i32);
        for p in 0..2 {
            let car = GAME_SETUP.car_info(p);
            car.set_hud_tach(m, pick(r, 2) as i32);
            car.set_hud_map(m, pick(r, 3) as i32);
            car.set_hud_opponent_id(m, pick(r, 3) as i32);
            m.w32(HUD_SHOWN + 4 * p, (pick(r, 4) != 0) as u32);
            m.w8(WINGMAN_INTERFACE + p, (pick(r, 4) == 0) as u8);
            m.w32(PERP_OVERLAY_ON + 4 * p, pick(r, 2));
            m.w32(PERP_OVERLAY_MESSAGE + 4 * p, pick(r, 4));
        }
    }

    /// The perps' records: the next slot per player (0 to 3), each record caught or not.
    fn perp_records(c: &mut Cpu, r: &mut Rng) {
        let m = &mut c.m;
        m.w32(REPLAY_MODE, [0, 1, 2][pick(r, 3) as usize]);
        m.w32(BUSTED_OVERLAY, (pick(r, 4) == 0) as u32);
        for p in 0..2 {
            m.w16(NEXT_PERP + 2 * p, pick(r, 4) as u16);
            for k in 0..10 {
                m.w32(BTC_PERP_INFO + PERP_INFOS * p + PERP_INFO_SIZE * k + PERP_CAUGHT, pick(r, 2));
            }
        }
        let n = pick(r, 12);
        for k in 0..16 {
            m.w8(BTC_CURRENT_PERP_NAME + k, if k < n { b'a' + pick(r, 26) as u8 } else { 0 });
        }
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
