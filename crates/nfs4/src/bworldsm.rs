//! BWORLDSM.CPP: the track as the simulation sees it. The road is a chain of slices (cross
//! sections), each a strip of quads; these find the slice and quad under a point, the road's
//! normal and forward there (cached), its edges and light.

use crate::abi;
use crate::fixed::{crossproduct, fixedmult};
use crate::globals::*;
use crate::mathnfs::{dist_xz, normalize_vector};
use crate::types::{BWorldSmPos, Chunk, Coord, TNormalCacheEntry, TrkNewSimQuad, TrkNewSimSlice, TrkNewSlice, TrkNewStrip};
use crate::vec::Vec3;
use ps1rt::{Cpu, Mem};

/// The track's slice `i`.
fn slice_at(m: &mut Mem, i: i32) -> TrkNewSlice {
    TrkNewSlice(BWORLDSM_SLICES.get(m) as u32).at(i as u32)
}

/// The track's chunk `i`.
fn chunk_at(m: &mut Mem, i: u8) -> Chunk {
    Chunk(TRACK_CHUNK_LIST.get(m) as u32).at(i as u32)
}

/// The chunk `pos` is in.
fn pos_chunk(m: &mut Mem, pos: BWorldSmPos) -> Chunk {
    let i = pos.chunk(m);
    chunk_at(m, i)
}

/// The slice `pos` is on.
fn pos_slice(m: &mut Mem, pos: BWorldSmPos) -> TrkNewSlice {
    let i = pos.slice(m) as i32;
    slice_at(m, i)
}

/// The data of a `Group` (after its header).
fn group_data(group: u32) -> u32 {
    group.wrapping_add(4)
}

/// `a % b` as the R3000's `div` leaves the remainder (by 0, `a`).
fn rem(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { a.wrapping_rem(b) }
}

/// `BWorldSm_Init` (BWORLDSM.CPP, 0x8007f334): the track's slices from its data (a count, then
/// the slices).
pub fn init(c: &mut Cpu, data: u32) -> i32 {
    let count = c.m.r32(data) as i32;
    BWORLDSM_SLICES.set(&mut c.m, data.wrapping_add(4) as i32);
    MAX_FIND_QUAD_SLICE_ITERATIONS.set(&mut c.m, 5);
    NUM_SLICES.set(&mut c.m, count);
    restart(c);
    1
}

pub fn abi_init(c: &mut Cpu) {
    c.r[2] = init(c, c.r[4]) as u32;
}

/// `BWorldSm_Restart` (BWORLDSM.CPP, 0x8007f36c).
pub fn restart(c: &mut Cpu) {
    normal_cache_init(c);
}

pub fn abi_restart(c: &mut Cpu) {
    restart(c);
}

/// `BWorldSm_DeInit` (BWORLDSM.CPP, 0x8007f38c): nothing.
pub fn abi_deinit(_: &mut Cpu) {}

/// `FindAbsClosestSliceCrude` (BWORLDSM.CPP, 0x8007f394): the closest of every eighth slice
/// to `pt` (where the slice found last is far off).
pub fn find_abs_closest_slice_crude(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) {
    let count = NUM_SLICES.get(&mut c.m);
    let (mut best, mut closest) = (i32::MAX, -1);
    let mut s = 0;
    while s < count {
        let at = Coord(slice_at(&mut c.m, s).0);
        let d = crate::bworld::xzsquaredist32(c, at, pt);
        if d < best {
            closest = s;
            best = d;
        }
        s += 8;
    }
    pos.set_slice(&mut c.m, closest as i16);
}

pub fn abi_find_abs_closest_slice_crude(c: &mut Cpu) {
    find_abs_closest_slice_crude(c, Coord(c.r[4]), BWorldSmPos(c.r[5]));
}

/// `BWorldSm_FindClosestSlice` (BWORLDSM.CPP, 0x8007f438): the slice closest to `pt`, searched
/// from the one found last (from scratch when that is more than 128 m off); whether it
/// changed.
pub fn find_closest_slice(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) -> i32 {
    let last = pos.slice(&mut c.m);
    let at = Coord(slice_at(&mut c.m, last as i32).0);
    if 0x80_0000 < dist_xz(c, at, pt) {
        find_abs_closest_slice_crude(c, pt, pos);
    }
    raw_find_closest_slice(c, pt, pos);
    let changed = (pos.slice(&mut c.m) != last) as i8;
    pos.set_quad_changed(&mut c.m, changed);
    pos.set_slice_changed(&mut c.m, changed);
    let slice = pos.slice(&mut c.m);
    let chunk = slice_at(&mut c.m, slice as i32).chunk_index(&mut c.m);
    pos.set_chunk(&mut c.m, chunk);
    changed as i32
}

pub fn abi_find_closest_slice(c: &mut Cpu) {
    c.r[2] = find_closest_slice(c, Coord(c.r[4]), BWorldSmPos(c.r[5])) as u32;
}

/// `RawFindClosestSlice` (BWORLDSM.CPP, 0x8007f4d4): from `pos`'s slice, step to a neighbour
/// (round the track's ends) while it is closer to `pt` (squared distance in 1/128 m).
pub fn raw_find_closest_slice(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) {
    let count = NUM_SLICES.get(&mut c.m);
    let mut s = pos.slice(&mut c.m) as i32;
    if s != -1 {
        let last = count.wrapping_add(-1);
        let p = pt.get(&mut c.m);
        let dist = |m: &mut Mem, i: i32| {
            let center = slice_at(m, i);
            let dx = p.x.wrapping_sub(center.center(m, 0)) >> 9;
            let dz = p.z.wrapping_sub(center.center(m, 2)) >> 9;
            dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz))
        };
        loop {
            let from = s;
            let here = if s < last { dist(&mut c.m, s) } else { dist(&mut c.m, rem(s, count)) };
            let next = if s < last.wrapping_sub(1) { s + 1 } else { rem(s.wrapping_add(1), count) };
            if dist(&mut c.m, next) < here {
                s = next;
            } else if s > 0 {
                if dist(&mut c.m, s - 1) < here {
                    s -= 1;
                }
            } else {
                // (round the start: modulo the count)
                let prev = rem(s.wrapping_add(count).wrapping_sub(1), count);
                let back = if dist(&mut c.m, prev) < here { s.wrapping_add(count).wrapping_sub(1) } else { s.wrapping_add(count) };
                s = rem(back.wrapping_add(1).wrapping_add(last), count);
            }
            if s == from {
                break;
            }
        }
    }
    pos.set_slice(&mut c.m, s as i16);
}

pub fn abi_raw_find_closest_slice(c: &mut Cpu) {
    raw_find_closest_slice(c, Coord(c.r[4]), BWorldSmPos(c.r[5]));
}

/// `BWorldSm_SetSlice` (BWORLDSM.CPP, 0x8007f788): `pos` put on slice `slice`, its quad not
/// yet found.
pub fn set_slice(c: &mut Cpu, slice: i32, pos: BWorldSmPos) {
    let m = &mut c.m;
    pos.set_slice(m, slice as i16);
    let slice = pos.slice(m);
    pos.set_slice_changed(m, 0);
    pos.set_quad_changed(m, 0);
    pos.set_off_edge(m, 0);
    pos.set_sim_slice(m, TrkNewSimSlice(0));
    pos.set_sim_quad(m, TrkNewSimQuad(0));
    pos.set_sim_rot_flag(m, 0);
    let chunk = slice_at(m, slice as i32).chunk_index(m);
    pos.set_last_rez_requested(m, -2);
    pos.set_rez(m, 1);
    pos.set_triangle_flag(m, 0);
    pos.set_quad(m, -1);
    pos.set_chunk(m, chunk);
}

pub fn abi_set_slice(c: &mut Cpu) {
    set_slice(c, c.r[4] as i32, BWorldSmPos(c.r[5]));
}

/// A strip's vertices: the top and bottom rows' vertex `k`.
fn strip_vertices(m: &mut Mem, pos: BWorldSmPos, vertices: u32) -> (u32, u32) {
    let strip = pos.strip(m);
    let k = pos.strip_quad_ind(m) as i32;
    let top = (strip.top_vert(m) as i32).wrapping_add(k);
    let bottom = (strip.bot_vert(m) as i32).wrapping_add(k);
    (vertices.wrapping_add((top << 3) as u32), vertices.wrapping_add((bottom << 3) as u32))
}

/// `GetStmQuadPts` (BWORLDSM.CPP, 0x8007f7dc): the corners of `pos`'s quad at full precision,
/// from its chunk's vertices (1/64 m from `origin`, the chunk's centre).
pub fn get_stm_quad_pts(c: &mut Cpu, pos: BWorldSmPos, origin: Coord) {
    let m = &mut c.m;
    let o = origin.get(m);
    let chunk = pos_chunk(m, pos);
    let vertices = group_data(chunk.vertex_buf(m));
    let (top, bottom) = strip_vertices(m, pos, vertices);
    let corner = |m: &mut Mem, v: u32| {
        let r = |m: &mut Mem, k: u32| (m.r16(v + 2 * k) as i16 as i32) << 10;
        Vec3::new(o.x.wrapping_add(r(m, 0)), o.y.wrapping_add(r(m, 1)), o.z.wrapping_add(r(m, 2)))
    };
    // corners 1, 2 along the top row, 3, 0 along the bottom
    let p = corner(m, top + 8);
    pos.quad_pts(1).set(m, p);
    let p = corner(m, top);
    pos.quad_pts(2).set(m, p);
    let p = corner(m, bottom);
    pos.quad_pts(3).set(m, p);
    let p = corner(m, bottom + 8);
    pos.quad_pts(0).set(m, p);
}

pub fn abi_get_stm_quad_pts(c: &mut Cpu) {
    get_stm_quad_pts(c, BWorldSmPos(c.r[4]), Coord(c.r[5]));
}

/// `SetStrip` (BWORLDSM.CPP, 0x8007f904): the strip holding `pos`'s quad (by its sim slice's
/// strip index, then the quads of the strips before), and the quad's place in it.
pub fn set_strip(c: &mut Cpu, pos: BWorldSmPos) {
    let m = &mut c.m;
    let chunk = pos_chunk(m, pos);
    let mut strip = TrkNewStrip(group_data(chunk.strip_buf(m)));
    pos.set_strip(m, strip);
    let next = |m: &mut Mem, s: TrkNewStrip| TrkNewStrip(s.0.wrapping_add((s.quad_count(m) as u8 as u32) * 2 + 4));
    let skip = pos.sim_slice(m).strip_index(m);
    for _ in 0..skip {
        let current = pos.strip(m);
        strip = next(m, current);
        pos.set_strip(m, strip);
    }
    let mut k = pos.quad(m) as i32;
    let mut quads = strip.quad_count(m) as u8 as i32;
    while k >= quads {
        k -= quads;
        strip = next(m, strip);
        pos.set_strip(m, strip);
        quads = strip.quad_count(m) as u8 as i32;
    }
    pos.set_strip_quad_ind(m, k as i16);
}

pub fn abi_set_strip(c: &mut Cpu) {
    set_strip(c, BWorldSmPos(c.r[4]));
}

/// `GetFirstStmQuadPts` (BWORLDSM.CPP, 0x8007f9bc): `pos`'s strip, and its quad's corners
/// across the ground in 1/64 m from the chunk's centre (for the point-in-quad tests).
pub fn get_first_stm_quad_pts(c: &mut Cpu, pos: BWorldSmPos, vertices: u32) {
    set_strip(c, pos);
    let m = &mut c.m;
    let (top, bottom) = strip_vertices(m, pos, vertices);
    let copy = |m: &mut Mem, corner: u32, v: u32| {
        let q = pos.quad_pts16(corner);
        let x = m.r16(v);
        let z = m.r16(v + 4);
        q.set_x(m, x as i16);
        q.set_z(m, z as i16);
    };
    copy(m, 1, top + 8);
    copy(m, 2, top);
    copy(m, 3, bottom);
    copy(m, 0, bottom + 8);
}

pub fn abi_get_first_stm_quad_pts(c: &mut Cpu) {
    get_first_stm_quad_pts(c, BWorldSmPos(c.r[4]), c.r[5]);
}

/// `BWorld_SetSimSlice` (BWORLDSM.CPP, 0x8007fa58): `pos`'s chunk and its sim slice record.
pub fn set_sim_slice(c: &mut Cpu, pos: BWorldSmPos) {
    let m = &mut c.m;
    let slice = pos.slice(m) as i32;
    let index = slice_at(m, slice).chunk_index(m);
    pos.set_chunk(m, index);
    let chunk = chunk_at(m, index);
    let first = chunk.first_sim_slice_ind(m) as i32;
    let at = group_data(chunk.sim_slice_buf(m)).wrapping_add((slice.wrapping_sub(first) * 5) as u32);
    pos.set_sim_slice(m, TrkNewSimSlice(at));
}

pub fn abi_set_sim_slice(c: &mut Cpu) {
    set_sim_slice(c, BWorldSmPos(c.r[4]));
}

/// `BworldSm_UpdateSimQuad` (BWORLDSM.CPP, 0x8007fab8): `pos`'s sim quad (its surface), or the
/// global default one when its quad has none.
pub fn update_sim_quad(c: &mut Cpu, pos: BWorldSmPos) {
    let m = &mut c.m;
    let ss = pos.sim_slice(m);
    let k = (pos.quad(m) as i32).wrapping_sub(ss.simquad_start_index(m) as i32);
    if k < 0 || k >= ss.simquad_count(m) as i32 {
        pos.set_sim_quad(m, TrkNewSimQuad(GLOBAL_SIM_QUAD));
        return;
    }
    let chunk = pos_chunk(m, pos);
    let base = group_data(chunk.sim_quad_buf(m));
    pos.set_sim_quad(m, TrkNewSimQuad(base));
    let at = base.wrapping_add((ss.simquad_index(m) as i32 + k) as u32);
    pos.set_sim_quad(m, TrkNewSimQuad(at));
}

pub fn abi_update_sim_quad(c: &mut Cpu) {
    update_sim_quad(c, BWorldSmPos(c.r[4]));
}

/// Road surface 14: not road (a gap in the sim quads).
const SURFACE_NONE: u8 = 14;

/// `BworldSm_IsSimQuadValid` (BWORLDSM.CPP, 0x8007fb40): `pos` has a sim quad, and it is road.
pub fn is_sim_quad_valid(c: &mut Cpu, pos: BWorldSmPos) -> bool {
    let q = pos.sim_quad(&mut c.m);
    q.0 != 0 && q.surface(&mut c.m) & 0xF != SURFACE_NONE
}

pub fn abi_is_sim_quad_valid(c: &mut Cpu) {
    c.r[2] = is_sim_quad_valid(c, BWorldSmPos(c.r[4])) as u32;
}

/// Whether (x, z) is inside `pos`'s quad as [`get_first_stm_quad_pts`] left it: on the inner
/// side of all four edges.
fn inside_quad16(m: &mut Mem, pos: BWorldSmPos, x: i32, z: i32) -> bool {
    let q: [(i32, i32); 4] = std::array::from_fn(|i| {
        let p = pos.quad_pts16(i as u32);
        (p.x(m) as i32, p.z(m) as i32)
    });
    let outside = |a: (i32, i32), b: (i32, i32)| (a.0.wrapping_sub(b.0)).wrapping_mul(z.wrapping_sub(b.1)).wrapping_sub((x.wrapping_sub(b.0)).wrapping_mul(a.1.wrapping_sub(b.1))) > 0;
    !(outside(q[1], q[2]) || outside(q[0], q[1]) || outside(q[2], q[3]) || outside(q[3], q[0]))
}

/// How far (x, z) is from `pos`'s quad's corners in all (the search stops when this grows).
fn corner_distance(m: &mut Mem, pos: BWorldSmPos, x: i32, z: i32) -> i32 {
    let d: [(i32, i32); 4] = std::array::from_fn(|i| {
        let p = pos.quad_pts16(i as u32);
        (x.wrapping_sub(p.x(m) as i32).wrapping_abs(), z.wrapping_sub(p.z(m) as i32).wrapping_abs())
    });
    d[0].0.wrapping_add(d[0].1).wrapping_add(d[1].0.wrapping_add(d[1].1)).wrapping_add(d[2].0.wrapping_add(d[2].1)).wrapping_add(d[3].0.wrapping_add(d[3].1))
}

/// `RawFindClosestQuad` (BWORLDSM.CPP, 0x8007fb70): the quad under `pt`: along `pos`'s slice
/// from its quad (or the middle one) up, then down, while the quads get no farther; then on
/// the slices around (+1, -1, +2...) up to `gMaxFindQuadSliceIterations`. Found, its corners
/// are set at full precision (1); otherwise the slice is put back and the quad is left unset
/// (0), `offEdge` saying which edge of the first slice it was past (1 the first quad, 2 the
/// last).
pub fn raw_find_closest_quad(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) -> i32 {
    abi::frame(c, 64, |c, sp| raw_find_closest_quad_in_frame(c, pt, pos, sp))
}

fn raw_find_closest_quad_in_frame(c: &mut Cpu, pt: Coord, pos: BWorldSmPos, sp: u32) -> i32 {
    pos.set_sim_quad(&mut c.m, TrkNewSimQuad(0));
    pos.set_sim_rot_flag(&mut c.m, 0);
    set_sim_slice(c, pos);
    let mut iteration = 0;
    let mut moved = 0i32;
    let quad = pos.quad(&mut c.m);
    let count = pos.sim_slice(&mut c.m).quad_count(&mut c.m) as i32;
    let start = if quad >= 0 && (quad as i32) < count - 1 && pos.off_edge(&mut c.m) == 0 {
        quad
    } else {
        let middle = (count - 1) / 2;
        pos.set_quad(&mut c.m, middle as i8);
        middle as i8
    };
    let mut first_edge = 0i8;
    pos.set_off_edge(&mut c.m, 0);
    pos.set_quad(&mut c.m, start);
    while iteration < MAX_FIND_QUAD_SLICE_ITERATIONS.get(&mut c.m) {
        set_sim_slice(c, pos);
        pos.set_quad(&mut c.m, start);
        let count = pos.sim_slice(&mut c.m).quad_count(&mut c.m) as i32;
        if count - 1 < start as i32 {
            pos.set_quad(&mut c.m, (count - 2) as i8);
        }
        // the point across the ground in 1/64 m from the chunk's centre (16 bits)
        let center = Coord(CHUNK_CENTERS.get(&mut c.m) as u32 + 12 * pos.chunk(&mut c.m) as u32);
        let p = pt.get(&mut c.m);
        let x = (p.x.wrapping_sub(center.x(&mut c.m)) >> 10) as i16;
        c.m.w16(sp + 16, x as u16);
        let z = (p.z.wrapping_sub(center.z(&mut c.m)) >> 10) as i16;
        c.m.w16(sp + 20, z as u16);
        let (x, z) = (x as i32, z as i32);
        let vertices = group_data(pos_chunk(&mut c.m, pos).vertex_buf(&mut c.m));
        get_first_stm_quad_pts(c, pos, vertices);
        update_sim_quad(c, pos);

        let found = |c: &mut Cpu| {
            pos.set_rez(&mut c.m, 2);
            get_stm_quad_pts(c, pos, center);
            pos.set_off_edge(&mut c.m, 0);
            1
        };
        let quads = |c: &mut Cpu| pos.sim_slice(&mut c.m).quad_count(&mut c.m) as i32;
        // up the slice
        let mut best = i32::MAX;
        while (pos.quad(&mut c.m) as i32) < quads(c) {
            get_first_stm_quad_pts(c, pos, vertices);
            if inside_quad16(&mut c.m, pos, x, z) && is_sim_quad_valid(c, pos) {
                return found(c);
            }
            if pos.quad(&mut c.m) as i32 == quads(c) - 1 {
                pos.set_off_edge(&mut c.m, 2);
                break;
            }
            if is_sim_quad_valid(c, pos) {
                let d = corner_distance(&mut c.m, pos, x, z);
                if d.wrapping_sub(best) >= 41 {
                    break;
                }
                best = d;
            }
            pos.update_quad(&mut c.m, |q| q.wrapping_add(1));
            update_sim_quad(c, pos);
        }
        // down from the start
        best = i32::MAX;
        let down = if pos.off_edge(&mut c.m) != 0 && iteration == 0 { quads(c) - 1 } else { start as i32 - 1 };
        pos.set_quad(&mut c.m, down as i8);
        if pos.quad(&mut c.m) < 0 {
            pos.set_quad(&mut c.m, 0);
        }
        get_first_stm_quad_pts(c, pos, vertices);
        update_sim_quad(c, pos);
        while pos.quad(&mut c.m) >= 0 {
            get_first_stm_quad_pts(c, pos, vertices);
            if inside_quad16(&mut c.m, pos, x, z) && is_sim_quad_valid(c, pos) {
                return found(c);
            }
            if pos.quad(&mut c.m) == 0 {
                pos.set_off_edge(&mut c.m, 1);
                break;
            }
            if is_sim_quad_valid(c, pos) {
                let d = corner_distance(&mut c.m, pos, x, z);
                if d.wrapping_sub(best) >= 41 {
                    break;
                }
                best = d;
            }
            pos.update_quad(&mut c.m, |q| q.wrapping_sub(1));
            update_sim_quad(c, pos);
        }
        // the next slice to try
        if iteration == 0 {
            first_edge = pos.off_edge(&mut c.m);
        }
        let step = SLICE_SEARCH_STEPS.get(&mut c.m, iteration as u32);
        let slices = NUM_SLICES.get(&mut c.m);
        let mut s = (pos.slice(&mut c.m) as i32).wrapping_add(step);
        if s >= slices {
            s -= slices;
        }
        if s < 0 {
            s += slices;
        }
        pos.set_slice(&mut c.m, s as i16);
        iteration += 1;
        moved = moved.wrapping_add(step);
    }
    // not found: back on the slice it started on
    let slices = NUM_SLICES.get(&mut c.m);
    let mut s = (pos.slice(&mut c.m) as i32).wrapping_sub(moved);
    if s >= slices {
        s -= slices;
    }
    if s < 0 {
        s += slices;
    }
    pos.set_slice(&mut c.m, s as i16);
    set_sim_slice(c, pos);
    pos.set_off_edge(&mut c.m, first_edge);
    pos.set_sim_quad(&mut c.m, TrkNewSimQuad(0));
    pos.set_triangle_flag(&mut c.m, 0);
    0
}

pub fn abi_raw_find_closest_quad(c: &mut Cpu) {
    c.r[2] = raw_find_closest_quad(c, Coord(c.r[4]), BWorldSmPos(c.r[5])) as u32;
}

/// `FindClosestQuad` (BWORLDSM.CPP, 0x8008031c): the slice, then the quad under `pt`. Off the
/// road's edge, the edge quad; with none found, a point moved toward the slice's centre (a
/// 32nd at a time, ten times at most, `corrPt`). Whether the slice changed (and the quad).
pub fn find_closest_quad(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) -> i32 {
    let (last_slice, last_quad) = (pos.slice(&mut c.m), pos.quad(&mut c.m));
    find_closest_slice(c, pt, pos);
    let slice = pos.slice(&mut c.m);
    if raw_find_closest_quad(c, pt, pos) == 0 {
        let edge = pos.off_edge(&mut c.m);
        if edge != 0 {
            pos.set_slice(&mut c.m, slice);
            set_sim_slice(c, pos);
            let quad = if pos.off_edge(&mut c.m) == 1 { 0 } else { pos.sim_slice(&mut c.m).quad_count(&mut c.m) as i32 - 1 };
            pos.set_quad(&mut c.m, quad as i8);
            update_sim_quad(c, pos);
            pos.set_rez(&mut c.m, 2);
            set_strip(c, pos);
            let center = Coord(CHUNK_CENTERS.get(&mut c.m) as u32 + 12 * pos.chunk(&mut c.m) as u32);
            get_stm_quad_pts(c, pos, center);
        }
        let p = pt.get(&mut c.m);
        CORR_PT.set(&mut c.m, p);
        let mut tries = 0;
        while pos.sim_quad(&mut c.m).0 == 0 && tries < 10 {
            let s = pos_slice(&mut c.m, pos);
            let (cx, cz) = (s.center(&mut c.m, 0), s.center(&mut c.m, 2));
            CORR_PT.update_x(&mut c.m, |v| v.wrapping_add(cx.wrapping_sub(v) >> 5));
            CORR_PT.update_z(&mut c.m, |v| v.wrapping_add(cz.wrapping_sub(v) >> 5));
            raw_find_closest_quad(c, CORR_PT, pos);
            tries += 1;
        }
    }
    let slice_changed = (last_slice != pos.slice(&mut c.m)) as i8;
    pos.set_slice_changed(&mut c.m, slice_changed);
    let quad_changed = last_quad != pos.quad(&mut c.m) || slice_changed != 0;
    pos.set_quad_changed(&mut c.m, quad_changed as i8);
    pos.slice_changed(&mut c.m) as i32
}

pub fn abi_find_closest_quad(c: &mut Cpu) {
    c.r[2] = find_closest_quad(c, Coord(c.r[4]), BWorldSmPos(c.r[5])) as u32;
}

/// `BWorldSm_FindClosestQuadRez` (BWORLDSM.CPP, 0x800804e8): where `pt` is on the road, at
/// resolution `rez`: 0 the slice only; otherwise the quad, unless it is still in the quad
/// found last.
pub fn find_closest_quad_rez(c: &mut Cpu, pt: Coord, pos: BWorldSmPos, rez: i32) -> i32 {
    pos.set_triangle_flag(&mut c.m, 3);
    if rez == 0 {
        pos.set_last_rez_requested(&mut c.m, 1);
        pos.set_rez(&mut c.m, 1);
        pos.set_sim_slice(&mut c.m, TrkNewSimSlice(0));
        pos.set_sim_quad(&mut c.m, TrkNewSimQuad(0));
        pos.set_quad(&mut c.m, -1);
        pos.set_triangle_flag(&mut c.m, 0);
        return find_closest_slice(c, pt, pos);
    }
    pos.set_last_rez_requested(&mut c.m, 2);
    if pos.sim_quad(&mut c.m).0 != 0 && inside_quad(&mut c.m, pos, pt) {
        pos.set_quad_changed(&mut c.m, 0);
        pos.set_slice_changed(&mut c.m, 0);
        return 0;
    }
    find_closest_quad(c, pt, pos)
}

/// Whether `pt` is inside `pos`'s quad at full precision (the last edge counting as inside).
fn inside_quad(m: &mut Mem, pos: BWorldSmPos, pt: Coord) -> bool {
    let p = pt.get(m);
    let q: [Vec3; 4] = std::array::from_fn(|i| pos.quad_pts(i as u32).get(m));
    let side = |a: Vec3, b: Vec3| fixedmult(a.x.wrapping_sub(b.x), p.z.wrapping_sub(b.z)).wrapping_sub(fixedmult(p.x.wrapping_sub(b.x), a.z.wrapping_sub(b.z)));
    side(q[1], q[2]) <= 0 && side(q[0], q[1]) <= 0 && side(q[2], q[3]) <= 0 && side(q[3], q[0]) < 1
}

pub fn abi_find_closest_quad_rez(c: &mut Cpu) {
    c.r[2] = find_closest_quad_rez(c, Coord(c.r[4]), BWorldSmPos(c.r[5]), c.r[6] as i32) as u32;
}

/// `BWorldSm_FindClosestQuadMaxIterations` (BWORLDSM.CPP, 0x800806b4).
pub fn find_closest_quad_max_iterations(c: &mut Cpu, pt: Coord, pos: BWorldSmPos) -> i32 {
    find_closest_quad_rez(c, pt, pos, 1)
}

pub fn abi_find_closest_quad_max_iterations(c: &mut Cpu) {
    c.r[2] = find_closest_quad_max_iterations(c, Coord(c.r[4]), BWorldSmPos(c.r[5])) as u32;
}

/// `PointDirection` (BWORLDSM.CPP, 0x800806d4): which side of the line from `b` to `a` the
/// point `p` is on, across the ground (negative to the left).
pub fn point_direction(c: &mut Cpu, a: Coord, b: Coord, p: Coord) -> i32 {
    let (a, b, p) = (a.get(&mut c.m), b.get(&mut c.m), p.get(&mut c.m));
    fixedmult(a.x.wrapping_sub(b.x), p.z.wrapping_sub(b.z)).wrapping_sub(fixedmult(p.x.wrapping_sub(b.x), a.z.wrapping_sub(b.z)))
}

pub fn abi_point_direction(c: &mut Cpu) {
    c.r[2] = point_direction(c, Coord(c.r[4]), Coord(c.r[5]), Coord(c.r[6])) as u32;
}

/// `BWorldSm_FindEdgeOff` (BWORLDSM.CPP, 0x8008074c): which sides of `pos`'s quad `pt` is on, as
/// flags (1: within 1.5 m of the side from corner 3 to 2 or beyond, 2: likewise from 1 to 0;
/// 8 and 4: past the sides from 0 to 3 and 2 to 1), and in `height` the average height of
/// `other`'s quad corners above it.
pub fn find_edge_off(c: &mut Cpu, pt: Coord, pos: BWorldSmPos, other: BWorldSmPos, height: u32) -> i32 {
    let q = |i: u32| pos.quad_pts(i);
    let mut flags = (point_direction(c, q(2), q(3), pt) >= -0x1_8000) as i32;
    if point_direction(c, q(3), q(0), pt) >= 0 {
        flags |= 8;
    }
    if point_direction(c, q(0), q(1), pt) >= -0x1_8000 {
        flags |= 2;
    }
    if point_direction(c, q(1), q(2), pt) >= 0 {
        flags |= 4;
    }
    let sum = (0..4).fold(0i32, |s, i| s.wrapping_add(other.quad_pts(i).y(&mut c.m)));
    let h = (sum >> 2).wrapping_sub(pt.y(&mut c.m));
    c.m.w32(height, h as u32);
    flags
}

pub fn abi_find_edge_off(c: &mut Cpu) {
    c.r[2] = find_edge_off(c, Coord(c.r[4]), BWorldSmPos(c.r[5]), BWorldSmPos(c.r[6]), c.r[7]) as u32;
}

/// `BWorldSm_QuadLight` (BWORLDSM.CPP, 0x80080868): the light at `pos`'s quad: the average of
/// its corners' colours (0xBBGGRR; mid grey when its quad isn't known at full resolution).
/// The top byte is whatever was on the stack there: the original builds the colour in a
/// stack word and sets three of its bytes.
pub fn quad_light(c: &mut Cpu, pos: BWorldSmPos) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let m = &mut c.m;
        if pos.rez(m) != 2 {
            return 0x7F_7F7F;
        }
        let chunk = pos_chunk(m, pos);
        let vertices = group_data(chunk.vertex_buf(m));
        let (top, bottom) = strip_vertices(m, pos, vertices);
        // the corners' light indices: top + 1, top, bottom, bottom + 1
        let lights = [top + 8, top, bottom, bottom + 8].map(|v| m.r16(v + 6) as i16 as i32);
        for (k, &l) in lights.iter().enumerate() {
            let at = (CHUNK_LIGHT_TABLE.get(m) as u32).wrapping_add((l << 2) as u32);
            for b in 0..4 {
                let v = m.r8(at.wrapping_add(b));
                m.w8(sp + 8 + 8 * k as u32 + b, v);
            }
        }
        for channel in 0..3 {
            let s = (0..4).fold(0u32, |s, k| s + m.r8(sp + 8 + 8 * k + channel) as u32);
            m.w8(sp + channel, (s >> 2) as u8);
        }
        m.r32(sp)
    })
}

pub fn abi_quad_light(c: &mut Cpu) {
    c.r[2] = quad_light(c, BWorldSmPos(c.r[4]));
}

/// Slice acoustics: a tunnel.
const ACOUSTIC_TUNNEL: u8 = 0x44;
/// Road surface 8: a tunnel's.
const SURFACE_TUNNEL: u8 = 8;

/// `BWorldSm_TunnelFlagSm` (BWORLDSM.CPP, 0x800809f8): whether `pos` is in a tunnel (by its
/// slice's acoustics or its quad's surface).
pub fn tunnel_flag_sm(c: &mut Cpu, pos: BWorldSmPos) -> bool {
    let slice = pos.slice(&mut c.m) as i32;
    if slice_at(&mut c.m, slice).acoustic_type(&mut c.m) & ACOUSTIC_TUNNEL != 0 {
        return true;
    }
    let q = pos.sim_quad(&mut c.m);
    let surface = if q.0 == 0 { SURFACE_NONE } else { q.surface(&mut c.m) & 0xF };
    surface == SURFACE_TUNNEL
}

pub fn abi_tunnel_flag_sm(c: &mut Cpu) {
    c.r[2] = tunnel_flag_sm(c, BWorldSmPos(c.r[4])) as u32;
}

const CACHE_SIZE: u32 = 16;

fn cache_entry(i: i32) -> TNormalCacheEntry {
    TNormalCacheEntry(NORMAL_CACHE).at(i as u32)
}

/// `NormalCache_AddEntry` (BWORLDSM.CPP, 0x80080a50): `pos`'s normal and forward cached for
/// its quad, in place of the entry used longest ago.
pub fn normal_cache_add_entry(c: &mut Cpu, pos: BWorldSmPos) {
    let m = &mut c.m;
    NORMAL_CACHE_TIME.update(m, |t| t.wrapping_add(1));
    let (mut oldest, mut index) = (u32::MAX, -1);
    for i in 0..CACHE_SIZE as i32 {
        let t = cache_entry(i).access_time(m);
        if t < oldest {
            index = i;
            oldest = t;
        }
    }
    let e = cache_entry(index);
    let now = NORMAL_CACHE_TIME.get(m);
    e.set_access_time(m, now);
    let forward = pos.forward().get(m);
    e.forward().set(m, forward);
    let normal = pos.normal().get(m);
    e.normal().set(m, normal);
    let slice = pos.slice(m);
    e.set_slice_ind(m, slice);
    let quad = pos.quad(m);
    e.set_quad_ind(m, quad);
    let tri = pos.triangle_flag(m) as u8;
    e.set_triangle_flag(m, tri);
}

pub fn abi_normal_cache_add_entry(c: &mut Cpu) {
    normal_cache_add_entry(c, BWorldSmPos(c.r[4]));
}

/// `NormalCache_FindEntry` (BWORLDSM.CPP, 0x80080b0c): `pos`'s normal and forward from the
/// cache, if its quad (and triangle) is there. (The original compares the cached triangle
/// flag unsigned with `pos`'s signed.)
pub fn normal_cache_find_entry(c: &mut Cpu, pos: BWorldSmPos) -> bool {
    let m = &mut c.m;
    NORMAL_CACHE_TIME.update(m, |t| t.wrapping_add(1));
    let (slice, quad) = (pos.slice(m), pos.quad(m));
    for i in 0..CACHE_SIZE as i32 {
        let e = cache_entry(i);
        if e.slice_ind(m) == slice && e.quad_ind(m) == quad && e.triangle_flag(m) as i32 == pos.triangle_flag(m) as i32 {
            let now = NORMAL_CACHE_TIME.get(m);
            e.set_access_time(m, now);
            let normal = e.normal().get(m);
            pos.normal().set(m, normal);
            let forward = e.forward().get(m);
            pos.forward().set(m, forward);
            return true;
        }
    }
    false
}

pub fn abi_normal_cache_find_entry(c: &mut Cpu) {
    c.r[2] = normal_cache_find_entry(c, BWorldSmPos(c.r[4])) as u32;
}

/// `NormalCache_Init` (BWORLDSM.CPP, 0x80080bd0).
pub fn normal_cache_init(c: &mut Cpu) {
    let m = &mut c.m;
    NORMAL_CACHE_TIME.set(m, 0);
    for i in 0..CACHE_SIZE as i32 {
        let e = cache_entry(i);
        e.set_slice_ind(m, -1);
        e.set_quad_ind(m, -1);
        e.set_triangle_flag(m, 0);
        e.set_access_time(m, 0);
    }
}

pub fn abi_normal_cache_init(c: &mut Cpu) {
    normal_cache_init(c);
}

/// `Check_Rot` (BWORLDSM.CPP, 0x80080c0c): `pos`'s road normal and forward, unless they are
/// still those of its quad and triangle: from the cache or its corners (a triangle's two
/// edges, or for the whole quad (`triangleFlag` 3) each side plus an eighth of the other),
/// the normal kept from pointing straight up.
pub fn check_rot(c: &mut Cpu, pos: BWorldSmPos) {
    abi::frame(c, 64, |c, sp| {
        let tri = pos.triangle_flag(&mut c.m);
        if pos.sim_rot_flag(&mut c.m) != tri as i16 && !normal_cache_find_entry(c, pos) {
            let q: [Vec3; 4] = std::array::from_fn(|i| pos.quad_pts(i as u32).get(&mut c.m));
            let sub = |a: Vec3, b: Vec3| [a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y), a.z.wrapping_sub(b.z)];
            let (forward, across) = match tri {
                3 => {
                    let mix = |a: [i32; 3], b: [i32; 3]| -> [i32; 3] { std::array::from_fn(|i| a[i].wrapping_add(b[i] / 8)) };
                    (mix(sub(q[2], q[3]), sub(q[1], q[0])), mix(sub(q[0], q[3]), sub(q[1], q[2])))
                }
                2 => (sub(q[1], q[0]), sub(q[1], q[2])),
                _ => (sub(q[2], q[3]), sub(q[0], q[3])),
            };
            let (f, a) = (Coord(sp + 32), Coord(sp + 16));
            f.set(&mut c.m, forward.into());
            a.set(&mut c.m, across.into());
            pos.forward().set(&mut c.m, forward.into());
            crossproduct(&mut c.m, f.0, a.0, pos.normal().0);
            normalize_vector(c, pos.normal());
            if pos.normal().y(&mut c.m) > 0xFFFF {
                pos.normal().set_y(&mut c.m, 0xFFF9);
            }
            normalize_vector(c, pos.forward());
            normal_cache_add_entry(c, pos);
        }
        let tri = pos.triangle_flag(&mut c.m);
        pos.set_sim_rot_flag(&mut c.m, tri as i16);
    });
}

pub fn abi_check_rot(c: &mut Cpu) {
    check_rot(c, BWorldSmPos(c.r[4]));
}

/// `BWorldSm_UNormal` (BWORLDSM.CPP, 0x80080f44): `pos`'s road normal (up from the road).
pub fn u_normal(c: &mut Cpu, pos: BWorldSmPos) -> Coord {
    abi::frame(c, 24, |c, _| check_rot(c, pos));
    pos.normal()
}

pub fn abi_u_normal(c: &mut Cpu) {
    c.r[2] = u_normal(c, BWorldSmPos(c.r[4])).0;
}

/// `BWorldSm_UForward` (BWORLDSM.CPP, 0x80080f6c): `pos`'s road forward.
pub fn u_forward(c: &mut Cpu, pos: BWorldSmPos) -> Coord {
    abi::frame(c, 24, |c, _| check_rot(c, pos));
    pos.forward()
}

pub fn abi_u_forward(c: &mut Cpu) {
    c.r[2] = u_forward(c, BWorldSmPos(c.r[4])).0;
}

/// `BWorldSm_FindClosestTriangleRez` (BWORLDSM.CPP, 0x80080f94): [`find_closest_quad_rez`], and
/// which of the quad's triangles `pt` is in (1 or 2, across its diagonal from corner 0 to 2).
pub fn find_closest_triangle_rez(c: &mut Cpu, pt: Coord, pos: BWorldSmPos, rez: i32) -> i32 {
    abi::frame(c, 32, |c, _| {
        let r = find_closest_quad_rez(c, pt, pos, rez);
        if pos.sim_quad(&mut c.m).0 != 0 {
            let side = point_direction(c, pos.quad_pts(2), pos.quad_pts(0), pt);
            pos.set_triangle_flag(&mut c.m, if side > 0 { 1 } else { 2 });
        }
        r
    })
}

pub fn abi_find_closest_triangle_rez(c: &mut Cpu) {
    c.r[2] = find_closest_triangle_rez(c, Coord(c.r[4]), BWorldSmPos(c.r[5]), c.r[6] as i32) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, cpu_with, race_ram, ram_diff, report};
    use crate::types::Car;

    /// The track queries from random points up to 100 m around the player's car, with its road
    /// position (`simRoadInfo`) started from random slices, quads and states.
    #[test]
    fn track_query_fuzz() {
        let Some(ram) = race_ram() else { return };
        let mut rng = Rng(0x7EAC);
        let car = Car(u32::from_le_bytes(ram[0x11_0CA0..0x11_0CA4].try_into().unwrap()));
        let pos = car.n().sim_road_info();
        let pt = Coord(SCRATCH);
        let mut c = cpu_with(&ram);
        let slices = NUM_SLICES.get(&mut c.m);
        let mut mismatches = 0;
        let cases: [(&str, fn(&mut Cpu), fn(&mut Cpu, Coord, BWorldSmPos, u32) -> u32); 6] = [
            ("BWorldSm_FindClosestTriangleRez", nfs4rc::f_80080f94, |c, p, q, r| find_closest_triangle_rez(c, p, q, r as i32) as u32),
            ("BWorldSm_FindClosestQuadRez", nfs4rc::f_800804e8, |c, p, q, r| find_closest_quad_rez(c, p, q, r as i32) as u32),
            ("BWorldSm_UNormal", nfs4rc::f_80080f44, |c, _, q, _| u_normal(c, q).0),
            ("BWorldSm_QuadLight", nfs4rc::f_80080868, |c, _, q, _| quad_light(c, q)),
            ("BWorldSm_TunnelFlagSm", nfs4rc::f_800809f8, |c, _, q, _| tunnel_flag_sm(c, q) as u32),
            ("BWorldSm_FindEdgeOff", nfs4rc::f_8008074c, |c, p, q, r| find_edge_off(c, p, q, q, r) as u32),
        ];
        for i in 0..30_000u32 {
            c.m.ram.copy_from_slice(&ram);
            let base = car.n().position().get(&mut c.m);
            let mut off = |k: u64| (rng.next() % (2 * k)) as i32 - k as i32;
            let p = Vec3::new(base.x.wrapping_add(off(0x64_0000)), base.y.wrapping_add(off(0x5_0000)), base.z.wrapping_add(off(0x64_0000)));
            pt.set(&mut c.m, p);
            match rng.next() % 4 {
                0 => {}
                1 => pos.set_slice(&mut c.m, (rng.next() % slices as u64) as i16),
                2 => {
                    let s = (rng.next() % slices as u64) as i32;
                    set_slice(&mut c, s, pos);
                }
                _ => pos.update_slice(&mut c.m, |s| s.wrapping_add((rng.next() % 9) as i16 - 4)),
            }
            pos.set_quad(&mut c.m, (rng.next() % 12) as i8 - 1);
            pos.set_off_edge(&mut c.m, (rng.next() % 3) as i8);
            pos.set_triangle_flag(&mut c.m, (rng.next() % 4) as i8);
            pos.set_rez(&mut c.m, (rng.next() % 3) as i8);
            if rng.next() % 3 == 0 {
                pos.set_sim_quad(&mut c.m, TrkNewSimQuad(0));
            }
            let (name, original, port) = cases[(i % cases.len() as u32) as usize];
            let arg = if name == "BWorldSm_FindEdgeOff" { SCRATCH + 0x20 } else { (rng.next() % 2) as u32 };
            let args: Vec<u32> = match name {
                "BWorldSm_UNormal" | "BWorldSm_QuadLight" | "BWorldSm_TunnelFlagSm" => vec![pos.0],
                "BWorldSm_FindEdgeOff" => vec![pt.0, pos.0, pos.0, arg],
                _ => vec![pt.0, pos.0, arg],
            };
            let state = c.m.ram.clone();
            let want_v = crate::test_util::run(&mut c, original, &args);
            let want = c.m.ram.clone();
            c.m.ram.copy_from_slice(&state);
            let got_v = port(&mut c, pt, pos, arg);
            let diff = ram_diff(&want, &c.m.ram);
            let value_differs = got_v != want_v;
            if !diff.is_empty() || value_differs {
                mismatches += 1;
                if mismatches <= 5 {
                    report(name, i, &want, &c.m.ram, &diff);
                    eprintln!("  returned {got_v:#x}, the original {want_v:#x}");
                }
            }
        }
        assert_eq!(mismatches, 0, "track queries differ from the original");
    }
}
