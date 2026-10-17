//! High-level replacements for the PS1 BIOS calls the game uses (A0 table: C string/memory
//! helpers). Calls are made as `jal 0xA0` with the function number in t1 (r9).

use super::Cpu;

fn strlen(c: &mut Cpu, a: u32) -> u32 {
    let mut n = 0;
    while c.m.r8(a + n) != 0 {
        n += 1;
    }
    n
}

/// C-style integer parsing: whitespace, sign, digits in `base` (0 = by prefix). Returns the
/// value and the address after the last character used.
fn parse_int(c: &mut Cpu, mut a: u32, base: u32) -> (i32, u32) {
    while matches!(c.m.r8(a), b' ' | 9..=13) {
        a += 1;
    }
    let neg = match c.m.r8(a) {
        b'-' => {
            a += 1;
            true
        }
        b'+' => {
            a += 1;
            false
        }
        _ => false,
    };
    let mut base = base;
    if (base == 0 || base == 16) && c.m.r8(a) == b'0' && matches!(c.m.r8(a + 1), b'x' | b'X') {
        a += 2;
        base = 16;
    } else if base == 0 {
        base = if c.m.r8(a) == b'0' { 8 } else { 10 };
    }
    let mut v: i64 = 0;
    loop {
        let d = match c.m.r8(a) {
            ch @ b'0'..=b'9' => (ch - b'0') as u32,
            ch @ b'a'..=b'z' => (ch - b'a') as u32 + 10,
            ch @ b'A'..=b'Z' => (ch - b'A') as u32 + 10,
            _ => break,
        };
        if d >= base {
            break;
        }
        v = v * base as i64 + d as i64;
        a += 1;
    }
    ((if neg { -v } else { v }) as i32, a)
}

pub fn call(c: &mut Cpu, table: u32) {
    let (a0, a1, a2) = (c.r[4], c.r[5], c.r[6]);
    let f = c.r[9];
    let ret = match (table, f) {
        (0xA0, 0x15) => {
            // strcat
            let d = a0 + strlen(c, a0);
            let mut i = 0;
            loop {
                let ch = c.m.r8(a1 + i);
                c.m.w8(d + i, ch);
                i += 1;
                if ch == 0 {
                    break;
                }
            }
            a0
        }
        (0xA0, 0x16) => {
            // strncat
            let d = a0 + strlen(c, a0);
            let mut i = 0;
            while i < a2 {
                let ch = c.m.r8(a1 + i);
                if ch == 0 {
                    break;
                }
                c.m.w8(d + i, ch);
                i += 1;
            }
            c.m.w8(d + i, 0);
            a0
        }
        (0xA0, 0x17) | (0xA0, 0x18) => {
            // strcmp / strncmp
            let mut i = 0;
            let mut res = 0i32;
            while f == 0x17 || i < a2 {
                let (x, y) = (c.m.r8(a0 + i), c.m.r8(a1 + i));
                if x != y {
                    res = x as i32 - y as i32;
                    break;
                }
                if x == 0 {
                    break;
                }
                i += 1;
            }
            res as u32
        }
        (0xA0, 0x19) => {
            // strcpy
            let mut i = 0;
            loop {
                let ch = c.m.r8(a1 + i);
                c.m.w8(a0 + i, ch);
                i += 1;
                if ch == 0 {
                    break;
                }
            }
            a0
        }
        (0xA0, 0x1A) => {
            // strncpy
            let mut end = false;
            for i in 0..a2 {
                let ch = if end { 0 } else { c.m.r8(a1 + i) };
                end |= ch == 0;
                c.m.w8(a0 + i, ch);
            }
            a0
        }
        (0xA0, 0x1B) => strlen(c, a0),
        (0xA0, 0x1C) | (0xA0, 0x1E) => {
            // index / strchr
            let mut i = 0;
            loop {
                let ch = c.m.r8(a0 + i);
                if ch == a1 as u8 {
                    break a0 + i;
                }
                if ch == 0 {
                    break 0;
                }
                i += 1;
            }
        }
        (0xA0, 0x1D) | (0xA0, 0x1F) => {
            // rindex / strrchr
            let (mut i, mut res) = (0, 0);
            loop {
                let ch = c.m.r8(a0 + i);
                if ch == a1 as u8 {
                    res = a0 + i;
                }
                if ch == 0 {
                    break res;
                }
                i += 1;
            }
        }
        (0xA0, 0x25) => (a0 as u8).to_ascii_uppercase() as u32,
        (0xA0, 0x26) => (a0 as u8).to_ascii_lowercase() as u32,
        (0xA0, 0x2A) | (0xA0, 0x2C) => {
            // memcpy / memmove
            let mut buf = vec![0u8; a2 as usize];
            c.m.read_bytes(a1, &mut buf);
            c.m.write_bytes(a0, &buf);
            a0
        }
        (0xA0, 0x2B) => {
            // memset
            for i in 0..a2 {
                c.m.w8(a0 + i, a1 as u8);
            }
            a0
        }
        (0xA0, 0x2D) => {
            // memcmp
            let mut res = 0i32;
            for i in 0..a2 {
                let (x, y) = (c.m.r8(a0 + i), c.m.r8(a1 + i));
                if x != y {
                    res = x as i32 - y as i32;
                    break;
                }
            }
            res as u32
        }
        (0xA0, 0x0C) | (0xA0, 0x0D) | (0xA0, 0x10) | (0xA0, 0x11) => {
            // strtol(s, endp, base) / strtoul / atoi(s) / atol(s)
            let base = if matches!(f, 0x0C | 0x0D) { a2 } else { 10 };
            let (value, end) = parse_int(c, a0, base);
            if matches!(f, 0x0C | 0x0D) && a1 != 0 {
                c.m.w32(a1, end);
            }
            value as u32
        }
        (0xA0, 0x0E) | (0xA0, 0x0F) => (a0 as i32).unsigned_abs(), // abs / labs
        (0xA0, 0x24) => {
            // strstr(s, sub)
            let n = strlen(c, a1);
            let len = strlen(c, a0);
            (0..=len.saturating_sub(n))
                .find(|&i| (0..n).all(|k| c.m.r8(a0 + i + k) == c.m.r8(a1 + k)))
                .filter(|_| n <= len)
                .map_or(0, |i| a0 + i)
        }
        (0xA0, 0x2E) => {
            // memchr(s, ch, n)
            (0..a2).find(|&i| c.m.r8(a0 + i) == a1 as u8).map_or(0, |i| a0 + i)
        }
        (0xA0, 0x2F) => {
            // rand: the BIOS's linear congruential generator
            c.rand_seed = c.rand_seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
            (c.rand_seed >> 16) & 0x7FFF
        }
        (0xA0, 0x30) => {
            // srand(seed)
            c.rand_seed = a0;
            0
        }
        // SysEnqIntRP / SysDeqIntRP: the host raises interrupts itself (libpad's
        // _padStopCom dequeues its handler when the game closes the pad for the memory card)
        (0xC0, 0x02) | (0xC0, 0x03) => 0,
        _ => {
            static REPORTED: std::sync::Mutex<Vec<(u32, u32)>> = std::sync::Mutex::new(Vec::new());
            if let Ok(mut r) = REPORTED.lock() {
                if !r.contains(&(table, f)) {
                    r.push((table, f));
                    eprintln!("unhandled BIOS call {table:#x}:{f:#x} (returns 0)");
                }
            }
            0
        }
    };
    c.r[2] = ret;
}
