//! SCREENTRACKINFO.CPP: a tournament's track screen (`tScreenTrackInfo`). It lists the
//! tournament's tracks (the one to race lit), four headings, the track's direction, mirroring,
//! time of day and weather, its country, the background picture, and a wall of ten TVs showing
//! the track (`TR%02d%c`, by time of day and weather) turned on one by one. Back turns the wall
//! off and, before the tournament's first race, gives its entry fee back.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, call_addr};
use crate::femenu::KEY_BACK;
use crate::femenuextended::{SET_ABR_MODE, SPRINTF};
use crate::globals::{FRONT_END, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::types::{Screen, TextureShapeInfo};
use crate::{fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +100 the track to race (`tTrackInfo`: +100 its number, +101
/// its direction, +102 mirrored, +103 its time of day, +104 its weather), +140 the wall's ten
/// TVs (FETV's), +620 the wall (`tVideoWall`).
const TRACK: u32 = 100;
const DIRECTION: u32 = 101;
const MIRRORED: u32 = 102;
const TIME_OF_DAY: u32 = 103;
const WEATHER: u32 = 104;
const TVS: u32 = 140;
const WALL: u32 = 620;
const WALL_TVS: u32 = 10;

/// The tournament manager: +4 the tier, +8 the tournament within it, +12 the race (from 0), +20
/// the money, +24 the definition (each tier's first tournament at +2 of its 12 bytes; the
/// tournaments from +36, 84 bytes each, +48 the entry fee).
const MANAGER_TIER: u32 = 4;
const TOURNAMENT: u32 = 8;
const RACE: u32 = 12;
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const ENTRY_FEE: u32 = 36 + 48;

/// The front end's tournament (`frontEnd + 0x122`: a tier, or 0) and its kind's, for a tier or
/// none (`+0x121`, `+0x120`).
const TIER: u32 = FRONT_END + 0x122;
const TIER_KIND: u32 = FRONT_END + 0x121;
const KIND: u32 = FRONT_END + 0x120;
/// The headings' four texts; the directions' and the settings' texts (by value); the wall's
/// turning order; the countries' texts (from 67, by the track's +42).
const HEADINGS: u32 = 0x8001_1F1C;
const DIRECTIONS: u32 = 0x8005_1E14;
const SETTINGS: u32 = 0x8005_1DFC;
const WALL_ORDER: u32 = 0x8005_2548;
const COUNTRIES: u32 = 67;
const COUNTRY: u32 = 42;
/// "TR%02d%c", "zInfo" (43 shapes; the track's 10 pictures); the swapped file's name.
const SWAP_FORMAT: u32 = 0x8001_1F10;
const INFO_NAME: u32 = 0x8001_1F08;
const SHAPES: u16 = 43;
const SWAP_NAME: u32 = 0x8005_3148;

/// `PSXDrawTransSquare`: the host's.
const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// `tTournamentManager::GetTrackToRace`, `::GetTrackList` (FETOURN.CPP's ports);
/// `tTrackManager::GetTrackByID`.
const GET_TRACK_TO_RACE: fn(&mut Cpu) = fetourn::get_track_to_race_entry;
const GET_TRACK_LIST: fn(&mut Cpu) = fetourn::get_track_list_entry;
const GET_TRACK_BY_ID: u32 = 0x8002_1E60;
/// The video wall's (FEVIDEOWALL's ports): `Initialize`, `UpdateImages`, `TurnOn`,
/// `TurnOffInstant`, `Update`, `Draw`.
const WALL_INITIALIZE: u32 = 0x8003_6988;
const WALL_UPDATE_IMAGES: u32 = 0x8003_6A38;
const WALL_TURN_ON: u32 = 0x8003_70F0;
const WALL_TURN_OFF_INSTANT: u32 = 0x8003_7074;
const WALL_UPDATE: u32 = 0x8003_6BDC;
const WALL_DRAW: u32 = 0x8003_6DAC;

/// The text at `table`'s entry `k` (a `short`).
fn text(m: &mut Mem, table: u32, k: u32) -> u32 {
    m.r16(table.wrapping_add(k << 1)) as i16 as i32 as u32
}

/// `tScreenTrackInfo::GetShapeInfo` (SCREENTRACKINFO.CPP, 0x800425d8): 43 shapes of "zInfo",
/// 10 of the track to race's pictures for its time of day and weather.
pub fn get_shape_info(c: &mut Cpu, this: Screen, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    c.m.w16(permanent, SHAPES);
    c.m.w16(swap, WALL_TVS as u16);
    call(c, GET_TRACK_TO_RACE, &[TOURNAMENT_MANAGER, this.0 + TRACK]);
    let m = &mut c.m;
    m.w32(name, INFO_NAME);
    let number = m.r8(this.0 + TRACK) as i8 as i32 as u32;
    let letter = 2 * m.r8(this.0 + TIME_OF_DAY) as u32 + m.r8(this.0 + WEATHER) as u32 + b'a' as u32;
    call(c, SPRINTF, &[SWAP_NAME, SWAP_FORMAT, number, letter]);
    c.m.w32(swap_name, SWAP_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, Screen(c.r[4]), c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTrackInfo::DrawBackground` (SCREENTRACKINFO.CPP, 0x80042678): the tournament's
/// tracks (the race's lit), the headings, the track's settings and country, the picture, a
/// band, the wall turning and drawn.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let number = c.m.r8(this.0 + TRACK) as i8 as i32 as u32;
    let track = call_addr(c, GET_TRACK_BY_ID, &[TRACK_MANAGER, number]);
    let m = &mut c.m;
    let tier = m.r8(TIER) as u32;
    let kind = m.r8(if tier != 0 { TIER_KIND } else { KIND }) as u32;
    // the tournament's tracks (a list of text ids ending in 0)
    let list = call(c, GET_TRACK_LIST, &[TOURNAMENT_MANAGER, tier, kind]);
    let mut k = 0;
    loop {
        let id = text(&mut c.m, list, k);
        if id == 0 {
            break;
        }
        let state = if k == c.m.r32(TOURNAMENT_MANAGER + RACE) { 2 } else { 1 };
        fetextrender::menu_text_positioned(c, id, 170, 143 + 9 * k, state, 4);
        k += 1;
    }
    for k in 0..4 {
        let id = text(&mut c.m, HEADINGS, k);
        fetextrender::menu_text_positioned(c, id, 340, 143 + 18 * k, 1, 4);
    }
    // the track's settings
    let setting = |c: &mut Cpu, table: u32, field: u32, y: u32| {
        let k = c.m.r8(this.0 + field) as u32;
        let id = text(&mut c.m, table, k);
        fetextrender::menu_text_positioned_justify(c, id, 480, y, 1, 2, 4);
    };
    setting(c, DIRECTIONS, DIRECTION, 152);
    setting(c, SETTINGS, MIRRORED, 170);
    setting(c, SETTINGS, TIME_OF_DAY, 188);
    setting(c, SETTINGS, WEATHER, 206);
    let country = c.m.r8(track + COUNTRY) as u32 + COUNTRIES;
    fetextrender::menu_text_positioned_justify(c, country, 478, 33, 1, 0, 11);
    // the picture, the band, the wall
    let table = this.perm_shapes().shapes(&mut c.m);
    fescreen::draw_background_image(c, this, 0, 33, table, 0);
    call_addr(c, DRAW_TRANS_SQUARE, &[0, 320, 30, 160, 10, 1]);
    call(c, SET_ABR_MODE, &[0]);
    call_addr(c, WALL_UPDATE, &[this.0 + WALL]);
    call_addr(c, WALL_DRAW, &[this.0 + WALL]);
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTrackInfo::Initialize` (SCREENTRACKINFO.CPP, 0x80042900): the screen's files read;
/// the wall of ten TVs set from the track's pictures and turning on.
pub fn initialize(c: &mut Cpu, this: Screen) {
    fescreen::initialize(c, this);
    let wall = this.0 + WALL;
    let table: TextureShapeInfo = this.swap_shapes().shapes(&mut c.m);
    call_addr(c, WALL_INITIALIZE, &[wall, this.0 + TVS, table.0, 0, WALL_TVS, WALL_ORDER, 0]);
    call_addr(c, WALL_UPDATE_IMAGES, &[wall]);
    call_addr(c, WALL_TURN_ON, &[wall]);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTrackInfo::ProcessInput` (SCREENTRACKINFO.CPP, 0x8004296c): back turns the wall
/// off; before the tournament's first race its entry fee is given back.
pub fn process_input(c: &mut Cpu, this: Screen, key: u32) {
    if c.m.r32(key) != KEY_BACK {
        return;
    }
    call_addr(c, WALL_TURN_OFF_INSTANT, &[this.0 + WALL]);
    let (m, t) = (&mut c.m, TOURNAMENT_MANAGER);
    if m.r32(t + RACE) != 0 {
        return;
    }
    let definition = m.r32(t + DEFINITION);
    let tier = m.r32(t + MANAGER_TIER);
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
    let k = first.wrapping_add(m.r32(t + TOURNAMENT));
    let fee = m.r32(definition.wrapping_add(k.wrapping_mul(84)) + ENTRY_FEE);
    let money = m.r32(t + MONEY).wrapping_add(fee);
    m.w32(t + MONEY, money);
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, Screen(c.r[4]), c.r[6]);
}

/// `tScreenTrackInfo::~tScreenTrackInfo` (SCREENTRACKINFO.H, 0x80042a08): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, pick, player, via_abi};
    use crate::globals::{FE_APP, GAME_TICKS};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's track info screen (its vtable the real one); the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8017_F92C;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen: the car select's shapes (its permanent ones and
    /// its swapped ones), the track to race and its settings, the wall of ten TVs (turning
    /// either way or not about each time a TV turns, its TVs on or off, available and valid
    /// or not, a sign or none); the tier, the tournament, the race and the money;
    /// the front end's tier and kind; the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12);
        let s = SCREEN;
        for k in (0..80).step_by(4) {
            let v = m.r32(from + k);
            m.w32(s + k, v);
        }
        m.w32(s + 12, 0);
        m.w32(s + 52, 0);
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        m.w8(s + TRACK, pick(rng, 10) as u8);
        for f in [DIRECTION, MIRRORED, TIME_OF_DAY, WEATHER] {
            m.w8(s + f, pick(rng, 2) as u8);
        }
        let w = s + WALL;
        m.w32(w, s + TVS);
        m.w16(w + 4, 0);
        m.w16(w + 6, 10);
        let swap = m.r32(s + 40);
        m.w32(w + 8, swap);
        m.w32(w + 12, WALL_ORDER);
        m.w32(w + 16, ticks.wrapping_sub([0, 7, 8, 40, 80, 200, pick(rng, 300)][pick(rng, 7) as usize]));
        m.w16(w + 20, [0xFFFF, 0, 1][pick(rng, 3) as usize]);
        m.w16(w + 22, 0);
        m.w32(w + 24, 0);
        m.w16(w + 28, [0xFFFF, pick(rng, 600) as u16][pick(rng, 2) as usize]);
        m.w16(w + 30, pick(rng, 2) as u16);
        m.w16(w + 32, [0, 4, 64, pick(rng, 65)][pick(rng, 4) as usize] as u16);
        m.w16(w + 34, pick(rng, 2) as u16);
        m.w32(w + 40, 0);
        m.w32(w + 52, pick(rng, 2));
        // (the TVs set from the shapes, as the game's are: the screen was never set up in the
        // front end's RAM, and TVs of any size draw over everything)
        for k in 0..10 {
            crate::abi::call(c, nfs4rc::f_80023168, &[s + TVS + 48 * k, swap, k]);
            c.m.w32(s + TVS + 48 * k, [0, 1, 1, 2, 3][pick(rng, 5) as usize]);
        }
        let m = &mut c.m;
        let t = TOURNAMENT_MANAGER;
        m.w32(t + MANAGER_TIER, pick(rng, 3));
        m.w32(t + TOURNAMENT, pick(rng, 3));
        m.w32(t + RACE, [0, 0, 1, pick(rng, 6)][pick(rng, 4) as usize]);
        m.w32(t + MONEY, rng.next() as u32 & 0xFFFF);
        m.w8(TIER, [0, 0, 1, 2][pick(rng, 4) as usize] as u8);
        m.w8(TIER_KIND, pick(rng, 3) as u8);
        m.w8(KIND, pick(rng, 3) as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTrackInfo::GetShapeInfo", 0x8004_25d8, nfs4rc::f_800425d8, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTrackInfo::DrawBackground", 0x8004_2678, nfs4rc::f_80042678, |_, _| vec![SCREEN]),
            ("tScreenTrackInfo::Initialize", 0x8004_2900, nfs4rc::f_80042900, |_, _| vec![SCREEN]),
            ("tScreenTrackInfo::ProcessInput", 0x8004_296c, nfs4rc::f_8004296c, |c, r| {
                if pick(r, 2) == 0 {
                    c.m.w32(KEY, KEY_BACK);
                }
                vec![SCREEN, player(r), KEY, COMMAND]
            }),
            ("~tScreenTrackInfo", 0x8004_2a08, nfs4rc::f_80042a08, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screentrackinfo_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7141 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7142 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
