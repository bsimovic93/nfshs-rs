//! SCREENTRACKRECORDS.CPP: the records of the track just raced (`tScreenTrackRecords`): its
//! records read (`Stattool_GetRecords`, 18 of 20 bytes: a name, the car, the time, the best
//! lap), a heading per column and a row per record, the new ones lit with a breathing halo,
//! rolling in as the screen fades in; the track's name and the units, a mark sliding to and fro
//! under it, the columns' headings and lines.
//!
//! The squares are the host's (called by address).

use crate::abi::Saved;
use crate::femenuextended::{DRAW_SHAPE_EXTENDED, SPRINTF, TEXT_PIXELS, TEXT_WORD};
use crate::globals::GAME_TICKS;
use crate::{fefades, felines, fescreen, fetextrender};
use ps1rt::{Cpu, Mem};

// The screen: a `tScreen` (+92 its fade), then +100 the records (`tRecordBuffer`, 20 bytes
// each: +0 the name, +8 the car, +12 the time (0: none), +16 the best lap (negative: no record)),
// +104 the halo's size, +108 its clock (0..60), +112 cleared.
const FADE: u32 = 92;
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
/// The permanent shapes' table now.
const PERMANENT_SHAPES: u32 = 0x8005_1C54;
/// "zRec", "trkrcrds", "%s %s".
const REC_NAME: u32 = 0x8001_1F84;
const MEMORY_NAME: u32 = 0x8001_1F8C;
const TITLE_FORMAT: u32 = 0x8001_1F98;
/// The places' texts: x by `TextSys_WordX`, y by `TextSys_WordY`.
const HALO_AT: u32 = 584;
const NAME_X: u32 = 588;
const CAR_X: u32 = 589;
const TIME_X: u32 = 590;
const LAP_X: u32 = 591;
const ROWS: u32 = 600;
const LAP_ROW: u32 = 609;
/// The cars' names (from 340), "--" (no time), the units' format, the tracks' names (from 214).
const CAR_NAMES: u32 = 340;
const NO_TIME: u32 = 610;
const UNITS: u32 = 594;
const TRACK_NAMES: u32 = 214;

/// `PSXDrawSquare`: the host's.
const DRAW_SQUARE: u32 = 0x8004_F12C;
/// `TextSys_WordX`, `TextSys_WordY`, `Flare_2DHalo`, `Stattool_ParseTime`,
/// `Stattool_GetRecords`, the records' cleanup (0x8004ad14), `Front_GetTrackRaced`, the units
/// (0x80028004: 2 metric), `reservememadr`, `purgememadr`.
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
/// `Flare_2DHalo`, hooked (the host captures its primitives): called by address.
const HALO_2D: u32 = 0x800C_ECD0;
const PARSE_TIME: fn(&mut Cpu) = nfs4rc::f_8004ae74;
const GET_RECORDS: fn(&mut Cpu) = nfs4rc::f_8004b2f4;
const RECORDS_DONE: fn(&mut Cpu) = nfs4rc::f_8004ad14;
const TRACK_RACED: fn(&mut Cpu) = crate::front::get_track_raced_entry;
const UNITS_KIND: fn(&mut Cpu) = crate::front::get_laps_for_type_entry;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

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

/// The halo's breathing: 0..15 and back as its clock goes round 60.
fn breath(clock: i32) -> i32 {
    (if clock < 31 { clock } else { 60i32.wrapping_sub(clock) }) / 2
}

/// `tScreenTrackRecords::GetShapeInfo` (SCREENTRACKRECORDS.CPP, 0x80042a28): 40 shapes of
/// "zRec", none swapped.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w16(permanent, 40);
    m.w16(swap, 0);
    m.w32(name, REC_NAME);
    m.w32(swap_name, 0);
}

/// `tScreenTrackRecords::Initialize` (SCREENTRACKRECORDS.CPP, 0x80042a4c): the screen's files
/// read; the track raced's records read into memory of their own.
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    crate::abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w32(this + CLOCK, 0);
        port(c, &r, 0x8002_60cc, 0x8004_2a64, &[this], fescreen::abi_initialize);
        let records = r.call_leaving(c, RESERVE, &[MEMORY_NAME, 360, 16], 0x8004_2a78);
        c.m.w32(this + RECORDS, records);
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_2a80);
        let records = c.m.r32(this + RECORDS);
        r.call_leaving(c, GET_RECORDS, &[sx(track), records], 0x8004_2a90);
        c.m.w32(this + CLEARED, 0);
    })
}

/// `tScreenTrackRecords::Cleanup` (SCREENTRACKRECORDS.CPP, 0x80042aa4): the records freed, the
/// screen's.
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    crate::abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let records = c.m.r32(this + RECORDS);
        r.call_leaving(c, PURGE, &[records], 0x8004_2ac0);
        c.m.w32(this + RECORDS, 0);
        r.call_leaving(c, RECORDS_DONE, &[], 0x8004_2ac8);
        port(c, &r, 0x8002_61c0, 0x8004_2ad0, &[this], fescreen::abi_cleanup);
    })
}

/// `tScreenTrackRecords::DrawOneRecord` (SCREENTRACKRECORDS.CPP, 0x80042ae0): record `k` at
/// `y` (none with no best lap): lit with the halo when new; its name, car, time and best lap.
pub fn draw_one_record(c: &mut Cpu, this: u32, k: u32, lit: u32, y: u32) {
    crate::abi::frame(c, 144, |c, sp| {
        let mut r = Saved::of(c);
        r.0[4] = this;
        r.0[2] = k;
        r.0[6] = lit;
        r.0[5] = y;
        let at = k.wrapping_mul(RECORD_SIZE);
        r.0[0] = at;
        let lit2 = ((lit != 0) as u32) << 1;
        r.0[3] = lit2;
        let record = |m: &mut Mem| m.r32(this + RECORDS).wrapping_add(at);
        let rec = record(&mut c.m);
        if (c.m.r32(rec + BEST_LAP) as i32) < 0 {
            return;
        }
        if lit != 0 {
            let x = r.call_leaving(c, WORD_X, &[HALO_AT], 0x8004_2b4c);
            let size = c.m.r32(this + HALO) as i32;
            r.call_ptr_leaving(c, HALO_2D, &[x.wrapping_add(3), y.wrapping_add(4), (size / 2) as u32, (size.wrapping_mul(2) / 3) as u32, 23], 0x8004_2b8c);
            let x = r.call_leaving(c, WORD_X, &[HALO_AT], 0x8004_2b94);
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[0, 0, x, y, 0, 0, 0], 0x8004_2bb4);
        }
        let x = r.call_leaving(c, WORD_X, &[NAME_X], 0x8004_2bbc);
        let name = record(&mut c.m);
        let state = if lit != 0 { 2 } else { 1 };
        r.call_leaving(c, fetextrender::full_text_entry, &[name, sx(x), sx(y), 11, state, 0], 0x8004_2bf4);
        let rec = record(&mut c.m);
        let car = c.m.r32(rec + CAR);
        let w = r.call_leaving(c, TEXT_WORD, &[car.wrapping_add(CAR_NAMES)], 0x8004_2c14);
        r.0[1] = w;
        let x = r.call_leaving(c, WORD_X, &[CAR_X], 0x8004_2c20);
        r.call_leaving(c, fetextrender::full_text_entry, &[w, sx(x), sx(y), 11, lit2, 0], 0x8004_2c44);
        let rec = record(&mut c.m);
        r.0[0] = rec;
        let text = sp + 32;
        let time = c.m.r32(rec + TIME);
        if time != 0 {
            r.call_leaving(c, PARSE_TIME, &[time, text], 0x8004_2c68);
        } else {
            let w = r.call_leaving(c, TEXT_WORD, &[NO_TIME], 0x8004_2c78);
            r.call_leaving(c, SPRINTF, &[text, w], 0x8004_2c84);
        }
        let x = r.call_leaving(c, WORD_X, &[TIME_X], 0x8004_2c8c);
        let state = if lit != 0 || k != 0 { 2 } else { 0 };
        r.call_leaving(c, fetextrender::full_text_entry, &[text, sx(x), sx(y), 11, state, 0], 0x8004_2cc8);
        let rec = record(&mut c.m);
        let lap = c.m.r32(rec + BEST_LAP);
        r.call_leaving(c, PARSE_TIME, &[lap, text], 0x8004_2ce8);
        let x = r.call_leaving(c, WORD_X, &[LAP_X], 0x8004_2cf0);
        let state = if k != 0 || lit2 != 0 { lit2 } else { 2 };
        r.call_leaving(c, fetextrender::full_text_entry, &[text, sx(x), sx(y), 11, state, 0], 0x8004_2d28);
    })
}

pub fn abi_draw_one_record(c: &mut Cpu) {
    draw_one_record(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tScreenTrackRecords::DrawRecords` (SCREENTRACKRECORDS.CPP, 0x80042d50): the halo breathing;
/// the first `count` rows (up to 8: each row's place, its record), with more than 8 the best
/// lap's record.
pub fn draw_records(c: &mut Cpu, this: u32, count: u32) {
    crate::abi::frame(c, 64, |c, _| {
        let mut r = Saved::of(c);
        r.0[4] = this;
        r.0[8] = count;
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
        let units = r.call_leaving(c, UNITS_KIND, &[], 0x8004_2de4);
        let first = if units == 2 { 1 } else { 9 };
        r.0[2] = 0;
        r.0[7] = sx(count);
        r.0[5] = NEW;
        r.0[6] = first;
        loop {
            let k = sx(r.0[2]);
            r.0[1] = k;
            r.0[0] = k.wrapping_add(ROWS);
            if k as i32 >= sx(count) as i32 {
                break;
            }
            let w = r.call_leaving(c, TEXT_WORD, &[k.wrapping_add(ROWS)], 0x8004_2e28);
            r.0[3] = w;
            let x = r.call_leaving(c, WORD_X, &[586], 0x8004_2e34);
            r.0[0] = sx(x);
            let y = r.call_leaving(c, WORD_Y, &[k.wrapping_add(ROWS)], 0x8004_2e44);
            let state = if c.m.r32(NEW.wrapping_add(k << 2)) != 0 { 2 } else { 0 };
            r.call_leaving(c, fetextrender::full_text_entry, &[w, sx(x), sx(y), 11, state, 0], 0x8004_2e84);
            r.0[0] = k;
            let y = r.call_leaving(c, WORD_Y, &[k.wrapping_add(ROWS)], 0x8004_2e94);
            r.0[0] = NEW.wrapping_add(k << 2);
            let lit = c.m.r32(r.0[0]);
            port(c, &r, 0x8004_2ae0, 0x8004_2eb0, &[this, k.wrapping_add(first), lit, y], abi_draw_one_record);
            r.0[2] = r.0[2].wrapping_add(1);
            if sx(r.0[2]) as i32 >= 8 {
                break;
            }
        }
        if sx(count) as i32 >= 9 {
            let y = r.call_leaving(c, WORD_Y, &[LAP_ROW], 0x8004_2ee8);
            let lit = c.m.r32(NEW_LAP);
            port(c, &r, 0x8004_2ae0, 0x8004_2f00, &[this, 0, lit, y], abi_draw_one_record);
        }
    })
}

pub fn abi_draw_records(c: &mut Cpu) {
    draw_records(c, c.r[4], c.r[5]);
}

/// `tScreenTrackRecords::DrawBackground` (SCREENTRACKRECORDS.CPP, 0x80042f30): as the screen
/// fades in, the rows rolling in and the colours coming; the records; the track's name and the
/// units with a band under them, a mark sliding to and fro under it; the columns' headings and
/// lines; the picture.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    crate::abi::frame(c, 264, |c, sp| {
        let mut r = Saved::of(c);
        c.m.w32(sp + 264, this);
        let v = lh(&mut c.m, this + FADE).wrapping_mul(308) / 128;
        r.0[2] = v.wrapping_sub(180).clamp(0, 128) as u32;
        let rolled = v.clamp(0, 180);
        r.0[5] = ((v << 7) / 180).clamp(0, 128) as u32;
        r.0[3] = ((180 - sx(rolled as u32) as i32) / 20) as u32;
        let x1 = r.call_leaving(c, WORD_X, &[585], 0x8004_3040);
        r.0[4] = x1;
        let y1 = r.call_leaving(c, WORD_Y, &[599], 0x8004_304c);
        r.0[7] = y1;
        let x2 = r.call_leaving(c, WORD_X, &[592], 0x8004_3058);
        c.m.w16(sp + 200, x2.wrapping_sub(x1) as u16);
        let y2 = r.call_leaving(c, WORD_Y, &[608], 0x8004_3068);
        r.0[2] = sx(r.0[2]);
        r.0[1] = COLORS;
        r.0[0] = 0x8005_1AA0;
        c.m.w16(sp + 208, y2 as u16);
        let color = |m: &mut Mem, at: u32| {
            let k = m.r8(at) as u32;
            m.r32(COLORS + (k << 2))
        };
        let text_color = color(&mut c.m, TEXT_COLOR);
        let shown = r.0[2];
        let text = port(c, &r, 0x8004_bd68, 0x8004_309c, &[text_color, shown], fefades::abi_fade_out);
        r.0[8] = text;
        let title_color = color(&mut c.m, TITLE_COLOR);
        let title = port(c, &r, 0x8004_bd68, 0x8004_30b8, &[title_color, shown], fefades::abi_fade_out);
        let rows = sx(r.0[3]);
        r.0[2] = title;
        c.m.w32(sp + 216, rows);
        port(c, &r, 0x8004_2d50, 0x8004_30d4, &[this, rows], abi_draw_records);
        // the track's name and the units
        let format = r.call_leaving(c, TEXT_WORD, &[UNITS], 0x8004_30dc);
        r.0[0] = format;
        let units = r.call_leaving(c, UNITS_KIND, &[], 0x8004_30e4);
        let unit_text = sp + 88;
        r.0[1] = unit_text;
        r.call_leaving(c, SPRINTF, &[unit_text, format, units], 0x8004_30f8);
        r.0[0] = 260;
        let track = r.call_leaving(c, TRACK_RACED, &[], 0x8004_3100);
        let name = r.call_leaving(c, TEXT_WORD, &[sx(track).wrapping_add(TRACK_NAMES)], 0x8004_3110);
        let line = sp + 32;
        r.call_leaving(c, SPRINTF, &[line, TITLE_FORMAT, name, unit_text], 0x8004_3128);
        let y = r.call_leaving(c, WORD_Y, &[598], 0x8004_3130);
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[line, 260, sx(y), title, 0, 2], 0x8004_3154);
        let px = r.call_leaving(c, TEXT_PIXELS, &[line], 0x8004_315c);
        let x = 260u32.wrapping_sub(((px as i32) >> 1) as u32);
        r.0[0] = x;
        let y = r.call_leaving(c, WORD_Y, &[598], 0x8004_316c);
        r.0[1] = y;
        let px = r.call_leaving(c, TEXT_PIXELS, &[line], 0x8004_3178);
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, x, y, px, 9], 0x8004_3194);
        // the mark sliding under it
        let m = &mut c.m;
        let shape = m.r32(PERMANENT_SHAPES) + 1216;
        let w = lh(m, shape + 16);
        assert!(w != 0, "DrawBackground: a mark 0 wide (the original's division `break`s)");
        let mut slide = (m.r32(GAME_TICKS) as i32).wrapping_rem(w);
        r.0[6] = 0x0023_2323;
        m.w16(sp + 168, r.0[5] as u16);
        m.w16(sp + 176, r.0[3] as u16);
        m.w16(sp + 184, r.0[4] as u16);
        m.w16(sp + 192, r.0[7] as u16);
        if w / 2 < slide {
            slide = w - slide;
        }
        let mid = ((w >> 1) - 2).wrapping_sub(lh(m, shape + 20));
        r.0[2] = slide as u32;
        r.0[3] = mid as u32;
        let y = r.call_leaving(c, WORD_Y, &[598], 0x8004_3228);
        let v = c.m.r32(sp + 216);
        c.m.w32(sp + 220, v);
        r.0[1] = 0;
        r.0[0] = 1;
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[39, 0, mid.wrapping_add(slide) as u32, y.wrapping_add(1), fade, 1, 0], 0x8004_3260);
        let y = r.call_leaving(c, WORD_Y, &[598], 0x8004_3268);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[39, 0, mid.wrapping_sub(slide) as u32, y.wrapping_add(1), fade, 1, 0], 0x8004_3290);
        c.m.w32(sp + 148, 0x0050_5050);
        let fade = lh(&mut c.m, this + FADE) as u32;
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[38, 1040, (-2i32) as u32, 0, fade, 0, sp + 144], 0x8004_32c8);
        // the columns' headings
        let top = r.0[7];
        r.0[4] = top.wrapping_add(4) << 16;
        r.0[3] = sx(top).wrapping_add(2);
        r.0[2] = sx(r.0[5]);
        r.0[1] = 0;
        loop {
            let k = sx(r.0[1]);
            r.0[0] = k;
            if k as i32 >= 3 {
                break;
            }
            let x = r.call_leaving(c, WORD_X, &[k + CAR_X], 0x8004_3300);
            r.0[0] = x;
            let w = r.call_leaving(c, TEXT_WORD, &[k + 595], 0x8004_330c);
            r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, sx(x), ((r.0[4] as i32) >> 16) as u32, text, 0, 0], 0x8004_332c);
            if (c.m.r32(sp + 220) as i32) > 0 {
                let alpha = (128i32.wrapping_sub(r.0[2] as i32)) / 16;
                r.call_ptr_leaving(c, DRAW_SQUARE, &[0x0023_2323, x.wrapping_sub(6), r.0[3], 2, alpha as u32], 0x8004_3364);
            }
            r.0[1] = r.0[1].wrapping_add(1);
        }
        let w = r.call_leaving(c, TEXT_WORD, &[611], 0x8004_3374);
        r.0[0] = w;
        let x = r.call_leaving(c, WORD_X, &[586], 0x8004_3380);
        let y = sx((c.m.r16(sp + 208) as u32).wrapping_add(3));
        r.call_leaving(c, fetextrender::full_text_rgb_entry, &[w, sx(x), y, text, 0, 0], 0x8004_33ac);
        // the lines
        let m = &mut c.m;
        let (left, top, width, shade) = (lh(m, sp + 184) as u32, lh(m, sp + 192) as u32, lh(m, sp + 200) as u32, lh(m, sp + 168) as u32);
        r.0[1] = top;
        r.0[0] = shade;
        for (k, v) in [u32::MAX, 2, shade, 35].into_iter().enumerate() {
            m.w32(sp + 16 + 4 * k as u32, v);
        }
        port(c, &r, 0x8004_c018, 0x8004_3404, &[0x0023_2323, left, top.wrapping_add(3), width], felines::abi_draw_bright_end_line);
        let x = r.call_leaving(c, WORD_X, &[CAR_X], 0x8004_340c);
        let m = &mut c.m;
        let down = (lh(m, sp + 208) - 12) as u32;
        for (k, v) in [down.wrapping_sub(top), 1, shade, 0].into_iter().enumerate() {
            m.w32(sp + 16 + 4 * k as u32, v);
        }
        port(c, &r, 0x8004_c018, 0x8004_3448, &[0x0023_2323, x.wrapping_sub(6), top.wrapping_add(4), 2], felines::abi_draw_bright_end_line);
        if lh(&mut c.m, sp + 176) >= 9 {
            let x = r.call_leaving(c, WORD_X, &[CAR_X], 0x8004_346c);
            r.0[0] = x.wrapping_sub(6);
            let y = r.call_leaving(c, WORD_Y, &[LAP_ROW], 0x8004_3478);
            r.call_ptr_leaving(c, DRAW_SQUARE, &[0x0023_2323, x.wrapping_sub(6), y.wrapping_sub(1), 2, 8], 0x8004_3494);
        }
        // the picture
        let table = c.m.r32(PERMANENT_SHAPES);
        c.m.w32(sp + 16, 0);
        port(c, &r, 0x8002_5ce8, 0x8004_34b0, &[this, 11, 27, table], fescreen::abi_draw_background_image);
    })
}

/// `tScreenTrackRecords::~tScreenTrackRecords` (SCREENTRACKRECORDS.H, 0x800434e0): the
/// screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    crate::abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_34f0, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::globals::FRONT_END;
    use crate::test_util::{Rng, against_original_in, fe_ram};

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
