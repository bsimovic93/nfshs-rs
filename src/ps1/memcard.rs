//! The memory card in slot 1, for libmcrd (MemCardExist, MemCardReadFile, ...): its files are
//! kept as files in a directory on the host. Slot 2 is empty.

use std::path::PathBuf;

use ps1rt::Cpu;

/// libmcrd's results (McErr*) and commands (McFunc*).
const ERR_NONE: u32 = 0;
const ERR_CARD_NOT_EXIST: u32 = 1;
const ERR_FILE_NOT_EXIST: u32 = 5;
const ERR_ALREADY_EXIST: u32 = 7;
const FUNC_EXIST: u32 = 1;
const FUNC_ACCEPT: u32 = 2;
const FUNC_READ_FILE: u32 = 3;
const FUNC_WRITE_FILE: u32 = 4;

const BLOCK: usize = 8192;

pub struct MemCard {
    dir: PathBuf,
    /// The asynchronous command (Exist, Accept, ReadFile, WriteFile) waiting for MemCardSync,
    /// and its result: it completes at once.
    pending: Option<(u32, u32)>,
}

impl MemCard {
    pub fn new() -> Self {
        let dir = std::env::var_os("NFSHS_MEMCARD").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("memcard"));
        Self { dir, pending: None }
    }

    fn path(&self, name: &str) -> PathBuf {
        // card file names are ASCII; keep them usable as host file names
        let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
        self.dir.join(safe)
    }

    fn files(&self) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Some((e.file_name().into_string().ok()?, e.metadata().ok()?.len() as usize)))
            .collect();
        v.sort();
        v
    }

    /// Handle a libmcrd call; None if `addr` is not one.
    pub fn call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        let arg = |c: &mut Cpu, i: u32| c.m.r32(c.r[29] + 16 + 4 * i);
        Some(match addr {
            // MemCardInit / End / Start / Stop / Callback
            0x800F_B458 | 0x800F_B484 | 0x800F_B4A4 | 0x800F_B4F4 | 0x800F_C494 => 0,
            // MemCardExist(chan) / MemCardAccept(chan): a formatted card in slot 1
            0x800F_B534 | 0x800F_B770 => {
                let cmd = if addr == 0x800F_B534 { FUNC_EXIST } else { FUNC_ACCEPT };
                self.pending = Some((cmd, if a0 & 0xF == 0 { ERR_NONE } else { ERR_CARD_NOT_EXIST }));
                1
            }
            0x800F_BDF4 => {
                // MemCardReadFile(chan, name, address, offset, bytes)
                let name = c.m.cstr(a1);
                let bytes = arg(c, 0) as usize;
                let result = match std::fs::read(self.path(&name)) {
                    Ok(d) if a0 & 0xF == 0 => {
                        let off = (a3 as usize).min(d.len());
                        let n = bytes.min(d.len() - off);
                        c.m.write_bytes(a2, &d[off..off + n]);
                        ERR_NONE
                    }
                    _ => ERR_FILE_NOT_EXIST,
                };
                self.pending = Some((FUNC_READ_FILE, result));
                1
            }
            0x800F_C014 => {
                // MemCardWriteFile(chan, name, address, offset, bytes)
                let name = c.m.cstr(a1);
                let bytes = arg(c, 0) as usize;
                let path = self.path(&name);
                let result = match std::fs::read(&path) {
                    Ok(mut d) if a0 & 0xF == 0 => {
                        let off = a3 as usize;
                        if d.len() < off + bytes {
                            d.resize(off + bytes, 0);
                        }
                        let mut buf = vec![0; bytes];
                        c.m.read_bytes(a2, &mut buf);
                        d[off..off + bytes].copy_from_slice(&buf);
                        if std::fs::write(&path, &d).is_ok() { ERR_NONE } else { ERR_FILE_NOT_EXIST }
                    }
                    _ => ERR_FILE_NOT_EXIST,
                };
                self.pending = Some((FUNC_WRITE_FILE, result));
                1
            }
            0x800F_C234 => {
                // MemCardGetDirentry(chan, pattern, entries, &count, first, max): DIRENTRY is
                // name[20], attr, size, next, head, system[4]
                let pattern = c.m.cstr(a1);
                let (first, max) = (arg(c, 0) as usize, arg(c, 1) as usize);
                let found: Vec<(String, usize)> = if a0 & 0xF == 0 { self.files().into_iter().filter(|(n, _)| wildcard(&pattern, n)).collect() } else { Vec::new() };
                let list: Vec<&(String, usize)> = found.iter().skip(first).take(max).collect();
                for (i, (name, size)) in list.iter().enumerate() {
                    let e = a2 + 40 * i as u32;
                    let mut n = [0u8; 20];
                    n[..name.len().min(19)].copy_from_slice(&name.as_bytes()[..name.len().min(19)]);
                    c.m.write_bytes(e, &n);
                    c.m.w32(e + 20, 0x51);
                    c.m.w32(e + 24, *size as u32);
                    c.m.w32(e + 28, 0);
                    c.m.w32(e + 32, 0);
                    c.m.w32(e + 36, 0);
                }
                c.m.w32(a3, list.len() as u32);
                0
            }
            0x800F_C4A8 => {
                // MemCardSync(mode, &command, &result): the command is done
                match self.pending.take() {
                    Some((cmd, result)) => {
                        if a1 != 0 {
                            c.m.w32(a1, cmd);
                        }
                        if a2 != 0 {
                            c.m.w32(a2, result);
                        }
                        1
                    }
                    None => u32::MAX,
                }
            }
            0x800F_C5C4 => {
                // MemCardCreateFile(chan, name, blocks)
                let path = self.path(&c.m.cstr(a1));
                if a0 & 0xF != 0 {
                    ERR_CARD_NOT_EXIST
                } else if path.exists() {
                    ERR_ALREADY_EXIST
                } else {
                    let _ = std::fs::create_dir_all(&self.dir);
                    if std::fs::write(&path, vec![0; a2 as usize * BLOCK]).is_ok() { ERR_NONE } else { ERR_CARD_NOT_EXIST }
                }
            }
            0x800F_C7CC => {
                // MemCardDeleteFile(chan, name)
                let path = self.path(&c.m.cstr(a1));
                if a0 & 0xF == 0 && std::fs::remove_file(path).is_ok() { ERR_NONE } else { ERR_FILE_NOT_EXIST }
            }
            0x800F_C988 => ERR_NONE, // MemCardFormat
            _ => return None,
        })
    }
}

/// `*` and `?` matching, as the card's directory search does.
fn wildcard(pattern: &str, name: &str) -> bool {
    fn m(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some(b'?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    m(pattern.as_bytes(), name.as_bytes())
}
