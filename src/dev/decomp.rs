//! `decomp-check`: the decompiled functions (crates/nfs4) against the recompiled originals over
//! a whole race. Two copies of the race run side by side, one with the decompiled code off and
//! one with it on, fed the same input; their RAM must be the same after every frame.

use crate::ps1::race::{FrameInput, RaceSetup, RaceThread};
use anyhow::{Result, bail};
use std::path::PathBuf;

/// The RAM taken as dead stack at a frame's end (the main stack's top: the decompiled code
/// leaves less behind below the stack pointer than the original).
const STACK: std::ops::Range<usize> = 0x1F_0000..0x20_0000;

/// Run `setup` twice in lockstep through `steps` (seconds, buttons, sticks); `both_original`
/// runs the recompiled code in both (the baseline: they must agree with each other too).
/// `setup` None powers the game on instead: its front end (and the races it starts) from the
/// first frame.
pub fn check(root: PathBuf, setup: Option<&RaceSetup>, steps: &[(f32, u16, Option<[u8; 4]>)], both_original: bool) -> Result<()> {
    let gs = setup.map(|s| s.game_setup()).transpose()?;
    let start = |root: PathBuf| match &gs {
        Some(gs) => RaceThread::start(root, gs.clone()),
        None => RaceThread::start_main(root),
    };
    let was = nfs4::enabled();
    nfs4::set_enabled(false);
    let mut a = start(root.clone())?;
    nfs4::set_enabled(!both_original);
    let mut b = start(root)?;
    nfs4::set_enabled(was);
    let what = if gs.is_some() { "races" } else { "games (from power-on)" };
    println!("two {what} booted (the second {})", if both_original { "recompiled too" } else { "decompiled" });
    let mut frames = 0u32;
    // NFSHS_DECOMP_RESYNC=1: a difference is reported (once per set of addresses), the
    // decompiled run given the original's words, and the run goes on (for listing every
    // leftover a scramble finds in one run)
    let resync = std::env::var("NFSHS_DECOMP_RESYNC").is_ok_and(|v| v == "1");
    let mut seen = std::collections::HashSet::new();
    let mut diverged = 0u32;
    for &(secs, buttons, sticks) in steps {
        for _ in 0..(secs * 60.0).round() as u32 {
            let input = FrameInput { buttons, buttons2: 0, want_ram: true, sticks: [sticks, None] };
            let ra = a.step(input)?.ram.clone().unwrap_or_default();
            let rb = b.step(input)?.ram.clone().unwrap_or_default();
            frames += 1;
            let diffs = differences(&ra, &rb);
            if !diffs.is_empty() && resync {
                diverged += 1;
                let word = |r: &[u8], a: usize| u32::from_le_bytes(r[a..a + 4].try_into().unwrap());
                if seen.insert(diffs.clone()) {
                    println!("frame {frames}: {} words differ", diffs.len());
                    for &a in diffs.iter().take(16) {
                        let addr = 0x8000_0000 | a as u32;
                        println!("  {addr:#010x} {}: original {:#010x} decompiled {:#010x}", nfs4::check::describe_in(&ra, addr), word(&ra, a), word(&rb, a));
                    }
                }
                b.poke(diffs.iter().map(|&a| (0x8000_0000 | a as u32, word(&ra, a))).collect());
            } else if !diffs.is_empty() {
                println!("frame {frames}: {} words differ", diffs.len());
                for &a in diffs.iter().take(32) {
                    let word = |r: &[u8]| u32::from_le_bytes(r[a..a + 4].try_into().unwrap());
                    let addr = 0x8000_0000 | a as u32;
                    println!("  {addr:#010x} {}: original {:#010x} decompiled {:#010x}", nfs4::check::describe_in(&ra, addr), word(&ra), word(&rb));
                }
                bail!("the decompiled code diverged at frame {frames}");
            }
        }
    }
    if diverged > 0 {
        bail!("{frames} frames: {diverged} differed (resynced each time)");
    }
    println!("{frames} frames: the same RAM after every one");
    if std::env::var_os("NFSHS_DECOMP_STATS").is_none() {
        println!("(NFSHS_DECOMP_STATS=1 shows how often each function ran)");
    }
    Ok(())
}

/// Where the libraries keep their caller's return address and stack pointers while they run
/// (`InitGeom` its return address at 0x80135d60; `_patch_gte` and the timer's set-up the stack
/// pointers): nothing reads them after, and a readable caller's stack depth differs.
const CALLER_SAVES: [usize; 3] = [0x5_3684, 0x13_5D60, 0x14_E2BC];

/// Known leftovers that only the pictures could use, and they don't: the skid marks' colours
/// (`gPrevSkidSm`, and the marks' segments at gp+5484) take their top byte from the stack
/// (`TrgSfx_AddSkidmark` builds the colour without it); the host draws the marks with the low
/// 24 bits. A difference there in the top byte alone isn't one. (See DECOMP.md, "The race's
/// leftovers".)
fn inert(a: &[u8], at: usize, x: &[u8], y: &[u8]) -> bool {
    if CALLER_SAVES.contains(&at) {
        return true;
    }
    if x[..3] != y[..3] {
        return false;
    }
    const PREV_SKIDS: std::ops::Range<usize> = 0x11_F5E0..0x11_F960;
    let word = |o: usize| u32::from_le_bytes(a[o..o + 4].try_into().unwrap());
    let base = (word((crate::ps1::race::addr::GP + 5484) as usize & 0x1F_FFFF) & 0x1F_FFFF) as usize;
    PREV_SKIDS.contains(&at) || (base != 0 && (base..base + 64 * 688).contains(&at))
}

/// The word offsets where two RAM images differ (but for the dead stack and the known inert
/// leftovers).
fn differences(a: &[u8], b: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    for (k, (x, y)) in a.chunks(256).zip(b.chunks(256)).enumerate() {
        if x == y {
            continue;
        }
        for w in (0..x.len()).step_by(4) {
            let at = k * 256 + w;
            if !STACK.contains(&at) && x[w..w + 4] != y[w..w + 4] && !inert(a, at, &x[w..w + 4], &y[w..w + 4]) {
                out.push(at);
            }
        }
    }
    out
}
