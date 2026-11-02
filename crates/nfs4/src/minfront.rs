//! MINFRONT.CPP: the front end's side of the game's start. Of the file only `ParseOptions` is
//! in the retail game.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::call_addr;
use ps1rt::Cpu;

/// The block's name, `"FE Data Stream"`; the memory library's `reservememadr` and
/// `resizememadr`; the stream's size.
const NAME: u32 = 0x8001_000C;
const RESERVE: u32 = 0x800E_5F28;
const RESIZE: u32 = 0x800F_2684;
const STREAM_SIZE: u32 = 8192;

/// `MinFront_ParseOptions` (MINFRONT.CPP, 0x800128a0): the front end's data stream (8192
/// bytes reserved, 16-aligned), empty (its first word 0) and cut down to that word; the stream.
pub fn parse_options(c: &mut Cpu) -> u32 {
    let stream = call_addr(c, RESERVE, &[NAME, STREAM_SIZE, 16]);
    c.m.w32(stream, 0);
    call_addr(c, RESIZE, &[stream, 4]);
    stream
}

pub fn abi_parse_options(c: &mut Cpu) {
    c.r[2] = parse_options(c);
}

#[cfg(test)]
mod tests {
    use crate::femenu::tests::{self as base, ADDR, via_abi};
    use crate::test_util::{against_original_in, fe_ram};

    #[test]
    fn minfront_fuzz() {
        let Some(ram) = fe_ram() else { return };
        ADDR.set(0x8001_28a0);
        against_original_in(&ram, "MinFront_ParseOptions", 20, 0x3A1, base::shuffle, nfs4rc::f_800128a0, &|_| vec![], via_abi);
    }
}
