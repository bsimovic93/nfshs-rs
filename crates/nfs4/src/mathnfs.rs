//! MATHNFS.CPP: vector maths on `coorddef`s.

use crate::abi;
use crate::fixed::{fixeddiv, fixedmult, fixedsin, fixedcos, fixedsqrt};
use crate::types::Coord;
use ps1rt::Cpu;

/// `Math_ResolveRotatedVector` (MATHNFS.CPP, 0x8009e0ec): (x, z) turned by `angle` (a full
/// turn = 0x1000000) into `out_x`, `out_z`.
pub fn resolve_rotated_vector(c: &mut Cpu, x: i32, z: i32, angle: i32, out_x: u32, out_z: u32) {
    let a = angle / 256;
    let sin = fixedsin(&mut c.m, a);
    let cos = fixedcos(&mut c.m, a);
    c.m.w32(out_x, fixedmult(cos, x).wrapping_sub(fixedmult(sin, z)) as u32);
    c.m.w32(out_z, fixedmult(sin, x).wrapping_add(fixedmult(cos, z)) as u32);
}

pub fn abi_resolve_rotated_vector(c: &mut Cpu) {
    let out_z = abi::arg(c, 4);
    resolve_rotated_vector(c, c.r[4] as i32, c.r[5] as i32, c.r[6] as i32, c.r[7], out_z);
}

/// `Math_NormalizeVector` (MATHNFS.CPP, 0x8009e4e0): `v` scaled to length 1 (left as it is
/// when it is 0). The components are first halved until each is within 256 and the sum of
/// their squares fits.
pub fn normalize_vector(c: &mut Cpu, v: Coord) {
    const LIMIT: i32 = 0x100_0000;
    let (mut x, mut y, mut z) = (v.x(&mut c.m), v.y(&mut c.m), v.z(&mut c.m));
    // (the magnitude of i32::MIN stays negative: it passes)
    while x.wrapping_abs() > LIMIT || y.wrapping_abs() > LIMIT || z.wrapping_abs() > LIMIT {
        (x, y, z) = (x >> 1, y >> 1, z >> 1);
    }
    let squares = |x: i32, y: i32, z: i32| [fixedmult(x, x), fixedmult(y, y), fixedmult(z, z)].map(|s| s as u32);
    let mut sq = squares(x, y, z);
    while (sq[0] >> 2).wrapping_add(sq[1] >> 2).wrapping_add(sq[2] >> 2) > 0x4000_0000 {
        (x, y, z) = (x >> 1, y >> 1, z >> 1);
        sq = squares(x, y, z);
    }
    let len = fixedsqrt(&mut c.m, sq[0].wrapping_add(sq[1]).wrapping_add(sq[2]) as i32);
    if len != 0 {
        v.set_x(&mut c.m, fixeddiv(x, len));
        v.set_y(&mut c.m, fixeddiv(y, len));
        v.set_z(&mut c.m, fixeddiv(z, len));
    }
}

pub fn abi_normalize_vector(c: &mut Cpu) {
    normalize_vector(c, Coord(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, cpu, run};

    #[test]
    fn rotate_and_normalize() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(31);
        let v = Coord(SCRATCH);
        for _ in 0..50_000 {
            let (x, y, z, a) = (rng.any(), rng.any(), rng.any(), rng.any());
            for (k, e) in [x, y, z].into_iter().enumerate() {
                c.m.w32(SCRATCH + 4 * k as u32, e as u32);
            }
            run(&mut c, nfs4rc::f_8009e4e0, &[SCRATCH]);
            let want = (v.x(&mut c.m), v.y(&mut c.m), v.z(&mut c.m));
            for (k, e) in [x, y, z].into_iter().enumerate() {
                c.m.w32(SCRATCH + 4 * k as u32, e as u32);
            }
            normalize_vector(&mut c, v);
            assert_eq!((v.x(&mut c.m), v.y(&mut c.m), v.z(&mut c.m)), want, "Math_NormalizeVector({x:#x}, {y:#x}, {z:#x})");

            let (ox, oz) = (SCRATCH + 0x20, SCRATCH + 0x24);
            run(&mut c, nfs4rc::f_8009e0ec, &[x as u32, z as u32, a as u32, ox, oz]);
            let want = (c.m.r32(ox), c.m.r32(oz));
            resolve_rotated_vector(&mut c, x, z, a, ox, oz);
            assert_eq!((c.m.r32(ox), c.m.r32(oz)), want, "Math_ResolveRotatedVector({x:#x}, {z:#x}, {a:#x})");
        }
    }
}
