//! EA audio: `BNKl` sample banks and `SCHl` streams with PS1 ADPCM ("VAG") data.
//!
//! Both use EA "PT" patch headers: `"PT" u8 platform u8 0`, then tag/length/value
//! records (`u8 tag, u8 len, big-endian value`); 0xFC/0xFD/0xFE carry no length,
//! 0xFF ends the header. Relevant tags: 0x82 channels, 0x84 sample rate,
//! 0x85 sample count, 0x86/0x87 loop start/end, 0x88 data offset (banks).
//! PS1 defaults: 22050 Hz, mono.

use super::u32le;
use anyhow::{Result, bail};
use std::collections::HashMap;

pub const DEFAULT_RATE: u32 = 22050;

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// Streaming PS1 ADPCM decoder state for one channel.
#[derive(Clone, Copy, Default)]
pub struct Adpcm {
    s1: i32,
    s2: i32,
}

impl Adpcm {
    /// Decode one 16-byte frame into 28 samples. Returns false on an end-of-sample flag.
    pub fn frame(&mut self, f: &[u8], out: &mut Vec<i16>) -> bool {
        let shift = (f[0] & 0x0F) as i32;
        let filter = ((f[0] >> 4) as usize).min(4);
        let (k0, k1) = FILTERS[filter];
        for i in 0..28 {
            let b = f[2 + i / 2];
            let nib = if i & 1 == 0 { b & 0x0F } else { b >> 4 } as i32;
            let raw = ((nib << 28) >> 28) << 12 >> shift.min(12);
            let v = (raw + ((self.s1 * k0 + self.s2 * k1 + 32) >> 6)).clamp(-32768, 32767);
            self.s2 = self.s1;
            self.s1 = v;
            out.push(v as i16);
        }
        f[1] & 0x01 == 0
    }
}

pub fn parse_pt(d: &[u8], mut o: usize) -> Result<(HashMap<u8, u32>, usize)> {
    if d.len() < o + 4 || &d[o..o + 2] != b"PT" {
        bail!("missing PT header");
    }
    o += 4;
    let mut tags = HashMap::new();
    while o < d.len() {
        let tag = d[o];
        o += 1;
        match tag {
            0xFF => break,
            0xFC | 0xFD | 0xFE => continue,
            _ => {
                let len = *d.get(o).unwrap_or(&0) as usize;
                o += 1;
                let mut v: u32 = 0;
                for k in 0..len.min(4) {
                    v = (v << 8) | *d.get(o + k).unwrap_or(&0) as u32;
                }
                tags.insert(tag, v);
                o += len;
            }
        }
    }
    Ok((tags, o))
}

#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub rate: u32,
    pub channels: u32,
    /// Interleaved PCM.
    pub pcm: Vec<i16>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
}

/// A `BNKl` sound bank. Entries can be empty (`None`).
pub struct Bank {
    pub sounds: Vec<Option<Sample>>,
}

impl Bank {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 16 || &d[..4] != b"BNKl" {
            bail!("not a BNKl bank");
        }
        let count = u16::from_le_bytes([d[6], d[7]]) as usize;
        let mut sounds = Vec::with_capacity(count);
        for i in 0..count {
            let p = 0x14 + i * 4;
            if p + 4 > d.len() {
                break;
            }
            let rel = u32le(d, p) as usize;
            if rel == 0 {
                sounds.push(None);
                continue;
            }
            let hdr = p + rel;
            let Ok((tags, _)) = parse_pt(d, hdr) else {
                sounds.push(None);
                continue;
            };
            let channels = *tags.get(&0x82).unwrap_or(&1);
            let rate = *tags.get(&0x84).unwrap_or(&DEFAULT_RATE);
            let samples = *tags.get(&0x85).unwrap_or(&0) as usize;
            let data = *tags.get(&0x88).unwrap_or(&0) as usize;
            let mut pcm = Vec::with_capacity(samples * channels as usize);
            if channels == 1 {
                let mut st = Adpcm::default();
                let mut o = data;
                while pcm.len() < samples && o + 16 <= d.len() {
                    st.frame(&d[o..o + 16], &mut pcm);
                    o += 16;
                }
                pcm.truncate(samples);
            } else {
                // frame-interleaved channels
                let ch = channels as usize;
                let mut st = vec![Adpcm::default(); ch];
                let mut bufs = vec![Vec::new(); ch];
                let mut o = data;
                while bufs[0].len() < samples && o + 16 * ch <= d.len() {
                    for c in 0..ch {
                        st[c].frame(&d[o + c * 16..o + c * 16 + 16], &mut bufs[c]);
                    }
                    o += 16 * ch;
                }
                for k in 0..samples.min(bufs[0].len()) {
                    for b in &bufs {
                        pcm.push(b[k]);
                    }
                }
            }
            let loop_start = tags.get(&0x86).map(|&v| v as usize);
            let loop_end = tags.get(&0x87).map(|&v| v as usize);
            sounds.push(Some(Sample { rate, channels, pcm, loop_start, loop_end }));
        }
        Ok(Self { sounds })
    }
}

/// Decode a complete `SCHl` stream (header + SCCl + SCDl blocks + SCEl) to interleaved PCM.
pub fn decode_stream(d: &[u8]) -> Result<Sample> {
    if d.len() < 8 || &d[..4] != b"SCHl" {
        bail!("not an SCHl stream");
    }
    let hdr_end = (u32le(d, 4) as usize).min(d.len());
    let pt = d[8..hdr_end].windows(2).position(|w| w == b"PT").map(|p| p + 8).unwrap_or(8);
    let (tags, _) = parse_pt(d, pt)?;
    let channels = (*tags.get(&0x82).unwrap_or(&1)).max(1) as usize;
    let rate = *tags.get(&0x84).unwrap_or(&DEFAULT_RATE);
    let mut state = vec![Adpcm::default(); channels];
    let mut pcm = Vec::new();
    let mut o = u32le(d, 4) as usize;
    while o + 8 <= d.len() {
        let id = &d[o..o + 4];
        let size = u32le(d, o + 4) as usize;
        if size < 8 {
            break;
        }
        if id == b"SCDl" {
            // u32 ?, u32 samples-in-block, then each channel's ADPCM frames back to back
            let samples = u32le(d, o + 12) as usize;
            let frames = samples.div_ceil(28);
            let body = o + 16;
            let mut chans: Vec<Vec<i16>> = vec![Vec::with_capacity(samples); channels];
            for c in 0..channels {
                let base = body + c * frames * 16;
                for f in 0..frames {
                    let p = base + f * 16;
                    if p + 16 > d.len() {
                        break;
                    }
                    state[c].frame(&d[p..p + 16], &mut chans[c]);
                }
                chans[c].truncate(samples);
            }
            let n = chans.iter().map(Vec::len).min().unwrap_or(0);
            for k in 0..n {
                for ch in &chans {
                    pcm.push(ch[k]);
                }
            }
        } else if id == b"SCEl" {
            break;
        }
        o += size;
    }
    Ok(Sample { rate, channels: channels as u32, pcm, loop_start: None, loop_end: None })
}

/// Write PCM to a WAV file (debug tooling).
pub fn write_wav(path: &std::path::Path, s: &Sample) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(44 + s.pcm.len() * 2);
    let data_len = (s.pcm.len() * 2) as u32;
    let ch = s.channels.max(1);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(ch as u16).to_le_bytes());
    out.extend_from_slice(&s.rate.to_le_bytes());
    out.extend_from_slice(&(s.rate * ch * 2).to_le_bytes());
    out.extend_from_slice(&((ch * 2) as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in &s.pcm {
        out.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, out)
}
