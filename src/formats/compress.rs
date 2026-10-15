//! EA compression codecs used by the `.Q??` files, ported from the game's own routines:
//! dispatcher at 0x800E4A74, Huffman 0x800F4EF8, RefPack 0x800F5C14 (NFS4.EXE, NTSC-U).
//!
//! The first two bytes identify the codec: `xx FB` with `xx & 0xFE` =
//! 0x10 RefPack, 0x30 Huffman, 0x32 Huffman + delta, 0x34 Huffman + double delta, 0x46 BTree.

use anyhow::{Result, bail};

pub fn is_compressed(d: &[u8]) -> bool {
    d.len() >= 5 && d[1] == 0xFB && matches!(d[0] & 0xFE, 0x10 | 0x30 | 0x32 | 0x34 | 0x46)
}

pub fn decompress(d: &[u8]) -> Result<Vec<u8>> {
    if !is_compressed(d) {
        bail!("unknown compression header {:02x}{:02x}", d[0], d[1]);
    }
    match d[0] & 0xFE {
        0x10 => refpack(d),
        0x30 | 0x32 | 0x34 => huffman(d),
        0x46 => btree(d),
        _ => unreachable!(),
    }
}

fn be24(d: &[u8], o: usize) -> usize {
    ((d[o] as usize) << 16) | ((d[o + 1] as usize) << 8) | d[o + 2] as usize
}

pub fn refpack(d: &[u8]) -> Result<Vec<u8>> {
    let mut i = if d[0] & 1 != 0 { 5 } else { 2 };
    let ulen = be24(d, i);
    i += 3;
    let mut out: Vec<u8> = Vec::with_capacity(ulen);
    let byte = |i: usize| d.get(i).copied().unwrap_or(0);
    let copy = |out: &mut Vec<u8>, off: usize, len: usize| -> Result<()> {
        if off > out.len() {
            bail!("refpack: bad back-reference");
        }
        let start = out.len() - off;
        for k in 0..len {
            let b = out[start + k];
            out.push(b);
        }
        Ok(())
    };
    while i < d.len() {
        let b0 = byte(i) as usize;
        if b0 & 0x80 == 0 {
            let b1 = byte(i + 1) as usize;
            let lit = b0 & 3;
            out.extend_from_slice(&d[i + 2..(i + 2 + lit).min(d.len())]);
            i += 2 + lit;
            copy(&mut out, ((b0 & 0x60) << 3) + b1 + 1, ((b0 >> 2) & 7) + 3)?;
        } else if b0 & 0x40 == 0 {
            let (b1, b2) = (byte(i + 1) as usize, byte(i + 2) as usize);
            let lit = b1 >> 6;
            out.extend_from_slice(&d[i + 3..(i + 3 + lit).min(d.len())]);
            i += 3 + lit;
            copy(&mut out, ((b1 & 0x3F) << 8) + b2 + 1, (b0 & 0x3F) + 4)?;
        } else if b0 & 0x20 == 0 {
            let (b1, b2, b3) = (byte(i + 1) as usize, byte(i + 2) as usize, byte(i + 3) as usize);
            let lit = b0 & 3;
            out.extend_from_slice(&d[i + 4..(i + 4 + lit).min(d.len())]);
            i += 4 + lit;
            copy(&mut out, ((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1, ((b0 & 0x0C) << 6) + b3 + 5)?;
        } else if b0 < 0xFC {
            let lit = ((b0 & 0x1F) + 1) * 4;
            out.extend_from_slice(&d[i + 1..(i + 1 + lit).min(d.len())]);
            i += 1 + lit;
        } else {
            let lit = b0 & 3;
            out.extend_from_slice(&d[i + 1..(i + 1 + lit).min(d.len())]);
            break;
        }
    }
    out.truncate(ulen);
    Ok(out)
}

/// MSB-first bit reader over 16-bit big-endian units (pads with zeros past the end).
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    buf: u64,
    n: u32,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8], pos: usize) -> Self {
        Self { d, pos, buf: 0, n: 0 }
    }
    fn fill(&mut self) {
        while self.n <= 48 {
            let b = self.d.get(self.pos).copied().unwrap_or(0) as u64;
            self.pos += 1;
            self.buf |= b << (56 - self.n);
            self.n += 8;
        }
    }
    fn peek(&mut self, k: u32) -> u32 {
        self.fill();
        (self.buf >> (64 - k)) as u32
    }
    fn skip(&mut self, k: u32) {
        self.fill();
        self.buf <<= k;
        self.n -= k;
    }
    fn get(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        let v = self.peek(k);
        self.skip(k);
        v
    }
    /// EA variable-length number: z zero bits, a one bit, then z+2 value bits; result v + 2^(z+2) - 4.
    fn num(&mut self) -> u32 {
        let mut z = 0;
        while self.get(1) == 0 {
            z += 1;
            if z > 30 {
                return 0;
            }
        }
        let n = z + 2;
        self.get(n) + (1 << n) - 4
    }
}

pub fn huffman(d: &[u8]) -> Result<Vec<u8>> {
    let kind = u16::from_be_bytes([d[0], d[1]]);
    let mut b = Bits::new(d, 2);
    if kind & 0x100 != 0 {
        b.get(24); // compressed size
    }
    let ulen = b.get(24) as usize;
    let clue = b.get(8) as u8;

    // code-length counts, canonical bases and 16-bit-aligned limits
    let mut count = [0u32; 33];
    let mut base = [0i64; 33];
    let mut limit = [0u64; 33];
    let mut code: u64 = 0;
    let mut leaves: u32 = 0;
    let mut n = 1usize;
    let max_len;
    loop {
        if n > 32 {
            bail!("huffman: code table overflow");
        }
        code <<= 1;
        base[n] = code as i64 - leaves as i64;
        let c = b.num();
        count[n] = c;
        leaves += c;
        code += c as u64;
        let lim = if c != 0 { (code << (16 - n.min(16))) & 0xFFFF } else { 0 };
        limit[n] = lim;
        if c != 0 && lim == 0 {
            limit[n] = u64::MAX;
            max_len = n;
            break;
        }
        n += 1;
    }

    // symbol list: each entry is a skip count over not-yet-used byte values
    let mut used = [false; 256];
    let mut syms = Vec::with_capacity(leaves as usize);
    let mut prev: u8 = 0xFF;
    for _ in 0..leaves {
        let mut skip = b.num() + 1;
        loop {
            prev = prev.wrapping_add(1);
            if !used[prev as usize] {
                skip -= 1;
                if skip == 0 {
                    break;
                }
            }
        }
        used[prev as usize] = true;
        syms.push(prev);
    }

    // 8-bit fast lookup
    const LONG: u8 = 0x40;
    const ESC: u8 = 0x60;
    let mut fast_sym = [0u8; 256];
    let mut fast_len = [LONG; 256];
    let mut clue_len = 0u32;
    let mut k = 0usize;
    let mut si = 0usize;
    for len in 1..=max_len.min(8) {
        let reps = 1usize << (8 - len);
        for _ in 0..count[len] {
            let s = syms[si];
            si += 1;
            let mut l = len as u8;
            if s == clue {
                clue_len = len as u32;
                l = ESC;
            }
            for _ in 0..reps {
                fast_sym[k] = s;
                fast_len[k] = l;
                k += 1;
            }
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(ulen);
    loop {
        let peek = b.peek(8) as usize;
        let fl = fast_len[peek];
        if fl != LONG && fl != ESC {
            out.push(fast_sym[peek]);
            b.skip(fl as u32);
            continue;
        }
        let len = if fl == ESC {
            clue_len
        } else {
            let top = b.peek(16) as u64;
            let mut l = 9;
            while l < max_len && top >= limit[l] {
                l += 1;
            }
            l as u32
        };
        let c = b.get(len) as i64;
        let idx = (c - base[len as usize]) as usize;
        let s = *syms.get(idx).ok_or_else(|| anyhow::anyhow!("huffman: bad code"))?;
        if s != clue {
            out.push(s);
            continue;
        }
        // escape
        let run = b.num();
        if run != 0 {
            let last = *out.last().unwrap_or(&0);
            out.extend(std::iter::repeat_n(last, run as usize));
        } else if b.get(1) == 0 {
            out.push(b.get(8) as u8);
        } else {
            break;
        }
        if out.len() > ulen + 0x10000 {
            bail!("huffman: runaway output");
        }
    }
    out.truncate(ulen);
    match kind & !0x100 {
        0x32FB => {
            let mut acc = 0u8;
            for v in out.iter_mut() {
                acc = acc.wrapping_add(*v);
                *v = acc;
            }
        }
        0x34FB => {
            let (mut a, mut b2) = (0u8, 0u8);
            for v in out.iter_mut() {
                a = a.wrapping_add(*v);
                b2 = b2.wrapping_add(a);
                *v = b2;
            }
        }
        _ => {}
    }
    Ok(out)
}

/// Byte-pair "BTree" codec (0x800F5F10): node bytes expand recursively into two bytes,
/// the clue byte escapes a literal (or 0 = end of stream).
pub fn btree(d: &[u8]) -> Result<Vec<u8>> {
    let mut i = if d[0] == 0x47 { 5 } else { 2 };
    let ulen = be24(d, i);
    i += 3;
    let byte = |i: usize| d.get(i).copied().unwrap_or(0);
    let mut kind = [0i8; 256];
    let mut left = [0u8; 256];
    let mut right = [0u8; 256];
    kind[byte(i) as usize] = 1;
    let nodes = byte(i + 1) as usize;
    i += 2;
    for _ in 0..nodes {
        let n = byte(i) as usize;
        left[n] = byte(i + 1);
        right[n] = byte(i + 2);
        kind[n] = -1;
        i += 3;
    }
    fn expand(x: u8, kind: &[i8; 256], l: &[u8; 256], r: &[u8; 256], out: &mut Vec<u8>, depth: u32) {
        if kind[x as usize] == 0 || depth > 64 {
            out.push(x);
        } else {
            expand(l[x as usize], kind, l, r, out, depth + 1);
            expand(r[x as usize], kind, l, r, out, depth + 1);
        }
    }
    let mut out = Vec::with_capacity(ulen);
    while i < d.len() {
        let b = byte(i);
        i += 1;
        match kind[b as usize] {
            0 => out.push(b),
            k if k < 0 => {
                expand(left[b as usize], &kind, &left, &right, &mut out, 0);
                expand(right[b as usize], &kind, &left, &right, &mut out, 0);
            }
            _ => {
                let lit = byte(i);
                i += 1;
                if lit == 0 {
                    break;
                }
                out.push(lit);
            }
        }
    }
    out.truncate(ulen);
    Ok(out)
}
