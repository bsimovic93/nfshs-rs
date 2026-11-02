//! MMEFFECT.CPP: the front end's blending mode, a `DR_MODE` primitive in the ordering table.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::call;
use crate::femenuextended::{NEXT_PRIM, OT};
use ps1rt::{Cpu, Mem};

/// `GetTPage`, `SetDrawMode`.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const SET_DRAW_MODE: fn(&mut Cpu) = nfs4rc::f_800eed18;

/// A `DR_MODE`'s size; a 16-bit texture page at 0, 256.
const DR_MODE_SIZE: u32 = 12;
const PAGE_16BIT: u32 = 2;

/// A primitive of `size` bytes taken from the scratchpad's buffer and linked into the ordering
/// table (the original's reads and writes in its order).
pub(crate) fn add_prim(m: &mut Mem, size: u32) -> u32 {
    let prim = m.r32(NEXT_PRIM);
    let ot = m.r32(OT);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    let entry = m.r32(ot);
    m.w32(NEXT_PRIM, prim.wrapping_add(size));
    m.w32(ot, (entry & 0xFF00_0000) | (prim & 0x00FF_FFFF));
    prim
}

/// `FeDraw_SetABRMode` (MMEFFECT.CPP, 0x8004dbc0): a `DR_MODE` setting the semi-transparency
/// mode `mode` (on a 16-bit texture page at 0, 256; no dithering, no drawing to the display).
pub fn set_abr_mode(c: &mut Cpu, mode: u32) {
    let prim = add_prim(&mut c.m, DR_MODE_SIZE);
    let tpage = call(c, GET_TPAGE, &[PAGE_16BIT, mode, 0, 256]);
    call(c, SET_DRAW_MODE, &[prim, 0, 0, tpage & 0xFFFF, 0]);
}

pub fn abi_set_abr_mode(c: &mut Cpu) {
    set_abr_mode(c, c.r[4]);
}

/// For callers not rewritten yet: `FeDraw_SetABRMode` with the C convention's arguments.
pub(crate) fn set_abr_mode_entry(c: &mut Cpu) {
    abi_set_abr_mode(c);
}
