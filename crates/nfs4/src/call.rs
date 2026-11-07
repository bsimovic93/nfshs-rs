//! Calls out of the readable ports (the front end's, rewritten without the original's stack and
//! register layout): the C calling convention and nothing more. Arguments go in a0..a3 and from
//! the fifth on in an outgoing area on the PS1 stack, as the callee expects; every register is as
//! before afterwards. Unlike [`crate::abi`], nothing the original leaves behind on the stack or in
//! registers is reproduced: `NFSHS_DECOMP_SCRAMBLE=1` (see [`crate::abi::scramble`]) showed that
//! nothing in the front end depends on it.
//!
//! A function the host hooks, a virtual function or any function pointer is called by address
//! ([`call_addr`]): `Cpu::call` lets the host see it and routes it to its port if it has one.

use ps1rt::Cpu;

/// Run `run` with `args` set up as a call's, and every register restored afterwards; v0 and v1.
fn invoke(c: &mut Cpu, args: &[u32], run: impl FnOnce(&mut Cpu)) -> (u32, u32) {
    let (regs, hi, lo) = (c.r, c.hi, c.lo);
    // the outgoing argument area: four words at least (the callee may store a0..a3 there)
    let size = (4 * args.len().max(4) as u32 + 7) & !7;
    let sp = c.r[29].wrapping_sub(size) & !7;
    c.r[29] = sp;
    for (i, &a) in args.iter().enumerate() {
        if i < 4 {
            c.r[4 + i] = a;
        } else {
            c.m.w32(sp + 4 * i as u32, a);
        }
    }
    c.r[31] = 0;
    run(c);
    if c.cont != 0 {
        c.run_cont();
    }
    let ret = (c.r[2], c.r[3]);
    (c.r, c.hi, c.lo) = (regs, hi, lo);
    ret
}

/// Call the game's function `f` (a recompiled `nfs4rc::f_XXXXXXXX`, or a port's `*_entry`) with
/// `args`; its result (v0).
pub fn call(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> u32 {
    invoke(c, args, f).0
}

/// [`call`] for a function returning a double or a 64-bit value: (low word, high word).
pub fn call64(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> (u32, u32) {
    invoke(c, args, f)
}

/// Call the function at `addr` with `args`, wherever it is (a port, recompiled, the host's):
/// for hooked functions, virtual functions and function pointers. Its result (v0).
pub fn call_addr(c: &mut Cpu, addr: u32, args: &[u32]) -> u32 {
    invoke(c, args, |c| c.call(addr)).0
}

/// The functions the recompiled code always calls through `Cpu::call` so the host sees them
/// (`tools/hook_funcs.txt`, the recompiler's list).
fn hooked(addr: u32) -> bool {
    static HOOKED: std::sync::OnceLock<std::collections::HashSet<u32>> = std::sync::OnceLock::new();
    HOOKED
        .get_or_init(|| {
            include_str!("../../../tools/hook_funcs.txt")
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .filter_map(|w| u32::from_str_radix(w, 16).ok())
                .collect()
        })
        .contains(&addr)
}

/// Call the game's function at `addr` with `args` as the original's `jal` does: to its port
/// if it has one, else straight into the recompiled code, and through the host (`Cpu::call`,
/// which ticks and restarts the host's spin count) only for the functions the host replaces
/// or hooks. [`call_addr`] always goes through the host, as a `jalr` does: in code that runs
/// while the race loads that changes when the host's clock moves on (see [`ticks`]).
pub fn call_jal(c: &mut Cpu, addr: u32, args: &[u32]) -> u32 {
    invoke(c, args, |c| jal(c, addr)).0
}

/// [`call_jal`]'s call alone, the arguments already set (for the bit-exact ports' `Saved`).
pub fn jal(c: &mut Cpu, addr: u32) {
    if hooked(addr) || !((c.decomp)(c, addr) || nfs4rc::dispatch(c, addr)) {
        c.call(addr);
    }
}

/// [`call_addr`] for a function returning a double or a 64-bit value.
pub fn call_addr64(c: &mut Cpu, addr: u32, args: &[u32]) -> (u32, u32) {
    invoke(c, args, |c| c.call(addr))
}

/// The C++ virtual function at `slot` of `obj`, whose vtable pointer is at `obj + vptr`: the
/// object adjusted by the entry's offset, and the function's address.
pub fn virtual_fn(c: &mut Cpu, obj: u32, vptr: u32, slot: u32) -> (u32, u32) {
    let m = &mut c.m;
    let vt = m.r32(obj.wrapping_add(vptr));
    let delta = m.r16(vt.wrapping_add(slot)) as i16 as i32 as u32;
    (obj.wrapping_add(delta), m.r32(vt.wrapping_add(slot + 4)))
}

/// Call `obj`'s virtual function at `slot` (its vtable pointer at `obj + vptr`) with `args`
/// after the object; its result.
pub fn call_virtual(c: &mut Cpu, obj: u32, vptr: u32, slot: u32, args: &[u32]) -> u32 {
    let (this, f) = virtual_fn(c, obj, vptr, slot);
    let mut all = Vec::with_capacity(args.len() + 1);
    all.push(this);
    all.extend_from_slice(args);
    call_addr(c, f, &all)
}

/// Run `f` with `size` bytes of the PS1 stack as a buffer (its address given), for data the
/// game's code takes by address: a string to print into, a rectangle, a file's header.
pub fn with_buffer<R>(c: &mut Cpu, size: u32, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    let sp = c.r[29];
    let at = sp.wrapping_sub(size) & !7;
    c.r[29] = at;
    let r = f(c, at);
    c.r[29] = sp;
    r
}

/// `n` of the original's loop passes: the recompiled code ticks the CPU at each backward branch
/// taken, and the host takes 20,000 ticks without a call to it for the game spinning (moving
/// its clock on), so ports of code that runs while the race loads tick as often as their
/// originals (DECOMP.md, "the spin-tick trap").
pub fn ticks(c: &mut Cpu, n: u32) {
    for _ in 0..n {
        c.tick();
    }
}
