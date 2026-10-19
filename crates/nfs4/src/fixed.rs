//! The EA fixed-point library the game's maths is built on (16.16 numbers; angles with a full
//! turn = 0x10000). The original routines are hand-written assembly; these give their exact
//! results, rounding and overflow included.

use crate::globals;
use ps1rt::Mem;

/// `a / b` as the R3000's `div` leaves it (the compiler's checks only trap): truncated
/// toward zero; by 0, -1 or 1 by the sign of `a`; `i32::MIN / -1` stays `i32::MIN`.
pub fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        if a < 0 { 1 } else { -1 }
    } else {
        a.wrapping_div(b)
    }
}

/// `fixedmult` (0x800e4f14): `a * b` in 16.16, rounded to nearest (halves up).
pub fn fixedmult(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64 + 0x8000) >> 16) as i32
}

/// `fixeddiv` (0x800e4ff0): `a / b` in 16.16, truncated toward zero; 0 when `b` is 0. The
/// whole part wraps past 16 bits.
pub fn fixeddiv(a: i32, b: i32) -> i32 {
    let negative = (a < 0) != (b < 0);
    let (mut rem, mut div) = (a.unsigned_abs(), b.unsigned_abs());
    if div == 0 {
        return 0;
    }
    let mut q = (rem / div) << 16;
    rem %= div;
    // the 16 fraction bits by long division; while the remainder has its top bit set it can't
    // double, and the divisor halves instead (so its low bits are lost)
    let step = |rem: &mut u32, div: &mut u32, top: bool| if top { *div >>= 1 } else { *rem <<= 1 };
    let top = (rem as i32) < 0;
    step(&mut rem, &mut div, top);
    let mut bit = 0x10000u32;
    while bit != 1 {
        bit >>= 1;
        let top = (rem as i32) < 0;
        if rem >= div {
            q |= bit;
            rem = rem.wrapping_sub(div);
        }
        step(&mut rem, &mut div, top);
    }
    if negative { (q as i32).wrapping_neg() } else { q as i32 }
}

/// `fixedsqrt` (0x800eba08): the square root of a 16.16 number, from a table by the position
/// of its top bit: `x * slope >> 16 + offset` (truncated).
pub fn fixedsqrt(m: &mut Mem, x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    let entry = globals::SQRT_SLOPES - 4 * x.leading_zeros();
    let slope = m.r32(entry) as i32;
    let offset = m.r32(entry + 132) as i32;
    (((x as i64 * slope as i64) >> 16) as i32).wrapping_add(offset)
}

/// `fastintsin` (0x800f261c): the sine of a 1024-step angle (16.16), from the quarter-wave
/// table.
pub fn fastintsin(m: &mut Mem, a: i32) -> i32 {
    let i = (a & 0xFF) as u32;
    let v = if a & 0x100 == 0 { m.r32(globals::SINE_TABLE + 4 * i) } else { m.r32(globals::SINE_TABLE + 4 * (256 - i)) } as i32;
    if a & 0x200 == 0 { v } else { v.wrapping_neg() }
}

/// `fastintcos` (0x800f2618): a quarter turn on, the sine.
pub fn fastintcos(m: &mut Mem, a: i32) -> i32 {
    fastintsin(m, a.wrapping_add(256))
}

/// The table steps of a full-turn angle and the fraction between them (`fixedsin`/`fixedcos`
/// interpolate with the derivative: 411774 / 512 is 2 pi / 1024 in 16.16 per 1/64 step).
fn interpolate(m: &mut Mem, a: i32, base: fn(&mut Mem, i32) -> i32, slope: fn(&mut Mem, i32) -> i32) -> i32 {
    let step = a >> 6;
    let v = base(m, step);
    let d = slope(m, step);
    let frac = (a & 63).wrapping_mul(411_774) >> 9;
    v.wrapping_add((d >> 2).wrapping_mul(frac) >> 21)
}

/// `fixedsin` (0x800edd80): the sine of a full-turn angle (0x10000 = 360 degrees), 16.16.
pub fn fixedsin(m: &mut Mem, a: i32) -> i32 {
    interpolate(m, a, fastintsin, fastintcos)
}

/// `fixedcos` (0x800ede00).
pub fn fixedcos(m: &mut Mem, a: i32) -> i32 {
    interpolate(m, a, fastintcos, |m, a| fastintsin(m, a).wrapping_neg())
}

/// `make64` (0x800fee34): `v` shifted left by `shift` as a 64-bit number (lo, hi). Shifts of
/// 32 and more take only the low 5 bits of the rest; 0 and less sign-extend `v`.
pub fn make64(v: i32, shift: i32) -> (u32, u32) {
    if shift >= 32 {
        (0, (v as u32) << ((shift - 32) & 31))
    } else if shift <= 0 {
        (v as u32, (v >> 31) as u32)
    } else {
        ((v as u32) << shift, (v >> (32 - shift)) as u32)
    }
}

/// `divu64` (0x800fee8c): the 64-bit number (hi, lo) divided by `d`, 32 quotient bits by
/// restoring division (the remainder's carry out of 32 bits is lost).
pub fn divu64(lo: u32, hi: u32, d: u32) -> u32 {
    let (mut lo, mut rem, mut q) = (lo, hi, 0u32);
    for _ in 0..32 {
        q <<= 1;
        rem = (rem << 1).wrapping_add(lo >> 31);
        if rem >= d {
            rem = rem.wrapping_sub(d);
            q += 1;
        }
        lo <<= 1;
    }
    q
}

/// `fixedatan` (0x800ede84): the angle of (x, y) as a full-turn angle, from the arctangent
/// table over the first octant (0x2000 = 45 degrees).
pub fn fixedatan(m: &mut Mem, y: i32, x: i32) -> i32 {
    // octant: bit 1 x < 0, bit 2 y < 0, bit 0 |y| > |x|
    let mut octant = 0;
    let (mut lo, mut hi) = (y, x);
    if hi < 0 {
        octant = 2;
        hi = hi.wrapping_neg();
    }
    if lo < 0 {
        octant |= 4;
        lo = lo.wrapping_neg();
    }
    let a = if lo == hi {
        0x2000
    } else {
        if hi < lo {
            (lo, hi) = (hi, lo);
            octant |= 1;
        }
        // lo / hi as a 32-bit fraction; its top byte indexes the table, the next 16 bits
        // interpolate
        let (l, h) = make64(lo, 32);
        let ratio = divu64(l, h, hi as u32);
        let i = ratio >> 24;
        let a0 = m.r32(globals::ATAN_TABLE + 4 * i);
        let a1 = m.r32(globals::ATAN_TABLE + 4 * (i + 1));
        (a0.wrapping_add(a1.wrapping_sub(a0).wrapping_mul((ratio >> 8) & 0xFFFF) >> 16)) as i32
    };
    match octant {
        0 => a,
        1 => 0x4000 - a,
        2 => 0x8000 - a,
        3 => a + 0x4000,
        4 => a.wrapping_neg(),
        5 => a - 0x4000,
        6 => a - 0x8000,
        _ => -a - 0x4000,
    }
}

/// `transpose` (0x800e4f44): `dst` = the transpose of `src` (they may be the same matrix).
pub fn transpose(m: &mut Mem, src: u32, dst: u32) {
    let e: [u32; 9] = std::array::from_fn(|i| m.r32(src + 4 * i as u32));
    for r in 0..3 {
        for k in 0..3 {
            m.w32(dst + 4 * (3 * r + k) as u32, e[3 * k + r]);
        }
    }
}

/// `fixedinverse` (0x800edd48): 1 / `a` in 16.16, rounded (0xFFFFFFFF / |a|, the sign put back).
pub fn fixedinverse(a: i32) -> i32 {
    let sign = a >> 31;
    let d = ((a ^ sign).wrapping_sub(sign)) as u32;
    // divu by 0: the quotient all ones, the remainder the dividend
    let (q, r) = u32::MAX.checked_div(d).map_or((u32::MAX, u32::MAX), |q| (q, u32::MAX % d));
    let half = ((d as i32) >> 1) as u32;
    let v = q.wrapping_add((half < r) as u32) as i32;
    (v ^ sign).wrapping_sub(sign)
}

/// `intatan` (0x800e6724): the angle of (x, y) in 1024ths of a turn, from the byte table of
/// the first octant (`atantbl`).
pub fn intatan(m: &mut Mem, y: i32, x: i32) -> i32 {
    let (mut lo, mut hi) = (y, x);
    let mut flip = false;
    let mut base = 0;
    if hi < 0 {
        hi = hi.wrapping_neg();
        flip = true;
        base = 512;
    }
    if lo < 0 {
        lo = lo.wrapping_neg();
        flip = !flip;
        base = -base;
    }
    if hi < lo {
        (lo, hi) = (hi, lo);
        base += if flip { -256 } else { 256 };
        flip = !flip;
    }
    let a = if lo == hi {
        128
    } else {
        // lo / hi in 1/256ths, rounded
        let i = if lo as u32 & 0xFF80_0000 != 0 {
            let (l, h) = make64(lo, 32);
            let q = divu64(l, h, hi as u32);
            (q >> 24) + ((q >> 23) & 1)
        } else {
            ((lo as u32) << 8).wrapping_add((hi >> 1) as u32) / hi as u32
        };
        m.r8(globals::ATAN_BYTES + i) as i32
    };
    if flip { base - a } else { base + a }
}

/// `intsincos` (0x800eb718): the sine and cosine of a 1024-step angle.
pub fn intsincos(m: &mut Mem, a: i32) -> (i32, i32) {
    (fastintsin(m, a), fastintcos(m, a))
}

/// `fixedsincos` (0x800f3fcc): the sine and cosine of a full-turn angle (0x10000), interpolated
/// between the table's steps (rounding the cosine its own way, unlike [`fixedcos`]).
pub fn fixedsincos(m: &mut Mem, a: i32) -> (i32, i32) {
    let (s, c) = intsincos(m, a >> 6);
    let frac = (a & 63).wrapping_mul(411_774) >> 9;
    (s.wrapping_add((c >> 2).wrapping_mul(frac) >> 21), c.wrapping_sub((s >> 2).wrapping_mul(frac) >> 21))
}

/// `fixedxformx` (0x800eb508): `out` = the rotation by `angle` about x.
pub fn fixedxformx(m: &mut Mem, out: u32, angle: i32) {
    let (s, c) = fixedsincos(m, angle);
    write_matrix(m, out, [0x1_0000, 0, 0, 0, c, s, 0, s.wrapping_neg(), c]);
}

/// `fixedxformy` (0x800eb56c): about y.
pub fn fixedxformy(m: &mut Mem, out: u32, angle: i32) {
    let (s, c) = fixedsincos(m, angle);
    write_matrix(m, out, [c, 0, s.wrapping_neg(), 0, 0x1_0000, 0, s, 0, c]);
}

/// `fixedxformz` (0x800eb5d0): about z.
pub fn fixedxformz(m: &mut Mem, out: u32, angle: i32) {
    let (s, c) = fixedsincos(m, angle);
    write_matrix(m, out, [c, s, 0, s.wrapping_neg(), c, 0, 0, 0, 0x1_0000]);
}

fn write_matrix(m: &mut Mem, at: u32, v: [i32; 9]) {
    for (i, e) in v.into_iter().enumerate() {
        m.w32(at + 4 * i as u32, e as u32);
    }
}

/// `crossproduct` (0x800eb440): `out` = `a` x `b`, written a component at a time as the
/// original does (`out` may be one of them).
pub fn crossproduct(m: &mut Mem, a: u32, b: u32, out: u32) {
    let r = |m: &mut Mem, p: u32, i: u32| m.r32(p + 4 * i) as i32;
    let x = fixedmult(r(m, a, 1), r(m, b, 2)).wrapping_sub(fixedmult(r(m, a, 2), r(m, b, 1)));
    m.w32(out, x as u32);
    let y = fixedmult(r(m, a, 2), r(m, b, 0)).wrapping_sub(fixedmult(r(m, a, 0), r(m, b, 2)));
    m.w32(out + 4, y as u32);
    let z = fixedmult(r(m, a, 0), r(m, b, 1)).wrapping_sub(fixedmult(r(m, a, 1), r(m, b, 0)));
    m.w32(out + 8, z as u32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Rng, cpu, run};

    const EDGES: &[i32] = &[0, 1, -1, 2, -2, 0x8000, -0x8000, 0xFFFF, 0x10000, -0x10000, 0x7FFF_FFFF, i32::MIN, 0x4000_0000, -0x4000_0000, 12345, -98765, 0x1234_5678];

    fn pairs(n: usize) -> Vec<(i32, i32)> {
        let mut rng = Rng(0x1234);
        let mut v: Vec<(i32, i32)> = EDGES.iter().flat_map(|&a| EDGES.iter().map(move |&b| (a, b))).collect();
        v.extend((0..n).map(|_| (rng.any(), rng.any())));
        v
    }

    #[test]
    fn mult_div() {
        let Some(mut c) = cpu() else { return };
        for (a, b) in pairs(100_000) {
            assert_eq!(fixedmult(a, b), run(&mut c, nfs4rc::f_800e4f14, &[a as u32, b as u32]) as i32, "fixedmult({a:#x}, {b:#x})");
            assert_eq!(fixeddiv(a, b), run(&mut c, nfs4rc::f_800e4ff0, &[a as u32, b as u32]) as i32, "fixeddiv({a:#x}, {b:#x})");
        }
    }

    #[test]
    fn sqrt_trig() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(77);
        let values: Vec<i32> = EDGES.iter().copied().chain((0..100_000).map(|_| rng.any())).collect();
        for &a in &values {
            assert_eq!(fixedsqrt(&mut c.m, a), run(&mut c, nfs4rc::f_800eba08, &[a as u32]) as i32, "fixedsqrt({a:#x})");
            assert_eq!(fixedsin(&mut c.m, a), run(&mut c, nfs4rc::f_800edd80, &[a as u32]) as i32, "fixedsin({a:#x})");
            assert_eq!(fixedcos(&mut c.m, a), run(&mut c, nfs4rc::f_800ede00, &[a as u32]) as i32, "fixedcos({a:#x})");
            // the table angles are 10 bits (callers pass small ones)
            let s = a & 0xFFFF;
            assert_eq!(fastintsin(&mut c.m, s), run(&mut c, nfs4rc::f_800f261c, &[s as u32]) as i32, "fastintsin({s:#x})");
            assert_eq!(fastintcos(&mut c.m, s), run(&mut c, nfs4rc::f_800f2618, &[s as u32]) as i32, "fastintcos({s:#x})");
        }
    }

    #[test]
    fn atan() {
        let Some(mut c) = cpu() else { return };
        for (y, x) in pairs(100_000) {
            assert_eq!(fixedatan(&mut c.m, y, x), run(&mut c, nfs4rc::f_800ede84, &[y as u32, x as u32]) as i32, "fixedatan({y:#x}, {x:#x})");
        }
    }

    #[test]
    fn wide() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(5);
        let out = crate::test_util::SCRATCH;
        for _ in 0..20_000 {
            let v = rng.any();
            let shift = (rng.any() % 80) - 20;
            run(&mut c, nfs4rc::f_800fee34, &[out, v as u32, shift as u32]);
            assert_eq!(make64(v, shift), (c.m.r32(out), c.m.r32(out + 4)), "make64({v:#x}, {shift})");
            let (lo, hi, d) = (rng.any() as u32, rng.any() as u32, rng.any() as u32);
            assert_eq!(divu64(lo, hi, d), run(&mut c, nfs4rc::f_800fee8c, &[lo, hi, d]), "divu64({lo:#x}, {hi:#x}, {d:#x})");
        }
    }

    #[test]
    fn transposed() {
        let Some(mut c) = cpu() else { return };
        let (a, b) = (crate::test_util::SCRATCH, crate::test_util::SCRATCH + 0x40);
        let mut rng = Rng(9);
        for same in [false, true] {
            let src: Vec<u32> = (0..9).map(|_| rng.any() as u32).collect();
            let dst = if same { a } else { b };
            for (i, &v) in src.iter().enumerate() {
                c.m.w32(a + 4 * i as u32, v);
            }
            run(&mut c, nfs4rc::f_800e4f44, &[a, dst]);
            let want: Vec<u32> = (0..9).map(|i| c.m.r32(dst + 4 * i)).collect();
            for (i, &v) in src.iter().enumerate() {
                c.m.w32(a + 4 * i as u32, v);
            }
            transpose(&mut c.m, a, dst);
            let got: Vec<u32> = (0..9).map(|i| c.m.r32(dst + 4 * i)).collect();
            assert_eq!(got, want);
        }
    }
    #[test]
    fn inverse_atan_sincos() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(41);
        for (y, x) in pairs(50_000) {
            assert_eq!(fixedinverse(y), run(&mut c, nfs4rc::f_800edd48, &[y as u32]) as i32, "fixedinverse({y:#x})");
            assert_eq!(intatan(&mut c.m, y, x), run(&mut c, nfs4rc::f_800e6724, &[y as u32, x as u32]) as i32, "intatan({y:#x}, {x:#x})");
            let (s, k) = (crate::test_util::SCRATCH, crate::test_util::SCRATCH + 4);
            run(&mut c, nfs4rc::f_800f3fcc, &[y as u32, s, k]);
            assert_eq!(fixedsincos(&mut c.m, y), (c.m.r32(s) as i32, c.m.r32(k) as i32), "fixedsincos({y:#x})");
            run(&mut c, nfs4rc::f_800eb718, &[(y & 0xFFFF) as u32, s, k]);
            assert_eq!(intsincos(&mut c.m, y & 0xFFFF), (c.m.r32(s) as i32, c.m.r32(k) as i32), "intsincos({y:#x})");
        }
        let at = crate::test_util::SCRATCH + 0x100;
        for _ in 0..20_000 {
            let a = rng.any();
            for (f, port) in [(nfs4rc::f_800eb508 as fn(&mut ps1rt::Cpu), fixedxformx as fn(&mut Mem, u32, i32)), (nfs4rc::f_800eb56c, fixedxformy), (nfs4rc::f_800eb5d0, fixedxformz)] {
                run(&mut c, f, &[at, a as u32]);
                let want: Vec<u32> = (0..9).map(|i| c.m.r32(at + 4 * i)).collect();
                for i in 0..9 {
                    c.m.w32(at + 4 * i, 0xDEAD);
                }
                port(&mut c.m, at, a);
                assert_eq!((0..9).map(|i| c.m.r32(at + 4 * i)).collect::<Vec<_>>(), want, "fixedxform({a:#x})");
            }
        }
    }

    #[test]
    fn cross() {
        let Some(mut c) = cpu() else { return };
        let mut rng = Rng(42);
        let (a, b, o) = (crate::test_util::SCRATCH, crate::test_util::SCRATCH + 12, crate::test_util::SCRATCH + 24);
        for k in 0..30_000 {
            let v: Vec<u32> = (0..6).map(|_| rng.any() as u32).collect();
            // the output separate, or the same as an input
            let out = [o, a, b][k % 3];
            let set = |c: &mut ps1rt::Cpu| {
                for (i, &e) in v.iter().enumerate() {
                    c.m.w32(a + 4 * i as u32, e);
                }
            };
            set(&mut c);
            run(&mut c, nfs4rc::f_800eb440, &[a, b, out]);
            let want: Vec<u32> = (0..3).map(|i| c.m.r32(out + 4 * i)).collect();
            set(&mut c);
            crossproduct(&mut c.m, a, b, out);
            assert_eq!((0..3).map(|i| c.m.r32(out + 4 * i)).collect::<Vec<_>>(), want);
        }
    }
}
