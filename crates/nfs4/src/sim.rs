//! SIM.CPP: the race's simulation: setting it up, the main loop (input, pausing, catching up
//! with the clock in ticks, drawing), and each tick's schedules.

use crate::abi::{self, Saved};
use crate::globals::{
    CLOCK_FRAMES, CLOCK_TICKS, COUNTDOWN, COUNTDOWN_TICKS, FORCE_PAUSE, GAME_SETUP, HUD_ACTIVATE_CD_PLAYER, HUMAN_RACE_CARS, IN_BETWEEN, INPUT_TIME, LOOK_BEHIND, MASTER_SFX_LEVEL,
    NUM_HUMAN_RACE_CARS, PAUSE_PORT_INDEX, QUIT_TYPE, REPLAY_INTERFACE, REPLAY_MODE, SIM_GLOBAL, SIM_QUEUE_BLOCK_SELF, SIM_QUEUE_TICKER, SIM_VAR, SKIP_RENDER, STACK_SPEED_UP_ENABLED,
    UNPAUSE_DELAY, W_SAVE_PTR,
};
use crate::types::Car;
use crate::{cars, collide, schedule, simqueue, stats};
use ps1rt::{Cpu, Mem};

/// The top of the scratchpad, where the schedules run (`SetSp`).
const SCRATCH_STACK: u32 = 0x1F80_03FC;
/// `Car_tObj.carFlags`: a human playing the police (car class 0x40, set up with the car):
/// in a pursuit there is no countdown.
const CAR_HUMAN_COP: i32 = 0x200;
/// The camera's per-tick function (CAMERA.CPP), on the 64 Hz schedule.
const CAMERA_TICK: u32 = 0x8008_3dfc;
/// The schedules' names ("Sc32-1", "Sc32-2", "Sc64" in the executable).
const SCHEDULE_NAMES: [u32; 3] = [0x8013_E6F4, 0x8013_E6FC, 0x8013_E704];
/// `Replay_ReplayMode`: watching a replay, the attract mode.
const REPLAY_WATCH: i32 = 2;
const REPLAY_ATTRACT: i32 = 3;

/// `SetSp`: the stack moved to `to`; the stack pointer it was at.
fn set_sp(c: &mut Cpu, to: u32) -> u32 {
    std::mem::replace(&mut c.r[29], to)
}

/// The schedules' work on the scratchpad stack (`SetSp` there, the old stack pointer in
/// `gWSavePtr`, `stackSpeedUpEnbabledFlag` set), and back.
fn on_scratchpad<R>(c: &mut Cpu, f: impl FnOnce(&mut Cpu) -> R) -> R {
    let old = set_sp(c, SCRATCH_STACK);
    W_SAVE_PTR.set(&mut c.m, old);
    STACK_SPEED_UP_ENABLED.set(&mut c.m, 1);
    let v = f(c);
    let back = W_SAVE_PTR.get(&mut c.m);
    let scratch = set_sp(c, back);
    W_SAVE_PTR.set(&mut c.m, scratch);
    STACK_SPEED_UP_ENABLED.set(&mut c.m, 0);
    v
}

/// `Sim_FadeInSFX` (SIM.CPP, 0x800b74cc): the sound effects silent on the first tick (their
/// level kept in the user settings), faded in over ticks 17..80 (a quarter as loud in a
/// replay's stats screen).
pub fn fade_in_sfx(m: &mut Mem) {
    let ticks = SIM_GLOBAL.game_ticks(m);
    let setting = GAME_SETUP.user_setting();
    if ticks == 0 {
        let level = MASTER_SFX_LEVEL.get(m);
        MASTER_SFX_LEVEL.set(m, 0);
        setting.set_sfx_level(m, level);
        return;
    }
    if (ticks.wrapping_sub(17) as u32) < 64 {
        let level = setting.sfx_level(m);
        let level = if REPLAY_INTERFACE.stats_screen(m) != 0 { level >> 2 } else { level };
        MASTER_SFX_LEVEL.set(m, level.wrapping_mul(ticks.wrapping_sub(16)) >> 6);
    }
}

pub fn abi_fade_in_sfx(c: &mut Cpu) {
    fade_in_sfx(&mut c.m);
}

/// `Sim_ProcessSimSchedules` (SIM.CPP, 0x800b754c): one 64 Hz tick. Before the start, the
/// countdown's beeps (loaded ahead, played on their ticks; the race starts with the last, at
/// 512 ticks or later; straight away in a pursuit where a player is the police) and the sounds fading
/// in. Then, on the scratchpad stack: every other tick the 32 Hz schedule (after clearing the
/// race order and the collisions; the cars' bookkeeping after), and the accident scenes; the
/// 64 Hz schedule; on the other ticks the AI, the second 32 Hz schedule, the race's end and
/// the catch-up glue, and the cars' sounds.
pub fn process_sim_schedules(c: &mut Cpu) {
    abi::frame(c, 40, |c, _| {
        // s0 the countdown beep, then simGlobal; s1 35 (the first beep's sound), then 1
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let pursuit = matches!(GAME_SETUP.race_type(m), 1 | 5);
        let cop = |m: &mut Mem, i: u32| Car(m.r32(HUMAN_RACE_CARS + 4 * i)).car_flags(m) & CAR_HUMAN_COP != 0;
        if pursuit && (cop(m, 0) || (NUM_HUMAN_RACE_CARS.get(m) == 2 && cop(m, 1))) {
            fade_in_sfx(m);
            SIM_GLOBAL.set_game_started(m, 1);
        } else if SIM_GLOBAL.game_started(m) == 0 {
            let first = (m.r8(COUNTDOWN) as i32 - 1).max(0);
            if first < 4 {
                r.0[1] = 35;
            }
            for beep in first..4 {
                r.0[0] = beep as u32;
                r.call(c, nfs4rc::f_80077300, &[2, beep as u32 + 35, 0], 0x800b_7614); // AudioCmn_GetAsyncSfx
            }
            r.0[0] = first.max(4) as u32;
            let m = &mut c.m;
            let beep = m.r8(COUNTDOWN) as u32;
            let ticks = SIM_GLOBAL.game_ticks(m);
            // (`counter[4]`, past the last beep, is simGlobal.gameStarted: 0)
            if ticks >= m.r32(COUNTDOWN_TICKS + 4 * beep) as i32 {
                if ticks >= 512 {
                    SIM_GLOBAL.set_game_started(m, 1);
                }
                r.call(c, nfs4rc::f_80078d5c, &[-4i32 as u32, beep + 35, 0, 127, 64], 0x800b_7684); // AudioCmn_PlaySound
                let m = &mut c.m;
                let next = m.r8(COUNTDOWN).wrapping_add(1);
                m.w8(COUNTDOWN, next);
            }
            fade_in_sfx(&mut c.m);
        }
        r.0[0] = SIM_GLOBAL.0;
        if SIM_GLOBAL.game_ticks(&mut c.m) & 1 == 0 {
            r.call_ptr(c, 0x800e_77f0, &[0], 0x800b_76e4); // systemtask
            let s32 = SIM_GLOBAL.schedule32_hz(&mut c.m);
            on_scratchpad(c, |c| {
                stats::clear_position(&mut c.m);
                r.enter(c, 0x8009_1fc4, 0x800b_7714, |c| collide::clear_collision_registry(c));
                r.enter(c, 0x800b_71d4, 0x800b_7720, |c| schedule::execute(c, s32));
                r.enter(c, 0x8008_ba88, 0x800b_7728, |c| cars::manage_bureaucracy(c));
            });
            r.enter(c, 0x8008_bc50, 0x800b_7750, |c| cars::check_for_accident_scenes(c));
        }
        let s64 = SIM_GLOBAL.schedule64_hz(&mut c.m);
        on_scratchpad(c, |c| {
            r.0[1] = 1;
            r.enter(c, 0x800b_71d4, 0x800b_777c, |c| schedule::execute(c, s64));
        });
        if SIM_GLOBAL.game_ticks(&mut c.m) & 1 != 0 {
            on_scratchpad(c, |c| {
                r.call(c, nfs4rc::f_8005ba00, &[], 0x800b_77cc); // AIHigh_Execute
            });
            let s32b = SIM_GLOBAL.schedule32_hz2(&mut c.m);
            on_scratchpad(c, |c| {
                r.enter(c, 0x800b_71d4, 0x800b_7814, |c| schedule::execute(c, s32b));
                r.enter(c, 0x800b_9a80, 0x800b_781c, |c| stats::track_end_game(c));
                r.enter(c, 0x800b_8c2c, 0x800b_7824, |c| stats::do_player_glue(c));
            });
            r.call(c, nfs4rc::f_80076b90, &[], 0x800b_784c); // AudioClc_SoundCars
        }
        SIM_GLOBAL.update_game_ticks(&mut c.m, |t| t.wrapping_add(1));
    });
}

pub fn abi_process_sim_schedules(c: &mut Cpu) {
    process_sim_schedules(c);
}

/// `Sim_ProcessPause` (SIM.CPP, 0x800b7870): the pause menu (once its input delay is over):
/// carry on (the sound and force feedback back, the input unblocked), restart, or quit (the
/// opponents' times extrapolated; 5 forfeits a pink slip when a second pad paused); then the
/// menu closed.
pub fn process_pause(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if CLOCK_FRAMES.get(m) < UNPAUSE_DELAY.get(m) {
            return;
        }
        let mut r = Saved::of(c);
        r.0[0] = 1;
        let choice = r.call(c, nfs4rc::f_8009f290, &[], 0x800b_7898); // MPause_Logic
        match choice {
            1 => {
                r.0[0] = SIM_VAR.0;
                if SIM_VAR.quick_pause_sim(&mut c.m) == 0 {
                    r.call(c, nfs4rc::f_8007a6a0, &[], 0x800b_78c4); // AudioCmn_UnPause
                }
                SIM_QUEUE_BLOCK_SELF.set(&mut c.m, 0);
                SIM_VAR.set_pause_sim(&mut c.m, 0);
                r.call(c, nfs4rc::f_800cbf54, &[], 0x800b_78d4); // Force_UnPause
            }
            2 => {
                r.call(c, nfs4rc::f_8007a7f8, &[], 0x800b_78ec); // AudioCmn_UnPauseAndRestart
                let m = &mut c.m;
                SIM_VAR.set_pause_sim(m, 1);
                SIM_VAR.set_end_sim_game(m, 1);
                SIM_VAR.set_restart_game(m, 1);
            }
            4 | 5 => {
                if choice == 5 {
                    let second = (c.m.r8(PAUSE_PORT_INDEX) != 0) as i32;
                    GAME_SETUP.set_pink_slips_forfeit(&mut c.m, second);
                }
                r.call(c, nfs4rc::f_800da434, &[], 0x800b_7934); // Hud_BTC_QuitOut
                r.call(c, nfs4rc::f_8007a7f8, &[], 0x800b_793c); // AudioCmn_UnPauseAndRestart
                let m = &mut c.m;
                SIM_VAR.set_pause_sim(m, 0);
                SIM_VAR.set_end_sim_game(m, 1);
                SIM_VAR.set_restart_game(m, 0);
                r.enter(c, 0x800b_92d8, 0x800b_7958, |c| stats::extrapolate_opponent_times(c, 1));
                QUIT_TYPE.set(&mut c.m, 1);
            }
            _ => return,
        }
        r.call(c, nfs4rc::f_8009f9bc, &[], 0x800b_7964); // MPause_EndPauseMenu
    });
}

pub fn abi_process_pause(c: &mut Cpu) {
    process_pause(c);
}

/// `Sim_CheckForPause` (SIM.CPP, 0x800b7974): pause when racing (after the first half
/// second, not while restarting) and `pad`'s pause button is pressed (when `pad` is set) or a
/// pause is forced: the menu takes input four frames on, the sound, force feedback and input
/// stopped.
pub fn check_for_pause(c: &mut Cpu, pad: i32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        if REPLAY_MODE.get(m) >= 2 || SIM_VAR.restart_game(m) != 0 || SIM_GLOBAL.game_ticks(m) < 33 {
            return;
        }
        let r = Saved::of(c);
        let pressed = pad != 0 && r.call(c, nfs4rc::f_8009dddc, &[6, 1], 0x800b_79c4) != 0; // Input_Interface
        if !pressed && FORCE_PAUSE.get(&mut c.m) == 0 {
            return;
        }
        let m = &mut c.m;
        let at = CLOCK_FRAMES.get(m).wrapping_add(4);
        UNPAUSE_DELAY.set(m, at);
        r.call(c, nfs4rc::f_8007a5e8, &[], 0x800b_79f8); // AudioCmn_Pause
        SIM_QUEUE_BLOCK_SELF.set(&mut c.m, 1);
        SIM_VAR.set_pause_sim(&mut c.m, 1);
        r.call(c, nfs4rc::f_800cbf14, &[], 0x800b_7a10); // Force_Pause
    });
}

pub fn abi_check_for_pause(c: &mut Cpu) {
    check_for_pause(c, c.r[4] as i32);
}

/// `Sim_StartUp` (SIM.CPP, 0x800b7248): a race's simulation set up: its state (restarting,
/// tick 0), the schedules (32 Hz for 70 functions, the second 32 Hz for 11, 64 Hz for 26),
/// the random numbers from the race's seed, the police, the AI, the cars and their drawing
/// and sounds (the loading screen moved on between), and the camera on the 64 Hz schedule.
pub fn start_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        SIM_VAR.set_restart_game(m, 1);
        SIM_VAR.set_end_sim_game(m, 0);
        SIM_VAR.set_pause_sim(m, 0);
        SIM_VAR.set_quick_pause_sim(m, 0);
        SIM_VAR.set_goal_clock_ticks(m, 0);
        SIM_VAR.set_current_clock_ticks(m, 0);
        SIM_VAR.set_key_release(m, 0);
        SIM_GLOBAL.set_game_started(m, 0);
        SIM_GLOBAL.set_game_ticks(m, 0);
        // s0 simGlobal
        let mut r = Saved::of(c);
        r.0[0] = SIM_GLOBAL.0;
        let create = |c: &mut Cpu, k: usize, max: i32, ra: u32| r.enter(c, 0x800b_6fc4, ra, |c| schedule::create_new_schedule(c, SCHEDULE_NAMES[k], max)).0;
        let s = create(c, 0, 70, 0x800b_729c);
        c.m.w32(SIM_GLOBAL.0 + 0x10, s);
        let s = create(c, 1, 11, 0x800b_72b0);
        c.m.w32(SIM_GLOBAL.0 + 0x14, s);
        let s = create(c, 2, 26, 0x800b_72c4);
        c.m.w32(SIM_GLOBAL.0 + 0xC, s);
        let seed = GAME_SETUP.rand_seed(&mut c.m) as u32;
        r.call(c, nfs4rc::f_8009bdfc, &[seed], 0x800b_72d4); // FastRandom_StartUp
        r.call(c, nfs4rc::f_800671ac, &[], 0x800b_72dc); // AICop_StartUp
        r.call(c, nfs4rc::f_800673b0, &[], 0x800b_72e4); // AIInit_StartUp1
        r.call(c, nfs4rc::f_800ae9ac, &[], 0x800b_72ec); // R3DCar_StartUp
        r.call(c, nfs4rc::f_800bccf8, &[], 0x800b_72f4); // CarIO_StartUp
        r.call(c, nfs4rc::f_8008b064, &[], 0x800b_72fc); // Cars_StartUp
        r.call(c, nfs4rc::f_800aea7c, &[], 0x800b_7304); // R3DCar_PostStartUp
        r.call(c, nfs4rc::f_8005b584, &[], 0x800b_730c); // AIHigh_StartUp
        r.call_ptr(c, 0x800d_bdec, &[8], 0x800b_7314); // Loading_UpdateLoadingScreen
        r.call_ptr(c, 0x800d_bdec, &[9], 0x800b_731c);
        r.call(c, nfs4rc::f_80067424, &[], 0x800b_7324); // AIInit_StartUp2
        r.call_ptr(c, 0x800d_bdec, &[10], 0x800b_732c);
        r.call(c, nfs4rc::f_80077474, &[], 0x800b_7334); // AudioCmn_Init
        let s64 = SIM_GLOBAL.schedule64_hz(&mut c.m);
        schedule::add_function(&mut c.m, s64, CAMERA_TICK, 0, 100);
    });
}

pub fn abi_start_up(c: &mut Cpu) {
    start_up(c);
}

/// `Sim_Restart` (SIM.CPP, 0x800b735c): the race from the start again: its state (no longer
/// restarting is left to the caller), the countdown, the arrests, the random numbers, the
/// police, the AI, the cars, the HUD, the track's drawing, the input queue, and the 32 Hz
/// clock at the pad's.
pub fn restart(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        QUIT_TYPE.set(m, 1);
        SIM_VAR.set_end_sim_game(m, 0);
        SIM_VAR.set_pause_sim(m, 0);
        SIM_VAR.set_quick_pause_sim(m, 0);
        SIM_VAR.set_goal_clock_ticks(m, 0);
        SIM_VAR.set_current_clock_ticks(m, 0);
        SIM_GLOBAL.set_game_started(m, 0);
        m.w8(COUNTDOWN, 0);
        SIM_GLOBAL.set_game_ticks(m, 0);
        GAME_SETUP.set_perp_arrests(m, 0);
        let seed = GAME_SETUP.rand_seed(m) as u32;
        // s0 simGlobal
        let mut r = Saved::of(c);
        r.0[0] = SIM_GLOBAL.0;
        r.call(c, nfs4rc::f_8009bdfc, &[seed], 0x800b_73b8); // FastRandom_StartUp
        r.call(c, nfs4rc::f_80067258, &[], 0x800b_73c0); // AICop_Restart
        r.call(c, nfs4rc::f_800674fc, &[], 0x800b_73c8); // AIInit_Reset1
        r.call(c, nfs4rc::f_800aeb60, &[], 0x800b_73d0); // R3DCar_Restart
        r.call(c, nfs4rc::f_8005b92c, &[], 0x800b_73d8); // AIHigh_Restart1
        r.call(c, nfs4rc::f_8008af58, &[], 0x800b_73e0); // Cars_Restart
        r.call(c, nfs4rc::f_8005b94c, &[], 0x800b_73e8); // AIHigh_Restart2
        r.call(c, nfs4rc::f_80067508, &[], 0x800b_73f0); // AIInit_Reset2
        r.call(c, nfs4rc::f_800da3dc, &[], 0x800b_73f8); // Hud_Reset
        r.call(c, nfs4rc::f_800b447c, &[], 0x800b_7400); // Render_RestartTrackRender
        simqueue::reset(&mut c.m);
        let t = INPUT_TIME.get(&mut c.m);
        SIM_GLOBAL.set_time32_hz(&mut c.m, t);
    });
}

pub fn abi_restart(c: &mut Cpu) {
    restart(c);
}

/// `Sim_CleanUp` (SIM.CPP, 0x800b7428): the race's simulation taken down, the reverse of
/// [`start_up`].
pub fn clean_up(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.call(c, nfs4rc::f_800cbe44, &[], 0x800b_7438); // Force_Disable
        simqueue::clean_up(&mut c.m);
        // s0 simGlobal
        r.0[0] = SIM_GLOBAL.0;
        let s64 = SIM_GLOBAL.schedule64_hz(&mut c.m);
        schedule::delete_function(&mut c.m, s64, CAMERA_TICK, 0);
        r.call(c, nfs4rc::f_80067644, &[], 0x800b_7460); // AIInit_CleanUp2
        r.call(c, nfs4rc::f_8005b974, &[], 0x800b_7468); // AIHigh_CleanUp
        r.call(c, nfs4rc::f_8008b4cc, &[], 0x800b_7470); // Cars_CleanUp
        r.call(c, nfs4rc::f_800aebd8, &[], 0x800b_7478); // R3DCar_CleanUp
        r.call(c, nfs4rc::f_800bcd64, &[], 0x800b_7480); // CarIO_CleanUp
        r.call(c, nfs4rc::f_8006760c, &[], 0x800b_7488); // AIInit_CleanUp1
        r.call(c, nfs4rc::f_800672a8, &[], 0x800b_7490); // AICop_CleanUp
        r.call(c, nfs4rc::f_8009bdf4, &[], 0x800b_7498); // FastRandom_CleanUp
        for (at, ra) in [(0x10, 0x800b_74a4), (0x14, 0x800b_74b0), (0xC, 0x800b_74bc)] {
            let s = crate::types::SchedSchedule(c.m.r32(SIM_GLOBAL.0 + at));
            r.enter(c, 0x800b_7034, ra, |c| schedule::clean_up_schedule(c, s));
        }
    });
}

pub fn abi_clean_up(c: &mut Cpu) {
    clean_up(c);
}

/// `Input_Interface` (INPUT.CPP, still recompiled): whether the pad's `key` is pressed (`edge`:
/// newly).
fn input(c: &mut Cpu, r: &Saved, key: u32, edge: u32, ra: u32) -> u32 {
    r.call(c, nfs4rc::f_8009dddc, &[key, edge], ra)
}

/// `Sim_MainGameLoop` (SIM.CPP, 0x800b7a20): the race until it ends. Each time round: the
/// system's task; the ticks to simulate (twice the input queue's, one more when the clock ran
/// ahead of them); then for each 32 Hz tick the pad has been read for (catching up at most
/// 16): a replay's controls (its end, pause), the pause menu, or the music, camera, dashboard,
/// look-behind and pause buttons (both players' in a split screen) once the last key held is
/// let go; then the 64 Hz ticks due (a replay slowed or sped up by its speed, the frames in
/// between only moving the camera), or just the camera while paused, or nothing when ahead;
/// the frame drawn; the exit key (or the attract mode's second) ends it. A race's arrests
/// count at the end.
pub fn main_game_loop(c: &mut Cpu) {
    abi::frame(c, 64, |c, _| {
        // s1 simVar, s3 simGlobal, s4 the replay's controls, fp their upper half; s6 the clock
        // at the last time round (10000 on at first), s7 the tick to reach; s5 and s2
        // GameSetup; s0 a player, or 1 in the ticks
        let mut r = Saved::of(c);
        (r.0[1], r.0[3], r.0[4], r.0[8]) = (SIM_VAR.0, SIM_GLOBAL.0, REPLAY_INTERFACE.0, 0x8012_0000);
        let m = &mut c.m;
        QUIT_TYPE.set(m, 1);
        r.0[6] = CLOCK_TICKS.get(m).wrapping_add(10000) as u32;
        r.0[7] = 0;
        let t = INPUT_TIME.get(m);
        SIM_GLOBAL.set_time32_hz(m, t);
        while SIM_VAR.end_sim_game(&mut c.m) == 0 {
            r.call_ptr(c, 0x800e_77f0, &[0], 0x800b_7aa4); // systemtask
            let m = &mut c.m;
            let now = CLOCK_TICKS.get(m);
            let goal = SIM_QUEUE_TICKER.get(m).wrapping_shl(1);
            SIM_VAR.set_goal_clock_ticks(m, goal);
            if goal.wrapping_sub(r.0[7] as i32) < now.wrapping_sub(r.0[6] as i32) {
                SIM_VAR.set_goal_clock_ticks(m, goal.wrapping_add(1));
            }
            r.0[7] = SIM_VAR.goal_clock_ticks(m) as u32;
            let pad = INPUT_TIME.get(m);
            if pad.wrapping_sub(SIM_GLOBAL.time32_hz(m)) >= 17 {
                SIM_GLOBAL.set_time32_hz(m, pad);
            }
            r.0[6] = now as u32;
            r.0[5] = GAME_SETUP.0;
            while SIM_GLOBAL.time32_hz(&mut c.m) <= INPUT_TIME.get(&mut c.m) && SIM_VAR.end_sim_game(&mut c.m) == 0 {
                input_tick(c, &mut r);
                SIM_GLOBAL.update_time32_hz(&mut c.m, |t| t.wrapping_add(1));
                c.tick();
            }
            let m = &mut c.m;
            SKIP_RENDER.set(m, 0);
            if SIM_VAR.pause_sim(m) != 0 || SIM_VAR.quick_pause_sim(m) != 0 || SIM_VAR.end_sim_game(m) != 0 {
                r.call(c, nfs4rc::f_80083dfc, &[], 0x800b_7e70); // the camera
            } else if SIM_VAR.goal_clock_ticks(m) < SIM_VAR.current_clock_ticks(m) {
                SKIP_RENDER.set(m, 1);
            } else {
                r.0[0] = 1;
                loop {
                    sim_tick(c, &r);
                    let m = &mut c.m;
                    let next = SIM_VAR.current_clock_ticks(m).wrapping_add(1);
                    SIM_VAR.set_current_clock_ticks(m, next);
                    if SIM_VAR.goal_clock_ticks(m) < next {
                        break;
                    }
                    c.tick();
                }
            }
            if SKIP_RENDER.get(&mut c.m) == 0 {
                let paused = SIM_VAR.pause_sim(&mut c.m) as u32;
                r.call(c, nfs4rc::f_800b4170, &[paused], 0x800b_7e8c); // Render_Render
            }
            let exit = r.call(c, nfs4rc::f_8009de80, &[], 0x800b_7e94) != 0; // Input_MainExitKey
            let m = &mut c.m;
            if exit || (REPLAY_MODE.get(m) == REPLAY_ATTRACT && SIM_GLOBAL.game_ticks(m) >= 65) {
                SIM_VAR.set_end_sim_game(m, 1);
            }
            c.tick();
        }
        let m = &mut c.m;
        if REPLAY_MODE.get(m) < REPLAY_WATCH {
            let arrests = GAME_SETUP.perp_arrests(m);
            GAME_SETUP.set_final_perp_arrests(m, arrests);
        }
    });
}

pub fn abi_main_game_loop(c: &mut Cpu) {
    main_game_loop(c);
}

/// A 32 Hz tick's input in [`main_game_loop`].
fn input_tick(c: &mut Cpu, r: &mut Saved) {
    let m = &mut c.m;
    if REPLAY_MODE.get(m) == REPLAY_WATCH && SIM_VAR.pause_sim(m) == 0 {
        r.call(c, nfs4rc::f_800b5d44, &[], 0x800b_7b58); // Replay_GetInterfaceKey
        let m = &mut c.m;
        if REPLAY_INTERFACE.depressed(m) == 4 {
            let end = REPLAY_INTERFACE.end(m);
            REPLAY_INTERFACE.set_depressed(m, 0);
            SIM_VAR.set_end_sim_game(m, end);
            if GAME_SETUP.instant_replay(m) != 0 {
                r.call(c, nfs4rc::f_8007a7f8, &[], 0x800b_7b84); // AudioCmn_UnPauseAndRestart
            } else if end != 0 && SIM_VAR.quick_pause_sim(m) != 0 {
                SIM_VAR.set_quick_pause_sim(m, 0);
                r.call(c, nfs4rc::f_8007a710, &[], 0x800b_7bac); // AudioCmn_UnPauseAndQuit
            } else {
                let pause = REPLAY_INTERFACE.pause(m);
                if SIM_VAR.quick_pause_sim(m) != pause {
                    SIM_VAR.set_quick_pause_sim(m, pause);
                    if pause != 0 {
                        r.call(c, nfs4rc::f_80083dfc, &[], 0x800b_7bd8); // the camera
                        r.call(c, nfs4rc::f_8007a5e8, &[], 0x800b_7be0); // AudioCmn_Pause
                    } else {
                        r.call(c, nfs4rc::f_8007a6a0, &[], 0x800b_7bf0); // AudioCmn_UnPause
                    }
                }
            }
        }
    }
    let m = &mut c.m;
    if SIM_VAR.end_sim_game(m) != 0 {
        return;
    }
    if SIM_VAR.pause_sim(m) != 0 {
        r.enter(c, 0x800b_7870, 0x800b_7c18, process_pause);
        SIM_VAR.set_key_release(&mut c.m, 24);
        return;
    }
    if input(c, r, 21, 1, 0x800b_7c2c) != 0 {
        r.call(c, nfs4rc::f_8007ab50, &[], 0x800b_7c3c); // AudioMus_SwitchSong
        HUD_ACTIVATE_CD_PLAYER.set(&mut c.m, 1);
    }
    // a key still held from the pause menu: nothing until it is let go
    let held = SIM_VAR.key_release(&mut c.m);
    r.0[0] = 0;
    if held != 0 {
        if input(c, r, held as u32, 0, 0x800b_7c60) == 0 {
            SIM_VAR.set_key_release(&mut c.m, 0);
        }
        return;
    }
    r.0[2] = GAME_SETUP.0;
    let mut player = 0u32;
    loop {
        r.0[0] = player;
        if input(c, r, 26 + player.min(1), 1, 0x800b_7c94) != 0 && REPLAY_MODE.get(&mut c.m) < REPLAY_WATCH {
            r.call(c, nfs4rc::f_80086240, &[player], 0x800b_7cbc); // Camera_NextMode
        }
        if input(c, r, 28 + player.min(1), 1, 0x800b_7cd0) != 0 {
            r.call(c, nfs4rc::f_8009baa0, &[player], 0x800b_7ce0); // DashHUD_ToggleHud
        }
        let look = input(c, r, player + 22, 0, 0x800b_7cec);
        c.m.w32(LOOK_BEHIND + 4 * player, look);
        r.enter(c, 0x800b_7974, 0x800b_7d08, |c| check_for_pause(c, 1));
        player += 1;
        let split = (GAME_SETUP.comm_mode(&mut c.m) == 1) as u32;
        if split < player {
            break;
        }
        c.tick();
    }
}

/// A 64 Hz tick in [`main_game_loop`]: the input queue moves on each 32 Hz tick, and the
/// schedules run (a replay at speed 2; at 1 every other tick, at 0 every fourth, twice at 3;
/// on the ticks between only the camera moves).
fn sim_tick(c: &mut Cpu, r: &Saved) {
    let m = &mut c.m;
    let now = SIM_VAR.current_clock_ticks(m);
    if now & 1 == 0 {
        simqueue::set_current_input(m, now >> 1);
    }
    IN_BETWEEN.set(m, 0);
    let steps = if REPLAY_MODE.get(m) != REPLAY_WATCH {
        1
    } else {
        match REPLAY_INTERFACE.speed(m) {
            2 => 1,
            1 if now & 1 == 0 => 1,
            0 if now & 3 == 0 => 1,
            3 => 2,
            _ => 0,
        }
    };
    if steps == 0 {
        IN_BETWEEN.set(m, 1);
        r.call(c, nfs4rc::f_80083dfc, &[], 0x800b_7e38); // the camera
        return;
    }
    if steps == 2 {
        r.call_ptr(c, 0x800b_754c, &[], 0x800b_7e24); // Sim_ProcessSimSchedules
    }
    r.call_ptr(c, 0x800b_754c, &[], 0x800b_7e48);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, against_original, same_stack};

    /// The race as `cars::tests::shuffle` leaves it, any tick (odd or even), before the start
    /// (any beep of the countdown due or not) or after; a pursuit with a player as the police or
    /// not.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        crate::cars::tests::shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        SIM_GLOBAL.set_game_ticks(m, [pick(2000), pick(600), 511 + pick(3), pick(40)][pick(4) as usize]);
        SIM_GLOBAL.set_game_started(m, [1, 1, 0][pick(3) as usize]);
        m.w8(COUNTDOWN, pick(6) as u8);
        GAME_SETUP.set_race_type(m, [0, 1, 5, 2, 3][pick(5) as usize]);
        for i in 0..2 {
            let car = Car(m.r32(HUMAN_RACE_CARS + 4 * i));
            if car.0 != 0 {
                car.update_car_flags(m, |f| f & !CAR_HUMAN_COP | [0, 0, CAR_HUMAN_COP][pick(3) as usize]);
            }
        }
        let two = m.r32(HUMAN_RACE_CARS + 4) != 0;
        NUM_HUMAN_RACE_CARS.set(m, 1 + (two && pick(2) == 0) as i32);
        MASTER_SFX_LEVEL.set(m, pick(128));
        REPLAY_INTERFACE.set_stats_screen(m, pick(2));
    }

    /// A tick on the race's RAM leaves the stack (the scratchpad the schedules run on, and the
    /// main stack) and RAM as the original's.
    #[test]
    fn tick_stack_fuzz() {
        same_stack("Sim_ProcessSimSchedules", 0x800b_754c, nfs4rc::f_800b754c, 0x800b_7e48, 0x801F_FE00, 1500, 0x71C4, shuffle, &|_, _| vec![]);
    }

    #[test]
    fn fade_in_fuzz() {
        against_original("Sim_FadeInSFX", 2_000, 0xFADE, shuffle, nfs4rc::f_800b74cc, &|_| vec![], |c, _| {
            fade_in_sfx(&mut c.m);
            None
        });
    }

    /// The pause menu's moment: paused or not, its input delay over or not, a pause forced or
    /// not, any pad buttons (their edges too), racing or in a replay, early in the race or not.
    fn paused(c: &mut Cpu, rng: &mut Rng) {
        shuffle(c, rng);
        let m = &mut c.m;
        let mut pick = |n: u32| (rng.next() % n as u64) as i32;
        let frames = CLOCK_FRAMES.get(m);
        UNPAUSE_DELAY.set(m, frames + pick(3) - 1);
        FORCE_PAUSE.set(m, [0, 0, 1][pick(3) as usize]);
        SIM_VAR.set_restart_game(m, [0, 0, 0, 1][pick(4) as usize]);
        SIM_VAR.set_quick_pause_sim(m, pick(2));
        REPLAY_MODE.set(m, [0, 0, 1, 2, 3][pick(5) as usize]);
        m.w8(PAUSE_PORT_INDEX, pick(2) as u8);
        // the buttons of the last 32 ticks (`Input_gInterfaceResults`, what `Input_Interface`
        // reads), mostly none
        for i in 0..32 {
            let b = if pick(4) == 0 { 1 << pick(32) } else { 0 };
            m.w32(0x8011_51CC + 4 * i, b as u32);
        }
    }

    #[test]
    fn pause_fuzz() {
        same_stack("Sim_CheckForPause", 0x800b_7974, nfs4rc::f_800b7974, 0x800b_7d08, 0x801F_FE00, 2_000, 0x9A05, paused, &|_, rng| vec![(rng.next() % 2) as u32]);
        same_stack("Sim_ProcessPause", 0x800b_7870, nfs4rc::f_800b7870, 0x800b_7c18, 0x801F_FE00, 2_000, 0x9A06, paused, &|_, _| vec![]);
    }
}
