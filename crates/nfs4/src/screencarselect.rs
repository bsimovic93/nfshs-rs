//! SCREENCARSELECT.CPP: the car select screens: the single player's (`tScreenCarSelect`), the
//! duel's (`tScreenCarSelectDuel`, the opponent's car beside), two players' and the pink
//! slips' (`tScreenCarSelectTwoPlayer`, `tScreenPinkSlipsCarSelect`, saving the bet cars):
//! the car turning on its stand (its 3D model drawn with its colours), the overlays (the car's
//! name and price, its stats, the upgrades, the sliders) moving between their places, the
//! video wall showing the car's videos, the brightness, the input.
//!
//! The screen (`tScreen` first): +100 the overlays (7 of 24 bytes), +268 the four overlay
//! slots, +284 the state, +286.. the car shown on the wall (its index, id, livery), +292 the
//! wall's ten TVs (48 bytes each), +772 the wall, +884 the walls' brightness (two halves) and
//! +888 their targets, +892 when the wall went dark, +896, +900 the wall's images loaded, +904
//! in the showroom, +908 the spline's clock, +912 the showroom's start, +916 the speech played,
//! +920 the spline's point, +924 the car's turn.

use crate::call::{call, call_addr, call_virtual, with_buffer};
use crate::globals::{FE_APP, GAME_TICKS, TEXT_COLORS};
use crate::types::{CarInfo, CarManager, Screen};
use crate::{drawshp, fecars, fescreen, fetextrender, fevideowall};
use ps1rt::{Cpu, Mem};

/// The screen's fields (see above); the screen's own: +0 its permanent shapes, +40 its swap
/// shapes' information, +44 their file read, +48 the videos' buffer, +58 its flags (1 the
/// shapes in), +84 its transition, +92 its fade, +96 its vtable.
const PERMANENT: u32 = 0;
const SWAP_INFO: u32 = 40;
const SWAP_READ: u32 = 44;
const ASYNC_BUFFER: u32 = 48;
const FLAGS: u32 = 58;
const TRANSITION: u32 = 84;
const FADE: u32 = 92;
const VPTR: u32 = 96;
const OVERLAYS: u32 = 100;
const SLOTS: u32 = 268;
const STATE: u32 = 284;
const SHOWN_INDEX: u32 = 286;
const SHOWN_CAR: u32 = 288;
const SHOWN_LIVERY: u32 = 290;
const TVS: u32 = 292;
const WALL: u32 = 772;
const BRIGHTNESS: u32 = 884;
const TARGET: u32 = 888;
const DARK_SINCE: u32 = 892;
const IMAGES_LOADED: u32 = 900;
const IN_SHOWROOM: u32 = 904;
const SPLINE_CLOCK: u32 = 908;
const SHOWROOM_START: u32 = 912;
const SPOKEN: u32 = 916;
const SPLINE_POINT: u32 = 920;
const TURN: u32 = 924;

/// The virtual methods (their slots): `DrawVideoWall`, `InitializeVideoWall`,
/// `UpdateVideoWall`, `GetCar`, `AllocateAsyncBuffer`, `FreeAsyncBuffer`.
const DRAW_VIDEO_WALL: u32 = 80;
const INITIALIZE_VIDEO_WALL: u32 = 88;
const UPDATE_VIDEO_WALL: u32 = 96;
const GET_CAR: u32 = 104;
const ALLOCATE_ASYNC_BUFFER: u32 = 112;
const FREE_ASYNC_BUFFER: u32 = 120;

/// The states: the cars (0), the garage (1), the second player's cars (2), the stats (3), the
/// race (4), the showroom (5), the second's showroom (6), the test drive's garage (7).
const CARS: u32 = 0;
const GARAGE: u32 = 1;
const SECOND_CARS: u32 = 2;
const STATS: u32 = 3;
const RACE: u32 = 4;
const SHOWROOM: u32 = 5;
const SECOND_SHOWROOM: u32 = 6;
const TEST_DRIVE: u32 = 7;

/// An overlay (24 bytes): +0 its rectangle from and +8 to (x, y, w, h halves), +16 its fade
/// (0..128, from to to), +18 its step, +20 its direction (1 in, -1 out, 0 still), +22 its
/// kind (0 the car's name, 1..3 its price, 4 its options, 5 its stats and the choice, 6 the
/// sliders).
const OVERLAY_SIZE: u32 = 24;
const OV_FADE: u32 = 16;
const OV_STEP: u32 = 18;
const OV_DIR: u32 = 20;
const OV_KIND: u32 = 22;

/// The overlays' places, the car objects (by player: +648 the model, +2264 busy), their draw
/// counts, the car's 3D world, the car manager and its player's car, the money, the menus.
const OVERLAY_PLACES: u32 = 0x8005_2454;
const CAR_OBJECTS: u32 = 0x8005_2EF8;
const CAR_DRAWS: u32 = 0x8005_174C;
const CAR_WORLD: u32 = 0x8011_8234;
const CAR_MANAGER: CarManager = CarManager(0x8011_552C);
const PLAYER_CAR: u32 = 0x8011_59E1;
const MONEY: u32 = 0x8011_5DA4;
const ALL_MENUS: u32 = 0x8005_1EE0;
const PAD_TYPE: u32 = 0x8013_FBA9;
/// The car's drawing's flags (the turning's half).
const DRAW_FLAG: u32 = 0x8013_EAF0;
const DRAW_HALF: u32 = 0x8013_EAF4;
const VTABLE: u32 = 0x8001_1CB8;
/// The options' price slots and their upgrade bits (three halves each), by the menu's choice.
const PRICE_SLOTS: u32 = 0x8001_1A4C;
const UPGRADE_BITS: u32 = 0x8001_1A54;
/// The menus' fields (`ALL_MENUS` +): the stats' choice, the items enabled.
const CHOICE: u32 = 7996;
const PINK_ITEM: u32 = 8248;
const RACE_ITEM: u32 = 8376;

const FIXEDMULT: fn(&mut Cpu) = nfs4rc::f_800e4f14;
const MENU_COLOR_DATA: fn(&mut Cpu) = nfs4rc::f_800bf8a8;
/// The host's `PSXDrawSquare`.
const DRAW_SQUARE: u32 = 0x8004_F12C;

fn sx(v: u32) -> u32 {
    v as u16 as i16 as i32 as u32
}

fn hs(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn hu(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

fn b(m: &mut Mem, at: u32) -> u32 {
    m.r8(at) as u32
}

/// The screen's car (`GetCar`, virtual) into `car`; whether there's one (none: `car` as it
/// was).
fn get_car_virtual(c: &mut Cpu, this: u32, car: u32) -> bool {
    call_virtual(c, this, VPTR, GET_CAR, &[car]) != 0
}

/// `size` bytes of the PS1 stack for `f`, cleared (where the original's stack held whatever
/// was there: read only when a screen has no car, and only for drawing).
fn zeroed<R>(c: &mut Cpu, size: u32, f: impl FnOnce(&mut Cpu, u32) -> R) -> R {
    with_buffer(c, size, |c, at| {
        for k in (0..size).step_by(4) {
            c.m.w32(at + k, 0);
        }
        f(c, at)
    })
}

/// Overlay `k`'s direction set (1 fading in, -1 out).
fn set_overlay_dir(m: &mut Mem, this: u32, k: u32, dir: u16) {
    m.w16(this + OVERLAYS + OVERLAY_SIZE * k + OV_DIR, dir);
}

/// The front end's application's player (its +556).
fn app_player(m: &mut Mem) -> u32 {
    let app = m.r32(FE_APP);
    b(m, app + 556)
}

/// `TransformVector` (SCREENCARSELECT.CPP, 0x8003ab80): `out` the 4-vector `v` times the 4x4
/// fixed-point matrix `mat` (`out[j] = sum v[i] * mat[i][j]`).
pub fn transform_vector(c: &mut Cpu, v: u32, mat: u32, out: u32) {
    for j in 0..4 {
        let at = out + 4 * j;
        c.m.w32(at, 0);
        for i in 0..4 {
            let (a, x) = (c.m.r32(v + 4 * i), c.m.r32(mat + 16 * i + 4 * j));
            let p = call(c, FIXEDMULT, &[a, x]);
            let s = c.m.r32(at).wrapping_add(p);
            c.m.w32(at, s);
        }
    }
}

pub fn abi_transform_vector(c: &mut Cpu) {
    transform_vector(c, c.r[4], c.r[5], c.r[6]);
}

/// `DrawCar` (SCREENCARSELECT.CPP, 0x8003ac64, hooked: called by address): `car` (none: its
/// empty stand) turning on its stand at `x`, `y` for `player`: the car object given its model,
/// livery and upgrades, its colours set, drawn with the camera `distance` back and `height` up
/// (floats), turned by `angle`, its facets with `facet`, reflected or not (the clock's turn's
/// half flagged).
#[allow(clippy::too_many_arguments)]
pub fn draw_car(c: &mut Cpu, car: u32, x: u32, y: u32, distance: u32, height: u32, facet: u32, reflection: u32, angle: u32, player: u32) {
    let m = &mut c.m;
    let half = (m.r32(GAME_TICKS) as i32).wrapping_rem(2048) >= 1024;
    m.w32(DRAW_FLAG, 0);
    m.w32(DRAW_HALF, half as u32);
    let info = CarInfo(car);
    if info.car_id(m) < 0 {
        crate::fe3dmenu::draw_empty_view(c, CAR_WORLD, sx(x), sx(y), player, 0, angle, distance, height);
        return;
    }
    let object = m.r32(CAR_OBJECTS + 4 * player);
    let model = m.r32(object + 648);
    let v = info.sim_number(m) as u32;
    m.w32(model, v);
    let v = info.country(m) as u32;
    m.w32(model + 160, v);
    let draws = m.r32(CAR_DRAWS + 4 * player).wrapping_add(3);
    m.w32(CAR_DRAWS + 4 * player, draws);
    let upgrades = info.upgrades(m) as u32;
    for (bit, at) in [(2, 60), (1, 64), (0, 68)] {
        m.w32(model + at, (upgrades >> bit) & 1);
    }
    let colour = info.color(m) as u32;
    call(c, MENU_COLOR_DATA, &[colour, object, player]);
    let view = crate::fe3dmenu::CarView { player, spin: 0, angle, distance, height, facet, reflection: reflection != 0 };
    crate::fe3dmenu::draw_menu_rendering_view(c, object, CAR_WORLD, sx(x), sx(y), &view);
}

pub fn abi_draw_car(c: &mut Cpu) {
    let a: Vec<u32> = (4..9).map(|k| crate::abi::arg(c, k)).collect();
    draw_car(c, c.r[4], c.r[5], c.r[6], c.r[7], a[0], a[1] & 0xFF, a[2], a[3], a[4]);
}

/// `DrawCar` by address (the host hooks it).
const DRAW_CAR: u32 = 0x8003_AC64;

/// `tScreenCarSelect::tScreenCarSelect` (SCREENCARSELECT.CPP, 0x8003ae58): a screen with its
/// seven overlays at their places (none fading, their kinds), no state before, no slots.
pub fn screen_car_select(c: &mut Cpu, this: u32) -> u32 {
    fescreen::screen(c, Screen(this));
    let m = &mut c.m;
    m.w32(this + VPTR, VTABLE);
    m.w16(this + SHOWN_INDEX, 0);
    m.w16(this + SHOWN_CAR, 0xFFFF);
    m.w16(this + SHOWN_LIVERY, 0);
    for k in 0..7 {
        let o = this + OVERLAYS + OVERLAY_SIZE * k;
        for j in 0..16 {
            let v = m.r8(OVERLAY_PLACES + 16 * k + j);
            m.w8(o + j, v);
        }
        m.w16(o + OV_KIND, k as u16);
        m.w16(o + OV_DIR, 0);
        m.w16(o + OV_FADE, 0);
        m.w16(o + OV_STEP, 6);
    }
    for k in 0..4 {
        m.w32(this + SLOTS + 4 * k, 0);
    }
    this
}

pub fn abi_screen_car_select(c: &mut Cpu) {
    c.r[2] = screen_car_select(c, c.r[4]);
}

/// `tScreenCarSelect::~tScreenCarSelect` (SCREENCARSELECT.CPP, 0x8003af68).
pub fn screen_car_select_destructor(c: &mut Cpu, this: u32, flags: u32) {
    c.m.w32(this + VPTR, VTABLE);
    fescreen::screen_destructor(c, Screen(this), flags);
}

pub fn abi_screen_car_select_destructor(c: &mut Cpu) {
    screen_car_select_destructor(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelect::Cleanup` (SCREENCARSELECT.CPP, 0x8003af90): the spinning cars' menu
/// cleaned up, the screen's, its videos' buffer freed (virtual).
pub fn cleanup(c: &mut Cpu, this: u32) {
    crate::psxfront::cleanup_spinning_cars_menu(c);
    fescreen::cleanup(c, Screen(this));
    call_virtual(c, this, VPTR, FREE_ASYNC_BUFFER, &[]);
}

pub fn abi_cleanup(c: &mut Cpu) {
    cleanup(c, c.r[4]);
}

/// A lerped rectangle's side: `from + fade * (to - from) / 128`.
fn lerp(m: &mut Mem, o: u32, from: u32, to: u32) -> u32 {
    let d = hs(m, o + to).wrapping_sub(hs(m, o + from)) as i32;
    let p = (hs(m, o + OV_FADE) as i32).wrapping_mul(d) / 128;
    hu(m, o + from).wrapping_add(p as u32)
}

/// A rectangle's halves (x, y, w, h), sign-extended.
fn rect_of(m: &mut Mem, rect: u32) -> [u32; 4] {
    [0, 2, 4, 6].map(|k| hs(m, rect + k))
}

fn set_rect(m: &mut Mem, rect: u32, v: [u32; 4]) {
    for (k, side) in v.into_iter().enumerate() {
        m.w16(rect + 2 * k as u32, side as u16);
    }
}

/// The menu's stats choice.
fn choice(m: &mut Mem) -> u32 {
    let all = m.r32(ALL_MENUS);
    hs(m, all + CHOICE)
}

/// One of an overlay's three marks at `rect` (lit by the car's upgrade), its digit by the clock.
fn draw_mark(c: &mut Cpu, rect: u32, options: u32, upgrades: u32, k: u32, x0: u32) {
    let m = &mut c.m;
    let bit = hs(m, UPGRADE_BITS + 2 * k);
    let lit = upgrades & bit != 0;
    m.w32(options + 4, 0xBEBE);
    let [x, y, _, _] = rect_of(m, rect);
    let digit = (m.r32(GAME_TICKS) as i32 >> 4).wrapping_rem(10) as u32;
    let shape = (10 * k).wrapping_add(digit).wrapping_add(98);
    // (top-left, coloured; semi-transparent when not lit)
    let flags = (!lit as u32) | 0x410;
    crate::psxfront::draw_shape_extended(c, shape, flags, x.wrapping_add(40 * k + x0), y.wrapping_add(6), if lit { 0 } else { 96 }, 1, options);
}

/// `tScreenCarSelect::DrawOverlay` (SCREENCARSELECT.CPP, 0x8003afd8): overlay `o` faded a step
/// and its rectangle moved between its places by the fade, then by its kind: the car's name
/// (a round rectangle), its price (bought, the garage's, used) and the money, its options'
/// marks, its stats' marks and the screen's choice with its help, the sliders; its frame.
pub fn draw_overlay(c: &mut Cpu, this: u32, o: u32) {
    if o == 0 {
        return;
    }
    zeroed(c, 40 + CarInfo::SIZE, |c, buf| {
        let (rect, inner, options, car) = (buf, buf + 8, buf + 16, buf + 40);
        let info = CarInfo(car);
        let has_car = get_car_virtual(c, this, car);
        let m = &mut c.m;
        if m.r16(o + OV_DIR) != 0 {
            let v = hu(m, o + OV_STEP).wrapping_mul(hu(m, o + OV_DIR)).wrapping_add(hu(m, o + OV_FADE)) as u16 as i16;
            m.w16(o + OV_FADE, v as u16);
            if v <= 0 || v >= 128 {
                m.w16(o + OV_FADE, if v <= 0 { 0 } else { 128 });
                m.w16(o + OV_DIR, 0);
            }
        }
        for (k, (from, to)) in [(0, 8), (2, 10), (4, 12), (6, 14)].into_iter().enumerate() {
            let v = lerp(m, o, from, to);
            m.w16(rect + 2 * k as u32, v as u16);
        }
        let [x, y, w, h] = rect_of(m, rect);
        let name = sx((info.car_id(m) as i32 as u32).wrapping_add(290));
        let upgrades = info.upgrades(m) as u32;
        let kind = hs(m, o + OV_KIND);
        match kind {
            0 => {
                if has_car {
                    drawshp::round_rectangle(c, name, rect, 0);
                }
            }
            1..=3 => {
                // the price box: the car's name, its price (bought, the garage's, used), the money
                let inner_v = [sx(x + 15), sx(y.wrapping_add(h).wrapping_sub(30)), sx(w.wrapping_sub(30)), 23];
                set_rect(m, inner, inner_v);
                let [ix, iy, iw, _] = inner_v;
                let right = ix.wrapping_add(iw).wrapping_sub(12);
                let mid = sx(ix.wrapping_add((iw as i32 >> 1) as u32));
                if has_car {
                    fetextrender::menu_text_positioned_justify(c, name, sx(right), sx(y.wrapping_add(2)), 1, 1, 3);
                }
                let m = &mut c.m;
                let title = match kind {
                    2 => 142,
                    3 => 143,
                    _ => 141,
                };
                let (price, colour) = if !has_car {
                    (0, 0x23_2323)
                } else if kind == 2 {
                    (m.r32(car + 32), 0xBEBE)
                } else if kind == 3 {
                    let k = choice(m);
                    let slot = hs(m, PRICE_SLOTS + 2 * k);
                    (m.r32((car + 32).wrapping_add(slot << 2)), 0xBEBE)
                } else {
                    let p = b(m, PLAYER_CAR) as i32;
                    (fecars::calc_used_price(m, CAR_MANAGER, p) as u32, 0xBEBE)
                };
                crate::screenpost::draw_money(c, right, iy.wrapping_add(3), 6, price, colour, 0x23_2323);
                fetextrender::menu_text_positioned_justify(c, title, mid, sx(iy.wrapping_add(3)), 1, 1, 3);
                let money = c.m.r32(MONEY);
                crate::screenpost::draw_money(c, right, iy.wrapping_add(13), 9, money, 0xBEBE, 0x23_2323);
                fetextrender::menu_text_positioned_justify(c, 124, mid, sx(iy.wrapping_add(13)), 1, 1, 3);
                drawshp::rectangle(c, inner);
                call_addr(c, DRAW_SQUARE, &[0, x, y, w, 10]);
            }
            4 => {
                for k in 0..3 {
                    draw_mark(c, rect, options, upgrades, k, 33);
                }
            }
            5 => {
                if m.r16(o + OV_FADE) == 128 {
                    for k in 0..3 {
                        draw_mark(c, rect, options, upgrades, k, 133);
                    }
                    // the choice's name, its help (a sound system's for the two cars without
                    // one) and its hint (bought, or how to buy)
                    let m = &mut c.m;
                    set_rect(m, inner, [sx(x + 30), sx(y + 35), sx(w.wrapping_sub(60)), sx(h.wrapping_sub(75))]);
                    let k = choice(m);
                    let a1 = sx(x.wrapping_add((w as i32 >> 1) as u32));
                    fetextrender::menu_text_positioned_justify(c, sx(k.wrapping_add(151)), a1, sx(y + 24), 2, 2, 3);
                    let m = &mut c.m;
                    let k = choice(m);
                    let id = info.car_id(m);
                    let help = if k == 1 && (id == 12 || id == 10) { 65 } else { k.wrapping_add(176) };
                    fetextrender::word_wrap(c, sx(help), inner, 2, 8);
                    let m = &mut c.m;
                    let k = choice(m);
                    let bit = hs(m, UPGRADE_BITS + 2 * k);
                    let hint = if upgrades & bit != 0 {
                        161
                    } else if m.r8(PAD_TYPE) == 35 {
                        160
                    } else {
                        159
                    };
                    fetextrender::menu_text_positioned_justify(c, hint, sx(x.wrapping_add(w).wrapping_sub(15)), sx(y.wrapping_add(h).wrapping_sub(20)), 1, 2, 3);
                }
                if (w as i32) >= 67 && (h as i32) >= 51 {
                    set_rect(&mut c.m, inner, [sx(x + 15), sx(y + 20), sx(w.wrapping_sub(30)), sx(h.wrapping_sub(25))]);
                    drawshp::rectangle(c, inner);
                }
            }
            6 => {
                if !has_car {
                    for k in 0..20 {
                        m.w8(car + 48 + k, 0);
                    }
                }
                draw_sliders(c, car, sx(x + 13), sx(y + 4));
            }
            _ => {}
        }
        if c.m.r16(o + OV_KIND) != 0 {
            drawshp::trans_rectangle(c, rect, 1);
        }
    });
}

pub fn abi_draw_overlay(c: &mut Cpu) {
    draw_overlay(c, c.r[4], c.r[5]);
}

/// The spline's control points (5 words each: x, y, z, w, the target), its basis matrix, the
/// camera's offsets (one per point).
const SPLINE_POINTS: u32 = 0x8005_2308;
const SPLINE_BASIS: u32 = 0x8005_23A8;
const CAMERA_OFFSETS: u32 = 0x8005_30A8;
/// The overlays each state shows (4 a state, -1 none), the wall's TV layout, the music's flag.
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

/// The overlay state `state` shows in slot `k` (-1 none).
fn state_overlay(m: &mut Mem, state: u32, k: u32) -> u32 {
    m.r8(STATE_OVERLAYS.wrapping_add(k).wrapping_add(state << 2)) as i8 as i32 as u32
}

/// Slot `k` given overlay `want`, fading in.
fn fade_in(m: &mut Mem, this: u32, k: u32, want: u32) {
    let o = (this + OVERLAYS).wrapping_add(want.wrapping_mul(OVERLAY_SIZE));
    m.w32(this + SLOTS + 4 * k, o);
    m.w16(o + OV_FADE, 0);
    m.w16(o + OV_DIR, 1);
}

/// `tScreenCarSelect::SetState` (SCREENCARSELECT.CPP, 0x8003b874): a new state: (leaving the
/// wall's states, the second player's cars and stats) the wall off and dark; each of the four
/// overlay slots kept if the state shows it, fading out if not, or given the state's overlay
/// fading in; then for the cars' states and the showrooms the clocks, the music's flag, and
/// in a showroom the music stopped, the camera's spline set off and the screen transitioned
/// off; else the wall's TVs reset, the screen transitioned on (unless from the garage) and the
/// wall on.
pub fn set_state(c: &mut Cpu, this: u32, state: u32) {
    let old = c.m.r16(this + STATE) as u32;
    if state != hs(&mut c.m, this + STATE) {
        c.m.w16(this + STATE, state as u16);
        if old.wrapping_sub(SECOND_CARS) & 0xFFFF < 2 {
            fevideowall::turn_off(&mut c.m, this + WALL);
            set_brightness(&mut c.m, this, 0, 0);
            c.m.w16(this + SHOWN_INDEX, 0xFFFF);
            c.m.w16(this + SHOWN_CAR, 0xFFFF);
        }
        let m = &mut c.m;
        for k in 0..4 {
            let ov = m.r32(this + SLOTS + 4 * k);
            let want = state_overlay(m, state, k);
            if ov != 0 {
                if hs(m, ov + OV_KIND) != want {
                    m.w16(ov + OV_DIR, 0xFFFF);
                }
            } else if (want as i32) >= 0 {
                fade_in(m, this, k, want);
            }
        }
    }
    if ![CARS, SECOND_CARS, SHOWROOM, SECOND_SHOWROOM].contains(&state) {
        return;
    }
    let m = &mut c.m;
    let showroom = state == SHOWROOM || state == SECOND_SHOWROOM;
    m.w32(this + IN_SHOWROOM, showroom as u32);
    let t = m.r32(GAME_TICKS);
    m.w32(MUSIC_FLAG, 1);
    m.w32(this + SPOKEN, 0);
    m.w32(this + SHOWROOM_START, t);
    m.w32(this + SPLINE_CLOCK, t);
    if showroom {
        call(c, STOP_SONG, &[1000]);
        let m = &mut c.m;
        m.w32(this + SPLINE_POINT, 0);
        // the first point the turn's place, the one before mirrored about the second
        let v = m.r32(this + TURN) & 0x3FF;
        m.w32(SPLINE_POINTS + 36, v);
        for k in 0..5 {
            let at = SPLINE_POINTS + 4 * k;
            let (a, b0) = (m.r32(at + 40), m.r32(at + 20));
            m.w32(at, b0.wrapping_sub(a.wrapping_sub(b0)));
        }
        for k in 0..4 {
            m.w32(CAMERA_OFFSETS + 4 * k, 0x1_0000);
        }
        fescreen::transition_off(m, Screen(this));
    } else {
        let m = &mut c.m;
        for k in 0..10 {
            m.w32(this + TVS + 48 * k, 0);
            m.w16(this + TVS + 4 + 48 * k, 0);
        }
        if old as u16 as i16 != GARAGE as i16 {
            fescreen::transition_on(m, Screen(this));
        }
        fevideowall::turn_on(m, this + WALL);
    }
}

pub fn abi_set_state(c: &mut Cpu) {
    set_state(c, c.r[4], c.r[5]);
}

/// A spline output's halving and rounding to 16.16's integer part.
fn half16(v: u32) -> u32 {
    let h = v as i32 >> 1;
    ((if h < 0 { h.wrapping_add(0xFFFF) } else { h }) >> 16) as u32
}

/// `tScreenCarSelect::CalcSplinePosition` (SCREENCARSELECT.CPP, 0x8003baf8): the camera on
/// the spline through control points `points` at `t` (of 600): its position (x, y halved,
/// z, w rounded) and its target (the four points' fifth words plus the camera's offsets) into
/// `outs`.
pub fn calc_spline_position(c: &mut Cpu, points: [u32; 4], t: u32, outs: [u32; 5]) {
    zeroed(c, 16 + 64 + 16 + 16, |c, buf| {
        let (powers, control, basis_out, out) = (buf, buf + 16, buf + 80, buf + 96);
        // (t^3, t^2, t, 1)
        let t = call(c, FIXEDDIV, &[t << 16, 0x0258_0000]);
        let t2 = call(c, FIXEDMULT, &[t, t]);
        let t3 = call(c, FIXEDMULT, &[t2, t]);
        let m = &mut c.m;
        for (k, v) in [t3, t2, t, 0x1_0000].into_iter().enumerate() {
            m.w32(powers + 4 * k as u32, v);
        }
        let point = |p: u32| SPLINE_POINTS.wrapping_add(p.wrapping_mul(20));
        for k in 0..4 {
            for (row, &p) in points.iter().enumerate() {
                let v = m.r32(point(p) + 4 * k);
                m.w32(control + 16 * row as u32 + 4 * k, v);
            }
        }
        transform_vector(c, powers, SPLINE_BASIS, basis_out);
        transform_vector(c, basis_out, control, out);
        let m = &mut c.m;
        let x = (m.r32(out) as i32 >> 1) as u32;
        m.w32(outs[0], x);
        let y = (m.r32(out + 4) as i32 >> 1) as u32;
        m.w32(outs[1], y);
        let z = half16(m.r32(out + 8));
        m.w32(outs[2], z);
        let w = half16(m.r32(out + 12));
        m.w32(outs[3], w);
        // the target: the first column the points' fifth words plus the offsets
        for (row, &p) in points.iter().enumerate() {
            let v = m.r32(point(p) + 16);
            let o = m.r32(CAMERA_OFFSETS + 4 * row as u32);
            m.w32(control + 16 * row as u32, v.wrapping_add(o));
        }
        transform_vector(c, powers, SPLINE_BASIS, basis_out);
        transform_vector(c, basis_out, control, out);
        let v = half16(c.m.r32(out));
        c.m.w32(outs[4], v);
    });
}

pub fn abi_calc_spline_position(c: &mut Cpu) {
    let a: Vec<u32> = (4..11).map(|k| crate::abi::arg(c, k)).collect();
    calc_spline_position(c, [c.r[5], c.r[6], c.r[7], a[0]], a[1], [a[2], a[3], a[4], a[5], a[6]]);
}

/// The car shown on the wall noted: its index, id and livery.
fn note_shown(m: &mut Mem, this: u32, car: u32) {
    let info = CarInfo(car);
    let v = info.car_index(m);
    m.w16(this + SHOWN_INDEX, v as u16);
    let v = info.car_id(m) as i16;
    m.w16(this + SHOWN_CAR, v as u16);
    let v = info.country(m);
    m.w16(this + SHOWN_LIVERY, v as u16);
}

/// `tScreenCarSelect::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003bdb8): 142 shapes of
/// `zcars`, 11 swapped from the car's video's file (the chosen car's, or the first stock
/// car's), which is noted as shown.
pub fn get_shape_info(c: &mut Cpu, this: u32, count: u32, swap: u32, file: u32, swap_file: u32) {
    let m = &mut c.m;
    m.w16(count, 142);
    m.w16(swap, 11);
    m.w32(file, SHAPE_FILE);
    zeroed(c, CarInfo::SIZE, |c, car| {
        if !get_car_virtual(c, this, car) {
            fecars::get_stock_car(c, CAR_MANAGER, 0, car);
        }
        note_shown(&mut c.m, this, car);
        call(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, car + 8]);
    });
    c.m.w32(swap_file, VIDEO_FILE);
}

pub fn abi_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    get_shape_info(c, c.r[4], c.r[5], c.r[6], c.r[7], swap_file);
}

/// `tScreenCarSelect::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003be7c): another car (or
/// livery) than shown: its video's file read (a real car's), noted, the wall dark and off.
pub fn update_video_wall(c: &mut Cpu, this: u32, car: u32) {
    update_wall(c, this, car, 8);
}

pub fn abi_update_video_wall(c: &mut Cpu) {
    update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelect::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003bf4c): the videos'
/// buffer (40000 bytes of the DCT's).
pub fn allocate_async_buffer(c: &mut Cpu, this: u32) {
    let v = call(c, GET_DCT_BUFFER, &[0x9C40, WALL_NAME]);
    c.m.w32(this + ASYNC_BUFFER, v);
}

pub fn abi_allocate_async_buffer(c: &mut Cpu) {
    allocate_async_buffer(c, c.r[4]);
}

/// `tScreenCarSelect::FreeAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003bf80).
pub fn free_async_buffer(c: &mut Cpu, this: u32) {
    call(c, RESET_DCT_BUFFER, &[]);
    c.m.w32(this + ASYNC_BUFFER, 0);
}

pub fn abi_free_async_buffer(c: &mut Cpu) {
    free_async_buffer(c, c.r[4]);
}

/// `tScreenCarSelect::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003bfa8): the wall's ten
/// TVs over the swap shapes, its text and icon places, its images (the shapes in).
pub fn initialize_video_wall(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let wall = this + WALL;
    let shapes = m.r32(this + SWAP_INFO);
    fevideowall::initialize(m, wall, this + TVS, shapes, 0, 10, WALL_TVS, 150);
    fevideowall::set_available_text(m, wall, 249, 320, 80);
    let icons = m.r32(this + PERMANENT);
    fevideowall::set_available_icon(m, wall, 28, 10, 310, 60, icons);
    if m.r16(this + FLAGS) & 1 != 0 {
        fevideowall::update_images(c, wall);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
}

pub fn abi_initialize_video_wall(c: &mut Cpu) {
    initialize_video_wall(c, c.r[4]);
}

/// The front end's structure, the track manager and the tournament manager, the track's kind
/// for the race, the front end's car select flag.
const FE: u32 = crate::globals::FRONT_END;
const TRACK_MANAGER: u32 = 0x8011_5D08;
const TOURNAMENT_MANAGER: u32 = 0x8011_5D90;
const TRACK_KIND: u32 = 0x8011_44E0;
const IN_CAR_SELECT: u32 = 0x8005_199C;
/// `frontEnd`'s: the players (1 two), the race's kind (2 a tournament), the slot, the stock
/// cars, the garage's, the second's, the options, the opponents.
const PLAYERS: u32 = 3;
const RACE_KIND: u32 = 4;
const SLOT: u32 = 70;
const TRACKS: u32 = 35;
const STOCK_CAR: u32 = 92;
const GARAGE_CAR: u32 = 291;
const SECOND_CAR: u32 = 296;
const OPTIONS_ON: u32 = 298;
const OPPONENTS: u32 = 798;

/// `tScreenCarSelect::Initialize` (SCREENCARSELECT.CPP, 0x8003c060): the race's track's kind
/// (a tournament's track to race, or the chosen one), the menu's tournament flag, the
/// screen's, the videos' buffer (virtual), the licence plate, the shown car noted (none: -1),
/// the brightness and the clocks, the wall initialized (virtual), the overlays' fades and
/// slots reset.
pub fn initialize(c: &mut Cpu, this: u32) {
    let kind = zeroed(c, 48, |c, track| {
        if c.m.r8(FE + RACE_KIND) == 2 {
            crate::fetourn::get_track_to_race(c, TOURNAMENT_MANAGER, track);
            let id = c.m.r8(track) as i8 as i32 as u32;
            let t = crate::fetracks::get_track_by_id(&mut c.m, TRACK_MANAGER, id);
            b(&mut c.m, t + 1)
        } else {
            let m = &mut c.m;
            let slot = b(m, FE + SLOT);
            let k = b(m, FE + slot + TRACKS);
            crate::fetracks::get_track(c, TRACK_MANAGER, k, track);
            b(&mut c.m, track + 1)
        }
    });
    let m = &mut c.m;
    m.w32(TRACK_KIND, kind);
    let all = m.r32(ALL_MENUS);
    m.w32(IN_CAR_SELECT, 1);
    let tournament = (m.r8(FE + RACE_KIND) == 2) as u32;
    let v = (m.r32(all + PINK_ITEM) & !1) | tournament;
    m.w32(all + PINK_ITEM, v);
    fescreen::initialize(c, Screen(this));
    call_virtual(c, this, VPTR, ALLOCATE_ASYNC_BUFFER, &[]);
    crate::front::set_license_plate(c);
    let m = &mut c.m;
    m.w32(this + IMAGES_LOADED, 0);
    m.w32(this + TURN, 0);
    m.w32(this + IN_SHOWROOM, 0);
    zeroed(c, CarInfo::SIZE, |c, car| {
        if get_car_virtual(c, this, car) {
            note_shown(&mut c.m, this, car);
        } else {
            for at in [SHOWN_INDEX, SHOWN_LIVERY, SHOWN_CAR] {
                c.m.w16(this + at, 0xFFFF);
            }
        }
    });
    let m = &mut c.m;
    let t = m.r32(GAME_TICKS);
    for at in [BRIGHTNESS, BRIGHTNESS + 2, TARGET, TARGET + 2] {
        m.w16(this + at, 0);
    }
    m.w32(this + SPLINE_CLOCK, t);
    m.w32(this + 896, t.wrapping_sub(256));
    m.w32(this + DARK_SINCE, t.wrapping_sub(256));
    call_virtual(c, this, VPTR, INITIALIZE_VIDEO_WALL, &[]);
    let m = &mut c.m;
    for k in 0..7 {
        let o = this + OVERLAYS + OVERLAY_SIZE * k;
        m.w16(o + OV_FADE, 0);
        m.w16(o + OV_DIR, 0);
    }
    for k in 0..4 {
        m.w32(this + SLOTS + 4 * k, 0);
    }
}

pub fn abi_initialize(c: &mut Cpu) {
    initialize(c, c.r[4]);
}

/// The menu's keys: a move, triangle.
const KEY_MOVE: u32 = 8;
const KEY_BACK: u32 = 16;

/// `tScreenCarSelect::ProcessInput` (SCREENCARSELECT.CPP, 0x8003c268): on a move, the
/// player's buy item's word (buy, a bonus car's, a police car's) and its enabling (the car
/// buyable), the race item off for one opponent or two players; on triangle, the state back
/// (the showroom to the cars, the second's showroom to its cars, the second's cars, the stats
/// and the race to the garage but with two players).
pub fn process_input(c: &mut Cpu, this: u32, key: u32) {
    if c.m.r32(key) == KEY_MOVE {
        zeroed(c, CarInfo::SIZE, |c, car| {
            let ok = get_car_virtual(c, this, car);
            let m = &mut c.m;
            let all = m.r32(ALL_MENUS);
            let item = if app_player(m) == 0 { all + 8216 } else { all + 8312 };
            m.w32(item + 4, 268);
            let v = m.r32(item) & !1;
            m.w32(item, v);
            if ok {
                let info = CarInfo(car);
                let id = info.car_id(m);
                if id == 8 || id == 24 {
                    m.w32(item + 4, 269);
                }
                if id == 1 {
                    m.w32(item + 4, 270);
                }
                if info.abs_available(m) == 0 {
                    m.w32(item, v | 1);
                }
            }
            if m.r8(FE + OPPONENTS) == 1 || m.r8(FE + PLAYERS) == 1 {
                let v = m.r32(all + RACE_ITEM) | 1;
                m.w32(all + RACE_ITEM, v);
            }
        });
    }
    let m = &mut c.m;
    if m.r32(key) != KEY_BACK {
        return;
    }
    let to = match hs(m, this + STATE) {
        SHOWROOM => CARS,
        SECOND_SHOWROOM => SECOND_CARS,
        SECOND_CARS..=RACE if m.r8(FE + PLAYERS) != 1 => GARAGE,
        _ => return,
    };
    set_state(c, this, to);
}

pub fn abi_process_input(c: &mut Cpu) {
    process_input(c, c.r[4], c.r[6]);
}

/// `tScreenCarSelect::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003c424): the background, the
/// first overlay, the wall's images (once its shapes are in) or off while the screen fades, the
/// wall updated, valid with a car, its availability the car's, drawn.
pub fn draw_video_wall(c: &mut Cpu, this: u32) {
    let (ok, available) = zeroed(c, CarInfo::SIZE, |c, car| {
        let ok = get_car_virtual(c, this, car);
        (ok, CarInfo(car).available(&mut c.m) as u32)
    });
    let icons = c.m.r32(this + PERMANENT);
    fescreen::draw_background_image(c, Screen(this), 0, 28, crate::types::TextureShapeInfo(icons), 150);
    let o = c.m.r32(this + SLOTS);
    draw_overlay(c, this, o);
    let wall = this + WALL;
    if c.m.r16(this + FLAGS) & 1 != 0 && c.m.r32(this + IMAGES_LOADED) == 0 {
        fevideowall::update_images(c, wall);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
    let m = &mut c.m;
    if (hs(m, this + FADE) as i32) > 0 && m.r32(this + TRANSITION) != 0 {
        fevideowall::turn_off_instant(m, wall);
        set_brightness(m, this, 0, 0);
    }
    fevideowall::update(m, wall);
    fevideowall::set_valid(m, wall, ok as u32);
    fevideowall::set_available(m, wall, available);
    fevideowall::draw(c, wall);
}

pub fn abi_draw_video_wall(c: &mut Cpu) {
    draw_video_wall(c, c.r[4]);
}

/// The front end's byte for the car's id at `at` (its colour, its livery).
fn by_car(m: &mut Mem, car: u32, at: u32) -> u8 {
    let id = CarInfo(car).car_id(m) as i32 as u32;
    m.r8(FE.wrapping_add(id) + at)
}

/// The front end's colour choices and liveries (by car id).
const COLOURS: u32 = 96;
const LIVERIES: u32 = 192;

/// `tScreenCarSelect::GetCar` (SCREENCARSELECT.CPP, 0x8003c530): the car the state shows: the
/// player's chosen stock car (its colour if a real one, its livery), the second's (bought), a
/// test drive's garage car (owned ones needed with the options), else the garage's (some
/// owned, or a single race; a tournament's needs its cars, but in its stats the player's car):
/// its colour then the car's own index of it; whether there's one.
pub fn get_car(c: &mut Cpu, this: u32, car: u32) -> bool {
    let info = CarInfo(car);
    let state = hs(&mut c.m, this + STATE);
    match state {
        CARS | SHOWROOM => {
            let id = b(&mut c.m, FE + STOCK_CAR);
            fecars::get_stock_car(c, CAR_MANAGER, id, car);
            let m = &mut c.m;
            if (id as i32) < m.r32(CAR_MANAGER.0) as i32 {
                let v = by_car(m, car, COLOURS);
                info.set_color(m, v);
            }
            let v = by_car(m, car, LIVERIES);
            info.set_country(m, v);
        }
        SECOND_CARS | SECOND_SHOWROOM => {
            let id = b(&mut c.m, FE + SECOND_CAR);
            fecars::get_stock_car(c, CAR_MANAGER, id, car);
            let m = &mut c.m;
            let v = by_car(m, car, COLOURS);
            info.set_available(m, 1);
            info.set_color(m, v);
        }
        TEST_DRIVE => {
            let m = &mut c.m;
            let options = m.r8(FE + OPTIONS_ON);
            if options == 1 && fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as i16 <= 0 {
                return false;
            }
            let id = if options == 0 { b(m, FE + STOCK_CAR) } else { b(m, FE + GARAGE_CAR) };
            fecars::get_stock_car(c, CAR_MANAGER, id, car);
            let m = &mut c.m;
            if m.r8(FE + OPTIONS_ON) == 0 {
                let v = by_car(m, car, COLOURS);
                info.set_color(m, v);
            }
            let v = by_car(m, car, LIVERIES);
            info.set_country(m, v);
        }
        _ => {
            let m = &mut c.m;
            if fecars::get_num_owned_cars(m, CAR_MANAGER, 0) as i16 <= 0 && m.r8(FE + RACE_KIND) != 1 {
                return false;
            }
            if m.r8(FE + RACE_KIND) == 2 && state != STATS && fecars::get_num_tourney_cars(c, CAR_MANAGER, 0) as i16 <= 0 {
                return false;
            }
            let m = &mut c.m;
            let id = if hs(m, this + STATE) == STATS { b(m, PLAYER_CAR) } else { b(m, FE + GARAGE_CAR) };
            fecars::get_stock_car(c, CAR_MANAGER, id, car);
            let m = &mut c.m;
            let v = by_car(m, car, LIVERIES);
            info.set_country(m, v);
        }
    }
    // the colour choice made the car's own colour
    let m = &mut c.m;
    let k = info.color(m) as u32;
    let v = m.r8(car + k + 175);
    info.set_color(m, v);
    true
}

pub fn abi_get_car(c: &mut Cpu) {
    c.r[2] = get_car(c, c.r[4], c.r[5]) as u32;
}

/// `tScreenCarSelect::SetBrightness` (SCREENCARSELECT.CPP, 0x8003c7b0): wall `k`'s brightness
/// to head for.
pub fn set_brightness(m: &mut Mem, this: u32, v: u32, k: u32) {
    let at = this.wrapping_add(sx(k) << 1);
    m.w16(at + TARGET, v as u16);
}

pub fn abi_set_brightness(c: &mut Cpu) {
    set_brightness(&mut c.m, c.r[4], c.r[5], c.r[6]);
}

/// `tScreenCarSelect::UpdateBrightness` (SCREENCARSELECT.CPP, 0x8003c7c4): wall `k`'s
/// brightness a step (8) toward its target, not past it. Brightening: the target; dimming:
/// whether it overshot.
pub fn update_brightness(m: &mut Mem, this: u32, k: u32) -> u32 {
    let at = this.wrapping_add(sx(k) << 1);
    let (target, cur) = (hs(m, at + TARGET) as i32, hs(m, at + BRIGHTNESS) as i32);
    let raw = hu(m, at + BRIGHTNESS);
    if cur < target {
        let v = raw.wrapping_add(8);
        m.w16(at + BRIGHTNESS, v as u16);
        if target < sx(v) as i32 {
            m.w16(at + BRIGHTNESS, target as u16);
        }
        target as u16 as u32
    } else if target < cur {
        let v = raw.wrapping_sub(8);
        m.w16(at + BRIGHTNESS, v as u16);
        let past = (sx(v) as i32) < target;
        if past {
            m.w16(at + BRIGHTNESS, target as u16);
        }
        past as u32
    } else {
        0
    }
}

pub fn abi_update_brightness(c: &mut Cpu) {
    c.r[2] = update_brightness(&mut c.m, c.r[4], c.r[5]);
}

/// The sliders' labels and their stats' offsets in the car's info; the commentary's word
/// kinds (6 bytes each, the text type at +4); the showroom's flag.
const SLIDER_LABELS: u32 = 0x8005_23FC;
const SLIDER_STATS: u32 = 0x8005_23E8;
const WORD_KINDS: u32 = 0x8005_1AA0;
const SHOWROOM_DRAW: u32 = 0x8005_1748;

/// Hooked (the host captures its primitives): called by address, never `nfs4rc::f_80024c4c`.
const DRAW_SLIDER: u32 = 0x8002_4C4C;
const FMUL: fn(&mut Cpu) = nfs4rc::f_800f3e0c;
const I2F: fn(&mut Cpu) = nfs4rc::f_800f6b80;
const WORD_FLAGS: fn(&mut Cpu) = nfs4rc::f_800b9f38;
const WORD_X: fn(&mut Cpu) = nfs4rc::f_800b9f58;
const WORD_Y: fn(&mut Cpu) = nfs4rc::f_800b9f78;
const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
const GETTICK: fn(&mut Cpu) = nfs4rc::f_800e8e0c;

/// Whether the car object of `player` is busy (loading its car).
fn car_busy(m: &mut Mem, player: u32) -> bool {
    let o = m.r32(CAR_OBJECTS + 4 * player);
    m.r32(o + 2264) != 0
}

/// Whether the wall has been dark long enough to light (128 ticks).
fn dark_long(m: &mut Mem, this: u32) -> bool {
    m.r32(GAME_TICKS).wrapping_sub(m.r32(this + DARK_SINCE)) as i32 >= 129
}

/// `tScreenCarSelect::DrawBackground` (SCREENCARSELECT.CPP, 0x8003c848): with a car, its
/// video's shapes uploaded once read (the car object idle, a while after the last change), the
/// wall on and lit (bright for a car with its video, dim else); the video wall (virtual) while
/// the screen fades in.
pub fn draw_background(c: &mut Cpu, this: u32) {
    zeroed(c, CarInfo::SIZE, |c, car| {
        if !get_car_virtual(c, this, car) {
            return;
        }
        let info = crate::types::ShapeInformation(this + SWAP_INFO);
        fescreen::is_shape_file_loaded(c, Screen(this), info);
        let m = &mut c.m;
        let ready = m.r32(this + SWAP_READ) != 0
            && hs(m, this + WALL + 20) != u32::MAX
            && { let p = app_player(m); !car_busy(m, p) }
            && dark_long(m, this);
        if ready {
            fescreen::upload_swap_shapes(c, Screen(this), 11);
            let m = &mut c.m;
            fevideowall::turn_on(m, this + WALL);
            if m.r16(this + BRIGHTNESS) == m.r16(this + TARGET) {
                let v = if CarInfo(car).available(m) != 0 { 128 } else { 32 };
                set_brightness(m, this, v, 0);
            }
        }
    });
    if (hs(&mut c.m, this + FADE) as i32) < 128 {
        call_virtual(c, this, VPTR, DRAW_VIDEO_WALL, &[0]);
    }
}

pub fn abi_draw_background(c: &mut Cpu) {
    draw_background(c, c.r[4]);
}

/// `tScreenCarSelect::DrawSliders` (SCREENCARSELECT.CPP, 0x8003c978): the car's five stats as
/// sliders under their labels, each with its upgrades' (the car's three upgrade bits) added.
pub fn draw_sliders(c: &mut Cpu, car: u32, x: u32, y: u32) {
    let x = sx(x);
    let mut row = y;
    for k in 0..5 {
        let m = &mut c.m;
        let label = hs(m, SLIDER_LABELS + 2 * k);
        fetextrender::menu_text_positioned(c, label, x, sx(row.wrapping_add(4)), 0, 14);
        let m = &mut c.m;
        let at = car.wrapping_add(m.r32(SLIDER_STATS + 4 * k));
        let upgrades = CarInfo(car).upgrades(m) as u32;
        let mut v = b(m, at + 48);
        for (bit, o) in [(1, 53), (2, 58), (4, 63)] {
            if upgrades & bit != 0 {
                v = v.wrapping_add(b(m, at + o));
            }
        }
        call_addr(c, DRAW_SLIDER, &[sx(v), 0, 11, x, sx(row), 104, 3, 7, 3, 0, 0, 128, 0]);
        row = row.wrapping_add(15);
    }
}

pub fn abi_draw_sliders(c: &mut Cpu) {
    draw_sliders(c, c.r[5], c.r[6], c.r[7]);
}

/// The overlay slots updated for the state (as `SetState`, an overlay fading out freed once
/// out).
fn update_slots(m: &mut Mem, this: u32) {
    for k in 0..4 {
        let slot = this + SLOTS + 4 * k;
        let ov = m.r32(slot);
        if ov != 0 {
            let want = { let st = hs(m, this + STATE); state_overlay(m, st, k) };
            if hs(m, ov + OV_KIND) == want {
                continue;
            }
            m.w16(ov + OV_DIR, 0xFFFF);
            if (hs(m, ov + OV_FADE) as i32) > 0 {
                continue;
            }
            m.w32(slot, 0);
        }
        let want = { let st = hs(m, this + STATE); state_overlay(m, st, k) };
        if (want as i32) >= 0 {
            fade_in(m, this, k, want);
        }
    }
}

/// `tScreenCarSelect::DrawForeground` (SCREENCARSELECT.CPP, 0x8003cae4): the state's menu
/// items enabled for the car (buy, a tournament's car, the garage's sell and the pink slips'
/// colour), the overlays' arrow, the overlays updated and drawn, the wall updated for the car
/// (virtual) and its brightness; the car turning on its stand, or in the showroom flown round
/// by the camera's spline with the commentator's words (and its speech once) over it; the wall
/// lit a while after the car object's idle.
pub fn draw_foreground(c: &mut Cpu, this: u32) {
    zeroed(c, CarInfo::SIZE + 24 + 20, |c, car| {
        let info = CarInfo(car);
        let m = &mut c.m;
        let app = m.r32(FE_APP);
        let stack = m.r32(app + 4);
        let n = m.r32(stack + 8);
        let menu = m.r32(stack.wrapping_add(n << 2) + 16);
        let ok = get_car_virtual(c, this, car);
        let m = &mut c.m;
        let all = m.r32(ALL_MENUS);
        let v = m.r32(all + RACE_ITEM) | 1;
        m.w32(all + RACE_ITEM, v);
        // the arrow: on the state's own menu
        let mut arrow = false;
        match hs(m, this + STATE) {
            GARAGE => {
                arrow = menu == all + 4840;
                let v = m.r32(all + 4924) & !1;
                m.w32(all + 4924, v);
                if m.r8(FE + RACE_KIND) == 2 && m.r8(FE + 290) == 0 && crate::fecheats::is_cheat_enabled(m, 13) {
                    let v = m.r32(all + RACE_ITEM) & !1;
                    m.w32(all + RACE_ITEM, v);
                }
                if ok && info.car_class(m) < 5 {
                    set_overlay_dir(m, this, 4, 1);
                } else {
                    let v = m.r32(all + 4924) | 1;
                    m.w32(all + 4924, v);
                    set_overlay_dir(m, this, 4, 0xFFFF);
                }
            }
            CARS => {
                arrow = menu == all + 4564;
                let on = (info.car_class(m) == 7) as u32;
                for at in [4604, 4644] {
                    let v = (m.r32(all + at) & !1) | on;
                    m.w32(all + at, v);
                }
            }
            SECOND_CARS => arrow = menu == all + 7328,
            STATS => {
                arrow = menu == all + 7648;
                let v = m.r32(all + 7688) & !1;
                m.w32(all + 7688, v | !ok as u32);
                set_overlay_dir(m, this, 4, if ok { 1 } else { 0xFFFF });
            }
            RACE => {
                arrow = true;
                set_overlay_dir(m, this, 5, 1);
                set_overlay_dir(m, this, 3, 1);
            }
            _ => {}
        }
        let arrow = arrow && ok;
        set_overlay_dir(m, this, 6, if arrow { 1 } else { 0xFFFF });
        update_slots(m, this);
        for k in 1..4 {
            let o = c.m.r32(this + SLOTS + 4 * k);
            draw_overlay(c, this, o);
        }
        if !ok {
            c.m.w8(car, 0xFF);
        }
        call_virtual(c, this, VPTR, UPDATE_VIDEO_WALL, &[car]);
        let m = &mut c.m;
        let p = app_player(m);
        if car_busy(m, p) {
            set_brightness(m, this, 0, 0);
            fevideowall::turn_off(m, this + WALL);
            let t = m.r32(GAME_TICKS);
            m.w32(this + DARK_SINCE, t);
        }
        update_brightness(m, this, 0);
        if (m.r16(this + STATE) as u32).wrapping_sub(SHOWROOM) >= 2 {
            m.w32(SHOWROOM_DRAW, 0);
            let turn = m.r32(this + TURN).wrapping_add(3);
            m.w32(this + TURN, turn);
            let facet = b(m, this + BRIGHTNESS);
            call_addr(c, DRAW_CAR, &[car, 314, 84, 0x4080_0000, 0xC0F0_0000, facet, 1, turn, 0]);
        } else {
            showroom(c, this, car, car + CarInfo::SIZE, ok);
        }
        let m = &mut c.m;
        if !car_busy(m, 0)
            && m.r16(this + BRIGHTNESS) == m.r16(this + TARGET)
            && m.r16(this + BRIGHTNESS) == 0
            && dark_long(m, this)
        {
            let v = if info.available(m) != 0 { 128 } else { 32 };
            set_brightness(m, this, v, 0);
            fevideowall::turn_on(m, this + WALL);
        }
    });
}

pub fn abi_draw_foreground(c: &mut Cpu) {
    draw_foreground(c, c.r[4]);
}

/// The showroom's part of `DrawForeground` (`buf`: room for the drawing's options and the
/// spline's outputs): the commentator's words a while in (19 per car, each 512 ticks, faded
/// in and out), the screen's background fading in, the camera's spline's four points (a step
/// on every 600 ticks, wrapping the seven with an offset past them), the car flown round on
/// it, the commentary's speech once.
fn showroom(c: &mut Cpu, this: u32, car: u32, buf: u32, ok: bool) {
    let (options, outs) = (buf, buf + 24);
    let info = CarInfo(car);
    let m = &mut c.m;
    let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWROOM_START)).wrapping_sub(256) as i32;
    for k in 0..5 {
        m.w32(outs + 4 * k, 0);
    }
    let speech = info.speech_car_id(m) as i32;
    if since >= 0 && speech >= 0 {
        let n = since >> 9;
        let word = ((n % 19) + 997 + speech * 19) as u32;
        let phase = (since - (n << 9)) as u32;
        let kind = call(c, WORD_FLAGS, &[sx(word)]);
        let m = &mut c.m;
        let t = b(m, WORD_KINDS.wrapping_add(kind.wrapping_mul(6)) + 4);
        let mut colour = m.r32(TEXT_COLORS.wrapping_add(t << 2));
        if phase < 128 {
            colour = crate::fefades::fade_out(colour, 128 - phase);
        } else if 512 - phase < 128 {
            colour = crate::fefades::fade_out(colour, phase - 384);
        }
        let text = call(c, TEXT_WORD, &[word]);
        let x = sx(call(c, WORD_X, &[word]));
        let y = sx(call(c, WORD_Y, &[word]));
        fetextrender::full_text_rgb(c, text, x, y, colour, 0, 0);
    }
    let m = &mut c.m;
    m.w32(options + 4, 0x55_1E00);
    let shapes = m.r32(this + SWAP_INFO);
    m.w32(options + 20, shapes);
    let t = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SHOWROOM_START)).wrapping_sub(128) as i32;
    let fade = t.clamp(0, 128);
    let fade = (128 - fade) as u32;
    if m.r16(this + FLAGS) & 1 != 0 && (fade as i32) < 128 {
        // the background (its own table)
        crate::psxfront::draw_shape_extended(c, 10, 512, 0, 0, fade, 0, options);
    }
    // the spline's clock: a point on every 600 ticks (past the seventh back to the third)
    let m = &mut c.m;
    let mut t = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + SPLINE_CLOCK));
    while t as i32 >= 601 {
        let v = m.r32(this + SPLINE_CLOCK).wrapping_add(600);
        m.w32(this + SPLINE_CLOCK, v);
        let k = m.r32(this + SPLINE_POINT);
        let next = if (k.wrapping_add(1) as i32) < 7 { k.wrapping_add(1) } else { k.wrapping_sub(4) };
        m.w32(this + SPLINE_POINT, next);
        t = t.wrapping_sub(600);
    }
    // the four points from it (wrapping, each wrap a turn further: the offsets)
    let mut points = [0u32; 4];
    let mut p = m.r16(this + SPLINE_POINT) as u32;
    m.w32(CAMERA_OFFSETS, 0);
    if sx(p) as i32 >= 7 {
        p = p.wrapping_sub(5);
        m.w32(CAMERA_OFFSETS, 0x400_0000);
    }
    points[0] = p;
    for k in 1..4u32 {
        let off = m.r32(CAMERA_OFFSETS + 4 * (k - 1));
        m.w32(CAMERA_OFFSETS + 4 * k, off);
        let prev = p;
        p = prev.wrapping_add(1);
        if sx(p) as i32 >= 7 {
            p = prev.wrapping_sub(4);
            m.w32(CAMERA_OFFSETS + 4 * k, off.wrapping_add(0x400_0000));
        }
        points[k as usize] = p;
    }
    let outs5 = [outs, outs + 4, outs + 8, outs + 12, outs + 16];
    calc_spline_position(c, points.map(sx), t, outs5);
    c.m.w32(SHOWROOM_DRAW, 1);
    let to_float = |c: &mut Cpu, v: u32| {
        let f = call(c, I2F, &[v]);
        call(c, FMUL, &[f, 0x3780_0000])
    };
    let x = c.m.r32(outs);
    let fx = to_float(c, x);
    let y = c.m.r32(outs + 4);
    let fy = to_float(c, y);
    let m = &mut c.m;
    let (a1, a2, facet, turn) = (hs(m, outs + 8), hs(m, outs + 12), b(m, this + BRIGHTNESS), m.r32(outs + 16));
    call_addr(c, DRAW_CAR, &[car, a1, a2, fx, fy, facet, 1, turn, 0]);
    if ok {
        let now = call(c, GETTICK, &[]);
        let m = &mut c.m;
        if now.wrapping_sub(m.r32(this + SHOWROOM_START)) >= 641 && m.r32(this + SPOKEN) == 0 && info.speech_car_id(m) >= 0 {
            m.w32(this + SPOKEN, 1);
            let speech = info.speech_car_id(m) as i32 as u32;
            crate::feaudio::async_play_speech(c, 0, speech);
        }
    }
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
        let cars = m.r32(CAR_MANAGER.0);
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

    /// With no car, `DrawVideoWall` gives the wall the `fAvailable` of the car it didn't get:
    /// the original's stack leftover (the readable port's buffer is cleared). The screen at `screen` is put in the cars' state (its
    /// stock car always there) when its `GetCar` fails.
    pub(super) fn wall_needs_a_car(c: &mut Cpu, screen: u32) {
        if crate::abi::call(c, nfs4rc::f_8003c530, &[screen, OUTS + 0x40]) == 0 {
            c.m.w16(screen + 284, 0);
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
                wall_needs_a_car(c, SCREEN);
                marks_need_a_car(c);
                vec![SCREEN, 0]
            }),
            ("tScreenCarSelect::GetCar", 0x8003_c530, nfs4rc::f_8003c530, |_, _| vec![SCREEN, OUTS + 0x40]),
            ("tScreenCarSelect::SetBrightness", 0x8003_c7b0, nfs4rc::f_8003c7b0, |_, r| vec![SCREEN, pick(r, 200), pick(r, 2)]),
            ("tScreenCarSelect::UpdateBrightness", 0x8003_c7c4, nfs4rc::f_8003c7c4, |_, r| vec![SCREEN, pick(r, 2)]),
            ("tScreenCarSelect::DrawBackground", 0x8003_c848, nfs4rc::f_8003c848, |c, _| {
                wall_needs_a_car(c, SCREEN);
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
            ("tScreenCarSelect::DrawForeground", 0x8003_cae4, nfs4rc::f_8003cae4, |c, _| {
                // (and with no car, `UpdateVideoWall` notes the livery of the car not got)
                wall_needs_a_car(c, SCREEN);
                vec![SCREEN]
            }),
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

/// The subclasses' fields: the duel's opponent's wall (+828; its TVs +532, its shapes'
/// information +936 and file read +940, its videos' buffer +944, its flags +954), the car it
/// shows (+928, its index) and its images loaded (+932); two players' dialog (+928).
const THEIR_WALL: u32 = 828;
const THEIR_TVS: u32 = 532;
const THEIR_SHAPES: u32 = 936;
const THEIR_BUFFER: u32 = 944;
const THEIR_FLAGS: u32 = 954;
const THEIR_SHOWN: u32 = 928;
const THEIR_IMAGES_LOADED: u32 = 932;
const DIALOG: u32 = 928;
/// The opponent's brightness (the second wall's) and target, when its wall went dark.
const THEIR_BRIGHTNESS: u32 = BRIGHTNESS + 2;
const THEIR_TARGET: u32 = TARGET + 2;
const THEIR_DARK_SINCE: u32 = 896;

/// The subclasses' virtual methods (their slots): the duel's `DrawOpponentVideoWall`, two
/// players' `TurnOffVideoWall`; two players' `SetDialog`.
const SUBCLASS_WALL: u32 = 128;
const SET_DIALOG: u32 = 136;

/// `tDialog::Display`, `tDialog::Hide` (hooked: by address).
const DIALOG_DISPLAY: u32 = 0x8001_8980;
const DIALOG_HIDE: u32 = 0x8001_8A44;

/// A car's shape file's name into `name` (`"z"` and its small name).
fn shape_file_name(c: &mut Cpu, name: u32, car: u32) {
    call(c, SPRINTF, &[name, Z_FORMAT, car + 16]);
}

/// The car's video's file read if the shown car changed (`UpdateVideoWall`'s, the file's name
/// at `name` in the car's info), the wall dark and off.
fn update_wall(c: &mut Cpu, this: u32, car: u32, name: u32) {
    let m = &mut c.m;
    let info = CarInfo(car);
    let changed = info.car_index(m) as u32 != hs(m, this + SHOWN_INDEX)
        || info.car_id(m) as i32 as u32 != hs(m, this + SHOWN_CAR)
        || (info.car_class(m) == 7 && hs(m, this + SHOWN_LIVERY) != info.country(m) as u32);
    if !changed {
        return;
    }
    if info.car_id(m) >= 0 {
        fescreen::async_load_swap_shape_file(c, Screen(this), car + name);
    }
    let m = &mut c.m;
    note_shown(m, this, car);
    m.w32(this + IMAGES_LOADED, 0);
    set_brightness(m, this, 0, 0);
    fevideowall::turn_off(m, this + WALL);
}

/// `tScreenCarSelectDuel::PreLoad` (SCREENCARSELECT.CPP, 0x8003d3bc): the opponent's shapes'
/// information, the screen's preload, two videos' buffers (16000 bytes each), the player's car's
/// (or the first stock car's) shapes and the opponent's read.
pub fn duel_pre_load(c: &mut Cpu, this: u32) {
    let theirs = crate::types::ShapeInformation(this + THEIR_SHAPES);
    c.m.w32(theirs.0, 0);
    fescreen::initialize_shapes(c, theirs, 5);
    fescreen::pre_load(c, Screen(this));
    let v = call(c, GET_DCT_BUFFER, &[16000, WALL_NAME]);
    c.m.w32(this + ASYNC_BUFFER, v);
    let v = call(c, GET_DCT_BUFFER, &[16000, OPPONENT_NAME]);
    c.m.w32(this + THEIR_BUFFER, v);
    zeroed(c, CarInfo::SIZE + 16, |c, car| {
        let name = car + CarInfo::SIZE;
        if !get_car_virtual(c, this, car) {
            fecars::get_stock_car(c, CAR_MANAGER, 0, car);
        }
        shape_file_name(c, name, car);
        fescreen::async_load_shape_file(c, Screen(this), name, crate::types::ShapeInformation(this + SWAP_INFO));
        let id = b(&mut c.m, OPPONENT_CAR);
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        shape_file_name(c, name, car);
        fescreen::async_load_shape_file(c, Screen(this), name, theirs);
    });
    c.m.w32(this + THEIR_IMAGES_LOADED, 0);
}

pub fn abi_duel_pre_load(c: &mut Cpu) {
    duel_pre_load(c, c.r[4]);
}

/// `tScreenCarSelectDuel::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003d4e4): nothing (the
/// preload has them).
pub fn abi_duel_allocate_async_buffer(_c: &mut Cpu) {}

/// `tScreenCarSelectDuel::FreeAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003d4ec): both videos'.
pub fn duel_free_async_buffer(c: &mut Cpu, this: u32) {
    call(c, RESET_DCT_BUFFER, &[]);
    c.m.w32(this + ASYNC_BUFFER, 0);
    c.m.w32(this + THEIR_BUFFER, 0);
}

pub fn abi_duel_free_async_buffer(c: &mut Cpu) {
    duel_free_async_buffer(c, c.r[4]);
}

/// `tScreenCarSelectDuel::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003d518): two walls
/// of five TVs, the player's and the opponent's (moved down), their images once loaded (the
/// opponent's text placed on the player's wall, as the original does).
pub fn duel_initialize_video_wall(c: &mut Cpu, this: u32) {
    let (mine, theirs) = (this + WALL, this + THEIR_WALL);
    let m = &mut c.m;
    let shapes = m.r32(this + SWAP_INFO);
    fevideowall::initialize(m, mine, this + TVS, shapes, 0, 5, DUEL_TVS, 0);
    fevideowall::set_available_text(m, mine, 249, 270, 45);
    if m.r16(this + FLAGS) & 1 != 0 {
        fevideowall::set_offset(m, mine, 6, 0);
        fevideowall::update_images(c, mine);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
    let m = &mut c.m;
    let shapes = m.r32(this + THEIR_SHAPES);
    fevideowall::initialize(m, theirs, this + THEIR_TVS, shapes, 0, 5, DUEL_TVS, 0);
    fevideowall::set_available_text(m, mine, 249, 270, 150);
    if m.r16(this + THEIR_FLAGS) & 1 != 0 {
        fevideowall::set_offset(m, theirs, 6, 105);
        fevideowall::update_images(c, theirs);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
}

pub fn abi_duel_initialize_video_wall(c: &mut Cpu) {
    duel_initialize_video_wall(c, c.r[4]);
}

/// `tScreenCarSelectDuel::Initialize` (SCREENCARSELECT.CPP, 0x8003d634): the car select's, in
/// the cars' state, not the car select's flag.
pub fn duel_initialize(c: &mut Cpu, this: u32) {
    initialize(c, this);
    c.m.w16(this + STATE, CARS as u16);
    c.m.w32(IN_CAR_SELECT, 0);
}

pub fn abi_duel_initialize(c: &mut Cpu) {
    duel_initialize(c, c.r[4]);
}

/// `tScreenCarSelectDuel::Cleanup` (SCREENCARSELECT.CPP, 0x8003d664): the opponent's shapes
/// freed, the car select's.
pub fn duel_cleanup(c: &mut Cpu, this: u32) {
    fescreen::free_shapes(c, Screen(this), crate::types::ShapeInformation(this + THEIR_SHAPES));
    cleanup(c, this);
}

pub fn abi_duel_cleanup(c: &mut Cpu) {
    duel_cleanup(c, c.r[4]);
}

/// A half's wall's background shapes (12) faded with the screen and raised by `dy`.
fn wall_shapes(c: &mut Cpu, this: u32, dy: u32) {
    for k in 0..12 {
        let fade = hs(&mut c.m, this + FADE);
        crate::psxfront::draw_shape_extended(c, k, 0, 0, 0u32.wrapping_sub(dy), fade, 0, 0);
    }
}

/// The wall dark and off at once while the screen fades (wall `k`).
fn off_while_fading(m: &mut Mem, this: u32, wall: u32, k: u32) {
    if (hs(m, this + FADE) as i32) > 0 && m.r32(this + TRANSITION) != 0 {
        fevideowall::turn_off_instant(m, wall);
        set_brightness(m, this, 0, k);
    }
}

/// `tScreenCarSelectDuel::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003d694): the wall's
/// background raised by `dy`, the wall off while the screen fades, its images once loaded
/// (offset), updated, valid with a car, drawn.
pub fn duel_draw_video_wall(c: &mut Cpu, this: u32, dy: u32) {
    let (ok, available) = zeroed(c, CarInfo::SIZE, |c, car| {
        let ok = get_car_virtual(c, this, car);
        (ok, CarInfo(car).available(&mut c.m) as u32)
    });
    wall_shapes(c, this, sx(dy));
    let mine = this + WALL;
    off_while_fading(&mut c.m, this, mine, 0);
    if c.m.r16(this + FLAGS) & 1 != 0 && c.m.r32(this + IMAGES_LOADED) == 0 {
        fevideowall::set_offset(&mut c.m, mine, 6, 0);
        fevideowall::update_images(c, mine);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
    let m = &mut c.m;
    fevideowall::update(m, mine);
    fevideowall::set_valid(m, mine, ok as u32);
    fevideowall::set_available(m, mine, available);
    fevideowall::draw(c, mine);
}

pub fn abi_duel_draw_video_wall(c: &mut Cpu) {
    duel_draw_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelectDuel::DrawOpponentVideoWall` (SCREENCARSELECT.CPP, 0x8003d7ec): the
/// opponent's wall as `DrawVideoWall` the player's (always valid and available).
pub fn duel_draw_opponent_video_wall(c: &mut Cpu, this: u32, dy: u32) {
    wall_shapes(c, this, sx(dy));
    let theirs = this + THEIR_WALL;
    off_while_fading(&mut c.m, this, theirs, 1);
    if c.m.r16(this + FLAGS) & 1 != 0 && c.m.r32(this + THEIR_IMAGES_LOADED) == 0 {
        fevideowall::set_offset(&mut c.m, theirs, 6, 105);
        fevideowall::update_images(c, theirs);
        c.m.w32(this + THEIR_IMAGES_LOADED, 1);
    }
    let m = &mut c.m;
    fevideowall::update(m, theirs);
    fevideowall::set_valid(m, theirs, 1);
    fevideowall::set_available(m, theirs, 1);
    fevideowall::draw(c, theirs);
}

pub fn abi_duel_draw_opponent_video_wall(c: &mut Cpu) {
    duel_draw_opponent_video_wall(c, c.r[4], c.r[5]);
}

/// The player's car (or the first stock car) into `car`, noted as shown.
fn shown_or_first(c: &mut Cpu, this: u32, car: u32) {
    if !get_car_virtual(c, this, car) {
        fecars::get_stock_car(c, CAR_MANAGER, 0, car);
    }
    note_shown(&mut c.m, this, car);
}

/// `tScreenCarSelectDuel::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003d910): 52 shapes of
/// `zDuel` and 5 swapped (no file), the car shown noted, no opponent's.
pub fn duel_get_shape_info(c: &mut Cpu, this: u32, count: u32, swap: u32, file: u32, swap_file: u32) {
    c.m.w16(count, 52);
    c.m.w16(swap, 5);
    zeroed(c, CarInfo::SIZE, |c, car| shown_or_first(c, this, car));
    let m = &mut c.m;
    m.w16(this + THEIR_SHOWN, 0xFFFF);
    m.w32(file, DUEL_FILE);
    m.w32(swap_file, 0);
}

pub fn abi_duel_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    duel_get_shape_info(c, c.r[4], c.r[5], c.r[6], c.r[7], swap_file);
}

/// `tScreenCarSelectDuel::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003d9cc): as the car
/// select's, the video's file the car's small name.
pub fn duel_update_video_wall(c: &mut Cpu, this: u32, car: u32) {
    update_wall(c, this, car, 16);
}

pub fn abi_duel_update_video_wall(c: &mut Cpu) {
    duel_update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelectDuel::UpdateOpponentVideoWall` (SCREENCARSELECT.CPP, 0x8003da9c): another
/// opponent's car: its shapes read, its wall dark and off.
pub fn duel_update_opponent_video_wall(c: &mut Cpu, this: u32, car: u32) {
    let index = CarInfo(car).car_index(&mut c.m) as u32;
    if index == hs(&mut c.m, this + THEIR_SHOWN) {
        return;
    }
    zeroed(c, 16, |c, name| {
        shape_file_name(c, name, car);
        fescreen::async_load_shape_file(c, Screen(this), name, crate::types::ShapeInformation(this + THEIR_SHAPES));
    });
    let m = &mut c.m;
    m.w32(this + THEIR_IMAGES_LOADED, 0);
    m.w16(this + THEIR_SHOWN, index as u16);
    set_brightness(m, this, 0, 1);
    fevideowall::turn_off(m, this + THEIR_WALL);
}

pub fn abi_duel_update_opponent_video_wall(c: &mut Cpu) {
    duel_update_opponent_video_wall(c, c.r[4], c.r[5]);
}

/// The drawing environments' buffers, the screen's height; the duel's slider labels.
const DRAW_ENV_A: u32 = 0x8013_E6B0;
const DRAW_ENV_B: u32 = 0x8013_EA9C;
const SCREEN_HEIGHT: u32 = 0x8013_EF6C;
const DUEL_LABELS: u32 = 0x8005_2408;
const GET_DRAWENV: fn(&mut Cpu) = nfs4rc::f_800beb84;
/// Hooked (the host records the clipping): called by address.
const SET_DRAW_AREA: u32 = 0x800E_EC70;

/// A drawing area primitive (12 bytes) linked into the OT for the band at `y`, `h` high.
fn set_draw_area(c: &mut Cpu, y: u32, h: u32) {
    zeroed(c, 8, |c, rect| {
        let m = &mut c.m;
        set_rect(m, rect, [0, y, 512, h]);
        let prim = crate::mmeffect::add_prim(m, 12);
        call_addr(c, SET_DRAW_AREA, &[prim, rect]);
    });
}

/// The drawing environment's top (the current buffer's).
fn draw_env_top(c: &mut Cpu) -> u32 {
    let (ea, eb) = (c.m.r32(DRAW_ENV_A), c.m.r32(DRAW_ENV_B));
    let env = call(c, GET_DRAWENV, &[ea, eb]);
    hu(&mut c.m, env + 2)
}

/// Whether a wall's video shapes can go up: read (the file read at `info + 4`), a shown car's
/// (`shown`), the car object of `player` idle, a while after the wall's last change (`since`).
fn wall_ready(m: &mut Mem, this: u32, info: u32, shown: u32, player: u32, since: u32) -> bool {
    m.r32(this + info + 4) != 0
        && hs(m, this + shown) != u32::MAX
        && !car_busy(m, player)
        && m.r32(GAME_TICKS).wrapping_sub(m.r32(this + since)) as i32 >= 129
}

/// The cars' camera in the halves: the distance back, the height up (floats).
const HALF_DISTANCE: u32 = 0x3FD9_999A;
const HALF_HEIGHT: u32 = 0xC11E_6666;
/// The cars' list iterators in the menus: the opponent's, the player's stock and garage cars.
const OPPONENT_LIST: u32 = 5216;
const STOCK_LIST: u32 = 4460;
const GARAGE_LIST: u32 = 4812;

/// `tScreenCarSelectDuel::DrawBackground` (SCREENCARSELECT.CPP, 0x8003db20): the duel's two
/// halves: the opponent's (its name's plate, its car in its default colour and its wall
/// updated, its shapes up once read, its brightness, its car turning, its wall), the player's
/// below (its band, its car's name's plate, the wall updated (virtual), brightness, car,
/// shapes); the video wall (virtual).
pub fn duel_draw_background(c: &mut Cpu, this: u32) {
    let top = draw_env_top(c);
    c.m.w16(this + STATE, TEST_DRIVE as u16);
    let height = c.m.r32(SCREEN_HEIGHT);
    set_draw_area(c, top, height);
    zeroed(c, 8 + CarInfo::SIZE, |c, plate| {
        let car = plate + 8;
        let info = CarInfo(car);
        // the opponent's half
        let m = &mut c.m;
        set_rect(m, plate, [290, 130, 200, 12]);
        let all = m.r32(ALL_MENUS);
        let name = fecars::list_iterator_car_text_value(m, all + OPPONENT_LIST, u32::MAX);
        drawshp::round_rectangle(c, sx(name), plate, 0);
        let id = b(&mut c.m, OPPONENT_CAR);
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        let m = &mut c.m;
        let k = info.default_color(m) as u32;
        let v = m.r8(car + k + 175);
        info.set_color(m, v);
        duel_update_opponent_video_wall(c, this, car);
        fescreen::is_shape_file_loaded(c, Screen(this), crate::types::ShapeInformation(this + THEIR_SHAPES));
        let theirs = this + THEIR_WALL;
        if wall_ready(&mut c.m, this, THEIR_SHAPES, THEIR_WALL + 20, 1, THEIR_DARK_SINCE) {
            fescreen::upload_shapes(c, Screen(this), crate::types::ShapeInformation(this + THEIR_SHAPES), 0, 65, 5, 0);
            let m = &mut c.m;
            m.w32(this + THEIR_IMAGES_LOADED, 0);
            fevideowall::turn_on(m, theirs);
            if m.r16(this + THEIR_BRIGHTNESS) == m.r16(this + THEIR_TARGET) {
                set_brightness(m, this, 128, 1);
            }
        }
        let m = &mut c.m;
        let turn = m.r32(this + TURN).wrapping_add(3);
        m.w32(this + TURN, turn);
        info.set_upgrades(m, 0);
        if car_busy(m, 1) && dark_long_since(m, this, THEIR_DARK_SINCE) {
            set_brightness(m, this, 0, 1);
            fevideowall::turn_off(m, theirs);
            let t = m.r32(GAME_TICKS);
            m.w32(this + THEIR_DARK_SINCE, t);
        }
        update_brightness(m, this, 1);
        m.w32(SHOWROOM_DRAW, 0);
        let facet = b(m, this + THEIR_BRIGHTNESS);
        call_addr(c, DRAW_CAR, &[car, 278, 184, HALF_DISTANCE, HALF_HEIGHT, facet, 0, turn, 1]);
        let m = &mut c.m;
        if !car_busy(m, 1)
            && m.r16(this + THEIR_BRIGHTNESS) == m.r16(this + THEIR_TARGET)
            && m.r16(this + THEIR_BRIGHTNESS) == 0
            && dark_long_since(m, this, THEIR_DARK_SINCE)
        {
            set_brightness(m, this, 128, 1);
            fevideowall::turn_on(m, theirs);
        }
        call_virtual(c, this, VPTR, SUBCLASS_WALL, &[105]);
        // the player's half
        let height = c.m.r32(SCREEN_HEIGHT);
        set_draw_area(c, top.wrapping_add(128), height.wrapping_sub(128));
        let half = ((c.m.r32(SCREEN_HEIGHT) as i32) / 2) as u32;
        call_addr(c, DRAW_SQUARE, &[0, 0, half, 512, half]);
        let ok = get_car_virtual(c, this, car);
        if ok {
            let m = &mut c.m;
            m.w16(plate + 2, 25);
            let all = m.r32(ALL_MENUS);
            let list = all + if m.r8(FE + OPTIONS_ON) == 0 { STOCK_LIST } else { GARAGE_LIST };
            let name = fecars::list_iterator_car_text_value(m, list, u32::MAX);
            drawshp::round_rectangle(c, sx(name), plate, 0);
        } else {
            c.m.w8(car, 0xFF);
        }
        call_virtual(c, this, VPTR, UPDATE_VIDEO_WALL, &[car]);
        let mine = this + WALL;
        let m = &mut c.m;
        if car_busy(m, 0) && dark_long(m, this) {
            set_brightness(m, this, 0, 0);
            fevideowall::turn_off(m, mine);
            let t = m.r32(GAME_TICKS);
            m.w32(this + DARK_SINCE, t);
        }
        update_brightness(m, this, 0);
        m.w32(SHOWROOM_DRAW, 0);
        let (facet, turn) = (b(m, this + BRIGHTNESS), m.r32(this + TURN));
        call_addr(c, DRAW_CAR, &[car, 278, 79, HALF_DISTANCE, HALF_HEIGHT, facet, 0, turn, 0]);
        let m = &mut c.m;
        let lit = if info.available(m) != 0 { 128 } else { 32 };
        if !car_busy(m, 0) && m.r16(this + BRIGHTNESS) == m.r16(this + TARGET) && m.r16(this + BRIGHTNESS) == 0 && dark_long(m, this) {
            set_brightness(m, this, lit, 0);
            fevideowall::turn_on(m, mine);
        }
        if ok {
            fescreen::is_shape_file_loaded(c, Screen(this), crate::types::ShapeInformation(this + SWAP_INFO));
            if wall_ready(&mut c.m, this, SWAP_INFO, WALL + 20, 0, DARK_SINCE) {
                fescreen::upload_swap_shapes(c, Screen(this), 5);
                let m = &mut c.m;
                fevideowall::turn_on(m, mine);
                if m.r16(this + TARGET) == m.r16(this + BRIGHTNESS) {
                    set_brightness(m, this, lit, 0);
                }
            }
        }
    });
    call_virtual(c, this, VPTR, DRAW_VIDEO_WALL, &[0]);
}

pub fn abi_duel_draw_background(c: &mut Cpu) {
    duel_draw_background(c, c.r[4]);
}

/// Whether wall's dark clock at `since` is 128 ticks behind.
fn dark_long_since(m: &mut Mem, this: u32, since: u32) -> bool {
    m.r32(GAME_TICKS).wrapping_sub(m.r32(this + since)) as i32 >= 129
}

/// A car's stat with its upgrades (the car's three upgrade bits), as the sliders show it.
fn stat(m: &mut Mem, car: u32, k: u32) -> u32 {
    let at = car.wrapping_add(m.r32(SLIDER_STATS + 4 * k));
    let upgrades = CarInfo(car).upgrades(m) as u32;
    let mut v = b(m, at + 48);
    for (bit, o) in [(1, 53), (2, 58), (4, 63)] {
        if upgrades & bit != 0 {
            v = v.wrapping_add(b(m, at + o));
        }
    }
    v
}

/// A half's five stats as sliders (empty without a car) from `y` down, their labels typed `ty`.
fn half_sliders(c: &mut Cpu, car: Option<u32>, y: u32, ty: u32) {
    let mut y = y;
    for k in 0..5 {
        let label = hs(&mut c.m, DUEL_LABELS + 2 * k);
        fetextrender::menu_text_positioned_justify(c, label, 500, sx(y + 4), 1, 0, ty);
        let v = match car {
            Some(car) => sx(stat(&mut c.m, car, k)),
            None => 0,
        };
        call_addr(c, DRAW_SLIDER, &[v, 0, 11, 417, sx(y), 73, 3, 4, 3, 1, 0, 128, 0]);
        y += 15;
    }
}

/// `tScreenCarSelectDuel::DrawForeground` (SCREENCARSELECT.CPP, 0x8003e198): the player's
/// car's five stats as sliders (empty without a car), then the opponent's below (empty
/// likewise without the player's).
pub fn duel_draw_foreground(c: &mut Cpu, this: u32) {
    zeroed(c, CarInfo::SIZE, |c, car| {
        let ok = get_car_virtual(c, this, car);
        half_sliders(c, ok.then_some(car), 45, 4);
        let id = b(&mut c.m, OPPONENT_CAR);
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        half_sliders(c, ok.then_some(car), 150, 4);
    });
}

pub fn abi_duel_draw_foreground(c: &mut Cpu) {
    duel_draw_foreground(c, c.r[4]);
}

/// `"zcarsb"`.
const TWO_PLAYER_FILE: u32 = 0x8001_1AD8;

/// `tScreenCarSelectTwoPlayer::GetCar` (SCREENCARSELECT.CPP, 0x8003e370): the player's car:
/// a stock car (its colour choice), or a garage car (the first player's garage while the
/// player has none; none at all: no car); its colour and livery.
pub fn two_player_get_car(c: &mut Cpu, car: u32) -> bool {
    let info = CarInfo(car);
    let p = app_player(&mut c.m);
    let k = if c.m.r8(FE + OPTIONS_ON) == 0 {
        let id = b(&mut c.m, FE + p + STOCK_CAR);
        fecars::get_stock_car(c, CAR_MANAGER, id, car);
        let m = &mut c.m;
        let id = info.car_id(m) as i32 as u32;
        b(m, id.wrapping_add(p * 48).wrapping_add(FE) + COLOURS)
    } else {
        let m = &mut c.m;
        let who = if fecars::get_num_owned_cars(m, CAR_MANAGER, p) as i16 > 0 { p } else { 0 };
        if fecars::get_num_owned_cars(m, CAR_MANAGER, who) as i16 <= 0 {
            return false;
        }
        let id = b(m, FE + p + GARAGE_CAR);
        fecars::get_garage_car(c, CAR_MANAGER, id, car, who);
        info.color(&mut c.m) as u32
    };
    let m = &mut c.m;
    let v = m.r8(car + k + 175);
    info.set_color(m, v);
    let id = info.car_id(m) as i32 as u32;
    let v = m.r8(id.wrapping_add(p * 48).wrapping_add(FE) + LIVERIES);
    info.set_country(m, v);
    true
}

pub fn abi_two_player_get_car(c: &mut Cpu) {
    c.r[2] = two_player_get_car(c, c.r[5]) as u32;
}

/// `tScreenCarSelectTwoPlayer::DrawVideoWall` (SCREENCARSELECT.CPP, 0x8003e4c0): the wall's
/// background raised by `dy`, the player's wall's images once loaded (the second player's
/// moved down, its text there), off while the screen fades, valid with a car, drawn.
pub fn two_player_draw_video_wall(c: &mut Cpu, this: u32, dy: u32) {
    let (ok, available) = zeroed(c, CarInfo::SIZE, |c, car| {
        let ok = get_car_virtual(c, this, car);
        (ok, CarInfo(car).available(&mut c.m) as u32)
    });
    wall_shapes(c, this, sx(dy));
    let wall = this + WALL;
    let m = &mut c.m;
    if m.r16(this + FLAGS) & 1 != 0 && m.r32(this + IMAGES_LOADED) == 0 {
        let second = app_player(m) != 0;
        fevideowall::set_offset(m, wall, 6, if second { 105 } else { 0 });
        fevideowall::set_available_text(m, wall, 249, 270, if second { 150 } else { 45 });
        fevideowall::update_images(c, wall);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
    let m = &mut c.m;
    off_while_fading(m, this, wall, 0);
    fevideowall::set_valid(m, wall, ok as u32);
    fevideowall::set_available(m, wall, available);
    fevideowall::update(m, wall);
    fevideowall::draw(c, wall);
}

pub fn abi_two_player_draw_video_wall(c: &mut Cpu) {
    two_player_draw_video_wall(c, c.r[4], c.r[5]);
}

/// No car shown.
fn none_shown(m: &mut Mem, this: u32) {
    for at in [SHOWN_INDEX, SHOWN_CAR, SHOWN_LIVERY] {
        m.w16(this + at, 0xFFFF);
    }
}

/// `tScreenCarSelectTwoPlayer::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003e660): 52 shapes of
/// `zcarsb`, 5 swapped: the first player's car's video's (noted as shown), none for the
/// second.
pub fn two_player_get_shape_info(c: &mut Cpu, this: u32, count: u32, swap: u32, file: u32, swap_file: u32) {
    let m = &mut c.m;
    m.w16(count, 52);
    m.w16(swap, 5);
    none_shown(m, this);
    m.w32(file, TWO_PLAYER_FILE);
    let video = zeroed(c, CarInfo::SIZE, |c, car| {
        if get_car_virtual(c, this, car) && app_player(&mut c.m) == 0 {
            note_shown(&mut c.m, this, car);
            call(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, car + 16]);
            VIDEO_FILE
        } else {
            0
        }
    });
    c.m.w32(swap_file, video);
}

pub fn abi_two_player_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    two_player_get_shape_info(c, c.r[4], c.r[5], c.r[6], c.r[7], swap_file);
}

/// `tScreenCarSelectTwoPlayer::UpdateVideoWall` (SCREENCARSELECT.CPP, 0x8003e748): as the
/// duel's.
pub fn two_player_update_video_wall(c: &mut Cpu, this: u32, car: u32) {
    update_wall(c, this, car, 16);
}

pub fn abi_two_player_update_video_wall(c: &mut Cpu) {
    two_player_update_video_wall(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelectTwoPlayer::TurnOffVideoWall` (SCREENCARSELECT.CPP, 0x8003e81c).
pub fn two_player_turn_off_video_wall(m: &mut Mem, this: u32) {
    fevideowall::turn_off_instant(m, this + WALL);
}

pub fn abi_two_player_turn_off_video_wall(c: &mut Cpu) {
    two_player_turn_off_video_wall(&mut c.m, c.r[4]);
}

/// The waiting message's buffer.
const WAIT_TEXT: u32 = 0x8005_30E8;

/// `tScreenCarSelectTwoPlayer::DrawBackground` (SCREENCARSELECT.CPP, 0x8003e83c): the
/// player's half (the first's at the top, the second's below): the car's name's plate, the wall
/// updated (virtual), brightness and car turning; none: the car's stand, the wall off
/// (virtual); the car's video's shapes up once read; the video wall and the dialog (virtual);
/// the drawing area clipped to the half.
pub fn two_player_draw_background(c: &mut Cpu, this: u32) {
    let top = draw_env_top(c);
    let height = c.m.r32(SCREEN_HEIGHT);
    set_draw_area(c, top, height);
    let second = app_player(&mut c.m) == 1;
    zeroed(c, 8 + CarInfo::SIZE, |c, plate| {
        let car = plate + 8;
        let info = CarInfo(car);
        set_rect(&mut c.m, plate, [290, if second { 130 } else { 25 }, 200, 12]);
        let p = app_player(&mut c.m);
        if get_car_virtual(c, this, car) {
            let m = &mut c.m;
            m.w16(plate + 2, if second { 128 } else { 20 });
            let turn = m.r32(this + TURN).wrapping_add(3);
            m.w32(this + TURN, turn);
            let name = (info.car_id(m) as i32 as u32).wrapping_add(290);
            drawshp::round_rectangle(c, name, plate, 0);
            call_virtual(c, this, VPTR, UPDATE_VIDEO_WALL, &[car]);
            let m = &mut c.m;
            if car_busy(m, p) {
                set_brightness(m, this, 0, 0);
                let t = m.r32(GAME_TICKS);
                m.w32(this + DARK_SINCE, t);
            }
            if !car_busy(m, p) && m.r16(this + BRIGHTNESS) == m.r16(this + TARGET) && m.r16(this + BRIGHTNESS) == 0 && dark_long(m, this) {
                let v = if info.available(m) != 0 { 128 } else { 32 };
                set_brightness(m, this, v, 0);
                fevideowall::turn_on(m, this + WALL);
            }
            update_brightness(m, this, 0);
            m.w32(SHOWROOM_DRAW, 0);
            let facet = b(m, this + BRIGHTNESS);
            let y = if second { 184 } else { 79 };
            call_addr(c, DRAW_CAR, &[car, 278, y, HALF_DISTANCE, HALF_HEIGHT, facet, 0, turn, p]);
        } else {
            c.m.w8(car, 0xFF);
            call_virtual(c, this, VPTR, UPDATE_VIDEO_WALL, &[car]);
            let m = &mut c.m;
            m.w32(SHOWROOM_DRAW, 0);
            let (facet, turn) = (b(m, this + BRIGHTNESS), m.r32(this + TURN));
            call_addr(c, DRAW_CAR, &[car, 278, 79, HALF_DISTANCE, HALF_HEIGHT, facet, 0, turn, p]);
            call_virtual(c, this, VPTR, SUBCLASS_WALL, &[]);
        }
        fescreen::is_shape_file_loaded(c, Screen(this), crate::types::ShapeInformation(this + SWAP_INFO));
        let p = app_player(&mut c.m);
        if wall_ready(&mut c.m, this, SWAP_INFO, WALL + 20, p, DARK_SINCE) {
            let first = if second { 65 } else { 0 };
            fescreen::upload_shapes(c, Screen(this), crate::types::ShapeInformation(this + SWAP_INFO), 0, first, 5, 0);
            let m = &mut c.m;
            fevideowall::turn_on(m, this + WALL);
            if m.r16(this + TARGET) == m.r16(this + BRIGHTNESS) {
                let v = if info.available(m) != 0 { 128 } else { 32 };
                set_brightness(m, this, v, 0);
            }
        }
    });
    let dy = if second { 105 } else { 0 };
    call_virtual(c, this, VPTR, DRAW_VIDEO_WALL, &[dy]);
    call_virtual(c, this, VPTR, SET_DIALOG, &[]);
    let half = ((c.m.r32(SCREEN_HEIGHT) as i32) / 2) as u32;
    set_draw_area(c, if second { top.wrapping_add(half) } else { top }, half);
}

pub fn abi_two_player_draw_background(c: &mut Cpu) {
    two_player_draw_background(c, c.r[4]);
}

/// `tScreenCarSelectTwoPlayer::DrawForeground` (SCREENCARSELECT.CPP, 0x8003ed94): the
/// player's colour item off but for a cop car, its car's five stats as sliders in its half.
pub fn two_player_draw_foreground(c: &mut Cpu, this: u32) {
    zeroed(c, CarInfo::SIZE, |c, car| {
        let ok = get_car_virtual(c, this, car);
        let m = &mut c.m;
        let p = app_player(m);
        let all = m.r32(ALL_MENUS);
        let item = all + if p == 0 { 5784 } else { 6032 };
        let off = !ok || CarInfo(car).car_class(m) == 7;
        let v = (m.r32(item) & !1) | off as u32;
        m.w32(item, v);
        half_sliders(c, ok.then_some(car), if p == 1 { 150 } else { 45 }, 14);
    });
}

pub fn abi_two_player_draw_foreground(c: &mut Cpu) {
    two_player_draw_foreground(c, c.r[4]);
}

/// The dialog placed for player `p` (off to its side).
fn place_dialog(m: &mut Mem, d: u32, p: u32) {
    m.w16(d + 124, 0);
    m.w16(d + 126, if p != 0 { 60 } else { 0xFFC4 });
    m.w16(d + 100, p as u16);
}

/// The dialog shown waiting for the other player (by name).
fn show_waiting(c: &mut Cpu, d: u32, p: u32) {
    let format = call(c, TEXT_WORD, &[681]);
    let name = crate::front::player_name(c, 1u32.wrapping_sub(p));
    call(c, SPRINTF, &[WAIT_TEXT, format, name]);
    c.m.w32(d + 144, WAIT_TEXT);
    call_addr(c, DIALOG_DISPLAY, &[d]);
}

/// Whether player `p` has chosen (the application's +560, a word each).
fn has_chosen(m: &mut Mem, p: u32) -> bool {
    let app = m.r32(FE_APP);
    m.r32(app + 560 + 4 * p) != 0
}

/// `tScreenCarSelectTwoPlayer::SetDialog` (SCREENCARSELECT.CPP, 0x8003efd0): while the player
/// has chosen, the dialog waiting for the other player (by name), off to the side; else hidden.
pub fn two_player_set_dialog(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let p = app_player(m);
    let d = this + DIALOG;
    if !has_chosen(m, p) {
        call_addr(c, DIALOG_HIDE, &[d]);
        return;
    }
    place_dialog(m, d, p);
    show_waiting(c, d, p);
}

pub fn abi_two_player_set_dialog(c: &mut Cpu) {
    two_player_set_dialog(c, c.r[4]);
}

/// `tScreenCarSelectTwoPlayer::AllocateAsyncBuffer` (SCREENCARSELECT.CPP, 0x8003f090): the
/// video's buffer (16000 bytes).
pub fn two_player_allocate_async_buffer(c: &mut Cpu, this: u32) {
    let v = call(c, GET_DCT_BUFFER, &[16000, WALL_NAME]);
    c.m.w32(this + ASYNC_BUFFER, v);
}

pub fn abi_two_player_allocate_async_buffer(c: &mut Cpu) {
    two_player_allocate_async_buffer(c, c.r[4]);
}

/// `tScreenCarSelectTwoPlayer::InitializeVideoWall` (SCREENCARSELECT.CPP, 0x8003f0c4): a wall
/// of five TVs, its images once loaded.
pub fn two_player_initialize_video_wall(c: &mut Cpu, this: u32) {
    let wall = this + WALL;
    let m = &mut c.m;
    let shapes = m.r32(this + SWAP_INFO);
    fevideowall::initialize(m, wall, this + TVS, shapes, 0, 5, DUEL_TVS, 0);
    if m.r16(this + FLAGS) & 1 != 0 {
        fevideowall::update_images(c, wall);
        c.m.w32(this + IMAGES_LOADED, 1);
    }
}

pub fn abi_two_player_initialize_video_wall(c: &mut Cpu) {
    two_player_initialize_video_wall(c, c.r[4]);
}

/// `tScreenCarSelectTwoPlayer::Initialize` (SCREENCARSELECT.CPP, 0x8003f140): the car
/// select's; the second player's shows no car yet, the first's wall's text placed; the cars'
/// state, the walls dark, not the car select's flag.
pub fn two_player_initialize(c: &mut Cpu, this: u32) {
    initialize(c, this);
    let m = &mut c.m;
    if app_player(m) == 1 {
        none_shown(m, this);
    } else {
        fevideowall::set_available_text(m, this + WALL, 249, 270, 45);
    }
    for at in [STATE, THEIR_TARGET, TARGET, THEIR_BRIGHTNESS, BRIGHTNESS] {
        m.w16(this + at, 0);
    }
    m.w32(IN_CAR_SELECT, 0);
}

pub fn abi_two_player_initialize(c: &mut Cpu) {
    two_player_initialize(c, c.r[4]);
}

/// `tScreenCarSelectTwoPlayer::Cleanup` (SCREENCARSELECT.CPP, 0x8003f1c0): the dialog hidden,
/// the car select's.
pub fn two_player_cleanup(c: &mut Cpu, this: u32) {
    call_addr(c, DIALOG_HIDE, &[this + DIALOG]);
    cleanup(c, this);
}

pub fn abi_two_player_cleanup(c: &mut Cpu) {
    two_player_cleanup(c, c.r[4]);
}

/// The pink slips' players' load states: to load, the load's errors (1..3), too few cars (4),
/// too many (5), ready (6), leaving (7), loading (8); a save going on (FEMEMCARD's);
/// `"zHACK2"`; the application's redraw (hooked).
const PINK_STATES: u32 = 0x8005_24C4;
const PINK_TO_LOAD: u32 = 0;
const PINK_TOO_FEW: u32 = 4;
const PINK_FULL: u32 = 5;
const PINK_READY: u32 = 6;
const PINK_LEAVING: u32 = 7;
const PINK_LOADING: u32 = 8;
const CARD_ACTIVE: u32 = 0x8005_1F04;
const HACK_FILE: u32 = 0x8001_1B04;
const REDRAW: u32 = 0x8001_3894;
/// The pink slips screen's: the dialog's shown flag (+1060), frames since it was (+1080), the
/// card's record (+1084), the message's clock (+1088), an error shown (+1092), leaving (+1096).
const DIALOG_SHOWN: u32 = DIALOG + 132;
const DIALOG_FRAMES: u32 = 1080;
const CARD_INFO: u32 = 1084;
const MESSAGE_SINCE: u32 = 1088;
const ERROR_SHOWN: u32 = 1092;
const LEAVING: u32 = 1096;
/// The pink slips' car list's iterator in the menus; each player's money in the front end.
const PINK_LIST: u32 = 6612;
const PLAYER_MONEY: u32 = 76;
const PINK_CARS: u32 = 293;

fn pink_state(m: &mut Mem, p: u32) -> u32 {
    m.r32(PINK_STATES + 4 * p)
}

fn both_ready(m: &mut Mem) -> bool {
    pink_state(m, 0) == PINK_READY && pink_state(m, 1) == PINK_READY
}

/// `tScreenPinkSlipsCarSelect::GetCar` (SCREENCARSELECT.CPP, 0x8003f1f0): with both players'
/// cars loaded, the player's pink slips car (its colour).
pub fn pink_get_car(c: &mut Cpu, car: u32) -> bool {
    if !both_ready(&mut c.m) {
        return false;
    }
    let p = app_player(&mut c.m);
    let id = b(&mut c.m, FE + p + PINK_CARS);
    fecars::get_pink_slips_car(c, CAR_MANAGER, id, car, p);
    let m = &mut c.m;
    let info = CarInfo(car);
    let k = info.color(m) as u32;
    let v = m.r8(car + k + 175);
    info.set_color(m, v);
    true
}

pub fn abi_pink_get_car(c: &mut Cpu) {
    c.r[2] = pink_get_car(c, c.r[5]) as u32;
}

/// `tScreenPinkSlipsCarSelect::DrawBackground` (SCREENCARSELECT.CPP, 0x8003f280): the cards
/// handled, two players', the dialog hidden once leaving.
pub fn pink_draw_background(c: &mut Cpu, this: u32) {
    pink_do_mem_card_stuff(c, this);
    two_player_draw_background(c, this);
    if c.m.r32(this + LEAVING) != 0 {
        call_addr(c, DIALOG_HIDE, &[this + DIALOG]);
    }
}

pub fn abi_pink_draw_background(c: &mut Cpu) {
    pink_draw_background(c, c.r[4]);
}

/// `tScreenPinkSlipsCarSelect::DoMemCardStuff` (SCREENCARSELECT.CPP, 0x8003f2c4): the player's
/// card watched (the second's once the first's is ready) a few frames after the dialog's in:
/// gone (to load again), or a game loaded from it (`LoadGame`, the frame redrawn twice first):
/// its pink slips cars counted (all 32: full; two or more, or one and money for the cheapest
/// car: ready; else too few), or the load's error; ready: the car list's iterator refreshed;
/// both ready: the card layer down, the licence plates set.
pub fn pink_do_mem_card_stuff(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let p = app_player(m);
    let card = if p != 0 { 5 } else { 1 };
    if m.r32(this + LEAVING) != 0 || (p == 1 && pink_state(m, 0) != PINK_READY) {
        return;
    }
    if pink_state(m, 0) == PINK_LOADING || pink_state(m, 1) == PINK_LOADING {
        return;
    }
    let mine = PINK_STATES + 4 * p;
    if m.r32(mine) == PINK_READY {
        return;
    }
    if m.r32(this + DIALOG_SHOWN) != 1 {
        m.w32(this + DIALOG_FRAMES, 0);
        return;
    }
    let frames = m.r32(this + DIALOG_FRAMES);
    if (frames as i32) < 5 {
        m.w32(this + DIALOG_FRAMES, frames.wrapping_add(1));
        return;
    }
    crate::memcard::handlecardevents(c, card);
    let m = &mut c.m;
    let info = crate::memcard::getcard(m, card);
    m.w32(this + CARD_INFO, info);
    if m.r32(info) == u32::MAX {
        m.w32(mine, PINK_TO_LOAD);
    } else if m.r32(CARD_ACTIVE) == 0 && m.r32(mine) == PINK_TO_LOAD {
        m.w32(mine, PINK_LOADING);
        for _ in 0..2 {
            let app = c.m.r32(FE_APP);
            call_addr(c, REDRAW, &[app]);
        }
        let state = match crate::fememcard::load_game(c, p, true, false) as i16 {
            0 => {
                let m = &mut c.m;
                let n = fecars::get_num_pink_slips_cars(m, CAR_MANAGER, p) as i16;
                if n == 32 {
                    PINK_FULL
                } else {
                    let enough = n >= 2
                        || (n == 1 && (m.r32(FE + PLAYER_MONEY + 4 * p) as i32) >= fecars::cheapest_car_stock_price(m, CAR_MANAGER));
                    if enough { PINK_READY } else { PINK_TOO_FEW }
                }
            }
            1 => 1,
            2 => 3,
            _ => 2,
        };
        c.m.w32(mine, state);
    }
    if c.m.r32(mine) == PINK_READY {
        let list = c.m.r32(ALL_MENUS) + PINK_LIST;
        fecars::list_iterator_car_decrement(c, list, p);
        let list = c.m.r32(ALL_MENUS) + PINK_LIST;
        fecars::list_iterator_car_increment(c, list, p);
    }
    if both_ready(&mut c.m) {
        crate::fememcard::deinit_memcard(c);
        crate::front::set_license_plate(c);
    }
}

pub fn abi_pink_do_mem_card_stuff(c: &mut Cpu) {
    pink_do_mem_card_stuff(c, c.r[4]);
}

/// `tScreenPinkSlipsCarSelect::DrawForeground` (SCREENCARSELECT.CPP, 0x8003f5c8): two players'.
pub fn abi_pink_draw_foreground(c: &mut Cpu) {
    two_player_draw_foreground(c, c.r[4]);
}

/// `tScreenPinkSlipsCarSelect::Initialize` (SCREENCARSELECT.CPP, 0x8003f5e8): nothing loaded,
/// the first card's record, the card layer started, two players' screen, its wall's images
/// not in.
pub fn pink_initialize(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    for at in [DIALOG_FRAMES, MESSAGE_SINCE, ERROR_SHOWN] {
        m.w32(this + at, 0);
    }
    m.w32(PINK_STATES, PINK_TO_LOAD);
    m.w32(PINK_STATES + 4, PINK_TO_LOAD);
    let info = crate::memcard::getcard(m, 1);
    m.w32(this + CARD_INFO, info);
    crate::fememcard::init_memcard(c, true, true);
    c.m.w32(this + LEAVING, 0);
    two_player_initialize(c, this);
    let f = c.m.r16(this + FLAGS) & !1;
    c.m.w16(this + FLAGS, f);
}

pub fn abi_pink_initialize(c: &mut Cpu) {
    pink_initialize(c, c.r[4]);
}

/// Both players leaving.
fn pink_leave(m: &mut Mem, this: u32) {
    m.w32(this + LEAVING, 1);
    m.w32(PINK_STATES, PINK_LEAVING);
    m.w32(PINK_STATES + 4, PINK_LEAVING);
}

/// `tScreenPinkSlipsCarSelect::Cleanup` (SCREENCARSELECT.CPP, 0x8003f658): leaving, the dialog
/// hidden, the card layer down, two players'.
pub fn pink_cleanup(c: &mut Cpu, this: u32) {
    pink_leave(&mut c.m, this);
    call_addr(c, DIALOG_HIDE, &[this + DIALOG]);
    crate::fememcard::deinit_memcard(c);
    two_player_cleanup(c, this);
}

pub fn abi_pink_cleanup(c: &mut Cpu) {
    pink_cleanup(c, c.r[4]);
}

/// `tScreenPinkSlipsCarSelect::SetDialog` (SCREENCARSELECT.CPP, 0x8003f6ac): the player's card's
/// state as the dialog (the second player's only once the first's ready; none when leaving):
/// insert a card (its message, after 800 ticks another; after an error a pause of 900), the
/// load's errors and outcomes, ready (waiting for the other player while either hasn't
/// chosen), gone (hidden).
pub fn pink_set_dialog(c: &mut Cpu, this: u32) {
    let m = &mut c.m;
    let p = app_player(m);
    let d = this + DIALOG;
    place_dialog(m, d, p);
    let hide = |c: &mut Cpu| {
        call_addr(c, DIALOG_HIDE, &[d]);
    };
    if (pink_state(m, 0) != PINK_READY && p == 1) || m.r32(this + LEAVING) != 0 {
        return hide(c);
    }
    let state = pink_state(m, p);
    if state != PINK_TO_LOAD {
        m.w32(this + MESSAGE_SINCE, 0);
    }
    let show = |c: &mut Cpu, word: u32| {
        let t = call(c, TEXT_WORD, &[word]);
        c.m.w32(d + 144, t);
        call_addr(c, DIALOG_DISPLAY, &[d]);
    };
    let clocks_clear = |m: &mut Mem| {
        m.w32(this + MESSAGE_SINCE, 0);
        m.w32(this + ERROR_SHOWN, 0);
    };
    match state {
        PINK_TO_LOAD => {
            if m.r32(this + MESSAGE_SINCE) == 0 {
                let t = m.r32(GAME_TICKS);
                m.w32(this + MESSAGE_SINCE, t);
            }
            let since = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + MESSAGE_SINCE)) as i32;
            if m.r32(this + ERROR_SHOWN) == 0 {
                show(c, p + if since < 800 { 684 } else { 682 });
            } else if since >= 901 {
                clocks_clear(m);
            }
        }
        1..=3 => {
            show(c, p + [686, 688, 690][state as usize - 1]);
            c.m.w32(this + ERROR_SHOWN, 1);
            c.m.w32(this + MESSAGE_SINCE, 0);
        }
        PINK_TOO_FEW | PINK_FULL | PINK_LOADING => {
            let word = match state {
                PINK_TOO_FEW => 814,
                PINK_FULL => 816,
                _ => 641,
            };
            show(c, p + word);
            clocks_clear(&mut c.m);
        }
        PINK_READY => {
            if has_chosen(m, p) || pink_state(m, 1 - p) != PINK_READY {
                show_waiting(c, d, p);
            } else {
                hide(c);
            }
            clocks_clear(&mut c.m);
        }
        PINK_LEAVING => hide(c),
        _ => {}
    }
}

pub fn abi_pink_set_dialog(c: &mut Cpu) {
    pink_set_dialog(c, c.r[4]);
}

/// `tScreenPinkSlipsCarSelect::ProcessInput` (SCREENCARSELECT.CPP, 0x8003f93c): until both
/// players' cars are loaded only back works (other keys but 4 made nothing); back then leaves
/// (the dialog hidden); then the car select's.
pub fn pink_process_input(c: &mut Cpu, this: u32, key: u32) {
    let m = &mut c.m;
    let k = m.r32(key);
    if k != KEY_BACK && k != 4 && !both_ready(m) {
        m.w32(key, 1);
    }
    if m.r32(key) == KEY_BACK && !both_ready(m) {
        pink_leave(m, this);
        call_addr(c, DIALOG_HIDE, &[this + DIALOG]);
    }
    process_input(c, this, key);
}

pub fn abi_pink_process_input(c: &mut Cpu) {
    pink_process_input(c, c.r[4], c.r[6]);
}

/// `tScreenPinkSlipsCarSelect::GetShapeInfo` (SCREENCARSELECT.CPP, 0x8003fa24): 52 shapes of
/// `zcarsb`, 5 swapped from a placeholder video's file; no car shown.
pub fn pink_get_shape_info(c: &mut Cpu, this: u32, count: u32, swap: u32, file: u32, swap_file: u32) {
    let m = &mut c.m;
    m.w16(count, 52);
    m.w16(swap, 5);
    none_shown(m, this);
    m.w32(file, TWO_PLAYER_FILE);
    call(c, SPRINTF, &[VIDEO_FILE, VIDEO_FORMAT, HACK_FILE]);
    c.m.w32(swap_file, VIDEO_FILE);
}

pub fn abi_pink_get_shape_info(c: &mut Cpu) {
    let swap_file = crate::abi::arg(c, 4);
    pink_get_shape_info(c, c.r[4], c.r[5], c.r[6], c.r[7], swap_file);
}

/// A two-player screen's destructor: its dialog's, then the car select's.
pub fn two_player_destructor(c: &mut Cpu, this: u32, flags: u32) {
    fescreen::screen_destructor(c, Screen(this + DIALOG), 2);
    screen_car_select_destructor(c, this, flags);
}

/// `tScreenPinkSlipsCarSelect::~tScreenPinkSlipsCarSelect` (SCREENCARSELECT.CPP, 0x8003fa9c).
pub fn abi_pink_destructor(c: &mut Cpu) {
    two_player_destructor(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelectTwoPlayer::~tScreenCarSelectTwoPlayer` (SCREENCARSELECT.CPP, 0x8003fae0).
pub fn abi_two_player_destructor(c: &mut Cpu) {
    two_player_destructor(c, c.r[4], c.r[5]);
}

/// `tScreenCarSelectDuel::~tScreenCarSelectDuel` (SCREENCARSELECT.CPP, 0x8003fb24).
pub fn abi_duel_destructor(c: &mut Cpu) {
    screen_car_select_destructor(c, c.r[4], c.r[5]);
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
        let cars = m.r32(CAR_MANAGER.0);
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
            ("tScreenCarSelectDuel::DrawVideoWall", 0x8003_d694, nfs4rc::f_8003d694, |c, r| {
                super::tests::wall_needs_a_car(c, DUEL);
                vec![DUEL, [0, 105][pick(r, 2) as usize]]
            }),
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
            // (both players' cars loaded: else `GetCar` fills nothing and the wall's availability is
            // the original's stack leftover)
            ("tScreenPinkSlipsCarSelect::DrawBackground", 0x8003_f280, nfs4rc::f_8003f280, |c, _| {
                c.m.w32(PINK_STATES, 6);
                c.m.w32(PINK_STATES + 4, 6);
                vec![PINK]
            }),
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
