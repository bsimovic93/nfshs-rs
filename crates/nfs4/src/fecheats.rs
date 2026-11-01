//! FECHEATS.CPP: the cheats typed as the user's name and the bonus codes: a name encoded (its
//! eight letters' bits transposed; the bonus codes' inverted and their bits swapped about) and
//! compared with the cheats' and the codes'; what each unlocks (car classes, three cars), the
//! cheats and bonuses on, saved and loaded with the game.

use crate::abi::{self, Saved};
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
/// The bonuses saved (15 of them).
const SAVED_BONUSES: u32 = 15;

/// What each cheat or bonus unlocks (a jump table: a class of cars, or one of three cars).
const ACTIVATIONS: u32 = 0x8001_1020;
const ACTIVATION_COUNT: u32 = 11;
const UNLOCK_CLASS: u32 = 0x8002_33D4;
const UNLOCK_CARS: [(u32, u32); 3] = [(0x8002_33EC, 19), (0x8002_33FC, 20), (0x8002_340C, 28)];
/// `tCarManager::SetClassAvailable`, `SetCarAvailable` of `carManager`.
const CAR_MANAGER: u32 = 0x8011_552C;
const SET_CLASS_AVAILABLE: fn(&mut Cpu) = nfs4rc::f_80016eb0;
const SET_CAR_AVAILABLE: fn(&mut Cpu) = nfs4rc::f_80016f34;

/// The cheat's sound; its message ("cheat activated") in the front end's message dialog
/// (`gFEApp + 568`).
const CHEAT_SOUND: u32 = 26;
const CHEAT_TEXT: u32 = 635;
const APP_MESSAGE: u32 = 568;
const MESSAGE_TEXT: u32 = 144;
const DISPLAY: u32 = 0x8001_8980;

/// `FECheat_HandleActivation` (FECHEATS.CPP, 0x800233a0): what cheat or bonus `code` unlocks
/// made available.
pub fn handle_activation(c: &mut Cpu, code: u32) {
    abi::frame(c, 24, |c, _| {
        if code >= ACTIVATION_COUNT {
            return;
        }
        let target = c.m.r32(ACTIVATIONS + 4 * code);
        let r = Saved::of(c);
        if target == UNLOCK_CLASS {
            r.call_leaving(c, SET_CLASS_AVAILABLE, &[CAR_MANAGER, code, 1], 0x8002_33e4);
        } else if let Some(&(_, car)) = UNLOCK_CARS.iter().find(|&&(t, _)| t == target) {
            r.call_leaving(c, SET_CAR_AVAILABLE, &[CAR_MANAGER, car, 1], 0x8002_3420);
        }
    })
}

pub fn abi_handle_activation(c: &mut Cpu) {
    handle_activation(c, c.r[4]);
}

/// `FECheat_EncodeString` (FECHEATS.CPP, 0x80023430): a name's eight letters' bits transposed
/// (bit `j` of letter `i` becoming bit `i` of byte `j`), through a copy on the stack.
pub fn encode_string(c: &mut Cpu, from: u32, to: u32) {
    abi::frame(c, 8, |c, sp| {
        let m = &mut c.m;
        for k in 0..8 {
            let b = m.r8(from + k);
            m.w8(sp + k, b);
            m.w8(to + k, 0);
        }
        for j in 0..8 {
            for i in 0..8 {
                let (b, d) = (m.r8(sp + i) as u32, m.r8(to + j) as u32);
                m.w8(to + j, (d | (b & 1) << i) as u8);
                m.w8(sp + i, (b >> 1) as u8);
            }
        }
    })
}

pub fn abi_encode_string(c: &mut Cpu) {
    encode_string(c, c.r[4], c.r[5]);
}

/// `FECheat_EncodeString2` (FECHEATS.CPP, 0x800234bc): a code's eight letters inverted, then
/// the first byte's bits swapped with the diagonal's, and each other byte's bit 0 with its
/// bit before the diagonal (from the inverted copy on the stack).
pub fn encode_string2(c: &mut Cpu, from: u32, to: u32) {
    abi::frame(c, 8, |c, sp| {
        let m = &mut c.m;
        for k in 0..8 {
            let b = !m.r8(from + k);
            m.w8(sp + k, b);
            let b = !m.r8(from + k);
            m.w8(to + k, b);
        }
        let bit = |v: u32, k: u32| ((v >> k) & 1) << k;
        for k in 0..8 {
            let first = m.r8(to) as u32;
            m.w8(to, (first ^ bit(first, k)) as u8);
            let v = m.r8(to + k) as u32;
            m.w8(to + k, (v ^ bit(v, k)) as u8);
            let (s, first) = (m.r8(sp + k) as u32, m.r8(to) as u32);
            m.w8(to, (first | bit(s, k)) as u8);
            let (s0, v) = (m.r8(sp) as u32, m.r8(to + k) as u32);
            m.w8(to + k, (v | bit(s0, k)) as u8);
        }
        for k in 1..8 {
            let b = k - 1;
            let v = m.r8(to + k) as u32 & 0xFE;
            m.w8(to + k, v as u8);
            let v = v ^ bit(v, b);
            m.w8(to + k, v as u8);
            let s = m.r8(sp + k) as u32;
            let v = v | ((s >> b) & 1);
            m.w8(to + k, v as u8);
            let s = m.r8(sp + k) as u32;
            m.w8(to + k, (v | (s & 1) << b) as u8);
        }
    })
}

pub fn abi_encode_string2(c: &mut Cpu) {
    encode_string2(c, c.r[4], c.r[5]);
}

/// `FECheat_ActivateBonus` (FECHEATS.CPP, 0x800235f8): bonus `code` unlocked.
pub fn activate_bonus(c: &mut Cpu, code: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = code;
        r.enter(c, 0x8002_33a0, 0x8002_360c, |c| handle_activation(c, code));
        let v = c.m.r32(BONUSES) | 1u32.wrapping_shl(code);
        c.m.w32(BONUSES, v);
    })
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
pub fn activate_cheat(c: &mut Cpu, name: u32) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = 0;
        let code = sp + 16;
        r.enter(c, 0x8002_3430, 0x8002_3654, |c| encode_string(c, name, code));
        let Some(k) = find(&mut c.m, CHEATS, CHEAT_COUNT, code) else { return 0 };
        r.0[2] = k;
        r.call_leaving(c, PLAY_FE_SFX, &[CHEAT_SOUND], 0x8002_36c0);
        r.0[1] = 0x8005_0000;
        let dialog = c.m.r32(FE_APP).wrapping_add(APP_MESSAGE);
        r.0[0] = dialog;
        let word = r.call_leaving(c, TEXT_WORD, &[CHEAT_TEXT], 0x8002_36d4);
        let dialog2 = c.m.r32(FE_APP).wrapping_add(APP_MESSAGE);
        c.m.w32(dialog + MESSAGE_TEXT, word);
        r.call_ptr(c, DISPLAY, &[dialog2], 0x8002_36e4);
        let entry = CHEATS + ENTRY_SIZE * k;
        r.0[0] = entry;
        r.0[1] = 1;
        let cheat = c.m.r32(entry + 8);
        r.enter(c, 0x8002_33a0, 0x8002_3708, |c| handle_activation(c, cheat));
        let cheat = c.m.r32(entry + 8);
        let v = c.m.r32(CHEATS_ON) | 1u32.wrapping_shl(cheat);
        c.m.w32(CHEATS_ON, v);
        1
    })
}

pub fn abi_activate_cheat(c: &mut Cpu) {
    c.r[2] = activate_cheat(c, c.r[4]);
}

/// `FECheat_IsCheatEnabled` (FECHEATS.CPP, 0x8002373c): whether cheat or bonus `code` is on.
pub fn abi_is_cheat_enabled(c: &mut Cpu) {
    let (m, code) = (&mut c.m, c.r[4]);
    c.r[2] = ((m.r32(CHEATS_ON) | m.r32(BONUSES)) & 1u32.wrapping_shl(code) != 0) as u32;
}

/// `FECheat_SaveBonus` (FECHEATS.CPP, 0x80023768): the bonuses, for a saved game.
pub fn abi_save_bonus(c: &mut Cpu) {
    let v = c.m.r32(BONUSES);
    c.m.w32(c.r[4], v);
}

/// `FECheat_LoadBonus` (FECHEATS.CPP, 0x80023778): the bonuses from a saved game (what each
/// unlocks made available again), no cheats on.
pub fn abi_load_bonus(c: &mut Cpu) {
    let from = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[0, 0x8014_0000, 1]);
        let v = c.m.r32(from);
        c.m.w32(CHEATS_ON, 0);
        c.m.w32(BONUSES, v);
        for k in 0..SAVED_BONUSES {
            r.0[0] = k;
            if c.m.r32(BONUSES) & 1 << k != 0 {
                r.enter(c, 0x8002_33a0, 0x8002_37c8, |c| handle_activation(c, k));
            }
        }
    })
}

/// `FECheat_IsTheUserACryBabyCheater` (FECHEATS.CPP, 0x800237f0): whether any cheat is on.
pub fn abi_is_the_user_a_cry_baby_cheater(c: &mut Cpu) {
    c.r[2] = (c.m.r32(CHEATS_ON) != 0) as u32;
}

/// `FECheat_ActivateBonusByCode` (FECHEATS.CPP, 0x80023800): whether `name` is a bonus code;
/// if so its sound and the bonus unlocked.
pub fn activate_bonus_by_code(c: &mut Cpu, name: u32) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = 0;
        let code = sp + 16;
        r.enter(c, 0x8002_34bc, 0x8002_381c, |c| encode_string2(c, name, code));
        r.0[0] = BONUS_CODES;
        let Some(k) = find(&mut c.m, BONUS_CODES, BONUS_CODE_COUNT, code) else { return 0 };
        let entry = BONUS_CODES + ENTRY_SIZE * k;
        r.0[0] = entry;
        r.call_leaving(c, PLAY_FE_SFX, &[CHEAT_SOUND], 0x8002_387c);
        r.0[1] = 1;
        let bonus = c.m.r32(entry + 8);
        r.enter(c, 0x8002_35f8, 0x8002_3888, |c| activate_bonus(c, bonus));
        1
    })
}

pub fn abi_activate_bonus_by_code(c: &mut Cpu) {
    c.r[2] = activate_bonus_by_code(c, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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
        abi::call(c, encode, &[NAME, OUT]);
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

    #[test]
    fn fecheats_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xC4F1 + k as u64, shuffle, &args);
        }
    }
}
