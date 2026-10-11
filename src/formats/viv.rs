//! `BIGF` archives (`.VIV`). Big-endian directory of (offset, size, NUL-terminated name).

use super::u32be;
use anyhow::{Result, bail};

pub struct Viv {
    pub entries: Vec<(String, Vec<u8>)>,
}

impl Viv {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 16 || &d[..4] != b"BIGF" {
            bail!("not a BIGF archive");
        }
        let count = u32be(d, 8) as usize;
        let mut o = 16;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let off = u32be(d, o) as usize;
            let size = u32be(d, o + 4) as usize;
            o += 8;
            let end = o + d[o..].iter().position(|&c| c == 0).unwrap_or(0);
            let name = String::from_utf8_lossy(&d[o..end]).into_owned();
            o = end + 1;
            if off + size > d.len() {
                bail!("entry {name} out of range");
            }
            entries.push((name, d[off..off + size].to_vec()));
        }
        Ok(Self { entries })
    }

    /// Case-insensitive lookup by file name.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, d)| d.as_slice())
    }

    /// First entry whose name ends with the given suffix (case-insensitive).
    pub fn find_suffix(&self, suffix: &str) -> Option<(&str, &[u8])> {
        let suffix = suffix.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|(n, _)| n.to_ascii_lowercase().ends_with(&suffix))
            .map(|(n, d)| (n.as_str(), d.as_slice()))
    }
}
