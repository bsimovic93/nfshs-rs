//! The bridge between decompiled and recompiled code: calling the original's functions with
//! its calling convention, and keeping the original's stack layout.
//!
//! A decompiled function that calls recompiled code, or passes the address of a local, runs in
//! a [`frame`] of the original's size, with those locals at the original's offsets in it. The
//! original reads stack it never wrote in places (`Physics_DoBarrierCheck` passes an unset
//! wall normal on: whatever an earlier call left at that place), so what the decompiled code
//! reads and leaves on the stack has to be where the original's would be.

use ps1rt::Cpu;

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
    let (r, hi, lo) = (c.r, c.hi, c.lo);
    let sp = c.r[29];
    for (i, &a) in args.iter().enumerate() {
        if i < 4 {
            c.r[4 + i] = a;
        } else {
            c.m.w32(sp + 4 * i as u32, a);
        }
    }
    c.r[31] = 0;
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
