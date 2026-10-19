//! The game's files, read from the extracted disc (the EA FILE_* layer the host replaces).

use super::*;

/// PS1_FILELOG: print the files the game opens.
pub fn file_log(s: String) {
    if std::env::var_os("PS1_FILELOG").is_some() {
        eprintln!("{s}");
    }
}

/// The car model files (`zzz....viv`) the game has opened, lower case: which police car
/// livery it chose for a track, say (`zzzgbmws.viv` for the cop BMW in Germany).
static CAR_FILES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub(super) fn note_open(name: &str) {
    let n = name.rsplit(['\\', '/', ':']).next().unwrap_or(name).to_ascii_lowercase();
    if n.starts_with("zzz") && n.ends_with(".viv") {
        // (a file opened again moves to the end: the latest race's files come last)
        if let Ok(mut v) = CAR_FILES.lock() {
            v.retain(|o| *o != n);
            v.push(n);
        }
    }
}

/// The model file the game opened for car `code` whose first letter it replaced (police cars:
/// "CBMW" -> "zzzgbmws.viv"), as the name to read: the detailed "h" model for a player's car,
/// the "s" one for the others (an AI police car of the player's model opens both; the "s" file
/// lacks the body's detailed parts the original draws for a player).
pub fn opened_variant(code: &str, player: bool) -> Option<String> {
    let tail = code.get(1..)?.to_ascii_lowercase();
    let v = CAR_FILES.lock().ok()?;
    let matches = |n: &&String| n.len() == 3 + 1 + tail.len() + 5 && n[4..4 + tail.len()] == tail;
    let want = if player { 'h' } else { 's' };
    let found = v.iter().rev().filter(matches).find(|n| n.as_bytes()[4 + tail.len()] == want as u8);
    found.or_else(|| v.iter().rev().find(matches)).map(|n| n.to_ascii_uppercase())
}

/// Paints the car select offers that the original leaves out: (car, paint 0..15 as
/// [`crate::ps1::race::CarState::colour`] has it, 0..7 the "l" body sheet, 8..15 the "d").
/// Each is a finished palette row (on every body part, dents too), not near a paint the car
/// already has; the car select gives them in this order, after the original's.
const EXTRA_PAINTS: &[(&str, u8)] = &[
    ("BMWZ", 2),  // olive green
    ("BMWZ", 10), // black
    ("CMRO", 3),  // mustard gold
    ("TRAM", 2),  // gold
    ("TRAM", 11), // charcoal
    ("F550", 3),  // silver
    ("F550", 2),  // champagne
    ("F550", 12), // dark green
    ("F375", 1),  // navy
    ("F375", 5),  // silver
    ("F375", 8),  // dark green
    ("DIAB", 1),  // lime green
    ("MCF1", 3),  // silver
    ("MCF1", 4),  // purple
    ("MCF1", 5),  // blue
    ("MCF1", 6),  // green
    ("MCF1", 7),  // gold
];

/// Add [`EXTRA_PAINTS`] to a car list (`ZFECARS*.CAR`, tCarManager::LoadDescription): a u32
/// count, then 204-byte entries with the model ("zDIAB1") at +8, the number of "l" and "d"
/// paints at +172 and +173 (the game only uses their sum) and the paints from +175 (16 at
/// most). Only a model's first entry, the stock car; a later one is its police car, whose one
/// paint is its livery. The new paint goes last, so saved choices keep their places.
fn add_paints(d: &mut [u8]) {
    const SIZE: usize = 204;
    let n = if d.len() >= 4 { crate::formats::u32le(d, 0) as usize } else { 0 };
    for &(code, paint) in EXTRA_PAINTS {
        let model = format!("z{code}1");
        let Some(e) = (0..n).map(|i| 4 + i * SIZE).take_while(|e| e + SIZE <= d.len()).find(|&e| d[e + 8..e + 16].split(|&b| b == 0).next() == Some(model.as_bytes())) else { continue };
        let count = (d[e + 172] + d[e + 173]) as usize;
        if count >= 16 || d[e + 175..e + 175 + count].contains(&paint) {
            continue;
        }
        d[e + 175 + count] = paint;
        d[e + if paint < 8 { 172 } else { 173 }] += 1;
    }
}

impl GameHost {
    pub(super) fn new_op(&mut self, result: u32, user: u32) -> u32 {
        let op = self.next_op;
        self.next_op += 1;
        self.file_ops.insert(op, (result, user));
        op
    }

    pub(super) fn resolve(&self, name: &str) -> Option<PathBuf> {
        let n = name.trim_start_matches("cdrom:").trim_start_matches('\\').split(';').next().unwrap_or("");
        let p = self.root.join(n.to_ascii_uppercase());
        p.is_file().then_some(p)
    }

    pub(super) fn lookup(&self, name: &str) -> Option<Vec<u8>> {
        if std::env::var_os("PS1_FILES").is_some() {
            eprintln!("file: {name}");
        }
        let base = name.rsplit(['\\', '/', ':']).next().unwrap_or(name);
        let mut data = match self.resolve(name) {
            Some(p) => std::fs::read(p).ok()?,
            None => self.mounts.iter().rev().find_map(|(_, entries)| entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(base)).map(|(_, d)| d.clone()))?,
        };
        let upper = base.to_ascii_uppercase();
        if upper.starts_with("ZFECARS") && upper.ends_with(".CAR") {
            add_paints(&mut data);
        }
        if upper.starts_with("ZTEXT.") {
            super::options::add_words(&mut data);
        }
        Some(data)
    }

    /// The EA file layer (FILE_*sync): files from the extracted disc. `None`: not one of these (or the original's own code runs).
    pub(super) fn file_call(&mut self, c: &mut Cpu, addr: u32) -> Option<u32> {
        let (a0, a1, a2, a3) = (c.r[4], c.r[5], c.r[6], c.r[7]);
        Some(match addr {
            // ---- EA file layer (FILE_*sync)
            0x800E_B204 => {
                let name = c.m.cstr(a0);
                match self.lookup(&name) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        crate::ps1::host::file_log(format!("open {name} ({} bytes)", data.len()));
                        note_open(&name);
                        if std::env::var_os("PS1_FILES").is_some() {
                            eprintln!("open {name}");
                        }
                        self.files.insert(h, data);
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        crate::ps1::host::file_log(format!("open {name}: not found"));
                        c.m.w32(a3, 0);
                        0
                    }
                }
            }
            // the asynchronous operations (asyncload*, streams): done as soon as asked for
            0x800E_CCC8 => {
                // FILE_open(name, mode, prio, user) -> op; its result is the handle (0: failed)
                let name = c.m.cstr(a0);
                let h = match self.lookup(&name) {
                    Some(data) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        crate::ps1::host::file_log(format!("open {name} ({} bytes, async)", data.len()));
                        note_open(&name);
                        self.files.insert(h, data);
                        h
                    }
                    None => {
                        crate::ps1::host::file_log(format!("open {name}: not found"));
                        0
                    }
                };
                self.new_op(h, a3)
            }
            0x800E_CE48 => {
                // FILE_read(handle, offset, address, size, prio, user)
                let data = self.files.get(&a0).cloned().unwrap_or_default();
                let off = (a1 as usize).min(data.len());
                let n = (a3 as usize).min(data.len() - off);
                c.m.write_bytes(a2, &data[off..off + n]);
                let user = c.m.r32(c.r[29] + 20);
                self.new_op(n as u32, user)
            }
            0x800E_CF2C => {
                // FILE_size(handle, prio, user)
                let n = self.files.get(&a0).map_or(0, |d| d.len() as u32);
                self.new_op(n, a2)
            }
            0x800E_CD88 => {
                // FILE_close(handle, prio, user)
                self.files.remove(&a0);
                self.new_op(0, a2)
            }
            0x800E_C720 | 0x800E_CB18 => u32::from(self.file_ops.contains_key(&a0)), // FILE_opstatus / waitop: done
            0x800E_C778 => 0,                                                         // FILE_operror
            0x800E_C828 => 0,                                                         // FILE_priorityop
            0x800E_CC0C => self.file_ops.remove(&a0).map_or(0, |(r, _)| r),         // FILE_completeop
            0x800E_C964 => {
                self.file_ops.remove(&a0);
                0
            }
            0x800E_C7A8 => {
                // FILE_callbackop(op, callback): done, so the callback runs now (op, status, user)
                if let Some(&(_, user)) = self.file_ops.get(&a0) {
                    c.deferred.push((a1, vec![a0, 1, user]));
                }
                0
            }
            0x800E_B27C => {
                let data = self.files.get(&a0).cloned().unwrap_or_default();
                let off = (a1 as usize).min(data.len());
                let n = (a3 as usize).min(data.len() - off);
                c.m.write_bytes(a2, &data[off..off + n]);
                n as u32
            }
            0x800E_B2AC => {
                self.files.remove(&a0);
                0
            }
            0x800E_B300 => self.files.get(&a0).map_or(0, |d| d.len() as u32),
            0x800E_B354 => {
                // FILE_addbigsync(name, mode, prio, &handle): mount a BIGF archive
                let name = c.m.cstr(a0);
                let viv = self.lookup(&name).and_then(|d| crate::formats::viv::Viv::parse(&d).ok());
                match viv {
                    Some(v) => {
                        let h = self.next_handle;
                        self.next_handle += 1;
                        crate::ps1::host::file_log(format!("mount {name} ({} entries)", v.entries.len()));
                        self.mounts.push((h, v.entries));
                        c.m.w32(a3, h);
                        1
                    }
                    None => {
                        crate::ps1::host::file_log(format!("mount {name}: failed"));
                        c.m.w32(a3, 0);
                        0
                    }
                }
            }
            0x800E_B3CC => {
                // FILE_delbigsync(handle, prio)
                self.mounts.retain(|(h, _)| *h != a0);
                0
            }
            _ => return None,
        })
    }
}
