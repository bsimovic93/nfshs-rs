//! Bridge to the recompiled original game code: runtime (`ps1rt`), generated NFS4.EXE
//! (`nfs4rc`) and the host-side hardware replacements.

pub mod gpu;
pub mod host;
pub mod race;

pub use ps1rt::*;

/// A CPU wired to the recompiled NFS4.EXE.
pub fn new_cpu(m: Mem) -> Cpu {
    let mut c = Cpu::new(m);
    c.dispatch = nfs4rc::dispatch;
    c.dispatch_at = nfs4rc::dispatch_at;
    c
}
