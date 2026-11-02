//! SCREENCONGRATS.CPP: the screens after a win (`tScreenCongrats` and its four kinds): the
//! trophy or medal won (its picture turning, or "congb"'s ribbon), studs round the edge, the
//! prize money counted into the total, a speech, and the car won turning on its stand; each
//! kind's message. The pink slips' (the winner's car taken), the tournament's trophy (a place
//! or knocked out, the car won), Be the Cop's and a tier's (the tier's car won).
//!
//! Written as FEMENU.CPP is (see there). `DrawCar` is the host's (called by address).

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::femenu::{KEY_ACCEPT, KEY_HELP, KEY_NONE, PLAY_FE_SFX};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_WORD};
use crate::globals::{CAR_MANAGER, FRONT_END, GAME_TICKS, TEXT_COLORS, TOURNAMENT_MANAGER};
use crate::types::{DrawShapeExtended, Screen, TextureShapeInfo};
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
/// The prizes: gold, silver, bronze, a car, nothing; the pictures: the trophy, the ribbon.
const PRIZE_GOLD: u32 = 0;
const PRIZE_SILVER: u32 = 1;
const PRIZE_BRONZE: u32 = 2;
const PRIZE_CAR: u32 = 3;
const PRIZE_NONE: u32 = 4;
const PICTURE_TROPHY: u32 = 1;
const PICTURE_RIBBON: u32 = 2;
/// The car's distance and height (`float`s: 4, -7.4; lost -8.2).
const DISTANCE: u32 = 0x4080_0000;
const HEIGHT: u32 = 0xC0EC_CCCD;
const LOST_HEIGHT: u32 = 0xC103_3333;
/// The drawing's flags: coloured by its top-left corner, and with the screen's own shapes
/// (`psxfront`'s); the ribbon's colour.
const COLOURED_TOP_LEFT: u32 = 0x410;
const OWN_TABLE_FLAGS: u32 = 0x610;
const RIBBON_COLOR: i32 = 0x0055_1E00;
/// The money's tick; `DrawMoney`'s kinds (an amount, a total); a message's room.
const SOUND_TICK: u32 = 21;
const AMOUNT: u32 = 6;
const TOTAL_KIND: u32 = 9;
const TEXT_SIZE: u32 = 504;
/// The police Diablo (it has its own words).
const POLICE_DIABLO: u8 = 28;

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
const KNOCKOUT: u32 = 5;
// The award (`tAwardInformation`): the tournament's money, a car won, the garage full, its
// money, its model, colour and upgrades; the tier's text, car, garage full and money.
const AWARD_SIZE: u32 = 68;
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
const RELEASE_CARS: fn(&mut Cpu) = crate::psxfront::cleanup_spinning_cars_menu_entry;
const SCALE_SHAPE_EXTENDED: fn(&mut Cpu) = crate::psxfront::scale_shape_extended_entry;
const SET_LICENSE_PLATE: fn(&mut Cpu) = crate::front::set_license_plate_entry;
const PLAYER_NAME: fn(&mut Cpu) = crate::front::player_name_entry;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const PLAY_SPEECH: fn(&mut Cpu) = crate::feaudio::async_play_speech_entry;
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

/// The screen's fade.
fn fade(m: &mut Mem, this: u32) -> u32 {
    lh(m, this + FADE)
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
/// player (competitor 0), of the racers left (and one more in a knock-out).
fn player_place(m: &mut Mem, t: u32) -> u32 {
    let knock_out = (m.r8(t + KNOCKOUT) != 0) as u32;
    let n = sx(sx(m.r16(TOURNAMENT_MANAGER + RACERS) as u32).wrapping_add(knock_out)) as i32;
    let mut place = 900;
    for k in 1..=n.max(0) as u32 {
        if fetourn::player_ranking(m, TOURNAMENT_MANAGER, sx(k)) << 16 == 0 {
            place = k;
        }
    }
    place
}

/// The award (`GetAwardInformation`'s copy) read by `f`.
fn with_award<R>(c: &mut Cpu, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    with_buffer(c, AWARD_SIZE, |c, award| {
        fetourn::get_award_information(&mut c.m, TOURNAMENT_MANAGER, award);
        f(c, award)
    })
}

/// A car description copied from the car manager's.
fn copy_car(c: &mut Cpu, id: u32, info: u32) {
    let car = fecars::get_car_from_id(&mut c.m, CAR_MANAGER, id as i32);
    call(c, BLOCKMOVE, &[car, info, CAR_SIZE]);
}

/// `tScreenCongrats::Cleanup` (SCREENCONGRATS.CPP, 0x800482e0): the 3D cars released, then the
/// screen's.
pub fn cleanup(c: &mut Cpu, this: u32) {
    call(c, RELEASE_CARS, &[this]);
    fescreen::cleanup(c, Screen(this));
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, c.r[4]);
}

/// `tScreenCongrats::GetShapeInfo` (SCREENCONGRATS.CPP, 0x8004830c): 43 permanent shapes of
/// "zcong%d" (22 of "zelim%d" if lost, by language); swapped in: the trophy's 32 (by the
/// player's place) unless the prize is a car or nothing, else the trophy's (the prize a car
/// with it), the ribbon's 10, or none.
pub fn get_shape_info(c: &mut Cpu, this: u32, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    let m = &mut c.m;
    let t = tournament(m);
    m.w32(this + FRAMES, 0);
    m.w32(this + SWAP_COUNT, 0);
    let lost = m.r32(this + LOST) != 0;
    m.w16(permanent, if lost { 22 } else { 43 });
    let file = if lost { LOST_NAME } else { CONGRATS_NAME };
    let language = m.r8(LANGUAGE) as u32;
    call(c, SPRINTF, &[SWAP_NAME, NAME_FORMAT, file, language]);
    let m = &mut c.m;
    m.w32(name, SWAP_NAME);
    if m.r32(this + PRIZE).wrapping_sub(PRIZE_CAR) >= 2 {
        let place = player_place(m, t);
        fetourn::get_trophy_name(c, t, 2, TROPHY_NAME, place);
        let m = &mut c.m;
        m.w32(swap_name, TROPHY_NAME);
        m.w16(swap, 32);
        m.w32(this + SWAP_COUNT, 32);
        return;
    }
    match m.r32(this + PICTURE) {
        PICTURE_TROPHY => {
            m.w16(swap, 32);
            m.w32(this + FRAMES, 32);
            fetourn::get_trophy_name(c, t, 0, TROPHY_NAME, u32::MAX);
            c.m.w32(swap_name, TROPHY_NAME);
        }
        PICTURE_RIBBON => {
            m.w16(swap, 10);
            m.w32(this + FRAMES, 10);
            m.w32(swap_name, RIBBON_NAME);
        }
        _ => {
            m.w32(swap_name, 0);
            m.w16(swap, 0);
        }
    }
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(c, c.r[4], c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenCongrats::GetCar` (SCREENCONGRATS.CPP, 0x8004855c): the garage's car, in its colour.
pub fn get_car(c: &mut Cpu, info: u32) -> u32 {
    let car = c.m.r8(GARAGE_CAR) as u32;
    fecars::get_garage_car(c, CAR_MANAGER, car, info, 0);
    own_color(&mut c.m, info);
    1
}

pub fn abi_get_car(c: &mut Cpu) {
    c.r[2] = get_car(c, c.r[5]);
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
pub fn draw_background(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let going = if m.r32(this + GOING_OFF) != 0 { fade(m, this) } else { 0 };
    let shapes = m.r32(this + SWAP_TABLE);
    let angle = m.r32(ANGLE).wrapping_add(3);
    m.w32(ANGLE, angle);
    call_virtual(c, this, VPTR, DRAW_CONGRATS_MESSAGE, &[]);
    let m = &mut c.m;
    let prize = m.r32(this + PRIZE);
    let ready = prize != PRIZE_CAR
        || (m.r32(this + SHOWN_AT) != u32::MAX
            && m.r32(this + HAS_CAR) != 0
            && (m.r32(ASYNC_LOADING) as i32) < 0
            && (ticks(m).wrapping_sub(m.r32(this + UP_AT)) as i32) >= 60);
    if !ready {
        let now = ticks(m);
        m.w32(this + SHOWN_AT, now);
    } else {
        draw_counting(c, this, going, shapes);
    }
    draw_studs(c, this);
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, c.r[4]);
}

/// DrawBackground from the clock on: the money, the speech, the stars and pictures (`going`
/// the fade while going off; the swapped shapes' table).
fn draw_counting(c: &mut Cpu, this: u32, going: u32, shapes: u32) {
    let m = &mut c.m;
    if m.r32(this + SHOWN_AT) == u32::MAX {
        let now = ticks(m);
        m.w32(this + SHOWN_AT, now);
    }
    if m.r32(this + TO_COUNT) != u32::MAX {
        draw_money(c, this);
    }
    // the speech
    let m = &mut c.m;
    let speech = m.r32(this + SPEECH);
    if speech != 0 && ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32 >= 129 {
        call(c, PLAY_SPEECH, &[2, speech]);
        c.m.w32(this + SPEECH, 0);
    }
    with_buffer(c, 2 * DrawShapeExtended::SIZE, |c, buffer| {
        let (stars_options, trophy_options) = (DrawShapeExtended(buffer), DrawShapeExtended(buffer + DrawShapeExtended::SIZE));
        let m = &mut c.m;
        stars_options.set_custom_shapes(m, TextureShapeInfo(shapes));
        trophy_options.set_custom_shapes(m, TextureShapeInfo(shapes));
        // the stars: grown, then now and then twinkling; the lost ones grey
        let lost = m.r32(this + LOST) != 0;
        let star_color = if lost { fefades::fade_out(0x0064_6464, going) } else { fefades::fade_out(0xBEBE, fade(m, this)) };
        stars_options.set_tint(m, 0, star_color as i32);
        trophy_options.set_tint(m, 0, fefades::fade_out(0x0080_8080, going) as i32);
        let now = ticks(m) as i32;
        let mut scaled = false;
        if lost {
            let v = now.wrapping_sub(m.r32(this + SHOWN_AT) as i32) / 4;
            m.w32(this + STARS, v as u32);
            if v >= 21 {
                m.w32(this + STARS, 20);
            }
        } else if m.r32(this + TWINKLING) != 0 {
            let v = now.wrapping_sub(m.r32(this + TWINKLED_AT) as i32) / 6 + 21;
            m.w32(this + STARS, v as u32);
            scaled = true;
            if v >= 42 {
                m.w32(this + TWINKLING, 0);
                m.w32(this + STARS, 20);
            }
        } else {
            let shown = m.r32(this + SHOWN_AT) as i32;
            let v = now.wrapping_sub(shown) / 2;
            m.w32(this + STARS, v as u32);
            if v >= 20 {
                scaled = true;
                m.w32(this + STARS, 20);
                if now.wrapping_sub(shown / 4) % 1500 < 45 {
                    m.w32(this + TWINKLING, 1);
                    m.w32(this + TWINKLED_AT, now as u32);
                }
            }
        }
        let y = if m.r32(this + LOST) == 1 { 10 } else { 0 };
        let stars = m.r32(this + STARS);
        let draw = if scaled { SCALE_SHAPE_EXTENDED } else { DRAW_SHAPE_EXTENDED };
        call(c, draw, &[stars, COLOURED_TOP_LEFT, 0, y, 0, 0, stars_options.0]);
        // the trophy turning, the prize's picture
        let m = &mut c.m;
        if m.r32(this + PRIZE).wrapping_sub(PRIZE_CAR) >= 2 {
            let frame = (ticks(m) as i32 / 12) % 32;
            call(c, SCALE_SHAPE_EXTENDED, &[frame as u32, OWN_TABLE_FLAGS, 70, 15, 0, 0, trophy_options.0]);
        }
        let m = &mut c.m;
        if m.r32(this + PRIZE) < PRIZE_CAR {
            return;
        }
        match m.r32(this + PICTURE) {
            PICTURE_TROPHY => {
                let frame = crate::fixed::rem(ticks(m) as i32 >> 3, m.r32(this + FRAMES) as i32);
                call(c, SCALE_SHAPE_EXTENDED, &[frame as u32, OWN_TABLE_FLAGS, 41, 190, 0, 0, trophy_options.0]);
            }
            PICTURE_RIBBON => {
                stars_options.set_tint(m, 0, RIBBON_COLOR);
                let frame = crate::fixed::rem(ticks(m) as i32 / 20, m.r32(this + FRAMES) as i32);
                call(c, DRAW_SHAPE_EXTENDED, &[frame as u32, OWN_TABLE_FLAGS, (-193i32) as u32, 86, 0, 0, stars_options.0]);
            }
            _ => {}
        }
    });
}

/// The money won counted down (by its step, once skipped or 1000 ticks on; a tick sounded each
/// step) and the total counted up, each with its label.
fn draw_money(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let since = ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32;
    if since >= 1001 || m.r32(this + SKIPPED) != 0 {
        if m.r32(this + TO_COUNT) != 0 {
            call(c, PLAY_FE_SFX, &[SOUND_TICK]);
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
    let f = fade(m, this);
    let gold = fefades::fade_out(color, f);
    let shade = fefades::fade_out(0x0023_2323, fade(m, this));
    for (j, (label, at, kind)) in [(MONEY_LABEL, MONEY_AT, AMOUNT), (TOTAL_LABEL, TOTAL_AT, TOTAL_KIND)].into_iter().enumerate() {
        let f = fade(&mut c.m, this);
        fetextrender::menu_text_fade(c, f, label, 2, 11);
        let x = call(c, WORD_X, &[at]);
        let y = call(c, WORD_Y, &[at]);
        let m = &mut c.m;
        let left = m.r32(this + TO_COUNT);
        let amount = if j == 0 { left } else { m.r32(this + TOTAL).wrapping_sub(left) };
        screenpost::draw_money(c, x, y, kind, amount, gold, shade);
    }
}

/// DrawBackground's end: the studs round the edge in the prize's colour (fewer when lost or the
/// prize is a car: every third left out), the car turning.
fn draw_studs(c: &mut Cpu, this: u32) {
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
    let color = fefades::fade_out(color, fade(m, this));
    let rows: &[(u32, u32)] = if !lost && prize != PRIZE_CAR { &[(1, 30), (34, 63)] } else { &[(7, 40)] };
    with_buffer(c, DrawShapeExtended::SIZE, |c, options| {
        DrawShapeExtended(options).set_tint(&mut c.m, 0, color as i32);
        for &(from, to) in rows {
            for k in from..to {
                if k % 3 != 0 {
                    let f = fade(&mut c.m, this);
                    call(c, DRAW_SHAPE_EXTENDED, &[shape, COLOURED_TOP_LEFT, 2 * k, 0, f, 1, options]);
                }
            }
        }
    });
    // the car
    let m = &mut c.m;
    if m.r32(this + PRIZE) == PRIZE_CAR && m.r32(this + HAS_CAR) != 0 {
        m.w32(DRAW_CAR_FLAG, 0);
        let (x, y) = (lh(m, this + CAR_X), lh(m, this + CAR_Y));
        let (distance, height) = (m.r32(this + CAR_DISTANCE), m.r32(this + CAR_HEIGHT));
        let (angle, player) = (m.r32(ANGLE), m.r32(this + PLAYER));
        call_addr(c, DRAW_CAR, &[this + CAR, x, y, distance, height, 128, 1, angle, player]);
    }
}

/// `tScreenCongrats::DrawForeground` (SCREENCONGRATS.CPP, 0x80048e28): nothing.
pub fn abi_draw_foreground(_: &mut Cpu) {}

/// `tScreenCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80048e30): nothing won, no money;
/// the car's place.
pub fn calculate_prizes(m: &mut Mem, this: u32) {
    m.w32(this + PRIZE, PRIZE_NONE);
    m.w32(this + TO_COUNT, u32::MAX);
    m.w16(this + CAR_X, 278);
    m.w32(this + LOST, 0);
    m.w16(this + CAR_Y, 63);
    m.w32(this + CAR_HEIGHT, HEIGHT);
    let lost = m.r32(this + LOST);
    m.w32(this + PICTURE, 0);
    m.w32(this + PLAYER, 0);
    m.w32(this + TOTAL, 0);
    m.w32(this + CAR_DISTANCE, DISTANCE);
    car_place(m, this, lost);
}

pub fn abi_calculate_prizes(c: &mut Cpu) {
    calculate_prizes(&mut c.m, c.r[4]);
}

/// The car further off when lost.
fn car_place(m: &mut Mem, this: u32, lost: u32) {
    if lost == 1 {
        m.w16(this + CAR_X, 288);
        m.w16(this + CAR_Y, 73);
        m.w32(this + CAR_HEIGHT, LOST_HEIGHT);
    }
}

/// The car's place for a car won.
fn car_won_place(m: &mut Mem, this: u32) {
    m.w16(this + CAR_X, 278);
    m.w16(this + CAR_Y, 75);
    m.w32(this + CAR_DISTANCE, DISTANCE);
    m.w32(this + CAR_HEIGHT, HEIGHT);
}

/// `tScreenCongrats::Initialize` (SCREENCONGRATS.CPP, 0x80048ea4): the clocks reset, the
/// licence plate, the prizes, the car; the money counted in 64 steps (none: counted); the
/// screen's.
pub fn initialize(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let now = ticks(m);
    m.w32(this + SPEECH, 0);
    m.w32(this + SHOWN_AT, u32::MAX);
    m.w32(this + STARS, u32::MAX);
    m.w32(this + TWINKLING, 0);
    m.w32(this + UP_AT, now);
    call(c, SET_LICENSE_PLATE, &[]);
    call_virtual(c, this, VPTR, CALCULATE_PRIZES, &[]);
    let has = call_virtual(c, this, VPTR, GET_CAR, &[this + CAR]);
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
    fescreen::initialize(c, Screen(this));
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, c.r[4]);
}

/// `tScreenCongrats::ProcessInput` (SCREENCONGRATS.CPP, 0x80048f68): keys ignored (made none)
/// while the narration speaks or for the first 150 ticks; help passes.
pub fn process_input(m: &mut Mem, this: u32, key: u32) {
    if m.r32(key) == KEY_HELP {
        return;
    }
    if m.r16(SPEAKING) != 0 || (ticks(m).wrapping_sub(m.r32(this + SHOWN_AT)) as i32) < 150 {
        m.w32(key, KEY_NONE);
    }
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(&mut c.m, c.r[4], c.r[6]);
}

/// `tScreenPinkSlipCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x80048fc0): the winner
/// taking the loser's car (the words in the language's order), and the winner's number.
pub fn pink_slip_draw_congrats_message(c: &mut Cpu, this: u32) {
    let language = c.m.r8(LANGUAGE) as u32;
    let car_name = |c: &mut Cpu| {
        let id = c.m.r8(this + CAR + CAR_ID) as i8 as i32 as u32;
        call(c, TEXT_WORD, &[id.wrapping_add(CAR_NAMES)])
    };
    let loser = |c: &mut Cpu| {
        let w = lh(&mut c.m, this + WINNER);
        call(c, PLAYER_NAME, &[1u32.wrapping_sub(w)])
    };
    let format = call(c, TEXT_WORD, &[PINK_MESSAGE]);
    let w = lh(&mut c.m, this + WINNER);
    let winner = call(c, PLAYER_NAME, &[w]);
    // (the car before the loser in languages 2 and 3)
    let (third, fourth) = if language.wrapping_sub(2) < 2 {
        let car = car_name(c);
        (car, loser(c))
    } else {
        let l = loser(c);
        (l, car_name(c))
    };
    let number = lh(&mut c.m, this + WINNER).wrapping_add(1);
    with_buffer(c, 8 + 280, |c, buffer| {
        let (rect, text) = (buffer, buffer + 8);
        set_rect(&mut c.m, rect, [41, 60, 420, 200]);
        call(c, SPRINTF, &[text, format, winner, third, fourth, number]);
        fetextrender::word_wrap_text(c, text, rect, 2, 10);
    });
}

pub fn abi_pink_slip_draw_congrats_message(c: &mut Cpu) {
    pink_slip_draw_congrats_message(c, c.r[4]);
}

/// A `RECT`.
fn set_rect(m: &mut Mem, rect: u32, v: [u16; 4]) {
    for (k, v) in v.into_iter().enumerate() {
        m.w16(rect + 2 * k as u32, v);
    }
}

/// `tScreenPinkSlipCongrats::GetCar` (SCREENCONGRATS.CPP, 0x800490dc): the loser's car, in its
/// colour.
pub fn pink_slip_get_car(c: &mut Cpu, this: u32, info: u32) -> u32 {
    let m = &mut c.m;
    let w = m.r16(this + WINNER) as u32;
    let car = m.r8(PINK_SLIPS_CAR.wrapping_add(1u32.wrapping_sub(sx(w)))) as u32;
    let player = sx(1u32.wrapping_sub(w));
    fecars::get_pink_slips_car(c, CAR_MANAGER, car, info, player);
    own_color(&mut c.m, info);
    1
}

pub fn abi_pink_slip_get_car(c: &mut Cpu) {
    c.r[2] = pink_slip_get_car(c, c.r[4], c.r[5]);
}

/// `tScreenPinkSlipCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049154): the loser's car
/// won (with the ribbon), no money; the loser's licence plate made; the car's speech (by its
/// own or the player).
pub fn pink_slip_calculate_prizes(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let w = lh(m, this + WINNER);
    m.w32(this + PRIZE, PRIZE_CAR);
    m.w32(this + PICTURE, PICTURE_RIBBON);
    m.w32(this + TOTAL, 0);
    m.w32(this + TO_COUNT, u32::MAX);
    m.w32(this + LOST, 0);
    let loser = 1u32.wrapping_sub(w);
    let w = lh(m, this + WINNER);
    m.w32(this + PLAYER, 1u32.wrapping_sub(w));
    call(c, CLEAN_UP_LICENSE, &[loser]);
    call(c, CREATE_LICENSE, &[PLATES.wrapping_add(1u32.wrapping_sub(loser) << 3), 0, loser]);
    with_buffer(c, CAR_SIZE, |c, info| {
        call_virtual(c, this, VPTR, GET_CAR, &[info]);
        let m = &mut c.m;
        let speech = m.r8(info + CAR_SPEECH) as i8 as i32 as u32;
        let w = lh(m, this + WINNER);
        let v = if speech != u32::MAX { (speech << 1).wrapping_add(w.wrapping_add(19)) } else { w.wrapping_add(23) };
        m.w32(this + SPEECH, v);
    });
    car_won_place(&mut c.m, this);
}

pub fn abi_pink_slip_calculate_prizes(c: &mut Cpu) {
    pink_slip_calculate_prizes(c, c.r[4]);
}

/// `tScreenPinkSlipCongrats::Initialize` (SCREENCONGRATS.CPP, 0x80049264): the winner (the
/// first player if they won more than half the tracks, else the second), then the screen's.
pub fn pink_slip_initialize(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    m.w16(this + WINNER, 0xFFFF);
    let half = (m.r8(PINK_COUNT) as i32 >> 1) + 1;
    let wins = m.r8(PINK_WINS) as i32;
    m.w16(this + WINNER, if wins < half { 1 } else { 0 });
    initialize(c, this);
}

pub fn abi_pink_slip_initialize(c: &mut Cpu) {
    pink_slip_initialize(c, c.r[4]);
}

/// `tScreenPinkSlipCongrats::Cleanup` (SCREENCONGRATS.CPP, 0x800492bc): as the screen's.
pub fn abi_pink_slip_cleanup(c: &mut Cpu) {
    cleanup(c, c.r[4]);
}

/// `tScreenTournamentTrophy::ProcessInput` (SCREENCONGRATS.CPP, 0x800492e8): accept while the
/// money still counts skips the counting; then the screen's.
pub fn trophy_process_input(m: &mut Mem, this: u32, key: u32) {
    if m.r32(key) == KEY_ACCEPT && m.r32(this + COUNTED) == 0 {
        m.w32(key, KEY_NONE);
        m.w32(this + SKIPPED, 1);
    }
    process_input(m, this, key);
}

pub fn abi_trophy_process_input(c: &mut Cpu) {
    trophy_process_input(&mut c.m, c.r[4], c.r[6]);
}

/// `tScreenTournamentTrophy::GetCar` (SCREENCONGRATS.CPP, 0x80049330): the car won, in its
/// colour and upgrades (lost: the garage's).
pub fn trophy_get_car(c: &mut Cpu, this: u32, info: u32) -> u32 {
    if c.m.r32(this + LOST) == 0 {
        with_award(c, |c, award| {
            let id = lh(&mut c.m, award + AWARD_CAR_MODEL);
            copy_car(c, id, info);
            let m = &mut c.m;
            let upgrades = m.r8(award + AWARD_CAR_UPGRADES);
            m.w8(info + CAR_UPGRADES, upgrades);
            let color = m.r8(award + AWARD_CAR_COLOR);
            m.w8(info + CAR_COLOR, color);
        });
    } else {
        let car = c.m.r8(GARAGE_CAR) as u32;
        fecars::get_garage_car(c, CAR_MANAGER, car, info, 0);
        own_color(&mut c.m, info);
    }
    1
}

pub fn abi_trophy_get_car(c: &mut Cpu) {
    c.r[2] = trophy_get_car(c, c.r[4], c.r[5]);
}

/// `tScreenTournamentTrophy::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x800493dc): lost: "LOST"
/// in grey. Else (best placed fourth or worse before) the trophy's and the prize's words, (won
/// the car, or not first before) what winning brings, a word-wrapped paragraph or two; the
/// garage-full words if the car's money was given instead.
pub fn trophy_draw_congrats_message(c: &mut Cpu, this: u32) {
    if c.m.r32(this + LOST) != 0 {
        let y = call(c, WORD_Y, &[LOST_TEXT]);
        let w = call(c, TEXT_WORD, &[LOST_TEXT]);
        fetextrender::full_text_rgb(c, w, 288, sx(y.wrapping_sub(10)), 0x0041_4141, 3, 2);
        return;
    }
    with_buffer(c, 8 + 2 * TEXT_SIZE, |c, buffer| {
        let (rect, first, second) = (buffer, buffer + 8, buffer + 8 + TEXT_SIZE);
        set_rect(&mut c.m, rect, [41, 53, 420, 400]);
        let (mut has_first, mut has_second) = (false, false);
        let t = tournament(&mut c.m);
        let m = &mut c.m;
        let prize = m.r32(this + PRIZE) as i32;
        if (m.r8(TOURNAMENT_MANAGER + LAST_BEST) as i8) >= 4 {
            let id = m.r8(t) as i8 as i32 as u32;
            has_first = true;
            let k = match prize {
                1 => 1,
                0 | 3 => 0,
                _ => 2,
            };
            let a = call(c, TEXT_WORD, &[id.wrapping_add(TROPHY_TEXTS)]);
            let b = call(c, TEXT_WORD, &[k + PRIZE_TEXTS]);
            call(c, SPRINTF, &[first, a, b]);
        }
        let m = &mut c.m;
        let prize = m.r32(this + PRIZE);
        if (prize == 0 && m.r8(TOURNAMENT_MANAGER + LAST_BEST) as i8 != 1) || prize == PRIZE_CAR {
            let id = m.r8(t) as i8 as i32 as u32;
            let w = call(c, TEXT_WORD, &[id.wrapping_add(WIN_TEXTS)]);
            if !has_first {
                has_first = true;
                call(c, SPRINTF, &[first, w]);
            } else {
                has_second = true;
                call(c, SPRINTF, &[second, w]);
            }
        }
        if has_first {
            let f = fade(&mut c.m, this);
            fetextrender::word_wrap_text_fade(c, f, first, rect, 2, 10);
            let width = lh(&mut c.m, rect + 4);
            let h = fetextrender::word_wrap_height(c, width, first);
            let m = &mut c.m;
            let y = (m.r16(rect + 2) as u32).wrapping_sub(14).wrapping_add(h);
            m.w16(rect + 2, y as u16);
        }
        if has_second {
            c.m.w16(rect + 4, 200);
            let f = fade(&mut c.m, this);
            fetextrender::word_wrap_text_fade(c, f, second, rect, 2, 10);
        }
        with_award(c, |c, award| {
            if c.m.r32(award + AWARD_CAR_FULL) != 0 {
                let amount = c.m.r32(award + AWARD_CAR_MONEY);
                for (k, v) in [260u16, 200, 240].into_iter().enumerate() {
                    c.m.w16(rect + 2 * k as u32, v);
                }
                // (the words printed with the money, and not shown: as the original)
                garage_full(c, amount, first);
                let w = call(c, TEXT_WORD, &[GARAGE_FULL]);
                let f = fade(&mut c.m, this);
                fetextrender::word_wrap_text_fade(c, f, w, rect, 1, 10);
            }
        });
    });
}

pub fn abi_trophy_draw_congrats_message(c: &mut Cpu) {
    trophy_draw_congrats_message(c, c.r[4]);
}

/// The garage-full words with `amount` printed into `text` (which the screens then don't show:
/// they draw the words' own text).
fn garage_full(c: &mut Cpu, amount: u32, text: u32) {
    with_buffer(c, 64, |c, money| {
        fetools::format_money(c, money, amount);
        let w = call(c, TEXT_WORD, &[GARAGE_FULL]);
        call(c, SPRINTF, &[text, w, money]);
    });
}

/// `tScreenTournamentTrophy::CalculatePrizes` (SCREENCONGRATS.CPP, 0x800496c0): by the player's
/// place: first the car (if one is won) or gold, second silver and third bronze (each with its
/// speech) unless knocked out, else lost; the money won counted into the total (less the
/// tier's car's money when the garage was full); the car's place.
pub fn trophy_calculate_prizes(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    m.w32(this + LOST, 1);
    m.w32(this + PRIZE, PRIZE_NONE);
    m.w32(this + PICTURE, PICTURE_RIBBON);
    with_award(c, |c, award| {
        let m = &mut c.m;
        let t = tournament(m);
        let place = player_place(m, t);
        let (lost, prize, picture) = if place == 1 {
            m.w32(this + SPEECH, 15);
            if m.r32(award + AWARD_CAR) != 0 { (0, PRIZE_CAR, PICTURE_TROPHY) } else { (0, PRIZE_GOLD, 0) }
        } else if (place as i32) <= 0 || place as i32 >= 4 || m.r8(t + KNOCKOUT) != 0 {
            (1, PRIZE_CAR, 0)
        } else {
            m.w32(this + SPEECH, place.wrapping_add(14));
            (0, if place == 2 { PRIZE_SILVER } else { PRIZE_BRONZE }, 0)
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
    });
    let m = &mut c.m;
    m.w16(this + CAR_X, 278);
    m.w16(this + CAR_Y, 63);
    m.w32(this + CAR_HEIGHT, HEIGHT);
    m.w32(this + CAR_DISTANCE, DISTANCE);
    let lost = m.r32(this + LOST);
    car_place(m, this, lost);
}

pub fn abi_trophy_calculate_prizes(c: &mut Cpu) {
    trophy_calculate_prizes(c, c.r[4]);
}

/// `tScreenBeTheCopCongrats::GetCar` (SCREENCONGRATS.CPP, 0x80049900): the cop car, in its
/// colour.
pub fn cop_get_car(c: &mut Cpu, info: u32) -> u32 {
    let id = c.m.r8(COP_CAR) as u32;
    copy_car(c, id, info);
    let color = c.m.r8(COP_CAR_COLOR);
    c.m.w8(info + COP_COLOR, color);
    1
}

pub fn abi_cop_get_car(c: &mut Cpu) {
    c.r[2] = cop_get_car(c, c.r[5]);
}

/// `tScreenBeTheCopCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049960): the screen's,
/// then the car won.
pub fn cop_calculate_prizes(m: &mut Mem, this: u32) {
    calculate_prizes(m, this);
    m.w32(this + PRIZE, PRIZE_CAR);
    car_won_place(m, this);
}

pub fn abi_cop_calculate_prizes(c: &mut Cpu) {
    cop_calculate_prizes(&mut c.m, c.r[4]);
}

/// `tScreenBeTheCopCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x800499b0): the cop car
/// earned (the police Diablo's own words), with its name.
pub fn cop_draw_congrats_message(c: &mut Cpu, this: u32) {
    let message = if c.m.r8(COP_CAR) == POLICE_DIABLO { COP_MESSAGE + 1 } else { COP_MESSAGE };
    let name = |c: &mut Cpu| {
        let id = c.m.r8(this + CAR + CAR_ID) as i8 as i32 as u32;
        call(c, TEXT_WORD, &[id.wrapping_add(CAR_NAMES)])
    };
    // (the pad read, its answer only fetching the name again: as the original)
    if call(c, PAD_BUTTONS, &[4]) & 0xFFFF != 0 {
        name(c);
    }
    let w = call(c, TEXT_WORD, &[message]);
    let car = name(c);
    with_buffer(c, 8 + 272, |c, buffer| {
        let (rect, text) = (buffer, buffer + 8);
        set_rect(&mut c.m, rect, [41, 60, 420, 200]);
        call(c, SPRINTF, &[text, w, car]);
        fetextrender::word_wrap_text(c, text, rect, 2, 10);
    });
}

pub fn abi_cop_draw_congrats_message(c: &mut Cpu) {
    cop_draw_congrats_message(c, c.r[4]);
}

/// `tScreenTournamentCongrats::GetCar` (SCREENCONGRATS.CPP, 0x80049a70): the tier's car.
pub fn tier_get_car(c: &mut Cpu, info: u32) -> u32 {
    with_award(c, |c, award| {
        let id = lh(&mut c.m, award + COMPLETED_CAR);
        copy_car(c, id, info);
    });
    1
}

pub fn abi_tier_get_car(c: &mut Cpu) {
    c.r[2] = tier_get_car(c, c.r[5]);
}

/// `tScreenTournamentCongrats::CalculatePrizes` (SCREENCONGRATS.CPP, 0x80049ac4): the tier's
/// car won, the total; the car's money counted in if the garage was full.
pub fn tier_calculate_prizes(c: &mut Cpu, this: u32) {
    with_award(c, |c, award| {
        let m = &mut c.m;
        calculate_prizes(m, this);
        m.w32(this + PRIZE, PRIZE_CAR);
        let money = m.r32(TOURNAMENT_MANAGER + MONEY);
        m.w32(this + TOTAL, money);
        let left = if m.r32(award + COMPLETED_FULL) != 0 { m.r32(award + COMPLETED_MONEY) } else { u32::MAX };
        m.w32(this + TO_COUNT, left);
        car_won_place(m, this);
    });
}

pub fn abi_tier_calculate_prizes(c: &mut Cpu) {
    tier_calculate_prizes(c, c.r[4]);
}

/// `tScreenTournamentCongrats::DrawCongratsMessage` (SCREENCONGRATS.CPP, 0x80049b5c): the tier's
/// text; the garage-full words if the car's money was given instead.
pub fn tier_draw_congrats_message(c: &mut Cpu) {
    with_buffer(c, 8 + TEXT_SIZE, |c, buffer| {
        let (rect, text) = (buffer, buffer + 8);
        set_rect(&mut c.m, rect, [41, 60, 420, 400]);
        with_award(c, |c, award| {
            let id = lh(&mut c.m, award + COMPLETED_TEXT);
            let w = call(c, TEXT_WORD, &[id]);
            fetextrender::word_wrap_text(c, w, rect, 2, 10);
            if c.m.r32(award + COMPLETED_FULL) != 0 {
                let amount = c.m.r32(award + COMPLETED_MONEY);
                c.m.w16(rect + 2, 180);
                c.m.w16(rect + 4, 430);
                garage_full(c, amount, text);
                let w = call(c, TEXT_WORD, &[GARAGE_FULL]);
                fetextrender::word_wrap_text(c, w, rect, 2, 10);
            }
        });
    });
}

pub fn abi_tier_draw_congrats_message(c: &mut Cpu) {
    tier_draw_congrats_message(c);
}

/// The destructors (SCREENCONGRATS.H): `~tScreenTournamentCongrats` (0x80049c28),
/// `~tScreenBeTheCopCongrats` (0x80049c48), `~tScreenPinkSlipCongrats` (0x80049c68),
/// `~tScreenTournamentTrophy` (0x80049c88), `~tScreenCongrats` (0x80049ca8): the screen's.
fn destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

pub fn abi_tier_destructor(c: &mut Cpu) {
    destructor(c);
}

pub fn abi_cop_destructor(c: &mut Cpu) {
    destructor(c);
}

pub fn abi_pink_slip_destructor(c: &mut Cpu) {
    destructor(c);
}

pub fn abi_trophy_destructor(c: &mut Cpu) {
    destructor(c);
}

pub fn abi_destructor(c: &mut Cpu) {
    destructor(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::globals::FE_APP;
    use crate::test_util::{Rng, against_original_in, fe_ram};

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
            // (a trophy only for a place in the first three, as in the game: else its metal letter is
            // past its table, the original's stack)
            ("tScreenCongrats::GetShapeInfo", 0x8004_830c, nfs4rc::f_8004830c, |c, r| {
                let s = screen(r);
                let t = tournament(&mut c.m);
                if !(1..=3).contains(&player_place(&mut c.m, t)) {
                    c.m.w32(s + PRIZE, PRIZE_CAR);
                }
                vec![s, OUT, OUT + 2, OUT + 4, OUT + 8]
            }),
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

}
