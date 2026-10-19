//! The bridge between decompiled and recompiled code: calling the original's functions with
//! its calling convention, and room on the PS1 stack for what the original keeps there.

use ps1rt::Cpu;

/// Space a call leaves below the caller's stack: the callee's argument home slots (in a
/// race the stack is in the 1 KB scratchpad, and the original's frames nearly fill it).
const CALL_GAP: u32 = 0x10;

/// Call the recompiled function `f` (an `nfs4rc::f_XXXXXXXX`) as the original does with `jal`:
/// `args` in a0..a3 and from the fifth on at sp+16. Returns v0; every register is as before.
pub fn call(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> u32 {
    let (r, hi, lo) = (c.r, c.hi, c.lo);
    let sp = c.r[29].wrapping_sub(CALL_GAP + 4 * args.len() as u32) & !7;
    for (i, &a) in args.iter().enumerate() {
        if i < 4 {
            c.r[4 + i] = a;
        } else {
            c.m.w32(sp + 4 * i as u32, a);
        }
    }
    c.r[29] = sp;
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

/// Run `f` with `size` bytes reserved on the PS1 stack (8-byte aligned), for locals the
/// original passes to the functions it calls by address.
pub fn with_stack<R>(c: &mut Cpu, size: u32, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    let sp = c.r[29];
    let at = sp.wrapping_sub(size) & !7;
    c.r[29] = at;
    let r = f(c, at);
    c.r[29] = sp;
    r
}

/// The `n`th argument (0-based) of the function just entered with the original's calling
/// convention.
pub fn arg(c: &mut Cpu, n: u32) -> u32 {
    if n < 4 { c.r[4 + n as usize] } else { c.m.r32(c.r[29].wrapping_add(4 * n)) }
}
