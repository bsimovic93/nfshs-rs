//! SCREENTROPHYROOM.CPP: the trophy room (`tScreenTrophyRoom`). It shows the tier's six or
//! eight trophies on shelves of three or four, each a picture of its trophy in the colour of
//! the best place won (read once from a case's file); the chosen one turns. Up, down, left and
//! right move the choice, and accept opens it (the trophy info). It also shows the chosen
//! tournament's name and the pads' hint.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, call_addr, with_buffer};
use crate::femenu::{COMMAND_GO_TO, KEY_ACCEPT, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, PLAY_FE_SFX};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, PAD_INFO, SONGS_SCREEN, TOURNAMENT_MANAGER};
use crate::types::{DrawShapeExtended, Menu, MenuCommand, Screen, ShapeInformation, TextureShapeInfo};
use crate::{fefades, fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

/// The trophy room: a `tScreen` (its swapped shapes the chosen trophy's), then +100 the
/// trophies' shapes (a `tShapeInformation`, read from the case's file), +140 how many trophies,
/// +144 the tick the chosen one was swapped in, +156 the trophy swapped in, +160 cleared, +208
/// the tier, +212 each tier's chosen trophy (`short`), +216 the trophies shown (`short`s, ended
/// by 0).
const TROPHIES: u32 = 100;
const COUNT: u32 = 140;
const SWAPPED_AT: u32 = 144;
const PREVIOUS: u32 = 156;
const CLEAR: u32 = 160;
const TIER: u32 = 208;
const CHOSEN: u32 = 212;
const SHOWN: u32 = 216;
/// A shape file's flags: uploaded.
const UPLOADED: u16 = 1;

/// The front end's tier (`frontEnd + 0x122`).
const FE_TIER: u32 = FRONT_END + 0x122;
/// The tournament manager's definition (+24: each tier's first tournament at +2 of its 12
/// bytes; tournaments from +36, 84 bytes each, +0 their id) and best places (+496, by id).
const DEFINITION: u32 = 24;
const BEST_PLACES: u32 = 496;
/// The screen's vtable; the trophy's file's name; "zTrophy" (38 shapes; a trophy's picture
/// 32), "zCase2", "zCase" (8 trophies, 4 colours each); the menu accept goes to (in the menus'
/// block); the permanent shapes' table now.
const TROPHY_ROOM_VT: u32 = 0x8001_1DD8;
const TROPHY_NAME: u32 = 0x8005_24F4;
const ROOM_NAME: u32 = 0x8001_1DBC;
const SHAPES: u16 = 38;
const TROPHY_SHAPES: u32 = 32;
const CASE2_NAME: u32 = 0x8001_1DC4;
const CASE_NAME: u32 = 0x8001_1DCC;
const CASE_TROPHIES: u32 = 8;
const INFO_MENU: u32 = 512;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
/// The pads (`gPadinfo`, 32 bytes each): +4 plugged in, +5 its kind (NeGcon 35).
const PAD_PLUGGED: u32 = 4;
const PAD_KIND: u32 = 5;
const PAD_SIZE: u32 = 32;
const PAD_NEGCON: u8 = 35;
/// The tournaments' names (853 on), the hints (991 a pad's, 992 with a NeGcon and another pad,
/// 993 a NeGcon's).
const NAMES: u32 = 853;
/// The shelves: trophies 95 apart about x 276, shelves 45 apart from y 70.
const SHELF_X: u32 = 276;
const TROPHY_WIDTH: i32 = 95;
const SHELF_Y: i32 = 70;
const SHELF_HEIGHT: i32 = 45;
/// The sounds up and down.
const SOUND_UP: u32 = 3;
const SOUND_DOWN: u32 = 4;
/// The shapes' flags: the spinning picture's (coloured, from its top left); the trophies' (their
/// own table, from their top left).
const PICTURE_FLAGS: u32 = 0x410;
const TROPHY_FLAGS: u32 = 0x600;
/// `GetTrophyName`; `systemtask` (hooked: the host lets time pass in wait loops), called by
/// address; `FETexture_LoadPmxAtOffset`, `purgememadr`, `ScaleShapeExtended`; `DrawBrightEndLine`
/// (FELINES's port).
const GET_TROPHY_NAME: fn(&mut Cpu) = fetourn::get_trophy_name_entry;
const SYSTEM_TASK: u32 = 0x800E_77F0;
const LOAD_PMX: fn(&mut Cpu) = crate::fetexture::load_pmx_at_offset_entry;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;
const DRAW_BRIGHT_END_LINE: u32 = 0x8004_C018;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// Tournament `k` of the front end's tier (`tTourneyInfo *`).
fn tournament(m: &mut Mem, k: u32) -> u32 {
    let definition = m.r32(TOURNAMENT_MANAGER + DEFINITION);
    let tier = m.r8(FE_TIER) as u32;
    let first = m.r8(definition.wrapping_add(tier * 12) + 2) as u32;
    definition.wrapping_add(first.wrapping_add(k).wrapping_mul(84)) + 36
}

/// The chosen trophy of tier `tier` (`fRealCurrentTourn[tier]`'s place).
fn chosen(this: u32, tier: u32) -> u32 {
    this.wrapping_add(tier << 1) + CHOSEN
}

/// The chosen trophy of the room's tier.
fn chosen_now(m: &mut Mem, this: u32) -> u32 {
    let tier = m.r32(this + TIER);
    chosen(this, tier)
}

/// The trophies' shapes.
fn trophies(this: u32) -> ShapeInformation {
    ShapeInformation(this + TROPHIES)
}

/// `tScreenTrophyRoom::tScreenTrophyRoom` (SCREENTROPHYROOM.CPP, 0x80040950): a screen with the
/// room's vtable, no trophy swapped in, each tier's first chosen.
pub fn screen_trophy_room(c: &mut Cpu, this: Screen) -> Screen {
    fescreen::screen(c, this);
    let m = &mut c.m;
    this.set_vtable(m, TROPHY_ROOM_VT);
    m.w8(this.0 + PREVIOUS, 0);
    m.w16(this.0 + CHOSEN, 0);
    m.w16(this.0 + CHOSEN + 2, 0);
    this
}

pub fn abi_screen_trophy_room(c: &mut Cpu) {
    c.r[2] = screen_trophy_room(c, Screen(c.r[4])).0;
}

/// `tScreenTrophyRoom::~tScreenTrophyRoom` (SCREENTROPHYROOM.CPP, 0x80040990): the screen's.
pub fn destructor(c: &mut Cpu, this: Screen, flags: u32) {
    this.set_vtable(&mut c.m, TROPHY_ROOM_VT);
    fescreen::screen_destructor(c, this, flags);
}

pub fn abi_destructor(c: &mut Cpu) {
    destructor(c, Screen(c.r[4]), c.r[5]);
}

/// `tScreenTrophyRoom::GetShapeInfo` (SCREENTROPHYROOM.CPP, 0x800409b8): 38 shapes of
/// "zTrophy", 32 of the chosen tournament's trophy (the front end's tier's choice).
pub fn get_shape_info(c: &mut Cpu, this: Screen, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    let m = &mut c.m;
    m.w16(permanent, SHAPES);
    m.w16(swap, TROPHY_SHAPES as u16);
    let tier = m.r8(FE_TIER) as u32;
    let k = m.r8(chosen(this.0, tier)) as u32;
    let t = tournament(m, k);
    call(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 0, TROPHY_NAME, u32::MAX]);
    c.m.w32(name, ROOM_NAME);
    c.m.w32(swap_name, TROPHY_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, Screen(c.r[4]), c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTrophyRoom::ProcessInput` (SCREENTROPHYROOM.CPP, 0x80040a78): accept opens the
/// trophy's info; up and down move between the shelves (half the trophies over, then kept from
/// 0 to the count); left and right along them (round). (The screen's own does nothing.)
pub fn process_input(c: &mut Cpu, this: Screen, key: u32, command: MenuCommand) {
    let (this, m) = (this.0, &mut c.m);
    if m.r32(key) == KEY_ACCEPT {
        let menus = m.r32(SONGS_SCREEN);
        command.set_type_(m, COMMAND_GO_TO as i32);
        command.set_next_menu(m, Menu(menus + INFO_MENU));
    }
    let k = m.r32(key);
    if k == KEY_UP || k == KEY_DOWN {
        let at = chosen_now(m, this);
        let half = lh(m, this + COUNT) / 2;
        let v = m.r16(at);
        let sound = if (v as i16 as i32) < half {
            m.w16(at, v.wrapping_add(half as u16));
            SOUND_UP
        } else {
            m.w16(at, v.wrapping_sub(half as u16));
            SOUND_DOWN
        };
        call(c, PLAY_FE_SFX, &[sound]);
        let m = &mut c.m;
        let at = chosen_now(m, this);
        let kept = lh(m, at).max(0).min(lh(m, this + COUNT));
        m.w16(at, kept as u16);
    }
    let m = &mut c.m;
    if m.r32(key) == KEY_LEFT {
        let at = chosen_now(m, this);
        let v = m.r16(at).wrapping_sub(1);
        m.w16(at, v);
        let at = chosen_now(m, this);
        if lh(m, at) < 0 {
            let v = m.r16(this + COUNT).wrapping_sub(1);
            m.w16(at, v);
        }
    }
    if m.r32(key) == KEY_RIGHT {
        let at = chosen_now(m, this);
        let v = m.r16(at).wrapping_add(1);
        m.w16(at, v);
        let at = chosen_now(m, this);
        if lh(m, at) >= lh(m, this + COUNT) {
            m.w16(at, 0);
        }
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, Screen(c.r[4]), c.r[6], MenuCommand(c.r[7]));
}

/// `tScreenTrophyRoom::PreLoad` (SCREENTROPHYROOM.CPP, 0x80040c88): the screen's files; the
/// case's file (a tier's: "zCase2", or "zCase") read for eight trophies.
pub fn pre_load(c: &mut Cpu, this: Screen) {
    fescreen::pre_load(c, this);
    let trophies = trophies(this.0);
    trophies.set_shapes(&mut c.m, TextureShapeInfo(0));
    fescreen::initialize_shapes(c, trophies, CASE_TROPHIES);
    let name = if c.m.r8(FE_TIER) != 0 { CASE2_NAME } else { CASE_NAME };
    fescreen::async_load_shape_file(c, this, name, trophies);
}

pub fn abi_pre_load(c: &mut Cpu) {
    pre_load(c, Screen(c.r[4]));
}

/// `tScreenTrophyRoom::Initialize` (SCREENTROPHYROOM.CPP, 0x80040cf0). The screen's files are
/// read and the case's file waited for. The tier's six (eight past the first tier) trophies are
/// shown, each read from the case's file in the colour of the best place won, and the file is
/// freed. The chosen trophy is kept in range and swapped in from now.
pub fn initialize(c: &mut Cpu, this: Screen) {
    fescreen::initialize(c, this);
    let (this, trophies) = (this.0, trophies(this.0));
    loop {
        call_addr(c, SYSTEM_TASK, &[0]);
        if fescreen::is_shape_file_loaded(c, Screen(this), trophies) {
            break;
        }
    }
    let m = &mut c.m;
    let tier = m.r8(FE_TIER) as u32;
    m.w32(this + TIER, tier);
    let n = if m.r8(FE_TIER) != 0 { 8 } else { 6 };
    m.w16(this + COUNT, n);
    m.w32(this + CLEAR, 1);
    let mut k = 0u32;
    while (k as i32) < lh(&mut c.m, this + COUNT) {
        let m = &mut c.m;
        m.w16(this.wrapping_add(k << 1) + SHOWN, 1);
        let t = tournament(m, k & 0xFF);
        let id = m.r8(t) as i8 as i32 as u32;
        let place = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32;
        let place = if (1..=3).contains(&place) { place as u32 } else { 0 };
        let shape = trophies.shapes(m).at(k);
        let file = trophies.file(m);
        call(c, LOAD_PMX, &[file, (k << 2).wrapping_add(place), shape.0, 0, 0]);
        k += 1;
    }
    let file = trophies.file(&mut c.m);
    call(c, PURGE, &[file]);
    let m = &mut c.m;
    trophies.set_file(m, 0);
    let n = lh(m, this + COUNT);
    m.w16(this.wrapping_add((n as u32) << 1) + SHOWN, 0);
    let at = chosen_now(m, this);
    let v = lh(m, at).min(n - 1);
    m.w16(at, v as u16);
    let at = chosen_now(m, this);
    let v = m.r8(at);
    m.w8(this + PREVIOUS, v);
    let ticks = m.r32(GAME_TICKS);
    m.w32(this + SWAPPED_AT, ticks);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTrophyRoom::Cleanup` (SCREENTROPHYROOM.CPP, 0x80040ec8): the trophies' shapes freed;
/// the screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    fescreen::free_shapes(c, this, trophies(this.0));
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// The pads' hint: a NeGcon's with another pad (992), a NeGcon's (993), a pad's (991).
fn hint(m: &mut Mem) -> u32 {
    let pad = |m: &mut Mem, k: u32| (m.r8(PAD_INFO + PAD_SIZE * k + PAD_PLUGGED), m.r8(PAD_INFO + PAD_SIZE * k + PAD_KIND));
    let ((plugged0, kind0), (plugged1, kind1)) = (pad(m, 0), pad(m, 1));
    if (kind0 == PAD_NEGCON && (kind1 == PAD_NEGCON || plugged1 != 0)) || (kind1 == PAD_NEGCON && plugged0 != 0) {
        992
    } else if kind0 == PAD_NEGCON || kind1 == PAD_NEGCON {
        993
    } else {
        991
    }
}

/// `tScreenTrophyRoom::DrawBackground` (SCREENTROPHYROOM.CPP, 0x80040ef8): the spinning
/// picture; the room; a line; the chosen trophy swapped in (and uploaded); the tournament's
/// name, the pads' hint; the trophies on their shelves, the chosen one turning once uploaded.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let ticks = c.m.r32(GAME_TICKS) as i32;
    with_buffer(c, DrawShapeExtended::SIZE, |c, at| {
        DrawShapeExtended(at).set_tint(&mut c.m, 0, 0x00CE_C844);
        let shape = ((ticks >> 4) % 10 + 28) as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[shape, PICTURE_FLAGS, 16, 16, 0, 0, at]);
    });
    let cols: u32 = if c.m.r8(FE_TIER) != 0 { 4 } else { 3 };
    let left = SHELF_X.wrapping_sub((cols * TROPHY_WIDTH as u32) >> 1);
    let table = TextureShapeInfo(c.m.r32(PERMANENT_SHAPES));
    fescreen::draw_background_image(c, this, 0, 24, table, 0);
    let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
    call_addr(c, DRAW_BRIGHT_END_LINE, &[0x0023_2323, 106, 57, 300, 1, 3, fade, 30]);
    load_trophy(c, this.0);
    fescreen::is_shape_file_loaded(c, this, this.swap_shapes());
    if this.swap_shapes().file(&mut c.m) != 0 {
        fescreen::upload_swap_shapes(c, this, TROPHY_SHAPES);
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(this.0 + SWAPPED_AT, ticks);
    }
    // the tournament's name, the hint
    let m = &mut c.m;
    let at = chosen_now(m, this.0);
    let k = m.r8(at) as u32;
    let t = tournament(m, k);
    let id = (m.r8(t) as i8 as i32 as u32).wrapping_add(NAMES);
    let fade = this.screen_fade_val(m) as i32 as u32;
    fetextrender::menu_text_positioned_justify_fade(c, fade, sx(id), 256, 47, 2, 2, 4);
    let id = hint(&mut c.m);
    let w = call(c, TEXT_WORD, &[id]);
    let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
    let color = fefades::fade(0x0050_5050, 0, fade);
    fetextrender::full_text_rgb(c, w, 256, 200, color, 0, 2);
    // the trophies: the chosen one turning from the swapped file once uploaded, the others still
    with_buffer(c, DrawShapeExtended::SIZE, |c, buffer| {
        let options = DrawShapeExtended(buffer);
        let mut k = 0u32;
        while (k as i32) < lh(&mut c.m, this.0 + COUNT) {
            let (row, col) = (k as i32 / cols as i32, k as i32 % cols as i32);
            let x = left.wrapping_add((col * TROPHY_WIDTH) as u32);
            let y = (row * SHELF_HEIGHT + SHELF_Y) as u32;
            let m = &mut c.m;
            let chosen = chosen_now(m, this.0);
            let selected = k as i32 == lh(m, chosen) && this.swap_shapes().flags(m) & UPLOADED != 0;
            let (shape, x) = if selected {
                let table = this.swap_shapes().shapes(m);
                options.set_custom_shapes(m, table);
                let turned = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this.0 + SWAPPED_AT)) as i32) / 12;
                ((turned % 32) as u32, x)
            } else {
                let table = trophies(this.0).shapes(m);
                options.set_custom_shapes(m, table);
                (sx(k), sx(x))
            };
            let fade = this.screen_fade_val(m) as i32 as u32;
            call(c, SCALE_SHAPE_EXTENDED, &[shape, TROPHY_FLAGS, x, sx(y), fade, 0, options.0]);
            k += 1;
        }
    });
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTrophyRoom::LoadTrophy` (SCREENTROPHYROOM.CPP, 0x8004130c): a chosen trophy other
/// than the one swapped in: its picture read.
pub fn load_trophy(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let at = chosen_now(m, this);
    if lh(m, at) == m.r8(this + PREVIOUS) as i32 {
        return;
    }
    let k = m.r8(at) as u32;
    let t = tournament(m, k);
    call(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 0, TROPHY_NAME, u32::MAX]);
    fescreen::async_load_swap_shape_file(c, Screen(this), TROPHY_NAME);
    let m = &mut c.m;
    let at = chosen_now(m, this);
    let v = m.r8(at);
    m.w8(this + PREVIOUS, v);
}

pub fn abi_load_trophy(c: &mut Cpu) {
    load_trophy(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, pick, player, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The room's swapped shapes (their table, the file read, uploaded), its fade, the trophies'
    /// file read.
    const SWAP_TABLE: u32 = 40;
    const SWAP_DATA: u32 = 44;
    const SWAP_UPLOADED: u32 = 58;
    const FADE: u32 = 92;
    const TROPHIES_DATA: u32 = 104;

    /// The game's trophy room; the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_0354;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the room: the car select's shapes as the trophies' and the
    /// swapped ones (read or not, uploaded or not), its fade; the tier and each tier's chosen
    /// trophy about the shelves' ends, the trophy swapped in that one or another, how many
    /// trophies; the front end's tier, the best places won; the pads plugged in and their kinds
    /// (NeGcons or not); the clock.
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
        m.w32(s + SWAP_TABLE + 12, 0);
        m.w32(s + 36, 0);
        m.w32(s + SWAP_TABLE + 36, 0);
        let buffer = m.r32(from + SWAP_TABLE + 8);
        m.w32(s + SWAP_DATA, [0, buffer][pick(rng, 2) as usize]);
        let up = (m.r16(s + SWAP_UPLOADED) & !1) | pick(rng, 2) as u16;
        m.w16(s + SWAP_UPLOADED, up);
        let perm = m.r32(from);
        m.w32(s + TROPHIES, perm);
        m.w32(s + TROPHIES + 12, 0);
        m.w32(s + TROPHIES + 36, 0);
        m.w32(s + TROPHIES_DATA, 0);
        m.w16(s + FADE, [0, 64, 128, pick(rng, 129)][pick(rng, 4) as usize] as u16);
        let n = [6, 8, pick(rng, 9)][pick(rng, 3) as usize];
        m.w16(s + COUNT, n as u16);
        m.w32(s + TIER, pick(rng, 2));
        for k in 0..2 {
            let v = [0xFFFF, 0, 2, 3, 4, n.wrapping_sub(1), n, pick(rng, 9)][pick(rng, 8) as usize];
            m.w16(s + CHOSEN + 2 * k, v as u16);
        }
        let at = chosen(s, m.r32(s + TIER));
        let prev = if pick(rng, 2) == 0 { m.r8(at) } else { pick(rng, 9) as u8 };
        m.w8(s + PREVIOUS, prev);
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        m.w32(s + SWAPPED_AT, ticks.wrapping_sub(pick(rng, 2000)));
        m.w8(FE_TIER, pick(rng, 2) as u8);
        for k in 0..64 {
            m.w8(TOURNAMENT_MANAGER + BEST_PLACES + k, [0, 1, 2, 3, 4, 0xFF][pick(rng, 6) as usize]);
        }
        for p in 0..2 {
            m.w8(PAD_INFO + 32 * p + 4, pick(rng, 2) as u8);
            m.w8(PAD_INFO + 32 * p + 5, [0x41, 0x73, PAD_NEGCON as u32, 0][pick(rng, 4) as usize] as u8);
        }
    }

    fn key(c: &mut Cpu, r: &mut Rng) {
        let k = [KEY_ACCEPT, KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, 8][pick(r, 6) as usize];
        c.m.w32(KEY, k);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTrophyRoom", 0x8004_0950, nfs4rc::f_80040950, |_, _| vec![OUT + 0x100]),
            ("~tScreenTrophyRoom", 0x8004_0990, nfs4rc::f_80040990, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
            ("tScreenTrophyRoom::GetShapeInfo", 0x8004_09b8, nfs4rc::f_800409b8, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTrophyRoom::ProcessInput", 0x8004_0a78, nfs4rc::f_80040a78, |c, r| {
                key(c, r);
                vec![SCREEN, player(r), KEY, COMMAND]
            }),
            ("tScreenTrophyRoom::PreLoad", 0x8004_0c88, nfs4rc::f_80040c88, |_, _| vec![SCREEN]),
            ("tScreenTrophyRoom::Cleanup", 0x8004_0ec8, nfs4rc::f_80040ec8, |_, _| vec![SCREEN]),
            ("tScreenTrophyRoom::DrawBackground", 0x8004_0ef8, nfs4rc::f_80040ef8, |_, _| vec![SCREEN]),
            ("LoadTrophy", 0x8004_130c, nfs4rc::f_8004130c, |_, _| vec![SCREEN]),
        ]
    }

    #[test]
    fn screentrophyroom_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7E0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7E1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
