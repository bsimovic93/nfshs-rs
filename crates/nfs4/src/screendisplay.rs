//! SCREENDISPLAY.CPP: the display options' screen (`tScreenDisplay`): its 32 pictures drawn as
//! bright as the options menu's fade.

use crate::abi::{self, Saved};
use crate::femenuextended::DRAW_SHAPE_EXTENDED;
use crate::globals::SONGS_SCREEN;
use crate::fescreen;
use ps1rt::Cpu;

/// The options menu's fade, in the menus' block (`SONGS_SCREEN`'s).
const OPTIONS_FADE: u32 = 10972;
/// "zDsply".
const DISPLAY_NAME: u32 = 0x8001_2078;

/// A call to a function that is ported too, as FEAPP's.
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        c.r[4..4 + args.len()].copy_from_slice(args);
        f(c);
        c.r[2]
    })
}

/// `tScreenDisplay::DrawBackground` (SCREENDISPLAY.CPP, 0x80045d38): shapes 11 to 42 at half the
/// options menu's fade (at most 128).
pub fn abi_draw_background(c: &mut Cpu) {
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let menus = c.m.r32(SONGS_SCREEN);
        let v = (c.m.r32(menus + OPTIONS_FADE) as i32 >> 1) as u32;
        let bright = if (v as i16) >= 129 { 128 } else { v as i16 as i32 as u32 };
        r.0[1] = bright;
        for k in 0..32 {
            r.0[0] = k;
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[k + 11, 0, 0, 0, bright, 0, 0], 0x8004_5da0);
        }
    })
}

/// `tScreenDisplay::GetShapeInfo` (SCREENDISPLAY.CPP, 0x80045dc4): 43 shapes of "zDsply", none
/// swapped.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_name = abi::arg(c, 4);
    let (permanent, swap, name) = (c.r[5], c.r[6], c.r[7]);
    let m = &mut c.m;
    m.w32(swap_name, 0);
    m.w16(swap, 0);
    m.w16(permanent, 43);
    m.w32(name, DISPLAY_NAME);
}

/// `tScreenDisplay::~tScreenDisplay` (SCREENDISPLAY.H, 0x80045dec): the screen's.
pub fn abi_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        port(c, &Saved::of(c), 0x8002_6098, 0x8004_5dfc, &[this, flags], fescreen::abi_screen_destructor);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, TOP, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};

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

    #[test]
    fn screendisplay_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xD26 + k as u64, shuffle, &args);
        }
    }
}
