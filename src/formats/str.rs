//! PlayStation movies (.XA files on the disc, here with whole Mode 2 sectors: 8-byte
//! subheader + 2328 bytes): STR video frames (MDEC bitstream version 2) interleaved with XA
//! ADPCM audio.

use std::collections::HashMap;

const SECTOR: usize = 2336;

pub struct Movie {
    pub width: u32,
    pub height: u32,
    /// Each frame's MDEC bitstream.
    pub frames: Vec<Vec<u8>>,
    /// Interleaved stereo (or mono) samples at `rate` Hz.
    pub audio: Vec<i16>,
    pub stereo: bool,
    pub rate: u32,
}

impl Movie {
    /// The front end's video files (.DCT): chunks "mTCD", size, 0, width, height, then one
    /// frame's MDEC bitstream; no sound.
    pub fn parse_dct(data: &[u8]) -> Self {
        let mut m = Movie { width: 0, height: 0, frames: Vec::new(), audio: Vec::new(), stereo: false, rate: 37800 };
        let mut o = 0;
        while o + 16 <= data.len() && &data[o..o + 4] == b"mTCD" {
            let size = u32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap()) as usize;
            if size < 16 || o + size > data.len() {
                break;
            }
            m.width = u16::from_le_bytes([data[o + 12], data[o + 13]]) as u32;
            m.height = u16::from_le_bytes([data[o + 14], data[o + 15]]) as u32;
            m.frames.push(data[o + 16..o + size].to_vec());
            o += size;
        }
        m
    }

    pub fn parse(data: &[u8]) -> Self {
        let mut m = Movie { width: 0, height: 0, frames: Vec::new(), audio: Vec::new(), stereo: true, rate: 37800 };
        let mut current: Option<(u32, Vec<Option<Vec<u8>>>)> = None;
        let mut adpcm = [[0i32; 2]; 2];
        for s in data.chunks_exact(SECTOR) {
            let (submode, coding) = (s[2], s[3]);
            let body = &s[8..];
            if submode & 0x04 != 0 {
                // XA audio: 18 sound groups of 128 bytes
                m.stereo = coding & 1 != 0;
                m.rate = if coding & 4 != 0 { 18900 } else { 37800 };
                for g in body[..18 * 128].chunks_exact(128) {
                    decode_sound_group(g, m.stereo, &mut adpcm, &mut m.audio);
                }
            } else if submode & 0x02 != 0 && u16::from_le_bytes([body[0], body[1]]) == 0x0160 {
                let chunk = u16::from_le_bytes([body[4], body[5]]) as usize;
                let chunks = u16::from_le_bytes([body[6], body[7]]) as usize;
                let frame = u32::from_le_bytes(body[8..12].try_into().unwrap());
                m.width = u16::from_le_bytes([body[16], body[17]]) as u32;
                m.height = u16::from_le_bytes([body[18], body[19]]) as u32;
                if current.as_ref().is_none_or(|c| c.0 != frame) {
                    if let Some((_, parts)) = current.take() {
                        m.frames.push(parts.into_iter().flatten().flatten().collect());
                    }
                    current = Some((frame, vec![None; chunks.max(1)]));
                }
                if let Some((_, parts)) = &mut current {
                    if chunk < parts.len() {
                        parts[chunk] = Some(body[32..32 + 2016].to_vec());
                    }
                }
            }
        }
        if let Some((_, parts)) = current {
            m.frames.push(parts.into_iter().flatten().flatten().collect());
        }
        m
    }
}

// ---------------------------------------------------------------- XA ADPCM

const K0: [i32; 5] = [0, 60, 115, 98, 122];
const K1: [i32; 5] = [0, 0, -52, -55, -60];

/// One 128-byte sound group of 4-bit ADPCM: 8 units of 28 samples (alternating left and
/// right in stereo).
fn decode_sound_group(g: &[u8], stereo: bool, prev: &mut [[i32; 2]; 2], out: &mut Vec<i16>) {
    let mut units: [[i16; 28]; 8] = [[0; 28]; 8];
    for (u, unit) in units.iter_mut().enumerate() {
        let param = g[4 + u];
        let range = (param & 0xF).min(12) as i32;
        let filter = ((param >> 4) as usize).min(4);
        let ch = if stereo { u & 1 } else { 0 };
        for (j, s) in unit.iter_mut().enumerate() {
            let byte = g[16 + j * 4 + u / 2];
            let nibble = if u & 1 == 0 { byte & 0xF } else { byte >> 4 };
            // the signed nibble in the top of 16 bits, shifted down by the range
            let raw = (((nibble as u16) << 12) as i16 as i32) >> range;
            let v = raw + (prev[ch][0] * K0[filter] + prev[ch][1] * K1[filter] + 32) / 64;
            let v = v.clamp(-32768, 32767);
            prev[ch][1] = prev[ch][0];
            prev[ch][0] = v;
            *s = v as i16;
        }
    }
    if stereo {
        for pair in units.chunks_exact(2) {
            for j in 0..28 {
                out.push(pair[0][j]);
                out.push(pair[1][j]);
            }
        }
    } else {
        for unit in &units {
            out.extend_from_slice(unit);
        }
    }
}

// ---------------------------------------------------------------- MDEC bitstream

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    /// Bit `i` of the stream: 16-bit little-endian words, most significant bit first.
    fn bit(&self, i: usize) -> u32 {
        let w = i / 16;
        if w * 2 + 1 >= self.d.len() {
            return 0;
        }
        let word = u16::from_le_bytes([self.d[w * 2], self.d[w * 2 + 1]]);
        ((word >> (15 - i % 16)) & 1) as u32
    }
    fn peek(&self, n: usize) -> u32 {
        (0..n).fold(0, |v, k| (v << 1) | self.bit(self.pos + k))
    }
    fn get(&mut self, n: usize) -> u32 {
        let v = self.peek(n);
        self.pos += n;
        v
    }
}

/// MPEG-1 table B.14 (the AC coefficients after the first): code, run, level; the sign bit
/// follows the code.
const AC_CODES: &[(&str, u8, u16)] = &[
    ("11", 0, 1), ("011", 1, 1), ("0100", 0, 2), ("0101", 2, 1), ("00101", 0, 3), ("00111", 3, 1), ("00110", 4, 1),
    ("000110", 1, 2), ("000111", 5, 1), ("000101", 6, 1), ("000100", 7, 1), ("0000110", 0, 4), ("0000100", 2, 2),
    ("0000111", 8, 1), ("0000101", 9, 1), ("00100110", 0, 5), ("00100001", 0, 6), ("00100101", 1, 3), ("00100100", 3, 2),
    ("00100111", 10, 1), ("00100011", 11, 1), ("00100010", 12, 1), ("00100000", 13, 1), ("0000001010", 0, 7),
    ("0000001100", 1, 4), ("0000001011", 2, 3), ("0000001111", 4, 2), ("0000001001", 5, 2), ("0000001110", 14, 1),
    ("0000001101", 15, 1), ("0000001000", 16, 1), ("000000011101", 0, 8), ("000000011000", 0, 9), ("000000010011", 0, 10),
    ("000000010000", 0, 11), ("000000011011", 1, 5), ("000000010100", 2, 4), ("000000011100", 3, 3), ("000000010010", 4, 3),
    ("000000011110", 6, 2), ("000000010101", 7, 2), ("000000010001", 8, 2), ("000000011111", 17, 1), ("000000011010", 18, 1),
    ("000000011001", 19, 1), ("000000010111", 20, 1), ("000000010110", 21, 1), ("0000000011010", 0, 12), ("0000000011001", 0, 13),
    ("0000000011000", 0, 14), ("0000000010111", 0, 15), ("0000000010110", 1, 6), ("0000000010101", 1, 7), ("0000000010100", 2, 5),
    ("0000000010011", 3, 4), ("0000000010010", 5, 3), ("0000000010001", 9, 2), ("0000000010000", 10, 2), ("0000000011111", 22, 1),
    ("0000000011110", 23, 1), ("0000000011101", 24, 1), ("0000000011100", 25, 1), ("0000000011011", 26, 1),
    ("00000000011111", 0, 16), ("00000000011110", 0, 17), ("00000000011101", 0, 18), ("00000000011100", 0, 19),
    ("00000000011011", 0, 20), ("00000000011010", 0, 21), ("00000000011001", 0, 22), ("00000000011000", 0, 23),
    ("00000000010111", 0, 24), ("00000000010110", 0, 25), ("00000000010101", 0, 26), ("00000000010100", 0, 27),
    ("00000000010011", 0, 28), ("00000000010010", 0, 29), ("00000000010001", 0, 30), ("00000000010000", 0, 31),
    ("000000000011000", 0, 32), ("000000000010111", 0, 33), ("000000000010110", 0, 34), ("000000000010101", 0, 35),
    ("000000000010100", 0, 36), ("000000000010011", 0, 37), ("000000000010010", 0, 38), ("000000000010001", 0, 39),
    ("000000000010000", 0, 40), ("000000000011111", 1, 8), ("000000000011110", 1, 9), ("000000000011101", 1, 10),
    ("000000000011100", 1, 11), ("000000000011011", 1, 12), ("000000000011010", 1, 13), ("000000000011001", 1, 14),
    ("0000000000010011", 1, 15), ("0000000000010010", 1, 16), ("0000000000010001", 1, 17), ("0000000000010000", 1, 18),
    ("0000000000010100", 6, 3), ("0000000000011010", 11, 2), ("0000000000011001", 12, 2), ("0000000000011000", 13, 2),
    ("0000000000010111", 14, 2), ("0000000000010110", 15, 2), ("0000000000010101", 16, 2), ("0000000000011111", 27, 1),
    ("0000000000011110", 28, 1), ("0000000000011101", 29, 1), ("0000000000011100", 30, 1), ("0000000000011011", 31, 1),
];

/// The MDEC's quantization table (MPEG-1 default intra, DC entry 2), in natural order.
const QUANT: [u32; 64] = [
    2, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38, 22, 22, 26, 27, 29, 34, 37, 40, 22, 26, 27, 29, 32, 35, 40,
    48, 26, 27, 29, 32, 35, 40, 48, 58, 26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29,
    22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub struct Decoder {
    codes: HashMap<(u8, u32), (u8, u16)>,
    cos: [[f32; 8]; 8],
}

impl Default for Decoder {
    fn default() -> Self {
        let codes = AC_CODES.iter().map(|&(c, r, l)| ((c.len() as u8, u32::from_str_radix(c, 2).unwrap()), (r, l))).collect();
        let cos = std::array::from_fn(|x| {
            std::array::from_fn(|u| {
                let c = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                c * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos() / 2.0
            })
        });
        Self { codes, cos }
    }
}

impl Decoder {
    /// One 8x8 block: DC (10 bits, signed), then run/level codes up to end of block ("10").
    fn block(&self, b: &mut Bits, qscale: u32) -> [f32; 64] {
        let mut coef = [0f32; 64];
        let dc = ((b.get(10) as i32) << 22) >> 22;
        coef[0] = (dc * QUANT[0] as i32) as f32;
        let mut k = 0usize;
        loop {
            if b.peek(2) == 0b10 {
                b.pos += 2;
                break;
            }
            let (run, level) = if b.peek(6) == 0b000001 {
                // escape: 6-bit run, 10-bit signed level
                b.pos += 6;
                let run = b.get(6) as usize;
                let level = ((b.get(10) as i32) << 22) >> 22;
                (run, level)
            } else {
                let mut found = None;
                for len in 2..=16u8 {
                    if let Some(&(r, l)) = self.codes.get(&(len, b.peek(len as usize))) {
                        b.pos += len as usize;
                        let sign = b.get(1);
                        found = Some((r as usize, if sign == 1 { -(l as i32) } else { l as i32 }));
                        break;
                    }
                }
                match found {
                    Some(f) => f,
                    None => break, // corrupt: end the block
                }
            };
            k += run + 1;
            if k >= 64 {
                break;
            }
            let n = ZIGZAG[k];
            coef[n] = ((level * (QUANT[n] * qscale) as i32 + 4) / 8) as f32;
        }
        // separable inverse DCT
        let mut tmp = [0f32; 64];
        for y in 0..8 {
            for x in 0..8 {
                tmp[y * 8 + x] = (0..8).map(|u| self.cos[x][u] * coef[y * 8 + u]).sum();
            }
        }
        let mut out = [0f32; 64];
        for x in 0..8 {
            for y in 0..8 {
                out[y * 8 + x] = (0..8).map(|v| self.cos[y][v] * tmp[v * 8 + x]).sum();
            }
        }
        out
    }

    /// A frame as RGBA: macroblocks of 16x16 go down the columns, each Cr, Cb and four Y blocks.
    pub fn frame(&self, bs: &[u8], w: u32, h: u32) -> Vec<u8> {
        let (w, h) = (w as usize, h as usize);
        let mut rgba = vec![0u8; w * h * 4];
        if bs.len() < 8 {
            return rgba;
        }
        let qscale = u16::from_le_bytes([bs[4], bs[5]]) as u32;
        let mut b = Bits { d: &bs[8..], pos: 0 };
        let (mbw, mbh) = (w.div_ceil(16), h.div_ceil(16));
        for mx in 0..mbw {
            for my in 0..mbh {
                let cr = self.block(&mut b, qscale);
                let cb = self.block(&mut b, qscale);
                let ys = [self.block(&mut b, qscale), self.block(&mut b, qscale), self.block(&mut b, qscale), self.block(&mut b, qscale)];
                for py in 0..16 {
                    for px in 0..16 {
                        let (x, y) = (mx * 16 + px, my * 16 + py);
                        if x >= w || y >= h {
                            continue;
                        }
                        let yb = &ys[(py / 8) * 2 + px / 8];
                        let l = yb[(py % 8) * 8 + px % 8] + 128.0;
                        let c = (py / 2) * 8 + px / 2;
                        let (r_, b_) = (cr[c], cb[c]);
                        let r = l + 1.402 * r_;
                        let g = l - 0.3437 * b_ - 0.7143 * r_;
                        let bl = l + 1.772 * b_;
                        let i = (y * w + x) * 4;
                        rgba[i] = r.clamp(0.0, 255.0) as u8;
                        rgba[i + 1] = g.clamp(0.0, 255.0) as u8;
                        rgba[i + 2] = bl.clamp(0.0, 255.0) as u8;
                        rgba[i + 3] = 255;
                    }
                }
            }
        }
        rgba
    }
}
