//! FETOURN.CPP: the tournaments (`tTournamentManager`, `tournamentManager`): their definition
//! read from `tourn.trn` (tiers, tournaments, tracks); a tournament started, its tracks' settings
//! (random where the definition leaves them open), the racers' points and places after each track
//! (knock-outs eliminate the last), the money won, the damage billed and the bonus, the prizes,
//! what winning unlocks; the tournament list's iterator (`tListIteratorTournament`), which skips
//! tournaments not yet earned.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::femenuextended::SPRINTF;
use crate::globals::{FRONT_END, PATHS, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::types::{CarInfo, CarManager, ListIterator};
use crate::{fecars, fecheats, femenu, fetracks};
use ps1rt::{Cpu, Mem};

// A tournament manager: +0 how many tiers, +4 the tier, +8 the tournament, +12 the track (from
// 0), +16 the racers left, +20 the money, +24 the definition, +36 and +44 each tier's finish prize
// and its change (`short`s), +52 the tournaments listed, +182 the tracks listed (texts, ended by
// 0), +216, +232, +248, +264 each track's direction, mirroring, time of day and weather, +280 the
// competitors (16 bytes each: +0 their personality, +5 the player's, +8 eliminated, +12 points,
// +14 place), +376 the car lineup (20 bytes each: +0 the player's, +4 personality, +8 place),
// +496 each tournament's best place (by id; 7 not placed), +560 the last best place, +561 the
// points per place, +567 the ranking, +576 the award (`tAwardInformation`, 68 bytes).
const TIERS: u32 = 0;
const TIER: u32 = 4;
const TOURNAMENT: u32 = 8;
const TRACK: u32 = 12;
const RACERS: u32 = 16;
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const TIER_PRIZE: u32 = 36;
const TIER_PRIZE_CHANGE: u32 = 44;
const TOURNAMENT_LIST: u32 = 52;
const TRACK_LIST: u32 = 182;
const DIRECTIONS: u32 = 216;
const MIRRORS: u32 = 232;
const TIMES_OF_DAY: u32 = 248;
const WEATHERS: u32 = 264;
const COMPETITORS: u32 = 280;
const LINEUP: u32 = 376;
const LINEUP_SIZE: u32 = 20;
const BEST_PLACES: u32 = 496;
const LAST_BEST: u32 = 560;
const FINISH_POINTS: u32 = 561;
const RANKING: u32 = 567;
const AWARD: u32 = 576;
const AWARD_SIZE: u32 = 68;
/// Each track's settings (+1..+4 of a track's description), in the manager by track.
const SETTINGS: [u32; 4] = [DIRECTIONS, MIRRORS, TIMES_OF_DAY, WEATHERS];
/// A setting the definition leaves open (random).
const RANDOM: u8 = 2;
/// The money to start with; the places with prizes and points; not placed.
const START_MONEY: u32 = 20000;
const PRIZE_PLACES: u32 = 6;
const NOT_PLACED: u8 = 7;
/// A saved game's best places (+104).
const SAVE_BEST_PLACES: u32 = 104;

// A competitor: its personality, the player's, eliminated, points, place.
const PERSONALITY: u32 = 0;
const IS_PLAYER: u32 = 5;
const ELIMINATED: u32 = 8;
const POINTS: u32 = 12;
const PLACE: u32 = 14;

// The award: +0 money won, +4 the tournament's, +8 what it unlocks (`short` flags: 1 a track, 2
// a class (a bonus code), 4 a car, 8 a track class, 16 a bonus), +10 the track, +12 the class,
// +16 the car, +20 the track class, +24 the bonus, +28 a car won, +32 the garage full, +36 its
// money, +40 its model, +44 its colour, +45 its upgrades, +48 the tier completed, +52 its text,
// +56 its car, +60 the garage full, +64 its money.
const AWARD_MONEY: u32 = 0;
const AWARD_TOURN_MONEY: u32 = 4;
const ACTIVATE: u32 = 8;
const ACT_TRACK: u32 = 10;
const ACT_CLASS: u32 = 12;
const ACT_CAR: u32 = 16;
const ACT_TRACK_CLASS: u32 = 20;
const ACT_BONUS: u32 = 24;
const AWARD_CAR: u32 = 28;
const AWARD_CAR_FULL: u32 = 32;
const AWARD_CAR_MONEY: u32 = 36;
const AWARD_CAR_MODEL: u32 = 40;
const AWARD_CAR_COLOR: u32 = 44;
const AWARD_CAR_UPGRADES: u32 = 45;
const COMPLETED_TIER: u32 = 48;
const COMPLETED_TEXT: u32 = 52;
const COMPLETED_CAR: u32 = 56;
const COMPLETED_FULL: u32 = 60;
const COMPLETED_MONEY: u32 = 64;
/// What a tournament unlocks.
const UNLOCK_TRACK: u16 = 1;
const UNLOCK_CLASS: u16 = 2;
const UNLOCK_CAR: u16 = 4;
const UNLOCK_TRACK_CLASS: u16 = 8;
const UNLOCK_BONUS: u16 = 0x10;

// The definition: the tiers (12 bytes each: +0 how many tournaments, +2 the first), the
// tournaments from +36 (84 bytes each: +0 id, +1 tracks, +2 the first track, +3 the opponents'
// class (10: the player's), +5 knock-out, +6 cars, +7 a car awarded, +8 its model, +9 its
// upgrades, +10 what it unlocks, +12 what it requires (1 a tournament placed, 2 won, 4 a car),
// +14..+16 the track, class and car unlocked, +17 the tournament required, +18 the car, +19 its
// upgrades (0 none, 1 any, 2 some, 3 these), +20 these, +22 the trophy, +24 the prizes (6),
// +48 the entry fee, +51 the opponents' personalities, +67 the track class unlocked, +68 the
// bonus), the tracks from +5412 (40 bytes each: +0 number, +1 direction, +2 mirrored, +3 time of
// day, +4 weather (2 and up: random), +8 the prizes (6)); 10532 bytes.
const TIER_SIZE: u32 = 12;
const TOURNAMENTS: u32 = 36;
const TOURNAMENT_SIZE: u32 = 84;
const OPPONENT_CLASS: u32 = 3;
const PLAYERS_CLASS: u8 = 10;
const KNOCKOUT: u32 = 5;
const CARS: u32 = 6;
const UNLOCKS: u32 = 10;
const REQUIRES: u32 = 12;
const REQUIRES_PLACED: u16 = 1;
const REQUIRES_WON: u16 = 2;
const REQUIRES_CAR: u16 = 4;
const TROPHY: u32 = 22;
const TOURNAMENT_PRIZES: u32 = 24;
const OPPONENTS: u32 = 51;
const TRACKS: u32 = 5412;
const TRACK_SIZE: u32 = 40;
const TRACK_PRIZES: u32 = 8;
const DEFINITION_SIZE: u32 = 10532;

/// The race's results (`Stats`, 160 bytes a car: +112 damage (16.16), +128 its state (2
/// finished), +132 place).
const STATS: u32 = 0x8011_0E08;
const STATS_SIZE: u32 = 160;
const STAT_DAMAGE: u32 = 112;
const STAT_STATE: u32 = 128;
const STAT_PLACE: u32 = 132;
const FINISHED: u32 = 2;
/// The car manager; the player's car in the garage (`frontEnd + 0x123`); the front end's tier
/// (`+0x122`); a car description's colour order (+0xAF).
const CAR_MANAGER: CarManager = crate::globals::CAR_MANAGER;
const GARAGE_CAR: u32 = FRONT_END + 0x123;
const FE_TIER: u32 = FRONT_END + 0x122;
const COLOR_ORDER: u32 = 0xAF;
/// The damage bill and the bonus worked out (`CalcTrackFinishDamageBill`'s).
const DAMAGE_BILL: u32 = 0x8005_1EE8;
const BONUS: u32 = 0x8005_1EEC;
/// "%s%s", "tourn.trn", "Tourney"; the front end's directory; the prize multipliers by class
/// (24 bytes each, 7 of them); "SML", "WGSB", "TR%c%02d%c"; the iterator's vtable (+12), its
/// manager (+16).
const PATH_FORMAT: u32 = 0x8001_153C;
const FILE: u32 = 0x8001_1544;
const MEMORY_NAME: u32 = 0x8001_1550;
const FRONT_END_PATH: u32 = PATHS + 0x94;
const MULTIPLIERS: u32 = 0x8001_1558;
const SIZES: u32 = 0x8001_1600;
const PLACES: u32 = 0x8001_1604;
const TROPHY_FORMAT: u32 = 0x8001_1608;
const ITERATOR_VT: u32 = 0x8001_1618;
const ITERATOR_VPTR: u32 = 12;
const ITERATOR_MANAGER: u32 = 16;
/// The texts: the tracks' names (214 on), the tournaments' (834 on, by id), a tier's
/// completion (986 on).
const TRACK_NAMES: u32 = 214;
const NAMES: u32 = 834;
const COMPLETED_TEXTS: u32 = 986;
/// The bonus code of each tier (13 on), the car it gives (19 on).
const TIER_BONUS: u32 = 13;
const TIER_CAR: u32 = 19;
/// The garage's size.
const GARAGE: i32 = 32;

/// `loadfileadr`, `reservememadr`, `blockmove`, `purgememadr`, `rand`, `qsort`, `fixedmult`;
/// `tournPointsCompare` (`qsort` calls it by address).
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e63d4;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const QSORT: fn(&mut Cpu) = nfs4rc::f_800e6978;
const FIXEDMULT: fn(&mut Cpu) = nfs4rc::f_800e4f14;
const POINTS_COMPARE: u32 = 0x8003_2BA0;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// Tournament `k` of tier `tier` of the definition.
fn tournament_of(m: &mut Mem, definition: u32, tier: u32, k: u32) -> u32 {
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(TIER_SIZE)) + 2) as u32;
    definition.wrapping_add(first.wrapping_add(k).wrapping_mul(TOURNAMENT_SIZE)) + TOURNAMENTS
}

/// The manager's tournament.
fn current(m: &mut Mem, this: u32) -> u32 {
    let (definition, tier, k) = (m.r32(this + DEFINITION), m.r32(this + TIER), m.r32(this + TOURNAMENT));
    tournament_of(m, definition, tier, k)
}

/// Track `k` of the definition.
fn track(definition: u32, k: u32) -> u32 {
    definition.wrapping_add(k.wrapping_mul(TRACK_SIZE)) + TRACKS
}

/// Competitor `k`.
fn competitor(this: u32, k: u32) -> u32 {
    this.wrapping_add(k << 4) + COMPETITORS
}

/// `tTournamentManager::Initialize` (FETOURN.CPP, 0x80032378): 20000 to spend, no definition,
/// no tracks listed, nobody ranked, no tournament placed in, each tier's prize 6; the player's
/// last car the one in the garage.
pub fn initialize(m: &mut Mem, this: u32) {
    m.w32(this + MONEY, START_MONEY);
    m.w32(this + TRACK, 0);
    m.w8(this + TIERS, 0);
    m.w32(this + DEFINITION, 0);
    m.w16(this + TOURNAMENT_LIST, 0);
    m.w16(this + TRACK_LIST, 0);
    for k in 0..PRIZE_PLACES {
        m.w8(this + RANKING + k, 0);
    }
    for k in 0..64 {
        m.w8(this + BEST_PLACES + k, NOT_PLACED);
    }
    for k in 0..3 {
        m.w16(this + TIER_PRIZE + 2 * k, 6);
        m.w16(this + TIER_PRIZE_CHANGE + 2 * k, 0);
    }
    let last = m.r32(CAR_MANAGER.0).wrapping_sub(1) as u8;
    m.w8(GARAGE_CAR, last);
    m.w8(GARAGE_CAR + 1, last);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, c.r[4]);
}

/// `tTournamentManager::LoadDescription` (FETOURN.CPP, 0x8003244c): `tourn.trn` read (the old
/// definition freed first): the points per place, the tiers, each tier's tournaments, each
/// tournament's tracks copied into a new definition (each in its place by the numbers the
/// file gives); the file freed.
pub fn load_description(c: &mut Cpu, this: u32) {
    let data = with_buffer(c, 80, |c, name| {
        let path = c.m.r32(FRONT_END_PATH);
        call(c, SPRINTF, &[name, PATH_FORMAT, path, FILE]);
        release_description(c, this);
        call(c, LOAD_FILE, &[name, 16])
    });
    call(c, BLOCKMOVE, &[data, this + FINISH_POINTS, PRIZE_PLACES]);
    let tiers = c.m.r8(data + 6);
    c.m.w8(this + TIERS, tiers);
    let definition = call(c, RESERVE, &[MEMORY_NAME, DEFINITION_SIZE, 0]);
    c.m.w32(this + DEFINITION, definition);
    let def = |c: &mut Cpu| c.m.r32(this + DEFINITION);
    let mut from = data + 7;
    let mut t = 0u32;
    if c.m.r8(this + TIERS) == 0 {
        call(c, PURGE, &[data]);
        return;
    }
    loop {
        let tier_at = sx(t).wrapping_mul(TIER_SIZE);
        let d = def(c);
        call(c, BLOCKMOVE, &[from, d.wrapping_add(tier_at), TIER_SIZE]);
        let tier = def(c).wrapping_add(tier_at);
        let (first, n) = (c.m.r8(tier + 2) as u32, c.m.r8(tier) as u32);
        from = from.wrapping_add(TIER_SIZE);
        // its tournaments
        let mut k = first;
        while (k as i32) < first.wrapping_add(n) as i32 {
            let at = sx(k).wrapping_mul(TOURNAMENT_SIZE);
            let d = def(c);
            call(c, BLOCKMOVE, &[from, d.wrapping_add(at) + TOURNAMENTS, TOURNAMENT_SIZE]);
            let tournament = def(c).wrapping_add(at) + TOURNAMENTS;
            let (first_track, tracks) = (c.m.r8(tournament + 2) as u32, c.m.r8(tournament + 1) as u32);
            from = from.wrapping_add(TOURNAMENT_SIZE);
            // its tracks
            if (first_track as i32) < first_track.wrapping_add(tracks) as i32 {
                let mut j = first_track;
                loop {
                    let d = def(c);
                    call(c, BLOCKMOVE, &[from, track(d, sx(j)), TRACK_SIZE]);
                    j = j.wrapping_add(1);
                    let d = def(c);
                    let tracks = c.m.r8(d.wrapping_add(at) + TOURNAMENTS + 1) as u32;
                    from = from.wrapping_add(TRACK_SIZE);
                    if sx(j) as i32 >= first_track.wrapping_add(tracks) as i32 {
                        break;
                    }
                }
            }
            k = k.wrapping_add(1);
            let d = def(c);
            let n = c.m.r8(d.wrapping_add(sx(t).wrapping_mul(TIER_SIZE))) as u32;
            if sx(k) as i32 >= first.wrapping_add(n) as i32 {
                break;
            }
        }
        t = t.wrapping_add(1);
        if sx(t) as i32 >= c.m.r8(this + TIERS) as i32 {
            break;
        }
    }
    call(c, PURGE, &[data]);
}

pub fn abi_load_description(c: &mut Cpu) {
    load_description(c, c.r[4]);
}

/// `tTournamentManager::ReleaseDescription` (FETOURN.CPP, 0x80032698): the definition freed.
pub fn release_description(c: &mut Cpu, this: u32) {
    let definition = c.m.r32(this + DEFINITION);
    if definition != 0 {
        call(c, PURGE, &[definition]);
        c.m.w32(this + DEFINITION, 0);
    }
}

pub fn abi_release_description(c: &mut Cpu) {
    release_description(c, c.r[4]);
}

/// `tTournamentManager::UpdateTrackList` (FETOURN.CPP, 0x800326d4): the tracks of tournament
/// `k` of tier `tier` listed (their names' texts), ended by 0.
pub fn update_track_list(m: &mut Mem, this: u32, tier: u32, k: u32) {
    let definition = m.r32(this + DEFINITION);
    let t = tournament_of(m, definition, sx(tier), sx(k));
    let (n, first) = (m.r8(t + 1) as u32, m.r8(t + 2) as u32);
    for j in 0..n {
        let definition = m.r32(this + DEFINITION);
        let number = m.r8(track(definition, sx(first.wrapping_add(j)))) as i8 as i32 as u32;
        m.w16(this + TRACK_LIST + 2 * sx(j), number.wrapping_add(TRACK_NAMES) as u16);
    }
    m.w16(this + TRACK_LIST + 2 * n, 0);
}

pub fn abi_update_track_list(c: &mut Cpu) {
    update_track_list(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tTournamentManager::GetTrackList` (FETOURN.CPP, 0x800327a4): the tracks of tournament `k`
/// of tier `tier` listed; the list.
pub fn get_track_list(m: &mut Mem, this: u32, tier: u32, k: u32) -> u32 {
    update_track_list(m, this, sx(tier), sx(k));
    this + TRACK_LIST
}

pub fn abi_get_track_list(c: &mut Cpu) {
    c.r[2] = get_track_list(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tTournamentManager::GetTrackToRace` (FETOURN.CPP, 0x800327dc): the track to race into `out`,
/// with its settings for this tournament.
pub fn get_track_to_race(c: &mut Cpu, this: u32, out: u32) {
    let m = &mut c.m;
    let t = current(m, this);
    let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK));
    let definition = m.r32(this + DEFINITION);
    call(c, BLOCKMOVE, &[track(definition, k), out, TRACK_SIZE]);
    let m = &mut c.m;
    for (f, at) in SETTINGS.into_iter().enumerate() {
        let j = m.r32(this + TRACK);
        let v = m.r8(this.wrapping_add(j) + at);
        m.w8(out + 1 + f as u32, v);
    }
}

pub fn abi_get_track_to_race(c: &mut Cpu) {
    get_track_to_race(c, c.r[4], c.r[5]);
}

/// `tTournamentManager::StartNewTournament` (FETOURN.CPP, 0x800328d4): tournament `k` of tier
/// `tier` from its first track: its competitors (the player first, last on the grid; the others
/// its personalities, placed in order), each track's settings (random where the definition
/// leaves them open).
pub fn start_new_tournament(c: &mut Cpu, this: u32, tier: u32, k: u32) {
    let m = &mut c.m;
    m.w32(this + TIER, tier & 0xFF);
    m.w32(this + TOURNAMENT, k & 0xFF);
    m.w32(this + TRACK, 0);
    let n = sx(get_num_competitors(m, this));
    m.w32(this + RACERS, n);
    let t = current(m, this);
    let mut j = 0u32;
    while (sx(j) as i32) < m.r32(this + RACERS) as i32 {
        let comp = competitor(this, sx(j));
        m.w16(comp + POINTS, 0);
        m.w32(comp + ELIMINATED, 0);
        m.w8(comp + IS_PLAYER, 0);
        let personality = if sx(j) != 0 { m.r8(t.wrapping_add(sx(j)) + OPPONENTS) as u32 } else { 0 };
        m.w32(comp + PERSONALITY, personality);
        let place = if sx(j) != 0 { j as u8 } else { m.r8(this + RACERS) };
        m.w8(comp + PLACE, place);
        j = j.wrapping_add(1);
    }
    let mut j = 0u32;
    while (sx(j) as i32) < c.m.r8(t + 1) as i32 {
        let m = &mut c.m;
        let definition = m.r32(this + DEFINITION);
        let tr = track(definition, (m.r8(t + 2) as u32).wrapping_add(sx(j)));
        let settings = this.wrapping_add(sx(j));
        for (f, at) in SETTINGS.into_iter().enumerate() {
            let v = c.m.r8(tr + 1 + f as u32);
            c.m.w8(settings + at, v);
            if v >= RANDOM {
                let v = call(c, RAND, &[]) & 1;
                c.m.w8(settings + at, v as u8);
            }
        }
        j = j.wrapping_add(1);
    }
    c.m.w8(this + COMPETITORS + IS_PLAYER, 1);
}

pub fn abi_start_new_tournament(c: &mut Cpu) {
    start_new_tournament(c, c.r[4], c.r[5], c.r[6]);
}

/// `tTournamentManager::IsTournamentFinished` (FETOURN.CPP, 0x80032b0c): every track raced, or
/// (knocking out) the player's points behind the track.
pub fn is_tournament_finished(m: &mut Mem, this: u32) -> bool {
    let t = current(m, this);
    let j = m.r32(this + TRACK) as i32;
    if j < m.r8(t + 1) as i32 { m.r8(t + KNOCKOUT) != 0 && (m.r16(this + COMPETITORS + POINTS) as i32) < j } else { true }
}

pub fn abi_is_tournament_finished(c: &mut Cpu) {
    c.r[2] = is_tournament_finished(&mut c.m, c.r[4]) as u32;
}

/// `tournPointsCompare` (FETOURN.CPP, 0x80032ba0): `qsort`'s order of two competitors (indexes
/// at `a`, `b`): more points first, then the player, then the better place in the race.
pub fn tourn_points_compare(m: &mut Mem, a: u32, b: u32) -> u32 {
    let (ka, kb) = (m.r8(a) as u32, m.r8(b) as u32);
    let (ca, cb) = (competitor(TOURNAMENT_MANAGER, ka), competitor(TOURNAMENT_MANAGER, kb));
    let mut v = (m.r16(cb + POINTS) as u32).wrapping_sub(m.r16(ca + POINTS) as u32);
    if v == 0 {
        v = (m.r8(cb + IS_PLAYER) as i8 as i32 as u32).wrapping_sub(m.r8(ca + IS_PLAYER) as i8 as i32 as u32);
        if v == 0 {
            v = m.r32(STATS + STATS_SIZE * ka + STAT_PLACE).wrapping_sub(m.r32(STATS + STATS_SIZE * kb + STAT_PLACE));
        }
    }
    v
}

pub fn abi_tourn_points_compare(c: &mut Cpu) {
    c.r[2] = tourn_points_compare(&mut c.m, c.r[4], c.r[5]);
}

/// The award's money: `money` added.
fn add_award(m: &mut Mem, this: u32, at: u32, money: u32) {
    let v = m.r32(this + AWARD + at).wrapping_add(money);
    m.w32(this + AWARD + at, v);
}

/// `tTournamentManager::UpdateTournFinishMoney` (FETOURN.CPP, 0x80032c2c): the tournament's
/// prize for the player's place (knocking out) or each ranked place it holds, its best place
/// (the one before kept).
pub fn update_tourn_finish_money(m: &mut Mem, this: u32) {
    let t = current(m, this);
    let id = m.r8(t) as i8 as i32 as u32;
    let best = m.r8(this.wrapping_add(id) + BEST_PLACES);
    m.w8(this + LAST_BEST, best);
    if m.r8(t + KNOCKOUT) != 0 {
        let place = (m.r8(this + COMPETITORS + PLACE) as u32).wrapping_sub(1);
        let prize = get_tournament_finish_prize(m, this, place);
        add_award(m, this, AWARD_TOURN_MONEY, prize);
        return;
    }
    for k in 0..PRIZE_PLACES {
        // (the player ranked in place `k`)
        if m.r8(this + RANKING + k) == 0 {
            let prize = get_tournament_finish_prize(m, this, k);
            add_award(m, this, AWARD_TOURN_MONEY, prize);
            let id = m.r8(t) as i8 as i32 as u32;
            let at = this.wrapping_add(id) + BEST_PLACES;
            if (k as i32) < m.r8(at) as i8 as i32 {
                m.w8(at, (k + 1) as u8);
            }
        }
    }
}

pub fn abi_update_tourn_finish_money(c: &mut Cpu) {
    update_tourn_finish_money(&mut c.m, c.r[4]);
}

/// `tTournamentManager::UpdateTrackFinishMoney` (FETOURN.CPP, 0x80032d5c): the track's prize for
/// the player's place, if it finished.
pub fn update_track_finish_money(c: &mut Cpu, this: u32) {
    if c.m.r32(STATS + STAT_STATE) != FINISHED {
        return;
    }
    let place = sx((c.m.r16(STATS + STAT_PLACE) as u32).wrapping_sub(1));
    let prize = get_track_finish_prize(c, this, place);
    add_award(&mut c.m, this, AWARD_MONEY, prize);
}

pub fn abi_update_track_finish_money(c: &mut Cpu) {
    update_track_finish_money(c, c.r[4]);
}

/// A garage car's worth: its price and those of the upgrades it has (the first `n`).
fn worth(m: &mut Mem, car: CarInfo, n: u32) -> u32 {
    let upgrades = car.upgrades(m) as u32;
    let mut worth = car.prices(m, 0) as u32;
    for k in 0..n {
        if upgrades & (1 << k) != 0 {
            worth = worth.wrapping_add(car.prices(m, k + 1) as u32);
        }
    }
    worth
}

/// `tTournamentManager::CalcTrackFinishDamageBill` (FETOURN.CPP, 0x80032dbc): (worked out when
/// `work`) the player's car's worth (with its first two upgrades); damaged, the bill is three
/// ten thousandths of it per damage point; undamaged and in the first three, a bonus (a
/// twentieth, three hundredths, a hundredth); the bill and the bonus at `bill_out`,
/// `bonus_out`.
pub fn calc_track_finish_damage_bill(c: &mut Cpu, work: bool, bill_out: u32, bonus_out: u32) {
    if work {
        let k = c.m.r8(GARAGE_CAR) as u32;
        let worth = with_buffer(c, CarInfo::SIZE, |c, car| {
            fecars::get_garage_car(c, CAR_MANAGER, k, car, 0);
            worth(&mut c.m, CarInfo(car), 2) as i32
        });
        let m = &mut c.m;
        let raw = m.r32(STATS + STAT_DAMAGE) as i32;
        // (16.16, toward zero)
        let damage = (if raw < 0 { raw.wrapping_add(0xFFFF) } else { raw }) >> 16;
        if damage != 0 || m.r32(STATS + STAT_PLACE) as i32 >= 4 {
            m.w32(BONUS, 0);
            m.w32(DAMAGE_BILL, (worth.wrapping_mul(damage).wrapping_mul(3) / 10000) as u32);
        } else {
            let place = m.r32(STATS + STAT_PLACE);
            m.w32(DAMAGE_BILL, 0);
            m.w32(BONUS, (worth / 20) as u32);
            if place == 2 {
                m.w32(BONUS, (worth.wrapping_mul(3) / 100) as u32);
            } else if place == 3 {
                m.w32(BONUS, (worth / 100) as u32);
            }
        }
    }
    let m = &mut c.m;
    let (bill, bonus) = (m.r32(DAMAGE_BILL), m.r32(BONUS));
    m.w32(bill_out, bill);
    m.w32(bonus_out, bonus);
}

pub fn abi_calc_track_finish_damage_bill(c: &mut Cpu) {
    calc_track_finish_damage_bill(c, c.r[5] != 0, c.r[6], c.r[7]);
}

/// `tTournamentManager::UpdateTrackFinishPoints` (FETOURN.CPP, 0x80032f80): knocking out, each
/// racer still in a point (the last eliminated) and its place, one racer fewer; else each
/// finisher's points for its place, the competitors ranked by points (`qsort`), their places.
pub fn update_track_finish_points(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let n = sx(get_num_competitors(m, this)) as i32;
    let t = current(m, this);
    if m.r8(t + KNOCKOUT) != 0 {
        let mut racer = 0;
        for k in 0..n.max(0) as u32 {
            let comp = competitor(this, k);
            if m.r32(comp + ELIMINATED) != 0 {
                continue;
            }
            let stat = STATS.wrapping_add(STATS_SIZE.wrapping_mul(sx(racer)));
            let place = m.r32(stat + STAT_PLACE);
            if place.wrapping_sub(1) < PRIZE_PLACES {
                if (place as i32) < m.r32(this + RACERS) as i32 {
                    let v = m.r16(comp + POINTS).wrapping_add(1);
                    m.w16(comp + POINTS, v);
                } else {
                    m.w32(comp + ELIMINATED, 1);
                }
                let v = m.r8(stat + STAT_PLACE);
                m.w8(comp + PLACE, v);
            }
            racer += 1;
        }
        let v = m.r32(this + RACERS).wrapping_sub(1);
        m.w32(this + RACERS, v);
        return;
    }
    for k in 0..n.max(0) as u32 {
        let stat = STATS + STATS_SIZE * k;
        let place = m.r32(stat + STAT_PLACE).wrapping_sub(1);
        if place < PRIZE_PLACES && m.r32(stat + STAT_STATE) == FINISHED {
            let comp = competitor(this, k);
            let v = m.r16(comp + POINTS).wrapping_add(m.r8(this + FINISH_POINTS + place) as u16);
            m.w16(comp + POINTS, v);
        }
    }
    for k in 0..PRIZE_PLACES {
        m.w8(this + RANKING + k, k as u8);
    }
    call(c, QSORT, &[this + RANKING, n as u32, 1, POINTS_COMPARE]);
    let m = &mut c.m;
    for k in 0..PRIZE_PLACES {
        let who = m.r8(this + RANKING + k) as u32;
        m.w8(competitor(this, who) + PLACE, (k + 1) as u8);
    }
}

pub fn abi_update_track_finish_points(c: &mut Cpu) {
    update_track_finish_points(c, c.r[4]);
}

/// `tTournamentManager::AdvanceToNextTrack` (FETOURN.CPP, 0x80033184): (unless past the last
/// track) the award cleared; the track's money and points; after the last track the tournament's
/// money and the tier's prize; then, the tournament over, what placing in the first three
/// unlocks, what winning gives (its car, its bonus, its best place) and, with every tournament
/// of the tier won and its bonus code not yet used, the tier's car (bought, or its money when the
/// garage is full). The track now (-1: past the last).
pub fn advance_to_next_track(c: &mut Cpu, this: u32) -> u32 {
    let m = &mut c.m;
    let t = current(m, this);
    if ((m.r8(t + 1) as i32) - 1) < m.r32(this + TRACK) as i32 {
        return u32::MAX;
    }
    let award = this + AWARD;
    m.w32(award + AWARD_MONEY, 0);
    m.w32(award + AWARD_TOURN_MONEY, 0);
    m.w16(award + ACTIVATE, 0);
    m.w32(award + AWARD_CAR, 0);
    update_track_finish_money(c, this);
    update_track_finish_points(c, this);
    let m = &mut c.m;
    if m.r32(this + TRACK) == (m.r8(t + 1) as u32).wrapping_sub(1) {
        update_tourn_finish_money(m, this);
        calc_tier_finish_prize(m, this);
    }
    let j = m.r32(this + TRACK).wrapping_add(1);
    m.w32(this + TRACK, j);
    if (j as i32) < m.r8(t + 1) as i32 {
        return j;
    }
    // the tournament over: in the first three, what it unlocks
    m.w32(award + AWARD_CAR_FULL, 0);
    m.w32(award + COMPLETED_FULL, 0);
    let flags = m.r16(t + UNLOCKS);
    if m.r8(this + COMPETITORS + PLACE) < 4 {
        let values = [m.r8(t + 14) as u32, m.r8(t + 15) as u32, m.r8(t + 16) as u32, m.r8(t + 67) as u32];
        for (k, (bit, at)) in [(UNLOCK_TRACK, ACT_TRACK), (UNLOCK_CLASS, ACT_CLASS), (UNLOCK_CAR, ACT_CAR), (UNLOCK_TRACK_CLASS, ACT_TRACK_CLASS)].into_iter().enumerate() {
            if flags & bit != 0 {
                let v = m.r16(award + ACTIVATE) | bit;
                m.w16(award + ACTIVATE, v);
                if at == ACT_TRACK {
                    m.w8(award + at, values[k] as u8);
                } else {
                    m.w32(award + at, values[k]);
                }
            }
        }
        m.w32(award + COMPLETED_TIER, 0);
    }
    if m.r8(this + COMPETITORS + PLACE) >= 2 {
        return m.r32(this + TRACK);
    }
    // won: its car (its money when the garage is full), its bonus, its best place
    if m.r8(t + 7) != 0 {
        let model = m.r8(t + 8) as i32;
        let car = fecars::get_car_from_id(m, CAR_MANAGER, model);
        m.w32(award + AWARD_CAR, 1);
        let model = m.r8(t + 8) as u32;
        m.w32(award + AWARD_CAR_MODEL, model);
        let color = CarInfo(car).default_color(m) as u8;
        m.w8(award + AWARD_CAR_COLOR, color);
        let upgrades = m.r8(t + 9);
        m.w8(award + AWARD_CAR_UPGRADES, upgrades);
        if sx(fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as u32) as i32 >= GARAGE {
            m.w32(award + AWARD_CAR_FULL, 1);
            let price = CarInfo(car).prices(m, 0) as u32;
            add_award(m, this, AWARD_TOURN_MONEY, price);
            let price = CarInfo(car).prices(m, 0) as u32;
            m.w32(award + AWARD_CAR_MONEY, price);
        }
    }
    if flags & UNLOCK_BONUS != 0 {
        let v = m.r16(award + ACTIVATE) | UNLOCK_BONUS;
        m.w16(award + ACTIVATE, v);
        let bonus = m.r8(t + 68) as u32;
        m.w32(award + ACT_BONUS, bonus);
    }
    m.w32(award + COMPLETED_TIER, 0);
    let id = m.r8(t) as i8 as i32 as u32;
    m.w8(this.wrapping_add(id) + BEST_PLACES, 1);
    let tier = m.r32(this + TIER);
    if fecheats::is_cheat_enabled(m, tier.wrapping_add(TIER_BONUS)) {
        return m.r32(this + TRACK);
    }
    // the tier completed: every one of its tournaments won
    m.w32(award + COMPLETED_TIER, 1);
    let tier = m.r32(this + TIER);
    let definition = m.r32(this + DEFINITION);
    let info = definition.wrapping_add(tier.wrapping_mul(TIER_SIZE));
    let n = m.r8(info) as u32;
    for k in 0..n {
        let tt = definition.wrapping_add((m.r8(info + 2) as u32).wrapping_add(k).wrapping_mul(TOURNAMENT_SIZE)) + TOURNAMENTS;
        let id = m.r8(tt) as i8 as i32 as u32;
        if m.r8(this.wrapping_add(id) + BEST_PLACES) as i8 != 1 {
            m.w32(award + COMPLETED_TIER, 0);
        }
    }
    if m.r32(award + COMPLETED_TIER) == 0 {
        return m.r32(this + TRACK);
    }
    // the tier's car (and its bonus code used)
    let mut owned = sx(fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as u32);
    if m.r32(award + AWARD_CAR) != 0 {
        owned = owned.wrapping_add(1);
    }
    let tier = m.r32(this + TIER);
    fecheats::activate_bonus(c, tier.wrapping_add(TIER_BONUS));
    let m = &mut c.m;
    let tier = m.r32(this + TIER);
    m.w32(award + COMPLETED_CAR, tier.wrapping_add(TIER_CAR));
    let model = lh(m, award + COMPLETED_CAR);
    let car = CarInfo(fecars::get_car_from_id(m, CAR_MANAGER, model));
    let model = m.r32(award + COMPLETED_CAR);
    fecars::set_car_available(m, CAR_MANAGER, model, 1);
    let model = m.r32(award + COMPLETED_CAR);
    fecars::set_car_viewable(m, CAR_MANAGER, model, 1);
    if owned as i32 >= GARAGE {
        m.w32(award + COMPLETED_FULL, 1);
        let price = car.prices(m, 0) as u32;
        m.w32(award + COMPLETED_MONEY, price);
        let v = m.r32(this + MONEY).wrapping_add(price);
        m.w32(this + MONEY, v);
    } else {
        let model = lh(m, award + COMPLETED_CAR);
        let color = car.default_color(m) as i32;
        fecars::purchase_car(m, CAR_MANAGER, model, color, 0);
    }
    let v = m.r16(this + TIER).wrapping_add(COMPLETED_TEXTS as u16);
    m.w16(award + COMPLETED_TEXT, v);
    m.r32(this + TRACK)
}

pub fn abi_advance_to_next_track(c: &mut Cpu) {
    c.r[2] = advance_to_next_track(c, c.r[4]);
}

/// `tTournamentManager::GetLastTrackRaced` (FETOURN.CPP, 0x800335c8): the last track's number.
pub fn get_last_track_raced(m: &mut Mem, this: u32) -> u32 {
    let t = current(m, this);
    let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK)).wrapping_sub(1);
    let definition = m.r32(this + DEFINITION);
    m.r8(track(definition, k)) as i8 as i32 as u32
}

pub fn abi_get_last_track_raced(c: &mut Cpu) {
    c.r[2] = get_last_track_raced(&mut c.m, c.r[4]);
}

/// `tTournamentManager::SaveTournament` (FETOURN.CPP, 0x80033638): the money and the best
/// places, for a saved game.
pub fn save_tournament(m: &mut Mem, this: u32, save: u32) {
    let v = m.r32(this + MONEY);
    m.w32(save, v);
    for k in 0..64 {
        let v = m.r8(this + BEST_PLACES + k);
        m.w8(save + SAVE_BEST_PLACES + k, v);
    }
}

pub fn abi_save_tournament(c: &mut Cpu) {
    save_tournament(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::LoadTournament` (FETOURN.CPP, 0x80033680): the money and the best places
/// from a saved game.
pub fn load_tournament(m: &mut Mem, this: u32, save: u32) {
    let v = m.r32(save);
    m.w32(this + MONEY, v);
    for k in 0..64 {
        let v = m.r8(save + SAVE_BEST_PLACES + k);
        m.w8(this + BEST_PLACES + k, v);
    }
}

pub fn abi_load_tournament(c: &mut Cpu) {
    load_tournament(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::GetNumCompetitors` (FETOURN.CPP, 0x800336c8): the tournament's cars.
pub fn get_num_competitors(m: &mut Mem, this: u32) -> u32 {
    let t = current(m, this);
    m.r8(t + CARS) as u32
}

pub fn abi_get_num_competitors(c: &mut Cpu) {
    c.r[2] = get_num_competitors(&mut c.m, c.r[4]);
}

/// `tTournamentManager::UpdateCarLineup` (FETOURN.CPP, 0x80033714): the racers still in, in
/// order: the player's or not, their personalities, their places.
pub fn update_car_lineup(m: &mut Mem, this: u32) {
    let n = sx(get_num_competitors(m, this)) as i32;
    let mut at = 0;
    for k in 0..n.max(0) as u32 {
        let comp = competitor(this, k);
        if m.r32(comp + ELIMINATED) != 0 {
            continue;
        }
        let line = this.wrapping_add(sx(at).wrapping_mul(LINEUP_SIZE)) + LINEUP;
        let player = (m.r8(comp + IS_PLAYER) as i8 != 0) as u32;
        m.w32(line, player);
        let personality = m.r32(comp + PERSONALITY);
        m.w32(line + 4, personality);
        let place = m.r8(comp + PLACE);
        m.w8(line + 8, place);
        at += 1;
    }
}

pub fn abi_update_car_lineup(c: &mut Cpu) {
    update_car_lineup(&mut c.m, c.r[4]);
}

/// `tTournamentManager::GetTrackFinishPrize` (FETOURN.CPP, 0x800337c4): the track's prize for
/// `place` (none past sixth), and against cars of the player's class its car's worth (with its
/// upgrades) by the class's multiplier for the place.
pub fn get_track_finish_prize(c: &mut Cpu, this: u32, place: u32) -> u32 {
    if (place & 0xFFFF) >= PRIZE_PLACES {
        return 0;
    }
    let t = current(&mut c.m, this);
    let mut bonus = 0;
    if c.m.r8(t + OPPONENT_CLASS) == PLAYERS_CLASS {
        let k = c.m.r8(GARAGE_CAR) as u32;
        let (class, worth) = with_buffer(c, CarInfo::SIZE, |c, car| {
            fecars::get_garage_car(c, CAR_MANAGER, k, car, 0);
            let car = CarInfo(car);
            let m = &mut c.m;
            (car.car_class(m) as u8 as u32, worth(m, car, 3))
        });
        if class < 7 {
            let multiplier = c.m.r32(MULTIPLIERS + class * 24 + (sx(place) << 2));
            bonus = call(c, FIXEDMULT, &[multiplier, worth]);
        }
    }
    let m = &mut c.m;
    let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK));
    let definition = m.r32(this + DEFINITION);
    let prize = m.r32(track(definition, k).wrapping_add(sx(place) << 2) + TRACK_PRIZES);
    bonus.wrapping_add(prize)
}

pub fn abi_get_track_finish_prize(c: &mut Cpu) {
    c.r[2] = get_track_finish_prize(c, c.r[4], c.r[5]);
}

/// `tTournamentManager::GetTournamentFinishPrize` (FETOURN.CPP, 0x8003399c): the tournament's
/// prize for `place` (none past sixth).
pub fn get_tournament_finish_prize(m: &mut Mem, this: u32, place: u32) -> u32 {
    if (place & 0xFFFF) >= PRIZE_PLACES {
        return 0;
    }
    let t = current(m, this);
    m.r32(t.wrapping_add(sx(place) << 2) + TOURNAMENT_PRIZES)
}

pub fn abi_get_tournament_finish_prize(c: &mut Cpu) {
    c.r[2] = get_tournament_finish_prize(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::GetAwardInformation` (FETOURN.CPP, 0x80033a10): the award copied to
/// `out`.
pub fn get_award_information(m: &mut Mem, this: u32, out: u32) {
    for k in (0..AWARD_SIZE).step_by(4) {
        let v = m.r32(this + AWARD + k);
        m.w32(out + k, v);
    }
}

pub fn abi_get_award_information(c: &mut Cpu) {
    get_award_information(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::UpdateAwardInformation` (FETOURN.CPP, 0x80033a50): the money (less the
/// bill, with the bonus and the prizes); what the award unlocks (bonus codes, a car, a track, a
/// track class, a bonus); a car won (not for a full garage) bought, with its upgrades.
pub fn update_award_information(c: &mut Cpu, this: u32) {
    let (bill, bonus) = with_buffer(c, 8, |c, out| {
        calc_track_finish_damage_bill(c, true, out, out + 4);
        (c.m.r32(out), c.m.r32(out + 4))
    });
    let m = &mut c.m;
    let award = this + AWARD;
    let money = m.r32(this + MONEY).wrapping_sub(bill).wrapping_add(bonus);
    let money = money.wrapping_add(m.r32(award + AWARD_MONEY)).wrapping_add(m.r32(award + AWARD_TOURN_MONEY));
    m.w32(this + MONEY, money);
    let flags = |c: &mut Cpu| c.m.r16(award + ACTIVATE);
    if flags(c) & UNLOCK_CLASS != 0 {
        let v = c.m.r32(award + ACT_CLASS);
        fecheats::activate_bonus(c, v);
    }
    if flags(c) & UNLOCK_CAR != 0 {
        let v = c.m.r32(award + ACT_CAR);
        fecars::set_car_available(&mut c.m, CAR_MANAGER, v, 1);
    }
    if flags(c) & UNLOCK_TRACK != 0 {
        let v = c.m.r8(award + ACT_TRACK) as u32;
        fetracks::set_track_available(&mut c.m, TRACK_MANAGER, v, 1);
    }
    if flags(c) & UNLOCK_TRACK_CLASS != 0 {
        let v = c.m.r32(award + ACT_TRACK_CLASS);
        fetracks::set_class_available(&mut c.m, TRACK_MANAGER, v, 1);
    }
    if flags(c) & UNLOCK_BONUS != 0 {
        let v = c.m.r32(award + ACT_BONUS);
        fecheats::activate_bonus(c, v);
    }
    let m = &mut c.m;
    if m.r32(award + AWARD_CAR) != 0 && m.r32(award + AWARD_CAR_FULL) == 0 {
        let model = lh(m, award + AWARD_CAR_MODEL);
        let car = fecars::get_car_from_id(m, CAR_MANAGER, model);
        // (the colour by its place in the car's colour order)
        let k = m.r8(award + AWARD_CAR_COLOR) as u32;
        let color = m.r8(car.wrapping_add(k) + COLOR_ORDER) as i8 as i32;
        let model = lh(m, award + AWARD_CAR_MODEL);
        fecars::purchase_car(m, CAR_MANAGER, model, color, 0);
        let (k, upgrades) = (m.r8(GARAGE_CAR) as i32, m.r8(award + AWARD_CAR_UPGRADES) as i32);
        fecars::purchase_upgrade(m, CAR_MANAGER, k, upgrades, 0);
    }
}

pub fn abi_update_award_information(c: &mut Cpu) {
    update_award_information(c, c.r[4]);
}

/// `tTournamentManager::TournPointTotal` (FETOURN.CPP, 0x80033bc8): competitor `*k`'s points.
pub fn tourn_point_total(m: &mut Mem, this: u32, k: u32) -> u32 {
    let who = lh(m, k) as u32;
    m.r16(competitor(this, who) + POINTS) as u32
}

pub fn abi_tourn_point_total(c: &mut Cpu) {
    c.r[2] = tourn_point_total(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::PlayerRanking` (FETOURN.CPP, 0x80033be4): the competitor in `place` (0:
/// none).
pub fn player_ranking(m: &mut Mem, this: u32, place: u32) -> u32 {
    let n = sx(get_num_competitors(m, this)) as i32;
    let place = sx(place);
    (0..n.max(0) as u32).find(|&k| m.r8(competitor(this, k) + PLACE) as u32 == place).unwrap_or(0)
}

pub fn abi_player_ranking(c: &mut Cpu) {
    c.r[2] = player_ranking(&mut c.m, c.r[4], c.r[5]);
}

/// `tTournamentManager::CalcTierFinishPrize` (FETOURN.CPP, 0x80033c74): the tier's prize 7,
/// unchanged.
pub fn calc_tier_finish_prize(m: &mut Mem, this: u32) {
    let tier = m.r32(this + TIER);
    m.w16(this.wrapping_add(tier << 1) + TIER_PRIZE, 7);
    let tier = m.r32(this + TIER);
    m.w16(this.wrapping_add(tier << 1) + TIER_PRIZE_CHANGE, 0);
}

pub fn abi_calc_tier_finish_prize(c: &mut Cpu) {
    calc_tier_finish_prize(&mut c.m, c.r[4]);
}

/// `tTournamentManager::GetTrophyName` (FETOURN.CPP, 0x80033ca0): tournament `t`'s trophy's
/// picture's file name into `out`: `TR` + its size (S, M, L by `size`) + its number + its metal
/// (W, G, S, B by `place`; -1: the best place won, W when not placed).
pub fn get_trophy_name(c: &mut Cpu, t: u32, size: u32, out: u32, place: u32) {
    let m = &mut c.m;
    let id = m.r8(t) as i8 as i32 as u32;
    let best = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32 as u32;
    let metal = if place == u32::MAX { sx(if best.wrapping_sub(1) < 3 { best } else { 0 }) } else { place };
    let trophy = m.r8(t + TROPHY) as u32;
    // (past the tables, the original reads its stack: no letter here; the game always has one)
    let metal = if metal < 4 { m.r8(PLACES + metal) as u32 } else { 0 };
    let size = if size < 3 { m.r8(SIZES + size) as u32 } else { 0 };
    call(c, SPRINTF, &[out, TROPHY_FORMAT, size, trophy, metal]);
}

pub fn abi_get_trophy_name(c: &mut Cpu) {
    let place = crate::abi::arg(c, 4);
    get_trophy_name(c, c.r[5], c.r[6], c.r[7], place);
}

/// `tTournamentManager::ValidCar` (FETOURN.CPP, 0x80033d60): a car the tournament allows: of
/// its class (any class with the first tier's cheat on), and the car and upgrades it requires.
pub fn valid_car(m: &mut Mem, this: u32, car: u32) -> u32 {
    let t = current(m, this);
    let class = m.r8(t + OPPONENT_CLASS);
    let mut ok = 1;
    if class != PLAYERS_CLASS {
        ok = (m.r8(car + 2) == class) as u32;
        if fecheats::is_cheat_enabled(m, TIER_BONUS) && m.r32(this + TIER) == 0 {
            ok = 1;
        }
    }
    if m.r16(t + REQUIRES) & REQUIRES_CAR == 0 {
        return ok;
    }
    if m.r8(car) as i8 as i32 != m.r8(t + 18) as i32 {
        return 0;
    }
    // the upgrades it requires: any, none, some, these
    let owned = CarInfo(car).upgrades(m);
    match m.r8(t + 19) {
        1 => ok,
        0 if owned != 0 => 0,
        2 if owned == 0 => 0,
        3 if owned & m.r8(t + 20) != m.r8(t + 20) => 0,
        _ => ok,
    }
}

pub fn abi_valid_car(c: &mut Cpu) {
    c.r[2] = valid_car(&mut c.m, c.r[4], c.r[5]);
}

/// `tListIteratorTournament::tListIteratorTournament` (FETOURN.CPP, 0x80033ed8): an iterator
/// over `values` of `manager`'s tournaments.
pub fn list_iterator_tournament(m: &mut Mem, this: u32, values: u32, manager: u32) -> u32 {
    femenu::list_iterator(m, ListIterator(this), 0, values);
    m.w32(this + ITERATOR_VPTR, ITERATOR_VT);
    m.w32(this + ITERATOR_MANAGER, manager);
    this
}

pub fn abi_list_iterator_tournament(c: &mut Cpu) {
    c.r[2] = list_iterator_tournament(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tListIteratorTournament::~tListIteratorTournament` (FETOURN.CPP, 0x80033f28).
pub fn list_iterator_tournament_dtor(c: &mut Cpu, this: u32, flags: u32) {
    c.m.w32(this + ITERATOR_VPTR, ITERATOR_VT);
    femenu::list_iterator_dtor(c, this, flags);
}

pub fn abi_list_iterator_tournament_dtor(c: &mut Cpu) {
    list_iterator_tournament_dtor(c, c.r[4], c.r[5]);
}

/// `tListIteratorTournament::Value` (FETOURN.CPP, 0x80033f50).
pub fn value(m: &mut Mem, this: u32) -> u32 {
    let values = m.r32(this + 4);
    m.r8(values) as u32
}

pub fn abi_value(c: &mut Cpu) {
    c.r[2] = value(&mut c.m, c.r[4]);
}

/// The front end's tier's tournament `k` of the iterator's manager.
fn tier_tournament(m: &mut Mem, this: u32, k: u32) -> u32 {
    let tier = m.r8(FE_TIER) as u32;
    let manager = m.r32(this + ITERATOR_MANAGER);
    let definition = m.r32(manager + DEFINITION);
    tournament_of(m, definition, tier, k)
}

/// `tListIteratorTournament::TextValue` (FETOURN.CPP, 0x80033f64): the tournament's name.
pub fn text_value(m: &mut Mem, this: u32) -> u32 {
    let k = value(m, this);
    let t = tier_tournament(m, this, k);
    (m.r8(t) as i8 as i32 as u32).wrapping_add(NAMES)
}

pub fn abi_text_value(c: &mut Cpu) {
    c.r[2] = text_value(&mut c.m, c.r[4]);
}

/// The tier's tournaments' count (its description's first byte) for the iterator.
fn tier_info(m: &mut Mem, this: u32) -> u32 {
    let tier = m.r8(FE_TIER) as u32;
    let manager = m.r32(this + ITERATOR_MANAGER);
    m.r32(manager + DEFINITION).wrapping_add(tier * TIER_SIZE)
}

/// `tListIteratorTournament::Increment` (FETOURN.CPP, 0x80033fbc): the next tournament of the
/// tier (round to the first) until one earned.
pub fn increment(m: &mut Mem, this: u32) {
    let info = tier_info(m, this);
    loop {
        let at = m.r32(this + 4);
        let v = m.r8(at).wrapping_add(1);
        m.w8(at, v);
        let at = m.r32(this + 4);
        if m.r8(at) >= m.r8(info) {
            m.w8(at, 0);
        }
        let k = value(m, this);
        if valid_tournament(m, this, k) {
            return;
        }
    }
}

pub fn abi_increment(c: &mut Cpu) {
    increment(&mut c.m, c.r[4]);
}

/// `tListIteratorTournament::Decrement` (FETOURN.CPP, 0x8003405c): the tournament before (round
/// to the last) until one earned.
pub fn decrement(m: &mut Mem, this: u32) {
    let info = tier_info(m, this);
    loop {
        let at = m.r32(this + 4);
        let v = m.r8(at);
        let v = if v == 0 { m.r8(info).wrapping_sub(1) } else { v - 1 };
        m.w8(at, v);
        let k = value(m, this);
        if valid_tournament(m, this, k) {
            return;
        }
    }
}

pub fn abi_decrement(c: &mut Cpu) {
    decrement(&mut c.m, c.r[4]);
}

/// `tListIteratorTournament::ValidTournament` (FETOURN.CPP, 0x800340ec): tournament `k` earned:
/// the tournament it requires placed in (in the first three) or won.
pub fn valid_tournament(m: &mut Mem, this: u32, k: u32) -> bool {
    let t = tier_tournament(m, this, k & 0xFF);
    let manager = m.r32(this + ITERATOR_MANAGER);
    let flags = m.r16(t + REQUIRES);
    let best = |m: &mut Mem| {
        let id = m.r8(t + 17) as u32;
        m.r8(manager.wrapping_add(id) + BEST_PLACES) as i8 as i32
    };
    let mut ok = true;
    if flags & REQUIRES_PLACED != 0 {
        ok = best(m) < 4;
    }
    if flags & REQUIRES_WON != 0 && best(m) >= 2 {
        ok = false;
    }
    ok
}

pub fn abi_valid_tournament(c: &mut Cpu) {
    c.r[2] = valid_tournament(&mut c.m, c.r[4], c.r[5]) as u32;
}

/// For callers not rewritten yet: the functions with the C convention's arguments.
pub(crate) fn get_track_list_entry(c: &mut Cpu) {
    abi_get_track_list(c);
}

pub(crate) fn get_track_to_race_entry(c: &mut Cpu) {
    abi_get_track_to_race(c);
}

pub(crate) fn calc_track_finish_damage_bill_entry(c: &mut Cpu) {
    abi_calc_track_finish_damage_bill(c);
}

pub(crate) fn get_award_information_entry(c: &mut Cpu) {
    abi_get_award_information(c);
}

pub(crate) fn tourn_point_total_entry(c: &mut Cpu) {
    abi_tourn_point_total(c);
}

pub(crate) fn player_ranking_entry(c: &mut Cpu) {
    abi_player_ranking(c);
}

pub(crate) fn get_trophy_name_entry(c: &mut Cpu) {
    abi_get_trophy_name(c);
}

pub(crate) fn valid_car_entry(c: &mut Cpu) {
    abi_valid_car(c);
}

pub(crate) fn text_value_entry(c: &mut Cpu) {
    abi_text_value(c);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's tournament manager (its definition loaded); an iterator and its values; a
    /// saved game, a car, outputs.
    const TM: u32 = TOURNAMENT_MANAGER;
    const ITERATOR: u32 = 0x8014_7100;
    const VALUES: u32 = 0x8014_7140;
    const SAVE: u32 = 0x8014_7200;
    const CAR: u32 = 0x8014_7300;
    const OUT: u32 = 0x8014_7000;
    /// The cheats on (FECHEATS's).
    const CHEATS_ON: u32 = 0x8013_E550;

    /// FEMENU's structures, then the manager: the tier and a tournament of it, the track about
    /// its ends, the racers left, the money; the competitors (personalities, the player's,
    /// eliminated, points about ties, places a shuffled order); the best places, the points per
    /// place, the ranking, the award, the tiers' prizes; the race's results (finished or not,
    /// places, damage about whole points); the car in the garage; the tiers' bonus codes on or
    /// not; the front end's tier; an iterator over a value; a saved game; a car.
    pub(crate) fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        // the definition's tournaments: now and then of the player's class, a name past 127;
        // what they require (a tournament placed in or won, a car with its upgrades; the first
        // nothing, so the list always has one); a car awarded or not
        let definition = m.r32(TM + DEFINITION);
        for tier in 0..3 {
            let (n, first) = (m.r8(definition + tier * TIER_SIZE) as u32, m.r8(definition + tier * TIER_SIZE + 2) as u32);
            for k in 0..n {
                let t = definition + TOURNAMENTS + (first + k) * TOURNAMENT_SIZE;
                if pick(rng, 3) == 0 {
                    m.w8(t + 3, 10);
                }
                if pick(rng, 16) == 0 {
                    let id = m.r8(t) | 0x80;
                    m.w8(t, id);
                }
                m.w16(t + 12, if k == 0 { 0 } else { pick(rng, 8) as u16 });
                m.w8(t + 17, pick(rng, 64) as u8);
                m.w8(t + 18, pick(rng, 30) as u8);
                m.w8(t + 19, pick(rng, 4) as u8);
                m.w8(t + 20, pick(rng, 8) as u8);
                m.w8(t + 7, pick(rng, 2) as u8);
            }
        }
        let tier = pick(rng, 3);
        m.w32(TM + TIER, tier);
        let definition = m.r32(TM + DEFINITION);
        let n = m.r8(definition + tier * TIER_SIZE) as u32;
        m.w32(TM + TOURNAMENT, pick(rng, n.max(1)));
        let t = current(m, TM);
        let tracks = m.r8(t + 1) as u32;
        m.w32(TM + TRACK, [0, tracks.wrapping_sub(1), tracks, pick(rng, tracks + 1)][pick(rng, 4) as usize]);
        m.w32(TM + RACERS, pick(rng, 7));
        m.w32(TM + MONEY, pick(rng, 200_000));
        let mut places: Vec<u32> = (1..7).collect();
        for k in (1..places.len()).rev() {
            places.swap(k, pick(rng, k as u32 + 1) as usize);
        }
        for k in 0..6u32 {
            let comp = competitor(TM, k);
            m.w32(comp + PERSONALITY, pick(rng, 10));
            m.w8(comp + IS_PLAYER, (k == 0 || pick(rng, 8) == 0) as u8);
            m.w32(comp + ELIMINATED, (pick(rng, 4) == 0) as u32);
            m.w16(comp + POINTS, [0, 5, 10, pick(rng, 30)][pick(rng, 4) as usize] as u16);
            m.w8(comp + PLACE, places[k as usize] as u8);
            m.w8(TM + RANKING + k, places[k as usize] as u8 - 1);
            m.w8(TM + FINISH_POINTS + k, (1 + pick(rng, 10)) as u8);
        }
        // the best places: all won, won or none, or any
        let best = pick(rng, 3);
        for k in 0..64 {
            let v = match best {
                0 => 1,
                1 => pick(rng, 2),
                _ => [1, 2, 3, 4, 7, pick(rng, 8)][pick(rng, 6) as usize],
            };
            m.w8(TM + BEST_PLACES + k, v as u8);
        }
        // each track's settings
        for at in [DIRECTIONS, MIRRORS, TIMES_OF_DAY, WEATHERS] {
            for k in 0..16 {
                m.w8(TM + at + k, pick(rng, 4) as u8);
            }
        }
        for k in (0..68).step_by(4) {
            m.w32(TM + AWARD + k, [0, 1, pick(rng, 5000)][pick(rng, 3) as usize]);
        }
        for k in 0..3 {
            m.w16(TM + TIER_PRIZE + 2 * k, pick(rng, 8) as u16);
            m.w16(TM + TIER_PRIZE_CHANGE + 2 * k, pick(rng, 3) as u16);
        }
        for k in 0..6 {
            let stat = STATS + STATS_SIZE * k;
            m.w32(stat + 112, [0, 0, 0x8000, 0x1_0000, 0x3_8000, (-0x8000i32) as u32, (-0x2_0000i32) as u32][pick(rng, 7) as usize]);
            m.w32(stat + 128, [0, 1, 2, 2][pick(rng, 4) as usize]);
            m.w32(stat + 132, [0, 1, 2, 3, 4, 5, 6, 7][pick(rng, 8) as usize]);
        }
        // the garage: empty, all but full, full or any, its cars any model with any upgrades;
        // the garage car one of them or a model; the tracks available or not
        let cars = m.r32(CAR_MANAGER.0);
        let owned = [0, 31, 32, pick(rng, 33)][pick(rng, 4) as usize];
        for k in 0..32 {
            let e = CAR_MANAGER.0 + 8 + 4 * k;
            m.w8(e, if k < owned { pick(rng, cars) as u8 } else { 0xFF });
            m.w8(e + 1, pick(rng, 8) as u8);
        }
        m.w8(GARAGE_CAR, [pick(rng, 3), cars + pick(rng, owned.max(1))][pick(rng, 2) as usize] as u8);
        for k in 0..16 {
            m.w32(TRACK_MANAGER + 8 + 4 * k, pick(rng, 2));
        }
        m.w32(CHEATS_ON, [0, 1 << 13, 1 << 14, 1 << 15, rng.next() as u32][pick(rng, 5) as usize]);
        m.w8(FE_TIER, pick(rng, 3) as u8);
        m.w32(ITERATOR + 4, VALUES);
        m.w32(ITERATOR + 12, ITERATOR_VT);
        m.w32(ITERATOR + 16, TM);
        let fe_tier = m.r8(FE_TIER) as u32;
        let n = m.r8(definition + fe_tier * TIER_SIZE) as u32;
        m.w8(VALUES, pick(rng, n.max(1)) as u8);
        for k in 0..176 {
            m.w8(SAVE + k, rng.next() as u8);
        }
        for k in 0..200 {
            m.w8(CAR + k, 0);
        }
        let cur = current(m, TM);
        let required = m.r8(cur + 18) as u32;
        m.w8(CAR, [pick(rng, 30), required][pick(rng, 2) as usize] as u8);
        m.w8(CAR + 2, pick(rng, 12) as u8);
        m.w8(CAR + 196, [0, 1, 3, 7, pick(rng, 256)][pick(rng, 5) as usize] as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tTournamentManager::Initialize", 0x8003_2378, nfs4rc::f_80032378, |_, _| vec![TM]),
            ("tTournamentManager::ReleaseDescription", 0x8003_2698, nfs4rc::f_80032698, |c, r| {
                // (a definition of the heap's now and then, to free: the real one stays)
                let d = crate::abi::call(c, RESERVE, &[MEMORY_NAME, 64, 0]);
                c.m.w32(OUT, d);
                vec![[OUT - DEFINITION, OUT - DEFINITION, OUT + 8 - DEFINITION][pick(r, 3) as usize]]
            }),
            ("tTournamentManager::UpdateTrackList", 0x8003_26d4, nfs4rc::f_800326d4, |_, r| vec![TM, pick(r, 3), pick(r, 6)]),
            ("tTournamentManager::GetTrackList", 0x8003_27a4, nfs4rc::f_800327a4, |_, r| vec![TM, pick(r, 3), pick(r, 6)]),
            ("tTournamentManager::GetTrackToRace", 0x8003_27dc, nfs4rc::f_800327dc, |_, _| vec![TM, OUT]),
            ("tTournamentManager::StartNewTournament", 0x8003_28d4, nfs4rc::f_800328d4, |_, r| vec![TM, pick(r, 3), pick(r, 6)]),
            ("tTournamentManager::IsTournamentFinished", 0x8003_2b0c, nfs4rc::f_80032b0c, |_, _| vec![TM]),
            ("tournPointsCompare", 0x8003_2ba0, nfs4rc::f_80032ba0, |c, r| {
                c.m.w8(OUT, pick(r, 6) as u8);
                c.m.w8(OUT + 1, pick(r, 6) as u8);
                vec![OUT, OUT + 1]
            }),
            ("tTournamentManager::UpdateTournFinishMoney", 0x8003_2c2c, nfs4rc::f_80032c2c, |_, _| vec![TM]),
            ("tTournamentManager::UpdateTrackFinishMoney", 0x8003_2d5c, nfs4rc::f_80032d5c, |_, _| vec![TM]),
            ("tTournamentManager::CalcTrackFinishDamageBill", 0x8003_2dbc, nfs4rc::f_80032dbc, |_, r| vec![TM, pick(r, 2), OUT, OUT + 4]),
            ("tTournamentManager::UpdateTrackFinishPoints", 0x8003_2f80, nfs4rc::f_80032f80, |_, _| vec![TM]),
            ("tTournamentManager::AdvanceToNextTrack", 0x8003_3184, nfs4rc::f_80033184, |_, _| vec![TM]),
            ("tTournamentManager::GetLastTrackRaced", 0x8003_35c8, nfs4rc::f_800335c8, |_, _| vec![TM]),
            ("tTournamentManager::SaveTournament", 0x8003_3638, nfs4rc::f_80033638, |_, _| vec![TM, SAVE]),
            ("tTournamentManager::LoadTournament", 0x8003_3680, nfs4rc::f_80033680, |_, _| vec![TM, SAVE]),
            ("tTournamentManager::GetNumCompetitors", 0x8003_36c8, nfs4rc::f_800336c8, |_, _| vec![TM]),
            ("tTournamentManager::UpdateCarLineup", 0x8003_3714, nfs4rc::f_80033714, |_, _| vec![TM]),
            ("tTournamentManager::GetTrackFinishPrize", 0x8003_37c4, nfs4rc::f_800337c4, |_, r| vec![TM, [0, 1, 5, 6, 0xFFFF, pick(r, 6)][pick(r, 6) as usize]]),
            ("tTournamentManager::GetTournamentFinishPrize", 0x8003_399c, nfs4rc::f_8003399c, |_, r| vec![TM, [0, 1, 5, 6, 0xFFFF, pick(r, 6)][pick(r, 6) as usize]]),
            ("tTournamentManager::GetAwardInformation", 0x8003_3a10, nfs4rc::f_80033a10, |_, _| vec![TM, OUT]),
            ("tTournamentManager::UpdateAwardInformation", 0x8003_3a50, nfs4rc::f_80033a50, |_, _| vec![TM]),
            ("tTournamentManager::TournPointTotal", 0x8003_3bc8, nfs4rc::f_80033bc8, |c, r| {
                c.m.w16(OUT, pick(r, 6) as u16);
                vec![TM, OUT]
            }),
            ("tTournamentManager::PlayerRanking", 0x8003_3be4, nfs4rc::f_80033be4, |_, r| vec![TM, pick(r, 8)]),
            ("tTournamentManager::CalcTierFinishPrize", 0x8003_3c74, nfs4rc::f_80033c74, |_, _| vec![TM]),
            ("tTournamentManager::GetTrophyName", 0x8003_3ca0, nfs4rc::f_80033ca0, |c, r| {
                let t = current(&mut c.m, TM);
                vec![TM, t, pick(r, 3), OUT, [u32::MAX, u32::MAX, 0, 1, 2, 3][pick(r, 6) as usize]]
            }),
            ("tTournamentManager::ValidCar", 0x8003_3d60, nfs4rc::f_80033d60, |_, _| vec![TM, CAR]),
            ("tListIteratorTournament", 0x8003_3ed8, nfs4rc::f_80033ed8, |_, _| vec![OUT + 0x180, VALUES, TM]),
            ("~tListIteratorTournament", 0x8003_3f28, nfs4rc::f_80033f28, |_, r| vec![ITERATOR, 2 * pick(r, 2)]),
            ("tListIteratorTournament::Value", 0x8003_3f50, nfs4rc::f_80033f50, |_, _| vec![ITERATOR, 0]),
            ("tListIteratorTournament::TextValue", 0x8003_3f64, nfs4rc::f_80033f64, |_, _| vec![ITERATOR, 0]),
            ("tListIteratorTournament::Increment", 0x8003_3fbc, nfs4rc::f_80033fbc, |_, _| vec![ITERATOR, 0]),
            ("tListIteratorTournament::Decrement", 0x8003_405c, nfs4rc::f_8003405c, |_, _| vec![ITERATOR, 0]),
            ("tListIteratorTournament::ValidTournament", 0x8003_40ec, nfs4rc::f_800340ec, |_, r| vec![ITERATOR, pick(r, 8)]),
        ]
    }

    #[test]
    fn fetourn_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7044 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7045 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
