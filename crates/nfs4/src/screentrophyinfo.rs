//! SCREENTROPHYINFO.CPP: a trophy's screen (`tScreenTrophyInfo`): the trophy room's chosen
//! tournament's trophy (its picture's 32 shapes swapped in), its banner in the colour of the
//! best place won, the tournament's name, up to three paragraphs of its texts (headings lit),
//! its description, the trophy turning, and two rows of studs.

use crate::abi::{self, Saved};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, STRLEN, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, TOURNAMENT_MANAGER};
use crate::{fefades, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen` (+40 its swapped shapes: +40 their table, +44 the file read; +92 its
// fade), then +100 the banner's colour.
const SWAP_TABLE: u32 = 40;
const SWAP_DATA: u32 = 44;
const FADE: u32 = 92;
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
/// "zSTI"; the colours' table and the name's colour's index.
const INFO_NAME: u32 = 0x8001_1E34;
const COLORS: u32 = 0x8005_1B50;
const NAME_COLOR: u32 = 0x8005_1ABD;
/// The texts by tournament id: names (834 on), the three paragraphs (891, 929, 910 on) with
/// their headings (988, 990, 989), the descriptions (872 on).
const NAMES: u32 = 834;
const PARAGRAPHS: [(u32, u32); 3] = [(891, 988), (929, 990), (910, 989)];
const DESCRIPTIONS: u32 = 872;
/// The texts' grey.
const GREY: u32 = 0x0050_5050;

/// `tTournamentManager::GetTrophyName` (FETOURN.CPP, still recompiled), `ScaleShapeExtended`.
const GET_TROPHY_NAME: fn(&mut Cpu) = nfs4rc::f_80033ca0;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = nfs4rc::f_8004ef70;

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

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

/// `tScreenTrophyInfo::GetShapeInfo` (SCREENTROPHYINFO.CPP, 0x800413e8): the banner's colour
/// by the best place won; 11 shapes of "zSTI", 32 of the trophy's picture.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, permanent, swap, name) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = permanent;
        r.0[1] = swap;
        r.0[2] = name;
        let m = &mut c.m;
        let t = tournament(m);
        let id = m.r8(t) as i8 as i32 as u32;
        let place = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32;
        let place = if ((place - 1) as u32) < 3 { place as u32 } else { 0 };
        let color = m.r32(BANNER_COLORS.wrapping_add(place << 2));
        m.w32(this + BANNER, color);
        r.0[0] = TROPHY_NAME;
        r.call_leaving(c, GET_TROPHY_NAME, &[TOURNAMENT_MANAGER, t, 2, TROPHY_NAME, u32::MAX], 0x8004_14d4);
        let m = &mut c.m;
        m.w16(swap, 32);
        m.w32(swap_name, TROPHY_NAME);
        m.w32(name, INFO_NAME);
        m.w16(permanent, 11);
    })
}

/// `tScreenTrophyInfo::DrawBackground` (SCREENTROPHYINFO.CPP, 0x80041518): the texts fading in
/// over the screen's second half; the tournament's name; each paragraph it has, its heading lit
/// (the first on a line, the others word-wrapped one under another); the trophy's picture
/// uploaded once read; the description; the trophy turning; the studs.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 120, |c, sp| {
        let mut r = Saved::of(c);
        r.0[6] = this;
        let fade = c.m.r16(this + FADE) as i16 as i32;
        r.0[4] = (fade << 1).clamp(0, 128) as u32;
        let texts = ((fade - 64).max(0) << 1) as u32;
        r.0[3] = texts;
        let m = &mut c.m;
        let t = tournament(m);
        let id = m.r8(t) as i8 as i32 as u32;
        r.0[5] = id;
        r.0[0] = t;
        let k = m.r8(NAME_COLOR) as u32;
        let name_color = m.r32(COLORS + (k << 2));
        let grey = |c: &mut Cpu, r: &Saved, ra: u32| port(c, r, 0x8004_bd68, ra, &[GREY, texts], fefades::abi_fade_out);
        let color = port(c, &r, 0x8004_bd68, 0x8004_15f4, &[name_color, texts], fefades::abi_fade_out);
        r.0[1] = 175;
        r.0[0] = color;
        let w = r.call_leaving(c, TEXT_WORD, &[id.wrapping_add(NAMES)], 0x8004_1608);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, 30, 25, color, 3, 3], 0x8004_1628);
        // the paragraphs
        let ras = [
            [0x8004_1634, 0x8004_163c, 0x8004_166c, 0x8004_1674, 0x8004_1688, 0x8004_16a4, 0, 0],
            [0x8004_16b4, 0x8004_16bc, 0x8004_16f0, 0x8004_1714, 0x8004_1728, 0x8004_1738, 0x8004_1740, 0x8004_174c],
            [0x8004_175c, 0x8004_1764, 0x8004_1798, 0x8004_17bc, 0x8004_17d0, 0x8004_17e0, 0x8004_17e8, 0x8004_17f4],
        ];
        let rect = sp + 32;
        for (j, ((text, heading), ras)) in PARAGRAPHS.into_iter().zip(ras).enumerate() {
            let text = id.wrapping_add(text);
            if j == 0 {
                r.0[0] = text;
            } else {
                r.0[2] = text;
            }
            let w = r.call_leaving(c, TEXT_WORD, &[text], ras[0]);
            if r.call_leaving(c, STRLEN, &[w], ras[1]) == 0 {
                continue;
            }
            let y = if j == 0 { r.0[1] } else { r.0[1] as i16 as i32 as u32 };
            r.call_leaving(c, menu_text_positioned_justify_fade_entry, &[texts, heading, 140, y, 1, 2, 4], ras[2]);
            if j == 0 {
                let w = r.call_leaving(c, TEXT_WORD, &[text], ras[3]);
                r.0[0] = w;
                let color = grey(c, &r, ras[4]);
                r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, 145, r.0[1], color, 0, 0], ras[5]);
                r.0[1] = 183;
            } else {
                for (k, v) in [(0, 145u16), (4, 347), (2, r.0[1] as u16), (6, 100)] {
                    c.m.w16(rect + k, v);
                }
                let w = r.call_leaving(c, TEXT_WORD, &[text], ras[3]);
                r.0[0] = w;
                let color = grey(c, &r, ras[4]);
                r.call_leaving(c, fetextrender::word_wrap_text_rgb_entry, &[w, rect, color], ras[5]);
                let w = r.call_leaving(c, TEXT_WORD, &[text], ras[6]);
                let h = r.call_leaving(c, fetextrender::word_wrap_height_entry, &[347, w], ras[7]);
                if j == 1 {
                    r.0[1] = r.0[1].wrapping_add(h);
                }
            }
        }
        // the trophy's picture, the description
        port(c, &r, 0x8002_5e2c, 0x8004_1800, &[this, this + SWAP_TABLE], fescreen::abi_is_shape_file_loaded);
        if c.m.r32(this + SWAP_DATA) != 0 {
            port(c, &r, 0x8002_5ff0, 0x8004_181c, &[this, 32], fescreen::abi_upload_swap_shapes);
        }
        for (k, v) in [(0, 35u16), (2, 45), (4, 452), (6, 100)] {
            c.m.w16(rect + k, v);
        }
        let w = r.call_leaving(c, TEXT_WORD, &[id.wrapping_add(DESCRIPTIONS)], 0x8004_1844);
        r.0[0] = w;
        let color = grey(c, &r, 0x8004_1858);
        r.call_leaving(c, fetextrender::word_wrap_text_rgb_justify_entry, &[w, rect, color, 3, 0, 0], 0x8004_1874);
        // the trophy turning
        let frame = (c.m.r32(GAME_TICKS) as i32 / 12) % 32;
        let table = c.m.r32(this + SWAP_TABLE);
        c.m.w32(sp + 60, table);
        r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[frame as u32, 1536, 70, (-5i32) as u32, texts, 0, sp + 40], 0x8004_18d8);
        // the studs
        r.0[2] = 0x5555_5556;
        let banner = c.m.r32(this + BANNER);
        r.0[1] = sp + 64;
        c.m.w32(sp + 68, banner);
        for (from, to, ra) in [(1, 30, 0x8004_192c), (34, 63, 0x8004_1988)] {
            for k in from..to {
                r.0[0] = k;
                if k % 3 != 0 {
                    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[0, 1040, 2 * k, 0, r.0[4], 0, sp + 64], ra);
                }
            }
        }
    })
}

/// `tScreenTrophyInfo::~tScreenTrophyInfo` (SCREENTROPHYINFO.H, 0x800419c0): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_19d0, &[this, flags], fescreen::abi_screen_destructor);
    })
}

use fetextrender::menu_text_positioned_justify_fade_entry;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn screentrophyinfo_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7C1 + k as u64, shuffle, &args);
        }
    }
}
