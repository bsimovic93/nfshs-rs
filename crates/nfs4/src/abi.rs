//! The bridge between decompiled and recompiled code: calling the original's functions with
//! its calling convention, and keeping the original's stack layout.
//!
//! A decompiled function that calls recompiled code, or passes the address of a local, runs in
//! a [`frame`] of the original's size, with those locals at the original's offsets in it. The
//! original reads stack it never wrote in places (`Physics_DoBarrierCheck` passes an unset
//! wall normal on: whatever an earlier call left at that place), so what the decompiled code
//! reads and leaves on the stack has to be where the original's would be.

use ps1rt::Cpu;
use std::cell::RefCell;
use std::collections::HashMap;

/// Run `f` in a stack frame of `size` bytes, as the original's prologue makes one; `f` gets
/// its stack pointer.
pub fn frame<R>(c: &mut Cpu, size: u32, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    let sp = c.r[29];
    let at = sp.wrapping_sub(size);
    c.r[29] = at;
    let r = f(c, at);
    c.r[29] = sp;
    r
}

/// Call the recompiled function `f` (an `nfs4rc::f_XXXXXXXX`) from inside a [`frame`], as the
/// original does with `jal`: `args` in a0..a3 and from the fifth on at sp+16 (the frame's
/// outgoing arguments). Returns v0; every register is as before.
pub fn call(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> u32 {
    call_from(c, f, args, 0, &[])
}

/// [`call`] as from the original's call site: the return address `ra` and the caller's
/// s-registers from s0 on (`saved`) in the registers, where the callee's prologue saves them
/// on the stack (for code that later reads stack it never wrote). Every register is as
/// before afterwards.
pub fn call_from(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32], ra: u32, saved: &[u32]) -> u32 {
    call_with(c, args, ra, saved, |c| {
        f(c);
        if c.cont != 0 {
            c.run_cont();
        }
    })
}

/// The arguments in a0..a3 and from the fifth on at sp+16, and the return address.
fn set_args(c: &mut Cpu, args: &[u32], ra: u32) {
    scramble::before_call(c);
    let sp = c.r[29];
    for (i, &a) in args.iter().enumerate() {
        if i < 4 {
            c.r[4 + i] = a;
        } else {
            c.m.w32(sp + 4 * i as u32, a);
        }
    }
    c.r[31] = ra;
}

/// [`call_from`] with `run` making the call.
fn call_with(c: &mut Cpu, args: &[u32], ra: u32, saved: &[u32], run: impl FnOnce(&mut Cpu)) -> u32 {
    let r = c.r;
    c.r[16..16 + saved.len()].copy_from_slice(saved);
    set_args(c, args, ra);
    let (hi, lo) = (c.hi, c.lo);
    run(c);
    let ret = c.r[2];
    (c.r, c.hi, c.lo) = (r, hi, lo);
    ret
}

/// The callee-saved registers as the original function being ported has them, s0..s7 and fp
/// (`[8]`), kept up to date by the port where the original sets them. Its callees' prologues
/// save them in their frames, where later code reads them as leftovers (see
/// [`leave_prologue`]); a port that calls out often keeps one of these instead of naming the
/// registers at each call.
#[derive(Clone, Copy, Debug)]
pub struct Saved(pub [u32; 9]);

impl Saved {
    /// The registers as the function was entered with (its caller's).
    pub fn of(c: &Cpu) -> Self {
        let mut s = [0; 9];
        s[..8].copy_from_slice(&c.r[16..24]);
        s[8] = c.r[30];
        Self(s)
    }

    fn load(&self, c: &mut Cpu) {
        c.r[16..24].copy_from_slice(&self.0[..8]);
        c.r[30] = self.0[8];
    }

    /// [`call_from`] the call site returning to `ra`, with these registers.
    pub fn call(&self, c: &mut Cpu, f: fn(&mut Cpu), args: &[u32], ra: u32) -> u32 {
        let r = c.r;
        self.load(c);
        let v = call_from(c, f, args, ra, &[]);
        c.r = r;
        v
    }

    /// [`Saved::call`], the callee's scratch registers (a0..a3, t, v, hi/lo) left as it
    /// leaves them: for a following call that reads one it isn't given (a variadic `sprintf`
    /// homes a2 and a3 whatever they hold).
    pub fn call_leaving(&self, c: &mut Cpu, f: fn(&mut Cpu), args: &[u32], ra: u32) -> u32 {
        let r = c.r;
        self.load(c);
        set_args(c, args, ra);
        f(c);
        if c.cont != 0 {
            c.run_cont();
        }
        scramble::after_call(c);
        // the callee-saved registers, gp, sp and the return address as before
        for k in (16..24).chain([28, 29, 30, 31]) {
            c.r[k] = r[k];
        }
        c.r[2]
    }

    /// [`Saved::call`] through a function pointer (the original's `jalr`): the function at
    /// `addr`, wherever it is (decompiled, recompiled or the host's).
    pub fn call_ptr(&self, c: &mut Cpu, addr: u32, args: &[u32], ra: u32) -> u32 {
        let r = c.r;
        self.load(c);
        let v = call_with(c, args, ra, &[], |c| c.call(addr));
        c.r = r;
        v
    }

    /// [`Saved::call_ptr`], the callee's scratch registers left as it leaves them (as
    /// [`Saved::call_leaving`]): for the original's virtual calls, whose callees get the
    /// registers the caller didn't set from the calls before.
    pub fn call_ptr_leaving(&self, c: &mut Cpu, addr: u32, args: &[u32], ra: u32) -> u32 {
        let r = c.r;
        self.load(c);
        set_args(c, args, ra);
        c.call(addr);
        scramble::after_call(c);
        for k in (16..24).chain([28, 29, 30, 31]) {
            c.r[k] = r[k];
        }
        c.r[2]
    }

    /// Run the decompiled function `f` (the original at `addr`) as called from the call site
    /// returning to `ra`, with these registers: its prologue's stores left as the original's
    /// would be, and the registers as before afterwards. `addr` is on [`Cpu::stack`] meanwhile
    /// (see [`on_stack`]).
    pub fn enter<R>(&self, c: &mut Cpu, addr: u32, ra: u32, f: impl FnOnce(&mut Cpu) -> R) -> R {
        let r = c.r;
        self.load(c);
        c.r[31] = ra;
        leave_prologue(c, addr, None, &[]);
        let v = on_stack(c, addr, f);
        c.r = r;
        v
    }

    /// [`Saved::enter`], the callee's scratch registers (a0..a3, t, v, hi/lo) left as it leaves
    /// them, as [`Saved::call_leaving`] does for a recompiled callee: for a later call that
    /// homes a register it isn't given.
    pub fn enter_leaving<R>(&self, c: &mut Cpu, addr: u32, ra: u32, f: impl FnOnce(&mut Cpu) -> R) -> R {
        let r = c.r;
        self.load(c);
        c.r[31] = ra;
        leave_prologue(c, addr, None, &[]);
        let v = on_stack(c, addr, f);
        for k in (16..24).chain([28, 29, 30, 31]) {
            c.r[k] = r[k];
        }
        v
    }
}

/// Run `f`, a decompiled function (the original at `addr`), with `addr` on [`Cpu::stack`] as
/// the recompiled original would have it: the host reads the stack to know what the game is
/// doing (the save the host started is `GenericMenuSaveGame` on it, a dialog's sound
/// `tDialog::Display` calling the sound), and without this a port's frames are missing.
pub fn on_stack<R>(c: &mut Cpu, addr: u32, f: impl FnOnce(&mut Cpu) -> R) -> R {
    let depth = c.stack.len();
    c.stack.push(addr);
    let v = f(c);
    c.stack.truncate(depth);
    v
}

/// A value as the original's load leaves it in a register: signed types sign-extended (`lb`,
/// `lh`), unsigned ones zero-extended (`lbu`, `lhu`). A port passes a field on as
/// `view.field(m).reg()` where the original loads it and passes the register.
pub trait Reg {
    fn reg(self) -> u32;
}

macro_rules! reg_extend {
    ($($t:ty),*) => {$(impl Reg for $t {
        fn reg(self) -> u32 {
            self as i32 as u32
        }
    })*};
}
reg_extend!(i8, i16, i32, u8, u16, u32);

impl Reg for bool {
    fn reg(self) -> u32 {
        self as u32
    }
}

/// Call the function at `addr` (a function pointer the original calls with `jalr`), wherever
/// it is: decompiled, recompiled or the host's.
pub fn call_addr(c: &mut Cpu, addr: u32, args: &[u32]) -> u32 {
    assert!(args.len() <= 4, "call_addr: stack arguments");
    c.call_nested(addr, args)
}

/// The `n`th argument (0-based) of the function just entered with the original's calling
/// convention.
pub fn arg(c: &mut Cpu, n: u32) -> u32 {
    if n < 4 { c.r[4 + n as usize] } else { c.m.r32(c.r[29].wrapping_add(4 * n)) }
}

/// What an original function's prologue saves: its frame size and `(register, offset)` of
/// each callee-saved register and the return address it stores (before its first branch, or
/// in that branch's delay slot).
struct Prologue {
    frame: u32,
    saves: Vec<(usize, u32)>,
}

fn prologue(m: &mut ps1rt::Mem, addr: u32) -> Prologue {
    let mut p = Prologue { frame: 0, saves: Vec::new() };
    let mut last = false;
    for k in 0..48 {
        let w = m.r32(addr + 4 * k);
        let (op, rs, rt, imm) = (w >> 26, (w >> 21) & 31, ((w >> 16) & 31) as usize, w as u16 as i16 as i32);
        if op == 0x09 && rs == 29 && rt == 29 && imm < 0 && p.frame == 0 {
            p.frame = imm.unsigned_abs();
        } else if op == 0x2B && rs == 29 && p.frame != 0 && matches!(rt, 16..=23 | 30 | 31) {
            p.saves.push((rt, imm as u32));
        }
        if last {
            break;
        }
        last = matches!(op, 1..=7 | 0x14..=0x17) || (op == 0 && matches!(w & 0x3F, 8 | 9));
    }
    p
}

thread_local! {
    static PROLOGUES: RefCell<HashMap<u32, Prologue>> = RefCell::new(HashMap::new());
}

/// Store what the original function at `addr` saves on entry (its caller's s-registers, its
/// return address) where its prologue saves it, in its frame just below the stack pointer:
/// the registers' values, but for `ra` (when given) and `regs` (`(register, value)`), the ones
/// a decompiled caller would have had in them.
///
/// The ports keep no registers, and later code reads stack it never wrote (a skid mark's
/// colour takes a byte of `Physics_CorrectPostCollisionYaw`'s saved return address). A call
/// from recompiled code has the caller's registers ([`crate::decomp`] replays every one); a
/// port calling a decompiled function where the leftovers matter passes its own.
pub fn leave_prologue(c: &mut Cpu, addr: u32, ra: Option<u32>, regs: &[(usize, u32)]) {
    PROLOGUES.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.entry(addr).or_insert_with(|| prologue(&mut c.m, addr));
        let sp = c.r[29].wrapping_sub(p.frame);
        for &(reg, off) in &p.saves {
            let v = match regs.iter().find(|r| r.0 == reg) {
                Some(&(_, v)) => v,
                None if reg == 31 => ra.unwrap_or(c.r[31]),
                None => c.r[reg],
            };
            c.m.w32(sp.wrapping_add(off), v);
        }
    });
}

/// `rand()`: the BIOS's, through the game's stub (0x800eb430).
pub fn rand(c: &mut Cpu) -> u32 {
    call(c, nfs4rc::f_800eb430, &[])
}

/// `NFSHS_DECOMP_SCRAMBLE=1`: an experiment for dropping the front end's stack and register
/// fidelity. The leftovers a front-end port reproduces or leaves are made junk, so whatever
/// still matches the original doesn't depend on them: the scratch registers when it is entered,
/// when a call it makes returns and when it returns (but v0, and its arguments on entry); the
/// s-registers it mirrors from the original and the dead stack below sp when it calls out. What a
/// port sets in a register itself before a call is kept (those places are in its code). Run with
/// `decomp-check --main` and `tools/tourshots.py`; a divergence names a reader of leftovers.
pub mod scramble {
    use ps1rt::Cpu;
    use std::cell::Cell;
    use std::sync::OnceLock;

    /// The front end's ports: FRONT.BIN, loaded below this.
    const FRONT_END_END: u32 = 0x8005_2000;
    /// The main stack (the front end's) and how much below sp is scrambled.
    const STACK: std::ops::Range<u32> = 0x801F_0000..0x8020_0000;
    const SPAN: u32 = 4096;

    pub fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var("NFSHS_DECOMP_SCRAMBLE").is_ok_and(|v| v == "1"))
    }

    pub fn front_end(addr: u32) -> bool {
        (0x8001_0000..FRONT_END_END).contains(&addr)
    }

    /// Whether the innermost function running is a front-end port (or recompiled front-end code).
    fn in_front_end(c: &Cpu) -> bool {
        on() && c.stack.last().is_some_and(|&f| front_end(f))
    }

    thread_local!(static SEED: Cell<u32> = const { Cell::new(0x9E37_79B9) });

    fn junk() -> u32 {
        SEED.with(|s| {
            let mut x = s.get();
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            s.set(x);
            x
        })
    }

    fn regs(c: &mut Cpu, which: impl Iterator<Item = usize>) {
        for k in which {
            c.r[k] = junk();
        }
        (c.hi, c.lo) = (junk(), junk());
    }

    fn stack(c: &mut Cpu) {
        let sp = c.r[29];
        if !STACK.contains(&sp) {
            return;
        }
        let lo = sp.saturating_sub(SPAN).max(STACK.start) & !3;
        let (a, b) = ((lo & 0x1F_FFFF) as usize, (sp & 0x1F_FFFC) as usize);
        let seed = junk();
        for (k, w) in c.m.ram[a..b].chunks_exact_mut(4).enumerate() {
            w.copy_from_slice(&(seed ^ (k as u32).wrapping_mul(0x0101_0101)).to_le_bytes());
        }
    }

    /// A front-end port is entered from recompiled code (after its prologue's stores): the
    /// scratch registers but its arguments, and the dead stack.
    pub fn on_entry(c: &mut Cpu) {
        regs(c, [1, 2, 3, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25].into_iter());
        stack(c);
    }

    /// A front-end port has returned to recompiled code: the scratch registers but v0 and v1 (a
    /// double comes back in both), and the
    /// dead stack.
    pub fn after_return(c: &mut Cpu) {
        regs(c, [1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25].into_iter());
        stack(c);
    }

    /// A front-end port calls out: the s-registers and fp it mirrors, and the dead stack.
    pub fn before_call(c: &mut Cpu) {
        if in_front_end(c) {
            regs(c, (16..24).chain([30]));
            stack(c);
        }
    }

    /// A call a front-end port made has returned: the callee's scratch registers but v0 and v1.
    pub fn after_call(c: &mut Cpu) {
        if in_front_end(c) {
            regs(c, [1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 24, 25].into_iter());
        }
    }
}
