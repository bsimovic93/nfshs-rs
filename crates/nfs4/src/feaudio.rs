//! FEAUDIO.CPP: the front end's speech. The commentary's big file (a `.viv`: a lumpy header of
//! entries, each its offset and size big-endian and its name) is opened for the language; a
//! speech (`SPEECHINFO`, one at 0x800519D0) names a patch, which is read from the CD into a
//! buffer, added as a sound bank and played; a commentary's chain of patches (`c` names are
//! one-offs) follows on, the next read while one plays.
//!
//! `SPEECHINFO`: +0 the name (4 letters, the third counting up the chain), +4 a chain, +8 the
//! bank, +12 the playing sound, +16 reading, +17 playing, +18 read (to start), +20 a copied
//! header, +24 the read buffer, +28 the bank's data, +32 the read's handle.
//!
//! Written as FEMENU.CPP is (see there). `SNDplay` (the host logs it) and `systemtask` (the
//! host's) are called by address.

use crate::call::{call, call_addr, with_buffer};
use crate::globals::PATHS;
use ps1rt::{Cpu, Mem};

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
/// The name's letter counting up a chain (to `z` at most).
const CHAIN_LETTER: u32 = 2;
const LAST_LETTER: u8 = b'z';

/// A stop asked for; the chain's count (a byte, -1 none); the big files' names by language;
/// the open big file's header and its file name; the speech's volume (at most 90).
const STOP: u32 = 0x8005_19B0;
const CHAIN_COUNT: u32 = 0x8005_19B4;
const VIV_NAMES: u32 = 0x8005_19B8;
const VIV_HEADER: u32 = 0x8005_19F4;
const VIV_FILE: u32 = 0x8005_19F8;
const VOLUME: u32 = 0x8005_1A20;
const MAX_VOLUME: i32 = 90;
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
/// A big file's header: +0 its size, +8 its entries' count, +12 its first offset; the entries
/// from +16 (offset, size, name).
const VIV_COUNT: u32 = 8;
const VIV_ENTRIES: u32 = 16;
const VIV_HEAD_SIZE: u32 = 16;

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
/// A bank whose header the library keeps (`SNDbankadd`'s answer).
const BANK_KEEPS_HEADER: u32 = 7;
/// The music's volume (dimmed under speech).
const MUSIC_VOLUME: fn(&mut Cpu) = nfs4rc::f_8007bc44;
const SND_PLAY: u32 = 0x800E_861C;
const SYSTEM_TASK: u32 = 0x800E_77F0;
/// `SNDPLAYOPTS`: +0 cleared, +4 the bank, +8 the volume (12 bytes on).
const PLAY_OPTIONS: u32 = 28;

/// A big-endian word.
fn swap(v: u32) -> u32 {
    v.swap_bytes()
}

/// The music at `level * by / 128` of the music's level.
fn music_at(c: &mut Cpu, by: u32) {
    let level = c.m.r32(MUSIC_LEVEL);
    call(c, MUSIC_VOLUME, &[(level.wrapping_mul(by) as i32 >> 7) as u32]);
}

/// The block at `INFO + at` freed, if any.
fn free(c: &mut Cpu, at: u32) {
    let p = c.m.r32(INFO + at);
    if p != 0 {
        call(c, PURGE, &[p]);
        c.m.w32(INFO + at, 0);
    }
}

/// `FEAudio_StartLoadPatch` (FEAUDIO.CPP, 0x800158a8): the speech's patch found in the big
/// file and its read started (the last buffer freed); reading. False when it isn't there.
pub fn start_load_patch(c: &mut Cpu, info: u32) -> bool {
    call(c, FILE_DELETE, &[info]);
    let head = c.m.r32(VIV_HEADER);
    let (offset, size) = locate_bigfile(c, head, info);
    if offset == 0 || size == 0 {
        return false;
    }
    let buffer = c.m.r32(info + BUFFER);
    if buffer != 0 {
        call(c, PURGE, &[buffer]);
        c.m.w32(info + BUFFER, 0);
    }
    let v = start_bigfile_read(c, VIV_FILE, offset, size, info + HANDLE);
    let m = &mut c.m;
    m.w32(info + BUFFER, v);
    m.w8(info + READING, 1);
    m.w8(info + READ, 0);
    true
}

pub fn abi_start_load_patch(c: &mut Cpu) {
    c.r[2] = start_load_patch(c, c.r[4]) as u32;
}

/// `Feaudio_StartPatch` (FEAUDIO.CPP, 0x8001594c): the read patch added as a bank (a bank
/// whose header the library keeps: the header copied, the buffer freed; else the buffer kept as
/// the bank's data) and played at the speech's volume; a chain's next patch (up to `z`) read
/// meanwhile, else the speech done.
pub fn start_patch(c: &mut Cpu, info: u32) {
    let buffer = c.m.r32(info + BUFFER);
    if buffer == 0 {
        return;
    }
    if call(c, SND_BANK_ADD, &[info + BANK, buffer]) == BANK_KEEPS_HEADER {
        let bank = c.m.r32(info + BANK);
        let size = call(c, SND_BANK_HEADER_SIZE, &[bank]);
        let header = call(c, RESERVE, &[COM_HEADER, size, 16]);
        let bank = c.m.r32(info + BANK);
        c.m.w32(info + HEADER, header);
        call(c, SND_BANK_HEADER_COPY, &[header, bank]);
        let buffer = c.m.r32(info + BUFFER);
        if buffer != 0 {
            call(c, PURGE, &[buffer]);
            c.m.w32(info + BUFFER, 0);
        }
    } else {
        let m = &mut c.m;
        let buffer = m.r32(info + BUFFER);
        m.w32(info + BUFFER, 0);
        m.w32(info + DATA, buffer);
    }
    let sound = with_buffer(c, PLAY_OPTIONS, |c, options| {
        call(c, SND_PLAY_SET_DEF, &[options]);
        let m = &mut c.m;
        let bank = m.r8(info + BANK);
        let volume = m.r8(VOLUME);
        m.w32(options, 0);
        m.w8(options + 4, bank);
        m.w8(options + 8, volume);
        call_addr(c, SND_PLAY, &[options])
    });
    let m = &mut c.m;
    let chain = m.r32(info + CHAIN);
    m.w32(info + SOUND, sound);
    m.w8(info + PLAYING, 1);
    if chain == 0 {
        return;
    }
    if m.r8(info + CHAIN_LETTER) < LAST_LETTER {
        let n = m.r8(CHAIN_COUNT).wrapping_add(1);
        m.w8(CHAIN_COUNT, n);
        let letter = m.r8(info + CHAIN_LETTER).wrapping_add(1);
        m.w8(info + CHAIN_LETTER, letter);
        start_load_patch(c, info);
    } else {
        m.w8(info + READING, 0);
        m.w8(info + READ, 0);
    }
}

pub fn abi_start_patch(c: &mut Cpu) {
    start_patch(c, c.r[4]);
}

/// Waits for the speech's read, the system's task run meanwhile.
fn wait_read(c: &mut Cpu) {
    loop {
        let handle = c.m.r32(INFO + HANDLE);
        if call(c, READ_STATUS, &[handle]) != 0 {
            break;
        }
        call_addr(c, SYSTEM_TASK, &[0]);
    }
}

/// The speech's sound stopped (faded to nothing) and its bank dropped.
fn stop_sound(c: &mut Cpu) {
    let sound = c.m.r32(INFO + SOUND);
    call(c, SND_AUTO_VOL, &[sound, 0, u32::MAX]);
    let bank = c.m.r32(INFO + BANK);
    call(c, SND_BANK_REMOVE, &[bank]);
}

/// `FeAudio_systemtask` (FEAUDIO.CPP, 0x80015a70): the system's task; then the speech's read
/// finished; a sound over: its bank and memory freed (the next read waited for), a stop asked
/// for when nothing more is coming; a stop: the read waited for, the sound and bank dropped,
/// the music back up, every buffer freed; a patch read and nothing playing or reading: it
/// starts.
pub fn systemtask(c: &mut Cpu) {
    call_addr(c, SYSTEM_TASK, &[]);
    if c.m.r8(INFO + READING) != 0 {
        let handle = c.m.r32(INFO + HANDLE);
        if call(c, READ_STATUS, &[handle]) != 0 {
            c.m.w8(INFO + READING, 0);
            c.m.w8(INFO + READ, 1);
        }
    }
    if c.m.r8(INFO + PLAYING) != 0 {
        let sound = c.m.r32(INFO + SOUND);
        if call(c, SND_OVER, &[sound]) != 0 {
            stop_sound(c);
            free(c, DATA);
            free(c, HEADER);
            if c.m.r8(INFO + READING) != 0 {
                wait_read(c);
                c.m.w8(INFO + READING, 0);
                c.m.w8(INFO + READ, 1);
            }
            let m = &mut c.m;
            m.w8(INFO + PLAYING, 0);
            // (reading and read, as one word's bytes 0 and 2)
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
            wait_read(c);
        }
        if c.m.r8(INFO + PLAYING) != 0 {
            stop_sound(c);
        }
        music_at(c, 70);
        free(c, BUFFER);
        free(c, DATA);
        free(c, HEADER);
        let m = &mut c.m;
        m.w8(INFO + PLAYING, 0);
        m.w8(INFO + READ, 0);
        m.w32(STOP, 0);
        m.w8(INFO + READING, 0);
    }
    // (reading and playing, as one halfword)
    if c.m.r8(INFO + READ) != 0 && c.m.r16(INFO + READING) == 0 {
        start_patch(c, INFO);
        c.m.w8(INFO + READ, 0);
    }
}

pub fn abi_systemtask(c: &mut Cpu) {
    systemtask(c);
}

/// For callers not rewritten yet.
pub(crate) fn systemtask_entry(c: &mut Cpu) {
    systemtask(c);
}

/// The speech cleared: no bank, no sound, nothing reading, playing or read, no buffers.
fn clear(m: &mut Mem) {
    for at in [READING, PLAYING, READ] {
        m.w8(INFO + at, 0);
    }
    for at in [HEADER, BUFFER, DATA, HANDLE] {
        m.w32(INFO + at, 0);
    }
}

/// `FeAudio_AsyncPlayCommentary` (FEAUDIO.CPP, 0x80015cf4): the speech `name` (a chain unless
/// it starts with `c`) started reading at the narration's volume (at most 90), the music dimmed
/// once it reads.
pub fn async_play_commentary(c: &mut Cpu, name: u32) {
    let level = c.m.r32(NARRATION_LEVEL);
    let volume = level.wrapping_mul(110) as i32 >> 7;
    c.m.w32(VOLUME, volume as u32);
    if volume > MAX_VOLUME {
        c.m.w32(VOLUME, MAX_VOLUME as u32);
    }
    call(c, STRNCPY, &[INFO, name, 4]);
    let m = &mut c.m;
    m.w32(INFO + BANK, 0);
    m.w32(INFO + SOUND, 0);
    let first = m.r8(name);
    clear(m);
    m.w8(CHAIN_COUNT, 0xFF);
    m.w32(INFO + CHAIN, (first != b'c') as u32);
    if start_load_patch(c, INFO) {
        music_at(c, 35);
    }
}

pub fn abi_async_play_commentary(c: &mut Cpu) {
    async_play_commentary(c, c.r[4]);
    c.r[2] = 1;
}

/// `FeAudio_AsyncPlaySpeech` (FEAUDIO.CPP, 0x80015de0): speech `n` of kind `kind` (2: the
/// commentary `c` and two digits; 0: `a` and a letter; else the kind's letter and a digit)
/// played.
pub fn async_play_speech(c: &mut Cpu, kind: u32, n: u32) {
    with_buffer(c, 16, |c, name| {
        let args = match kind {
            2 => [name, COMMENTARY_FORMAT, b'c' as u32, n],
            0 => [name, SPEECH_FORMAT, b'a' as u32, n.wrapping_add(b'a' as u32)],
            _ => [name, SPEECH_FORMAT, kind.wrapping_add(b'a' as u32), n.wrapping_add(b'0' as u32)],
        };
        call(c, SPRINTF, &args);
        async_play_commentary(c, name);
    })
}

pub fn abi_async_play_speech(c: &mut Cpu) {
    async_play_speech(c, c.r[4], c.r[5]);
    c.r[2] = 1;
}

/// For callers not rewritten yet.
pub(crate) fn async_play_speech_entry(c: &mut Cpu) {
    abi_async_play_speech(c);
}

/// `FeAudio_StartBigfileRead` (FEAUDIO.CPP, 0x80015e5c): `size` bytes at `offset` of the file
/// `name` read into a new buffer, the read's handle at `handle`. The buffer, or 0 (no size,
/// no memory).
pub fn start_bigfile_read(c: &mut Cpu, name: u32, offset: u32, size: u32, handle: u32) -> u32 {
    if size == 0 {
        return 0;
    }
    let buffer = call(c, RESERVE, &[STREAM_BUFFER, size, 0]);
    if buffer == 0 {
        return 0;
    }
    call(c, SET_ASYNC_FILE, &[name]);
    let v = call(c, ASYNC_LOAD_SEGMENT, &[offset, buffer, size]);
    c.m.w32(handle, v);
    buffer
}

pub fn abi_start_bigfile_read(c: &mut Cpu) {
    c.r[2] = start_bigfile_read(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `FeAudio_LocateBigfile` (FEAUDIO.CPP, 0x80015ef4): the entry `name` of the big file's
/// header `head`: its offset and size (0 when it isn't there).
pub fn locate_bigfile(c: &mut Cpu, head: u32, name: u32) -> (u32, u32) {
    if head == 0 {
        return (0, 0);
    }
    let n = c.m.r32(head + VIV_COUNT);
    let mut entry = head + VIV_ENTRIES;
    for _ in 0..n {
        let entry_name = entry + 8;
        if call(c, STRCMP, &[name, entry_name]) == 0 {
            let m = &mut c.m;
            return (swap(m.r32(entry)), swap(m.r32(entry + 4)));
        }
        let m = &mut c.m;
        let mut end = entry_name;
        while m.r8(end) != 0 {
            end = end.wrapping_add(1);
        }
        entry = end.wrapping_add(1);
    }
    (0, 0)
}

pub fn abi_locate_bigfile(c: &mut Cpu) {
    let (offset, size) = (c.r[6], c.r[7]);
    c.m.w32(offset, 0);
    c.m.w32(size, 0);
    let (o, s) = locate_bigfile(c, c.r[4], c.r[5]);
    if (o, s) != (0, 0) {
        c.m.w32(offset, o);
        c.m.w32(size, s);
    }
}

/// A read of the big file waited for (the system's task run meanwhile).
fn read_and_wait(c: &mut Cpu, to: u32, size: u32) {
    let handle = call(c, ASYNC_LOAD_SEGMENT, &[0, to, size]);
    loop {
        call_addr(c, SYSTEM_TASK, &[0]);
        if call(c, READ_STATUS, &[handle]) != 0 {
            break;
        }
    }
}

/// `FeAudio_InitViv` (FEAUDIO.CPP, 0x8001604c): the big file `name`'s header read (its 16
/// bytes, then all of it, waited for) and made little-endian (size, count, first offset).
/// The header, or 0 (no memory).
pub fn init_viv(c: &mut Cpu, name: u32) -> u32 {
    call(c, SET_ASYNC_FILE, &[name]);
    let length = with_buffer(c, VIV_HEAD_SIZE, |c, first| {
        read_and_wait(c, first, VIV_HEAD_SIZE);
        swap(c.m.r32(first + 12))
    });
    let head = call(c, RESERVE, &[LUMPY_HEAD, length.wrapping_add(32), 0]);
    if head == 0 {
        return 0;
    }
    read_and_wait(c, head, length.wrapping_add(32));
    let m = &mut c.m;
    for at in [0, 12, 8] {
        let v = swap(m.r32(head + at));
        m.w32(head + at, v);
    }
    head
}

pub fn abi_init_viv(c: &mut Cpu) {
    c.r[2] = init_viv(c, c.r[4]);
}

/// `FeAudio_InitCommentary` (FEAUDIO.CPP, 0x80016200): the speech cleared (a chain, named
/// `000`), the language's big file opened.
pub fn init_commentary(c: &mut Cpu, language: u32) {
    let m = &mut c.m;
    m.w32(INFO + BANK, 0);
    let no_name = m.r32(NO_NAME);
    m.w32(INFO + CHAIN, 1);
    m.w32(INFO + SOUND, 0);
    clear(m);
    m.w32(INFO + NAME, no_name);
    let path = m.r32(SPEECH_PATH);
    let viv = m.r32((language << 2).wrapping_add(VIV_NAMES));
    call(c, SPRINTF, &[VIV_FILE, VIV_FORMAT, path, viv]);
    let head = init_viv(c, VIV_FILE);
    c.m.w32(VIV_HEADER, head);
}

pub fn abi_init_commentary(c: &mut Cpu) {
    init_commentary(c, c.r[4]);
}

/// `FeAudio_DeInitCommentary` (FEAUDIO.CPP, 0x800162a0): the big file's header freed.
pub fn deinit_commentary(c: &mut Cpu) {
    let head = c.m.r32(VIV_HEADER);
    if head != 0 {
        call(c, PURGE, &[head]);
        c.m.w32(VIV_HEADER, 0);
    }
}

pub fn abi_deinit_commentary(c: &mut Cpu) {
    deinit_commentary(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

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
            *b = crate::abi::call(c, RESERVE, &[RESERVE_NAME, 64, 0]);
            crate::abi::call(c, nfs4rc::f_800f24d4, &[*b, 64]);
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

}
