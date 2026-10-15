//! The cars as the original decides to draw them (R3DCar_InsertCarFacetII recorded instead of
//! drawn), and the light glows and smoke it builds around them.

use super::*;

/// A light's glow as the original built it this frame (screen-space primitives) and the
/// world point it belongs to (PS1 16.16), or the car (Car_tObj) it is on.
#[derive(Clone, Debug)]
pub struct Glow {
    pub anchor: [i32; 3],
    pub car: Option<u32>,
    pub cmds: Vec<OverlayCmd>,
    /// A smoke or dust puff (Sfx_BuildSouffleFacet), sorted at its own depth; otherwise a
    /// light's glow.
    pub smoke: bool,
}

pub(super) fn smoke_marker(c: &mut Cpu, addr: u32) -> bool {
    if addr == SFX_BUILD_SOUFFLE_FACET {
        // Souffle_tISouffle: its position (coorddef) at +12
        let p = c.r[5] + 12;
        let anchor = [c.m.r32(p) as i32, c.m.r32(p + 4) as i32, c.m.r32(p + 8) as i32];
        let at = c.m.r32(0x1F80_0004);
        GLARE_MARKS.with_borrow_mut(|m| m.push((at, anchor)));
    }
    false
}

pub(super) fn glare_marker(c: &mut Cpu, addr: u32) -> bool {
    if addr == FLARE_HALO || addr == FLARE_HALO2 {
        let p = c.r[7];
        let anchor = [c.m.r32(p) as i32, c.m.r32(p + 4) as i32, c.m.r32(p + 8) as i32];
        let at = c.m.r32(0x1F80_0004);
        GLARE_MARKS.with_borrow_mut(|m| m.push((at, anchor)));
    }
    false
}

/// Inner hook for R3DCar_InsertCarFacetII(car): record what the original decided to draw
/// (see [`CarFacet::parse`]) instead of building primitives.
pub(super) fn record_car_facet(c: &mut Cpu, addr: u32) -> bool {
    if HALO_PASS.get() {
        // the original's car drawing for its halos: the body's primitives are skipped
        if CAR_BODY_DRAWING.contains(&addr) {
            c.r[2] = 0;
            return true;
        }
        return false;
    }
    if addr == R3DCAR_VISIBILITY && !VISIBILITY_PASS.get() {
        // the original's test with the view's x row scaled down: the cone it allows is as
        // much wider; the depth row (distance and level of detail) is left alone
        let (car, view) = (c.r[4], c.r[5]);
        let row: [u32; 3] = std::array::from_fn(|i| c.m.r32(view + 20 + 4 * i as u32));
        for (i, v) in row.iter().enumerate() {
            c.m.w32(view + 20 + 4 * i as u32, (*v as i32 / CAR_CONE_WIDTH) as u32);
        }
        VISIBILITY_PASS.set(true);
        let depth = c.call_nested(R3DCAR_VISIBILITY, &[car, view]);
        VISIBILITY_PASS.set(false);
        for (i, v) in row.iter().enumerate() {
            c.m.w32(view + 20 + 4 * i as u32, *v);
        }
        c.r[2] = depth;
        return true;
    }
    if addr != R3DCAR_INSERT_CAR_FACET_II {
        return false;
    }
    let car = c.r[4];
    // not drawn this frame (R3DCar_Visibilty): the original returns at once (its
    // R3DCar_ObjectVisible is still another car's)
    if (c.m.r16(car + CAR_DETAIL) as i16) < 0 {
        c.r[2] = 0;
        return true;
    }
    let light = c.call_nested(BWORLDSM_QUAD_LIGHT, &[car + 8]) & 0xFF_FFFF;
    let mut rec = vec![car, light, c.m.r32(SHADOW_ON)];
    // Newton_CalcRealShadowCoordinates left the shadow's corners at car + 488; keep them
    // relative to the car's position in x and z (the car is drawn at an interpolated pose)
    let pos = [c.m.r32(car + 0xA0), 0, c.m.r32(car + 0xA8)];
    rec.extend((0..12).map(|i| c.m.r32(car + 488 + 4 * i).wrapping_sub(pos[i as usize % 3])));
    rec.push(c.m.r32(SHADOW_COLOUR) & 0xFF_FFFF);
    rec.extend((0..57).map(|i| c.m.r8(R3DCAR_OBJECT_VISIBLE + i) as u32));
    // DrawC_PrimStart sets up the car's drawing (light level, reflection maps chosen from the
    // track position, their colour and scroll) in the car cache on the scratchpad
    let (r, g, b) = (light & 0xFF, (light >> 8) & 0xFF, (light >> 16) & 0xFF);
    let level = ((r + g + b) / 3).max(24);
    c.m.w32(CAR_CACHE + 140, level * 0x01_0101);
    let ok = c.call_nested(DRAWC_PRIM_START, &[R3DCAR_CENTER, car, level, CAR_CACHE]) as i32 >= 0;
    rec.push(ok as u32);
    rec.extend((0..8).map(|i| c.m.r32(CAR_CACHE + 100 + 4 * i)));
    rec.extend([c.m.r32(CAR_CACHE + 132), c.m.r32(CAR_CACHE + 144), c.m.r16(R3DCAR_CENTER + 0x14) as u32]);
    // DrawC_PrimStart's light and damage codes for this car (per texture, see CarFacet::codes)
    rec.extend((0..26).map(|i| c.m.r32(LIGHT_CODES + 4 * i)));
    c.capture.extend(rec);
    // then the original itself, which adds the lights' halos (DrawC_PrimHalo: brake lights,
    // headlights) to the primitive buffer, everything else skipped
    let start = c.m.r32(0x1F80_0004);
    HALO_PASS.set(true);
    c.call_nested(R3DCAR_INSERT_CAR_FACET_II, &[car]);
    HALO_PASS.set(false);
    let end = c.m.r32(0x1F80_0004);
    if end > start && end - start < 0x1_0000 {
        HALO_RANGES.with_borrow_mut(|r| r.push((start, end, car)));
    }
    c.r[2] = 0;
    true
}

/// What the game decided to draw of one car this frame.
#[derive(Clone, Debug, Default)]
pub struct CarFacet {
    /// `Car_tObj` address.
    pub car: u32,
    /// `R3DCar_ObjectVisible`: the model parts drawn.
    pub parts: Vec<u8>,
    /// The track's light at the car (BWorldSm_QuadLight, 0xBBGGRR).
    pub light: u32,
    /// The ground shadow's corners (16.16: x and z relative to the car's position, y the
    /// world height) and colour, when drawn.
    pub shadow: Option<([[i32; 3]; 4], [u8; 3])>,
    /// The reflection maps (DrawC_PrimStart), when the car is drawn.
    pub env: Option<CarEnv>,
    /// DrawC_PrimStart's light and damage codes, per GEO texture (see `LIGHT_CODES`).
    pub codes: Vec<u16>,
}

/// A car's reflection mapping for this frame: two of the track's reflection maps
/// (Track_gReflectionMaps pixmaps: u, v, clut, u, v, tpage, ...), the second one's scroll,
/// their colour (0xBBGGRR) and the camera's yaw around the car (4096 per turn).
#[derive(Clone, Copy, Debug, Default)]
pub struct CarEnv {
    pub a: Option<[u32; 4]>,
    pub b: Option<[u32; 4]>,
    pub scroll: u32,
    pub colour: u32,
    pub yaw: i16,
}

impl CarFacet {
    const WORDS: usize = 3 + 12 + 1 + 57 + 1 + 8 + 3 + 26;

    /// Records left by `record_car_facet`.
    pub(super) fn parse(w: &[u32]) -> Vec<Self> {
        w.chunks_exact(Self::WORDS)
            .map(|r| Self {
                car: r[0],
                light: r[1],
                shadow: (r[2] != 0).then(|| {
                    (std::array::from_fn(|k| std::array::from_fn(|i| r[3 + 3 * k + i] as i32)), std::array::from_fn(|i| (r[15] >> (8 * i)) as u8))
                }),
                parts: r[16..73].iter().map(|&p| p as u8).collect(),
                env: (r[73] != 0).then(|| {
                    let map = |o: usize| (r[o] != 0).then(|| [r[o], r[o + 1], r[o + 2], r[o + 3]]);
                    CarEnv { a: map(74), b: map(78), scroll: r[82], colour: r[83] & 0xFF_FFFF, yaw: r[84] as u16 as i16 }
                }),
                codes: r[85..111].iter().flat_map(|&w| [w as u16, (w >> 16) as u16]).collect(),
            })
            .collect()
    }
}
