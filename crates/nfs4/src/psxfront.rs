//! PSXFRONT.CPP's shape drawing: a front-end shape (`tTexture_ShapeInfo`, 32 bytes: +4 its
//! CLUT, +8 its depth (4, 8 or 16 bits), +9 its blending, +16 and +18 its size, +20 and +22
//! its origin, +24 and +26 its texture's place in VRAM) drawn as `POLY_GT4`s: by its origin,
//! centred, mirrored about a line, coloured (one colour, four corners, a gradient by height)
//! and faded; whole when scaled, else in strips under 256 texels wide (a texture page's): those
//! `DrawGouraudShape` emits, which the host does.
//!
//! The extended drawing's options (`tDrawShapeExtended`): +0 the mirroring line, +4 its colour
//! (or the corners' four), +20 a shape table of its own.

use crate::abi::{self, Saved};
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

/// `AdjustShapeDrawing` (PSXFRONT.CPP, 0x8004e420): `*x`, `*y` and `*flags` adjusted for the
/// shape: by its top-left corner, else by its origin (at or above the screen's top, `*y` measured
/// up from it); centred; mirrored about the options' line (flipped). The colours: one (faded),
/// four corners (faded), a gradient by distance from the line, or grey by the fade.
pub fn abi_adjust_shape_drawing(c: &mut Cpu) {
    let (shape, x, y, flags) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (fade, colours, options) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    let m = &mut c.m;
    if m.r32(flags) & TOP_LEFT != 0 {
        let v = m.r32(x).wrapping_sub(lh(m, shape + ORIGIN_X));
        m.w32(x, v);
        let v = m.r32(y).wrapping_sub(lh(m, shape + ORIGIN_Y));
        m.w32(y, v);
    } else if m.r32(y) as i32 <= 0 {
        let v = m.r32(x).wrapping_sub(lh(m, shape + ORIGIN_X));
        m.w32(x, v);
        let v = 0u32.wrapping_sub(lh(m, shape + ORIGIN_Y).wrapping_add(m.r32(y)));
        m.w32(y, v);
    }
    if m.r32(flags) & CENTRED != 0 {
        let v = m.r32(x).wrapping_sub((lh(m, shape + WIDTH) as i32 >> 1) as u32);
        m.w32(x, v);
        let v = m.r32(y).wrapping_sub((lh(m, shape + HEIGHT) as i32 >> 1) as u32);
        m.w32(y, v);
    }
    if m.r32(flags) & REFLECT != 0 {
        let line = lh(m, options + LINE);
        let v = (line << 1).wrapping_sub(m.r32(y).wrapping_add(lh(m, shape + HEIGHT))).wrapping_add(1);
        m.w32(y, v);
        let f = m.r32(flags) | FLIP;
        m.w32(flags, f);
    }
    let f = m.r32(flags);
    if f & COLOURED != 0 {
        let v = fade_colour(m, options + COLOUR, fade);
        m.w32(colours, v);
    } else if f & CORNERS != 0 {
        for k in 0..4 {
            let v = fade_colour(m, options + COLOUR + 4 * k, fade);
            m.w32(colours + 4 * k, v);
        }
    } else if f & GRADIENT != 0 {
        let line = m.r16(options + LINE) as u32;
        let top = m.r16(y) as u32;
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
    ((m.r8(shape + BLEND) as u32 & 3) << 7)
        | ((abr & 3) << 5)
        | ((ty & 0x100) >> 4)
        | ((tex_x & 0x3C0) >> 6)
        | ((ty & 0x200) << 2)
}

/// The CLUT packed in a shape (its x by 16 in the low 6 bits, its y above).
fn clut_args(m: &mut Mem, shape: u32) -> [u32; 2] {
    let v = m.r32(shape + CLUT);
    [(v & 0x3F) << 4, (v as i32 >> 6) as u32]
}

/// `DrawFlatShape` (PSXFRONT.CPP, 0x8004eb20): `DrawGouraudShape` (the host's) with one colour.
pub fn abi_draw_flat_shape(c: &mut Cpu) {
    let args = [c.r[4], c.r[5], c.r[6], c.r[7], abi::arg(c, 4), abi::arg(c, 5)];
    abi::frame(c, 32, |c, _| {
        let colours = args[4];
        let v = c.m.r32(colours);
        for k in [12, 8, 4] {
            c.m.w32(colours + k, v);
        }
        Saved::of(c).call_ptr_leaving(c, DRAW_GOURAUD_SHAPE, &args, 0x8004_eb50);
    })
}

fn draw_flat_shape_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_eb20, None, &[]);
    abi_draw_flat_shape(c);
}

fn scale_gouraud_shape_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_ec64, None, &[]);
    abi_scale_gouraud_shape(c);
}

fn scale_flat_shape_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_ef20, None, &[]);
    abi_scale_flat_shape(c);
}

fn adjust_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_e420, None, &[]);
    abi_adjust_shape_drawing(c);
}

/// Shape `k`'s entry: of the help's table, the options' own, or the permanent shapes'.
fn shape_of(m: &mut Mem, k: u32, flags: u32, options: u32) -> u32 {
    let table = if flags & HELP != 0 { m.r32(HELP_SHAPES) } else { m.r32(PERMANENT_SHAPES) };
    let at = k.wrapping_mul(SHAPE_SIZE);
    if flags & OWN_TABLE != 0 { m.r32(options + TABLE).wrapping_add(at) } else { table.wrapping_add(at) }
}

/// `DrawShapeExtended` (PSXFRONT.CPP, 0x8004eb60): shape `k` at `x`, `y`, adjusted and faded
/// (`fade`: 0 full, 128 gone), with four colours (gradient or corners) or one; blending `abr`.
pub fn abi_draw_shape_extended(c: &mut Cpu) {
    let (k, flags, x, y) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (fade, abr, options) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = abr;
        let m = &mut c.m;
        m.w32(sp + 68, flags);
        m.w32(sp + 72, x);
        m.w32(sp + 76, y);
        let shape = shape_of(m, k, flags, options);
        r.0[0] = shape;
        let colours = sp + 32;
        r.0[1] = colours;
        let fade = sx(128u32.wrapping_sub(fade));
        r.call_leaving(c, adjust_entry, &[shape, sp + 72, sp + 76, sp + 68, fade, colours, options], 0x8004_ec08);
        let m = &mut c.m;
        let (flags, x, y) = (m.r32(sp + 68), m.r32(sp + 72), m.r32(sp + 76));
        let args = [shape, flags, x, y, colours, abr];
        if flags & (GRADIENT | CORNERS) != 0 {
            r.call_ptr_leaving(c, DRAW_GOURAUD_SHAPE, &args, 0x8004_ec30);
        } else {
            r.call_leaving(c, draw_flat_shape_entry, &args, 0x8004_ec4c);
        }
    })
}

/// `ScaleGouraudShape` (PSXFRONT.CPP, 0x8004ec64): the whole shape at `x`, `y` scaled by
/// `scale_x` and `scale_y` (16.16), cornered colours, semi-transparent, flipped or mirrored.
pub fn abi_scale_gouraud_shape(c: &mut Cpu) {
    let (shape, flags, x, y) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (scale_x, scale_y, colours, abr) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6), abi::arg(c, 7));
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = shape;
        r.0[7] = flags;
        let (mut x, mut y) = (x, y);
        r.0[6] = x;
        r.0[5] = y;
        let m = &mut c.m;
        let mut w = m.r16(shape + WIDTH) as u32;
        let mut h = m.r16(shape + HEIGHT) as u32;
        r.0[2] = w;
        r.0[1] = h;
        let prim = add_poly_gt4(m);
        r.0[3] = prim;
        let depth = m.r8(shape + DEPTH) as u16;
        m.w16(sp + 16, depth);
        r.0[0] = abr;
        set_colours(m, prim, colours);
        r.call_leaving(c, SET_POLY_GT4, &[prim], 0x8004_ed40);
        r.call_leaving(c, SET_SEMI_TRANS, &[prim, flags & SEMI], 0x8004_ed4c);
        let args = clut_args(&mut c.m, shape);
        let clut = r.call_leaving(c, GET_CLUT, &args, 0x8004_ed64);
        r.0[0] = (abr & 3) << 5;
        let mirror = flags & MIRROR;
        r.0[8] = mirror;
        let m = &mut c.m;
        m.w16(prim + 14, clut as u16);
        let tex_x = m.r16(shape + TEX_X) as u32;
        let page = tpage(m, shape, abr, tex_x);
        m.w16(prim + 26, page as u16);
        if mirror != 0 {
            x = x.wrapping_add(r.call_leaving(c, FIXEDMULT, &[scale_x, sx(w)], 0x8004_edcc));
            r.0[6] = x;
            w = 0u32.wrapping_sub(w);
            r.0[2] = w;
        }
        let flip = flags & FLIP;
        r.0[7] = flip;
        if flip != 0 {
            y = y.wrapping_add(r.call_leaving(c, FIXEDMULT, &[scale_y, sx(h)], 0x8004_edf0));
            r.0[5] = y;
            h = 0u32.wrapping_sub(h);
            r.0[1] = h;
        }
        let (w, h) = (sx(w), sx(h));
        r.0[2] = w;
        c.m.w16(prim + 8, x as u16);
        c.m.w16(prim + 10, y as u16);
        let v = r.call_leaving(c, FIXEDMULT, &[scale_x, w], 0x8004_ee14);
        r.0[1] = h;
        let x1 = x.wrapping_sub(1);
        r.0[0] = x1;
        c.m.w16(prim + 20, x1.wrapping_add(v) as u16);
        c.m.w16(prim + 22, y as u16);
        c.m.w16(prim + 32, x as u16);
        let v = r.call_leaving(c, FIXEDMULT, &[scale_y, h], 0x8004_ee3c);
        c.m.w16(prim + 34, y.wrapping_add(v) as u16);
        let v = r.call_leaving(c, FIXEDMULT, &[scale_x, w], 0x8004_ee50);
        let x3 = x1.wrapping_add(v);
        r.0[0] = x3;
        c.m.w16(prim + 44, x3 as u16);
        let v = r.call_leaving(c, FIXEDMULT, &[scale_y, h], 0x8004_ee64);
        let m = &mut c.m;
        m.w16(prim + 46, y.wrapping_add(v) as u16);
        let depth = m.r16(sp + 16) as i32;
        let u = crate::fixed::div(((m.r16(shape + TEX_X) as u32 & 0x3F) << 4) as i32, depth) as u32;
        let (wb, hb, mut v0) = (m.r8(shape + WIDTH) as u32, m.r8(shape + HEIGHT) as u32, m.r8(shape + TEX_Y) as u32);
        let u0 = if mirror != 0 { u.wrapping_sub(1) } else { u };
        let u1 = u0.wrapping_add(wb);
        if flip != 0 {
            v0 = v0.wrapping_sub(1);
        }
        let v1 = v0.wrapping_add(hb);
        for (at, v) in [(13, v0), (25, v0), (12, u0), (24, u1), (36, u0), (37, v1), (48, u1), (49, v1)] {
            m.w8(prim + at, v as u8);
        }
    })
}

/// `ScaleFlatShape` (PSXFRONT.CPP, 0x8004ef20): `ScaleGouraudShape` with one colour.
pub fn abi_scale_flat_shape(c: &mut Cpu) {
    let args = [c.r[4], c.r[5], c.r[6], c.r[7], abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6), abi::arg(c, 7)];
    abi::frame(c, 40, |c, _| {
        let colours = args[6];
        let v = c.m.r32(colours);
        for k in [12, 8, 4] {
            c.m.w32(colours + k, v);
        }
        Saved::of(c).call_leaving(c, scale_gouraud_shape_entry, &args, 0x8004_ef60);
    })
}

/// `ScaleShapeExtended` (PSXFRONT.CPP, 0x8004ef70): shape `k` at `x`, `y`, adjusted and faded,
/// drawn whole at twice its width.
pub fn abi_scale_shape_extended(c: &mut Cpu) {
    let (k, flags, x, y) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (fade, abr, options) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = abr;
        r.0[2] = 0x2_0000;
        r.0[3] = 0x1_0000;
        let m = &mut c.m;
        m.w32(sp + 76, flags);
        m.w32(sp + 80, x);
        m.w32(sp + 84, y);
        let shape = shape_of(m, k, flags, options);
        r.0[0] = shape;
        let colours = sp + 32;
        r.0[1] = colours;
        let fade = sx(128u32.wrapping_sub(fade));
        r.call_leaving(c, adjust_entry, &[shape, sp + 80, sp + 84, sp + 76, fade, colours, options], 0x8004_f028);
        let m = &mut c.m;
        let (flags, x, y) = (m.r32(sp + 76), m.r32(sp + 80), m.r32(sp + 84));
        let args = [shape, flags, x, y, 0x2_0000, 0x1_0000, colours, abr];
        if flags & (GRADIENT | CORNERS) != 0 {
            r.call_leaving(c, scale_gouraud_shape_entry, &args, 0x8004_f058);
        } else {
            r.call_leaving(c, scale_flat_shape_entry, &args, 0x8004_f07c);
        }
    })
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
const INIT_SHOWROOM: fn(&mut Cpu) = nfs4rc::f_800128e8;
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
const MOVIE_PLAY: fn(&mut Cpu) = crate::movie::play_movie_entry;
const INIT_LINK_MODE: fn(&mut Cpu) = nfs4rc::f_800f1328;
const LOAD_WORDS: fn(&mut Cpu) = nfs4rc::f_800b9ef0;
const SECONDARY_MEM_CARD_CHECK: fn(&mut Cpu) = crate::front::secondary_mem_card_check_entry;
const SET_PLAY_LIST: fn(&mut Cpu) = crate::front::set_play_list_entry;
const LOAD_FE_SAMPLES: fn(&mut Cpu) = nfs4rc::f_800780f0;
const LOAD_SHAPE: fn(&mut Cpu) = nfs4rc::f_800f2970;
const LOAD_PMX: fn(&mut Cpu) = crate::fetexture::load_pmx_entry;
/// `DrawSync`, `systemtask` (the host's), `DoTitleScreen` (the host shows the title).
const DRAW_SYNC: u32 = 0x800E_E1D8;
const SYSTEM_TASK: u32 = 0x800E_77F0;
const DO_TITLE_SCREEN: u32 = 0x8004_E01C;

/// `Quick_DD` (PSXFRONT.CPP, 0x8004dc60): the whole screen (512 by 240) cleared to the colour
/// `r`, `g`, `b` through the direct drawing environment.
pub fn abi_quick_dd(c: &mut Cpu) {
    let (red, green, blue) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 48, |c, _| {
        Saved::of(c).call_leaving(c, DRAW_DIRECT_SET_ENVIRONMENT, &[0, 0, 512, 240, red, green, blue, 0, 0, 0], 0x8004_dc94);
    })
}

/// `PSXFront_AllocateDrawMemory` (PSXFRONT.CPP, 0x8004dca4): the view's ordering table, the two
/// primitive buffers and the player's view's.
pub fn abi_allocate_draw_memory(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.call_leaving(c, DRAW_INIT_VIEW_OT, &[], 0x8004_dcb4);
        let v = r.call_leaving(c, RESERVE, &[PS0, PRIM_BUFFER, 16], 0x8004_dccc);
        r.0[0] = ENVIRO;
        c.m.w32(ENVIRO + 20, v);
        let v = r.call_leaving(c, RESERVE, &[PS1, PRIM_BUFFER, 16], 0x8004_dcf0);
        c.m.w32(ENVIRO + 44, v);
        let view = c.m.r32(PLAYER1_VIEW);
        r.call_leaving(c, DRAW_ALLOC_VIEW, &[view, PRIM_BUFFER], 0x8004_dd08);
    })
}

/// `PSXFront_FreeDrawMemory` (PSXFRONT.CPP, 0x8004dd18): the views' memory freed.
pub fn abi_free_draw_memory(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).call_leaving(c, DRAW_FREE_VIEW, &[], 0x8004_dd28);
    })
}

/// `InitializeSpinningCars` (PSXFRONT.CPP, 0x8004dd38): once, the showroom's 3D cars: the car
/// code started (as if loading the front end), the showroom, two car objects (each the setup's
/// next car record, car 1, its name, instantiated), the camera view cleared, the menu's
/// colours white, the lights off.
pub fn abi_initialize_spinning_cars(c: &mut Cpu) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let up = c.m.r8(CARS_UP) as u32;
        if up != 0 {
            c.r[2] = up;
            return;
        }
        r.0[1] = 1;
        c.m.w32(IN_MENU, 1);
        r.call_leaving(c, RESET_DCT_BUFFER, &[], 0x8004_dd74);
        r.0[0] = 0x8014_0000;
        r.call_leaving(c, TEXTURE_MENU_START, &[], 0x8004_dd7c);
        c.m.w32(IN_FRONT_END, 1);
        r.call_leaving(c, CARIO_START_UP, &[], 0x8004_dd84);
        r.0[1] = 0;
        r.call_leaving(c, R3DCAR_START_UP, &[], 0x8004_dd8c);
        c.m.w32(IN_FRONT_END, 0);
        r.call_leaving(c, READ_E_MAP_DATA, &[], 0x8004_dd94);
        r.0[4] = 1;
        r.call_leaving(c, INIT_SHOWROOM, &[], 0x8004_dd9c);
        r.0[5] = CAR_NAMES;
        r.0[2] = SETUP_CARS;
        r.0[3] = CAR_OBJECTS;
        c.m.w32(GAME_SETUP.0 + 0x48, 0);
        for k in 0..2u32 {
            let car = r.call_leaving(c, RESERVE, &[CAR_OBJ, 2268, 0], 0x8004_ddd8);
            r.0[0] = car;
            c.m.w32(r.0[3], car);
            r.call_leaving(c, BLOCKCLEAR, &[car, 2268], 0x8004_ddec);
            let m = &mut c.m;
            m.w32(car + 648, r.0[2]);
            m.w32(r.0[2], 1);
            let record = m.r32(car + 648);
            let id = m.r32(record);
            r.0[2] = r.0[2].wrapping_add(180);
            r.0[3] = r.0[3].wrapping_add(4);
            let name = id.wrapping_mul(5).wrapping_add(CAR_NAMES);
            r.call_leaving(c, STRCPY, &[car + 576, name], 0x8004_de18);
            c.m.w32(car, k | 0x100);
            r.call_leaving(c, R3DCAR_INSTANTIATE, &[car, k], 0x8004_de2c);
            r.0[1] = k + 1;
            c.m.w8(car + 145, 1);
        }
        r.call_leaving(c, R3DCAR_POST_START_UP, &[], 0x8004_de40);
        c.m.w32(CAR_FLAGS + 4, 0);
        c.m.w32(CAR_FLAGS, 0);
        r.call_leaving(c, BLOCKCLEAR, &[C_VIEW, 140], 0x8004_de60);
        let m = &mut c.m;
        m.w32(MENU_COLOR + 4, u32::MAX);
        m.w32(MENU_COLOR, u32::MAX);
        m.w32(MENU_LIGHTS, 0);
        m.w32(MENU_LIGHTS_DIRECTION, 0);
        m.w8(CARS_UP, 1);
        c.r[2] = 1;
    })
}

/// `CleanupSpinningCars` (PSXFRONT.CPP, 0x8004deb4): when set up, the showroom's cars freed and
/// the car code cleaned up, the draw memory freed, the frame flip reset; the movie decoder's
/// table read back from the CD (waited for).
pub fn abi_cleanup_spinning_cars(c: &mut Cpu) {
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        if c.m.r8(CARS_UP) == 0 {
            return;
        }
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8004_ded8);
        for k in 0..2 {
            let at = CAR_OBJECTS + 4 * k;
            r.0[0] = at;
            r.0[1] = k + 1;
            let car = c.m.r32(at);
            r.call_leaving(c, R3DCAR_DEINSTANTIATE, &[car], 0x8004_def0);
            r.0[0] = at + 4;
            let car = c.m.r32(at);
            r.call_leaving(c, PURGE, &[car], 0x8004_defc);
        }
        r.0[0] = 0x8014_0000;
        c.m.w32(IN_FRONT_END, 1);
        r.call_leaving(c, R3DCAR_CLEAN_UP, &[], 0x8004_df14);
        r.call_leaving(c, CARIO_CLEAN_UP, &[], 0x8004_df1c);
        c.m.w32(IN_FRONT_END, 0);
        r.call_leaving(c, TEXTURE_CLEAN_UP, &[], 0x8004_df24);
        r.enter_leaving(c, 0x8004_dd18, 0x8004_df2c, abi_free_draw_memory);
        let m = &mut c.m;
        m.w32(IN_MENU, 0);
        m.w32(G_FLIP, u32::MAX);
        m.w8(CARS_UP, 0);
        r.call_leaving(c, RESET_DCT_BUFFER, &[], 0x8004_df4c);
        let path = c.m.r32(CD_PATH);
        r.call_leaving(c, SPRINTF, &[sp + 16, DCT_FILE, path], 0x8004_df64);
        let handle = r.call_leaving(c, ASYNC_LOAD_FILE_AT, &[sp + 16, DVLC], 0x8004_df74);
        r.0[0] = handle;
        while r.call_leaving(c, READ_STATUS, &[handle], 0x8004_df80) == 0 {
            r.call_ptr_leaving(c, SYSTEM_TASK, &[0], 0x8004_df90);
        }
    })
}

/// `CleanupSpinningCarsMenu` (PSXFRONT.CPP, 0x8004dfac): when set up, the showroom's cars let
/// go of for a race (kept loaded), the car code restarted.
pub fn abi_cleanup_spinning_cars_menu(c: &mut Cpu) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        if c.m.r8(CARS_UP) == 0 {
            return;
        }
        r.call_ptr_leaving(c, DRAW_SYNC, &[0], 0x8004_dfd0);
        for k in 0..2 {
            let at = CAR_OBJECTS + 4 * k;
            let car = c.m.r32(at);
            r.0[0] = at + 4;
            r.0[1] = k + 1;
            r.call_leaving(c, R3DCAR_DEINSTANTIATE_MENU, &[car], 0x8004_dfec);
        }
        r.call_leaving(c, TEXTURE_MENU_END, &[], 0x8004_e000);
        r.call_leaving(c, CARIO_RESTART, &[], 0x8004_e008);
    })
}

/// `DoLanguageScreen` (PSXFRONT.CPP, 0x8004e11c): no screen in this release:
/// `Front_InitialMemCardCheck` (FRONT.CPP: the records' defaults, the front end's structure,
/// language 0); none chosen (255) made the first.
pub fn abi_do_language_screen(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).call_leaving(c, INITIAL_MEM_CARD_CHECK, &[], 0x8004_e12c);
        let language = c.m.r8(LANGUAGE) as u32;
        if language == 255 {
            c.m.w8(LANGUAGE, 0);
        }
        (c.r[2], c.r[3], c.r[4]) = (255, language, FRONT_END);
    })
}

/// `PSXExitFrontend` (PSXFRONT.CPP, 0x8004e158): the showroom's cars cleaned up, the timer
/// stopped, the sound driver shut, the help's shapes freed; not the first time any more.
pub fn abi_psx_exit_frontend(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.enter_leaving(c, 0x8004_deb4, 0x8004_e168, abi_cleanup_spinning_cars);
        r.call_leaving(c, DEL_TIMER, &[TIMER], 0x8004_e174);
        r.0[0] = 0x8005_0000;
        r.call_leaving(c, AUDIO_DEINIT_DRIVER, &[], 0x8004_e17c);
        let shapes = c.m.r32(HELP_SHAPES);
        if shapes != 0 {
            r.call_leaving(c, PURGE, &[shapes], 0x8004_e194);
            c.m.w32(HELP_SHAPES, 0);
        }
        c.m.w32(FIRST_TIME, 0);
        c.r[2] = 0x8005_0000;
    })
}

/// `PSX_AllocShapes` (PSXFRONT.CPP, 0x8004e1b0): the help's shapes' table, cleared.
pub fn abi_psx_alloc_shapes(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8005_0000;
        let shapes = r.call_leaving(c, RESERVE, &[HELP_NAME, HELP_SIZE, 0], 0x8004_e1d4);
        c.m.w32(HELP_SHAPES, shapes);
        r.call_leaving(c, BLOCKCLEAR, &[shapes, HELP_SIZE], 0x8004_e1e4);
    })
}

/// `Init_RenderingEnvironment` (PSXFRONT.CPP, 0x8004e1f4): the two display environments (512
/// by 240, at 256 and 0), the views (no rear view; the player's whole screen, the camera's),
/// the draw memory, VSync on, text blending off, the frame flip 0.
pub fn abi_init_rendering_environment(c: &mut Cpu) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = ENVIRO;
        r.0[1] = 240;
        r.call_leaving(c, SET_DEF_DISP_ENV, &[ENVIRO, 0, 256, 512, 240], 0x8004_e22c);
        r.call_leaving(c, SET_DEF_DISP_ENV, &[ENVIRO + 24, 0, 0, 512, 240], 0x8004_e244);
        r.0[2] = 1;
        r.call_leaving(c, DRAW_INIT_VIEWS, &[], 0x8004_e24c);
        c.m.w32(REAR_VIEW, u32::MAX);
        let view = r.call_leaving(c, DRAW_SET_VIEW, &[0, 0, 0, 256, 512, 240, 0, 1, 10], 0x8004_e288);
        r.0[1] = 0x8012_0000;
        r.0[0] = 0x8014_0000;
        c.m.w32(PLAYER1_VIEW, view);
        r.call_leaving(c, BLOCKCLEAR, &[C_VIEW, 140], 0x8004_e2a0);
        let view = c.m.r32(PLAYER1_VIEW);
        c.m.w32(C_VIEW, view);
        r.enter_leaving(c, 0x8004_dca4, 0x8004_e2ac, abi_allocate_draw_memory);
        c.m.w32(DO_VSYNC, 1);
        r.enter_leaving(c, 0x8002_1b48, 0x8004_e2c0, |c| {
            (c.r[4], c.r[5]) = (0, 0);
            crate::fetextrender::abi_set_abr(c)
        });
        c.m.w32(G_FLIP, 0);
        c.r[2] = 0x8014_0000;
    })
}

/// `Init_PSX_FrontEnd` (PSXFRONT.CPP, 0x8004e2e0): the timer, the geometry, the help's shapes,
/// the menus' CLUT, a 512-wide 16-bit screen; the first time, the intro movies (the second
/// unless the first was skipped), the environment, the defaults, the language's words, the
/// title (the host's), `Front_SecondaryMemCardCheck` (FRONT.CPP); else the environment and the
/// words; then the fonts, the sound driver, the music's list, the samples, the help's shapes
/// loaded.
pub fn abi_init_psx_front_end(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        c.m.w32(G_FLIP, u32::MAX);
        r.call_leaving(c, ADD_TIMER, &[TIMER], 0x8004_e300);
        r.call_leaving(c, INIT_GEOM, &[], 0x8004_e308);
        r.enter_leaving(c, 0x8004_e1b0, 0x8004_e310, abi_psx_alloc_shapes);
        r.call_leaving(c, INIT_MENU_CLUT, &[], 0x8004_e318);
        c.m.w32(SCREEN_WIDTH, 512);
        c.m.w32(SCREEN_BPP, 16);
        if c.m.r32(FIRST_TIME) != 0 {
            if r.call_leaving(c, MOVIE_PLAY, &[4], 0x8004_e344) != 1 {
                r.call_leaving(c, MOVIE_PLAY, &[0], 0x8004_e358);
            }
            r.call_leaving(c, INIT_LINK_MODE, &[0, 1, 1], 0x8004_e368);
            r.enter_leaving(c, 0x8004_e1f4, 0x8004_e370, abi_init_rendering_environment);
            r.call_leaving(c, INIT_LINK_MODE, &[0, 20, 0], 0x8004_e380);
            r.enter_leaving(c, 0x8004_e11c, 0x8004_e388, abi_do_language_screen);
            let language = c.m.r8(LANGUAGE) as u32;
            r.call_leaving(c, LOAD_WORDS, &[language], 0x8004_e398);
            r.call_ptr_leaving(c, DO_TITLE_SCREEN, &[], 0x8004_e3a0);
            r.call_leaving(c, INIT_LINK_MODE, &[0, 1, 1], 0x8004_e3b0);
            r.call_leaving(c, SECONDARY_MEM_CARD_CHECK, &[], 0x8004_e3b8);
        } else {
            r.enter_leaving(c, 0x8004_e1f4, 0x8004_e3c8, abi_init_rendering_environment);
            let language = c.m.r8(LANGUAGE) as u32;
            r.call_leaving(c, LOAD_WORDS, &[language], 0x8004_e3d8);
        }
        r.enter_leaving(c, 0x8002_6bc8, 0x8004_e3e0, crate::fetools::abi_init);
        r.call_leaving(c, AUDIO_INIT_DRIVER, &[0xD800, 0x1_8000], 0x8004_e3f0);
        let list = c.m.r32(PLAY_LIST);
        r.call_leaving(c, SET_PLAY_LIST, &[list], 0x8004_e400);
        r.call_leaving(c, LOAD_FE_SAMPLES, &[], 0x8004_e408);
        r.enter_leaving(c, 0x8004_f09c, 0x8004_e410, abi_load_all_help_shapes);
    })
}

/// `LoadAllHelpShapes` (PSXFRONT.CPP, 0x8004f09c): the permanent shapes' file (`zperm.psh`)
/// read, its 59 shapes put in VRAM into the help's table, the file freed.
pub fn abi_load_all_help_shapes(c: &mut Cpu) {
    abi::frame(c, 80, |c, sp| {
        let mut r = Saved::of(c);
        let path = c.m.r32(CD_PATH);
        r.call_leaving(c, SPRINTF, &[sp + 16, ZPERM_FILE, path], 0x8004_f0c8);
        let file = r.call_leaving(c, LOAD_SHAPE, &[sp + 16, 0], 0x8004_f0d4);
        r.0[1] = file;
        r.0[0] = 0;
        if file == 0 {
            return;
        }
        r.0[2] = 0x8005_0000;
        for k in 0..HELP_COUNT {
            let shape = c.m.r32(HELP_SHAPES).wrapping_add(k << 5);
            r.0[0] = k + 1;
            r.call_leaving(c, LOAD_PMX, &[file, k, shape], 0x8004_f100);
        }
        r.call_leaving(c, PURGE, &[file], 0x8004_f114);
    })
}

/// `FontUpsideDownBlit` (PSXFRONT.CPP, 0x8004f49c): a font's character drawn upside down as a
/// `POLY_FT4` (40 bytes) in the font's tint and CLUT, at `x`, `y` from its texture's place
/// (`u`, `v` plus the font's row), measured from its baseline.
pub fn abi_font_upside_down_blit(c: &mut Cpu) {
    let (x, y, font, u) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (row, ch) = (abi::arg(c, 4), abi::arg(c, 5));
    let m = &mut c.m;
    let prim = m.r32(NEXT_PRIM);
    let w = m.r8(ch + 2) as u32;
    let h = m.r8(ch + 3) as u32;
    let base = m.r8(ch + 10) as i8 as i32 as u32;
    m.w32(NEXT_PRIM, prim.wrapping_add(40));
    let head = m.r32(prim) & 0xFF00_0000;
    let ot = m.r32(OT);
    let place = m.r32(font + 12);
    let top = y.wrapping_sub(base);
    let tall = h.wrapping_add(base);
    let v = ((((place << 4) as i32 >> 20) as u32).wrapping_add(row) & 0xFF).wrapping_sub(1);
    let entry = m.r32(ot) & 0x00FF_FFFF;
    m.w32(prim, head | entry);
    let entry = m.r32(ot) & 0xFF00_0000;
    let tint = m.r32(FONT_TINT);
    m.w32(ot, entry | (prim & 0x00FF_FFFF));
    m.w32(prim + 4, tint);
    m.w8(prim + 3, 9);
    m.w8(prim + 7, 44);
    let bottom = top.wrapping_add(5).wrapping_sub(tall);
    let clut = m.r16(FONT_CLUT);
    m.w16(prim + 14, clut);
    let mode = m.r8(font) as u32;
    let place = m.r32(font + 12);
    let u1 = u.wrapping_add(w);
    m.w8(prim + 13, v as u8);
    m.w8(prim + 21, v as u8);
    let v1 = v.wrapping_add(h);
    let top = bottom.wrapping_add(h);
    let x1 = x.wrapping_add(w);
    m.w8(prim + 12, u as u8);
    m.w8(prim + 20, u1 as u8);
    m.w8(prim + 28, u as u8);
    m.w8(prim + 29, v1 as u8);
    m.w8(prim + 36, u1 as u8);
    m.w8(prim + 37, v1 as u8);
    m.w16(prim + 8, x as u16);
    let page = ((mode & 3) << 7) | ((place >> 20) & 0x10) | (((place & 0x3FF) as i32 >> 6) as u32);
    m.w16(prim + 22, page as u16);
    m.w16(prim + 10, top as u16);
    m.w16(prim + 16, x1 as u16);
    m.w16(prim + 18, top as u16);
    m.w16(prim + 24, x as u16);
    m.w16(prim + 26, bottom as u16);
    m.w16(prim + 32, x1 as u16);
    m.w16(prim + 34, bottom as u16);
    (c.r[2], c.r[3]) = ((place >> 20) & 0x10, page);
}

/// `CleanupSpinningCarsMenu` entered as the original's callers `jal` it.
pub(crate) fn cleanup_spinning_cars_menu_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_dfac, None, &[]);
    abi_cleanup_spinning_cars_menu(c);
}

/// `DrawShapeExtended` and `ScaleShapeExtended` entered as the original's callers `jal` them.
pub(crate) fn draw_shape_extended_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_eb60, None, &[]);
    abi_draw_shape_extended(c);
}

pub(crate) fn scale_shape_extended_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_ef70, None, &[]);
    abi_scale_shape_extended(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn psxfront_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x861 + k as u64, shuffle, &args);
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

    #[test]
    fn psxfront_glue_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in glue_cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x881 + k as u64, glue_shuffle, &args);
        }
    }
}
