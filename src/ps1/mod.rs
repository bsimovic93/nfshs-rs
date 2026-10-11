//! Minimal PS1 machine model for statically recompiled game code (see tools/recomp.py).
//!
//! The original NFS4.EXE routines are translated 1:1 into Rust functions that operate on
//! this register file and memory image, so they reproduce the console's integer maths exactly.

pub mod physics;

pub const RAM_SIZE: usize = 0x20_0000;
const SCRATCH_BASE: u32 = 0x1F80_0000;
const SCRATCH_SIZE: usize = 0x400;

/// PS1 main RAM (2 MB, mirrored through KUSEG/KSEG0/KSEG1) plus the 1 KB scratchpad.
pub struct Mem {
    pub ram: Vec<u8>,
    pub scratch: [u8; SCRATCH_SIZE],
}

impl Default for Mem {
    fn default() -> Self {
        Self { ram: vec![0; RAM_SIZE], scratch: [0; SCRATCH_SIZE] }
    }
}

impl Mem {
    #[inline]
    fn slot(&mut self, addr: u32, n: usize) -> &mut [u8] {
        let a = addr & 0x1FFF_FFFF;
        if (SCRATCH_BASE..SCRATCH_BASE + SCRATCH_SIZE as u32).contains(&a) {
            let o = (a - SCRATCH_BASE) as usize;
            &mut self.scratch[o..o + n]
        } else {
            let o = (a as usize) & (RAM_SIZE - 1);
            &mut self.ram[o..o + n]
        }
    }
    #[inline]
    pub fn r8(&mut self, a: u32) -> u8 {
        self.slot(a, 1)[0]
    }
    #[inline]
    pub fn r16(&mut self, a: u32) -> u16 {
        let s = self.slot(a, 2);
        u16::from_le_bytes([s[0], s[1]])
    }
    #[inline]
    pub fn r32(&mut self, a: u32) -> u32 {
        let s = self.slot(a, 4);
        u32::from_le_bytes([s[0], s[1], s[2], s[3]])
    }
    #[inline]
    pub fn w8(&mut self, a: u32, v: u8) {
        self.slot(a, 1)[0] = v;
    }
    #[inline]
    pub fn w16(&mut self, a: u32, v: u16) {
        self.slot(a, 2).copy_from_slice(&v.to_le_bytes());
    }
    #[inline]
    pub fn w32(&mut self, a: u32, v: u32) {
        self.slot(a, 4).copy_from_slice(&v.to_le_bytes());
    }
    pub fn read_bytes(&mut self, a: u32, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = self.r8(a.wrapping_add(i as u32));
        }
    }
    pub fn write_bytes(&mut self, a: u32, data: &[u8]) {
        for (i, b) in data.iter().enumerate() {
            self.w8(a.wrapping_add(i as u32), *b);
        }
    }
    /// Copy the PS-X EXE text/data image into RAM at its load address.
    pub fn load_exe(&mut self, exe: &[u8]) {
        let t_addr = u32::from_le_bytes(exe[0x18..0x1C].try_into().unwrap());
        let t_size = u32::from_le_bytes(exe[0x1C..0x20].try_into().unwrap()) as usize;
        self.write_bytes(t_addr, &exe[0x800..0x800 + t_size.min(exe.len() - 0x800)]);
    }
}

pub struct Cpu {
    pub r: [u32; 32],
    pub hi: u32,
    pub lo: u32,
    pub m: Mem,
}

impl Cpu {
    pub fn new(m: Mem) -> Self {
        Self { r: [0; 32], hi: 0, lo: 0, m }
    }
    #[inline]
    pub fn mult(&mut self, a: u32, b: u32) {
        let p = (a as i32 as i64) * (b as i32 as i64);
        self.lo = p as u32;
        self.hi = (p >> 32) as u32;
    }
    #[inline]
    pub fn multu(&mut self, a: u32, b: u32) {
        let p = (a as u64) * (b as u64);
        self.lo = p as u32;
        self.hi = (p >> 32) as u32;
    }
    #[inline]
    pub fn div(&mut self, a: u32, b: u32) {
        let (a, b) = (a as i32, b as i32);
        if b == 0 {
            self.lo = if a < 0 { 1 } else { 0xFFFF_FFFF };
            self.hi = a as u32;
        } else if a == i32::MIN && b == -1 {
            self.lo = a as u32;
            self.hi = 0;
        } else {
            self.lo = (a / b) as u32;
            self.hi = (a % b) as u32;
        }
    }
    #[inline]
    pub fn divu(&mut self, a: u32, b: u32) {
        if b == 0 {
            self.lo = 0xFFFF_FFFF;
            self.hi = a;
        } else {
            self.lo = a / b;
            self.hi = a % b;
        }
    }
    pub fn lwl(&mut self, addr: u32, cur: u32) -> u32 {
        let word = self.m.r32(addr & !3);
        let sh = (3 - (addr & 3)) * 8;
        let mask = if sh == 0 { 0 } else { 0xFFFF_FFFFu32 >> (32 - sh) };
        (cur & mask) | (word << sh)
    }
    pub fn lwr(&mut self, addr: u32, cur: u32) -> u32 {
        let word = self.m.r32(addr & !3);
        let sh = (addr & 3) * 8;
        let mask = if sh == 0 { 0 } else { 0xFFFF_FFFFu32 << (32 - sh) };
        (cur & mask) | (word >> sh)
    }
    pub fn swl(&mut self, addr: u32, v: u32) {
        let a = addr & !3;
        let word = self.m.r32(a);
        let sh = (3 - (addr & 3)) * 8;
        let keep = if sh == 0 { 0 } else { 0xFFFF_FFFFu32 << (32 - sh) };
        self.m.w32(a, (word & keep) | (v >> sh));
    }
    pub fn swr(&mut self, addr: u32, v: u32) {
        let a = addr & !3;
        let word = self.m.r32(a);
        let sh = (addr & 3) * 8;
        let keep = if sh == 0 { 0 } else { 0xFFFF_FFFFu32 >> (32 - sh) };
        self.m.w32(a, (word & keep) | (v << sh));
    }
    pub fn call(&mut self, addr: u32) {
        if !physics::dispatch(self, addr) {
            panic!("call to non-recompiled function {addr:#x}");
        }
    }
    pub fn syscall(&mut self) {
        panic!("syscall in recompiled code");
    }
}
