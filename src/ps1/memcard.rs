//! The memory card in slot 1, for libmcrd (MemCardExist, MemCardReadFile, ...): a raw 128 KiB
//! card image, the format DuckStation and other emulators keep their `.mcd` cards in (so a
//! card can be moved between them). Slot 2 is empty.
//!
//! The image is 16 blocks of 8 KiB. Block 0 is the directory: frame 0 the "MC" header, frames
//! 1..15 one 128-byte entry per data block (allocation state, file size, next block of the
//! file, name, XOR checksum in byte 127); blocks 1..15 hold the files.

use std::path::{Path, PathBuf};

use ps1rt::Cpu;

/// libmcrd's results (McErr*) and commands (McFunc*). The game (iMCRD_DoFileWrite) asks to
/// overwrite on McErrAlreadyExist and reports a full card on McErrBlockFull.
const ERR_NONE: u32 = 0;
const ERR_CARD_NOT_EXIST: u32 = 1;
const ERR_FILE_NOT_EXIST: u32 = 5;
const ERR_ALREADY_EXIST: u32 = 6;
const ERR_BLOCK_FULL: u32 = 7;
const FUNC_EXIST: u32 = 1;
const FUNC_ACCEPT: u32 = 2;
const FUNC_READ_FILE: u32 = 3;
const FUNC_WRITE_FILE: u32 = 4;

const BLOCK: usize = 8192;
const FRAME: usize = 128;
const CARD: usize = 16 * BLOCK;
/// Directory entry states: in use (first, middle, last block of a file) and free.
const FIRST: u32 = 0x51;
const MIDDLE: u32 = 0x52;
const LAST: u32 = 0x53;
const FREE: u32 = 0xA0;
const NO_NEXT: u16 = 0xFFFF;

pub struct MemCard {
    path: PathBuf,
    image: Vec<u8>,
    /// The asynchronous command (Exist, Accept, ReadFile, WriteFile) waiting for MemCardSync,
    /// and its result: it completes at once.
    pending: Option<(u32, u32)>,
}

impl MemCard {
    /// The card image `memcard.mcd` next to the game data directory (NFSHS_MEMCARD=FILE: another
    /// one, such as an emulator's card); a new, formatted card when there is none yet.
    pub fn new(data_root: &Path) -> Self {
        let path = std::env::var_os("NFSHS_MEMCARD").map(PathBuf::from).unwrap_or_else(|| {
            let dir = std::fs::canonicalize(data_root).ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
            dir.join("memcard.mcd")
        });
        Self::at(path)
    }

    /// The card image at `path`.
    pub fn at(path: PathBuf) -> Self {
        let image = match std::fs::read(&path) {
            Ok(d) if d.len() == CARD && &d[..2] == b"MC" => d,
            Ok(d) => {
                log::warn!("{}: not a 128 KiB memory card image ({} bytes); using an empty card", path.display(), d.len());
                formatted()
            }
            Err(_) => formatted(),
        };
        log::info!("memory card {}", path.display());
        Self { path, image, pending: None }
    }

    fn save(&self) -> bool {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&self.path, &self.image) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("memory card {}: {e}", self.path.display());
                false
            }
        }
    }

    /// Directory entry of data block `b` (1..=15).
    fn entry(&self, b: usize) -> &[u8] {
        &self.image[b * FRAME..(b + 1) * FRAME]
    }

    fn state(&self, b: usize) -> u32 {
        u32::from_le_bytes(self.entry(b)[0..4].try_into().unwrap())
    }

    fn next(&self, b: usize) -> u16 {
        u16::from_le_bytes(self.entry(b)[8..10].try_into().unwrap())
    }

    fn name(&self, b: usize) -> String {
        self.entry(b)[10..31].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
    }

    fn set_entry(&mut self, b: usize, state: u32, size: u32, next: u16, name: &str) {
        let e = &mut self.image[b * FRAME..(b + 1) * FRAME];
        e.fill(0);
        e[0..4].copy_from_slice(&state.to_le_bytes());
        e[4..8].copy_from_slice(&size.to_le_bytes());
        e[8..10].copy_from_slice(&next.to_le_bytes());
        let n = name.as_bytes();
        e[10..10 + n.len().min(20)].copy_from_slice(&n[..n.len().min(20)]);
        e[127] = e[..127].iter().fold(0, |a, &b| a ^ b);
    }

    /// The files on the card: name, size, first block.
    fn files(&self) -> Vec<(String, usize, usize)> {
        (1..16).filter(|&b| self.state(b) == FIRST).map(|b| (self.name(b), u32::from_le_bytes(self.entry(b)[4..8].try_into().unwrap()) as usize, b)).collect()
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.files().into_iter().find(|f| f.0 == name).map(|f| f.2)
    }

    /// A file's blocks in order.
    fn chain(&self, first: usize) -> Vec<usize> {
        let mut out = vec![first];
        let mut b = first;
        while out.len() < 15 {
            let n = self.next(b);
            if n == NO_NEXT || n as usize >= 15 {
                break;
            }
            b = n as usize + 1;
            out.push(b);
        }
        out
    }

    /// Copy between a file's bytes [off, off + len) and `buf` (reading when `read`).
    fn access(&mut self, first: usize, off: usize, buf: &mut [u8], read: bool) -> bool {
        let blocks = self.chain(first);
        if off + buf.len() > blocks.len() * BLOCK {
            return false;
        }
        for (i, byte) in buf.iter_mut().enumerate() {
            let p = off + i;
            let at = blocks[p / BLOCK] * BLOCK + p % BLOCK;
            if read {
                *byte = self.image[at];
            } else {
                self.image[at] = *byte;
            }
        }
        true
    }

    /// Handle a libmcrd call; None if `addr` is not one.
    pub fn call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let r = self.handle(c, addr);
        // NFSHS_MCLOG=1: the game's card calls and their results
        if let (Some(r), Some(_)) = (r, std::env::var_os("NFSHS_MCLOG")) {
            let name = if matches!(addr, 0x800F_BDF4 | 0x800F_C014 | 0x800F_C234 | 0x800F_C5C4 | 0x800F_C7CC) { c.m.cstr(c.r[5]) } else { String::new() };
            eprintln!("memcard {addr:#x} {name} -> {r:#x} (pending {:?})", self.pending);
        }
        r
    }

    fn handle(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        let arg = |c: &mut Cpu, i: u32| c.m.r32(c.r[29] + 16 + 4 * i);
        let slot1 = a0 & 0xF == 0;
        Some(match addr {
            // MemCardInit / End / Start / Stop / Callback
            0x800F_B458 | 0x800F_B484 | 0x800F_B4A4 | 0x800F_B4F4 | 0x800F_C494 => 0,
            // MemCardExist(chan) / MemCardAccept(chan): a formatted card in slot 1
            0x800F_B534 | 0x800F_B770 => {
                let cmd = if addr == 0x800F_B534 { FUNC_EXIST } else { FUNC_ACCEPT };
                self.pending = Some((cmd, if slot1 { ERR_NONE } else { ERR_CARD_NOT_EXIST }));
                1
            }
            0x800F_BDF4 => {
                // MemCardReadFile(chan, name, address, offset, bytes)
                let name = c.m.cstr(a1);
                let mut buf = vec![0; arg(c, 0) as usize];
                let result = match self.find(&name).filter(|_| slot1) {
                    Some(first) if self.access(first, a3 as usize, &mut buf, true) => {
                        c.m.write_bytes(a2, &buf);
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
                let mut buf = vec![0; arg(c, 0) as usize];
                c.m.read_bytes(a2, &mut buf);
                let result = match self.find(&name).filter(|_| slot1) {
                    Some(first) if self.access(first, a3 as usize, &mut buf, false) && self.save() => ERR_NONE,
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
                let found: Vec<(String, usize, usize)> = if slot1 { self.files().into_iter().filter(|(n, _, _)| wildcard(&pattern, n)).collect() } else { Vec::new() };
                let list: Vec<&(String, usize, usize)> = found.iter().skip(first).take(max).collect();
                for (i, (name, size, head)) in list.iter().enumerate() {
                    let e = a2 + 40 * i as u32;
                    let mut n = [0u8; 20];
                    n[..name.len().min(19)].copy_from_slice(&name.as_bytes()[..name.len().min(19)]);
                    c.m.write_bytes(e, &n);
                    c.m.w32(e + 20, FIRST);
                    c.m.w32(e + 24, *size as u32);
                    c.m.w32(e + 28, 0);
                    c.m.w32(e + 32, *head as u32 - 1);
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
                let name = c.m.cstr(a1);
                let blocks = (a2 as usize).max(1);
                if !slot1 {
                    ERR_CARD_NOT_EXIST
                } else if self.find(&name).is_some() {
                    ERR_ALREADY_EXIST
                } else {
                    let free: Vec<usize> = (1..16).filter(|&b| self.state(b) & 0xF0 == FREE).take(blocks).collect();
                    if free.len() < blocks {
                        ERR_BLOCK_FULL
                    } else {
                        for (i, &b) in free.iter().enumerate() {
                            let state = if i == 0 { FIRST } else if i + 1 == blocks { LAST } else { MIDDLE };
                            let next = free.get(i + 1).map_or(NO_NEXT, |&n| (n - 1) as u16);
                            let (size, n) = if i == 0 { ((blocks * BLOCK) as u32, name.as_str()) } else { (0, "") };
                            self.set_entry(b, state, size, next, n);
                            self.image[b * BLOCK..(b + 1) * BLOCK].fill(0);
                        }
                        if self.save() { ERR_NONE } else { ERR_CARD_NOT_EXIST }
                    }
                }
            }
            0x800F_C7CC => {
                // MemCardDeleteFile(chan, name): its blocks marked deleted (free)
                match self.find(&c.m.cstr(a1)).filter(|_| slot1) {
                    Some(first) => {
                        for b in self.chain(first) {
                            let e = &mut self.image[b * FRAME..(b + 1) * FRAME];
                            e[0] = (e[0] & 0x0F) | 0xA0;
                            e[127] = e[..127].iter().fold(0, |a, &b| a ^ b);
                        }
                        if self.save() { ERR_NONE } else { ERR_FILE_NOT_EXIST }
                    }
                    None => ERR_FILE_NOT_EXIST,
                }
            }
            0x800F_C988 => {
                // MemCardFormat(chan)
                self.image = formatted();
                if self.save() { ERR_NONE } else { ERR_CARD_NOT_EXIST }
            }
            _ => return None,
        })
    }
}

/// An empty, formatted card: the "MC" header (also in the write-test frame), 15 free
/// directory entries and an empty broken-sector list.
fn formatted() -> Vec<u8> {
    let mut card = vec![0u8; CARD];
    let checksum = |f: &mut [u8]| f[127] = f[..127].iter().fold(0, |a, &b| a ^ b);
    card[0] = b'M';
    card[1] = b'C';
    checksum(&mut card[0..FRAME]);
    for b in 1..16 {
        let f = &mut card[b * FRAME..(b + 1) * FRAME];
        f[0..4].copy_from_slice(&FREE.to_le_bytes());
        f[8..10].copy_from_slice(&NO_NEXT.to_le_bytes());
        checksum(f);
    }
    for i in 16..36 {
        let f = &mut card[i * FRAME..(i + 1) * FRAME];
        f[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        f[8..10].copy_from_slice(&NO_NEXT.to_le_bytes());
        checksum(f);
    }
    let header: Vec<u8> = card[..FRAME].to_vec();
    card[63 * FRAME..64 * FRAME].copy_from_slice(&header);
    card
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Call a libmcrd function with arguments (the 5th and 6th on the stack) on a card.
    fn call(card: &mut MemCard, c: &mut Cpu, addr: u32, args: &[u32]) -> u32 {
        c.r[29] = 0x801F_0000;
        for (i, &a) in args.iter().enumerate() {
            if i < 4 {
                c.r[4 + i] = a;
            } else {
                c.m.w32(c.r[29] + 16 + 4 * (i as u32 - 4), a);
            }
        }
        card.call(c, addr).expect("a libmcrd call")
    }

    fn sync(card: &mut MemCard, c: &mut Cpu) -> u32 {
        call(card, c, 0x800F_C4A8, &[0, 0x8010_0000, 0x8010_0004]);
        c.m.r32(0x8010_0004)
    }

    #[test]
    fn create_write_read_delete() {
        let dir = std::env::temp_dir().join(format!("nfshs-memcard-test-{}", std::process::id()));
        let path = dir.join("card.mcd");
        let mut card = MemCard::at(path.clone());
        let mut c = crate::ps1::new_cpu(crate::ps1::Mem::default());
        let name = 0x8000_1000;
        c.m.write_bytes(name, b"BASLUS-00826NFS4\0");
        // a new file of one block, then the same again: already there
        assert_eq!(call(&mut card, &mut c, 0x800F_C5C4, &[0, name, 1]), ERR_NONE);
        assert_eq!(call(&mut card, &mut c, 0x800F_C5C4, &[0, name, 1]), ERR_ALREADY_EXIST);
        // written at an offset, read back
        let data: Vec<u8> = (0..300).map(|i| i as u8).collect();
        c.m.write_bytes(0x8000_2000, &data);
        call(&mut card, &mut c, 0x800F_C014, &[0, name, 0x8000_2000, 100, 300]);
        assert_eq!(sync(&mut card, &mut c), ERR_NONE);
        call(&mut card, &mut c, 0x800F_BDF4, &[0, name, 0x8000_3000, 100, 300]);
        assert_eq!(sync(&mut card, &mut c), ERR_NONE);
        let mut back = vec![0; 300];
        c.m.read_bytes(0x8000_3000, &mut back);
        assert_eq!(back, data);
        // kept on disk: a new card object reads it
        let mut again = MemCard::at(path);
        call(&mut again, &mut c, 0x800F_BDF4, &[0, name, 0x8000_4000, 100, 300]);
        assert_eq!(sync(&mut again, &mut c), ERR_NONE);
        // fourteen more blocks fill the card, then it is full
        c.m.write_bytes(0x8000_1100, b"BIG\0");
        assert_eq!(call(&mut again, &mut c, 0x800F_C5C4, &[0, 0x8000_1100, 14]), ERR_NONE);
        c.m.write_bytes(0x8000_1200, b"ONE_MORE\0");
        assert_eq!(call(&mut again, &mut c, 0x800F_C5C4, &[0, 0x8000_1200, 1]), ERR_BLOCK_FULL);
        // deleting frees its blocks
        assert_eq!(call(&mut again, &mut c, 0x800F_C7CC, &[0, 0x8000_1100]), ERR_NONE);
        assert_eq!(call(&mut again, &mut c, 0x800F_C5C4, &[0, 0x8000_1200, 1]), ERR_NONE);
        // the directory lists the two files
        c.m.write_bytes(0x8000_1300, b"*\0");
        call(&mut again, &mut c, 0x800F_C234, &[0, 0x8000_1300, 0x8000_5000, 0x8000_6000, 0, 15]);
        assert_eq!(c.m.r32(0x8000_6000), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
