//! UDFF.CPP: the game's data files read in order (a `Udff_tInfo` and where it is in the file).

use crate::types::UdffInfo;
use ps1rt::{Cpu, Mem};

/// `Udff_GetInt` (UDFF.CPP, 0x800bc8f8): the next word of the file.
pub fn get_int(m: &mut Mem, file: UdffInfo) -> i32 {
    let at = file.mem_ptr(m);
    let v = m.r32(at);
    file.set_mem_ptr(m, at.wrapping_add(4));
    v as i32
}

pub fn abi_get_int(c: &mut Cpu) {
    c.r[2] = get_int(&mut c.m, UdffInfo(c.r[4])) as u32;
}
