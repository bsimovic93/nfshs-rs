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

use crate::abi::{self, Saved};
use ps1rt::Cpu;

const S: u32 = 0x8005_31F8;
pub(crate) const STATE: u32 = S;
const R: u32 = S + 608;
const CARDS: u32 = S + 1180;
const CARD_SIZE: u32 = 616;
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

/// A port called from a port, as feaudio's and front's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// A block size in 8 KB blocks, rounded toward zero (`+8191` first when negative).
fn blocks(v: u32) -> u32 {
    ((v as i32) / 8192) as u32
}

/// `MCRD_init` (MEMCARD.C, 0x8004f8c8): the state cleared, the callbacks the defaults, the
/// library started, the eight cards set up (a moment waited before each), the timer added.
pub fn abi_init(c: &mut Cpu) {
    let arg = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (arg, S);
        r.call_leaving(c, BLOCKCLEAR, &[S, 6108], 0x8004_f8f0);
        let m = &mut c.m;
        m.w32(S + 36, DEFAULT_CB);
        m.w32(S + 32, DEFAULT_CB);
        m.w32(S + 48, 0);
        m.w32(S + 56, arg);
        m.w32(S + 44, WAIT_CB);
        m.w32(S + 40, WAIT_CB);
        r.call_ptr_leaving(c, MC_INIT, &[1], 0x8004_f920);
        for card in 1..9 {
            r.0[0] = card;
            r.call_leaving(c, TIMED_WAIT, &[10], 0x8004_f92c);
            port(c, &r, 0x8004_f968, 0x8004_f934, &[card], abi_init_card);
        }
        r.0[0] = 9;
        r.call_leaving(c, ADD_TIMER, &[TIMER_SUB], 0x8004_f94c);
        r.call_ptr_leaving(c, MC_START, &[], 0x8004_f954);
    })
}

/// `iMCRD_InitCard` (MEMCARD.C, 0x8004f968): card `card` unknown (-1), its event none, 15
/// blocks free, its directory cleared. 0, or -1 for no card.
pub fn abi_init_card(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        let at = port(c, &r, 0x8005_072c, 0x8004_f978, &[card], abi_getcard);
        if at == 0 {
            c.r[2] = u32::MAX;
            return;
        }
        let m = &mut c.m;
        m.w32(at, u32::MAX);
        m.w32(at + 4, 0);
        m.w32(at + 8, 0);
        m.w32(at + 12, 15);
        r.call_leaving(c, BLOCKCLEAR, &[at + 16, 600], 0x8004_f9ac);
        c.r[2] = 0;
    })
}

/// `MCRD_restore` (MEMCARD.C, 0x8004f9c0): the timer removed, the library synced, stopped and
/// ended.
pub fn abi_restore(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.call_leaving(c, DEL_TIMER, &[TIMER_SUB], 0x8004_f9d4);
        r.call_ptr_leaving(c, MC_SYNC, &[0, 0, 0], 0x8004_f9e4);
        r.call_ptr_leaving(c, MC_STOP, &[], 0x8004_f9ec);
        r.call_ptr_leaving(c, MC_END, &[], 0x8004_f9f4);
    })
}

/// Eight bytes copied (the original's unaligned word pair).
fn copy8(m: &mut ps1rt::Mem, from: u32, to: u32) {
    let v: Vec<u8> = (0..8).map(|k| m.r8(from + k)).collect();
    for (k, b) in v.into_iter().enumerate() {
        m.w8(to + k as u32, b);
    }
}

/// `MCRD_getopts` (MEMCARD.C, 0x8004fa04): the options into `o` (the region, no prefix, the
/// callbacks, +8, the 8 bytes at +12).
pub fn abi_getopts(c: &mut Cpu) {
    let o = c.r[4];
    let m = &mut c.m;
    let region = m.r32(S);
    m.w32(o + 4, 0);
    m.w32(o, region);
    for (to, from) in [(20, 32), (24, 36), (28, 40), (32, 44), (8, 20)] {
        let v = m.r32(S + from);
        m.w32(o + to, v);
    }
    copy8(m, S + 24, o + 12);
}

/// `MCRD_setopts` (MEMCARD.C, 0x8004fa7c): the options from `o`: with a prefix, `B` and the
/// region's letter (`I` 1, `E` 2, else `A`) and the prefix (10 letters), and the region; else
/// none; the callbacks, +20, the 8 bytes at +24.
pub fn abi_setopts(c: &mut Cpu) {
    let o = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = o;
        if c.m.r32(o + 4) != 0 {
            let m = &mut c.m;
            m.w8(S + 4, 66);
            let letter = match m.r32(o) {
                1 => 73,
                2 => 69,
                _ => 65,
            };
            m.w8(S + 5, letter);
            r.0[0] = S + 6;
            let prefix = m.r32(o + 4);
            r.call_leaving(c, STRNCPY, &[S + 6, prefix, 10], 0x8004_fb0c);
            let m = &mut c.m;
            m.w8(S + 16, 0);
            let region = m.r32(o);
            m.w32(S, region);
        } else {
            c.m.w8(S + 4, 0);
            c.m.w32(S, 0);
        }
        let m = &mut c.m;
        for (from, to) in [(20, 32), (24, 36), (28, 40), (32, 44), (8, 20)] {
            let v = m.r32(o + from);
            m.w32(S + to, v);
        }
        copy8(m, o + 12, S + 24);
    })
}

/// `MCRD_loadfile` (MEMCARD.C, 0x8004fba0): a read of `info`'s file (+0 its name, the prefix
/// first unless `bare`) from card `card` asked for: the title, the data and the icons'
/// destinations noted, the frames' and blocks' bytes pointed to; the read pending.
pub fn abi_loadfile(c: &mut Cpu) {
    let (card, info, bare) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[4], r.0[1], r.0[0], r.0[3]) = (card, info, bare, 0x8005_0000, R);
        r.call_leaving(c, BLOCKCLEAR, &[R, 572], 0x8004_fbdc);
        c.m.w32(R, card);
        if bare == 0 {
            r.call_leaving(c, STRCPY, &[R + 4, S + 4], 0x8004_fbf0);
        }
        let name = c.m.r32(info);
        r.call_leaving(c, STRCAT, &[R + 4, name], 0x8004_fbfc);
        let m = &mut c.m;
        for (from, to) in [(4, 28), (8, 32), (12, 36), (24, 560), (28, 564), (32, 568), (20, 556)] {
            let v = m.r32(info + from);
            m.w32(R + to, v);
        }
        m.w32(info + 36, R + 46);
        m.w32(info + 40, R + 47);
        m.w32(S + 48, 3);
        m.w32(S + 52, 0);
    })
}

/// Waits for the library's operation (`MemCardSync` with `mode`), calling `wait` (the state's
/// wait callback, with a0 3) and a vertical blank between tries when given. The result.
fn sync(c: &mut Cpu, r: &Saved, sp: u32, mode: u32, ra: u32, wait: Option<(u32, u32)>) -> u32 {
    loop {
        if r.call_ptr_leaving(c, MC_SYNC, &[mode, sp + 24, sp + 28], ra) != 0 {
            return c.m.r32(sp + 28);
        }
        if let Some((wait_ra, vsync_ra)) = wait {
            let cb = c.m.r32(S + 40);
            r.call_ptr_leaving(c, cb, &[3], wait_ra);
            r.call_ptr_leaving(c, VSYNC, &[0], vsync_ra);
        }
    }
}

/// `iMCRD_DoFileLoad` (MEMCARD.C, 0x8004fc8c): the pending read done on card `card`: the
/// header (title or icons wanted) read and checked, the title back in ASCII, each icon made a
/// shape (its CLUT and 16x16 picture, then its palette); the data read. 15 done, 16 a read
/// refused, or the error's code.
pub fn abi_do_file_load(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[7]) = (R, card);
        let m = &mut c.m;
        if m.r32(R + 28) != 0 || m.r32(R + 560) != 0 {
            r.0[0] = S;
            c.m.w32(sp + 16, 512);
            let port_id = c.m.r32(S + 60);
            let v = r.call_ptr_leaving(c, MC_READ, &[port_id, R + 4, R + 44, 0, 512], 0x8004_fd00);
            c.m.w32(sp + 28, v);
            let result = sync(c, &r, sp, 1, 0x8004_fd14, Some((0x8004_fd2c, 0x8004_fd34)));
            let e = port(c, &r, 0x8005_09b4, 0x8004_fd48, &[3, result, card], abi_handle_error);
            if e != 0 {
                c.r[2] = e;
                return;
            }
            if c.m.r32(R + 28) != 0 {
                let mut k = 0u32;
                loop {
                    r.0[0] = R + 2 * k;
                    r.0[1] = k;
                    let code = c.m.r16(R + 48 + 2 * k) as i16 as i32 as u32;
                    let v = port(c, &r, 0x8005_0cf8, 0x8004_fd70, &[code], abi_sjis2ascii);
                    let title = c.m.r32(R + 28);
                    c.m.w8(title.wrapping_add(k), v as u8);
                    if v & 0xFF == 0 {
                        break;
                    }
                    k += 1;
                }
            }
            (r.0[1], r.0[6], r.0[4], r.0[3], r.0[5]) = (0, 1, 16, 0xF000_FFFF, 0xFFFF_F000);
            for k in 0..3u32 {
                let mut shape = c.m.r32(R + 560 + 4 * k);
                r.0[0] = shape;
                if shape == 0 {
                    break;
                }
                let icon = [R + 172, R + 300, R + 428][k as usize];
                r.call_leaving(c, BLOCKMOVE, &[icon, shape + 16, 128], 0x8004_fdec);
                let v = r.call_leaving(c, ICON_CLUT, &[4], 0x8004_fdf4);
                let m = &mut c.m;
                m.w8(shape, v as u8);
                let head = (v & 0xFF) | 0x9000;
                let place = m.r32(shape + 12);
                m.w32(shape, head);
                m.w16(shape + 6, 16);
                m.w16(shape + 4, 16);
                m.w16(shape + 10, 0);
                m.w16(shape + 8, 0);
                m.w32(shape + 12, place & 0xF000_FFFF & 0xFFFF_F000);
                shape = shape.wrapping_add((head as i32 >> 8) as u32);
                r.0[0] = shape;
                r.call_leaving(c, BLOCKMOVE, &[R + 140, shape + 16, 32], 0x8004_fe3c);
                let v = r.call_leaving(c, ICON_TPAGE, &[16], 0x8004_fe44);
                let m = &mut c.m;
                m.w8(shape, v as u8);
                let place = m.r32(shape + 12);
                r.0[1] = k + 1;
                m.w16(shape + 4, 16);
                m.w16(shape + 6, 1);
                m.w16(shape + 10, 0);
                m.w16(shape + 8, 0);
                m.w32(shape, v & 0xFF);
                m.w32(shape + 12, place & 0xF000_FFFF & 0xFFFF_F000);
            }
        }
        let size = c.m.r32(R + 32);
        if size != 0 {
            r.0[0] = S;
            let m = &mut c.m;
            let (port_id, data, offset) = (m.r32(S + 60), m.r32(R + 556), m.r32(R + 36).wrapping_add(512));
            m.w32(sp + 16, size);
            let v = r.call_ptr_leaving(c, MC_READ, &[port_id, R + 4, data, offset, size], 0x8004_feb0);
            c.m.w32(sp + 28, v);
            if v == 0 {
                c.m.w32(S + 52, 1);
                c.r[2] = 16;
                return;
            }
            let result = sync(c, &r, sp, 0, 0x8004_fed8, None);
            let e = port(c, &r, 0x8005_09b4, 0x8004_fef0, &[3, result, card], abi_handle_error);
            if e != 0 {
                c.r[2] = e;
                return;
            }
        }
        c.m.w32(S + 52, 1);
        c.r[2] = 15;
    })
}

/// `MCRD_savefile` (MEMCARD.C, 0x8004ff34): a write of `info` (+0 the name, +4 the title, +8
/// the size: whole 128-byte frames, +12 the offset, +16 the flags: 0x200 a new file with its
/// header, +20 the data, +24.. up to three icons) to card `card` asked for: the header made
/// (`SC`, the icon's frames, the blocks, the title in Shift-JIS, the first icon's CLUT, the
/// icons). 0, or -1 (a size not of whole frames, no icons, no title or one past 32 letters).
pub fn abi_savefile(c: &mut Cpu) {
    let (card, info) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3], r.0[1]) = (card, info, 0x8005_0000);
        r.0[4] = R;
        c.r[2] = u32::MAX;
        if c.m.r32(info + 8) & 0x7F != 0 {
            return;
        }
        r.call_leaving(c, BLOCKCLEAR, &[R, 572], 0x8004_ff7c);
        r.0[0] = R + 4;
        c.m.w8(R + 44, 83);
        c.m.w8(R + 45, 67);
        r.call_leaving(c, STRCPY, &[R + 4, S + 4], 0x8004_ff9c);
        let name = c.m.r32(info);
        r.call_leaving(c, STRCAT, &[R + 4, name], 0x8004_ffa8);
        let m = &mut c.m;
        m.w32(R, card);
        for (from, to) in [(8, 32), (16, 40), (12, 36), (20, 556)] {
            let v = m.r32(info + from);
            m.w32(R + to, v);
        }
        r.0[0] = 0;
        if m.r32(info + 16) & 0x200 != 0 {
            let icons = (0..3).filter(|k| m.r32(info + 24 + 4 * k) != 0).count() as u32;
            (r.0[2], r.0[0]) = (icons, 3);
            c.r[2] = u32::MAX;
            if icons == 0 {
                return;
            }
            m.w8(R + 46, (icons + 16) as u8);
            let size = m.r32(info + 8);
            m.w8(R + 47, blocks(size.wrapping_add(8192)) as u8);
            let title = m.r32(info + 4);
            if title == 0 {
                return;
            }
            if r.call_leaving(c, STRLEN, &[title], 0x8005_0064) == 0 {
                c.r[2] = u32::MAX;
                return;
            }
            let title = c.m.r32(info + 4);
            r.0[0] = 0;
            if r.call_leaving(c, STRLEN, &[title], 0x8005_0078) >= 33 {
                c.r[2] = u32::MAX;
                return;
            }
            r.0[1] = R;
            let mut k = 0u32;
            loop {
                let title = c.m.r32(info + 4);
                let ch = c.m.r8(title.wrapping_add(k)) as u32;
                let v = port(c, &r, 0x8005_0bd8, 0x8005_00a8, &[ch], abi_ascii2sjis);
                c.m.w16(R + 48 + 2 * k, v as u16);
                if v & 0xFFFF == 0 {
                    break;
                }
                k += 1;
                r.0[0] = k;
                r.0[1] = R + 2 * k;
                if k >= 32 {
                    break;
                }
            }
            let first = c.m.r32(info + 24);
            let clut = r.call_leaving(c, SHAPE_CLUT, &[first], 0x8005_00d4);
            r.call_leaving(c, BLOCKMOVE, &[clut.wrapping_add(16), R + 140, 32], 0x8005_00e4);
            let first = c.m.r32(info + 24);
            r.call_leaving(c, BLOCKMOVE, &[first.wrapping_add(16), R + 172, 128], 0x8005_00f8);
            if icons >= 2 {
                let second = c.m.r32(info + 28);
                r.call_leaving(c, BLOCKMOVE, &[second.wrapping_add(16), R + 300, 128], 0x8005_0118);
                if icons == 3 {
                    let third = c.m.r32(info + 32);
                    r.call_leaving(c, BLOCKMOVE, &[third.wrapping_add(16), R + 428, 128], 0x8005_0138);
                }
            }
        }
        c.m.w32(S + 48, 2);
        c.m.w32(S + 52, 0);
        c.r[2] = 0;
    })
}

/// `iMCRD_DoFileWrite` (MEMCARD.C, 0x80050170): the pending write done on card `card`: a new
/// file created and its header written first; then the data. 12 done, 13 a write refused, or
/// the error's code.
pub fn abi_do_file_write(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[3]) = (R, card);
        let refused = |c: &mut Cpu| {
            c.m.w32(S + 52, 1);
            c.r[2] = 13;
        };
        if c.m.r32(R + 40) & 0x200 != 0 {
            (r.0[1], r.0[2]) = (S, R + 4);
            let (port_id, blocks) = (c.m.r32(S + 60), c.m.r8(R + 47) as u32);
            let v = r.call_ptr_leaving(c, MC_CREATE, &[port_id, R + 4, blocks], 0x8005_01bc);
            c.m.w32(sp + 28, v);
            let e = port(c, &r, 0x8005_09b4, 0x8005_01d0, &[2, v, card], abi_handle_error);
            if e != 0 {
                c.r[2] = e;
                return;
            }
            r.call_leaving(c, TIMED_WAIT, &[64], 0x8005_01e0);
            c.m.w32(sp + 16, 512);
            let port_id = c.m.r32(S + 60);
            let v = r.call_ptr_leaving(c, MC_WRITE, &[port_id, R + 4, R + 44, 0, 512], 0x8005_01fc);
            c.m.w32(sp + 28, v);
            if v == 0 {
                refused(c);
                return;
            }
            let result = sync(c, &r, sp, 0, 0x8005_0214, None);
            let e = port(c, &r, 0x8005_09b4, 0x8005_022c, &[2, result, card], abi_handle_error);
            if e != 0 {
                c.r[2] = e;
                return;
            }
        }
        r.0[1] = S;
        let m = &mut c.m;
        let (port_id, data, offset, size) = (m.r32(S + 60), m.r32(R + 556), m.r32(R + 36).wrapping_add(512), m.r32(R + 32));
        m.w32(sp + 16, size);
        let v = r.call_ptr_leaving(c, MC_WRITE, &[port_id, R + 4, data, offset, size], 0x8005_025c);
        c.m.w32(sp + 28, v);
        if v == 0 {
            refused(c);
            return;
        }
        let result = sync(c, &r, sp, 0, 0x8005_0284, None);
        let e = port(c, &r, 0x8005_09b4, 0x8005_029c, &[2, result, card], abi_handle_error);
        if e != 0 {
            c.r[2] = e;
            return;
        }
        c.m.w32(S + 52, 0);
        c.m.w32(S + 48, 1);
        c.r[2] = 12;
    })
}

/// `iMCRD_DoFileDelete` (MEMCARD.C, 0x800502d4): the request's file deleted from card `card`:
/// 17 (the card to be reloaded), or 18 when there's no such file (the card's event 19).
pub fn abi_do_file_delete(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[1], r.0[0]) = (card, 17, S);
        let port_id = c.m.r32(S + 60);
        let v = r.call_ptr_leaving(c, MC_DELETE, &[port_id, R + 4], 0x8005_0308);
        c.r[2] = 17;
        if v == 0 {
            c.m.w32(S + 48, 1);
        } else if v == 5 {
            let at = port(c, &r, 0x8005_072c, 0x8005_0338, &[card], abi_getcard);
            c.m.w32(at + 4, 19);
            c.m.w32(S + 52, 1);
            c.r[2] = 18;
        }
    })
}

/// `MCRD_handlecardevents` (MEMCARD.C, 0x80050368): card `card`'s events: the library's last
/// operation's end (a card checked: there, new, gone, unformatted; a card loaded: its state)
/// or, with none pending, the pending operation started for the request's card (a card's
/// existence checked once its timer is out, its directory loaded, a write, a read, a delete).
/// The event: 2..5 the card's state, 21 busy, 22 waiting, 23 nothing, or the operation's.
pub fn abi_handlecardevents(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (card, 23);
        let info = port(c, &r, 0x8005_072c, 0x8005_0388, &[card], abi_getcard);
        r.0[2] = info;
        let state = r.call_ptr_leaving(c, MC_SYNC, &[0, sp + 16, sp + 20], 0x8005_039c);
        let timer = S + 64 + 4 * card.wrapping_sub(1);
        let v = match state as i32 {
            0 => 21,
            1 => {
                let (cmd, result) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
                match (cmd, result) {
                    (1, 1) => {
                        let now = c.m.r32(VSYNC_COUNT);
                        c.m.w32(S + 52, 1);
                        c.m.w32(timer, now);
                        c.m.w32(info, u32::MAX);
                        2
                    }
                    (1, 0) => {
                        let now = c.m.r32(VSYNC_COUNT);
                        c.m.w32(S + 52, 1);
                        c.m.w32(timer, now);
                        r.0[0] = 22;
                        if c.m.r32(info) == u32::MAX {
                            let port_id = c.m.r32(S + 60);
                            r.call_ptr_leaving(c, MC_ACCEPT, &[port_id], 0x8005_045c);
                        }
                        22
                    }
                    (1, 2) => {
                        c.m.w32(S + 52, 1);
                        c.m.w32(info, (-4i32) as u32);
                        3
                    }
                    (1, 3) => {
                        let port_id = c.m.r32(S + 60);
                        r.0[0] = 21;
                        r.call_ptr_leaving(c, MC_ACCEPT, &[port_id], 0x8005_04bc);
                        21
                    }
                    (1, _) => 23,
                    (2, 0 | 3) => {
                        c.m.w32(R, card);
                        c.m.w32(S + 48, 1);
                        4
                    }
                    (2, 1) => {
                        r.0[0] = 4;
                        port(c, &r, 0x8004_f968, 0x8005_0518, &[card], abi_init_card);
                        c.m.w32(info, u32::MAX);
                        2
                    }
                    (2, 2) => {
                        r.0[0] = 4;
                        port(c, &r, 0x8004_f968, 0x8005_0530, &[card], abi_init_card);
                        3
                    }
                    (2, 4) => {
                        c.m.w32(info, (-2i32) as u32);
                        5
                    }
                    _ => 23,
                }
            }
            -1 => {
                let pending = c.m.r32(S + 48);
                let start = |c: &mut Cpu, r: &Saved, f: fn(&mut Cpu), addr: u32, ra: u32| -> u32 {
                    if card != c.m.r32(R) {
                        return 23;
                    }
                    c.m.w32(info + 4, 0);
                    c.m.w32(S + 52, 0);
                    c.m.w32(S + 48, 0);
                    port(c, r, addr, ra, &[card], f)
                };
                match pending {
                    0 => {
                        if (c.m.r32(timer) as i32) >= 0 {
                            22
                        } else {
                            let port_id = c.m.r32(S + 60);
                            c.m.w32(S + 52, 0);
                            r.0[0] = 23;
                            if r.call_ptr_leaving(c, MC_EXIST, &[port_id], 0x8005_05b0) == 0 { 23 } else { 21 }
                        }
                    }
                    1 => start(c, &r, abi_load_card, 0x8005_082c, 0x8005_05f4),
                    2 => start(c, &r, abi_do_file_write, 0x8005_0170, 0x8005_0628),
                    3 => start(c, &r, abi_do_file_load, 0x8004_fc8c, 0x8005_065c),
                    4 => start(c, &r, abi_do_file_delete, 0x8005_02d4, 0x8005_0690),
                    _ => 23,
                }
            }
            _ => 23,
        };
        c.r[2] = v;
    })
}

/// `MCRD_fileexists` (MEMCARD.C, 0x800506b4): the directory entry of card `card` named the
/// prefix and `name`, or -1.
pub fn abi_fileexists(c: &mut Cpu) {
    let (card, name) = (c.r[4], c.r[5]);
    abi::frame(c, 56, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = name;
        let at = port(c, &r, 0x8005_072c, 0x8005_06cc, &[card], abi_getcard);
        r.0[1] = at + 16;
        r.call_leaving(c, STRCPY, &[sp + 16, S + 4], 0x8005_06e0);
        r.call_leaving(c, STRCAT, &[sp + 16, name], 0x8005_06ec);
        for k in 0..15u32 {
            r.0[0] = k;
            r.0[1] = at + 16 + 40 * k;
            if r.call_leaving(c, STRCMP, &[at + 16 + 40 * k, sp + 16], 0x8005_06fc) == 0 {
                c.r[2] = k;
                return;
            }
        }
        c.r[2] = u32::MAX;
    })
}

/// `MCRD_getcard` (MEMCARD.C, 0x8005072c): card `card`'s record (1..8), its port noted.
pub fn abi_getcard(c: &mut Cpu) {
    let k = c.r[4].wrapping_sub(1);
    c.m.w32(S + 60, ((k & 4) << 2) | (k & 3));
    (c.r[4], c.r[5], c.r[3]) = (k, S + 1180, k & 3);
    c.r[2] = k.wrapping_mul(CARD_SIZE).wrapping_add(CARDS);
}

/// `iMCRD_timersub` (MEMCARD.C, 0x80050774): every card's timer counted down (the timer's
/// handler).
pub fn abi_timersub(c: &mut Cpu) {
    for k in 0..8 {
        let v = c.m.r32(S + 64 + 4 * k).wrapping_sub(1);
        c.m.w32(S + 64 + 4 * k, v);
    }
    (c.r[4], c.r[3]) = (8, S + 32);
}

/// `garyMemCardGrabBlocks` (MEMCARD.C, 0x800507a4): card `card`'s directory read, entry `k`'s
/// size in blocks.
pub fn abi_grab_blocks(c: &mut Cpu) {
    let (card, k) = (c.r[4], c.r[5]);
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = k;
        let at = port(c, &r, 0x8005_072c, 0x8005_07bc, &[card], abi_getcard);
        r.0[0] = at + 16;
        c.m.w32(sp + 16, 0);
        c.m.w32(sp + 20, 15);
        let port_id = c.m.r32(S + 60);
        r.call_ptr_leaving(c, MC_DIRENTRY, &[port_id, ALL_FILES, at + 16, at + 8, 0, 15], 0x8005_07e8);
        let entry = if (k as i32) > 0 { at + 16 + 40 * k } else { at + 16 };
        c.r[2] = blocks(c.m.r32(entry + 24));
    })
}

/// `iMCRD_LoadCard` (MEMCARD.C, 0x8005082c): card `card` set up and its directory read; its
/// free blocks counted (none: -3, the card full). 9, 11 no card, or the error's code.
pub fn abi_load_card(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = card;
        let at = port(c, &r, 0x8005_072c, 0x8005_0848, &[card], abi_getcard);
        (r.0[1], r.0[0]) = (at, at + 16);
        if port(c, &r, 0x8004_f968, 0x8005_0858, &[card], abi_init_card) != 0 {
            c.r[2] = 11;
            return;
        }
        c.m.w32(sp + 16, 0);
        c.m.w32(sp + 20, 15);
        let port_id = c.m.r32(S + 60);
        let v = r.call_ptr_leaving(c, MC_DIRENTRY, &[port_id, ALL_FILES, at + 16, at + 8, 0, 15], 0x8005_0888);
        let e = port(c, &r, 0x8005_09b4, 0x8005_0898, &[1, v, card], abi_handle_error);
        if e != 0 {
            c.r[2] = e;
            return;
        }
        let m = &mut c.m;
        for k in 0..15 {
            let entry = at + 16 + 40 * k;
            if m.r8(entry) != 0 {
                let used = blocks(m.r32(entry + 24));
                let free = m.r32(at + 12).wrapping_sub(used);
                m.w32(at + 12, free);
            }
        }
        let state = if m.r32(at + 12) == 0 { (-3i32) as u32 } else { 0 };
        m.w32(at, state);
        m.w32(S + 52, 1);
        c.r[2] = 9;
    })
}

/// `iMCRD_FormatCard` (MEMCARD.C, 0x80050928): card `card` formatted: 15 blocks free, its
/// state cleared. 0, or -1 (the library's 1 or 2: no card, a bad one).
pub fn abi_format_card(c: &mut Cpu) {
    let card = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = 0;
        let at = port(c, &r, 0x8005_072c, 0x8005_0940, &[card], abi_getcard);
        r.0[0] = at;
        let port_id = c.m.r32(S + 60);
        let v = r.call_ptr_leaving(c, MC_FORMAT, &[port_id], 0x8005_0950);
        let result = match v as i32 {
            1 => {
                c.m.w32(at, u32::MAX);
                u32::MAX
            }
            2 => {
                c.m.w32(at, (-4i32) as u32);
                u32::MAX
            }
            _ => 0,
        };
        c.m.w32(at, 0);
        c.m.w32(at + 12, 15);
        c.r[2] = result;
    })
}

/// `iMCRD_HandleError` (MEMCARD.C, 0x800509b4): the library's error `err` of operation `op`
/// (1 the directory, 2 a write, 3 a read) on card `card`: the card's state and event set (no
/// card, a new one accepted, unformatted: formatted when the format callback agrees, a full
/// card, an existing file: deleted when the overwrite callback agrees). The operation's code
/// (10, 13, 16) when it failed, 0 when fine; 6 formatted, 21 an old file deleted.
pub fn abi_handle_error(c: &mut Cpu) {
    let (op, err, card) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[2], r.0[3], r.0[4]) = (op, err, card, 0);
        let at = port(c, &r, 0x8005_072c, 0x8005_09e8, &[card], abi_getcard);
        r.0[1] = at;
        let code = if err == 0 {
            0
        } else {
            match op {
                1 => 10,
                2 => 13,
                3 => 16,
                _ => 0,
            }
        };
        r.0[4] = code;
        let event = |c: &mut Cpu, e: u32| {
            c.m.w32(at + 4, e);
            c.m.w32(S + 52, 1);
            c.r[2] = code;
        };
        match err {
            0 => c.r[2] = code,
            1 => {
                c.m.w32(at, u32::MAX);
                event(c, 2);
            }
            2 => {
                c.m.w32(at, (-4i32) as u32);
                event(c, 3);
            }
            3 => {
                r.0[0] = S;
                let port_id = c.m.r32(S + 60);
                if r.call_ptr_leaving(c, MC_ACCEPT, &[port_id], 0x8005_0a98) == 0 {
                    event(c, 23);
                } else {
                    c.m.w32(S + 48, 1);
                    c.m.w32(S + 52, 0);
                    c.r[2] = code;
                }
            }
            4 => {
                if op != 2 {
                    c.m.w32(at, (-2i32) as u32);
                    event(c, 5);
                    return;
                }
                let cb = c.m.r32(S + 32);
                if r.call_ptr_leaving(c, cb, &[], 0x8005_0ad4) == 0 {
                    c.m.w32(at, (-2i32) as u32);
                    event(c, 5);
                    return;
                }
                (r.0[0], r.0[2]) = (0, u32::MAX);
                let mut v = u32::MAX;
                for k in 0..3 {
                    v = port(c, &r, 0x8005_0928, 0x8005_0ae8, &[card], abi_format_card);
                    r.0[0] = k + 1;
                    if v != u32::MAX {
                        break;
                    }
                }
                if v == u32::MAX {
                    c.m.w32(at, (-2i32) as u32);
                    event(c, 7);
                } else {
                    c.m.w32(S + 48, 2);
                    c.r[2] = 6;
                }
            }
            5 => event(c, 19),
            6 => {
                r.0[0] = S;
                let cb = c.m.r32(S + 36);
                if r.call_ptr_leaving(c, cb, &[], 0x8005_0b5c) == 0 {
                    event(c, 14);
                } else {
                    let port_id = c.m.r32(S + 60);
                    r.call_ptr_leaving(c, MC_DELETE, &[port_id, R + 4], 0x8005_0b70);
                    c.m.w32(S + 48, 2);
                    c.r[2] = 21;
                }
            }
            7 => event(c, 20),
            _ => event(c, 23),
        }
    })
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
pub fn abi_ascii2sjis(c: &mut Cpu) {
    let ch = c.r[4];
    let b = |v: u32| v & 0xFF;
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
        c.r[2] = 0;
        return;
    };
    let m = &mut c.m;
    let code = if punctuation != 0 {
        let at = (b(ch).wrapping_sub(punctuation + 31) << 1).wrapping_add(SJIS_PUNCTUATION);
        m.r16(at) as u32
    } else {
        let at = SJIS_RANGES + (range << 2);
        (m.r16(at) as u32).wrapping_add(b(ch)).wrapping_sub(m.r16(at + 2) as u32)
    };
    c.r[2] = swapped(code);
}

/// `sjis2ascii` (MEMCARD.C, 0x80050cf8): a header's Shift-JIS code (bytes swapped) in ASCII:
/// punctuation (0x81) from a table, digits and letters (0x82) from their ranges; 0 for others.
pub fn abi_sjis2ascii(c: &mut Cpu) {
    let code = c.r[4];
    let low = ((code << 16) as i32 >> 24) as u32;
    let m = &mut c.m;
    c.r[2] = match code & 0xFF {
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
    };
}

/// `MCRD_handlecardevents` and `MCRD_getcard` entered as the original's callers `jal` them.
pub(crate) fn handlecardevents_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8005_0368, None, &[]);
    abi_handlecardevents(c);
}

pub(crate) fn getcard_entry(c: &mut Cpu) {
    abi_getcard(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{CARD_ANSWERS, Rng, against_original_in, fe_ram};

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
