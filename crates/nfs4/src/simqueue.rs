//! SIMQUEUE.CPP: the players' pad input queued for the simulation ticks (a few ticks ahead,
//! for linked games), and handed out a tick at a time.

use crate::globals::{GAME_SETUP, INPUT_QUEUE, MAX_TICKS_PER_FRAME, QUEUE_OUTPUT, SIM_QUEUE_BLOCK_OTHER, SIM_QUEUE_BLOCK_SELF, SIM_QUEUE_TICKER};
use crate::types::InputResults;
use ps1rt::{Cpu, Mem};

/// Ticks of input each player's queue holds.
const SLOTS: u32 = 32;
/// The ticks queued ahead from the start (empty input).
const LEAD: i32 = 4;

/// A linked or local game (`GameSetup_tData.commMode` 0 or 1), where the queue runs ahead.
fn local_or_linked(m: &mut Mem) -> bool {
    matches!(GAME_SETUP.comm_mode(m), 0 | 1)
}

fn copy(m: &mut Mem, from: InputResults, to: InputResults) {
    for i in 0..InputResults::SIZE {
        let b = m.r8(from.0.wrapping_add(i));
        m.w8(to.0.wrapping_add(i), b);
    }
}

/// `SimQueue_StartUp` (SIMQUEUE.CPP, 0x800b7fc0): the queue blocked until reset, and its lead.
pub fn start_up(m: &mut Mem) {
    SIM_QUEUE_BLOCK_SELF.set(m, 1);
    SIM_QUEUE_BLOCK_OTHER.set(m, 1);
    set_lag(m);
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(&mut c.m);
}

/// `SimQueue_Reset` (SIMQUEUE.CPP, 0x800b7fec): both players' queues empty but for the first
/// ticks' empty input, the head at tick 0.
pub fn reset(m: &mut Mem) {
    SIM_QUEUE_BLOCK_SELF.set(m, 1);
    SIM_QUEUE_BLOCK_OTHER.set(m, 1);
    for player in 0..2 {
        for k in 0..SLOTS {
            let i = player * SLOTS + k;
            INPUT_QUEUE.set_validity(m, i, ((k as i32) < LEAD) as i32);
            m.w32(INPUT_QUEUE.buffer(i).0, 0);
        }
        INPUT_QUEUE.set_tail_time(m, player, LEAD);
    }
    INPUT_QUEUE.set_head_time(m, 0);
    SIM_QUEUE_TICKER.set(m, 3);
    SIM_QUEUE_BLOCK_OTHER.set(m, 0);
    SIM_QUEUE_BLOCK_SELF.set(m, 0);
}

pub fn abi_reset(c: &mut Cpu) {
    reset(&mut c.m);
}

/// `SimQueue_CleanUp` (SIMQUEUE.CPP, 0x800b808c): the queue blocked.
pub fn clean_up(m: &mut Mem) {
    SIM_QUEUE_BLOCK_SELF.set(m, 1);
    SIM_QUEUE_BLOCK_OTHER.set(m, 1);
}

pub fn abi_clean_up(c: &mut Cpu) {
    clean_up(&mut c.m);
}

/// `SimQueue_Put` (SIMQUEUE.CPP, 0x800b80a0): a tick of `player`'s input put at the end of
/// its queue, unless the queue is as far ahead as it may be or that slot isn't used yet;
/// whether it was. The ticker counts the ticks put by the player who isn't the link's
/// master (any player but the first when alone).
pub fn put(m: &mut Mem, player: i32, input: InputResults) -> bool {
    let slot = INPUT_QUEUE.tail_time(m, player as u32) as u32 & (SLOTS - 1);
    if is_blocking(m, player) {
        return false;
    }
    let i = (player as u32).wrapping_mul(SLOTS).wrapping_add(slot);
    if INPUT_QUEUE.validity(m, i) != 0 {
        return false;
    }
    copy(m, input, INPUT_QUEUE.buffer(i));
    INPUT_QUEUE.set_validity(m, i, 1);
    INPUT_QUEUE.update_tail_time(m, player as u32, |t| t.wrapping_add(1));
    let counted = match GAME_SETUP.comm_mode(m) {
        0 => GAME_SETUP.num_player_race_cars(m) < 2 || player != 0,
        1 => player != 0,
        _ => false,
    };
    if counted {
        SIM_QUEUE_TICKER.update(m, |t| t.wrapping_add(1));
    }
    true
}

pub fn abi_put(c: &mut Cpu) {
    c.r[2] = put(&mut c.m, c.r[4] as i32, InputResults(c.r[5])) as u32;
}

/// `SimQueue_SetCurrentInput` (SIMQUEUE.CPP, 0x800b81b8): the players' input for tick `time`
/// taken off their queues (`SimQueue_GetCurrentInput` hands it out), and the head moved on.
pub fn set_current_input(m: &mut Mem, time: i32) {
    let slot = time as u32 & (SLOTS - 1);
    let mut p = 0;
    while p < GAME_SETUP.num_player_race_cars(m) {
        copy(m, INPUT_QUEUE.buffer(p as u32 * SLOTS + slot), QUEUE_OUTPUT.at(p as u32));
        p += 1;
    }
    let mut p = 0;
    while p < GAME_SETUP.num_player_race_cars(m) {
        INPUT_QUEUE.set_validity(m, p as u32 * SLOTS + slot, 0);
        p += 1;
    }
    INPUT_QUEUE.update_head_time(m, |t| t.wrapping_add(1));
}

pub fn abi_set_current_input(c: &mut Cpu) {
    set_current_input(&mut c.m, c.r[4] as i32);
}

/// `SimQueue_GetCurrentInput` (SIMQUEUE.CPP, 0x800b8278): `player`'s input this tick.
pub fn get_current_input(m: &mut Mem, player: i32, out: InputResults) {
    copy(m, QUEUE_OUTPUT.at(player as u32), out);
}

pub fn abi_get_current_input(c: &mut Cpu) {
    get_current_input(&mut c.m, c.r[4] as i32, InputResults(c.r[5]));
}

/// `SimQueue_SetLag` (SIMQUEUE.CPP, 0x800b82a0): how far ahead the queue may run (4 ticks in
/// a local or linked game).
pub fn set_lag(m: &mut Mem) {
    if local_or_linked(m) {
        MAX_TICKS_PER_FRAME.set(m, 4);
    }
}

pub fn abi_set_lag(c: &mut Cpu) {
    set_lag(&mut c.m);
}

/// `SimQueue_IsBlocking` (SIMQUEUE.CPP, 0x800b82cc): the first player's queue is as far ahead
/// of the head as it may run (in a local or linked game; the others' never are).
pub fn is_blocking(m: &mut Mem, player: i32) -> bool {
    if player != 0 || !local_or_linked(m) {
        return false;
    }
    let limit = INPUT_QUEUE.head_time(m).wrapping_add(MAX_TICKS_PER_FRAME.get(m));
    INPUT_QUEUE.tail_time(m, 0) >= limit
}

pub fn abi_is_blocking(c: &mut Cpu) {
    c.r[2] = is_blocking(&mut c.m, c.r[4] as i32) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original};

    /// Where the tests' input comes from and goes to (above the dead stack the harness skips).
    const IO: u32 = 0x801F_FF80;

    /// The queues in any state: slots used or not, the head and tails anywhere (the tails a
    /// few ticks ahead), one player or two, any link mode and lag; some input to put.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        for i in 0..2 * SLOTS {
            INPUT_QUEUE.set_validity(m, i, [0, 0, 1][pick(3) as usize]);
            m.w32(INPUT_QUEUE.buffer(i).0, pick(i32::MAX as u32) as u32);
        }
        let head = [pick(100), pick(0x7FFF_FFF0), 0][pick(3) as usize];
        INPUT_QUEUE.set_head_time(m, head);
        for p in 0..2 {
            INPUT_QUEUE.set_tail_time(m, p, head.wrapping_add(pick(9) - 2));
        }
        GAME_SETUP.set_comm_mode(m, [0, 0, 1, 2, 3][pick(5) as usize]);
        GAME_SETUP.set_num_player_race_cars(m, 1 + pick(2));
        MAX_TICKS_PER_FRAME.set(m, [4, 1, 2, 8][pick(4) as usize]);
        SIM_QUEUE_TICKER.set(m, pick(1000));
        m.w32(IO, pick(i32::MAX as u32) as u32);
        m.w32(IO + 4, pick(i32::MAX as u32) as u32);
    }

    fn player(c: &mut Cpu) -> u32 {
        c.m.r32(IO) % 3
    }

    #[test]
    fn queue_fuzz() {
        against_original("SimQueue_Put", 20_000, 0x9017, shuffle, nfs4rc::f_800b80a0, &|c| vec![player(c), IO + 4], |c, a| Some(put(&mut c.m, a[0] as i32, InputResults(a[1])) as u32));
        against_original("SimQueue_IsBlocking", 5_000, 0xB10C, shuffle, nfs4rc::f_800b82cc, &|c| vec![player(c)], |c, a| Some(is_blocking(&mut c.m, a[0] as i32) as u32));
        against_original("SimQueue_SetCurrentInput", 5_000, 0x5E7, shuffle, nfs4rc::f_800b81b8, &|c| vec![c.m.r32(IO + 4)], |c, a| {
            set_current_input(&mut c.m, a[0] as i32);
            None
        });
        against_original("SimQueue_GetCurrentInput", 2_000, 0x6E7, shuffle, nfs4rc::f_800b8278, &|c| vec![player(c) & 1, IO + 4], |c, a| {
            get_current_input(&mut c.m, a[0] as i32, InputResults(a[1]));
            None
        });
        let plain: [(&str, fn(&mut Cpu), fn(&mut Cpu, &[u32]) -> Option<u32>); 4] = [
            ("SimQueue_StartUp", nfs4rc::f_800b7fc0, |c, _| {
                start_up(&mut c.m);
                None
            }),
            ("SimQueue_Reset", nfs4rc::f_800b7fec, |c, _| {
                reset(&mut c.m);
                None
            }),
            ("SimQueue_CleanUp", nfs4rc::f_800b808c, |c, _| {
                clean_up(&mut c.m);
                None
            }),
            ("SimQueue_SetLag", nfs4rc::f_800b82a0, |c, _| {
                set_lag(&mut c.m);
                None
            }),
        ];
        for (name, original, port) in plain {
            against_original(name, 500, 0x51, shuffle, original, &|_| vec![], port);
        }
    }
}
