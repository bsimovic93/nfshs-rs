//! MINFRONT.CPP: the front end's side of the game's start. Of the file only `ParseOptions` is
//! in the retail game.

use crate::abi::{self, Saved};
use ps1rt::Cpu;

/// The block's name, `"FE Data Stream"`; the memory library's `reservememadr` and
/// `resizememadr`.
const NAME: u32 = 0x8001_000C;
const RESERVE: u32 = 0x800E_5F28;
const RESIZE: u32 = 0x800F_2684;

/// `MinFront_ParseOptions` (MINFRONT.CPP, 0x800128a0): the front end's data stream (8192
/// bytes reserved, 16-aligned), empty (its first word 0) and cut down to that word; the stream.
pub fn abi_parse_options(c: &mut Cpu) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        let stream = r.call_ptr_leaving(c, RESERVE, &[NAME, 8192, 16], 0x8001_28c0);
        r.0[0] = stream;
        c.m.w32(stream, 0);
        r.call_ptr_leaving(c, RESIZE, &[stream, 4], 0x8001_28d4);
        c.r[2] = stream;
    })
}

#[cfg(test)]
mod tests {
    use crate::femenu::tests::{self as base, ADDR, TOP, via_abi};
    use crate::test_util::{against_original_in, fe_ram, same_stack_in};

    #[test]
    fn minfront_fuzz() {
        let Some(ram) = fe_ram() else { return };
        ADDR.set(0x8001_28a0);
        against_original_in(&ram, "MinFront_ParseOptions", 20, 0x3A1, base::shuffle, nfs4rc::f_800128a0, &|_| vec![], via_abi);
        same_stack_in(&ram, "MinFront_ParseOptions", 0x8001_28a0, nfs4rc::f_800128a0, 0x8001_3000, TOP, 20, 0x3A2, base::shuffle, &|_, _| vec![]);
    }
}
