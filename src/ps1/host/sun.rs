//! The sun and its lens flare (Flare_Sun, Flare_LensFlare), recorded apart: the sun goes
//! behind the world with the sky, the flare over it (the original puts it in the ordering
//! table's nearest entry).
//!
//! The original measures how much of the sun shows by reading its own frame back: the flare
//! draws a 5x5 white quad at the sun behind the world, and the next frame StoreImage reads
//! those pixels into the flare's state ([`LENS_FLARE`] + 16); the flare is as bright as the
//! count of them still white. That frame is never drawn here: the renderer tests the same
//! points against its own picture and hands the result over in [`SUN_VISIBLE`].

use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Flare_LensFlare(sun's screen position, cache).
const FLARE_LENS_FLARE: u32 = 0x800C_F8CC;
/// Its state: the sun's position (+0, +2) and last frame's (+8, +10), the 25 pixels read
/// back (+16), the flare's size (+116), its elements (+120: 9 of 16 bytes, the first word
/// how far along the line through the screen's centre, 16.16) and the read-back request (+124).
const LENS_FLARE: u32 = 0x8012_1568;
/// The flare's pieces: the spikes at the sun (Flare_Spikes) and the elements along the line
/// (Flare_SingleColorTex, Hex, Oct, OctRing), each called with its index in s0.
const FLARE_SPIKES: u32 = 0x800C_D118;
const FLARE_ELEMENTS: [u32; 4] = [0x800C_F458, 0x800C_F4FC, 0x800C_F610, 0x800C_F6E8];

/// The renderer's test of the sun: bit `5 * row + column` set where that pixel of the sun
/// is not covered by the world.
pub static SUN_VISIBLE: AtomicU32 = AtomicU32::new(0);

/// One piece of the lens flare: its primitives, and how far it moves as the sun moves
/// (1 at the sun; an element at t along the line through the screen's centre is the sun
/// mirrored through it by 1 - 2t).
#[derive(Clone, Debug)]
pub struct LensPiece {
    pub shift: f32,
    pub cmds: Vec<OverlayCmd>,
}

#[derive(Default)]
struct LensMarks {
    /// The primitive buffer from where Flare_LensFlare started to where it ended.
    range: Option<(u32, u32)>,
    /// Where each piece starts, and its shift.
    pieces: Vec<(u32, f32)>,
}

thread_local! {
    static LENS_MARKS: std::cell::RefCell<LensMarks> = std::cell::RefCell::new(LensMarks::default());
}

/// Inner hook while Flare_Sun runs: Flare_LensFlare run here with its pieces marked.
fn lens_flare_marker(c: &mut Cpu, addr: u32) -> bool {
    if addr != FLARE_LENS_FLARE {
        return false;
    }
    let start = c.m.r32(0x1F80_0004);
    c.inner_hook = Some(lens_piece_marker);
    c.call_nested(FLARE_LENS_FLARE, &[c.r[4], c.r[5]]);
    c.inner_hook = Some(lens_flare_marker);
    let end = c.m.r32(0x1F80_0004);
    LENS_MARKS.with_borrow_mut(|m| m.range = Some((start, end)));
    true
}

fn lens_piece_marker(c: &mut Cpu, addr: u32) -> bool {
    let shift = if addr == FLARE_SPIKES {
        1.0
    } else if FLARE_ELEMENTS.contains(&addr) {
        let elements = c.m.r32(LENS_FLARE + 120);
        let t = c.m.r32(elements + 16 * c.r[16]) as i32;
        1.0 - 2.0 * t as f32 / 65536.0
    } else {
        return false;
    };
    let at = c.m.r32(0x1F80_0004);
    LENS_MARKS.with_borrow_mut(|m| m.pieces.push((at, shift)));
    false
}

impl GameHost {
    /// Flare_Sun with the GTE set up (a0, a1 in place): the sun's primitives (with the white
    /// quad the flare is measured with), and the lens flare's pieces when it was drawn.
    pub(super) fn record_sun(&mut self, c: &mut Cpu) -> (Vec<OverlayCmd>, Option<Vec<LensPiece>>) {
        LENS_MARKS.with_borrow_mut(|m| *m = LensMarks::default());
        let saved = std::mem::take(&mut self.overlay);
        self.prim_offset = [0, 0];
        let start = c.m.r32(0x1F80_0004);
        c.inner_hook = Some(lens_flare_marker);
        c.call_nested(FLARE_SUN, &[c.r[4], c.r[5]]);
        c.inner_hook = None;
        let end = c.m.r32(0x1F80_0004);
        let marks = LENS_MARKS.with_borrow_mut(std::mem::take);
        let mut lens = None;
        match marks.range {
            Some((from, to)) if start <= from && to <= end => {
                // the white quad first, then the pieces from the first one on
                let first = marks.pieces.first().map_or(to, |p| p.0);
                self.record_packets(c, start, first);
                self.record_packets(c, to, end);
                let sun = std::mem::take(&mut self.overlay);
                let cuts: Vec<u32> = marks.pieces.iter().map(|p| p.0).collect();
                let at = self.record_marked(c, first, to, &cuts);
                let cmds = std::mem::take(&mut self.overlay);
                lens = Some(
                    marks
                        .pieces
                        .iter()
                        .enumerate()
                        .map(|(k, p)| LensPiece { shift: p.1, cmds: cmds[at[k]..at.get(k + 1).copied().unwrap_or(cmds.len())].to_vec() })
                        .filter(|p| !p.cmds.is_empty())
                        .collect(),
                );
                self.overlay = sun;
            }
            _ => self.record_packets(c, start, end),
        }
        (std::mem::replace(&mut self.overlay, saved), lens)
    }

    /// StoreImage into the lens flare's read-back: the sun's pixels as the renderer last saw
    /// them (white where uncovered).
    pub(super) fn sun_pixels(&self, c: &mut Cpu, dest: u32, w: usize, h: usize) -> bool {
        if self.mode != Mode::Race || dest != LENS_FLARE + 16 || w * h != 25 {
            return false;
        }
        let bits = SUN_VISIBLE.load(Ordering::Relaxed);
        if std::env::var_os("NFSHS_SUNLOG").is_some() {
            eprintln!("sun read back: {:025b}", bits);
        }
        for i in 0..25 {
            c.m.w16(dest + 2 * i, if bits >> i & 1 != 0 { 0x7FFF } else { 0 });
        }
        true
    }
}
