//! FECHEATS.CPP: the cheats typed as the user's name and the bonus codes: a name encoded (its
//! eight letters' bits transposed; the bonus codes' inverted and their bits swapped about) and
//! compared with the cheats' and the codes'; what each unlocks (car classes, three cars), the
//! cheats and bonuses on, saved and loaded with the game.
//!
//! Written as FEMENU.CPP is (see there). The encodings work on a copy and write the eight
//! bytes once (the original builds them in place, which only differs for overlapping buffers,
//! which the game never passes).

use crate::call::{call, call_addr, with_buffer};
use crate::femenu::PLAY_FE_SFX;
use crate::femenuextended::TEXT_WORD;
use crate::globals::FE_APP;
use ps1rt::{Cpu, Mem};

/// The cheats on and the bonuses unlocked (a bit each).
const CHEATS_ON: u32 = 0x8013_E550;
const BONUSES: u32 = 0x8013_E554;
/// The cheats' encoded names (12 bytes each: eight, then the cheat) and the bonus codes'.
const CHEATS: u32 = 0x8005_1B10;
const CHEAT_COUNT: u32 = 3;
const BONUS_CODES: u32 = 0x8005_1B34;
const BONUS_CODE_COUNT: u32 = 2;
const ENTRY_SIZE: u32 = 12;
const ENTRY_CODE: u32 = 8;
/// The bonuses saved (15 of them).
const SAVED_BONUSES: u32 = 15;

/// What each cheat or bonus unlocks (the original's jump table: a class of cars, or one of
/// three cars; nothing for the others).
const ACTIVATIONS: u32 = 0x8001_1020;
const ACTIVATION_COUNT: u32 = 11;
const UNLOCK_CLASS: u32 = 0x8002_33D4;
const UNLOCK_CARS: [(u32, u32); 3] = [(0x8002_33EC, 19), (0x8002_33FC, 20), (0x8002_340C, 28)];
/// `tCarManager::SetClassAvailable`, `SetCarAvailable` of `carManager`.
const CAR_MANAGER: u32 = 0x8011_552C;
const SET_CLASS_AVAILABLE: u32 = 0x8001_6eb0;
const SET_CAR_AVAILABLE: u32 = 0x8001_6f34;

/// The cheat's sound; its message ("cheat activated") in the front end's message dialog
/// (`gFEApp + 568`: its text at +144; `tDialog::Display`, called by address: hooked).
const CHEAT_SOUND: u32 = 26;
const CHEAT_TEXT: u32 = 635;
const APP_MESSAGE: u32 = 568;
const MESSAGE_TEXT: u32 = 144;
const DISPLAY: u32 = 0x8001_8980;

/// `FECheat_HandleActivation` (FECHEATS.CPP, 0x800233a0): what cheat or bonus `code` unlocks
/// made available.
pub fn handle_activation(c: &mut Cpu, code: u32) {
    if code >= ACTIVATION_COUNT {
        return;
    }
    let target = c.m.r32(ACTIVATIONS + 4 * code);
    if target == UNLOCK_CLASS {
        call_addr(c, SET_CLASS_AVAILABLE, &[CAR_MANAGER, code, 1]);
    } else if let Some(&(_, car)) = UNLOCK_CARS.iter().find(|&&(t, _)| t == target) {
        call_addr(c, SET_CAR_AVAILABLE, &[CAR_MANAGER, car, 1]);
    }
}

pub fn abi_handle_activation(c: &mut Cpu) {
    handle_activation(c, c.r[4]);
}

fn read8(m: &mut Mem, at: u32) -> [u8; 8] {
    std::array::from_fn(|k| m.r8(at + k as u32))
}

fn write8(m: &mut Mem, at: u32, v: [u8; 8]) {
    for (k, b) in v.into_iter().enumerate() {
        m.w8(at + k as u32, b);
    }
}

/// `FECheat_EncodeString` (FECHEATS.CPP, 0x80023430): a name's eight letters' bits transposed
/// (bit `j` of letter `i` becoming bit `i` of byte `j`).
pub fn encode_string(m: &mut Mem, from: u32, to: u32) {
    let s = read8(m, from);
    let out = std::array::from_fn(|j| (0..8).fold(0u8, |d, i| d | ((s[i] >> j) & 1) << i));
    write8(m, to, out);
}

pub fn abi_encode_string(c: &mut Cpu) {
    encode_string(&mut c.m, c.r[4], c.r[5]);
}

/// `FECheat_EncodeString2` (FECHEATS.CPP, 0x800234bc): a code's eight letters inverted; then
/// for each `k`, bit `k` of the first byte and of byte `k` take each other's (the inverted
/// letters'); then each other byte's bit 0 and bit `k - 1` swapped (again from the inverted
/// letters).
pub fn encode_string2(m: &mut Mem, from: u32, to: u32) {
    let s = read8(m, from).map(|b| !b);
    let mut t = s;
    let bit = |v: u8, k: usize| v & (1 << k);
    for k in 0..8 {
        t[0] &= !(1 << k);
        t[k] &= !(1 << k);
        t[0] |= bit(s[k], k);
        t[k] |= bit(s[0], k);
    }
    for k in 1..8 {
        let b = k - 1;
        let v = (t[k] & 0xFE) & !(1 << b);
        t[k] = v | ((s[k] >> b) & 1) | (s[k] & 1) << b;
    }
    write8(m, to, t);
}

pub fn abi_encode_string2(c: &mut Cpu) {
    encode_string2(&mut c.m, c.r[4], c.r[5]);
}

/// `FECheat_ActivateBonus` (FECHEATS.CPP, 0x800235f8): bonus `code` unlocked.
pub fn activate_bonus(c: &mut Cpu, code: u32) {
    handle_activation(c, code);
    let v = c.m.r32(BONUSES) | 1u32.wrapping_shl(code);
    c.m.w32(BONUSES, v);
}

pub fn abi_activate_bonus(c: &mut Cpu) {
    activate_bonus(c, c.r[4]);
}

/// The entry of `table` (`count` of them) whose eight bytes the encoded name at `code` is.
fn find(m: &mut Mem, table: u32, count: u32, code: u32) -> Option<u32> {
    (0..count).find(|&k| (0..8).all(|i| m.r8(table + ENTRY_SIZE * k + i) == m.r8(code + i)))
}

/// `FECheat_ActivateCheat` (FECHEATS.CPP, 0x80023634): whether `name` is a cheat's; if so its
/// sound, its message shown and it on.
pub fn activate_cheat(c: &mut Cpu, name: u32) -> bool {
    let found = with_buffer(c, 8, |c, code| {
        encode_string(&mut c.m, name, code);
        find(&mut c.m, CHEATS, CHEAT_COUNT, code)
    });
    let Some(k) = found else { return false };
    call(c, PLAY_FE_SFX, &[CHEAT_SOUND]);
    let dialog = c.m.r32(FE_APP).wrapping_add(APP_MESSAGE);
    let word = call(c, TEXT_WORD, &[CHEAT_TEXT]);
    let shown = c.m.r32(FE_APP).wrapping_add(APP_MESSAGE);
    c.m.w32(dialog + MESSAGE_TEXT, word);
    call_addr(c, DISPLAY, &[shown]);
    let entry = CHEATS + ENTRY_SIZE * k;
    let cheat = c.m.r32(entry + ENTRY_CODE);
    handle_activation(c, cheat);
    let cheat = c.m.r32(entry + ENTRY_CODE);
    let v = c.m.r32(CHEATS_ON) | 1u32.wrapping_shl(cheat);
    c.m.w32(CHEATS_ON, v);
    true
}

pub fn abi_activate_cheat(c: &mut Cpu) {
    c.r[2] = activate_cheat(c, c.r[4]) as u32;
}

/// `FECheat_IsCheatEnabled` (FECHEATS.CPP, 0x8002373c): whether cheat or bonus `code` is on.
pub fn is_cheat_enabled(m: &mut Mem, code: u32) -> bool {
    (m.r32(CHEATS_ON) | m.r32(BONUSES)) & 1u32.wrapping_shl(code) != 0
}

pub fn abi_is_cheat_enabled(c: &mut Cpu) {
    c.r[2] = is_cheat_enabled(&mut c.m, c.r[4]) as u32;
}

/// `FECheat_SaveBonus` (FECHEATS.CPP, 0x80023768): the bonuses, for a saved game.
pub fn save_bonus(m: &mut Mem, to: u32) {
    let v = m.r32(BONUSES);
    m.w32(to, v);
}

pub fn abi_save_bonus(c: &mut Cpu) {
    save_bonus(&mut c.m, c.r[4]);
}

/// `FECheat_LoadBonus` (FECHEATS.CPP, 0x80023778): the bonuses from a saved game (what each
/// unlocks made available again), no cheats on.
pub fn load_bonus(c: &mut Cpu, from: u32) {
    let v = c.m.r32(from);
    c.m.w32(CHEATS_ON, 0);
    c.m.w32(BONUSES, v);
    for k in 0..SAVED_BONUSES {
        if c.m.r32(BONUSES) & 1 << k != 0 {
            handle_activation(c, k);
        }
    }
}

pub fn abi_load_bonus(c: &mut Cpu) {
    load_bonus(c, c.r[4]);
}

/// `FECheat_IsTheUserACryBabyCheater` (FECHEATS.CPP, 0x800237f0): whether any cheat is on.
pub fn is_the_user_a_cry_baby_cheater(m: &mut Mem) -> bool {
    m.r32(CHEATS_ON) != 0
}

pub fn abi_is_the_user_a_cry_baby_cheater(c: &mut Cpu) {
    c.r[2] = is_the_user_a_cry_baby_cheater(&mut c.m) as u32;
}

/// `FECheat_ActivateBonusByCode` (FECHEATS.CPP, 0x80023800): whether `name` is a bonus code;
/// if so its sound and the bonus unlocked.
pub fn activate_bonus_by_code(c: &mut Cpu, name: u32) -> bool {
    let found = with_buffer(c, 8, |c, code| {
        encode_string2(&mut c.m, name, code);
        find(&mut c.m, BONUS_CODES, BONUS_CODE_COUNT, code)
    });
    let Some(k) = found else { return false };
    call(c, PLAY_FE_SFX, &[CHEAT_SOUND]);
    let bonus = c.m.r32(BONUS_CODES + ENTRY_SIZE * k + ENTRY_CODE);
    activate_bonus(c, bonus);
    true
}

pub fn abi_activate_bonus_by_code(c: &mut Cpu) {
    c.r[2] = activate_bonus_by_code(c, c.r[4]) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// A name typed (a cheat's or a bonus code's now and then), the encoded name's room, a
    /// saved game's bonuses.
    const NAME: u32 = 0x8014_7000;
    const OUT: u32 = 0x8014_7020;
    const SAVED: u32 = 0x8014_7040;

    /// FEMENU's structures, then the cheats on and the bonuses unlocked, a name of eight
    /// letters (made a cheat's or a bonus code's now and then: its encoding, as the original
    /// encodes it, put in their table), the saved bonuses.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        m.w32(CHEATS_ON, [0, 1 << pick(rng, 11), rng.next() as u32 & 0x7FF][pick(rng, 3) as usize]);
        m.w32(BONUSES, [0, 1 << pick(rng, 15), rng.next() as u32 & 0x7FFF][pick(rng, 3) as usize]);
        for k in 0..8 {
            m.w8(NAME + k, [65 + pick(rng, 26), pick(rng, 256)][pick(rng, 2) as usize] as u8);
        }
        m.w8(NAME + 8, 0);
        m.w32(SAVED, rng.next() as u32);
        // (the cars and classes not all available and viewable: what a cheat unlocks shows)
        for k in 0..48 {
            m.w8(CAR_MANAGER + 0x208 + k, pick(rng, 2) as u8);
            m.w8(CAR_MANAGER + 0x238 + k, pick(rng, 2) as u8);
        }
        let (table, count, encode): (u32, u32, fn(&mut Cpu)) = match pick(rng, 3) {
            0 => (CHEATS, CHEAT_COUNT, nfs4rc::f_80023430),
            1 => (BONUS_CODES, BONUS_CODE_COUNT, nfs4rc::f_800234bc),
            _ => return,
        };
        crate::abi::call(c, encode, &[NAME, OUT]);
        let k = pick(rng, count);
        for i in 0..8 {
            let b = c.m.r8(OUT + i);
            c.m.w8(table + ENTRY_SIZE * k + i, b);
        }
        // (now and then a near miss: the last byte not the name's)
        if pick(rng, 3) == 0 {
            let at = table + ENTRY_SIZE * k + 7;
            let b = c.m.r8(at) ^ 1;
            c.m.w8(at, b);
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("HandleActivation", 0x8002_33a0, nfs4rc::f_800233a0, |_, r| vec![pick(r, 13)]),
            ("EncodeString", 0x8002_3430, nfs4rc::f_80023430, |_, _| vec![NAME, OUT]),
            ("EncodeString2", 0x8002_34bc, nfs4rc::f_800234bc, |_, _| vec![NAME, OUT]),
            ("ActivateBonus", 0x8002_35f8, nfs4rc::f_800235f8, |_, r| vec![pick(r, 16)]),
            ("ActivateCheat", 0x8002_3634, nfs4rc::f_80023634, |_, _| vec![NAME]),
            ("IsCheatEnabled", 0x8002_373c, nfs4rc::f_8002373c, |_, r| vec![pick(r, 16)]),
            ("SaveBonus", 0x8002_3768, nfs4rc::f_80023768, |_, _| vec![SAVED]),
            ("LoadBonus", 0x8002_3778, nfs4rc::f_80023778, |_, _| vec![SAVED]),
            ("IsTheUserACryBabyCheater", 0x8002_37f0, nfs4rc::f_800237f0, |_, _| vec![]),
            ("ActivateBonusByCode", 0x8002_3800, nfs4rc::f_80023800, |_, _| vec![NAME]),
        ]
    }

    #[test]
    fn fecheats_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xC4E0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xC4E1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
