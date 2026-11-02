//! FETRACKS.CPP: the front end's track manager (`tTrackManager`, `trackManager`): the tracks'
//! descriptions from fetrk.trk, which tracks are available and which are bonus tracks, saved
//! and loaded with the game; and the track menu's iterator (`tListIteratorTrack`), which skips
//! the tracks the race type can't have.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::{call, with_buffer};
use crate::femenu;
use crate::femenuextended::SPRINTF;
use crate::globals::{FRONT_END, PATHS};
use crate::types::ListIteratorIndexed;
use ps1rt::{Cpu, Mem};

// A track manager: +0 how many tracks, +4 their descriptions (48 bytes each: +0 the track's
// id, +2 its class, +3 available (from the start in the file; a copy's: now), +4 a bonus
// track), +8 each id's track available (16 words), +72 each id's track a bonus one unlocked (16
// words).
const COUNT: u32 = 0;
const DESCRIPTIONS: u32 = 4;
const AVAILABLE: u32 = 8;
const UNLOCKED: u32 = 72;
const DESCRIPTION_SIZE: u32 = 48;
const TRACKS: u32 = 16;
const ID: u32 = 0;
const CLASS: u32 = 2;
const AVAILABLE_NOW: u32 = 3;
const BONUS: u32 = 4;
/// Tournaments race the first three classes only.
const TOURNAMENT_CLASSES: u8 = 3;

/// The track iterator's vtable (at +12); its value's list (+4), the index of the player's value
/// (+16) and its track manager (+20).
const TRACK_ITERATOR_VT: u32 = 0x8001_0FE0;
const VPTR: u32 = 12;
const VALUES: u32 = 4;
const INDEX: u32 = 16;
const MANAGER: u32 = 20;
/// The tracks' names' texts (from 214 on, by id).
const TRACK_NAMES: u32 = 214;
/// The race type (`frontEnd + 4`): a single race, a tournament's.
const RACE_TYPE: u32 = FRONT_END + 4;
const SINGLE_RACE: u8 = 0;
const TOURNAMENT: u8 = 1;

/// The descriptions' file ("%s%s" of the front end's path and "fetrk.trk"), the memory they
/// go in ("Track List"); `loadfileadr`, `reservememadr`, `blockmove`, `purgememadr`.
const PATH_FORMAT: u32 = 0x8001_0FC0;
const FILE_NAME: u32 = 0x8001_0FC8;
const MEMORY_NAME: u32 = 0x8001_0FD4;
const FRONT_END_PATH: u32 = PATHS + 0x94;
const LOAD_FILE: fn(&mut Cpu) = nfs4rc::f_800e63d4;
const RESERVE: fn(&mut Cpu) = nfs4rc::f_800e5f28;
const BLOCKMOVE: fn(&mut Cpu) = nfs4rc::f_800e6ec8;
const PURGE: fn(&mut Cpu) = nfs4rc::f_800e612c;

fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

/// The description `k` (from 0).
fn description(m: &mut Mem, this: u32, k: u32) -> u32 {
    m.r32(this + DESCRIPTIONS).wrapping_add(k.wrapping_mul(DESCRIPTION_SIZE))
}

/// The word in table `table` (`AVAILABLE` or `UNLOCKED`) for the track of description `d`.
fn by_id(m: &mut Mem, this: u32, table: u32, d: u32) -> u32 {
    let id = m.r8(d + ID) as i8 as i32 as u32;
    this.wrapping_add(id << 2).wrapping_add(table)
}

/// `tTrackManager::Initialize` (FETRACKS.CPP, 0x80021b70): no descriptions, no track available
/// or unlocked.
pub fn initialize(m: &mut Mem, this: u32) {
    m.w32(this + COUNT, 0);
    m.w32(this + DESCRIPTIONS, 0);
    for k in 0..TRACKS {
        m.w32(this + AVAILABLE + 4 * k, 0);
        m.w32(this + UNLOCKED + 4 * k, 0);
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(&mut c.m, c.r[4]);
}

/// `tTrackManager::LoadTracks` (FETRACKS.CPP, 0x80021bb0): which tracks are available, from a
/// saved game's bytes.
pub fn load_tracks(m: &mut Mem, this: u32, save: u32) {
    for k in 0..TRACKS {
        let v = (m.r8(save + k) != 0) as u32;
        m.w32(this + AVAILABLE + 4 * k, v);
    }
}

pub fn abi_load_tracks(c: &mut Cpu) {
    load_tracks(&mut c.m, c.r[4], c.r[5]);
}

/// `tTrackManager::SaveTracks` (FETRACKS.CPP, 0x80021bdc): which tracks are available, into a
/// saved game's bytes.
pub fn save_tracks(m: &mut Mem, this: u32, save: u32) {
    for k in 0..TRACKS {
        let v = m.r8(this + AVAILABLE + 4 * k);
        m.w8(save + k, v);
    }
}

pub fn abi_save_tracks(c: &mut Cpu) {
    save_tracks(&mut c.m, c.r[4], c.r[5]);
}

/// `tTrackManager::GetTrack` (FETRACKS.CPP, 0x80021c04): description `k` (a `short`) copied to
/// `out`, marked available as its track is now.
pub fn get_track(c: &mut Cpu, this: u32, k: u32, out: u32) {
    let from = description(&mut c.m, this, sx(k));
    call(c, BLOCKMOVE, &[from, out, DESCRIPTION_SIZE]);
    let m = &mut c.m;
    let at = by_id(m, this, AVAILABLE, out);
    let v = m.r8(at);
    m.w8(out + AVAILABLE_NOW, v);
}

pub fn abi_get_track(c: &mut Cpu) {
    get_track(c, c.r[4], c.r[5], c.r[6]);
}

/// `tTrackManager::LoadDescription` (FETRACKS.CPP, 0x80021c74): the descriptions read from
/// fetrk.trk (their count, then them) into memory of their own, those before released; the
/// tracks available from the start made available, the bonus ones' unlocked flags set for the
/// others.
pub fn load_description(c: &mut Cpu, this: u32) {
    let file = with_buffer(c, 96, |c, name| {
        let path = c.m.r32(FRONT_END_PATH);
        call(c, SPRINTF, &[name, PATH_FORMAT, path, FILE_NAME]);
        release_description(c, this);
        call(c, LOAD_FILE, &[name, 16])
    });
    let n = c.m.r32(file);
    c.m.w32(this + COUNT, n);
    let memory = call(c, RESERVE, &[MEMORY_NAME, n.wrapping_mul(DESCRIPTION_SIZE), 0]);
    c.m.w32(this + DESCRIPTIONS, memory);
    let n = c.m.r32(this + COUNT);
    call(c, BLOCKMOVE, &[file + 4, memory, n.wrapping_mul(DESCRIPTION_SIZE)]);
    let m = &mut c.m;
    let mut k = 0u32;
    while k < m.r32(this + COUNT) {
        let d = description(m, this, k);
        if m.r8(d + AVAILABLE_NOW) != 0 {
            let at = by_id(m, this, AVAILABLE, d);
            m.w32(at, 1);
        }
        let d = description(m, this, k);
        if m.r8(d + BONUS) == 0 {
            let at = by_id(m, this, UNLOCKED, d);
            m.w32(at, 1);
        }
        k += 1;
    }
    call(c, PURGE, &[file]);
}

pub fn abi_load_description(c: &mut Cpu) {
    load_description(c, c.r[4]);
}

/// `tTrackManager::ReleaseDescription` (FETRACKS.CPP, 0x80021dac): the descriptions' memory
/// freed, none left.
pub fn release_description(c: &mut Cpu, this: u32) {
    let memory = c.m.r32(this + DESCRIPTIONS);
    if memory != 0 {
        call(c, PURGE, &[memory]);
    }
    c.m.w32(this + DESCRIPTIONS, 0);
    c.m.w32(this + COUNT, 0);
}

pub fn abi_release_description(c: &mut Cpu) {
    release_description(c, c.r[4]);
}

/// `tTrackManager::SetTrackAvailable` (FETRACKS.CPP, 0x80021dec): track `id` available or not.
pub fn set_track_available(m: &mut Mem, this: u32, id: u32, on: u32) {
    m.w32(this.wrapping_add(sx(id) << 2).wrapping_add(AVAILABLE), on);
}

pub fn abi_set_track_available(c: &mut Cpu) {
    set_track_available(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tTrackManager::SetClassAvailable` (FETRACKS.CPP, 0x80021e00): the tracks of a class
/// available or not.
pub fn set_class_available(m: &mut Mem, this: u32, class: u32, on: u32) {
    let mut k = 0u32;
    while k < m.r32(this + COUNT) {
        let d = description(m, this, k);
        if m.r8(d + CLASS) as u32 == class {
            let at = by_id(m, this, AVAILABLE, d);
            m.w32(at, on);
        }
        k += 1;
    }
}

pub fn abi_set_class_available(c: &mut Cpu) {
    set_class_available(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tTrackManager::GetTrackByID` (FETRACKS.CPP, 0x80021e60): the description of track `id` (the
/// first when there is none).
pub fn get_track_by_id(m: &mut Mem, this: u32, id: u32) -> u32 {
    let n = m.r32(this + COUNT);
    let mut d = m.r32(this + DESCRIPTIONS);
    for _ in 0..n {
        if m.r8(d + ID) as i8 as i32 as u32 == sx(id) {
            return d;
        }
        d = d.wrapping_add(DESCRIPTION_SIZE);
    }
    m.r32(this + DESCRIPTIONS)
}

pub fn abi_get_track_by_id(c: &mut Cpu) {
    c.r[2] = get_track_by_id(&mut c.m, c.r[4], c.r[5]);
}

/// `tListIteratorTrack::tListIteratorTrack` (FETRACKS.CPP, 0x80021eb4): an indexed iterator
/// over `values` (the player's at `*index`), the tracks of `manager`.
pub fn list_iterator_track(c: &mut Cpu, this: u32, values: u32, index: u32, manager: u32) -> u32 {
    femenu::indexed(c, ListIteratorIndexed(this), 0, values, index);
    c.m.w32(this + VPTR, TRACK_ITERATOR_VT);
    c.m.w32(this + MANAGER, manager);
    this
}

pub fn abi_list_iterator_track(c: &mut Cpu) {
    c.r[2] = list_iterator_track(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tListIteratorTrack::~tListIteratorTrack` (FETRACKS.CPP, 0x80021f0c): the indexed
/// iterator's.
pub fn list_iterator_track_dtor(c: &mut Cpu, this: u32, flags: u32) {
    c.m.w32(this + VPTR, TRACK_ITERATOR_VT);
    femenu::indexed_dtor(c, this, flags);
}

pub fn abi_list_iterator_track_dtor(c: &mut Cpu) {
    list_iterator_track_dtor(c, c.r[4], c.r[5]);
}

/// The track in the iterator's value now (`fValue[*fIndex]`'s place).
fn value_at(m: &mut Mem, this: u32) -> u32 {
    let index = m.r32(this + INDEX);
    let k = m.r8(index) as u32;
    m.r32(this + VALUES).wrapping_add(k)
}

/// `tListIteratorTrack::TextValue` (FETRACKS.CPP, 0x80021f34): the track's name.
pub fn list_iterator_track_text_value(m: &mut Mem, this: u32) -> u32 {
    let at = value_at(m, this);
    let k = m.r8(at) as u32;
    let manager = m.r32(this + MANAGER);
    let d = description(m, manager, k);
    (m.r8(d + ID) as i8 as i32 as u32).wrapping_add(TRACK_NAMES)
}

pub fn abi_list_iterator_track_text_value(c: &mut Cpu) {
    c.r[2] = list_iterator_track_text_value(&mut c.m, c.r[4]);
}

/// `tListIteratorTrack::Increment` (FETRACKS.CPP, 0x80021f74): the next track (round to the
/// first) until one the race type can have.
pub fn list_iterator_track_increment(m: &mut Mem, this: u32) {
    loop {
        let at = value_at(m, this);
        let v = m.r8(at).wrapping_add(1);
        m.w8(at, v);
        let at = value_at(m, this);
        let manager = m.r32(this + MANAGER);
        if m.r8(at) as u32 >= m.r32(manager + COUNT) {
            m.w8(at, 0);
        }
        let at = value_at(m, this);
        let k = m.r8(at) as u32;
        if valid_track(m, this, k) == 1 {
            return;
        }
    }
}

pub fn abi_list_iterator_track_increment(c: &mut Cpu) {
    list_iterator_track_increment(&mut c.m, c.r[4]);
}

/// `tListIteratorTrack::Decrement` (FETRACKS.CPP, 0x80022020): the track before (round to the
/// last) until one the race type can have.
pub fn list_iterator_track_decrement(m: &mut Mem, this: u32) {
    loop {
        let at = value_at(m, this);
        let v = m.r8(at);
        let v = if v == 0 {
            let manager = m.r32(this + MANAGER);
            m.r8(manager + COUNT).wrapping_sub(1)
        } else {
            v - 1
        };
        m.w8(at, v);
        let at = value_at(m, this);
        let k = m.r8(at) as u32;
        if valid_track(m, this, k) == 1 {
            return;
        }
    }
}

pub fn abi_list_iterator_track_decrement(c: &mut Cpu) {
    list_iterator_track_decrement(&mut c.m, c.r[4]);
}

/// `tListIteratorTrack::ValidTrack` (FETRACKS.CPP, 0x800220b0): whether track `k` can be
/// raced: a single race's, available or unlocked; a tournament's, those but a bonus track,
/// in the first three classes; otherwise, its available word as it is.
pub fn valid_track(m: &mut Mem, this: u32, k: u32) -> u32 {
    let manager = m.r32(this + MANAGER);
    let d = description(m, manager, k & 0xFF);
    let at = by_id(m, manager, 0, d);
    let available = m.r32(at + AVAILABLE);
    match m.r8(RACE_TYPE) {
        SINGLE_RACE => ((available | m.r32(at + UNLOCKED)) != 0) as u32,
        TOURNAMENT => {
            let open = (available | m.r32(at + UNLOCKED)) != 0;
            let bonus = m.r8(d + BONUS) != 0;
            let class_ok = m.r8(d + CLASS) < TOURNAMENT_CLASSES;
            (open && !bonus && class_ok) as u32
        }
        _ => available,
    }
}

pub fn abi_valid_track(c: &mut Cpu) {
    c.r[2] = valid_track(&mut c.m, c.r[4], c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, NEW, pick, player, via_abi};
    use crate::globals::TRACK_MANAGER;
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A copy of the game's track manager (over its real descriptions); a saved game's bytes, a
    /// description's room; a track iterator, its values (a track per player) and its index.
    const MANAGER_COPY: u32 = 0x8014_7000;
    const SAVE: u32 = 0x8014_7100;
    const OUT: u32 = 0x8014_7200;
    const ITERATOR: u32 = 0x8014_7300;
    const ITERATOR_VALUES: u32 = 0x8014_7340;
    const ITERATOR_INDEX: u32 = 0x8014_7350;

    /// FEMENU's structures, then the manager copied, its tracks' flags (available, unlocked,
    /// from the start, bonus) and classes shuffled, one track always one the race type can
    /// have (else the iterator's walk goes round for ever); the iterator's values; the race
    /// type; a saved game's bytes.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        for k in 0..(UNLOCKED + 4 * TRACKS) {
            let b = m.r8(TRACK_MANAGER + k);
            m.w8(MANAGER_COPY + k, b);
        }
        let n = m.r32(MANAGER_COPY + COUNT);
        for k in 0..TRACKS {
            m.w32(MANAGER_COPY + AVAILABLE + 4 * k, [0, 1, 2][pick(rng, 3) as usize]);
            m.w32(MANAGER_COPY + UNLOCKED + 4 * k, pick(rng, 2));
        }
        for k in 0..n {
            let d = description(m, MANAGER_COPY, k);
            m.w8(d + 2, pick(rng, 5) as u8);
            m.w8(d + 3, pick(rng, 2) as u8);
            m.w8(d + 4, pick(rng, 2) as u8);
        }
        let race_type = [0, 1, 2][pick(rng, 3) as usize];
        m.w8(RACE_TYPE, race_type);
        // (now and then fewer tracks than descriptions: those past the last are real ones, as
        // a walk past the end reads)
        let n = if pick(rng, 3) == 0 { 1 + pick(rng, n) } else { n };
        m.w32(MANAGER_COPY + COUNT, n);
        let k = pick(rng, n);
        let d = description(m, MANAGER_COPY, k);
        m.w8(d + 2, 0);
        m.w8(d + 4, 0);
        let at = by_id(m, MANAGER_COPY, AVAILABLE, d);
        m.w32(at, 1);
        m.w32(ITERATOR, 0);
        m.w32(ITERATOR + VALUES, ITERATOR_VALUES);
        m.w32(ITERATOR + 12, TRACK_ITERATOR_VT);
        m.w32(ITERATOR + INDEX, ITERATOR_INDEX);
        m.w32(ITERATOR + MANAGER, MANAGER_COPY);
        for p in 0..2 {
            m.w8(ITERATOR_VALUES + p, pick(rng, n) as u8);
        }
        m.w8(ITERATOR_INDEX, pick(rng, 2) as u8);
        for k in 0..TRACKS {
            m.w8(SAVE + k, [0, 1, pick(rng, 256)][pick(rng, 3) as usize] as u8);
        }
    }

    fn track(c: &mut Cpu, r: &mut Rng) -> u32 {
        let n = c.m.r32(MANAGER_COPY + COUNT);
        pick(r, n)
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Initialize", 0x8002_1b70, nfs4rc::f_80021b70, |_, _| vec![MANAGER_COPY]),
            ("LoadTracks", 0x8002_1bb0, nfs4rc::f_80021bb0, |_, _| vec![MANAGER_COPY, SAVE]),
            ("SaveTracks", 0x8002_1bdc, nfs4rc::f_80021bdc, |_, _| vec![MANAGER_COPY, SAVE]),
            ("GetTrack", 0x8002_1c04, nfs4rc::f_80021c04, |c, r| vec![MANAGER_COPY, track(c, r), OUT]),
            ("ReleaseDescription", 0x8002_1dac, nfs4rc::f_80021dac, |_, _| vec![MANAGER_COPY]),
            ("SetTrackAvailable", 0x8002_1dec, nfs4rc::f_80021dec, |_, r| vec![MANAGER_COPY, pick(r, TRACKS), pick(r, 3)]),
            ("SetClassAvailable", 0x8002_1e00, nfs4rc::f_80021e00, |_, r| vec![MANAGER_COPY, pick(r, 6), pick(r, 3)]),
            ("GetTrackByID", 0x8002_1e60, nfs4rc::f_80021e60, |_, r| vec![MANAGER_COPY, pick(r, 18).wrapping_sub(1)]),
            ("tListIteratorTrack", 0x8002_1eb4, nfs4rc::f_80021eb4, |_, _| vec![NEW, ITERATOR_VALUES, ITERATOR_INDEX, MANAGER_COPY]),
            ("~tListIteratorTrack", 0x8002_1f0c, nfs4rc::f_80021f0c, |_, r| vec![ITERATOR, 2 * pick(r, 2)]),
            ("TextValue", 0x8002_1f34, nfs4rc::f_80021f34, |_, r| vec![ITERATOR, player(r)]),
            ("Increment", 0x8002_1f74, nfs4rc::f_80021f74, |_, r| vec![ITERATOR, player(r)]),
            ("Decrement", 0x8002_2020, nfs4rc::f_80022020, |_, r| vec![ITERATOR, player(r)]),
            ("ValidTrack", 0x8002_20b0, nfs4rc::f_800220b0, |c, r| vec![ITERATOR, track(c, r) | (pick(r, 2) << 8)]),
        ]
    }

    #[test]
    fn fetracks_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x7A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0x7A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
