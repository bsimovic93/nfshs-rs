//! SCREENCARSELECT.CPP: the car select screens: the single player's (`tScreenCarSelect`), the
//! duel's (`tScreenCarSelectDuel`, the opponent's car beside), two players' and the pink
//! slips' (`tScreenCarSelectTwoPlayer`, `tScreenPinkSlipsCarSelect`, saving the bet cars):
//! the car turning on its stand (its 3D model drawn with its colours), the overlays (the car's
//! name and price, its stats, the upgrades, the sliders) moving between their places, the
//! video wall showing the car's videos, the brightness, the input.
//!
//! The screen: +100 the overlays (7 of 24 bytes: a rectangle from and to, the fade, its step,
//! the kind), +268 four words, +286 the state, +288 the state before, +290 the brightness.

use crate::abi::{self, Saved};
use crate::globals::{GAME_TICKS, TEXT_COLORS};
use ps1rt::Cpu;

/// The overlays' places, the car objects (by player), their draw counts, the car's 3D
/// world, the car manager and its player's car, the money, the menus.
const OVERLAY_PLACES: u32 = 0x8005_2454;
const CAR_OBJECTS: u32 = 0x8005_2EF8;
const CAR_DRAWS: u32 = 0x8005_174C;
const CAR_WORLD: u32 = 0x8011_8234;
const CAR_MANAGER: u32 = 0x8011_552C;
const PLAYER_CAR: u32 = 0x8011_59E1;
const MONEY: u32 = 0x8011_5DA4;
const ALL_MENUS: u32 = 0x8005_1EE0;
const PAD_TYPE: u32 = 0x8013_FBA9;
/// The car's drawing's flags (the turning's half).
const DRAW_FLAG: u32 = 0x8013_EAF0;
const DRAW_HALF: u32 = 0x8013_EAF4;
const VTABLE: u32 = 0x8001_1CB8;
/// The overlays' bits (three of each), their jump table.
const BITS_A: u32 = 0x8001_1A4C;
const BITS_B: u32 = 0x8001_1A54;

const FIXEDMULT: fn(&mut Cpu) = nfs4rc::f_800e4f14;
const MENU_COLOR_DATA: fn(&mut Cpu) = nfs4rc::f_800bf8a8;
const DRAW_CAR_MODEL: fn(&mut Cpu) = crate::fe3dmenu::draw_menu_rendering_view_entry;
const DRAW_NO_CAR: fn(&mut Cpu) = crate::fe3dmenu::draw_empty_view_entry;
/// The host's `PSXDrawSquare`.
const DRAW_SQUARE: u32 = 0x8004_F12C;

/// A port called from a port with `args` in a0.. (stack ones written by the caller first).
fn port(c: &mut Cpu, r: &Saved, addr: u32, ra: u32, args: &[u32], f: fn(&mut Cpu)) -> u32 {
    r.enter_leaving(c, addr, ra, |c| {
        let n = args.len().min(4);
        c.r[4..4 + n].copy_from_slice(&args[..n]);
        let sp = c.r[29];
        for (k, &v) in args.iter().enumerate().skip(4) {
            c.m.w32(sp + 4 * k as u32, v);
        }
        f(c);
        c.r[2]
    })
}

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

fn hs(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn hu(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

fn b(m: &mut ps1rt::Mem, at: u32) -> u32 {
    m.r8(at) as u32
}

/// A virtual call: the method at `slot` of `this`'s vtable (its offset at `slot - 4`).
fn virtual_call(c: &mut Cpu, r: &Saved, this: u32, slot: u32, args: &[u32], ra: u32) -> u32 {
    let vt = c.m.r32(this + 96);
    let off = hs(&mut c.m, vt + slot - 4);
    let f = c.m.r32(vt + slot);
    let mut a = vec![this.wrapping_add(off)];
    a.extend_from_slice(args);
    r.call_ptr_leaving(c, f, &a, ra)
}

/// `TransformVector` (SCREENCARSELECT.CPP, 0x8003ab80): `out` the 4-vector `v` times the 4x4
/// fixed-point matrix `m` (`out[j] = sum v[i] * m[i][j]`).
pub fn abi_transform_vector(c: &mut Cpu) {
    let (v, mat, out) = (c.r[4], c.r[5], c.r[6]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        (r.0[4], r.0[5], r.0[6]) = (v, mat, out);
        for j in 0..4u32 {
            r.0[3] = j;
            let at = out + 4 * j;
            c.m.w32(at, 0);
            (r.0[2], r.0[1]) = (4 * j, at);
            for i in 0..4u32 {
                r.0[0] = i;
                let (a, m) = (c.m.r32(v + 4 * i), c.m.r32(mat + 16 * i + 4 * j));
                let p = r.call_leaving(c, FIXEDMULT, &[a, m], 0x8003_abfc);
                let s = c.m.r32(at).wrapping_add(p);
                c.m.w32(at, s);
            }
            r.0[0] = 4;
        }
    })
}

/// `DrawCar` (SCREENCARSELECT.CPP, 0x8003ac64): the car (none: its empty stand) turning on its
/// stand at `x`, `y`: the player's car object given its model, colour and lights, its
/// colours set, drawn (the clock's turn's half flagged).
pub fn abi_draw_car(c: &mut Cpu) {
    let (car, x, y, scale) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let (angle, shade, lit, flags, player) = (abi::arg(c, 4), abi::arg(c, 5) & 0xFF, abi::arg(c, 6), abi::arg(c, 7), abi::arg(c, 8));
    abi::frame(c, 88, |c, _| {
        let mut r = Saved::of(c);
        (r.0[6], r.0[5], r.0[1], r.0[4], r.0[7], r.0[2], r.0[3]) = (angle, flags, player, shade, scale, x, y);
        let m = &mut c.m;
        let t = m.r32(GAME_TICKS) as i32;
        let half = t.wrapping_rem(2048) as u32;
        m.w32(DRAW_FLAG, 0);
        m.w32(DRAW_HALF, (half >= 1024) as u32);
        r.0[0] = 0x8005_0000;
        if (m.r8(car) as i8) >= 0 {
            let s0 = CAR_OBJECTS + 4 * player;
            r.0[0] = s0;
            let colour = b(m, car + 197);
            let o = m.r32(s0);
            let model = m.r32(o + 648);
            let v = b(m, car + 1);
            m.w32(model, v);
            let o = m.r32(s0);
            let model = m.r32(o + 648);
            let v = b(m, car + 199);
            m.w32(model + 160, v);
            let draws = m.r32(CAR_DRAWS + 4 * player).wrapping_add(3);
            m.w32(CAR_DRAWS + 4 * player, draws);
            for (shift, at) in [(2, 60), (1, 64), (0, 68)] {
                let v = (b(m, car + 196) >> shift) & 1;
                let o = m.r32(s0);
                let model = m.r32(o + 648);
                m.w32(model + at, v);
            }
            let o = m.r32(s0);
            r.call_leaving(c, MENU_COLOR_DATA, &[colour, o, player], 0x8003_adb0);
            let o = c.m.r32(s0);
            r.call_leaving(c, DRAW_CAR_MODEL, &[o, CAR_WORLD, sx(x), sx(y), player, 0, flags, scale, angle, shade, lit], 0x8003_adf4);
        } else {
            r.call_leaving(c, DRAW_NO_CAR, &[CAR_WORLD, sx(x), sx(y), player, 0, flags, scale, angle], 0x8003_ae2c);
        }
    })
}

/// `tScreenCarSelect::tScreenCarSelect` (SCREENCARSELECT.CPP, 0x8003ae58): a screen with its
/// seven overlays at their places (none fading, their kinds), no state before, its four words
/// clear.
pub fn abi_screen_car_select(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_6040, 0x8003_ae6c, &[this], crate::fescreen::abi_screen);
        let m = &mut c.m;
        m.w32(this + 96, VTABLE);
        m.w16(this + 286, 0);
        m.w16(this + 288, 0xFFFF);
        m.w16(this + 290, 0);
        for k in 0..7 {
            let o = this + 100 + 24 * k;
            for j in 0..16 {
                let v = m.r8(OVERLAY_PLACES + 16 * k + j);
                m.w8(o + j, v);
            }
            m.w16(o + 22, k as u16);
            m.w16(o + 20, 0);
            m.w16(o + 16, 0);
            m.w16(o + 18, 6);
        }
        for k in 0..4 {
            m.w32(this + 268 + 4 * k, 0);
        }
        c.r[2] = this;
    })
}

/// `tScreenCarSelect::~tScreenCarSelect` (SCREENCARSELECT.CPP, 0x8003af68).
pub fn abi_screen_car_select_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        c.m.w32(this + 96, VTABLE);
        port(c, &r, 0x8002_6098, 0x8003_af80, &[this, flags], crate::fescreen::abi_screen_destructor);
    })
}

/// `tScreenCarSelect::Cleanup` (SCREENCARSELECT.CPP, 0x8003af90): the spinning cars' menu
/// cleaned up, the screen's, its read's buffer freed (virtual).
pub fn abi_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8004_dfac, 0x8003_afa4, &[], crate::psxfront::abi_cleanup_spinning_cars_menu);
        port(c, &r, 0x8002_61c0, 0x8003_afac, &[this], crate::fescreen::abi_cleanup);
        virtual_call(c, &r, this, 124, &[], 0x8003_afc8);
    })
}

/// A lerped rectangle's side: `from + fade * (to - from) / 128` (toward 0).
fn lerp(m: &mut ps1rt::Mem, o: u32, from: u32, to: u32) -> u32 {
    let d = hs(m, o + to).wrapping_sub(hs(m, o + from));
    let mut p = (hs(m, o + 16) as i32).wrapping_mul(d as i32);
    if p < 0 {
        p += 127;
    }
    hu(m, o + from).wrapping_add((p >> 7) as u32)
}

/// One of an overlay's three marks (lit by the car's bits), its digit by the clock.
fn draw_mark(c: &mut Cpu, r: &Saved, sp: u32, k: u32, x0: u32, ra: u32) {
    let m = &mut c.m;
    let bits = sp + 288 + 2 * k;
    let t3 = b(m, sp + 244);
    let fade = if t3 & hs(m, bits) != 0 { 0 } else { 96 };
    m.w32(sp + 260, 0xBEBE);
    let bit = hs(m, bits);
    let (x, y) = (hs(m, sp + 32), hs(m, sp + 34));
    let t = (m.r32(GAME_TICKS) as i32 >> 4).wrapping_rem(10) as u32;
    let shape = (10 * k).wrapping_add(t).wrapping_add(98);
    let mode = ((t3 & bit == 0) as u32) | 0x410;
    port(c, r, 0x8004_eb60, ra, &[shape, mode, x.wrapping_add(40 * k + x0), y.wrapping_add(6), fade, 1, sp + 256], crate::psxfront::abi_draw_shape_extended);
}

/// `tScreenCarSelect::DrawOverlay` (SCREENCARSELECT.CPP, 0x8003afd8): an overlay faded a step
/// and its rectangle moved between its places by the fade, then by its kind: the car's name
/// (a round rectangle), its price (bought, the garage's, used) and the money, its options'
/// marks, its stats' marks and the screen's choice with its help, the sliders; its frame.
pub fn abi_draw_overlay(c: &mut Cpu) {
    let (this, o) = (c.r[4], c.r[5]);
    abi::frame(c, 320, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[3], r.0[2]) = (o, this);
        let m = &mut c.m;
        for (from, to) in [(BITS_A, sp + 280), (BITS_B, sp + 288)] {
            for j in 0..6 {
                let v = m.r8(from + j);
                m.w8(to + j, v);
            }
        }
        if o == 0 {
            return;
        }
        let car = sp + 48;
        let s1 = virtual_call(c, &r, this, 108, &[car], 0x8003_b058);
        r.0[1] = s1;
        let m = &mut c.m;
        if m.r16(o + 20) != 0 {
            let v = (hu(m, o + 18).wrapping_mul(hu(m, o + 20))).wrapping_add(hu(m, o + 16));
            m.w16(o + 16, v as u16);
            let f = v as u16 as i16 as i32;
            if f <= 0 {
                m.w16(o + 16, 0);
                m.w16(o + 20, 0);
            } else if f >= 128 {
                m.w16(o + 16, 128);
                m.w16(o + 20, 0);
            }
        }
        for (k, (from, to)) in [(0, 8), (2, 10), (4, 12), (6, 14)].into_iter().enumerate() {
            let v = lerp(m, o, from, to);
            m.w16(sp + 32 + 2 * k as u32, v as u16);
        }
        let all = m.r32(ALL_MENUS);
        match hs(m, o + 22) {
            0 => {
                if s1 != 0 {
                    let name = (m.r8(car) as i8 as i32 as u32).wrapping_add(290);
                    port(c, &r, 0x8004_c490, 0x8003_b1c8, &[name, sp + 32, 0], crate::drawshp::abi_round_rectangle);
                }
            }
            1..=3 => {
                let (x, y) = (hu(m, sp + 32), hu(m, sp + 34));
                m.w16(sp + 46, 23);
                let a1 = x.wrapping_add(15);
                let h = y.wrapping_add(hu(m, sp + 38)).wrapping_sub(30);
                let w = hu(m, sp + 36).wrapping_sub(30);
                m.w16(sp + 40, a1 as u16);
                m.w16(sp + 42, h as u16);
                m.w16(sp + 44, w as u16);
                if s1 != 0 {
                    let name = sx((m.r8(car) as i8 as i32 as u32).wrapping_add(290));
                    port(c, &r, 0x8002_12e0, 0x8003_b254, &[name, sx(a1.wrapping_add(w).wrapping_sub(12)), sx(y.wrapping_add(2)), 1, 1, 3], crate::fetextrender::abi_menu_text_positioned_justify);
                }
                let m = &mut c.m;
                let kind = hs(m, o + 22);
                let title = match kind {
                    2 => 142,
                    3 => 143,
                    _ => 141,
                };
                r.0[0] = title;
                let price;
                if s1 == 0 {
                    r.0[1] = 0x23_2323;
                    price = 0;
                } else {
                    r.0[1] = 0xBEBE;
                    price = if kind == 2 {
                        m.r32(sp + 80)
                    } else if kind == 3 {
                        let all = m.r32(ALL_MENUS);
                        let k = hs(m, all + 7996);
                        let slot = hs(m, sp + 280 + 2 * k);
                        m.r32((sp + 80).wrapping_add(slot << 2))
                    } else {
                        let p = b(m, PLAYER_CAR);
                        port(c, &r, 0x8001_648c, 0x8003_b2f0, &[CAR_MANAGER, p], crate::fecars::abi_calc_used_price)
                    };
                }
                r.0[2] = 0x23_2323;
                let s1v = r.0[1];
                let m = &mut c.m;
                let (x, y, w) = (hs(m, sp + 40), hs(m, sp + 42), hs(m, sp + 44));
                port(c, &r, 0x8003_9a80, 0x8003_b334, &[x.wrapping_add(w).wrapping_sub(12), y.wrapping_add(3), 6, price, s1v, 0x23_2323], crate::screenpost::abi_draw_money);
                (r.0[1], r.0[0]) = (1, 3);
                let m = &mut c.m;
                let mid = |m: &mut ps1rt::Mem| sx(hu(m, sp + 40).wrapping_add(((hu(m, sp + 44) << 16) as i32 >> 17) as u32));
                let x = mid(m);
                let y = sx(hu(m, sp + 42).wrapping_add(3));
                port(c, &r, 0x8002_12e0, 0x8003_b37c, &[title, x, y, 1, 1, 3], crate::fetextrender::abi_menu_text_positioned_justify);
                let m = &mut c.m;
                let (x, y, w) = (hs(m, sp + 40), hs(m, sp + 42), hs(m, sp + 44));
                let money = m.r32(MONEY);
                port(c, &r, 0x8003_9a80, 0x8003_b3b0, &[x.wrapping_add(w).wrapping_sub(12), y.wrapping_add(13), 9, money, 0xBEBE, 0x23_2323], crate::screenpost::abi_draw_money);
                let m = &mut c.m;
                let x = mid(m);
                let y = sx(hu(m, sp + 42).wrapping_add(13));
                port(c, &r, 0x8002_12e0, 0x8003_b3f0, &[124, x, y, 1, 1, 3], crate::fetextrender::abi_menu_text_positioned_justify);
                port(c, &r, 0x8004_c69c, 0x8003_b3f8, &[sp + 40], crate::drawshp::abi_rectangle);
                let m = &mut c.m;
                let (x, y, w) = (hs(m, sp + 32), hs(m, sp + 34), hs(m, sp + 36));
                r.call_ptr_leaving(c, DRAW_SQUARE, &[0, x, y, w, 10], 0x8003_b414);
            }
            4 => {
                for k in 0..3 {
                    r.0[0] = k + 1;
                    draw_mark(c, &r, sp, k, 33, 0x8003_b4f0);
                }
            }
            5 => {
                r.0[0] = 0;
                if m.r16(o + 16) == 128 {
                    for k in 0..3 {
                        r.0[0] = k + 1;
                        draw_mark(c, &r, sp, k, 133, 0x8003_b5d8);
                    }
                    r.0[0] = 0x8005_0000;
                    let m = &mut c.m;
                    let (x, y, w, h) = (hu(m, sp + 32), hu(m, sp + 34), hu(m, sp + 36), hu(m, sp + 38));
                    let a1 = sx(x.wrapping_add(((w << 16) as i32 >> 17) as u32));
                    let a2 = sx(y.wrapping_add(24));
                    m.w16(sp + 42, y.wrapping_add(35) as u16);
                    m.w16(sp + 40, x.wrapping_add(30) as u16);
                    m.w16(sp + 44, w.wrapping_sub(60) as u16);
                    m.w16(sp + 46, h.wrapping_sub(75) as u16);
                    let all = m.r32(ALL_MENUS);
                    let choice = sx(m.r32(all + 7996).wrapping_add(151));
                    port(c, &r, 0x8002_12e0, 0x8003_b660, &[choice, a1, a2, 2, 2, 3], crate::fetextrender::abi_menu_text_positioned_justify);
                    let m = &mut c.m;
                    let all = m.r32(ALL_MENUS);
                    let k = hs(m, all + 7996);
                    let mut help = k.wrapping_add(176);
                    if help == 177 {
                        let v = m.r8(car) as i8 as i32;
                        if v == 12 || v == 10 {
                            help = 65;
                        }
                    }
                    port(c, &r, 0x8002_19ec, 0x8003_b6b4, &[sx(help), sp + 40, 2, 8], crate::fetextrender::abi_word_wrap);
                    let m = &mut c.m;
                    let all = m.r32(ALL_MENUS);
                    let k = hs(m, all + 7996);
                    let bit = hs(m, (sp + 288).wrapping_add(k << 1));
                    let hint = if b(m, sp + 244) & bit != 0 {
                        161
                    } else if m.r8(PAD_TYPE) == 35 {
                        160
                    } else {
                        159
                    };
                    r.0[0] = hint;
                    let (x, w, y, h) = (hu(m, sp + 32), hu(m, sp + 36), hu(m, sp + 34), hu(m, sp + 38));
                    port(c, &r, 0x8002_12e0, 0x8003_b74c, &[hint, sx(x.wrapping_add(w).wrapping_sub(15)), sx(y.wrapping_add(h).wrapping_sub(20)), 1, 2, 3], crate::fetextrender::abi_menu_text_positioned_justify);
                }
                let m = &mut c.m;
                if (hs(m, sp + 36) as i32) >= 67 && (hs(m, sp + 38) as i32) >= 51 {
                    let (w, h, x, y) = (hu(m, sp + 36), hu(m, sp + 38), hu(m, sp + 32), hu(m, sp + 34));
                    m.w16(sp + 44, w.wrapping_sub(30) as u16);
                    m.w16(sp + 46, h.wrapping_sub(25) as u16);
                    m.w16(sp + 40, x.wrapping_add(15) as u16);
                    m.w16(sp + 42, y.wrapping_add(20) as u16);
                    port(c, &r, 0x8004_c69c, 0x8003_b7a0, &[sp + 40], crate::drawshp::abi_rectangle);
                }
            }
            6 => {
                if s1 == 0 {
                    for k in 0..20 {
                        m.w8(sp + 96 + k, 0);
                    }
                    r.0[0] = 5;
                }
                let (x, y) = (hu(m, sp + 32), hu(m, sp + 34));
                port(c, &r, 0x8003_c978, 0x8003_b840, &[this, car, sx(x.wrapping_add(13)), sx(y.wrapping_add(4))], abi_draw_sliders);
            }
            _ => {}
        }
        let _ = all;
        if c.m.r16(o + 22) != 0 {
            port(c, &r, 0x8004_c9bc, 0x8003_b858, &[sp + 32, 1], crate::drawshp::abi_trans_rectangle);
        }
    })
}

/// The spline's control points (5 words each), its basis matrix, the camera's offsets.
const SPLINE_POINTS: u32 = 0x8005_2308;
const SPLINE_BASIS: u32 = 0x8005_23A8;
const CAMERA_OFFSETS: u32 = 0x8005_30A8;
/// The overlays each state shows (4 a state), the wall's TV layout, the music's flag.
const STATE_OVERLAYS: u32 = 0x8005_2434;
const WALL_TVS: u32 = 0x8005_2414;
const MUSIC_FLAG: u32 = 0x8005_19B0;
/// `"zcars"`, the car's video's file (`sprintf`'s format and its buffer), `"VideoWall"`.
const SHAPE_FILE: u32 = 0x8001_1A7C;
const VIDEO_FORMAT: u32 = 0x8001_1A84;
const VIDEO_FILE: u32 = 0x8005_30B8;
const WALL_NAME: u32 = 0x8001_1A88;

const FIXEDDIV: fn(&mut Cpu) = nfs4rc::f_800e4ff0;
const STOP_SONG: fn(&mut Cpu) = nfs4rc::f_8007b798;
const GET_DCT_BUFFER: fn(&mut Cpu) = nfs4rc::f_800dd0c0;
const RESET_DCT_BUFFER: fn(&mut Cpu) = nfs4rc::f_800dd0ac;
const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;

/// `tScreenCarSelect::SetState` (SCREENCARSELECT.CPP, 0x8003b874): a new state: (leaving the
/// wall's states) the wall off and dark; each of the four overlay slots kept if the state shows
/// it, fading out if not, or given the state's overlay fading in; then for the race's states
/// (the showroom, the stats, the race) the clock's marks, the music stopped and the camera's
/// spline set off for the showroom, else the wall's TVs reset, the screen transitioned on
/// (unless from the stats) and the wall on.
pub fn abi_set_state(c: &mut Cpu) {
    let (this, state) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let old = c.m.r16(this + 284) as u32;
        (r.0[0], r.0[1], r.0[2]) = (this, state, old);
        if state != hs(&mut c.m, this + 284) {
            c.m.w16(this + 284, state as u16);
            if old.wrapping_sub(2) & 0xFFFF < 2 {
                port(c, &r, 0x8003_7050, 0x8003_b8b8, &[this + 772], crate::fevideowall::abi_turn_off);
                port(c, &r, 0x8003_c7b0, 0x8003_b8c8, &[this, 0, 0], abi_set_brightness);
                c.m.w16(this + 286, 0xFFFF);
                c.m.w16(this + 288, 0xFFFF);
            }
            let m = &mut c.m;
            for k in 0..4u32 {
                let slot = this + 4 * k;
                let ov = m.r32(slot + 268);
                let want = m.r8(STATE_OVERLAYS.wrapping_add(k).wrapping_add(state << 2)) as i8 as i32 as u32;
                if ov != 0 {
                    if hs(m, ov + 22) != want {
                        m.w16(ov + 20, 0xFFFF);
                    }
                } else if (want as i32) >= 0 {
                    let o = (this + 100).wrapping_add(want.wrapping_mul(24));
                    m.w32(slot + 268, o);
                    m.w16(o + 16, 0);
                    let o = m.r32(slot + 268);
                    m.w16(o + 20, 1);
                }
            }
        }
        if !matches!(state, 0 | 2 | 5 | 6) {
            return;
        }
        let m = &mut c.m;
        let showroom = (state.wrapping_sub(5) < 2) as u32;
        m.w32(this + 904, showroom);
        let t = m.r32(GAME_TICKS);
        m.w32(MUSIC_FLAG, 1);
        m.w32(this + 916, 0);
        m.w32(this + 912, t);
        m.w32(this + 908, t);
        if showroom != 0 {
            r.call_leaving(c, STOP_SONG, &[1000], 0x8003_b9f0);
            let m = &mut c.m;
            m.w32(this + 920, 0);
            let v = m.r32(this + 924) & 0x3FF;
            m.w32(SPLINE_POINTS + 36, v);
            for k in 0..5 {
                let at = SPLINE_POINTS + 4 * k;
                let (a, b0) = (m.r32(at + 40), m.r32(at + 20));
                m.w32(at, b0.wrapping_sub(a.wrapping_sub(b0)));
            }
            for k in 0..4 {
                m.w32(CAMERA_OFFSETS + 4 * k, 0x1_0000);
            }
            port(c, &r, 0x8002_6824, 0x8003_ba74, &[this, 2, 0], crate::fescreen::abi_transition_off);
        } else {
            let m = &mut c.m;
            for k in 0..10 {
                m.w32(this + 292 + 48 * k, 0);
                m.w16(this + 296 + 48 * k, 0);
            }
            if old as u16 as i16 != 1 {
                port(c, &r, 0x8002_6844, 0x8003_bad8, &[this, 2, 0], crate::fescreen::abi_transition_on);
            }
            port(c, &r, 0x8003_70f0, 0x8003_bae0, &[this + 772], crate::fevideowall::abi_turn_on);
        }
    })
}

/// A spline output's halving and rounding to 16.16's integer part.
fn half16(v: u32) -> u32 {
    let h = v as i32 >> 1;
    ((if h < 0 { h.wrapping_add(0xFFFF) } else { h }) >> 16) as u32
}

/// `tScreenCarSelect::CalcSplinePosition` (SCREENCARSELECT.CPP, 0x8003baf8): the camera on
/// the spline through control points `p0`..`p3` at `t` (of 600): its position (x, y halved,
/// z) and its target (the four points' fifth words plus the camera's offsets).
pub fn abi_calc_spline_position(c: &mut Cpu) {
    let (p0, p1, p2) = (c.r[5], c.r[6], c.r[7]);
    let p3 = abi::arg(c, 4);
    let t = abi::arg(c, 5);
    let outs: Vec<u32> = (6..11).map(|k| abi::arg(c, k)).collect();
    abi::frame(c, 168, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[3], r.0[5], r.0[6]) = (p0, p1, p2, p3);
        let t = r.call_leaving(c, FIXEDDIV, &[t << 16, 0x0258_0000], 0x8003_bb40);
        c.m.w32(sp + 24, t);
        let t2 = r.call_leaving(c, FIXEDMULT, &[t, t], 0x8003_bb50);
        c.m.w32(sp + 20, t2);
        let t3 = r.call_leaving(c, FIXEDMULT, &[t2, t], 0x8003_bb60);
        let m = &mut c.m;
        m.w32(sp + 16, t3);
        m.w32(sp + 28, 0x1_0000);
        let points = [p0, p1, p2, p3];
        for k in 0..4u32 {
            for (row, &p) in points.iter().enumerate() {
                let v = m.r32(SPLINE_POINTS.wrapping_add(p.wrapping_mul(20)).wrapping_add(4 * k));
                m.w32(sp + 32 + 16 * row as u32 + 4 * k, v);
            }
        }
        (r.0[7], r.0[0]) = (SPLINE_BASIS, sp + 96);
        port(c, &r, 0x8003_ab80, 0x8003_bc30, &[sp + 16, SPLINE_BASIS, sp + 96], abi_transform_vector);
        (r.0[4], r.0[2]) = (sp + 32, sp + 112);
        port(c, &r, 0x8003_ab80, 0x8003_bc48, &[sp + 96, sp + 32, sp + 112], abi_transform_vector);
        let m = &mut c.m;
        let x = (m.r32(sp + 112) as i32 >> 1) as u32;
        m.w32(outs[0], x);
        let y = (m.r32(sp + 116) as i32 >> 1) as u32;
        m.w32(outs[1], y);
        let z = half16(m.r32(sp + 120));
        m.w32(outs[2], z);
        let w = half16(m.r32(sp + 124));
        m.w32(outs[3], w);
        for (row, &p) in points.iter().enumerate() {
            let v = m.r32(SPLINE_POINTS.wrapping_add(p.wrapping_mul(20)) + 16);
            let o = m.r32(CAMERA_OFFSETS + 4 * row as u32);
            m.w32(sp + 32 + 16 * row as u32, v.wrapping_add(o));
        }
        port(c, &r, 0x8003_ab80, 0x8003_bd58, &[sp + 16, SPLINE_BASIS, sp + 96], abi_transform_vector);
        port(c, &r, 0x8003_ab80, 0x8003_bd68, &[sp + 96, sp + 32, sp + 112], abi_transform_vector);
        let v = half16(c.m.r32(sp + 112));
        c.m.w32(outs[4], v);
    })
}

/// `tScreenCarSelect::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003bdb8): 142 shapes of
/// `zcars`, 11 swapped from the car's video's file (the chosen car's, or the first stock
/// car's), whose car, colour and kind are noted as shown.
pub fn abi_get_shape_info(c: &mut Cpu) {
    let (this, count, swap, file) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        m.w16(count, 142);
        m.w16(swap, 11);
        m.w32(file, SHAPE_FILE);
        let car = sp + 16;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_be04);
        if ok ^ 1 != 0 {
            port(c, &r, 0x8001_6fd8, 0x8003_be20, &[CAR_MANAGER, 0, car], crate::fecars::abi_get_stock_car);
        }
        r.0[0] = VIDEO_FILE;
        let m = &mut c.m;
        let v = b(m, sp + 216);
        m.w16(this + 286, v as u16);
        let v = m.r8(car) as i8 as i16;
        m.w16(this + 288, v as u16);
        let v = b(m, sp + 215);
        m.w16(this + 290, v as u16);
        r.call_leaving(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, sp + 24], 0x8003_be5c);
        c.m.w32(swap_file, VIDEO_FILE);
    })
}

/// `tScreenCarSelect::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003be7c): another car (or
/// colour, or a bonus car's livery) than shown: its video's file read (a real car's), noted,
/// the wall dark and off.
pub fn abi_update_video_wall(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    update_wall(c, this, car, 8, [0x8003_befc, 0x8003_bf30, 0x8003_bf38]);
}

/// `tScreenCarSelect::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003bf4c): the videos'
/// buffer (40000 bytes of the DCT's).
pub fn abi_allocate_async_buffer(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let v = r.call_leaving(c, GET_DCT_BUFFER, &[0x9C40, WALL_NAME], 0x8003_bf6c);
        c.m.w32(this + 48, v);
    })
}

/// `tScreenCarSelect::FreeAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003bf80).
pub fn abi_free_async_buffer(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_leaving(c, RESET_DCT_BUFFER, &[], 0x8003_bf94);
        c.m.w32(this + 48, 0);
    })
}

/// `tScreenCarSelect::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003bfa8): the wall's ten
/// TVs over the shapes, its text and icon places, its images (the shapes loaded).
pub fn abi_initialize_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let wall = this + 772;
        (r.0[1], r.0[0]) = (this, wall);
        let shapes = c.m.r32(this + 40);
        port(c, &r, 0x8003_6988, 0x8003_bff0, &[wall, this + 292, shapes, 0, 10, WALL_TVS, 150], crate::fevideowall::abi_initialize);
        port(c, &r, 0x8003_6b84, 0x8003_c004, &[wall, 249, 320, 80], crate::fevideowall::abi_set_available_text);
        let icons = c.m.r32(this);
        port(c, &r, 0x8003_6ba0, 0x8003_c028, &[wall, 28, 10, 310, 60, icons], crate::fevideowall::abi_set_available_icon);
        if c.m.r16(this + 58) & 1 != 0 {
            port(c, &r, 0x8003_6a38, 0x8003_c044, &[wall], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
    })
}

/// The front end's structure, the track manager and the tournament manager, the track's kind
/// for the race, the front end's car select flag.
const FE: u32 = crate::globals::FRONT_END;
const TRACK_MANAGER: u32 = 0x8011_5D08;
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const TRACK_KIND: u32 = 0x8011_44E0;
const IN_CAR_SELECT: u32 = 0x8005_199C;

/// `tScreenCarSelect::Initialize` (SCREENCARSELECT.CPP, 0x8003c060): the race's track's kind
/// (a tournament's track to race, or the chosen one), the menu's tournament flag, the
/// screen's, the wall initialized (virtual), the licence plate, the shown car noted (none:
/// -1), the brightness and the clocks, the overlays' fades and slots reset (virtual first).
pub fn abi_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 320, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let kind = if c.m.r8(FE + 4) == 2 {
            port(c, &r, 0x8003_27dc, 0x8003_c094, &[TOURNAMENT_MANAGER, sp + 272], crate::fetourn::abi_get_track_to_race);
            let id = c.m.r8(sp + 272) as i8 as i32 as u32;
            let t = port(c, &r, 0x8002_1e60, 0x8003_c0a4, &[TRACK_MANAGER, id], crate::fetracks::abi_get_track_by_id);
            b(&mut c.m, t + 1)
        } else {
            let m = &mut c.m;
            let slot = b(m, FE + 70);
            let track = b(m, FE + slot + 35);
            port(c, &r, 0x8002_1c04, 0x8003_c0cc, &[TRACK_MANAGER, track, sp + 224], crate::fetracks::abi_get_track);
            b(&mut c.m, sp + 225)
        };
        let m = &mut c.m;
        m.w32(TRACK_KIND, kind);
        let all = m.r32(ALL_MENUS);
        m.w32(IN_CAR_SELECT, 1);
        let v = m.r32(all + 8248) & !1;
        m.w32(all + 8248, v);
        if m.r8(FE + 4) == 2 {
            m.w32(all + 8248, v | 1);
        }
        port(c, &r, 0x8002_60cc, 0x8003_c11c, &[this], crate::fescreen::abi_initialize);
        virtual_call(c, &r, this, 116, &[], 0x8003_c138);
        port(c, &r, 0x8002_b84c, 0x8003_c140, &[], crate::front::abi_set_license_plate);
        let m = &mut c.m;
        m.w32(this + 900, 0);
        m.w32(this + 924, 0);
        m.w32(this + 904, 0);
        let ok = virtual_call(c, &r, this, 108, &[sp + 16], 0x8003_c168);
        let m = &mut c.m;
        if ok != 0 {
            let v = b(m, sp + 216);
            m.w16(this + 286, v as u16);
            let v = m.r8(sp + 16) as i8 as i16;
            m.w16(this + 288, v as u16);
            let v = b(m, sp + 215);
            m.w16(this + 290, v as u16);
        } else {
            for at in [286, 290, 288] {
                m.w16(this + at, 0xFFFF);
            }
        }
        let t = m.r32(GAME_TICKS);
        for at in [886, 884, 890, 888] {
            m.w16(this + at, 0);
        }
        m.w32(this + 908, t);
        m.w32(this + 896, t.wrapping_sub(256));
        m.w32(this + 892, t.wrapping_sub(256));
        virtual_call(c, &r, this, 92, &[], 0x8003_c1ec);
        let m = &mut c.m;
        for k in 0..7 {
            m.w16(this + 116 + 24 * k, 0);
            m.w16(this + 120 + 24 * k, 0);
        }
        for k in 0..4 {
            m.w32(this + 268 + 4 * k, 0);
        }
    })
}

/// `tScreenCarSelect::ProcessInput` (SCREENCARSELECT.CPP, 0x8003c268): on a move, the
/// player's buy item's word (buy, a bonus car's, a police car's) and its enabling (the car
/// buyable), the race item on for a test drive or two players; on triangle, the state back
/// (the showroom to the cars, the stats' race to the stats, the wall's states to the list
/// but in a pink slips' race).
pub fn abi_process_input(c: &mut Cpu) {
    let (this, key) = (c.r[4], c.r[6]);
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (key, this);
        let mut k = c.m.r32(key);
        if k == 8 {
            let ok = virtual_call(c, &r, this, 108, &[sp + 16], 0x8003_c2a8);
            let m = &mut c.m;
            let app = m.r32(crate::globals::FE_APP);
            let all = m.r32(ALL_MENUS);
            let item = if m.r8(app + 556) == 0 { all + 8216 } else { all + 8312 };
            m.w32(item + 4, 268);
            let v = m.r32(item) & !1;
            m.w32(item, v);
            if ok != 0 {
                let kind = m.r8(sp + 16) as i8;
                if kind == 8 || kind == 24 {
                    m.w32(item + 4, 269);
                }
                if m.r8(sp + 16) as i8 == 1 {
                    m.w32(item + 4, 270);
                }
                if m.r8(sp + 19) == 0 {
                    let v = m.r32(item) | 1;
                    m.w32(item, v);
                }
            }
            if m.r8(FE + 798) == 1 || m.r8(FE + 3) == 1 {
                let all = m.r32(ALL_MENUS);
                let v = m.r32(all + 8376) | 1;
                m.w32(all + 8376, v);
            }
            k = m.r32(key);
        }
        if k != 16 {
            return;
        }
        let m = &mut c.m;
        let to = match hs(m, this + 284) as i32 {
            5 => 0,
            6 => 2,
            2..=4 if m.r8(FE + 3) != 1 => 1,
            _ => return,
        };
        port(c, &r, 0x8003_b874, 0x8003_c410, &[this, to], abi_set_state);
    })
}

/// `tScreenCarSelect::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003c424): the background, the
/// first overlay, the wall's images (once its shapes are in) or off while the screen fades, the
/// wall updated, valid with a car, its availability the car's, drawn.
pub fn abi_draw_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 248, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let ok = virtual_call(c, &r, this, 108, &[sp + 24], 0x8003_c454);
        r.0[1] = ok;
        let icons = c.m.r32(this);
        port(c, &r, 0x8002_5ce8, 0x8003_c474, &[this, 0, 28, icons, 150], crate::fescreen::abi_draw_background_image);
        let o = c.m.r32(this + 268);
        port(c, &r, 0x8003_afd8, 0x8003_c480, &[this, o], abi_draw_overlay);
        let wall = this + 772;
        if c.m.r16(this + 58) & 1 != 0 && c.m.r32(this + 900) == 0 {
            port(c, &r, 0x8003_6a38, 0x8003_c4ac, &[wall], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
        if (hs(&mut c.m, this + 92) as i32) > 0 && c.m.r32(this + 84) != 0 {
            port(c, &r, 0x8003_7074, 0x8003_c4dc, &[wall], crate::fevideowall::abi_turn_off_instant);
            port(c, &r, 0x8003_c7b0, 0x8003_c4ec, &[this, 0, 0], abi_set_brightness);
        }
        r.0[0] = wall;
        port(c, &r, 0x8003_6bdc, 0x8003_c4f8, &[wall], crate::fevideowall::abi_update);
        port(c, &r, 0x8003_6bd4, 0x8003_c508, &[wall, sx(ok)], crate::fevideowall::abi_set_valid);
        let avail = b(&mut c.m, sp + 31);
        port(c, &r, 0x8003_6bcc, 0x8003_c514, &[wall, avail], crate::fevideowall::abi_set_available);
        port(c, &r, 0x8003_6dac, 0x8003_c51c, &[wall], crate::fevideowall::abi_draw);
    })
}

fn stock_car(c: &mut Cpu, r: &Saved, id: u32, car: u32, ra: u32) {
    port(c, r, 0x8001_6fd8, ra, &[CAR_MANAGER, id, car], crate::fecars::abi_get_stock_car);
}

/// The front end's byte for the car's id at `at` (its colour, its livery).
fn by_car(m: &mut ps1rt::Mem, car: u32, at: u32) -> u32 {
    let id = m.r8(car) as i8 as i32 as u32;
    b(m, FE.wrapping_add(id) + at)
}

/// `tScreenCarSelect::GetCar` (SCREENCARSELECT.CPP, 0x8003c530): the car the state shows: the
/// player's chosen stock car (its colour if a real one, its livery), the second's (bought), a
/// pink slips' car (owned ones needed), else the garage's (some owned, or a test drive; a
/// tournament's needs its cars, but in its stats): its colour then the car's own index of it;
/// whether there's one.
pub fn abi_get_car(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[2]) = (this, car);
        let state = hs(&mut c.m, this + 284);
        let none = |c: &mut Cpu| c.r[2] = 0;
        match state {
            0 | 5 => {
                (r.0[0], r.0[1]) = (0x8011_0000, FE);
                let id = b(&mut c.m, FE + 92);
                stock_car(c, &r, id, car, 0x8003_c598);
                let m = &mut c.m;
                if (b(m, FE + 92) as i32) < m.r32(CAR_MANAGER) as i32 {
                    let v = by_car(m, car, 96);
                    m.w8(car + 197, v as u8);
                }
                let v = by_car(m, car, 192);
                m.w8(car + 199, v as u8);
            }
            2 | 6 => {
                r.0[0] = FE;
                let id = b(&mut c.m, FE + 296);
                stock_car(c, &r, id, car, 0x8003_c5fc);
                let m = &mut c.m;
                let v = by_car(m, car, 96);
                m.w8(car + 7, 1);
                m.w8(car + 197, v as u8);
            }
            7 => {
                if c.m.r8(FE + 298) == 1 {
                    let n = port(c, &r, 0x8001_747c, 0x8003_c640, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
                    if (sx(n) as i32) <= 0 {
                        return none(c);
                    }
                }
                let m = &mut c.m;
                let id = if m.r8(FE + 298) == 0 { b(m, FE + 92) } else { b(m, FE + 291) };
                stock_car(c, &r, id, car, 0x8003_c67c);
                let m = &mut c.m;
                if m.r8(FE + 298) == 0 {
                    let v = by_car(m, car, 96);
                    m.w8(car + 197, v as u8);
                }
                let v = by_car(m, car, 192);
                m.w8(car + 199, v as u8);
            }
            _ => {
                let n = port(c, &r, 0x8001_747c, 0x8003_c6d4, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_owned_cars);
                if (sx(n) as i32) <= 0 && c.m.r8(FE + 4) != 1 {
                    return none(c);
                }
                if c.m.r8(FE + 4) == 2 && hs(&mut c.m, this + 284) != 3 {
                    let n = port(c, &r, 0x8001_74c0, 0x8003_c724, &[CAR_MANAGER, 0], crate::fecars::abi_get_num_tourney_cars);
                    if (sx(n) as i32) <= 0 {
                        return none(c);
                    }
                }
                let m = &mut c.m;
                let id = if hs(m, this + 284) == 3 { b(m, PLAYER_CAR) } else { b(m, FE + 291) };
                stock_car(c, &r, id, car, 0x8003_c764);
                let m = &mut c.m;
                let v = by_car(m, car, 192);
                m.w8(car + 199, v as u8);
            }
        }
        let m = &mut c.m;
        let k = b(m, car + 197);
        let v = b(m, car + k + 175);
        m.w8(car + 197, v as u8);
        c.r[2] = 1;
    })
}

/// `tScreenCarSelect::SetBrightness` (SCREENCARSELECT.CPP, 0x8003c7b0): wall `k`'s brightness
/// to head for.
pub fn abi_set_brightness(c: &mut Cpu) {
    let (this, v, k) = (c.r[4], c.r[5], c.r[6]);
    let at = this.wrapping_add(sx(k) << 1);
    c.m.w16(at + 888, v as u16);
}

/// `tScreenCarSelect::UpdateBrightness` (SCREENCARSELECT.CPP, 0x8003c7c4): wall `k`'s
/// brightness a step (8) toward its target.
pub fn abi_update_brightness(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5]);
    let at = this.wrapping_add(sx(k) << 1);
    let m = &mut c.m;
    let (target, cur) = (hs(m, at + 888) as i32, hs(m, at + 884) as i32);
    let raw = hu(m, at + 884);
    c.r[2] = if cur < target {
        let v = raw.wrapping_add(8);
        m.w16(at + 884, v as u16);
        let t = hu(m, at + 888);
        if target < sx(v) as i32 {
            m.w16(at + 884, t as u16);
        }
        t
    } else if target < cur {
        let v = raw.wrapping_sub(8);
        m.w16(at + 884, v as u16);
        let past = (sx(v) as i32) < hs(m, at + 888) as i32;
        let t = hu(m, at + 888);
        if past {
            m.w16(at + 884, t as u16);
        }
        past as u32
    } else {
        0
    };
}

/// The sliders' labels and their stats' offsets in the car's info; the commentary's word
/// kinds (6 bytes each, the text type at +4); the showroom's flag.
const SLIDER_LABELS: u32 = 0x8005_23FC;
const SLIDER_STATS: u32 = 0x8005_23E8;
const WORD_KINDS: u32 = 0x8005_1AA0;
const SHOWROOM_DRAW: u32 = 0x8005_1748;

/// `DrawCar`: hooked (the host notes the showroom's car), so called by address, never
/// `abi_draw_car` directly: `Cpu::call` lets the host see it, then routes to the port.
const DRAW_CAR: u32 = 0x8003_AC64;
/// Hooked (the host captures its primitives): called by address, never `nfs4rc::f_80024c4c`.
const DRAW_SLIDER: u32 = 0x8002_4C4C;
const FMUL: fn(&mut Cpu) = nfs4rc::f_800f3e0c;
const I2F: fn(&mut Cpu) = nfs4rc::f_800f6b80;
const WORD_FLAGS: fn(&mut Cpu) = nfs4rc::f_800b9f38;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const GETTICK: fn(&mut Cpu) = nfs4rc::f_800e8e0c;

/// `tScreenCarSelect::DrawBackground` (SCREENCARSELECT.CPP, 0x8003c848): with a car, its
/// video's shapes uploaded once read (the car object idle, a while after the last change), the
/// wall on and lit (bright for a car with its video, dim else); the menu's background
/// (virtual) while the screen fades in.
pub fn abi_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 232, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let ok = virtual_call(c, &r, this, 108, &[sp + 16], 0x8003_c874);
        if ok != 0 {
            port(c, &r, 0x8002_5e2c, 0x8003_c884, &[this, this + 40], crate::fescreen::abi_is_shape_file_loaded);
            let m = &mut c.m;
            let mut ready = false;
            if m.r32(this + 44) != 0 && hs(m, this + 792) != u32::MAX {
                let app = m.r32(crate::globals::FE_APP);
                let p = b(m, app + 556);
                let o = m.r32(CAR_OBJECTS + 4 * p);
                if m.r32(o + 2264) == 0 {
                    ready = !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 892)) as i32) < 129);
                }
            }
            if ready {
                port(c, &r, 0x8002_5ff0, 0x8003_c8fc, &[this, 11], crate::fescreen::abi_upload_swap_shapes);
                port(c, &r, 0x8003_70f0, 0x8003_c904, &[this + 772], crate::fevideowall::abi_turn_on);
                let m = &mut c.m;
                if m.r16(this + 884) == m.r16(this + 888) {
                    let v = if m.r8(sp + 23) != 0 { 128 } else { 32 };
                    port(c, &r, 0x8003_c7b0, 0x8003_c938, &[this, v, 0], abi_set_brightness);
                }
            }
        }
        if (hs(&mut c.m, this + 92) as i32) < 128 {
            virtual_call(c, &r, this, 84, &[0], 0x8003_c968);
        }
    })
}

/// `tScreenCarSelect::DrawSliders` (SCREENCARSELECT.CPP, 0x8003c978): the car's five stats as
/// sliders under their labels, each with its upgrades' (the car's three upgrade bits) added.
pub fn abi_draw_sliders(c: &mut Cpu) {
    let (car, x, y) = (c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 88, |c, _| {
        let mut r = Saved::of(c);
        let x = sx(x);
        (r.0[4], r.0[1], r.0[2], r.0[6], r.0[3], r.0[5]) = (car, y, 0, SLIDER_LABELS, x, SLIDER_STATS);
        loop {
            let k = sx(r.0[2]);
            r.0[0] = k;
            if (k as i32) >= 5 {
                break;
            }
            let m = &mut c.m;
            let label = hs(m, SLIDER_LABELS + 2 * k);
            let yy = r.0[1];
            port(c, &r, 0x8002_1174, 0x8003_c9fc, &[label, x, sx(yy.wrapping_add(4)), 0, 14], crate::fetextrender::abi_menu_text_positioned);
            let m = &mut c.m;
            let off = m.r32(SLIDER_STATS + 4 * k);
            let bits = b(m, car + 196);
            let at = car.wrapping_add(off);
            let mut v = b(m, at + 48);
            for (bit, o) in [(1, 53), (2, 58), (4, 63)] {
                if bits & bit != 0 {
                    v = v.wrapping_add(b(m, at + o));
                }
            }
            r.call_ptr_leaving(c, DRAW_SLIDER, &[sx(v), 0, 11, x, sx(yy), 104, 3, 7, 3, 0, 0, 128, 0], 0x8003_cab0);
            r.0[1] = r.0[1].wrapping_add(15);
            r.0[2] = r.0[2].wrapping_add(1);
        }
    })
}

/// The overlay slots updated for the state (as `SetState`, an overlay fading out freed once
/// out).
fn update_slots(m: &mut ps1rt::Mem, this: u32) {
    for k in 0..4u32 {
        let slot = this + 4 * k;
        let st = hs(m, this + 284);
        let ov = m.r32(slot + 268);
        if ov != 0 {
            let want = m.r8(STATE_OVERLAYS.wrapping_add(k).wrapping_add(st << 2)) as i8 as i32 as u32;
            if hs(m, ov + 22) == want {
                continue;
            }
            m.w16(ov + 20, 0xFFFF);
            let ov = m.r32(slot + 268);
            if (hs(m, ov + 16) as i32) > 0 {
                continue;
            }
            m.w32(slot + 268, 0);
        }
        let st = hs(m, this + 284);
        let want = m.r8(STATE_OVERLAYS.wrapping_add(k).wrapping_add(st << 2)) as i8 as i32 as u32;
        if (want as i32) >= 0 {
            let o = (this + 100).wrapping_add(want.wrapping_mul(24));
            m.w32(slot + 268, o);
            m.w16(o + 16, 0);
            let o = m.r32(slot + 268);
            m.w16(o + 20, 1);
        }
    }
}

/// `tScreenCarSelect::DrawForeground` (SCREENCARSELECT.CPP, 0x8003cae4): the state's menu
/// items enabled for the car (buy, a tournament's car, the garage's sell and the pink slips'
/// colour), the overlays' arrow, the overlays updated and drawn, the wall updated for the car
/// (virtual) and its brightness; the car turning on its stand, or in the showroom flown round
/// by the camera's spline with the commentator's words (and its speech once) over it; the wall
/// lit a while after the car object's idle.
pub fn abi_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 336, |c, sp| {
        let mut r = Saved::of(c);
        r.0[3] = this;
        let m = &mut c.m;
        let app = m.r32(crate::globals::FE_APP);
        let stack = m.r32(app + 4);
        let n = m.r32(stack + 8);
        let menu = m.r32(stack.wrapping_add(n << 2) + 16);
        r.0[0] = menu;
        let ok = virtual_call(c, &r, this, 108, &[sp + 48], 0x8003_cb40);
        (r.0[4], r.0[1], r.0[2], r.0[5]) = (0x8005_0000, 0, menu, ok);
        let m = &mut c.m;
        let all = m.r32(ALL_MENUS);
        let v = m.r32(all + 8376) | 1;
        m.w32(all + 8376, v);
        let mut s1 = 0u32;
        match hs(m, this + 284) {
            1 => {
                s1 = (menu == all + 4840) as u32;
                r.0[1] = s1;
                r.0[0] = (-2i32) as u32;
                let v = m.r32(all + 4924) & !1;
                m.w32(all + 4924, v);
                if m.r8(FE + 4) == 2
                    && m.r8(FE + 290) == 0
                    && port(c, &r, 0x8002_373c, 0x8003_cbc0, &[13], crate::fecheats::abi_is_cheat_enabled) != 0
                {
                    let m = &mut c.m;
                    let all = m.r32(ALL_MENUS);
                    let v = m.r32(all + 8376) & !1;
                    m.w32(all + 8376, v);
                }
                let m = &mut c.m;
                if ok != 0 && b(m, sp + 50) < 5 {
                    m.w16(this + 216, 1);
                } else {
                    let all = m.r32(ALL_MENUS);
                    let v = m.r32(all + 4924) | 1;
                    m.w32(all + 4924, v);
                    m.w16(this + 216, 0xFFFF);
                }
            }
            0 => {
                if menu == all + 4564 {
                    s1 = 1;
                }
                let (a, b0) = (m.r32(all + 4604) & !1, m.r32(all + 4644) & !1);
                m.w32(all + 4604, a);
                m.w32(all + 4644, b0);
                if m.r8(sp + 50) == 7 {
                    m.w32(all + 4604, a | 1);
                    m.w32(all + 4644, b0 | 1);
                }
            }
            2 => {
                if menu == all + 7328 {
                    s1 = 1;
                }
            }
            3 => {
                if menu == all + 7648 {
                    s1 = 1;
                }
                let v = m.r32(all + 7688) & !1;
                m.w32(all + 7688, v);
                if ok != 0 {
                    m.w16(this + 216, 1);
                } else {
                    m.w16(this + 216, 0xFFFF);
                    m.w32(all + 7688, v | 1);
                }
            }
            4 => {
                s1 = 1;
                m.w16(this + 240, 1);
                m.w16(this + 192, 1);
            }
            _ => {}
        }
        if ok == 0 {
            s1 = 0;
        }
        r.0[1] = s1;
        let m = &mut c.m;
        m.w16(this + 264, if s1 != 0 { 1 } else { 0xFFFF });
        update_slots(m, this);
        // (the loop's registers left for the calls after: its index, table, and 1)
        (c.r[6], c.r[7], c.r[8]) = (3, STATE_OVERLAYS, 1);
        for k in 1..4 {
            r.0[0] = k;
            let o = c.m.r32(this + 268 + 4 * k);
            port(c, &r, 0x8003_afd8, 0x8003_ce1c, &[this, o], abi_draw_overlay);
        }
        r.0[0] = 4;
        if ok == 0 {
            c.m.w8(sp + 48, 0xFF);
        }
        virtual_call(c, &r, this, 100, &[sp + 48], 0x8003_ce60);
        let m = &mut c.m;
        let app = m.r32(crate::globals::FE_APP);
        let p = b(m, app + 556);
        let o = m.r32(CAR_OBJECTS + 4 * p);
        if m.r32(o + 2264) != 0 {
            port(c, &r, 0x8003_c7b0, 0x8003_cea0, &[this, 0, 0], abi_set_brightness);
            port(c, &r, 0x8003_7050, 0x8003_cea8, &[this + 772], crate::fevideowall::abi_turn_off);
            let t = c.m.r32(GAME_TICKS);
            c.m.w32(this + 892, t);
        }
        port(c, &r, 0x8003_c7c4, 0x8003_cec4, &[this, 0], abi_update_brightness);
        let m = &mut c.m;
        if (m.r16(this + 284) as u32).wrapping_sub(5) >= 2 {
            m.w32(SHOWROOM_DRAW, 0);
            let turn = m.r32(this + 924).wrapping_add(3);
            m.w32(this + 924, turn);
            let shade = b(m, this + 884);
            r.call_ptr_leaving(c, DRAW_CAR, &[sp + 48, 314, 84, 0x4080_0000, 0xC0F0_0000, shade, 1, turn, 0], 0x8003_cf28);
        } else {
            showroom(c, &mut r, this, sp, ok);
        }
        let m = &mut c.m;
        let o = m.r32(CAR_OBJECTS);
        if m.r32(o + 2264) == 0
            && m.r16(this + 884) == m.r16(this + 888)
            && m.r16(this + 884) == 0
            && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 892)) as i32) < 129)
        {
            let v = if m.r8(sp + 55) != 0 { 128 } else { 32 };
            port(c, &r, 0x8003_c7b0, 0x8003_d390, &[this, v, 0], abi_set_brightness);
            port(c, &r, 0x8003_70f0, 0x8003_d398, &[this + 772], crate::fevideowall::abi_turn_on);
        }
    })
}

/// The showroom's part of `DrawForeground`: the commentator's words a while in, the screen's
/// fade, the camera's spline's four points (each a step on, wrapping the seven with an offset
/// past them), the car flown round on it, the commentary's speech once.
fn showroom(c: &mut Cpu, r: &mut Saved, this: u32, sp: u32, ok: u32) {
    let m = &mut c.m;
    let t0 = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 912)).wrapping_sub(256);
    for at in [288, 292, 296, 280, 284] {
        m.w32(sp + at, 0);
    }
    let speech = m.r8(sp + 241) as i8 as i32;
    if (t0 as i32) >= 0 && speech >= 0 {
        let a1 = (t0 as i32 >> 9) as u32;
        let hi = ((a1 as i32 as i64 * 0x6BCA_1AF3) >> 32) as i32;
        let q = ((hi >> 3) - (t0 as i32 >> 31)) as u32;
        let word = a1.wrapping_sub(q.wrapping_mul(19)).wrapping_add(997).wrapping_add((speech as u32).wrapping_mul(19));
        r.0[0] = TEXT_COLORS;
        r.0[4] = word;
        let phase = t0.wrapping_sub(a1 << 9);
        r.0[1] = phase;
        let kind = r.call_leaving(c, WORD_FLAGS, &[sx(word)], 0x8003_cfcc);
        let m = &mut c.m;
        let t = b(m, WORD_KINDS.wrapping_add(kind.wrapping_mul(6)) + 4);
        let mut colour = m.r32(TEXT_COLORS.wrapping_add(t << 2));
        r.0[2] = colour;
        if phase < 128 {
            colour = port(c, r, 0x8004_bd68, 0x8003_d02c, &[colour, 128 - phase], crate::fefades::abi_fade_out);
        } else if 512u32.wrapping_sub(phase) < 128 {
            colour = port(c, r, 0x8004_bd68, 0x8003_d02c, &[colour, phase.wrapping_sub(384)], crate::fefades::abi_fade_out);
        }
        r.0[2] = colour;
        let text = r.call_leaving(c, TEXT_WORD, &[word], 0x8003_d038);
        r.0[0] = text;
        let x = sx(r.call_leaving(c, WORD_X, &[word], 0x8003_d044));
        r.0[1] = x;
        let y = sx(r.call_leaving(c, WORD_Y, &[word], 0x8003_d054));
        port(c, r, 0x8002_0eac, 0x8003_d074, &[text, x, y, colour, 0, 0], crate::fetextrender::abi_full_text_rgb);
    }
    let m = &mut c.m;
    m.w32(sp + 260, 0x55_1E00);
    let info = m.r32(this + 40);
    m.w32(sp + 276, info);
    let t = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 912)).wrapping_sub(128) as i32;
    let fade = if t <= 0 {
        128
    } else if t < 128 {
        (128 - t) as u32
    } else {
        0
    };
    if m.r16(this + 58) & 1 != 0 && (fade as i32) < 128 {
        port(c, r, 0x8004_eb60, 0x8003_d0f8, &[10, 512, 0, 0, fade, 0, sp + 256], crate::psxfront::abi_draw_shape_extended);
    }
    let m = &mut c.m;
    let mut t0 = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 908));
    while !((t0 as i32) < 601) {
        let v = m.r32(this + 908).wrapping_add(600);
        m.w32(this + 908, v);
        let k = m.r32(this + 920);
        m.w32(this + 920, k.wrapping_add(1));
        if !((k.wrapping_add(1) as i32) < 7) {
            m.w32(this + 920, k.wrapping_sub(4));
        }
        t0 = t0.wrapping_sub(600);
    }
    let mut points = [0u32; 4];
    let mut p = m.r16(this + 920) as u32;
    let mut off = 0u32;
    m.w32(CAMERA_OFFSETS, 0);
    for (k, point) in points.iter_mut().enumerate() {
        if k > 0 {
            let prev = p;
            p = prev.wrapping_add(1);
            off = m.r32(CAMERA_OFFSETS + 4 * (k as u32 - 1));
            m.w32(CAMERA_OFFSETS + 4 * k as u32, off);
            if !((sx(p) as i32) < 7) {
                p = prev.wrapping_sub(4);
                m.w32(CAMERA_OFFSETS + 4 * k as u32, off.wrapping_add(0x400_0000));
            }
        } else if !((sx(p) as i32) < 7) {
            p = p.wrapping_sub(5);
            m.w32(CAMERA_OFFSETS, 0x400_0000);
        }
        *point = p;
    }
    let _ = off;
    let args = [this, sx(points[0]), sx(points[1]), sx(points[2]), sx(points[3]), t0, sp + 280, sp + 284, sp + 288, sp + 292, sp + 296];
    port(c, r, 0x8003_baf8, 0x8003_d258, &args, abi_calc_spline_position);
    r.0[2] = 1;
    c.m.w32(SHOWROOM_DRAW, 1);
    let x = c.m.r32(sp + 280);
    let f = r.call_leaving(c, I2F, &[x], 0x8003_d26c);
    r.0[0] = 0x3780_0000;
    let fx = r.call_leaving(c, FMUL, &[f, 0x3780_0000], 0x8003_d27c);
    r.0[1] = fx;
    let y = c.m.r32(sp + 284);
    let f = r.call_leaving(c, I2F, &[y], 0x8003_d288);
    let fy = r.call_leaving(c, FMUL, &[f, 0x3780_0000], 0x8003_d294);
    let m = &mut c.m;
    let (a1, a2, shade, turn) = (hs(m, sp + 288), hs(m, sp + 292), b(m, this + 884), m.r32(sp + 296));
    r.call_ptr_leaving(c, DRAW_CAR, &[sp + 48, a1, a2, fx, fy, shade, 1, turn, 0], 0x8003_d2c4);
    if ok != 0 {
        let now = r.call_leaving(c, GETTICK, &[], 0x8003_d2d4);
        let m = &mut c.m;
        if now.wrapping_sub(m.r32(this + 912)) >= 641 && m.r32(this + 916) == 0 && (m.r8(sp + 241) as i8) >= 0 {
            m.w32(this + 916, 1);
            let speech = m.r8(sp + 241) as i8 as i32 as u32;
            port(c, r, 0x8001_5de0, 0x8003_d31c, &[0, speech], crate::feaudio::abi_async_play_speech);
        }
    }
}

/// `tScreenCarSelect::tScreenCarSelect` entered as the original's callers `jal` it.
pub(crate) fn screen_car_select_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_ae58, None, &[]);
    abi_screen_car_select(c);
}

/// `tScreenCarSelect::~tScreenCarSelect` entered as the original's callers `jal` it.
pub(crate) fn screen_car_select_destructor_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8003_af68, None, &[]);
    abi_screen_car_select_destructor(c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's car select screen (the RAM image's current one); a vector and a matrix, the
    /// spline's outputs, `GetShapeInfo`'s.
    const SCREEN: u32 = 0x8017_E040;
    const VEC: u32 = 0x8014_7000;
    const OUTS: u32 = 0x8014_7100;

    /// FEMENU's structures, then: the race kind, players, cars chosen (the player's, the
    /// second's, the garage's), the pink slips' and test drive's flags; the menus' items'
    /// flags; the screen's state, its overlays' fades, steps and directions, the slots (none or
    /// any overlay), the car shown, the wall's flags, brightness and clocks about their limits,
    /// the spline's point, the screen's fade; the car objects busy or idle; a vector and a
    /// matrix of any words. (The showroom's speech is said already: its file waits on the CD.)
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        m.w8(FE + 4, pick(rng, 4) as u8);
        m.w8(FE + 3, pick(rng, 3) as u8);
        let cars = m.r32(CAR_MANAGER);
        for at in [92, 291, 296, 297] {
            m.w8(FE + at, [0, 1, pick(rng, cars.max(1)), cars.wrapping_sub(1)][pick(rng, 4) as usize] as u8);
        }
        m.w8(FE + 298, pick(rng, 2) as u8);
        m.w8(FE + 290, pick(rng, 2) as u8);
        m.w8(FE + 798, pick(rng, 2) as u8);
        let all = m.r32(ALL_MENUS);
        for at in [4604, 4644, 4924, 7688, 8216, 8248, 8312, 8376] {
            let v = m.r32(all + at) & !1;
            m.w32(all + at, v | pick(rng, 2));
        }
        let s = SCREEN;
        m.w16(s + 284, pick(rng, 8) as u16);
        for k in 0..7 {
            let o = s + 100 + 24 * k;
            m.w16(o + 16, [0, 1, 6, 64, 122, 127, 128][pick(rng, 7) as usize]);
            m.w16(o + 18, [6, 0xFFFA, 0][pick(rng, 3) as usize]);
            m.w16(o + 20, [0, 1, 0xFFFF][pick(rng, 3) as usize]);
        }
        for k in 0..4 {
            let v = if pick(rng, 3) == 0 { 0 } else { s + 100 + 24 * pick(rng, 7) };
            m.w32(s + 268 + 4 * k, v);
        }
        for at in [286, 288, 290] {
            m.w16(s + at, [0xFFFF, 0, 1, 16, 19, pick(rng, 30)][pick(rng, 6) as usize] as u16);
        }
        let t = m.r32(GAME_TICKS);
        for at in [892, 896, 908, 912] {
            m.w32(s + at, t.wrapping_sub([0, 100, 128, 129, 256, 300, 600, 601, 700, 1300][pick(rng, 10) as usize]));
        }
        m.w32(s + 900, pick(rng, 2));
        m.w32(s + 904, pick(rng, 2));
        m.w32(s + 916, 1);
        m.w32(s + 920, pick(rng, 7));
        m.w32(s + 924, pick(rng, 4096));
        for at in [884, 886, 888, 890] {
            m.w16(s + at, [0, 8, 32, 120, 128, pick(rng, 140)][pick(rng, 6) as usize] as u16);
        }
        m.w16(s + 92, [0, 64, 127, 128, 200][pick(rng, 5) as usize]);
        m.w32(s + 84, pick(rng, 2));
        let f = m.r16(s + 58) & !1;
        m.w16(s + 58, f | pick(rng, 2) as u16);
        m.w16(s + 792, [0xFFFF, 0][pick(rng, 2) as usize]);
        for p in 0..2 {
            let o = m.r32(CAR_OBJECTS + 4 * p);
            m.w32(o + 2264, pick(rng, 2));
        }
        for k in 0..20 {
            m.w32(VEC + 4 * k, [0, 0x1_0000, 0xFFFF_0000, rng.next() as u32 & 0x3_FFFF][pick(rng, 4) as usize]);
        }
    }

    /// With no car to show (`GetCar` failing: an empty garage), `DrawOverlay`'s marks (overlay
    /// kinds 4 and 5) test bits of the car it didn't get: the original reads its stack's leftover
    /// there (the background strips' last `y`, homed by `DrawShapeExtended`), which the readable
    /// FESCREEN no longer leaves. Such overlays are made a kind drawing nothing without a car.
    fn marks_need_a_car(c: &mut Cpu) {
        if crate::abi::call(c, nfs4rc::f_8003c530, &[SCREEN, OUTS + 0x40]) != 0 {
            return;
        }
        for k in 0..7 {
            let kind = SCREEN + 100 + 24 * k + 22;
            if matches!(c.m.r16(kind), 4 | 5) {
                c.m.w16(kind, 0);
            }
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("TransformVector", 0x8003_ab80, nfs4rc::f_8003ab80, |_, _| vec![VEC, VEC + 16, OUTS]),
            ("DrawCar", 0x8003_ac64, nfs4rc::f_8003ac64, |c, r| {
                let car = OUTS + 0x40;
                c.m.w8(car, [0xFF, 0, 1, 5][pick(r, 4) as usize]);
                vec![car, pick(r, 320), pick(r, 240), 0x4080_0000, 0xC0F0_0000, pick(r, 256), 1, pick(r, 4096), pick(r, 2)]
            }),
            ("tScreenCarSelect", 0x8003_ae58, nfs4rc::f_8003ae58, |_, _| vec![OUTS + 0x100]),
            ("~tScreenCarSelect", 0x8003_af68, nfs4rc::f_8003af68, |_, r| vec![SCREEN, 2 * pick(r, 2)]),
            ("tScreenCarSelect::DrawOverlay", 0x8003_afd8, nfs4rc::f_8003afd8, |c, r| {
                marks_need_a_car(c);
                vec![SCREEN, [0, SCREEN + 100 + 24 * pick(r, 7)][(pick(r, 8) != 0) as usize]]
            }),
            ("tScreenCarSelect::SetState", 0x8003_b874, nfs4rc::f_8003b874, |_, r| vec![SCREEN, [0, 1, 2, 3, 4, 7][pick(r, 6) as usize]]),
            ("tScreenCarSelect::CalcSplinePosition", 0x8003_baf8, nfs4rc::f_8003baf8, |_, r| {
                let mut a = vec![SCREEN, pick(r, 7), pick(r, 7), pick(r, 7), pick(r, 7), pick(r, 601)];
                a.extend((0..5).map(|k| OUTS + 4 * k));
                a
            }),
            ("tScreenCarSelect::GetShapeInfo", 0x8003_bdb8, nfs4rc::f_8003bdb8, |_, _| vec![SCREEN, OUTS, OUTS + 4, OUTS + 8, OUTS + 12]),
            ("tScreenCarSelect::UpdateVideoWall", 0x8003_be7c, nfs4rc::f_8003be7c, |c, r| {
                let car = OUTS + 0x40;
                for (at, v) in [(0, [0xFF, 0, 16, 19][pick(r, 4) as usize]), (2, [0, 7][pick(r, 2) as usize]), (199, pick(r, 3)), (200, [16, 19, 0][pick(r, 3) as usize])] {
                    c.m.w8(car + at, v as u8);
                }
                vec![SCREEN, car]
            }),
            ("tScreenCarSelect::AllocateAsyncBuffer", 0x8003_bf4c, nfs4rc::f_8003bf4c, |_, _| vec![SCREEN]),
            ("tScreenCarSelect::FreeAsyncBuffer", 0x8003_bf80, nfs4rc::f_8003bf80, |_, _| vec![SCREEN]),
            ("tScreenCarSelect::InitializeVideoWall", 0x8003_bfa8, nfs4rc::f_8003bfa8, |_, _| vec![SCREEN]),
            ("tScreenCarSelect::Initialize", 0x8003_c060, nfs4rc::f_8003c060, |_, _| vec![SCREEN]),
            ("tScreenCarSelect::ProcessInput", 0x8003_c268, nfs4rc::f_8003c268, |c, r| {
                c.m.w32(OUTS, [8, 16, 1, 4][pick(r, 4) as usize]);
                vec![SCREEN, 0, OUTS, OUTS + 4]
            }),
            ("tScreenCarSelect::DrawVideoWall", 0x8003_c424, nfs4rc::f_8003c424, |c, _| {
                marks_need_a_car(c);
                vec![SCREEN, 0]
            }),
            ("tScreenCarSelect::GetCar", 0x8003_c530, nfs4rc::f_8003c530, |_, _| vec![SCREEN, OUTS + 0x40]),
            ("tScreenCarSelect::SetBrightness", 0x8003_c7b0, nfs4rc::f_8003c7b0, |_, r| vec![SCREEN, pick(r, 200), pick(r, 2)]),
            ("tScreenCarSelect::UpdateBrightness", 0x8003_c7c4, nfs4rc::f_8003c7c4, |_, r| vec![SCREEN, pick(r, 2)]),
            ("tScreenCarSelect::DrawBackground", 0x8003_c848, nfs4rc::f_8003c848, |c, _| {
                marks_need_a_car(c);
                vec![SCREEN]
            }),
            ("tScreenCarSelect::DrawSliders", 0x8003_c978, nfs4rc::f_8003c978, |c, r| {
                let car = OUTS + 0x40;
                for k in 0..200 {
                    c.m.w8(car + k, pick(r, 40) as u8);
                }
                c.m.w8(car + 196, pick(r, 8) as u8);
                vec![SCREEN, car, pick(r, 300), pick(r, 200)]
            }),
            ("tScreenCarSelect::DrawForeground", 0x8003_cae4, nfs4rc::f_8003cae4, |_, _| vec![SCREEN]),
        ]
    }

    #[test]
    fn screencarselect_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xD0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 200, 0xD0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}

/// `"OpponentVid"`, `"z%s"`, `"zDuel"`; the duel's walls' TV layout; the opponent's car.
const OPPONENT_NAME: u32 = 0x8001_1AC0;
const Z_FORMAT: u32 = 0x8001_1ACC;
const DUEL_FILE: u32 = 0x8001_1AD0;
const DUEL_TVS: u32 = 0x8005_2428;
const OPPONENT_CAR: u32 = 0x8011_5BD5;

/// The car's video's file read if the shown car changed (`UpdateVideoWall`'s, the file's name
/// at `name` in the car's info), the wall dark and off.
fn update_wall(c: &mut Cpu, this: u32, car: u32, name: u32, ras: [u32; 3]) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (this, car);
        let m = &mut c.m;
        let changed = b(m, car + 200) != hs(m, this + 286)
            || m.r8(car) as i8 as i32 as u32 != hs(m, this + 288)
            || (m.r8(car + 2) == 7 && hs(m, this + 290) != b(m, car + 199));
        if !changed {
            return;
        }
        if (m.r8(car) as i8) >= 0 {
            port(c, &r, 0x8002_5dd8, ras[0], &[this, car + name], crate::fescreen::abi_async_load_swap_shape_file);
        }
        let m = &mut c.m;
        let v = b(m, car + 200);
        m.w16(this + 286, v as u16);
        let v = m.r8(car) as i8 as i16;
        m.w16(this + 288, v as u16);
        let v = b(m, car + 199);
        m.w32(this + 900, 0);
        m.w16(this + 290, v as u16);
        port(c, &r, 0x8003_c7b0, ras[1], &[this, 0, 0], abi_set_brightness);
        port(c, &r, 0x8003_7050, ras[2], &[this + 772], crate::fevideowall::abi_turn_off);
    })
}

/// `tScreenCarSelectDuel::PreLoad` (SCREENCARSELECT.CPP, 0x8003d3bc): the opponent's shapes'
/// information, the screen's preload, two videos' buffers (16000 bytes each), the player's car's
/// (or the first stock car's) shapes and the opponent's read.
pub fn abi_duel_pre_load(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 280, |c, sp| {
        let mut r = Saved::of(c);
        let theirs = this + 936;
        (r.0[3], r.0[4]) = (this, theirs);
        c.m.w32(this + 936, 0);
        port(c, &r, 0x8002_640c, 0x8003_d3f0, &[this, theirs, 5], crate::fescreen::abi_initialize_shapes);
        port(c, &r, 0x8002_67a0, 0x8003_d3f8, &[this], crate::fescreen::abi_pre_load);
        let v = r.call_leaving(c, GET_DCT_BUFFER, &[16000, WALL_NAME], 0x8003_d408);
        c.m.w32(this + 48, v);
        let v = r.call_leaving(c, GET_DCT_BUFFER, &[16000, OPPONENT_NAME], 0x8003_d41c);
        c.m.w32(this + 944, v);
        let car = sp + 16;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_d43c);
        if ok ^ 1 != 0 {
            port(c, &r, 0x8001_6fd8, 0x8003_d458, &[CAR_MANAGER, 0, car], crate::fecars::abi_get_stock_car);
        }
        let name = sp + 224;
        (r.0[1], r.0[0], r.0[2]) = (name, Z_FORMAT, sp + 32);
        r.call_leaving(c, SPRINTF, &[name, Z_FORMAT, sp + 32], 0x8003_d478);
        port(c, &r, 0x8002_6260, 0x8003_d488, &[this, name, this + 40], crate::fescreen::abi_async_load_shape_file);
        let id = b(&mut c.m, OPPONENT_CAR);
        port(c, &r, 0x8001_6fd8, 0x8003_d4a0, &[CAR_MANAGER, id, car], crate::fecars::abi_get_stock_car);
        r.call_leaving(c, SPRINTF, &[name, Z_FORMAT, sp + 32], 0x8003_d4b0);
        port(c, &r, 0x8002_6260, 0x8003_d4c0, &[this, name, theirs], crate::fescreen::abi_async_load_shape_file);
        c.m.w32(this + 932, 0);
    })
}

/// `tScreenCarSelectDuel::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003d4e4): nothing (the
/// preload has them).
pub fn abi_duel_allocate_async_buffer(_c: &mut Cpu) {}

/// `tScreenCarSelectDuel::FreeAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003d4ec): both videos'.
pub fn abi_duel_free_async_buffer(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_leaving(c, RESET_DCT_BUFFER, &[], 0x8003_d500);
        c.m.w32(this + 48, 0);
        c.m.w32(this + 944, 0);
    })
}

/// `tScreenCarSelectDuel::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003d518): two walls
/// of five TVs, the player's and the opponent's (moved down), their images once loaded.
pub fn abi_duel_initialize_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let (mine, theirs) = (this + 772, this + 828);
        (r.0[2], r.0[1], r.0[4], r.0[3]) = (this, mine, 5, DUEL_TVS);
        let shapes = c.m.r32(this + 40);
        port(c, &r, 0x8003_6988, 0x8003_d568, &[mine, this + 292, shapes, 0, 5, DUEL_TVS, 0], crate::fevideowall::abi_initialize);
        port(c, &r, 0x8003_6b84, 0x8003_d57c, &[mine, 249, 270, 45], crate::fevideowall::abi_set_available_text);
        if c.m.r16(this + 58) & 1 != 0 {
            port(c, &r, 0x8003_6bc0, 0x8003_d59c, &[mine, 6, 0], crate::fevideowall::abi_set_offset);
            port(c, &r, 0x8003_6a38, 0x8003_d5a4, &[mine], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
        r.0[0] = theirs;
        let shapes = c.m.r32(this + 936);
        port(c, &r, 0x8003_6988, 0x8003_d5d0, &[theirs, this + 532, shapes, 0, 5, DUEL_TVS, 0], crate::fevideowall::abi_initialize);
        port(c, &r, 0x8003_6b84, 0x8003_d5e4, &[mine, 249, 270, 150], crate::fevideowall::abi_set_available_text);
        if c.m.r16(this + 954) & 1 != 0 {
            port(c, &r, 0x8003_6bc0, 0x8003_d604, &[theirs, 6, 105], crate::fevideowall::abi_set_offset);
            port(c, &r, 0x8003_6a38, 0x8003_d60c, &[theirs], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
    })
}

/// `tScreenCarSelectDuel::Initialize` (SCREENCARSELECT.CPP, 0x8003d634): the car select's, in
/// state 0, not the car select's flag.
pub fn abi_duel_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8003_c060, 0x8003_d648, &[this], abi_initialize);
        c.m.w16(this + 284, 0);
        c.m.w32(IN_CAR_SELECT, 0);
    })
}

/// `tScreenCarSelectDuel::Cleanup` (SCREENCARSELECT.CPP, 0x8003d664): the opponent's shapes
/// freed, the car select's.
pub fn abi_duel_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8002_64b4, 0x8003_d67c, &[this, this + 936], crate::fescreen::abi_free_shapes);
        port(c, &r, 0x8003_af90, 0x8003_d684, &[this], abi_cleanup);
    })
}

/// A duel's wall's background shapes (12) faded with the screen and raised by `dy`.
fn wall_shapes(c: &mut Cpu, r: &mut Saved, k_reg: usize, this: u32, dy: u32, ra: u32) {
    for k in 0..12u32 {
        r.0[k_reg] = k;
        let fade = hs(&mut c.m, this + 92);
        port(c, r, 0x8004_eb60, ra, &[k, 0, 0, 0u32.wrapping_sub(dy), fade, 0, 0], crate::psxfront::abi_draw_shape_extended);
    }
    r.0[k_reg] = 12;
}

/// `tScreenCarSelectDuel::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003d694): the wall's
/// background raised by `dy`, the wall off while the screen fades, its images once loaded
/// (offset), updated, valid with a car, drawn.
pub fn abi_duel_draw_video_wall(c: &mut Cpu) {
    let (this, dy) = (c.r[4], c.r[5]);
    abi::frame(c, 264, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[0], r.0[1]) = (this, dy, 0);
        let ok = virtual_call(c, &r, this, 108, &[sp + 32], 0x8003_d6d4);
        r.0[3] = ok;
        let dy = sx(dy);
        r.0[0] = dy;
        wall_shapes(c, &mut r, 1, this, dy, 0x8003_d708);
        let mine = this + 772;
        if (hs(&mut c.m, this + 92) as i32) > 0 && c.m.r32(this + 84) != 0 {
            port(c, &r, 0x8003_7074, 0x8003_d74c, &[mine], crate::fevideowall::abi_turn_off_instant);
            port(c, &r, 0x8003_c7b0, 0x8003_d75c, &[this, 0, 0], abi_set_brightness);
        }
        r.0[0] = mine;
        if c.m.r16(this + 58) & 1 != 0 && c.m.r32(this + 900) == 0 {
            port(c, &r, 0x8003_6bc0, 0x8003_d790, &[mine, 6, 0], crate::fevideowall::abi_set_offset);
            port(c, &r, 0x8003_6a38, 0x8003_d798, &[mine], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
        port(c, &r, 0x8003_6bdc, 0x8003_d7ac, &[mine], crate::fevideowall::abi_update);
        port(c, &r, 0x8003_6bd4, 0x8003_d7bc, &[mine, sx(ok)], crate::fevideowall::abi_set_valid);
        let avail = b(&mut c.m, sp + 39);
        port(c, &r, 0x8003_6bcc, 0x8003_d7c8, &[mine, avail], crate::fevideowall::abi_set_available);
        port(c, &r, 0x8003_6dac, 0x8003_d7d0, &[mine], crate::fevideowall::abi_draw);
    })
}

/// `tScreenCarSelectDuel::DrawOpponentVideoWall` (SCREENCARSELECT.CPP, 0x8003d7ec): the
/// opponent's wall as `DrawVideoWall` the player's (always valid and available).
pub fn abi_duel_draw_opponent_video_wall(c: &mut Cpu) {
    let (this, dy) = (c.r[4], c.r[5]);
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let dy = sx(dy);
        (r.0[1], r.0[0], r.0[2]) = (this, 0, dy);
        wall_shapes(c, &mut r, 0, this, dy, 0x8003_d838);
        let theirs = this + 828;
        if (hs(&mut c.m, this + 92) as i32) > 0 && c.m.r32(this + 84) != 0 {
            port(c, &r, 0x8003_7074, 0x8003_d87c, &[theirs], crate::fevideowall::abi_turn_off_instant);
            port(c, &r, 0x8003_c7b0, 0x8003_d88c, &[this, 0, 1], abi_set_brightness);
        }
        r.0[0] = theirs;
        if c.m.r16(this + 58) & 1 != 0 && c.m.r32(this + 932) == 0 {
            port(c, &r, 0x8003_6bc0, 0x8003_d8bc, &[theirs, 6, 105], crate::fevideowall::abi_set_offset);
            port(c, &r, 0x8003_6a38, 0x8003_d8c4, &[theirs], crate::fevideowall::abi_update_images);
            c.m.w32(this + 932, 1);
        }
        port(c, &r, 0x8003_6bdc, 0x8003_d8d8, &[theirs], crate::fevideowall::abi_update);
        port(c, &r, 0x8003_6bd4, 0x8003_d8e4, &[theirs, 1], crate::fevideowall::abi_set_valid);
        port(c, &r, 0x8003_6bcc, 0x8003_d8f0, &[theirs, 1], crate::fevideowall::abi_set_available);
        port(c, &r, 0x8003_6dac, 0x8003_d8f8, &[theirs], crate::fevideowall::abi_draw);
    })
}

/// `tScreenCarSelectDuel::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003d910): 52 shapes of
/// `zDuel` and 5 swapped (no file), the car shown noted, no opponent's.
pub fn abi_duel_get_shape_info(c: &mut Cpu) {
    let (this, count, swap, file) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        c.m.w16(count, 52);
        c.m.w16(swap, 5);
        r.0[1] = file;
        let car = sp + 16;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_d954);
        if ok ^ 1 != 0 {
            port(c, &r, 0x8001_6fd8, 0x8003_d970, &[CAR_MANAGER, 0, car], crate::fecars::abi_get_stock_car);
        }
        let m = &mut c.m;
        let v = b(m, sp + 216);
        m.w16(this + 286, v as u16);
        let v = m.r8(car) as i8 as i16;
        m.w16(this + 288, v as u16);
        let v = b(m, sp + 215);
        m.w16(this + 928, 0xFFFF);
        m.w16(this + 290, v as u16);
        m.w32(file, DUEL_FILE);
        m.w32(swap_file, 0);
    })
}

/// `tScreenCarSelectDuel::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003d9cc): as the car
/// select's, the video's file the car's other name.
pub fn abi_duel_update_video_wall(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    update_wall(c, this, car, 16, [0x8003_da4c, 0x8003_da80, 0x8003_da88]);
}

/// `tScreenCarSelectDuel::UpdateOpponentVideoWall` (SCREENCARSELECT.CPP, 0x8003da9c): another
/// opponent's car: its shapes read, its wall dark and off.
pub fn abi_duel_update_opponent_video_wall(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this, car);
        if b(&mut c.m, car + 200) == hs(&mut c.m, this + 928) {
            return;
        }
        r.call_leaving(c, SPRINTF, &[sp + 16, Z_FORMAT, car + 16], 0x8003_dad8);
        port(c, &r, 0x8002_6260, 0x8003_dae8, &[this, sp + 16, this + 936], crate::fescreen::abi_async_load_shape_file);
        let m = &mut c.m;
        m.w32(this + 932, 0);
        let v = b(m, car + 200);
        m.w16(this + 928, v as u16);
        port(c, &r, 0x8003_c7b0, 0x8003_db04, &[this, 0, 1], abi_set_brightness);
        port(c, &r, 0x8003_7050, 0x8003_db0c, &[this + 828], crate::fevideowall::abi_turn_off);
    })
}

/// The drawing environments' buffers, the screen's height; the second car object; the
/// duel's slider labels.
const DRAW_ENV_A: u32 = 0x8013_E6B0;
const DRAW_ENV_B: u32 = 0x8013_EA9C;
const SCREEN_HEIGHT: u32 = 0x8013_EF6C;
const DUEL_LABELS: u32 = 0x8005_2408;
const GET_DRAWENV: fn(&mut Cpu) = nfs4rc::f_800beb84;
/// Hooked (the host records the clipping): called by address.
const SET_DRAW_AREA: u32 = 0x800E_EC70;

/// A drawing area primitive (12 bytes) linked into the OT and set to the screen's band from
/// `env`'s top plus `dy` (the screen's height less `dy` high), as the original's inline
/// linking leaves the registers.
fn draw_area(c: &mut Cpu, r: &Saved, env: u32, sp: u32, dy: u32, ra: u32) {
    let m = &mut c.m;
    m.w16(sp + 256, 0);
    let y = hu(m, env + 2).wrapping_add(dy);
    m.w16(sp + 260, 512);
    let h = hu(m, SCREEN_HEIGHT).wrapping_sub(dy);
    m.w16(sp + 258, y as u16);
    m.w16(sp + 262, h as u16);
    let ot = m.r32(0x1F80_0000);
    let prim = crate::mmeffect::add_prim(m, 12);
    (c.r[6], c.r[7], c.r[8], c.r[9]) = (ot, 0xFF_FFFF, 0xFF00_0000, 0x1F80_0004);
    r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, sp + 256], ra);
}

/// Whether a wall's video shapes can go up: read (`data` at `info + 4`), a shown car's
/// (`picture`), the car object (`object`) idle, a while after the wall's last change (`since`).
fn wall_ready(m: &mut ps1rt::Mem, this: u32, info: u32, picture: u32, object: u32, since: u32) -> bool {
    m.r32(this + info + 4) != 0
        && hs(m, this + picture) != u32::MAX
        && {
            let o = m.r32(object);
            m.r32(o + 2264) == 0
        }
        && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + since)) as i32) < 129)
}

/// `tScreenCarSelectDuel::DrawBackground` (SCREENCARSELECT.CPP, 0x8003db20): the duel's two
/// halves: the opponent's (its name's plate, its car got and its wall updated, its shapes up
/// once read, its brightness, its car turning, its wall), the player's below (its band's
/// background, its car's name's plate, the wall updated (virtual), brightness, car, shapes);
/// the menu's background (virtual).
pub fn abi_duel_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 280, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let (ea, eb) = (c.m.r32(DRAW_ENV_A), c.m.r32(DRAW_ENV_B));
        let env = r.call_leaving(c, GET_DRAWENV, &[ea, eb], 0x8003_db4c);
        r.0[2] = env;
        c.m.w16(this + 284, 7);
        draw_area(c, &r, env, sp, 0, 0x8003_dbd4);
        let m = &mut c.m;
        for (at, v) in [(40, 290), (44, 200), (46, 12), (42, 130)] {
            m.w16(sp + at, v);
        }
        let all = m.r32(ALL_MENUS);
        let t = port(c, &r, 0x8001_7ac4, 0x8003_dc08, &[all + 5216, u32::MAX], crate::fecars::abi_list_iterator_car_text_value);
        port(c, &r, 0x8004_c490, 0x8003_dc1c, &[sx(t), sp + 40, 0], crate::drawshp::abi_round_rectangle);
        let car = sp + 48;
        r.0[0] = car;
        let id = b(&mut c.m, OPPONENT_CAR);
        port(c, &r, 0x8001_6fd8, 0x8003_dc38, &[CAR_MANAGER, id, car], crate::fecars::abi_get_stock_car);
        let m = &mut c.m;
        let k = b(m, sp + 52);
        let v = m.r8(car + k + 175);
        m.w8(sp + 245, v);
        port(c, &r, 0x8003_da9c, 0x8003_dc54, &[this, car], abi_duel_update_opponent_video_wall);
        port(c, &r, 0x8002_5e2c, 0x8003_dc60, &[this, this + 936], crate::fescreen::abi_is_shape_file_loaded);
        let theirs = this + 828;
        if wall_ready(&mut c.m, this, 936, 848, CAR_OBJECTS + 4, 896) {
            port(c, &r, 0x8002_6614, 0x8003_dcd4, &[this, this + 936, 0, 65, 5, 0], crate::fescreen::abi_upload_shapes);
            c.m.w32(this + 932, 0);
            port(c, &r, 0x8003_70f0, 0x8003_dce0, &[theirs], crate::fevideowall::abi_turn_on);
            if c.m.r16(this + 886) == c.m.r16(this + 890) {
                port(c, &r, 0x8003_c7b0, 0x8003_dd00, &[this, 128, 1], abi_set_brightness);
            }
        }
        let m = &mut c.m;
        let turn = m.r32(this + 924).wrapping_add(3);
        m.w32(this + 924, turn);
        m.w8(sp + 244, 0);
        r.0[0] = 0x8014_0000;
        let o = m.r32(CAR_OBJECTS + 4);
        if m.r32(o + 2264) != 0 && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 896)) as i32) < 129) {
            port(c, &r, 0x8003_c7b0, 0x8003_dd54, &[this, 0, 1], abi_set_brightness);
            port(c, &r, 0x8003_7050, 0x8003_dd5c, &[theirs], crate::fevideowall::abi_turn_off);
            let t = c.m.r32(GAME_TICKS);
            c.m.w32(this + 896, t);
        }
        port(c, &r, 0x8003_c7c4, 0x8003_dd74, &[this, 1], abi_update_brightness);
        let m = &mut c.m;
        m.w32(SHOWROOM_DRAW, 0);
        let (shade, turn) = (b(m, this + 886), m.r32(this + 924));
        r.call_ptr_leaving(c, DRAW_CAR, &[car, 278, 184, 0x3FD9_999A, 0xC11E_6666, shade, 0, turn, 1], 0x8003_ddbc);
        let m = &mut c.m;
        let o = m.r32(CAR_OBJECTS + 4);
        if m.r32(o + 2264) == 0
            && m.r16(this + 886) == m.r16(this + 890)
            && m.r16(this + 886) == 0
            && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 896)) as i32) < 129)
        {
            port(c, &r, 0x8003_c7b0, 0x8003_de1c, &[this, 128, 1], abi_set_brightness);
            port(c, &r, 0x8003_70f0, 0x8003_de24, &[theirs], crate::fevideowall::abi_turn_on);
        }
        virtual_call(c, &r, this, 132, &[105], 0x8003_de40);
        r.0[0] = 0x8014_0000;
        draw_area(c, &r, env, sp, 128, 0x8003_dec4);
        let h = c.m.r32(SCREEN_HEIGHT);
        let half = ((h as i32) / 2) as u32;
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, 0, half, 512, half], 0x8003_dee8);
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_df04);
        r.0[2] = ok;
        if ok != 0 {
            let m = &mut c.m;
            m.w16(sp + 42, 25);
            let all = m.r32(ALL_MENUS);
            let list = if m.r8(FE + 298) == 0 { all + 4460 } else { all + 4812 };
            let t = port(c, &r, 0x8001_7ac4, 0x8003_df4c, &[list, u32::MAX], crate::fecars::abi_list_iterator_car_text_value);
            port(c, &r, 0x8004_c490, 0x8003_df60, &[sx(t), sp + 40, 0], crate::drawshp::abi_round_rectangle);
        } else {
            c.m.w8(car, 0xFF);
        }
        virtual_call(c, &r, this, 100, &[car], 0x8003_df8c);
        let mine = this + 772;
        let m = &mut c.m;
        r.0[0] = 0x8014_0000;
        let o = m.r32(CAR_OBJECTS);
        if m.r32(o + 2264) != 0 && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 892)) as i32) < 129) {
            port(c, &r, 0x8003_c7b0, 0x8003_dfd0, &[this, 0, 0], abi_set_brightness);
            port(c, &r, 0x8003_7050, 0x8003_dfd8, &[mine], crate::fevideowall::abi_turn_off);
            let t = c.m.r32(GAME_TICKS);
            c.m.w32(this + 892, t);
        }
        port(c, &r, 0x8003_c7c4, 0x8003_dff0, &[this, 0], abi_update_brightness);
        let m = &mut c.m;
        m.w32(SHOWROOM_DRAW, 0);
        let (shade, turn) = (b(m, this + 884), m.r32(this + 924));
        r.call_ptr_leaving(c, DRAW_CAR, &[car, 278, 79, 0x3FD9_999A, 0xC11E_6666, shade, 0, turn, 0], 0x8003_e034);
        let m = &mut c.m;
        let o = m.r32(CAR_OBJECTS);
        if m.r32(o + 2264) == 0
            && m.r16(this + 884) == m.r16(this + 888)
            && m.r16(this + 884) == 0
            && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 892)) as i32) < 129)
        {
            let v = if m.r8(sp + 55) != 0 { 128 } else { 32 };
            port(c, &r, 0x8003_c7b0, 0x8003_e0a8, &[this, v, 0], abi_set_brightness);
            port(c, &r, 0x8003_70f0, 0x8003_e0b0, &[mine], crate::fevideowall::abi_turn_on);
        }
        if ok != 0 {
            port(c, &r, 0x8002_5e2c, 0x8003_e0c4, &[this, this + 40], crate::fescreen::abi_is_shape_file_loaded);
            if wall_ready(&mut c.m, this, 40, 792, CAR_OBJECTS, 892) {
                port(c, &r, 0x8002_5ff0, 0x8003_e128, &[this, 5], crate::fescreen::abi_upload_swap_shapes);
                port(c, &r, 0x8003_70f0, 0x8003_e130, &[mine], crate::fevideowall::abi_turn_on);
                let m = &mut c.m;
                if m.r16(this + 888) == m.r16(this + 884) {
                    let v = if m.r8(sp + 55) != 0 { 128 } else { 32 };
                    port(c, &r, 0x8003_c7b0, 0x8003_e164, &[this, v, 0], abi_set_brightness);
                }
            }
        }
        virtual_call(c, &r, this, 84, &[0], 0x8003_e180);
    })
}

/// A car's stat with its upgrades (the car's three upgrade bits), as the sliders show it.
fn stat(m: &mut ps1rt::Mem, car: u32, k: u32) -> u32 {
    let off = m.r32(SLIDER_STATS + 4 * k);
    let bits = b(m, car + 196);
    let at = car.wrapping_add(off);
    let mut v = b(m, at + 48);
    for (bit, o) in [(1, 53), (2, 58), (4, 63)] {
        if bits & bit != 0 {
            v = v.wrapping_add(b(m, at + o));
        }
    }
    v
}

/// `tScreenCarSelectDuel::DrawForeground` (SCREENCARSELECT.CPP, 0x8003e198): the player's
/// car's five stats as sliders (empty without a car), then the opponent's below.
pub fn abi_duel_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 304, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[3], r.0[5]) = (45, 0, 4);
        let car = sp + 56;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_e1e8);
        (r.0[4], r.0[7]) = (ok, DUEL_LABELS);
        for pass in 0..2u32 {
            (r.0[1], r.0[6]) = (0, SLIDER_STATS);
            for k in 0..5u32 {
                r.0[0] = k;
                let m = &mut c.m;
                let label = hs(m, DUEL_LABELS + 2 * k);
                let y = sx(r.0[2].wrapping_add(4));
                port(c, &r, 0x8002_12e0, 0x8003_e24c, &[label, 500, y, 1, 0, 4], crate::fetextrender::abi_menu_text_positioned_justify);
                let v = if ok != 0 { sx(stat(&mut c.m, car, k)) } else { 0 };
                let y = sx(r.0[2]);
                r.call_ptr_leaving(c, DRAW_SLIDER, &[v, 0, 11, 417, y, 73, 3, 4, 3, 1, 0, 128, 0], 0x8003_e310);
                r.0[2] = r.0[2].wrapping_add(15);
                r.0[1] = k + 1;
            }
            r.0[0] = 5;
            r.0[2] = 150;
            r.0[3] = pass + 1;
            let id = b(&mut c.m, OPPONENT_CAR);
            port(c, &r, 0x8001_6fd8, 0x8003_e33c, &[CAR_MANAGER, id, car], crate::fecars::abi_get_stock_car);
        }
    })
}

/// `"zcarsb"`.
const TWO_PLAYER_FILE: u32 = 0x8001_1AD8;

fn player(m: &mut ps1rt::Mem) -> u32 {
    let app = m.r32(crate::globals::FE_APP);
    b(m, app + 556)
}

/// `tScreenCarSelectTwoPlayer::GetCar` (SCREENCARSELECT.CPP, 0x8003e370): the player's car:
/// a stock car (its colour), or a garage car (the first player's garage while the second has
/// none; none at all: no car); its colour index and livery.
pub fn abi_two_player_get_car(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    let _ = this;
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let p = player(&mut c.m);
        (r.0[2], r.0[0], r.0[3], r.0[5], r.0[4]) = (car, p, p, p, FE);
        let at;
        if c.m.r8(FE + 298) == 0 {
            let id = b(&mut c.m, FE + p + 92);
            port(c, &r, 0x8001_6fd8, 0x8003_e3d0, &[CAR_MANAGER, id, car], crate::fecars::abi_get_stock_car);
            let m = &mut c.m;
            let id = m.r8(car) as i8 as i32 as u32;
            let v = b(m, id.wrapping_add(p * 48).wrapping_add(FE) + 96);
            at = car + v;
        } else {
            r.0[1] = CAR_MANAGER;
            let n = port(c, &r, 0x8001_747c, 0x8003_e408, &[CAR_MANAGER, p], crate::fecars::abi_get_num_owned_cars);
            if (sx(n) as i32) <= 0 {
                r.0[3] = 0;
            }
            let who = r.0[3];
            r.0[0] = who;
            let n = port(c, &r, 0x8001_747c, 0x8003_e424, &[CAR_MANAGER, who], crate::fecars::abi_get_num_owned_cars);
            if (sx(n) as i32) <= 0 {
                c.r[2] = 0;
                return;
            }
            let id = b(&mut c.m, FE + p + 291);
            port(c, &r, 0x8001_7094, 0x8003_e44c, &[CAR_MANAGER, id, car, who], crate::fecars::abi_get_garage_car);
            at = car + b(&mut c.m, car + 197);
        }
        let m = &mut c.m;
        let v = m.r8(at + 175);
        m.w8(car + 197, v);
        let p = player(m);
        let id = m.r8(car) as i8 as i32 as u32;
        let v = m.r8(id.wrapping_add(p * 48).wrapping_add(FE) + 192);
        m.w8(car + 199, v);
        c.r[2] = 1;
    })
}

/// `tScreenCarSelectTwoPlayer::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003e4c0): the wall's
/// background raised by `dy`, the player's wall's images once loaded (the second player's
/// moved down, its text there), off while the screen fades, valid with a car, drawn.
pub fn abi_two_player_draw_video_wall(c: &mut Cpu) {
    let (this, dy) = (c.r[4], c.r[5]);
    abi::frame(c, 264, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[2], r.0[0], r.0[1]) = (this, dy, 0);
        let ok = virtual_call(c, &r, this, 108, &[sp + 32], 0x8003_e500);
        r.0[3] = ok;
        let dy = sx(dy);
        r.0[0] = dy;
        wall_shapes(c, &mut r, 1, this, dy, 0x8003_e534);
        let wall = this + 772;
        if c.m.r16(this + 58) & 1 != 0 {
            r.0[1] = 0x8005_0000;
        }
        if c.m.r16(this + 58) & 1 != 0 && c.m.r32(this + 900) == 0 {
            let off = if player(&mut c.m) != 0 { 105 } else { 0 };
            r.0[0] = wall;
            port(c, &r, 0x8003_6bc0, 0x8003_e5a0, &[wall, 6, off], crate::fevideowall::abi_set_offset);
            let h = if player(&mut c.m) != 0 { 150 } else { 45 };
            port(c, &r, 0x8003_6b84, 0x8003_e5cc, &[wall, 249, 270, h], crate::fevideowall::abi_set_available_text);
            port(c, &r, 0x8003_6a38, 0x8003_e5d4, &[wall], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
        r.0[0] = wall;
        if (hs(&mut c.m, this + 92) as i32) > 0 && c.m.r32(this + 84) != 0 {
            port(c, &r, 0x8003_7074, 0x8003_e604, &[wall], crate::fevideowall::abi_turn_off_instant);
            port(c, &r, 0x8003_c7b0, 0x8003_e614, &[this, 0, 0], abi_set_brightness);
        }
        port(c, &r, 0x8003_6bd4, 0x8003_e628, &[wall, sx(ok)], crate::fevideowall::abi_set_valid);
        let avail = b(&mut c.m, sp + 39);
        port(c, &r, 0x8003_6bcc, 0x8003_e634, &[wall, avail], crate::fevideowall::abi_set_available);
        port(c, &r, 0x8003_6bdc, 0x8003_e63c, &[wall], crate::fevideowall::abi_update);
        port(c, &r, 0x8003_6dac, 0x8003_e644, &[wall], crate::fevideowall::abi_draw);
    })
}

/// `tScreenCarSelectTwoPlayer::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003e660): 52 shapes of
/// `zcarsb`, 5 swapped: the first player's car's video's (noted as shown), none for the
/// second.
pub fn abi_two_player_get_shape_info(c: &mut Cpu) {
    let (this, count, swap, file) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    abi::frame(c, 240, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0], r.0[2]) = (this, 0, swap_file);
        let m = &mut c.m;
        m.w16(count, 52);
        m.w16(swap, 5);
        for at in [286, 288, 290] {
            m.w16(this + at, 0xFFFF);
        }
        m.w32(file, TWO_PLAYER_FILE);
        let ok = virtual_call(c, &r, this, 108, &[sp + 16], 0x8003_e6c8);
        let first = ok != 0 && player(&mut c.m) == 0;
        if first {
            r.0[0] = VIDEO_FILE;
            let m = &mut c.m;
            let v = b(m, sp + 216);
            m.w16(this + 286, v as u16);
            let v = b(m, sp + 215);
            m.w16(this + 290, v as u16);
            let v = m.r8(sp + 16) as i8 as i16;
            m.w16(this + 288, v as u16);
            r.call_leaving(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, sp + 32], 0x8003_e724);
            c.m.w32(swap_file, VIDEO_FILE);
        } else {
            c.m.w32(swap_file, 0);
        }
    })
}

/// `tScreenCarSelectTwoPlayer::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003e748): as the duel's,
/// the wall off before it's darkened.
pub fn abi_two_player_update_video_wall(c: &mut Cpu) {
    let (this, car) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[0]) = (this, car);
        let m = &mut c.m;
        let changed = b(m, car + 200) != hs(m, this + 286)
            || m.r8(car) as i8 as i32 as u32 != hs(m, this + 288)
            || (m.r8(car + 2) == 7 && hs(m, this + 290) != b(m, car + 199));
        if !changed {
            return;
        }
        if (m.r8(car) as i8) >= 0 {
            port(c, &r, 0x8002_5dd8, 0x8003_e7c4, &[this, car + 16], crate::fescreen::abi_async_load_swap_shape_file);
        }
        let m = &mut c.m;
        let v = b(m, car + 200);
        m.w16(this + 286, v as u16);
        let v = m.r8(car) as i8 as i16;
        m.w16(this + 288, v as u16);
        let v = b(m, car + 199);
        m.w32(this + 900, 0);
        m.w16(this + 290, v as u16);
        port(c, &r, 0x8003_7050, 0x8003_e7f8, &[this + 772], crate::fevideowall::abi_turn_off);
        port(c, &r, 0x8003_c7b0, 0x8003_e808, &[this, 0, 0], abi_set_brightness);
    })
}

/// `tScreenCarSelectTwoPlayer::TurnOffVideoWall` (SCREENCARSELECT.CPP, 0x8003e81c).
pub fn abi_two_player_turn_off_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        port(c, &r, 0x8003_7074, 0x8003_e82c, &[this + 772], crate::fevideowall::abi_turn_off_instant);
    })
}

/// The waiting message's buffer.
const WAIT_TEXT: u32 = 0x8005_30E8;

/// A drawing area primitive linked into the OT for the rectangle at `sp + 256`, as the
/// original's inline linking leaves the registers.
fn link_area(c: &mut Cpu, r: &Saved, sp: u32, ra: u32) {
    let m = &mut c.m;
    let ot = m.r32(0x1F80_0000);
    let prim = crate::mmeffect::add_prim(m, 12);
    (c.r[6], c.r[7], c.r[8], c.r[9]) = (ot, 0xFF_FFFF, 0xFF00_0000, 0x1F80_0004);
    r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, sp + 256], ra);
}

/// `tScreenCarSelectTwoPlayer::DrawBackground` (SCREENCARSELECT.CPP, 0x8003e83c): the
/// player's half (the first's at the top, the second's below): the car's name's plate, the wall
/// updated (virtual), brightness and car turning; none: the wall off; the car's video's shapes
/// up once read; the menu's background (virtual) and the dialog (virtual); the drawing area
/// clipped to the half.
pub fn abi_two_player_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 296, |c, sp| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[4]) = (this, 79);
        let (ea, eb) = (c.m.r32(DRAW_ENV_A), c.m.r32(DRAW_ENV_B));
        let env = r.call_leaving(c, GET_DRAWENV, &[ea, eb], 0x8003_e878);
        r.0[5] = env;
        draw_area(c, &r, env, sp, 0, 0x8003_e8f8);
        (r.0[2], r.0[3], r.0[1]) = (0x8005_0000, 1, sp + 48);
        let m = &mut c.m;
        m.w16(sp + 40, 290);
        m.w16(sp + 42, 25);
        if player(m) == 1 {
            m.w16(sp + 42, 130);
        }
        m.w16(sp + 44, 200);
        m.w16(sp + 46, 12);
        let car = sp + 48;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_e958);
        if ok != 0 {
            let m = &mut c.m;
            m.w16(sp + 42, 20);
            if player(m) == 1 {
                r.0[4] = 184;
                m.w16(sp + 42, 128);
            }
            let turn = m.r32(this + 924).wrapping_add(3);
            m.w32(this + 924, turn);
            let name = (m.r8(car) as i8 as i32 as u32).wrapping_add(290);
            port(c, &r, 0x8004_c490, 0x8003_e9a0, &[name, sp + 40, 0], crate::drawshp::abi_round_rectangle);
            virtual_call(c, &r, this, 100, &[car], 0x8003_e9bc);
            r.0[1] = CAR_OBJECTS;
            let m = &mut c.m;
            let p = player(m);
            let o = m.r32(CAR_OBJECTS + 4 * p);
            if m.r32(o + 2264) != 0 {
                port(c, &r, 0x8003_c7b0, 0x8003_e9f8, &[this, 0, 0], abi_set_brightness);
                let t = c.m.r32(GAME_TICKS);
                c.m.w32(this + 892, t);
            }
            let m = &mut c.m;
            let p = player(m);
            let o = m.r32(CAR_OBJECTS + 4 * p);
            if m.r32(o + 2264) == 0
                && m.r16(this + 884) == m.r16(this + 888)
                && m.r16(this + 884) == 0
                && !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 892)) as i32) < 129)
            {
                let v = if m.r8(sp + 55) != 0 { 128 } else { 32 };
                port(c, &r, 0x8003_c7b0, 0x8003_ea90, &[this, v, 0], abi_set_brightness);
                port(c, &r, 0x8003_70f0, 0x8003_ea98, &[this + 772], crate::fevideowall::abi_turn_on);
            }
            port(c, &r, 0x8003_c7c4, 0x8003_eaa4, &[this, 0], abi_update_brightness);
            let m = &mut c.m;
            m.w32(SHOWROOM_DRAW, 0);
            let (p, shade, turn) = (player(m), b(m, this + 884), m.r32(this + 924));
            let y = r.0[4];
            r.call_ptr_leaving(c, DRAW_CAR, &[car, 278, y, 0x3FD9_999A, 0xC11E_6666, shade, 0, turn, p], 0x8003_eaf4);
        } else {
            c.m.w8(car, 0xFF);
            virtual_call(c, &r, this, 100, &[car], 0x8003_eb20);
            let m = &mut c.m;
            m.w32(SHOWROOM_DRAW, 0);
            let (p, shade, turn) = (player(m), b(m, this + 884), m.r32(this + 924));
            r.call_ptr_leaving(c, DRAW_CAR, &[car, 278, 79, 0x3FD9_999A, 0xC11E_6666, shade, 0, turn, p], 0x8003_eb6c);
            virtual_call(c, &r, this, 132, &[], 0x8003_eb88);
        }
        port(c, &r, 0x8002_5e2c, 0x8003_eb94, &[this, this + 40], crate::fescreen::abi_is_shape_file_loaded);
        let p = player(&mut c.m);
        if wall_ready(&mut c.m, this, 40, 792, CAR_OBJECTS + 4 * p, 892) {
            let first = if player(&mut c.m) == 1 { 65 } else { 0 };
            port(c, &r, 0x8002_6614, 0x8003_ec3c, &[this, this + 40, 0, first, 5, 0], crate::fescreen::abi_upload_shapes);
            port(c, &r, 0x8003_70f0, 0x8003_ec44, &[this + 772], crate::fevideowall::abi_turn_on);
            r.0[1] = 0x8005_0000;
            let m = &mut c.m;
            if m.r16(this + 888) == m.r16(this + 884) {
                let v = if m.r8(sp + 55) != 0 { 128 } else { 32 };
                port(c, &r, 0x8003_c7b0, 0x8003_ec78, &[this, v, 0], abi_set_brightness);
            }
        }
        (r.0[1], r.0[2]) = (0x8005_0000, 1);
        let m = &mut c.m;
        m.w16(sp + 42, 0);
        if player(m) == 1 {
            m.w16(sp + 42, 105);
        }
        let dy = hs(m, sp + 42);
        virtual_call(c, &r, this, 84, &[dy], 0x8003_ecb4);
        virtual_call(c, &r, this, 140, &[], 0x8003_ecd0);
        let m = &mut c.m;
        m.w16(sp + 256, 0);
        let y = hu(m, env + 2);
        let h = m.r32(SCREEN_HEIGHT);
        m.w16(sp + 260, 512);
        let half = ((h as i32) / 2) as u32;
        m.w16(sp + 258, y as u16);
        m.w16(sp + 262, half as u16);
        if player(m) == 1 {
            m.w16(sp + 258, y.wrapping_add(half) as u16);
        }
        link_area(c, &r, sp, 0x8003_ed70);
    })
}

/// `tScreenCarSelectTwoPlayer::DrawForeground` (SCREENCARSELECT.CPP, 0x8003ed94): the
/// player's colour item off but for a pink slips' car, its car's five stats as sliders in its
/// half.
pub fn abi_two_player_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 296, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = 0x8005_0000;
        r.0[1] = if player(&mut c.m) == 1 { 150 } else { 45 };
        let car = sp + 56;
        let ok = virtual_call(c, &r, this, 108, &[car], 0x8003_edf4);
        r.0[4] = ok;
        let m = &mut c.m;
        let all = m.r32(ALL_MENUS);
        let item = if player(m) == 0 { all + 5784 } else { all + 6032 };
        let v = m.r32(item) & !1;
        m.w32(item, v);
        if ok == 0 || m.r8(sp + 58) == 7 {
            m.w32(item, v | 1);
        }
        (r.0[2], r.0[6], r.0[5], r.0[3]) = (0, DUEL_LABELS, SLIDER_STATS, car);
        for k in 0..5u32 {
            r.0[0] = k;
            let label = hs(&mut c.m, DUEL_LABELS + 2 * k);
            let y = sx(r.0[1].wrapping_add(4));
            port(c, &r, 0x8002_12e0, 0x8003_eed8, &[label, 500, y, 1, 0, 14], crate::fetextrender::abi_menu_text_positioned_justify);
            let v = if ok != 0 { sx(stat(&mut c.m, car, k)) } else { 0 };
            let y = sx(r.0[1]);
            r.call_ptr_leaving(c, DRAW_SLIDER, &[v, 0, 11, 417, y, 73, 3, 4, 3, 1, 0, 128, 0], 0x8003_ef9c);
            r.0[1] = r.0[1].wrapping_add(15);
            r.0[2] = k + 1;
        }
    })
}

/// `tScreenCarSelectTwoPlayer::SetDialog` (SCREENCARSELECT.CPP, 0x8003efd0): while the player
/// has chosen, the dialog waiting for the other player (by name), off to the side; else hidden.
pub fn abi_two_player_set_dialog(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        let app = m.r32(crate::globals::FE_APP);
        let p = b(m, app + 556);
        r.0[0] = p;
        let dialog = this + 928;
        if m.r32(app + 560 + 4 * p) == 0 {
            r.call_ptr_leaving(c, 0x8001_8a44, &[dialog], 0x8003_f078);
            return;
        }
        let side = if p != 0 { 60 } else { (-60i32) as u32 };
        r.0[1] = dialog;
        m.w16(dialog + 124, 0);
        m.w16(dialog + 126, side as u16);
        m.w16(dialog + 100, p as u16);
        let format = r.call_leaving(c, TEXT_WORD, &[681], 0x8003_f034);
        r.0[2] = format;
        let name = port(c, &r, 0x8002_ba7c, 0x8003_f044, &[1u32.wrapping_sub(p)], crate::front::abi_player_name);
        r.0[0] = WAIT_TEXT;
        r.call_leaving(c, SPRINTF, &[WAIT_TEXT, format, name], 0x8003_f05c);
        c.m.w32(dialog + 144, WAIT_TEXT);
        r.call_ptr_leaving(c, 0x8001_8980, &[dialog], 0x8003_f068);
    })
}

/// `tScreenCarSelectTwoPlayer::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003f090): the
/// video's buffer (16000 bytes).
pub fn abi_two_player_allocate_async_buffer(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let v = r.call_leaving(c, GET_DCT_BUFFER, &[16000, WALL_NAME], 0x8003_f0b0);
        c.m.w32(this + 48, v);
    })
}

/// `tScreenCarSelectTwoPlayer::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003f0c4): a wall
/// of five TVs, its images once loaded.
pub fn abi_two_player_initialize_video_wall(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 48, |c, _| {
        let mut r = Saved::of(c);
        let wall = this + 772;
        (r.0[0], r.0[1]) = (this, wall);
        let shapes = c.m.r32(this + 40);
        port(c, &r, 0x8003_6988, 0x8003_f108, &[wall, this + 292, shapes, 0, 5, DUEL_TVS, 0], crate::fevideowall::abi_initialize);
        if c.m.r16(this + 58) & 1 != 0 {
            port(c, &r, 0x8003_6a38, 0x8003_f124, &[wall], crate::fevideowall::abi_update_images);
            c.m.w32(this + 900, 1);
        }
    })
}

/// `tScreenCarSelectTwoPlayer::Initialize` (SCREENCARSELECT.CPP, 0x8003f140): the car
/// select's; the second player's shows no car yet, the first's wall's text placed; state 0,
/// the walls dark, not the car select's flag.
pub fn abi_two_player_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8003_c060, 0x8003_f154, &[this], abi_initialize);
        if player(&mut c.m) == 1 {
            for at in [286, 288, 290] {
                c.m.w16(this + at, 0xFFFF);
            }
        } else {
            port(c, &r, 0x8003_6b84, 0x8003_f194, &[this + 772, 249, 270, 45], crate::fevideowall::abi_set_available_text);
        }
        let m = &mut c.m;
        for at in [284, 890, 888, 886, 884] {
            m.w16(this + at, 0);
        }
        m.w32(IN_CAR_SELECT, 0);
    })
}

/// `tScreenCarSelectTwoPlayer::Cleanup` (SCREENCARSELECT.CPP, 0x8003f1c0): the dialog hidden,
/// the car select's.
pub fn abi_two_player_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        r.call_ptr_leaving(c, 0x8001_8a44, &[this + 928], 0x8003_f1d8);
        port(c, &r, 0x8003_af90, 0x8003_f1e0, &[this], abi_cleanup);
    })
}

/// The pink slips' players' load states (0 to load, 1..5 errors and outcomes, 6 ready, 7 gone,
/// 8 loading); a save going on (FEMEMCARD's); `"zHACK2"`.
const PINK_STATES: u32 = 0x8005_24C4;
const CARD_ACTIVE: u32 = 0x8005_1F04;
const HACK_FILE: u32 = 0x8001_1B04;
const REDRAW: u32 = 0x8001_3894;

fn both_ready(m: &mut ps1rt::Mem) -> bool {
    m.r32(PINK_STATES) == 6 && m.r32(PINK_STATES + 4) == 6
}

/// `tScreenPinkSlipsCarSelect::GetCar` (SCREENCARSELECT.CPP, 0x8003f1f0): with both players'
/// cars loaded, the player's pink slips car (its colour index).
pub fn abi_pink_get_car(c: &mut Cpu) {
    let car = c.r[5];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = car;
        if !both_ready(&mut c.m) {
            c.r[2] = 0;
            return;
        }
        let m = &mut c.m;
        let p = player(m);
        let id = b(m, FE + p + 293);
        port(c, &r, 0x8001_71a0, 0x8003_f250, &[CAR_MANAGER, id, car, p], crate::fecars::abi_get_pink_slips_car);
        let m = &mut c.m;
        let k = b(m, car + 197);
        let v = m.r8(car + k + 175);
        m.w8(car + 197, v);
        c.r[2] = 1;
    })
}

/// `tScreenPinkSlipsCarSelect::DrawBackground` (SCREENCARSELECT.CPP, 0x8003f280): the cards
/// handled, the two players', the dialog hidden once leaving.
pub fn abi_pink_draw_background(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        port(c, &r, 0x8003_f2c4, 0x8003_f294, &[this], abi_pink_do_mem_card_stuff);
        port(c, &r, 0x8003_e83c, 0x8003_f29c, &[this], abi_two_player_draw_background);
        if c.m.r32(this + 1096) != 0 {
            r.call_ptr_leaving(c, 0x8001_8a44, &[this + 928], 0x8003_f2b4);
        }
    })
}

/// `tScreenPinkSlipsCarSelect::DoMemCardStuff` (SCREENCARSELECT.CPP, 0x8003f2c4): the player's
/// card watched (the second's once the first's is ready) a few frames after the dialog's in:
/// gone (to load again), or a game loaded from it (`LoadGame`, the frame redrawn twice first):
/// its pink slips cars counted (all 32: full; enough for the race or money for the cheapest
/// car: ready; else too few), or the load's error; ready: the car list's iterator refreshed;
/// both ready: the card layer down, the licence plates set.
pub fn abi_pink_do_mem_card_stuff(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let p = player(m);
        let card = if p != 0 { 5 } else { 1 };
        (r.0[5], r.0[3], r.0[2], r.0[1], r.0[6]) = (0x8005_0000, this, p, card, 1);
        if m.r32(this + 1096) != 0 {
            return;
        }
        if p == 1 && m.r32(PINK_STATES) != 6 {
            return;
        }
        r.0[4] = 8;
        if m.r32(PINK_STATES) == 8 || m.r32(PINK_STATES + 4) == 8 {
            return;
        }
        let mine = PINK_STATES + 4 * p;
        (r.0[7], r.0[0]) = (p << 2, mine);
        let state = m.r32(mine);
        if state == 6 {
            return;
        }
        if m.r32(this + 1060) ^ 1 != 0 {
            m.w32(this + 1080, 0);
            return;
        }
        let wait = m.r32(this + 1080);
        if (wait as i32) < 5 {
            m.w32(this + 1080, wait.wrapping_add(1));
            return;
        }
        let mut s3_lui = true;
        if state != 8 {
            port(c, &r, 0x8005_0368, 0x8003_f3b4, &[card], crate::memcard::abi_handlecardevents);
            let info = port(c, &r, 0x8005_072c, 0x8003_f3bc, &[card], crate::memcard::abi_getcard);
            let m = &mut c.m;
            m.w32(this + 1084, info);
            if m.r32(info) == u32::MAX {
                m.w32(mine, 0);
            } else if m.r32(CARD_ACTIVE) != 0 || m.r32(mine) != 0 {
                r.0[3] = 0x8005_0000;
                s3_lui = false;
            } else {
                m.w32(mine, 8);
                let app = m.r32(crate::globals::FE_APP);
                r.call_ptr_leaving(c, REDRAW, &[app], 0x8003_f404);
                r.0[1] = p;
                let app = c.m.r32(crate::globals::FE_APP);
                r.call_ptr_leaving(c, REDRAW, &[app], 0x8003_f410);
                let v = sx(port(c, &r, 0x8003_4f70, 0x8003_f420, &[p, 1, 0], crate::fememcard::abi_load_game));
                match v {
                    0 => {
                        r.0[3] = CAR_MANAGER;
                        let n = port(c, &r, 0x8001_7568, 0x8003_f440, &[CAR_MANAGER, p], crate::fecars::abi_get_num_pink_slips_cars);
                        r.0[4] = 0;
                        if sx(n) == 32 {
                            c.m.w32(mine, 5);
                        } else {
                            let n = port(c, &r, 0x8001_7568, 0x8003_f46c, &[CAR_MANAGER, p], crate::fecars::abi_get_num_pink_slips_cars);
                            let mut ok = !((sx(n) as i32) < 2);
                            if !ok {
                                let n = port(c, &r, 0x8001_7568, 0x8003_f488, &[CAR_MANAGER, p], crate::fecars::abi_get_num_pink_slips_cars);
                                if sx(n) == 1 {
                                    r.0[0] = FE + (p << 2);
                                    let price = port(c, &r, 0x8001_6434, 0x8003_f4a8, &[CAR_MANAGER], crate::fecars::abi_cheapest_car_stock_price);
                                    let money = c.m.r32(FE + (p << 2) + 76);
                                    ok = !((money as i32) < price as i32);
                                }
                            }
                            r.0[4] = ok as u32;
                            c.m.w32(mine, if ok { 6 } else { 4 });
                        }
                    }
                    1 => c.m.w32(mine, 1),
                    2 => c.m.w32(mine, 3),
                    _ => c.m.w32(mine, 2),
                }
            }
        }
        if s3_lui {
            r.0[3] = 0x8005_0000;
        }
        r.0[4] = PINK_STATES;
        (r.0[1], r.0[0]) = (6, 0x8005_0000);
        if c.m.r32(mine) == 6 {
            let list = c.m.r32(ALL_MENUS) + 6612;
            port(c, &r, 0x8001_81f4, 0x8003_f55c, &[list, p], crate::fecars::abi_list_iterator_car_decrement);
            let list = c.m.r32(ALL_MENUS) + 6612;
            port(c, &r, 0x8001_81d4, 0x8003_f56c, &[list, p], crate::fecars::abi_list_iterator_car_increment);
        }
        if both_ready(&mut c.m) {
            port(c, &r, 0x8003_4938, 0x8003_f594, &[], crate::fememcard::abi_deinit_memcard);
            port(c, &r, 0x8002_b84c, 0x8003_f59c, &[], crate::front::abi_set_license_plate);
        }
    })
}

/// `tScreenPinkSlipsCarSelect::DrawForeground` (SCREENCARSELECT.CPP, 0x8003f5c8): two players'.
pub fn abi_pink_draw_foreground(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        port(c, &r, 0x8003_ed94, 0x8003_f5d8, &[this], abi_two_player_draw_foreground);
    })
}

/// `tScreenPinkSlipsCarSelect::Initialize` (SCREENCARSELECT.CPP, 0x8003f5e8): nothing loaded,
/// the first card's record, the card layer started, two players' screen, its wall's images
/// not loaded.
pub fn abi_pink_initialize(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        for at in [1080, 1088, 1092] {
            m.w32(this + at, 0);
        }
        m.w32(PINK_STATES, 0);
        m.w32(PINK_STATES + 4, 0);
        let info = port(c, &r, 0x8005_072c, 0x8003_f61c, &[1], crate::memcard::abi_getcard);
        c.m.w32(this + 1084, info);
        port(c, &r, 0x8003_47d4, 0x8003_f62c, &[1, 1], crate::fememcard::abi_init_memcard);
        c.m.w32(this + 1096, 0);
        port(c, &r, 0x8003_f140, 0x8003_f638, &[this], abi_two_player_initialize);
        let f = c.m.r16(this + 58) & 0xFFFE;
        c.m.w16(this + 58, f);
    })
}

/// `tScreenPinkSlipsCarSelect::Cleanup` (SCREENCARSELECT.CPP, 0x8003f658): leaving, both
/// players gone, the dialog hidden, the card layer down, two players'.
pub fn abi_pink_cleanup(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        m.w32(this + 1096, 1);
        m.w32(PINK_STATES, 7);
        m.w32(PINK_STATES + 4, 7);
        r.call_ptr_leaving(c, 0x8001_8a44, &[this + 928], 0x8003_f68c);
        port(c, &r, 0x8003_4938, 0x8003_f694, &[], crate::fememcard::abi_deinit_memcard);
        port(c, &r, 0x8003_f1c0, 0x8003_f69c, &[this], abi_two_player_cleanup);
    })
}

/// `tScreenPinkSlipsCarSelect::SetDialog` (SCREENCARSELECT.CPP, 0x8003f6ac): the player's card's
/// state as the dialog (the second player's only once the first's ready; none when leaving):
/// insert a card (its message, then a pause), the load's errors and outcomes, ready (waiting for
/// the other player while either hasn't chosen), gone (hidden).
pub fn abi_pink_set_dialog(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        let p = player(m);
        (r.0[2], r.0[0]) = (this, p);
        let d = this + 928;
        m.w16(d + 124, 0);
        m.w16(d + 126, if p != 0 { 60 } else { 0xFFC4 });
        m.w16(d + 100, p as u16);
        let hide = |c: &mut Cpu, r: &Saved, ra: u32| {
            r.call_ptr_leaving(c, 0x8001_8a44, &[d], ra);
        };
        if (m.r32(PINK_STATES) != 6 && p == 1) || m.r32(this + 1096) != 0 {
            hide(c, &r, 0x8003_f770);
            return;
        }
        let mine = PINK_STATES + 4 * p;
        if m.r32(mine) != 0 {
            m.w32(this + 1088, 0);
        }
        let show = |c: &mut Cpu, r: &mut Saved, word: u32, ras: (u32, u32)| {
            r.0[0] = d;
            let t = r.call_leaving(c, TEXT_WORD, &[word], ras.0);
            c.m.w32(d + 144, t);
            r.call_ptr_leaving(c, 0x8001_8980, &[d], ras.1);
        };
        match m.r32(mine) {
            0 => {
                if m.r32(this + 1092) == 0 {
                    if m.r32(this + 1088) == 0 {
                        let t = m.r32(GAME_TICKS);
                        m.w32(this + 1088, t);
                    }
                    let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 1088));
                    let word = p + if (since as i32) < 800 { 684 } else { 682 };
                    show(c, &mut r, word, (0x8003_f860, 0x8003_f86c));
                } else {
                    if m.r32(this + 1088) == 0 {
                        let t = m.r32(GAME_TICKS);
                        m.w32(this + 1088, t);
                    }
                    if !((m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 1088)) as i32) < 901) {
                        m.w32(this + 1088, 0);
                        m.w32(this + 1092, 0);
                    }
                }
            }
            k @ 1..=3 => {
                let word = p + [686, 688, 690][k as usize - 1];
                show(c, &mut r, word, (0x8003_f8d4, 0x8003_f8e0));
                c.m.w32(this + 1092, 1);
                c.m.w32(this + 1088, 0);
            }
            k @ (4 | 5 | 8) => {
                let word = p + match k {
                    4 => 814,
                    5 => 816,
                    _ => 641,
                };
                show(c, &mut r, word, (0x8003_f90c, 0x8003_f91c));
                c.m.w32(this + 1088, 0);
                c.m.w32(this + 1092, 0);
            }
            6 => {
                let app = m.r32(crate::globals::FE_APP);
                let chosen = m.r32(app + 560 + 4 * p) != 0;
                if chosen || m.r32(PINK_STATES + 4 * (1 - p)) != 6 {
                    let format = r.call_leaving(c, TEXT_WORD, &[681], 0x8003_f7c4);
                    r.0[1] = format;
                    let name = port(c, &r, 0x8002_ba7c, 0x8003_f7d4, &[1u32.wrapping_sub(p)], crate::front::abi_player_name);
                    r.0[0] = WAIT_TEXT;
                    r.call_leaving(c, SPRINTF, &[WAIT_TEXT, format, name], 0x8003_f7ec);
                    c.m.w32(d + 144, WAIT_TEXT);
                    r.call_ptr_leaving(c, 0x8001_8980, &[d], 0x8003_f91c);
                    c.m.w32(this + 1088, 0);
                    c.m.w32(this + 1092, 0);
                } else {
                    hide(c, &r, 0x8003_f800);
                    c.m.w32(this + 1088, 0);
                    c.m.w32(this + 1092, 0);
                }
            }
            7 => hide(c, &r, 0x8003_f770),
            _ => {}
        }
    })
}

/// `tScreenPinkSlipsCarSelect::ProcessInput` (SCREENCARSELECT.CPP, 0x8003f93c): until both
/// players' cars are loaded only back works (other keys made nothing); back leaves (the
/// dialog hidden); then the car select's.
pub fn abi_pink_process_input(c: &mut Cpu) {
    let (this, who, key, cmd) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    abi::frame(c, 40, |c, _| {
        let mut r = Saved::of(c);
        (r.0[1], r.0[2], r.0[0], r.0[3]) = (this, who, key, cmd);
        let m = &mut c.m;
        let mut k = m.r32(key);
        if k != 16 {
            if k != 4 {
                if !both_ready(m) {
                    m.w32(key, 1);
                }
                k = m.r32(key);
            }
        }
        if k == 16 && !both_ready(&mut c.m) {
            let m = &mut c.m;
            m.w32(this + 1096, 1);
            m.w32(PINK_STATES, 7);
            m.w32(PINK_STATES + 4, 7);
            r.call_ptr_leaving(c, 0x8001_8a44, &[this + 928], 0x8003_f9f4);
        }
        port(c, &r, 0x8003_c268, 0x8003_fa08, &[this, who, key, cmd], abi_process_input);
    })
}

/// `tScreenPinkSlipsCarSelect::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003fa24): 52 shapes of
/// `zcarsb`, 5 swapped from a placeholder video's file; no car shown.
pub fn abi_pink_get_shape_info(c: &mut Cpu) {
    let (this, count, swap, file) = (c.r[4], c.r[5], c.r[6], c.r[7]);
    let swap_file = abi::arg(c, 4);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        let m = &mut c.m;
        m.w16(count, 52);
        m.w16(swap, 5);
        for at in [286, 288, 290] {
            m.w16(this + at, 0xFFFF);
        }
        m.w32(file, TWO_PLAYER_FILE);
        (r.0[1], r.0[0]) = (swap_file, VIDEO_FILE);
        r.call_leaving(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, HACK_FILE], 0x8003_fa84);
        c.m.w32(swap_file, VIDEO_FILE);
    })
}

/// A two-player screen's destructor: its dialog's, then the car select's.
fn two_player_destructor(c: &mut Cpu, ras: (u32, u32)) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        (r.0[0], r.0[1]) = (this, flags);
        port(c, &r, 0x8002_6098, ras.0, &[this + 928, 2], crate::fescreen::abi_screen_destructor);
        port(c, &r, 0x8003_af68, ras.1, &[this, flags], abi_screen_car_select_destructor);
    })
}

/// `tScreenPinkSlipsCarSelect::~tScreenPinkSlipsCarSelect` (SCREENCARSELECT.CPP, 0x8003fa9c).
pub fn abi_pink_destructor(c: &mut Cpu) {
    two_player_destructor(c, (0x8003_fac0, 0x8003_facc));
}

/// `tScreenCarSelectTwoPlayer::~tScreenCarSelectTwoPlayer` (SCREENCARSELECT.CPP, 0x8003fae0).
pub fn abi_two_player_destructor(c: &mut Cpu) {
    two_player_destructor(c, (0x8003_fb04, 0x8003_fb10));
}

/// `tScreenCarSelectDuel::~tScreenCarSelectDuel` (SCREENCARSELECT.CPP, 0x8003fb24).
pub fn abi_duel_destructor(c: &mut Cpu) {
    let (this, flags) = (c.r[4], c.r[5]);
    abi::frame(c, 24, |c, _| {
        let r = Saved::of(c);
        port(c, &r, 0x8003_af68, 0x8003_fb34, &[this, flags], abi_screen_car_select_destructor);
    })
}

#[cfg(test)]
mod variant_tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, pick, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram};

    /// The game's duel, two players' and pink slips' car selects (the RAM image's); the outputs
    /// of `GetShapeInfo`, a key.
    const DUEL: u32 = 0x8017_E3E0;
    const TWO: u32 = 0x8017_E7B0;
    const PINK: u32 = 0x8017_F020;
    const OUTS: u32 = 0x8014_7100;
    const INITED: u32 = 0x8005_1F08;

    /// FEMENU's structures, the front end's choices and the menus' items as the single
    /// player's tests have them; then each screen: its shape information the car select's
    /// (the duel's opponent's too, its data there or not), its state, overlays, slots, car
    /// shown, wall flags, brightness and clocks; the player, both players' chosen flags, the
    /// dialog's clocks and flags, the leaving flag, both players' load states (a card's load
    /// never started: it waits on the card's dialogs) and the card layer down.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let m = &mut c.m;
        m.w8(FE + 4, pick(rng, 4) as u8);
        m.w8(FE + 3, pick(rng, 3) as u8);
        let cars = m.r32(CAR_MANAGER);
        for at in [92, 93, 291, 292, 293, 294, 296, 297] {
            m.w8(FE + at, [0, 1, pick(rng, cars.max(1)), cars.wrapping_sub(1)][pick(rng, 4) as usize] as u8);
        }
        m.w8(FE + 298, pick(rng, 2) as u8);
        m.w8(OPPONENT_CAR, pick(rng, cars.max(1)) as u8);
        let all = m.r32(ALL_MENUS);
        for at in [4604, 4644, 4924, 5784, 6032, 7688, 8216, 8248, 8312, 8376] {
            let v = m.r32(all + at) & !1;
            m.w32(all + at, v | pick(rng, 2));
        }
        let app = m.r32(crate::globals::FE_APP);
        m.w8(app + 556, pick(rng, 2) as u8);
        m.w32(app + 560, pick(rng, 2));
        m.w32(app + 564, pick(rng, 2));
        let select = m.r32(app + 12);
        let t = m.r32(GAME_TICKS);
        for s in [DUEL, TWO, PINK] {
            // (the walls and their TVs the single player's: these screens' were never set up)
            for k in (292..828).step_by(4) {
                let v = m.r32(select + k);
                m.w32(s + k, v);
            }
            if s == DUEL {
                for k in (0..56).step_by(4) {
                    let v = m.r32(select + 772 + k);
                    m.w32(s + 828 + k, v);
                }
            }
            for k in 0..10 {
                let v = m.r32(select + 40 + 4 * k);
                m.w32(s + 40 + 4 * k, v);
            }
            let buffer = m.r32(select + 48);
            m.w32(s + 52, 0);
            m.w32(s + 44, [0, buffer][pick(rng, 2) as usize]);
            m.w16(s + 284, pick(rng, 8) as u16);
            for k in 0..7 {
                let o = s + 100 + 24 * k;
                m.w16(o + 16, [0, 1, 64, 127, 128][pick(rng, 5) as usize]);
                m.w16(o + 18, [6, 0xFFFA][pick(rng, 2) as usize]);
                m.w16(o + 20, [0, 1, 0xFFFF][pick(rng, 3) as usize]);
            }
            for k in 0..4 {
                let v = if pick(rng, 3) == 0 { 0 } else { s + 100 + 24 * pick(rng, 7) };
                m.w32(s + 268 + 4 * k, v);
            }
            for at in [286, 288, 290] {
                m.w16(s + at, [0xFFFF, 0, 16, pick(rng, 30)][pick(rng, 4) as usize] as u16);
            }
            for at in [892, 896, 908, 912] {
                m.w32(s + at, t.wrapping_sub([0, 128, 129, 600, 700][pick(rng, 5) as usize]));
            }
            for at in [900, 904] {
                m.w32(s + at, pick(rng, 2));
            }
            m.w32(s + 916, 1);
            m.w32(s + 924, pick(rng, 4096));
            for at in [884, 886, 888, 890] {
                m.w16(s + at, [0, 32, 128, pick(rng, 140)][pick(rng, 4) as usize] as u16);
            }
            m.w16(s + 92, [0, 64, 128, 200][pick(rng, 4) as usize]);
            m.w32(s + 84, pick(rng, 2));
            let f = m.r16(s + 58) & !1;
            m.w16(s + 58, f | pick(rng, 2) as u16);
            m.w16(s + 792, [0xFFFF, 0][pick(rng, 2) as usize]);
        }
        for k in 0..10 {
            let v = m.r32(select + 40 + 4 * k);
            m.w32(DUEL + 936 + 4 * k, v);
        }
        let buffer = m.r32(select + 48);
        m.w32(DUEL + 948, 0);
        m.w32(DUEL + 940, [0, buffer][pick(rng, 2) as usize]);
        m.w16(DUEL + 928, [0xFFFF, 0, 16][pick(rng, 3) as usize]);
        m.w32(DUEL + 932, pick(rng, 2));
        m.w16(DUEL + 848, [0xFFFF, 0][pick(rng, 2) as usize]);
        m.w32(PINK + 1060, pick(rng, 2));
        m.w32(PINK + 1080, [0, 4, 5, 6][pick(rng, 4) as usize]);
        m.w32(PINK + 1088, [0, t, t.wrapping_sub(799), t.wrapping_sub(800), t.wrapping_sub(901)][pick(rng, 5) as usize]);
        m.w32(PINK + 1092, pick(rng, 2));
        m.w32(PINK + 1096, (pick(rng, 4) == 0) as u32);
        for k in 0..2 {
            m.w32(PINK_STATES + 4 * k, [0, 1, 2, 3, 4, 5, 6, 6, 7, 8][pick(rng, 10) as usize]);
        }
        let any_new = m.r32(PINK_STATES) == 0 || m.r32(PINK_STATES + 4) == 0;
        m.w32(CARD_ACTIVE, any_new as u32);
        m.w32(INITED, 0);
        for p in 0..2 {
            let o = m.r32(CAR_OBJECTS + 4 * p);
            m.w32(o + 2264, pick(rng, 2));
        }
    }

    fn cases() -> Vec<Case> {
        vec![
            ("tScreenCarSelectDuel::PreLoad", 0x8003_d3bc, nfs4rc::f_8003d3bc, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::AllocateAsyncBuffer", 0x8003_d4e4, nfs4rc::f_8003d4e4, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::FreeAsyncBuffer", 0x8003_d4ec, nfs4rc::f_8003d4ec, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::InitializeVideoWall", 0x8003_d518, nfs4rc::f_8003d518, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::Initialize", 0x8003_d634, nfs4rc::f_8003d634, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::DrawVideoWall", 0x8003_d694, nfs4rc::f_8003d694, |_, r| vec![DUEL, [0, 105][pick(r, 2) as usize]]),
            ("tScreenCarSelectDuel::DrawOpponentVideoWall", 0x8003_d7ec, nfs4rc::f_8003d7ec, |_, r| vec![DUEL, [0, 105][pick(r, 2) as usize]]),
            ("tScreenCarSelectDuel::GetShapeInfo", 0x8003_d910, nfs4rc::f_8003d910, |_, _| vec![DUEL, OUTS, OUTS + 4, OUTS + 8, OUTS + 12]),
            ("tScreenCarSelectDuel::UpdateVideoWall", 0x8003_d9cc, nfs4rc::f_8003d9cc, |c, r| car_args(c, r, DUEL)),
            ("tScreenCarSelectDuel::UpdateOpponentVideoWall", 0x8003_da9c, nfs4rc::f_8003da9c, |c, r| car_args(c, r, DUEL)),
            ("tScreenCarSelectDuel::DrawBackground", 0x8003_db20, nfs4rc::f_8003db20, |_, _| vec![DUEL]),
            ("tScreenCarSelectDuel::DrawForeground", 0x8003_e198, nfs4rc::f_8003e198, |_, _| vec![DUEL]),
            ("tScreenCarSelectTwoPlayer::GetCar", 0x8003_e370, nfs4rc::f_8003e370, |_, _| vec![TWO, OUTS + 0x40]),
            ("tScreenCarSelectTwoPlayer::DrawVideoWall", 0x8003_e4c0, nfs4rc::f_8003e4c0, |_, r| vec![TWO, [0, 105][pick(r, 2) as usize]]),
            ("tScreenCarSelectTwoPlayer::GetShapeInfo", 0x8003_e660, nfs4rc::f_8003e660, |_, _| vec![TWO, OUTS, OUTS + 4, OUTS + 8, OUTS + 12]),
            ("tScreenCarSelectTwoPlayer::UpdateVideoWall", 0x8003_e748, nfs4rc::f_8003e748, |c, r| car_args(c, r, TWO)),
            ("tScreenCarSelectTwoPlayer::TurnOffVideoWall", 0x8003_e81c, nfs4rc::f_8003e81c, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::DrawBackground", 0x8003_e83c, nfs4rc::f_8003e83c, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::DrawForeground", 0x8003_ed94, nfs4rc::f_8003ed94, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::SetDialog", 0x8003_efd0, nfs4rc::f_8003efd0, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::AllocateAsyncBuffer", 0x8003_f090, nfs4rc::f_8003f090, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::InitializeVideoWall", 0x8003_f0c4, nfs4rc::f_8003f0c4, |_, _| vec![TWO]),
            ("tScreenCarSelectTwoPlayer::Initialize", 0x8003_f140, nfs4rc::f_8003f140, |_, _| vec![TWO]),
            ("tScreenPinkSlipsCarSelect::GetCar", 0x8003_f1f0, nfs4rc::f_8003f1f0, |_, _| vec![PINK, OUTS + 0x40]),
            ("tScreenPinkSlipsCarSelect::DrawBackground", 0x8003_f280, nfs4rc::f_8003f280, |_, _| vec![PINK]),
            ("tScreenPinkSlipsCarSelect::DoMemCardStuff", 0x8003_f2c4, nfs4rc::f_8003f2c4, |_, _| vec![PINK]),
            ("tScreenPinkSlipsCarSelect::DrawForeground", 0x8003_f5c8, nfs4rc::f_8003f5c8, |_, _| vec![PINK]),
            ("tScreenPinkSlipsCarSelect::SetDialog", 0x8003_f6ac, nfs4rc::f_8003f6ac, |_, _| vec![PINK]),
            ("tScreenPinkSlipsCarSelect::ProcessInput", 0x8003_f93c, nfs4rc::f_8003f93c, |c, r| {
                c.m.w32(OUTS, [8, 16, 4, 1][pick(r, 4) as usize]);
                vec![PINK, 0, OUTS, OUTS + 4]
            }),
            ("tScreenPinkSlipsCarSelect::GetShapeInfo", 0x8003_fa24, nfs4rc::f_8003fa24, |_, _| vec![PINK, OUTS, OUTS + 4, OUTS + 8, OUTS + 12]),
            ("~tScreenPinkSlipsCarSelect", 0x8003_fa9c, nfs4rc::f_8003fa9c, |_, r| vec![PINK, 2 * pick(r, 2)]),
            ("~tScreenCarSelectTwoPlayer", 0x8003_fae0, nfs4rc::f_8003fae0, |_, r| vec![TWO, 2 * pick(r, 2)]),
            ("~tScreenCarSelectDuel", 0x8003_fb24, nfs4rc::f_8003fb24, |_, r| vec![DUEL, 2 * pick(r, 2)]),
        ]
    }

    /// A car's info for the walls' updates: its id, kind, livery, colour and video's number.
    fn car_args(c: &mut Cpu, r: &mut Rng, s: u32) -> Vec<u32> {
        let car = OUTS + 0x40;
        for (at, v) in [(0, [0xFF, 0, 16][pick(r, 3) as usize]), (2, [0, 7][pick(r, 2) as usize]), (199, pick(r, 3)), (200, [16, 0, 0xFF][pick(r, 3) as usize])] {
            c.m.w8(car + at, v as u8);
        }
        for k in 0..8 {
            c.m.w8(car + 16 + k, if k < 4 { b'a' + pick(r, 26) as u8 } else { 0 });
        }
        vec![s, car]
    }

    #[test]
    fn screencarselect_variants_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xE0D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 150, 0xE0E + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

}
