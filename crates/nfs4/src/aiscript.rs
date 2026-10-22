//! AISCRIPT.CPP: an AI car's scripted reactions to what a human player does to it. An action
//! (of 7 kinds; 7 none) a human submits is taken up on the next tick: the script's details for
//! it give up to four reactions in turn (bits, 1 none), each for a time; the AI asks which are
//! on (`DoReAction`) and gets the human's index.

use crate::types::AIScript;
use ps1rt::{Cpu, Mem};

/// `AIScript_t.actionIndex`, `detectAction`: none.
const NO_ACTION: i32 = 7;
/// `AIScript_t.reaction`: no reaction (its bit 0).
const NO_REACTION: i32 = 1;

/// The script's details (`AIScript_tReactionDetails[7]`, a pointer at +0x1c: 4 reactions and
/// their half seconds each).
fn details(m: &mut Mem, s: AIScript) -> u32 {
    m.r32(s.0.wrapping_add(0x1c))
}

/// `&lastReactionIndex[action]` (the action is not checked).
fn last_reaction_at(s: AIScript, action: i32) -> u32 {
    s.0.wrapping_add(0x20).wrapping_add((action as u32).wrapping_mul(4))
}

/// `AIScript_Assign` (AISCRIPT.CPP, 0x8006fef8): the script's details, and it started.
pub fn assign(m: &mut Mem, s: AIScript, data: u32) {
    m.w32(s.0.wrapping_add(0x1c), data);
    startup(m, s);
}

pub fn abi_assign(c: &mut Cpu) {
    assign(&mut c.m, AIScript(c.r[4]), c.r[5]);
}

/// `AIScript_ClearLastReactionIndex` (AISCRIPT.CPP, 0x8006ff18): each action's reactions from
/// the first again.
pub fn clear_last_reaction_index(m: &mut Mem, s: AIScript) {
    for i in (0..7).rev() {
        s.set_last_reaction_index(m, i, -1);
    }
}

pub fn abi_clear_last_reaction_index(c: &mut Cpu) {
    clear_last_reaction_index(&mut c.m, AIScript(c.r[4]));
}

/// `AIScript_Startup` (AISCRIPT.CPP, 0x8006ff3c): no action, no reaction.
pub fn startup(m: &mut Mem, s: AIScript) {
    s.set_action_index(m, NO_ACTION);
    s.set_detect_action(m, NO_ACTION);
    s.set_reaction(m, NO_REACTION);
    clear_last_reaction_index(m, s);
    s.set_last_action_time(m, 0);
}

pub fn abi_startup(c: &mut Cpu) {
    startup(&mut c.m, AIScript(c.r[4]));
}

/// `AIScript_Cleanup` (AISCRIPT.CPP, 0x8006ff78): nothing.
pub fn abi_cleanup(_: &mut Cpu) {}

/// `AIScript_SubmitPlayerAction` (AISCRIPT.CPP, 0x8006ff80): human `human` did `action` to the
/// car at `time`: taken up if nothing is under way and it ranks before what is already waiting
/// (the reactions start over after 20 s without one).
pub fn submit_player_action(m: &mut Mem, s: AIScript, human: i32, action: i32, time: i32) {
    if s.last_action_time(m) < time.wrapping_sub(1280) {
        clear_last_reaction_index(m, s);
    }
    s.set_last_action_time(m, time);
    if s.action_index(m) == NO_ACTION && action < s.detect_action(m) {
        s.set_detect_action(m, action);
        s.set_detect_hum_car_index(m, human);
    }
}

pub fn abi_submit_player_action(c: &mut Cpu) {
    submit_player_action(&mut c.m, AIScript(c.r[4]), c.r[5] as i32, c.r[6] as i32, c.r[7] as i32);
}

/// `AIScript_ProcessActionsAndReactions` (AISCRIPT.CPP, 0x80070008, each tick): a waiting
/// action taken up: its next reactions (those the script has after the last given for it) on
/// until one that lasts, which runs for its time (an action with none ends at once); then the
/// reaction's time counted down by `elapsed`, the action over when it runs out.
pub fn process_actions_and_reactions(m: &mut Mem, s: AIScript, elapsed: i32) {
    let data = details(m, s);
    if s.action_index(m) == NO_ACTION {
        let detected = s.detect_action(m);
        if detected == NO_ACTION {
            return;
        }
        s.set_action_index(m, detected);
        s.set_detect_action(m, NO_ACTION);
        s.set_reaction_ticks_left(m, 0);
        let human = s.detect_hum_car_index(m);
        s.set_action_hum_car_index(m, human);
        let last = last_reaction_at(s, s.action_index(m));
        s.set_reaction(m, NO_REACTION);
        let first = m.r32(last) as i32;
        s.set_reaction_index(m, first);
        let mut more = true;
        // (the details of `action`, reaction `i`, and its duration)
        let reaction_of = |m: &mut Mem, action: i32, i: i32, at: u32| {
            m.r8(data.wrapping_add((action as u32).wrapping_mul(8)).wrapping_add(i as u32).wrapping_add(at)) as u32
        };
        while more {
            let next = s.reaction_index(m).wrapping_add(1);
            if next < 4 {
                let action = s.action_index(m);
                let r = reaction_of(m, action, next, 0);
                if 1u32.wrapping_shl(r) != 2 {
                    s.set_reaction_index(m, next);
                    let v = m.r32(last).wrapping_add(1);
                    m.w32(last, v);
                }
            }
            let (action, i) = (s.action_index(m), s.reaction_index(m));
            let r = reaction_of(m, action, i, 0);
            s.update_reaction(m, |v| v | 1u32.wrapping_shl(r) as i32);
            let (action, i) = (s.action_index(m), s.reaction_index(m));
            let half_seconds = reaction_of(m, action, i, 4);
            if half_seconds != 0 {
                s.set_reaction_ticks_left(m, (half_seconds << 4) as i32);
                more = false;
            }
            if s.reaction(m) == NO_REACTION {
                s.set_action_index(m, NO_ACTION);
                s.set_detect_action(m, NO_ACTION);
            }
        }
        if s.action_index(m) == NO_ACTION {
            return;
        }
    }
    let left = s.reaction_ticks_left(m);
    if left > 0 {
        s.set_reaction_ticks_left(m, left.wrapping_sub(elapsed));
    } else {
        s.set_action_index(m, NO_ACTION);
        s.set_detect_action(m, NO_ACTION);
    }
}

pub fn abi_process_actions_and_reactions(c: &mut Cpu) {
    process_actions_and_reactions(&mut c.m, AIScript(c.r[4]), c.r[5] as i32);
}

/// `AIScript_DoReAction` (AISCRIPT.CPP, 0x80070170): the human the car reacts to when any of
/// `reactions` is on, else -1.
pub fn do_re_action(m: &mut Mem, s: AIScript, reactions: i32) -> i32 {
    if s.action_index(m) == NO_ACTION || reactions & s.reaction(m) == 0 {
        return -1;
    }
    s.action_hum_car_index(m)
}

pub fn abi_do_re_action(c: &mut Cpu) {
    c.r[2] = do_re_action(&mut c.m, AIScript(c.r[4]), c.r[5] as i32) as u32;
}

/// `AIScript_GetReactionTicksLeft` (AISCRIPT.CPP, 0x800701a0).
pub fn get_reaction_ticks_left(m: &mut Mem, s: AIScript) -> i32 {
    if s.action_index(m) == NO_ACTION { 0 } else { s.reaction_ticks_left(m) }
}

pub fn abi_get_reaction_ticks_left(c: &mut Cpu) {
    c.r[2] = get_reaction_ticks_left(&mut c.m, AIScript(c.r[4])) as u32;
}
