//! ANIM.CPP: the track's animated objects. A track's animations come from its "A.viv" (up to 10
//! scripts, "tr00a00.can" to "tr00a09.can", kept in the DCT buffer); each is a run of keyframes
//! (20 bytes: a place, three words; a turn, a quaternion of four shorts) a number of ticks apart
//! (`+0xA`, 1 to 400, else 6), their count at `+8`. The persistent objects of types 3 and 7 with
//! an index (`+3`) are animated instances (`Anim_gInstanceFromIndex`). An `AnimScript` (20 bytes:
//! its start (+0), its flags (+8: 2 run once, 4 turned a quarter; 6 at first), its animations
//! (+0x10, an array)) plays animations from the race's ticks; `Anim_Handle` keeps them in 32
//! slots (`animSlots`).
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the original
//! leaves it, the stack and CPU registers free; the loops tick as the original's (see
//! `call::ticks`).

use crate::call::{call_jal, ticks, with_buffer};
use crate::fixed::{div, rem};
use crate::globals::SIM_GLOBAL;
use ps1rt::{Cpu, Mem};

/// The slots (`animSlots`, 32 `AnimScript` pointers), the animated instances by index
/// (`Anim_gInstanceFromIndex`, 32), the scripts (`animScripts`, 10), the persistent objects
/// (`gPersistObjInst`: a count, then records of their own sizes, their type +2, index +3, kind
/// +0xD).
const SLOTS: u32 = 0x8010_F410;
const SLOT_COUNT: u32 = 32;
const INSTANCES: u32 = 0x8010_F490;
const INSTANCE_COUNT: u32 = 32;
const SCRIPTS: u32 = 0x8010_F510;
const SCRIPT_COUNT: u32 = 10;
const PERSIST_OBJ_INST: u32 = 0x8013_E7A4;
const ANIMATED: [u8; 2] = [3, 7];

/// An animation: its keyframes (+8) and their spacing (+0xA), the keyframes from +0xC (an
/// instance's from +0x10 unless of type 3 or 7: `Anim_GetRotPos`); a keyframe's turn (+0xC).
const KEYS: u32 = 8;
const SPACING: u32 = 10;
const KEYS_AT: u32 = 12;
const INSTANCE_KEYS_AT: u32 = 16;
const KEY_SIZE: u32 = 20;
const KEY_TURN: u32 = 12;
const MAX_SPACING: u32 = 400;
const DEFAULT_SPACING: i32 = 6;

/// An `AnimScript`: its start, its flags, its animations; the flags: 2 the animation runs once,
/// 4 turned a quarter about x (the matrix's rows 1 and 2).
const SCRIPT_START: u32 = 0;
const SCRIPT_FLAGS: u32 = 8;
const SCRIPT_ANIMS: u32 = 16;
const SCRIPT_SIZE: u32 = 20;
const DEFAULT_FLAGS: u32 = 6;
const ONCE: u32 = 2;
const TURNED: u32 = 4;

/// Functions called: the library's `sprintf`, `loadfileadrz(name, 0)`, `filesize`, `blockmove`,
/// `purgememadr`, `strstr`, `locatebig(file, name)`; `Platform_GetDCTBuffer(size, name)`,
/// `__builtin_new`, `__builtin_vec_new`, `__builtin_delete`, `__builtin_vec_delete`,
/// `Quatern_QuatToMat(q, m)`, `Quatern_Interpolate(qa, qb, pa, pb, t, q, p)`,
/// `Quatern_VecInterpolate(pa, pb, t, p)`, `DrawW_ResetAnimationTimer`.
const SPRINTF: u32 = 0x800E_50A8;
const LOAD_FILE_Z: u32 = 0x800E_6394;
const FILE_SIZE: u32 = 0x800E_6258;
const BLOCK_MOVE: u32 = 0x800E_6EC8;
const PURGE: u32 = 0x800E_612C;
const STRSTR: u32 = 0x800E_6EB8;
const LOCATE_BIG: u32 = 0x800E_6E04;
const GET_DCT_BUFFER: u32 = 0x800D_D0C0;
const NEW: u32 = 0x800A_48C0;
const VEC_NEW: u32 = 0x800A_48E8;
const DELETE: u32 = 0x800A_4910;
const VEC_DELETE: u32 = 0x800A_4930;
const QUAT_TO_MAT: u32 = 0x800A_E418;
const INTERPOLATE: u32 = 0x800A_E310;
const VEC_INTERPOLATE: u32 = 0x800A_E260;
const RESET_ANIMATION_TIMER: u32 = 0x800C_837C;
/// "%sA.viv", what `Anim_InitSystem` looks for in the path (and doesn't use), "animScripts",
/// "tr00a%02d.can".
const VIV_FORMAT: u32 = 0x8013_D8EC;
const PATH_PART: u32 = 0x8013_D8F4;
const DCT_NAME: u32 = 0x8005_5E6C;
const SCRIPT_FORMAT: u32 = 0x8005_5E78;

fn sh(m: &mut Mem, at: u32) -> i32 {
    m.r16(at) as i16 as i32
}

fn now(m: &mut Mem) -> i32 {
    SIM_GLOBAL.game_ticks(m)
}

/// An `AnimScript`'s `k`-th animation.
fn script_anim(m: &mut Mem, script: u32, k: u32) -> u32 {
    let anims = m.r32(script + SCRIPT_ANIMS);
    m.r32(anims.wrapping_add(k << 2))
}

/// An `AnimScript` and its animations given back.
fn free_script(c: &mut Cpu, script: u32) {
    let anims = c.m.r32(script + SCRIPT_ANIMS);
    if anims != 0 {
        call_jal(c, VEC_DELETE, &[anims]);
    }
    call_jal(c, DELETE, &[script]);
}

/// `Anim_Restart` (ANIM.CPP, 0x80074374): the slots' scripts given back, the animation timer
/// reset.
pub fn restart(c: &mut Cpu) {
    for k in 0..SLOT_COUNT {
        let script = c.m.r32(SLOTS + 4 * k);
        if script != 0 {
            free_script(c, script);
        }
        c.m.w32(SLOTS + 4 * k, 0);
        c.tick();
    }
    call_jal(c, RESET_ANIMATION_TIMER, &[]);
}

pub fn abi_restart(c: &mut Cpu) {
    restart(c);
}

/// `Anim_InitSystem` (ANIM.CPP, 0x800743fc): the slots emptied; the track's scripts from its
/// "A.viv" at `path` (moved to the DCT buffer), or none; the animated instances found. 0.
pub fn init_system(c: &mut Cpu, path: u32) -> u32 {
    restart(c);
    with_buffer(c, 80, |c, name| {
        call_jal(c, SPRINTF, &[name, VIV_FORMAT, path]);
        let file = call_jal(c, LOAD_FILE_Z, &[name, 0]);
        if file != 0 {
            let size = call_jal(c, FILE_SIZE, &[name]);
            let buffer = call_jal(c, GET_DCT_BUFFER, &[size, DCT_NAME]);
            call_jal(c, BLOCK_MOVE, &[file, buffer, size]);
            call_jal(c, PURGE, &[file]);
            call_jal(c, STRSTR, &[path, PATH_PART]);
            for k in 0..SCRIPT_COUNT {
                call_jal(c, SPRINTF, &[name, SCRIPT_FORMAT, k]);
                let script = call_jal(c, LOCATE_BIG, &[buffer, name]);
                c.m.w32(SCRIPTS + 4 * k, script);
            }
        } else {
            for k in 0..SCRIPT_COUNT {
                c.m.w32(SCRIPTS + 4 * k, 0);
            }
        }
        ticks(c, SCRIPT_COUNT - 1);
    });
    init_instances(c);
    0
}

/// The persistent objects of the animated types with an index noted by it.
pub fn init_instances(c: &mut Cpu) {
    let m = &mut c.m;
    let group = m.r32(PERSIST_OBJ_INST);
    if group == 0 {
        return;
    }
    let n = m.r32(group);
    let mut record = group + 4;
    for _ in 0..n {
        let index = m.r8(record + 3) as u32;
        if ANIMATED.contains(&m.r8(record + 2)) && index != 0 {
            m.w32(INSTANCES + 4 * index, record);
        }
        record = record.wrapping_add(sh(m, record) as u32);
    }
    ticks(c, n.saturating_sub(1));
}

pub fn abi_init_system(c: &mut Cpu) {
    c.r[2] = init_system(c, c.r[4]);
}

/// `Anim_DeInitSystem` (ANIM.CPP, 0x80074590): the instances forgotten (after an empty loop the
/// compiler left, 9 passes).
pub fn de_init_system(c: &mut Cpu) {
    ticks(c, 9);
    for k in 0..INSTANCE_COUNT {
        c.m.w32(INSTANCES + 4 * k, 0);
    }
    ticks(c, INSTANCE_COUNT - 1);
}

pub fn abi_de_init_system(c: &mut Cpu) {
    de_init_system(c);
}

/// `Anim_Handle` (ANIM.CPP, 0x800745c8): a script playing script `k` in the first free slot (the
/// 33rd, past the slots, when none is), its number.
pub fn handle(c: &mut Cpu, k: u32) -> u32 {
    let slot = (0..SLOT_COUNT).find(|&s| c.m.r32(SLOTS + 4 * s) == 0).unwrap_or(SLOT_COUNT);
    ticks(c, slot.min(SLOT_COUNT - 1));
    let script = call_jal(c, NEW, &[SCRIPT_SIZE]);
    let script = script_new(c, script, k);
    c.m.w32(SLOTS + 4 * slot, script);
    slot
}

pub fn abi_handle(c: &mut Cpu) {
    c.r[2] = handle(c, c.r[4]);
}

/// `Anim_FreeHandle` (ANIM.CPP, 0x8007464c): slot `h`'s script given back. 0.
pub fn free_handle(c: &mut Cpu, h: u32) -> u32 {
    let at = SLOTS.wrapping_add(h << 2);
    let script = c.m.r32(at);
    if script != 0 {
        free_script(c, script);
    }
    c.m.w32(at, 0);
    0
}

pub fn abi_free_handle(c: &mut Cpu) {
    c.r[2] = free_handle(c, c.r[4]);
}

/// `Anim_GetAnim` (ANIM.CPP, 0x800746cc): slot `h`'s script.
pub fn get_anim(m: &mut Mem, h: u32) -> u32 {
    m.r32(SLOTS.wrapping_add(h << 2))
}

pub fn abi_get_anim(c: &mut Cpu) {
    c.r[2] = get_anim(&mut c.m, c.r[4]);
}

/// A keyframe's place (three words) to `pos`.
fn key_place(m: &mut Mem, key: u32, pos: u32) {
    for k in 0..3 {
        let v = m.r32(key + 4 * k);
        m.w32(pos.wrapping_add(4 * k), v);
    }
}

/// The matrix `mat` turned a quarter about x: its row 1 the old row 2, its row 2 the old row 1
/// negated.
fn turn_quarter(m: &mut Mem, mat: u32) {
    for k in 0..3 {
        let (r1, r2) = (mat + 12 + 4 * k, mat + 24 + 4 * k);
        let (a, b) = (m.r32(r1), m.r32(r2));
        m.w32(r1, b);
        m.w32(r2, a.wrapping_neg());
    }
}

/// `Anim_GetLastRotPos` (ANIM.CPP, 0x800746e8): `anim`'s last keyframe's place to `pos`, its turn
/// as a matrix to `mat`.
pub fn get_last_rot_pos(c: &mut Cpu, anim: u32, pos: u32, mat: u32) {
    let last = sh(&mut c.m, anim + KEYS).wrapping_sub(1);
    let key = anim.wrapping_add(KEYS_AT).wrapping_add(last.wrapping_mul(KEY_SIZE as i32) as u32);
    with_buffer(c, 8, |c, quat| {
        for k in 0..8 {
            let b = c.m.r8(key.wrapping_add(KEY_TURN + k));
            c.m.w8(quat + k, b);
        }
        key_place(&mut c.m, key, pos);
        call_jal(c, QUAT_TO_MAT, &[quat, mat]);
    });
}

pub fn abi_get_last_rot_pos(c: &mut Cpu) {
    get_last_rot_pos(c, c.r[4], c.r[5], c.r[6]);
}

/// `Anim_GetLastAnimPosRot` (ANIM.CPP, 0x80074760): script `k`'s last place and turn (turned a
/// quarter when `flags` say). 1.
pub fn get_last_anim_pos_rot(c: &mut Cpu, k: u32, flags: u32, pos: u32, mat: u32) -> u32 {
    let anim = c.m.r32(SCRIPTS.wrapping_add(k << 2));
    get_last_rot_pos(c, anim, pos, mat);
    if flags & TURNED != 0 {
        turn_quarter(&mut c.m, mat);
    }
    1
}

pub fn abi_get_last_anim_pos_rot(c: &mut Cpu) {
    c.r[2] = get_last_anim_pos_rot(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// The keyframes' spacing (ticks): its own when 1 to 400, else 6.
fn spacing(m: &mut Mem, anim: u32) -> i32 {
    let v = m.r16(anim + SPACING) as u32;
    if v.wrapping_sub(1) < MAX_SPACING { v as i16 as i32 } else { DEFAULT_SPACING }
}

/// Where `time` (ticks) falls in `anim`: the keyframe before it (round the animation's), and how
/// far to the next (16.16); none past the end when the animation runs once.
fn place_in(m: &mut Mem, anim: u32, flags: u32, time: i32) -> Option<(u32, i32)> {
    let step = spacing(m, anim);
    let gaps = sh(m, anim + KEYS).wrapping_sub(1);
    let frame = div(time, step);
    if flags & ONCE != 0 && frame >= gaps {
        return None;
    }
    let t = div(rem(time, step) << 16, step);
    Some((rem(frame, gaps) as u32, t))
}

/// `Anim_GetRotPos` (ANIM.CPP, 0x800747f8): instance `anim`'s place (to `pos`) and turn (to `mat`)
/// at `time`: between its keyframes, the turns interpolated as quaternions; 0 past its end when
/// it runs once, else 1.
pub fn get_rot_pos(c: &mut Cpu, anim: u32, flags: u32, time: i32, pos: u32, mat: u32) -> u32 {
    let keys_at = if ANIMATED.contains(&c.m.r8(anim + 2)) { KEYS_AT } else { INSTANCE_KEYS_AT };
    let Some((frame, t)) = place_in(&mut c.m, anim, flags, time) else { return 0 };
    let key = anim.wrapping_add(keys_at).wrapping_add(frame.wrapping_mul(KEY_SIZE));
    let next = key.wrapping_add(KEY_SIZE);
    with_buffer(c, 64, |c, buf| {
        let (qa, qb, pa, pb, q) = (buf, buf + 8, buf + 16, buf + 32, buf + 48);
        for k in 0..8 {
            let (a, b) = (c.m.r8(key.wrapping_add(KEY_TURN + k)), c.m.r8(next.wrapping_add(KEY_TURN + k)));
            c.m.w8(qa + k, a);
            c.m.w8(qb + k, b);
        }
        key_place(&mut c.m, key, pa);
        key_place(&mut c.m, next, pb);
        call_jal(c, INTERPOLATE, &[qa, qb, pa, pb, t as u32, q, pos]);
        call_jal(c, QUAT_TO_MAT, &[q, mat]);
    });
    1
}

pub fn abi_get_rot_pos(c: &mut Cpu) {
    let mat = c.m.r32(c.r[29] + 16);
    c.r[2] = get_rot_pos(c, c.r[4], c.r[5], c.r[6] as i32, c.r[7], mat);
}

/// `Anim_GetPos` (ANIM.CPP, 0x80074a0c): `anim`'s place at `time` (to `pos`), and (when asked)
/// the time within the animation and its length; 0 past its end when it runs once, else 1.
pub fn get_pos(c: &mut Cpu, anim: u32, flags: u32, time: i32, pos: u32, within: u32, length: u32) -> u32 {
    let Some((frame, t)) = place_in(&mut c.m, anim, flags, time) else { return 0 };
    let key = anim.wrapping_add(KEYS_AT).wrapping_add(frame.wrapping_mul(KEY_SIZE));
    with_buffer(c, 32, |c, buf| {
        key_place(&mut c.m, key, buf);
        key_place(&mut c.m, key.wrapping_add(KEY_SIZE), buf + 16);
        call_jal(c, VEC_INTERPOLATE, &[buf, buf + 16, t as u32, pos]);
    });
    let m = &mut c.m;
    let full = sh(m, anim + KEYS).wrapping_sub(1).wrapping_mul(spacing(m, anim));
    if within != 0 {
        m.w32(within, rem(time, full) as u32);
    }
    if length != 0 {
        m.w32(length, full as u32);
    }
    1
}

pub fn abi_get_pos(c: &mut Cpu) {
    let sp = c.r[29];
    let (within, length) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
    c.r[2] = get_pos(c, c.r[4], c.r[5], c.r[6] as i32, c.r[7], within, length);
}

/// A new script's start (now) and flags (6).
fn script_begin(m: &mut Mem, script: u32) -> u32 {
    m.w32(script + SCRIPT_FLAGS, DEFAULT_FLAGS);
    let t = now(m);
    m.w32(script + SCRIPT_START, t as u32);
    script
}

/// `AnimScript::AnimScript(int)` (ANIM.CPP, 0x80074c40): `script` playing script `k`.
pub fn script_new(c: &mut Cpu, script: u32, k: u32) -> u32 {
    let anims = call_jal(c, VEC_NEW, &[4]);
    c.m.w32(script + SCRIPT_ANIMS, anims);
    let anim = c.m.r32(SCRIPTS.wrapping_add(k << 2));
    c.m.w32(anims, anim);
    script_begin(&mut c.m, script)
}

pub fn abi_script_new(c: &mut Cpu) {
    c.r[2] = script_new(c, c.r[4], c.r[5]);
}

/// `AnimScript::AnimScript(int, int)` (ANIM.CPP, 0x80074cac): `script` playing the `n` scripts
/// from `first`.
pub fn script_new_run(c: &mut Cpu, script: u32, first: u32, n: i32) -> u32 {
    let anims = call_jal(c, VEC_NEW, &[(n << 2) as u32]);
    c.m.w32(script + SCRIPT_ANIMS, anims);
    for k in 0..n.max(0) as u32 {
        let m = &mut c.m;
        let anim = m.r32(SCRIPTS.wrapping_add(first.wrapping_add(k) << 2));
        let at = m.r32(script + SCRIPT_ANIMS).wrapping_add(k << 2);
        m.w32(at, anim);
    }
    ticks(c, (n.max(1) - 1) as u32);
    script_begin(&mut c.m, script)
}

pub fn abi_script_new_run(c: &mut Cpu) {
    c.r[2] = script_new_run(c, c.r[4], c.r[5], c.r[6] as i32);
}

/// `AnimScript::AnimScript(Group *, int, int, int)` (ANIM.CPP, 0x80074d48): `script` playing the
/// `group`'s instances of type `kind` and kind `id` (room for `n`).
pub fn script_new_group(c: &mut Cpu, script: u32, group: u32, kind: u32, id: u32, n: u32) -> u32 {
    let count = c.m.r32(group);
    let anims = call_jal(c, VEC_NEW, &[n << 2]);
    c.m.w32(script + SCRIPT_ANIMS, anims);
    let m = &mut c.m;
    let (mut record, mut found) = (group.wrapping_add(4), 0u32);
    for _ in 0..count {
        if m.r8(record + 2) as u32 == kind && m.r8(record + 13) as u32 == id {
            let at = m.r32(script + SCRIPT_ANIMS).wrapping_add(found << 2);
            m.w32(at, record);
            found += 1;
        }
        record = record.wrapping_add(sh(m, record) as u32);
    }
    ticks(c, count.saturating_sub(1));
    script_begin(&mut c.m, script)
}

pub fn abi_script_new_group(c: &mut Cpu) {
    let n = c.m.r32(c.r[29] + 16);
    c.r[2] = script_new_group(c, c.r[4], c.r[5], c.r[6], c.r[7], n);
}

/// `AnimScript::SetAnimAttrib` (ANIM.CPP, 0x80074e14): the flags.
pub fn set_anim_attrib(m: &mut Mem, script: u32, flags: u32) {
    m.w32(script + SCRIPT_FLAGS, flags);
}

pub fn abi_set_anim_attrib(c: &mut Cpu) {
    set_anim_attrib(&mut c.m, c.r[4], c.r[5]);
}

/// `AnimScript::GetAnimFrameInfo` (ANIM.CPP, 0x80074e1c): the first animation's keyframe now
/// (round it) to `frame`, its keyframes' gaps to `frames`.
pub fn get_anim_frame_info(m: &mut Mem, script: u32, frame: u32, frames: u32) {
    let anim = script_anim(m, script, 0);
    let elapsed = now(m).wrapping_sub(m.r32(script + SCRIPT_START) as i32);
    let gaps = sh(m, anim + KEYS).wrapping_sub(1);
    let f = rem(div(elapsed, spacing(m, anim)), gaps);
    m.w32(frame, f as u32);
    let gaps = sh(m, anim + KEYS).wrapping_sub(1);
    m.w32(frames, gaps as u32);
}

pub fn abi_get_anim_frame_info(c: &mut Cpu) {
    get_anim_frame_info(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `AnimScript::GetTimedAnimPosRot(coorddef *, matrixtdef *)` (ANIM.CPP, 0x80074ed8): the first
/// animation's.
pub fn get_timed_anim_pos_rot_first(c: &mut Cpu, script: u32, pos: u32, mat: u32) -> u32 {
    get_timed_anim_pos_rot(c, script, 0, pos, mat)
}

pub fn abi_get_timed_anim_pos_rot_first(c: &mut Cpu) {
    c.r[2] = get_timed_anim_pos_rot_first(c, c.r[4], c.r[5], c.r[6]);
}

/// `AnimScript::GetTimedAnimPosRot(int, coorddef *, matrixtdef *)` (ANIM.CPP, 0x80074f04): the
/// `k`-th animation's place and turn now (turned a quarter when the flags say): 1; the script
/// ended (its start -1, the animation forgotten) when a once animation is past its end: 0; -1
/// when the script plays nothing.
pub fn get_timed_anim_pos_rot(c: &mut Cpu, script: u32, k: u32, pos: u32, mat: u32) -> u32 {
    if get_status(&mut c.m, script) != 1 {
        return -1i32 as u32;
    }
    let m = &mut c.m;
    let anim = script_anim(m, script, k);
    let (flags, elapsed) = (m.r32(script + SCRIPT_FLAGS), now(m).wrapping_sub(m.r32(script + SCRIPT_START) as i32));
    if get_rot_pos(c, anim, flags, elapsed, pos, mat) == 0 {
        let m = &mut c.m;
        m.w32(script + SCRIPT_START, -1i32 as u32);
        let at = m.r32(script + SCRIPT_ANIMS).wrapping_add(k << 2);
        m.w32(at, 0);
        return 0;
    }
    if c.m.r32(script + SCRIPT_FLAGS) & TURNED != 0 {
        turn_quarter(&mut c.m, mat);
    }
    1
}

pub fn abi_get_timed_anim_pos_rot(c: &mut Cpu) {
    c.r[2] = get_timed_anim_pos_rot(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `AnimScript::GetStatus` (ANIM.CPP, 0x80074ffc): 1 while the first animation is there.
pub fn get_status(m: &mut Mem, script: u32) -> u32 {
    (script_anim(m, script, 0) != 0) as u32
}

pub fn abi_get_status(c: &mut Cpu) {
    c.r[2] = get_status(&mut c.m, c.r[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{COMPARE_TICKS, Rng, against_original};

    /// Free RAM (the race leaves it zero): six animations (or instances), a script and its
    /// animations, a script to build, a place and a matrix out, the time and length out, a
    /// group of instances.
    const ANIMS: u32 = 0x8014_0000;
    const ANIM_STEP: u32 = 0x200;
    const ANIM_COUNT: u32 = 6;
    const SCRIPT: u32 = 0x8014_1000;
    const SCRIPT_LIST: u32 = 0x8014_1100;
    const SCRIPT_OUT: u32 = 0x8014_1180;
    const POS: u32 = 0x8014_1200;
    const MAT: u32 = 0x8014_1220;
    const WITHIN: u32 = 0x8014_1260;
    const LENGTH: u32 = 0x8014_1264;
    const GROUP: u32 = 0x8014_1400;
    const GROUP_RECORDS: u32 = 12;

    fn pick_i(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
        lo + pick(rng, (hi - lo) as u32) as i32
    }

    fn anim(rng: &mut Rng) -> u32 {
        ANIMS + ANIM_STEP * pick(rng, ANIM_COUNT)
    }

    /// The race RAM with any animations: six (types 3, 7 or others; 2 to 9 keyframes of any
    /// spacing, 0 and past 400 among them; places and turns any), the scripts among them or
    /// none, a script (started a while ago or ended, any flags) playing some, the slots full or
    /// not, a group of instances (types and kinds from a few), the outputs any.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        for k in 0..ANIM_COUNT {
            let a = ANIMS + ANIM_STEP * k;
            m.w8(a + 2, [3, 7, 1, rng.next() as u8][pick(rng, 4) as usize]);
            m.w16(a + KEYS, (2 + pick(rng, 8)) as u16);
            m.w16(a + SPACING, [0, 1, 6, 100, 400, 401, 0xFFFF, pick(rng, 500)][pick(rng, 8) as usize] as u16);
            for j in (12..ANIM_STEP).step_by(4) {
                m.w32(a + j, rng.next() as u32);
            }
            for key in 0..10 {
                for q in 0..4 {
                    for at in [KEYS_AT, INSTANCE_KEYS_AT] {
                        m.w16(a + at + KEY_SIZE * key + KEY_TURN + 2 * q, pick_i(rng, -4096, 4097) as u16);
                    }
                }
            }
        }
        for k in 0..SCRIPT_COUNT {
            m.w32(SCRIPTS + 4 * k, if pick(rng, 5) == 0 { 0 } else { anim(rng) });
        }
        let now = now(m);
        m.w32(SCRIPT + SCRIPT_START, if pick(rng, 6) == 0 { -1i32 as u32 } else { now.wrapping_sub(pick(rng, 3000) as i32) as u32 });
        m.w32(SCRIPT + SCRIPT_FLAGS, pick(rng, 8));
        m.w32(SCRIPT + SCRIPT_ANIMS, SCRIPT_LIST);
        for k in 0..4 {
            m.w32(SCRIPT_LIST + 4 * k, if pick(rng, 6) == 0 { 0 } else { anim(rng) });
        }
        for k in 0..8 {
            m.w32(SCRIPT_OUT + 4 * k, rng.next() as u32);
        }
        for k in 0..SLOT_COUNT + 1 {
            m.w32(SLOTS + 4 * k, if pick(rng, 3) == 0 { 0 } else { rng.next() as u32 });
        }
        if pick(rng, 8) == 0 {
            for k in 0..SLOT_COUNT {
                m.w32(SLOTS + 4 * k, 1);
            }
        }
        for k in 0..24 {
            m.w32(POS + 4 * k, rng.next() as u32);
        }
        m.w32(GROUP, pick(rng, GROUP_RECORDS + 1));
        for k in 0..GROUP_RECORDS {
            let r = GROUP + 4 + 16 * k;
            m.w16(r, 16);
            m.w8(r + 2, [3, 7, 1][pick(rng, 3) as usize]);
            m.w8(r + 3, [0, 1 + pick(rng, 31)][pick(rng, 2) as usize] as u8);
            m.w8(r + 13, pick(rng, 3) as u8);
        }
        m.w32(PERSIST_OBJ_INST, [GROUP, GROUP, 0][pick(rng, 3) as usize]);
    }

    /// A script made as the game would (its animations' array too), or none.
    fn heap_script(c: &mut Cpu, r: &mut Rng) -> u32 {
        if pick(r, 4) == 0 {
            return 0;
        }
        let script = crate::abi::call(c, nfs4rc::f_800a48c0, &[SCRIPT_SIZE]);
        let anims = if pick(r, 4) == 0 { 0 } else { crate::abi::call(c, nfs4rc::f_800a48e8, &[8]) };
        c.m.w32(script + SCRIPT_ANIMS, anims);
        script
    }

    /// `Anim_InitSystem`'s instances (the test CPUs find no file): the original entered after
    /// the scripts are set, its frame as it has it there.
    fn instances_original(c: &mut Cpu) {
        let sp = c.r[29].wrapping_sub(120);
        c.r[29] = sp;
        c.m.w32(sp + 112, c.r[31]);
        for (k, slot) in (16..20).zip([96, 100, 104, 108]) {
            c.m.w32(sp + slot, c.r[k]);
        }
        nfs4rc::f_800743fc_at(c, 0x8007_44f4);
    }

    fn instances_port(c: &mut Cpu, _: &[u32]) -> Option<u32> {
        init_instances(c);
        None
    }

    fn cases() -> Vec<Case> {
        vec![
            ("Anim_Restart", 0x8007_4374, nfs4rc::f_80074374, |c, r| {
                for k in 0..SLOT_COUNT {
                    let s = heap_script(c, r);
                    c.m.w32(SLOTS + 4 * k, s);
                }
                vec![]
            }),
            ("Anim_DeInitSystem", 0x8007_4590, nfs4rc::f_80074590, |_, _| vec![]),
            ("Anim_Handle", 0x8007_45c8, nfs4rc::f_800745c8, |_, r| vec![pick(r, SCRIPT_COUNT)]),
            ("Anim_FreeHandle", 0x8007_464c, nfs4rc::f_8007464c, |c, r| {
                let h = pick(r, SLOT_COUNT);
                let s = heap_script(c, r);
                c.m.w32(SLOTS + 4 * h, s);
                vec![h]
            }),
            ("Anim_GetAnim", 0x8007_46cc, nfs4rc::f_800746cc, |_, r| vec![pick(r, SLOT_COUNT)]),
            ("Anim_GetLastRotPos", 0x8007_46e8, nfs4rc::f_800746e8, |_, r| vec![anim(r), POS, MAT]),
            ("Anim_GetLastAnimPosRot", 0x8007_4760, nfs4rc::f_80074760, |c, r| {
                let k = pick(r, SCRIPT_COUNT);
                let a = anim(r);
                c.m.w32(SCRIPTS + 4 * k, a);
                vec![k, pick(r, 8), POS, MAT]
            }),
            ("Anim_GetRotPos", 0x8007_47f8, nfs4rc::f_800747f8, |_, r| vec![anim(r), pick(r, 8), pick_i(r, -100, 5000) as u32, POS, MAT]),
            ("Anim_GetPos", 0x8007_4a0c, nfs4rc::f_80074a0c, |_, r| {
                vec![anim(r), pick(r, 8), pick_i(r, -100, 5000) as u32, POS, [0, WITHIN][pick(r, 2) as usize], [0, LENGTH][pick(r, 2) as usize]]
            }),
            ("AnimScript::AnimScript(int)", 0x8007_4c40, nfs4rc::f_80074c40, |_, r| vec![SCRIPT_OUT, pick(r, SCRIPT_COUNT)]),
            ("AnimScript::AnimScript(int, int)", 0x8007_4cac, nfs4rc::f_80074cac, |_, r| {
                let first = pick(r, SCRIPT_COUNT);
                vec![SCRIPT_OUT, first, pick(r, SCRIPT_COUNT - first + 1)]
            }),
            ("AnimScript::AnimScript(Group *, int, int, int)", 0x8007_4d48, nfs4rc::f_80074d48, |_, r| {
                vec![SCRIPT_OUT, GROUP, [3, 7, 1][pick(r, 3) as usize], pick(r, 3), GROUP_RECORDS]
            }),
            ("AnimScript::SetAnimAttrib", 0x8007_4e14, nfs4rc::f_80074e14, |_, r| vec![SCRIPT, r.next() as u32]),
            ("AnimScript::GetAnimFrameInfo", 0x8007_4e1c, nfs4rc::f_80074e1c, |c, r| {
                let a = anim(r);
                c.m.w32(SCRIPT_LIST, a);
                vec![SCRIPT, WITHIN, LENGTH]
            }),
            ("AnimScript::GetTimedAnimPosRot(coorddef *, matrixtdef *)", 0x8007_4ed8, nfs4rc::f_80074ed8, |_, _| vec![SCRIPT, POS, MAT]),
            ("AnimScript::GetTimedAnimPosRot(int, coorddef *, matrixtdef *)", 0x8007_4f04, nfs4rc::f_80074f04, |c, r| {
                let k = pick(r, 4);
                if c.m.r32(SCRIPT_LIST + 4 * k) == 0 {
                    let a = anim(r);
                    c.m.w32(SCRIPT_LIST + 4 * k, a);
                }
                vec![SCRIPT, k, POS, MAT]
            }),
            ("AnimScript::GetStatus", 0x8007_4ffc, nfs4rc::f_80074ffc, |_, _| vec![SCRIPT]),
        ]
    }

    #[test]
    fn anim_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            COMPARE_TICKS.set(true);
            let args_rng = std::cell::RefCell::new(Rng(0x5A0 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 400, 0x5A1 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn anim_instances_fuzz() {
        COMPARE_TICKS.set(true);
        against_original("Anim_InitSystem (the instances)", 400, 0x5B1, shuffle, instances_original, &|_| vec![], instances_port);
    }
}
