//! SCREENPOST.CPP: the screens after a race.
//! - `DrawMoney`: an amount and its currency word.
//! - The tournament's standings (`tScreenTournamentStandings`, and the three-item one): each
//!   racer's place, name and points (or knock-out mark), the track and the tournament, the
//!   money won, the damage billed and the bonus counted down into the total.
//! - The pink slips' standings (`tScreenPinkSlipStandings`): the tracks raced (the one chosen
//!   lit) with their winners, and the two players' wins.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.
//! `PSXDrawSquare` is the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenu::{KEY_ACCEPT, KEY_NONE, PLAY_FE_SFX};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::{FRONT_END, GAME_TICKS, SONGS_SCREEN, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::types::{AwardInformation, DrawShapeExtended, Screen, TextureShapeInfo, TrackInformation};
use crate::{fefades, fescreen, fetextrender, fetourn};
use ps1rt::{Cpu, Mem};

/// The standings: a `tScreen`, then +100 the final money, +104 the money won, +108 the damage
/// billed, +112 the bonus, +116 the tick it came up, +120 money won, +124 a bonus, +128 billed,
/// +132 the money shown, +136 counted down, +140 the countdown started, +144 its speed.
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
/// The countdown's steps; the money shown on its own after this long (ticks); the tick sound.
const COUNT_STEPS: i32 = 24;
const MONEY_AFTER: i32 = 1001;
const SOUND_TICK: u32 = 21;

/// The tournament manager: +4 the tier, +8 the tournament, +16 the racers, +20 the money, +24
/// the definition (each tier's first tournament at +2 of its 12 bytes; tournaments from +36, 84
/// bytes each: +5 knock-out), +280 the competitors (16 bytes each: +0 their name's).
const TIER: u32 = 4;
const TOURNAMENT: u32 = 8;
const RACERS: u32 = 16;
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const KNOCKOUT: u32 = 5;
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
/// A track's description (`tTrackInformation`, 48 bytes).
const TRACK_SIZE: u32 = 48;
/// In the menus' block: the tournaments' iterators (a tier's, the others').
const TIER_ITERATOR: u32 = 1932;
const ITERATOR: u32 = 1704;
/// The vtables; "zTrnSt1", "zTrnStd" (41 shapes each), "%d %s"; the colours' table and the
/// money's colour's index; the permanent shapes' table now; the title's mark (shape 39: its
/// width at +16, its centre at +20) and its halves (40).
const STANDINGS_VT: u32 = 0x8001_19F0;
const STANDINGS_NAME: u32 = 0x8001_1934;
const STANDINGS3_NAME: u32 = 0x8001_193C;
const SHAPES: u16 = 41;
const POINTS_FORMAT: u32 = 0x8001_1944;
const COLORS: u32 = 0x8005_1B50;
const MONEY_COLOR: u32 = 0x8005_1AE7;
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
const MARK: u32 = 39;
const MARK_HALF: u32 = 40;
/// The texts' places (`TextSys_WordX`/`Y`'s ids): the title's x (759), the rows' place, name and
/// value columns (760, 761, 764), the money's labels' column (763); the rows (767 on), the
/// title's y (765), the track's (766); the players' wins' rows (788 on).
const TITLE_X: u32 = 759;
const PLACE_X: u32 = 760;
const NAME_X: u32 = 761;
const LABEL_X: u32 = 763;
const VALUE_X: u32 = 764;
const TITLE_Y: u32 = 765;
const TRACK_Y: u32 = 766;
const ROWS: u32 = 767;
const WINS_ROWS: u32 = 788;
/// The texts: the places (600 on), the tracks' names (214 on), the tournaments' (their text
/// value + 19); survived, out (796, 797), "points" (798); the money's labels (won 787, billed
/// 788, bonus 789, total 790); no winner (782); the wins (799 "%d wins", 800 one); the pink
/// slips' title (706); the currency words (135, 136, 137, by money kind 3, 6, 9).
const PLACES: u32 = 600;
const TRACK_NAMES: u32 = 214;
const TOURNAMENT_NAMES: u32 = 19;
const SURVIVED: u32 = 796;
const KNOCKED_OUT: u32 = 797;
const POINTS: u32 = 798;
const WON: u32 = 787;
const BILLED: u32 = 788;
const BONUS_TEXT: u32 = 789;
const TOTAL: u32 = 790;
const NO_WINNER: u32 = 782;
const WINS: u32 = 799;
const ONE_WIN: u32 = 800;
const PINK_TITLE: u32 = 706;
/// The money's kinds: an amount, a total.
const AMOUNT: u32 = 6;
const TOTAL_KIND: u32 = 9;
/// The colours: the lines' shade, the mark's band's grey.
const SHADE: u32 = 0x0023_2323;
const BAND: u32 = 0x0028_2828;

/// `PSXDrawSquare`: the host's.
const DRAW_SQUARE: u32 = 0x8004_F12C;
/// FETOURN's `GetAwardInformation`, `CalcTrackFinishDamageBill`, `PlayerRanking`,
/// `TournPointTotal`, `tListIteratorTournament::TextValue` (ported); FRONT's
/// `Front_GetTrackRaced`, `PlayerName`; a competitor's name (0x8004b428): still recompiled.
/// `TextSys_WordX`, `TextSys_WordY`.
const GET_AWARD: fn(&mut Cpu) = fetourn::get_award_information_entry;
const DAMAGE_BILL: fn(&mut Cpu) = fetourn::calc_track_finish_damage_bill_entry;
const PLAYER_RANKING: fn(&mut Cpu) = fetourn::player_ranking_entry;
const POINT_TOTAL: fn(&mut Cpu) = fetourn::tourn_point_total_entry;
const TOURNAMENT_TEXT: fn(&mut Cpu) = fetourn::text_value_entry;
const TRACK_RACED: fn(&mut Cpu) = crate::front::get_track_raced_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const COMPETITOR_NAME: fn(&mut Cpu) = nfs4rc::f_8004b428;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// Ports called by address: FETOOLS' `FormatMoney`, FETRACKS' `GetTrack`, FELINES'
/// `DrawBrightEndLine`.
const FORMAT_MONEY: u32 = 0x8002_6D00;
const GET_TRACK: u32 = 0x8002_1C04;
const DRAW_BRIGHT_END_LINE: u32 = 0x8004_C018;

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn word_x(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_X, &[k])
}

fn word_y(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_Y, &[k])
}

/// The screen's fade.
fn fade(m: &mut Mem, this: u32) -> u32 {
    Screen(this).screen_fade_val(m) as i32 as u32
}

/// `DrawMoney` (SCREENPOST.CPP, 0x80039a80): `amount` formatted at x, y in `color`, and over it
/// the currency word of `kind` (3, 6, 9) in `word_color`. (For other kinds the original draws
/// its stack's leftover as the word, which the game never asks for; none here.)
#[allow(clippy::too_many_arguments)]
pub fn draw_money(c: &mut Cpu, x: u32, y: u32, kind: u32, amount: u32, color: u32, word_color: u32) {
    with_buffer(c, 128, |c, at| {
        let (word, money) = (at, at + 64);
        // (each kind's format given as many zeros as the original passes)
        match kind {
            3 => {
                let w = call(c, TEXT_WORD, &[135]);
                call(c, SPRINTF, &[word, w, 0]);
            }
            6 => {
                let w = call(c, TEXT_WORD, &[136]);
                call(c, SPRINTF, &[word, w, 0, 0]);
            }
            9 => {
                let w = call(c, TEXT_WORD, &[137]);
                call(c, SPRINTF, &[word, w, 0, 0, 0]);
            }
            _ => c.m.w8(word, 0),
        }
        call_addr(c, FORMAT_MONEY, &[money, amount]);
        fetextrender::full_text_rgb(c, money, sx(x), sx(y), color, 0, 1);
        fetextrender::full_text_rgb(c, word, sx(x), sx(y), word_color, 0, 1);
    })
}

pub fn abi_draw_money(c: &mut Cpu) {
    let (color, word_color) = (crate::abi::arg(c, 4), crate::abi::arg(c, 5));
    draw_money(c, c.r[4], c.r[5], c.r[6], c.r[7], color, word_color);
}

/// `tScreenTournamentStandings::tScreenTournamentStandings` (SCREENPOST.CPP, 0x80039bb8): no
/// money shown.
pub fn screen_tournament_standings(c: &mut Cpu, this: Screen) -> Screen {
    fescreen::screen(c, this);
    this.set_vtable(&mut c.m, STANDINGS_VT);
    c.m.w32(this.0 + DRAW_MONEY_ON, 0);
    this
}

pub fn abi_screen_tournament_standings(c: &mut Cpu) {
    c.r[2] = screen_tournament_standings(c, Screen(c.r[4])).0;
}

/// `tScreenTournamentStandings::Initialize` (SCREENPOST.CPP, 0x80039bf0). The screen's files
/// are read and the award asked for. With the money shown: what was won, billed and given as a
/// bonus (counted down in 24 steps of the largest), and the money before them (and before a
/// full garage's bonus); else the money before the tournament's.
pub fn initialize(c: &mut Cpu, this: Screen) {
    fescreen::initialize(c, this);
    let this = this.0;
    with_buffer(c, AwardInformation::SIZE, |c, at| {
        let award = AwardInformation(at);
        call(c, GET_AWARD, &[TOURNAMENT_MANAGER, at]);
        let m = &mut c.m;
        m.w32(this + GOT_MONEY, 0);
        m.w32(this + GOT_BONUS, 0);
        m.w32(this + GOT_BILLED, 0);
        let before = m.r32(TOURNAMENT_MANAGER + MONEY).wrapping_sub(award.tourn_money(m) as u32);
        if m.r32(this + DRAW_MONEY_ON) == 0 {
            m.w32(this + FINAL, before);
            return;
        }
        let won = award.money(m) as u32;
        if won != 0 {
            m.w32(this + GOT_MONEY, 1);
        }
        m.w32(this + FINAL, before);
        if award.completed_garage_full(m) != 0 {
            let v = before.wrapping_sub(award.completed_bonus_money(m) as u32);
            m.w32(this + FINAL, v);
        }
        m.w32(this + AWARDED, won);
        call(c, DAMAGE_BILL, &[TOURNAMENT_MANAGER, 0, this + DAMAGE, this + BONUS]);
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
        let largest = (m.r32(this + BONUS) as i32).max(award.money(m)).max(m.r32(this + DAMAGE) as i32);
        m.w32(this + COUNT_SPEED, (largest / COUNT_STEPS) as u32);
    });
    let ticks = c.m.r32(GAME_TICKS);
    c.m.w32(this + START, ticks);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTournamentStandings::Cleanup` (SCREENPOST.CPP, 0x80039d7c): no money shown; the
/// screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    c.m.w32(this.0 + DRAW_MONEY_ON, 0);
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// A standings screen's shapes: 41 of `file`, none swapped.
fn shape_info(m: &mut Mem, file: u32, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w16(swap, 0);
    m.w32(swap_name, 0);
    m.w16(permanent, SHAPES);
    m.w32(name, file);
}

/// `tScreenTournamentStandings::GetShapeInfo` (SCREENPOST.CPP, 0x80039d9c): "zTrnSt1".
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    shape_info(&mut c.m, STANDINGS_NAME, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTournamentStandings3item::GetShapeInfo` (SCREENPOST.CPP, 0x80039dc0): "zTrnStd".
pub fn abi_get_shape_info3(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    shape_info(&mut c.m, STANDINGS3_NAME, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTournamentStandings::ProcessInput` (SCREENPOST.CPP, 0x80039de4): accept before the
/// money is counted down starts the countdown instead.
pub fn process_input(m: &mut Mem, this: Screen, key: u32) {
    if m.r32(key) == KEY_ACCEPT && m.r32(this.0 + COUNTED_DOWN) == 0 {
        m.w32(key, KEY_NONE);
        m.w32(this.0 + COUNTDOWN_NOW, 1);
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(&mut c.m, Screen(c.r[4]), c.r[6]);
}

/// The title's mark sliding to and fro under it (`MARK`'s width): the slide and the middle.
fn slide(m: &mut Mem) -> (i32, i32) {
    let mark = TextureShapeInfo(m.r32(PERMANENT_SHAPES)).at(MARK).0;
    let w = lh(m, mark + 16);
    assert!(w != 0, "a mark 0 wide (the original's division `break`s)");
    let mut slide = (m.r32(GAME_TICKS) as i32).wrapping_rem(w);
    if w / 2 < slide {
        slide = w - slide;
    }
    (slide, ((w >> 1) - 2).wrapping_sub(lh(m, mark + 20)))
}

/// The title's mark: its halves sliding apart and back, then the band under the title.
fn title_mark(c: &mut Cpu, this: u32) {
    let (s, mid) = slide(&mut c.m);
    for x in [mid.wrapping_add(s), mid.wrapping_sub(s)] {
        let y = word_y(c, TITLE_Y);
        let f = fade(&mut c.m, this);
        call(c, DRAW_SHAPE_EXTENDED, &[MARK_HALF, 0, x as u32, y.wrapping_add(1), f, 1, 0]);
    }
    with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
        DrawShapeExtended(options).set_tint(&mut c.m, 0, BAND as i32);
        let f = fade(&mut c.m, this);
        call(c, DRAW_SHAPE_EXTENDED, &[MARK, 0x400, 0, u32::MAX, f, 0, options]);
    });
}

/// A title's box: a black box the text `id`'s width under it, centred on the title's place.
fn boxed_title(c: &mut Cpu, id: u32) {
    let w = call(c, TEXT_WORD, &[id]);
    let px = call(c, TEXT_PIXELS, &[w]);
    let x = word_x(c, TITLE_X).wrapping_sub(((px as i32) >> 1) as u32);
    let y = word_y(c, TITLE_Y);
    call_addr(c, DRAW_SQUARE, &[0, x, y.wrapping_sub(1), px, 9]);
}

/// `tScreenTournamentStandings::DrawBackground` (SCREENPOST.CPP, 0x80039e14). It draws each
/// racer's place, name (the player's lit) and points (or, knocking out, survived or out); the
/// track and the tournament, boxed, with a mark sliding under them; a line. With the money
/// shown (after 1000 ticks, or once asked), what was won, billed and the bonus count down. Then
/// the final money and the picture.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    let m = &mut c.m;
    let f = fade(m, this);
    let definition = m.r32(TOURNAMENT_MANAGER + DEFINITION);
    let tier = m.r32(TOURNAMENT_MANAGER + TIER);
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
    let t = definition.wrapping_add(first.wrapping_add(m.r32(TOURNAMENT_MANAGER + TOURNAMENT)).wrapping_mul(84)) + 36;
    // a knock-out tournament's rows: the racers and the one knocked out
    let knockout = m.r8(t + KNOCKOUT) != 0;
    let n = sx((m.r16(TOURNAMENT_MANAGER + RACERS) as u32).wrapping_add(knockout as u32));
    let mut k = 0u32;
    while (k as i32) < n as i32 {
        draw_racer(c, f, t, k, n);
        k += 1;
    }
    // the track and the tournament
    let track = call(c, TRACK_RACED, &[]);
    // (the track's description read, and not used: as the original)
    with_buffer(c, TRACK_SIZE, |c, info| call_addr(c, GET_TRACK, &[TRACK_MANAGER, sx(track), info]));
    let track = call(c, TRACK_RACED, &[]);
    let w = call(c, TEXT_WORD, &[sx(track).wrapping_add(TRACK_NAMES)]);
    let x = sx(word_x(c, TITLE_X));
    let y = word_y(c, TRACK_Y);
    fetextrender::full_text_fade(c, f, w, x, sx(y), 11, 2, 2);
    let m = &mut c.m;
    let it = m.r32(SONGS_SCREEN) + if m.r8(FE_TIER) != 0 { TIER_ITERATOR } else { ITERATOR };
    let name = sx(call(c, TOURNAMENT_TEXT, &[it, u32::MAX])).wrapping_add(TOURNAMENT_NAMES);
    let x = sx(word_x(c, TITLE_X));
    let y = word_y(c, TITLE_Y);
    fetextrender::menu_text_positioned_justify_fade(c, f, sx(name), x, sx(y), 2, 2, 11);
    boxed_title(c, name);
    title_mark(c, this);
    // a line
    let x = word_x(c, TITLE_X).wrapping_sub(150);
    let y = word_y(c, TRACK_Y);
    call_addr(c, DRAW_BRIGHT_END_LINE, &[SHADE, x, y.wrapping_add(10), 300, 1, 3, f, 30]);
    // the money: counted down once shown
    let m = &mut c.m;
    let k = m.r8(MONEY_COLOR) as u32;
    let colours = (fefades::fade(m.r32(COLORS + 4 * k), 0, f), fefades::fade(SHADE, 0, f));
    let late = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + START)) as i32 >= MONEY_AFTER;
    if late || m.r32(this + COUNTDOWN_NOW) != 0 {
        if (m.r32(this + AWARDED) as i32) > 0 || (m.r32(this + DAMAGE) as i32) > 0 || (m.r32(this + BONUS) as i32) > 0 {
            call(c, PLAY_FE_SFX, &[SOUND_TICK]);
        }
        count_down(&mut c.m, this);
    }
    if c.m.r32(this + DRAW_MONEY_ON) != 0 {
        for (id, got, amount) in [(WON, GOT_MONEY, AWARDED), (BILLED, GOT_BILLED, DAMAGE), (BONUS_TEXT, GOT_BONUS, BONUS)] {
            let w = call(c, TEXT_WORD, &[id]);
            let x = sx(word_x(c, LABEL_X));
            let y = word_y(c, id);
            let state = (c.m.r32(this + got) != 0) as u32;
            fetextrender::full_text_fade(c, f, w, x, sx(y), 11, state, 1);
            let (x, y) = (word_x(c, VALUE_X), word_y(c, id));
            let value = c.m.r32(this + amount);
            draw_money(c, x, y, AMOUNT, value, colours.0, colours.1);
        }
    }
    // the final money: what is left to count down taken off
    let w = call(c, TEXT_WORD, &[TOTAL]);
    let x = sx(word_x(c, LABEL_X));
    let y = word_y(c, TOTAL);
    fetextrender::full_text_fade(c, f, w, x, sx(y), 11, 2, 1);
    let (x, y) = (word_x(c, VALUE_X), word_y(c, TOTAL));
    let m = &mut c.m;
    let total = m.r32(this + FINAL).wrapping_sub(m.r32(this + AWARDED)).wrapping_add(m.r32(this + DAMAGE)).wrapping_sub(m.r32(this + BONUS));
    draw_money(c, x, y, TOTAL_KIND, total, colours.0, colours.1);
    let table = TextureShapeInfo(c.m.r32(PERMANENT_SHAPES));
    fescreen::draw_background_image(c, Screen(this), 10, 29, table, 0);
}

/// Row `k` of the standings (of `n`) in tournament `t`, faded by `f`: the place, the racer
/// there (the player's lit), its points (or, knocking out, survived or out: the last row).
fn draw_racer(c: &mut Cpu, f: u32, t: u32, k: u32, n: u32) {
    let place = sx(call(c, PLAYER_RANKING, &[TOURNAMENT_MANAGER, sx(k + 1)]));
    let player = place == 0;
    let w = call(c, TEXT_WORD, &[k + PLACES]);
    let x = sx(word_x(c, PLACE_X));
    let y = word_y(c, ROWS + k);
    fetextrender::full_text_fade(c, f, w, x, sx(y), 11, if player { 2 } else { 0 }, 0);
    let state = if player { 2 } else { 1 };
    let name = if player {
        call(c, PLAYER_NAME, &[0])
    } else {
        let id = c.m.r32(TOURNAMENT_MANAGER.wrapping_add(place << 4) + COMPETITORS);
        call(c, COMPETITOR_NAME, &[id])
    };
    let x = sx(word_x(c, NAME_X));
    let y = word_y(c, ROWS + k);
    fetextrender::full_text_fade(c, f, name, x, sx(y), 11, state, 0);
    // the points (the place given by address), or the knock-out mark
    with_buffer(c, 136, |c, at| {
        let (racer, text) = (at, at + 8);
        c.m.w16(racer, place as u16);
        if c.m.r8(t + KNOCKOUT) != 0 {
            let w = call(c, TEXT_WORD, &[if k == n.wrapping_sub(1) { KNOCKED_OUT } else { SURVIVED }]);
            call(c, SPRINTF, &[text, w]);
        } else {
            let points = sx(call(c, POINT_TOTAL, &[TOURNAMENT_MANAGER, racer]));
            let w = call(c, TEXT_WORD, &[POINTS]);
            call(c, SPRINTF, &[text, POINTS_FORMAT, points, w]);
        }
        let x = sx(word_x(c, VALUE_X));
        let y = word_y(c, ROWS + k);
        fetextrender::full_text_fade(c, f, text, x, sx(y), 11, state, 1);
    });
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

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenPinkSlipStandings::DrawBackground` (SCREENPOST.CPP, 0x8003a6d8): the tracks raced
/// (the one chosen lit, those before it half) and their winners (or none); the two players'
/// wins; the title boxed, a mark sliding under it; the picture.
pub fn pink_slip_draw_background(c: &mut Cpu, this: Screen) {
    let this = this.0;
    let f = fade(&mut c.m, this);
    let text_at = |c: &mut Cpu, text: u32, row: u32, state: u32| {
        let x = sx(word_x(c, VALUE_X));
        let y = word_y(c, row);
        fetextrender::full_text_fade(c, f, text, x, sx(y), 11, state, 1);
    };
    with_buffer(c, TRACK_SIZE + 128, |c, at| {
        let (info, text) = (TrackInformation(at), at + TRACK_SIZE);
        let mut k = 0u32;
        while (k as i32) < c.m.r8(PINK_COUNT) as i32 {
            let m = &mut c.m;
            let chosen = m.r8(PINK_CHOSEN) as u32;
            let state = if k == chosen { 2 } else { ((k as i32) < chosen as i32) as u32 };
            let id = m.r8(PINK_TRACKS.wrapping_add(k)) as u32;
            call_addr(c, GET_TRACK, &[TRACK_MANAGER, id, info.0]);
            let x = sx(word_x(c, PLACE_X));
            let y = word_y(c, ROWS + k);
            let m = &mut c.m;
            let name = sx((info.track_id(m) as i32 as u32).wrapping_add(TRACK_NAMES));
            let f = fade(m, this);
            fetextrender::menu_text_positioned_justify_fade(c, f, name, x, sx(y), 0, state, 11);
            let winner = lh(&mut c.m, PINK_WINNERS.wrapping_add(k << 1));
            let w = if winner == -1 { call(c, TEXT_WORD, &[NO_WINNER]) } else { call(c, PLAYER_NAME, &[winner as u32]) };
            call(c, SPRINTF, &[text, w]);
            text_at(c, text, ROWS + k, state);
            k += 1;
        }
        // the players' wins
        for k in 0..2u32 {
            let name = call(c, PLAYER_NAME, &[k]);
            let x = sx(word_x(c, NAME_X));
            let y = word_y(c, WINS_ROWS + k);
            fetextrender::full_text_fade(c, f, name, x, sx(y), 11, 2, 0);
            let wins = c.m.r8(PINK_WINS + k) as u32;
            if wins == 1 {
                let w = call(c, TEXT_WORD, &[ONE_WIN]);
                call(c, SPRINTF, &[text, w]);
            } else {
                let w = call(c, TEXT_WORD, &[WINS]);
                call(c, SPRINTF, &[text, w, wins]);
            }
            text_at(c, text, WINS_ROWS + k, 2);
        }
    });
    // the title, the mark, the picture
    let x = sx(word_x(c, TITLE_X));
    let y = word_y(c, TITLE_Y);
    fetextrender::menu_text_positioned_justify_fade(c, f, PINK_TITLE, x, sx(y), 2, 2, 11);
    boxed_title(c, PINK_TITLE);
    title_mark(c, this);
    let table = TextureShapeInfo(c.m.r32(PERMANENT_SHAPES));
    fescreen::draw_background_image(c, Screen(this), 10, 29, table, 0);
}

pub fn abi_pink_slip_draw_background(c: &mut Cpu) {
    pink_slip_draw_background(c, Screen(c.r[4]));
}

/// `tScreenPinkSlipStandings::ProcessInput` (SCREENPOST.CPP, 0x8003aafc): the screen's (which
/// does nothing).
pub fn abi_pink_slip_process_input(_: &mut Cpu) {}

/// `tScreenPinkSlipStandings::~tScreenPinkSlipStandings` (0x8003ab1c),
/// `tScreenTournamentStandings3item::~...` (0x8003ab3c), `tScreenTournamentStandings::~...`
/// (0x8003ab5c), all SCREENPOST.H: the screen's.
pub fn abi_pink_slip_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

pub fn abi_destructor3(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, COMMAND, Case, KEY, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The screen's fade.
    const FADE: u32 = 92;

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
            // (only the kinds with a currency word: for others the original draws its stack
            // leftover, and the game never asks for them)
            ("DrawMoney", 0x8003_9a80, nfs4rc::f_80039a80, |_, r| vec![pick(r, 400), pick(r, 240), [3, 6, 9][pick(r, 3) as usize], [0, 1, 999, 1_000_000, (-5000i32) as u32, r.next() as u32][pick(r, 6) as usize], 0xBEBE, 0x0023_2323]),
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

}
