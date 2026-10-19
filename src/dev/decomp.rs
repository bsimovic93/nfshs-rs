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
pub fn check(root: PathBuf, setup: &RaceSetup, steps: &[(f32, u16, Option<[u8; 4]>)], both_original: bool) -> Result<()> {
    let gs = setup.game_setup()?;
    let was = nfs4::enabled();
    nfs4::set_enabled(false);
    let mut a = RaceThread::start(root.clone(), gs.clone())?;
    nfs4::set_enabled(!both_original);
    let mut b = RaceThread::start(root, gs)?;
    nfs4::set_enabled(was);
    println!("two races booted (the second {})", if both_original { "recompiled too" } else { "decompiled" });
    let mut frames = 0u32;
    for &(secs, buttons, sticks) in steps {
        for _ in 0..(secs * 60.0).round() as u32 {
            let input = FrameInput { buttons, buttons2: 0, want_ram: true, sticks: [sticks, None] };
            let ra = a.step(input)?.ram.clone().unwrap_or_default();
            let rb = b.step(input)?.ram.clone().unwrap_or_default();
            frames += 1;
            let diffs = differences(&ra, &rb);
            if !diffs.is_empty() {
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
    println!("{frames} frames: the same RAM after every one");
    if std::env::var_os("NFSHS_DECOMP_STATS").is_none() {
        println!("(NFSHS_DECOMP_STATS=1 shows how often each function ran)");
    }
    Ok(())
}

/// The word offsets where two RAM images differ (but for the dead stack).
fn differences(a: &[u8], b: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    for (k, (x, y)) in a.chunks(256).zip(b.chunks(256)).enumerate() {
        if x == y {
            continue;
        }
        for w in (0..x.len()).step_by(4) {
            let at = k * 256 + w;
            if !STACK.contains(&at) && x[w..w + 4] != y[w..w + 4] {
                out.push(at);
            }
        }
    }
    out
}
