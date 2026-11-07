//! NIGHT.CPP: night driving. The world's vertex colours (`Chunk_lightTable`) are lit by
//! lookup tables: for each colour and each of 16 light levels the index of the colour it becomes
//! under the headlights (and the police's red and blue lights, the rain's two tints), new
//! colours added to the table as needed (up to 256), close ones shared. The headlights' pool of
//! light ahead of the car is a shape (`night.psh`'s "nght", 64 wide) of light levels added to
//! vertices in front of the camera. And the thunderstorms of rainy nights: lightning at random
//! times (flickers, the sky lit, thunder), not inside tunnels or the track's sheltered stretches.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call_jal, ticks, with_buffer};
use crate::globals::{GAME_SETUP, SIM_GLOBAL};
use crate::types::GameSetupCarData;
use ps1rt::{Cpu, Mem};

/// The lighting's colours while the tables are built (`gTableCache`, on the scratchpad: 256
/// r, g, b triples; `gNightInitCache` points there too), how many (`Night_gTotalLights`), the
/// world's colours (`Chunk_lightTable`, `CVECTOR`s, `Chunk_numLight`).
const TABLE_CACHE: u32 = 0x8013_ED74;
const INIT_CACHE: u32 = 0x8013_ED70;
const SCRATCHPAD: u32 = 0x1F80_0000;
const TOTAL_LIGHTS: u32 = 0x8013_ED40;
const MAX_LIGHTS: i32 = 256;
const LIGHT_TABLE: u32 = 0x8013_DB0C;
const NUM_LIGHT: u32 = 0x8013_E7D4;
/// The search's start (more than any distance) and how close a colour counts as the same.
const FAR: i32 = 0x2_FA03;
const CLOSE: i32 = 64;
const SHARED: i32 = 512;

/// The tables (each colour's index by level): the headlights' (`Night_gPlayerLightingTable`,
/// 256 x 16), the police's (`...CopLightingTableRed`, `...Blue`, 256 x 8), the rain's two
/// (`Night_gWeatherLightingTable`, 256 each); the one drawn with (`Night_gCurrentNightColor`)
/// and the police car's two (`Night_gCopColor`).
const PLAYER_TABLE: u32 = 0x8013_ECC4;
const COP_RED: u32 = 0x8013_ECC8;
const COP_BLUE: u32 = 0x8013_ECCC;
const WEATHER_TABLES: u32 = 0x8013_ECD0;
const CURRENT_TABLE: u32 = 0x8013_ED28;
const COP_COLOR: u32 = 0x8013_ED2C;
/// The colours: each player's headlights (`Night_gPlayerHeadLightColor`, from the track's
/// night spec), the rain's two (`Night_gWeatherColor`), the police's red and blue; the levels
/// (`0x8012209c`: 16, a byte each).
const HEADLIGHT_COLOR: u32 = 0x8013_ED60;
const WEATHER_COLOR: u32 = 0x8013_ED68;
const WEATHER_COLORS: [u32; 2] = [0x57_4054, 0x6C_4040];
const COP_BLUE_COLOR: u32 = 0x60_2020;
const COP_RED_COLOR: u32 = 0x10_1054;
const LEVELS: u32 = 0x8012_209C;
const NUM_LEVELS: u32 = 16;
const COP_LEVELS: u32 = 8;
/// The track's spec (`TrackSpec_gSpec`): its night colour (+0xEC), the depth cue's colour
/// (+0xF0) and distance (+0xF4).
const NIGHT_COLOR: u32 = 0x8012_456C + 0xEC;
const DEPTH_CUE_COLOR: u32 = 0x8012_456C + 0xF0;
const DEPTH_CUE_DISTANCE: u32 = 0x8012_456C + 0xF4;
/// The police cars' lights: which country's by car type (`CopCarTypeLights`, from car type 22),
/// the countries' two colours (`Night_gCopCountryLightTbl`: 0 red, 1 blue, by country and car).
const COP_CAR_TYPE_LIGHTS: u32 = 0x8013_ECD8 - 22;
const COP_COUNTRY_LIGHTS: u32 = 0x8012_2008;
const CAR_COUNTRY: u32 = 0xA0;

/// The headlights' pool of light: night's on (`gNight_renderNight`: one player at night), the
/// shape (`nightfile`, `night.psh`; its levels `Night_gNightTbl`, 64 wide), its reach and near
/// edge (`Night_gXDist`, `ZDist`, their shifts, `ZNear`), the colours its levels add
/// (`Night_gAdditiveHeadlightColor`, 16 `CVECTOR`s: the headlights' by level / 15).
const RENDER_NIGHT: u32 = 0x8013_ED08;
const NIGHT_FILE: u32 = 0x8013_ED78;
const NIGHT_TBL: u32 = 0x8013_ED20;
const X_DIST: u32 = 0x8013_ED0C;
const Z_NEAR: u32 = 0x8013_ED10;
const Z_DIST: u32 = 0x8013_ED14;
const X_DIST_SHIFT: u32 = 0x8013_ED18;
const Z_DIST_SHIFT: u32 = 0x8013_ED1C;
const ADDITIVE: u32 = 0x8012_20AC;
const NIGHT_TBL_WIDTH_SHIFT: u32 = 6;

/// The lightning: one to show (`Night_gLightning`), drawn (`Night_gDrawLightning`, not in a
/// tunnel), its kind (`Night_gLightningType`), when the next comes and ends (`Night_gNext...`,
/// `EndNext...`, in ticks), the next flicker, the flash's bearing and strength, whether it
/// forks (`Night_gShowForks`), a new bolt to place in the sky (`0x8013ecde`); the track's
/// sheltered stretches (`Night_gLightningPauseAreas`: from, to slice); the rain (`Weather_gType`).
const LIGHTNING: u32 = 0x8013_ED44;
const DRAW_LIGHTNING: u32 = 0x8013_ECC0;
const LIGHTNING_TYPE: u32 = 0x8013_ED24;
const NEXT_LIGHTNING: u32 = 0x8013_ED48;
const END_LIGHTNING: u32 = 0x8013_ED4C;
const NEXT_FLICKER: u32 = 0x8013_ED50;
const FLASH_AZIMUTH: u32 = 0x8013_ED54;
const SHOW_FORKS: u32 = 0x8013_ED58;
const FLASH_INTENSITY: u32 = 0x8013_ED5C;
const NEW_BOLT: u32 = 0x8013_ECDE;
const PAUSE_AREAS: u32 = 0x8012_201C;
const WEATHER_TYPE: u32 = 0x8013_EECC;

/// The players' cameras (`Camera_gInfo`, 272 bytes): the one they follow (+4), their road
/// position (+0x8C: its slice first); a car's lights' bits (+1095: bit 2 high beams).
const CAMERA_INFO: u32 = 0x8011_0570;
const CAMERA_SIZE: u32 = 0x110;
const CAMERA_TARGET: u32 = 4;
const CAMERA_POS: u32 = 0x8C;
const CAR_LIGHTS: u32 = 1095;
const HIGH_BEAMS: u8 = 4;
/// A view (`DRender_tView`): its player (+4).
const VIEW_PLAYER: u32 = 4;
const SPLIT_SCREEN: i32 = 1;
const RAIN: i32 = 1;

/// The files: "%snight.psh" in the shapes' directory (`Paths_Paths + 0x64`), its block's name,
/// the shape's ("nght"); the tables' blocks' names.
const NIGHT_PSH_FORMAT: u32 = 0x8005_7110;
const NIGHT_PSH_NAME: u32 = 0x8005_711C;
const NGHT: u32 = 0x8013_ED00;
const SHAPES_PATH: u32 = crate::globals::PATHS + 0x64;
const PLAYER_TABLE_NAME: u32 = 0x8013_ECE0;
const COP_RED_NAME: u32 = 0x8013_ECE8;
const COP_BLUE_NAME: u32 = 0x8013_ECF0;
const WEATHER_TABLE_NAME: u32 = 0x8013_ECF8;

/// Functions called: the library's `random`, `sprintf`, `filesize`, `reservememadr(name, size,
/// 0)`, `loadshapeadr(name, 0)`, `blockmove(from, to, n)`, `purgememadr`, `locateshape(file,
/// name)`; the thunder (`AudioCmn_PlayThunder(intensity, azimuth)`), the sky's lightning
/// (`Hrz_LightningFlicker(on)`, `Hrz_SetLightingPosInSky(view)`, `Hrz_CalculateLightning`), a
/// tunnel (`BWorldSm_TunnelFlagSm(pos)`), the camera's mode (`Camera_GetMode`).
const RANDOM: u32 = 0x800E_8394;
const SPRINTF: u32 = 0x800E_50A8;
const FILE_SIZE: u32 = 0x800E_6258;
const RESERVE: u32 = 0x800E_5F28;
const LOAD_SHAPE: u32 = 0x800F_2970;
const BLOCK_MOVE: u32 = 0x800E_6EC8;
const PURGE: u32 = 0x800E_612C;
const LOCATE_SHAPE: u32 = 0x800E_BA6C;
const PLAY_THUNDER: u32 = 0x8007_6EBC;
const LIGHTNING_FLICKER: u32 = 0x800D_14C0;
const LIGHTNING_POS_IN_SKY: u32 = 0x800D_1294;
const CALCULATE_LIGHTNING: u32 = 0x800D_1128;
const TUNNEL_FLAG: u32 = 0x8008_09F8;
const CAMERA_GET_MODE: u32 = 0x8008_5F8C;

fn cache(m: &mut Mem) -> u32 {
    m.r32(TABLE_CACHE)
}

fn total(m: &mut Mem) -> i32 {
    m.r32(TOTAL_LIGHTS) as i32
}

fn game_ticks(m: &mut Mem) -> i32 {
    SIM_GLOBAL.game_ticks(m)
}

fn random(c: &mut Cpu) -> u32 {
    call_jal(c, RANDOM, &[])
}

/// `Night_FindClosestColor` (NIGHT.CPP, 0x800dbf04): the lighting colour nearest `rgb` (its
/// low three bytes; the search stops at the first closer than 8 a channel): the squared
/// distance, its index into `index` (left as it was when none is nearer than `FAR`).
pub fn find_closest_color(c: &mut Cpu, rgb: u32, index: u32) -> i32 {
    let (r, g, b) = ((rgb & 0xFF) as i32, (rgb >> 8 & 0xFF) as i32, (rgb >> 16 & 0xFF) as i32);
    let mut best = FAR;
    let n = total(&mut c.m).max(0) as u32;
    for k in 0..n {
        let m = &mut c.m;
        let at = cache(m).wrapping_add(3 * k);
        let (dr, dg, db) = (r - m.r8(at) as i32, g - m.r8(at + 1) as i32, b - m.r8(at + 2) as i32);
        let d = dr * dr + dg * dg + db * db;
        if d < best {
            best = d;
            m.w32(index, k);
            if best < CLOSE {
                return best;
            }
        }
        // (the loop's branch back, but after the last)
        if k + 1 < n {
            c.tick();
        }
    }
    best
}

pub fn abi_find_closest_color(c: &mut Cpu) {
    c.r[2] = find_closest_color(c, c.r[4], c.r[5]) as u32;
}

/// `Night_CreateNightTableElement` (NIGHT.CPP, 0x800dbfcc): lighting colour `light` with `rgb`
/// added at `level` (of 15) into `out`: the index of the nearest colour, or of the result
/// itself added to the lighting colours when none is close (while there is room and the colour
/// is one of the world's or the four after them).
pub fn create_night_table_element(c: &mut Cpu, light: i32, rgb: u32, level: i32, out: u32) {
    let m = &mut c.m;
    let scale = level.wrapping_mul(0x111);
    let at = cache(m).wrapping_add(3 * light as u32);
    let lit: [u8; 3] = std::array::from_fn(|k| {
        let add = ((rgb >> (8 * k)) & 0xFF) as i32;
        let v = (m.r8(at.wrapping_add(k as u32)) as i32).wrapping_add(add.wrapping_mul(scale) >> 12);
        (v.min(255) & !7) as u8
    });
    let packed = lit[0] as u32 | (lit[1] as u32) << 8 | (lit[2] as u32) << 16;
    // (the index found: none is found only when no colour is nearer than `FAR`, then the
    // colour is added anyway unless the table is full)
    let (distance, found) = with_buffer(c, 4, |c, index| {
        c.m.w32(index, 0);
        let d = find_closest_color(c, packed, index);
        (d, c.m.r8(index))
    });
    let m = &mut c.m;
    let room = light < (m.r32(NUM_LIGHT) as i32).wrapping_add(4) && total(m) < MAX_LIGHTS;
    if distance > SHARED && room {
        let n = total(m) as u32;
        for (k, v) in lit.into_iter().enumerate() {
            let at = cache(m).wrapping_add(3 * n + k as u32);
            m.w8(at, v);
        }
        m.w8(out, n as u8);
        m.w32(TOTAL_LIGHTS, n.wrapping_add(1));
    } else {
        m.w8(out, found);
    }
}

pub fn abi_create_night_table_element(c: &mut Cpu) {
    create_night_table_element(c, c.r[4] as i32, c.r[5], c.r[6] as i32, c.r[7]);
}

/// `Night_CreateNightTable` (NIGHT.CPP, 0x800dc190): [`create_night_table_element`] into
/// `table`'s row `light`, column `level` (16 a row).
pub fn create_night_table(c: &mut Cpu, light: i32, rgb: u32, level: i32, table: u32) {
    let out = table.wrapping_add((light as u32) << 4).wrapping_add(level as u32);
    create_night_table_element(c, light, rgb, level, out);
}

pub fn abi_create_night_table(c: &mut Cpu) {
    create_night_table(c, c.r[4] as i32, c.r[5], c.r[6] as i32, c.r[7]);
}

/// `Night_GenerateNextLightningEvent` (NIGHT.CPP, 0x800dc1b8): the next lightning: from 31 to
/// 2078 ticks on, 15 to 30 long, flickering from its start, its bearing, forked or not.
pub fn generate_next_lightning_event(c: &mut Cpu) {
    let r = random(c) & 0x7FF;
    let next = game_ticks(&mut c.m).wrapping_add((r + 31) as i32);
    c.m.w32(NEXT_LIGHTNING, next as u32);
    let r = random(c) & 0xF;
    let next = c.m.r32(NEXT_LIGHTNING);
    c.m.w32(END_LIGHTNING, next.wrapping_add(r + 15));
    c.m.w32(NEXT_FLICKER, next);
    let r = random(c) & 0xFFFF;
    c.m.w32(FLASH_AZIMUTH, r);
    let r = random(c) & 1;
    c.m.w8(SHOW_FORKS, r as u8);
}

pub fn abi_generate_next_lightning_event(c: &mut Cpu) {
    generate_next_lightning_event(c);
}

/// `Night_PauseLightningEffect` (NIGHT.CPP, 0x800dc22c): no lightning now (the next and its end
/// now) while player `p`'s camera is in one of the track's sheltered stretches, or without rain.
pub fn pause_lightning_effect(m: &mut Mem, p: u32) {
    let slice = m.r16(CAMERA_INFO.wrapping_add(p.wrapping_mul(CAMERA_SIZE)) + CAMERA_POS) as i16 as i32;
    let area = PAUSE_AREAS.wrapping_add((GAME_SETUP.track(m) as u32) << 3);
    let (from, to) = (m.r32(area) as i32, m.r32(area.wrapping_add(4)) as i32);
    let sheltered = from < slice && slice < to;
    if sheltered || m.r32(WEATHER_TYPE) == 0 {
        let now = game_ticks(m) as u32;
        m.w32(NEXT_LIGHTNING, now);
        m.w32(END_LIGHTNING, now);
    }
}

pub fn abi_pause_lightning_effect(c: &mut Cpu) {
    pause_lightning_effect(&mut c.m, c.r[4]);
}

/// `Night_DoLightningEffect` (NIGHT.CPP, 0x800dc2b0): a frame of the storm for `view`: the last
/// frame's flash heard (thunder) and the sky put back; during a lightning, at each flicker a
/// flash (of a kind, the sky lit, drawn unless the camera is in a tunnel; the next flicker 0 to
/// 3 ticks on; its strength) and a new bolt placed in the sky (if it forks); once it is over,
/// the next lightning chosen.
pub fn do_lightning_effect(c: &mut Cpu, view: u32) {
    if c.m.r32(LIGHTNING) != 0 {
        let (intensity, azimuth) = (c.m.r32(FLASH_INTENSITY), c.m.r32(FLASH_AZIMUTH));
        call_jal(c, PLAY_THUNDER, &[intensity, azimuth]);
        call_jal(c, LIGHTNING_FLICKER, &[0]);
        c.m.w32(LIGHTNING, 0);
    }
    let m = &mut c.m;
    let now = game_ticks(m);
    let (next, end, flicker) = (m.r32(NEXT_LIGHTNING) as i32, m.r32(END_LIGHTNING) as i32, m.r32(NEXT_FLICKER) as i32);
    if next < now && now < end && flicker < now {
        let kind = random(c) & 1;
        c.m.w32(LIGHTNING_TYPE, kind);
        call_jal(c, LIGHTNING_FLICKER, &[1]);
        let p = c.m.r32(view.wrapping_add(VIEW_PLAYER));
        let pos = CAMERA_INFO.wrapping_add(p.wrapping_mul(CAMERA_SIZE)).wrapping_add(CAMERA_POS);
        let tunnel = call_jal(c, TUNNEL_FLAG, &[pos]) != 0;
        c.m.w8(DRAW_LIGHTNING, !tunnel as u8);
        c.m.w32(LIGHTNING, 1);
        let r = random(c) & 3;
        let now = game_ticks(&mut c.m);
        c.m.w32(NEXT_FLICKER, now.wrapping_add(r as i32) as u32);
        let r = random(c) & 0x1F;
        let m = &mut c.m;
        let kind = m.r32(LIGHTNING_TYPE);
        m.w32(FLASH_INTENSITY, kind.wrapping_add(1).wrapping_mul(r).wrapping_add(64));
        if m.r8(NEW_BOLT) != 0 {
            if m.r8(SHOW_FORKS) != 0 {
                call_jal(c, LIGHTNING_POS_IN_SKY, &[view]);
            }
            c.m.w8(NEW_BOLT, 0);
        }
    }
    let m = &mut c.m;
    if (m.r32(END_LIGHTNING) as i32) < game_ticks(m) {
        generate_next_lightning_event(c);
        call_jal(c, CALCULATE_LIGHTNING, &[]);
        c.m.w8(NEW_BOLT, 1);
    }
}

pub fn abi_do_lightning_effect(c: &mut Cpu) {
    do_lightning_effect(c, c.r[4]);
}

/// `Night_SetCopColor` (NIGHT.CPP, 0x800dc434): the police car's two lights' tables (red or
/// blue each, by the car type's country and the car's country).
pub fn set_cop_color(m: &mut Mem, car: GameSetupCarData) {
    let car_type = car.car_type(m) as u32;
    let country = m.r8(COP_CAR_TYPE_LIGHTS.wrapping_add(car_type)) as u32;
    let entry = COP_COUNTRY_LIGHTS.wrapping_add(country.wrapping_mul(5).wrapping_add(m.r32(car.0 + CAR_COUNTRY)).wrapping_mul(2));
    // (the original keeps the two tables in its frame and indexes them by the entry: only 0
    // and 1 are in the game's table)
    let tables = [m.r32(COP_RED), m.r32(COP_BLUE)];
    for k in 0..2 {
        let which = m.r8(entry.wrapping_add(k)) as usize;
        m.w32(COP_COLOR + 4 * k, tables[which & 1]);
    }
}

pub fn abi_set_cop_color(c: &mut Cpu) {
    set_cop_color(&mut c.m, GameSetupCarData(c.r[4]));
}

/// `Night_InitPlayerHeadLightColor` (NIGHT.CPP, 0x800dc4c8): the headlights' table reserved
/// (once), player `p`'s headlights the track's night colour.
pub fn init_player_head_light_color(c: &mut Cpu, p: u32) {
    if c.m.r32(PLAYER_TABLE) == 0 {
        let table = call_jal(c, RESERVE, &[PLAYER_TABLE_NAME, 256 * NUM_LEVELS, 0]);
        c.m.w32(PLAYER_TABLE, table);
    }
    let colour = c.m.r32(NIGHT_COLOR);
    c.m.w32(HEADLIGHT_COLOR.wrapping_add(p << 2), colour);
}

pub fn abi_init_player_head_light_color(c: &mut Cpu) {
    init_player_head_light_color(c, c.r[4]);
}

/// `Night_SetPlayerHeadLightColor` (NIGHT.CPP, 0x800dc524): lighting colour `light` under
/// player `p`'s headlights at `level`, into the headlights' table.
pub fn set_player_head_light_color(c: &mut Cpu, p: u32, light: i32, level: i32) {
    let colour = c.m.r32(HEADLIGHT_COLOR.wrapping_add(p << 2));
    let table = c.m.r32(PLAYER_TABLE);
    create_night_table(c, light, colour, level, table);
}

pub fn abi_set_player_head_light_color(c: &mut Cpu) {
    set_player_head_light_color(c, c.r[4], c.r[5] as i32, c.r[6] as i32);
}

/// `Night_InitCopLightColors` (NIGHT.CPP, 0x800dc560): the police lights' tables reserved
/// (once).
pub fn init_cop_light_colors(c: &mut Cpu) {
    for (table, name) in [(COP_RED, COP_RED_NAME), (COP_BLUE, COP_BLUE_NAME)] {
        if c.m.r32(table) == 0 {
            let at = call_jal(c, RESERVE, &[name, 256 * COP_LEVELS, 0]);
            c.m.w32(table, at);
        }
    }
}

pub fn abi_init_cop_light_colors(c: &mut Cpu) {
    init_cop_light_colors(c);
}

/// `Night_SetCopLightColors` (NIGHT.CPP, 0x800dc5bc): lighting colour `light` under the
/// police's blue and red at `level` (into their tables, a column every second level).
pub fn set_cop_light_colors(c: &mut Cpu, light: i32, level: i32) {
    let column = ((light as u32) << 3).wrapping_add((level >> 1) as u32);
    let blue = c.m.r32(COP_BLUE).wrapping_add(column);
    create_night_table_element(c, light, COP_BLUE_COLOR, level, blue);
    let red = c.m.r32(COP_RED).wrapping_add(column);
    create_night_table_element(c, light, COP_RED_COLOR, level, red);
}

pub fn abi_set_cop_light_colors(c: &mut Cpu) {
    set_cop_light_colors(c, c.r[4] as i32, c.r[5] as i32);
}

/// `Night_InitWeatherTables` (NIGHT.CPP, 0x800dc63c): the rain's two tables reserved (once),
/// its two tints.
pub fn init_weather_tables(c: &mut Cpu) {
    for k in 0..2 {
        if c.m.r32(WEATHER_TABLES + 4 * k) == 0 {
            let at = call_jal(c, RESERVE, &[WEATHER_TABLE_NAME, 256, 0]);
            c.m.w32(WEATHER_TABLES + 4 * k, at);
        }
    }
    c.tick();
    for (k, colour) in WEATHER_COLORS.into_iter().enumerate() {
        c.m.w32(WEATHER_COLOR + 4 * k as u32, colour);
    }
}

pub fn abi_init_weather_tables(c: &mut Cpu) {
    init_weather_tables(c);
}

/// `Night_SetWeatherColors` (NIGHT.CPP, 0x800dc6c0): lighting colour `light` under the rain's
/// two tints at full strength, into their tables.
pub fn set_weather_colors(c: &mut Cpu, light: i32) {
    for k in 0..2 {
        let colour = c.m.r32(WEATHER_COLOR + 4 * k);
        let out = c.m.r32(WEATHER_TABLES + 4 * k).wrapping_add(light as u32);
        create_night_table_element(c, light, colour, 15, out);
    }
    c.tick();
}

pub fn abi_set_weather_colors(c: &mut Cpu) {
    set_weather_colors(c, c.r[4] as i32);
}

/// `Night_GenerateAllLightTables` (NIGHT.CPP, 0x800dc73c): every table, from the world's
/// colours copied to the scratchpad (all 256 places) and back after with the colours added:
/// each colour (the new ones too) under the rain's tints (when it rains), the headlights at
/// each level, the police's at the first eight (with police); then the headlights' pool's
/// colours by level.
pub fn generate_all_light_tables(c: &mut Cpu) {
    let m = &mut c.m;
    m.w32(INIT_CACHE, SCRATCHPAD);
    m.w32(TABLE_CACHE, SCRATCHPAD);
    let n = m.r32(NUM_LIGHT);
    m.w32(TOTAL_LIGHTS, n);
    for k in 0..MAX_LIGHTS as u32 {
        for j in 0..3 {
            let from = m.r32(LIGHT_TABLE).wrapping_add(4 * k + j);
            let v = m.r8(from);
            let to = cache(m).wrapping_add(3 * k + j);
            m.w8(to, v);
        }
    }
    // (each of the original's loops ticks once a pass but the last; the levels' and the
    // lights' each once a pass)
    ticks(c, MAX_LIGHTS as u32 - 1);
    let mut light = 0;
    while light < total(&mut c.m) {
        if GAME_SETUP.weather(&mut c.m) == RAIN {
            set_weather_colors(c, light);
        }
        for s in 0..NUM_LEVELS {
            let level = c.m.r8(LEVELS + s) as i32;
            set_player_head_light_color(c, 0, light, level);
            if GAME_SETUP.cops(&mut c.m) != 0 && s < COP_LEVELS {
                set_cop_light_colors(c, light, level);
            }
            c.tick();
        }
        c.tick();
        light += 1;
    }
    let m = &mut c.m;
    for k in 0..MAX_LIGHTS as u32 {
        for j in 0..3 {
            let from = cache(m).wrapping_add(3 * k + j);
            let v = m.r8(from);
            let to = m.r32(LIGHT_TABLE).wrapping_add(4 * k + j);
            m.w8(to, v);
        }
    }
    for level in 0..NUM_LEVELS as i32 {
        for j in 0..3 {
            let v = m.r8(HEADLIGHT_COLOR + j) as i32;
            m.w8(ADDITIVE + 4 * level as u32 + j, (v * level / 15) as u8);
        }
    }
    ticks(c, MAX_LIGHTS as u32 - 1 + NUM_LEVELS - 1);
}

pub fn abi_generate_all_light_tables(c: &mut Cpu) {
    generate_all_light_tables(c);
}

/// The lightning's first after a start: within 511 ticks, up to 49 long.
fn first_lightning(c: &mut Cpu) {
    c.m.w32(LIGHTNING, 0);
    let r = random(c) & 0x1FF;
    let next = game_ticks(&mut c.m).wrapping_add(r as i32) as u32;
    c.m.w32(NEXT_LIGHTNING, next);
    let r = random(c) & 0x31;
    let next = c.m.r32(NEXT_LIGHTNING);
    c.m.w32(END_LIGHTNING, next.wrapping_add(r));
    c.m.w32(NEXT_FLICKER, next);
}

/// `Night_InitNightDriving` (NIGHT.CPP, 0x800dc9d0): night (one player only: with two, the
/// depth cue made plain): the headlights' pool's shape loaded, the tables made (the police's
/// with police, the rain's when it rains), and the storm's first lightning when it rains.
pub fn init_night_driving(c: &mut Cpu) {
    let m = &mut c.m;
    let night = GAME_SETUP.time(m) != 0;
    let render = night && GAME_SETUP.comm_mode(m) != SPLIT_SCREEN;
    m.w32(RENDER_NIGHT, render as u32);
    if night && !render {
        m.w32(DEPTH_CUE_DISTANCE, 255);
        m.w32(DEPTH_CUE_COLOR, 0);
    }
    if !render {
        return;
    }
    with_buffer(c, 256, |c, name| {
        let path = c.m.r32(SHAPES_PATH);
        call_jal(c, SPRINTF, &[name, NIGHT_PSH_FORMAT, path]);
        let size = call_jal(c, FILE_SIZE, &[name]);
        let file = call_jal(c, RESERVE, &[NIGHT_PSH_NAME, size, 0]);
        c.m.w32(NIGHT_FILE, file);
        let shapes = call_jal(c, LOAD_SHAPE, &[name, 0]);
        let size = call_jal(c, FILE_SIZE, &[name]);
        let file = c.m.r32(NIGHT_FILE);
        call_jal(c, BLOCK_MOVE, &[shapes, file, size]);
        call_jal(c, PURGE, &[shapes]);
    });
    let file = c.m.r32(NIGHT_FILE);
    let shape = call_jal(c, LOCATE_SHAPE, &[file, NGHT]);
    c.m.w32(NIGHT_TBL, shape.wrapping_add(16));
    init_player_head_light_color(c, 0);
    if GAME_SETUP.cops(&mut c.m) != 0 {
        init_cop_light_colors(c);
    }
    if GAME_SETUP.weather(&mut c.m) == RAIN {
        init_weather_tables(c);
    }
    generate_all_light_tables(c);
    if GAME_SETUP.weather(&mut c.m) == RAIN {
        first_lightning(c);
        call_jal(c, CALCULATE_LIGHTNING, &[]);
    }
}

pub fn abi_init_night_driving(c: &mut Cpu) {
    init_night_driving(c);
}

/// `Night_KillNightDriving` (NIGHT.CPP, 0x800dcb6c): the shape and the tables freed.
pub fn kill_night_driving(c: &mut Cpu) {
    for at in [NIGHT_FILE, PLAYER_TABLE, COP_RED, COP_BLUE, WEATHER_TABLES, WEATHER_TABLES + 4] {
        let block = c.m.r32(at);
        if block != 0 {
            call_jal(c, PURGE, &[block]);
        }
        c.m.w32(at, 0);
    }
}

pub fn abi_kill_night_driving(c: &mut Cpu) {
    kill_night_driving(c);
}

/// `Night_RestartNightDriving` (NIGHT.CPP, 0x800dcc0c): a rainy night's race restarted: the
/// storm's first lightning again, the sky put back.
pub fn restart_night_driving(c: &mut Cpu) {
    let m = &mut c.m;
    if GAME_SETUP.weather(m) != RAIN || GAME_SETUP.time(m) == 0 {
        return;
    }
    first_lightning(c);
    call_jal(c, LIGHTNING_FLICKER, &[0]);
}

pub fn abi_restart_night_driving(c: &mut Cpu) {
    restart_night_driving(c);
}

/// `Night_SetEnviroment` (NIGHT.CPP, 0x800dcc94): a night frame's set-up for `view`: drawn with
/// the headlights' table; the storm (the first player's view only plays it); the headlights'
/// pool: 1024 wide and 4096 deep from 128 ahead (2048 by 8192 with high beams), from 512 behind
/// in the bumper camera.
pub fn set_enviroment(c: &mut Cpu, view: u32) {
    let m = &mut c.m;
    if GAME_SETUP.time(m) == 0 {
        return;
    }
    let table = m.r32(PLAYER_TABLE);
    let rain = GAME_SETUP.weather(m) == RAIN;
    m.w8(DRAW_LIGHTNING, 0);
    m.w32(CURRENT_TABLE, table);
    if rain {
        let p = m.r32(view.wrapping_add(VIEW_PLAYER));
        pause_lightning_effect(m, p);
        if c.m.r32(view.wrapping_add(VIEW_PLAYER)) == 0 {
            do_lightning_effect(c, view);
        }
    }
    let m = &mut c.m;
    m.w32(X_DIST_SHIFT, 10);
    let p = m.r32(view.wrapping_add(VIEW_PLAYER));
    m.w32(Z_DIST_SHIFT, 12);
    let car = m.r32(CAMERA_INFO.wrapping_add(p.wrapping_mul(CAMERA_SIZE)) + CAMERA_TARGET);
    m.w32(Z_NEAR, 128);
    if m.r8(car.wrapping_add(CAR_LIGHTS)) & HIGH_BEAMS != 0 {
        m.w32(Z_DIST_SHIFT, 13);
        m.w32(X_DIST_SHIFT, 11);
    }
    let p = m.r32(view.wrapping_add(VIEW_PLAYER));
    if call_jal(c, CAMERA_GET_MODE, &[p]) == 0 {
        c.m.w32(Z_NEAR, -512i32 as u32);
    }
    let m = &mut c.m;
    let (x_shift, z_shift) = (m.r32(X_DIST_SHIFT), m.r32(Z_DIST_SHIFT));
    m.w32(X_DIST, 1u32.wrapping_shl(x_shift));
    m.w32(Z_DIST, 1u32.wrapping_shl(z_shift));
    m.w32(Z_DIST_SHIFT, z_shift.wrapping_sub(6));
    m.w32(X_DIST_SHIFT, x_shift.wrapping_sub(5));
}

pub fn abi_set_enviroment(c: &mut Cpu) {
    set_enviroment(c, c.r[4]);
}

/// `Night_AdditiveNightCalc` (NIGHT.CPP, 0x800dcda4): the headlights' pool's light added to
/// `colour` for a vertex at `v` (camera space) inside it: its level by the vertex's place (64
/// columns across, rows from the near edge), that level's colour added (each channel to 255).
pub fn additive_night_calc(m: &mut Mem, v: u32, colour: u32) {
    let z_shift = m.r32(Z_DIST_SHIFT);
    let z = m.r32(v.wrapping_add(8)) as i32;
    let near = m.r32(Z_NEAR) as i32;
    let far = near.wrapping_add(1i32.wrapping_shl(z_shift.wrapping_add(6)));
    if !(near < z && z < far) {
        return;
    }
    let x_shift = m.r32(X_DIST_SHIFT);
    let x = m.r32(v) as i32;
    let half = 1i32.wrapping_shl(x_shift.wrapping_add(5));
    if !(half.wrapping_neg() < x && x < half) {
        return;
    }
    let row = z.wrapping_sub(near).wrapping_shr(z_shift);
    let col = x.wrapping_add(half).wrapping_shr(x_shift);
    let at = m.r32(NIGHT_TBL).wrapping_add(((row << NIGHT_TBL_WIDTH_SHIFT).wrapping_add(col)) as u32);
    let level = m.r8(at) as u32;
    let add = m.r32(ADDITIVE.wrapping_add(level << 2));
    let lit: [u8; 3] = std::array::from_fn(|k| {
        let v = m.r8(colour.wrapping_add(k as u32)) as u32 + ((add >> (8 * k)) & 0xFF);
        v.min(255) as u8
    });
    for (k, v) in lit.into_iter().enumerate() {
        m.w8(colour.wrapping_add(k as u32), v);
    }
}

pub fn abi_additive_night_calc(c: &mut Cpu) {
    additive_night_calc(&mut c.m, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM (the race leaves it zero) for what a night race sets up (the race RAM is a day
    /// race's): the tables, the world's colours (256 places), the headlights' pool's levels
    /// (64 by 64), a view, a car's setup, a colour, a vertex.
    const PLAYER: u32 = 0x8014_0000;
    const RED: u32 = 0x8014_1000;
    const BLUE: u32 = 0x8014_1800;
    const WEATHER: [u32; 2] = [0x8014_2000, 0x8014_2100];
    const POOL: u32 = 0x8014_3000;
    const LIGHTS: u32 = 0x8014_4000;
    const VIEW: u32 = 0x8014_4800;
    const CAR_DATA: u32 = 0x8014_4900;
    const COLOUR: u32 = 0x8014_4A00;
    const VERTEX: u32 = 0x8014_4A10;
    const OUT: u32 = 0x8014_4A40;

    /// The race RAM made a night's: the tables at free RAM (each now and then unset, for the
    /// reservations), the world's colours (45, or more) there and on the scratchpad with some
    /// lighting colours added (up to 256: room or none), the headlights', police's and rain's
    /// colours, the levels; the storm's times around now, its kind, a new bolt, forks; the
    /// setup (night, rain, police, two players, the track), the rain's type; the headlights'
    /// pool (its levels, shifts, near edge), a view (either player), a police car's setup.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        for (at, table) in [(PLAYER_TABLE, PLAYER), (COP_RED, RED), (COP_BLUE, BLUE), (WEATHER_TABLES, WEATHER[0]), (WEATHER_TABLES + 4, WEATHER[1])] {
            m.w32(at, if pick(rng, 8) == 0 { 0 } else { table });
        }
        for k in 0..0x2200 {
            m.w8(PLAYER + k, rng.next() as u8);
        }
        let n = [45, 45, 100, 250][pick(rng, 4) as usize];
        m.w32(LIGHT_TABLE, LIGHTS);
        m.w32(NUM_LIGHT, n);
        for k in 0..256 * 4 {
            let v = rng.next() as u8;
            m.w8(LIGHTS + k, [v, v & 0xE0, 255, 0][pick(rng, 4) as usize]);
        }
        m.w32(TABLE_CACHE, SCRATCHPAD);
        m.w32(INIT_CACHE, SCRATCHPAD);
        for k in 0..256 * 3 {
            let v = rng.next() as u8;
            m.w8(SCRATCHPAD + k, [v, v & 0xF8, 248][pick(rng, 3) as usize]);
        }
        m.w32(TOTAL_LIGHTS, [n, n + pick(rng, 40), 255, 256][pick(rng, 4) as usize]);
        for k in 0..2 {
            m.w32(HEADLIGHT_COLOR + 4 * k, rng.next() as u32 & 0xFF_FFFF);
            m.w32(WEATHER_COLOR + 4 * k, WEATHER_COLORS[k as usize]);
        }
        m.w32(NIGHT_COLOR, rng.next() as u32 & 0xFF_FFFF);
        let now = game_ticks(m);
        let around = |rng: &mut Rng| now.wrapping_add(pick(rng, 40) as i32 - 20) as u32;
        let (next, end, flicker) = (around(rng), around(rng), around(rng));
        m.w32(NEXT_LIGHTNING, next);
        m.w32(END_LIGHTNING, end);
        m.w32(NEXT_FLICKER, flicker);
        m.w32(LIGHTNING, pick(rng, 2));
        m.w32(LIGHTNING_TYPE, pick(rng, 2));
        m.w8(NEW_BOLT, pick(rng, 2) as u8);
        m.w8(SHOW_FORKS, pick(rng, 2) as u8);
        m.w32(FLASH_INTENSITY, pick(rng, 200));
        m.w32(FLASH_AZIMUTH, pick(rng, 0x1_0000));
        GAME_SETUP.set_time(m, pick(rng, 2) as i32);
        GAME_SETUP.set_weather(m, pick(rng, 2) as i32);
        GAME_SETUP.set_cops(m, pick(rng, 2) as i32);
        GAME_SETUP.set_comm_mode(m, pick(rng, 2) as i32);
        m.w32(WEATHER_TYPE, pick(rng, 2));
        // the headlights' pool
        m.w32(NIGHT_TBL, POOL);
        for k in 0..64 * 64 {
            m.w8(POOL + k, pick(rng, 16) as u8);
        }
        for level in 0..16 {
            m.w32(ADDITIVE + 4 * level, rng.next() as u32 & 0xFF_FFFF);
        }
        let high = pick(rng, 2);
        m.w32(X_DIST_SHIFT, 5 + high);
        m.w32(Z_DIST_SHIFT, 6 + high);
        m.w32(Z_NEAR, [128, -512i32 as u32][pick(rng, 2) as usize]);
        m.w32(VIEW + VIEW_PLAYER, [0, 0, 1][pick(rng, 3) as usize]);
        for k in 0..64 {
            m.w8(CAR_DATA + k, 0);
        }
        m.w32(CAR_DATA, 22 + pick(rng, 6));
        m.w32(CAR_DATA + CAR_COUNTRY, pick(rng, 5));
        m.w32(COLOUR, rng.next() as u32);
        // a vertex in the pool or outside it
        let z = (m.r32(Z_NEAR) as i32).wrapping_add(pick(rng, 9000) as i32 - 500);
        let half = 1i32 << (m.r32(X_DIST_SHIFT) + 5);
        let x = [pick(rng, 5000) as i32 - 2500, -half, -half + 1, half - 1, half][pick(rng, 5) as usize];
        m.w32(VERTEX, x as u32);
        m.w32(VERTEX + 4, rng.next() as u32);
        m.w32(VERTEX + 8, z as u32);
        // a car's lights (the cameras' anchors): high beams or not
        let car = m.r32(CAMERA_INFO + CAMERA_TARGET);
        let lights = m.r8(car + CAR_LIGHTS) & !HIGH_BEAMS | if pick(rng, 2) == 0 { HIGH_BEAMS } else { 0 };
        m.w8(car + CAR_LIGHTS, lights);
    }

    fn light(c: &mut Cpu, r: &mut Rng) -> u32 {
        let total = total(&mut c.m) as u32;
        [pick(r, 45), pick(r, total.max(1)), pick(r, 256)][pick(r, 3) as usize]
    }

    fn rgb(r: &mut Rng) -> u32 {
        [COP_BLUE_COLOR, COP_RED_COLOR, WEATHER_COLORS[0], r.next() as u32 & 0xFF_FFFF][pick(r, 4) as usize]
    }

    fn level(r: &mut Rng) -> u32 {
        [0, 15, pick(r, 16), pick(r, 32)][pick(r, 4) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Night_FindClosestColor", 0x800d_bf04, nfs4rc::f_800dbf04, |c, r| {
                // (now and then a colour exactly 64 away, a nearer one after it)
                let rgb = r.next() as u32 & 0xF8_F8F8;
                if pick(r, 2) == 0 {
                    let total = total(&mut c.m).clamp(2, 256) as u32;
                    let (first, later) = (pick(r, total - 1), total - 1);
                    let near = rgb ^ 8 << (8 * pick(r, 3));
                    for (k, v) in [(first, near), (later, rgb)] {
                        for j in 0..3 {
                            c.m.w8(SCRATCHPAD + 3 * k + j, (v >> (8 * j)) as u8);
                        }
                    }
                }
                vec![rgb, OUT]
            }),
            ("Night_CreateNightTableElement", 0x800d_bfcc, nfs4rc::f_800dbfcc, |c, r| vec![light(c, r), rgb(r), level(r), OUT]),
            ("Night_CreateNightTable", 0x800d_c190, nfs4rc::f_800dc190, |c, r| vec![light(c, r), rgb(r), pick(r, 16), PLAYER]),
            ("Night_GenerateNextLightningEvent", 0x800d_c1b8, nfs4rc::f_800dc1b8, |_, _| vec![]),
            ("Night_PauseLightningEffect", 0x800d_c22c, nfs4rc::f_800dc22c, |c, r| {
                // (now and then the track's sheltered stretch around the camera's slice)
                if pick(r, 2) == 0 {
                    let slice = c.m.r16(CAMERA_INFO + CAMERA_POS) as i16 as i32;
                    let area = PAUSE_AREAS + ((GAME_SETUP.track(&mut c.m) as u32) << 3);
                    c.m.w32(area, slice.wrapping_sub(pick(r, 3) as i32) as u32);
                    c.m.w32(area + 4, slice.wrapping_add(pick(r, 3) as i32) as u32);
                }
                vec![0]
            }),
            ("Night_DoLightningEffect", 0x800d_c2b0, nfs4rc::f_800dc2b0, |c, r| {
                // (the camera's slice in a tunnel or not: its acoustics)
                let p = c.m.r32(VIEW + VIEW_PLAYER);
                let slice = c.m.r16(CAMERA_INFO + p * CAMERA_SIZE + CAMERA_POS) as i16 as i32 as u32;
                let slices = crate::globals::BWORLDSM_SLICES.get(&mut c.m) as u32;
                let at = slices.wrapping_add(slice.wrapping_mul(crate::types::TrkNewSlice::SIZE)) + 0x15;
                c.m.w8(at, [0, 0x44, 0x04, 0x40][pick(r, 4) as usize]);
                vec![VIEW]
            }),
            ("Night_SetCopColor", 0x800d_c434, nfs4rc::f_800dc434, |_, _| vec![CAR_DATA]),
            ("Night_InitPlayerHeadLightColor", 0x800d_c4c8, nfs4rc::f_800dc4c8, |_, r| vec![pick(r, 2)]),
            ("Night_SetPlayerHeadLightColor", 0x800d_c524, nfs4rc::f_800dc524, |c, r| {
                c.m.w32(PLAYER_TABLE, PLAYER);
                vec![pick(r, 2), light(c, r), pick(r, 16)]
            }),
            ("Night_InitCopLightColors", 0x800d_c560, nfs4rc::f_800dc560, |_, _| vec![]),
            ("Night_SetCopLightColors", 0x800d_c5bc, nfs4rc::f_800dc5bc, |c, r| {
                c.m.w32(COP_RED, RED);
                c.m.w32(COP_BLUE, BLUE);
                vec![light(c, r), pick(r, 16)]
            }),
            ("Night_InitWeatherTables", 0x800d_c63c, nfs4rc::f_800dc63c, |_, _| vec![]),
            ("Night_SetWeatherColors", 0x800d_c6c0, nfs4rc::f_800dc6c0, |c, r| {
                c.m.w32(WEATHER_TABLES, WEATHER[0]);
                c.m.w32(WEATHER_TABLES + 4, WEATHER[1]);
                vec![light(c, r)]
            }),
            ("Night_GenerateAllLightTables", 0x800d_c73c, nfs4rc::f_800dc73c, |c, _| {
                for (at, table) in [(PLAYER_TABLE, PLAYER), (COP_RED, RED), (COP_BLUE, BLUE), (WEATHER_TABLES, WEATHER[0]), (WEATHER_TABLES + 4, WEATHER[1])] {
                    c.m.w32(at, table);
                }
                vec![]
            }),
            ("Night_InitNightDriving", 0x800d_c9d0, nfs4rc::f_800dc9d0, |c, r| {
                // (the night's set-up loads files the test CPUs don't have: only its other paths)
                if pick(r, 2) == 0 {
                    GAME_SETUP.set_time(&mut c.m, 0);
                } else {
                    GAME_SETUP.set_comm_mode(&mut c.m, SPLIT_SCREEN);
                }
                vec![]
            }),
            ("Night_KillNightDriving", 0x800d_cb6c, nfs4rc::f_800dcb6c, |c, r| {
                // (only blocks the heap holds can be freed: some reserved, the rest unset)
                for at in [NIGHT_FILE, PLAYER_TABLE, COP_RED, COP_BLUE, WEATHER_TABLES, WEATHER_TABLES + 4] {
                    let block = if pick(r, 2) == 0 { crate::abi::call(c, nfs4rc::f_800e5f28, &[PLAYER_TABLE_NAME, 256, 0]) } else { 0 };
                    c.m.w32(at, block);
                }
                vec![]
            }),
            ("Night_RestartNightDriving", 0x800d_cc0c, nfs4rc::f_800dcc0c, |_, _| vec![]),
            ("Night_SetEnviroment", 0x800d_cc94, nfs4rc::f_800dcc94, |_, _| vec![VIEW]),
            ("Night_AdditiveNightCalc", 0x800d_cda4, nfs4rc::f_800dcda4, |_, _| vec![VERTEX, COLOUR]),
        ]
    }

    #[test]
    fn night_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xE10 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let n = if name == "Night_GenerateAllLightTables" { 40 } else { 400 };
            crate::test_util::COMPARE_TICKS.set(true);
            against_original(name, n, 0xE11 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
