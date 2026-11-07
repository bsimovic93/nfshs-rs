//! HRZSKU.CPP: the sky and the horizon. The sky is a dome of five rings of 17 points (`gSkyMesh`,
//! 85 points around the camera, its rings at the track's angles or at even steps) drawn as 64
//! quads, each ring shaded round from one colour at its first point to another at its far side
//! (`gSkyColor`), or textured from the horizon's pictures; the stars (points of a random sky, the
//! same each race of a track), the sun and the moon sit in it. The horizon is a ring of 17 points
//! far out (`gRngCoordTop`, 32,000 out) carrying a band of 16 textured pictures from the ground to
//! a height that each picture's own height sets (`gfxPmxHeightPercentage`), its bottom and top
//! edges shaded round the ring as the sky's rings are. At night a lightning's forks (a little tree of up to 16
//! textured squares, `gHrz_Lightning`) are drawn into the sky where the flash is, which turns
//! the whole sky its colour while it lasts.
//!
//! The host draws the sky and horizon itself from what these set up (`src/ps1/race.rs`'s
//! `SkyData`), so the building functions run only in the tests; the set-up runs while the race
//! loads and ticks as the original's loops do (see `call::ticks`), as do the lightning's flicker
//! and the sky's colours (the night's code runs them).
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data and the GTE's
//! registers as the original leaves them, the stack and CPU registers free.

use crate::call::{call_jal, ticks};
use crate::fixed::{fixedcos_ticked, fixedmult};
use crate::gpu;
use ps1rt::{Cpu, Mem};

/// The track's spec (`TrackSpec_gSpec`): the horizon band's switch (+4) and the sky's (+6), the
/// sun's and moon's heights (+0x90, +0x98) over the horizon's (+0xC0); its horizon record
/// (`Hrz_gTrackSpec`, +0x28) and sky record (`Sky_gTrackSpec`, +0x58), the sky's flags (+0x5C:
/// 4 the sun, 8 the moon, 0x10 stars).
const SPEC: u32 = 0x8012_456C;
const BAND_ON: u32 = SPEC + 4;
const SKY_ON: u32 = SPEC + 6;
const SUN_HEIGHT: u32 = SPEC + 0x90;
const MOON_HEIGHT: u32 = SPEC + 0x98;
const HORIZON_HEIGHT: u32 = SPEC + 0xC0;
const HORIZON_RECORD: u32 = SPEC + 0x28;
const SKY_RECORD: u32 = SPEC + 0x58;
const SKY_FLAGS: u32 = SPEC + 0x5C;
const SHOW_SUN: u32 = 4;
const SHOW_MOON: u32 = 8;
const SHOW_STARS: u32 = 0x10;

/// The horizon record (`Hrz_gTrackSpec`): the ring's turn (+4), the band's bottom (+8) and height
/// (+0xC), its colours (four bytes each: the bottom edge's at the ring's first point +0x10 and at
/// its far side +0x18, the top edge's +0x14 and +0x1C), each segment's picture (+0x20: 16 bytes, `& 7` the picture, 8 and up
/// mirrored, 16 none).
const HRZ_TURN: u32 = 4;
const HRZ_BOTTOM: u32 = 8;
const HRZ_HEIGHT: u32 = 0xC;
const HRZ_PEAK_COLORS: u32 = 0x18;
const HRZ_BASE_COLORS: u32 = 0x10;
const HRZ_SEGMENTS: u32 = 0x20;
const NO_PICTURE: u8 = 16;
const MIRRORED: u8 = 8;

/// The sky record (`Sky_gTrackSpec`): textured (+0: 1), its flags (+4: 1 the rings projected as
/// they are, 2 the lower rings at half steps, 0x20 the texture shaded, 0x40 dithered, 0x80 the
/// rings' angles given), the flat texture's colour (+8), each ring's colours (four bytes each: at
/// its first point +8, the first ring's the flat texture's; at its far side +0x1C), the sun's and moon's bearings (+0x34, +0x3C), the view's
/// colour (+0x30: three bytes), the stars (+0x44 how many, +0x48..0x4C their elevations, +0x50..
/// 0x54 their brightness, +0x5C the seed), the sky's height (+0x68), each quad's picture
/// (+0x6C: 4 rows of 4, repeated round the ring), the rings' angles (+0x80).
const SKY_TEXTURED: u32 = 0;
const SKY_FLAG_BITS: u32 = 4;
const SKY_COLOR: u32 = 8;
const RING_BASE_COLORS: u32 = 8;
const RING_PEAK_COLORS: u32 = 0x1C;
const VIEW_COLOR: u32 = 0x30;
const SUN_BEARING: u32 = 0x34;
const MOON_BEARING: u32 = 0x3C;
const STAR_COUNT: u32 = 0x44;
const STAR_LOW: u32 = 0x48;
const STAR_HIGH: u32 = 0x4C;
const STAR_DIM: u32 = 0x50;
const STAR_BRIGHT: u32 = 0x54;
const STAR_SEED: u32 = 0x5C;
const SKY_HEIGHT: u32 = 0x68;
const SKY_PICTURES: u32 = 0x6C;
const RING_ANGLES: u32 = 0x80;
const AS_IS: u32 = 1;
const HALF_STEPS: u32 = 2;
const SHADED: u32 = 0x20;
const DITHERED: u32 = 0x40;
const ANGLES_GIVEN: u32 = 0x80;

/// The records' places (`Hrz_gTrackSpec`, `Sky_gTrackSpec`), the ring (`gRngCoordTop`: 17
/// `SVECTOR`s, reserved), the stars (`starPosInSky`: an `SVECTOR` each; `starColors`: a word
/// each), the lightning's place in the sky (`Hrz_gLightningPosInSky`), the sun's and moon's
/// (`sunPosInSky`, `moonPosInSky`), the flat texture's colour kept through a lightning
/// (`Hrz_gSaveCol + 4`).
const HRZ_SPEC: u32 = 0x8013_EB70;
const SKY_SPEC: u32 = 0x8013_EB74;
const RING: u32 = 0x8013_EB78;
const STARS: u32 = 0x8013_EB60;
const STAR_COLORS: u32 = 0x8013_EB64;
const LIGHTNING_POS: u32 = 0x8013_EB7C;
const SUN_POS: u32 = 0x8013_F0F8;
const MOON_POS: u32 = 0x8013_F100;
const SAVED_COLOR: u32 = 0x8013_F68C;

/// The horizon's pictures (`gHorizonPixmap`: 12 of the track's, then 4 mirrored copies of the
/// last 4 in `gHorizonExtraSkyPixmaps`; a pixmap's corners' uv, 16 bytes), the dome
/// (`gSkyMesh`: 85 `SVECTOR`s; `gSkyColor`: a colour word each; `gSkyPixmapIndex`: a byte per
/// quad), each segment's picture and its height against the tallest's (`gpPmx`,
/// `gfxPmxHeightPercentage`, 16.16), the band's colours (`gHrzRingColor`: the bottom edge's 17,
/// then the top's), the lightning (`gHrz_Lightning`: 16 forks of x, y shorts and a kind byte;
/// +0x60 its brightness, +0x64 its forks), its textures (`gLightningPixmap`: one per kind).
const PIXMAPS: u32 = 0x8012_15E8;
const EXTRA_PIXMAPS: u32 = 0x8012_1628;
const MESH: u32 = 0x8012_1668;
const SKY_COLORS: u32 = 0x8012_1910;
const SKY_PIXMAP_INDEX: u32 = 0x8012_1A64;
const SEGMENT_PIXMAPS: u32 = 0x8012_1AA4;
const HEIGHT_RATIOS: u32 = 0x8012_1B6C;
const BAND_COLORS: u32 = 0x8012_1AE4;
const LIGHTNING: u32 = 0x8012_1BAC;
const BRIGHTNESS: u32 = LIGHTNING + 0x60;
const FORKS: u32 = LIGHTNING + 0x64;
const FORK_SIZE: u32 = 6;
const LIGHTNING_PIXMAPS: u32 = 0x8011_3E4C;

/// The dome's rings and points round each (the 17th the first again), the horizon's segments,
/// how far out it is, the sky's distance.
const RINGS: u32 = 5;
const POINTS: u32 = 17;
const SEGMENTS: u32 = 16;
const SKY_QUADS: u32 = 64;
const RING_RADIUS: i32 = 32_000;
const SKY_RADIUS: i32 = 1000;
const PICTURES: u32 = 12;
/// The sky's pictures are the horizon's from the 9th: its last four and their mirrored copies.
const FIRST_SKY_PICTURE: u8 = 8;
const MIRRORED_PICTURES: u32 = 4;
/// The band drawn when a segment's end is this far (or its other end): nearer, the world hides it.
const BAND_NEAR: i32 = 16_000;
/// Each segment's top edge stretched this far past its end (16.16), so neighbours overlap.
const OVERLAP: i32 = 0x1_07AE;
/// The projection's distance kept at most this while the horizon is drawn.
const MAX_GEOM_SCREEN: i32 = 700;

/// The colours a lightning lights the sky: a fork's, a sheet's.
const FORK_FLASH: u32 = 0xFF_C0C0;
const SHEET_FLASH: u32 = 0xC0_2828;
const FLASH_STEP: i32 = 16;
const FLASH_START: u32 = 200;
/// How far the lightning lands from the flash's place (x and z either side, y up).
const LIGHTNING_SPREAD: u32 = 8000;
const LIGHTNING_RISE: u32 = 3000;
const LIGHTNING_UP: i32 = 8000;

/// The game's set-up (`GameSetup_gData + 0xC`, 1 two players), the views (`Draw_gPlayer1View`,
/// `...2View`), the ordering table's length (`Draw_gViewOtSize`), the projection's distance
/// (`Camera_gGeomScreen`), the night's lightning (`Night_gLightning`, `Night_gShowForks`).
const COMM_MODE: u32 = 0x8011_44B0;
const SPLIT_SCREEN: u32 = 1;
const PLAYER1_VIEW: u32 = 0x8013_E6B0;
const PLAYER2_VIEW: u32 = 0x8013_E6B4;
const VIEW_OT_SIZE: u32 = 0x8013_EA98;
const GEOM_SCREEN: u32 = 0x8013_DAD0;
const NIGHT_LIGHTNING: u32 = 0x8013_ED44;
const SHOW_FORKS: u32 = 0x8013_ED58;

/// The scratchpad as the drawing has it (`dcache`): the ordering table (+0), the next primitive
/// (+4), the screen's width and height (+0x10, +0x12), the horizon's top points (+0x14, the
/// dome's points too), its ground points (+0x58), the segments' tops at each end (+0x9C, +0xE0),
/// the ring's depths (+0x124, the dome's at +0x168), the ordering table entry drawn into
/// (+0x94).
const DCACHE: u32 = 0x1F80_0000;
const SCREEN_W: u32 = DCACHE + 0x10;
const SCREEN_H: u32 = DCACHE + 0x12;
const TOPS: u32 = DCACHE + 0x14;
const GROUND: u32 = DCACHE + 0x58;
const SEGMENT_TOPS: u32 = DCACHE + 0x9C;
const SEGMENT_ENDS: u32 = DCACHE + 0xE0;
const DEPTHS: u32 = DCACHE + 0x124;
const SKY_DEPTHS: u32 = DCACHE + 0x168;
const OT_INDEX: u32 = 0x94;
/// The views' screen centre line in the dome's own projection (one view, two).
const HALF_HEIGHT: i32 = 120;
const SPLIT_HALF_HEIGHT: i32 = 60;

/// A view (`DRender_tView`): its turn (+0x44).
const VIEW_TURN: u32 = 0x44;

/// Primitives: a shaded quad (`POLY_G4`, 36 bytes), a flat textured one (`POLY_FT4`, 40), a
/// shaded textured one (`POLY_GT4`, 52), a dot (`TILE_1`, 12), a draw mode (12).
const G4_SIZE: u32 = 36;
const FT4_SIZE: u32 = 40;
const GT4_SIZE: u32 = 52;
const DOT_SIZE: u32 = 12;
const DOT_LEN: u8 = 2;
const DOT_CODE: u8 = 0x68;
const DRAW_MODE_SIZE: u32 = 12;
const FT4_SEMI_CODE: u8 = 0x2E;
/// The draw mode's texture page (`SetDrawMode`'s), the dome's dithering off then on.
const DRAW_MODE_TPAGE: u32 = 256;

/// The GTE: the rotation (control 0..4), the translation (5..7); vector 0 (data 0, 1), the
/// screen places (SXY0..2, data 12..14), the depths (SZ1..3, data 17..19), the results
/// (MAC2, MAC3: data 26, 27). RTPS, RTPT.
const RT: usize = 32;
const TR: usize = 32 + 5;
const VXY0: usize = 0;
const SXY0: usize = 12;
const SXY2: usize = 14;
const SZ1: usize = 17;
const SZ3: usize = 19;
const MAC2: usize = 26;
const MAC3: usize = 27;
const RTPS: u32 = 0x18_0001;
const RTPT: u32 = 0x28_0030;

/// Functions called: the library's `random`, `seedrandom`, `reservememadr(name, size, 0)`,
/// `purgememadr`, `SetGeomScreen`, `SetDrawMode(prim, dfe, dtd, tpage, rect)`;
/// `Draw_SetViewColor(view, r, g, b)`, `Texture_CloneUVPmx(from, how, to)`,
/// `Flare_InitLensFlare`, `Flare_Sun(pos, dcache)`, `Flare_Moon(pos, dcache)`.
const RANDOM: u32 = 0x800E_8394;
const SEED_RANDOM: u32 = 0x800E_8464;
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const SET_GEOM_SCREEN: u32 = 0x800E_B4F8;
const SET_DRAW_MODE: u32 = 0x800E_ED18;
const SET_VIEW_COLOR: u32 = 0x800B_EBEC;
const CLONE_UV_PMX: u32 = 0x800E_0F64;
const INIT_LENS_FLARE: u32 = 0x800C_F8A4;
const FLARE_SUN: u32 = 0x800C_FF30;
const FLARE_MOON: u32 = 0x800D_021C;
/// The blocks' names: "stars", "starCols", "gRngCoordTop".
const STARS_NAME: u32 = 0x8013_EB68;
const STAR_COLORS_NAME: u32 = 0x8005_7068;
const RING_NAME: u32 = 0x8005_7074;

fn random(c: &mut Cpu) -> u32 {
    call_jal(c, RANDOM, &[])
}

/// `a % b` as the R3000's `divu` leaves it (by 0, `a`).
fn remu(a: u32, b: u32) -> u32 {
    a.checked_rem(b).unwrap_or(a)
}

fn sky(m: &mut Mem) -> u32 {
    m.r32(SKY_SPEC)
}

fn horizon(m: &mut Mem) -> u32 {
    m.r32(HRZ_SPEC)
}

/// A word of the sky record, of the horizon record.
fn sky_u32(m: &mut Mem, offset: u32) -> u32 {
    let at = sky(m).wrapping_add(offset);
    m.r32(at)
}

fn hrz_u32(m: &mut Mem, offset: u32) -> u32 {
    let at = horizon(m).wrapping_add(offset);
    m.r32(at)
}

fn short(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn split_screen(m: &mut Mem) -> bool {
    m.r32(COMM_MODE) == SPLIT_SCREEN
}

/// A point of a circle `radius` out (16.16 sine and cosine scaled), on a full-turn `angle`.
fn on_circle(c: &mut Cpu, angle: i32, radius: i32) -> (i32, i32) {
    let sin = crate::fixed::fixedsin_ticked(c, angle);
    let cos = fixedcos_ticked(c, angle);
    (fixedmult(sin, radius), fixedmult(cos, radius))
}

/// Points' screen places `a` to `b` (`DVECTOR`s) a fraction `t` (16.16) of the way.
fn lerp(a: i16, b: i16, t: i32) -> u16 {
    let d = (b as i32 - a as i32).wrapping_mul(t);
    (a as u16).wrapping_add((d / 0x1_0000) as u16)
}

/// `Horizon_InterpolateLineSCoords` (HRZSKU.CPP, 0x800d02d4): `n` screen places `ratios` (16.16
/// each, or the first for all) of the way from `from`'s to `to`'s, into `out`.
pub fn interpolate_line_s_coords(m: &mut Mem, out: u32, from: u32, to: u32, ratios: u32, n: i32, each: bool) {
    for k in 0..n.max(0) as u32 {
        let t = m.r32(ratios.wrapping_add(if each { 4 * k } else { 0 })) as i32;
        let (a, b) = (from.wrapping_add(4 * k), to.wrapping_add(4 * k));
        for half in [0, 2] {
            let v = lerp(m.r16(a + half) as i16, m.r16(b + half) as i16, t);
            m.w16(out.wrapping_add(4 * k + half), v);
        }
    }
}

pub fn abi_interpolate_line_s_coords(c: &mut Cpu) {
    let sp = c.r[29];
    let (n, each) = (c.m.r32(sp + 16) as i32, c.m.r32(sp + 20) != 0);
    interpolate_line_s_coords(&mut c.m, c.r[4], c.r[5], c.r[6], c.r[7], n, each);
}

/// `Sky_InitStars` (HRZSKU.CPP, 0x800d0414): the stars, unless they are: each at a random
/// bearing and an elevation between the record's two, a grey between its two, from the track's
/// own seed (so the same sky each race); the game's random numbers then go on as they were.
pub fn init_stars(c: &mut Cpu) {
    if c.m.r32(STARS) != 0 {
        return;
    }
    let saved = random(c);
    let seed = sky_u32(&mut c.m, STAR_SEED);
    call_jal(c, SEED_RANDOM, &[seed]);
    let n = sky_u32(&mut c.m, STAR_COUNT);
    let stars = call_jal(c, RESERVE, &[STARS_NAME, n << 3, 0]);
    c.m.w32(STARS, stars);
    let n = sky_u32(&mut c.m, STAR_COUNT);
    let colors = call_jal(c, RESERVE, &[STAR_COLORS_NAME, n << 2, 0]);
    c.m.w32(STAR_COLORS, colors);
    let mut k = 0u32;
    while (k as i32) < sky_u32(&mut c.m, STAR_COUNT) as i32 {
        let bearing = random(c) & 0xFFFF;
        let r = random(c);
        let s = sky(&mut c.m);
        let (low, high) = (c.m.r32(s + STAR_LOW), c.m.r32(s + STAR_HIGH));
        let elevation = low.wrapping_add(remu(r, high.wrapping_sub(low))) as i32;
        let (up, out) = on_circle(c, elevation, SKY_RADIUS);
        let (x, z) = on_circle(c, bearing as i32, out);
        let m = &mut c.m;
        let star = m.r32(STARS).wrapping_add(8 * k);
        m.w16(star, x as u16);
        m.w16(star + 2, up as u16);
        m.w16(star + 4, z as u16);
        let r = random(c);
        let s = sky(&mut c.m);
        let (dim, bright) = (c.m.r32(s + STAR_DIM), c.m.r32(s + STAR_BRIGHT));
        let grey = dim.wrapping_add(remu(r, bright.wrapping_sub(dim)));
        let at = c.m.r32(STAR_COLORS).wrapping_add(4 * k);
        c.m.w32(at, grey << 16 | grey << 8 | grey);
        k += 1;
        c.tick();
    }
    call_jal(c, SEED_RANDOM, &[saved]);
}

pub fn abi_init_stars(c: &mut Cpu) {
    init_stars(c);
}

/// `Sky_KillStars` (HRZSKU.CPP, 0x800d05fc): the stars' blocks given back.
pub fn kill_stars(c: &mut Cpu) {
    for at in [STARS, STAR_COLORS] {
        let block = c.m.r32(at);
        if block != 0 {
            call_jal(c, PURGE, &[block]);
        }
    }
    c.m.w32(STARS, 0);
    c.m.w32(STAR_COLORS, 0);
}

pub fn abi_kill_stars(c: &mut Cpu) {
    kill_stars(c);
}

/// Colours round a ring of `points` from `base` (four bytes) at its first point to `peak` at its
/// far side (the 9th), an eighth of the way more at each step; the bytes wrap as the original's
/// do.
fn shade_ring(m: &mut Mem, base: u32, peak: u32, out: u32, points: u32) {
    let (b, p): ([u8; 3], [u8; 3]) = (std::array::from_fn(|k| m.r8(base + k as u32)), std::array::from_fn(|k| m.r8(peak + k as u32)));
    let step: [u8; 3] = std::array::from_fn(|k| ((p[k] as i32 - b[k] as i32) / 8) as u8);
    for v in 0..points {
        let f = if v < 9 { v } else { 16 - v } as u8;
        for k in 0..3 {
            m.w8(out + 4 * v + k as u32, b[k].wrapping_add(step[k].wrapping_mul(f)));
        }
    }
}

/// `Hrz_InitSkyColor` (HRZSKU.CPP, 0x800d0640): the dome's colours, each ring's shaded round from
/// its first point's colour to its far side's.
pub fn init_sky_color(c: &mut Cpu) {
    for ring in 0..RINGS {
        let s = sky(&mut c.m);
        shade_ring(&mut c.m, s + RING_BASE_COLORS + 4 * ring, s + RING_PEAK_COLORS + 4 * ring, SKY_COLORS + 4 * POINTS * ring, POINTS);
        ticks(c, POINTS + 1);
    }
}

pub fn abi_init_sky_color(c: &mut Cpu) {
    init_sky_color(c);
}

/// `Hrz_InitSky` (HRZSKU.CPP, 0x800d079c): the sun and moon placed in the sky at their bearings
/// (1000 out, at their heights over the horizon's), the views' colour, the dome's points (each
/// ring at its angle up: given, or 22.5 degrees a ring, the lower four at half that), the quads'
/// pictures when textured, the colours (unless a flat texture's), the lens flare's set-up.
pub fn init_sky(c: &mut Cpu) {
    for (pos, bearing, height) in [(SUN_POS, SUN_BEARING, SUN_HEIGHT), (MOON_POS, MOON_BEARING, MOON_HEIGHT)] {
        let angle = sky_u32(&mut c.m, bearing) as i32;
        let (z, x) = on_circle(c, angle, SKY_RADIUS);
        let m = &mut c.m;
        m.w16(pos, x as u16);
        m.w16(pos + 4, z as u16);
        let y = m.r16(height).wrapping_add(m.r16(HORIZON_HEIGHT));
        m.w16(pos + 2, y);
    }
    let views: &[u32] = if split_screen(&mut c.m) { &[PLAYER1_VIEW, PLAYER2_VIEW] } else { &[PLAYER1_VIEW] };
    for &view in views {
        let s = sky(&mut c.m);
        let at = c.m.r32(view);
        let rgb: [u32; 3] = std::array::from_fn(|k| c.m.r8(s + VIEW_COLOR + k as u32) as u32);
        call_jal(c, SET_VIEW_COLOR, &[at, rgb[0], rgb[1], rgb[2]]);
    }
    for ring in 0..RINGS {
        let flags = sky_u32(&mut c.m, SKY_FLAG_BITS);
        let angle = if flags & ANGLES_GIVEN != 0 {
            sky_u32(&mut c.m, RING_ANGLES + 4 * ring) as i32
        } else if flags & HALF_STEPS != 0 && ring < 4 {
            (ring << 11) as i32
        } else {
            (ring << 12) as i32
        };
        let (up, out) = on_circle(c, angle, SKY_RADIUS);
        for k in 0..POINTS {
            let a = ((k << 12) as i32).wrapping_sub(sky_u32(&mut c.m, SUN_BEARING) as i32).wrapping_add(0x4000);
            let (x, z) = on_circle(c, a, out);
            let m = &mut c.m;
            let s = sky(m);
            let point = MESH + 8 * (ring * POINTS + k);
            m.w16(point, x as u16);
            let y = m.r16(s + SKY_HEIGHT).wrapping_add(up as u16);
            m.w16(point + 2, y);
            m.w16(point + 4, z as u16);
        }
    }
    ticks(c, RINGS * (POINTS - 1) + RINGS - 1);
    let s = sky(&mut c.m);
    if c.m.r32(s + SKY_TEXTURED) == 1 {
        for row in 0..4 {
            for k in 0..16 {
                let s = sky(&mut c.m);
                let picture = c.m.r8(s + SKY_PICTURES + 4 * row + k % 4).wrapping_add(FIRST_SKY_PICTURE);
                c.m.w8(SKY_PIXMAP_INDEX + 16 * row + k, picture);
            }
        }
        ticks(c, 4 * 15 + 3);
    }
    let s = sky(&mut c.m);
    let flat = c.m.r32(s + SKY_TEXTURED) == 1 && c.m.r32(s + SKY_FLAG_BITS) & SHADED == 0;
    if !flat {
        init_sky_color(c);
    }
    call_jal(c, INIT_LENS_FLARE, &[]);
}

pub fn abi_init_sky(c: &mut Cpu) {
    init_sky(c);
}

/// `Hrz_Init2DRing` (HRZSKU.CPP, 0x800d0ae0): the horizon's ring (17 points round, from the
/// record's turn), each segment's picture and its height against the tallest's, the band's
/// colours (the bottom edge's and the top's, each shaded round the ring, the 17th point's the
/// first's), the sky, the flat texture's colour kept.
pub fn init_2d_ring(c: &mut Cpu) {
    for k in 0..POINTS {
        let angle = ((k << 12) as i32).wrapping_add(hrz_u32(&mut c.m, HRZ_TURN) as i32);
        let (x, z) = on_circle(c, angle, RING_RADIUS);
        let m = &mut c.m;
        let point = m.r32(RING).wrapping_add(8 * k);
        m.w16(point, x as u16);
        m.w16(point + 2, 0);
        m.w16(point + 4, z as u16);
    }
    let m = &mut c.m;
    let h = horizon(m);
    let mut heights = [0i32; SEGMENTS as usize];
    let mut tallest = 1;
    for k in 0..SEGMENTS {
        let picture = (m.r8(h + HRZ_SEGMENTS + k) & 7) as u32;
        let pmx = m.r32(PIXMAPS + 4 * picture);
        m.w32(SEGMENT_PIXMAPS + 4 * k, pmx);
        let height = m.r8(pmx.wrapping_add(9)) as i32 - m.r8(pmx.wrapping_add(5)) as i32;
        heights[k as usize] = height;
        tallest = tallest.max(height);
    }
    for (k, &height) in heights.iter().enumerate() {
        m.w32(HEIGHT_RATIOS + 4 * k as u32, ((height << 16) / tallest) as u32);
    }
    for edge in 0..2 {
        let h = horizon(m);
        let out = BAND_COLORS + 4 * POINTS * edge;
        shade_ring(m, h + HRZ_BASE_COLORS + 4 * edge, h + HRZ_PEAK_COLORS + 4 * edge, out, SEGMENTS);
        let first = m.r32(out);
        m.w32(out + 4 * SEGMENTS, first);
    }
    ticks(c, (POINTS - 1) + SEGMENTS + (SEGMENTS - 1) + 2 * (SEGMENTS + 1));
    init_sky(c);
    let color = sky_u32(&mut c.m, SKY_COLOR);
    c.m.w32(SAVED_COLOR, color);
}

pub fn abi_init_2d_ring(c: &mut Cpu) {
    init_2d_ring(c);
}

/// `Hrz_GetHorizonPixMap` (HRZSKU.CPP, 0x800d0dc4): the horizon's 12 pictures from the track's
/// pixmaps at `pixmaps`, then the last four mirrored (for the sky's far side).
pub fn get_horizon_pix_map(c: &mut Cpu, pixmaps: u32) {
    for k in 0..PICTURES {
        c.m.w32(PIXMAPS + 4 * k, pixmaps.wrapping_add(16 * k));
    }
    ticks(c, PICTURES - 1);
    for k in 0..MIRRORED_PICTURES {
        let from = c.m.r32(PIXMAPS + 4 * (PICTURES - MIRRORED_PICTURES + k));
        let copy = EXTRA_PIXMAPS + 16 * k;
        call_jal(c, CLONE_UV_PMX, &[from, 0, copy]);
        c.m.w32(PIXMAPS + 4 * (PICTURES + k), copy);
    }
    ticks(c, MIRRORED_PICTURES - 1);
}

pub fn abi_get_horizon_pix_map(c: &mut Cpu) {
    get_horizon_pix_map(c, c.r[4]);
}

/// `Hrz_InitHorizon` (HRZSKU.CPP, 0x800d0e78): the records found, the ring reserved and set up
/// (with the sky), the stars when the track has them.
pub fn init_horizon(c: &mut Cpu) {
    c.m.w32(HRZ_SPEC, HORIZON_RECORD);
    c.m.w32(SKY_SPEC, SKY_RECORD);
    let ring = call_jal(c, RESERVE, &[RING_NAME, 8 * POINTS, 0]);
    c.m.w32(RING, ring);
    init_2d_ring(c);
    if c.m.r32(SKY_FLAGS) & SHOW_STARS != 0 {
        init_stars(c);
    }
}

pub fn abi_init_horizon(c: &mut Cpu) {
    init_horizon(c);
}

/// `Hrz_KillHorizon` (HRZSKU.CPP, 0x800d0ee4): the ring and the stars given back.
pub fn kill_horizon(c: &mut Cpu) {
    let ring = c.m.r32(RING);
    call_jal(c, PURGE, &[ring]);
    kill_stars(c);
}

pub fn abi_kill_horizon(c: &mut Cpu) {
    kill_horizon(c);
}

/// A fork's kinds (its texture): a branch going on (2, 3), one forking in two (6), an end (4, 5),
/// the first (0, 1); each +8 leaning left.
const FORK_ON: u8 = 2;
const FORK_SPLIT: u8 = 6;
const FORK_END: u8 = 4;
const LEANS_LEFT: u8 = 8;

/// `Hrz_LightningAddFork` (HRZSKU.CPP, 0x800d0f10): a fork at (`x`, `y`) in steps of 15 (`y` down
/// the bolt), going on toward `dir` (-1 left, 1 right): the first goes one way at random, each
/// after it goes on, splits in two or ends at random, those three down end.
pub fn lightning_add_fork(c: &mut Cpu, dir: i8, x: i8, y: i8) {
    #[derive(Clone, Copy)]
    enum Next {
        On,
        Split,
        End,
        First,
    }
    let next = match y {
        0 => Next::First,
        3.. => Next::End,
        _ => [Next::On, Next::Split, Next::End][(random(c) % 3) as usize],
    };
    let m = &mut c.m;
    let k = m.r8(FORKS);
    m.w8(FORKS, k.wrapping_add(1));
    let fork = LIGHTNING + FORK_SIZE * k as u32;
    m.w16(fork, (x as i32 * 15) as u16);
    m.w16(fork + 2, (y as i32 * 15) as u16);
    let below = y.wrapping_add(1);
    // (each leans the way it was sent, the first the way it went)
    let (kind, lean) = match next {
        Next::On => {
            lightning_add_fork(c, dir, x.wrapping_add(dir), below);
            (FORK_ON + (random(c) & 1) as u8, dir)
        }
        Next::Split => {
            lightning_add_fork(c, -1, x.wrapping_sub(1), below);
            lightning_add_fork(c, 1, x.wrapping_add(1), below);
            (FORK_SPLIT, dir)
        }
        Next::End => (FORK_END + (random(c) & 1) as u8, dir),
        Next::First => {
            let way = (1 - 2 * (random(c) & 1) as i32) as i8;
            lightning_add_fork(c, way, x.wrapping_add(way), below);
            ((random(c) & 1) as u8, way)
        }
    };
    c.m.w8(fork + 4, if lean < 0 { kind + LEANS_LEFT } else { kind });
}

pub fn abi_lightning_add_fork(c: &mut Cpu) {
    lightning_add_fork(c, c.r[4] as i8, c.r[5] as i8, c.r[6] as i8);
}

/// `Hrz_CalculateLightning` (HRZSKU.CPP, 0x800d1128): a new bolt, at its brightest.
pub fn calculate_lightning(c: &mut Cpu) {
    c.m.w8(FORKS, 0);
    c.m.w32(BRIGHTNESS, FLASH_START);
    lightning_add_fork(c, 1, 0, 0);
}

pub fn abi_calculate_lightning(c: &mut Cpu) {
    calculate_lightning(c);
}

/// The ordering table entry `index` of the scratchpad's table.
fn ot_entry(m: &mut Mem, index: u32) -> u32 {
    m.r32(DCACHE).wrapping_add(index << 2)
}

/// The view's ordering table's back entry (the sky's).
fn back_entry(m: &mut Mem) -> u32 {
    let index = m.r32(VIEW_OT_SIZE).wrapping_sub(2);
    ot_entry(m, index)
}

/// `Hrz_TextureQuad` (HRZSKU.CPP, 0x800d1164): a fork's square (16 pixels, its corner at the
/// screen place `xy`), its kind's texture, grey `shade`, semi-transparent, into `dcache`'s
/// ordering table entry.
pub fn texture_quad(c: &mut Cpu, xy: u32, kind: u8, shade: u8, dcache: u32) {
    let m = &mut c.m;
    let index = m.r32(dcache.wrapping_add(OT_INDEX));
    let entry = ot_entry(m, index);
    let prim = gpu::add_prim_to(m, entry, FT4_SIZE);
    m.w8(prim + 3, 9);
    m.w8(prim + 7, FT4_SEMI_CODE);
    for k in 4..7 {
        m.w8(prim + k, shade);
    }
    let (x, y) = (m.r16(xy), m.r16(xy + 2));
    for (at, dx, dy) in [(8, 0, 0), (16, 16, 0), (24, 0, 16), (32, 16, 16)] {
        m.w16(prim + at, x.wrapping_add(dx));
        m.w16(prim + at + 2, y.wrapping_add(dy));
    }
    let pmx = m.r32(LIGHTNING_PIXMAPS + 4 * kind as u32);
    for k in 0..4 {
        let uv = m.r32(pmx.wrapping_add(4 * k));
        m.w32(prim + 12 + 8 * k, uv);
    }
}

pub fn abi_texture_quad(c: &mut Cpu) {
    texture_quad(c, c.r[4], c.r[5] as u8, c.r[6] as u8, c.r[7]);
}

/// `Hrz_SetLightingPosInSky` (HRZSKU.CPP, 0x800d1294): the lightning placed over the flash's
/// place `pos` (a quarter of the way, 16.16 to the sky's units), up to 4000 either way and 8000
/// to 11000 up.
pub fn set_lighting_pos_in_sky(c: &mut Cpu, pos: u32) {
    let p: [i32; 3] = std::array::from_fn(|k| c.m.r32(pos + 44 + 4 * k as u32) as i32);
    let spread = LIGHTNING_SPREAD as i32 / 2;
    let x = (p[0] >> 2).wrapping_add(remu(random(c), LIGHTNING_SPREAD) as i32 - spread);
    c.m.w16(LIGHTNING_POS, x as u16);
    let y = (p[1] >> 2).wrapping_add(LIGHTNING_UP).wrapping_add(remu(random(c), LIGHTNING_RISE) as i32);
    c.m.w16(LIGHTNING_POS + 2, y as u16);
    let z = (p[2] >> 2).wrapping_add(remu(random(c), LIGHTNING_SPREAD) as i32 - spread);
    c.m.w16(LIGHTNING_POS + 4, z as u16);
}

pub fn abi_set_lighting_pos_in_sky(c: &mut Cpu) {
    set_lighting_pos_in_sky(c, c.r[4]);
}

/// Vector 0 from the words at `v` (x and y, then z and the pad) projected: RTPS.
fn project(c: &mut Cpu, v: u32) {
    let (xy, z) = (c.m.r32(v), c.m.r32(v.wrapping_add(4)));
    c.gte_write(VXY0, xy);
    c.gte_write(VXY0 + 1, z);
    c.gte_command(RTPS);
}

/// `Hrz_BuildForkLightning` (HRZSKU.CPP, 0x800d13a4): while it shows, the lightning's forks drawn
/// where its place in the sky projects (at the back of the view's ordering table), fading.
pub fn build_fork_lightning(c: &mut Cpu, dcache: u32) {
    if c.m.r32(BRIGHTNESS) as i32 <= 0 {
        return;
    }
    let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
    c.m.w32(dcache.wrapping_add(OT_INDEX), back);
    set_psx_translation_to(c, [0; 3]);
    project(c, LIGHTNING_POS);
    let base = c.gte_read(SXY2);
    let mut k = 0u8;
    while k < c.m.r8(FORKS) {
        let fork = LIGHTNING + FORK_SIZE * k as u32;
        let x = c.m.r16(fork).wrapping_add(base as u16);
        let y = c.m.r16(fork + 2).wrapping_add((base >> 16) as u16);
        let kind = c.m.r8(fork + 4);
        let shade = c.m.r8(BRIGHTNESS);
        crate::call::with_buffer(c, 4, |c, xy| {
            c.m.w16(xy, x);
            c.m.w16(xy + 2, y);
            texture_quad(c, xy, kind, shade, dcache);
        });
        k = k.wrapping_add(1);
    }
    let left = (c.m.r32(BRIGHTNESS) as i32).wrapping_sub(FLASH_STEP);
    c.m.w32(BRIGHTNESS, left as u32);
}

pub fn abi_build_fork_lightning(c: &mut Cpu) {
    build_fork_lightning(c, c.r[4]);
}

/// The whole sky (and the flat texture) one colour.
fn flash_sky(c: &mut Cpu, color: u32) {
    let s = sky(&mut c.m);
    c.m.w32(s + SKY_COLOR, color);
    for k in 0..RINGS * POINTS {
        c.m.w32(SKY_COLORS + 4 * k, color);
    }
    ticks(c, RINGS * POINTS - 1);
}

/// `Hrz_LightningFlicker` (HRZSKU.CPP, 0x800d14c0): the sky lit by a flash (`on` 1 a fork's,
/// else a sheet's) unless the forks show, or (0) put back.
pub fn lightning_flicker(c: &mut Cpu, on: u32) {
    match on {
        0 => {
            let color = c.m.r32(SAVED_COLOR);
            let s = sky(&mut c.m);
            c.m.w32(s + SKY_COLOR, color);
            init_sky_color(c);
        }
        _ if c.m.r8(SHOW_FORKS) != 0 => {}
        1 => flash_sky(c, FORK_FLASH),
        _ => flash_sky(c, SHEET_FLASH),
    }
}

pub fn abi_lightning_flicker(c: &mut Cpu) {
    lightning_flicker(c, c.r[4]);
}

/// `HrzSetPsxMatrix` (HRZSKU.CPP, 0x800d159c): the GTE's rotation from `matrix` (its transpose,
/// 16.16 to 4.12, y turned over for the screen's down).
pub fn set_psx_matrix(c: &mut Cpu, matrix: u32) {
    let m = &mut c.m;
    let e: [i32; 9] = std::array::from_fn(|i| m.r32(matrix.wrapping_add(4 * i as u32)) as i32);
    let flip = |v: i32| v.wrapping_neg();
    let r: [i32; 9] = [e[0], e[3], e[6], flip(e[1]), flip(e[4]), flip(e[7]), e[2], e[5], e[8]];
    let r: [u16; 9] = r.map(|v| (v >> 4) as u16);
    for k in 0..4 {
        c.gte_write(RT + k, r[2 * k] as u32 | (r[2 * k + 1] as u32) << 16);
    }
    c.gte_write(RT + 4, r[8] as u32);
}

pub fn abi_set_psx_matrix(c: &mut Cpu) {
    set_psx_matrix(c, c.r[4]);
}

fn set_psx_translation_to(c: &mut Cpu, v: [i32; 3]) {
    for (k, v) in v.into_iter().enumerate() {
        c.gte_write(TR + k, (v >> 10) as u32);
    }
}

/// `HrzSetPsxTranslation` (HRZSKU.CPP, 0x800d167c): the GTE's translation from `v` (16.16 to
/// the sky's units, a 1024th).
pub fn set_psx_translation(c: &mut Cpu, v: u32) {
    let t: [i32; 3] = std::array::from_fn(|k| c.m.r32(v.wrapping_add(4 * k as u32)) as i32);
    set_psx_translation_to(c, t);
}

pub fn abi_set_psx_translation(c: &mut Cpu) {
    set_psx_translation(c, c.r[4]);
}

/// `Hrz_RotProj16` (HRZSKU.CPP, 0x800d16cc): `n` points (`SVECTOR`s at `v`) projected, their
/// screen places to `sxy` and depths (MAC3) to `sz`.
pub fn rot_proj16(c: &mut Cpu, n: u32, v: u32, sz: u32, sxy: u32) {
    for k in 0..n {
        project(c, v.wrapping_add(8 * k));
        let (place, depth) = (c.gte_read(SXY2), c.gte_read(MAC3));
        c.m.w32(sxy.wrapping_add(4 * k), place);
        c.m.w32(sz.wrapping_add(4 * k), depth);
    }
}

pub fn abi_rot_proj16(c: &mut Cpu) {
    rot_proj16(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `Hrz_SetDitheringPrim` (HRZSKU.CPP, 0x800d1710): a draw mode dithering or not into the
/// ordering table's entry `index`.
pub fn set_dithering_prim(c: &mut Cpu, dither: u32, index: u32) {
    let entry = ot_entry(&mut c.m, index);
    let prim = gpu::add_prim_to(&mut c.m, entry, DRAW_MODE_SIZE);
    call_jal(c, SET_DRAW_MODE, &[prim, 0, dither, DRAW_MODE_TPAGE, 0]);
}

pub fn abi_set_dithering_prim(c: &mut Cpu) {
    set_dithering_prim(c, c.r[4], c.r[5]);
}

/// The dome's line of each view's screen centre (its projection's y offset).
fn half_height(m: &mut Mem) -> i32 {
    if split_screen(m) { SPLIT_HALF_HEIGHT } else { HALF_HEIGHT }
}

/// The dome's 85 points projected: as they are (three at a time), or each with its y from its
/// height (MAC2) a quarter down from the screen's centre line.
fn project_dome(c: &mut Cpu) {
    if sky_u32(&mut c.m, SKY_FLAG_BITS) & AS_IS != 0 {
        let mut k = 0;
        while k + 3 <= RINGS * POINTS {
            for j in 0..3 {
                let v = MESH + 8 * (k + j);
                let (xy, z) = (c.m.r32(v), c.m.r32(v + 4));
                c.gte_write(VXY0 + 2 * j as usize, xy);
                c.gte_write(VXY0 + 2 * j as usize + 1, z);
            }
            c.gte_command(RTPT);
            for j in 0..3 {
                let (place, depth) = (c.gte_read(SXY0 + j as usize), c.gte_read(SZ1 + j as usize));
                c.m.w32(TOPS + 4 * (k + j), place);
                c.m.w32(SKY_DEPTHS + 4 * (k + j), depth);
            }
            k += 3;
        }
        while k < RINGS * POINTS {
            project(c, MESH + 8 * k);
            let (place, depth) = (c.gte_read(SXY2), c.gte_read(SZ3));
            c.m.w32(TOPS + 4 * k, place);
            c.m.w32(SKY_DEPTHS + 4 * k, depth);
            k += 1;
        }
    } else {
        let centre = half_height(&mut c.m);
        for k in 0..RINGS * POINTS {
            project(c, MESH + 8 * k);
            let (place, height, depth) = (c.gte_read(SXY2), c.gte_read(MAC2) as i32, c.gte_read(SZ3));
            c.m.w32(TOPS + 4 * k, place);
            c.m.w16(TOPS + 4 * k + 2, ((height >> 2) + centre) as u16);
            c.m.w32(SKY_DEPTHS + 4 * k, depth);
        }
    }
}

/// The four corners (`a`, `b`, `c`, `d`) of a quad off the screen: each beyond the same edge.
fn off_screen(m: &mut Mem, places: [u32; 4]) -> bool {
    let (w, h) = (short(m, SCREEN_W), short(m, SCREEN_H));
    let x: [i32; 4] = places.map(|p| short(m, p));
    let y: [i32; 4] = places.map(|p| short(m, p + 2));
    x.iter().all(|&x| w < x) || x.iter().all(|&x| x < 0) || y.iter().all(|&y| h < y) || y.iter().all(|&y| y < 0)
}

/// `Hrz_BuildSky` (HRZSKU.CPP, 0x800d1798): the dome drawn at the back of the view's ordering
/// table: its points projected, the sun's flare, then each of its 64 quads with any corner in
/// front of the camera and on the screen, shaded, textured, or textured and shaded; dithered
/// when the sky says so.
pub fn build_sky(c: &mut Cpu) {
    set_psx_translation_to(c, [0; 3]);
    project_dome(c);
    let dithered = sky_u32(&mut c.m, SKY_FLAG_BITS) & DITHERED != 0;
    let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
    if dithered {
        set_dithering_prim(c, 0, back);
    }
    if c.m.r32(SKY_FLAGS) & SHOW_SUN != 0 {
        let index = c.m.r32(DCACHE + OT_INDEX);
        call_jal(c, FLARE_SUN, &[SUN_POS, DCACHE]);
        c.m.w32(DCACHE + OT_INDEX, index);
    }
    for quad in 0..SKY_QUADS {
        // its corners: the ring above's two, then its own two (round the ring)
        let i = quad + quad / SEGMENTS;
        let corners = [i + POINTS, i + POINTS + 1, i, i + 1];
        let m = &mut c.m;
        if corners.iter().all(|&k| m.r32(SKY_DEPTHS + 4 * k) == 0) || off_screen(m, corners.map(|k| TOPS + 4 * k)) {
            continue;
        }
        let place = |m: &mut Mem, n: usize| m.r32(TOPS + 4 * corners[n]);
        let color = |m: &mut Mem, n: usize| m.r32(SKY_COLORS + 4 * corners[n]);
        let s = sky(m);
        let entry = back_entry(m);
        if m.r32(s + SKY_TEXTURED) == 1 {
            let picture = m.r8(SKY_PIXMAP_INDEX + quad) as u32;
            let pmx = m.r32(PIXMAPS + 4 * picture);
            let uv = |m: &mut Mem, n: u32| m.r32(pmx.wrapping_add(4 * n));
            if m.r32(s + SKY_FLAG_BITS) & SHADED != 0 {
                let prim = gpu::add_prim_to(m, entry, GT4_SIZE);
                for n in 0..4 {
                    let v = color(m, n);
                    m.w32(prim + 4 + 12 * n as u32, v);
                }
                gpu::set_poly_gt4(m, prim);
                for n in 0..4 {
                    let (t, p) = (uv(m, n as u32), place(m, n));
                    m.w32(prim + 12 + 12 * n as u32, t);
                    m.w32(prim + 8 + 12 * n as u32, p);
                }
            } else {
                let prim = gpu::add_prim_to(m, entry, FT4_SIZE);
                let v = m.r32(s + SKY_COLOR);
                m.w32(prim + 4, v);
                gpu::set_poly_ft4(m, prim);
                for n in 0..4 {
                    let (t, p) = (uv(m, n as u32), place(m, n));
                    m.w32(prim + 12 + 8 * n as u32, t);
                    m.w32(prim + 8 + 8 * n as u32, p);
                }
            }
        } else {
            let prim = gpu::add_prim_to(m, entry, G4_SIZE);
            for n in 0..4 {
                let v = color(m, n);
                m.w32(prim + 4 + 8 * n as u32, v);
            }
            gpu::set_poly_g4(m, prim);
            for n in 0..4 {
                let p = place(m, n);
                m.w32(prim + 8 + 8 * n as u32, p);
            }
        }
    }
    if dithered {
        let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
        set_dithering_prim(c, 1, back);
    }
}

pub fn abi_build_sky(c: &mut Cpu) {
    build_sky(c);
}

/// `Sky_RenderStars` (HRZSKU.CPP, 0x800d1ec0): the stars in front of the camera and on
/// `dcache`'s screen drawn as dots (at their heights a quarter down from the screen's centre
/// line) into the ordering table's entry `index`.
pub fn render_stars(c: &mut Cpu, dcache: u32, index: u32) {
    let centre = half_height(&mut c.m);
    let stars = c.m.r32(STARS);
    set_psx_translation_to(c, [0; 3]);
    let mut k = 0u32;
    while (k as i32) < sky_u32(&mut c.m, STAR_COUNT) as i32 {
        project(c, stars.wrapping_add(8 * k));
        let (depth, place, height) = (c.gte_read(SZ3) as i32, c.gte_read(SXY2), c.gte_read(MAC2) as i32);
        let m = &mut c.m;
        let (x, y) = (place as i16 as i32, ((height >> 2) + centre) as i16 as i32);
        let on_screen = (0..=short(m, dcache.wrapping_add(0x10))).contains(&x) && (0..=short(m, dcache.wrapping_add(0x12))).contains(&y);
        if depth > 0 && on_screen {
            let entry = ot_entry(m, index);
            let prim = gpu::add_prim_to(m, entry, DOT_SIZE);
            let colors = m.r32(STAR_COLORS);
            let color = m.r32(colors.wrapping_add(4 * k));
            m.w8(prim + 3, DOT_LEN);
            m.w32(prim + 4, color);
            m.w8(prim + 7, DOT_CODE);
            m.w32(prim + 8, gpu::xy(x as u16 as u32, y as u16 as u32));
        }
        k += 1;
    }
}

pub fn abi_render_stars(c: &mut Cpu) {
    render_stars(c, c.r[4], c.r[5]);
}

/// A point's screen place as a word, and its parts.
fn place_at(m: &mut Mem, at: u32) -> (i16, i16) {
    (m.r16(at) as i16, m.r16(at + 2) as i16)
}

/// The ring's points with a depth moved by the same (`to` less the nearest point's place).
fn shift_ring(m: &mut Mem, out: u32, nearest: u32, to: (i16, i16)) {
    let (nx, ny) = place_at(m, GROUND + 4 * nearest);
    let (dx, dy) = ((to.0 as i32 - nx as i32) as u16, (to.1 as i32 - ny as i32) as u16);
    for k in 0..POINTS {
        if m.r32(DEPTHS + 4 * k) as i32 > 0 {
            let (x, y) = (m.r16(GROUND + 4 * k), m.r16(GROUND + 4 * k + 2));
            m.w16(out + 4 * k, x.wrapping_add(dx));
            m.w16(out + 4 * k + 2, y.wrapping_add(dy));
        }
    }
}

/// `Hrz_BuildHorizon` (HRZSKU.CPP, 0x800d207c): the sky as `view` sees it, behind everything: the
/// horizon's ring projected (no farther than 700 the projection's distance) and set where its
/// nearest point's ground and top really project; the band's segments (those far enough, on
/// the screen, with a picture) drawn from the ground to their pictures' heights, shaded; the
/// lightning, the moon, the stars, the dome.
pub fn build_horizon(c: &mut Cpu, view: u32) {
    let geom = c.m.r32(GEOM_SCREEN) as i32;
    let restore = if geom > MAX_GEOM_SCREEN {
        call_jal(c, SET_GEOM_SCREEN, &[MAX_GEOM_SCREEN as u32]);
        geom
    } else {
        0
    };
    set_psx_matrix(c, view.wrapping_add(VIEW_TURN));
    set_psx_translation_to(c, [0; 3]);
    let ring = c.m.r32(RING);
    rot_proj16(c, POINTS, ring, DEPTHS, GROUND);
    // the nearest point (the deepest of the first 16)
    let m = &mut c.m;
    let mut nearest = 0;
    let mut deepest = 0;
    for k in 0..SEGMENTS {
        let depth = m.r32(DEPTHS + 4 * k) as i32;
        if deepest < depth {
            deepest = depth;
            nearest = k;
        }
    }
    let point = m.r32(RING).wrapping_add(8 * nearest);
    let h = horizon(m);
    let (x, z, bottom) = (m.r16(point), m.r16(point + 4), m.r16(h + HRZ_BOTTOM));
    let top = bottom.wrapping_add(m.r16(h + HRZ_HEIGHT));
    let project_at = |c: &mut Cpu, y: u16| {
        c.gte_write(VXY0, x as u32 | (y as u32) << 16);
        c.gte_write(VXY0 + 1, z as u32);
        c.gte_command(RTPS);
        let p = c.gte_read(SXY2);
        (p as i16, (p >> 16) as i16)
    };
    let ground_at = project_at(c, bottom);
    let top_at = project_at(c, top);
    let m = &mut c.m;
    shift_ring(m, TOPS, nearest, top_at);
    shift_ring(m, GROUND, nearest, ground_at);
    interpolate_line_s_coords(m, SEGMENT_TOPS, GROUND, TOPS, HEIGHT_RATIOS, SEGMENTS as i32, true);
    interpolate_line_s_coords(m, SEGMENT_ENDS, GROUND + 4, TOPS + 4, HEIGHT_RATIOS, SEGMENTS as i32, true);
    if short(m, BAND_ON) != 0 {
        for k in 0..SEGMENTS {
            build_segment(c, k);
        }
    }
    if c.m.r32(NIGHT_LIGHTNING) != 0 && c.m.r8(SHOW_FORKS) != 0 {
        build_fork_lightning(c, DCACHE);
    }
    if c.m.r32(SKY_FLAGS) & SHOW_MOON != 0 {
        set_psx_translation_to(c, [0; 3]);
        call_jal(c, FLARE_MOON, &[MOON_POS, DCACHE]);
    }
    if c.m.r32(SKY_FLAGS) & SHOW_STARS != 0 {
        let back = c.m.r32(VIEW_OT_SIZE).wrapping_sub(2);
        render_stars(c, DCACHE, back);
    }
    if short(&mut c.m, SKY_ON) != 0 {
        build_sky(c);
    }
    if restore != 0 {
        call_jal(c, SET_GEOM_SCREEN, &[restore as u32]);
    }
}

/// The band's segment `k`: from the ground's points `k` and `k + 1` to its top at each end,
/// shaded from the top edge's colours to the bottom's, its picture mirrored or not.
fn build_segment(c: &mut Cpu, k: u32) {
    let m = &mut c.m;
    let near = |m: &mut Mem, n: u32| (m.r32(DEPTHS + 4 * n) as i32) < BAND_NEAR;
    if near(m, k) && near(m, k + 1) {
        return;
    }
    let (left_top, right_top) = (SEGMENT_TOPS + 4 * k, SEGMENT_ENDS + 4 * k);
    let (left, right) = (GROUND + 4 * k, GROUND + 4 * (k + 1));
    if off_screen(m, [left_top, right_top, right, left]) {
        return;
    }
    // the top edge stretched a little past the segment's right end
    let stretched = crate::call::with_buffer(c, 8, |c, at| {
        c.m.w32(at + 4, OVERLAP as u32);
        interpolate_line_s_coords(&mut c.m, at, left_top, right_top, at + 4, 1, false);
        c.m.r32(at)
    });
    let m = &mut c.m;
    let h = horizon(m);
    let picture = m.r8(h.wrapping_add(HRZ_SEGMENTS + k));
    let pmx = m.r32(SEGMENT_PIXMAPS + 4 * k);
    if picture == NO_PICTURE {
        return;
    }
    let entry = back_entry(m);
    let prim = gpu::add_prim_to(m, entry, GT4_SIZE);
    let top_colors = BAND_COLORS + 4 * POINTS;
    let colors = [top_colors + 4 * k, top_colors + 4 * (k + 1), BAND_COLORS + 4 * k, BAND_COLORS + 4 * (k + 1)];
    for (n, at) in colors.into_iter().enumerate() {
        let v = m.r32(at);
        m.w32(prim + 4 + 12 * n as u32, v);
    }
    gpu::set_poly_gt4(m, prim);
    for n in 0..4 {
        let uv = m.r32(pmx.wrapping_add(4 * n));
        m.w32(prim + 12 + 12 * n, uv);
    }
    let left_top = m.r32(left_top);
    let (left, right) = (m.r32(left), m.r32(right));
    let places = if picture < MIRRORED { [left_top, stretched, left, right] } else { [stretched, left_top, right, left] };
    for (n, p) in places.into_iter().enumerate() {
        m.w32(prim + 8 + 12 * n as u32, p);
    }
}

pub fn abi_build_horizon(c: &mut Cpu) {
    build_horizon(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{COMPARE_TICKS, Rng, against_original};

    /// `random`'s state (six words).
    const RANDOM_STATE: u32 = 0x8012_478C;
    /// The views' primitives' end (`Draw_gMaxPrim`) and the scratchpad's.
    const MAX_PRIM: u32 = 0x8013_EAA8;
    const PRIM_END: u32 = 0x1F80_0008;

    /// Free RAM (the race leaves it zero): the GTE's registers before and after a run, the
    /// scratchpad's drawing state after it, the ring, the pictures, the lightning's textures,
    /// the stars, lines of screen places, a view, a place; the ordering table and primitives
    /// (below the stack).
    const GTE_IN: u32 = 0x8014_4000;
    const GTE_OUT: u32 = 0x8014_4100;
    const SCRATCH_OUT: u32 = 0x8014_4200;
    const SCRATCH_SIZE: u32 = 0x300;
    const RING_BLOCK: u32 = 0x8014_0000;
    const PIXMAP_BLOCK: u32 = 0x8014_0100;
    const FORK_PIXMAPS: u32 = 0x8014_0200;
    const STAR_BLOCK: u32 = 0x8014_0300;
    const STAR_COLOR_BLOCK: u32 = 0x8014_0500;
    const LINE_FROM: u32 = 0x8014_0600;
    const LINE_TO: u32 = 0x8014_0680;
    const LINE_OUT: u32 = 0x8014_0700;
    const LINE_RATIOS: u32 = 0x8014_0780;
    const VIEW: u32 = 0x8014_0A00;
    const PLACE: u32 = 0x8014_0B00;
    const XY: u32 = 0x8014_0B80;
    const OT_BASE: u32 = 0x801E_2A00;
    const PRIMS: u32 = 0x801E_3000;
    const MAX_STARS: u32 = 48;
    const FORK_KINDS: u32 = 14;

    fn load_gte(c: &mut Cpu) {
        for r in 0..64 {
            c.gte_regs[r] = c.m.r32(GTE_IN + 4 * r as u32);
        }
    }

    /// The GTE's registers and the scratchpad's drawing state (not in the compared RAM) into it.
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

    /// A rotation (4.12) near the identity or anything, as the GTE's control words.
    fn rotation(rng: &mut Rng) -> [u32; 5] {
        let r: [i32; 9] = std::array::from_fn(|i| {
            if pick(rng, 3) == 0 {
                pick_i(rng, -4096, 4096)
            } else if i % 4 == 0 {
                4096 - pick_i(rng, 0, 300)
            } else {
                pick_i(rng, -300, 300)
            }
        });
        let r = r.map(|v| v as u16 as u32);
        [r[0] | r[1] << 16, r[2] | r[3] << 16, r[4] | r[5] << 16, r[6] | r[7] << 16, r[8]]
    }

    /// The race RAM made any track's sky: the records' flags, colours, bearings, stars and
    /// pictures any (the stars' elevations and greys never equal: the original's `divu` by 0
    /// breaks); the ring, the dome, its colours and pictures, the stars as the race leaves them or
    /// any; the band's pictures, heights and colours; the lightning (its forks, brightness,
    /// place, textures), the night's switches; one view or two; the scratchpad's drawing state
    /// (the screen, the ordering table at free RAM, the points as a last frame left them); the
    /// GTE any, its screen as the race has it.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(HRZ_SPEC, HORIZON_RECORD);
        m.w32(SKY_SPEC, SKY_RECORD);
        m.w32(COMM_MODE, (pick(rng, 3) == 0) as u32);
        let s = SKY_RECORD;
        m.w32(s + SKY_TEXTURED, [0, 1, 1, 2][pick(rng, 4) as usize]);
        m.w32(s + SKY_FLAG_BITS, rng.next() as u32 & 0xFF);
        for k in (RING_BASE_COLORS..VIEW_COLOR).step_by(4) {
            m.w32(s + k, rng.next() as u32);
        }
        m.w32(s + VIEW_COLOR, rng.next() as u32 & 0x00FF_FFFF);
        for k in [SUN_BEARING, MOON_BEARING] {
            m.w32(s + k, pick(rng, 0x1_0000));
        }
        m.w32(s + STAR_COUNT, [0, 1, pick(rng, MAX_STARS + 1), MAX_STARS][pick(rng, 4) as usize]);
        let low = pick(rng, 0x3000);
        m.w32(s + STAR_LOW, low);
        m.w32(s + STAR_HIGH, low + 1 + pick(rng, 0x1000));
        let dim = pick(rng, 200);
        m.w32(s + STAR_DIM, dim);
        m.w32(s + STAR_BRIGHT, dim + 1 + pick(rng, 300));
        m.w32(s + STAR_SEED, rng.next() as u32);
        m.w16(s + SKY_HEIGHT, pick_i(rng, -2000, 2000) as u16);
        for k in 0..16 {
            m.w8(s + SKY_PICTURES + k, pick(rng, 8) as u8);
        }
        for k in 0..RINGS {
            m.w32(s + RING_ANGLES + 4 * k, pick(rng, 0x4000));
        }
        for at in [SUN_HEIGHT, MOON_HEIGHT] {
            m.w16(at, pick_i(rng, -500, 2000) as u16);
        }
        m.w16(BAND_ON, pick(rng, 4).min(1) as u16);
        m.w16(SKY_ON, pick(rng, 4).min(1) as u16);
        // the horizon's record
        let h = HORIZON_RECORD;
        m.w32(h + HRZ_TURN, pick(rng, 0x1_0000));
        m.w16(h + HRZ_BOTTOM, pick_i(rng, -3000, 3000) as u16);
        m.w16(h + HRZ_HEIGHT, pick_i(rng, 0, 6000) as u16);
        for k in (HRZ_BASE_COLORS..HRZ_SEGMENTS).step_by(4) {
            m.w32(h + k, rng.next() as u32);
        }
        for k in 0..SEGMENTS {
            let picture = [pick(rng, 8), 8 + pick(rng, 8), NO_PICTURE as u32, pick(rng, 256)][pick(rng, 4).min(pick(rng, 4)) as usize];
            m.w8(h + HRZ_SEGMENTS + k, picture as u8);
        }
        // the pictures: 16 pixmaps any (their heights from bytes 5 and 9)
        for k in 0..16 {
            m.w32(PIXMAPS + 4 * k, PIXMAP_BLOCK + 16 * k);
            m.w32(SEGMENT_PIXMAPS + 4 * k, PIXMAP_BLOCK + 16 * pick(rng, 16));
            m.w32(HEIGHT_RATIOS + 4 * k, pick(rng, 0x1_0001));
        }
        for k in 0..64 {
            m.w32(PIXMAP_BLOCK + 4 * k, rng.next() as u32);
            m.w8(SKY_PIXMAP_INDEX + k, pick(rng, 16) as u8);
        }
        for k in 0..2 * POINTS {
            m.w32(BAND_COLORS + 4 * k, rng.next() as u32);
        }
        // the ring, the dome, the stars
        // (now and then depths from a few: ties for the nearest point, depths of 0)
        let few_depths = pick(rng, 3) == 0;
        if pick(rng, 2) == 0 {
            m.w32(RING, RING_BLOCK);
            for k in 0..POINTS {
                let z = if few_depths { [0, 4000, -4000, 8000][pick(rng, 4) as usize] } else { pick_i(rng, -32000, 32000) };
                let x = pick_i(rng, -32000, 32000);
                m.w16(RING_BLOCK + 8 * k, x as u16);
                m.w16(RING_BLOCK + 8 * k + 2, pick_i(rng, -100, 100) as u16);
                m.w16(RING_BLOCK + 8 * k + 4, z as u16);
                m.w16(RING_BLOCK + 8 * k + 6, rng.next() as u16);
            }
        }
        for k in 0..RINGS * POINTS {
            if pick(rng, 2) == 0 {
                for j in 0..3 {
                    m.w16(MESH + 8 * k + 2 * j, pick_i(rng, -1100, 1100) as u16);
                }
            }
            m.w32(SKY_COLORS + 4 * k, rng.next() as u32);
        }
        if pick(rng, 2) == 0 {
            m.w32(STARS, STAR_BLOCK);
            m.w32(STAR_COLORS, STAR_COLOR_BLOCK);
            // (now and then far out: heights past a short's on the screen)
            let reach = if pick(rng, 3) == 0 { 32000 } else { 1000 };
            for k in 0..MAX_STARS {
                for j in 0..3 {
                    m.w16(STAR_BLOCK + 8 * k + 2 * j, pick_i(rng, -reach, reach) as u16);
                }
                m.w32(STAR_COLOR_BLOCK + 4 * k, rng.next() as u32);
            }
        }
        // the lightning
        let forks = pick(rng, 7);
        m.w8(FORKS, forks as u8);
        for k in 0..16 {
            let fork = LIGHTNING + FORK_SIZE * k;
            m.w16(fork, pick_i(rng, -60, 60) as u16);
            m.w16(fork + 2, pick_i(rng, 0, 60) as u16);
            m.w8(fork + 4, pick(rng, FORK_KINDS) as u8);
        }
        m.w32(BRIGHTNESS, [pick_i(rng, -40, 220), 0, 16, 200][pick(rng, 4) as usize] as u32);
        for k in 0..3 {
            m.w16(LIGHTNING_POS + 2 * k, pick_i(rng, -3000, 12000) as u16);
        }
        for k in 0..FORK_KINDS {
            m.w32(LIGHTNING_PIXMAPS + 4 * k, FORK_PIXMAPS + 16 * (k % 8));
        }
        for k in 0..32 {
            m.w32(FORK_PIXMAPS + 4 * k, rng.next() as u32);
        }
        m.w32(NIGHT_LIGHTNING, pick(rng, 2));
        m.w8(SHOW_FORKS, pick(rng, 2) as u8);
        m.w32(SAVED_COLOR, rng.next() as u32);
        m.w32(GEOM_SCREEN, [256, 700, 701, pick(rng, 1200)][pick(rng, 4) as usize]);
        // a view's turn and a place
        for k in 0..9 {
            let v = if few_depths { if k % 4 == 0 { 0x1_0000 } else { 0 } } else { pick_i(rng, -0x1_0000, 0x1_0000) };
            m.w32(VIEW + VIEW_TURN + 4 * k, v as u32);
        }
        for k in 0..3 {
            m.w32(PLACE + 44 + 4 * k, rng.next() as u32);
        }
        for k in 0..32 {
            m.w32(LINE_FROM + 4 * k, rng.next() as u32);
            m.w32(LINE_RATIOS + 4 * k, pick_i(rng, -0x2_0000, 0x2_0000) as u32);
        }
        m.w32(XY, rng.next() as u32);
        // the drawing: the ordering table at free RAM, the screen, the last frame's points
        let ot_size = 2 + pick(rng, 60);
        m.w32(VIEW_OT_SIZE, ot_size);
        m.w32(DCACHE, OT_BASE);
        m.w32(gpu::NEXT_PRIM, PRIMS);
        m.w32(PRIM_END, PRIMS + 0x8000);
        m.w32(MAX_PRIM, PRIMS + 0x8000);
        for k in 0..64 {
            m.w32(OT_BASE + 4 * k, rng.next() as u32);
        }
        for k in (0x10..SCRATCH_SIZE).step_by(4) {
            m.w32(DCACHE + k, rng.next() as u32);
        }
        m.w16(SCREEN_W, [320, 512, pick(rng, 640)][pick(rng, 3) as usize] as u16);
        m.w16(SCREEN_H, [240, 120, pick(rng, 480)][pick(rng, 3) as usize] as u16);
        m.w32(DCACHE + OT_INDEX, pick(rng, ot_size));
        for r in 0..64 {
            m.w32(GTE_IN + 4 * r, rng.next() as u32);
        }
        if pick(rng, 4) != 0 {
            for (k, v) in rotation(rng).into_iter().enumerate() {
                m.w32(GTE_IN + 4 * (32 + k as u32), v);
            }
        }
        // the screen's set-up (as the race has it) for the projections
        m.w32(GTE_IN + 4 * (32 + 24), 160 << 16);
        m.w32(GTE_IN + 4 * (32 + 25), 120 << 16);
        m.w32(GTE_IN + 4 * (32 + 26), 256);
        // (the library's `random` state: else every case draws the same numbers)
        for k in 0..6 {
            m.w32(RANDOM_STATE + 4 * k, rng.next() as u32);
        }
    }

    /// A block of the heap's, or none.
    fn block(c: &mut Cpu, r: &mut Rng, size: u32) -> u32 {
        if pick(r, 3) == 0 { 0 } else { crate::abi::call(c, nfs4rc::f_800e5f28, &[STARS_NAME, size, 0]) }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Horizon_InterpolateLineSCoords", 0x800d_02d4, |c| { load_gte(c); nfs4rc::f_800d02d4(c); dump(c) }, |_, r| {
                vec![LINE_OUT, LINE_FROM, LINE_TO, LINE_RATIOS, [0, 1, 16, pick(r, 18)][pick(r, 4) as usize], pick(r, 2)]
            }),
            ("Sky_InitStars", 0x800d_0414, |c| { load_gte(c); nfs4rc::f_800d0414(c); dump(c) }, |c, r| {
                if pick(r, 4) != 0 {
                    c.m.w32(STARS, 0);
                }
                vec![]
            }),
            ("Sky_KillStars", 0x800d_05fc, |c| { load_gte(c); nfs4rc::f_800d05fc(c); dump(c) }, |c, r| {
                for at in [STARS, STAR_COLORS] {
                    let b = block(c, r, 64);
                    c.m.w32(at, b);
                }
                vec![]
            }),
            ("Hrz_InitSkyColor", 0x800d_0640, |c| { load_gte(c); nfs4rc::f_800d0640(c); dump(c) }, |_, _| vec![]),
            ("Hrz_InitSky", 0x800d_079c, |c| { load_gte(c); nfs4rc::f_800d079c(c); dump(c) }, |_, _| vec![]),
            ("Hrz_Init2DRing", 0x800d_0ae0, |c| { load_gte(c); nfs4rc::f_800d0ae0(c); dump(c) }, |c, _| {
                c.m.w32(RING, RING_BLOCK);
                vec![]
            }),
            ("Hrz_GetHorizonPixMap", 0x800d_0dc4, |c| { load_gte(c); nfs4rc::f_800d0dc4(c); dump(c) }, |_, _| vec![PIXMAP_BLOCK]),
            ("Hrz_InitHorizon", 0x800d_0e78, |c| { load_gte(c); nfs4rc::f_800d0e78(c); dump(c) }, |c, r| {
                if pick(r, 2) == 0 {
                    c.m.w32(STARS, 0);
                }
                vec![]
            }),
            ("Hrz_KillHorizon", 0x800d_0ee4, |c| { load_gte(c); nfs4rc::f_800d0ee4(c); dump(c) }, |c, r| {
                for (at, size) in [(RING, 136), (STARS, 64), (STAR_COLORS, 32)] {
                    let b = block(c, r, size);
                    c.m.w32(at, b);
                }
                vec![]
            }),
            ("Hrz_LightningAddFork", 0x800d_0f10, |c| { load_gte(c); nfs4rc::f_800d0f10(c); dump(c) }, |_, r| {
                vec![pick_i(r, -1, 2) as u32, pick_i(r, -3, 4) as u32, [0, 1, 2, 3, pick_i(r, -2, 5) as u32][pick(r, 5) as usize]]
            }),
            ("Hrz_CalculateLightning", 0x800d_1128, |c| { load_gte(c); nfs4rc::f_800d1128(c); dump(c) }, |_, _| vec![]),
            ("Hrz_TextureQuad", 0x800d_1164, |c| { load_gte(c); nfs4rc::f_800d1164(c); dump(c) }, |_, r| vec![XY, pick(r, FORK_KINDS), pick(r, 256), DCACHE]),
            ("Hrz_SetLightingPosInSky", 0x800d_1294, |c| { load_gte(c); nfs4rc::f_800d1294(c); dump(c) }, |_, _| vec![PLACE]),
            ("Hrz_BuildForkLightning", 0x800d_13a4, |c| { load_gte(c); nfs4rc::f_800d13a4(c); dump(c) }, |_, _| vec![DCACHE]),
            ("Hrz_LightningFlicker", 0x800d_14c0, |c| { load_gte(c); nfs4rc::f_800d14c0(c); dump(c) }, |_, r| vec![[0, 1, 2, 0xFF][pick(r, 4) as usize]]),
            ("HrzSetPsxMatrix", 0x800d_159c, |c| { load_gte(c); nfs4rc::f_800d159c(c); dump(c) }, |_, _| vec![VIEW + VIEW_TURN]),
            ("HrzSetPsxTranslation", 0x800d_167c, |c| { load_gte(c); nfs4rc::f_800d167c(c); dump(c) }, |_, _| vec![PLACE + 44]),
            ("Hrz_RotProj16", 0x800d_16cc, |c| { load_gte(c); nfs4rc::f_800d16cc(c); dump(c) }, |_, r| {
                vec![[0, 1, POINTS, pick(r, POINTS)][pick(r, 4) as usize], RING_BLOCK, DEPTHS, GROUND]
            }),
            ("Hrz_SetDitheringPrim", 0x800d_1710, |c| { load_gte(c); nfs4rc::f_800d1710(c); dump(c) }, |_, r| vec![pick(r, 2), pick(r, 60)]),
            ("Hrz_BuildSky", 0x800d_1798, |c| { load_gte(c); nfs4rc::f_800d1798(c); dump(c) }, |_, _| vec![]),
            ("Sky_RenderStars", 0x800d_1ec0, |c| { load_gte(c); nfs4rc::f_800d1ec0(c); dump(c) }, |_, r| vec![DCACHE, pick(r, 60)]),
            ("Hrz_BuildHorizon", 0x800d_207c, |c| { load_gte(c); nfs4rc::f_800d207c(c); dump(c) }, |_, _| vec![VIEW]),
        ]
    }

    #[test]
    fn hrzsku_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            // (the ticks too, but for the drawing's loops: it runs only while the race does)
            COMPARE_TICKS.set(!matches!(name, "Horizon_InterpolateLineSCoords" | "Hrz_BuildForkLightning" | "Hrz_RotProj16" | "Hrz_BuildSky" | "Sky_RenderStars" | "Hrz_BuildHorizon"));
            let args_rng = std::cell::RefCell::new(Rng(0x4A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0x4A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), gte_port);
        }
    }
}
