//! PSXFRONT.CPP's shape drawing: a front-end shape (`tTexture_ShapeInfo`, 32 bytes: +4 its
//! CLUT, +8 its depth (4, 8 or 16 bits), +9 its blending, +16 and +18 its size, +20 and +22
//! its origin, +24 and +26 its texture's place in VRAM) drawn as `POLY_GT4`s: by its origin,
//! centred, mirrored about a line, coloured (one colour, four corners, a gradient by height)
//! and faded; whole when scaled, else in strips under 256 texels wide (a texture page's): those
//! `DrawGouraudShape` emits, which the host does.
//!
//! The extended drawing's options (`tDrawShapeExtended`): +0 the mirroring line, +4 its colour
//! (or the corners' four), +20 a shape table of its own.
//!
//! Written as FEMENU.CPP is (see there). `DrawGouraudShape` and `ScaleGouraudShape` are the
//! host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::types::TextureShapeInfo;
use crate::femenuextended::{NEXT_PRIM, OT};
use crate::globals::{CAR_NAMES, FLIP as G_FLIP, FRONT_END, GAME_SETUP, PATHS, PLAYER1_VIEW, SCREEN_WIDTH, SHAPES as HELP_SHAPES};
use ps1rt::{Cpu, Mem};

/// The permanent shapes' table now (FESCREEN's).
const PERMANENT_SHAPES: u32 = 0x8005_1C54;

// A shape's CLUT, depth, blending, size, origin, texture place.
const CLUT: u32 = 4;
const DEPTH: u32 = 8;
const BLEND: u32 = 9;
const WIDTH: u32 = 16;
const HEIGHT: u32 = 18;
const ORIGIN_X: u32 = 20;
const ORIGIN_Y: u32 = 22;
const TEX_X: u32 = 24;
const TEX_Y: u32 = 26;
const SHAPE_SIZE: u32 = 32;

// The drawing's flags: semi-transparent, flipped (vertically), mirrored (horizontally), the
// help's shapes, coloured (one colour), centred, mirrored about a line, a gradient by height,
// four corners, by its top-left corner, the options' shape table.
const SEMI: u32 = 1;
const FLIP: u32 = 2;
const MIRROR: u32 = 4;
const HELP: u32 = 8;
const COLOURED: u32 = 0x10;
const REFLECT: u32 = 0x20;
const GRADIENT: u32 = 0x40;
const CORNERS: u32 = 0x80;
const CENTRED: u32 = 0x100;
const OWN_TABLE: u32 = 0x200;
const TOP_LEFT: u32 = 0x400;

// The options: the mirroring line, the colour(s), the own table.
const LINE: u32 = 0;
const COLOUR: u32 = 4;
const TABLE: u32 = 20;

/// `DrawGouraudShape`: the host's (it draws the strips' quads).
const DRAW_GOURAUD_SHAPE: u32 = 0x8004_E74C;
/// `GetClut`, `SetPolyGT4`, `SetSemiTrans`, `fixedmult`.
const GET_CLUT: fn(&mut Cpu) = nfs4rc::f_800f4dac;
const SET_POLY_GT4: fn(&mut Cpu) = nfs4rc::f_800f3dec;
const SET_SEMI_TRANS: fn(&mut Cpu) = nfs4rc::f_800f3a58;
const FIXEDMULT: fn(&mut Cpu) = nfs4rc::f_800e4f14;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

/// A colour's channel faded: `v * fade / 128`, toward zero.
fn faded(v: u32, fade: u32) -> u32 {
    ((v as i32).wrapping_mul(fade as i32) / 128) as u32
}

/// A colour (`0x00BBGGRR` in a word's bytes 0, 1, 2) faded.
fn fade_colour(m: &mut Mem, at: u32, fade: u32) -> u32 {
    let b = faded(m.r8(at + 2) as u32, fade);
    let g = faded((m.r32(at) >> 8) & 0xFF, fade);
    let r = faded(m.r8(at) as u32, fade);
    (b << 16) | (g << 8) | r
}

/// Twice `v`'s distance from 0 (as a `short`), at most 128.
fn gradient_step(v: u32) -> u32 {
    let a = if (v << 16) as i32 >= 0 { v } else { 0u32.wrapping_sub(v) };
    let a = a << 1;
    if (sx(a) as i32) < 129 { a } else { 128 }
}

/// `AdjustShapeDrawing` (PSXFRONT.CPP, 0x8004e420): `x`, `y` and `flags` adjusted for the
/// shape: by its top-left corner, else by its origin (at or above the screen's top, `y` measured
/// up from it); centred; mirrored about the options' line (flipped). The colours at `colours`:
/// one (faded), four corners (faded), a gradient by distance from the line, or grey by the fade.
#[allow(clippy::too_many_arguments)]
pub fn adjust_shape_drawing(m: &mut Mem, shape: u32, x: &mut u32, y: &mut u32, flags: &mut u32, fade: u32, colours: u32, options: u32) {
    if *flags & TOP_LEFT != 0 {
        *x = x.wrapping_sub(lh(m, shape + ORIGIN_X));
        *y = y.wrapping_sub(lh(m, shape + ORIGIN_Y));
    } else if *y as i32 <= 0 {
        *x = x.wrapping_sub(lh(m, shape + ORIGIN_X));
        *y = 0u32.wrapping_sub(lh(m, shape + ORIGIN_Y).wrapping_add(*y));
    }
    if *flags & CENTRED != 0 {
        *x = x.wrapping_sub((lh(m, shape + WIDTH) as i32 >> 1) as u32);
        *y = y.wrapping_sub((lh(m, shape + HEIGHT) as i32 >> 1) as u32);
    }
    if *flags & REFLECT != 0 {
        let line = lh(m, options + LINE);
        *y = (line << 1).wrapping_sub(y.wrapping_add(lh(m, shape + HEIGHT))).wrapping_add(1);
        *flags |= FLIP;
    }
    if *flags & COLOURED != 0 {
        let v = fade_colour(m, options + COLOUR, fade);
        m.w32(colours, v);
    } else if *flags & CORNERS != 0 {
        for k in 0..4 {
            let v = fade_colour(m, options + COLOUR + 4 * k, fade);
            m.w32(colours + 4 * k, v);
        }
    } else if *flags & GRADIENT != 0 {
        // the colours by the top's and the bottom's distance from the line
        let line = m.r16(options + LINE) as u32;
        let top = *y & 0xFFFF;
        let near = gradient_step(line.wrapping_sub(top).wrapping_add(1));
        let bottom = top.wrapping_add(m.r16(shape + HEIGHT) as u32);
        let far = gradient_step(line.wrapping_sub(bottom).wrapping_add(1));
        let v = 128u32.wrapping_sub(sx(far));
        let grey = (v << 16) | (v << 8) | v;
        m.w32(colours, grey);
        m.w32(colours + 4, grey);
        let v = 128u32.wrapping_sub(sx(near));
        let grey = (v << 16) | (v << 8) | v;
        m.w32(colours + 8, grey);
        m.w32(colours + 12, grey);
    } else {
        m.w32(colours, (fade << 16) | (fade << 8) | fade);
    }
}

pub fn abi_adjust_shape_drawing(c: &mut Cpu) {
    let (shape, at_x, at_y, at_flags) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (fade, colours, options) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    let m = &mut c.m;
    let (mut x, mut y, mut flags) = (m.r32(at_x), m.r32(at_y), m.r32(at_flags));
    adjust_shape_drawing(m, shape, &mut x, &mut y, &mut flags, fade, colours, options);
    m.w32(at_x, x);
    m.w32(at_y, y);
    m.w32(at_flags, flags);
}

/// A `POLY_GT4` (52 bytes) taken from the scratchpad's buffer and linked into the ordering
/// table.
fn add_poly_gt4(m: &mut Mem) -> u32 {
    crate::mmeffect::add_prim(m, 52)
}

/// The four corners' colours into a primitive.
fn set_colours(m: &mut Mem, prim: u32, colours: u32) {
    for (k, at) in [4, 16, 28, 40].into_iter().enumerate() {
        let v = m.r32(colours + 4 * k as u32);
        m.w32(prim + at, v);
    }
}

/// A shape's texture page: its depth, the blending `abr`, its texture's page.
fn tpage(m: &mut Mem, shape: u32, abr: u32, tex_x: u32) -> u32 {
    let ty = m.r16(shape + TEX_Y) as u32;
    ((m.r8(shape + BLEND) as u32 & 3) << 7) | ((abr & 3) << 5) | ((ty & 0x100) >> 4) | ((tex_x & 0x3C0) >> 6) | ((ty & 0x200) << 2)
}

/// The CLUT packed in a shape (its x by 16 in the low 6 bits, its y above).
fn clut_args(m: &mut Mem, shape: u32) -> [u32; 2] {
    let v = m.r32(shape + CLUT);
    [(v & 0x3F) << 4, (v as i32 >> 6) as u32]
}

/// The first colour made all four's.
fn one_colour(m: &mut Mem, colours: u32) {
    let v = m.r32(colours);
    for k in [12, 8, 4] {
        m.w32(colours + k, v);
    }
}

/// `DrawFlatShape` (PSXFRONT.CPP, 0x8004eb20): `DrawGouraudShape` (the host's) with one colour.
pub fn draw_flat_shape(c: &mut Cpu, args: [u32; 6]) {
    one_colour(&mut c.m, args[4]);
    call_addr(c, DRAW_GOURAUD_SHAPE, &args);
}

pub fn abi_draw_flat_shape(c: &mut Cpu) {
    let args = [c.r[4], c.r[5], c.r[6], c.r[7], crate::abi::arg(c, 4), crate::abi::arg(c, 5)];
    draw_flat_shape(c, args);
}

/// `ScaleGouraudShape`: hooked (the host captures its primitives), so called by address.
const SCALE_GOURAUD_SHAPE: u32 = 0x8004_EC64;

/// Shape `k`'s entry: of the help's table, the options' own, or the permanent shapes'.
fn shape_of(m: &mut Mem, k: u32, flags: u32, options: u32) -> u32 {
    let table = if flags & HELP != 0 { m.r32(HELP_SHAPES) } else { m.r32(PERMANENT_SHAPES) };
    let at = k.wrapping_mul(SHAPE_SIZE);
    if flags & OWN_TABLE != 0 { m.r32(options + TABLE).wrapping_add(at) } else { table.wrapping_add(at) }
}

/// `DrawShapeExtended` (PSXFRONT.CPP, 0x8004eb60): shape `k` at `x`, `y`, adjusted and faded
/// (`fade`: 0 full, 128 gone), with four colours (gradient or corners) or one; blending `abr`.
#[allow(clippy::too_many_arguments)]
pub fn draw_shape_extended(c: &mut Cpu, k: u32, flags: u32, x: u32, y: u32, fade: u32, abr: u32, options: u32) {
    let shape = shape_of(&mut c.m, k, flags, options);
    with_buffer(c, 16, |c, colours| {
        let (mut x, mut y, mut flags) = (x, y, flags);
        adjust_shape_drawing(&mut c.m, shape, &mut x, &mut y, &mut flags, sx(128u32.wrapping_sub(fade)), colours, options);
        let args = [shape, flags, x, y, colours, abr];
        if flags & (GRADIENT | CORNERS) != 0 {
            call_addr(c, DRAW_GOURAUD_SHAPE, &args);
        } else {
            draw_flat_shape(c, args);
        }
    })
}

pub fn abi_draw_shape_extended(c: &mut Cpu) {
    let (fade, abr, options) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    draw_shape_extended(c, c.r[4], c.r[5], c.r[6], c.r[7], fade, abr, options);
}

/// `ScaleGouraudShape` (PSXFRONT.CPP, 0x8004ec64): the whole shape at `x`, `y` scaled by
/// `scale_x` and `scale_y` (16.16) as one `POLY_GT4`: cornered colours, semi-transparent,
/// flipped or mirrored (the far edge measured back by the scaled size).
#[allow(clippy::too_many_arguments)]
pub fn scale_gouraud_shape(c: &mut Cpu, shape: u32, flags: u32, x: u32, y: u32, scale_x: u32, scale_y: u32, colours: u32, abr: u32) {
    let (mut x, mut y) = (x, y);
    let m = &mut c.m;
    let mut w = m.r16(shape + WIDTH) as u32;
    let mut h = m.r16(shape + HEIGHT) as u32;
    let prim = add_poly_gt4(m);
    let depth = m.r8(shape + DEPTH) as i32;
    set_colours(m, prim, colours);
    call(c, SET_POLY_GT4, &[prim]);
    call(c, SET_SEMI_TRANS, &[prim, flags & SEMI]);
    let args = clut_args(&mut c.m, shape);
    let clut = call(c, GET_CLUT, &args);
    let m = &mut c.m;
    m.w16(prim + 14, clut as u16);
    let tex_x = m.r16(shape + TEX_X) as u32;
    let page = tpage(m, shape, abr, tex_x);
    m.w16(prim + 26, page as u16);
    let fixedmult = |c: &mut Cpu, a: u32, b: u32| call(c, FIXEDMULT, &[a, b]);
    let mirror = flags & MIRROR != 0;
    if mirror {
        x = x.wrapping_add(fixedmult(c, scale_x, sx(w)));
        w = 0u32.wrapping_sub(w);
    }
    let flip = flags & FLIP != 0;
    if flip {
        y = y.wrapping_add(fixedmult(c, scale_y, sx(h)));
        h = 0u32.wrapping_sub(h);
    }
    let (w, h) = (sx(w), sx(h));
    // the corners: (x, y), (x - 1 + w', y), (x, y + h'), (x - 1 + w', y + h')
    c.m.w16(prim + 8, x as u16);
    c.m.w16(prim + 10, y as u16);
    let x1 = x.wrapping_sub(1);
    let v = fixedmult(c, scale_x, w);
    c.m.w16(prim + 20, x1.wrapping_add(v) as u16);
    c.m.w16(prim + 22, y as u16);
    c.m.w16(prim + 32, x as u16);
    let v = fixedmult(c, scale_y, h);
    c.m.w16(prim + 34, y.wrapping_add(v) as u16);
    let v = fixedmult(c, scale_x, w);
    c.m.w16(prim + 44, x1.wrapping_add(v) as u16);
    let v = fixedmult(c, scale_y, h);
    let m = &mut c.m;
    m.w16(prim + 46, y.wrapping_add(v) as u16);
    // the texture's corners (u in the page by the depth; one in from a mirrored or flipped edge)
    let u = crate::fixed::div(((m.r16(shape + TEX_X) as u32 & 0x3F) << 4) as i32, depth) as u32;
    let (wb, hb, mut v0) = (m.r8(shape + WIDTH) as u32, m.r8(shape + HEIGHT) as u32, m.r8(shape + TEX_Y) as u32);
    let u0 = if mirror { u.wrapping_sub(1) } else { u };
    let u1 = u0.wrapping_add(wb);
    if flip {
        v0 = v0.wrapping_sub(1);
    }
    let v1 = v0.wrapping_add(hb);
    for (at, v) in [(13, v0), (25, v0), (12, u0), (24, u1), (36, u0), (37, v1), (48, u1), (49, v1)] {
        m.w8(prim + at, v as u8);
    }
}

pub fn abi_scale_gouraud_shape(c: &mut Cpu) {
    let a: Vec<u32> = (4..8).map(|k| crate::abi::arg(c, k)).collect();
    scale_gouraud_shape(c, c.r[4], c.r[5], c.r[6], c.r[7], a[0], a[1], a[2], a[3]);
}

/// `ScaleFlatShape` (PSXFRONT.CPP, 0x8004ef20): `ScaleGouraudShape` with one colour.
pub fn scale_flat_shape(c: &mut Cpu, args: [u32; 8]) {
    one_colour(&mut c.m, args[6]);
    call_addr(c, SCALE_GOURAUD_SHAPE, &args);
}

pub fn abi_scale_flat_shape(c: &mut Cpu) {
    let a: Vec<u32> = (4..8).map(|k| crate::abi::arg(c, k)).collect();
    scale_flat_shape(c, [c.r[4], c.r[5], c.r[6], c.r[7], a[0], a[1], a[2], a[3]]);
}

/// `ScaleShapeExtended` (PSXFRONT.CPP, 0x8004ef70): shape `k` at `x`, `y`, adjusted and faded,
/// drawn whole at twice its width.
#[allow(clippy::too_many_arguments)]
pub fn scale_shape_extended(c: &mut Cpu, k: u32, flags: u32, x: u32, y: u32, fade: u32, abr: u32, options: u32) {
    let shape = shape_of(&mut c.m, k, flags, options);
    with_buffer(c, 16, |c, colours| {
        let (mut x, mut y, mut flags) = (x, y, flags);
        adjust_shape_drawing(&mut c.m, shape, &mut x, &mut y, &mut flags, sx(128u32.wrapping_sub(fade)), colours, options);
        let args = [shape, flags, x, y, 0x2_0000, 0x1_0000, colours, abr];
        if flags & (GRADIENT | CORNERS) != 0 {
            call_addr(c, SCALE_GOURAUD_SHAPE, &args);
        } else {
            scale_flat_shape(c, args);
        }
    })
}

pub fn abi_scale_shape_extended(c: &mut Cpu) {
    let (fade, abr, options) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5), crate::abi::arg(c, 6));
    scale_shape_extended(c, c.r[4], c.r[5], c.r[6], c.r[7], fade, abr, options);
}

/// The front end's drawing environments (`gEnviro`: two display environments, +20 and +44 the
/// two primitive buffers), the camera view (`gCView`), the rear view, VSync's switch, the
/// screen's depth, the frame flip.
const ENVIRO: u32 = 0x8012_0714;
const C_VIEW: u32 = 0x8011_8234;
const REAR_VIEW: u32 = 0x8013_E6C4;
const DO_VSYNC: u32 = 0x8013_EA90;
const SCREEN_BPP: u32 = 0x8013_EF70;
/// The showroom's cars: set up (a byte), their two 3D car objects, the car flags and lights
/// they're drawn with; whether the 3D cars are drawn for the menus, whether the front end is
/// loading (the car code's), the setup's car records (180 bytes, +0 the car).
const CARS_UP: u32 = 0x8005_2EF0;
const CAR_OBJECTS: u32 = 0x8005_2EF8;
const CAR_FLAGS: u32 = 0x8005_174C;
const MENU_COLOR: u32 = 0x8013_EAE8;
const MENU_LIGHTS: u32 = 0x8013_EAF0;
const MENU_LIGHTS_DIRECTION: u32 = 0x8013_EAF4;
const IN_MENU: u32 = 0x8013_E608;
const IN_FRONT_END: u32 = 0x8013_EA40;
const SETUP_CARS: u32 = 0x8011_4878;
/// The first time the front end starts (the title and the language come first then); the
/// front end's language (+0x340, 255 none yet); the music's play list (+0x444).
const FIRST_TIME: u32 = 0x8005_1C74;
const LANGUAGE: u32 = FRONT_END + 0x340;
const PLAY_LIST: u32 = FRONT_END + 0x444;
/// The CD's path (`Paths` +0x80); the movie decoder's table (`CF_DVLC`).
const CD_PATH: u32 = PATHS + 0x80;
const DVLC: u32 = 0x8012_532C;
/// The heap's names and the files' (`"ps0"`, `"ps1"`, `"carObj"`, `"%sDCT.BIN"`,
/// `"gHelpShapes"`, `"%szperm.psh"`); `PAD_update`, which the front end's timer runs.
const PS0: u32 = 0x8001_279C;
const PS1: u32 = 0x8001_27A0;
const CAR_OBJ: u32 = 0x8001_27A4;
const DCT_FILE: u32 = 0x8001_27AC;
const HELP_NAME: u32 = 0x8001_27E0;
const ZPERM_FILE: u32 = 0x8001_27EC;
const TIMER: u32 = 0x800E_4E08;
/// The font's tint and CLUT.
const FONT_TINT: u32 = 0x8013_EB44;
const FONT_CLUT: u32 = 0x8013_EB4C;
/// A primitive buffer's size; the help shapes' (59 of 32 bytes, and a spare).
const PRIM_BUFFER: u32 = 0x1_3880;
const HELP_SIZE: u32 = 1888;
const HELP_COUNT: u32 = 59;
/// A showroom car object (2268 bytes): +145 shown, +576 its name, +648 its car record; a setup
/// car record's size; no language chosen yet.
const CAR_OBJECT_SIZE: u32 = 2268;
const CAR_SHOWN: u32 = 145;
const CAR_NAME: u32 = 576;
const CAR_RECORD: u32 = 648;
const SETUP_CAR_SIZE: u32 = 180;
const NO_LANGUAGE: u8 = 255;

const DRAW_DIRECT_SET_ENVIRONMENT: fn(&mut Cpu) = nfs4rc::f_800bf1fc;
const DRAW_INIT_VIEW_OT: fn(&mut Cpu) = nfs4rc::f_800be9c8;
const DRAW_ALLOC_VIEW: fn(&mut Cpu) = nfs4rc::f_800bebc0;
const DRAW_FREE_VIEW: fn(&mut Cpu) = nfs4rc::f_800bece8;
const DRAW_INIT_VIEWS: fn(&mut Cpu) = nfs4rc::f_800be9bc;
const DRAW_SET_VIEW: fn(&mut Cpu) = nfs4rc::f_800be8a8;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const BLOCKCLEAR: fn(&mut Cpu) = nfs4rc::f_800f24d4;
const STRCPY: fn(&mut Cpu) = nfs4rc::f_800e6714;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const RESET_DCT_BUFFER: fn(&mut Cpu) = nfs4rc::f_800dd0ac;
const TEXTURE_MENU_START: fn(&mut Cpu) = nfs4rc::f_800e121c;
const TEXTURE_MENU_END: fn(&mut Cpu) = nfs4rc::f_800e1244;
const TEXTURE_CLEAN_UP: fn(&mut Cpu) = nfs4rc::f_800e1460;
const CARIO_START_UP: fn(&mut Cpu) = nfs4rc::f_800bccf8;
const CARIO_CLEAN_UP: fn(&mut Cpu) = nfs4rc::f_800bcd64;
const CARIO_RESTART: fn(&mut Cpu) = nfs4rc::f_800bcd8c;
const R3DCAR_START_UP: fn(&mut Cpu) = nfs4rc::f_800ae9ac;
const R3DCAR_POST_START_UP: fn(&mut Cpu) = nfs4rc::f_800aea7c;
const R3DCAR_CLEAN_UP: fn(&mut Cpu) = nfs4rc::f_800aebd8;
const R3DCAR_INSTANTIATE: fn(&mut Cpu) = nfs4rc::f_800afbc4;
const R3DCAR_DEINSTANTIATE: fn(&mut Cpu) = nfs4rc::f_800af910;
const R3DCAR_DEINSTANTIATE_MENU: fn(&mut Cpu) = nfs4rc::f_800af94c;
const READ_E_MAP_DATA: fn(&mut Cpu) = nfs4rc::f_800bf67c;
const ASYNC_LOAD_FILE_AT: fn(&mut Cpu) = nfs4rc::f_800f2170;
const READ_STATUS: fn(&mut Cpu) = nfs4rc::f_800f240c;
const INITIAL_MEM_CARD_CHECK: fn(&mut Cpu) = crate::front::initial_mem_card_check_entry;
const DEL_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb9a4;
const ADD_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb944;
const AUDIO_DEINIT_DRIVER: fn(&mut Cpu) = nfs4rc::f_800bca4c;
const AUDIO_INIT_DRIVER: fn(&mut Cpu) = nfs4rc::f_800bc958;
const SET_DEF_DISP_ENV: fn(&mut Cpu) = nfs4rc::f_800f2c3c;
const INIT_GEOM: fn(&mut Cpu) = nfs4rc::f_800f2b00;
const INIT_MENU_CLUT: fn(&mut Cpu) = nfs4rc::f_800e111c;
const INIT_LINK_MODE: fn(&mut Cpu) = nfs4rc::f_800f1328;
const LOAD_WORDS: fn(&mut Cpu) = nfs4rc::f_800b9ef0;
const SECONDARY_MEM_CARD_CHECK: fn(&mut Cpu) = crate::front::secondary_mem_card_check_entry;
const SET_PLAY_LIST: fn(&mut Cpu) = crate::front::set_play_list_entry;
const LOAD_FE_SAMPLES: fn(&mut Cpu) = nfs4rc::f_800780f0;
const LOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f2970;
/// `DrawSync`, `systemtask` (the host's), `DoTitleScreen` (the host shows the title).
const DRAW_SYNC: u32 = 0x800E_E1D8;
const SYSTEM_TASK: u32 = 0x800E_77F0;
const DO_TITLE_SCREEN: u32 = 0x8004_E01C;

/// `Quick_DD` (PSXFRONT.CPP, 0x8004dc60): the whole screen (512 by 240) cleared to the colour
/// `r`, `g`, `b` through the direct drawing environment.
pub fn quick_dd(c: &mut Cpu, red: u32, green: u32, blue: u32) {
    call(c, DRAW_DIRECT_SET_ENVIRONMENT, &[0, 0, 512, 240, red, green, blue, 0, 0, 0]);
}

pub fn abi_quick_dd(c: &mut Cpu) {
    quick_dd(c, c.r[4], c.r[5], c.r[6]);
}

/// `PSXFront_AllocateDrawMemory` (PSXFRONT.CPP, 0x8004dca4): the view's ordering table, the two
/// primitive buffers and the player's view's.
pub fn allocate_draw_memory(c: &mut Cpu) {
    call(c, DRAW_INIT_VIEW_OT, &[]);
    let v = call(c, RESERVE, &[PS0, PRIM_BUFFER, 16]);
    c.m.w32(ENVIRO + 20, v);
    let v = call(c, RESERVE, &[PS1, PRIM_BUFFER, 16]);
    c.m.w32(ENVIRO + 44, v);
    let view = c.m.r32(PLAYER1_VIEW);
    call(c, DRAW_ALLOC_VIEW, &[view, PRIM_BUFFER]);
}

pub fn abi_allocate_draw_memory(c: &mut Cpu) {
    allocate_draw_memory(c);
}

/// `PSXFront_FreeDrawMemory` (PSXFRONT.CPP, 0x8004dd18): the views' memory freed.
pub fn free_draw_memory(c: &mut Cpu) {
    call(c, DRAW_FREE_VIEW, &[]);
}

pub fn abi_free_draw_memory(c: &mut Cpu) {
    free_draw_memory(c);
}

/// `InitializeSpinningCars` (PSXFRONT.CPP, 0x8004dd38): once, the showroom's 3D cars: the car
/// code started (as if loading the front end), the showroom, two car objects (each the setup's
/// next car record, car 1, its name, instantiated), the camera view cleared, the menu's
/// colours white, the lights off.
pub fn initialize_spinning_cars(c: &mut Cpu) {
    if c.m.r8(CARS_UP) != 0 {
        return;
    }
    c.m.w32(IN_MENU, 1);
    call(c, RESET_DCT_BUFFER, &[]);
    call(c, TEXTURE_MENU_START, &[]);
    c.m.w32(IN_FRONT_END, 1);
    call(c, CARIO_START_UP, &[]);
    call(c, R3DCAR_START_UP, &[]);
    c.m.w32(IN_FRONT_END, 0);
    call(c, READ_E_MAP_DATA, &[]);
    crate::fe3dmenu::init_showroom(c);
    c.m.w32(GAME_SETUP.0 + 0x48, 0);
    for k in 0..2u32 {
        let car = call(c, RESERVE, &[CAR_OBJ, CAR_OBJECT_SIZE, 0]);
        c.m.w32(CAR_OBJECTS + 4 * k, car);
        call(c, BLOCKCLEAR, &[car, CAR_OBJECT_SIZE]);
        let m = &mut c.m;
        let record = SETUP_CARS + SETUP_CAR_SIZE * k;
        m.w32(car + CAR_RECORD, record);
        m.w32(record, 1);
        let record = m.r32(car + CAR_RECORD);
        let id = m.r32(record);
        call(c, STRCPY, &[car + CAR_NAME, id.wrapping_mul(5).wrapping_add(CAR_NAMES)]);
        c.m.w32(car, k | 0x100);
        call(c, R3DCAR_INSTANTIATE, &[car, k]);
        c.m.w8(car + CAR_SHOWN, 1);
    }
    call(c, R3DCAR_POST_START_UP, &[]);
    c.m.w32(CAR_FLAGS + 4, 0);
    c.m.w32(CAR_FLAGS, 0);
    call(c, BLOCKCLEAR, &[C_VIEW, 140]);
    let m = &mut c.m;
    m.w32(MENU_COLOR + 4, u32::MAX);
    m.w32(MENU_COLOR, u32::MAX);
    m.w32(MENU_LIGHTS, 0);
    m.w32(MENU_LIGHTS_DIRECTION, 0);
    m.w8(CARS_UP, 1);
}

pub fn abi_initialize_spinning_cars(c: &mut Cpu) {
    initialize_spinning_cars(c);
}

/// `CleanupSpinningCars` (PSXFRONT.CPP, 0x8004deb4): when set up, the showroom's cars freed and
/// the car code cleaned up, the draw memory freed, the frame flip reset; the movie decoder's
/// table read back from the CD (waited for).
pub fn cleanup_spinning_cars(c: &mut Cpu) {
    if c.m.r8(CARS_UP) == 0 {
        return;
    }
    call_addr(c, DRAW_SYNC, &[0]);
    for k in 0..2 {
        let car = c.m.r32(CAR_OBJECTS + 4 * k);
        call(c, R3DCAR_DEINSTANTIATE, &[car]);
        let car = c.m.r32(CAR_OBJECTS + 4 * k);
        call(c, PURGE, &[car]);
    }
    c.m.w32(IN_FRONT_END, 1);
    call(c, R3DCAR_CLEAN_UP, &[]);
    call(c, CARIO_CLEAN_UP, &[]);
    c.m.w32(IN_FRONT_END, 0);
    call(c, TEXTURE_CLEAN_UP, &[]);
    free_draw_memory(c);
    let m = &mut c.m;
    m.w32(IN_MENU, 0);
    m.w32(G_FLIP, u32::MAX);
    m.w8(CARS_UP, 0);
    call(c, RESET_DCT_BUFFER, &[]);
    with_buffer(c, 80, |c, name| {
        let path = c.m.r32(CD_PATH);
        call(c, SPRINTF, &[name, DCT_FILE, path]);
        let handle = call(c, ASYNC_LOAD_FILE_AT, &[name, DVLC]);
        while call(c, READ_STATUS, &[handle]) == 0 {
            call_addr(c, SYSTEM_TASK, &[0]);
        }
    });
}

pub fn abi_cleanup_spinning_cars(c: &mut Cpu) {
    cleanup_spinning_cars(c);
}

/// `CleanupSpinningCarsMenu` (PSXFRONT.CPP, 0x8004dfac): when set up, the showroom's cars let
/// go of for a race (kept loaded), the car code restarted.
pub fn cleanup_spinning_cars_menu(c: &mut Cpu) {
    if c.m.r8(CARS_UP) == 0 {
        return;
    }
    call_addr(c, DRAW_SYNC, &[0]);
    for k in 0..2 {
        let car = c.m.r32(CAR_OBJECTS + 4 * k);
        call(c, R3DCAR_DEINSTANTIATE_MENU, &[car]);
    }
    call(c, TEXTURE_MENU_END, &[]);
    call(c, CARIO_RESTART, &[]);
}

pub fn abi_cleanup_spinning_cars_menu(c: &mut Cpu) {
    cleanup_spinning_cars_menu(c);
}

/// `DoLanguageScreen` (PSXFRONT.CPP, 0x8004e11c): no screen in this release:
/// `Front_InitialMemCardCheck` (FRONT.CPP: the records' defaults, the front end's structure,
/// language 0); none chosen (255) made the first.
pub fn do_language_screen(c: &mut Cpu) {
    call(c, INITIAL_MEM_CARD_CHECK, &[]);
    if c.m.r8(LANGUAGE) == NO_LANGUAGE {
        c.m.w8(LANGUAGE, 0);
    }
}

pub fn abi_do_language_screen(c: &mut Cpu) {
    do_language_screen(c);
}

/// `PSXExitFrontend` (PSXFRONT.CPP, 0x8004e158): the showroom's cars cleaned up, the timer
/// stopped, the sound driver shut, the help's shapes freed; not the first time any more.
pub fn psx_exit_frontend(c: &mut Cpu) {
    cleanup_spinning_cars(c);
    call(c, DEL_TIMER, &[TIMER]);
    call(c, AUDIO_DEINIT_DRIVER, &[]);
    let shapes = c.m.r32(HELP_SHAPES);
    if shapes != 0 {
        call(c, PURGE, &[shapes]);
        c.m.w32(HELP_SHAPES, 0);
    }
    c.m.w32(FIRST_TIME, 0);
}

pub fn abi_psx_exit_frontend(c: &mut Cpu) {
    psx_exit_frontend(c);
}

/// `PSX_AllocShapes` (PSXFRONT.CPP, 0x8004e1b0): the help's shapes' table, cleared.
pub fn psx_alloc_shapes(c: &mut Cpu) {
    let shapes = call(c, RESERVE, &[HELP_NAME, HELP_SIZE, 0]);
    c.m.w32(HELP_SHAPES, shapes);
    call(c, BLOCKCLEAR, &[shapes, HELP_SIZE]);
}

pub fn abi_psx_alloc_shapes(c: &mut Cpu) {
    psx_alloc_shapes(c);
}

/// `Init_RenderingEnvironment` (PSXFRONT.CPP, 0x8004e1f4): the two display environments (512
/// by 240, at 256 and 0), the views (no rear view; the player's whole screen, the camera's),
/// the draw memory, VSync on, text blending off, the frame flip 0.
pub fn init_rendering_environment(c: &mut Cpu) {
    call(c, SET_DEF_DISP_ENV, &[ENVIRO, 0, 256, 512, 240]);
    call(c, SET_DEF_DISP_ENV, &[ENVIRO + 24, 0, 0, 512, 240]);
    call(c, DRAW_INIT_VIEWS, &[]);
    c.m.w32(REAR_VIEW, u32::MAX);
    let view = call(c, DRAW_SET_VIEW, &[0, 0, 0, 256, 512, 240, 0, 1, 10]);
    c.m.w32(PLAYER1_VIEW, view);
    call(c, BLOCKCLEAR, &[C_VIEW, 140]);
    let view = c.m.r32(PLAYER1_VIEW);
    c.m.w32(C_VIEW, view);
    allocate_draw_memory(c);
    c.m.w32(DO_VSYNC, 1);
    crate::fetextrender::set_abr(c, 0, 0);
    c.m.w32(G_FLIP, 0);
}

pub fn abi_init_rendering_environment(c: &mut Cpu) {
    init_rendering_environment(c);
}

/// `Init_PSX_FrontEnd` (PSXFRONT.CPP, 0x8004e2e0): the timer, the geometry, the help's shapes,
/// the menus' CLUT, a 512-wide 16-bit screen; the first time, the intro movies (the second
/// unless the first was skipped), the environment, the defaults, the language's words, the
/// title (the host's), `Front_SecondaryMemCardCheck` (FRONT.CPP); else the environment and the
/// words; then the fonts, the sound driver, the music's list, the samples, the help's shapes
/// loaded.
pub fn init_psx_front_end(c: &mut Cpu) {
    c.m.w32(G_FLIP, u32::MAX);
    call(c, ADD_TIMER, &[TIMER]);
    call(c, INIT_GEOM, &[]);
    psx_alloc_shapes(c);
    call(c, INIT_MENU_CLUT, &[]);
    c.m.w32(SCREEN_WIDTH, 512);
    c.m.w32(SCREEN_BPP, 16);
    if c.m.r32(FIRST_TIME) != 0 {
        if crate::movie::play_movie(c, 4) != 1 {
            crate::movie::play_movie(c, 0);
        }
        call(c, INIT_LINK_MODE, &[0, 1, 1]);
        init_rendering_environment(c);
        call(c, INIT_LINK_MODE, &[0, 20, 0]);
        do_language_screen(c);
        let language = c.m.r8(LANGUAGE) as u32;
        call(c, LOAD_WORDS, &[language]);
        call_addr(c, DO_TITLE_SCREEN, &[]);
        call(c, INIT_LINK_MODE, &[0, 1, 1]);
        call(c, SECONDARY_MEM_CARD_CHECK, &[]);
    } else {
        init_rendering_environment(c);
        let language = c.m.r8(LANGUAGE) as u32;
        call(c, LOAD_WORDS, &[language]);
    }
    crate::fetools::init(c);
    call(c, AUDIO_INIT_DRIVER, &[0xD800, 0x1_8000]);
    let list = c.m.r32(PLAY_LIST);
    call(c, SET_PLAY_LIST, &[list]);
    call(c, LOAD_FE_SAMPLES, &[]);
    load_all_help_shapes(c);
}

pub fn abi_init_psx_front_end(c: &mut Cpu) {
    init_psx_front_end(c);
}

/// `LoadAllHelpShapes` (PSXFRONT.CPP, 0x8004f09c): the permanent shapes' file (`zperm.psh`)
/// read, its 59 shapes put in VRAM into the help's table, the file freed.
pub fn load_all_help_shapes(c: &mut Cpu) {
    let file = with_buffer(c, 64, |c, name| {
        let path = c.m.r32(CD_PATH);
        call(c, SPRINTF, &[name, ZPERM_FILE, path]);
        call(c, LOAD_SHAPE, &[name, 0])
    });
    if file == 0 {
        return;
    }
    for k in 0..HELP_COUNT {
        let shape = c.m.r32(HELP_SHAPES).wrapping_add(k << 5);
        crate::fetexture::load_pmx(c, file, k, TextureShapeInfo(shape));
    }
    call(c, PURGE, &[file]);
}

pub fn abi_load_all_help_shapes(c: &mut Cpu) {
    load_all_help_shapes(c);
}

/// `FontUpsideDownBlit` (PSXFRONT.CPP, 0x8004f49c): a font's character drawn upside down as a
/// `POLY_FT4` (40 bytes) in the font's tint and CLUT, at `x`, `y` from its texture's place
/// (`u`, `v` plus the font's row), measured from its baseline.
#[allow(clippy::too_many_arguments)]
pub fn font_upside_down_blit(m: &mut Mem, x: u32, y: u32, font: u32, u: u32, row: u32, ch: u32) {
    let prim = m.r32(NEXT_PRIM);
    let w = m.r8(ch.wrapping_add(2)) as u32;
    let h = m.r8(ch.wrapping_add(3)) as u32;
    let base = m.r8(ch.wrapping_add(10)) as i8 as i32 as u32;
    m.w32(NEXT_PRIM, prim.wrapping_add(40));
    let head = m.r32(prim) & 0xFF00_0000;
    let ot = m.r32(OT);
    let place = m.r32(font.wrapping_add(12));
    let top = y.wrapping_sub(base);
    let tall = h.wrapping_add(base);
    let v = ((((place << 4) as i32 >> 20) as u32).wrapping_add(row) & 0xFF).wrapping_sub(1);
    let entry = m.r32(ot) & 0x00FF_FFFF;
    m.w32(prim, head | entry);
    let entry = m.r32(ot) & 0xFF00_0000;
    let tint = m.r32(FONT_TINT);
    m.w32(ot, entry | (prim & 0x00FF_FFFF));
    m.w32(prim.wrapping_add(4), tint);
    m.w8(prim.wrapping_add(3), 9);
    m.w8(prim.wrapping_add(7), 44);
    let bottom = top.wrapping_add(5).wrapping_sub(tall);
    let clut = m.r16(FONT_CLUT);
    m.w16(prim.wrapping_add(14), clut);
    let mode = m.r8(font) as u32;
    let place = m.r32(font.wrapping_add(12));
    let u1 = u.wrapping_add(w);
    m.w8(prim.wrapping_add(13), v as u8);
    m.w8(prim.wrapping_add(21), v as u8);
    let v1 = v.wrapping_add(h);
    let top = bottom.wrapping_add(h);
    let x1 = x.wrapping_add(w);
    m.w8(prim.wrapping_add(12), u as u8);
    m.w8(prim.wrapping_add(20), u1 as u8);
    m.w8(prim.wrapping_add(28), u as u8);
    m.w8(prim.wrapping_add(29), v1 as u8);
    m.w8(prim.wrapping_add(36), u1 as u8);
    m.w8(prim.wrapping_add(37), v1 as u8);
    m.w16(prim.wrapping_add(8), x as u16);
    let page = ((mode & 3) << 7) | ((place >> 20) & 0x10) | (((place & 0x3FF) as i32 >> 6) as u32);
    m.w16(prim.wrapping_add(22), page as u16);
    m.w16(prim.wrapping_add(10), top as u16);
    m.w16(prim.wrapping_add(16), x1 as u16);
    m.w16(prim.wrapping_add(18), top as u16);
    m.w16(prim.wrapping_add(24), x as u16);
    m.w16(prim.wrapping_add(26), bottom as u16);
    m.w16(prim.wrapping_add(32), x1 as u16);
    m.w16(prim.wrapping_add(34), bottom as u16);
}

pub fn abi_font_upside_down_blit(c: &mut Cpu) {
    let (row, ch) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    font_upside_down_blit(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], row, ch);
}

/// For callers not rewritten yet: the functions with the C convention's arguments.
pub(crate) fn cleanup_spinning_cars_menu_entry(c: &mut Cpu) {
    cleanup_spinning_cars_menu(c);
}

pub(crate) fn draw_shape_extended_entry(c: &mut Cpu) {
    abi_draw_shape_extended(c);
}

pub(crate) fn scale_shape_extended_entry(c: &mut Cpu) {
    abi_scale_shape_extended(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// Two tables of 32 shapes (the help's, their sizes, places, depths and texture places
    /// shuffled), options, colours, outputs.
    const SHAPES_AT: u32 = 0x8014_5000;
    const OWN: u32 = 0x8014_5400;
    const OPTIONS: u32 = 0x8014_5800;
    const COLOURS: u32 = 0x8014_5840;
    const OUT: u32 = 0x8014_5880;
    const PERMANENT_AT: u32 = 0x8014_6000;

    /// FEMENU's structures, then: the shapes (depths 4, 8 or 16; sizes up to 300 wide, about a
    /// texture page; origins any; texture places any), the help's and the permanent tables
    /// ours; the options (a line, four colours, the own table); colours; a place.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let help = m.r32(HELP_SHAPES);
        for t in [SHAPES_AT, OWN, PERMANENT_AT] {
            for k in 0..32 * SHAPE_SIZE {
                let b = m.r8(help + k % (8 * SHAPE_SIZE));
                m.w8(t + k, b);
            }
            for k in 0..32 {
                let s = t + SHAPE_SIZE * k;
                m.w8(s + DEPTH, [4, 8, 16][pick(rng, 3) as usize]);
                m.w8(s + BLEND, pick(rng, 4) as u8);
                m.w16(s + WIDTH, [1, 64, 127, 128, 255, 256, 300, pick(rng, 400), 0x8001][pick(rng, 9) as usize] as u16);
                m.w16(s + HEIGHT, [0, 1, 40, pick(rng, 300)][pick(rng, 4) as usize] as u16);
                m.w16(s + ORIGIN_X, pick(rng, 0x1_0000) as u16);
                m.w16(s + ORIGIN_Y, pick(rng, 0x1_0000) as u16);
                m.w16(s + TEX_X, pick(rng, 0x1_0000) as u16);
                m.w16(s + TEX_Y, pick(rng, 0x1_0000) as u16);
                m.w32(s + CLUT, rng.next() as u32);
            }
        }
        m.w32(HELP_SHAPES, SHAPES_AT);
        m.w32(PERMANENT_SHAPES, PERMANENT_AT);
        m.w16(OPTIONS + LINE, pick(rng, 300) as u16);
        for k in 0..4 {
            m.w32(OPTIONS + COLOUR + 4 * k, rng.next() as u32 & 0xFF_FFFF);
            m.w32(COLOURS + 4 * k, rng.next() as u32);
        }
        m.w32(OPTIONS + TABLE, OWN);
        m.w32(OUT, [0, 1, pick(rng, 640), (-(pick(rng, 300) as i32)) as u32][pick(rng, 4) as usize]);
        m.w32(OUT + 4, [0, 1, pick(rng, 480), (-(pick(rng, 300) as i32)) as u32][pick(rng, 4) as usize]);
        m.w32(OUT + 8, rng.next() as u32 & 0x7FF);
    }

    fn flags(r: &mut Rng) -> u32 {
        r.next() as u32 & 0x7FF
    }

    fn place(r: &mut Rng) -> u32 {
        [0, pick(r, 640), (-(pick(r, 300) as i32)) as u32][pick(r, 3) as usize]
    }

    fn fade(r: &mut Rng) -> u32 {
        [0, 64, 128, pick(r, 129), pick(r, 300)][pick(r, 5) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("AdjustShapeDrawing", 0x8004_e420, nfs4rc::f_8004e420, |_, r| vec![SHAPES_AT + SHAPE_SIZE * pick(r, 32), OUT, OUT + 4, OUT + 8, sx(128u32.wrapping_sub(fade(r))), COLOURS, OPTIONS]),
            ("DrawFlatShape", 0x8004_eb20, nfs4rc::f_8004eb20, |_, r| vec![SHAPES_AT + SHAPE_SIZE * pick(r, 32), flags(r), place(r), place(r), COLOURS, pick(r, 4)]),
            ("DrawShapeExtended", 0x8004_eb60, nfs4rc::f_8004eb60, |_, r| vec![pick(r, 32), flags(r), place(r), place(r), fade(r), pick(r, 4), OPTIONS]),
            ("ScaleGouraudShape", 0x8004_ec64, nfs4rc::f_8004ec64, |_, r| {
                vec![SHAPES_AT + SHAPE_SIZE * pick(r, 32), flags(r), place(r), place(r), [0x1_0000, 0x2_0000, pick(r, 0x4_0000)][pick(r, 3) as usize], [0x1_0000, pick(r, 0x4_0000)][pick(r, 2) as usize], COLOURS, pick(r, 4)]
            }),
            ("ScaleFlatShape", 0x8004_ef20, nfs4rc::f_8004ef20, |_, r| {
                vec![SHAPES_AT + SHAPE_SIZE * pick(r, 32), flags(r), place(r), place(r), [0x1_0000, 0x2_0000, pick(r, 0x4_0000)][pick(r, 3) as usize], [0x1_0000, pick(r, 0x4_0000)][pick(r, 2) as usize], COLOURS, pick(r, 4)]
            }),
            ("ScaleShapeExtended", 0x8004_ef70, nfs4rc::f_8004ef70, |_, r| vec![pick(r, 32), flags(r), place(r), place(r), fade(r), pick(r, 4), OPTIONS]),
        ]
    }

    #[test]
    fn psxfront_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x850 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x851 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    /// The rest of the file, on FEMENU's structures with the shapes' shuffle: a font and a
    /// character of any bytes, the font's tint and CLUT; the showroom's cars set up or not (the
    /// RAM image's two at the car select); the help's table freed now and then.
    fn glue_shuffle(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let m = &mut c.m;
        for k in 0..16 {
            m.w8(FONT + k, rng.next() as u8);
            m.w8(CHAR + k, rng.next() as u8);
        }
        m.w32(FONT_TINT, rng.next() as u32);
        m.w16(FONT_CLUT, rng.next() as u16);
        m.w8(CARS_UP, pick(rng, 2) as u8);
        m.w8(LANGUAGE, [0, 1, 255, pick(rng, 256)][pick(rng, 4) as usize] as u8);
    }

    const FONT: u32 = 0x8014_5900;
    const CHAR: u32 = 0x8014_5920;

    /// (`Quick_DD`'s direct drawing and `DoLanguageScreen`'s loading icon wait on the hardware:
    /// the tours run them.)
    fn glue_cases() -> Vec<Case> {
        vec![
            ("FontUpsideDownBlit", 0x8004_f49c, nfs4rc::f_8004f49c, |_, r| vec![place(r), place(r), FONT, pick(r, 256), [0, 1, pick(r, 256), rng_word(r)][pick(r, 4) as usize], CHAR, rng_word(r)]),
            ("FeDraw_SetABRMode", 0x8004_dbc0, nfs4rc::f_8004dbc0, |_, r| vec![[0, 1, 2, 3, pick(r, 8)][pick(r, 5) as usize]]),
            ("PSX_AllocShapes", 0x8004_e1b0, nfs4rc::f_8004e1b0, |_, _| vec![]),
            ("PSXFront_AllocateDrawMemory", 0x8004_dca4, nfs4rc::f_8004dca4, |_, _| vec![]),
            ("PSXFront_FreeDrawMemory", 0x8004_dd18, nfs4rc::f_8004dd18, |_, _| vec![]),
            ("Init_RenderingEnvironment", 0x8004_e1f4, nfs4rc::f_8004e1f4, |_, _| vec![]),
            ("CleanupSpinningCarsMenu", 0x8004_dfac, nfs4rc::f_8004dfac, |_, _| vec![]),
        ]
    }

    fn rng_word(r: &mut Rng) -> u32 {
        r.next() as u32
    }

    #[test]
    fn psxfront_glue_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in glue_cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x870 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x871 + k as u64, glue_shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
