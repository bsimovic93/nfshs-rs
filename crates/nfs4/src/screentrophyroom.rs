//! SCREENTROPHYROOM.CPP: the trophy room (`tScreenTrophyRoom`): the tier's six or eight
//! trophies on shelves of three or four (each a picture of its trophy in the colour of the best
//! place won, read into a case's file once), the chosen one turning; up, down, left and right
//! move the choice, accept opens it (the trophy info); the chosen tournament's name and the
//! pads' hint.

use crate::abi::{self, Saved};
use crate::femenu::{COMMAND_GO_TO, KEY_ACCEPT, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, PLAY_FE_SFX};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, PAD_INFO, SONGS_SCREEN, TOURNAMENT_MANAGER};
use crate::{fefades, felines, fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

// The trophy room: a `tScreen` (+0 its permanent shapes' table, +40 its swapped ones (the chosen
// trophy's): +40 their table, +44 the file read, +58 uploaded (bit 0); +92 its fade), then
// +100 the trophies' shapes (`tShapeInformation`: +100 their table, +104 the case's file read),
// +140 how many trophies, +144 the tick the chosen one was swapped in, +156 the trophy swapped in,
// +160 cleared, +208 the tier, +212 each tier's chosen trophy (`short`), +216 the trophies shown
// (`short`s, ended by 0).
const SWAP_TABLE: u32 = 40;
const SWAP_DATA: u32 = 44;
const SWAP_UPLOADED: u32 = 58;
const FADE: u32 = 92;
const TROPHIES: u32 = 100;
const TROPHIES_DATA: u32 = 104;
const COUNT: u32 = 140;
const SWAPPED_AT: u32 = 144;
const PREVIOUS: u32 = 156;
const CLEAR: u32 = 160;
const TIER: u32 = 208;
const CHOSEN: u32 = 212;
const SHOWN: u32 = 216;

/// The front end's tier (`frontEnd + 0x122`).
const FE_TIER: u32 = FRONT_END + 0x122;
/// The tournament manager's definition (+24: each tier's first tournament at +2 of its 12
/// bytes; tournaments from +36, 84 bytes each, +0 their id) and best places (+496, by id).
const DEFINITION: u32 = 24;
const BEST_PLACES: u32 = 496;
/// The screen's vtable; the trophy's file's name; "zTrophy", "zCase2", "zCase"; the menu
/// accept goes to (in the menus' block); the permanent shapes' table now.
const TROPHY_ROOM_VT: u32 = 0x8001_1DD8;
const TROPHY_NAME: u32 = 0x8005_24F4;
const ROOM_NAME: u32 = 0x8001_1DBC;
const CASE2_NAME: u32 = 0x8001_1DC4;
const CASE_NAME: u32 = 0x8001_1DCC;
const INFO_MENU: u32 = 512;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
/// The pads (`gPadinfo`, 32 bytes each): +4 a second pad plugged in, +5 its kind (NeGcon 35).
const PAD_NEGCON: u8 = 35;
/// The tournaments' names (853 on), the hints (991 a pad's, 992 with a NeGcon and another pad,
/// 993 a NeGcon's).
const NAMES: u32 = 853;
/// `GetTrophyName`, `systemtask`, `FETexture_LoadPmxAtOffset`, `purgememadr`,
/// `ScaleShapeExtended`.
const GET_TROPHY_NAME: fn(&mut Cpu) = fetourn::get_trophy_name_entry;
const SYSTEM_TASK: fn(&mut Cpu) = nfs4rc::f_800e77f0;
const LOAD_PMX: fn(&mut Cpu) = crate::fetexture::load_pmx_at_offset_entry;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
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

/// `tScreenTrophyRoom::tScreenTrophyRoom` (SCREENTROPHYROOM.CPP, 0x80040950): a screen with the
/// room's vtable, no trophy swapped in, each tier's first chosen.
pub fn abi_screen_trophy_room(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8004_0964, &[this], fescreen::abi_screen);
        let m = &mut c.m;
        m.w32(this + 96, TROPHY_ROOM_VT);
        m.w8(this + PREVIOUS, 0);
        m.w16(this + CHOSEN, 0);
        m.w16(this + CHOSEN + 2, 0);
        c.r[2] = this;
    })
}

/// `tScreenTrophyRoom::~tScreenTrophyRoom` (SCREENTROPHYROOM.CPP, 0x80040990): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + 96, TROPHY_ROOM_VT);
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_09a8, &[this, flags], fescreen::abi_screen_destructor);
    })
}

/// `tScreenTrophyRoom::GetShapeInfo` (SCREENTROPHYROOM.CPP, 0x800409b8): 38 shapes of
/// "zTrophy", 32 of the chosen tournament's trophy.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, permanent, swap, name) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = name;
        r.0[2] = swap_name;
        let m = &mut c.m;
        m.w16(permanent, 38);
        m.w16(swap, 32);
        r.0[0] = TROPHY_NAME;
        let tier = m.r8(FE_TIER) as u32;
        let k = m.r8(chosen(this, tier)) as u32;
        let t = tournament(m, k);
        r.call_leaving(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 0, TROPHY_NAME, u32::MAX], 0x8004_0a50);
        c.m.w32(name, ROOM_NAME);
        c.m.w32(swap_name, TROPHY_NAME);
    })
}

/// `tScreenTrophyRoom::ProcessInput` (SCREENTROPHYROOM.CPP, 0x80040a78): accept opens the
/// trophy's info; up and down move between the shelves (half the trophies over, kept in range),
/// left and right along them (round); then the screen's.
pub fn abi_process_input(c: &mut Cpu) {
    let (this, player, key, command) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[3] = player;
        r.0[1] = key;
        r.0[2] = command;
        let m = &mut c.m;
        if m.r32(key) == KEY_ACCEPT {
            let menus = m.r32(SONGS_SCREEN);
            m.w32(command, COMMAND_GO_TO);
            m.w32(command + 4, menus + INFO_MENU);
        }
        let k = m.r32(key);
        if k == KEY_UP || k == KEY_DOWN {
            let at = chosen_now(m, this);
            let n = lh(m, this + COUNT);
            let half = n / 2;
            let v = m.r16(at);
            let sound = if (v as i16 as i32) < half {
                m.w16(at, v.wrapping_add(half as u16));
                3
            } else {
                m.w16(at, v.wrapping_sub(half as u16));
                4
            };
            r.call_leaving(c, PLAY_FE_SFX, &[sound], 0x8004_0b34);
            let m = &mut c.m;
            let at = chosen_now(m, this);
            let v = lh(m, at);
            let n = lh(m, this + COUNT);
            let kept = if v > 0 {
                if v < n { v } else { n }
            } else if n <= 0 {
                n
            } else {
                v.max(0)
            };
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
        port(c, &r, 0x8002_68f8, 0x8004_0c6c, &[this, player, key, command], fescreen::abi_process_input);
    })
}

/// `tScreenTrophyRoom::PreLoad` (SCREENTROPHYROOM.CPP, 0x80040c88): the screen's files; the
/// case's file (a tier's: "zCase2", or "zCase") read for eight trophies.
pub fn abi_pre_load(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_67a0, 0x8004_0c9c, &[this], fescreen::abi_pre_load);
        c.m.w32(this + TROPHIES, 0);
        port(c, &r, 0x8002_640c, 0x8004_0cb0, &[this, this + TROPHIES, 8], fescreen::abi_initialize_shapes);
        let name = if c.m.r8(FE_TIER) != 0 { CASE2_NAME } else { CASE_NAME };
        port(c, &r, 0x8002_6260, 0x8004_0ce0, &[this, name, this + TROPHIES], fescreen::abi_async_load_shape_file);
    })
}

/// `tScreenTrophyRoom::Initialize` (SCREENTROPHYROOM.CPP, 0x80040cf0): the screen's files; the
/// case's file waited for; the tier's six (eight past the first tier) trophies shown, each read
/// from the case's file in the colour of the best place won; the file freed; the chosen trophy
/// kept in range, swapped in from now.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_60cc, 0x8004_0d10, &[this], fescreen::abi_initialize);
        loop {
            r.call_leaving(c, SYSTEM_TASK, &[0], 0x8004_0d18);
            if port(c, &r, 0x8002_5e2c, 0x8004_0d24, &[this, this + TROPHIES], fescreen::abi_is_shape_file_loaded) == 1 {
                break;
            }
        }
        let m = &mut c.m;
        let tier = m.r8(FE_TIER) as u32;
        m.w32(this + TIER, tier);
        let n = if m.r8(FE_TIER) != 0 { 8 } else { 6 };
        r.0[1] = 0;
        r.0[3] = FRONT_END;
        r.0[2] = TOURNAMENT_MANAGER;
        m.w16(this + COUNT, n);
        m.w32(this + CLEAR, 1);
        loop {
            let k = sx(r.0[1]);
            let m = &mut c.m;
            if k as i32 >= lh(m, this + COUNT) {
                break;
            }
            m.w16(this.wrapping_add(k << 1) + SHOWN, 1);
            let t = tournament(m, r.0[1] & 0xFF);
            let id = m.r8(t) as i8 as i32 as u32;
            let place = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32;
            let place = if ((place - 1) as u32) < 3 { place } else { 0 };
            r.0[1] = r.0[1].wrapping_add(1);
            let table = m.r32(this + TROPHIES).wrapping_add(k << 5);
            let data = m.r32(this + TROPHIES_DATA);
            m.w32(sp + 16, 0);
            r.call_leaving(c, LOAD_PMX, &[data, (k << 2).wrapping_add(place as u32), table, 0], 0x8004_0e28);
        }
        let data = c.m.r32(this + TROPHIES_DATA);
        r.call_leaving(c, PURGE, &[data], 0x8004_0e3c);
        let m = &mut c.m;
        m.w32(this + TROPHIES_DATA, 0);
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
    })
}

/// `tScreenTrophyRoom::Cleanup` (SCREENTROPHYROOM.CPP, 0x80040ec8): the trophies' shapes freed;
/// the screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_64b4, 0x8004_0ee0, &[this, this + TROPHIES], fescreen::abi_free_shapes);
        port(c, &r, 0x8002_61c0, 0x8004_0ee8, &[this], fescreen::abi_cleanup);
    })
}

/// The pads' hint: a NeGcon's (993), a NeGcon's with another pad (992), a pad's (991).
fn hint(m: &mut Mem) -> u32 {
    let (plugged0, kind0) = (m.r8(PAD_INFO + 4), m.r8(PAD_INFO + 5));
    let (plugged1, kind1) = (m.r8(PAD_INFO + 36), m.r8(PAD_INFO + 37));
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
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 112, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[1] = 0x8014_0000;
        let ticks = c.m.r32(GAME_TICKS) as i32;
        let m = &mut c.m;
        m.w32(sp + 36, 0x00CE_C844);
        let shape = ((ticks >> 4) % 10 + 28) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 1040, 16, 16, 0, 0, sp + 32], 0x8004_0f84);
        r.0[0] = FRONT_END;
        let cols: u32 = if c.m.r8(FE_TIER) != 0 { 4 } else { 3 };
        r.0[3] = cols;
        r.0[5] = 276u32.wrapping_sub((cols * 95) >> 1);
        let table = c.m.r32(PERMANENT_SHAPES);
        c.m.w32(sp + 16, 0);
        port(c, &r, 0x8002_5ce8, 0x8004_0fd8, &[this, 0, 24, table], fescreen::abi_draw_background_image);
        let fade = lh(&mut c.m, this + FADE) as u32;
        for (k, v) in [1, 3, fade, 30].into_iter().enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, v);
        }
        port(c, &r, 0x8004_c018, 0x8004_1010, &[0x0023_2323, 106, 57, 300], felines::abi_draw_bright_end_line);
        port(c, &r, 0x8004_130c, 0x8004_1018, &[this], abi_load_trophy);
        port(c, &r, 0x8002_5e2c, 0x8004_1024, &[this, this + SWAP_TABLE], fescreen::abi_is_shape_file_loaded);
        if c.m.r32(this + SWAP_DATA) != 0 {
            port(c, &r, 0x8002_5ff0, 0x8004_1040, &[this, 32], fescreen::abi_upload_swap_shapes);
            let ticks = c.m.r32(GAME_TICKS);
            c.m.w32(this + SWAPPED_AT, ticks);
        }
        // the tournament's name, the hint
        let m = &mut c.m;
        let at = chosen_now(m, this);
        let k = m.r8(at) as u32;
        let t = tournament(m, k);
        let id = (m.r8(t) as i8 as i32 as u32).wrapping_add(NAMES);
        let fade = lh(m, this + FADE) as u32;
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[fade, sx(id), 256, 47, 2, 2, 4], 0x8004_10d4);
        let id = hint(&mut c.m);
        r.0[1] = 0;
        let w = r.call_leaving(c, TEXT_WORD, &[id], 0x8004_1170);
        r.0[4] = sp + 56;
        r.0[0] = w;
        let fade = lh(&mut c.m, this + FADE) as u32;
        let color = port(c, &r, 0x8004_bd68, 0x8004_1188, &[0x0050_5050, fade], fefades::abi_fade_out);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, 256, 200, color, 0, 2], 0x8004_11a8);
        // the trophies
        loop {
            let k = sx(r.0[1]) as i32;
            if k >= lh(&mut c.m, this + COUNT) {
                break;
            }
            let (row, col) = (k / cols as i32, k % cols as i32);
            let x = r.0[5].wrapping_add((col * 95) as u32);
            let y = (row * 45 + 70) as u32;
            let m = &mut c.m;
            let at = chosen_now(m, this);
            let selected = k == lh(m, at) && m.r16(this + SWAP_UPLOADED) & 1 != 0;
            let (shape, x) = if selected {
                let table = m.r32(this + SWAP_TABLE);
                m.w32(sp + 76, table);
                let turned = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SWAPPED_AT)) as i32) / 12;
                ((turned % 32) as u32, x)
            } else {
                let table = m.r32(this + TROPHIES);
                m.w32(sp + 76, table);
                (sx(r.0[1]), sx(x))
            };
            let fade = lh(m, this + FADE) as u32;
            r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[shape, 1536, x, sx(y), fade, 0, sp + 56], 0x8004_12e0);
            r.0[1] = r.0[1].wrapping_add(1);
        }
    })
}

/// `tScreenTrophyRoom::LoadTrophy` (SCREENTROPHYROOM.CPP, 0x8004130c): a chosen trophy other
/// than the one swapped in: its picture read.
pub fn load_trophy(c: &mut Cpu, this: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let at = chosen_now(m, this);
        if lh(m, at) == m.r8(this + PREVIOUS) as i32 {
            return;
        }
        r.0[0] = TROPHY_NAME;
        let k = m.r8(at) as u32;
        let t = tournament(m, k);
        r.call_leaving(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 0, TROPHY_NAME, u32::MAX], 0x8004_13a8);
        port(c, &r, 0x8002_5dd8, 0x8004_13b4, &[this, TROPHY_NAME], fescreen::abi_async_load_swap_shape_file);
        let m = &mut c.m;
        let at = chosen_now(m, this);
        let v = m.r8(at);
        m.w8(this + PREVIOUS, v);
    })
}

pub fn abi_load_trophy(c: &mut Cpu) {
    load_trophy(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, TOP, pick, player, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn screentrophyroom_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7F1 + k as u64, shuffle, &args);
        }
    }
}
