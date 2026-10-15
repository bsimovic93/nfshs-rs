//! Fallback MIPS interpreter for code the static recompiler did not cover (indirect-only
//! entry points, shared tails). Calls into recompiled functions whenever possible.

use crate::Cpu;

#[inline]
fn sx(v: u32) -> u32 {
    (v & 0xFFFF) as i16 as i32 as u32
}

impl Cpu {
    /// Interpret from `pc` until the current function returns (a `jr ra` at call depth 0).
    pub fn interpret(&mut self, mut pc: u32) {
        let mut depth = 0u32;
        let mut npc = pc.wrapping_add(4);
        loop {
            let w = self.m.r32(pc);
            let op = w >> 26;
            let rs = ((w >> 21) & 31) as usize;
            let rt = ((w >> 16) & 31) as usize;
            let rd = ((w >> 11) & 31) as usize;
            let sh = (w >> 6) & 31;
            let imm = w & 0xFFFF;
            let mut nnpc = npc.wrapping_add(4);
            // pending call target (executed after the delay slot)
            let mut call: Option<u32> = None;
            let mut ret = false;
            macro_rules! set {
                ($r:expr, $v:expr) => {{
                    let v = $v;
                    if $r != 0 {
                        self.r[$r] = v;
                    }
                }};
            }
            match op {
                0 => match w & 63 {
                    0 => set!(rd, self.r[rt] << sh),
                    2 => set!(rd, self.r[rt] >> sh),
                    3 => set!(rd, ((self.r[rt] as i32) >> sh) as u32),
                    4 => set!(rd, self.r[rt].wrapping_shl(self.r[rs] & 31)),
                    6 => set!(rd, self.r[rt].wrapping_shr(self.r[rs] & 31)),
                    7 => set!(rd, ((self.r[rt] as i32).wrapping_shr(self.r[rs] & 31)) as u32),
                    8 => {
                        if rs == 31 {
                            ret = true;
                        }
                        nnpc = self.r[rs];
                    }
                    9 => {
                        let t = self.r[rs];
                        set!(rd, npc.wrapping_add(4));
                        call = Some(t);
                    }
                    12 => self.syscall(),
                    13 => panic!("break at {pc:#x}"),
                    16 => set!(rd, self.hi),
                    17 => self.hi = self.r[rs],
                    18 => set!(rd, self.lo),
                    19 => self.lo = self.r[rs],
                    24 => self.mult(self.r[rs], self.r[rt]),
                    25 => self.multu(self.r[rs], self.r[rt]),
                    26 => self.div(self.r[rs], self.r[rt]),
                    27 => self.divu(self.r[rs], self.r[rt]),
                    32 | 33 => set!(rd, self.r[rs].wrapping_add(self.r[rt])),
                    34 | 35 => set!(rd, self.r[rs].wrapping_sub(self.r[rt])),
                    36 => set!(rd, self.r[rs] & self.r[rt]),
                    37 => set!(rd, self.r[rs] | self.r[rt]),
                    38 => set!(rd, self.r[rs] ^ self.r[rt]),
                    39 => set!(rd, !(self.r[rs] | self.r[rt])),
                    42 => set!(rd, ((self.r[rs] as i32) < (self.r[rt] as i32)) as u32),
                    43 => set!(rd, (self.r[rs] < self.r[rt]) as u32),
                    f => panic!("interp: special {f} at {pc:#x}"),
                },
                1 => {
                    let v = self.r[rs] as i32;
                    let taken = if rt & 1 != 0 { v >= 0 } else { v < 0 };
                    if rt & 16 != 0 {
                        self.r[31] = npc.wrapping_add(4);
                    }
                    if taken {
                        nnpc = npc.wrapping_add(sx(imm) << 2);
                    }
                }
                2 => nnpc = (npc & 0xF000_0000) | ((w & 0x03FF_FFFF) << 2),
                3 => {
                    self.r[31] = npc.wrapping_add(4);
                    call = Some((npc & 0xF000_0000) | ((w & 0x03FF_FFFF) << 2));
                }
                4..=7 => {
                    let (a, b) = (self.r[rs], self.r[rt]);
                    let taken = match op {
                        4 => a == b,
                        5 => a != b,
                        6 => (a as i32) <= 0,
                        _ => (a as i32) > 0,
                    };
                    if taken {
                        nnpc = npc.wrapping_add(sx(imm) << 2);
                    }
                }
                8 | 9 => set!(rt, self.r[rs].wrapping_add(sx(imm))),
                10 => set!(rt, ((self.r[rs] as i32) < (sx(imm) as i32)) as u32),
                11 => set!(rt, (self.r[rs] < sx(imm)) as u32),
                12 => set!(rt, self.r[rs] & imm),
                13 => set!(rt, self.r[rs] | imm),
                14 => set!(rt, self.r[rs] ^ imm),
                15 => set!(rt, imm << 16),
                16 => match rs {
                    0 => set!(rt, self.cop0[rd]),
                    4 => self.cop0[rd] = self.r[rt],
                    _ => {}
                },
                18 | 50 | 58 => self.gte(w),
                32..=46 => {
                    let a = self.r[rs].wrapping_add(sx(imm));
                    match op {
                        32 => set!(rt, self.m.r8(a) as i8 as i32 as u32),
                        33 => set!(rt, self.m.r16(a) as i16 as i32 as u32),
                        34 => set!(rt, self.lwl(a, self.r[rt])),
                        35 => set!(rt, self.m.r32(a)),
                        36 => set!(rt, self.m.r8(a) as u32),
                        37 => set!(rt, self.m.r16(a) as u32),
                        38 => set!(rt, self.lwr(a, self.r[rt])),
                        40 => self.m.w8(a, self.r[rt] as u8),
                        41 => self.m.w16(a, self.r[rt] as u16),
                        42 => self.swl(a, self.r[rt]),
                        43 => self.m.w32(a, self.r[rt]),
                        46 => self.swr(a, self.r[rt]),
                        _ => panic!("interp: mem op {op} at {pc:#x}"),
                    }
                }
                _ => panic!("interp: op {op} at {pc:#x} ({w:#010x})"),
            }
            // branch/jump delay slot
            if nnpc != npc.wrapping_add(4) || call.is_some() || ret {
                let delay_pc = npc;
                let saved_target = nnpc;
                // execute delay slot as a single-step interpretation
                self.step_one(delay_pc);
                if let Some(t) = call {
                    let back = self.r[31];
                    // prefer recompiled code; otherwise recurse in the interpreter
                    if (self.dispatch)(self, t) {
                        if self.cont != 0 {
                            self.run_cont();
                        }
                        pc = back;
                        npc = back.wrapping_add(4);
                    } else {
                        depth += 1;
                        pc = t;
                        npc = t.wrapping_add(4);
                    }
                    continue;
                }
                if ret {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                if saved_target <= delay_pc {
                    self.tick();
                }
                pc = saved_target;
                npc = pc.wrapping_add(4);
                // jumping into the start of a recompiled function (tail call)
                continue;
            }
            pc = npc;
            npc = nnpc;
        }
    }

    /// Execute exactly one non-control-flow instruction (branch delay slots).
    fn step_one(&mut self, pc: u32) {
        let w = self.m.r32(pc);
        let op = w >> 26;
        let is_cf = matches!(op, 1..=7) || (op == 0 && matches!(w & 63, 8 | 9));
        if w == 0 || is_cf {
            return;
        }
        // reuse the interpreter loop body by interpreting a single instruction at `pc`
        let saved31 = self.r[31];
        let _ = saved31;
        let rs = ((w >> 21) & 31) as usize;
        let rt = ((w >> 16) & 31) as usize;
        let rd = ((w >> 11) & 31) as usize;
        let sh = (w >> 6) & 31;
        let imm = w & 0xFFFF;
        macro_rules! set {
            ($r:expr, $v:expr) => {{
                let v = $v;
                if $r != 0 {
                    self.r[$r] = v;
                }
            }};
        }
        match op {
            0 => match w & 63 {
                0 => set!(rd, self.r[rt] << sh),
                2 => set!(rd, self.r[rt] >> sh),
                3 => set!(rd, ((self.r[rt] as i32) >> sh) as u32),
                4 => set!(rd, self.r[rt].wrapping_shl(self.r[rs] & 31)),
                6 => set!(rd, self.r[rt].wrapping_shr(self.r[rs] & 31)),
                7 => set!(rd, ((self.r[rt] as i32).wrapping_shr(self.r[rs] & 31)) as u32),
                16 => set!(rd, self.hi),
                17 => self.hi = self.r[rs],
                18 => set!(rd, self.lo),
                19 => self.lo = self.r[rs],
                24 => self.mult(self.r[rs], self.r[rt]),
                25 => self.multu(self.r[rs], self.r[rt]),
                26 => self.div(self.r[rs], self.r[rt]),
                27 => self.divu(self.r[rs], self.r[rt]),
                32 | 33 => set!(rd, self.r[rs].wrapping_add(self.r[rt])),
                34 | 35 => set!(rd, self.r[rs].wrapping_sub(self.r[rt])),
                36 => set!(rd, self.r[rs] & self.r[rt]),
                37 => set!(rd, self.r[rs] | self.r[rt]),
                38 => set!(rd, self.r[rs] ^ self.r[rt]),
                39 => set!(rd, !(self.r[rs] | self.r[rt])),
                42 => set!(rd, ((self.r[rs] as i32) < (self.r[rt] as i32)) as u32),
                43 => set!(rd, (self.r[rs] < self.r[rt]) as u32),
                _ => {}
            },
            8 | 9 => set!(rt, self.r[rs].wrapping_add(sx(imm))),
            10 => set!(rt, ((self.r[rs] as i32) < (sx(imm) as i32)) as u32),
            11 => set!(rt, (self.r[rs] < sx(imm)) as u32),
            12 => set!(rt, self.r[rs] & imm),
            13 => set!(rt, self.r[rs] | imm),
            14 => set!(rt, self.r[rs] ^ imm),
            15 => set!(rt, imm << 16),
            16 => match rs {
                0 => set!(rt, self.cop0[rd]),
                4 => self.cop0[rd] = self.r[rt],
                _ => {}
            },
            18 | 50 | 58 => self.gte(w),
            32..=46 => {
                let a = self.r[rs].wrapping_add(sx(imm));
                match op {
                    32 => set!(rt, self.m.r8(a) as i8 as i32 as u32),
                    33 => set!(rt, self.m.r16(a) as i16 as i32 as u32),
                    34 => set!(rt, self.lwl(a, self.r[rt])),
                    35 => set!(rt, self.m.r32(a)),
                    36 => set!(rt, self.m.r8(a) as u32),
                    37 => set!(rt, self.m.r16(a) as u32),
                    38 => set!(rt, self.lwr(a, self.r[rt])),
                    40 => self.m.w8(a, self.r[rt] as u8),
                    41 => self.m.w16(a, self.r[rt] as u16),
                    42 => self.swl(a, self.r[rt]),
                    43 => self.m.w32(a, self.r[rt]),
                    46 => self.swr(a, self.r[rt]),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
