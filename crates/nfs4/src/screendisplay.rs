//! SCREENDISPLAY.CPP: the display options' screen (`tScreenDisplay`): its 32 pictures drawn as
//! bright as the options menu's fade.
//!
//! Written as FEMENU.CPP is (see there): typed views, calls through `crate::call`.

use crate::call::call;
use crate::femenuextended::DRAW_SHAPE_EXTENDED;
use crate::fescreen;
use crate::globals::SONGS_SCREEN;
use crate::types::Screen;
use ps1rt::{Cpu, Mem};

/// The options menu's fade, in the menus' block (`SONGS_SCREEN`'s).
const OPTIONS_FADE: u32 = 10972;
/// The screen's pictures (32 shapes from 11 of its 43) and its shape file, "zDsply".
const FIRST_PICTURE: u32 = 11;
const PICTURES: u32 = 32;
const SHAPES: u16 = 43;
const DISPLAY_NAME: u32 = 0x8001_2078;

/// `tScreenDisplay::DrawBackground` (SCREENDISPLAY.CPP, 0x80045d38): the pictures at half the
/// options menu's fade (at most 128).
pub fn draw_background(c: &mut Cpu) {
    let menus = c.m.r32(SONGS_SCREEN);
    let half = (c.m.r32(menus + OPTIONS_FADE) as i32 >> 1) as i16;
    let bright = half.min(128) as i32 as u32;
    for k in 0..PICTURES {
        call(c, DRAW_SHAPE_EXTENDED, &[FIRST_PICTURE + k, 0, 0, 0, bright, 0, 0]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c);
}

/// `tScreenDisplay::GetShapeInfo` (SCREENDISPLAY.CPP, 0x80045dc4): 43 shapes of "zDsply", none
/// swapped.
pub fn get_shape_info(m: &mut Mem, permanent: u32, swap: u32, name: u32, swap_name: u32) {
    m.w32(swap_name, 0);
    m.w16(swap, 0);
    m.w16(permanent, SHAPES);
    m.w32(name, DISPLAY_NAME);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = crate::abi::arg(c, 4);
    get_shape_info(&mut c.m, c.r[5], c.r[6], c.r[7], swap_name);
}

/// `tScreenDisplay::~tScreenDisplay` (SCREENDISPLAY.H, 0x80045dec): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    fescreen::screen_destructor(c, Screen(c.r[4]), c.r[5]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's display screen; the outputs of `GetShapeInfo`.
    const SCREEN: u32 = 0x8018_0690;
    const OUT: u32 = 0x8014_7000;

    /// FEMENU's structures, then the options menu's fade about where the pictures are full
    /// bright, negative and past a short.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        let menus = m.r32(SONGS_SCREEN);
        let v = [0, 1, 255, 256, 257, 258, 259, (-2i32) as u32, 0x1_0102, rng.next() as u32][pick(rng, 10) as usize];
        m.w32(menus + OPTIONS_FADE, v);
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenDisplay::DrawBackground", 0x8004_5d38, nfs4rc::f_80045d38, |_, _| vec![SCREEN]),
            ("tScreenDisplay::GetShapeInfo", 0x8004_5dc4, nfs4rc::f_80045dc4, |_, _| vec![SCREEN, OUT, OUT + 2, OUT + 4, OUT + 8]),
            ("~tScreenDisplay", 0x8004_5dec, nfs4rc::f_80045dec, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn screendisplay_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xD15 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xD16 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
