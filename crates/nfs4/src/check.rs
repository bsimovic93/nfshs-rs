//! Shadow checks: a decompiled function against its recompiled original, on the same state.
//!
//! With `NFSHS_DECOMP_CHECK=all` (or a list of names), each call of a checked function from
//! recompiled code first runs the original, from a copy of the machine's state, with the
//! decompiled code stepped aside; then the decompiled function from the same state. Their
//! results must be the same: RAM and the scratchpad (but for the stack below the caller's,
//! which is dead; in a race it is in the scratchpad), the callee-saved registers, v0 for functions returning a value and the BIOS
//! rand() state. A mismatch is reported by field (`car[1].N.linearVel.y`) and the game goes
//! on with the original's results; `abort` in the list makes it stop instead, with both RAM
//! images written to `decomp_expected.bin` / `decomp_got.bin`.
//!
//! Both runs go without the host and the sound bus (the functions checked never need them;
//! one that does is reported and runs unchecked from then on). Checks don't nest: the
//! functions a checked one calls run as they are.

use crate::{FUNCS, count, set_suspended, types};
use ps1rt::Cpu;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

/// The stack below the caller's that is taken as dead (the frames of the function checked
/// and its callees).
const DEAD_STACK: u32 = 0x8000;
/// Mismatches reported in full; then only counted.
const REPORTS: u64 = 20;

struct Config {
    checked: Vec<bool>,
    abort: bool,
}

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let list = std::env::var("NFSHS_DECOMP_CHECK").unwrap_or_default();
        let words: Vec<&str> = list.split(',').map(str::trim).collect();
        let abort = words.contains(&"abort");
        let checked = if words.contains(&"all") {
            vec![true; FUNCS.len()]
        } else {
            let named = crate::named_in("NFSHS_DECOMP_CHECK", &["abort", "all"]).unwrap_or_default();
            (0..FUNCS.len()).map(|i| named.contains(&i)).collect()
        };
        Config { checked, abort }
    })
}

/// A replacement consulted while the host is detached ([`Cpu::inner_hook`]).
type Hook = fn(&mut Cpu, u32) -> bool;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Functions found to need the host: not checked again.
    static UNCHECKABLE: RefCell<Vec<bool>> = RefCell::new(vec![false; FUNCS.len()]);
    static NEEDED_HOST: Cell<u32> = const { Cell::new(0) };
    static OUTER_HOOK: Cell<Option<Hook>> = const { Cell::new(None) };
}

/// Whether this call of function `i` is to be checked.
pub fn wanted(i: usize) -> bool {
    config().checked[i] && !ACTIVE.with(Cell::get) && !UNCHECKABLE.with(|u| u.borrow()[i])
}

/// The machine state a function can change.
struct State {
    ram: Vec<u8>,
    scratch: [u8; 0x400],
    r: [u32; 32],
    hi: u32,
    lo: u32,
    rand_seed: u32,
    stack_len: usize,
}

impl State {
    fn save(c: &Cpu) -> Self {
        Self { ram: c.m.ram.clone(), scratch: c.m.scratch, r: c.r, hi: c.hi, lo: c.lo, rand_seed: c.rand_seed, stack_len: c.stack.len() }
    }
    fn restore(&self, c: &mut Cpu) {
        c.m.ram.copy_from_slice(&self.ram);
        c.m.scratch = self.scratch;
        (c.r, c.hi, c.lo, c.rand_seed) = (self.r, self.hi, self.lo, self.rand_seed);
        c.stack.truncate(self.stack_len);
        c.cont = 0;
    }
}

/// While checking: a call the host would have handled (an HLE function) is noted; the
/// replacements of a host call in progress still apply.
fn note_host_call(c: &mut Cpu, addr: u32) -> bool {
    if let Some(outer) = OUTER_HOOK.with(Cell::get)
        && outer(c, addr)
    {
        return true;
    }
    if nfs4rc::HLE.contains(&addr) {
        NEEDED_HOST.with(|n| n.set(addr));
    }
    false
}

/// Run function `i` checked against its original (see the module).
pub fn run(c: &mut Cpu, i: usize) {
    let f = &FUNCS[i];
    let entry = State::save(c);
    let sp = c.r[29];
    let host = c.host.take();
    let bus = c.m.bus.take();
    let hook = c.inner_hook.replace(note_host_call);
    OUTER_HOOK.with(|o| o.set(hook));
    NEEDED_HOST.with(|n| n.set(0));
    ACTIVE.with(|a| a.set(true));

    // the original
    set_suspended(true);
    if !nfs4rc::dispatch(c, f.addr) {
        panic!("{}: no recompiled original", f.name);
    }
    if c.cont != 0 {
        c.run_cont();
    }
    set_suspended(false);
    let expected = State::save(c);
    let needed = NEEDED_HOST.with(Cell::get);

    // the decompiled function, from the same state
    entry.restore(c);
    if needed == 0 {
        (f.run)(c);
    }

    ACTIVE.with(|a| a.set(false));
    c.inner_hook = hook;
    c.m.bus = bus;
    c.host = host;
    if needed != 0 {
        eprintln!("decomp check: {} calls the host ({needed:#x}): not checked", f.name);
        UNCHECKABLE.with(|u| u.borrow_mut()[i] = true);
        (f.run)(c);
        return;
    }
    count(i, |s| s.checks += 1);
    let diffs = compare(&expected, c, sp, f.returns);
    if diffs.is_empty() {
        return;
    }
    let mut n = 0;
    count(i, |s| {
        s.mismatches += 1;
        n = s.mismatches;
    });
    if n <= REPORTS {
        let calls = crate::stats()[i].calls;
        eprintln!("decomp check: {} (call {calls}, sp {sp:#x}) differs from the original:", f.name);
        for d in diffs.iter().take(24) {
            eprintln!("  {d}");
        }
        if diffs.len() > 24 {
            eprintln!("  ... {} more", diffs.len() - 24);
        }
    }
    if config().abort {
        std::fs::write("decomp_expected.bin", &expected.ram).ok();
        std::fs::write("decomp_got.bin", &c.m.ram).ok();
        panic!("decomp check: {} differs from the original (RAM images in decomp_expected.bin / decomp_got.bin)", f.name);
    }
    expected.restore(c);
}

/// What differs between the original's results and the machine's now.
fn compare(expected: &State, c: &mut Cpu, sp: u32, returns: bool) -> Vec<String> {
    let mut out = Vec::new();
    let sp_phys = (sp & 0x1F_FFFF) as usize;
    let dead = sp_phys.saturating_sub(DEAD_STACK as usize)..sp_phys;
    let got = &c.m.ram;
    let mut a = 0;
    while a < got.len() {
        if dead.contains(&a) {
            a = dead.end;
            continue;
        }
        // equal stretches skipped a block at a time, then the differing words
        let block = (a + 256).min(got.len()).min(if a < dead.start { dead.start } else { usize::MAX });
        if got[a..block] == expected.ram[a..block] {
            a = block;
            continue;
        }
        let w = a & !3;
        let e = u32::from_le_bytes(expected.ram[w..w + 4].try_into().unwrap());
        let g = u32::from_le_bytes(got[w..w + 4].try_into().unwrap());
        if e != g {
            out.push((0x8000_0000 | w as u32, e, g));
        }
        a = w + 4;
    }
    let mut lines: Vec<String> = Vec::new();
    for &(addr, e, g) in out.iter().take(64) {
        lines.push(format!("{addr:#010x} {}: expected {e:#010x} got {g:#010x}", describe(c, addr)));
    }
    if out.len() > 64 {
        lines.push(format!("({} differing words in all)", out.len()));
    }
    // in a race the stack is in the scratchpad: below sp it is dead
    let in_scratch = (sp & 0x1FFF_FFFF).wrapping_sub(0x1F80_0000);
    let scratch_dead = if in_scratch < 0x400 { in_scratch as usize } else { 0 };
    for (i, (e, g)) in expected.scratch.iter().zip(c.m.scratch.iter()).enumerate().skip(scratch_dead) {
        if e != g {
            lines.push(format!("scratchpad +{i:#x}: expected {e:#04x} got {g:#04x}"));
        }
    }
    for r in (16..24).chain([28, 29, 30, 31]) {
        if expected.r[r] != c.r[r] {
            lines.push(format!("register {r}: expected {:#x} got {:#x}", expected.r[r], c.r[r]));
        }
    }
    if returns && expected.r[2] != c.r[2] {
        lines.push(format!("v0: expected {:#x} got {:#x}", expected.r[2], c.r[2]));
    }
    if expected.rand_seed != c.rand_seed {
        lines.push(format!("rand seed: expected {:#x} got {:#x}", expected.rand_seed, c.rand_seed));
    }
    lines
}

/// `Cars_gList`: the simulated cars.
const CAR_LIST: u32 = 0x8011_0CA0;

/// A RAM address by the field it holds, where it is in one of the cars.
pub fn describe(c: &mut Cpu, addr: u32) -> String {
    describe_in(&c.m.ram, addr)
}

/// [`describe`] in a RAM image.
pub fn describe_in(ram: &[u8], addr: u32) -> String {
    let word = |a: u32| {
        let o = (a & 0x1F_FFFF) as usize;
        u32::from_le_bytes(ram[o..o + 4].try_into().unwrap())
    };
    for k in 0..32 {
        let car = word(CAR_LIST + 4 * k);
        if car & 0xFF00_0000 != 0x8000_0000 {
            break;
        }
        if (car..car + types::Car::SIZE).contains(&addr) {
            return format!("car[{k}].{}", field_path(types::struct_index("Car_tObj").unwrap(), addr - car));
        }
    }
    String::from("?")
}

/// The field at offset `off` of structure `s` (see [`types::STRUCTS`]), as a path.
pub fn field_path(s: usize, off: u32) -> String {
    let (_, _, fields) = types::STRUCTS[s];
    let Some(f) = fields.iter().rev().find(|f| f.off <= off && off < f.off + f.size.max(1)) else {
        return format!("+{off:#x}");
    };
    let mut rel = off - f.off;
    let mut name = f.name.to_string();
    if let Some(i) = rel.checked_div(f.elem) {
        name += &format!("[{i}]");
        rel %= f.elem;
    }
    match f.sub {
        Some(sub) => format!("{name}.{}", field_path(sub, rel)),
        None if rel != 0 => format!("{name}+{rel}"),
        None => name,
    }
}
