//! FEINPUT.CPP: the front end's keys: a pad's buttons (or an analog pad's sticks, a NeGcon's
//! twist and buttons, past their thresholds) as the front end's keys, each key taken once per
//! press but for those that repeat while held (up, down, left, right), a player's key from the
//! table of buttons and keys; a pad swapped for another kind reset.

use crate::abi::{self, Saved};
use crate::globals::{FRONT_END, GAME_TICKS, PAD_INFO};
use ps1rt::{Cpu, Mem};

/// The pads' kinds (their state's +5): the NeGcon, the analog joystick, the DualShock.
const PAD_NEGCON: u32 = 0x23;
const PAD_ANALOG: u32 = 0x53;
const PAD_DUAL_SHOCK: u32 = 0x73;
/// The kind each pad had (`frontEnd + 862`, a short each).
const PAD_KINDS: u32 = FRONT_END + 862;

/// The keys held (a word per pad), when a held key repeats next (0: not yet held), the repeat's
/// delay.
const HELD: u32 = 0x8005_2FF0;
const REPEAT_AT: u32 = 0x8005_1BC0;
const REPEAT_DELAY: u32 = 0x8005_1C58;
/// The keys that repeat while held (the pad's up, down, left, right as the front end has
/// them).
const REPEATING: [u32; 4] = [16, 128, 32, 64];
/// The front end's keys: a pad button (or stick direction) and its key, 16 of them; the
/// buttons that accept, start, help and give options, and the select button (back).
const KEYS: u32 = 0x8005_1BC4;
const KEY_COUNT: u32 = 16;
const BUTTON_KEYS: [(u32, u32, u32); 4] = [(0x4000, 2, 0x8002_3d54), (0x1000, 16, 0x8002_3d68), (0x2000, 4, 0x8002_3d7c), (0x8000, 8, 0x8002_3d90)];

/// `PAD_update`; the pad's buttons (`PAD_buttons`, 0x800e4da4); the pad reset
/// (`Front_ResetPSXController`).
const PAD_UPDATE: fn(&mut Cpu) = nfs4rc::f_800e4e08;
const PAD_BUTTONS: fn(&mut Cpu) = nfs4rc::f_800e4da4;
const RESET_CONTROLLER: fn(&mut Cpu) = crate::front::reset_psx_controller_entry;

/// `FEInput_VerifyControllerValues` (FEINPUT.CPP, 0x800238b8): a pad plugged in as another kind
/// than it was reset.
pub fn verify_controller_values(c: &mut Cpu, pad: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let info = (pad << 5).wrapping_add(PAD_INFO);
        if m.r8(info + 4) != 0 {
            return;
        }
        let was = m.r16(PAD_KINDS.wrapping_add(pad << 1)) as i16 as i32;
        if was != m.r8(info + 5) as i32 {
            Saved::of(c).call_leaving(c, RESET_CONTROLLER, &[pad, 0], 0x8002_3908);
        }
    })
}

pub fn abi_verify_controller_values(c: &mut Cpu) {
    verify_controller_values(c, c.r[4]);
}

/// The stick byte (of a pad's state) and threshold an analog pad's key `key` is, and whether
/// the stick must be below it (else at or over it).
fn analog_key(key: u32) -> Option<(u32, u32, bool)> {
    Some(match key {
        0x0080_0000 => (8, 64, true),
        0x0020_0000 => (8, 193, false),
        0x0010_0000 => (9, 64, true),
        0x0040_0000 => (9, 193, false),
        0x8000_0000 => (10, 64, true),
        0x2000_0000 => (10, 193, false),
        0x1000_0000 => (11, 64, true),
        0x4000_0000 => (11, 193, false),
        _ => return None,
    })
}

/// The same for a NeGcon.
fn negcon_key(key: u32) -> Option<(u32, u32, bool)> {
    Some(match key {
        0x0080_0000 => (8, 98, true),
        0x0020_0000 => (8, 161, false),
        0x4000 => (9, 65, false),
        0x8000 => (10, 65, false),
        _ => return None,
    })
}

/// `FEInput_GetNoDebounceKey` (FEINPUT.CPP, 0x80023918): whether `pad` (plugged in) has `key`
/// down: an analog pad's stick past its threshold, or a button (its state's, held low); a
/// NeGcon's twist or buttons I and II past theirs, or a button (`PAD_buttons`); a digital
/// pad's button.
pub fn get_no_debounce_key(c: &mut Cpu, key: u32, pad: u32) -> u32 {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = key;
        r.0[4] = pad;
        r.call_leaving(c, PAD_UPDATE, &[], 0x8002_3940);
        let info = (pad << 5).wrapping_add(PAD_INFO);
        r.0[3] = PAD_INFO;
        r.0[2] = pad << 5;
        r.0[1] = info;
        if c.m.r8(info + 4) != 0 {
            return 0;
        }
        r.enter(c, 0x8002_38b8, 0x8002_3968, |c| verify_controller_values(c, pad));
        let m = &mut c.m;
        let past = |m: &mut Mem, (at, threshold, below): (u32, u32, bool)| ((m.r8(info + at) < threshold as u8) == below) as u32;
        let kind = m.r8(info + 5) as u32;
        if kind == PAD_DUAL_SHOCK || kind == PAD_ANALOG {
            if let Some(k) = analog_key(key) {
                return past(m, k);
            }
            let buttons = m.r16(info + 6) as u32;
            return (!buttons & key != 0) as u32;
        }
        let ra = if kind == PAD_NEGCON {
            if let Some(k) = negcon_key(key) {
                return past(m, k);
            }
            0x8002_3b40
        } else {
            0x8002_3b60
        };
        let buttons = r.call_leaving(c, PAD_BUTTONS, &[pad << 2], ra);
        (buttons & 0xFFFF & key != 0) as u32
    })
}

pub fn abi_get_no_debounce_key(c: &mut Cpu) {
    c.r[2] = get_no_debounce_key(c, c.r[4], c.r[5]);
}

/// `FEInput_GetDebounceKey` (FEINPUT.CPP, 0x80023b94): whether `key` was just pressed on `pad`
/// (noted held); a held key that repeats again after the delay (longer the first time).
pub fn get_debounce_key(c: &mut Cpu, key: u32, pad: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..2].copy_from_slice(&[key, pad]);
        let down = r.enter(c, 0x8002_3918, 0x8002_3bb0, |c| get_no_debounce_key(c, key, pad));
        let m = &mut c.m;
        let held_at = HELD.wrapping_add(pad << 2);
        let held = m.r32(held_at);
        if down == 0 {
            m.w32(held_at, held & !key);
            return 0;
        }
        if held & key == 0 {
            m.w32(held_at, held | key);
            m.w32(REPEAT_AT, 0);
            return 1;
        }
        let ticks = m.r32(GAME_TICKS);
        if !REPEATING.contains(&key) {
            return 0;
        }
        if m.r32(REPEAT_AT) == 0 {
            let at = ticks.wrapping_add(m.r32(REPEAT_DELAY)).wrapping_add(10);
            m.w32(REPEAT_AT, at);
        }
        if (ticks as i32) < (m.r32(REPEAT_AT) as i32) {
            return 0;
        }
        let at = ticks.wrapping_add(m.r32(REPEAT_DELAY));
        m.w32(REPEAT_AT, at);
        1
    })
}

pub fn abi_get_debounce_key(c: &mut Cpu) {
    c.r[2] = get_debounce_key(c, c.r[4], c.r[5]);
}

/// `FEInput_GetKeyFromPlayer` (FEINPUT.CPP, 0x80023ca8): `player`'s key: the first of the
/// table's buttons just pressed (or held, for those not in `mask`), else accept, start, help,
/// options or back pressed, else none (0).
pub fn get_key_from_player(c: &mut Cpu, player: u32, mask: u32) -> u32 {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = player;
        r.0[5] = mask;
        r.0[1] = 0;
        r.0[4] = KEYS;
        r.0[2] = 0;
        while (r.0[1] & 0xFFFF) < KEY_COUNT {
            let entry = KEYS + ((r.0[1] & 0xFFFF) << 3);
            r.0[0] = entry;
            let button = c.m.r32(entry);
            let mut found = r.enter(c, 0x8002_3b94, 0x8002_3cf8, |c| get_debounce_key(c, button, player)) != 0;
            if !found && mask & c.m.r32(entry + 4) == 0 {
                let button = c.m.r32(entry);
                found = r.enter(c, 0x8002_3918, 0x8002_3d20, |c| get_no_debounce_key(c, button, player)) != 0;
            }
            if found {
                r.0[2] = 1;
                let entry = KEYS + ((r.0[1] & 0xFFFF) << 3);
                return c.m.r32(entry + 4);
            }
            r.0[1] = r.0[1].wrapping_add(1);
            r.0[2] = 0;
        }
        for (button, key, ra) in BUTTON_KEYS {
            if r.enter(c, 0x8002_3b94, ra, |c| get_debounce_key(c, button, player)) != 0 {
                return key;
            }
        }
        let back = r.enter(c, 0x8002_3b94, 0x8002_3da0, |c| get_debounce_key(c, 8, player));
        ((back != 0) as u32) << 13
    })
}

pub fn abi_get_key_from_player(c: &mut Cpu) {
    c.r[2] = get_key_from_player(c, c.r[4], c.r[5]);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The pads' library state (`Padglobal`, 34 bytes a pad: plugged in (0), kind, buttons held
    /// low, the sticks), which `PAD_update` copies into `gPadinfo`.
    const PAD_GLOBAL: u32 = 0x8013_FBF8;

    /// The table's buttons and stick directions, the NeGcon's, the buttons asked for alone.
    fn key(m: &mut Mem, rng: &mut Rng) -> u32 {
        let k = pick(rng, 4);
        match k {
            0 => m.r32(KEYS + 8 * pick(rng, KEY_COUNT)),
            1 => [0x4000, 0x8000, 0x0010_0000, 0x0020_0000, 0x0040_0000, 0x0080_0000, 0x1000_0000, 0x2000_0000, 0x4000_0000, 0x8000_0000, 8][pick(rng, 11) as usize],
            2 => [0x1000, 0x2000, 0x4000, 0x8000, 16, 32, 64, 128][pick(rng, 8) as usize],
            _ => 1 << pick(rng, 32),
        }
    }

    /// FEMENU's structures, then the pads: plugged in or not, their kinds, their buttons and
    /// their sticks about the thresholds; the keys held, the
    /// repeat's time and delay, the clock.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        pads(c, rng);
    }

    /// The pads alone (`shuffle` without FEMENU's structures).
    pub(crate) fn pads(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        for p in 0..2 {
            let g = PAD_GLOBAL + 34 * p;
            m.w8(g, (pick(rng, 4) == 0) as u8);
            let kind = [0x41, PAD_DUAL_SHOCK, PAD_ANALOG, PAD_NEGCON][pick(rng, 4) as usize];
            m.w8(g + 1, kind as u8);
            // (now and then a quiet pad: sticks centred, nothing down but maybe back)
            let quiet = pick(rng, 4) == 0;
            let buttons = if quiet { [0xFFFF, !8][pick(rng, 2) as usize] } else { [0xFFFF, rng.next() as u32 & 0xFFFF, !(1u32 << pick(rng, 16))][pick(rng, 3) as usize] };
            m.w16(g + 2, buttons as u16);
            for k in 4..8 {
                let near = [63, 64, 65, 97, 98, 160, 161, 192, 193][pick(rng, 9) as usize];
                let v = if quiet { 128 } else { [0, 128, 255, near, near, pick(rng, 256)][pick(rng, 6) as usize] };
                m.w8(g + k, v as u8);
            }
            // (the kind it was reset as: a pad swapped for another is reset after `timedwait`,
            // which waits for the clock the test CPUs don't run)
            m.w16(PAD_KINDS + 2 * p, kind as u16);
            // (`gPadinfo` as the last `PAD_update` left it: `VerifyControllerValues` reads it
            // without one)
            for k in 0..8 {
                let v = m.r8(g + k);
                m.w8(PAD_INFO + 32 * p + 4 + k, v);
            }
            m.w32(HELD + 4 * p, [0, rng.next() as u32, u32::MAX][pick(rng, 3) as usize]);
        }
        let ticks = m.r32(GAME_TICKS);
        m.w32(REPEAT_AT, [0, ticks, ticks.wrapping_add(5), ticks.wrapping_sub(5)][pick(rng, 4) as usize]);
        m.w32(REPEAT_DELAY, [0, 10, 30][pick(rng, 3) as usize]);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("VerifyControllerValues", 0x8002_38b8, nfs4rc::f_800238b8, |_, r| vec![pick(r, 2)]),
            ("GetNoDebounceKey", 0x8002_3918, nfs4rc::f_80023918, |c, r| vec![key(&mut c.m, r), pick(r, 2)]),
            ("GetDebounceKey", 0x8002_3b94, nfs4rc::f_80023b94, |c, r| vec![key(&mut c.m, r), pick(r, 2)]),
            ("GetKeyFromPlayer", 0x8002_3ca8, nfs4rc::f_80023ca8, |_, r| vec![pick(r, 2), [0, u32::MAX, 0x200, 0x1800][pick(r, 4) as usize]]),
        ]
    }

    #[test]
    fn feinput_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0x1A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 400, 0x1A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
