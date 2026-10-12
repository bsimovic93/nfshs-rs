//! Minimal PS1 machine model for statically recompiled game code (see tools/recomp.py).
//!
//! The original NFS4.EXE routines are translated 1:1 into Rust functions that operate on
//! this register file and memory image, so they reproduce the console's integer maths exactly.
//! Hardware is not emulated: functions that talk to it are replaced by a [`Host`].

pub mod bios;
mod interp;
mod gte;

pub const RAM_SIZE: usize = 0x20_0000;
const SCRATCH_BASE: u32 = 0x1F80_0000;
const SCRATCH_SIZE: usize = 0x400;

/// PS1 main RAM (2 MB, mirrored through KUSEG/KSEG0/KSEG1) plus the 1 KB scratchpad.
/// Hardware registers are not emulated: reads return 0 and writes are dropped (counted so
/// accidental hardware polling is detected), except for the SPU voices' current envelope
/// volume, which the host keeps so the sound driver can tell when a voice has ended.
pub struct Mem {
    pub ram: Vec<u8>,
    pub scratch: [u8; SCRATCH_SIZE],
    /// The SPU voices' current ADSR volume (0x1F801C0C + 16 * voice).
    pub spu_volume: [u16; 24],
    io: [u8; 4],
    pub io_accesses: u64,
    pub last_io: u32,
}

impl Default for Mem {
    fn default() -> Self {
        Self { ram: vec![0; RAM_SIZE], scratch: [0; SCRATCH_SIZE], spu_volume: [0; 24], io: [0; 4], io_accesses: 0, last_io: 0 }
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
            if (0x1F80_1C00..0x1F80_1D80).contains(&a) && a & 0xF == 0xC {
                self.io[..2].copy_from_slice(&self.spu_volume[((a - 0x1F80_1C00) >> 4) as usize].to_le_bytes());
            }
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
    /// The code has been spinning without calling the host (see [`Cpu::spin_limit`]).
    fn idle(&mut self, _c: &mut Cpu) {}
}

pub struct Cpu {
    pub r: [u32; 32],
    pub hi: u32,
    pub lo: u32,
    pub cop0: [u32; 32],
    /// GTE data (0..31) and control (32..63) registers. Commands are not executed: they only
    /// appear in rendering code, which the host engine replaces.
    pub gte_regs: [u32; 64],
    pub gte_commands: u64,
    /// Interrupt handlers for the 128 Hz root-counter timer (registered via OpenEvent).
    pub timer_handlers: Vec<u32>,
    /// Timer interrupts waiting to be delivered.
    pub pending_ticks: u32,
    in_irq: bool,
    /// Pending continuation address (a function ran past its end); see [`Cpu::run_cont`].
    pub cont: u32,
    /// Number of times execution fell back to the interpreter.
    pub interpreted: u64,
    /// Backward-branch counter (loop watchdog).
    pub ticks: u64,
    pub tick_limit: u64,
    /// Backward branches since the host last saw a call (the longest stretch is kept for
    /// diagnostics). After `spin_limit` of them the code is taken to be spinning on memory an
    /// interrupt would change, and the host lets time pass ([`Host::idle`]); 0 never.
    pub since_host: u64,
    pub max_since_host: u64,
    pub spin_limit: u64,
    /// The last calls the host saw (address, return address), for diagnosing hangs.
    pub recent: [(u32, u32); 16],
    recent_at: usize,
    pub m: Mem,
    pub host: Option<Box<dyn Host>>,
    pub calls: u64,
    pub trace: bool,
    /// Recompiled-function call stack (entry addresses), for diagnostics.
    pub stack: Vec<u32>,
    /// Entry point into recompiled code: returns false if `addr` is not a known function.
    pub dispatch: fn(&mut Cpu, u32) -> bool,
    /// Resume at an arbitrary address inside recompiled code (cross-function fall-through).
    pub dispatch_at: fn(&mut Cpu, u32) -> bool,
    /// Replacements consulted while the host itself is running (its nested calls cannot
    /// re-enter it): return true when handled. Results can be left in `capture`.
    pub inner_hook: Option<fn(&mut Cpu, u32) -> bool>,
    /// Calls a host hook asks for (function, arguments): run as soon as the hook returns, with
    /// the host available again (a hook cannot reach the host from a nested call).
    pub deferred: Vec<(u32, Vec<u32>)>,
    pub capture: Vec<u32>,
    /// State of the BIOS rand().
    pub rand_seed: u32,
}

impl Cpu {
    pub fn new(m: Mem) -> Self {
        Self { r: [0; 32], hi: 0, lo: 0, cop0: [0; 32], gte_regs: [0; 64], gte_commands: 0, timer_handlers: Vec::new(), pending_ticks: 0, in_irq: false, cont: 0, interpreted: 0, ticks: 0, tick_limit: u64::MAX, since_host: 0, max_since_host: 0, spin_limit: 0, recent: [(0, 0); 16], recent_at: 0, m, host: None, calls: 0, trace: false, stack: Vec::with_capacity(64), dispatch: |_, _| false, dispatch_at: |_, _| false, inner_hook: None, deferred: Vec::new(), capture: Vec::new(), rand_seed: 0 }
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
    /// Call `f` from host code in the middle of the game (e.g. inside a replaced function):
    /// registers are preserved and the callee gets a stack frame below the current one.
    pub fn call_nested(&mut self, f: u32, args: &[u32]) -> u32 {
        let (r, hi, lo) = (self.r, self.hi, self.lo);
        for (i, a) in args.iter().enumerate() {
            self.r[4 + i] = *a;
        }
        self.r[29] = (self.r[29] - 0x100) & !7;
        self.r[31] = 0;
        self.call(f);
        let ret = self.r[2];
        (self.r, self.hi, self.lo) = (r, hi, lo);
        ret
    }
    /// Call a function by address: BIOS vectors, host replacements, then recompiled code.
    pub fn call(&mut self, addr: u32) {
        self.calls += 1;
        self.tick();
        if self.trace {
            eprintln!("call {addr:#x} a0={:#x} a1={:#x} a2={:#x} ra={:#x}", self.r[4], self.r[5], self.r[6], self.r[31]);
        }
        if matches!(addr & 0x1FFF_FFFF, 0xA0 | 0xB0 | 0xC0) {
            bios::call(self, addr & 0xFF);
            return;
        }
        if let Some(mut h) = self.host.take() {
            self.max_since_host = self.max_since_host.max(self.since_host);
            self.since_host = 0;
            self.recent[self.recent_at % 16] = (addr, self.r[31]);
            self.recent_at += 1;
            let handled = h.call(self, addr);
            self.host = Some(h);
            while !self.deferred.is_empty() {
                let (f, args) = self.deferred.remove(0);
                self.call_nested(f, &args);
            }
            self.service_interrupts();
            if handled {
                return;
            }
        } else if let Some(hook) = self.inner_hook {
            if hook(self, addr) {
                return;
            }
        }
        if !(self.dispatch)(self, addr) {
            if self.trace {
                eprintln!("interpreting {addr:#x}");
            }
            self.interpreted += 1;
            self.interpret(addr);
        }
        if self.cont != 0 {
            self.run_cont();
        }
    }
    /// Deliver pending timer interrupts: run each registered handler with the CPU state saved
    /// and restored around it, as the PS1 interrupt dispatcher does.
    pub fn service_interrupts(&mut self) {
        if self.in_irq || self.pending_ticks == 0 || self.timer_handlers.is_empty() {
            return;
        }
        self.in_irq = true;
        let saved = (self.r, self.hi, self.lo, self.stack.len());
        while self.pending_ticks > 0 {
            self.pending_ticks -= 1;
            for h in self.timer_handlers.clone() {
                self.r[29] = saved.0[29].wrapping_sub(0x200) & !7; // interrupt stack below the current frame
                self.r[31] = 0;
                self.call(h);
            }
        }
        self.r = saved.0;
        self.hi = saved.1;
        self.lo = saved.2;
        self.stack.truncate(saved.3);
        self.in_irq = false;
    }

    #[inline]
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.since_host += 1;
        if self.spin_limit != 0 && self.since_host >= self.spin_limit && !self.in_irq {
            self.since_host = 0;
            if let Some(mut h) = self.host.take() {
                h.idle(self);
                self.host = Some(h);
                self.service_interrupts();
            }
        }
        if self.ticks > self.tick_limit {
            let recent: Vec<String> = (0..16).map(|i| self.recent[(self.recent_at + i) % 16]).map(|(a, ra)| format!("{a:#x}<-{ra:#x}")).collect();
            panic!("watchdog: {} backward branches; recent host calls {}", self.ticks, recent.join(" "));
        }
    }
    /// Trampoline for cross-function continuations: keeps resuming until no function leaves
    /// a continuation behind, without growing the Rust stack.
    pub fn run_cont(&mut self) {
        while self.cont != 0 {
            let pc = std::mem::take(&mut self.cont);
            self.tick();
            self.jump(pc);
        }
    }

    /// Control flowed past the end of a recompiled function (shared code / second entry).
    pub fn jump(&mut self, pc: u32) {
        if (self.dispatch)(self, pc) {
            return;
        }
        if !(self.dispatch_at)(self, pc) {
            self.interpreted += 1;
            self.interpret(pc);
        }
    }
    pub fn syscall(&mut self) {
        // SYSCALL 1/2 = Enter/ExitCriticalSection: interrupts are not emulated.
        self.r[2] = 1;
    }
    /// COP2 instruction: GTE command, register move or load/store (see gte.rs).
    pub fn gte(&mut self, w: u32) {
        let op = w >> 26;
        let rs = (w >> 21) & 31;
        let rt = ((w >> 16) & 31) as usize;
        let rd = ((w >> 11) & 31) as usize;
        match op {
            18 if w & (1 << 25) != 0 => {
                self.gte_commands += 1;
                self.gte_command(w & 0x1FF_FFFF);
            }
            18 => match rs {
                0 => {
                    let v = self.gte_read(rd);
                    if rt != 0 {
                        self.r[rt] = v;
                    }
                }
                2 => {
                    let v = self.gte_read(32 + rd);
                    if rt != 0 {
                        self.r[rt] = v;
                    }
                }
                4 => self.gte_write(rd, self.r[rt]),
                6 => self.gte_write(32 + rd, self.r[rt]),
                _ => {}
            },
            50 | 58 => {
                let base = ((w >> 21) & 31) as usize;
                let off = (w & 0xFFFF) as i16 as i32 as u32;
                let addr = self.r[base].wrapping_add(off);
                if op == 50 {
                    let v = self.m.r32(addr);
                    self.gte_write(rt, v);
                } else {
                    let v = self.gte_read(rt);
                    self.m.w32(addr, v);
                }
            }
            _ => {}
        }
    }
}
