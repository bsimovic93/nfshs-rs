//! SCREENCONGRATS.CPP: the screens after a win (`tScreenCongrats` and its four kinds): the
//! trophy or medal won (its picture turning, or "congb"'s ribbon), studs round the edge, the
//! prize money counted into the total, a speech, and the car won turning on its stand; each
//! kind's message. The pink slips' (the winner's car taken), the tournament's trophy (a place
//! or knocked out, the car won), Be the Cop's and a tier's (the tier's car won).
//!
//! `DrawCar` is the host's (called by address).

use crate::abi::{self, Saved};
use crate::femenu::{PLAY_FE_SFX, method};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_WORD};
use crate::globals::{CAR_MANAGER, FRONT_END, GAME_TICKS, TEXT_COLORS, TOURNAMENT_MANAGER};
use crate::{fecars, fefades, fescreen, fetextrender, fetools, fetourn, screenpost};
use ps1rt::{Cpu, Mem};

// A screen: a `tScreen` (+40 its swapped shapes' table, +84 going off, +92 its fade, +96 its
// vptr), then +100 lost (1: knocked out or placed too low), +104 the prize (0 gold, 1 silver, 2
// bronze, 3 a car, 4 nothing), +108 its picture (1 the trophy, 2 the ribbon), +112 and +116 how
// many swapped shapes and the picture's frames, +120 the player shown, +124 the money in all,
// +128 the money still to count (-1 none), +132 the stars' frame, +136 the tick shown from (-1
// not yet), +140 the stars twinkling and +144 since when, +148 the car shown (`tCarInfo`), +352
// there is one, +356 the tick it came up, +360 the money counted, +364 skipped, +368 the
// counting's step, +372 the speech still to play, +376 and +378 the car's place, +380 and +384
// its distance and height (`float`s), +388 the pink slips' winner.
const SWAP_TABLE: u32 = 40;
const GOING_OFF: u32 = 84;
const FADE: u32 = 92;
const VPTR: u32 = 96;
const LOST: u32 = 100;
const PRIZE: u32 = 104;
const PICTURE: u32 = 108;
const SWAP_COUNT: u32 = 112;
const FRAMES: u32 = 116;
const PLAYER: u32 = 120;
const TOTAL: u32 = 124;
const TO_COUNT: u32 = 128;
const STARS: u32 = 132;
const SHOWN_AT: u32 = 136;
const TWINKLING: u32 = 140;
const TWINKLED_AT: u32 = 144;
const CAR: u32 = 148;
const HAS_CAR: u32 = 352;
const UP_AT: u32 = 356;
const COUNTED: u32 = 360;
const SKIPPED: u32 = 364;
const STEP: u32 = 368;
const SPEECH: u32 = 372;
const CAR_X: u32 = 376;
const CAR_Y: u32 = 378;
const CAR_DISTANCE: u32 = 380;
const CAR_HEIGHT: u32 = 384;
const WINNER: u32 = 388;

/// The virtual `CalculatePrizes`, `DrawCongratsMessage` and `GetCar`.
const CALCULATE_PRIZES: u32 = 80;
const DRAW_CONGRATS_MESSAGE: u32 = 88;
const GET_CAR: u32 = 96;

// A car (`tCarInfo`, 204 bytes): +0 its id, +175 its colours, +193 its speech, +196 its
// upgrades, +197 its colour (an index into its colours, then the colour), +199 the cop car's.
const CAR_SIZE: u32 = 204;
const CAR_ID: u32 = 0;
const CAR_COLORS: u32 = 175;
const CAR_SPEECH: u32 = 193;
const CAR_UPGRADES: u32 = 196;
const CAR_COLOR: u32 = 197;
const COP_COLOR: u32 = 199;

/// The tournament manager: the tier, the tournament, the racers left, the money, the
/// definition (each tier's first tournament at +2 of its 12 bytes; the tournaments from +36, 84
/// bytes each: +0 their id, +5 knock-out), the last best place.
const TIER: u32 = 4;
const TOURNAMENT: u32 = 8;
const RACERS: u32 = 16;
const MONEY: u32 = 20;
const DEFINITION: u32 = 24;
const LAST_BEST: u32 = 560;
// The award (`tAwardInformation`): the tournament's money, a car won, the garage full, its
// money, its model, colour and upgrades; the tier's text, car, garage full and money.
const AWARD_TOURN_MONEY: u32 = 4;
const AWARD_CAR: u32 = 28;
const AWARD_CAR_FULL: u32 = 32;
const AWARD_CAR_MONEY: u32 = 36;
const AWARD_CAR_MODEL: u32 = 40;
const AWARD_CAR_COLOR: u32 = 44;
const AWARD_CAR_UPGRADES: u32 = 45;
const COMPLETED_TEXT: u32 = 52;
const COMPLETED_CAR: u32 = 56;
const COMPLETED_FULL: u32 = 60;
const COMPLETED_MONEY: u32 = 64;

/// The garage's car (`frontEnd + 0x123`), the pink slips' cars (`+0x125`, by player), how many
/// tracks (`+0x47`) and the first player's wins (`+0x48`), the cop car and its colour (`+0x12d`,
/// `+0x12e`), the licence plates (`+0x384`, 8 bytes each), the language (`+0x340`).
const GARAGE_CAR: u32 = FRONT_END + 0x123;
const PINK_SLIPS_CAR: u32 = FRONT_END + 0x125;
const PINK_COUNT: u32 = FRONT_END + 0x47;
const PINK_WINS: u32 = FRONT_END + 0x48;
const COP_CAR: u32 = FRONT_END + 0x12D;
const COP_CAR_COLOR: u32 = FRONT_END + 0x12E;
const PLATES: u32 = FRONT_END + 0x384;
const LANGUAGE: u32 = FRONT_END + 0x340;
/// The showroom car's angle, the swapped file's name, the trophy's; the money's colour's index;
/// the cars still loading (`R3DCar_aSyncLoading`); the narration still speaking; a flag
/// `DrawCar` is given cleared.
const ANGLE: u32 = 0x8005_2DD0;
const SWAP_NAME: u32 = 0x8005_2DF4;
const TROPHY_NAME: u32 = 0x8005_2DE4;
const MONEY_COLOR: u32 = 0x8005_1AE7;
const ASYNC_LOADING: u32 = 0x8013_E668;
const SPEAKING: u32 = 0x8005_19E0;
const DRAW_CAR_FLAG: u32 = 0x8005_1748;
/// "%s%d", "zcong", "zelim", "congb".
const NAME_FORMAT: u32 = 0x8001_2234;
const CONGRATS_NAME: u32 = 0x8001_223C;
const LOST_NAME: u32 = 0x8001_2244;
const RIBBON_NAME: u32 = 0x8001_224C;

/// `DrawCar` (the host's).
const DRAW_CAR: u32 = 0x8003_AC64;
/// The 3D cars released, `ScaleShapeExtended`, `SetLicensePlate`, `PlayerName`, `TextSys_WordX`
/// and `TextSys_WordY`, `FeAudio_AsyncPlaySpeech`, the licence plates, `PAD_buttons`,
/// `blockmove`.
const RELEASE_CARS: fn(&mut Cpu) = nfs4rc::f_8004dfac;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = nfs4rc::f_8004ef70;
const SET_LICENSE_PLATE: fn(&mut Cpu) = nfs4rc::f_8002b84c;
const PLAYER_NAME: fn(&mut Cpu) = nfs4rc::f_8002ba7c;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const PLAY_SPEECH: fn(&mut Cpu) = nfs4rc::f_80015de0;
const CLEAN_UP_LICENSE: fn(&mut Cpu) = nfs4rc::f_800bd3d8;
const CREATE_LICENSE: fn(&mut Cpu) = nfs4rc::f_800bd044;
const PAD_BUTTONS: fn(&mut Cpu) = nfs4rc::f_800e4da4;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;

/// The texts: the money's and total's labels and places, "%s ... %s"s, the speech's, "LOST".
const MONEY_LABEL: u32 = 792;
const MONEY_AT: u32 = 793;
const TOTAL_LABEL: u32 = 791;
const TOTAL_AT: u32 = 794;
const PINK_MESSAGE: u32 = 630;
const CAR_NAMES: u32 = 290;
const TROPHY_TEXTS: u32 = 948;
const PRIZE_TEXTS: u32 = 994;
const WIN_TEXTS: u32 = 967;
const GARAGE_FULL: u32 = 64;
const LOST_TEXT: u32 = 801;
const COP_MESSAGE: u32 = 77;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn ticks(m: &mut Mem) -> u32 {
    m.r32(GAME_TICKS)
}

/// A call to a function that is ported too, as FEAPP's (a0..a3; the caller writes the stack's).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// The screen's virtual function at `slot` with `args` after the object.
fn vcall(c: &mut Cpu, r: &Saved, this: u32, slot: u32, args: &[u32], ra: u32) -> u32 {
    let (obj, f) = method(&mut c.m, this, VPTR, slot);
    let mut a = vec![obj];
    a.extend_from_slice(args);
    r.call_ptr_leaving(c, f, &a, ra)
}

/// The tournament played (`tTourneyInfo *`).
fn tournament(m: &mut Mem) -> u32 {
    let tm = TOURNAMENT_MANAGER;
    let (tier, definition) = (m.r32(tm + TIER), m.r32(tm + DEFINITION));
    let first = m.r8(definition.wrapping_add(tier.wrapping_mul(12)) + 2) as u32;
    let k = first.wrapping_add(m.r32(tm + TOURNAMENT));
    definition.wrapping_add(k.wrapping_mul(84)).wrapping_add(36)
}

/// The player's place in the tournament (900 if none): the last place `PlayerRanking` gives the
/// player, of the racers left (and one more in a knock-out); `r.0[0]`, `r.0[1]` and `r.0[5]`
/// are the original's loop registers.
fn player_place(c: &mut Cpu, r: &mut Saved, t: u32, place: usize, ra: u32) -> u32 {
    r.0[place] = 900;
    let m = &mut c.m;
    let knock_out = (m.r8(t + 5) != 0) as u32;
    let n = sx(m.r16(TOURNAMENT_MANAGER + RACERS) as u32).wrapping_add(knock_out);
    let n = sx(n);
    if n as i32 > 0 {
        r.0[5] = 0x8011_0000;
        r.0[1] = n;
        let mut k = 1u32;
        loop {
            r.0[0] = k;
            let v = port(c, r, 0x8003_3be4, ra, &[TOURNAMENT_MANAGER, sx(k)], fetourn::abi_player_ranking);
            if v << 16 == 0 {
                r.0[place] = k;
            }
            k += 1;
            r.0[0] = k;
            if (n as i32) < k as i32 {
                break;
            }
        }
    }
    r.0[place]
}

/// `tScreenCongrats::Cleanup` (SCREENCONGRATS.CPP, 0x800482e0): the 3D cars released, then the
/// screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, 0x8004_82f4);
}

fn cleanup(c: &mut Cpu, ra: u32) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_leaving(c, RELEASE_CARS, &[this], ra);
        port(c, &r, 0x8002_61c0, ra + 8, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenCongrats::GetShapeInfo` (SCREENCONGRATS.CPP, 0x8004830c): 43 permanent shapes of
/// "zcong%d" (22 of "zelim%d" if lost, by language); swapped in: the trophy's 32 (by the
/// player's place) unless the prize is a car or nothing, else the trophy's (the prize a car
/// with it), the ribbon's 10, or none.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, permanent, swap, name) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_name = abi::arg(c, 4);
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        r.0[7] = swap;
        r.0[0] = name;
        let m = &mut c.m;
        let t = tournament(m);
        r.0[4] = t;
        m.w32(this + FRAMES, 0);
        m.w32(this + SWAP_COUNT, 0);
        r.0[6] = swap_name;
        let lost = m.r32(this + LOST) != 0;
        m.w16(permanent, if lost { 22 } else { 43 });
        let file = if lost { LOST_NAME } else { CONGRATS_NAME };
        let language = m.r8(LANGUAGE) as u32;
        r.call_leaving(c, SPRINTF, &[SWAP_NAME, NAME_FORMAT, file, language], 0x8004_83ec);
        let m = &mut c.m;
        m.w32(name, SWAP_NAME);
        if m.r32(this + PRIZE).wrapping_sub(3) >= 2 {
            let place = player_place(c, &mut r, t, 2, 0x8004_8454);
            r.0[0] = TROPHY_NAME;
            c.m.w32(sp + 16, place);
            port(c, &r, 0x8003_3ca0, 0x8004_8498, &[TOURNAMENT_MANAGER, t, 2, TROPHY_NAME], fetourn::abi_get_trophy_name);
            let m = &mut c.m;
            m.w32(swap_name, TROPHY_NAME);
            m.w16(swap, 32);
            m.w32(this + SWAP_COUNT, 32);
            return;
        }
        match m.r32(this + PICTURE) {
            1 => {
                r.0[0] = TROPHY_NAME;
                m.w16(swap, 32);
                m.w32(this + FRAMES, 32);
                m.w32(sp + 16, u32::MAX);
                port(c, &r, 0x8003_3ca0, 0x8004_8508, &[TOURNAMENT_MANAGER, t, 0, TROPHY_NAME], fetourn::abi_get_trophy_name);
                c.m.w32(swap_name, TROPHY_NAME);
            }
            2 => {
                m.w16(swap, 10);
                m.w32(this + FRAMES, 10);
                m.w32(swap_name, RIBBON_NAME);
            }
            _ => {
                m.w32(swap_name, 0);
                m.w16(swap, 0);
            }
        }
    })
}

/// `tScreenCongrats::GetCar` (SCREENCONGRATS.CPP, 0x8004855c): the garage's car, in its colour.
pub fn abi_get_car(c: &mut Cpu) {
    let info = c.r[5];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        let car = c.m.r8(GARAGE_CAR) as u32;
        port(c, &r, 0x8001_7094, 0x8004_8588, &[CAR_MANAGER.0, car, info, 0], fecars::abi_get_garage_car);
        own_color(&mut c.m, info);
        c.r[2] = 1;
    })
}

/// A car's colour index made its colour.
fn own_color(m: &mut Mem, info: u32) {
    let k = m.r8(info + CAR_COLOR) as u32;
    let color = m.r8(info.wrapping_add(k) + CAR_COLORS);
    m.w8(info + CAR_COLOR, color);
}

/// `tScreenCongrats::DrawBackground` (SCREENCONGRATS.CPP, 0x800485b4): the kind's message; the
/// clock started once the car (if it is the prize) has loaded and stood a second; the money
/// counted into the total (faster once skipped or after a while, with a tick each step), the
/// speech after two seconds; the stars (twinkling now and then once grown) or the lost ones;
/// the trophy turning; the trophy's or ribbon's picture; the studs in the prize's colour; the
/// car turning.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 136, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        r.0[4] = 0;
        let m = &mut c.m;
        if m.r32(this + GOING_OFF) != 0 {
            r.0[4] = lh(m, this + FADE);
        }
        let shapes = m.r32(this + SWAP_TABLE);
        m.w32(sp + 60, shapes);
        m.w32(sp + 84, shapes);
        let angle = m.r32(ANGLE).wrapping_add(3);
        m.w32(ANGLE, angle);
        vcall(c, &r, this, DRAW_CONGRATS_MESSAGE, &[], 0x8004_8624);
        let m = &mut c.m;
        let prize = m.r32(this + PRIZE);
        let ready = prize != 3
            || (m.r32(this + SHOWN_AT) != u32::MAX
                && m.r32(this + HAS_CAR) != 0
                && (m.r32(ASYNC_LOADING) as i32) < 0
                && (ticks(m).wrapping_sub(m.r32(this + UP_AT)) as i32) >= 60);
        if !ready {
            let now = ticks(m);
            m.w32(this + SHOWN_AT, now);
        } else {
            draw_counting(c, &mut r, this, sp);
        }
        draw_studs(c, &mut r, this, sp);
    })
}

/// DrawBackground from the clock on: the money, the speech, the stars and pictures.
fn draw_counting(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) {
    let m = &mut c.m;
    if m.r32(this + SHOWN_AT) == u32::MAX {
        let now = ticks(m);
        m.w32(this + SHOWN_AT, now);
    }
    // the money
    let left = m.r32(this + TO_COUNT);
    if left != u32::MAX {
        let since = ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32;
        if since >= 1001 || m.r32(this + SKIPPED) != 0 {
            if left != 0 {
                r.call_leaving(c, PLAY_FE_SFX, &[21], 0x8004_8704);
            }
            let m = &mut c.m;
            let v = m.r32(this + TO_COUNT).wrapping_sub(m.r32(this + STEP));
            m.w32(this + TO_COUNT, v);
            if (v as i32) < 0 {
                m.w32(this + TO_COUNT, 0);
                m.w32(this + COUNTED, 1);
            }
        }
        let m = &mut c.m;
        let k = m.r8(MONEY_COLOR) as u32;
        let color = m.r32(TEXT_COLORS + (k << 2));
        let fade = lh(m, this + FADE);
        let gold = port(c, r, 0x8004_bd68, 0x8004_8750, &[color, fade], fefades::abi_fade_out);
        r.0[2] = gold;
        let fade = lh(&mut c.m, this + FADE);
        let shade = port(c, r, 0x8004_bd68, 0x8004_8764, &[0x0023_2323, fade], fefades::abi_fade_out);
        r.0[1] = shade;
        let rows = [
            (MONEY_LABEL, MONEY_AT, 6, [0x8004_877c, 0x8004_8784, 0x8004_8790, 0x8004_87ac]),
            (TOTAL_LABEL, TOTAL_AT, 9, [0x8004_87c0, 0x8004_87c8, 0x8004_87d4, 0x8004_87f8]),
        ];
        for (j, (label, at, kind, ras)) in rows.into_iter().enumerate() {
            let fade = lh(&mut c.m, this + FADE);
            port(c, r, 0x8002_1214, ras[0], &[fade, label, 2, 11], fetextrender::abi_menu_text_fade);
            let x = r.call_leaving(c, WORD_X, &[at], ras[1]);
            r.0[0] = x;
            let y = r.call_leaving(c, WORD_Y, &[at], ras[2]);
            let m = &mut c.m;
            let left = m.r32(this + TO_COUNT);
            let amount = if j == 0 { left } else { m.r32(this + TOTAL).wrapping_sub(left) };
            m.w32(sp + 16, gold);
            m.w32(sp + 20, shade);
            port(c, r, 0x8003_9a80, ras[3], &[x, y, kind, amount], screenpost::abi_draw_money);
        }
    }
    // the speech
    let m = &mut c.m;
    let speech = m.r32(this + SPEECH);
    if speech != 0 && ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32 >= 129 {
        r.call_leaving(c, PLAY_SPEECH, &[2, speech], 0x8004_882c);
        c.m.w32(this + SPEECH, 0);
    }
    // the stars
    r.0[0] = 0;
    let lost = c.m.r32(this + LOST) != 0;
    let (star_color, ras) = if lost { (0x0064_6464, [0x8004_8974, 0x8004_8988]) } else { (0xBEBE, [0x8004_884c, 0x8004_8860]) };
    let fade = if lost { r.0[4] } else { lh(&mut c.m, this + FADE) };
    let v = port(c, r, 0x8004_bd68, ras[0], &[star_color, fade], fefades::abi_fade_out);
    c.m.w32(sp + 44, v);
    let v = port(c, r, 0x8004_bd68, ras[1], &[0x0080_8080, r.0[4]], fefades::abi_fade_out);
    let m = &mut c.m;
    m.w32(sp + 68, v);
    let now = ticks(m) as i32;
    if lost {
        let v = now.wrapping_sub(m.r32(this + SHOWN_AT) as i32) / 4;
        m.w32(this + STARS, v as u32);
        if v >= 21 {
            m.w32(this + STARS, 20);
        }
    } else if m.r32(this + TWINKLING) != 0 {
        let v = now.wrapping_sub(m.r32(this + TWINKLED_AT) as i32) / 6 + 21;
        m.w32(this + STARS, v as u32);
        r.0[0] = 1;
        if v >= 42 {
            m.w32(this + TWINKLING, 0);
            m.w32(this + STARS, 20);
        }
    } else {
        let shown = m.r32(this + SHOWN_AT) as i32;
        let v = now.wrapping_sub(shown) / 2;
        m.w32(this + STARS, v as u32);
        if v >= 20 {
            r.0[0] = 1;
            m.w32(this + STARS, 20);
            if now.wrapping_sub(shown / 4) % 1500 < 45 {
                m.w32(this + TWINKLING, 1);
                m.w32(this + TWINKLED_AT, now as u32);
            }
        }
    }
    let m = &mut c.m;
    let y = if m.r32(this + LOST) == 1 { 10 } else { 0 };
    let stars = m.r32(this + STARS);
    if r.0[0] != 0 {
        r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[stars, 1040, 0, y, 0, 0, sp + 40], 0x8004_8a04);
    } else {
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[stars, 1040, 0, y, 0, 0, sp + 40], 0x8004_8a40);
    }
    // the trophy turning, the prize's picture
    let m = &mut c.m;
    let prize = m.r32(this + PRIZE);
    if prize.wrapping_sub(3) >= 2 {
        let frame = (ticks(m) as i32 / 12) % 32;
        r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[frame as u32, 1552, 70, 15, 0, 0, sp + 64], 0x8004_8ab0);
    }
    let m = &mut c.m;
    if m.r32(this + PRIZE) < 3 {
        return;
    }
    match m.r32(this + PICTURE) {
        1 => {
            let frame = (ticks(m) as i32 >> 3) % m.r32(this + FRAMES) as i32;
            r.call_leaving(c, SCALE_SHAPE_EXTENDED, &[frame as u32, 1552, 41, 190, 0, 0, sp + 64], 0x8004_8b40);
        }
        2 => {
            m.w32(sp + 44, 0x0055_1E00);
            let frame = (ticks(m) as i32 / 20) % m.r32(this + FRAMES) as i32;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[frame as u32, 1552, (-193i32) as u32, 86, 0, 0, sp + 40], 0x8004_8bc4);
        }
        _ => {}
    }
}

/// DrawBackground's end: the studs in the prize's colour (fewer when lost or the prize is a car),
/// the car turning.
fn draw_studs(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32) {
    let m = &mut c.m;
    let lost = m.r32(this + LOST) != 0;
    let prize = m.r32(this + PRIZE);
    let color = if lost {
        0x0003_0022
    } else {
        match prize as i32 {
            0 | 3 => 0x3E44,
            1 => 0x0021_2121,
            2 => 0x3044,
            _ => 0x0003_0022,
        }
    };
    let shape = if lost { 21 } else { 42 };
    r.0[1] = shape;
    let fade = lh(m, this + FADE);
    let v = port(c, r, 0x8004_bd68, 0x8004_8c4c, &[color, fade], fefades::abi_fade_out);
    c.m.w32(sp + 92, v);
    r.0[0] = 7;
    let rows: &[(u32, u32, u32)] = if !lost && prize != 3 {
        r.0[0] = 1;
        &[(1, 30, 0x8004_8cc0), (34, 63, 0x8004_8d24)]
    } else {
        &[(7, 40, 0x8004_8d90)]
    };
    r.0[2] = 0x5555_5556;
    r.0[4] = 1;
    for &(from, to, ra) in rows {
        for k in from..to {
            r.0[0] = k;
            if k % 3 != 0 {
                let fade = lh(&mut c.m, this + FADE);
                r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[shape, 1040, 2 * k, 0, fade, 1, sp + 88], ra);
            }
        }
        r.0[0] = to;
    }
    // the car
    let m = &mut c.m;
    if m.r32(this + PRIZE) == 3 && m.r32(this + HAS_CAR) != 0 {
        m.w32(DRAW_CAR_FLAG, 0);
        let (x, y) = (lh(m, this + CAR_X), lh(m, this + CAR_Y));
        let (distance, height) = (m.r32(this + CAR_DISTANCE), m.r32(this + CAR_HEIGHT));
        let (angle, player) = (m.r32(ANGLE), m.r32(this + PLAYER));
        r.call_ptr_leaving(c, DRAW_CAR, &[this + CAR, x, y, distance, height, 128, 1, angle, player], 0x8004_8e08);
    }
}

/// `tScreenCongrats::DrawForeground` (SCREENCONGRATS.CPP, 0x80048e28): nothing.
pub fn abi_draw_foreground(_: &mut Cpu) {}

/// `tScreenCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80048e30): nothing won, no money;
/// the car's place.
pub fn abi_calculate_prizes(c: &mut Cpu) {
    let this = c.r[4];
    let m = &mut c.m;
    m.w32(this + PRIZE, 4);
    m.w32(this + TO_COUNT, u32::MAX);
    m.w16(this + CAR_X, 278);
    m.w32(this + LOST, 0);
    m.w16(this + CAR_Y, 63);
    m.w32(this + CAR_HEIGHT, 0xC0EC_CCCD);
    let lost = m.r32(this + LOST);
    m.w32(this + PICTURE, 0);
    m.w32(this + PLAYER, 0);
    m.w32(this + TOTAL, 0);
    m.w32(this + CAR_DISTANCE, 0x4080_0000);
    car_place(m, this, lost);
    c.r[2] = 1;
    c.r[3] = lost;
    c.r[5] = 0x4080_0000;
}

/// The car further off when lost.
fn car_place(m: &mut Mem, this: u32, lost: u32) {
    if lost == 1 {
        m.w16(this + CAR_X, 288);
        m.w16(this + CAR_Y, 73);
        m.w32(this + CAR_HEIGHT, 0xC103_3333);
    }
}

/// The car's place for a car won.
fn car_won_place(m: &mut Mem, this: u32) {
    m.w16(this + CAR_X, 278);
    m.w16(this + CAR_Y, 75);
    m.w32(this + CAR_DISTANCE, 0x4080_0000);
    m.w32(this + CAR_HEIGHT, 0xC0EC_CCCD);
}

/// `tScreenCongrats::Initialize` (SCREENCONGRATS.CPP, 0x80048ea4): the clocks reset, the
/// licence plate, the prizes, the car; the money counted in 64 steps (none: counted); the
/// screen's.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        let now = ticks(m);
        m.w32(this + SPEECH, 0);
        m.w32(this + SHOWN_AT, u32::MAX);
        m.w32(this + STARS, u32::MAX);
        m.w32(this + TWINKLING, 0);
        m.w32(this + UP_AT, now);
        r.call_leaving(c, SET_LICENSE_PLATE, &[], 0x8004_8ed8);
        vcall(c, &r, this, CALCULATE_PRIZES, &[], 0x8004_8ef4);
        let has = vcall(c, &r, this, GET_CAR, &[this + CAR], 0x8004_8f10);
        let m = &mut c.m;
        m.w32(this + HAS_CAR, has);
        let left = m.r32(this + TO_COUNT) as i32;
        let step = if left > 0 {
            m.w32(this + COUNTED, 0);
            m.w32(this + SKIPPED, 0);
            left / 64
        } else {
            m.w32(this + COUNTED, 1);
            m.w32(this + SKIPPED, 1);
            1000
        };
        m.w32(this + STEP, step as u32);
        port(c, &r, 0x8002_60cc, 0x8004_8f58, &[this], fescreen::abi_initialize);
    })
}

/// `tScreenCongrats::ProcessInput` (SCREENCONGRATS.CPP, 0x80048f68): keys ignored (made none)
/// while the narration speaks or for the first 150 ticks; back passes.
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    let m = &mut c.m;
    if m.r32(key) == 4 {
        return;
    }
    let ignore = m.r16(SPEAKING) != 0 || (ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32) < 150;
    c.r[5] = ignore as u32;
    if ignore {
        m.w32(key, 1);
    }
}

/// `tScreenPinkSlipCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x80048fc0): the winner
/// taking the loser's car (the words in the language's order), and the winner's number.
pub fn abi_pink_slip_draw_congrats_message(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 312, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        let rect = sp + 24;
        for (k, v) in [41u16, 60, 420, 200].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        let language = c.m.r8(LANGUAGE) as u32;
        let car_name = |c: &mut Cpu, r: &Saved, ra: u32| {
            let id = c.m.r8(this + CAR + CAR_ID) as i8 as i32 as u32;
            r.call_leaving(c, TEXT_WORD, &[id.wrapping_add(CAR_NAMES)], ra)
        };
        let loser = |c: &mut Cpu, r: &Saved, ra: u32| {
            let w = lh(&mut c.m, this + WINNER);
            r.call_leaving(c, PLAYER_NAME, &[1u32.wrapping_sub(w)], ra)
        };
        let last;
        if language.wrapping_sub(2) < 2 {
            r.0[2] = r.call_leaving(c, TEXT_WORD, &[PINK_MESSAGE], 0x8004_9018);
            let w = lh(&mut c.m, this + WINNER);
            r.0[1] = r.call_leaving(c, PLAYER_NAME, &[w], 0x8004_9024);
            r.0[0] = car_name(c, &r, 0x8004_903c);
            last = loser(c, &r, 0x8004_9048);
        } else {
            r.0[2] = r.call_leaving(c, TEXT_WORD, &[PINK_MESSAGE], 0x8004_9058);
            let w = lh(&mut c.m, this + WINNER);
            r.0[1] = r.call_leaving(c, PLAYER_NAME, &[w], 0x8004_9064);
            r.0[0] = loser(c, &r, 0x8004_9078);
            last = car_name(c, &r, 0x8004_9088);
        }
        let number = lh(&mut c.m, this + WINNER).wrapping_add(1);
        let text = sp + 32;
        r.call_leaving(c, SPRINTF, &[text, r.0[2], r.0[1], r.0[0], last, number], 0x8004_90ac);
        port(c, &r, 0x8002_1948, 0x8004_90c0, &[text, rect, 2, 10], fetextrender::abi_word_wrap_text);
    })
}

/// `tScreenPinkSlipCongrats::GetCar` (SCREENCONGRATS.CPP, 0x800490dc): the loser's car, in its
/// colour.
pub fn abi_pink_slip_get_car(c: &mut Cpu) {
    let (this, info) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        let m = &mut c.m;
        let w = m.r16(this + WINNER) as u32;
        let car = m.r8(PINK_SLIPS_CAR.wrapping_add(1u32.wrapping_sub(sx(w)))) as u32;
        let player = sx(1u32.wrapping_sub(w));
        port(c, &r, 0x8001_71a0, 0x8004_9128, &[CAR_MANAGER.0, car, info, player], fecars::abi_get_pink_slips_car);
        own_color(&mut c.m, info);
        c.r[2] = 1;
    })
}

/// `tScreenPinkSlipCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049154): the loser's car
/// won (with the ribbon), no money; the loser's licence plate made; the car's speech (by its
/// own or the player).
pub fn abi_pink_slip_calculate_prizes(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        r.0[2] = u32::MAX;
        let m = &mut c.m;
        let w = lh(m, this + WINNER);
        m.w32(this + PRIZE, 3);
        m.w32(this + PICTURE, 2);
        r.0[0] = 1;
        m.w32(this + TOTAL, 0);
        m.w32(this + TO_COUNT, u32::MAX);
        m.w32(this + LOST, 0);
        let loser = 1u32.wrapping_sub(w);
        r.0[1] = loser;
        let w = lh(m, this + WINNER);
        m.w32(this + PLAYER, 1u32.wrapping_sub(w));
        r.call_leaving(c, CLEAN_UP_LICENSE, &[loser], 0x8004_91ac);
        r.0[0] = 1u32.wrapping_sub(loser) << 3;
        r.call_leaving(c, CREATE_LICENSE, &[PLATES.wrapping_add(r.0[0]), 0, loser], 0x8004_91d0);
        let info = sp + 16;
        vcall(c, &r, this, GET_CAR, &[info], 0x8004_91ec);
        let m = &mut c.m;
        let speech = m.r8(info + CAR_SPEECH) as i8 as i32 as u32;
        let w = lh(m, this + WINNER);
        let v = if speech != u32::MAX { (speech << 1).wrapping_add(w.wrapping_add(19)) } else { w.wrapping_add(23) };
        m.w32(this + SPEECH, v);
        car_won_place(m, this);
    })
}

/// `tScreenPinkSlipCongrats::Initialize` (SCREENCONGRATS.CPP, 0x80049264): the winner (the
/// first player if they won more than half the tracks, else the second), then the screen's.
pub fn abi_pink_slip_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let m = &mut c.m;
        m.w16(this + WINNER, 0xFFFF);
        let half = (m.r8(PINK_COUNT) as i32 >> 1) + 1;
        let wins = m.r8(PINK_WINS) as i32;
        m.w16(this + WINNER, if wins < half { 1 } else { 0 });
        port(c, &r, 0x8004_8ea4, 0x8004_92ac, &[this], abi_initialize);
    })
}

/// `tScreenPinkSlipCongrats::Cleanup` (SCREENCONGRATS.CPP, 0x800492bc): as the screen's.
pub fn abi_pink_slip_cleanup(c: &mut Cpu) {
    cleanup(c, 0x8004_92d0);
}

/// `tScreenTournamentTrophy::ProcessInput` (SCREENCONGRATS.CPP, 0x800492e8): accept while the
/// money still counts skips the counting.
pub fn abi_trophy_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    let args = [c.r[4], c.r[5], c.r[6], c.r[7]];
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if m.r32(key) == 2 && m.r32(this + COUNTED) == 0 {
            m.w32(key, 1);
            m.w32(this + SKIPPED, 1);
        }
        port(c, &Saved::of(c), 0x8004_8f68, 0x8004_9320, &args, abi_process_input);
    })
}

/// `tScreenTournamentTrophy::GetCar` (SCREENCONGRATS.CPP, 0x80049330): the car won, in its
/// colour and upgrades (lost: the garage's).
pub fn abi_trophy_get_car(c: &mut Cpu) {
    let (this, info) = (c.r[4], c.r[5]);
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        if c.m.r32(this + LOST) == 0 {
            let award = sp + 16;
            port(c, &r, 0x8003_3a10, 0x8004_935c, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
            let id = lh(&mut c.m, award + AWARD_CAR_MODEL);
            let car = port(c, &r, 0x8001_6374, 0x8004_936c, &[CAR_MANAGER.0, id], fecars::abi_get_car_from_id);
            r.call_leaving(c, BLOCKMOVE, &[car, info, CAR_SIZE], 0x8004_937c);
            let m = &mut c.m;
            let upgrades = m.r8(award + AWARD_CAR_UPGRADES);
            m.w8(info + CAR_UPGRADES, upgrades);
            let color = m.r8(award + AWARD_CAR_COLOR);
            m.w8(info + CAR_COLOR, color);
        } else {
            let car = c.m.r8(GARAGE_CAR) as u32;
            port(c, &r, 0x8001_7094, 0x8004_93b0, &[CAR_MANAGER.0, car, info, 0], fecars::abi_get_garage_car);
            own_color(&mut c.m, info);
        }
        c.r[2] = 1;
    })
}

/// `tScreenTournamentTrophy::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x800493dc): lost: "LOST"
/// in grey. Else (best placed fourth or worse before) the trophy's and the prize's words, (won
/// the car, or not first before) what winning brings, a word-wrapped paragraph or two; the
/// garage-full words if the car's money was given instead.
pub fn abi_trophy_draw_congrats_message(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 1456, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = this;
        let rect = sp + 24;
        for (k, v) in [41u16, 53, 420, 400].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        if c.m.r32(this + LOST) != 0 {
            let y = r.call_leaving(c, WORD_Y, &[LOST_TEXT], 0x8004_9668);
            r.0[0] = y.wrapping_sub(10);
            let w = r.call_leaving(c, TEXT_WORD, &[LOST_TEXT], 0x8004_9674);
            r.0[0] <<= 16;
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, 288, sx(y.wrapping_sub(10)), 0x0041_4141, 3, 2], 0x8004_96a0);
            return;
        }
        let (first, second) = (sp + 104, sp + 608);
        r.0[2] = 0;
        r.0[3] = 0;
        let t = tournament(&mut c.m);
        r.0[1] = t;
        let m = &mut c.m;
        let prize = m.r32(this + PRIZE) as i32;
        if (m.r8(TOURNAMENT_MANAGER + LAST_BEST) as i8) >= 4 {
            let id = m.r8(t) as i8 as i32 as u32;
            r.0[2] = id.wrapping_add(TROPHY_TEXTS);
            let k = match prize {
                1 => 1,
                0 | 3 => 0,
                _ => 2,
            };
            r.0[0] = k;
            let a = r.call_leaving(c, TEXT_WORD, &[id.wrapping_add(TROPHY_TEXTS)], 0x8004_94e0);
            r.0[0] = a;
            let b = r.call_leaving(c, TEXT_WORD, &[k + PRIZE_TEXTS], 0x8004_94ec);
            r.call_leaving(c, SPRINTF, &[first, a, b], 0x8004_94fc);
        }
        let m = &mut c.m;
        let prize = m.r32(this + PRIZE);
        if (prize == 0 && m.r8(TOURNAMENT_MANAGER + LAST_BEST) as i8 != 1) || prize == 3 {
            let id = m.r8(t) as i8 as i32 as u32;
            let text = id.wrapping_add(WIN_TEXTS);
            if r.0[2] == 0 {
                r.0[2] = text;
                let w = r.call_leaving(c, TEXT_WORD, &[text], 0x8004_9544);
                r.call_leaving(c, SPRINTF, &[first, w], 0x8004_956c);
            } else {
                r.0[3] = text;
                let w = r.call_leaving(c, TEXT_WORD, &[text], 0x8004_9560);
                r.call_leaving(c, SPRINTF, &[second, w], 0x8004_956c);
            }
        }
        if r.0[2] != 0 {
            r.0[0] = first;
            let fade = lh(&mut c.m, this + FADE);
            r.call_leaving(c, word_wrap_text_fade_entry, &[fade, first, rect, 2, 10], 0x8004_9590);
            let width = lh(&mut c.m, rect + 4);
            let h = port(c, &r, 0x8002_1a28, 0x8004_959c, &[width, first], fetextrender::abi_word_wrap_height);
            let m = &mut c.m;
            let y = (m.r16(rect + 2) as u32).wrapping_sub(14).wrapping_add(h);
            m.w16(rect + 2, y as u16);
        }
        if r.0[3] != 0 {
            c.m.w16(rect + 4, 200);
            let fade = lh(&mut c.m, this + FADE);
            r.call_leaving(c, word_wrap_text_fade_entry, &[fade, second, rect, 2, 10], 0x8004_95d8);
        }
        let award = sp + 32;
        port(c, &r, 0x8003_3a10, 0x8004_95e8, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
        if c.m.r32(award + AWARD_CAR_FULL) != 0 {
            let money = sp + 1368;
            r.0[0] = money;
            let m = &mut c.m;
            let amount = m.r32(award + AWARD_CAR_MONEY);
            for (k, v) in [260u16, 200, 240].into_iter().enumerate() {
                m.w16(rect + 2 * k as u32, v);
            }
            port(c, &r, 0x8002_6d00, 0x8004_961c, &[money, amount], fetools::abi_format_money);
            let w = r.call_leaving(c, TEXT_WORD, &[GARAGE_FULL], 0x8004_9624);
            r.call_leaving(c, SPRINTF, &[sp + 1112, w, money], 0x8004_9634);
            let w = r.call_leaving(c, TEXT_WORD, &[GARAGE_FULL], 0x8004_963c);
            let fade = lh(&mut c.m, this + FADE);
            r.call_leaving(c, word_wrap_text_fade_entry, &[fade, w, rect, 1, 10], 0x8004_9658);
        }
    })
}

use fetextrender::word_wrap_text_fade_entry;

/// `tScreenTournamentTrophy::CalculatePrizes` (SCREENCONGRATS.CPP, 0x800496c0): by the player's
/// place: first the car (if one is won) or gold, second silver and third bronze (each with its
/// speech) unless knocked out, else lost; the money won counted into the total (less the car's
/// money when the garage was full); the car's place.
pub fn abi_trophy_calculate_prizes(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 120, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[4] = 0x8011_0000;
        r.0[0] = TOURNAMENT_MANAGER;
        let m = &mut c.m;
        m.w32(this + LOST, 1);
        m.w32(this + PRIZE, 4);
        m.w32(this + PICTURE, 2);
        let award = sp + 16;
        port(c, &r, 0x8003_3a10, 0x8004_9710, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
        let t = tournament(&mut c.m);
        r.0[5] = t;
        let place = player_place_trophy(c, &mut r, t);
        let m = &mut c.m;
        let (lost, prize, picture) = if place == 1 {
            let won = if m.r32(award + AWARD_CAR) != 0 { (3, place) } else { (0, 0) };
            m.w32(this + SPEECH, 15);
            (0, won.0, won.1)
        } else if (place as i32) <= 0 || place as i32 >= 4 || m.r8(t + 5) != 0 {
            (1, 3, 0)
        } else {
            m.w32(this + SPEECH, place.wrapping_add(14));
            (0, if place == 2 { 1 } else { 2 }, 0)
        };
        m.w32(this + LOST, lost);
        m.w32(this + PRIZE, prize);
        m.w32(this + PICTURE, picture);
        m.w32(this + PLAYER, 0);
        let money = m.r32(TOURNAMENT_MANAGER + MONEY);
        m.w32(this + TOTAL, money);
        if m.r32(award + COMPLETED_FULL) != 0 {
            let v = money.wrapping_sub(m.r32(award + COMPLETED_MONEY));
            m.w32(this + TOTAL, v);
        }
        let won = m.r32(award + AWARD_TOURN_MONEY);
        m.w32(this + TO_COUNT, if won == 0 { u32::MAX } else { won });
        m.w16(this + CAR_X, 278);
        m.w16(this + CAR_Y, 63);
        m.w32(this + CAR_HEIGHT, 0xC0EC_CCCD);
        m.w32(this + CAR_DISTANCE, 0x4080_0000);
        let lost = m.r32(this + LOST);
        car_place(m, this, lost);
    })
}

/// The player's place as `CalculatePrizes` finds it (its loop's registers: s1 the place tried,
/// s0 how many, s3 the place).
fn player_place_trophy(c: &mut Cpu, r: &mut Saved, t: u32) -> u32 {
    r.0[3] = 900;
    let m = &mut c.m;
    let knock_out = (m.r8(t + 5) != 0) as u32;
    let n = sx(sx(m.r16(TOURNAMENT_MANAGER + RACERS) as u32).wrapping_add(knock_out));
    r.0[1] = 1;
    if n as i32 > 0 {
        r.0[0] = n;
        let mut k = 1u32;
        loop {
            r.0[1] = k;
            let v = port(c, r, 0x8003_3be4, 0x8004_978c, &[TOURNAMENT_MANAGER, sx(k)], fetourn::abi_player_ranking);
            if v << 16 == 0 {
                r.0[3] = k;
            }
            k += 1;
            r.0[1] = k;
            if (n as i32) < k as i32 {
                break;
            }
        }
    }
    r.0[3]
}

/// `tScreenBeTheCopCongrats::GetCar` (SCREENCONGRATS.CPP, 0x80049900): the cop car, in its
/// colour.
pub fn abi_cop_get_car(c: &mut Cpu) {
    let info = c.r[5];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = info;
        r.0[0] = FRONT_END;
        let id = c.m.r8(COP_CAR) as u32;
        let car = port(c, &r, 0x8001_6374, 0x8004_992c, &[CAR_MANAGER.0, id], fecars::abi_get_car_from_id);
        r.call_leaving(c, BLOCKMOVE, &[car, info, CAR_SIZE], 0x8004_993c);
        let color = c.m.r8(COP_CAR_COLOR);
        c.m.w8(info + COP_COLOR, color);
        c.r[2] = 1;
    })
}

/// `tScreenBeTheCopCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049960): the screen's,
/// then the car won.
pub fn abi_cop_calculate_prizes(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8004_8e30, 0x8004_9974, &[this], abi_calculate_prizes);
        let m = &mut c.m;
        m.w32(this + PRIZE, 3);
        car_won_place(m, this);
    })
}

/// `tScreenBeTheCopCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x800499b0): the cop car
/// earned (the last one's own words), with its name.
pub fn abi_cop_draw_congrats_message(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 296, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = COP_MESSAGE;
        let rect = sp + 16;
        for (k, v) in [41u16, 60, 420, 200].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        if c.m.r8(COP_CAR) == 28 {
            r.0[0] = COP_MESSAGE + 1;
        }
        let name = |c: &mut Cpu, r: &Saved, ra: u32| {
            let id = c.m.r8(this + CAR + CAR_ID) as i8 as i32 as u32;
            r.call_leaving(c, TEXT_WORD, &[id.wrapping_add(CAR_NAMES)], ra)
        };
        if r.call_leaving(c, PAD_BUTTONS, &[4], 0x8004_9a04) & 0xFFFF != 0 {
            name(c, &r, 0x8004_9a1c);
        }
        let w = r.call_leaving(c, TEXT_WORD, &[r.0[0]], 0x8004_9a24);
        r.0[1] = w;
        let car = name(c, &r, 0x8004_9a34);
        let text = sp + 24;
        r.0[0] = text;
        r.call_leaving(c, SPRINTF, &[text, w, car], 0x8004_9a48);
        port(c, &r, 0x8002_1948, 0x8004_9a5c, &[text, rect, 2, 10], fetextrender::abi_word_wrap_text);
    })
}

/// `tScreenTournamentCongrats::GetCar` (SCREENCONGRATS.CPP, 0x80049a70): the tier's car.
pub fn abi_tier_get_car(c: &mut Cpu) {
    let info = c.r[5];
    abi::frame(c, 96, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        let award = sp + 16;
        port(c, &r, 0x8003_3a10, 0x8004_9a90, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
        let id = lh(&mut c.m, award + COMPLETED_CAR);
        let car = port(c, &r, 0x8001_6374, 0x8004_9aa0, &[CAR_MANAGER.0, id], fecars::abi_get_car_from_id);
        r.call_leaving(c, BLOCKMOVE, &[car, info, CAR_SIZE], 0x8004_9ab0);
        c.r[2] = 1;
    })
}

/// `tScreenTournamentCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049ac4): the tier's
/// car won, the total; the car's money counted in if the garage was full.
pub fn abi_tier_calculate_prizes(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = TOURNAMENT_MANAGER;
        let award = sp + 16;
        port(c, &r, 0x8003_3a10, 0x8004_9aec, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
        port(c, &r, 0x8004_8e30, 0x8004_9af4, &[this], abi_calculate_prizes);
        let m = &mut c.m;
        m.w32(this + PRIZE, 3);
        let money = m.r32(TOURNAMENT_MANAGER + MONEY);
        m.w32(this + TOTAL, money);
        let left = if m.r32(award + COMPLETED_FULL) != 0 { m.r32(award + COMPLETED_MONEY) } else { u32::MAX };
        m.w32(this + TO_COUNT, left);
        car_won_place(m, this);
    })
}

/// `tScreenTournamentCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x80049b5c): the tier's
/// text; the garage-full words if the car's money was given instead.
pub fn abi_tier_draw_congrats_message(c: &mut Cpu) {
    abi::frame(c, 424, |c, sp| {
        let mut r = Saved::of(c);
        let rect = sp + 16;
        for (k, v) in [41u16, 60, 420, 400].into_iter().enumerate() {
            c.m.w16(rect + 2 * k as u32, v);
        }
        let award = sp + 24;
        port(c, &r, 0x8003_3a10, 0x8004_9b98, &[TOURNAMENT_MANAGER, award], fetourn::abi_get_award_information);
        let id = lh(&mut c.m, award + COMPLETED_TEXT);
        let w = r.call_leaving(c, TEXT_WORD, &[id], 0x8004_9ba4);
        port(c, &r, 0x8002_1948, 0x8004_9bb8, &[w, rect, 2, 10], fetextrender::abi_word_wrap_text);
        if c.m.r32(award + COMPLETED_FULL) != 0 {
            let money = sp + 352;
            r.0[0] = money;
            let m = &mut c.m;
            let amount = m.r32(award + COMPLETED_MONEY);
            m.w16(rect + 2, 180);
            m.w16(rect + 4, 430);
            port(c, &r, 0x8002_6d00, 0x8004_9be4, &[money, amount], fetools::abi_format_money);
            let w = r.call_leaving(c, TEXT_WORD, &[GARAGE_FULL], 0x8004_9bec);
            r.call_leaving(c, SPRINTF, &[sp + 96, w, money], 0x8004_9bfc);
            let w = r.call_leaving(c, TEXT_WORD, &[GARAGE_FULL], 0x8004_9c04);
            port(c, &r, 0x8002_1948, 0x8004_9c18, &[w, rect, 2, 10], fetextrender::abi_word_wrap_text);
        }
    })
}

/// The destructors (SCREENCONGRATS.H): `~tScreenTournamentCongrats` (0x80049c28),
/// `~tScreenBeTheCopCongrats` (0x80049c48), `~tScreenPinkSlipCongrats` (0x80049c68),
/// `~tScreenTournamentTrophy` (0x80049c88), `~tScreenCongrats` (0x80049ca8): the screen's.
fn destructor(c: &mut Cpu, ra: u32) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, ra, &[this, flags], fescreen::abi_screen_destructor);
    })
}

pub fn abi_tier_destructor(c: &mut Cpu) {
    destructor(c, 0x8004_9c38);
}

pub fn abi_cop_destructor(c: &mut Cpu) {
    destructor(c, 0x8004_9c58);
}

pub fn abi_pink_slip_destructor(c: &mut Cpu) {
    destructor(c, 0x8004_9c78);
}

pub fn abi_trophy_destructor(c: &mut Cpu) {
    destructor(c, 0x8004_9c98);
}

pub fn abi_destructor(c: &mut Cpu) {
    destructor(c, 0x8004_9cb8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The game's congratulations screens (their vtables the real ones): a tier's, Be the Cop's,
    /// the pink slips' and the tournament's trophy; outputs, a car.
    const TIER_SCREEN: u32 = 0x8018_14DC;
    const COP_SCREEN: u32 = 0x8018_1358;
    const PINK_SCREEN: u32 = 0x8018_0DE0;
    const TROPHY_SCREEN: u32 = 0x8018_01CC;
    const SCREENS: [u32; 4] = [TIER_SCREEN, COP_SCREEN, PINK_SCREEN, TROPHY_SCREEN];
    const OUT: u32 = 0x8014_7000;
    const CAR_OUT: u32 = 0x8014_7400;
    /// 64 shapes (each the permanent table's first, moved right by its number so each draws
    /// apart), for the screens' permanent and swapped shapes: the screen up in the RAM image has
    /// fewer than the studs and stars index.
    const SHAPES: u32 = 0x8014_6000;
    /// The permanent shapes' table now (FESCREEN's).
    const PERMANENT_SHAPES: u32 = 0x8005_1C54;
    /// The tournament manager's award.
    const AWARD: u32 = TOURNAMENT_MANAGER + 576;
    /// `tCarManager::GetCarFromID`.
    const CAR_FROM_ID: fn(&mut Cpu) = nfs4rc::f_80016374;

    fn screen(rng: &mut Rng) -> u32 {
        SCREENS[pick(rng, 4) as usize]
    }

    /// FETOURN's (FEMENU's structures and the tournament manager), then the screens: the car
    /// select's swapped shapes (their table and the permanent one [`SHAPES`]); going off, the
    /// fade, lost, the prize and its picture, its frames, the money (none, counted, some, the
    /// step all of it), the clocks about their limits (not started, the car just up, counting
    /// slow or fast, the speech due, the stars grown, twinkling, the clocks behind it and gone
    /// round), a car of the game's, the pink slips' winner and cars; the cars loading or not, the
    /// narration, the language, the pink slips' tracks and wins, the cop car (the last one or
    /// not), the last best place, the award (cars of the game's, texts of the game's).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::fetourn::tests::shuffle(c, rng);
        let m = &mut c.m;
        m.w32(GAME_TICKS, [100_000 + pick(rng, 1_000_000), 0x8000_0000 + pick(rng, 1_000_000)][(pick(rng, 4) == 0) as usize]);
        let now = ticks(m);
        let app = m.r32(FE_APP);
        let from = m.r32(app + 12) + SWAP_TABLE;
        let first = m.r32(PERMANENT_SHAPES);
        for k in 0..64 * 32 {
            let v = m.r8(first + k % 32);
            m.w8(SHAPES + k, v);
        }
        for k in 0..64 {
            m.w16(SHAPES + 32 * k + 20, k as u16);
        }
        m.w32(PERMANENT_SHAPES, SHAPES);
        for s in SCREENS {
            for k in (0..40).step_by(4) {
                let v = m.r32(from + k);
                m.w32(s + SWAP_TABLE + k, v);
            }
            m.w32(s + SWAP_TABLE, SHAPES);
        }
        for s in SCREENS {
            let m = &mut c.m;
            m.w32(s + GOING_OFF, pick(rng, 2));
            m.w16(s + FADE, [0, 64, 128, pick(rng, 129)][pick(rng, 4) as usize] as u16);
            m.w32(s + LOST, [0, 0, 1, 2][pick(rng, 4) as usize]);
            m.w32(s + PRIZE, [0, 1, 2, 3, 3, 4, 5][pick(rng, 7) as usize]);
            m.w32(s + PICTURE, [0, 1, 2, 2, 3][pick(rng, 5) as usize]);
            m.w32(s + SWAP_COUNT, pick(rng, 40));
            m.w32(s + FRAMES, 1 + pick(rng, 32));
            m.w32(s + PLAYER, pick(rng, 2));
            m.w32(s + TOTAL, pick(rng, 100_000));
            m.w32(s + TO_COUNT, [u32::MAX, 0, 1, 63, 64, pick(rng, 50_000)][pick(rng, 6) as usize]);
            m.w32(s + STARS, pick(rng, 50));
            let back = [0, 1, 39, 40, 59, 60, 128, 129, 149, 150, 1000, 1001, pick(rng, 5000), pick(rng, 100_000), 0u32.wrapping_sub(1 + pick(rng, 100)), now.wrapping_add(1 + pick(rng, 100))];
            let shown = now.wrapping_sub(back[pick(rng, back.len() as u32) as usize]);
            m.w32(s + SHOWN_AT, [u32::MAX, shown, shown][pick(rng, 3) as usize]);
            m.w32(s + TWINKLING, pick(rng, 2));
            m.w32(s + TWINKLED_AT, now.wrapping_sub(pick(rng, 160)));
            m.w32(s + HAS_CAR, [0, 1, 1][pick(rng, 3) as usize]);
            m.w32(s + UP_AT, now.wrapping_sub([0, 59, 60, pick(rng, 200)][pick(rng, 4) as usize]));
            m.w32(s + COUNTED, pick(rng, 2));
            m.w32(s + SKIPPED, pick(rng, 2));
            let left = m.r32(s + TO_COUNT);
            m.w32(s + STEP, [pick(rng, 2000), left, 1][pick(rng, 3) as usize]);
            m.w32(s + SPEECH, [0, 15, pick(rng, 40)][pick(rng, 3) as usize]);
            m.w16(s + CAR_X, (270 + pick(rng, 20)) as u16);
            m.w16(s + CAR_Y, (60 + pick(rng, 20)) as u16);
            m.w32(s + CAR_DISTANCE, 0x4080_0000);
            m.w32(s + CAR_HEIGHT, [0xC0EC_CCCD, 0xC103_3333][pick(rng, 2) as usize]);
            m.w16(s + WINNER, pick(rng, 2) as u16);
            let id = pick(rng, 30);
            let car = crate::abi::call(c, CAR_FROM_ID, &[CAR_MANAGER.0, id]);
            for k in 0..CAR_SIZE {
                let v = c.m.r8(car + k);
                c.m.w8(s + CAR + k, v);
            }
            // (DrawCar's 3D cars aren't in the RAM image: a car drawn has no model)
            if c.m.r32(s + HAS_CAR) != 0 {
                c.m.w8(s + CAR + CAR_ID, 0xFF);
            }
        }
        let m = &mut c.m;
        m.w32(ASYNC_LOADING, [u32::MAX, 0, 1][pick(rng, 3) as usize]);
        m.w16(SPEAKING, [0, 0, 1][pick(rng, 3) as usize]);
        m.w32(ANGLE, pick(rng, 4096));
        m.w8(LANGUAGE, pick(rng, 5) as u8);
        m.w8(PINK_COUNT, pick(rng, 8) as u8);
        m.w8(PINK_WINS, pick(rng, 8) as u8);
        m.w8(COP_CAR, [28, pick(rng, 30)][pick(rng, 2) as usize] as u8);
        m.w8(COP_CAR_COLOR, pick(rng, 8) as u8);
        m.w8(PINK_SLIPS_CAR, pick(rng, 3) as u8);
        m.w8(PINK_SLIPS_CAR + 1, pick(rng, 3) as u8);
        m.w8(TOURNAMENT_MANAGER + LAST_BEST, [0xFF, 0, 1, 2, 3, 4, 5][pick(rng, 7) as usize]);
        m.w32(AWARD + AWARD_TOURN_MONEY, [0, pick(rng, 20_000)][pick(rng, 2) as usize]);
        m.w32(AWARD + AWARD_CAR, pick(rng, 2));
        m.w32(AWARD + AWARD_CAR_FULL, pick(rng, 2));
        m.w32(AWARD + AWARD_CAR_MONEY, pick(rng, 50_000));
        m.w32(AWARD + AWARD_CAR_MODEL, pick(rng, 30));
        m.w8(AWARD + AWARD_CAR_COLOR, pick(rng, 8) as u8);
        m.w8(AWARD + AWARD_CAR_UPGRADES, pick(rng, 8) as u8);
        m.w32(AWARD + COMPLETED_TEXT, [64, 630, 801, 948 + pick(rng, 30)][pick(rng, 4) as usize]);
        m.w32(AWARD + COMPLETED_CAR, pick(rng, 30));
        m.w32(AWARD + COMPLETED_FULL, pick(rng, 2));
        m.w32(AWARD + COMPLETED_MONEY, pick(rng, 50_000));
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenCongrats::Cleanup", 0x8004_82e0, nfs4rc::f_800482e0, |_, r| vec![screen(r)]),
            ("tScreenCongrats::GetShapeInfo", 0x8004_830c, nfs4rc::f_8004830c, |_, r| vec![screen(r), OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenCongrats::GetCar", 0x8004_855c, nfs4rc::f_8004855c, |_, r| vec![screen(r), CAR_OUT]),
            ("tScreenCongrats::DrawBackground", 0x8004_85b4, nfs4rc::f_800485b4, |_, r| vec![screen(r)]),
            ("tScreenCongrats::DrawForeground", 0x8004_8e28, nfs4rc::f_80048e28, |_, r| vec![screen(r)]),
            ("tScreenCongrats::CalculatePrizes", 0x8004_8e30, nfs4rc::f_80048e30, |_, r| vec![screen(r)]),
            ("tScreenCongrats::Initialize", 0x8004_8ea4, nfs4rc::f_80048ea4, |_, r| vec![screen(r)]),
            ("tScreenCongrats::ProcessInput", 0x8004_8f68, nfs4rc::f_80048f68, |c, r| {
                c.m.w32(OUT, pick(r, 6));
                vec![screen(r), 0, OUT, OUT + 4]
            }),
            ("tScreenPinkSlipCongrats::DrawCongratsMessage", 0x8004_8fc0, nfs4rc::f_80048fc0, |_, _| vec![PINK_SCREEN]),
            ("tScreenPinkSlipCongrats::GetCar", 0x8004_90dc, nfs4rc::f_800490dc, |_, _| vec![PINK_SCREEN, CAR_OUT]),
            ("tScreenPinkSlipCongrats::CalculatePrizes", 0x8004_9154, nfs4rc::f_80049154, |_, _| vec![PINK_SCREEN]),
            ("tScreenPinkSlipCongrats::Initialize", 0x8004_9264, nfs4rc::f_80049264, |_, _| vec![PINK_SCREEN]),
            ("tScreenPinkSlipCongrats::Cleanup", 0x8004_92bc, nfs4rc::f_800492bc, |_, _| vec![PINK_SCREEN]),
            ("tScreenTournamentTrophy::ProcessInput", 0x8004_92e8, nfs4rc::f_800492e8, |c, r| {
                c.m.w32(OUT, pick(r, 6));
                vec![TROPHY_SCREEN, 0, OUT, OUT + 4]
            }),
            ("tScreenTournamentTrophy::GetCar", 0x8004_9330, nfs4rc::f_80049330, |_, _| vec![TROPHY_SCREEN, CAR_OUT]),
            ("tScreenTournamentTrophy::DrawCongratsMessage", 0x8004_93dc, nfs4rc::f_800493dc, |_, _| vec![TROPHY_SCREEN]),
            ("tScreenTournamentTrophy::CalculatePrizes", 0x8004_96c0, nfs4rc::f_800496c0, |_, _| vec![TROPHY_SCREEN]),
            ("tScreenBeTheCopCongrats::GetCar", 0x8004_9900, nfs4rc::f_80049900, |_, _| vec![COP_SCREEN, CAR_OUT]),
            ("tScreenBeTheCopCongrats::CalculatePrizes", 0x8004_9960, nfs4rc::f_80049960, |_, _| vec![COP_SCREEN]),
            ("tScreenBeTheCopCongrats::DrawCongratsMessage", 0x8004_99b0, nfs4rc::f_800499b0, |_, _| vec![COP_SCREEN]),
            ("tScreenTournamentCongrats::GetCar", 0x8004_9a70, nfs4rc::f_80049a70, |_, _| vec![TIER_SCREEN, CAR_OUT]),
            ("tScreenTournamentCongrats::CalculatePrizes", 0x8004_9ac4, nfs4rc::f_80049ac4, |_, _| vec![TIER_SCREEN]),
            ("tScreenTournamentCongrats::DrawCongratsMessage", 0x8004_9b5c, nfs4rc::f_80049b5c, |_, _| vec![TIER_SCREEN]),
            ("~tScreenTournamentCongrats", 0x8004_9c28, nfs4rc::f_80049c28, |_, r| vec![TIER_SCREEN, 2 * pick(r, 2)]),
            ("~tScreenBeTheCopCongrats", 0x8004_9c48, nfs4rc::f_80049c48, |_, r| vec![COP_SCREEN, 2 * pick(r, 2)]),
            ("~tScreenPinkSlipCongrats", 0x8004_9c68, nfs4rc::f_80049c68, |_, r| vec![PINK_SCREEN, 2 * pick(r, 2)]),
            ("~tScreenTournamentTrophy", 0x8004_9c88, nfs4rc::f_80049c88, |_, r| vec![TROPHY_SCREEN, 2 * pick(r, 2)]),
            ("~tScreenCongrats", 0x8004_9ca8, nfs4rc::f_80049ca8, |_, r| vec![screen(r), 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screencongrats_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7D0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7D1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn screencongrats_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0x7E1 + k as u64, shuffle, &args);
        }
    }
}
