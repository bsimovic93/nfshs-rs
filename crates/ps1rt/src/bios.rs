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
        _ => {
            if c.trace {
                eprintln!("unhandled BIOS call {table:#x}:{f:#x}");
            }
            0
        }
    };
    c.r[2] = ret;
}
