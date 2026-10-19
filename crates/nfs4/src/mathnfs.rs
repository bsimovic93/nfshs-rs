//! MATHNFS.CPP: vector maths on `coorddef`s.

use crate::abi;
use crate::fixed::{fixedcos, fixeddiv, fixedinverse, fixedmult, fixedsin, fixedsqrt};
use crate::types::{Coord, Matrix};
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

/// The length of (x, z) as the game estimates it: the longer side and a quarter of the
/// shorter.
pub fn quick_length(x: i32, z: i32) -> i32 {
    longer_and_quarter(x.wrapping_abs(), z.wrapping_abs())
}

/// The longer of two magnitudes and a quarter of the shorter (taken as they are).
pub fn longer_and_quarter(x: i32, z: i32) -> i32 {
    if z < x { x.wrapping_add(z >> 2) } else { z.wrapping_add(x >> 2) }
}

/// A vector halved until each component is within 256 and the sum of the squares (as
/// `fixedmult` gives them) fits: the components, the squares and how many halvings.
fn scaled_down(mut v: [i32; 3]) -> ([i32; 3], [u32; 3], u32) {
    const LIMIT: i32 = 0x100_0000;
    let mut halvings = 0;
    // (the magnitude of i32::MIN stays negative: it passes)
    while v.iter().any(|e| e.wrapping_abs() > LIMIT) {
        v = v.map(|e| e >> 1);
        halvings += 1;
    }
    let squares = |v: [i32; 3]| v.map(|e| fixedmult(e, e) as u32);
    let mut sq = squares(v);
    while (sq[0] >> 2).wrapping_add(sq[1] >> 2).wrapping_add(sq[2] >> 2) > 0x4000_0000 {
        v = v.map(|e| e >> 1);
        halvings += 1;
        sq = squares(v);
    }
    (v, sq, halvings)
}

fn sum(sq: [u32; 3]) -> u32 {
    sq[0].wrapping_add(sq[1]).wrapping_add(sq[2])
}

/// `Math_NormalizeVector` (MATHNFS.CPP, 0x8009e4e0): `v` scaled to length 1 (left as it is
/// when it is 0), scaled down first so its square fits.
pub fn normalize_vector(c: &mut Cpu, v: Coord) {
    let ([x, y, z], sq, _) = scaled_down(v.get(&mut c.m).into());
    let len = fixedsqrt(&mut c.m, sum(sq) as i32);
    if len != 0 {
        v.set_x(&mut c.m, fixeddiv(x, len));
        v.set_y(&mut c.m, fixeddiv(y, len));
        v.set_z(&mut c.m, fixeddiv(z, len));
    }
}

/// `Math_VectorLength` (MATHNFS.CPP, 0x8009e1b0): the length of `v` (scaled down to fit and
/// back up, so its low bits are lost on long vectors).
pub fn vector_length(c: &mut Cpu, v: Coord) -> i32 {
    let (_, sq, halvings) = scaled_down(v.get(&mut c.m).into());
    let mut len = fixedsqrt(&mut c.m, sum(sq) as i32);
    for _ in 0..halvings {
        len <<= 1;
    }
    len
}

pub fn abi_vector_length(c: &mut Cpu) {
    c.r[2] = vector_length(c, Coord(c.r[4])) as u32;
}

/// `Math_VectorLength2` (MATHNFS.CPP, 0x8009e34c): the square of the length of `v`.
pub fn vector_length2(c: &mut Cpu, v: Coord) -> i32 {
    let (_, sq, halvings) = scaled_down(v.get(&mut c.m).into());
    let mut len2 = sum(sq);
    for _ in 0..halvings {
        len2 <<= 2;
    }
    len2 as i32
}

pub fn abi_vector_length2(c: &mut Cpu) {
    c.r[2] = vector_length2(c, Coord(c.r[4])) as u32;
}

/// `Math_NormalizeShortVector` (MATHNFS.CPP, 0x8009e698): `v` (short enough for its square
/// to fit) scaled to length 1, by its inverse length.
pub fn normalize_short_vector(c: &mut Cpu, v: Coord) {
    let e = v.get(&mut c.m);
    let len2 = fixedmult(e.x, e.x).wrapping_add(fixedmult(e.y, e.y)).wrapping_add(fixedmult(e.z, e.z));
    let len = fixedsqrt(&mut c.m, len2);
    if len != 0 {
        let inv = fixeddiv(0x1_0000, len);
        v.update_x(&mut c.m, |a| fixedmult(a, inv));
        v.update_y(&mut c.m, |a| fixedmult(a, inv));
        v.update_z(&mut c.m, |a| fixedmult(a, inv));
    }
}

pub fn abi_normalize_short_vector(c: &mut Cpu) {
    normalize_short_vector(c, Coord(c.r[4]));
}

/// `Math_QDNormalizeVector` (MATHNFS.CPP, 0x8009e744): `v` roughly normalized ("quick and
/// dirty"): by the inverse of its [`better_dist`] length, in 8.8.
pub fn qd_normalize_vector(c: &mut Cpu, v: Coord) {
    let e = v.get(&mut c.m);
    let len = better_dist(better_dist(e.x.wrapping_abs(), e.y.wrapping_abs()), e.z.wrapping_abs());
    if len == 0 {
        return;
    }
    let inv = fixedinverse(len) / 256;
    v.update_x(&mut c.m, |a| (a / 256).wrapping_mul(inv));
    v.update_y(&mut c.m, |a| (a / 256).wrapping_mul(inv));
    v.update_z(&mut c.m, |a| (a / 256).wrapping_mul(inv));
}

pub fn abi_qd_normalize_vector(c: &mut Cpu) {
    qd_normalize_vector(c, Coord(c.r[4]));
}

/// `Math_DistXZ` (MATHNFS.CPP, 0x8009dea8): the distance from `a` to `b` across the ground,
/// estimated ([`quick_length`]).
pub fn dist_xz(c: &mut Cpu, a: Coord, b: Coord) -> i32 {
    let (a, b) = (a.get(&mut c.m), b.get(&mut c.m));
    quick_length(a.x.wrapping_sub(b.x), a.z.wrapping_sub(b.z))
}

pub fn abi_dist_xz(c: &mut Cpu) {
    c.r[2] = dist_xz(c, Coord(c.r[4]), Coord(c.r[5])) as u32;
}

/// `Math_Dist3D` (MATHNFS.CPP, 0x8009defc): the distance from `a` to `b`, estimated.
pub fn dist_3d(c: &mut Cpu, a: Coord, b: Coord) -> i32 {
    let (a, b) = (a.get(&mut c.m), b.get(&mut c.m));
    let xy = quick_length(a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y));
    longer_and_quarter(xy, a.z.wrapping_sub(b.z).wrapping_abs())
}

pub fn abi_dist_3d(c: &mut Cpu) {
    c.r[2] = dist_3d(c, Coord(c.r[4]), Coord(c.r[5])) as u32;
}

/// `Math_BetterDist` (MATHNFS.CPP, 0x8009df8c): the length of (a, b) (both positive) more
/// closely than [`quick_length`]: the longer side and the shorter times a sum of powers of two
/// approximating sqrt(1 + r^2) - 1 over r, in four bands of their ratio r.
pub fn better_dist(a: i32, b: i32) -> i32 {
    let (long, short) = if b < a { (a, b) } else { (b, a) };
    let shifts: &[u32] = if short < long >> 2 {
        &[4, 6, 7, 9, 14, 15, 16]
    } else if short < long >> 1 {
        &[3, 5, 6, 7, 8, 9, 12, 13, 14, 16]
    } else if short < (long >> 1).wrapping_add(long >> 2) {
        &[2, 5, 8, 10, 11, 12, 13]
    } else {
        &[2, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15]
    };
    shifts.iter().fold(long, |d, &k| d.wrapping_add(short >> k))
}

pub fn abi_better_dist(c: &mut Cpu) {
    c.r[2] = better_dist(c.r[4] as i32, c.r[5] as i32) as u32;
}

/// `Math_fasttransmult` (MATHNFS.CPP, 0x8009e850): `out` = `a` x `b`, each product of 16.16
/// numbers taken as (a >> 3) (b >> 3) >> 10 (quicker, losing the low bits); `out` may be
/// one of them.
pub fn fasttransmult(c: &mut Cpu, a: Matrix, b: Matrix, out: Matrix) {
    let (a, b) = (a.get(&mut c.m).m, b.get(&mut c.m).m);
    let product = std::array::from_fn(|k| {
        let (i, j) = (k / 3, k % 3);
        (0..3).fold(0i32, |s, n| s.wrapping_add((a[3 * i + n] >> 3).wrapping_mul(b[3 * n + j] >> 3) >> 10))
    });
    out.set(&mut c.m, &crate::vec::Mat3 { m: product });
}

pub fn abi_fasttransmult(c: &mut Cpu) {
    fasttransmult(c, Matrix(c.r[4]), Matrix(c.r[5]), Matrix(c.r[6]));
}

pub fn abi_normalize_vector(c: &mut Cpu) {
    normalize_vector(c, Coord(c.r[4]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, SCRATCH, cpu, run};

    /// The vector functions (one coorddef in, and back or a result) on random vectors.
    #[test]
    fn vectors() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(77);
        let (a, b) = (Coord(SCRATCH), Coord(SCRATCH + 12));
        let cases: [(fn(&mut Cpu), fn(&mut Cpu, Coord, Coord) -> i32, &str); 8] = [
            (nfs4rc::f_8009e1b0, |c, a, _| vector_length(c, a), "Math_VectorLength"),
            (nfs4rc::f_8009e34c, |c, a, _| vector_length2(c, a), "Math_VectorLength2"),
            (nfs4rc::f_8009e698, |c, a, _| { normalize_short_vector(c, a); 0 }, "Math_NormalizeShortVector"),
            (nfs4rc::f_8009e744, |c, a, _| { qd_normalize_vector(c, a); 0 }, "Math_QDNormalizeVector"),
            (nfs4rc::f_8009dea8, dist_xz, "Math_DistXZ"),
            (nfs4rc::f_8009defc, dist_3d, "Math_Dist3D"),
            (nfs4rc::f_8007df6c, crate::bworld::xzsquaredist32, "xzsquaredist32"),
            (nfs4rc::f_8009df8c, |c, a, b| better_dist(a.x(&mut c.m).wrapping_abs(), b.x(&mut c.m).wrapping_abs()), "Math_BetterDist"),
        ];
        for k in 0..40_000 {
            let v: Vec<u32> = (0..6).map(|_| rng.any() as u32).collect();
            let set = |c: &mut Cpu| {
                for (i, &e) in v.iter().enumerate() {
                    c.m.w32(SCRATCH + 4 * i as u32, e);
                }
            };
            let (orig, port, name) = cases[k % cases.len()];
            set(&mut c);
            let args = if name == "Math_BetterDist" { [(v[0] as i32).wrapping_abs() as u32, (v[3] as i32).wrapping_abs() as u32] } else { [a.0, b.0] };
            let want_v = run(&mut c, orig, &args);
            let want: Vec<u32> = (0..6).map(|i| c.m.r32(SCRATCH + 4 * i)).collect();
            set(&mut c);
            let got_v = port(&mut c, a, b) as u32;
            let got: Vec<u32> = (0..6).map(|i| c.m.r32(SCRATCH + 4 * i)).collect();
            assert_eq!(got, want, "{name} {v:x?}");
            if !name.starts_with("Math_Normalize") && !name.starts_with("Math_QD") {
                assert_eq!(got_v, want_v, "{name} {v:x?}");
            }
        }
        // fasttransmult, the output separate or one of the inputs
        let (ma, mb, mo) = (Matrix(SCRATCH), Matrix(SCRATCH + 36), Matrix(SCRATCH + 72));
        for k in 0..20_000 {
            let v: Vec<u32> = (0..18).map(|_| rng.any() as u32).collect();
            let out = [mo, ma, mb][k % 3];
            let set = |c: &mut Cpu| {
                for (i, &e) in v.iter().enumerate() {
                    c.m.w32(SCRATCH + 4 * i as u32, e);
                }
            };
            set(&mut c);
            run(&mut c, nfs4rc::f_8009e850, &[ma.0, mb.0, out.0]);
            let want: Vec<u32> = (0..27).map(|i| c.m.r32(SCRATCH + 4 * i)).collect();
            set(&mut c);
            fasttransmult(&mut c, ma, mb, out);
            assert_eq!((0..27).map(|i| c.m.r32(SCRATCH + 4 * i)).collect::<Vec<_>>(), want, "Math_fasttransmult");
        }
    }

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
