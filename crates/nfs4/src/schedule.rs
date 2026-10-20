//! SCHEDULE.CPP: the simulation's schedules (lists of functions run each tick, by priority)
//! and how often far-off objects are simulated.

use crate::abi;
use crate::globals::{SCHED_EXECUTE_ELAPSED_TIMES, SCHED_EXECUTE_INFO, SCHED_EXECUTE_I_TIMES, SCHED_EXECUTE_MASKS, SCHED_EXECUTE_TIMES, SIM_COUNTER};
use crate::types::SchedSchedule;
use ps1rt::Cpu;

/// The priority of an empty slot (after every function's).
const EMPTY: i32 = 32766;

/// `Sched_ExecuteCheck` (SCHEDULE.CPP, 0x800b6e90): whether an object of `kind` at `dist`
/// (16.16) is simulated this tick, and its time step (`time`, `elapsed`, `i_time` set to the
/// rate's). The rate comes by the distance in 16 m steps (the 20th and on alike; the first
/// with `full`), and its mask picks the 64 Hz ticks, staggered by `id` times `stagger`.
/// Everything is simulated every tick for the first 16.
#[allow(clippy::too_many_arguments)]
pub fn execute_check(c: &mut Cpu, stagger: i32, kind: i32, dist: i32, id: i32, time: u32, elapsed: u32, i_time: u32, full: i32) -> bool {
    let m = &mut c.m;
    let counter = SIM_COUNTER.get(m);
    let rate = if counter < 16 {
        6
    } else {
        let step = (dist / 16 / 0x1_0000).min(19);
        let step = if full != 0 { 0 } else { step };
        m.r8(SCHED_EXECUTE_INFO.wrapping_add(kind.wrapping_mul(20).wrapping_add(step) as u32)) as u32
    };
    let table = |m: &mut ps1rt::Mem, t: u32| m.r32(t.wrapping_add(4 * rate));
    let v = table(m, SCHED_EXECUTE_TIMES);
    m.w32(time, v);
    let v = table(m, SCHED_EXECUTE_I_TIMES);
    m.w32(i_time, v);
    let v = table(m, SCHED_EXECUTE_ELAPSED_TIMES);
    m.w32(elapsed, v);
    if counter < 16 {
        return true;
    }
    let mask = table(m, SCHED_EXECUTE_MASKS);
    let tick = (counter / 2).wrapping_add(stagger.wrapping_mul(id)) as u32;
    tick & mask == mask
}

pub fn abi_execute_check(c: &mut Cpu) {
    let a: Vec<u32> = (0..8).map(|i| abi::arg(c, i)).collect();
    let due = execute_check(c, a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32, a[4], a[5], a[6], a[7] as i32);
    c.r[2] = due as u32;
}

/// `Sched_CreateNewSchedule` (SCHEDULE.CPP, 0x800b6fc4): a schedule for `max` functions,
/// reserved as `name`, empty.
pub fn create_new_schedule(c: &mut Cpu, name: u32, max: i32) -> SchedSchedule {
    abi::frame(c, 24, |c, _| {
        let mut r = abi::Saved::of(c);
        r.0[0] = max as u32;
        let size = (max as u32) << 4;
        let s = SchedSchedule(r.call(c, nfs4rc::f_800e5f28, &[name, size.wrapping_add(24), 0], 0x800b_6fe4)); // reservememadr
        let m = &mut c.m;
        s.set_max_num_functions(m, max);
        s.set_num_functions(m, 0);
        for i in 0..max.max(0) as u32 {
            let f = s.func(0).at(i);
            f.set_priority(m, EMPTY);
            f.set_function(m, 0);
        }
        s
    })
}

pub fn abi_create_new_schedule(c: &mut Cpu) {
    c.r[2] = create_new_schedule(c, c.r[4], c.r[5] as i32).0;
}

/// `Sched_CleanUpSchedule` (SCHEDULE.CPP, 0x800b7034): the schedule's memory freed.
pub fn clean_up_schedule(c: &mut Cpu, s: SchedSchedule) {
    abi::frame(c, 24, |c, _| {
        abi::Saved::of(c).call(c, nfs4rc::f_800e612c, &[s.0], 0x800b_7044); // purgememadr
    });
}

pub fn abi_clean_up_schedule(c: &mut Cpu) {
    clean_up_schedule(c, SchedSchedule(c.r[4]));
}

/// `Sched_AddFunction` (SCHEDULE.CPP, 0x800b7054): `function(var)` added to the schedule
/// before the first with a later `priority` (the empty slots' come after all). There is no
/// check for room.
pub fn add_function(m: &mut ps1rt::Mem, s: SchedSchedule, function: u32, var: u32, priority: i32) {
    s.update_num_functions(m, |n| n.wrapping_add(1));
    let n = s.num_functions(m);
    let entry = |i: i32| s.func(0).at(i as u32);
    let mut at = 0;
    for i in 0..n {
        at = i;
        if priority < entry(i).priority(m) {
            // the rest moved up a slot
            for k in (i + 1..n).rev() {
                for w in 0..4 {
                    let v = m.r32(entry(k - 1).0 + 4 * w);
                    m.w32(entry(k).0 + 4 * w, v);
                }
            }
            break;
        }
    }
    let e = entry(at);
    e.set_priority(m, priority);
    e.set_function(m, function);
    e.set_var1(m, var);
}

pub fn abi_add_function(c: &mut Cpu) {
    add_function(&mut c.m, SchedSchedule(c.r[4]), c.r[5], c.r[6], c.r[7] as i32);
}

/// `Sched_DeleteFunction` (SCHEDULE.CPP, 0x800b7108): `function(var)` taken off the schedule,
/// the rest moved down. When it isn't there, the count still drops (the last function is
/// lost); either way the slot past the old last is marked empty.
pub fn delete_function(m: &mut ps1rt::Mem, s: SchedSchedule, function: u32, var: u32) {
    let n = s.num_functions(m);
    if n == 0 {
        return;
    }
    let entry = |i: i32| s.func(0).at(i as u32);
    for i in 0..n {
        if entry(i).function(m) == function && entry(i).var1(m) == var {
            let mut k = i;
            while k < s.num_functions(m).wrapping_sub(1) {
                for w in 0..4 {
                    let v = m.r32(entry(k + 1).0 + 4 * w);
                    m.w32(entry(k).0 + 4 * w, v);
                }
                k += 1;
            }
            break;
        }
    }
    let n = s.num_functions(m);
    entry(n).set_priority(m, EMPTY);
    s.set_num_functions(m, n.wrapping_sub(1));
}

pub fn abi_delete_function(c: &mut Cpu) {
    delete_function(&mut c.m, SchedSchedule(c.r[4]), c.r[5], c.r[6]);
}

/// `Sched_Execute` (SCHEDULE.CPP, 0x800b71d4): the schedule's functions run in order, each
/// with its argument (the count read anew after each, as they may change the schedule).
pub fn execute(c: &mut Cpu, s: SchedSchedule) {
    abi::frame(c, 32, |c, _| {
        // s2 the schedule, s1 the index, s0 its entry
        let mut r = abi::Saved::of(c);
        r.0[2] = s.0;
        let mut i = 0;
        while i < s.num_functions(&mut c.m) {
            let e = s.func(0).at(i as u32);
            (r.0[0], r.0[1]) = (e.0.wrapping_sub(8), i as u32);
            let f = e.function(&mut c.m);
            if f != 0 {
                let var = e.var1(&mut c.m);
                r.call_ptr(c, f, &[var], 0x800b_721c);
            }
            i += 1;
        }
    });
}

pub fn abi_execute(c: &mut Cpu) {
    execute(c, SchedSchedule(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::SIM_GLOBAL;
    use crate::test_util::{Rng, against_original};

    /// Where the tests' schedule is built (above the dead stack the harness skips: room for
    /// 13 functions).
    const SCHED: u32 = 0x801F_FF20;
    /// Where `Sched_ExecuteCheck`'s results go.
    const OUT: u32 = 0x801F_FF80;

    /// Any tick (the first 16 more often).
    fn shuffle_tick(c: &mut Cpu, rng: &mut Rng) {
        let t = [rng.next() as i32 & 0xFFFF, (rng.next() % 20) as i32, rng.any()][(rng.next() % 3) as usize];
        SIM_COUNTER.set(&mut c.m, t);
    }

    /// A schedule of up to 12 functions (some empty, some alike), in order of priority or
    /// not, with its empty slots after them.
    fn shuffle_schedule(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let n = (rng.next() % 13) as i32;
        let s = SchedSchedule(SCHED);
        s.set_max_num_functions(m, 16);
        s.set_num_functions(m, n);
        for i in 0..13 {
            let e = s.func(0).at(i);
            let p = if (i as i32) < n { (rng.next() % 8) as i32 * 10 } else { EMPTY };
            e.set_priority(m, p);
            e.set_function(m, [0, 0x8000_1000, 0x8000_2000][(rng.next() % 3) as usize]);
            e.set_var1(m, (rng.next() % 3) as u32);
            e.set_var2(m, rng.next() as u32);
        }
        if rng.next() % 2 == 0 {
            // sorted, as the game keeps them
            let mut v: Vec<[u32; 4]> = (0..n as u32).map(|i| std::array::from_fn(|w| m.r32(s.func(0).at(i).0 + 4 * w as u32))).collect();
            v.sort_by_key(|e| e[0] as i32);
            for (i, e) in v.iter().enumerate() {
                for (w, x) in e.iter().enumerate() {
                    m.w32(s.func(0).at(i as u32).0 + 4 * w as u32, *x);
                }
            }
        }
    }

    #[test]
    fn execute_check_fuzz() {
        against_original(
            "Sched_ExecuteCheck",
            20_000,
            0x5C4E,
            shuffle_tick,
            nfs4rc::f_800b6e90,
            &|c| {
                let mut rng = Rng(c.m.r32(0x8011_F368) as u64 * 0x9E37 + 1);
                let dist = [rng.any(), (rng.next() % 0x180_0000) as i32, 0x10_0000 * (rng.next() % 22) as i32, -(rng.next() as i32 & 0xFF_FFFF)][(rng.next() % 4) as usize];
                let full = [0, 0, 0, 1][(rng.next() % 4) as usize];
                vec![(rng.next() % 4) as u32, (rng.next() % 4) as u32, dist as u32, (rng.next() % 12) as u32, OUT, OUT + 4, OUT + 8, full]
            },
            |c, a| {
                let sp = c.r[29];
                for (i, &v) in a[4..].iter().enumerate() {
                    c.m.w32(sp + 16 + 4 * i as u32, v);
                }
                Some(execute_check(c, a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32, a[4], a[5], a[6], a[7] as i32) as u32)
            },
        );
    }

    #[test]
    fn add_delete_fuzz() {
        let args = |c: &mut Cpu| {
            let t = c.m.r32(SCHED + 8);
            vec![SCHED, [0, 0x8000_1000, 0x8000_2000][(t % 3) as usize], t % 3, (t >> 8) % 90]
        };
        against_original("Sched_AddFunction", 10_000, 0xADD, shuffle_schedule, nfs4rc::f_800b7054, &args, |c, a| {
            add_function(&mut c.m, SchedSchedule(a[0]), a[1], a[2], a[3] as i32);
            None
        });
        against_original("Sched_DeleteFunction", 10_000, 0xDE1, shuffle_schedule, nfs4rc::f_800b7108, &args, |c, a| {
            delete_function(&mut c.m, SchedSchedule(a[0]), a[1], a[2]);
            None
        });
    }

    /// The race's schedules run, the cars as `cars::tests::shuffle` leaves them.
    #[test]
    fn execute_fuzz() {
        for (k, at) in [0x8011_F370u32, 0x8011_F374, 0x8011_F378].into_iter().enumerate() {
            let _ = SIM_GLOBAL;
            against_original("Sched_Execute", 300, 0xE7 + k as u64, crate::cars::tests::shuffle, nfs4rc::f_800b71d4, &|c| vec![c.m.r32(at)], |c, a| {
                execute(c, SchedSchedule(a[0]));
                None
            });
        }
    }
}
