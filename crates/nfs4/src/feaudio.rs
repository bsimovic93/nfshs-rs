//! FEAUDIO.CPP: the front end's speech. The commentary's big file (a `.viv`: a lumpy header of
//! entries, each its offset and size big-endian and its name) is opened for the language; a
//! speech (`SPEECHINFO`, one at 0x800519D0) names a patch, which is read from the CD into a
//! buffer, added as a sound bank and played; a commentary's chain of patches (`c` names are
//! one-offs) follows on, the next read while one plays.
//!
//! `SPEECHINFO`: +0 the name (4 letters, the third counting up the chain), +4 a chain, +8 the
//! bank, +12 the playing sound, +16 reading, +17 playing, +18 read (to start), +20 a copied
//! header, +24 the read buffer, +28 the bank's data, +32 the read's handle.

use crate::abi::{self, Saved};
use crate::globals::PATHS;
use ps1rt::Cpu;

const INFO: u32 = 0x8005_19D0;
const NAME: u32 = 0;
const CHAIN: u32 = 4;
const BANK: u32 = 8;
const SOUND: u32 = 12;
const READING: u32 = 16;
const PLAYING: u32 = 17;
const READ: u32 = 18;
const HEADER: u32 = 20;
const BUFFER: u32 = 24;
const DATA: u32 = 28;
const HANDLE: u32 = 32;

/// A stop asked for; the chain's count (a byte, -1 none); the big files' names by language;
/// the open big file's header and its file name; the speech's volume.
const STOP: u32 = 0x8005_19B0;
const CHAIN_COUNT: u32 = 0x8005_19B4;
const VIV_NAMES: u32 = 0x8005_19B8;
const VIV_HEADER: u32 = 0x8005_19F4;
const VIV_FILE: u32 = 0x8005_19F8;
const VOLUME: u32 = 0x8005_1A20;
/// The music's and the front end's narration's levels.
const MUSIC_LEVEL: u32 = 0x8013_D944;
const NARRATION_LEVEL: u32 = 0x8013_D950;
/// The heap's names and the formats (`"comHeader"`, `"%c%02d"`, `"%c%ca"`, `"streamBuffer"`,
/// `"lumpyhead"`, `"000"`, `"%s%s.viv"`); the speech's path (`Paths` +0x98).
const COM_HEADER: u32 = 0x8001_00FC;
const COMMENTARY_FORMAT: u32 = 0x8001_0108;
const SPEECH_FORMAT: u32 = 0x8001_0110;
const STREAM_BUFFER: u32 = 0x8001_0118;
const LUMPY_HEAD: u32 = 0x8001_0128;
const NO_NAME: u32 = 0x8001_0164;
const VIV_FORMAT: u32 = 0x8001_0168;
const SPEECH_PATH: u32 = PATHS + 0x98;

const FILE_DELETE: fn(&mut Cpu) = nfs4rc::f_800f6a70;
const STRNCPY: fn(&mut Cpu) = nfs4rc::f_800f6a60;
const STRCMP: fn(&mut Cpu) = nfs4rc::f_800e6968;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;
const SET_ASYNC_FILE: fn(&mut Cpu) = nfs4rc::f_800f2190;
const ASYNC_LOAD_SEGMENT: fn(&mut Cpu) = nfs4rc::f_800f22e4;
const READ_STATUS: fn(&mut Cpu) = nfs4rc::f_800f240c;
const SND_BANK_ADD: fn(&mut Cpu) = nfs4rc::f_800e89d8;
const SND_BANK_HEADER_SIZE: fn(&mut Cpu) = nfs4rc::f_800e8750;
const SND_BANK_HEADER_COPY: fn(&mut Cpu) = nfs4rc::f_800e8794;
const SND_BANK_REMOVE: fn(&mut Cpu) = nfs4rc::f_800e7260;
const SND_PLAY_SET_DEF: fn(&mut Cpu) = nfs4rc::f_800e7420;
const SND_OVER: fn(&mut Cpu) = nfs4rc::f_800e8700;
const SND_AUTO_VOL: fn(&mut Cpu) = nfs4rc::f_800e8ebc;
/// The music's volume (dimmed under speech).
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
/// `SNDplay` (hooked: the host logs it), `systemtask` (the host's).
const SND_PLAY: u32 = 0x800E_861C;
const SYSTEM_TASK: u32 = 0x800E_77F0;

/// A big-endian word.
fn swap(v: u32) -> u32 {
    v.swap_bytes()
}

/// `FEAudio_StartLoadPatch` (FEAUDIO.CPP, 0x800158a8): the speech's patch found in the big
/// file and its read started (the last buffer freed); reading. 0 when it isn't there.
pub fn abi_start_load_patch(c: &mut Cpu) {
    let info = c.r[4];
    abi::frame(c, 32, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        r.call_leaving(c, FILE_DELETE, &[info], 0x8001_58bc);
        let head = c.m.r32(VIV_HEADER);
        r.enter_leaving(c, 0x8001_5ef4, 0x8001_58d4, |c| {
            (c.r[4], c.r[5], c.r[6], c.r[7]) = (head, info, sp + 16, sp + 20);
            abi_locate_bigfile(c)
        });
        let (offset, size) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
        if offset == 0 || size == 0 {
            c.r[2] = 0;
            return;
        }
        let buffer = c.m.r32(info + BUFFER);
        if buffer != 0 {
            r.call_leaving(c, PURGE, &[buffer], 0x8001_590c);
            c.m.w32(info + BUFFER, 0);
        }
        let v = r.enter_leaving(c, 0x8001_5e5c, 0x8001_5928, |c| {
            (c.r[4], c.r[5], c.r[6], c.r[7]) = (VIV_FILE, offset, size, info + HANDLE);
            abi_start_bigfile_read(c);
            c.r[2]
        });
        let m = &mut c.m;
        m.w32(info + BUFFER, v);
        m.w8(info + READING, 1);
        m.w8(info + READ, 0);
        (c.r[2], c.r[3]) = (1, 1);
    })
}

/// `Feaudio_StartPatch` (FEAUDIO.CPP, 0x8001594c): the read patch added as a bank (a bank
/// whose header the library keeps (7): the header copied, the buffer freed; else the buffer
/// kept as the bank's data) and played at the speech's volume; a chain's next patch (up to
/// `z`) read meanwhile, else the speech done.
pub fn abi_start_patch(c: &mut Cpu) {
    let info = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = info;
        let buffer = c.m.r32(info + BUFFER);
        if buffer == 0 {
            return;
        }
        if r.call_leaving(c, SND_BANK_ADD, &[info + BANK, buffer], 0x8001_5974) == 7 {
            let bank = c.m.r32(info + BANK);
            let size = r.call_leaving(c, SND_BANK_HEADER_SIZE, &[bank], 0x8001_598c);
            let header = r.call_leaving(c, RESERVE, &[COM_HEADER, size, 16], 0x8001_59a0);
            let bank = c.m.r32(info + BANK);
            c.m.w32(info + HEADER, header);
            r.call_leaving(c, SND_BANK_HEADER_COPY, &[header, bank], 0x8001_59b0);
            let buffer = c.m.r32(info + BUFFER);
            if buffer != 0 {
                r.call_leaving(c, PURGE, &[buffer], 0x8001_59c8);
                c.m.w32(info + BUFFER, 0);
            }
        } else {
            let m = &mut c.m;
            let buffer = m.r32(info + BUFFER);
            m.w32(info + BUFFER, 0);
            m.w32(info + DATA, buffer);
        }
        r.call_leaving(c, SND_PLAY_SET_DEF, &[sp + 16], 0x8001_59e4);
        let m = &mut c.m;
        let bank = m.r8(info + BANK);
        let volume = m.r8(VOLUME);
        m.w32(sp + 16, 0);
        m.w8(sp + 20, bank);
        m.w8(sp + 24, volume);
        let sound = r.call_ptr_leaving(c, SND_PLAY, &[sp + 16], 0x8001_5a04);
        let m = &mut c.m;
        let chain = m.r32(info + CHAIN);
        m.w32(info + SOUND, sound);
        m.w8(info + PLAYING, 1);
        c.r[2] = 1;
        if chain == 0 {
            return;
        }
        let m = &mut c.m;
        if (m.r8(info + 2) as u32) < 122 {
            let n = m.r8(CHAIN_COUNT).wrapping_add(1);
            m.w8(CHAIN_COUNT, n);
            let letter = m.r8(info + 2).wrapping_add(1);
            m.w8(info + 2, letter);
            r.enter_leaving(c, 0x8001_58a8, 0x8001_5a50, |c| {
                c.r[4] = info;
                abi_start_load_patch(c)
            });
        } else {
            m.w8(info + READING, 0);
            m.w8(info + READ, 0);
            c.r[2] = 0x8005_0000;
        }
    })
}

/// Waits for the speech's read, the system's task run meanwhile; then not reading, read.
fn wait_read(c: &mut Cpu, r: &Saved, ra: u32, task_ra: u32) {
    loop {
        let handle = c.m.r32(INFO + HANDLE);
        if r.call_leaving(c, READ_STATUS, &[handle], ra) != 0 {
            break;
        }
        r.call_ptr_leaving(c, SYSTEM_TASK, &[0], task_ra);
    }
}

/// `FeAudio_systemtask` (FEAUDIO.CPP, 0x80015a70): the system's task; then the speech's read
/// finished; a sound over: its bank and memory freed (the next read waited for), a stop asked
/// for at the chain's end; a stop: the read waited for, the sound and bank dropped, the music
/// back up, every buffer freed; a patch read and nothing playing or reading: it starts.
pub fn abi_systemtask(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.call_ptr_leaving(c, SYSTEM_TASK, &[], 0x8001_5a80);
        r.0[0] = INFO;
        if c.m.r8(INFO + READING) != 0 {
            let handle = c.m.r32(INFO + HANDLE);
            if r.call_leaving(c, READ_STATUS, &[handle], 0x8001_5aa4) != 0 {
                c.m.w8(INFO + READING, 0);
                c.m.w8(INFO + READ, 1);
            }
        }
        if c.m.r8(INFO + PLAYING) != 0 {
            let sound = c.m.r32(INFO + SOUND);
            if r.call_leaving(c, SND_OVER, &[sound], 0x8001_5ad8) != 0 {
                let sound = c.m.r32(INFO + SOUND);
                r.call_leaving(c, SND_AUTO_VOL, &[sound, 0, u32::MAX], 0x8001_5aec);
                let bank = c.m.r32(INFO + BANK);
                r.call_leaving(c, SND_BANK_REMOVE, &[bank], 0x8001_5af8);
                for at in [DATA, HEADER] {
                    let p = c.m.r32(INFO + at);
                    if p != 0 {
                        r.call_leaving(c, PURGE, &[p], if at == DATA { 0x8001_5b10 } else { 0x8001_5b2c });
                        c.m.w32(INFO + at, 0);
                    }
                }
                if c.m.r8(INFO + READING) != 0 {
                    wait_read(c, &r, 0x8001_5b4c, 0x8001_5b5c);
                    c.m.w8(INFO + READING, 0);
                    c.m.w8(INFO + READ, 1);
                }
                let m = &mut c.m;
                m.w8(INFO + PLAYING, 0);
                let busy = m.r32(INFO + READING) & 0x00FF_00FF;
                m.w32(INFO + BANK, 0);
                if busy == 0 {
                    m.w32(STOP, 1);
                }
            }
        }
        if c.m.r32(STOP) != 0 {
            let reading = c.m.r8(INFO + READING);
            c.m.w8(CHAIN_COUNT, 0xFF);
            if reading != 0 {
                wait_read(c, &r, 0x8001_5bd8, 0x8001_5be8);
            }
            if c.m.r8(INFO + PLAYING) != 0 {
                let sound = c.m.r32(INFO + SOUND);
                r.call_leaving(c, SND_AUTO_VOL, &[sound, 0, u32::MAX], 0x8001_5c14);
                let bank = c.m.r32(INFO + BANK);
                r.call_leaving(c, SND_BANK_REMOVE, &[bank], 0x8001_5c20);
            }
            let level = c.m.r32(MUSIC_LEVEL);
            r.call_leaving(c, MUSIC_VOLUME, &[(level.wrapping_mul(70) as i32 >> 7) as u32], 0x8001_5c48);
            for (at, ra) in [(BUFFER, 0x8001_5c60), (DATA, 0x8001_5c7c), (HEADER, 0x8001_5c98)] {
                let p = c.m.r32(INFO + at);
                if p != 0 {
                    r.call_leaving(c, PURGE, &[p], ra);
                    c.m.w32(INFO + at, 0);
                }
            }
            let m = &mut c.m;
            m.w8(INFO + PLAYING, 0);
            m.w8(INFO + READ, 0);
            m.w32(STOP, 0);
            m.w8(INFO + READING, 0);
        }
        if c.m.r8(INFO + READ) != 0 && c.m.r16(INFO + READING) == 0 {
            r.enter_leaving(c, 0x8001_594c, 0x8001_5ce0, |c| {
                c.r[4] = INFO;
                abi_start_patch(c)
            });
            c.m.w8(INFO + READ, 0);
        }
    })
}

/// `FeAudio_AsyncPlayCommentary` (FEAUDIO.CPP, 0x80015cf4): the speech `name` (a chain unless
/// it starts with `c`) started reading at the narration's volume (at most 90), the music dimmed
/// once it reads. 1.
pub fn abi_async_play_commentary(c: &mut Cpu) {
    let name = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = name;
        let level = c.m.r32(NARRATION_LEVEL);
        let volume = level.wrapping_mul(110) as i32 >> 7;
        c.m.w32(VOLUME, volume as u32);
        if volume >= 91 {
            c.m.w32(VOLUME, 90);
        }
        r.0[0] = INFO;
        r.call_leaving(c, STRNCPY, &[INFO, name, 4], 0x8001_5d58);
        let m = &mut c.m;
        m.w32(INFO + BANK, 0);
        m.w32(INFO + SOUND, 0);
        let first = m.r8(name) as u32;
        for at in [READING, PLAYING, READ] {
            m.w8(INFO + at, 0);
        }
        for at in [HEADER, BUFFER, DATA, HANDLE] {
            m.w32(INFO + at, 0);
        }
        m.w8(CHAIN_COUNT, 0xFF);
        m.w32(INFO + CHAIN, (first != b'c' as u32) as u32);
        let started = r.enter_leaving(c, 0x8001_58a8, 0x8001_5da0, |c| {
            c.r[4] = INFO;
            abi_start_load_patch(c);
            c.r[2]
        });
        if started != 0 {
            let level = c.m.r32(MUSIC_LEVEL);
            r.call_leaving(c, MUSIC_VOLUME, &[(level.wrapping_mul(35) as i32 >> 7) as u32], 0x8001_5dc8);
        }
        c.r[2] = 1;
    })
}

/// `FeAudio_AsyncPlaySpeech` (FEAUDIO.CPP, 0x80015de0): speech `n` of kind `kind` (2: the
/// commentary `c` and two digits; 0: `a` and a letter; else the kind's letter and a digit)
/// played. Its result as a `short`.
pub fn abi_async_play_speech(c: &mut Cpu) {
    let (kind, n) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, sp| {
        let r = Saved::of(c);
        let args = match kind {
            2 => [sp + 16, COMMENTARY_FORMAT, 99, n],
            0 => [sp + 16, SPEECH_FORMAT, 97, n.wrapping_add(97)],
            _ => [sp + 16, SPEECH_FORMAT, kind.wrapping_add(97), n.wrapping_add(48)],
        };
        r.call_leaving(c, SPRINTF, &args, 0x8001_5e40);
        let v = r.enter_leaving(c, 0x8001_5cf4, 0x8001_5e48, |c| {
            c.r[4] = sp + 16;
            abi_async_play_commentary(c);
            c.r[2]
        });
        c.r[2] = v as i16 as i32 as u32;
    })
}

/// `FeAudio_StartBigfileRead` (FEAUDIO.CPP, 0x80015e5c): `size` bytes at `offset` of the file
/// `name` read into a new buffer, the read's handle at `handle`. The buffer, or 0 (no size,
/// no memory).
pub fn abi_start_bigfile_read(c: &mut Cpu) {
    let (name, offset, size, handle) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3], r.0[1], r.0[4]) = (name, offset, size, handle);
        c.r[2] = 0;
        if size == 0 {
            return;
        }
        let buffer = r.call_leaving(c, RESERVE, &[STREAM_BUFFER, size, 0], 0x8001_5ea0);
        r.0[0] = buffer;
        if buffer == 0 {
            c.r[2] = 0;
            return;
        }
        r.call_leaving(c, SET_ASYNC_FILE, &[name], 0x8001_5eb4);
        let v = r.call_leaving(c, ASYNC_LOAD_SEGMENT, &[offset, buffer, size], 0x8001_5ec4);
        c.m.w32(handle, v);
        c.r[2] = buffer;
    })
}

/// `FeAudio_LocateBigfile` (FEAUDIO.CPP, 0x80015ef4): the entry `name` of the big file's
/// header `head` (+8 the count, entries from +16: offset, size, name): its offset and size
/// (0 when it isn't there).
pub fn abi_locate_bigfile(c: &mut Cpu) {
    let (head, name, offset, size) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        (r.0[3], r.0[8], r.0[6], r.0[7]) = (head, name, offset, size);
        c.m.w32(offset, 0);
        c.m.w32(size, 0);
        if head == 0 {
            return;
        }
        let mut entry = head + 16;
        r.0[1] = entry;
        r.0[2] = 1;
        if c.m.r32(head + 8) == 0 {
            return;
        }
        (r.0[4], r.0[5]) = (0x00FF_0000, 0xFF00_0000);
        let mut k = 1u32;
        loop {
            let entry_name = entry + 8;
            r.0[0] = entry_name;
            if r.call_leaving(c, STRCMP, &[name, entry_name], 0x8001_5f68) == 0 {
                let m = &mut c.m;
                let v = swap(m.r32(entry));
                m.w32(offset, v);
                let v = swap(m.r32(entry + 4));
                m.w32(size, v);
                return;
            }
            let m = &mut c.m;
            let mut end = entry_name;
            while m.r8(end) != 0 {
                end = end.wrapping_add(1);
            }
            entry = end.wrapping_add(1);
            r.0[1] = entry;
            let more = k < m.r32(head + 8);
            k += 1;
            r.0[2] = k;
            if !more {
                c.r[2] = 0;
                return;
            }
        }
    })
}

/// `FeAudio_InitViv` (FEAUDIO.CPP, 0x8001604c): the big file `name`'s header read (its 16
/// bytes, then all of it, waited for) and made little-endian (size, count, first offset).
/// The header, or 0 (no memory).
pub fn abi_init_viv(c: &mut Cpu) {
    let name = c.r[4];
    abi::frame(c, 48, |c, sp| {
        let mut r = Saved::of(c);
        r.call_leaving(c, SET_ASYNC_FILE, &[name], 0x8001_6060);
        let handle = r.call_leaving(c, ASYNC_LOAD_SEGMENT, &[0, sp + 16, 16], 0x8001_6070);
        r.0[0] = handle;
        loop {
            r.call_ptr_leaving(c, SYSTEM_TASK, &[0], 0x8001_607c);
            if r.call_leaving(c, READ_STATUS, &[handle], 0x8001_6084) != 0 {
                break;
            }
        }
        let m = &mut c.m;
        let v = swap(m.r32(sp + 16));
        m.w32(sp + 16, v);
        let length = swap(m.r32(sp + 28));
        m.w32(sp + 28, length);
        let v = swap(m.r32(sp + 24));
        m.w32(sp + 24, v);
        let head = r.call_leaving(c, RESERVE, &[LUMPY_HEAD, length.wrapping_add(32), 0], 0x8001_6124);
        r.0[1] = head;
        c.r[4] = 0;
        if head == 0 {
            c.r[2] = 0;
            return;
        }
        let length = c.m.r32(sp + 28);
        let handle = r.call_leaving(c, ASYNC_LOAD_SEGMENT, &[0, head, length.wrapping_add(32)], 0x8001_6148);
        r.0[0] = handle;
        loop {
            r.call_ptr_leaving(c, SYSTEM_TASK, &[0], 0x8001_6154);
            if r.call_leaving(c, READ_STATUS, &[handle], 0x8001_615c) != 0 {
                break;
            }
        }
        let m = &mut c.m;
        let (a, b, d) = (m.r32(head), m.r32(head + 12), m.r32(head + 8));
        m.w32(head, swap(a));
        m.w32(head + 12, swap(b));
        m.w32(head + 8, swap(d));
        c.r[2] = head;
    })
}

/// `FeAudio_InitCommentary` (FEAUDIO.CPP, 0x80016200): the speech cleared (a chain, named
/// `000`), the language's big file opened.
pub fn abi_init_commentary(c: &mut Cpu) {
    let language = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = VIV_FILE;
        let m = &mut c.m;
        m.w32(INFO + BANK, 0);
        let no_name = m.r32(NO_NAME);
        m.w32(INFO + CHAIN, 1);
        m.w32(INFO + SOUND, 0);
        for at in [READING, PLAYING, READ] {
            m.w8(INFO + at, 0);
        }
        for at in [HEADER, BUFFER, DATA, HANDLE] {
            m.w32(INFO + at, 0);
        }
        let names = (language << 2).wrapping_add(VIV_NAMES);
        m.w32(INFO + NAME, no_name);
        let path = m.r32(SPEECH_PATH);
        let viv = m.r32(names);
        r.call_leaving(c, SPRINTF, &[VIV_FILE, VIV_FORMAT, path, viv], 0x8001_6280);
        let head = r.enter_leaving(c, 0x8001_604c, 0x8001_6288, |c| {
            c.r[4] = VIV_FILE;
            abi_init_viv(c);
            c.r[2]
        });
        c.m.w32(VIV_HEADER, head);
        c.r[2] = head;
    })
}

/// `FeAudio_DeInitCommentary` (FEAUDIO.CPP, 0x800162a0): the big file's header freed.
pub fn abi_deinit_commentary(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8005_0000;
        let head = c.m.r32(VIV_HEADER);
        if head != 0 {
            r.call_leaving(c, PURGE, &[head], 0x8001_62c4);
            c.m.w32(VIV_HEADER, 0);
        }
    })
}

/// `FeAudio_systemtask` and `FeAudio_AsyncPlaySpeech` entered as the original's callers `jal`
/// them.
pub(crate) fn systemtask_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_5a70, None, &[]);
    abi_systemtask(c);
}

pub(crate) fn async_play_speech_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_5de0, None, &[]);
    abi_async_play_speech(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

    /// The RAM image's commentary header (`zzEngl.viv`, 162 entries) and some of its names;
    /// names it hasn't.
    const REAL_HEADER: u32 = 0x8017_3C08;
    const NAMES: [&[u8]; 8] = [b"aaa\0", b"aba\0", b"afe\0", b"c07\0", b"c50\0", b"aag\0", b"zzz\0", b"c99\0"];
    /// A name to play, an empty header, outputs; the CD's read queue (FESCREEN's tests'
    /// entries 28 and 29).
    const NAME_AT: u32 = 0x8014_5000;
    const EMPTY_HEADER: u32 = 0x8014_5020;
    const OUT: u32 = 0x8014_5080;
    const REQUESTS: u32 = 0x8013_F1A0;
    const READS: [u32; 2] = [28, 29];
    const RESERVE_NAME: u32 = 0x8001_0118;
    /// The sound library's state (`sndgs`): +0x3c on, +0x94 its channels (100 bytes: +0 the
    /// sound's handle, +11 in use), 24 of them.
    const SNDGS: u32 = 0x8014_8B9C;

    fn put_name(m: &mut ps1rt::Mem, at: u32, rng: &mut Rng) {
        for (k, &b) in NAMES[pick(rng, NAMES.len() as u32) as usize].iter().enumerate() {
            m.w8(at + k as u32, b);
        }
    }

    /// The speech in any state: its name, chain, flags, buffers (0 or the heap's, cleared),
    /// its read done, failed or in error (none still reading: the original would wait for it
    /// for ever); a stop asked for or not; the chain's count; the levels about the volume's
    /// clamp; the big file open or not.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let mut blocks = [0; 3];
        for b in &mut blocks {
            *b = abi::call(c, RESERVE, &[RESERVE_NAME, 64, 0]);
            abi::call(c, nfs4rc::f_800f24d4, &[*b, 64]);
        }
        let m = &mut c.m;
        put_name(m, INFO + NAME, rng);
        put_name(m, NAME_AT, rng);
        m.w32(INFO + CHAIN, pick(rng, 2));
        m.w32(INFO + BANK, 0);
        // a sound still playing on a channel, one over, none; the library off now and then
        let channel = pick(rng, 24);
        let handle = 0x100 | channel;
        let chan = m.r32(SNDGS + 0x94) + 100 * channel;
        m.w32(chan, handle);
        m.w8(chan + 11, pick(rng, 2) as u8);
        m.w32(INFO + SOUND, [0, handle, handle][pick(rng, 3) as usize]);
        m.w8(SNDGS + 0x3C, (pick(rng, 5) != 0) as u8);
        for at in [READING, PLAYING, READ] {
            m.w8(INFO + at, (pick(rng, 3) == 0) as u8);
        }
        for (k, at) in [HEADER, BUFFER, DATA].into_iter().enumerate() {
            m.w32(INFO + at, [0, blocks[k]][pick(rng, 2) as usize]);
        }
        for (k, &n) in READS.iter().enumerate() {
            let e = m.r32(REQUESTS) + 44 * n;
            let state = [0, 2, 3][pick(rng, 3) as usize];
            m.w32(e, 0x300 | n);
            m.w32(e + 8, if state == 2 { 0 } else { 0x1000 });
            m.w32(e + 12, (state == 3) as u32);
            m.w32(e + 16, blocks[k]);
            m.w32(e + 20, 1);
            m.w32(e + 24, 0);
        }
        m.w32(INFO + HANDLE, 0x300 | READS[pick(rng, 2) as usize]);
        m.w8(INFO + 2, [b'a', b'g', b'y', b'z', pick(rng, 256) as u8][pick(rng, 5) as usize]);
        m.w32(STOP, (pick(rng, 3) == 0) as u32);
        m.w8(CHAIN_COUNT, pick(rng, 256) as u8);
        m.w32(NARRATION_LEVEL, [0, 105, 106, 127, pick(rng, 128)][pick(rng, 5) as usize]);
        m.w32(MUSIC_LEVEL, pick(rng, 128));
        m.w32(VOLUME, pick(rng, 128));
        m.w32(VIV_HEADER, [REAL_HEADER, REAL_HEADER, 0][pick(rng, 3) as usize]);
        // a small header: `aaa`, then the name to play (found only past a count of 1)
        m.w32(EMPTY_HEADER + 8, pick(rng, 3));
        m.w32(EMPTY_HEADER + 16, 0x1000_0000);
        m.w32(EMPTY_HEADER + 20, 0x2000_0000);
        m.w32(EMPTY_HEADER + 24, 0x0061_6161);
        let second = EMPTY_HEADER + 28;
        m.w32(second, 0x3000_0000);
        m.w32(second + 4, 0x4000_0000);
        for k in 0..4 {
            let b = m.r8(NAME_AT + k);
            m.w8(second + 8 + k, b);
        }
        // now and then an entry of the real header (`aba`, the 8th) of size 0
        let mut entry = REAL_HEADER + 16;
        for _ in 0..7 {
            entry += 8;
            while m.r8(entry) != 0 {
                entry += 1;
            }
            entry += 1;
        }
        let size = if pick(rng, 3) == 0 { 0 } else { 0x0010_0000 };
        m.w32(entry + 4, size);
        m.w32(OUT, rng.next() as u32);
        m.w32(OUT + 4, rng.next() as u32);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("FeAudio_LocateBigfile", 0x8001_5ef4, nfs4rc::f_80015ef4, |_, r| vec![[REAL_HEADER, REAL_HEADER, 0, EMPTY_HEADER][pick(r, 4) as usize], NAME_AT, OUT, OUT + 4]),
            ("FeAudio_StartBigfileRead", 0x8001_5e5c, nfs4rc::f_80015e5c, |_, r| vec![VIV_FILE, pick(r, 0x10_0000), [0, 1, 0x800, pick(r, 0x4000)][pick(r, 4) as usize], OUT]),
            ("FEAudio_StartLoadPatch", 0x8001_58a8, nfs4rc::f_800158a8, |_, _| vec![INFO]),
            ("Feaudio_StartPatch", 0x8001_594c, nfs4rc::f_8001594c, |_, _| vec![INFO]),
            ("FeAudio_systemtask", 0x8001_5a70, nfs4rc::f_80015a70, |_, r| vec![pick(r, 2)]),
            ("FeAudio_AsyncPlayCommentary", 0x8001_5cf4, nfs4rc::f_80015cf4, |_, _| vec![NAME_AT]),
            ("FeAudio_AsyncPlaySpeech", 0x8001_5de0, nfs4rc::f_80015de0, |_, r| vec![[0, 1, 2, 3, pick(r, 26)][pick(r, 5) as usize], [0, 1, 7, 9, 50, pick(r, 100)][pick(r, 6) as usize]]),
            ("FeAudio_DeInitCommentary", 0x8001_62a0, nfs4rc::f_800162a0, |_, _| vec![]),
        ]
    }

    #[test]
    fn feaudio_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x8A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0x8A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn feaudio_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 100, 0x8B1 + k as u64, shuffle, &args);
        }
    }
}
