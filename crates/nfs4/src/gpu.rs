//! The GPU's primitives as the game builds them in RAM: PSY-Q's libgpu set-up macros and
//! helpers (`SetPolyF4`, `SetSemiTrans`...), and the primitive buffer and ordering table the
//! game keeps on the scratchpad (`addPrim` as its drawing code inlines it).

use ps1rt::Mem;

/// The scratchpad's ordering table entry being drawn into and the next free primitive.
pub const OT: u32 = 0x1F80_0000;
pub const NEXT_PRIM: u32 = 0x1F80_0004;

/// A primitive of `size` bytes taken from the scratchpad's buffer and linked into the ordering
/// table (the original's reads and writes in its order).
pub fn add_prim(m: &mut Mem, size: u32) -> u32 {
    let ot = m.r32(OT);
    add_prim_to(m, ot, size)
}

/// [`add_prim`] into the ordering table entry at `ot`.
pub fn add_prim_to(m: &mut Mem, ot: u32, size: u32) -> u32 {
    let prim = m.r32(NEXT_PRIM);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    let entry = m.r32(ot);
    m.w32(NEXT_PRIM, prim.wrapping_add(size));
    m.w32(ot, (entry & 0xFF00_0000) | (prim & 0x00FF_FFFF));
    prim
}

/// A primitive's length in words after its tag (+3) and its command code (+7).
fn set_len_code(m: &mut Mem, prim: u32, len: u8, code: u8) {
    m.w8(prim + 3, len);
    m.w8(prim + 7, code);
}

/// `SetPolyF3`: a flat triangle.
pub fn set_poly_f3(m: &mut Mem, prim: u32) {
    set_len_code(m, prim, 4, 0x20);
}

/// `SetPolyF4`: a flat quad.
pub fn set_poly_f4(m: &mut Mem, prim: u32) {
    set_len_code(m, prim, 5, 0x28);
}

/// `SetPolyG4`: a gouraud-shaded quad.
pub fn set_poly_g4(m: &mut Mem, prim: u32) {
    set_len_code(m, prim, 8, 0x38);
}

/// `SetPolyGT4`: a gouraud-shaded textured quad.
pub fn set_poly_gt4(m: &mut Mem, prim: u32) {
    set_len_code(m, prim, 12, 0x3C);
}

/// `SetPolyFT4`: a flat textured quad.
pub fn set_poly_ft4(m: &mut Mem, prim: u32) {
    set_len_code(m, prim, 9, 0x2C);
}

/// `SetSemiTrans`: the primitive semi-transparent (its code's bit 1) or not.
pub fn set_semi_trans(m: &mut Mem, prim: u32, on: bool) {
    let code = m.r8(prim + 7);
    m.w8(prim + 7, if on { code | 2 } else { code & !2 });
}

/// A vertex word: `y` in the top half, `x` below (not masked, as the game's `or` leaves it).
pub fn xy(x: u32, y: u32) -> u32 {
    (y << 16) | x
}
