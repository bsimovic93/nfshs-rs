//! AITRIGER.CPP: the track's triggers (where the police set up a roadblock, wait off the road,
//! send traffic down a path...), sorted by slice; each of 9 users walks them with its own
//! cursor, and a trigger once taken waits 20 s before it fires again.

use crate::abi::{self, Saved};
use crate::fixed::fixeddiv;
use crate::globals::SIM_COUNTER;
use crate::types::{AITriggerTriggerManager, Trigger};
use ps1rt::{Cpu, Mem};

/// Ticks a trigger rests after it is taken (20 s).
const REST: i32 = 2560;
/// `trigger_t.type`s: the traffic path (its points follow it), and the one
/// `CheckForTriggerAtSlice` never gives.
const TRAFFIC_PATH: i32 = 5;
const NOT_AT_SLICE: i32 = 2;

fn trigger(m: &mut Mem, this: AITriggerTriggerManager, i: i32) -> Trigger {
    Trigger(this.triggers_(m, i as u32))
}

/// `AITrigger_TriggerManager::Init` (AITRIGER.CPP, 0x8007314c): no triggers, each one rested,
/// the cursors at the first; then those of `data` (a count and the triggers one after another)
/// put in, sorted by slice.
pub fn init(c: &mut Cpu, this: AITriggerTriggerManager, data: u32) {
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        r.0[3] = this.0;
        let m = &mut c.m;
        this.set_num_triggers_(m, 0);
        this.set_inv_num_triggers_(m, 0);
        for i in (0..100).rev() {
            this.set_check_time_(m, i, -REST);
        }
        for i in (0..9).rev() {
            this.set_last_trigger_checked_(m, i, 0);
        }
        if data == 0 {
            return;
        }
        r.0[2] = data;
        r.0[1] = data.wrapping_add(4);
        r.0[0] = 0;
        while (r.0[0] as i32) < c.m.r32(data) as i32 {
            let at = r.0[1];
            let size = r.enter(c, 0x8007_3228, 0x8007_31d0, |c| insert_trigger(c, this, Trigger(at), true));
            r.0[1] = at.wrapping_add(size as u32);
            r.0[0] = r.0[0].wrapping_add(1);
        }
        r.enter(c, 0x8007_3710, 0x8007_31f0, |c| sort(c, this));
        let n = this.num_triggers_(&mut c.m);
        if n != 0 {
            this.set_inv_num_triggers_(&mut c.m, fixeddiv(0x1_0000, n << 16));
        }
    })
}

pub fn abi_init(c: &mut Cpu) {
    init(c, AITriggerTriggerManager(c.r[4]), c.r[5]);
}

/// `AITrigger_TriggerManager::InsertTrigger` (AITRIGER.CPP, 0x80073228): `t` the next trigger;
/// its size in the data (a traffic path's points follow it: with `fixup` its pointer to them
/// set), 0 for a type it doesn't know.
pub fn insert_trigger(c: &mut Cpu, this: AITriggerTriggerManager, t: Trigger, fixup: bool) -> i32 {
    abi::frame(c, 40, |c, _| {
        let m = &mut c.m;
        describe_trigger(this, t);
        let kind = t.any().type_(m);
        let n = this.num_triggers_(m);
        this.set_triggers_(m, n as u32, t.0);
        let size = match kind {
            TRAFFIC_PATH => {
                let path = t.traffic_path();
                if fixup {
                    path.set_path(m, t.0.wrapping_add(64));
                }
                path.num_points(m).wrapping_mul(20).wrapping_add(64)
            }
            1 | 2 => 20,
            3 => 72,
            _ => 0,
        };
        this.update_num_triggers_(m, |n| n.wrapping_add(1));
        size
    })
}

pub fn abi_insert_trigger(c: &mut Cpu) {
    c.r[2] = insert_trigger(c, AITriggerTriggerManager(c.r[4]), Trigger(c.r[5]), c.r[6] != 0) as u32;
}

/// `AITrigger_TriggerManager::GetNextTrigger` (AITRIGER.CPP, 0x80073324): cursor `which` on to
/// the next trigger (the first after the last); it, or none (0).
pub fn get_next_trigger(m: &mut Mem, this: AITriggerTriggerManager, which: i32) -> u32 {
    if this.num_triggers_(m) == 0 {
        return 0;
    }
    let w = which as u32;
    this.update_last_trigger_checked_(m, w, |i| i.wrapping_add(1));
    if this.last_trigger_checked_(m, w) == this.num_triggers_(m) {
        this.set_last_trigger_checked_(m, w, 0);
    }
    cursor_trigger(m, this, w)
}

pub fn abi_get_next_trigger(c: &mut Cpu) {
    c.r[2] = get_next_trigger(&mut c.m, AITriggerTriggerManager(c.r[4]), c.r[5] as i32);
}

/// `AITrigger_TriggerManager::GetPrevTrigger` (AITRIGER.CPP, 0x80073394): cursor `which` back
/// to the trigger before (the last before the first).
pub fn get_prev_trigger(m: &mut Mem, this: AITriggerTriggerManager, which: i32) -> u32 {
    if this.num_triggers_(m) == 0 {
        return 0;
    }
    let w = which as u32;
    this.update_last_trigger_checked_(m, w, |i| i.wrapping_sub(1));
    if this.last_trigger_checked_(m, w) == -1 {
        let last = this.num_triggers_(m).wrapping_sub(1);
        this.set_last_trigger_checked_(m, w, last);
    }
    cursor_trigger(m, this, w)
}

pub fn abi_get_prev_trigger(c: &mut Cpu) {
    c.r[2] = get_prev_trigger(&mut c.m, AITriggerTriggerManager(c.r[4]), c.r[5] as i32);
}

/// The trigger at cursor `w`, or none (0) past the end.
fn cursor_trigger(m: &mut Mem, this: AITriggerTriggerManager, w: u32) -> u32 {
    let i = this.last_trigger_checked_(m, w);
    if i < this.num_triggers_(m) { this.triggers_(m, i as u32) } else { 0 }
}

/// `AITrigger_TriggerManager::CheckForTriggerAtSlice` (AITRIGER.CPP, 0x80073404): cursor
/// `which` moved to `slice` (on while its trigger is before it, back while after; not round the
/// end); the index of the trigger there if one is, rested and of another type than 2, else -1.
pub fn check_for_trigger_at_slice(c: &mut Cpu, this: AITriggerTriggerManager, which: i32, slice: i32) -> i32 {
    abi::frame(c, 40, |c, _| {
        let m = &mut c.m;
        let n = this.num_triggers_(m);
        if n == 0 {
            return -1;
        }
        let w = which as u32;
        let at = |m: &mut Mem| {
            let i = this.last_trigger_checked_(m, w);
            trigger(m, this, i).any().slice(m)
        };
        if at(m) < slice && n >= 2 {
            while this.last_trigger_checked_(m, w) != this.num_triggers_(m).wrapping_sub(1) {
                get_next_trigger(m, this, which);
                if at(m) >= slice {
                    break;
                }
            }
        } else if slice < at(m) && this.num_triggers_(m) >= 2 {
            while this.last_trigger_checked_(m, w) != 0 {
                get_prev_trigger(m, this, which);
                if at(m) <= slice {
                    break;
                }
            }
        }
        let i = this.last_trigger_checked_(m, w);
        let t = trigger(m, this, i);
        if t.any().slice(m) != slice
            || SIM_COUNTER.get(m).wrapping_sub(this.check_time_(m, i as u32)) <= REST
            || t.any().type_(m) == NOT_AT_SLICE
        {
            return -1;
        }
        i
    })
}

pub fn abi_check_for_trigger_at_slice(c: &mut Cpu) {
    c.r[2] = check_for_trigger_at_slice(c, AITriggerTriggerManager(c.r[4]), c.r[5] as i32, c.r[6] as i32) as u32;
}

/// `AITrigger_TriggerManager::GetTrigger` (AITRIGER.CPP, 0x800735b8): trigger `i` taken
/// (`*resting` whether it was still resting); it, or none (0) past the end.
pub fn get_trigger(m: &mut Mem, this: AITriggerTriggerManager, i: i32, resting: u32) -> u32 {
    let k = i as u32;
    let rests = SIM_COUNTER.get(m).wrapping_sub(this.check_time_(m, k)) <= REST;
    m.w32(resting, rests as u32);
    let now = SIM_COUNTER.get(m);
    this.set_check_time_(m, k, now);
    if i < this.num_triggers_(m) { this.triggers_(m, k) } else { 0 }
}

pub fn abi_get_trigger(c: &mut Cpu) {
    c.r[2] = get_trigger(&mut c.m, AITriggerTriggerManager(c.r[4]), c.r[5] as i32, c.r[6]);
}

/// `AITrigger_TriggerManager::CheckForClosestTriggerOfType` (AITRIGER.CPP, 0x80073620): the
/// index of the trigger of `kind` at `slice`, else with `dir` > 0 the first after it (round to
/// the first of the kind), with `dir` < 0 the last before it (one after it when none is
/// before, the next after that ending the search); -1 for none or `dir` 0.
pub fn check_for_closest_trigger_of_type(m: &mut Mem, this: AITriggerTriggerManager, slice: i32, kind: i32, dir: i32) -> i32 {
    let (mut best, mut first) = (0u32, 0u32);
    let (mut best_i, mut first_i) = (-1, -1);
    let n = this.num_triggers_(m);
    if n == 0 {
        return -1;
    }
    for i in 0..n {
        let t = trigger(m, this, i);
        if t.any().type_(m) != kind {
            continue;
        }
        if first == 0 {
            first = t.0;
            first_i = i;
        }
        let s = t.any().slice(m);
        if s == slice {
            return i;
        }
        if slice < s {
            if dir > 0 {
                return i;
            }
            if dir < 0 && best != 0 {
                return best_i;
            }
        }
        best = t.0;
        best_i = i;
    }
    if best != 0 && dir < 0 {
        return best_i;
    }
    if dir > 0 { first_i } else { -1 }
}

pub fn abi_check_for_closest_trigger_of_type(c: &mut Cpu) {
    c.r[2] = check_for_closest_trigger_of_type(&mut c.m, AITriggerTriggerManager(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32) as u32;
}

/// `AITrigger_TriggerManager::DescribeTrigger` (AITRIGER.CPP, 0x800736f0): nothing (debug output
/// in the development build).
pub fn describe_trigger(_: AITriggerTriggerManager, _: Trigger) {}

pub fn abi_describe_trigger(_: &mut Cpu) {}

/// `AITrigger_Compare` (AITRIGER.CPP, 0x800736f8): `qsort`'s order of two triggers (pointers
/// to them), by slice.
pub fn compare(m: &mut Mem, a: u32, b: u32) -> i32 {
    let (a, b) = (Trigger(m.r32(a)), Trigger(m.r32(b)));
    a.any().slice(m).wrapping_sub(b.any().slice(m))
}

pub fn abi_compare(c: &mut Cpu) {
    c.r[2] = compare(&mut c.m, c.r[4], c.r[5]) as u32;
}

/// `AITrigger_TriggerManager::Sort` (AITRIGER.CPP, 0x80073710): the triggers by slice (the C
/// library's `qsort`).
pub fn sort(c: &mut Cpu, this: AITriggerTriggerManager) {
    abi::frame(c, 24, |c, _| {
        let n = this.num_triggers_(&mut c.m);
        if n != 0 {
            let r = Saved::of(c);
            r.call(c, nfs4rc::f_800e6978, &[this.0.wrapping_add(8), n as u32, 4, 0x8007_36f8], 0x8007_3738);
        }
    })
}

pub fn abi_sort(c: &mut Cpu) {
    sort(c, AITriggerTriggerManager(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH_TOP, against_original, same_stack};

    /// RAM the race leaves unused: the manager, its triggers, `Init`'s data, `GetTrigger`'s out.
    const MANAGER: u32 = 0x8014_0000;
    const TRIGGERS: u32 = 0x8014_1000;
    const DATA: u32 = 0x8014_2000;
    const OUT: u32 = 0x8014_3000;

    /// A manager of 0 to 14 triggers of any type (0 to 6) on slices near one another, some
    /// sorted, rested or not, the cursors anywhere on them; `Init`'s data (any types, a
    /// traffic path's points); the clock anywhere.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let now = pick(0x10_0000);
        SIM_COUNTER.set(m, now);
        let n = [0, 1, 2, pick(15)][pick(4) as usize];
        let this = AITriggerTriggerManager(MANAGER);
        this.set_num_triggers_(m, n);
        let base = pick(400);
        let sorted = pick(2) == 0;
        let mut slice = base;
        for i in 0..n as u32 {
            let t = TRIGGERS + 0x50 * i;
            m.w32(t, pick(7) as u32);
            slice += pick(4) * pick(2);
            m.w32(t + 4, if sorted { slice } else { base + pick(30) } as u32);
            this.set_triggers_(m, i, t);
        }
        for i in 0..100 {
            this.set_check_time_(m, i, now - [0, 2560, 2561, pick(6000)][pick(4) as usize]);
        }
        for i in 0..9 {
            this.set_last_trigger_checked_(m, i, if n == 0 { pick(2) } else { pick(n as u32) });
        }
        // Init's data
        let count = [0, 1, pick(12)][pick(3) as usize];
        m.w32(DATA, count as u32);
        let mut at = DATA + 4;
        for _ in 0..count {
            let kind = [1, 2, 3, 5, pick(7)][pick(5) as usize];
            m.w32(at, kind as u32);
            m.w32(at + 4, (base + pick(30)) as u32);
            let points = pick(4);
            m.w32(at + 0x38, points as u32);
            at += match kind {
                1 | 2 => 20,
                3 => 72,
                5 => 64 + 20 * points as u32,
                _ => 0,
            };
        }
    }

    type Port = fn(&mut Cpu, &[u32]) -> Option<u32>;

    #[test]
    fn aitrigger_fuzz() {
        let cases: [(&str, fn(&mut Cpu), Port); 9] = [
            ("Init", nfs4rc::f_8007314c, |c, a| {
                init(c, AITriggerTriggerManager(a[0]), a[1]);
                None
            }),
            ("InsertTrigger", nfs4rc::f_80073228, |c, a| Some(insert_trigger(c, AITriggerTriggerManager(a[0]), Trigger(a[1]), a[2] != 0) as u32)),
            ("GetNextTrigger", nfs4rc::f_80073324, |c, a| Some(get_next_trigger(&mut c.m, AITriggerTriggerManager(a[0]), a[1] as i32))),
            ("GetPrevTrigger", nfs4rc::f_80073394, |c, a| Some(get_prev_trigger(&mut c.m, AITriggerTriggerManager(a[0]), a[1] as i32))),
            ("CheckForTriggerAtSlice", nfs4rc::f_80073404, |c, a| Some(check_for_trigger_at_slice(c, AITriggerTriggerManager(a[0]), a[1] as i32, a[2] as i32) as u32)),
            ("GetTrigger", nfs4rc::f_800735b8, |c, a| Some(get_trigger(&mut c.m, AITriggerTriggerManager(a[0]), a[1] as i32, a[2]))),
            ("CheckForClosestTriggerOfType", nfs4rc::f_80073620, |c, a| Some(check_for_closest_trigger_of_type(&mut c.m, AITriggerTriggerManager(a[0]), a[1] as i32, a[2] as i32, a[3] as i32) as u32)),
            ("Compare", nfs4rc::f_800736f8, |c, a| Some(compare(&mut c.m, a[0], a[1]) as u32)),
            ("Sort", nfs4rc::f_80073710, |c, a| {
                sort(c, AITriggerTriggerManager(a[0]));
                None
            }),
        ];
        for (k, (name, original, port)) in cases.into_iter().enumerate() {
            let args: &dyn Fn(&mut Cpu) -> Vec<u32> = &|c| {
                // (from the clock: the shuffle's generator isn't at hand)
                let r = SIM_COUNTER.get(&mut c.m) as u32;
                let n = AITriggerTriggerManager(MANAGER).num_triggers_(&mut c.m).max(1) as u32;
                let slice = c.m.r32(TRIGGERS + 4).wrapping_add(r % 7).wrapping_sub(3);
                match name {
                    "Init" => vec![MANAGER, [DATA, 0][(r % 8 == 0) as usize]],
                    "InsertTrigger" => vec![MANAGER, DATA + 4, r & 1],
                    "GetNextTrigger" | "GetPrevTrigger" => vec![MANAGER, r % 9],
                    "CheckForTriggerAtSlice" => vec![MANAGER, r % 9, slice],
                    "GetTrigger" => vec![MANAGER, (r >> 4) % (n + 2), OUT],
                    "CheckForClosestTriggerOfType" => vec![MANAGER, slice, (r >> 4) % 7, ((r >> 8) % 5).wrapping_sub(2)],
                    "Compare" => vec![MANAGER + 8, MANAGER + 8 + 4 * ((r >> 4) % n)],
                    _ => vec![MANAGER],
                }
            };
            against_original(name, 3_000, 0x7A1 + k as u64, shuffle, original, args, port);
        }
    }

    /// The stack `Init` (through `InsertTrigger`, `Sort` and the library's `qsort`) and
    /// `CheckForTriggerAtSlice` leave.
    #[test]
    fn trigger_stack_fuzz() {
        same_stack("Init", 0x8007_314c, nfs4rc::f_8007314c, 0x8006_728c, SCRATCH_TOP, 1_000, 0x7B1, shuffle, &|_, _| vec![MANAGER, DATA]);
        same_stack("CheckForTriggerAtSlice", 0x8007_3404, nfs4rc::f_80073404, 0x8006_6378, SCRATCH_TOP, 1_000, 0x7B2, shuffle, &|c, rng| {
            let slice = c.m.r32(TRIGGERS + 4).wrapping_add((rng.next() % 7) as u32).wrapping_sub(3);
            vec![MANAGER, (rng.next() % 9) as u32, slice]
        });
    }
}
