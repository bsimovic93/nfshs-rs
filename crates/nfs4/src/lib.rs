//! NFS4.EXE decompiled to Rust: the original's functions rewritten as readable code, one at a
//! time, each producing exactly the recompiled original's results (`crates/nfs4rc`).
//!
//! The game's state stays in PS1 RAM, where the code not yet decompiled reads it; the
//! decompiled functions reach it through the typed views in [`types`]. The recompiled calls to
//! a decompiled function ask [`install`]'s hook first (tools/decomp_funcs.txt lists them), and
//! the recompiled original stays the fallback and the reference [`check`] compares against.
//!
//! Modules are named after the original's source files (PHYSICS.CPP is [`physics`]...).
//!
//! Switches (environment, read once):
//! - `NFSHS_DECOMP=off`: run the recompiled originals only.
//! - `NFSHS_DECOMP_OFF=Name,...`: those functions run recompiled (for bisecting).
//! - `NFSHS_DECOMP_ONLY=Name,...`: only those run decompiled. Decompiled functions call each
//!   other directly, so this is how a function called from decompiled code gets checked on
//!   its own: its callers then run recompiled and call it through the hook.
//! - `NFSHS_DECOMP_CHECK=all|Name,...[,abort]`: shadow checks (see [`check`]).
//! - `NFSHS_DECOMP_STATS=1`: calls and checks per function when a game thread ends.

pub mod abi;
pub mod cars;
pub mod check;
pub mod fixed;
pub mod globals;
pub mod mathnfs;
pub mod physics;
pub mod types;
pub mod vec;

use ps1rt::Cpu;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// A decompiled function: its retail address and name, and its entry with the original's
/// calling convention (arguments in a0..a3 and on the stack, result in v0).
pub struct Func {
    pub addr: u32,
    pub name: &'static str,
    pub run: fn(&mut Cpu),
    /// It returns a value (v0 is compared by the checks).
    pub returns: bool,
}

const fn func(addr: u32, name: &'static str, run: fn(&mut Cpu)) -> Func {
    Func { addr, name, run, returns: false }
}

const fn func_ret(addr: u32, name: &'static str, run: fn(&mut Cpu)) -> Func {
    Func { addr, name, run, returns: true }
}

/// Every decompiled function the game is routed to, by address.
pub static FUNCS: &[Func] = &[
    func(0x8008_67a8, "Cars_DoGravityEffectsOnAcc", cars::abi_do_gravity_effects_on_acc),
    func(0x8009_e0ec, "Math_ResolveRotatedVector", mathnfs::abi_resolve_rotated_vector),
    func(0x8009_e4e0, "Math_NormalizeVector", mathnfs::abi_normalize_vector),
    func(0x800a_935c, "Physics_InitCarSpecs", physics::abi_init_car_specs),
    func(0x800a_95f8, "Physics_CalculateDerivedCarSpecs", physics::abi_calculate_derived_car_specs),
    func(0x800a_9b08, "Physics_CheckGamedata", physics::abi_check_gamedata),
    func_ret(0x800a_9b10, "Physics_AttenuateVelocity", physics::abi_attenuate_velocity),
    func(0x800a_9f6c, "Physics_SetCurrentWallType", physics::abi_set_current_wall_type),
    func_ret(0x800a_9f78, "Physics_GetTorque", physics::abi_get_torque),
    func(0x800a_9fb4, "Physics_CorrectPostCollisionYaw", physics::abi_correct_post_collision_yaw),
    func_ret(0x800a_a14c, "Physics_DoBarrierCheck", physics::abi_do_barrier_check),
    func(0x800a_a6e4, "Physics_AutoShift", physics::abi_auto_shift),
    func(0x800a_a9a0, "Physics_RampCarControlValues", physics::abi_ramp_car_control_values),
    func(0x800a_b178, "Physics_FixEngineRpm", physics::abi_fix_engine_rpm),
    func(0x800a_b2d0, "Physics_ResetCar", physics::abi_reset_car),
    func(0x800a_b36c, "Physics_StopCar", physics::abi_stop_car),
    func(0x800a_b444, "Physics_TestForBarrierCollision", physics::abi_test_for_barrier_collision),
    func(0x800a_b49c, "Physics_CalculateRoadGripModifiers", physics::abi_calculate_road_grip_modifiers),
    func_ret(0x800a_b618, "Physics_CalculateCarAcceleration", physics::abi_calculate_car_acceleration),
    func(0x800a_c130, "Physics_CalcWheelLockAcc", physics::abi_calc_wheel_lock_acc),
    func(0x800a_c32c, "Physics_CalcTractionCircleAcc", physics::abi_calc_traction_circle_acc),
    func(0x800a_c6d0, "Physics_CalculateTireForces", physics::abi_calculate_tire_forces),
    func_ret(0x800a_cc38, "Physics_CalculateRSControlDesiredPosition", physics::abi_calculate_rs_control_desired_position),
    func(0x800a_cdf8, "Physics_Real", physics::abi_real),
    func(0x800a_e1dc, "Physics_SimCar", physics::abi_sim_car),
];

/// The index of the function at `addr` in [`FUNCS`].
pub fn find(addr: u32) -> Option<usize> {
    FUNCS.binary_search_by_key(&addr, |f| f.addr).ok()
}

/// The index of the function called `name`.
pub fn find_name(name: &str) -> Option<usize> {
    FUNCS.iter().position(|f| f.name == name)
}

static ENABLED: OnceLock<AtomicBool> = OnceLock::new();

fn enabled_flag() -> &'static AtomicBool {
    ENABLED.get_or_init(|| AtomicBool::new(std::env::var("NFSHS_DECOMP").map_or(true, |v| v != "off" && v != "0")))
}

/// Whether CPUs created from now on run the decompiled code (`NFSHS_DECOMP` sets the default).
pub fn enabled() -> bool {
    enabled_flag().load(Ordering::Relaxed)
}

/// Turn the decompiled code on or off for CPUs created from now on.
pub fn set_enabled(on: bool) {
    enabled_flag().store(on, Ordering::Relaxed);
}

/// The functions an environment variable names (comma-separated), as indices into [`FUNCS`];
/// words in `extra` are skipped.
pub(crate) fn named_in(var: &str, extra: &[&str]) -> Option<Vec<usize>> {
    let list = std::env::var(var).ok()?;
    let mut out = Vec::new();
    for name in list.split(',').map(str::trim).filter(|n| !n.is_empty() && !extra.contains(n)) {
        match find_name(name) {
            Some(i) => out.push(i),
            None => eprintln!("{var}: no decompiled function {name}"),
        }
    }
    Some(out)
}

/// Per function: turned off by `NFSHS_DECOMP_OFF` or left out of `NFSHS_DECOMP_ONLY`.
fn turned_off() -> &'static [bool] {
    static OFF: OnceLock<Vec<bool>> = OnceLock::new();
    OFF.get_or_init(|| {
        let mut off = vec![false; FUNCS.len()];
        if let Some(only) = named_in("NFSHS_DECOMP_ONLY", &[]) {
            off = (0..FUNCS.len()).map(|i| !only.contains(&i)).collect();
        }
        for i in named_in("NFSHS_DECOMP_OFF", &[]).unwrap_or_default() {
            off[i] = true;
        }
        off
    })
}

/// Calls and checks of one function on this thread.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub calls: u64,
    pub checks: u64,
    pub mismatches: u64,
}

struct Thread {
    /// The decompiled code steps aside (a check's reference run).
    suspended: Cell<bool>,
    stats: RefCell<Vec<Stats>>,
}

impl Drop for Thread {
    fn drop(&mut self) {
        if std::env::var_os("NFSHS_DECOMP_STATS").is_some() {
            print_stats(&self.stats.borrow());
        }
    }
}

thread_local! {
    static THREAD: Thread = Thread { suspended: Cell::new(false), stats: RefCell::new(vec![Stats::default(); FUNCS.len()]) };
}

pub(crate) fn set_suspended(on: bool) {
    THREAD.with(|t| t.suspended.set(on));
}

pub(crate) fn count(i: usize, f: impl FnOnce(&mut Stats)) {
    THREAD.with(|t| f(&mut t.stats.borrow_mut()[i]));
}

/// This thread's calls and checks per function (as [`FUNCS`]).
pub fn stats() -> Vec<Stats> {
    THREAD.with(|t| t.stats.borrow().clone())
}

pub fn print_stats(stats: &[Stats]) {
    eprintln!("decompiled functions (calls, checks, mismatches):");
    for (f, s) in FUNCS.iter().zip(stats) {
        eprintln!("  {:<44} {:>10} {:>10} {:>6}", f.name, s.calls, s.checks, s.mismatches);
    }
}

/// Route `c`'s calls to the decompiled functions (unless the decompiled code is turned off).
pub fn install(c: &mut Cpu) {
    if enabled() {
        c.decomp = decomp;
        c.dispatch = dispatch;
    }
}

/// [`Cpu::decomp`]: run the decompiled function at `addr`, if there is one and it is on.
pub fn decomp(c: &mut Cpu, addr: u32) -> bool {
    let Some(i) = find(addr) else { return false };
    if turned_off()[i] || THREAD.with(|t| t.suspended.get()) {
        return false;
    }
    count(i, |s| s.calls += 1);
    if check::wanted(i) {
        check::run(c, i);
    } else {
        (FUNCS[i].run)(c);
    }
    true
}

/// [`Cpu::dispatch`]: calls by address (function pointers) reach the decompiled code too.
pub fn dispatch(c: &mut Cpu, addr: u32) -> bool {
    decomp(c, addr) || nfs4rc::dispatch(c, addr)
}

#[cfg(test)]
mod tests {
    #[test]
    fn funcs_sorted() {
        assert!(super::FUNCS.windows(2).all(|w| w[0].addr < w[1].addr));
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use ps1rt::{Cpu, Mem};

    /// The game's gp (its small globals are gp-relative).
    pub const GP: u32 = 0x8013_D83C;

    /// Free RAM the tests put their structures in.
    pub const SCRATCH: u32 = 0x801F_0000;

    /// A CPU with NFS4.EXE loaded (its tables), or None when the game data is not there.
    pub fn cpu() -> Option<Cpu> {
        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gamedata/NFS4.EXE");
        let Ok(exe) = std::fs::read(&exe) else {
            eprintln!("{} not found: skipped", exe.display());
            return None;
        };
        let mut m = Mem::default();
        m.load_exe(&exe);
        let mut c = Cpu::new(m);
        c.dispatch = nfs4rc::dispatch;
        c.dispatch_at = nfs4rc::dispatch_at;
        c.r[28] = GP;
        c.r[29] = 0x801F_FF00;
        Some(c)
    }

    /// Run a recompiled original with `args`; its v0.
    pub fn run(c: &mut Cpu, f: fn(&mut Cpu), args: &[u32]) -> u32 {
        crate::abi::call(c, f, args)
    }

    /// xorshift, with small and round numbers mixed in.
    pub struct Rng(pub u64);
    impl Rng {
        pub fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        pub fn any(&mut self) -> i32 {
            let v = self.next();
            let x = (v >> 32) as i32;
            match v & 7 {
                0 => x >> (v >> 8 & 31),
                1 => x & 0x1FFFF,
                2 => (x & 0xFF) - 0x80,
                _ => x,
            }
        }
    }
}
