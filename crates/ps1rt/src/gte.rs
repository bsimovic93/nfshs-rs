//! Geometry Transformation Engine (COP2): register file semantics and all commands, after
//! the hardware description in psx-spx ("Geometry Transformation Engine (GTE)").
//!
//! Registers live in `Cpu::gte_regs`: 0..31 data, 32..63 control.

use crate::Cpu;

const FLAG: usize = 32 + 31;

fn unr_table() -> &'static [u8; 0x101] {
    static T: std::sync::OnceLock<[u8; 0x101]> = std::sync::OnceLock::new();
    T.get_or_init(|| std::array::from_fn(|i| (((0x40000 / (i as i32 + 0x100) + 1) / 2) - 0x101).max(0) as u8))
}

/// The GTE's unsigned Newton-Raphson division `h * 0x20000 / sz3`, saturated to 0x1FFFF.
fn divide(h: u32, sz3: u32) -> Option<u32> {
    if h >= sz3 * 2 {
        return None;
    }
    let z = (sz3 as u16).leading_zeros();
    let n = (h as u64) << z;
    let d = (sz3 << z) as i64;
    let u = unr_table()[((d - 0x7FC0) >> 7) as usize] as i64 + 0x101;
    let d = (0x0200_0080 - d * u) >> 8;
    let d = (0x0000_0080 + d * u) >> 8;
    Some((((n as i64 * d + 0x8000) >> 16) as u64).min(0x1FFFF) as u32)
}

#[derive(Clone, Copy)]
struct Op {
    sf: u32,
    lm: bool,
    mx: u32,
    v: u32,
    cv: u32,
}

impl Cpu {
    // ---- register access with the hardware's read/write quirks

    pub fn gte_read(&self, r: usize) -> u32 {
        let g = &self.gte_regs;
        match r {
            1 | 3 | 5 | 8 | 9 | 10 | 11 => g[r] as i16 as i32 as u32,
            7 | 16 | 17 | 18 | 19 => g[r] & 0xFFFF,
            15 => g[14],
            28 | 29 => {
                let c = |v: u32| ((v as i16 as i32) >> 7).clamp(0, 0x1F) as u32;
                c(g[9]) | c(g[10]) << 5 | c(g[11]) << 10
            }
            31 => {
                let v = g[30];
                (if v & 0x8000_0000 != 0 { (!v).leading_zeros() } else { v.leading_zeros() }).min(32)
            }
            // control registers read sign-extended where they are 16 bits wide
            36 | 44 | 52 | 58 | 59 | 61 | 62 => g[r] as i16 as i32 as u32,
            _ => g[r],
        }
    }

    pub fn gte_write(&mut self, r: usize, v: u32) {
        let g = &mut self.gte_regs;
        match r {
            15 => {
                g[12] = g[13];
                g[13] = g[14];
                g[14] = v;
            }
            28 => {
                g[28] = v & 0x7FFF;
                g[9] = (v & 0x1F) << 7;
                g[10] = ((v >> 5) & 0x1F) << 7;
                g[11] = ((v >> 10) & 0x1F) << 7;
            }
            29 | 31 => {}
            30 => g[30] = v,
            FLAG => g[FLAG] = v & 0x7FFF_F000 | if v & 0x7F87_E000 != 0 { 0x8000_0000 } else { 0 },
            _ => g[r] = v,
        }
    }

    // ---- helpers

    fn d(&self, r: usize) -> i64 {
        self.gte_regs[r] as i32 as i64
    }
    fn lo(&self, r: usize) -> i64 {
        self.gte_regs[r] as i16 as i64
    }
    fn hi(&self, r: usize) -> i64 {
        (self.gte_regs[r] >> 16) as i16 as i64
    }
    fn flag(&mut self, bit: u32) {
        self.gte_regs[FLAG] |= 1 << bit;
    }
    fn ir(&self, i: usize) -> i64 {
        self.gte_regs[8 + i] as i16 as i64
    }

    /// 3x3 matrix starting at control register `base` (RT = 32, LLM = 40, LCM = 48).
    fn matrix(&self, base: usize) -> [[i64; 3]; 3] {
        let e = |k: usize| if k % 2 == 0 { self.lo(base + k / 2) } else { self.hi(base + k / 2) };
        [[e(0), e(1), e(2)], [e(3), e(4), e(5)], [e(6), e(7), e(8)]]
    }
    fn vector(&self, v: u32) -> [i64; 3] {
        match v {
            0..=2 => {
                let b = v as usize * 2;
                [self.lo(b), self.hi(b), self.lo(b + 1)]
            }
            _ => [self.ir(1), self.ir(2), self.ir(3)],
        }
    }

    /// Check a MAC1..3 result for 44-bit overflow, shift it and store it.
    fn set_mac(&mut self, i: usize, v: i64, shift: u32) -> i64 {
        if v > 0x7FF_FFFF_FFFF {
            self.flag(31 - i as u32);
        } else if v < -0x800_0000_0000 {
            self.flag(28 - i as u32);
        }
        let r = v >> shift;
        self.gte_regs[24 + i] = r as u32;
        r
    }
    fn set_mac0(&mut self, v: i64) -> i64 {
        if v > 0x7FFF_FFFF {
            self.flag(16);
        } else if v < -0x8000_0000 {
            self.flag(15);
        }
        self.gte_regs[24] = v as u32;
        v
    }
    fn set_ir(&mut self, i: usize, v: i64, lm: bool) {
        let min = if lm { 0 } else { -0x8000 };
        let c = v.clamp(min, 0x7FFF);
        if c != v {
            self.flag(25 - i as u32);
        }
        self.gte_regs[8 + i] = c as u32;
    }
    fn set_ir0(&mut self, v: i64) {
        let c = v.clamp(0, 0x1000);
        if c != v {
            self.flag(12);
        }
        self.gte_regs[8] = c as u32;
    }
    fn macs_to_irs(&mut self, lm: bool) {
        for i in 1..=3 {
            let v = self.gte_regs[24 + i] as i32 as i64;
            self.set_ir(i, v, lm);
        }
    }
    fn push_sz(&mut self, v: i64) {
        let c = v.clamp(0, 0xFFFF);
        if c != v {
            self.flag(18);
        }
        let g = &mut self.gte_regs;
        g[16] = g[17];
        g[17] = g[18];
        g[18] = g[19];
        g[19] = c as u32;
    }
    fn push_sxy(&mut self, x: i64, y: i64) {
        let cx = x.clamp(-0x400, 0x3FF);
        let cy = y.clamp(-0x400, 0x3FF);
        if cx != x {
            self.flag(14);
        }
        if cy != y {
            self.flag(13);
        }
        let g = &mut self.gte_regs;
        g[12] = g[13];
        g[13] = g[14];
        g[14] = (cx as u16 as u32) | (cy as u16 as u32) << 16;
    }
    fn push_color(&mut self) {
        let code = self.gte_regs[6] >> 24;
        let mut c = [0u32; 3];
        for i in 0..3 {
            let v = (self.gte_regs[25 + i] as i32 as i64) >> 4;
            let s = v.clamp(0, 0xFF);
            if s != v {
                self.flag(21 - i as u32);
            }
            c[i] = s as u32;
        }
        let g = &mut self.gte_regs;
        g[20] = g[21];
        g[21] = g[22];
        g[22] = c[0] | c[1] << 8 | c[2] << 16 | code << 24;
    }

    /// MAC = (cv * 0x1000 + M * V) >> shift for one matrix/vector/translation choice.
    fn mul_mat_vec(&mut self, m: [[i64; 3]; 3], v: [i64; 3], t: [i64; 3], o: Op) {
        for i in 0..3 {
            let mut acc = t[i] << 12;
            for j in 0..3 {
                acc += m[i][j] * v[j];
            }
            self.set_mac(i + 1, acc, o.sf * 12);
        }
        self.macs_to_irs(o.lm);
    }

    fn rtp(&mut self, vi: u32, o: Op, last: bool) {
        let m = self.matrix(32);
        let v = self.vector(vi);
        let tr = [self.d(37), self.d(38), self.d(39)];
        let mut mac = [0i64; 3];
        for i in 0..3 {
            let acc = (tr[i] << 12) + m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2];
            mac[i] = self.set_mac(i + 1, acc, o.sf * 12);
        }
        self.set_ir(1, mac[0], o.lm);
        self.set_ir(2, mac[1], o.lm);
        // IR3 quirk: the saturation flag looks at the value shifted by 12 regardless of sf
        let ir3 = mac[2].clamp(if o.lm { 0 } else { -0x8000 }, 0x7FFF);
        self.gte_regs[11] = ir3 as u32;
        let raw3 = ((tr[2] << 12) + m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]) >> 12;
        if raw3 != raw3.clamp(-0x8000, 0x7FFF) {
            self.flag(22);
        }
        self.push_sz(mac[2] >> ((1 - o.sf) * 12));
        let h = self.gte_regs[32 + 26] & 0xFFFF;
        let n = match divide(h, self.gte_regs[19]) {
            Some(n) => n as i64,
            None => {
                self.flag(17);
                0x1FFFF
            }
        };
        let x = self.set_mac0(n * self.ir(1) + self.d(32 + 24));
        let y = self.set_mac0(n * self.ir(2) + self.d(32 + 25));
        self.push_sxy(x >> 16, y >> 16);
        if last {
            let dq = self.set_mac0(n * self.lo(32 + 27) + self.d(32 + 28));
            self.set_ir0(dq >> 12);
        }
    }

    /// Depth cue: interpolate MAC1..3 (unshifted) towards the far colour by IR0, then shift.
    fn depth_cue(&mut self, mac: [i64; 3], o: Op) {
        let fc = [self.d(32 + 21), self.d(32 + 22), self.d(32 + 23)];
        let ir0 = self.ir(0);
        for i in 0..3 {
            let t = ((fc[i] << 12) - mac[i]) >> (o.sf * 12);
            self.set_ir(i + 1, t, false);
        }
        for i in 0..3 {
            let v = self.ir(i + 1) * ir0 + mac[i];
            self.set_mac(i + 1, v, o.sf * 12);
        }
        self.macs_to_irs(o.lm);
        self.push_color();
    }

    fn rgbc(&self) -> [i64; 3] {
        let c = self.gte_regs[6];
        [(c & 0xFF) as i64, ((c >> 8) & 0xFF) as i64, ((c >> 16) & 0xFF) as i64]
    }

    /// Normal colour pipeline for vertex `vi`: `kind` 0 = NC, 1 = NCC, 2 = NCD.
    fn nc(&mut self, vi: u32, o: Op, kind: u32) {
        let llm = self.matrix(40);
        let v = self.vector(vi);
        self.mul_mat_vec(llm, v, [0; 3], o);
        self.color_stage(o, kind);
    }

    /// BK + LCM * IR, then the colour (CC) or depth (DP) part.
    fn color_stage(&mut self, o: Op, kind: u32) {
        let lcm = self.matrix(48);
        let ir = self.vector(3);
        let bk = [self.d(32 + 13), self.d(32 + 14), self.d(32 + 15)];
        self.mul_mat_vec(lcm, ir, bk, o);
        match kind {
            0 => self.push_color(),
            1 => {
                let c = self.rgbc();
                for i in 0..3 {
                    let v = (c[i] * self.ir(i + 1)) << 4;
                    self.set_mac(i + 1, v, o.sf * 12);
                }
                self.macs_to_irs(o.lm);
                self.push_color();
            }
            _ => {
                let c = self.rgbc();
                let mac = std::array::from_fn(|i| (c[i] * self.ir(i + 1)) << 4);
                self.depth_cue(mac, o);
            }
        }
    }

    /// Execute a GTE command word (COP2 imm25).
    pub fn gte_command(&mut self, w: u32) {
        let o = Op { sf: (w >> 19) & 1, lm: w & (1 << 10) != 0, mx: (w >> 17) & 3, v: (w >> 15) & 3, cv: (w >> 13) & 3 };
        self.gte_regs[FLAG] = 0;
        match w & 0x3F {
            0x01 => self.rtp(0, o, true),
            0x30 => {
                self.rtp(0, o, false);
                self.rtp(1, o, false);
                self.rtp(2, o, true);
            }
            0x06 => {
                let s = |r: usize| (self.lo(r), self.hi(r));
                let ((x0, y0), (x1, y1), (x2, y2)) = (s(12), s(13), s(14));
                self.set_mac0(x0 * y1 + x1 * y2 + x2 * y0 - x0 * y2 - x1 * y0 - x2 * y1);
            }
            0x0C => {
                let (d1, d2, d3) = (self.lo(32), self.lo(34), self.lo(36));
                let (i1, i2, i3) = (self.ir(1), self.ir(2), self.ir(3));
                self.set_mac(1, i3 * d2 - i2 * d3, o.sf * 12);
                self.set_mac(2, i1 * d3 - i3 * d1, o.sf * 12);
                self.set_mac(3, i2 * d1 - i1 * d2, o.sf * 12);
                self.macs_to_irs(o.lm);
            }
            0x10 => {
                let c = self.rgbc();
                self.depth_cue(c.map(|x| x << 16), o);
            }
            0x2A => {
                for _ in 0..3 {
                    let c = self.gte_regs[20];
                    let c = [(c & 0xFF) as i64, ((c >> 8) & 0xFF) as i64, ((c >> 16) & 0xFF) as i64];
                    self.depth_cue(c.map(|x| x << 16), o);
                }
            }
            0x11 => {
                let ir = self.vector(3);
                self.depth_cue(ir.map(|x| x << 12), o);
            }
            0x29 => {
                let c = self.rgbc();
                let mac = std::array::from_fn(|i| (c[i] * self.ir(i + 1)) << 4);
                self.depth_cue(mac, o);
            }
            0x12 => {
                let m = match o.mx {
                    0 => self.matrix(32),
                    1 => self.matrix(40),
                    2 => self.matrix(48),
                    _ => {
                        // garbage matrix of the hardware
                        let r = (self.gte_regs[6] & 0xFF) as i64 * 16;
                        [[-r, r, self.ir(0)], [self.lo(33), self.lo(33), self.lo(33)], [self.hi(34), self.hi(34), self.hi(34)]]
                    }
                };
                let v = self.vector(o.v);
                let t = match o.cv {
                    0 => [self.d(37), self.d(38), self.d(39)],
                    1 => [self.d(45), self.d(46), self.d(47)],
                    2 => [self.d(53), self.d(54), self.d(55)],
                    _ => [0; 3],
                };
                if o.cv == 2 {
                    // hardware bug: only the last two columns reach MAC; the first only sets flags
                    for i in 0..3 {
                        let first = (t[i] << 12) + m[i][0] * v[0];
                        self.set_ir(i + 1, first >> (o.sf * 12), false);
                        self.set_mac(i + 1, m[i][1] * v[1] + m[i][2] * v[2], o.sf * 12);
                    }
                    self.macs_to_irs(o.lm);
                } else {
                    self.mul_mat_vec(m, v, t, o);
                }
            }
            0x13 => self.nc(0, o, 2),
            0x16 => {
                for v in 0..3 {
                    self.nc(v, o, 2);
                }
            }
            0x1B => self.nc(0, o, 1),
            0x3F => {
                for v in 0..3 {
                    self.nc(v, o, 1);
                }
            }
            0x1E => self.nc(0, o, 0),
            0x20 => {
                for v in 0..3 {
                    self.nc(v, o, 0);
                }
            }
            0x1C => self.color_stage(o, 1),
            0x14 => self.color_stage(o, 2),
            0x28 => {
                for i in 1..=3 {
                    let v = self.ir(i) * self.ir(i);
                    self.set_mac(i, v, o.sf * 12);
                }
                self.macs_to_irs(o.lm);
            }
            0x2D => {
                let s = self.lo(32 + 29) * (self.gte_read(17) + self.gte_read(18) + self.gte_read(19)) as i64;
                let m = self.set_mac0(s);
                self.set_otz(m >> 12);
            }
            0x2E => {
                let s = self.lo(32 + 30) * (self.gte_read(16) + self.gte_read(17) + self.gte_read(18) + self.gte_read(19)) as i64;
                let m = self.set_mac0(s);
                self.set_otz(m >> 12);
            }
            0x3D => {
                let ir0 = self.ir(0);
                for i in 1..=3 {
                    let v = self.ir(i) * ir0;
                    self.set_mac(i, v, o.sf * 12);
                }
                self.macs_to_irs(o.lm);
                self.push_color();
            }
            0x3E => {
                let ir0 = self.ir(0);
                for i in 1..=3 {
                    let v = ((self.gte_regs[24 + i] as i32 as i64) << (o.sf * 12)) + self.ir(i) * ir0;
                    self.set_mac(i, v, o.sf * 12);
                }
                self.macs_to_irs(o.lm);
                self.push_color();
            }
            _ => {}
        }
        let f = self.gte_regs[FLAG];
        if f & 0x7F87_E000 != 0 {
            self.gte_regs[FLAG] = f | 0x8000_0000;
        }
    }

    fn set_otz(&mut self, v: i64) {
        let c = v.clamp(0, 0xFFFF);
        if c != v {
            self.flag(18);
        }
        self.gte_regs[7] = c as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_matches_reference_points() {
        // result = h / sz in 16.16
        assert_eq!(divide(0x100, 0x100), Some(0x10000));
        assert_eq!(divide(0x100, 0x200), Some(0x8000));
        assert_eq!(divide(0x1FF, 0x100), Some(0x1FF00));
        assert_eq!(divide(0x200, 0x100), None);
    }

    #[test]
    fn rtps_projects_a_point() {
        let mut c = Cpu::new(crate::Mem::default());
        // identity rotation (1.0 = 0x1000), translation 0, H = 256, offset (160, 120) in 16.16
        c.gte_write(32, 0x1000);
        c.gte_write(34, 0x1000);
        c.gte_write(36, 0x1000);
        c.gte_write(32 + 24, 160 << 16);
        c.gte_write(32 + 25, 120 << 16);
        c.gte_write(32 + 26, 256);
        // V0 = (100, 50, 512)
        c.gte_write(0, 100 | 50 << 16);
        c.gte_write(1, 512);
        c.gte_command(0x0018_0001); // RTPS sf=1
        let sxy = c.gte_read(14);
        assert_eq!((sxy as i16, (sxy >> 16) as i16), (160 + 50, 120 + 25));
        assert_eq!(c.gte_read(19), 512);
    }
}
