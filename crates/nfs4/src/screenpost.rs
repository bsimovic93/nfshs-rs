//! SCREENPOST.CPP: the screens after a race. `DrawMoney` (an amount and its currency word);
//! the tournament's standings (`tScreenTournamentStandings`, and the three-item one): each
//! racer's place, name and points (or knock-out mark), the track and the tournament, the money
//! won, the damage billed and the bonus counted down into the total; the pink slips' standings
//! (`tScreenPinkSlipStandings`): the tracks raced (the one chosen lit) with their winners, the
//! two players' wins.
//!
//! `PSXDrawSquare`: the host's (called by address).

use crate::abi::{self, Saved};
use crate::femenu::{KEY_ACCEPT, KEY_NONE, PLAY_FE_SFX};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, SONGS_SCREEN, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::{fefades, felines, fescreen, fetextrender, fetools, fetracks};
use ps1rt::{Cpu, Mem};

// The standings: a `tScreen` (+92 its fade), then +100 the final money, +104 the money won, +108
// the damage billed, +112 the bonus, +116 the tick it came up, +120 money won, +124 a bonus, +128
// billed, +132 the money shown, +136 counted down, +140 the countdown started, +144 its speed.
const FADE: u32 = 92;
const FINAL: u32 = 100;
const AWARDED: u32 = 104;
const DAMAGE: u32 = 108;
const BONUS: u32 = 112;
const START: u32 = 116;
const GOT_MONEY: u32 = 120;
const GOT_BONUS: u32 = 124;
const GOT_BILLED: u32 = 128;
const DRAW_MONEY_ON: u32 = 132;
const COUNTED_DOWN: u32 = 136;
const COUNTDOWN_NOW: u32 = 140;
const COUNT_SPEED: u32 = 144;

/// The tournament manager: +4 the tier, +8 the tournament, +16 the racers, +20 the money, +24
/// the definition (each tier's first tournament at +2 of its 12 bytes; tournaments from +36, 84
/// bytes each: +5 knock-out), +280 the competitors (16 bytes each: +0 their name's).
const TIER: u32 = 4;
const TOURNAMENT: u32 = 8;
const RACERS: u32 = 16;
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const COMPETITORS: u32 = 280;
/// The front end: the pink slips' tracks (`+0x23`), the one chosen (`+0x46`), how many
/// (`+0x47`), each track's winner (`+12`, `short`s: -1 none), the players' wins (`+0x48`); the
/// tier (`+0x122`).
const PINK_TRACKS: u32 = FRONT_END + 0x23;
const PINK_CHOSEN: u32 = FRONT_END + 0x46;
const PINK_COUNT: u32 = FRONT_END + 0x47;
const PINK_WINNERS: u32 = FRONT_END + 12;
const PINK_WINS: u32 = FRONT_END + 0x48;
const FE_TIER: u32 = FRONT_END + 0x122;
/// In the menus' block: the tournaments' iterators (a tier's, the others').
const TIER_ITERATOR: u32 = 1932;
const ITERATOR: u32 = 1704;
/// The vtables; "zTrnSt1", "zTrnStd", "%d %s"; the colours' table and the money's colour's
/// index; the permanent shapes' table now.
const STANDINGS_VT: u32 = 0x8001_19F0;
const STANDINGS_NAME: u32 = 0x8001_1934;
const STANDINGS3_NAME: u32 = 0x8001_193C;
const POINTS_FORMAT: u32 = 0x8001_1944;
const COLORS: u32 = 0x8005_1B50;
const MONEY_COLOR: u32 = 0x8005_1AE7;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;

/// `PSXDrawSquare`: the host's.
const DRAW_SQUARE: u32 = 0x8004_F12C;
/// FETOURN's `GetAwardInformation`, `CalcTrackFinishDamageBill`, `PlayerRanking`,
/// `TournPointTotal`, `tListIteratorTournament::TextValue`; FRONT's `Front_GetTrackRaced`,
/// `PlayerName`; a competitor's name (0x8004b428): still recompiled. `TextSys_WordX`,
/// `TextSys_WordY`.
const GET_AWARD: fn(&mut Cpu) = nfs4rc::f_80033a10;
const DAMAGE_BILL: fn(&mut Cpu) = nfs4rc::f_80032dbc;
const PLAYER_RANKING: fn(&mut Cpu) = nfs4rc::f_80033be4;
const POINT_TOTAL: fn(&mut Cpu) = nfs4rc::f_80033bc8;
const TOURNAMENT_TEXT: fn(&mut Cpu) = nfs4rc::f_80033f64;
const TRACK_RACED: fn(&mut Cpu) = nfs4rc::f_8002b9e0;
const PLAYER_NAME: fn(&mut Cpu) = nfs4rc::f_8002ba7c;
const COMPETITOR_NAME: fn(&mut Cpu) = nfs4rc::f_8004b428;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;

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

/// `DrawMoney` (SCREENPOST.CPP, 0x80039a80): `amount` formatted at x, y in `color`, and over it
/// the currency word of `kind` (3, 6, 9; others none) in `word_color`.
#[allow(clippy::too_many_arguments)]
pub fn draw_money(c: &mut Cpu, x: u32, y: u32, kind: u32, amount: u32, color: u32, word_color: u32) {
    abi::frame(c, 160, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = x;
        r.0[3] = y;
        r.0[2] = amount;
        let word = sp + 24;
        let money = sp + 80;
        // (s0 the money's text already, from the kind's branches' delay slots, but for 6)
        if kind != 6 {
            r.0[0] = money;
        }
        match kind {
            3 => {
                let w = r.call_leaving(c, TEXT_WORD, &[135], 0x8003_9ae4);
                r.call_leaving(c, SPRINTF, &[word, w, 0], 0x8003_9af4);
            }
            6 => {
                let w = r.call_leaving(c, TEXT_WORD, &[136], 0x8003_9b04);
                r.call_leaving(c, SPRINTF, &[word, w, 0, 0], 0x8003_9b18);
            }
            9 => {
                let w = r.call_leaving(c, TEXT_WORD, &[137], 0x8003_9b28);
                c.m.w32(sp + 16, 0);
                r.call_leaving(c, SPRINTF, &[word, w, 0, 0], 0x8003_9b40);
            }
            _ => {}
        }
        r.0[0] = money;
        port(c, &r, 0x8002_6d00, 0x8003_9b50, &[money, amount], fetools::abi_format_money);
        r.0[1] = sx(x);
        r.0[0] = sx(y);
        r.0[2] = 1;
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[money, sx(x), sx(y), color, 0, 1], 0x8003_9b80);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[word, sx(x), sx(y), word_color, 0, 1], 0x8003_9b9c);
    })
}

pub fn abi_draw_money(c: &mut Cpu) {
    let (color, word_color) = (abi::arg(c, 4), abi::arg(c, 5));
    draw_money(c, c.r[4], c.r[5], c.r[6], c.r[7], color, word_color);
}

/// `tScreenTournamentStandings::tScreenTournamentStandings` (SCREENPOST.CPP, 0x80039bb8): no
/// money shown.
pub fn abi_screen_tournament_standings(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8003_9bcc, &[this], fescreen::abi_screen);
        c.m.w32(this + 96, STANDINGS_VT);
        c.m.w32(this + DRAW_MONEY_ON, 0);
        c.r[2] = this;
    })
}

/// `tScreenTournamentStandings::Initialize` (SCREENPOST.CPP, 0x80039bf0): the screen's files;
/// the award; with the money shown, what was won, billed and given as a bonus (counted down 24
/// steps of the largest), the money before them; else the money before the tournament's.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_60cc, 0x8003_9c08, &[this], fescreen::abi_initialize);
        r.0[1] = TOURNAMENT_MANAGER;
        let award = sp + 16;
        r.call_leaving(c, GET_AWARD, &[TOURNAMENT_MANAGER, award], 0x8003_9c1c);
        let m = &mut c.m;
        m.w32(this + GOT_MONEY, 0);
        m.w32(this + GOT_BONUS, 0);
        m.w32(this + GOT_BILLED, 0);
        let before = m.r32(TOURNAMENT_MANAGER + MONEY).wrapping_sub(m.r32(award + 4));
        if m.r32(this + DRAW_MONEY_ON) != 0 {
            let won = m.r32(award);
            if won != 0 {
                m.w32(this + GOT_MONEY, 1);
            }
            m.w32(this + FINAL, before);
            if m.r32(award + 60) != 0 {
                let v = before.wrapping_sub(m.r32(award + 64));
                m.w32(this + FINAL, v);
            }
            m.w32(this + AWARDED, won);
            r.call_leaving(c, DAMAGE_BILL, &[TOURNAMENT_MANAGER, 0, this + DAMAGE, this + BONUS], 0x8003_9c90);
            let m = &mut c.m;
            if (m.r32(this + BONUS) as i32) > 0 {
                m.w32(this + GOT_BONUS, 1);
            }
            if (m.r32(this + DAMAGE) as i32) > 0 {
                m.w32(this + GOT_BILLED, 1);
            }
            m.w32(this + COUNTED_DOWN, 0);
            if m.r32(this + BONUS) == 0 && m.r32(this + DAMAGE) == 0 && m.r32(this + AWARDED) == 0 {
                m.w32(this + COUNTED_DOWN, 1);
            }
            m.w32(this + COUNTDOWN_NOW, 0);
            let largest = (m.r32(this + BONUS) as i32).max(m.r32(award) as i32).max(m.r32(this + DAMAGE) as i32);
            m.w32(this + COUNT_SPEED, (largest / 24) as u32);
        } else {
            m.w32(this + FINAL, before);
        }
        let ticks = c.m.r32(GAME_TICKS);
        c.m.w32(this + START, ticks);
    })
}

/// `tScreenTournamentStandings::Cleanup` (SCREENPOST.CPP, 0x80039d7c): no money shown; the
/// screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + DRAW_MONEY_ON, 0);
        port(c, &Saved::of(c), 0x8002_61c0, 0x8003_9d8c, &[this], fescreen::abi_cleanup);
    })
}

/// A standings screen's shapes: 41 of `name`, none swapped.
fn shape_info(c: &mut Cpu, name_text: u32) {
    let swap_name = abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(swap, 0);
    m.w32(swap_name, 0);
    m.w16(permanent, 41);
    m.w32(name, name_text);
}

/// `tScreenTournamentStandings::GetShapeInfo` (SCREENPOST.CPP, 0x80039d9c): "zTrnSt1".
pub fn abi_get_shape_info(c: &mut Cpu) {
    shape_info(c, STANDINGS_NAME);
}

/// `tScreenTournamentStandings3item::GetShapeInfo` (SCREENPOST.CPP, 0x80039dc0): "zTrnStd".
pub fn abi_get_shape_info3(c: &mut Cpu) {
    shape_info(c, STANDINGS3_NAME);
}

/// `tScreenTournamentStandings::ProcessInput` (SCREENPOST.CPP, 0x80039de4): accept before the
/// money is counted down starts the countdown instead.
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    let m = &mut c.m;
    if m.r32(key) == KEY_ACCEPT && m.r32(this + COUNTED_DOWN) == 0 {
        m.w32(key, KEY_NONE);
        m.w32(this + COUNTDOWN_NOW, 1);
    }
}

/// The money's mark sliding to and fro under the title (shape 39's width) and its two halves
/// (shape 40), as both standings draw it: the slide and the middle.
fn slide(m: &mut Mem) -> (i32, i32) {
    let shape = m.r32(PERMANENT_SHAPES) + 1248;
    let w = lh(m, shape + 16);
    assert!(w != 0, "a mark 0 wide (the original's division `break`s)");
    let mut slide = (m.r32(GAME_TICKS) as i32).wrapping_rem(w);
    if w / 2 < slide {
        slide = w - slide;
    }
    (slide, ((w >> 1) - 2).wrapping_sub(lh(m, shape + 20)))
}

/// `tScreenTournamentStandings::DrawBackground` (SCREENPOST.CPP, 0x80039e14): each racer's
/// place, name (the player's lit) and points (or, knocking out, survived or out); the track and
/// the tournament (boxed, a mark sliding under them); a line; with the money shown (after 1000
/// ticks or once asked), what was won, billed and the bonus counted down; the final money; the
/// picture.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        r.0[7] = this;
        r.0[4] = 0;
        let m = &mut c.m;
        let fade = lh(m, this + FADE) as u32;
        r.0[8] = fade;
        m.w32(sp + 188, fade);
        let definition = m.r32(TOURNAMENT_MANAGER + DEFINITION);
        let tier = m.r32(TOURNAMENT_MANAGER + TIER);
        let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
        let t = definition.wrapping_add(first.wrapping_add(m.r32(TOURNAMENT_MANAGER + TOURNAMENT)).wrapping_mul(84)) + 36;
        r.0[6] = 767;
        m.w32(sp + 192, t);
        let knockout = m.r8(t + 5) != 0;
        let n = sx((m.r16(TOURNAMENT_MANAGER + RACERS) as u32).wrapping_add(knockout as u32));
        m.w32(sp + 196, n);
        m.w32(sp + 200, n.wrapping_sub(1));
        // the racers
        let ftf = |c: &mut Cpu, r: &Saved, args: [u32; 7], ra: u32| r.call_leaving(c, fetextrender::full_text_fade_entry, &args, ra);
        loop {
            let k = r.0[4];
            if (k as i32) >= c.m.r32(sp + 196) as i32 {
                break;
            }
            let place = sx(r.call_leaving(c, PLAYER_RANKING, &[TOURNAMENT_MANAGER, sx(k + 1)], 0x8003_9eec));
            r.0[3] = place;
            r.0[5] = if place == 0 { 2 } else { 1 };
            r.0[2] = if place == 0 { 2 } else { 0 };
            let w = r.call_leaving(c, TEXT_WORD, &[k + 600], 0x8003_9f14);
            r.0[1] = w;
            let x = sx(r.call_leaving(c, WORD_X, &[760], 0x8003_9f20));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[6]], 0x8003_9f30);
            ftf(c, &r, [r.0[8], w, x, sx(y), 11, r.0[2], 0], 0x8003_9f58);
            let name = if place == 0 {
                r.call_leaving(c, PLAYER_NAME, &[0], 0x8003_9f68)
            } else {
                let id = c.m.r32(TOURNAMENT_MANAGER.wrapping_add(place << 4) + COMPETITORS);
                r.call_leaving(c, COMPETITOR_NAME, &[id], 0x8003_9f88)
            };
            r.0[1] = name;
            let x = sx(r.call_leaving(c, WORD_X, &[761], 0x8003_9f94));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[6]], 0x8003_9fa4);
            ftf(c, &r, [r.0[8], name, x, sx(y), 11, r.0[5], 0], 0x8003_9fcc);
            let text = sp + 32;
            c.m.w16(sp + 184, place as u16);
            let t = c.m.r32(sp + 192);
            if c.m.r8(t + 5) != 0 {
                let id = if k == c.m.r32(sp + 200) { 797 } else { 796 };
                let w = r.call_leaving(c, TEXT_WORD, &[id], 0x8003_a000);
                r.call_leaving(c, SPRINTF, &[text, w], 0x8003_a00c);
            } else {
                let points = sx(r.call_leaving(c, POINT_TOTAL, &[TOURNAMENT_MANAGER, sp + 184], 0x8003_a020));
                r.0[0] = points;
                let w = r.call_leaving(c, TEXT_WORD, &[798], 0x8003_a030);
                r.call_leaving(c, SPRINTF, &[text, POINTS_FORMAT, points, w], 0x8003_a048);
            }
            let x = sx(r.call_leaving(c, WORD_X, &[764], 0x8003_a050));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[6]], 0x8003_a060);
            ftf(c, &r, [r.0[8], text, x, sx(y), 11, r.0[5], 1], 0x8003_a08c);
            r.0[6] = r.0[6].wrapping_add(1);
            r.0[4] = k + 1;
        }
        // the track and the tournament
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8003_a0a0);
        port(c, &r, 0x8002_1c04, 0x8003_a0b8, &[TRACK_MANAGER, sx(track), sp + 112], fetracks::abi_get_track);
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8003_a0c0);
        let w = r.call_leaving(c, TEXT_WORD, &[sx(track).wrapping_add(214)], 0x8003_a0d0);
        r.0[1] = w;
        let x = sx(r.call_leaving(c, WORD_X, &[759], 0x8003_a0dc));
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[766], 0x8003_a0ec);
        ftf(c, &r, [r.0[8], w, x, sx(y), 11, 2, 2], 0x8003_a118);
        let it = {
            let m = &mut c.m;
            m.r32(SONGS_SCREEN) + if m.r8(FE_TIER) != 0 { TIER_ITERATOR } else { ITERATOR }
        };
        let name = sx(r.call_leaving(c, TOURNAMENT_TEXT, &[it, u32::MAX], 0x8003_a150)).wrapping_add(19);
        r.0[4] = name;
        let x = sx(r.call_leaving(c, WORD_X, &[759], 0x8003_a164));
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_a174);
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[r.0[8], sx(name), x, sx(y), 2, 2, 11], 0x8003_a1a4);
        boxed_title(c, &mut r, sp, name, [0x8003_a1ac, 0x8003_a1b4, 0x8003_a1c0, 0x8003_a1d0, 0x8003_a1ec]);
        // the mark
        let (s, mid) = slide(&mut c.m);
        r.0[4] = 0x8014_0000;
        r.0[0] = s as u32;
        r.0[2] = mid as u32;
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_a268);
        r.0[1] = 1;
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[40, 0, mid.wrapping_add(s) as u32, y.wrapping_add(1), fade, 1, 0], 0x8003_a290);
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_a298);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[40, 0, mid.wrapping_sub(s) as u32, y.wrapping_add(1), fade, 1, 0], 0x8003_a2bc);
        c.m.w32(sp + 164, 0x0028_2828);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[39, 1024, 0, u32::MAX, fade, 0, sp + 160], 0x8003_a2f0);
        // a line
        let x = r.call_leaving(c, WORD_X, &[759], 0x8003_a2f8);
        r.0[0] = x.wrapping_sub(150);
        let y = r.call_leaving(c, WORD_Y, &[766], 0x8003_a304);
        let fade = c.m.r32(sp + 188);
        for (k, v) in [1, 3, fade, 30].into_iter().enumerate() {
            c.m.w32(sp + 16 + 4 * k as u32, v);
        }
        port(c, &r, 0x8004_c018, 0x8003_a338, &[0x0023_2323, r.0[0], y.wrapping_add(10), 300], felines::abi_draw_bright_end_line);
        // the money
        let base = {
            let m = &mut c.m;
            let k = m.r8(MONEY_COLOR) as u32;
            m.r32(COLORS + (k << 2))
        };
        let gold = port(c, &r, 0x8004_bd68, 0x8003_a35c, &[base, r.0[8]], fefades::abi_fade_out);
        r.0[3] = gold;
        let shade = port(c, &r, 0x8004_bd68, 0x8003_a370, &[0x0023_2323, r.0[8]], fefades::abi_fade_out);
        r.0[2] = shade;
        let m = &mut c.m;
        let late = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32 >= 1001;
        if late || m.r32(this + COUNTDOWN_NOW) != 0 {
            if (m.r32(this + AWARDED) as i32) > 0 || (m.r32(this + DAMAGE) as i32) > 0 || (m.r32(this + BONUS) as i32) > 0 {
                r.call_leaving(c, PLAY_FE_SFX, &[21], 0x8003_a3d4);
            }
            count_down(&mut c.m, this);
        }
        if c.m.r32(this + DRAW_MONEY_ON) != 0 {
            let lines = [
                (787, GOT_MONEY, AWARDED, [0x8003_a438, 0x8003_a444, 0x8003_a454, 0x8003_a498, 0x8003_a4a0, 0x8003_a4ac, 0x8003_a4c8]),
                (788, GOT_BILLED, DAMAGE, [0x8003_a4d0, 0x8003_a4dc, 0x8003_a4ec, 0x8003_a52c, 0x8003_a534, 0x8003_a540, 0x8003_a55c]),
                (789, GOT_BONUS, BONUS, [0x8003_a564, 0x8003_a570, 0x8003_a580, 0x8003_a5c0, 0x8003_a5c8, 0x8003_a5d4, 0x8003_a5f0]),
            ];
            for (id, flag, amount, ras) in lines {
                let w = r.call_leaving(c, TEXT_WORD, &[id], ras[0]);
                r.0[1] = w;
                let x = sx(r.call_leaving(c, WORD_X, &[763], ras[1]));
                r.0[0] = x;
                let y = r.call_leaving(c, WORD_Y, &[id], ras[2]);
                let state = (c.m.r32(this + flag) != 0) as u32;
                if id != 789 {
                    r.0[4] = 1;
                }
                ftf(c, &r, [r.0[8], w, x, sx(y), 11, state, 1], ras[3]);
                let x = r.call_leaving(c, WORD_X, &[764], ras[4]);
                r.0[0] = x;
                let y = r.call_leaving(c, WORD_Y, &[id], ras[5]);
                let value = c.m.r32(this + amount);
                c.m.w32(sp + 16, gold);
                c.m.w32(sp + 20, shade);
                port(c, &r, 0x8003_9a80, ras[6], &[x, y, 6, value], abi_draw_money);
            }
        }
        // the final money
        let w = r.call_leaving(c, TEXT_WORD, &[790], 0x8003_a5f8);
        r.0[1] = w;
        let x = sx(r.call_leaving(c, WORD_X, &[763], 0x8003_a604));
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[790], 0x8003_a614);
        ftf(c, &r, [r.0[8], w, x, sx(y), 11, 2, 1], 0x8003_a644);
        let x = r.call_leaving(c, WORD_X, &[764], 0x8003_a64c);
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[790], 0x8003_a658);
        let m = &mut c.m;
        let total = m.r32(this + FINAL).wrapping_sub(m.r32(this + AWARDED)).wrapping_add(m.r32(this + DAMAGE)).wrapping_sub(m.r32(this + BONUS));
        m.w32(sp + 16, gold);
        m.w32(sp + 20, shade);
        port(c, &r, 0x8003_9a80, 0x8003_a68c, &[x, y, 9, total], abi_draw_money);
        let table = c.m.r32(PERMANENT_SHAPES);
        c.m.w32(sp + 16, 0);
        port(c, &r, 0x8002_5ce8, 0x8003_a6a8, &[this, 10, 29, table], fescreen::abi_draw_background_image);
    })
}

/// The money counted down a step: what was won first, then the damage billed, then the bonus
/// (counted down once the bill is).
fn count_down(m: &mut Mem, this: u32) {
    let step = m.r32(this + COUNT_SPEED);
    let v = m.r32(this + AWARDED).wrapping_sub(step);
    m.w32(this + AWARDED, v);
    if v as i32 > 0 {
        return;
    }
    let v = m.r32(this + DAMAGE);
    m.w32(this + AWARDED, 0);
    let v = v.wrapping_sub(step);
    m.w32(this + DAMAGE, v);
    if v as i32 > 0 {
        return;
    }
    let bonus = m.r32(this + BONUS);
    m.w32(this + COUNTED_DOWN, 1);
    let bonus = bonus.wrapping_sub(step);
    m.w32(this + DAMAGE, 0);
    m.w32(this + BONUS, (bonus as i32).max(0) as u32);
}

/// A title's box: the text `id`'s width, a black box under it centred on the title's place.
fn boxed_title(c: &mut Cpu, r: &mut Saved, _sp: u32, id: u32, ras: [u32; 5]) {
    let w = r.call_leaving(c, TEXT_WORD, &[id], ras[0]);
    let px = r.call_leaving(c, TEXT_PIXELS, &[w], ras[1]);
    r.0[1] = px;
    let x = r.call_leaving(c, WORD_X, &[759], ras[2]);
    r.0[0] = x.wrapping_sub(((px as i32) >> 1) as u32);
    let y = r.call_leaving(c, WORD_Y, &[765], ras[3]);
    r.call_ptr_leaving(c, DRAW_SQUARE, &[0, r.0[0], y.wrapping_sub(1), px, 9], ras[4]);
}

/// `tScreenPinkSlipStandings::DrawBackground` (SCREENPOST.CPP, 0x8003a6d8): the tracks raced
/// (the one chosen lit, those before it half) and their winners (or none); the two players'
/// wins; the title boxed, a mark sliding under it; the picture.
pub fn abi_pink_slip_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 208, |c, sp| {
        let mut r = Saved::of(c);
        r.0[8] = this;
        r.0[6] = 11;
        r.0[2] = 0;
        r.0[3] = FRONT_END;
        r.0[1] = 767;
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.0[5] = fade;
        let ftf = |c: &mut Cpu, r: &Saved, args: [u32; 7], ra: u32| r.call_leaving(c, fetextrender::full_text_fade_entry, &args, ra);
        let info = sp + 88;
        loop {
            let k = r.0[2];
            r.0[4] = 2;
            let m = &mut c.m;
            if (k as i32) >= m.r8(PINK_COUNT) as i32 {
                break;
            }
            let chosen = m.r8(PINK_CHOSEN) as u32;
            if k != chosen {
                r.0[4] = ((k as i32) < chosen as i32) as u32;
            }
            let id = m.r8(PINK_TRACKS.wrapping_add(k)) as u32;
            port(c, &r, 0x8002_1c04, 0x8003_a760, &[TRACK_MANAGER, id, info], fetracks::abi_get_track);
            let x = sx(r.call_leaving(c, WORD_X, &[760], 0x8003_a768));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[1]], 0x8003_a778);
            let m = &mut c.m;
            let fade = lh(m, this + FADE) as u32;
            let name = sx((m.r8(info) as i8 as i32 as u32).wrapping_add(214));
            r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[fade, name, x, sx(y), 0, r.0[4], 11], 0x8003_a7b0);
            let winner = lh(&mut c.m, PINK_WINNERS.wrapping_add(k << 1));
            let text = sp + 136;
            let w = if winner == -1 {
                r.call_leaving(c, TEXT_WORD, &[782], 0x8003_a7d0)
            } else {
                r.call_leaving(c, PLAYER_NAME, &[winner as u32], 0x8003_a7e0)
            };
            r.call_leaving(c, SPRINTF, &[text, w], 0x8003_a7ec);
            let x = sx(r.call_leaving(c, WORD_X, &[764], 0x8003_a7f4));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[1]], 0x8003_a804);
            ftf(c, &r, [r.0[5], text, x, sx(y), 11, r.0[4], 1], 0x8003_a82c);
            r.0[1] = r.0[1].wrapping_add(1);
            r.0[2] = k + 1;
        }
        // the players' wins
        r.0[2] = 0;
        r.0[7] = 1;
        r.0[3] = 788;
        let ras = [[0x8003_a84c, 0x8003_a858, 0x8003_a868, 0x8003_a88c], [0x8003_a84c, 0x8003_a858, 0x8003_a868, 0x8003_a88c]];
        for (k, ras) in ras.into_iter().enumerate() {
            let k = k as u32;
            let name = r.call_leaving(c, PLAYER_NAME, &[k], ras[0]);
            r.0[1] = name;
            let x = sx(r.call_leaving(c, WORD_X, &[761], ras[1]));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[3]], ras[2]);
            ftf(c, &r, [r.0[5], name, x, sx(y), 11, r.0[4], 0], ras[3]);
            let text = sp + 32;
            r.0[0] = FRONT_END + k;
            let wins = c.m.r8(PINK_WINS + k) as u32;
            if wins == 1 {
                let w = r.call_leaving(c, TEXT_WORD, &[800], 0x8003_a8b0);
                r.call_leaving(c, SPRINTF, &[text, w], 0x8003_a8bc);
            } else {
                let w = r.call_leaving(c, TEXT_WORD, &[799], 0x8003_a8cc);
                r.call_leaving(c, SPRINTF, &[text, w, wins], 0x8003_a8dc);
            }
            let x = sx(r.call_leaving(c, WORD_X, &[764], 0x8003_a8e4));
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[r.0[3]], 0x8003_a8f4);
            ftf(c, &r, [r.0[5], text, x, sx(y), 11, r.0[4], 1], 0x8003_a918);
            r.0[2] = k + 1;
            r.0[3] = r.0[3].wrapping_add(1);
        }
        // the title, the mark, the picture
        let x = sx(r.call_leaving(c, WORD_X, &[759], 0x8003_a930));
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_a940);
        r.call_leaving(c, fetextrender::menu_text_positioned_justify_fade_entry, &[r.0[5], 706, x, sx(y), 2, 2, 11], 0x8003_a968);
        boxed_title(c, &mut r, sp, 706, [0x8003_a970, 0x8003_a978, 0x8003_a984, 0x8003_a994, 0x8003_a9b0]);
        r.0[3] = 0x8005_0000;
        let (s, mid) = slide(&mut c.m);
        r.0[1] = s as u32;
        r.0[2] = mid as u32;
        c.m.w32(sp + 92, 0x0028_2828);
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_aa38);
        r.0[0] = 1;
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[40, 0, mid.wrapping_add(s) as u32, y.wrapping_add(1), fade, 1, 0], 0x8003_aa60);
        let y = r.call_leaving(c, WORD_Y, &[765], 0x8003_aa68);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[40, 0, mid.wrapping_sub(s) as u32, y.wrapping_add(1), fade, 1, 0], 0x8003_aa8c);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[39, 1024, 0, u32::MAX, fade, 0, sp + 88], 0x8003_aab4);
        let table = c.m.r32(PERMANENT_SHAPES);
        c.m.w32(sp + 16, 0);
        port(c, &r, 0x8002_5ce8, 0x8003_aacc, &[this, 10, 29, table], fescreen::abi_draw_background_image);
    })
}

/// `tScreenPinkSlipStandings::ProcessInput` (SCREENPOST.CPP, 0x8003aafc): the screen's.
pub fn abi_pink_slip_process_input(c: &mut Cpu) {
    let args = [c.r[4], c.r[5], c.r[6], c.r[7]];
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_68f8, 0x8003_ab0c, &args, fescreen::abi_process_input);
    })
}

/// The standings' destructors (SCREENPOST.H): the screen's.
fn destructor(c: &mut Cpu, ra: u32) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, ra, &[this, flags], fescreen::abi_screen_destructor);
    })
}

/// `tScreenPinkSlipStandings::~tScreenPinkSlipStandings` (0x8003ab1c).
pub fn abi_pink_slip_destructor(c: &mut Cpu) {
    destructor(c, 0x8003_ab2c);
}

/// `tScreenTournamentStandings3item::~tScreenTournamentStandings3item` (0x8003ab3c).
pub fn abi_destructor3(c: &mut Cpu) {
    destructor(c, 0x8003_ab4c);
}

/// `tScreenTournamentStandings::~tScreenTournamentStandings` (0x8003ab5c).
pub fn abi_destructor(c: &mut Cpu) {
    destructor(c, 0x8003_ab6c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, TOP, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The game's standings screens (the tournament's, the three-item one, the pink slips');
    /// the outputs of `GetShapeInfo`.
    const STANDINGS: u32 = 0x8018_0138;
    const STANDINGS3: u32 = 0x8018_0FFC;
    const PINK: u32 = 0x8018_0F68;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the standings: their fade; the money and its countdown about
    /// each step (shown or not, the 1000 ticks about their end, asked for or not, counted down
    /// or not); the tier, the tournament (knocking out or not), 0 to 6 racers, their names and
    /// places, the award;
    /// the pink slips' tracks and chosen one, their winners (none, either player), the players'
    /// wins; the tier; the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let ticks = 4000 + pick(rng, 100_000);
        m.w32(GAME_TICKS, ticks);
        for s in [STANDINGS, STANDINGS3, PINK] {
            m.w16(s + FADE, [0, 64, 128, pick(rng, 129)][pick(rng, 4) as usize] as u16);
            m.w32(s + FINAL, rng.next() as u32 & 0xF_FFFF);
            for f in [AWARDED, DAMAGE, BONUS] {
                m.w32(s + f, [0, 1, 50, 5000, (-3i32) as u32, pick(rng, 20000)][pick(rng, 6) as usize]);
            }
            m.w32(s + START, ticks.wrapping_sub([0, 1000, 1001, 2000, pick(rng, 3000)][pick(rng, 5) as usize]));
            for f in [GOT_MONEY, GOT_BONUS, GOT_BILLED, DRAW_MONEY_ON, COUNTED_DOWN, COUNTDOWN_NOW] {
                m.w32(s + f, pick(rng, 2));
            }
            m.w32(s + COUNT_SPEED, [0, 1, 10, 100, pick(rng, 1000)][pick(rng, 5) as usize]);
        }
        let t = TOURNAMENT_MANAGER;
        m.w32(t + TIER, pick(rng, 3));
        m.w32(t + TOURNAMENT, pick(rng, 3));
        m.w16(t + RACERS, pick(rng, 7) as u16);
        m.w32(t + MONEY, rng.next() as u32 & 0xF_FFFF);
        for k in 0..6 {
            m.w32(t + COMPETITORS + 16 * k, pick(rng, 20));
        }
        // (the racers' places: a shuffled order, so the player isn't always first)
        let mut places: Vec<u32> = (1..7).collect();
        for k in (1..places.len()).rev() {
            places.swap(k, pick(rng, k as u32 + 1) as usize);
        }
        for (k, &p) in places.iter().enumerate() {
            m.w8(t + 294 + 16 * k as u32, p as u8);
        }
        // (the award (`GetAwardInformation` copies it): money won, the tournament's, a car
        // that didn't fit the garage and its bonus)
        let award = t + 576;
        m.w32(award, [0, 100, 5000, pick(rng, 50_000)][pick(rng, 4) as usize]);
        m.w32(award + 4, pick(rng, 50_000));
        m.w32(award + 60, pick(rng, 2));
        m.w32(award + 64, pick(rng, 20_000));
        let definition = m.r32(t + DEFINITION);
        let tier = m.r32(t + TIER);
        let first = m.r8(definition.wrapping_add(tier * 12) + 2) as u32;
        let tour = definition.wrapping_add(first.wrapping_add(m.r32(t + TOURNAMENT)) * 84) + 36;
        m.w8(tour + 5, pick(rng, 2) as u8);
        let n = pick(rng, 7);
        m.w8(PINK_COUNT, n as u8);
        m.w8(PINK_CHOSEN, pick(rng, n.max(1)) as u8);
        for k in 0..8 {
            m.w8(PINK_TRACKS + k, pick(rng, 10) as u8);
            m.w16(PINK_WINNERS + 2 * k, [0xFFFF, 0, 1][pick(rng, 3) as usize]);
        }
        for k in 0..2 {
            m.w8(PINK_WINS + k, pick(rng, 4) as u8);
        }
        m.w8(FE_TIER, pick(rng, 2) as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("DrawMoney", 0x8003_9a80, nfs4rc::f_80039a80, |_, r| vec![pick(r, 400), pick(r, 240), [3, 6, 9, 0, 5][pick(r, 5) as usize], [0, 1, 999, 1_000_000, (-5000i32) as u32, r.next() as u32][pick(r, 6) as usize], 0xBEBE, 0x0023_2323]),
            ("tScreenTournamentStandings", 0x8003_9bb8, nfs4rc::f_80039bb8, |_, _| vec![OUT + 0x100]),
            ("tScreenTournamentStandings::Initialize", 0x8003_9bf0, nfs4rc::f_80039bf0, |_, r| vec![[STANDINGS, STANDINGS3][pick(r, 2) as usize]]),
            ("tScreenTournamentStandings::Cleanup", 0x8003_9d7c, nfs4rc::f_80039d7c, |_, _| vec![STANDINGS]),
            ("tScreenTournamentStandings::GetShapeInfo", 0x8003_9d9c, nfs4rc::f_80039d9c, |_, _| vec![STANDINGS, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTournamentStandings3item::GetShapeInfo", 0x8003_9dc0, nfs4rc::f_80039dc0, |_, _| vec![STANDINGS3, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTournamentStandings::ProcessInput", 0x8003_9de4, nfs4rc::f_80039de4, |c, r| {
                c.m.w32(KEY, [KEY_ACCEPT, KEY_NONE, 16][pick(r, 3) as usize]);
                vec![STANDINGS, player(r), KEY, COMMAND]
            }),
            ("tScreenTournamentStandings::DrawBackground", 0x8003_9e14, nfs4rc::f_80039e14, |_, r| vec![[STANDINGS, STANDINGS3][pick(r, 2) as usize]]),
            ("tScreenPinkSlipStandings::DrawBackground", 0x8003_a6d8, nfs4rc::f_8003a6d8, |_, _| vec![PINK]),
            ("tScreenPinkSlipStandings::ProcessInput", 0x8003_aafc, nfs4rc::f_8003aafc, |c, r| {
                c.m.w32(KEY, [KEY_ACCEPT, KEY_NONE, 16][pick(r, 3) as usize]);
                vec![PINK, player(r), KEY, COMMAND]
            }),
            ("~tScreenPinkSlipStandings", 0x8003_ab1c, nfs4rc::f_8003ab1c, |_, r| vec![PINK, 2 * pick(r, 2)]),
            ("~tScreenTournamentStandings3item", 0x8003_ab3c, nfs4rc::f_8003ab3c, |_, r| vec![STANDINGS3, 2 * pick(r, 2)]),
            ("~tScreenTournamentStandings", 0x8003_ab5c, nfs4rc::f_8003ab5c, |_, r| vec![STANDINGS, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screenpost_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x9057 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x9058 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screenpost_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x9068 + k as u64, shuffle, &args);
        }
    }
}
