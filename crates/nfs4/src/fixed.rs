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
}
