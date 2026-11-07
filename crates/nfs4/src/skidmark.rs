//! SKIDMARK.CPP: the tyre marks. Marks are kept in chunks (`Skidmark_Chunk`, 688 bytes: an
//! origin, a count, the slice, 24 segments), 16 of them (8 with two players) used round in
//! turn. A segment (`Skidmark_Segment`) is a mark's two edge points across the tyre's track,
//! in 64ths of the world's units from its chunk's origin, its colour, its type and the next
//! segment along. A wheel's mark in progress is its `tSkid`: where it last was, its colour and
//! type, its last segment and that segment's chunk. The host draws the marks itself from the
//! chunks (`read_skids`), so what matters is the RAM.
//!
//! Written readable from the start (step 3 of DECOMP.md's plan): the game's data as the
//! original leaves it, the stack and registers free.

use crate::call::{call_addr, with_buffer};
use crate::fixed::{fastintcos, fastintsin, fixedmult, intatan};
use crate::globals::GAME_SETUP;
use crate::types::Coord;
use ps1rt::{Cpu, Mem};

/// The chunks (`gSm`), how many have been used (`gCountSm`), the one marks go into
/// (`gUseSm`), how many there are (`gMaxSChunk`: 16, 8 with two players).
const CHUNKS: u32 = 0x8013_D83C + 5484;
const COUNT: u32 = 0x8013_D83C + 5488;
const IN_USE: u32 = 0x8013_D83C + 5492;
const MAX_CHUNKS: u32 = 0x8013_D83C + 5496;
const CHUNKS_ONE_PLAYER: u32 = 16;
const CHUNKS_SPLIT: u32 = 8;
const SPLIT_SCREEN: i32 = 1;

/// A chunk: its origin (a world position), its segments' count (+12, a short), its slice
/// (+14), its segments (+16 on, 28 bytes each); room for 24, a new one taken past 24.
const CHUNK_SIZE: u32 = 688;
const CHUNK_N: u32 = 12;
const CHUNK_SLICE: u32 = 14;
const CHUNK_SEGMENTS: u32 = 16;
const CHUNK_ROOM: i32 = 25;
/// A chunk's origin is kept within this of the marks in it (on each axis).
const CHUNK_REACH: i32 = 0xF_FFFF;
/// A segment: two edge points (`SVECTOR`s, +0 and +8), its colour (+16), type (+20), the next
/// segment (+24).
const SEGMENT_SIZE: u32 = 28;
const SEG_RGB: u32 = 16;
const SEG_TYPE: u32 = 20;
const SEG_NEXT: u32 = 24;
/// A wheel's mark (`tSkid`): where it last was (+0), its colour (+12), type (+16), its last
/// segment (+20, 0 none: a new mark), that segment's chunk (+24).
const SKID_CLR: u32 = 12;
const SKID_TYPE: u32 = 16;
const SKID_SEG: u32 = 20;
const SKID_CHUNK: u32 = 24;

/// The library's heap (`reservememadr(name, size, 0)`, `purgememadr`); the skid marks'
/// drawing (`Draw_kCtrlSkidmark`, its control block: the view's turn and place, the chunks).
const RESERVE: u32 = 0x800E_5F28;
const PURGE: u32 = 0x800E_612C;
const DRAW_CTRL_SKIDMARK: u32 = 0x800C_9DFC;
const NAME: u32 = 0x8005_71B8;

fn chunk(m: &mut Mem, k: u32) -> u32 {
    m.r32(CHUNKS).wrapping_add(k.wrapping_mul(CHUNK_SIZE))
}

/// The chunk marks go into.
fn in_use(m: &mut Mem) -> u32 {
    let k = m.r32(IN_USE);
    chunk(m, k)
}

fn segment(ch: u32, k: i32) -> u32 {
    ch.wrapping_add(CHUNK_SEGMENTS).wrapping_add((k as u32).wrapping_mul(SEGMENT_SIZE))
}

/// The chunk's segment count.
fn count(m: &mut Mem, ch: u32) -> i32 {
    m.r16(ch + CHUNK_N) as i16 as i32
}

/// A colour (`CVECTOR`, maybe unaligned) copied.
fn copy_colour(m: &mut Mem, to: u32, from: u32) {
    for k in 0..4 {
        let b = m.r8(from.wrapping_add(k));
        m.w8(to.wrapping_add(k), b);
    }
}

/// Half the mark's width across the way from `from` to `to`: the offset of its edges (x, z).
fn half_width(m: &mut Mem, from: Coord, to: Coord, width: i32) -> (i32, i32) {
    let (dx, dz) = (to.x(m).wrapping_sub(from.x(m)), to.z(m).wrapping_sub(from.z(m)));
    let angle = intatan(m, dx, dz);
    let cos = fastintcos(m, angle);
    let x = fixedmult(width, cos.wrapping_neg()) >> 1;
    let sin = fastintsin(m, angle);
    let z = fixedmult(width, sin) >> 1;
    (x, z)
}

/// An edge point (`SVECTOR` at `at`): `p` moved by `(dx, dz)`, from `origin`, in 64ths.
fn edge(m: &mut Mem, at: u32, origin: Coord, p: Coord, dx: i32, dz: i32) {
    let x = p.x(m).wrapping_add(dx).wrapping_sub(origin.x(m)) >> 6;
    m.w16(at, x as u16);
    let y = p.y(m).wrapping_sub(origin.y(m)) >> 6;
    m.w16(at + 2, y as u16);
    let z = p.z(m).wrapping_add(dz).wrapping_sub(origin.z(m)) >> 6;
    m.w16(at + 4, z as u16);
}

/// `CalcStartSegment` (SKIDMARK.CPP, 0x800df300): a mark's first two segments, `first` at
/// `from` and `second` at `to`, `width` across the way between them.
pub fn calc_start_segment(m: &mut Mem, first: u32, second: u32, origin: Coord, from: Coord, to: Coord, width: i32) {
    let (dx, dz) = half_width(m, from, to, width);
    edge(m, first, origin, from, dx, dz);
    edge(m, first + 8, origin, from, dx.wrapping_neg(), dz.wrapping_neg());
    edge(m, second + 8, origin, to, dx.wrapping_neg(), dz.wrapping_neg());
    edge(m, second, origin, to, dx, dz);
}

pub fn abi_calc_start_segment(c: &mut Cpu) {
    let sp = c.r[29];
    let (to, width) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
    calc_start_segment(&mut c.m, c.r[4], c.r[5], Coord(c.r[6]), Coord(c.r[7]), Coord(to), width as i32);
}

/// `CalcOneSegment` (SKIDMARK.CPP, 0x800df4d0): the segment at `to`, `width` across the way
/// from `from`.
pub fn calc_one_segment(m: &mut Mem, seg: u32, origin: Coord, from: Coord, to: Coord, width: i32) {
    let (dx, dz) = half_width(m, from, to, width);
    edge(m, seg, origin, to, dx, dz);
    edge(m, seg + 8, origin, to, dx.wrapping_neg(), dz.wrapping_neg());
}

pub fn abi_calc_one_segment(c: &mut Cpu) {
    let width = c.m.r32(c.r[29] + 16);
    calc_one_segment(&mut c.m, c.r[4], Coord(c.r[5]), Coord(c.r[6]), Coord(c.r[7]), width as i32);
}

/// `Skidmark_CheckChunk` (SKIDMARK.CPP, 0x800df5f8): the chunk `added` more segments at
/// `pos` go into: the one in use, unless they don't fit in it or `pos` is too far from its
/// origin, then the next (a fresh one while some are unused, else the oldest again), emptied.
/// An empty chunk takes `pos` for its origin and `slice` for its slice.
pub fn check_chunk(m: &mut Mem, pos: Coord, added: i32, slice: i32) -> u32 {
    let mut ch = in_use(m);
    let far = |m: &mut Mem, k: u32| {
        let (o, p) = (m.r32(ch + 4 * k) as i32, m.r32(pos.0.wrapping_add(4 * k)) as i32);
        let d = o.wrapping_sub(p);
        if d > 0 { CHUNK_REACH < d } else { CHUNK_REACH < p.wrapping_sub(o) }
    };
    let full = count(m, ch).wrapping_add(added) >= CHUNK_ROOM;
    if full || far(m, 0) || far(m, 1) || far(m, 2) {
        let (used, max) = (m.r32(COUNT) as i32, m.r32(MAX_CHUNKS) as i32);
        if used < max {
            m.w32(IN_USE, used as u32);
            m.w32(COUNT, used.wrapping_add(1) as u32);
        } else {
            let k = m.r32(IN_USE) as i32;
            m.w32(IN_USE, if k < max.wrapping_sub(1) { k.wrapping_add(1) as u32 } else { 0 });
        }
        ch = in_use(m);
        m.w16(ch + CHUNK_N, 0);
    }
    if count(m, ch) == 0 {
        for k in 0..3 {
            let v = m.r32(pos.0.wrapping_add(4 * k));
            m.w32(ch + 4 * k, v);
        }
        m.w16(ch + CHUNK_SLICE, slice as u16);
    }
    ch
}

pub fn abi_check_chunk(c: &mut Cpu) {
    c.r[2] = check_chunk(&mut c.m, Coord(c.r[4]), c.r[5] as i32, c.r[6] as i32);
}

/// Where a mark's new segments went, for [`add`] (the wheel's mark follows them) and
/// [`add_stretch`] (the caller is told).
struct Added {
    seg: u32,
    chunk: u32,
    /// The chunk's count once they are in, which the caller writes last (as the original).
    ch: u32,
    n: i32,
}

impl Added {
    fn count_in(&self, m: &mut Mem) {
        m.w16(self.ch + CHUNK_N, self.n as u16);
    }
}

/// A mark's segments added at `pos` (the work of `Skidmark_Add` and `Skidmark_AddStretch`): a
/// new mark (none yet) starts with two segments, from where the wheel was and here, both in
/// this colour; a mark going on in the same chunk gets one more, joined to its last; a mark
/// going on into another chunk starts again there with two (the first in the mark's last
/// colour and type), as chunks aren't joined.
fn add_segments(m: &mut Mem, skid: u32, pos: Coord, colour: u32, width: i32, kind: i32, slice: i32) -> Added {
    let new_mark = m.r32(skid + SKID_SEG) == 0;
    let ch = check_chunk(m, pos, 2, slice);
    let origin = Coord(ch);
    if !new_mark && m.r32(skid + SKID_CHUNK) == m.r32(IN_USE) {
        let n = count(m, ch);
        let seg = segment(ch, n);
        calc_one_segment(m, seg, origin, Coord(skid), pos, width);
        copy_colour(m, seg + SEG_RGB, colour);
        m.w32(seg + SEG_TYPE, kind as u32);
        let last = m.r32(skid + SKID_SEG);
        m.w32(last + SEG_NEXT, seg);
        m.w32(seg + SEG_NEXT, 0);
        let chunk = m.r32(IN_USE);
        return Added { seg, chunk, ch, n: n.wrapping_add(1) };
    }
    let n = count(m, ch);
    let (first, second) = (segment(ch, n), segment(ch, n.wrapping_add(1)));
    calc_start_segment(m, first, second, origin, Coord(skid), pos, width);
    if new_mark {
        copy_colour(m, second + SEG_RGB, colour);
        copy_colour(m, first + SEG_RGB, second + SEG_RGB);
        m.w32(second + SEG_TYPE, kind as u32);
        m.w32(first + SEG_TYPE, kind as u32);
    } else {
        copy_colour(m, first + SEG_RGB, skid + SKID_CLR);
        let last_kind = m.r32(skid + SKID_TYPE);
        m.w32(first + SEG_TYPE, last_kind);
        copy_colour(m, second + SEG_RGB, colour);
        m.w32(second + SEG_TYPE, kind as u32);
    }
    m.w32(first + SEG_NEXT, second);
    m.w32(second + SEG_NEXT, 0);
    let chunk = m.r32(IN_USE);
    Added { seg: second, chunk, ch, n: n.wrapping_add(2) }
}

/// The wheel's mark now at `pos` (its colour, type, last segment and chunk).
fn follow(m: &mut Mem, skid: u32, pos: Coord, colour: u32, kind: i32, seg: u32, chunk: u32) {
    copy_colour(m, skid + SKID_CLR, colour);
    m.w32(skid + SKID_TYPE, kind as u32);
    for k in 0..3 {
        let v = m.r32(pos.0 + 4 * k);
        m.w32(skid + 4 * k, v);
    }
    m.w32(skid + SKID_SEG, seg);
    m.w32(skid + SKID_CHUNK, chunk);
}

/// `Skidmark_Add` (SKIDMARK.CPP, 0x800df7a4): wheel mark `skid` gone on to `pos` (in colour
/// `colour`, `width` wide, of type `kind`, on `slice`).
pub fn add(m: &mut Mem, skid: u32, pos: Coord, colour: u32, width: i32, kind: i32, slice: i32) {
    let added = add_segments(m, skid, pos, colour, width, kind, slice);
    follow(m, skid, pos, colour, kind, added.seg, added.chunk);
    added.count_in(m);
}

pub fn abi_add(c: &mut Cpu) {
    let sp = c.r[29];
    let (kind, slice) = (c.m.r32(sp + 16), c.m.r32(sp + 20));
    add(&mut c.m, c.r[4], Coord(c.r[5]), c.r[6], c.r[7] as i32, kind as i32, slice as i32);
}

/// `Skidmark_AddStretch` (SKIDMARK.CPP, 0x800dfb78): [`add`]'s segments for a mark that
/// stretches (the wheel's mark left as it was): its last segment and chunk to `seg_out` and
/// `chunk_out`.
#[allow(clippy::too_many_arguments)]
pub fn add_stretch(m: &mut Mem, seg_out: u32, chunk_out: u32, skid: u32, pos: Coord, colour: u32, width: i32, kind: i32, slice: i32) {
    let added = add_segments(m, skid, pos, colour, width, kind, slice);
    m.w32(seg_out, added.seg);
    m.w32(chunk_out, added.chunk);
    added.count_in(m);
}

pub fn abi_add_stretch(c: &mut Cpu) {
    let sp = c.r[29];
    let a: Vec<u32> = (4..8).map(|k| c.m.r32(sp + 4 * k)).collect();
    add_stretch(&mut c.m, c.r[4], c.r[5], c.r[6], Coord(c.r[7]), a[0], a[1] as i32, a[2] as i32, a[3] as i32);
}

/// `Skidmark_Stretch` (SKIDMARK.CPP, 0x800dff14): a stretching mark's last segment `seg` (in
/// chunk `chunk`) moved on to `pos`, in colour `colour` and of type `kind`.
#[allow(clippy::too_many_arguments)]
pub fn stretch(m: &mut Mem, seg: u32, chunk: u32, skid: u32, pos: Coord, colour: u32, width: i32, kind: i32) {
    let origin = Coord(self::chunk(m, chunk));
    calc_one_segment(m, seg, origin, Coord(skid), pos, width);
    copy_colour(m, seg + SEG_RGB, colour);
    m.w32(seg + SEG_TYPE, kind as u32);
}

pub fn abi_stretch(c: &mut Cpu) {
    let sp = c.r[29];
    let a: Vec<u32> = (4..7).map(|k| c.m.r32(sp + 4 * k)).collect();
    stretch(&mut c.m, c.r[4], c.r[5], c.r[6], Coord(c.r[7]), a[0], a[1] as i32, a[2] as i32);
}

/// `Skidmark_EndStretch` (SKIDMARK.CPP, 0x800dff94): [`stretch`], and the wheel's mark then
/// follows the stretched segment.
#[allow(clippy::too_many_arguments)]
pub fn end_stretch(m: &mut Mem, seg: u32, chunk: u32, skid: u32, pos: Coord, colour: u32, width: i32, kind: i32) {
    stretch(m, seg, chunk, skid, pos, colour, width, kind);
    follow(m, skid, pos, colour, kind, seg, chunk);
}

pub fn abi_end_stretch(c: &mut Cpu) {
    let sp = c.r[29];
    let a: Vec<u32> = (4..7).map(|k| c.m.r32(sp + 4 * k)).collect();
    end_stretch(&mut c.m, c.r[4], c.r[5], c.r[6], Coord(c.r[7]), a[0], a[1] as i32, a[2] as i32);
}

/// `Skidmark_OnyxBuildFacets` (SKIDMARK.CPP, 0x800e0070): the marks drawn for `view`
/// (`Draw_kCtrlSkidmark` with the view's turn, its y row and column negated, its place, the
/// chunks used).
pub fn onyx_build_facets(c: &mut Cpu, view: u32) {
    with_buffer(c, 0x38, |c, ctrl| {
        let m = &mut c.m;
        for k in 0..3 {
            let v = m.r32(view + 8 + 4 * k);
            m.w32(ctrl + 0x24 + 4 * k, v);
        }
        let (used, chunks) = (m.r32(COUNT), m.r32(CHUNKS));
        m.w32(ctrl + 0x30, used);
        m.w32(ctrl + 0x34, chunks);
        for k in 0..9 {
            let v = m.r32(view + 68 + 4 * k);
            // (the y's: the matrix's second column)
            m.w32(ctrl + 4 * k, if k % 3 == 1 { v.wrapping_neg() } else { v });
        }
        call_addr(c, DRAW_CTRL_SKIDMARK, &[ctrl]);
    });
}

pub fn abi_onyx_build_facets(c: &mut Cpu) {
    onyx_build_facets(c, c.r[4]);
}

/// `Skidmark_InitSkidmark` (SKIDMARK.CPP, 0x800e0110): the chunks reserved (half as many
/// with two players), none used.
pub fn init_skidmark(c: &mut Cpu) {
    c.m.w32(MAX_CHUNKS, CHUNKS_ONE_PLAYER);
    if GAME_SETUP.comm_mode(&mut c.m) == SPLIT_SCREEN {
        c.m.w32(MAX_CHUNKS, CHUNKS_SPLIT);
    }
    let max = c.m.r32(MAX_CHUNKS);
    let chunks = call_addr(c, RESERVE, &[NAME, max.wrapping_mul(CHUNK_SIZE), 0]);
    let m = &mut c.m;
    m.w16(chunks + CHUNK_N, 0);
    m.w32(CHUNKS, chunks);
    m.w32(COUNT, 0);
    m.w32(IN_USE, 0);
}

pub fn abi_init_skidmark(c: &mut Cpu) {
    init_skidmark(c);
}

/// `Skidmark_RestartSkidmark` (SKIDMARK.CPP, 0x800e0184): the marks gone (no chunk used).
pub fn restart_skidmark(m: &mut Mem) {
    m.w32(COUNT, 0);
    m.w32(IN_USE, 0);
    let chunks = m.r32(CHUNKS);
    m.w16(chunks + CHUNK_N, 0);
}

pub fn abi_restart_skidmark(c: &mut Cpu) {
    restart_skidmark(&mut c.m);
}

/// `Skidmark_KillSkidmark` (SKIDMARK.CPP, 0x800e0198): the chunks freed.
pub fn kill_skidmark(c: &mut Cpu) {
    let chunks = c.m.r32(CHUNKS);
    if chunks != 0 {
        call_addr(c, PURGE, &[chunks]);
    }
    c.m.w32(CHUNKS, 0);
}

pub fn abi_kill_skidmark(c: &mut Cpu) {
    kill_skidmark(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original};

    /// Free RAM (the race leaves it zero): a wheel's mark, where it is now, its colour (and an
    /// unaligned one), where `AddStretch` answers, a view.
    const SKID: u32 = 0x8014_0000;
    const POS: u32 = 0x8014_0040;
    const COLOUR: u32 = 0x8014_0060;
    const SEG_OUT: u32 = 0x8014_0080;
    const CHUNK_OUT: u32 = 0x8014_0084;
    const VIEW: u32 = 0x8014_0100;

    /// A step from `from`: small (a tick's), or past a chunk's reach now and then.
    fn step(rng: &mut Rng) -> i32 {
        match pick(rng, 8) {
            0 => pick(rng, 0x30_0000) as i32 - 0x18_0000,
            1 => 0,
            _ => pick(rng, 0x1_0000) as i32 - 0x8000,
        }
    }

    /// The race's marks (8 of 16 chunks used, the 8th taking marks), in any state the game
    /// can be in: fewer or all chunks used (8 with two players), the one in use empty, nearly
    /// full or full; a wheel's mark near its chunk's origin or far, new or going on, in that
    /// chunk or another; where the wheel is now, its colour, a view.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        GAME_SETUP.set_comm_mode(m, pick(rng, 2) as i32);
        let max = [16, 16, 8][pick(rng, 3) as usize];
        m.w32(MAX_CHUNKS, max);
        let used = [8, max, 1 + pick(rng, max)][pick(rng, 3) as usize].min(max);
        m.w32(COUNT, used);
        let k = [used - 1, pick(rng, used)][pick(rng, 2) as usize];
        m.w32(IN_USE, k);
        let ch = chunk(m, k);
        let n = [0, 1, 22, 23, 24, pick(rng, 25)][pick(rng, 6) as usize];
        m.w16(ch + CHUNK_N, n as u16);
        m.w16(ch + CHUNK_SLICE, pick(rng, 1100) as u16);
        for a in 0..3 {
            let o = m.r32(ch + 4 * a) as i32;
            m.w32(SKID + 4 * a, o.wrapping_add(step(rng)) as u32);
        }
        for a in 0..4 {
            m.w8(SKID + SKID_CLR + a, rng.next() as u8);
        }
        m.w32(SKID + SKID_TYPE, pick(rng, 3));
        let last = if n > 0 { segment(ch, n as i32 - 1) } else { segment(ch, 0) };
        m.w32(SKID + SKID_SEG, [0, last, last][pick(rng, 3) as usize]);
        m.w32(SKID + SKID_CHUNK, [k, k, pick(rng, used)][pick(rng, 3) as usize]);
        for a in 0..3 {
            let p = m.r32(SKID + 4 * a) as i32;
            m.w32(POS + 4 * a, p.wrapping_add(step(rng)) as u32);
        }
        for a in 0..8 {
            m.w8(COLOUR + a, rng.next() as u8);
        }
        m.w32(SEG_OUT, 0);
        m.w32(CHUNK_OUT, 0);
        for a in 0..26 {
            m.w32(VIEW + 4 * a, rng.any() as u32);
        }
    }

    /// The race's camera views (`gCView`, the one `gVi` points at); free RAM for the drawing
    /// (an ordering table entry, the primitives: compared, below the stack), the scratchpad's
    /// ordering table and primitive pointers (`OT`, `NEXT_PRIM`, the buffer's end) at it.
    const CVIEW: u32 = 0x8011_8234;
    const GVI: u32 = 0x8013_EB24;
    const OT_ENTRY: u32 = 0x801E_2A00;
    const PRIMS: u32 = 0x801E_3000;
    const PRIM_END: u32 = 0x1F80_0008;

    fn draw_into_free_ram(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        m.w32(crate::gpu::OT, OT_ENTRY);
        m.w32(crate::gpu::NEXT_PRIM, PRIMS);
        m.w32(PRIM_END, PRIMS + 0x8000);
        for k in (0..0x1000).step_by(4) {
            m.w32(PRIMS + k, rng.next() as u32);
        }
        m.w32(OT_ENTRY, rng.next() as u32);
    }

    /// The used chunks moved to where the camera is (the race RAM's marks are behind it, out
    /// of its view): the slice of the player's car (or near it), origins near the view's place.
    fn marks_in_view(c: &mut Cpu, rng: &mut Rng) {
        let m = &mut c.m;
        let car = crate::types::Car(m.r32(crate::globals::HUMAN_RACE_CARS));
        let slice = car.n().sim_road_info().slice(m) as i32;
        // (only the 8 chunks and 23 segments the race filled: others hold junk links)
        let used = m.r32(COUNT).min(8);
        m.w32(COUNT, used);
        for k in 0..used {
            let ch = chunk(m, k);
            let n = count(m, ch).clamp(0, 23);
            m.w16(ch + CHUNK_N, n as u16);
        }
        for k in 0..used {
            if pick(rng, 4) == 0 {
                continue;
            }
            let ch = chunk(m, k);
            m.w16(ch + CHUNK_SLICE, slice.wrapping_add(pick(rng, 5) as i32 - 2) as u16);
            for a in 0..3 {
                let t = m.r32(CVIEW + 8 + 4 * a) as i32;
                m.w32(ch + 4 * a, t.wrapping_add(pick(rng, 0x8_0000) as i32 - 0x4_0000) as u32);
            }
        }
    }

    fn width(rng: &mut Rng) -> u32 {
        [0x1_0000, 0x2_8000, 0, pick(rng, 0x8_0000)][pick(rng, 4) as usize]
    }

    fn colour(rng: &mut Rng) -> u32 {
        COLOUR + [0, 1, 2][pick(rng, 3) as usize]
    }

    /// A segment of a used chunk (a stretching mark's), and its chunk.
    fn stretched(c: &mut Cpu, rng: &mut Rng) -> (u32, u32) {
        let used = c.m.r32(COUNT).max(1);
        let k = pick(rng, used);
        let ch = chunk(&mut c.m, k);
        (segment(ch, pick(rng, 24) as i32), k)
    }

    fn cases() -> Vec<Case> {
        vec![
            ("CalcStartSegment", 0x800d_f300, nfs4rc::f_800df300, |c, r| {
                let ch = in_use(&mut c.m);
                vec![segment(ch, 3), segment(ch, 4), ch, SKID, POS, width(r)]
            }),
            ("CalcOneSegment", 0x800d_f4d0, nfs4rc::f_800df4d0, |c, r| {
                let ch = in_use(&mut c.m);
                vec![segment(ch, 5), ch, SKID, POS, width(r)]
            }),
            ("Skidmark_CheckChunk", 0x800d_f5f8, nfs4rc::f_800df5f8, |c, r| {
                // (now and then exactly at a chunk's reach, or just past it, on one axis)
                if pick(r, 3) == 0 {
                    let ch = in_use(&mut c.m);
                    let axis = 4 * pick(r, 3);
                    let o = c.m.r32(ch + axis) as i32;
                    let d = [CHUNK_REACH, CHUNK_REACH + 1][pick(r, 2) as usize];
                    let v = if pick(r, 2) == 0 { o.wrapping_add(d) } else { o.wrapping_sub(d) };
                    c.m.w32(POS + axis, v as u32);
                }
                vec![POS, [2, 1, pick(r, 30)][pick(r, 3) as usize], pick(r, 1100)]
            }),
            ("Skidmark_Add", 0x800d_f7a4, nfs4rc::f_800df7a4, |_, r| vec![SKID, POS, colour(r), width(r), pick(r, 3), pick(r, 1100)]),
            ("Skidmark_AddStretch", 0x800d_fb78, nfs4rc::f_800dfb78, |_, r| vec![SEG_OUT, CHUNK_OUT, SKID, POS, colour(r), width(r), pick(r, 3), pick(r, 1100)]),
            ("Skidmark_Stretch", 0x800d_ff14, nfs4rc::f_800dff14, |c, r| {
                let (seg, k) = stretched(c, r);
                vec![seg, k, SKID, POS, colour(r), width(r), pick(r, 3)]
            }),
            ("Skidmark_EndStretch", 0x800d_ff94, nfs4rc::f_800dff94, |c, r| {
                let (seg, k) = stretched(c, r);
                vec![seg, k, SKID, POS, colour(r), width(r), pick(r, 3)]
            }),
            ("Skidmark_OnyxBuildFacets", 0x800e_0070, nfs4rc::f_800e0070, |c, r| {
                draw_into_free_ram(c, r);
                marks_in_view(c, r);
                let gvi = c.m.r32(GVI);
                vec![[CVIEW, gvi][pick(r, 2) as usize]]
            }),
            ("Skidmark_InitSkidmark", 0x800e_0110, nfs4rc::f_800e0110, |_, _| vec![]),
            ("Skidmark_RestartSkidmark", 0x800e_0184, nfs4rc::f_800e0184, |_, _| vec![]),
            ("Skidmark_KillSkidmark", 0x800e_0198, nfs4rc::f_800e0198, |c, r| {
                if pick(r, 4) == 0 {
                    c.m.w32(CHUNKS, 0);
                }
                vec![]
            }),
        ]
    }

    #[test]
    fn skidmark_fuzz() {
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xB10 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original(name, 600, 0xB11 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }
}
