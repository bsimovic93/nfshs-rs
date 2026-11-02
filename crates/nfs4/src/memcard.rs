//! MEMCARD.C: the game's memory card layer over libmcrd (the host's `MemCard*`): the cards'
//! directories, loading, saving and deleting a file (with the save's title in Shift-JIS and
//! its icons), formatting, and the card events' state machine, which the front end polls.
//!
//! The state (`S`, 0x800531F8): +0 the region, +4 the file prefix (`BI` and the game's code),
//! +20.., +32 and +36 the confirm callbacks (format, overwrite), +40 and +44 the wait
//! callbacks, +48 the operation pending (1 load the card, 2 write, 3 read, 4 delete), +52 done,
//! +56, +60 the card's port, +64 the cards' timers (8 words). The request (`R`, `S` +608): +0
//! the card, +4 the file name, +28 the title out, +32 the size, +36 the offset, +40 the flags,
//! +44 the file header (`SC`, the icon's frames, the blocks, +48 the title in Shift-JIS, +140
//! the CLUT, +172/+300/+428 the icons), +556 the data, +560 the icons out. The cards (`S`
//! +1180, 616 bytes): +0 the state, +4 the event, +8, +12 the free blocks, +16 the directory
//! (15 entries of 40 bytes: +0 the name, +24 the size).
//!
//! Written as FEMENU.CPP is (see there). libmcrd is the host's (called by address).

use crate::call::{call, call_addr, with_buffer};
use ps1rt::{Cpu, Mem};

const S: u32 = 0x8005_31F8;
pub(crate) const STATE: u32 = S;
const R: u32 = S + 608;
const CARDS: u32 = S + 1180;
const CARD_SIZE: u32 = 616;
const CARD_COUNT: u32 = 8;
const STATE_SIZE: u32 = 6108;
// The state: the region, the file prefix, the format and overwrite callbacks, the wait
// callbacks, the operation pending, done, the card's port, the cards' timers.
const REGION: u32 = 0;
const PREFIX: u32 = 4;
const FORMAT_CB: u32 = 32;
const OVERWRITE_CB: u32 = 36;
const WAIT_CB1: u32 = 40;
const WAIT_CB2: u32 = 44;
const PENDING: u32 = 48;
const DONE: u32 = 52;
const PORT: u32 = 60;
const TIMERS: u32 = 64;
/// The operations pending: the card loaded, a write, a read, a delete; the library's
/// operations (for `HandleError`): the directory, a write, a read.
const PENDING_LOAD_CARD: u32 = 1;
const PENDING_WRITE: u32 = 2;
const PENDING_READ: u32 = 3;
const PENDING_DELETE: u32 = 4;
const OP_DIRECTORY: u32 = 1;
const OP_WRITE: u32 = 2;
const OP_READ: u32 = 3;
// The request: the card, the file name, the title out, the data's size and offset, the flags
// (0x200 a new file), the file header (+2 the icon's frames, +3 the blocks, +4 the title, +96
// the CLUT, +128 the three icons: 512 bytes), the data, the icons out.
const R_CARD: u32 = 0;
const R_NAME: u32 = 4;
const R_TITLE: u32 = 28;
const R_DATA_SIZE: u32 = 32;
const R_OFFSET: u32 = 36;
const R_FLAGS: u32 = 40;
const HEADER: u32 = 44;
const HEADER_FRAMES: u32 = 46;
const HEADER_BLOCKS: u32 = 47;
const HEADER_TITLE: u32 = 48;
const HEADER_CLUT: u32 = 140;
const HEADER_ICONS: u32 = 172;
const HEADER_SIZE: u32 = 512;
const R_DATA: u32 = 556;
const R_ICONS: u32 = 560;
const R_SIZE: u32 = 572;
const NEW_FILE: u32 = 0x200;
// A card: its state (-1 none, -2 unformatted, -3 full, -4 bad), its event, its free blocks, its
// directory (15 entries of 40 bytes: +24 the size).
const CARD_STATE: u32 = 0;
const CARD_EVENT: u32 = 4;
const CARD_FREE: u32 = 12;
const DIRECTORY: u32 = 16;
const ENTRY_SIZE: u32 = 40;
const ENTRY_BYTES: u32 = 24;
const DIRECTORY_ENTRIES: u32 = 15;
const NO_CARD: u32 = u32::MAX;
const UNFORMATTED: u32 = (-2i32) as u32;
const FULL: u32 = (-3i32) as u32;
const BAD_CARD: u32 = (-4i32) as u32;
/// The vertical blank's count (the cards' timers start from it); the timer's handler.
const VSYNC_COUNT: u32 = 0x8013_EFA4;
const TIMER_SUB: u32 = 0x8005_0774;
/// The default callback, the wait callback (both the original's).
const DEFAULT_CB: u32 = 0x8005_0BD0;
const WAIT_CB: u32 = 0x800F_6A70;
/// The directory's pattern (`"*"`), the jump tables' data: Shift-JIS conversion tables.
const ALL_FILES: u32 = 0x8001_2834;
const SJIS_PUNCTUATION: u32 = 0x8005_2F14;
const SJIS_RANGES: u32 = 0x8005_2F08;
const ASCII_PUNCTUATION: u32 = 0x8005_2F60;
const ASCII_RANGES: u32 = 0x8005_2F58;

const BLOCKCLEAR: fn(&mut Cpu) = nfs4rc::f_800f24d4;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const STRCPY: fn(&mut Cpu) = nfs4rc::f_800e6714;
const STRCAT: fn(&mut Cpu) = nfs4rc::f_800e84d4;
const STRNCPY: fn(&mut Cpu) = nfs4rc::f_800f6a60;
const STRCMP: fn(&mut Cpu) = nfs4rc::f_800e6968;
const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const TIMED_WAIT: fn(&mut Cpu) = nfs4rc::f_800e8e70;
const ADD_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb944;
const DEL_TIMER: fn(&mut Cpu) = nfs4rc::f_800eb9a4;
/// A CLUT's place for an icon; a texture page for one; the icon's CLUT.
const ICON_CLUT: fn(&mut Cpu) = nfs4rc::f_800fcc40;
const ICON_TPAGE: fn(&mut Cpu) = nfs4rc::f_800fcbfc;
const SHAPE_CLUT: fn(&mut Cpu) = nfs4rc::f_800f7598;
/// libmcrd (the host's).
const MC_INIT: u32 = 0x800F_B458;
const MC_START: u32 = 0x800F_B4A4;
const MC_STOP: u32 = 0x800F_B4F4;
const MC_END: u32 = 0x800F_B484;
const MC_SYNC: u32 = 0x800F_C4A8;
const MC_ACCEPT: u32 = 0x800F_B770;
const MC_EXIST: u32 = 0x800F_B534;
const MC_READ: u32 = 0x800F_BDF4;
const MC_WRITE: u32 = 0x800F_C014;
const MC_CREATE: u32 = 0x800F_C5C4;
const MC_DELETE: u32 = 0x800F_C7CC;
const MC_FORMAT: u32 = 0x800F_C988;
const MC_DIRENTRY: u32 = 0x800F_C234;
const VSYNC: u32 = 0x800F_2C78;

/// A block size in 8 KB blocks, rounded toward zero.
fn blocks(v: u32) -> u32 {
    ((v as i32) / 8192) as u32
}

/// The card port of the card the last `getcard` gave.
fn card_port(m: &mut Mem) -> u32 {
    m.r32(S + PORT)
}

/// `MCRD_init` (MEMCARD.C, 0x8004f8c8): the state cleared, the callbacks the defaults, the
/// library started, the eight cards set up (a moment waited before each), the timer added.
pub fn init(c: &mut Cpu, arg: u32) {
    call(c, BLOCKCLEAR, &[S, STATE_SIZE]);
    let m = &mut c.m;
    m.w32(S + FORMAT_CB, DEFAULT_CB);
    m.w32(S + OVERWRITE_CB, DEFAULT_CB);
    m.w32(S + PENDING, 0);
    m.w32(S + 56, arg);
    m.w32(S + WAIT_CB2, WAIT_CB);
    m.w32(S + WAIT_CB1, WAIT_CB);
    call_addr(c, MC_INIT, &[1]);
    for card in 1..=CARD_COUNT {
        call(c, TIMED_WAIT, &[10]);
        init_card(c, card);
    }
    call(c, ADD_TIMER, &[TIMER_SUB]);
    call_addr(c, MC_START, &[]);
}

pub fn abi_init(c: &mut Cpu) {
    init(c, c.r[4]);
}

/// `iMCRD_InitCard` (MEMCARD.C, 0x8004f968): card `card` unknown (-1), its event none, 15
/// blocks free, its directory cleared. 0, or -1 for no card.
pub fn init_card(c: &mut Cpu, card: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    if at == 0 {
        return u32::MAX;
    }
    let m = &mut c.m;
    m.w32(at + CARD_STATE, NO_CARD);
    m.w32(at + CARD_EVENT, 0);
    m.w32(at + 8, 0);
    m.w32(at + CARD_FREE, 15);
    call(c, BLOCKCLEAR, &[at + DIRECTORY, 600]);
    0
}

pub fn abi_init_card(c: &mut Cpu) {
    c.r[2] = init_card(c, c.r[4]);
}

/// `MCRD_restore` (MEMCARD.C, 0x8004f9c0): the timer removed, the library synced, stopped and
/// ended.
pub fn restore(c: &mut Cpu) {
    call(c, DEL_TIMER, &[TIMER_SUB]);
    call_addr(c, MC_SYNC, &[0, 0, 0]);
    call_addr(c, MC_STOP, &[]);
    call_addr(c, MC_END, &[]);
}

pub fn abi_restore(c: &mut Cpu) {
    restore(c);
}

/// Eight bytes copied (the original's unaligned word pair).
fn copy8(m: &mut Mem, from: u32, to: u32) {
    let v: Vec<u8> = (0..8).map(|k| m.r8(from + k)).collect();
    for (k, b) in v.into_iter().enumerate() {
        m.w8(to + k as u32, b);
    }
}

/// The options' callbacks and their places in the state (`MCRD_OPTS` +20.. against `S` +32..,
/// and +8 against +20).
const OPTION_WORDS: [(u32, u32); 5] = [(20, 32), (24, 36), (28, 40), (32, 44), (8, 20)];

/// `MCRD_getopts` (MEMCARD.C, 0x8004fa04): the options into `o` (the region, no prefix, the
/// callbacks, +8, the 8 bytes at +12).
pub fn getopts(m: &mut Mem, o: u32) {
    let region = m.r32(S + REGION);
    m.w32(o + 4, 0);
    m.w32(o, region);
    for (to, from) in OPTION_WORDS {
        let v = m.r32(S + from);
        m.w32(o + to, v);
    }
    copy8(m, S + 24, o + 12);
}

pub fn abi_getopts(c: &mut Cpu) {
    getopts(&mut c.m, c.r[4]);
}

/// `MCRD_setopts` (MEMCARD.C, 0x8004fa7c): the options from `o`: with a prefix, `B` and the
/// region's letter (`I` 1, `E` 2, else `A`) and the prefix (10 letters), and the region; else
/// none; the callbacks, +20, the 8 bytes at +24.
pub fn setopts(c: &mut Cpu, o: u32) {
    if c.m.r32(o + 4) != 0 {
        let m = &mut c.m;
        m.w8(S + PREFIX, b'B');
        let letter = match m.r32(o) {
            1 => b'I',
            2 => b'E',
            _ => b'A',
        };
        m.w8(S + PREFIX + 1, letter);
        let prefix = m.r32(o + 4);
        call(c, STRNCPY, &[S + PREFIX + 2, prefix, 10]);
        let m = &mut c.m;
        m.w8(S + PREFIX + 12, 0);
        let region = m.r32(o);
        m.w32(S + REGION, region);
    } else {
        c.m.w8(S + PREFIX, 0);
        c.m.w32(S + REGION, 0);
    }
    let m = &mut c.m;
    for (from, to) in OPTION_WORDS {
        let v = m.r32(o + from);
        m.w32(S + to, v);
    }
    copy8(m, o + 12, S + 24);
}

pub fn abi_setopts(c: &mut Cpu) {
    setopts(c, c.r[4]);
}

/// The request's file name: the prefix (unless `bare`) and `name`.
fn request_name(c: &mut Cpu, name: u32, bare: bool) {
    if !bare {
        call(c, STRCPY, &[R + R_NAME, S + PREFIX]);
    }
    call(c, STRCAT, &[R + R_NAME, name]);
}

/// `MCRD_loadfile` (MEMCARD.C, 0x8004fba0): a read of `info`'s file (+0 its name, the prefix
/// first unless `bare`) from card `card` asked for: the title, the data and the icons'
/// destinations noted, the frames' and blocks' bytes pointed to; the read pending.
pub fn loadfile(c: &mut Cpu, card: u32, info: u32, bare: bool) {
    call(c, BLOCKCLEAR, &[R, R_SIZE]);
    c.m.w32(R + R_CARD, card);
    let name = c.m.r32(info);
    if !bare {
        call(c, STRCPY, &[R + R_NAME, S + PREFIX]);
    }
    call(c, STRCAT, &[R + R_NAME, name]);
    let m = &mut c.m;
    for (from, to) in [(4, R_TITLE), (8, R_DATA_SIZE), (12, R_OFFSET), (24, R_ICONS), (28, R_ICONS + 4), (32, R_ICONS + 8), (20, R_DATA)] {
        let v = m.r32(info + from);
        m.w32(R + to, v);
    }
    m.w32(info + 36, R + HEADER_FRAMES);
    m.w32(info + 40, R + HEADER_BLOCKS);
    m.w32(S + PENDING, PENDING_READ);
    m.w32(S + DONE, 0);
}

pub fn abi_loadfile(c: &mut Cpu) {
    loadfile(c, c.r[4], c.r[5], c.r[6] != 0);
}

/// Waits for the library's operation (`MemCardSync` with `mode`), calling the state's wait
/// callback (with 3) and a vertical blank between tries when `wait`. The result.
fn sync(c: &mut Cpu, mode: u32, wait: bool) -> u32 {
    with_buffer(c, 8, |c, out| loop {
        if call_addr(c, MC_SYNC, &[mode, out, out + 4]) != 0 {
            return c.m.r32(out + 4);
        }
        if wait {
            let cb = c.m.r32(S + WAIT_CB1);
            call_addr(c, cb, &[3]);
            call_addr(c, VSYNC, &[0]);
        }
    })
}

/// `iMCRD_DoFileLoad` (MEMCARD.C, 0x8004fc8c): the pending read done on card `card`: the
/// header (title or icons wanted) read and checked, the title back in ASCII, each icon made a
/// shape (its CLUT and 16x16 picture, then its palette); the data read. 15 done, 16 a read
/// refused, or the error's code.
pub fn do_file_load(c: &mut Cpu, card: u32) -> u32 {
    let m = &mut c.m;
    if m.r32(R + R_TITLE) != 0 || m.r32(R + R_ICONS) != 0 {
        let port = card_port(&mut c.m);
        call_addr(c, MC_READ, &[port, R + R_NAME, R + HEADER, 0, HEADER_SIZE]);
        let result = sync(c, 1, true);
        let e = handle_error(c, OP_READ, result, card);
        if e != 0 {
            return e;
        }
        // the title back in ASCII
        if c.m.r32(R + R_TITLE) != 0 {
            let mut k = 0u32;
            loop {
                let code = c.m.r16(R + HEADER_TITLE + 2 * k) as i16 as i32 as u32;
                let v = sjis2ascii(&mut c.m, code);
                let title = c.m.r32(R + R_TITLE);
                c.m.w8(title.wrapping_add(k), v as u8);
                if v & 0xFF == 0 {
                    break;
                }
                k += 1;
            }
        }
        // each icon a 16-colour shape of 16 by 16 and its palette after it
        for k in 0..3u32 {
            let shape = c.m.r32(R + R_ICONS + 4 * k);
            if shape == 0 {
                break;
            }
            call(c, BLOCKMOVE, &[R + HEADER_ICONS + 128 * k, shape + 16, 128]);
            let v = call(c, ICON_CLUT, &[4]);
            let m = &mut c.m;
            m.w8(shape, v as u8);
            let head = (v & 0xFF) | 0x9000;
            let place = m.r32(shape + 12);
            m.w32(shape, head);
            m.w16(shape + 6, 16);
            m.w16(shape + 4, 16);
            m.w16(shape + 10, 0);
            m.w16(shape + 8, 0);
            m.w32(shape + 12, place & 0xF000_F000);
            let palette = shape.wrapping_add((head as i32 >> 8) as u32);
            call(c, BLOCKMOVE, &[R + HEADER_CLUT, palette + 16, 32]);
            let v = call(c, ICON_TPAGE, &[16]);
            let m = &mut c.m;
            m.w8(palette, v as u8);
            let place = m.r32(palette + 12);
            m.w16(palette + 4, 16);
            m.w16(palette + 6, 1);
            m.w16(palette + 10, 0);
            m.w16(palette + 8, 0);
            m.w32(palette, v & 0xFF);
            m.w32(palette + 12, place & 0xF000_F000);
        }
    }
    let size = c.m.r32(R + R_DATA_SIZE);
    if size != 0 {
        let m = &mut c.m;
        let (port, data, offset) = (card_port(m), m.r32(R + R_DATA), m.r32(R + R_OFFSET).wrapping_add(HEADER_SIZE));
        if call_addr(c, MC_READ, &[port, R + R_NAME, data, offset, size]) == 0 {
            c.m.w32(S + DONE, 1);
            return 16;
        }
        let result = sync(c, 0, false);
        let e = handle_error(c, OP_READ, result, card);
        if e != 0 {
            return e;
        }
    }
    c.m.w32(S + DONE, 1);
    15
}

pub fn abi_do_file_load(c: &mut Cpu) {
    c.r[2] = do_file_load(c, c.r[4]);
}

/// `MCRD_savefile` (MEMCARD.C, 0x8004ff34): a write of `info` (+0 the name, +4 the title, +8
/// the size: whole 128-byte frames, +12 the offset, +16 the flags: 0x200 a new file with its
/// header, +20 the data, +24.. up to three icons) to card `card` asked for: the header made
/// (`SC`, the icon's frames, the blocks, the title in Shift-JIS, the first icon's CLUT, the
/// icons). 0, or -1 (a size not of whole frames, no icons, no title or one past 32 letters).
pub fn savefile(c: &mut Cpu, card: u32, info: u32) -> u32 {
    if c.m.r32(info + 8) & 0x7F != 0 {
        return u32::MAX;
    }
    call(c, BLOCKCLEAR, &[R, R_SIZE]);
    c.m.w8(R + HEADER, b'S');
    c.m.w8(R + HEADER + 1, b'C');
    let name = c.m.r32(info);
    request_name(c, name, false);
    let m = &mut c.m;
    m.w32(R + R_CARD, card);
    for (from, to) in [(8, R_DATA_SIZE), (16, R_FLAGS), (12, R_OFFSET), (20, R_DATA)] {
        let v = m.r32(info + from);
        m.w32(R + to, v);
    }
    if m.r32(info + 16) & NEW_FILE != 0 {
        let icons = (0..3).filter(|k| m.r32(info + 24 + 4 * k) != 0).count() as u32;
        if icons == 0 {
            return u32::MAX;
        }
        m.w8(R + HEADER_FRAMES, (icons + 16) as u8);
        let size = m.r32(info + 8);
        m.w8(R + HEADER_BLOCKS, blocks(size.wrapping_add(8192)) as u8);
        let title = m.r32(info + 4);
        if title == 0 || call(c, STRLEN, &[title]) == 0 {
            return u32::MAX;
        }
        let title = c.m.r32(info + 4);
        if call(c, STRLEN, &[title]) >= 33 {
            return u32::MAX;
        }
        // the title in Shift-JIS (to its end, 32 letters at most)
        for k in 0..32u32 {
            let title = c.m.r32(info + 4);
            let ch = c.m.r8(title.wrapping_add(k)) as u32;
            let v = ascii2sjis(&mut c.m, ch);
            c.m.w16(R + HEADER_TITLE + 2 * k, v as u16);
            if v & 0xFFFF == 0 {
                break;
            }
        }
        let first = c.m.r32(info + 24);
        let clut = call(c, SHAPE_CLUT, &[first]);
        call(c, BLOCKMOVE, &[clut.wrapping_add(16), R + HEADER_CLUT, 32]);
        for k in 0..icons {
            let icon = c.m.r32(info + 24 + 4 * k);
            call(c, BLOCKMOVE, &[icon.wrapping_add(16), R + HEADER_ICONS + 128 * k, 128]);
        }
    }
    c.m.w32(S + PENDING, PENDING_WRITE);
    c.m.w32(S + DONE, 0);
    0
}

pub fn abi_savefile(c: &mut Cpu) {
    c.r[2] = savefile(c, c.r[4], c.r[5]);
}

/// `iMCRD_DoFileWrite` (MEMCARD.C, 0x80050170): the pending write done on card `card`: a new
/// file created and its header written first; then the data. 12 done, 13 a write refused, or
/// the error's code.
pub fn do_file_write(c: &mut Cpu, card: u32) -> u32 {
    let refused = |c: &mut Cpu| {
        c.m.w32(S + DONE, 1);
        13
    };
    if c.m.r32(R + R_FLAGS) & NEW_FILE != 0 {
        let (port, blocks) = (card_port(&mut c.m), c.m.r8(R + HEADER_BLOCKS) as u32);
        let v = call_addr(c, MC_CREATE, &[port, R + R_NAME, blocks]);
        let e = handle_error(c, OP_WRITE, v, card);
        if e != 0 {
            return e;
        }
        call(c, TIMED_WAIT, &[64]);
        let port = card_port(&mut c.m);
        if call_addr(c, MC_WRITE, &[port, R + R_NAME, R + HEADER, 0, HEADER_SIZE]) == 0 {
            return refused(c);
        }
        let result = sync(c, 0, false);
        let e = handle_error(c, OP_WRITE, result, card);
        if e != 0 {
            return e;
        }
    }
    let m = &mut c.m;
    let (port, data, offset, size) = (card_port(m), m.r32(R + R_DATA), m.r32(R + R_OFFSET).wrapping_add(HEADER_SIZE), m.r32(R + R_DATA_SIZE));
    if call_addr(c, MC_WRITE, &[port, R + R_NAME, data, offset, size]) == 0 {
        return refused(c);
    }
    let result = sync(c, 0, false);
    let e = handle_error(c, OP_WRITE, result, card);
    if e != 0 {
        return e;
    }
    c.m.w32(S + DONE, 0);
    c.m.w32(S + PENDING, PENDING_LOAD_CARD);
    12
}

pub fn abi_do_file_write(c: &mut Cpu) {
    c.r[2] = do_file_write(c, c.r[4]);
}

/// `iMCRD_DoFileDelete` (MEMCARD.C, 0x800502d4): the request's file deleted from card `card`:
/// 17 (the card to be reloaded), or 18 when there's no such file (the card's event 19).
pub fn do_file_delete(c: &mut Cpu, card: u32) -> u32 {
    let port = card_port(&mut c.m);
    match call_addr(c, MC_DELETE, &[port, R + R_NAME]) {
        0 => {
            c.m.w32(S + PENDING, PENDING_LOAD_CARD);
            17
        }
        5 => {
            let at = getcard(&mut c.m, card);
            c.m.w32(at + CARD_EVENT, 19);
            c.m.w32(S + DONE, 1);
            18
        }
        _ => 17,
    }
}

pub fn abi_do_file_delete(c: &mut Cpu) {
    c.r[2] = do_file_delete(c, c.r[4]);
}

/// `MCRD_handlecardevents` (MEMCARD.C, 0x80050368): card `card`'s events: the library's last
/// operation's end (a card checked: there, new, gone, unformatted; a card loaded: its state)
/// or, with none pending, the pending operation started for the request's card (a card's
/// existence checked once its timer is out, its directory loaded, a write, a read, a delete).
/// The event: 2..5 the card's state, 21 busy, 22 waiting, 23 nothing, or the operation's.
pub fn handlecardevents(c: &mut Cpu, card: u32) -> u32 {
    let info = getcard(&mut c.m, card);
    let (state, cmd, result) = with_buffer(c, 8, |c, out| {
        let state = call_addr(c, MC_SYNC, &[0, out, out + 4]);
        (state, c.m.r32(out), c.m.r32(out + 4))
    });
    let timer = S + TIMERS + 4 * card.wrapping_sub(1);
    match state as i32 {
        0 => 21,
        1 => match (cmd, result) {
            // a card checked: there, new, gone, unformatted
            (1, 1) => {
                let now = c.m.r32(VSYNC_COUNT);
                c.m.w32(S + DONE, 1);
                c.m.w32(timer, now);
                c.m.w32(info + CARD_STATE, NO_CARD);
                2
            }
            (1, 0) => {
                let now = c.m.r32(VSYNC_COUNT);
                c.m.w32(S + DONE, 1);
                c.m.w32(timer, now);
                if c.m.r32(info + CARD_STATE) == NO_CARD {
                    let port = card_port(&mut c.m);
                    call_addr(c, MC_ACCEPT, &[port]);
                }
                22
            }
            (1, 2) => {
                c.m.w32(S + DONE, 1);
                c.m.w32(info + CARD_STATE, BAD_CARD);
                3
            }
            (1, 3) => {
                let port = card_port(&mut c.m);
                call_addr(c, MC_ACCEPT, &[port]);
                21
            }
            (1, _) => 23,
            // a card accepted: to be loaded, gone, bad, unformatted
            (2, 0 | 3) => {
                c.m.w32(R + R_CARD, card);
                c.m.w32(S + PENDING, PENDING_LOAD_CARD);
                4
            }
            (2, 1) => {
                init_card(c, card);
                c.m.w32(info + CARD_STATE, NO_CARD);
                2
            }
            (2, 2) => {
                init_card(c, card);
                3
            }
            (2, 4) => {
                c.m.w32(info + CARD_STATE, UNFORMATTED);
                5
            }
            _ => 23,
        },
        -1 => {
            let pending = c.m.r32(S + PENDING);
            let start = |c: &mut Cpu, f: fn(&mut Cpu, u32) -> u32| -> u32 {
                if card != c.m.r32(R + R_CARD) {
                    return 23;
                }
                c.m.w32(info + CARD_EVENT, 0);
                c.m.w32(S + DONE, 0);
                c.m.w32(S + PENDING, 0);
                f(c, card)
            };
            match pending {
                0 => {
                    if (c.m.r32(timer) as i32) >= 0 {
                        22
                    } else {
                        let port = card_port(&mut c.m);
                        c.m.w32(S + DONE, 0);
                        if call_addr(c, MC_EXIST, &[port]) == 0 { 23 } else { 21 }
                    }
                }
                PENDING_LOAD_CARD => start(c, load_card),
                PENDING_WRITE => start(c, do_file_write),
                PENDING_READ => start(c, do_file_load),
                PENDING_DELETE => start(c, do_file_delete),
                _ => 23,
            }
        }
        _ => 23,
    }
}

pub fn abi_handlecardevents(c: &mut Cpu) {
    c.r[2] = handlecardevents(c, c.r[4]);
}

/// For callers not rewritten yet.
pub(crate) fn handlecardevents_entry(c: &mut Cpu) {
    abi_handlecardevents(c);
}

/// `MCRD_fileexists` (MEMCARD.C, 0x800506b4): the directory entry of card `card` named the
/// prefix and `name`, or -1.
pub fn fileexists(c: &mut Cpu, card: u32, name: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    with_buffer(c, 40, |c, full| {
        call(c, STRCPY, &[full, S + PREFIX]);
        call(c, STRCAT, &[full, name]);
        (0..DIRECTORY_ENTRIES).find(|&k| call(c, STRCMP, &[at + DIRECTORY + ENTRY_SIZE * k, full]) == 0).unwrap_or(u32::MAX)
    })
}

pub fn abi_fileexists(c: &mut Cpu) {
    c.r[2] = fileexists(c, c.r[4], c.r[5]);
}

/// `MCRD_getcard` (MEMCARD.C, 0x8005072c): card `card`'s record (1..8), its port noted.
pub fn getcard(m: &mut Mem, card: u32) -> u32 {
    let k = card.wrapping_sub(1);
    m.w32(S + PORT, ((k & 4) << 2) | (k & 3));
    k.wrapping_mul(CARD_SIZE).wrapping_add(CARDS)
}

pub fn abi_getcard(c: &mut Cpu) {
    c.r[2] = getcard(&mut c.m, c.r[4]);
}

/// For callers not rewritten yet.
pub(crate) fn getcard_entry(c: &mut Cpu) {
    abi_getcard(c);
}

/// `iMCRD_timersub` (MEMCARD.C, 0x80050774): every card's timer counted down (the timer's
/// handler).
pub fn timersub(m: &mut Mem) {
    for k in 0..CARD_COUNT {
        let v = m.r32(S + TIMERS + 4 * k).wrapping_sub(1);
        m.w32(S + TIMERS + 4 * k, v);
    }
}

pub fn abi_timersub(c: &mut Cpu) {
    timersub(&mut c.m);
}

/// Card `at`'s directory read from the library.
fn read_directory(c: &mut Cpu, at: u32) -> u32 {
    let port = card_port(&mut c.m);
    call_addr(c, MC_DIRENTRY, &[port, ALL_FILES, at + DIRECTORY, at + 8, 0, DIRECTORY_ENTRIES])
}

/// `garyMemCardGrabBlocks` (MEMCARD.C, 0x800507a4): card `card`'s directory read, entry `k`'s
/// size in blocks.
pub fn grab_blocks(c: &mut Cpu, card: u32, k: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    read_directory(c, at);
    let entry = if (k as i32) > 0 { at + DIRECTORY + ENTRY_SIZE * k } else { at + DIRECTORY };
    blocks(c.m.r32(entry + ENTRY_BYTES))
}

pub fn abi_grab_blocks(c: &mut Cpu) {
    c.r[2] = grab_blocks(c, c.r[4], c.r[5]);
}

/// `iMCRD_LoadCard` (MEMCARD.C, 0x8005082c): card `card` set up and its directory read; its
/// free blocks counted (none: -3, the card full). 9, 11 no card, or the error's code.
pub fn load_card(c: &mut Cpu, card: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    if init_card(c, card) != 0 {
        return 11;
    }
    let v = read_directory(c, at);
    let e = handle_error(c, OP_DIRECTORY, v, card);
    if e != 0 {
        return e;
    }
    let m = &mut c.m;
    for k in 0..DIRECTORY_ENTRIES {
        let entry = at + DIRECTORY + ENTRY_SIZE * k;
        if m.r8(entry) != 0 {
            let used = blocks(m.r32(entry + ENTRY_BYTES));
            let free = m.r32(at + CARD_FREE).wrapping_sub(used);
            m.w32(at + CARD_FREE, free);
        }
    }
    let state = if m.r32(at + CARD_FREE) == 0 { FULL } else { 0 };
    m.w32(at + CARD_STATE, state);
    m.w32(S + DONE, 1);
    9
}

pub fn abi_load_card(c: &mut Cpu) {
    c.r[2] = load_card(c, c.r[4]);
}

/// `iMCRD_FormatCard` (MEMCARD.C, 0x80050928): card `card` formatted: 15 blocks free, its
/// state cleared. 0, or -1 (the library's 1 or 2: no card, a bad one).
pub fn format_card(c: &mut Cpu, card: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    let port = card_port(&mut c.m);
    let result = match call_addr(c, MC_FORMAT, &[port]) as i32 {
        1 => {
            c.m.w32(at + CARD_STATE, NO_CARD);
            u32::MAX
        }
        2 => {
            c.m.w32(at + CARD_STATE, BAD_CARD);
            u32::MAX
        }
        _ => 0,
    };
    c.m.w32(at + CARD_STATE, 0);
    c.m.w32(at + CARD_FREE, 15);
    result
}

pub fn abi_format_card(c: &mut Cpu) {
    c.r[2] = format_card(c, c.r[4]);
}

/// `iMCRD_HandleError` (MEMCARD.C, 0x800509b4): the library's error `err` of operation `op`
/// (1 the directory, 2 a write, 3 a read) on card `card`: the card's state and event set (no
/// card, a new one accepted, unformatted: formatted when the format callback agrees, a full
/// card, an existing file: deleted when the overwrite callback agrees). The operation's code
/// (10, 13, 16) when it failed, 0 when fine; 6 formatted, 21 an old file deleted.
pub fn handle_error(c: &mut Cpu, op: u32, err: u32, card: u32) -> u32 {
    let at = getcard(&mut c.m, card);
    let code = if err == 0 {
        0
    } else {
        match op {
            OP_DIRECTORY => 10,
            OP_WRITE => 13,
            OP_READ => 16,
            _ => 0,
        }
    };
    let event = |c: &mut Cpu, e: u32| {
        c.m.w32(at + CARD_EVENT, e);
        c.m.w32(S + DONE, 1);
        code
    };
    match err {
        0 => code,
        1 => {
            c.m.w32(at + CARD_STATE, NO_CARD);
            event(c, 2)
        }
        2 => {
            c.m.w32(at + CARD_STATE, BAD_CARD);
            event(c, 3)
        }
        3 => {
            let port = card_port(&mut c.m);
            if call_addr(c, MC_ACCEPT, &[port]) == 0 {
                event(c, 23)
            } else {
                c.m.w32(S + PENDING, PENDING_LOAD_CARD);
                c.m.w32(S + DONE, 0);
                code
            }
        }
        4 => {
            // unformatted: formatted for a write when the callback agrees (three tries)
            let cb = c.m.r32(S + FORMAT_CB);
            if op != OP_WRITE || call_addr(c, cb, &[]) == 0 {
                c.m.w32(at + CARD_STATE, UNFORMATTED);
                return event(c, 5);
            }
            let formatted = (0..3).any(|_| format_card(c, card) != u32::MAX);
            if formatted {
                c.m.w32(S + PENDING, PENDING_WRITE);
                6
            } else {
                c.m.w32(at + CARD_STATE, UNFORMATTED);
                event(c, 7)
            }
        }
        5 => event(c, 19),
        6 => {
            // the file there: deleted (and the write again) when the callback agrees
            let cb = c.m.r32(S + OVERWRITE_CB);
            if call_addr(c, cb, &[]) == 0 {
                event(c, 14)
            } else {
                let port = card_port(&mut c.m);
                call_addr(c, MC_DELETE, &[port, R + R_NAME]);
                c.m.w32(S + PENDING, PENDING_WRITE);
                21
            }
        }
        7 => event(c, 20),
        _ => event(c, 23),
    }
}

pub fn abi_handle_error(c: &mut Cpu) {
    c.r[2] = handle_error(c, c.r[4], c.r[5], c.r[6]);
}

/// `iMCRD_DefaultCBProc1` (MEMCARD.C, 0x80050bd0): yes.
pub fn abi_default_cb(c: &mut Cpu) {
    c.r[2] = 1;
}

/// A Shift-JIS code's bytes swapped into the order the file header keeps, as a `short`.
fn swapped(v: u32) -> u32 {
    ((v << 8) | ((v & 0xFFFF) >> 8)) as u16 as i16 as i32 as u32
}

/// `ascii2sjis` (MEMCARD.C, 0x80050bd8): ASCII `ch` in Shift-JIS (full width: punctuation from
/// a table, digits and letters from their ranges), its bytes swapped; 0 for others.
pub fn ascii2sjis(m: &mut Mem, ch: u32) -> u32 {
    let b = |v: u32| v & 0xFF;
    // (a range of the ranges' table, or a place in the punctuation's)
    let (range, punctuation) = if b(ch.wrapping_sub(32)) < 16 {
        (0, 1)
    } else if b(ch.wrapping_sub(48)) < 10 {
        (0, 0)
    } else if b(ch.wrapping_sub(58)) < 7 {
        (0, 11)
    } else if b(ch.wrapping_sub(65)) < 26 {
        (1, 0)
    } else if b(ch.wrapping_sub(91)) < 6 {
        (0, 37)
    } else if b(ch.wrapping_sub(97)) < 26 {
        (2, 0)
    } else if b(ch.wrapping_sub(123)) < 4 {
        (0, 63)
    } else {
        return 0;
    };
    let code = if punctuation != 0 {
        m.r16((b(ch).wrapping_sub(punctuation + 31) << 1).wrapping_add(SJIS_PUNCTUATION)) as u32
    } else {
        let at = SJIS_RANGES + (range << 2);
        (m.r16(at) as u32).wrapping_add(b(ch)).wrapping_sub(m.r16(at + 2) as u32)
    };
    swapped(code)
}

pub fn abi_ascii2sjis(c: &mut Cpu) {
    c.r[2] = ascii2sjis(&mut c.m, c.r[4]);
}

/// `sjis2ascii` (MEMCARD.C, 0x80050cf8): a header's Shift-JIS code (bytes swapped) in ASCII:
/// punctuation (0x81) from a table, digits and letters (0x82) from their ranges; 0 for others.
pub fn sjis2ascii(m: &mut Mem, code: u32) -> u32 {
    let low = ((code << 16) as i32 >> 24) as u32;
    match code & 0xFF {
        0x81 => m.r8((low & 0xFF).wrapping_add(ASCII_PUNCTUATION).wrapping_sub(64)) as u32,
        0x82 => {
            let range = if low.wrapping_sub(79) < 10 {
                0
            } else if low.wrapping_sub(96) < 26 {
                1
            } else if low.wrapping_add(127) & 0xFF < 26 {
                2
            } else {
                0
            };
            let at = ASCII_RANGES + 2 * range;
            let (from, to) = (m.r8(at) as u32, m.r8(at + 1) as u32);
            to.wrapping_add(low.wrapping_sub(from)) & 0xFF
        }
        _ => 0,
    }
}

pub fn abi_sjis2ascii(c: &mut Cpu) {
    c.r[2] = sjis2ascii(&mut c.m, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{CARD_ANSWERS, DIRECTORY as ANSWER_DIRECTORY, DIRECTORY_MARK, Rng, against_original_in, fe_ram};

    /// A request's info, options, a name, buffers for the title and three icons, three icon
    /// shapes (a picture and a palette after it).
    const INFO: u32 = 0x8014_4000;
    const OPTS: u32 = 0x8014_4040;
    const NAME: u32 = 0x8014_4080;
    const TITLE: u32 = 0x8014_40A0;
    const PREFIX: u32 = 0x8014_40E0;
    const BUFFERS: u32 = 0x8014_4100;
    const ICONS: u32 = 0x8014_4800;
    /// Callbacks answering yes (the default) and no (`LoadConfig`'s 0); the wait callback the
    /// game's (`LoadingProc`, a leaf).
    const NO: u32 = 0x8002_7960;
    const WAIT: u32 = 0x8003_477C;

    fn put(m: &mut ps1rt::Mem, at: u32, s: &[u8]) {
        for (k, &b) in s.iter().enumerate() {
            m.w8(at + k as u32, b);
        }
        m.w8(at + s.len() as u32, 0);
    }

    /// The RAM image's card state (card 1 holding the game's save) with: the libmcrd answers
    /// (`MemCardSync` never 0, which loops; the event cases pick their own), the pending
    /// operation, the done flag, the cards' timers about 0, the callbacks yes or no, the request
    /// (its card, flags, title and icon buffers or none, sizes), each card's state, free blocks
    /// and directory (the save's name, others, none, sizes about the block boundaries).
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(CARD_ANSWERS, [1, 1, u32::MAX, 2][pick(rng, 4) as usize]);
        m.w32(CARD_ANSWERS + 4, [1, 2, 0, 3][pick(rng, 4) as usize]);
        m.w32(CARD_ANSWERS + 8, pick(rng, 6));
        for k in 3..11 {
            m.w32(CARD_ANSWERS + 4 * k, [0, 1, 2, 3, 4, 5, 6, 7, pick(rng, 10)][pick(rng, 9) as usize]);
        }
        m.w32(S + 48, pick(rng, 6));
        m.w32(S + 52, pick(rng, 2));
        for k in 0..8 {
            m.w32(S + 64 + 4 * k, [0, 1, u32::MAX, (-78i32) as u32, pick(rng, 200)][pick(rng, 5) as usize]);
        }
        m.w32(S + 32, [DEFAULT_CB, NO][pick(rng, 2) as usize]);
        m.w32(S + 36, [DEFAULT_CB, NO][pick(rng, 2) as usize]);
        m.w32(S + 40, WAIT);
        m.w32(R, 1 + pick(rng, 8));
        m.w32(R + 28, [0, TITLE][pick(rng, 2) as usize]);
        m.w32(R + 32, [0, 128, 0x2000, pick(rng, 0x4000)][pick(rng, 4) as usize]);
        m.w32(R + 36, pick(rng, 0x1000));
        // (never a new file: its creation waits with `timedwait`, which the test CPUs don't
        // run; the tours' saves create it)
        m.w32(R + 40, [0, 0x1FF, rng.next() as u32 & !0x200][pick(rng, 3) as usize]);
        m.w32(R + 556, BUFFERS);
        let icons = pick(rng, 4);
        for k in 0..3 {
            m.w32(R + 560 + 4 * k, if k < icons { BUFFERS + 0x100 + 0x180 * k } else { 0 });
        }
        for k in 0..32 {
            let v = [0x4F82u16, 0x6082, 0x8182, 0x4081, 0x4381, 0x1234, 0][pick(rng, 7) as usize];
            m.w16(R + 48 + 2 * k, if k == 31 { 0 } else { v });
        }
        for card in 0..8 {
            let at = CARDS + CARD_SIZE * card;
            m.w32(at, [0, u32::MAX, (-3i32) as u32, (-4i32) as u32][pick(rng, 4) as usize]);
            m.w32(at + 12, [0, 1, 14, 15, pick(rng, 16)][pick(rng, 5) as usize]);
            for e in 0..15 {
                let entry = at + 16 + 40 * e;
                let name: &[u8] = [&b"BASLUS-00826NFS4"[..], b"BASLUS-00826NFS4P", b"BESLES-00001X", b""][pick(rng, 4) as usize];
                put(m, entry, name);
                m.w32(entry + 24, [0, 8192, 8191, 16384, 0x6000, pick(rng, 0x20000)][pick(rng, 6) as usize]);
            }
        }
        // the directory the card answers with (now and then none): names or none, sizes
        m.w32(ANSWER_DIRECTORY, if pick(rng, 4) == 0 { 0 } else { DIRECTORY_MARK });
        for e in 0..15 {
            let entry = ANSWER_DIRECTORY + 4 + 40 * e;
            m.w8(entry, [0, b'B', b'X'][pick(rng, 3) as usize]);
            m.w32(entry + 24, [0, 8192, 8191, 16384, 0x6000, pick(rng, 0x20000)][pick(rng, 6) as usize]);
        }
        put(m, NAME, [&b"NFS4"[..], b"NFS4P", b"X"][pick(rng, 3) as usize]);
        let title: &[u8] = [&b"Need for Speed"[..], b"", b"a{|}~ !09AZaz[`", b"0123456789012345678901234567890123"][pick(rng, 4) as usize];
        put(m, TITLE, title);
        put(m, PREFIX, b"BASLUS-00826");
        m.w32(INFO, NAME);
        m.w32(INFO + 4, [TITLE, 0, TITLE][pick(rng, 3) as usize]);
        m.w32(INFO + 8, [128, 0x2000, 0x2080, 0x81, 0, pick(rng, 0x4000)][pick(rng, 6) as usize]);
        m.w32(INFO + 12, pick(rng, 0x1000));
        m.w32(INFO + 16, [0, 0x200, rng.next() as u32][pick(rng, 3) as usize]);
        m.w32(INFO + 20, BUFFERS);
        for k in 0..3 {
            let shape = ICONS + 0x200 * k;
            for j in 4..0x200 {
                m.w8(shape + j, rng.next() as u8);
            }
            m.w32(shape, 0x0000_9000 | 0x09);
            m.w32(shape + 0x90, 0x10);
            m.w32(INFO + 24 + 4 * k, if pick(rng, 4) == 0 { 0 } else { shape });
        }
        m.w32(OPTS, pick(rng, 4));
        m.w32(OPTS + 4, [0, PREFIX][pick(rng, 2) as usize]);
        for k in 2..9 {
            m.w32(OPTS + 4 * k, rng.next() as u32);
        }
    }

    fn card(r: &mut Rng) -> u32 {
        1 + pick(r, 8)
    }

    fn rng_high(r: &mut Rng) -> u32 {
        [0, 0xFFFF, r.next() as u32 & 0xFFFF][pick(r, 3) as usize]
    }

    fn cases() -> Vec<Case> {
        vec![
            ("MCRD_getcard", 0x8005_072c, nfs4rc::f_8005072c, |_, r| vec![card(r)]),
            ("iMCRD_timersub", 0x8005_0774, nfs4rc::f_80050774, |_, _| vec![]),
            ("iMCRD_InitCard", 0x8004_f968, nfs4rc::f_8004f968, |_, r| vec![card(r)]),
            ("MCRD_restore", 0x8004_f9c0, nfs4rc::f_8004f9c0, |_, _| vec![]),
            ("MCRD_getopts", 0x8004_fa04, nfs4rc::f_8004fa04, |_, _| vec![OPTS]),
            ("MCRD_setopts", 0x8004_fa7c, nfs4rc::f_8004fa7c, |_, _| vec![OPTS]),
            ("MCRD_loadfile", 0x8004_fba0, nfs4rc::f_8004fba0, |_, r| vec![card(r), INFO, pick(r, 2)]),
            ("iMCRD_DoFileLoad", 0x8004_fc8c, nfs4rc::f_8004fc8c, |_, r| vec![card(r)]),
            ("MCRD_savefile", 0x8004_ff34, nfs4rc::f_8004ff34, |_, r| vec![card(r), INFO]),
            ("iMCRD_DoFileWrite", 0x8005_0170, nfs4rc::f_80050170, |_, r| vec![card(r)]),
            ("iMCRD_DoFileDelete", 0x8005_02d4, nfs4rc::f_800502d4, |_, r| vec![card(r)]),
            ("MCRD_handlecardevents", 0x8005_0368, nfs4rc::f_80050368, |c, r| {
                c.m.w32(CARD_ANSWERS, [0, 1, u32::MAX, 2, (-2i32) as u32][pick(r, 5) as usize]);
                let k = card(r);
                if pick(r, 2) == 0 {
                    c.m.w32(R, k);
                }
                vec![k]
            }),
            ("MCRD_fileexists", 0x8005_06b4, nfs4rc::f_800506b4, |_, r| vec![card(r), NAME]),
            ("garyMemCardGrabBlocks", 0x8005_07a4, nfs4rc::f_800507a4, |_, r| vec![card(r), [0, 1, 14, pick(r, 15)][pick(r, 4) as usize]]),
            ("iMCRD_LoadCard", 0x8005_082c, nfs4rc::f_8005082c, |_, r| vec![card(r)]),
            ("iMCRD_FormatCard", 0x8005_0928, nfs4rc::f_80050928, |_, r| vec![card(r)]),
            ("iMCRD_HandleError", 0x8005_09b4, nfs4rc::f_800509b4, |_, r| vec![pick(r, 5), [0, 1, 2, 3, 4, 5, 6, 7, 8, pick(r, 20)][pick(r, 10) as usize], card(r)]),
            ("iMCRD_DefaultCBProc1", 0x8005_0bd0, nfs4rc::f_80050bd0, |_, _| vec![]),
            ("ascii2sjis", 0x8005_0bd8, nfs4rc::f_80050bd8, |_, r| vec![[32, 47, 48, 57, 58, 64, 65, 90, 91, 96, 97, 122, 123, 126, 127, pick(r, 256), r.next() as u32][pick(r, 17) as usize]]),
            ("sjis2ascii", 0x8005_0cf8, nfs4rc::f_80050cf8, |_, r| vec![[0x81, 0x82, pick(r, 256)][pick(r, 3) as usize] | ([0x40, 0x4F, 0x58, 0x60, 0x79, 0x81, 0x9A, pick(r, 256)][pick(r, 8) as usize] << 8) | (rng_high(r) << 16)]),
        ]
    }

    #[test]
    fn memcard_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x960 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x961 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
