//! Boot the recompiled NFS4.EXE straight into a race (no front end): load the executable
//! image, run the system start-up `main` performs, fill `GameSetup_gData` and call
//! `Nfs2_GameModuleStartUp`, then step the game loop.

use super::host::GameHost;
use super::{Cpu, Mem, new_cpu};
use anyhow::{Result, bail};
use std::path::Path;

/// Retail addresses (SLUS-00826).
pub mod addr {
    pub const GP: u32 = 0x8013_D83C;
    pub const STACK_TOP: u32 = 0x801F_FFF0;
    pub const BSS_START: u32 = 0x8013_F084;
    pub const BSS_END: u32 = 0x8014_9E40;
    /// `GameSetup_gData` (GameSetup_tData, 0xA28 bytes).
    pub const GAMESETUP: u32 = 0x8011_44A4;
    pub const GAMESETUP_SIZE: usize = 0xA28;
    pub const INIT_A: u32 = 0x800E_4C30;
    pub const INIT_B: u32 = 0x800D_D05C;
    pub const NFS2_SYSTEM_N_LIB_STARTUP: u32 = 0x800A_4950;
    pub const LOAD_FRONTEND_OVERLAY: u32 = 0x800A_506C;
    pub const NFS2_GAME_MODULE_STARTUP: u32 = 0x800A_4D1C;
    pub const RACE_RESTART: u32 = 0x800A_4A0C;
    pub const GAME_LOOP: u32 = 0x800B_7A20;
    /// Car_tObj* table (player first).
    pub const CAR_TABLE: u32 = 0x8011_0D0C;
}

pub struct OriginalGame {
    pub cpu: Cpu,
}

/// Run `f` with a fresh stack frame; returns Err with the call stack on a panic.
pub fn call(cpu: &mut Cpu, f: u32, args: &[u32]) -> Result<u32> {
    for (i, a) in args.iter().enumerate() {
        cpu.r[4 + i] = *a;
    }
    cpu.r[29] = addr::STACK_TOP - 0x100;
    cpu.r[30] = cpu.r[29];
    cpu.r[28] = addr::GP;
    cpu.r[31] = 0;
    cpu.stack.clear();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cpu.call(f)));
    match res {
        Ok(()) => Ok(cpu.r[2]),
        Err(e) => {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            let names = names();
            let stack: Vec<String> =
                cpu.stack.iter().rev().take(14).map(|a| format!("{a:#x} {}", names.get(a).map(String::as_str).unwrap_or("?"))).collect();
            bail!("{f:#x} failed: {msg}; stack {}", stack.join(" <- "))
        }
    }
}

/// Optional developer names (re/retail_syms.tsv, produced by tools/symmatch.py) for diagnostics.
pub fn names() -> std::collections::HashMap<u32, String> {
    std::fs::read_to_string("re/retail_syms.tsv")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((u32::from_str_radix(it.next()?, 16).ok()?, it.next()?.to_string()))
        })
        .collect()
}

impl OriginalGame {
    /// Load NFS4.EXE into a fresh 2 MB RAM image and perform the start-up `main` does.
    pub fn boot(data_root: &Path, gamesetup: &[u8]) -> Result<Self> {
        let exe = std::fs::read(data_root.join("NFS4.EXE"))?;
        let mut mem = Mem::default();
        mem.load_exe(&exe);
        for a in (addr::BSS_START..addr::BSS_END).step_by(4) {
            mem.w32(a, 0);
        }
        let mut cpu = new_cpu(mem);
        cpu.host = Some(Box::new(GameHost::new(data_root.to_path_buf())));
        cpu.trace = std::env::var("PS1_TRACE").is_ok();
        cpu.tick_limit = 300_000_000;
        call(&mut cpu, addr::INIT_A, &[])?;
        call(&mut cpu, addr::INIT_B, &[])?;
        call(&mut cpu, addr::NFS2_SYSTEM_N_LIB_STARTUP, &[])?;
        call(&mut cpu, addr::LOAD_FRONTEND_OVERLAY, &[])?;
        cpu.m.write_bytes(addr::GAMESETUP, &gamesetup[..addr::GAMESETUP_SIZE.min(gamesetup.len())]);
        call(&mut cpu, addr::NFS2_GAME_MODULE_STARTUP, &[addr::GAMESETUP])?;
        call(&mut cpu, addr::RACE_RESTART, &[])?;
        Ok(Self { cpu })
    }

    fn host(&mut self) -> &mut GameHost {
        // the host is always our GameHost
        let h = self.cpu.host.as_mut().expect("host");
        unsafe { &mut *(h.as_mut() as *mut dyn super::Host as *mut GameHost) }
    }

    /// Run the original main loop for `frames` 60 Hz frames with the given buttons held.
    pub fn run_frames(&mut self, frames: u32, buttons: u16) -> Result<u32> {
        {
            let h = self.host();
            h.frame.active = true;
            h.frame.frames_left = frames;
            h.frame.frames_run = 0;
            h.frame.buttons = buttons;
        }
        self.cpu.m.w32(super::host::MAIN_LOOP_EXIT_FLAG, 0);
        self.cpu.ticks = 0;
        self.cpu.tick_limit = 50_000_000;
        call(&mut self.cpu, addr::GAME_LOOP, &[])?;
        let h = self.host();
        h.frame.active = false;
        Ok(h.frame.frames_run)
    }

    /// The player's car object (Car_tObj*), from the global car table.
    pub fn player_car(&mut self) -> u32 {
        self.cpu.m.r32(addr::CAR_TABLE)
    }
}

/// PS1 pad buttons (active-high masks for [`OriginalGame::run_frames`]).
pub mod pad {
    pub const SELECT: u16 = 1 << 0;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;
}
