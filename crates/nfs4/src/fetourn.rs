//! FETOURN.CPP: the tournaments (`tTournamentManager`, `tournamentManager`): their definition
//! read from `tourn.trn` (tiers, tournaments, tracks); a tournament started, its tracks' settings
//! (random where the definition leaves them open), the racers' points and places after each track
//! (knock-outs eliminate the last), the money won, the damage billed and the bonus, the prizes,
//! what winning unlocks; the tournament list's iterator (`tListIteratorTournament`), which skips
//! tournaments not yet earned.

use crate::abi::{self, Saved};
use crate::femenuextended::SPRINTF;
use crate::globals::{FRONT_END, PATHS, TOURNAMENT_MANAGER, TRACK_MANAGER};
use crate::{fecars, fecheats, femenu, fetracks};
use ps1rt::{Cpu, Mem};

// A tournament manager: +0 how many tiers, +4 the tier, +8 the tournament, +12 the track (from
// 0), +16 the racers left, +20 the money, +24 the definition, +36 and +44 each tier's finish prize
// and its change (`short`s), +52 the tournaments listed, +182 the tracks listed (texts, ended by
// 0), +216, +232, +248, +264 each track's direction, mirroring, time of day and weather, +280 the
// competitors (16 bytes each: +0 their personality, +5 the player's, +8 eliminated, +12 points,
// +14 place), +376 the car lineup (20 bytes each: +0 the player's, +4 personality, +8 place),
// +496 each tournament's best place (by id), +560 the last best place, +561 the points per place,
// +567 the ranking, +576 the award (`tAwardInformation`, 68 bytes).
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
const BEST_PLACES: u32 = 496;
const LAST_BEST: u32 = 560;
const FINISH_POINTS: u32 = 561;
const RANKING: u32 = 567;
const AWARD: u32 = 576;

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

// The definition: the tiers (12 bytes each: +0 how many tournaments, +2 the first), the
// tournaments from +36 (84 bytes each: +0 id, +1 tracks, +2 the first track, +3 the opponents'
// class (10: the player's), +5 knock-out, +6 cars, +7 a car awarded, +8 its model, +9 its
// upgrades, +10 what it unlocks, +12 what it requires (1 a tournament placed, 2 won, 4 a car),
// +14..+16 the track, class and car unlocked, +17 the tournament required, +18 the car, +19 its
// upgrades (0 none, 1 any, 2 some, 3 these), +20 these, +22 the trophy, +24 the prizes (6),
// +48 the entry fee, +67 the track class unlocked, +68 the bonus), the tracks from +5412 (40
// bytes each: +0 number, +1 direction, +2 mirrored, +3 time of day, +4 weather (2 and up:
// random), +8 the prizes (6)).
const TIER_SIZE: u32 = 12;
const TOURNAMENTS: u32 = 36;
const TOURNAMENT_SIZE: u32 = 84;
const TRACKS: u32 = 5412;
const TRACK_SIZE: u32 = 40;

/// The race's results (`Stats`, 160 bytes a car: +112 damage (16.16), +128 finished (2), +132
/// place).
const STATS: u32 = 0x8011_0E08;
const STATS_SIZE: u32 = 160;
/// The car manager; the player's car in the garage (`frontEnd + 0x123`); the front end's tier
/// (`+0x122`).
const CAR_MANAGER: u32 = 0x8011_552C;
const GARAGE_CAR: u32 = FRONT_END + 0x123;
const FE_TIER: u32 = FRONT_END + 0x122;
/// The damage bill and the bonus worked out (`CalcTrackFinishDamageBill`'s).
const DAMAGE_BILL: u32 = 0x8005_1EE8;
const BONUS: u32 = 0x8005_1EEC;
/// "%s%s", "tourn.trn", "Tourney"; the front end's directory; the prize multipliers by class
/// (24 bytes each, 7 of them); "SML", "WGSB", "TR%c%02d%c"; the iterator's vtable.
const PATH_FORMAT: u32 = 0x8001_153C;
const FILE: u32 = 0x8001_1544;
const MEMORY_NAME: u32 = 0x8001_1550;
const FRONT_END_PATH: u32 = PATHS + 0x94;
const MULTIPLIERS: u32 = 0x8001_1558;
const SIZES: u32 = 0x8001_1600;
const PLACES: u32 = 0x8001_1604;
const TROPHY_FORMAT: u32 = 0x8001_1608;
const ITERATOR_VT: u32 = 0x8001_1618;
/// The texts: the tournaments' names (834 on, by id), a tier's completion (986 on).
const NAMES: u32 = 834;
const COMPLETED_TEXTS: u32 = 986;
/// The bonus code of each tier (13 on), the car it gives (19 on).
const TIER_BONUS: u32 = 13;
const TIER_CAR: u32 = 19;
/// The garage's size.
const GARAGE: i32 = 32;

/// `loadfileadr`, `reservememadr`, `blockmove`, `purgememadr`, `rand`, `qsort`, `fixedmult`.
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e63d4;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const RAND: fn(&mut Cpu) = nfs4rc::f_800eb430;
const QSORT: fn(&mut Cpu) = nfs4rc::f_800e6978;
const FIXEDMULT: fn(&mut Cpu) = nfs4rc::f_800e4f14;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
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
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    m.w32(this + MONEY, 20000);
    m.w32(this + TRACK, 0);
    m.w8(this + TIERS, 0);
    m.w32(this + DEFINITION, 0);
    m.w16(this + TOURNAMENT_LIST, 0);
    m.w16(this + TRACK_LIST, 0);
    for k in 0..6 {
        m.w8(this + RANKING + k, 0);
    }
    for k in 0..64 {
        m.w8(this + BEST_PLACES + k, 7);
    }
    for k in 0..3 {
        m.w16(this + TIER_PRIZE + 2 * k, 6);
        m.w16(this + TIER_PRIZE_CHANGE + 2 * k, 0);
    }
    let last = m.r32(CAR_MANAGER).wrapping_sub(1) as u8;
    m.w8(GARAGE_CAR, last);
    m.w8(GARAGE_CAR + 1, last);
}

/// `tTournamentManager::LoadDescription` (FETOURN.CPP, 0x8003244c): `tourn.trn` read (the old
/// definition freed first): the points per place, the tiers, each tier's tournaments, each
/// tournament's tracks copied into a new definition; the file freed.
pub fn abi_load_description(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 144, |c, sp| {
        let mut r = Saved::of(c);
        r.0[6] = this;
        let name = sp + 16;
        let path = c.m.r32(FRONT_END_PATH);
        r.call_leaving(c, SPRINTF, &[name, PATH_FORMAT, path, FILE], 0x8003_249c);
        port(c, &r, 0x8003_2698, 0x8003_24a4, &[this], abi_release_description);
        let data = r.call_leaving(c, LOAD_FILE, &[name, 16], 0x8003_24b0);
        c.m.w32(sp + 96, data);
        r.call_leaving(c, BLOCKMOVE, &[data, this + FINISH_POINTS, 6], 0x8003_24c4);
        let tiers = c.m.r8(data + 6);
        c.m.w8(this + TIERS, tiers);
        let definition = r.call_leaving(c, RESERVE, &[MEMORY_NAME, 10532, 0], 0x8003_24e4);
        r.0[8] = 0;
        c.m.w32(this + DEFINITION, definition);
        r.0[2] = data + 7;
        if c.m.r8(this + TIERS) != 0 {
            loop {
                let t = sx(r.0[8]);
                r.0[1] = t;
                r.0[0] = t.wrapping_mul(TIER_SIZE);
                let definition = c.m.r32(this + DEFINITION);
                r.call_leaving(c, BLOCKMOVE, &[r.0[2], definition.wrapping_add(r.0[0]), TIER_SIZE], 0x8003_2524);
                let definition = c.m.r32(this + DEFINITION);
                r.0[0] = definition.wrapping_add(r.0[0]);
                let tier = r.0[0];
                r.0[4] = c.m.r8(tier + 2) as u32;
                let n = c.m.r8(tier) as u32;
                r.0[2] = r.0[2].wrapping_add(TIER_SIZE);
                if (r.0[4] as i32) < r.0[4].wrapping_add(n) as i32 {
                    r.0[7] = r.0[4];
                    r.0[5] = t;
                    loop {
                        let k = sx(r.0[4]);
                        r.0[1] = k.wrapping_mul(TOURNAMENT_SIZE);
                        let definition = c.m.r32(this + DEFINITION);
                        r.call_leaving(c, BLOCKMOVE, &[r.0[2], definition.wrapping_add(r.0[1]) + TOURNAMENTS, TOURNAMENT_SIZE], 0x8003_2584);
                        let definition = c.m.r32(this + DEFINITION);
                        let tournament = definition.wrapping_add(r.0[1]) + TOURNAMENTS;
                        r.0[0] = c.m.r8(tournament + 2) as u32;
                        let n = c.m.r8(tournament + 1) as u32;
                        r.0[2] = r.0[2].wrapping_add(TOURNAMENT_SIZE);
                        if (r.0[0] as i32) < r.0[0].wrapping_add(n) as i32 {
                            r.0[3] = r.0[0];
                            loop {
                                let j = sx(r.0[0]);
                                let definition = c.m.r32(this + DEFINITION);
                                r.call_leaving(c, BLOCKMOVE, &[r.0[2], track(definition, j), TRACK_SIZE], 0x8003_25d8);
                                r.0[0] = r.0[0].wrapping_add(1);
                                let definition = c.m.r32(this + DEFINITION);
                                let n = c.m.r8(definition.wrapping_add(r.0[1]) + TOURNAMENTS + 1) as u32;
                                r.0[2] = r.0[2].wrapping_add(TRACK_SIZE);
                                if sx(r.0[0]) as i32 >= r.0[3].wrapping_add(n) as i32 {
                                    break;
                                }
                            }
                        }
                        r.0[4] = r.0[4].wrapping_add(1);
                        let definition = c.m.r32(this + DEFINITION);
                        let n = c.m.r8(definition.wrapping_add(r.0[5].wrapping_mul(TIER_SIZE))) as u32;
                        if sx(r.0[4]) as i32 >= r.0[7].wrapping_add(n) as i32 {
                            break;
                        }
                    }
                }
                r.0[8] = r.0[8].wrapping_add(1);
                if sx(r.0[8]) as i32 >= c.m.r8(this + TIERS) as i32 {
                    break;
                }
            }
        }
        let data = c.m.r32(sp + 96);
        r.call_leaving(c, PURGE, &[data], 0x8003_2668);
    })
}

/// `tTournamentManager::ReleaseDescription` (FETOURN.CPP, 0x80032698): the definition freed.
pub fn release_description(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let definition = c.m.r32(this + DEFINITION);
        if definition != 0 {
            r.call_leaving(c, PURGE, &[definition], 0x8003_26c0);
            c.m.w32(this + DEFINITION, 0);
        }
    })
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
        m.w16(this + TRACK_LIST + 2 * sx(j), number.wrapping_add(214) as u16);
    }
    m.w16(this + TRACK_LIST + 2 * n, 0);
}

pub fn abi_update_track_list(c: &mut Cpu) {
    update_track_list(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tTournamentManager::GetTrackList` (FETOURN.CPP, 0x800327a4): the tracks listed.
pub fn abi_get_track_list(c: &mut Cpu) {
    let (this, tier, k) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8003_26d4, 0x8003_27c8, &[this, sx(tier), sx(k)], abi_update_track_list);
        c.r[2] = this + TRACK_LIST;
    })
}

/// `tTournamentManager::GetTrackToRace` (FETOURN.CPP, 0x800327dc): the track to race, with its
/// settings for this tournament.
pub fn abi_get_track_to_race(c: &mut Cpu) {
    let (this, out) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = out;
        let m = &mut c.m;
        let t = current(m, this);
        let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK));
        let definition = m.r32(this + DEFINITION);
        r.call_leaving(c, BLOCKMOVE, &[track(definition, k), out, TRACK_SIZE], 0x8003_2860);
        let m = &mut c.m;
        for (f, at) in [(1, DIRECTIONS), (2, MIRRORS), (3, TIMES_OF_DAY), (4, WEATHERS)] {
            let j = m.r32(this + TRACK);
            let v = m.r8(this.wrapping_add(j) + at);
            m.w8(out + f, v);
        }
    })
}

/// `tTournamentManager::StartNewTournament` (FETOURN.CPP, 0x800328d4): tournament `k` of tier
/// `tier` from its first track: its competitors (the player first, the others its personalities,
/// placed in order), each track's settings (random where the definition leaves them open).
pub fn abi_start_new_tournament(c: &mut Cpu) {
    let (this, tier, k) = (c.r[4], c.r[5] & 0xFF, c.r[6] & 0xFF);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        c.m.w32(this + TIER, tier);
        c.m.w32(this + TOURNAMENT, k);
        c.m.w32(this + TRACK, 0);
        let n = sx(port(c, &r, 0x8003_36c8, 0x8003_290c, &[this], abi_get_num_competitors));
        let m = &mut c.m;
        m.w32(this + RACERS, n);
        let t = current(m, this);
        r.0[2] = 0;
        r.0[4] = t;
        if n as i32 > 0 {
            loop {
                let j = sx(r.0[2]);
                let comp = competitor(this, j);
                m.w16(comp + POINTS, 0);
                m.w32(comp + ELIMINATED, 0);
                m.w8(comp + IS_PLAYER, 0);
                let personality = if j != 0 { m.r8(t.wrapping_add(j) + 51) as u32 } else { 0 };
                m.w32(comp + PERSONALITY, personality);
                let place = if j != 0 { r.0[2] as u8 } else { m.r8(this + RACERS) };
                m.w8(comp + PLACE, place);
                r.0[2] = r.0[2].wrapping_add(1);
                if sx(r.0[2]) as i32 >= m.r32(this + RACERS) as i32 {
                    break;
                }
            }
        }
        r.0[2] = 0;
        if c.m.r8(t + 1) != 0 {
            loop {
                let j = sx(r.0[2]);
                let m = &mut c.m;
                let definition = m.r32(this + DEFINITION);
                let tr = track(definition, (m.r8(t + 2) as u32).wrapping_add(j));
                r.0[1] = tr;
                r.0[0] = this.wrapping_add(j);
                let ras = [0x8003_2a38, 0x8003_2a64, 0x8003_2a90, 0x8003_2abc];
                for (f, (at, ra)) in [DIRECTIONS, MIRRORS, TIMES_OF_DAY, WEATHERS].into_iter().zip(ras).enumerate() {
                    let v = c.m.r8(tr + 1 + f as u32);
                    c.m.w8(r.0[0] + at, v);
                    if v >= 2 {
                        let v = r.call_leaving(c, RAND, &[], ra) & 1;
                        c.m.w8(r.0[0] + at, v as u8);
                    }
                }
                r.0[2] = r.0[2].wrapping_add(1);
                if sx(r.0[2]) as i32 >= c.m.r8(t + 1) as i32 {
                    break;
                }
            }
        }
        c.m.w8(this + COMPETITORS + IS_PLAYER, 1);
    })
}

/// `tTournamentManager::IsTournamentFinished` (FETOURN.CPP, 0x80032b0c): every track raced, or
/// (knocking out) the player's points behind the track.
pub fn abi_is_tournament_finished(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    let t = current(m, this);
    let j = m.r32(this + TRACK) as i32;
    let finished = if j < m.r8(t + 1) as i32 { m.r8(t + 5) != 0 && (m.r16(this + COMPETITORS + POINTS) as i32) < j } else { true };
    c.r[2] = finished as u32;
}

/// `tournPointsCompare` (FETOURN.CPP, 0x80032ba0): `qsort`'s order of two competitors (indexes
/// at `a`, `b`): more points first, then the player, then the better place in the race.
pub fn abi_tourn_points_compare(c: &mut Cpu) {
    let (a, b) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let (ka, kb) = (m.r8(a) as u32, m.r8(b) as u32);
    let (ca, cb) = (competitor(TOURNAMENT_MANAGER, ka), competitor(TOURNAMENT_MANAGER, kb));
    let mut v = (m.r16(cb + POINTS) as u32).wrapping_sub(m.r16(ca + POINTS) as u32);
    if v == 0 {
        v = (m.r8(cb + IS_PLAYER) as i8 as i32 as u32).wrapping_sub(m.r8(ca + IS_PLAYER) as i8 as i32 as u32);
        if v == 0 {
            v = m.r32(STATS + STATS_SIZE * ka + 132).wrapping_sub(m.r32(STATS + STATS_SIZE * kb + 132));
        }
    }
    c.r[2] = v;
}

/// `tTournamentManager::UpdateTournFinishMoney` (FETOURN.CPP, 0x80032c2c): the tournament's
/// prize for the player's place (knocking out) or each ranked place it holds, its best place.
pub fn update_tourn_finish_money(c: &mut Cpu, this: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let m = &mut c.m;
        let t = current(m, this);
        r.0[3] = t;
        let id = m.r8(t) as i8 as i32 as u32;
        let best = m.r8(this.wrapping_add(id) + BEST_PLACES);
        m.w8(this + LAST_BEST, best);
        if m.r8(t + 5) != 0 {
            let place = (m.r8(this + COMPETITORS + PLACE) as u32).wrapping_sub(1);
            let prize = port(c, &r, 0x8003_399c, 0x8003_2cc0, &[this, place], abi_get_tournament_finish_prize);
            let v = c.m.r32(this + AWARD + AWARD_TOURN_MONEY).wrapping_add(prize);
            c.m.w32(this + AWARD + AWARD_TOURN_MONEY, v);
            return;
        }
        r.0[0] = 0;
        loop {
            let k = r.0[0] & 0xFF;
            r.0[1] = k;
            if c.m.r8(this + RANKING + k) == 0 {
                let prize = port(c, &r, 0x8003_399c, 0x8003_2cf8, &[this, k], abi_get_tournament_finish_prize);
                let m = &mut c.m;
                let v = m.r32(this + AWARD + AWARD_TOURN_MONEY).wrapping_add(prize);
                m.w32(this + AWARD + AWARD_TOURN_MONEY, v);
                let id = m.r8(t) as i8 as i32 as u32;
                let at = this.wrapping_add(id) + BEST_PLACES;
                if (k as i32) < m.r8(at) as i8 as i32 {
                    m.w8(at, r.0[0].wrapping_add(1) as u8);
                }
            }
            r.0[0] = r.0[0].wrapping_add(1);
            if (r.0[0] & 0xFF) >= 6 {
                break;
            }
        }
    })
}

pub fn abi_update_tourn_finish_money(c: &mut Cpu) {
    update_tourn_finish_money(c, c.r[4]);
}

/// `tTournamentManager::UpdateTrackFinishMoney` (FETOURN.CPP, 0x80032d5c): the track's prize for
/// the player's place, if it finished.
pub fn abi_update_track_finish_money(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        if c.m.r32(STATS + 128) != 2 {
            return;
        }
        let place = sx((c.m.r16(STATS + 132) as u32).wrapping_sub(1));
        let prize = port(c, &r, 0x8003_37c4, 0x8003_2d9c, &[this, place], abi_get_track_finish_prize);
        let v = c.m.r32(this + AWARD + AWARD_MONEY).wrapping_add(prize);
        c.m.w32(this + AWARD + AWARD_MONEY, v);
    })
}

/// `tTournamentManager::CalcTrackFinishDamageBill` (FETOURN.CPP, 0x80032dbc): (worked out when
/// asked) the player's car's worth (with its upgrades); damaged, the bill is three ten
/// thousandths of it per damage point; undamaged and in the first three, a bonus (a twentieth,
/// three hundredths, a hundredth); the bill and the bonus.
pub fn abi_calc_track_finish_damage_bill(c: &mut Cpu) {
    let (work, damage_out, bonus_out) = (c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = damage_out;
        r.0[2] = bonus_out;
        if work != 0 {
            r.0[0] = STATS;
            let car = sp + 16;
            let k = c.m.r8(GARAGE_CAR) as u32;
            port(c, &r, 0x8001_7094, 0x8003_2e00, &[CAR_MANAGER, k, car, 0], fecars::abi_get_garage_car);
            let m = &mut c.m;
            let mut worth = m.r32(car + 32);
            let upgrades = m.r8(car + 196) as u32;
            for k in 0..2 {
                if upgrades & (1 << k) != 0 {
                    worth = worth.wrapping_add(m.r32(car + 36 + 4 * k));
                }
            }
            let raw = m.r32(STATS + 112) as i32;
            let damage = (if raw < 0 { raw.wrapping_add(0xFFFF) } else { raw }) >> 16;
            if damage != 0 || m.r32(STATS + 132) as i32 >= 4 {
                let v = (worth as i32).wrapping_mul(damage).wrapping_mul(3);
                m.w32(BONUS, 0);
                m.w32(DAMAGE_BILL, (v / 10000) as u32);
            } else {
                let place = m.r32(STATS + 132);
                m.w32(DAMAGE_BILL, 0);
                m.w32(BONUS, (worth as i32 / 20) as u32);
                if place == 2 {
                    m.w32(BONUS, ((worth as i32).wrapping_mul(3) / 100) as u32);
                } else if place == 3 {
                    m.w32(BONUS, (worth as i32 / 100) as u32);
                }
            }
        }
        let m = &mut c.m;
        let (bill, bonus) = (m.r32(DAMAGE_BILL), m.r32(BONUS));
        m.w32(damage_out, bill);
        m.w32(bonus_out, bonus);
    })
}

/// `tTournamentManager::UpdateTrackFinishPoints` (FETOURN.CPP, 0x80032f80): knocking out, each
/// racer still in a point (the last eliminated) and its place, one racer fewer; else each
/// finisher's points for its place, the competitors ranked by points (`qsort`), their places.
pub fn abi_update_track_finish_points(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let n = port(c, &r, 0x8003_36c8, 0x8003_2f94, &[this], abi_get_num_competitors);
        let m = &mut c.m;
        let t = current(m, this);
        let n = sx(n) as i32;
        if m.r8(t + 5) != 0 {
            let mut racer = 0;
            for k in 0..n.max(0) as u32 {
                let comp = competitor(this, k);
                if m.r32(comp + ELIMINATED) != 0 {
                    continue;
                }
                let stat = STATS.wrapping_add(STATS_SIZE.wrapping_mul(sx(racer)));
                let place = m.r32(stat + 132);
                if place.wrapping_sub(1) < 6 {
                    if (place as i32) < m.r32(this + RACERS) as i32 {
                        let v = m.r16(comp + POINTS).wrapping_add(1);
                        m.w16(comp + POINTS, v);
                    } else {
                        m.w32(comp + ELIMINATED, 1);
                    }
                    let v = m.r8(stat + 132);
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
            let place = m.r32(stat + 132).wrapping_sub(1);
            if place < 6 && m.r32(stat + 128) == 2 {
                let comp = competitor(this, k);
                let v = m.r16(comp + POINTS).wrapping_add(m.r8(this + FINISH_POINTS + place) as u16);
                m.w16(comp + POINTS, v);
            }
        }
        for k in 0..6 {
            m.w8(this + RANKING + k, k as u8);
        }
        r.call_leaving(c, QSORT, &[this + RANKING, n as u32, 1, 0x8003_2ba0], 0x8003_3148);
        let m = &mut c.m;
        for k in 0..6 {
            let who = m.r8(this + RANKING + k) as u32;
            m.w8(competitor(this, who) + PLACE, (k + 1) as u8);
        }
    })
}

/// `tTournamentManager::AdvanceToNextTrack` (FETOURN.CPP, 0x80033184): (unless past the last
/// track) the award cleared; the track's money and points; after the last track the tournament's
/// money and the tier's prize; then, the tournament over, what placing in the first three
/// unlocks, what winning gives (its car, its bonus, its best place) and, with every tournament
/// of the tier won and its bonus code not yet used, the tier's car (bought, or its money when the
/// garage is full). The track now (-1: past the last).
pub fn abi_advance_to_next_track(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let t = current(m, this);
        r.0[2] = t;
        if ((m.r8(t + 1) as i32) - 1) < m.r32(this + TRACK) as i32 {
            c.r[2] = u32::MAX;
            return;
        }
        let award = this + AWARD;
        m.w32(award + AWARD_MONEY, 0);
        m.w32(award + AWARD_TOURN_MONEY, 0);
        m.w16(award + ACTIVATE, 0);
        m.w32(award + AWARD_CAR, 0);
        port(c, &r, 0x8003_2d5c, 0x8003_3218, &[this], abi_update_track_finish_money);
        port(c, &r, 0x8003_2f80, 0x8003_3220, &[this], abi_update_track_finish_points);
        if c.m.r32(this + TRACK) == (c.m.r8(t + 1) as u32).wrapping_sub(1) {
            port(c, &r, 0x8003_2c2c, 0x8003_323c, &[this], abi_update_tourn_finish_money);
            port(c, &r, 0x8003_3c74, 0x8003_3244, &[this], abi_calc_tier_finish_prize);
        }
        let m = &mut c.m;
        let j = m.r32(this + TRACK).wrapping_add(1);
        m.w32(this + TRACK, j);
        if (j as i32) < m.r8(t + 1) as i32 {
            c.r[2] = j;
            return;
        }
        m.w32(award + AWARD_CAR_FULL, 0);
        m.w32(award + COMPLETED_FULL, 0);
        let flags = m.r16(t + 10);
        if m.r8(this + COMPETITORS + PLACE) < 4 {
            let act = |m: &mut Mem, bit: u16, at: u32, value: u32, byte: bool| {
                if flags & bit != 0 {
                    let v = m.r16(award + ACTIVATE) | bit;
                    m.w16(award + ACTIVATE, v);
                    if byte {
                        m.w8(award + at, value as u8);
                    } else {
                        m.w32(award + at, value);
                    }
                }
            };
            let values = [m.r8(t + 14) as u32, m.r8(t + 15) as u32, m.r8(t + 16) as u32, m.r8(t + 67) as u32];
            act(m, 1, ACT_TRACK, values[0], true);
            act(m, 2, ACT_CLASS, values[1], false);
            act(m, 4, ACT_CAR, values[2], false);
            act(m, 8, ACT_TRACK_CLASS, values[3], false);
            m.w32(award + COMPLETED_TIER, 0);
        }
        if c.m.r8(this + COMPETITORS + PLACE) >= 2 {
            c.r[2] = c.m.r32(this + TRACK);
            return;
        }
        // won
        if c.m.r8(t + 7) != 0 {
            r.0[0] = CAR_MANAGER;
            let model = c.m.r8(t + 8) as u32;
            let car = port(c, &r, 0x8001_6374, 0x8003_3374, &[CAR_MANAGER, model], fecars::abi_get_car_from_id);
            r.0[4] = 1;
            r.0[3] = car;
            let m = &mut c.m;
            m.w32(award + AWARD_CAR, 1);
            let model = m.r8(t + 8) as u32;
            m.w32(award + AWARD_CAR_MODEL, model);
            let color = m.r8(car + 4);
            m.w8(award + AWARD_CAR_COLOR, color);
            let upgrades = m.r8(t + 9);
            m.w8(award + AWARD_CAR_UPGRADES, upgrades);
            let owned = sx(port(c, &r, 0x8001_747c, 0x8003_33a4, &[CAR_MANAGER, 0], fecars::abi_get_num_owned_cars)) as i32;
            if owned >= GARAGE {
                let m = &mut c.m;
                m.w32(award + AWARD_CAR_FULL, 1);
                let worth = m.r32(car + 32);
                let v = m.r32(award + AWARD_TOURN_MONEY).wrapping_add(worth);
                m.w32(award + AWARD_TOURN_MONEY, v);
                let worth = m.r32(car + 32);
                m.w32(award + AWARD_CAR_MONEY, worth);
            }
        }
        let m = &mut c.m;
        if flags & 0x10 != 0 {
            let v = m.r16(award + ACTIVATE) | 0x10;
            m.w16(award + ACTIVATE, v);
            let bonus = m.r8(t + 68) as u32;
            m.w32(award + ACT_BONUS, bonus);
        }
        m.w32(award + COMPLETED_TIER, 0);
        let id = m.r8(t) as i8 as i32 as u32;
        m.w8(this.wrapping_add(id) + BEST_PLACES, 1);
        let tier = m.r32(this + TIER);
        if port(c, &r, 0x8002_373c, 0x8003_342c, &[tier.wrapping_add(TIER_BONUS)], fecheats::abi_is_cheat_enabled) ^ 1 == 0 {
            c.r[2] = c.m.r32(this + TRACK);
            return;
        }
        // the tier completed?
        let m = &mut c.m;
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
        if c.m.r32(award + COMPLETED_TIER) == 0 {
            c.r[2] = c.m.r32(this + TRACK);
            return;
        }
        r.0[2] = CAR_MANAGER;
        let owned = sx(port(c, &r, 0x8001_747c, 0x8003_34f0, &[CAR_MANAGER, 0], fecars::abi_get_num_owned_cars));
        r.0[0] = owned;
        if c.m.r32(award + AWARD_CAR) != 0 {
            r.0[0] = owned.wrapping_add(1);
        }
        let tier = c.m.r32(this + TIER);
        port(c, &r, 0x8002_35f8, 0x8003_3514, &[tier.wrapping_add(TIER_BONUS)], fecheats::abi_activate_bonus);
        let tier = c.m.r32(this + TIER);
        c.m.w32(award + COMPLETED_CAR, tier.wrapping_add(TIER_CAR));
        let model = lh(&mut c.m, award + COMPLETED_CAR) as u32;
        let car = port(c, &r, 0x8001_6374, 0x8003_3530, &[CAR_MANAGER, model], fecars::abi_get_car_from_id);
        r.0[3] = car;
        let model = c.m.r32(award + COMPLETED_CAR);
        port(c, &r, 0x8001_6f34, 0x8003_3544, &[CAR_MANAGER, model, 1], fecars::abi_set_car_available);
        let model = c.m.r32(award + COMPLETED_CAR);
        port(c, &r, 0x8001_6fbc, 0x8003_3554, &[CAR_MANAGER, model, 1], fecars::abi_set_car_viewable);
        if (r.0[0] as i32) >= GARAGE {
            let m = &mut c.m;
            m.w32(award + COMPLETED_FULL, 1);
            let worth = m.r32(car + 32);
            m.w32(award + COMPLETED_MONEY, worth);
            let v = m.r32(this + MONEY).wrapping_add(worth);
            m.w32(this + MONEY, v);
        } else {
            let model = lh(&mut c.m, award + COMPLETED_CAR) as u32;
            let color = c.m.r8(car + 4) as u32;
            port(c, &r, 0x8001_6598, 0x8003_3594, &[CAR_MANAGER, model, color, 0], fecars::abi_purchase_car);
        }
        let v = c.m.r16(this + TIER).wrapping_add(COMPLETED_TEXTS as u16);
        c.m.w16(award + COMPLETED_TEXT, v);
        c.r[2] = c.m.r32(this + TRACK);
    })
}

/// `tTournamentManager::GetLastTrackRaced` (FETOURN.CPP, 0x800335c8): the last track's number.
pub fn abi_get_last_track_raced(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    let t = current(m, this);
    let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK)).wrapping_sub(1);
    let definition = m.r32(this + DEFINITION);
    c.r[2] = m.r8(track(definition, k)) as i8 as i32 as u32;
}

/// `tTournamentManager::SaveTournament` (FETOURN.CPP, 0x80033638): the money and the best
/// places, for a saved game.
pub fn abi_save_tournament(c: &mut Cpu) {
    let (this, save) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let v = m.r32(this + MONEY);
    m.w32(save, v);
    for k in 0..64 {
        let v = m.r8(this + BEST_PLACES + k);
        m.w8(save + 104 + k, v);
    }
}

/// `tTournamentManager::LoadTournament` (FETOURN.CPP, 0x80033680): the money and the best places
/// from a saved game.
pub fn abi_load_tournament(c: &mut Cpu) {
    let (this, save) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let v = m.r32(save);
    m.w32(this + MONEY, v);
    for k in 0..64 {
        let v = m.r8(save + 104 + k);
        m.w8(this + BEST_PLACES + k, v);
    }
}

/// `tTournamentManager::GetNumCompetitors` (FETOURN.CPP, 0x800336c8): the tournament's cars.
pub fn abi_get_num_competitors(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    let t = current(m, this);
    c.r[2] = m.r8(t + 6) as u32;
}

/// `tTournamentManager::UpdateCarLineup` (FETOURN.CPP, 0x80033714): the racers still in, in
/// order: the player's or not, their personalities, their places.
pub fn abi_update_car_lineup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let n = sx(port(c, &r, 0x8003_36c8, 0x8003_3728, &[this], abi_get_num_competitors)) as i32;
        let m = &mut c.m;
        let mut at = 0;
        for k in 0..n.max(0) as u32 {
            let comp = competitor(this, k);
            if m.r32(comp + ELIMINATED) != 0 {
                continue;
            }
            let line = this.wrapping_add(sx(at).wrapping_mul(20)) + LINEUP;
            let player = (m.r8(comp + IS_PLAYER) as i8 != 0) as u32;
            m.w32(line, player);
            let personality = m.r32(comp + PERSONALITY);
            m.w32(line + 4, personality);
            let place = m.r8(comp + PLACE);
            m.w8(line + 8, place);
            at += 1;
        }
    })
}

/// `tTournamentManager::GetTrackFinishPrize` (FETOURN.CPP, 0x800337c4): the track's prize for
/// `place` (none past sixth), and against cars of the player's class its car's worth (with its
/// upgrades) by the class's multiplier.
pub fn abi_get_track_finish_prize(c: &mut Cpu) {
    let (this, place) = (c.r[4], c.r[5]);
    abi::frame(c, 416, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[1] = place;
        r.0[0] = 0;
        let table = sp + 224;
        for k in (0..168).step_by(4) {
            let v = c.m.r32(MULTIPLIERS + k);
            c.m.w32(table + k, v);
        }
        if (place & 0xFFFF) >= 6 {
            c.r[2] = 0;
            return;
        }
        let t = current(&mut c.m, this);
        r.0[3] = t;
        if c.m.r8(t + 3) == 10 {
            let car = sp + 16;
            let k = c.m.r8(GARAGE_CAR) as u32;
            port(c, &r, 0x8001_7094, 0x8003_38b4, &[CAR_MANAGER, k, car, 0], fecars::abi_get_garage_car);
            let m = &mut c.m;
            let class = m.r8(car + 2) as u32;
            if class < 7 {
                let upgrades = m.r8(car + 196) as u32;
                let mut worth = m.r32(car + 32);
                for k in 0..3 {
                    if upgrades & (1 << k) != 0 {
                        worth = worth.wrapping_add(m.r32(car + 36 + 4 * k));
                    }
                }
                let multiplier = m.r32(table + (sx(place) << 2).wrapping_add(class * 24));
                let v = r.call_leaving(c, FIXEDMULT, &[multiplier, worth], 0x8003_3940);
                r.0[0] = v;
            }
        }
        let m = &mut c.m;
        let k = (m.r8(t + 2) as u32).wrapping_add(m.r32(this + TRACK));
        let definition = m.r32(this + DEFINITION);
        let prize = m.r32(track(definition, k).wrapping_add(sx(place) << 2) + 8);
        c.r[2] = r.0[0].wrapping_add(prize);
    })
}

/// `tTournamentManager::GetTournamentFinishPrize` (FETOURN.CPP, 0x8003399c): the tournament's
/// prize for `place` (none past sixth).
pub fn abi_get_tournament_finish_prize(c: &mut Cpu) {
    let (this, place) = (c.r[4], c.r[5]);
    if (place & 0xFFFF) >= 6 {
        c.r[2] = 0;
        return;
    }
    let m = &mut c.m;
    let t = current(m, this);
    c.r[2] = m.r32(t.wrapping_add(sx(place) << 2) + 24);
}

/// `tTournamentManager::GetAwardInformation` (FETOURN.CPP, 0x80033a10): the award copied.
pub fn abi_get_award_information(c: &mut Cpu) {
    let (this, out) = (c.r[4], c.r[5]);
    for k in (0..68).step_by(4) {
        let v = c.m.r32(this + AWARD + k);
        c.m.w32(out + k, v);
    }
}

/// `tTournamentManager::UpdateAwardInformation` (FETOURN.CPP, 0x80033a50): the money (less the
/// bill, with the bonus and the prizes); what the award unlocks (bonus codes, a car, a track, a
/// track class, a bonus); a car won (not for a full garage) bought, with its upgrades.
pub fn abi_update_award_information(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        port(c, &r, 0x8003_2dbc, 0x8003_3a74, &[this, 1, sp + 16, sp + 20], abi_calc_track_finish_damage_bill);
        let m = &mut c.m;
        let award = this + AWARD;
        let money = m.r32(this + MONEY).wrapping_sub(m.r32(sp + 16)).wrapping_add(m.r32(sp + 20));
        let money = money.wrapping_add(m.r32(award + AWARD_MONEY)).wrapping_add(m.r32(award + AWARD_TOURN_MONEY));
        m.w32(this + MONEY, money);
        let flags = |c: &mut Cpu| c.m.r16(award + ACTIVATE);
        if flags(c) & 2 != 0 {
            let v = c.m.r32(award + ACT_CLASS);
            port(c, &r, 0x8002_35f8, 0x8003_3ac4, &[v], fecheats::abi_activate_bonus);
        }
        if flags(c) & 4 != 0 {
            let v = c.m.r32(award + ACT_CAR);
            port(c, &r, 0x8001_6f34, 0x8003_3ae8, &[CAR_MANAGER, v, 1], fecars::abi_set_car_available);
        }
        if flags(c) & 1 != 0 {
            let v = c.m.r8(award + ACT_TRACK) as u32;
            port(c, &r, 0x8002_1dec, 0x8003_3b0c, &[TRACK_MANAGER, v, 1], fetracks::abi_set_track_available);
        }
        if flags(c) & 8 != 0 {
            let v = c.m.r32(award + ACT_TRACK_CLASS);
            port(c, &r, 0x8002_1e00, 0x8003_3b30, &[TRACK_MANAGER, v, 1], fetracks::abi_set_class_available);
        }
        if flags(c) & 0x10 != 0 {
            let v = c.m.r32(award + ACT_BONUS);
            port(c, &r, 0x8002_35f8, 0x8003_3b50, &[v], fecheats::abi_activate_bonus);
        }
        if c.m.r32(award + AWARD_CAR) != 0 && c.m.r32(award + AWARD_CAR_FULL) == 0 {
            r.0[0] = CAR_MANAGER;
            let model = lh(&mut c.m, award + AWARD_CAR_MODEL) as u32;
            let car = port(c, &r, 0x8001_6374, 0x8003_3b80, &[CAR_MANAGER, model], fecars::abi_get_car_from_id);
            let m = &mut c.m;
            let k = m.r8(award + AWARD_CAR_COLOR) as u32;
            let color = m.r8(car.wrapping_add(k) + 175) as i8 as i32 as u32;
            let model = lh(m, award + AWARD_CAR_MODEL) as u32;
            port(c, &r, 0x8001_6598, 0x8003_3b9c, &[CAR_MANAGER, model, color, 0], fecars::abi_purchase_car);
            let m = &mut c.m;
            let (k, upgrades) = (m.r8(GARAGE_CAR) as u32, m.r8(award + AWARD_CAR_UPGRADES) as u32);
            port(c, &r, 0x8001_67f4, 0x8003_3bb4, &[CAR_MANAGER, k, upgrades, 0], fecars::abi_purchase_upgrade);
        }
    })
}

/// `tTournamentManager::TournPointTotal` (FETOURN.CPP, 0x80033bc8): competitor `*k`'s points.
pub fn abi_tourn_point_total(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5]);
    let m = &mut c.m;
    let who = lh(m, k) as u32;
    c.r[2] = m.r16(competitor(this, who) + POINTS) as u32;
}

/// `tTournamentManager::PlayerRanking` (FETOURN.CPP, 0x80033be4): the competitor in `place` (0:
/// none).
pub fn abi_player_ranking(c: &mut Cpu) {
    let (this, place) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = place;
        let n = sx(port(c, &r, 0x8003_36c8, 0x8003_3c00, &[this], abi_get_num_competitors)) as i32;
        let place = sx(place);
        c.r[2] = 0;
        for k in 0..n.max(0) as u32 {
            if c.m.r8(competitor(this, k) + PLACE) as u32 == place {
                c.r[2] = k;
                return;
            }
        }
    })
}

/// `tTournamentManager::CalcTierFinishPrize` (FETOURN.CPP, 0x80033c74): the tier's prize 7,
/// unchanged.
pub fn abi_calc_tier_finish_prize(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    let tier = m.r32(this + TIER);
    m.w16(this.wrapping_add(tier << 1) + TIER_PRIZE, 7);
    let tier = m.r32(this + TIER);
    m.w16(this.wrapping_add(tier << 1) + TIER_PRIZE_CHANGE, 0);
}

/// `tTournamentManager::GetTrophyName` (FETOURN.CPP, 0x80033ca0): a trophy's picture's file name,
/// `TR` + its size (S, M, L) + its number + its metal (W, G, S, B: the place, -1 the best won,
/// not placed: W).
pub fn abi_get_trophy_name(c: &mut Cpu) {
    let (t, size, out) = (c.r[5], c.r[6], c.r[7]);
    let place = abi::arg(c, 4);
    abi::frame(c, 48, |c, sp| {
        let r = Saved::of(c);
        let m = &mut c.m;
        for k in 0..3 {
            let v = m.r8(SIZES + k);
            m.w8(sp + 24 + k, v);
        }
        let id = m.r8(t) as i8 as i32 as u32;
        let best = m.r8(TOURNAMENT_MANAGER.wrapping_add(id) + BEST_PLACES) as i8 as i32 as u32;
        for k in 0..4 {
            let v = m.r8(PLACES + k);
            m.w8(sp + 32 + k, v);
        }
        let metal = if place == u32::MAX { sx(if best.wrapping_sub(1) < 3 { best } else { 0 }) } else { place };
        let trophy = m.r8(t + 22) as u32;
        let metal = m.r8(sp.wrapping_add(metal) + 32) as u32;
        let size = m.r8(sp.wrapping_add(size) + 24) as u32;
        r.call_leaving(c, SPRINTF, &[out, TROPHY_FORMAT, size, trophy, metal], 0x8003_3d50);
    })
}

/// `tTournamentManager::ValidCar` (FETOURN.CPP, 0x80033d60): a car the tournament allows: of
/// its class (any class with the first tier's cheat on), and the car and upgrades it requires.
pub fn abi_valid_car(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[3] = car;
        let t = current(&mut c.m, this);
        r.0[1] = t;
        let class = c.m.r8(t + 3);
        r.0[0] = 1;
        if class != 10 {
            r.0[0] = (c.m.r8(car + 2) == class) as u32;
            let on = port(c, &r, 0x8002_373c, 0x8003_3de8, &[TIER_BONUS], fecheats::abi_is_cheat_enabled);
            if on != 0 && c.m.r32(this + TIER) == 0 {
                r.0[0] = 1;
            }
        }
        let m = &mut c.m;
        let ok = r.0[0];
        c.r[2] = if m.r16(t + 12) & 4 == 0 {
            ok
        } else if m.r8(car) as i8 as i32 != m.r8(t + 18) as i32 {
            0
        } else {
            let owned = m.r8(car + 196);
            match m.r8(t + 19) {
                1 => ok,
                0 if owned != 0 => 0,
                2 if owned == 0 => 0,
                3 if owned & m.r8(t + 20) != m.r8(t + 20) => 0,
                _ => ok,
            }
        };
    })
}

/// `tListIteratorTournament::tListIteratorTournament` (FETOURN.CPP, 0x80033ed8): an iterator
/// over `values` of `manager`'s tournaments.
pub fn abi_list_iterator_tournament(c: &mut Cpu) {
    let (this, values, manager) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.0[1] = manager;
        port(c, &r, 0x8002_3df0, 0x8003_3f00, &[this, 0, values], femenu::abi_list_iterator);
        c.m.w32(this + 12, ITERATOR_VT);
        c.m.w32(this + 16, manager);
        c.r[2] = this;
    })
}

/// `tListIteratorTournament::~tListIteratorTournament` (FETOURN.CPP, 0x80033f28).
pub fn abi_list_iterator_tournament_dtor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        c.m.w32(this + 12, ITERATOR_VT);
        port(c, &Saved::of(c), 0x8002_3e14, 0x8003_3f40, &[this, flags], femenu::abi_list_iterator_dtor);
    })
}

/// `tListIteratorTournament::Value` (FETOURN.CPP, 0x80033f50).
pub fn abi_value(c: &mut Cpu) {
    let values = c.m.r32(c.r[4] + 4);
    c.r[2] = c.m.r8(values) as u32;
}

/// The front end's tier's tournament `k` of the iterator's manager.
fn tier_tournament(m: &mut Mem, this: u32, k: u32) -> u32 {
    let tier = m.r8(FE_TIER) as u32;
    let manager = m.r32(this + 16);
    let definition = m.r32(manager + DEFINITION);
    tournament_of(m, definition, tier, k)
}

/// `tListIteratorTournament::TextValue` (FETOURN.CPP, 0x80033f64): the tournament's name.
pub fn abi_text_value(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    let values = m.r32(this + 4);
    let k = m.r8(values) as u32;
    let t = tier_tournament(m, this, k);
    c.r[2] = (m.r8(t) as i8 as i32 as u32).wrapping_add(NAMES);
}

/// `tListIteratorTournament::Increment` (FETOURN.CPP, 0x80033fbc): the next tournament of the
/// tier (round to the first) until one earned.
pub fn abi_increment(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let tier = m.r8(FE_TIER) as u32;
        let manager = m.r32(this + 16);
        let info = m.r32(manager + DEFINITION).wrapping_add(tier * TIER_SIZE);
        r.0[1] = info;
        loop {
            let m = &mut c.m;
            let at = m.r32(this + 4);
            let v = m.r8(at).wrapping_add(1);
            m.w8(at, v);
            let at = m.r32(this + 4);
            if m.r8(at) >= m.r8(info) {
                m.w8(at, 0);
            }
            let at = m.r32(this + 4);
            let k = m.r8(at) as u32;
            if port(c, &r, 0x8003_40ec, 0x8003_403c, &[this, k], abi_valid_tournament) == 1 {
                return;
            }
        }
    })
}

/// `tListIteratorTournament::Decrement` (FETOURN.CPP, 0x8003405c): the tournament before (round
/// to the last) until one earned.
pub fn abi_decrement(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let tier = m.r8(FE_TIER) as u32;
        let manager = m.r32(this + 16);
        let info = m.r32(manager + DEFINITION).wrapping_add(tier * TIER_SIZE);
        r.0[1] = info;
        loop {
            let m = &mut c.m;
            let at = m.r32(this + 4);
            let v = m.r8(at);
            let v = if v == 0 { m.r8(info).wrapping_sub(1) } else { v - 1 };
            m.w8(at, v);
            let at = m.r32(this + 4);
            let k = m.r8(at) as u32;
            if port(c, &r, 0x8003_40ec, 0x8003_40cc, &[this, k], abi_valid_tournament) == 1 {
                return;
            }
        }
    })
}

/// `tListIteratorTournament::ValidTournament` (FETOURN.CPP, 0x800340ec): tournament `k` earned:
/// the tournament it requires placed in (in the first three) or won.
pub fn abi_valid_tournament(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5] & 0xFF);
    let m = &mut c.m;
    let t = tier_tournament(m, this, k);
    let manager = m.r32(this + 16);
    let flags = m.r16(t + 12);
    let best = |m: &mut Mem| {
        let id = m.r8(t + 17) as u32;
        m.r8(manager.wrapping_add(id) + BEST_PLACES) as i8 as i32
    };
    let mut ok = 1;
    if flags & 1 != 0 {
        ok = (best(m) < 4) as u32;
    }
    if flags & 2 != 0 && best(m) >= 2 {
        ok = 0;
    }
    c.r[2] = ok;
}

/// The ports other ports call, entered as the originals' callers `jal` them.
pub(crate) fn get_track_list_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_27a4, None, &[]);
    abi_get_track_list(c);
}

pub(crate) fn get_track_to_race_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_27dc, None, &[]);
    abi_get_track_to_race(c);
}

pub(crate) fn calc_track_finish_damage_bill_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_2dbc, None, &[]);
    abi_calc_track_finish_damage_bill(c);
}

pub(crate) fn get_award_information_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3a10, None, &[]);
    abi_get_award_information(c);
}

pub(crate) fn tourn_point_total_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3bc8, None, &[]);
    abi_tourn_point_total(c);
}

pub(crate) fn player_ranking_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3be4, None, &[]);
    abi_player_ranking(c);
}

pub(crate) fn get_trophy_name_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3ca0, None, &[]);
    abi_get_trophy_name(c);
}

pub(crate) fn valid_car_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3d60, None, &[]);
    abi_valid_car(c);
}

pub(crate) fn text_value_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_3f64, None, &[]);
    abi_text_value(c);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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
        let cars = m.r32(CAR_MANAGER);
        let owned = [0, 31, 32, pick(rng, 33)][pick(rng, 4) as usize];
        for k in 0..32 {
            let e = CAR_MANAGER + 8 + 4 * k;
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

    #[test]
    fn fetourn_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7055 + k as u64, shuffle, &args);
        }
    }
}
