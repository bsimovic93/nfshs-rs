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
    let r = c.r;
    c.r[16..16 + saved.len()].copy_from_slice(saved);
    let sp = c.r[29];
    for (i, &a) in args.iter().enumerate() {
        if i < 4 {
            c.r[4 + i] = a;
        } else {
            c.m.w32(sp + 4 * i as u32, a);
        }
    }
    c.r[31] = ra;
    let (hi, lo) = (c.hi, c.lo);
    f(c);
    if c.cont != 0 {
        c.run_cont();
    }
    let ret = c.r[2];
    (c.r, c.hi, c.lo) = (r, hi, lo);
    ret
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
