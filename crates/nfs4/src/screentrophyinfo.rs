//! SCREENTROPHYINFO.CPP: a trophy's screen (`tScreenTrophyInfo`). It shows the trophy room's
//! chosen tournament: its trophy (the picture's 32 shapes swapped in), turning; its banner in
//! the colour of the best place won; the tournament's name; up to three paragraphs of its texts
//! (headings lit); its description; and two rows of studs.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::{call, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, STRLEN, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, TOURNAMENT_MANAGER};
use crate::types::{DrawShapeExtended, RECT, Screen};
use crate::{fefades, fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +100 the banner's colour.
const BANNER: u32 = 100;

/// The trophy room (`tScreenTrophyRoom *`): +208 its tier, +212 each tier's tournament
/// (`short`s, the low byte read).
const TROPHY_ROOM: u32 = 0x8005_24F0;
const ROOM_TIER: u32 = 208;
const ROOM_TOURNAMENTS: u32 = 212;
/// The front end's tier (`frontEnd + 0x122`).
const TIER: u32 = FRONT_END + 0x122;
/// The tournament manager's definition (+24: each tier's first tournament at +2 of its 12
/// bytes; the tournaments from +36, 84 bytes each, +0 their id) and best places (+496, by id).
const DEFINITION: u32 = 24;
const TOURNAMENTS: u32 = 36;
const TOURNAMENT_SIZE: u32 = 84;
const BEST_PLACES: u32 = 496;
/// The banners' colours (by place: none, first, second, third), the trophy's file's name.
const BANNER_COLORS: u32 = 0x8005_2508;
const TROPHY_NAME: u32 = 0x8005_251C;
/// "zSTI" (11 shapes; the trophy's picture 32); the colours' table and the name's colour's
/// index.
const INFO_NAME: u32 = 0x8001_1E34;
const SHAPES: u16 = 11;
const TROPHY_SHAPES: u32 = 32;
const COLORS: u32 = 0x8005_1B50;
const NAME_COLOR: u32 = 0x8005_1ABD;
/// The texts by tournament id: names (834 on), the three paragraphs (891, 929, 910 on) with
/// their headings (988, 990, 989), the descriptions (872 on).
const NAMES: u32 = 834;
const PARAGRAPHS: [(u32, u32); 3] = [(891, 988), (929, 990), (910, 989)];
const DESCRIPTIONS: u32 = 872;
/// The texts' grey.
const GREY: u32 = 0x0050_5050;
/// The shapes' flags: the trophy's (its own table, from its top left), the studs' (coloured,
/// from their top left).
const TROPHY_FLAGS: u32 = 0x600;
const STUD_FLAGS: u32 = 0x410;

/// `tTournamentManager::GetTrophyName` (FETOURN.CPP's port), `ScaleShapeExtended`.
const GET_TROPHY_NAME: fn(&mut Cpu) = fetourn::get_trophy_name_entry;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;

/// The trophy room's chosen tournament (`tTourneyInfo *`).
fn tournament(m: &mut Mem) -> u32 {
    let room = m.r32(TROPHY_ROOM);
    let tier = m.r32(room + ROOM_TIER);
    let current = m.r8(room.wrapping_add(tier << 1) + ROOM_TOURNAMENTS) as u32;
    let definition = m.r32(TOURNAMENT_MANAGER + DEFINITION);
    let tier = m.r8(TIER) as u32;
    let first = m.r8(definition.wrapping_add(tier * 12) + 2) as u32;
    definition.wrapping_add(first.wrapping_add(current).wrapping_mul(TOURNAMENT_SIZE)) + TOURNAMENTS
}

/// A tournament's id.
fn id(m: &mut Mem, tournament: u32) -> u32 {
    m.r8(tournament) as i8 as i32 as u32
}

/// `tScreenTrophyInfo::GetShapeInfo` (SCREENTROPHYINFO.CPP, 0x800413e8): the banner's colour
/// by the best place won; 11 shapes of "zSTI", 32 of the trophy's picture.
pub fn get_shape_info(c: &mut Cpu, this: Screen, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    let m = &mut c.m;
    let t = tournament(m);
    let id = id(m, t);
    let place = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32;
    let place = if (1..=3).contains(&place) { place as u32 } else { 0 };
    let color = m.r32(BANNER_COLORS + 4 * place);
    m.w32(this.0 + BANNER, color);
    call(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 2, TROPHY_NAME, u32::MAX]);
    let m = &mut c.m;
    m.w16(swap, TROPHY_SHAPES as u16);
    m.w32(swap_name, TROPHY_NAME);
    m.w32(name, INFO_NAME);
    m.w16(permanent, SHAPES);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, Screen(c.r[4]), c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTrophyInfo::DrawBackground` (SCREENTROPHYINFO.CPP, 0x80041518). The texts fade in
/// over the screen's second half. It draws the tournament's name, then each paragraph it has
/// with its heading lit: the first on one line, the others word-wrapped one under another. The
/// trophy's picture is uploaded once read; then come the description, the trophy turning, and
/// the studs.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let fade = this.screen_fade_val(&mut c.m) as i32;
    let bright = (fade << 1).clamp(0, 128) as u32;
    let texts = ((fade - 64).max(0) << 1) as u32;
    let m = &mut c.m;
    let t = tournament(m);
    let id = id(m, t);
    let k = m.r8(NAME_COLOR) as u32;
    let name_color = fefades::fade(m.r32(COLORS + 4 * k), 0, texts);
    let grey = fefades::fade(GREY, 0, texts);
    let w = call(c, TEXT_WORD, &[id.wrapping_add(NAMES)]);
    fetextrender::full_text_rgb(c, w, 30, 25, name_color, 3, 3);
    // the rectangle the texts wrap in, the trophy's shapes' options, the studs'
    with_buffer(c, 56, |c, at| {
        let (rect, trophy, studs) = (RECT(at), DrawShapeExtended(at + 8), DrawShapeExtended(at + 32));
        let mut y = 175u32;
        for (j, (text, heading)) in PARAGRAPHS.into_iter().enumerate() {
            let text = id.wrapping_add(text);
            let w = call(c, TEXT_WORD, &[text]);
            if call(c, STRLEN, &[w]) == 0 {
                continue;
            }
            fetextrender::menu_text_positioned_justify_fade(c, texts, heading, 140, y as i16 as i32 as u32, 1, 2, 4);
            let w = call(c, TEXT_WORD, &[text]);
            if j == 0 {
                fetextrender::full_text_rgb(c, w, 145, y, grey, 0, 0);
                y = 183;
            } else {
                set_rect(&mut c.m, rect, 145, y as i16, 347, 100);
                fetextrender::word_wrap_text_rgb(c, w, rect.0, grey);
                let w = call(c, TEXT_WORD, &[text]);
                let h = fetextrender::word_wrap_height(c, 347, w);
                if j == 1 {
                    y = y.wrapping_add(h);
                }
            }
        }
        // the trophy's picture, the description
        fescreen::is_shape_file_loaded(c, this, this.swap_shapes());
        if this.swap_shapes().file(&mut c.m) != 0 {
            fescreen::upload_swap_shapes(c, this, TROPHY_SHAPES);
        }
        set_rect(&mut c.m, rect, 35, 45, 452, 100);
        let w = call(c, TEXT_WORD, &[id.wrapping_add(DESCRIPTIONS)]);
        fetextrender::word_wrap_text_rgb_justify(c, w, rect, grey, 3, 0, 0);
        // the trophy turning
        let frame = (c.m.r32(GAME_TICKS) as i32 / 12) % 32;
        let table = this.swap_shapes().shapes(&mut c.m);
        trophy.set_custom_shapes(&mut c.m, table);
        call(c, SCALE_SHAPE_EXTENDED, &[frame as u32, TROPHY_FLAGS, 70, (-5i32) as u32, texts, 0, trophy.0]);
        // the studs: two rows, every third left out
        let banner = c.m.r32(this.0 + BANNER);
        studs.set_tint(&mut c.m, 0, banner as i32);
        for k in (1..30).chain(34..63) {
            if k % 3 != 0 {
                call(c, DRAW_SHAPE_EXTENDED, &[0, STUD_FLAGS, 2 * k, 0, bright, 0, studs.0]);
            }
        }
    })
}

fn set_rect(m: &mut Mem, rect: RECT, x: i16, y: i16, w: i16, h: i16) {
    rect.set_x(m, x);
    rect.set_y(m, y);
    rect.set_w(m, w);
    rect.set_h(m, h);
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTrophyInfo::~tScreenTrophyInfo` (SCREENTROPHYINFO.H, 0x800419c0): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The screen's swapped shapes (their table, the file read) and its fade.
    const SWAP_TABLE: u32 = 40;
    const SWAP_DATA: u32 = 44;
    const FADE: u32 = 92;

    /// The game's trophy info screen (its vtable the real one); the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_04AC;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen: the car select's swapped shapes (read or not), its
    /// fade about where the texts come, the banner's colour; the trophy room's tier and each
    /// tier's tournament, the front end's tier, the best places won; the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12) + SWAP_TABLE;
        let s = SCREEN;
        for k in (0..40).step_by(4) {
            let v = m.r32(from + k);
            m.w32(s + SWAP_TABLE + k, v);
        }
        let buffer = m.r32(from + 8);
        m.w32(s + SWAP_TABLE + 12, 0);
        m.w32(s + SWAP_DATA, [0, buffer][pick(rng, 2) as usize]);
        m.w16(s + FADE, [0, 32, 63, 64, 65, 128, pick(rng, 129)][pick(rng, 7) as usize] as u16);
        m.w32(s + BANNER, rng.next() as u32 & 0xFF_FFFF);
        let room = m.r32(TROPHY_ROOM);
        m.w32(room + ROOM_TIER, pick(rng, 2));
        for k in 0..2 {
            m.w16(room + ROOM_TOURNAMENTS + 2 * k, pick(rng, 3) as u16);
        }
        m.w8(TIER, pick(rng, 3) as u8);
        for k in 0..64 {
            m.w8(TOURNAMENT_MANAGER + BEST_PLACES + k, [0, 1, 2, 3, 4, 0xFF][pick(rng, 6) as usize]);
        }
        m.w32(GAME_TICKS, pick(rng, 1_000_000));
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTrophyInfo::GetShapeInfo", 0x8004_13e8, nfs4rc::f_800413e8, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTrophyInfo::DrawBackground", 0x8004_1518, nfs4rc::f_80041518, |_, _| vec![SCREEN]),
            ("~tScreenTrophyInfo", 0x8004_19c0, nfs4rc::f_800419c0, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screentrophyinfo_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7B0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7B1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
