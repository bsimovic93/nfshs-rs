//! Minimal PS1 machine model for statically recompiled game code (see tools/recomp.py).
//!
//! The original NFS4.EXE routines are translated 1:1 into Rust functions that operate on
//! this register file and memory image, so they reproduce the console's integer maths exactly.
//! Hardware is not emulated: functions that talk to it are replaced by a [`Host`].

pub mod bios;

pub const RAM_SIZE: usize = 0x20_0000;
const SCRATCH_BASE: u32 = 0x1F80_0000;
const SCRATCH_SIZE: usize = 0x400;

/// PS1 main RAM (2 MB, mirrored through KUSEG/KSEG0/KSEG1) plus the 1 KB scratchpad.
/// Hardware registers are not emulated: reads return 0 and writes are dropped (counted so
/// accidental hardware polling is detected).
pub struct Mem {
    pub ram: Vec<u8>,
    pub scratch: [u8; SCRATCH_SIZE],
    io: [u8; 4],
    pub io_accesses: u64,
    pub last_io: u32,
}

impl Default for Mem {
    fn default() -> Self {
        Self { ram: vec![0; RAM_SIZE], scratch: [0; SCRATCH_SIZE], io: [0; 4], io_accesses: 0, last_io: 0 }
    }
}

impl Mem {
    #[inline]
    fn slot(&mut self, addr: u32, n: usize) -> &mut [u8] {
        let a = addr & 0x1FFF_FFFF;
        if (SCRATCH_BASE..SCRATCH_BASE + SCRATCH_SIZE as u32).contains(&a) {
            let o = (a - SCRATCH_BASE) as usize;
            &mut self.scratch[o..o + n]
        } else if a >= 0x0080_0000 {
            self.io_accesses += 1;
            self.last_io = a;
            if self.io_accesses > 20_000_000 {
                panic!("hardware polling at {a:#x}");
            }
            self.io = [0; 4];
            &mut self.io[..n]
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
    pub fn cstr(&mut self, a: u32) -> String {
        let mut s = String::new();
        for i in 0..256 {
            let b = self.r8(a.wrapping_add(i));
            if b == 0 {
                break;
            }
            s.push(b as char);
        }
        s
    }
    /// Copy the PS-X EXE text/data image into RAM at its load address.
    pub fn load_exe(&mut self, exe: &[u8]) {
        let t_addr = u32::from_le_bytes(exe[0x18..0x1C].try_into().unwrap());
        let t_size = u32::from_le_bytes(exe[0x1C..0x20].try_into().unwrap()) as usize;
        self.write_bytes(t_addr, &exe[0x800..0x800 + t_size.min(exe.len() - 0x800)]);
    }
}

/// Host-side replacements for functions that touch hardware (files, GPU, timers...).
pub trait Host {
    /// Handle a call to `addr`; return false if not handled.
    fn call(&mut self, c: &mut Cpu, addr: u32) -> bool;
}

pub struct Cpu {
    pub r: [u32; 32],
    pub hi: u32,
    pub lo: u32,
    pub cop0: [u32; 32],
    pub m: Mem,
    pub host: Option<Box<dyn Host>>,
    pub calls: u64,
    pub trace: bool,
    /// Entry point into recompiled code: returns false if `addr` is not a known function.
    pub dispatch: fn(&mut Cpu, u32) -> bool,
}

impl Cpu {
    pub fn new(m: Mem) -> Self {
        Self { r: [0; 32], hi: 0, lo: 0, cop0: [0; 32], m, host: None, calls: 0, trace: false, dispatch: |_, _| false }
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
    /// Call a function by address: BIOS vectors, host replacements, then recompiled code.
    pub fn call(&mut self, addr: u32) {
        self.calls += 1;
        if self.trace {
            eprintln!("call {addr:#x} a0={:#x} a1={:#x} a2={:#x} ra={:#x}", self.r[4], self.r[5], self.r[6], self.r[31]);
        }
        if matches!(addr & 0x1FFF_FFFF, 0xA0 | 0xB0 | 0xC0) {
            bios::call(self, addr & 0xFF);
            return;
        }
        if let Some(mut h) = self.host.take() {
            let handled = h.call(self, addr);
            self.host = Some(h);
            if handled {
                return;
            }
        }
        if !(self.dispatch)(self, addr) {
            panic!("call to non-recompiled function {addr:#x}");
        }
    }
    pub fn syscall(&mut self) {
        // SYSCALL 1/2 = Enter/ExitCriticalSection: interrupts are not emulated.
        self.r[2] = 1;
    }
    pub fn gte(&mut self, w: u32) {
        panic!("GTE instruction {w:#010x} not supported");
    }
}
