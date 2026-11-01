//! MOVIE.CPP: the front end's movies (the intro, the attract mode's). Only `play_movie` runs:
//! the host plays the movies itself in place of `Movie_Play` (0x8004d1c4, the host's), so the
//! rest of the file (`Movie_Init`, `Movie_Load`, `Movie_NextFrame`, the CD streaming's
//! `str*` functions decoding into the MDEC) is never reached, like MDEC.CPP.

use crate::abi::{self, Saved};
use ps1rt::Cpu;

/// `Movie_Play` (MOVIE.CPP, 0x8004d1c4): the host's.
const MOVIE_PLAY: u32 = 0x8004_D1C4;

/// `play_movie` (MOVIE.CPP, 0x8004d3dc): movie `k` played (the host's `Movie_Play`); whether it
/// was skipped.
pub fn abi_play_movie(c: &mut Cpu) {
    let k = c.r[4] & 0xFF;
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        r.call_ptr_leaving(c, MOVIE_PLAY, &[k], 0x8004_d3ec);
    });
}

/// `play_movie` entered as the original's callers `jal` it.
pub(crate) fn play_movie_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8004_d3dc, None, &[]);
    abi_play_movie(c);
}
