//! Host replacements (HLE) for NFS4.EXE functions that drive PS1 hardware.
//! Addresses are for the NTSC-U executable (SLUS-00826).

use ps1rt::{Cpu, Host};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct GameHost {
    pub root: PathBuf,
    files: HashMap<u32, Vec<u8>>,
    next_handle: u32,
    pub vsync: u32,
    pub log: Vec<String>,
    pub unknown: HashMap<u32, u32>,
}

impl GameHost {
    pub fn new(root: PathBuf) -> Self {
        Self { root, files: HashMap::new(), next_handle: 0x100, vsync: 0, log: Vec::new(), unknown: HashMap::new() }
    }

    fn resolve(&self, name: &str) -> Option<PathBuf> {
        let n = name.trim_start_matches("cdrom:").trim_start_matches('\\').split(';').next().unwrap_or("");
        let p = self.root.join(n.to_ascii_uppercase());
        p.is_file().then_some(p)
    }
}

/// Functions replaced on the host side (excluded from recompilation).
pub const HLE_FUNCS: &[u32] = &[
    0x800E_B204, // file open(name, mode, prio, &handle) -> ok
    0x800E_B27C, // file read(handle, offset, buf, len) -> bytes
    0x800E_B2AC, // file close(handle, prio)
    0x800E_B300, // file size(handle, prio) -> bytes
    0x800F_2C78, // VSync(mode)
    0x800F_0318, // DrawSync(mode)
];

impl Host for GameHost {
    fn call(&mut self, c: &mut Cpu, addr: u32) -> bool {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        match addr {
            0x800E_B204 => {
                let name = c.m.cstr(a0);
                match self.resolve(&name).and_then(|p| std::fs::read(p).ok()) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        self.log.push(format!("open {name} ({} bytes) -> {h:#x}", data.len()));
                        self.files.insert(h, data);
                        c.m.w32(a3, h);
                        c.r[2] = 1;
                    }
                    None => {
                        self.log.push(format!("open {name} -> not found"));
                        c.m.w32(a3, 0);
                        c.r[2] = 0;
                    }
                }
            }
            0x800E_B27C => {
                let data = self.files.get(&a0).cloned().unwrap_or_default();
                let off = (a1 as usize).min(data.len());
                let n = (a3 as usize).min(data.len() - off);
                c.m.write_bytes(a2, &data[off..off + n]);
                self.log.push(format!("read {a0:#x} @{off:#x} -> {a2:#x} ({n} bytes)"));
                c.r[2] = n as u32;
            }
            0x800E_B2AC => {
                self.files.remove(&a0);
                c.r[2] = 0;
            }
            0x800E_B300 => {
                c.r[2] = self.files.get(&a0).map_or(0, |d| d.len() as u32);
            }
            0x800F_2C78 => {
                self.vsync += 1;
                c.r[2] = self.vsync;
            }
            0x800F_0318 => {
                c.r[2] = 0;
            }
            _ => return false,
        }
        true
    }
}
