//! Bridge to the recompiled original game code: runtime (`ps1rt`), generated NFS4.EXE
//! (`nfs4rc`) and the host-side hardware replacements.

pub mod host;
pub mod memcard;
pub mod video;
pub mod race;
pub mod spu;

pub use ps1rt::*;

/// A CPU wired to the recompiled NFS4.EXE.
pub fn new_cpu(m: Mem) -> Cpu {
    let mut c = Cpu::new(m);
    c.dispatch = nfs4rc::dispatch;
    c.dispatch_at = nfs4rc::dispatch_at;
    // the sound hardware the original's driver programs; interrupts enabled (COP0 SR IEc and
    // IM2) as the BIOS leaves them
    c.m.bus = Some(Box::new(spu::Spu::default()));
    c.cop0[12] = 0x401;
    c
}
