//! BWORLD.CPP: the track world as drawn (contexts, chunks), and a distance helper the track
//! queries use.

use crate::types::Coord;
use ps1rt::Cpu;

/// `xzsquaredist32` (BWORLD.CPP, 0x8007df6c): the squared distance from `a` to `b` across the
/// ground, in 1/16 m squared over 64 (so it fits).
pub fn xzsquaredist32(c: &mut Cpu, a: Coord, b: Coord) -> i32 {
    let (a, b) = (a.get(&mut c.m), b.get(&mut c.m));
    let dx = b.x.wrapping_sub(a.x) >> 12;
    let dz = b.z.wrapping_sub(a.z) >> 12;
    (dx.wrapping_mul(dx) >> 6).wrapping_add(dz.wrapping_mul(dz) >> 6)
}

pub fn abi_xzsquaredist32(c: &mut Cpu) {
    c.r[2] = xzsquaredist32(c, Coord(c.r[4]), Coord(c.r[5])) as u32;
}
