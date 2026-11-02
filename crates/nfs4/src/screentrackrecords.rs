//! SCREENTRACKRECORDS.CPP: the records of the track just raced (`tScreenTrackRecords`). The
//! records are read with `Stattool_GetRecords` (18 of 20 bytes each: a name, the car, the time,
//! the best lap). The screen draws a heading per column and a row per record, the new ones lit
//! with a breathing halo, rolling in as the screen fades in; then the track's name and the units
//! with a mark sliding to and fro under them, and the columns' headings and lines.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`. The squares
//! are the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::GAME_TICKS;
use crate::types::{DrawShapeExtended, Screen, TextureShapeInfo};
use crate::{fefades, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

/// The screen: a `tScreen`, then +100 the records (`tRecordBuffer`, 20 bytes each: +0 the name,
/// +8 the car, +12 the time (0: none), +16 the best lap (negative: no record)), +104 the halo's
/// size, +108 its clock (0..60), +112 cleared.
const RECORDS: u32 = 100;
const HALO: u32 = 104;
const CLOCK: u32 = 108;
const CLEARED: u32 = 112;
const RECORD_SIZE: u32 = 20;
const CAR: u32 = 8;
const TIME: u32 = 12;
const BEST_LAP: u32 = 16;

/// Each record new (lit), the best lap's; the colours' table and the two colours' indexes.
const NEW: u32 = 0x8005_2E08;
const NEW_LAP: u32 = 0x8005_2E28;
const COLORS: u32 = 0x8005_1B50;
const TEXT_COLOR: u32 = 0x8005_1AE6;
const TITLE_COLOR: u32 = 0x8005_1AE7;
/// The permanent shapes' table now; the mark (shape 38: its width at +16, its centre at +20)
/// and its halves (39).
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
const MARK: u32 = 38;
const MARK_HALF: u32 = 39;
/// "zRec" (40 shapes), "trkrcrds", "%s %s".
const REC_NAME: u32 = 0x8001_1F84;
const SHAPES: u16 = 40;
const MEMORY_NAME: u32 = 0x8001_1F8C;
const TITLE_FORMAT: u32 = 0x8001_1F98;
/// The places' texts: x by `TextSys_WordX`, y by `TextSys_WordY`: the halo, the columns
/// (name, car, time, best lap), the rows (600 on, the best lap's 609), the box's corners.
const HALO_AT: u32 = 584;
const BOX_LEFT: u32 = 585;
const PLACE_X: u32 = 586;
const NAME_X: u32 = 588;
const CAR_X: u32 = 589;
const TIME_X: u32 = 590;
const LAP_X: u32 = 591;
const BOX_RIGHT: u32 = 592;
const TITLE_Y: u32 = 598;
const BOX_TOP: u32 = 599;
const ROWS: u32 = 600;
const BOX_BOTTOM: u32 = 608;
const LAP_ROW: u32 = 609;
/// The cars' names (from 340), "--" (no time), the units' format, the columns' headings (595 on,
/// for the car, time and best lap), the best lap's row's heading, the tracks' names (from 214).
const CAR_NAMES: u32 = 340;
const NO_TIME: u32 = 610;
const UNITS: u32 = 594;
const HEADINGS: u32 = 595;
const LAP_HEADING: u32 = 611;
const TRACK_NAMES: u32 = 214;
/// The lines' and squares' grey; the band's.
const LINE: u32 = 0x0023_2323;
const BAND: u32 = 0x0050_5050;

/// `PSXDrawSquare`: the host's; `DrawBrightEndLine` (FELINES's port).
const DRAW_SQUARE: u32 = 0x8004_F12C;
const DRAW_BRIGHT_END_LINE: u32 = 0x8004_C018;
/// `TextSys_WordX`, `TextSys_WordY`; `Flare_2DHalo`, hooked (the host captures its
/// primitives): called by address; `Stattool_ParseTime`, `Stattool_GetRecords`, the records'
/// cleanup (0x8004ad14), `Front_GetTrackRaced`, the units (0x80028004: 2 metric),
/// `reservememadr`, `purgememadr`.
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const HALO_2D: u32 = 0x800C_ECD0;
const PARSE_TIME: fn(&mut Cpu) = nfs4rc::f_8004ae74;
const GET_RECORDS: fn(&mut Cpu) = nfs4rc::f_8004b2f4;
const RECORDS_DONE: fn(&mut Cpu) = nfs4rc::f_8004ad14;
const TRACK_RACED: fn(&mut Cpu) = crate::front::get_track_raced_entry;
const UNITS_KIND: fn(&mut Cpu) = crate::front::get_laps_for_type_entry;
const METRIC: u32 = 2;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn word_x(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_X, &[k])
}

fn word_y(c: &mut Cpu, k: u32) -> u32 {
    call(c, WORD_Y, &[k])
}

/// The halo's breathing: 0..15 and back as its clock goes round 60.
fn breath(clock: i32) -> i32 {
    (if clock < 31 { clock } else { 60i32.wrapping_sub(clock) }) / 2
}

/// `tScreenTrackRecords::GetShapeInfo` (SCREENTRACKRECORDS.CPP, 0x80042a28): 40 shapes of
/// "zRec", none swapped.
pub fn get_shape_info(m: &mut Mem, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w16(permanent, SHAPES);
    m.w16(swap, 0);
    m.w32(name, REC_NAME);
    m.w32(swap_name, 0);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenTrackRecords::Initialize` (SCREENTRACKRECORDS.CPP, 0x80042a4c): the screen's files
/// read; the track raced's records read into memory of their own.
pub fn initialize(c: &mut Cpu, this: Screen) {
    let this = this.0;
    c.m.w32(this + CLOCK, 0);
    fescreen::initialize(c, Screen(this));
    let records = call(c, RESERVE, &[MEMORY_NAME, 360, 16]);
    c.m.w32(this + RECORDS, records);
    let track = call(c, TRACK_RACED, &[]);
    let records = c.m.r32(this + RECORDS);
    call(c, GET_RECORDS, &[sx(track), records]);
    c.m.w32(this + CLEARED, 0);
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, Screen(c.r[4]));
}

/// `tScreenTrackRecords::Cleanup` (SCREENTRACKRECORDS.CPP, 0x80042aa4): the records freed, the
/// screen's.
pub fn cleanup(c: &mut Cpu, this: Screen) {
    let records = c.m.r32(this.0 + RECORDS);
    call(c, PURGE, &[records]);
    c.m.w32(this.0 + RECORDS, 0);
    call(c, RECORDS_DONE, &[]);
    fescreen::cleanup(c, this);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, Screen(c.r[4]));
}

/// `tScreenTrackRecords::DrawOneRecord` (SCREENTRACKRECORDS.CPP, 0x80042ae0): record `k` at
/// `y` (none with no best lap): lit with the halo when new; its name, car, time and best lap.
pub fn draw_one_record(c: &mut Cpu, this: u32, k: u32, lit: u32, y: u32) {
    let record = |m: &mut Mem| m.r32(this + RECORDS).wrapping_add(k.wrapping_mul(RECORD_SIZE));
    let rec = record(&mut c.m);
    if (c.m.r32(rec + BEST_LAP) as i32) < 0 {
        return;
    }
    let lit = lit != 0;
    if lit {
        let x = word_x(c, HALO_AT);
        let size = c.m.r32(this + HALO) as i32;
        call_addr(c, HALO_2D, &[x.wrapping_add(3), y.wrapping_add(4), (size / 2) as u32, (size.wrapping_mul(2) / 3) as u32, 23]);
        let x = word_x(c, HALO_AT);
        call(c, DRAW_SHAPE_EXTENDED, &[0, 0, x, y, 0, 0, 0]);
    }
    let (y, bright) = (sx(y), (lit as u32) << 1);
    let x = word_x(c, NAME_X);
    let name = record(&mut c.m);
    fetextrender::full_text(c, name, sx(x), y, 11, if lit { 2 } else { 1 }, 0);
    let rec = record(&mut c.m);
    let car = c.m.r32(rec + CAR);
    let w = call(c, TEXT_WORD, &[car.wrapping_add(CAR_NAMES)]);
    let x = word_x(c, CAR_X);
    fetextrender::full_text(c, w, sx(x), y, 11, bright, 0);
    with_buffer(c, 128, |c, text| {
        let rec = record(&mut c.m);
        let time = c.m.r32(rec + TIME);
        if time != 0 {
            call(c, PARSE_TIME, &[time, text]);
        } else {
            let w = call(c, TEXT_WORD, &[NO_TIME]);
            call(c, SPRINTF, &[text, w]);
        }
        let x = word_x(c, TIME_X);
        fetextrender::full_text(c, text, sx(x), y, 11, if lit || k != 0 { 2 } else { 0 }, 0);
        let rec = record(&mut c.m);
        let lap = c.m.r32(rec + BEST_LAP);
        call(c, PARSE_TIME, &[lap, text]);
        let x = word_x(c, LAP_X);
        fetextrender::full_text(c, text, sx(x), y, 11, if k != 0 || lit { bright } else { 2 }, 0);
    })
}

pub fn abi_draw_one_record(c: &mut Cpu) {
    draw_one_record(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tScreenTrackRecords::DrawRecords` (SCREENTRACKRECORDS.CPP, 0x80042d50): the halo breathing;
/// the first `count` rows (up to 8: each row's place, its record), with more than 8 the best
/// lap's record. The records are from the 1st in metric units, else from the 9th.
pub fn draw_records(c: &mut Cpu, this: u32, count: u32) {
    let m = &mut c.m;
    let clock = m.r32(this + CLOCK).wrapping_add(1);
    m.w32(this + CLOCK, clock);
    if clock as i32 >= 61 {
        m.w32(this + CLOCK, 0);
    }
    let size = breath(m.r32(this + CLOCK) as i32);
    m.w32(this + HALO, size as u32);
    let size = (m.r32(this + HALO).wrapping_add(20)) << 7;
    m.w32(this + HALO, size);
    let first = if call(c, UNITS_KIND, &[]) == METRIC { 1 } else { 9 };
    let count = sx(count) as i32;
    for k in 0..count.clamp(0, 8) as u32 {
        let w = call(c, TEXT_WORD, &[k + ROWS]);
        let x = word_x(c, PLACE_X);
        let y = word_y(c, k + ROWS);
        let state = if c.m.r32(NEW + 4 * k) != 0 { 2 } else { 0 };
        fetextrender::full_text(c, w, sx(x), sx(y), 11, state, 0);
        let y = word_y(c, k + ROWS);
        let lit = c.m.r32(NEW + 4 * k);
        draw_one_record(c, this, k + first, lit, y);
    }
    if count >= 9 {
        let y = word_y(c, LAP_ROW);
        let lit = c.m.r32(NEW_LAP);
        draw_one_record(c, this, 0, lit, y);
    }
}

pub fn abi_draw_records(c: &mut Cpu) {
    draw_records(c, c.r[4], c.r[5]);
}

/// A colour of the colours' table (by the index at `at`) faded by `t`.
fn color(m: &mut Mem, at: u32, t: u32) -> u32 {
    let k = m.r8(at) as u32;
    fefades::fade(m.r32(COLORS + 4 * k), 0, t)
}

/// `tScreenTrackRecords::DrawBackground` (SCREENTRACKRECORDS.CPP, 0x80042f30). As the screen
/// fades in, the rows roll in and the colours come. It draws the records; the track's name and
/// the units, with a band under them and a mark sliding to and fro under it; the columns'
/// headings and lines; the picture.
pub fn draw_background(c: &mut Cpu, this: Screen) {
    // the fade (0 full, 128 gone) to 0..308: past 180 the texts fade, under it the rows roll
    let fade = this.screen_fade_val(&mut c.m) as i32;
    let v = fade.wrapping_mul(308) / 128;
    let texts = v.wrapping_sub(180).clamp(0, 128) as u32;
    let shade = ((v << 7) / 180).clamp(0, 128) as i16 as i32 as u32;
    let rows = (180 - v.clamp(0, 180)) / 20;
    let (left, top) = (word_x(c, BOX_LEFT), word_y(c, BOX_TOP));
    let width = sx(word_x(c, BOX_RIGHT).wrapping_sub(left));
    let bottom = word_y(c, BOX_BOTTOM) as u16;
    let m = &mut c.m;
    let (text, title) = (color(m, TEXT_COLOR, texts), color(m, TITLE_COLOR, texts));
    draw_records(c, this.0, rows as u32);
    // the track's name and the units, a band under them
    with_buffer(c, 256, |c, at| {
        let (line, units) = (at, at + 128);
        let format = call(c, TEXT_WORD, &[UNITS]);
        let kind = call(c, UNITS_KIND, &[]);
        call(c, SPRINTF, &[units, format, kind]);
        let track = call(c, TRACK_RACED, &[]);
        let name = call(c, TEXT_WORD, &[sx(track).wrapping_add(TRACK_NAMES)]);
        call(c, SPRINTF, &[line, TITLE_FORMAT, name, units]);
        let y = word_y(c, TITLE_Y);
        fetextrender::full_text_rgb(c, line, 260, sx(y), title, 0, 2);
        let px = call(c, TEXT_PIXELS, &[line]);
        let x = 260u32.wrapping_sub(((px as i32) >> 1) as u32);
        let y = word_y(c, TITLE_Y);
        let px = call(c, TEXT_PIXELS, &[line]);
        call_addr(c, DRAW_SQUARE, &[0, x, y, px, 9]);
    });
    // the mark: its halves sliding apart and back under the name, then the band's shape
    let m = &mut c.m;
    let mark = TextureShapeInfo(m.r32(PERMANENT_SHAPES)).at(MARK).0;
    let w = lh(m, mark + 16);
    assert!(w != 0, "DrawBackground: a mark 0 wide (the original's division `break`s)");
    let mut slide = (m.r32(GAME_TICKS) as i32).wrapping_rem(w);
    if w / 2 < slide {
        slide = w - slide;
    }
    let mid = ((w >> 1) - 2).wrapping_sub(lh(m, mark + 20));
    for x in [mid.wrapping_add(slide), mid.wrapping_sub(slide)] {
        let y = word_y(c, TITLE_Y);
        let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[MARK_HALF, 0, x as u32, y.wrapping_add(1), fade, 1, 0]);
    }
    with_buffer(c, DrawShapeExtended::SIZE, |c, at| {
        DrawShapeExtended(at).set_tint(&mut c.m, 0, BAND as i32);
        let fade = this.screen_fade_val(&mut c.m) as i32 as u32;
        call(c, DRAW_SHAPE_EXTENDED, &[MARK, 0x410, (-2i32) as u32, 0, fade, 0, at]);
    });
    // the columns' headings (their dividers while rows show), the best lap's row's
    for k in 0..3 {
        let x = word_x(c, k + CAR_X);
        let w = call(c, TEXT_WORD, &[k + HEADINGS]);
        fetextrender::full_text_rgb(c, w, sx(x), sx(top.wrapping_add(4)), text, 0, 0);
        if rows > 0 {
            let alpha = (128i32.wrapping_sub(shade as i32)) / 16;
            call_addr(c, DRAW_SQUARE, &[LINE, x.wrapping_sub(6), sx(top).wrapping_add(2), 2, alpha as u32]);
        }
    }
    let w = call(c, TEXT_WORD, &[LAP_HEADING]);
    let x = word_x(c, PLACE_X);
    fetextrender::full_text_rgb(c, w, sx(x), sx((bottom as u32).wrapping_add(3)), text, 0, 0);
    // the lines: along the top, down the car's column
    let (left, top) = (sx(left), sx(top));
    call_addr(c, DRAW_BRIGHT_END_LINE, &[LINE, left, top.wrapping_add(3), width, u32::MAX, 2, shade, 35]);
    let x = word_x(c, CAR_X);
    let down = ((bottom as i16 as i32) - 12) as u32;
    call_addr(c, DRAW_BRIGHT_END_LINE, &[LINE, x.wrapping_sub(6), top.wrapping_add(4), 2, down.wrapping_sub(top), 1, shade, 0]);
    if rows as i16 >= 9 {
        let x = word_x(c, CAR_X);
        let y = word_y(c, LAP_ROW);
        call_addr(c, DRAW_SQUARE, &[LINE, x.wrapping_sub(6), y.wrapping_sub(1), 2, 8]);
    }
    // the picture
    let table = TextureShapeInfo(c.m.r32(PERMANENT_SHAPES));
    fescreen::draw_background_image(c, this, 11, 27, table, 0);
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, Screen(c.r[4]));
}

/// `tScreenTrackRecords::~tScreenTrackRecords` (SCREENTRACKRECORDS.H, 0x800434e0): the
/// screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::FRONT_END;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The screen's fade.
    const FADE: u32 = 92;

    /// The game's track records screen (its vtable the real one); 18 records; the outputs of
    /// `GetShapeInfo`.
    const SCREEN: u32 = 0x8017_F8B8;
    const BUFFER: u32 = 0x8014_7100;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the screen: its fade about where the rows and colours come,
    /// the halo's clock about its turns, 18 records (names of up to seven letters, any car,
    /// a time (1 now and then) or none, a best lap or none), which are new; the clock; the
    /// units either.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let s = SCREEN;
        m.w16(s + FADE, [0, 1, 74, 75, 76, 83, 84, 128, pick(rng, 129)][pick(rng, 9) as usize] as u16);
        m.w32(s + RECORDS, BUFFER);
        m.w32(s + CLOCK, [0, 29, 30, 31, 59, 60, pick(rng, 61)][pick(rng, 7) as usize]);
        m.w32(s + HALO, pick(rng, 5000));
        for k in 0..18 {
            let rec = BUFFER + RECORD_SIZE * k;
            let n = pick(rng, 8);
            for j in 0..8 {
                m.w8(rec + j, if j < n { b'A' + pick(rng, 26) as u8 } else { 0 });
            }
            m.w32(rec + CAR, pick(rng, 30));
            m.w32(rec + TIME, [0, 1, 1 + pick(rng, 100_000)][pick(rng, 3) as usize]);
            m.w32(rec + BEST_LAP, [u32::MAX, 0, 1 + pick(rng, 50_000)][pick(rng, 3) as usize]);
        }
        for k in 0..9 {
            m.w32(NEW + 4 * k, (pick(rng, 4) == 0) as u32);
        }
        m.w32(GAME_TICKS, pick(rng, 1_000_000));
        // (the units: a single race's, by its track's setting)
        m.w8(FRONT_END + 4, 0);
        m.w8(FRONT_END + 0x46, 0);
        m.w8(FRONT_END + 45, pick(rng, 2) as u8);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenTrackRecords::GetShapeInfo", 0x8004_2a28, nfs4rc::f_80042a28, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("tScreenTrackRecords::Initialize", 0x8004_2a4c, nfs4rc::f_80042a4c, |_, _| vec![SCREEN]),
            ("tScreenTrackRecords::Cleanup", 0x8004_2aa4, nfs4rc::f_80042aa4, |_, _| vec![SCREEN]),
            ("DrawOneRecord", 0x8004_2ae0, nfs4rc::f_80042ae0, |_, r| vec![SCREEN, pick(r, 18), pick(r, 2), pick(r, 240)]),
            ("DrawRecords", 0x8004_2d50, nfs4rc::f_80042d50, |_, r| vec![SCREEN, [0, 1, 8, 9, pick(r, 10)][pick(r, 5) as usize]]),
            ("tScreenTrackRecords::DrawBackground", 0x8004_2f30, nfs4rc::f_80042f30, |_, _| vec![SCREEN]),
            ("~tScreenTrackRecords", 0x8004_34e0, nfs4rc::f_800434e0, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screentrackrecords_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7EC ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7ED + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
