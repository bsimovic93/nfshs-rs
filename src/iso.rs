//! Reader for raw PlayStation CD images (MODE2/2352 `.bin`) with an ISO9660 filesystem.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const RAW_SECTOR: u64 = 2352;
pub const USER_DATA: usize = 2048;
/// MODE2 sectors: 12 sync + 4 header + 8 subheader before user data.
const MODE2_DATA_OFFSET: u64 = 24;

#[derive(Debug, Clone)]
pub struct IsoEntry {
    pub path: String,
    pub lba: u32,
    pub size: u32,
    pub is_dir: bool,
}

pub struct CdImage {
    file: File,
}

impl CdImage {
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self { file: File::open(path)? })
    }

    /// Read the 2048-byte user-data area of a MODE2 Form1 sector.
    pub fn read_sector(&mut self, lba: u32) -> io::Result<[u8; USER_DATA]> {
        let mut buf = [0u8; USER_DATA];
        self.file
            .seek(SeekFrom::Start(lba as u64 * RAW_SECTOR + MODE2_DATA_OFFSET))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read the whole raw 2352-byte sector (used for XA audio / STR video streams).
    pub fn read_raw_sector(&mut self, lba: u32) -> io::Result<[u8; RAW_SECTOR as usize]> {
        let mut buf = [0u8; RAW_SECTOR as usize];
        self.file.seek(SeekFrom::Start(lba as u64 * RAW_SECTOR))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn read_file(&mut self, entry: &IsoEntry) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(entry.size as usize);
        let sectors = (entry.size as usize).div_ceil(USER_DATA);
        for i in 0..sectors {
            out.extend_from_slice(&self.read_sector(entry.lba + i as u32)?);
        }
        out.truncate(entry.size as usize);
        Ok(out)
    }

    /// Walk the ISO9660 directory tree starting from the primary volume descriptor.
    pub fn list_files(&mut self) -> io::Result<Vec<IsoEntry>> {
        let pvd = self.read_sector(16)?;
        if &pvd[1..6] != b"CD001" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no ISO9660 PVD at sector 16"));
        }
        let root = &pvd[156..156 + 34];
        let lba = u32::from_le_bytes(root[2..6].try_into().unwrap());
        let size = u32::from_le_bytes(root[10..14].try_into().unwrap());
        let mut out = Vec::new();
        self.walk_dir(lba, size, "", &mut out)?;
        Ok(out)
    }

    fn walk_dir(&mut self, lba: u32, size: u32, prefix: &str, out: &mut Vec<IsoEntry>) -> io::Result<()> {
        let sectors = (size as usize).div_ceil(USER_DATA);
        for s in 0..sectors {
            let data = self.read_sector(lba + s as u32)?;
            let mut pos = 0usize;
            while pos < USER_DATA {
                let len = data[pos] as usize;
                if len == 0 {
                    break; // rest of sector is padding
                }
                let rec = &data[pos..pos + len];
                let e_lba = u32::from_le_bytes(rec[2..6].try_into().unwrap());
                let e_size = u32::from_le_bytes(rec[10..14].try_into().unwrap());
                let flags = rec[25];
                let name_len = rec[32] as usize;
                let name = &rec[33..33 + name_len];
                pos += len;
                if name == [0] || name == [1] {
                    continue; // "." and ".."
                }
                let mut name = String::from_utf8_lossy(name).into_owned();
                if let Some(i) = name.find(';') {
                    name.truncate(i);
                }
                let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
                let is_dir = flags & 2 != 0;
                out.push(IsoEntry { path: path.clone(), lba: e_lba, size: e_size, is_dir });
                if is_dir {
                    self.walk_dir(e_lba, e_size, &path, out)?;
                }
            }
        }
        Ok(())
    }
}
