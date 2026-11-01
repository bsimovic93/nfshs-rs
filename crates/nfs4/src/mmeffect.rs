//! MMEFFECT.CPP: the front end's blending mode, a `DR_MODE` primitive in the ordering table.

use crate::abi::{self, Saved};
use crate::femenuextended::{NEXT_PRIM, OT};
use ps1rt::{Cpu, Mem};

/// `GetTPage`, `SetDrawMode`.
const GET_TPAGE: fn(&mut Cpu) = nfs4rc::f_800f2930;
const SET_DRAW_MODE: fn(&mut Cpu) = nfs4rc::f_800eed18;

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

/// `FeDraw_SetABRMode` (MMEFFECT.CPP, 0x8004dbc0): a `DR_MODE` (12 bytes) setting the
/// semi-transparency mode `mode` on a 16-bit texture page at 0, 256, no dithering.
pub fn abi_set_abr_mode(c: &mut Cpu) {
    let mode = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let prim = add_prim(&mut c.m, 12);
        r.0[0] = prim;
        let tpage = r.call_leaving(c, GET_TPAGE, &[2, mode, 0, 256], 0x8004_dc34);
        r.call_leaving(c, SET_DRAW_MODE, &[prim, 0, 0, tpage & 0xFFFF, 0], 0x8004_dc4c);
    })
}

/// `FeDraw_SetABRMode` entered as the original's callers `jal` it.
pub(crate) fn set_abr_mode_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_dbc0, None, &[]);
    abi_set_abr_mode(c);
}
