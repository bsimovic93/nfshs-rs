//! TEXTUREPROCESS.CPP: colours by distance and the track's fog. The depth cue's palettes (each
//! of a texture's 16 colours pulled toward the fog's colour, 16 steps by distance, put in VRAM
//! next to the texture's own), the fog's distance along the track (keys by slice, read from the
//! track's `.fog` file for the time and weather, interpolated at each player's camera), and the
//! world's vertex colours tinted for some tracks' time and weather in two-player races.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call_jal, ticks, with_buffer};
use crate::fixed::{div, fixedmult};
use crate::globals::{GAME_SETUP, NUM_SLICES};
use ps1rt::{Cpu, Mem};

/// The track's spec (`TrackSpec_gSpec`): the fog's contrast (+0x10), colour (+0x14: r, g, b),
/// and its distance now (+0x18).
const SPEC: u32 = 0x8012_456C;
const FOG_CONTRAST: u32 = SPEC + 0x10;
const FOG_COLOUR: u32 = SPEC + 0x14;
const FOG_DISTANCE: u32 = SPEC + 0x18;

/// The depth cue's palettes: their ids by system and step (`gClutDepth`, a short each, 16 a
/// system), the next system (`TP_gZPaletteSystem`), the steps (`gZDepth`).
const CLUT_DEPTH: u32 = 0x8012_236C;
const PALETTE_SYSTEM: u32 = 0x8013_D83C + 5660;
const Z_DEPTH: u32 = 0x8013_D83C + 5656;
const STEPS: u32 = 16;
/// A colour (15 bits: blue, green, red from the top; bit 15 the transparency flag); 0 is
/// transparent black and stays so; blue never darker than 8.
const SEMI_TRANSPARENT: u16 = 0x8000;
const BLUE_MIN: i32 = 8;

/// The fog's keys (`FogKey`, 12 bytes: the distance, the slice, the next and previous keys, a
/// ring), their pool (`Fog_gBuf`, 32, `openkeys` marking the free), the ring's first
/// (`Fog_gHeadKey`), how many (`Fog_gNumKeys`), each player's key now (`Fog_gCurrentKey`),
/// each player's camera's road position (`fogslicePos`, `BWorldSm_Pos`, 132 bytes).
const KEY_SLICE: u32 = 2;
const KEY_NEXT: u32 = 4;
const KEY_PREV: u32 = 8;
const KEY_SIZE: u32 = 12;
const KEY_POOL: u32 = 0x8012_436C;
const KEYS_FREE: u32 = 0x8012_44EC;
const POOL_SIZE: u32 = 32;
const NUM_KEYS: u32 = 0x8013_D83C + 5664;
const CURRENT_KEY: u32 = 0x8013_D83C + 5668;
const HEAD_KEY: u32 = 0x8013_D83C + 5676;
const SLICE_POS: u32 = 0x8013_D83C + 5680;
const POS_SIZE: u32 = 132;
/// The camera (`gCView`) whose place the fog follows.
const CAMERA_PLACE: u32 = 0x8011_8234 + 8;

/// The fog files: the name built in `fogstrspc` ("%sTr%02d%s": the tracks' directory, the
/// track, the time and weather's suffix: "S.fog" both, "N.fog" night, "W.fog" weather,
/// ".fog"), the name of the camera places' block.
const FOG_NAME: u32 = 0x8013_FB64;
const FOG_FORMAT: u32 = 0x8005_725C;
const TRACKS_PATH: u32 = crate::globals::PATHS + 0x18;
const SUFFIX_NIGHT_WEATHER: u32 = 0x8013_EE2C;
const SUFFIX_NIGHT: u32 = 0x8013_EE34;
const SUFFIX_WEATHER: u32 = 0x8013_EE3C;
const SUFFIX_PLAIN: u32 = 0x8013_EE44;
const SLICE_POS_NAME: u32 = 0x8013_EE4C;

/// The world's vertex colours (`Chunk_lightTable`, `CVECTOR`s, `Chunk_numLight` of them).
const LIGHT_TABLE: u32 = 0x8013_DB0C;
const NUM_LIGHT: u32 = 0x8013_E7D4;
const SPLIT_SCREEN: i32 = 1;

/// Functions called: the GPU's `LoadImage(rect, data)` and `DrawSync`, `Texture_GetClutId(big,
/// &x, &y)`, the road's `BWorldSm_FindClosestQuadRez(pt, pos, rez)` and `SetSlice(slice, pos)`,
/// the library's `sprintf`, `loadfileadr(name, 0)`, `reservememadr(name, size, 0)`, `purgememadr`,
/// `memset`.
const LOAD_IMAGE: u32 = 0x800E_E3EC;
const DRAW_SYNC: u32 = 0x800E_E1D8;
const GET_CLUT_ID: u32 = 0x800E_0544;
const FIND_CLOSEST_QUAD_REZ: u32 = 0x8008_04E8;
const SET_SLICE: u32 = 0x8007_F788;
const SPRINTF: u32 = 0x800E_50A8;
const LOAD_FILE: u32 = 0x800E_63D4;
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const MEMSET: u32 = 0x800E_4F04;

/// `TextureProcess_TransColorCheck` (TEXTUREPROCESS.CPP, 0x800e1490): whether any of the `n`
/// colours at `clut` is semi-transparent (above bare 0x8000).
pub fn trans_color_check(c: &mut Cpu, clut: u32, n: u32) -> bool {
    let m = &mut c.m;
    let any = (0..n).fold(false, |any, k| any | (m.r16(clut.wrapping_add(2 * k)) > SEMI_TRANSPARENT));
    // (the original's loop: a tick a colour)
    ticks(c, n);
    any
}

pub fn abi_trans_color_check(c: &mut Cpu) {
    c.r[2] = trans_color_check(c, c.r[4], c.r[5]) as u32;
}

/// A colour's channel (0..255) pulled toward the fog's by `f`, kept in 0..255.
fn toward(channel: i32, fog: i32, f: i32) -> i32 {
    channel.wrapping_sub(fixedmult(channel.wrapping_sub(fog), f)).clamp(0, 255)
}

/// `TextureProcess_ColorClut` (TEXTUREPROCESS.CPP, 0x800e14c4): the palette of `count`
/// colours at `src` at depth step `step` of `steps` (pulled toward the fog's colour by the
/// fog's contrast times the step's share), put in VRAM at (x, y).
pub fn color_clut(c: &mut Cpu, step: i32, steps: i32, src: u32, count: i32, x: u32, y: u32) {
    let n = count.max(0) as u32;
    with_buffer(c, 2 * n + 8, |c, buffer| {
        let m = &mut c.m;
        let f = div(m.r32(FOG_CONTRAST).wrapping_mul(step as u32) as i32, steps.wrapping_sub(1));
        for k in 0..n {
            let colour = m.r16(src.wrapping_add(2 * k));
            let out = if colour == 0 {
                0
            } else {
                let red = toward(((colour & 0x1F) << 3) as i32, m.r8(FOG_COLOUR) as i32, f);
                let green = toward(((colour >> 2) & 0xF8) as i32, m.r8(FOG_COLOUR + 1) as i32, f);
                let blue = toward(((colour >> 7) & 0xF8) as i32, m.r8(FOG_COLOUR + 2) as i32, f).max(BLUE_MIN);
                (colour & SEMI_TRANSPARENT) | (((blue >> 3) & 0x1F) << 10) as u16 | (((green >> 3) & 0x1F) << 5) as u16 | ((red >> 3) & 0x1F) as u16
            };
            m.w16(buffer + 2 * k, out);
        }
        // (a tick a colour)
        ticks(c, n);
        let m = &mut c.m;
        // the rectangle: x, y, the count wide, one high
        let rect = buffer + 2 * n;
        m.w16(rect, x as u16);
        m.w16(rect + 4, count as u16);
        m.w16(rect + 2, y as u16);
        m.w16(rect + 6, 1);
        call_jal(c, LOAD_IMAGE, &[rect, buffer]);
    });
}

pub fn abi_color_clut(c: &mut Cpu) {
    let sp = c.r[29];
    let (x, y) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
    color_clut(c, c.r[4] as i32, c.r[5] as i32, c.r[6], c.r[7] as i32, x, y);
}

/// `TextureProcess_DepthColorCluts` (TEXTUREPROCESS.CPP, 0x800e16cc): the depth cue's 16
/// palettes for the `n` colours at `clut` (a big texture's place past 16 colours), each in
/// VRAM where `Texture_GetClutId` finds room, its id kept in the next system's row; that
/// system's number (then the next).
pub fn depth_color_cluts(c: &mut Cpu, clut: u32, n: i32) -> u32 {
    call_jal(c, DRAW_SYNC, &[0]);
    for step in 0..STEPS {
        let (x, y) = with_buffer(c, 8, |c, at| {
            call_jal(c, GET_CLUT_ID, &[(n >= 17) as u32, at, at + 4]);
            (c.m.r32(at), c.m.r32(at + 4))
        });
        let m = &mut c.m;
        let system = m.r32(PALETTE_SYSTEM);
        let id = ((y as u16) << 6) | ((x as i32 >> 4) & 0x3F) as u16;
        m.w16(CLUT_DEPTH.wrapping_add(system << 5).wrapping_add(step * 2), id);
        color_clut(c, step as i32, STEPS as i32, clut, n, x, y);
        c.tick();
    }
    let system = c.m.r32(PALETTE_SYSTEM);
    c.m.w32(PALETTE_SYSTEM, system.wrapping_add(1));
    system
}

pub fn abi_depth_color_cluts(c: &mut Cpu) {
    c.r[2] = depth_color_cluts(c, c.r[4], c.r[5] as i32);
}

fn key_slice(m: &mut Mem, key: u32) -> i32 {
    m.r16(key.wrapping_add(KEY_SLICE)) as i16 as i32
}

fn key_distance(m: &mut Mem, key: u32) -> i32 {
    m.r16(key) as i16 as i32
}

/// `Fog_CheckRange` (TEXTUREPROCESS.CPP, 0x800e17a4): `key` if `slice` is between it and the
/// next key (round the track's end when the next is before it), else 0.
pub fn check_range(m: &mut Mem, slice: i32, key: u32) -> u32 {
    let next = m.r32(key.wrapping_add(KEY_NEXT));
    let (from, to) = (key_slice(m, key), key_slice(m, next));
    let inside = if to < from { slice >= from || slice < to } else { slice >= from && slice < to };
    if inside { key } else { 0 }
}

pub fn abi_check_range(c: &mut Cpu) {
    c.r[2] = check_range(&mut c.m, c.r[4] as i32, c.r[5]);
}

/// `Fog_FindKey` (TEXTUREPROCESS.CPP, 0x800e17f8): the key whose stretch holds `slice`,
/// walking the ring from `from` (0: the first), back or on as the slice is before it or not.
pub fn find_key(c: &mut Cpu, slice: i32, from: u32) -> u32 {
    let m = &mut c.m;
    let mut key = if from == 0 { m.r32(HEAD_KEY) } else { from };
    if check_range(m, slice, key) != 0 {
        return key;
    }
    let link = if slice < key_slice(m, key) { KEY_PREV } else { KEY_NEXT };
    loop {
        key = c.m.r32(key.wrapping_add(link));
        if check_range(&mut c.m, slice, key) != 0 {
            return key;
        }
        // (the walk's branch back: a tick a step but the first)
        c.tick();
    }
}

pub fn abi_find_key(c: &mut Cpu) {
    c.r[2] = find_key(c, c.r[4] as i32, c.r[5]);
}

/// `Fog_AllocKey` (TEXTUREPROCESS.CPP, 0x800e188c): the first free key of the pool, taken (0
/// when none).
pub fn alloc_key(c: &mut Cpu) -> u32 {
    for k in 0..POOL_SIZE {
        if c.m.r32(KEYS_FREE + 4 * k) == 1 {
            c.m.w32(KEYS_FREE + 4 * k, 0);
            return KEY_POOL + KEY_SIZE * k;
        }
        // (a tick a taken key but the last)
        if k + 1 < POOL_SIZE {
            c.tick();
        }
    }
    0
}

pub fn abi_alloc_key(c: &mut Cpu) {
    c.r[2] = alloc_key(c);
}

/// `Fog_AddKey` (TEXTUREPROCESS.CPP, 0x800e18d8): a key of `distance` at `slice` put into the
/// ring after the one whose stretch holds the slice (the first, a ring of itself); not counted
/// when a key already has that slice (the new one is left taken, unlinked).
pub fn add_key(c: &mut Cpu, slice: i32, distance: i32) {
    let key = alloc_key(c);
    let m = &mut c.m;
    let head = m.r32(HEAD_KEY);
    m.w32(key.wrapping_add(KEY_PREV), 0);
    m.w32(key.wrapping_add(KEY_NEXT), 0);
    m.w16(key.wrapping_add(KEY_SLICE), slice as u16);
    m.w16(key, distance as u16);
    if head == 0 {
        m.w32(HEAD_KEY, key);
        m.w32(key.wrapping_add(KEY_NEXT), key);
        m.w32(key.wrapping_add(KEY_PREV), key);
    } else {
        let at = if m.r32(head.wrapping_add(KEY_NEXT)) == head { head } else { find_key(c, slice, 0) };
        let m = &mut c.m;
        if key_slice(m, at) == slice {
            return;
        }
        let next = m.r32(at.wrapping_add(KEY_NEXT));
        m.w32(at.wrapping_add(KEY_NEXT), key);
        m.w32(key.wrapping_add(KEY_PREV), at);
        m.w32(key.wrapping_add(KEY_NEXT), next);
        m.w32(next.wrapping_add(KEY_PREV), key);
    }
    let n = c.m.r32(NUM_KEYS);
    c.m.w32(NUM_KEYS, n.wrapping_add(1));
}

pub fn abi_add_key(c: &mut Cpu) {
    add_key(c, c.r[4] as i32, c.r[5] as i32);
}

/// `Fog_Update` (TEXTUREPROCESS.CPP, 0x800e198c): the fog's distance for player `p`'s camera
/// (with more than one key): its slice found, the key there (looked for from the player's
/// last), the distance between that key's and the next's by the slice (round the track's end
/// where the stretch crosses it).
pub fn update(c: &mut Cpu, p: u32) {
    if c.m.r32(NUM_KEYS) == 1 {
        return;
    }
    let pos = c.m.r32(SLICE_POS).wrapping_add(p.wrapping_mul(POS_SIZE));
    call_jal(c, FIND_CLOSEST_QUAD_REZ, &[CAMERA_PLACE, pos, 1]);
    let m = &mut c.m;
    let pos = m.r32(SLICE_POS).wrapping_add(p.wrapping_mul(POS_SIZE));
    let mut slice = m.r16(pos) as i16 as i32;
    let current = CURRENT_KEY.wrapping_add(p << 2);
    let from = m.r32(current);
    let key = find_key(c, slice, from);
    let m = &mut c.m;
    m.w32(current, key);
    let next = m.r32(key.wrapping_add(KEY_NEXT));
    let (near, far) = (key_distance(m, key), key_distance(m, next));
    let distance = if near == far {
        near
    } else {
        let start = key_slice(m, key);
        let mut end = key_slice(m, next);
        if end < start {
            let slices = NUM_SLICES.get(m);
            end = end.wrapping_add(slices);
            if slice >= 0 && slice < start {
                slice = slice.wrapping_add(slices);
            }
        }
        let along = slice.wrapping_sub(start).wrapping_mul(far.wrapping_sub(near));
        near.wrapping_add(div(along, end.wrapping_sub(start)))
    };
    m.w32(FOG_DISTANCE, distance as u32);
}

pub fn abi_update(c: &mut Cpu) {
    update(c, c.r[4]);
}

/// `Fog_MakeTrackPathName` (TEXTUREPROCESS.CPP, 0x800e1ad0): the track's file with `suffix`
/// (the name's buffer).
pub fn make_track_path_name(c: &mut Cpu, suffix: u32) -> u32 {
    let m = &mut c.m;
    let (path, track) = (m.r32(TRACKS_PATH), GAME_SETUP.track(m) as u32 & 0xF);
    call_jal(c, SPRINTF, &[FOG_NAME, FOG_FORMAT, path, track, suffix]);
    FOG_NAME
}

pub fn abi_make_track_path_name(c: &mut Cpu) {
    c.r[2] = make_track_path_name(c, c.r[4]);
}

/// `Fog_ReadFogKeys` (TEXTUREPROCESS.CPP, 0x800e1b20): the fog's keys from the track's file for
/// the race's time and weather (a count, then slice and distance pairs; fewer than 32), freed
/// after; whether there was one (a file with too many is kept, unread).
pub fn read_fog_keys(c: &mut Cpu) -> bool {
    let m = &mut c.m;
    let (night, weather) = (GAME_SETUP.time(m) != 0, GAME_SETUP.weather(m) != 0);
    let suffix = match (night, weather) {
        (true, true) => SUFFIX_NIGHT_WEATHER,
        (true, false) => SUFFIX_NIGHT,
        (false, true) => SUFFIX_WEATHER,
        (false, false) => SUFFIX_PLAIN,
    };
    let name = make_track_path_name(c, suffix);
    let data = call_jal(c, LOAD_FILE, &[name, 0]);
    read_keys(c, data)
}

/// [`read_fog_keys`]' keys from the file loaded at `data` (0: none).
fn read_keys(c: &mut Cpu, data: u32) -> bool {
    if data == 0 {
        return false;
    }
    let n = c.m.r32(data);
    if n >= POOL_SIZE {
        return false;
    }
    for k in 0..n {
        let (slice, distance) = (c.m.r32(data + 4 + 8 * k), c.m.r32(data + 8 + 8 * k));
        add_key(c, slice as i32, distance as i32);
        c.tick();
    }
    call_jal(c, PURGE, &[data]);
    true
}

pub fn abi_read_fog_keys(c: &mut Cpu) {
    c.r[2] = read_fog_keys(c) as u32;
}

/// `Fog_InitFogTriggers` (TEXTUREPROCESS.CPP, 0x800e1c38): the keys emptied and read (without a
/// file, one key of the spec's distance at slice 0), each player at the first, a road position
/// for each player's camera (two with two players).
pub fn init_fog_triggers(c: &mut Cpu) {
    let m = &mut c.m;
    m.w32(NUM_KEYS, 0);
    m.w32(HEAD_KEY, 0);
    for k in (0..POOL_SIZE).rev() {
        m.w32(KEYS_FREE + 4 * k, 1);
    }
    ticks(c, POOL_SIZE - 1);
    if !read_fog_keys(c) {
        let distance = c.m.r32(FOG_DISTANCE);
        add_key(c, 0, distance as i32);
    }
    let m = &mut c.m;
    let head = m.r32(HEAD_KEY);
    m.w32(CURRENT_KEY, head);
    m.w32(CURRENT_KEY + 4, head);
    let players = if GAME_SETUP.comm_mode(m) == SPLIT_SCREEN { 2 } else { 1 };
    let pos = call_jal(c, RESERVE, &[SLICE_POS_NAME, players * POS_SIZE, 0]);
    c.m.w32(SLICE_POS, pos);
    for p in 0..players {
        let pos = c.m.r32(SLICE_POS).wrapping_add(p * POS_SIZE);
        call_jal(c, SET_SLICE, &[0, pos]);
    }
    ticks(c, players - 1);
}

pub fn abi_init_fog_triggers(c: &mut Cpu) {
    init_fog_triggers(c);
}

/// `Fog_DeInitFogTriggers` (TEXTUREPROCESS.CPP, 0x800e1d1c): the cameras' road positions freed.
pub fn de_init_fog_triggers(c: &mut Cpu) {
    let pos = c.m.r32(SLICE_POS);
    call_jal(c, PURGE, &[pos]);
}

pub fn abi_de_init_fog_triggers(c: &mut Cpu) {
    de_init_fog_triggers(c);
}

/// `TextureProcess_Init` (TEXTUREPROCESS.CPP, 0x800e1d40): 16 depth steps, no palette systems
/// yet.
pub fn init(m: &mut Mem) {
    m.w32(Z_DEPTH, STEPS);
    m.w32(PALETTE_SYSTEM, 0);
}

pub fn abi_init(c: &mut Cpu) {
    init(&mut c.m);
}

/// `CV_ProcessWorldColors_FINAL` (TEXTUREPROCESS.CPP, 0x800e1d54): every vertex colour of the
/// world pulled toward `colour` by `f`, then `add` added, each channel kept in 0..255.
pub fn process_world_colors(c: &mut Cpu, f: i32, colour: u32, add: i32) {
    let mut k = 0;
    while (k as i32) < c.m.r32(NUM_LIGHT) as i32 {
        // (a tick a colour)
        c.tick();
        let m = &mut c.m;
        let at = m.r32(LIGHT_TABLE).wrapping_add(4 * k);
        let (r, g, b) = (m.r8(at) as i32, m.r8(at + 1) as i32, m.r8(at + 2) as i32);
        let r = toward(r, m.r8(colour) as i32, f);
        let g = toward(g, m.r8(colour.wrapping_add(1)) as i32, f);
        let b = toward(b, m.r8(colour.wrapping_add(2)) as i32, f);
        for (j, v) in [r, g, b].into_iter().enumerate() {
            let at = m.r32(LIGHT_TABLE).wrapping_add(4 * k);
            m.w8(at + j as u32, v.wrapping_add(add).clamp(0, 255) as u8);
        }
        k += 1;
    }
}

pub fn abi_process_world_colors(c: &mut Cpu) {
    process_world_colors(c, c.r[4] as i32, c.r[5], c.r[6] as i16 as i32);
}

/// The tint of the world's colours for a track (two players only, but for the eighth at
/// night): its pull, the colour pulled toward (r, g, b) and the amount added.
fn track_tint(split: bool, track: i32, weather: i32, night: i32) -> (i32, [u8; 3], i32) {
    let (clear_night, wet_night) = (weather == 0 && night == 1, weather == 1 && night == 1);
    if !split {
        return if track == 8 && clear_night { (0x6800, [0; 3], 22) } else { (0, [0; 3], 0) };
    }
    match track {
        2 if wet_night => (0xFFFF_6380_u32 as i32, [0, 16, 0], 16),
        3 if clear_night => (0xFFFF_0C00_u32 as i32, [0; 3], 0),
        3 if wet_night => (-0x7400, [16, 0, 0], 16),
        4 if clear_night => (-0x7800, [0; 3], 0),
        4 if wet_night => (-0x1_0000, [0, 24, 0], 16),
        6 if clear_night => (0xFFFF_69F0_u32 as i32, [0; 3], 49),
        7 if clear_night => (0xFFFF_4C00_u32 as i32, [0; 3], 20),
        8 if clear_night => (0x6800, [0; 3], 22),
        10 if wet_night => (-0x5400, [0; 3], 0),
        _ => (0, [0; 3], 0),
    }
}

/// `CV_ColorTracks` (TEXTUREPROCESS.CPP, 0x800e1f14): the world's colours tinted for `track`'s
/// `weather` and `night`.
pub fn color_tracks(c: &mut Cpu, track: i32, weather: i32, night: i32) {
    let split = GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN;
    let (f, rgb, add) = track_tint(split, track, weather, night);
    with_buffer(c, 4, |c, colour| {
        call_jal(c, MEMSET, &[colour, 0, 4]);
        for (k, v) in rgb.into_iter().enumerate() {
            if v != 0 {
                c.m.w8(colour + k as u32, v);
            }
        }
        process_world_colors(c, f, colour, add);
    });
}

pub fn abi_color_tracks(c: &mut Cpu) {
    color_tracks(c, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32);
}

/// `CV_InitColorVertices` (TEXTUREPROCESS.CPP, 0x800e211c): the world's colours tinted for the
/// race's track, weather and time.
pub fn init_color_vertices(c: &mut Cpu) {
    let m = &mut c.m;
    let (track, weather, night) = (GAME_SETUP.track(m) & 0xF, GAME_SETUP.weather(m), GAME_SETUP.time(m));
    color_tracks(c, track, weather, night);
}

pub fn abi_init_color_vertices(c: &mut Cpu) {
    init_color_vertices(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM (the race leaves it zero): a palette, a colour, two road positions for the
    /// cameras, a fog file.
    const CLUT: u32 = 0x8014_0000;
    const COLOUR: u32 = 0x8014_0080;
    const POSITIONS: u32 = 0x8014_0100;
    const FILE: u32 = 0x8014_0400;

    /// The race RAM's fog: a ring of 2 to 8 keys (by slice, round the track), some of the pool
    /// free, each player at one of them; the fog's contrast, colour and distance; a palette
    /// (transparent, semi-transparent and plain colours), the palette system; the world's
    /// colours; the setup (two players, the track, time and weather); a fog file.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let slices = NUM_SLICES.get(m) as u32;
        let n = 2 + pick(rng, 7);
        let mut at: Vec<u32> = Vec::new();
        while (at.len() as u32) < n {
            let s = pick(rng, slices);
            if !at.contains(&s) {
                at.push(s);
            }
        }
        at.sort();
        // (the keys in a shuffled order in the pool, the rest free or not)
        let mut slots: Vec<u32> = (0..POOL_SIZE).collect();
        for k in (1..slots.len()).rev() {
            slots.swap(k, pick(rng, k as u32 + 1) as usize);
        }
        let keys: Vec<u32> = slots[..n as usize].iter().map(|&k| KEY_POOL + KEY_SIZE * k).collect();
        for k in 0..POOL_SIZE {
            m.w32(KEYS_FREE + 4 * k, (pick(rng, 3) != 0) as u32);
        }
        for (i, &key) in keys.iter().enumerate() {
            m.w32(KEYS_FREE + 4 * slots[i], 0);
            m.w16(key, [pick(rng, 3000), 1235, 1235][pick(rng, 3) as usize] as u16);
            m.w16(key + KEY_SLICE, at[i] as u16);
            m.w32(key + KEY_NEXT, keys[(i + 1) % keys.len()]);
            m.w32(key + KEY_PREV, keys[(i + keys.len() - 1) % keys.len()]);
        }
        m.w32(NUM_KEYS, n);
        m.w32(HEAD_KEY, keys[[0, pick(rng, n)][pick(rng, 2) as usize] as usize]);
        for p in 0..2 {
            m.w32(CURRENT_KEY + 4 * p, [0, keys[pick(rng, n) as usize]][pick(rng, 4).min(1) as usize]);
        }
        m.w32(FOG_CONTRAST, [0x6400, 0x1_0000, rng.next() as u32 & 0x1_FFFF][pick(rng, 3) as usize]);
        m.w32(FOG_COLOUR, rng.next() as u32);
        m.w32(FOG_DISTANCE, pick(rng, 3000));
        for k in 0..32 {
            let v = rng.next() as u16;
            m.w16(CLUT + 2 * k, [0, v, v | 0x8000, 0x8000][pick(rng, 4) as usize]);
        }
        m.w32(COLOUR, rng.next() as u32);
        m.w32(PALETTE_SYSTEM, pick(rng, 255));
        let table = m.r32(LIGHT_TABLE);
        for k in 0..45 * 4 {
            m.w8(table + k, rng.next() as u8);
        }
        GAME_SETUP.set_comm_mode(m, pick(rng, 2) as i32);
        GAME_SETUP.set_track(m, pick(rng, 12) as i32 | (pick(rng, 2) << 4) as i32);
        GAME_SETUP.set_weather(m, pick(rng, 2) as i32);
        GAME_SETUP.set_time(m, pick(rng, 2) as i32);
        // a fog file: a count (now and then too many), slice and distance pairs
        m.w32(FILE, [1 + pick(rng, 6), 0, 32, pick(rng, 40)][pick(rng, 4) as usize]);
        for k in 0..64 {
            m.w32(FILE + 4 + 4 * k, if k % 2 == 0 { pick(rng, slices) } else { pick(rng, 3000) });
        }
    }

    /// Both players' cameras' road positions (the race has one) at free RAM, each a copy of
    /// the race's.
    fn two_positions(c: &mut Cpu) {
        let m = &mut c.m;
        let pos = m.r32(SLICE_POS);
        for k in 0..POS_SIZE {
            let b = m.r8(pos + k);
            m.w8(POSITIONS + k, b);
            m.w8(POSITIONS + POS_SIZE + k, b);
        }
        m.w32(SLICE_POS, POSITIONS);
    }

    fn key(c: &mut Cpu, r: &mut Rng) -> u32 {
        let n = c.m.r32(NUM_KEYS);
        let mut key = c.m.r32(HEAD_KEY);
        for _ in 0..pick(r, n) {
            key = c.m.r32(key + KEY_NEXT);
        }
        key
    }

    fn slice(c: &mut Cpu, r: &mut Rng) -> u32 {
        let slices = NUM_SLICES.get(&mut c.m) as u32;
        let k = key(c, r);
        let at = key_slice(&mut c.m, k);
        match pick(r, 3) {
            0 => pick(r, slices),
            1 => at as u32,
            _ => at.wrapping_add(pick(r, 5) as i32 - 2) as u32,
        }
    }

    /// `Fog_ReadFogKeys` from its file loaded at a0 (the test CPUs find no file): the original
    /// entered after `loadfileadr` (its frame and v0 as it has them there), the port's
    /// [`read_keys`].
    fn read_original(c: &mut Cpu) {
        let sp = c.r[29].wrapping_sub(40);
        c.r[29] = sp;
        c.m.w32(sp + 32, c.r[31]);
        for k in 0..4 {
            c.m.w32(sp + 16 + 4 * k, c.r[16 + k as usize]);
        }
        c.r[2] = c.r[4];
        nfs4rc::f_800e1b20_at(c, 0x800e_1bbc);
    }

    fn read_port(c: &mut Cpu, a: &[u32]) -> Option<u32> {
        Some(read_keys(c, a[0]) as u32)
    }

    #[test]
    fn fog_file_fuzz() {
        // (as `Fog_InitFogTriggers` reads it: the pool free, no ring yet)
        let emptied = |c: &mut Cpu| {
            c.m.w32(NUM_KEYS, 0);
            c.m.w32(HEAD_KEY, 0);
            for k in 0..POOL_SIZE {
                c.m.w32(KEYS_FREE + 4 * k, 1);
            }
            vec![FILE]
        };
        crate::test_util::COMPARE_TICKS.set(true);
        against_original("Fog_ReadFogKeys (loaded)", 400, 0xD01, shuffle, read_original, &emptied, read_port);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("TextureProcess_TransColorCheck", 0x800e_1490, nfs4rc::f_800e1490, |_, r| vec![CLUT, pick(r, 20)]),
            ("TextureProcess_ColorClut", 0x800e_14c4, nfs4rc::f_800e14c4, |_, r| {
                vec![pick(r, 16), [16, 16, 1, 0, pick(r, 20)][pick(r, 5) as usize], CLUT, [16, 16, pick(r, 33)][pick(r, 3) as usize], pick(r, 1024), pick(r, 512)]
            }),
            ("TextureProcess_DepthColorCluts", 0x800e_16cc, nfs4rc::f_800e16cc, |_, r| vec![CLUT, [16, 16, 17, 32][pick(r, 4) as usize]]),
            ("Fog_CheckRange", 0x800e_17a4, nfs4rc::f_800e17a4, |c, r| vec![slice(c, r), key(c, r)]),
            ("Fog_FindKey", 0x800e_17f8, nfs4rc::f_800e17f8, |c, r| {
                let from = [0, key(c, r)][pick(r, 2) as usize];
                vec![slice(c, r), from]
            }),
            ("Fog_AllocKey", 0x800e_188c, nfs4rc::f_800e188c, |_, _| vec![]),
            ("Fog_AddKey", 0x800e_18d8, nfs4rc::f_800e18d8, |c, r| {
                // (a ring of one now and then: no search)
                if pick(r, 4) == 0 {
                    let head = c.m.r32(HEAD_KEY);
                    c.m.w32(head + KEY_NEXT, head);
                    c.m.w32(head + KEY_PREV, head);
                    c.m.w32(NUM_KEYS, 1);
                }
                let s = slice(c, r);
                vec![s, pick(r, 3000)]
            }),
            ("Fog_Update", 0x800e_198c, nfs4rc::f_800e198c, |c, r| {
                two_positions(c);
                if pick(r, 6) == 0 {
                    c.m.w32(NUM_KEYS, 1);
                }
                vec![pick(r, 2)]
            }),
            ("Fog_MakeTrackPathName", 0x800e_1ad0, nfs4rc::f_800e1ad0, |_, r| vec![[SUFFIX_NIGHT, SUFFIX_PLAIN][pick(r, 2) as usize]]),
            ("Fog_ReadFogKeys", 0x800e_1b20, nfs4rc::f_800e1b20, |_, _| vec![]),
            ("Fog_InitFogTriggers", 0x800e_1c38, nfs4rc::f_800e1c38, |_, _| vec![]),
            ("Fog_DeInitFogTriggers", 0x800e_1d1c, nfs4rc::f_800e1d1c, |_, _| vec![]),
            ("TextureProcess_Init", 0x800e_1d40, nfs4rc::f_800e1d40, |_, _| vec![]),
            ("CV_ProcessWorldColors_FINAL", 0x800e_1d54, nfs4rc::f_800e1d54, |_, r| {
                let f = [0x6800, -0x7400, 0xFFFF_69F0_u32 as i32, 0, r.any()][pick(r, 5) as usize];
                vec![f as u32, COLOUR, [0, 16, 49, (pick(r, 600) as i32 - 300) as u32 as i32][pick(r, 4) as usize] as u32]
            }),
            ("CV_ColorTracks", 0x800e_1f14, nfs4rc::f_800e1f14, |_, r| vec![pick(r, 12), pick(r, 3), pick(r, 3)]),
            ("CV_InitColorVertices", 0x800e_211c, nfs4rc::f_800e211c, |_, _| vec![]),
        ]
    }

    #[test]
    fn textureprocess_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xD10 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            // (the ticks too, but `Fog_Update`'s: its road search, `BWorldSm_FindClosestQuadRez`,
            // is a bit-exact port whose loops don't tick)
            crate::test_util::COMPARE_TICKS.set(name != "Fog_Update");
            against_original(name, 400, 0xD11 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
