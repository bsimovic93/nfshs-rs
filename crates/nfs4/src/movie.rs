//! MOVIE.CPP: the front end's movies (the intro, the attract mode's). Only `play_movie` runs:
//! the host plays the movies itself in place of `Movie_Play` (0x8004d1c4, the host's), so the
//! rest of the file (`Movie_Init`, `Movie_Load`, `Movie_NextFrame`, the CD streaming's
//! `str*` functions decoding into the MDEC) is never reached, like MDEC.CPP.
//!
//! Written as FEMENU.CPP is (see there).

use crate::call::call_addr;
use ps1rt::Cpu;

/// `Movie_Play` (MOVIE.CPP, 0x8004d1c4): the host's.
const MOVIE_PLAY: u32 = 0x8004_D1C4;

/// `play_movie` (MOVIE.CPP, 0x8004d3dc): movie `k` played (the host's `Movie_Play`); whether it
/// was skipped.
pub fn play_movie(c: &mut Cpu, k: u32) -> u32 {
    call_addr(c, MOVIE_PLAY, &[k & 0xFF])
}

pub fn abi_play_movie(c: &mut Cpu) {
    c.r[2] = play_movie(c, c.r[4]);
}
