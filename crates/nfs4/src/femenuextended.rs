//! FEMENUEXTENDED.CPP: the front end's NFS4-style menus: the text box an item is drawn in
//! (its text twice, the second copy offset, between arrows, in a rounded box), the buttons and
//! left-right choices that slide in and fade between enabled and disabled, the options
//! screen's choices (a list's value, or one of two), and the menus that hold them (the NFS4
//! menu with its animated button image, the two-player, bottom, blank and options menus,
//! their transitions on and off).
//!
//! Like FEMENU.CPP's, the classes call each other through their vtables (`method`), the
//! registers each call gets as the original has them at its `jal` or `jalr`.

use crate::abi::{self, Saved};
use crate::fetextrender;
use crate::femenu::{self, ITEM_DISABLED, ITEM_VPTR, ITERATOR_VPTR, KEY_NONE, PLAY_FE_SFX, TEXT_VALUE, TITLE, VALUE, method};
use crate::globals::{FE_APP, FLIP, GAME_TICKS, HELP_POS, PLAYER1_VIEW, SCREEN_WIDTH, SHAPES};
use ps1rt::{Cpu, Mem};

/// The vtables.
const GO_TO_MENU_NFS4_BUTTON_VT: u32 = 0x8001_0900;
const NFS4_LEFT_RIGHT_CHOICE_VT: u32 = 0x8001_08A8;
const MENU_NFS4_VT: u32 = 0x8001_0798;
const MENU_NFS4_TWO_PLAYER_VT: u32 = 0x8001_0738;
const MENU_NFS4_BOTTOM_VT: u32 = 0x8001_06D8;
const MENU_BLANK_VT: u32 = 0x8001_0678;
const MENU_OPTIONS_VT: u32 = 0x8001_0618;

/// A menu's vtable pointer, and its virtual functions used here: `TransitionIsFinished`,
/// `DrawItem(int)`.
pub(crate) const MENU_VPTR: u32 = 104;
pub(crate) const MENU_TRANSITION_IS_FINISHED: u32 = 56;
const MENU_DRAW_ITEM: u32 = 88;
/// The items' virtual functions used here: `Draw(int, int, bool)`, `TransitionOff`,
/// `TransitionOn`, `TransitionIsFinished`, `UpdateTransition`.
pub(crate) const ITEM_DRAW_AT: u32 = 40;
pub(crate) const ITEM_TRANSITION_OFF: u32 = 56;
pub(crate) const ITEM_TRANSITION_ON: u32 = 64;
pub(crate) const ITEM_TRANSITION_IS_FINISHED: u32 = 72;
pub(crate) const ITEM_UPDATE_TRANSITION: u32 = 80;

/// The drawing library: `Draw_GetDRAWENV`, `CalcFadeVal`, `CalcTextFadeSelToHi`,
/// `CalcOnOffFade`, `FETextRender_SetFont`, `TextSys_Word`, `sprintf`, `s_upper`,
/// `textpixels`, `strlen`, `FETextRender_FullTextRGB`, `FETextRender_MenuTextPositionedJustify`,
/// `DrawShapeExtended`, `DrawShape_NFS4RoundRectangle`, `Font_SetBlitter`,
/// `Font_ReSetBlitter`, `FeDraw_SetABRMode`.
pub(crate) const GET_DRAWENV: fn(&mut Cpu) = nfs4rc::f_800beb84;
const CALC_FADE_VAL: fn(&mut Cpu) = nfs4rc::f_8004bcd8;
pub(crate) const CALC_TEXT_FADE_SEL_TO_HI: fn(&mut Cpu) = nfs4rc::f_8004be30;
pub(crate) const CALC_ON_OFF_FADE: fn(&mut Cpu) = nfs4rc::f_8004beb4;
const SET_FONT: fn(&mut Cpu) = fetextrender::set_font_entry;
pub(crate) const TEXT_WORD: fn(&mut Cpu) = nfs4rc::f_800b9f18;
pub(crate) const SPRINTF: fn(&mut Cpu) = nfs4rc::f_800e50a8;
pub(crate) const S_UPPER: fn(&mut Cpu) = nfs4rc::f_80026910;
pub(crate) const TEXT_PIXELS: fn(&mut Cpu) = nfs4rc::f_800e4a2c;
pub(crate) const STRLEN: fn(&mut Cpu) = nfs4rc::f_800ea8d0;
const FULL_TEXT_RGB: fn(&mut Cpu) = fetextrender::full_text_rgb_entry;
const MENU_TEXT_POSITIONED_JUSTIFY: fn(&mut Cpu) = fetextrender::menu_text_positioned_justify_entry;
pub(crate) const DRAW_SHAPE_EXTENDED: fn(&mut Cpu) = nfs4rc::f_8004eb60;
pub(crate) const ROUND_RECTANGLE: fn(&mut Cpu) = nfs4rc::f_8004c490;
pub(crate) const SET_BLITTER: fn(&mut Cpu) = nfs4rc::f_800cc52c;
pub(crate) const RESET_BLITTER: fn(&mut Cpu) = nfs4rc::f_800cc538;
pub(crate) const SET_ABR_MODE: fn(&mut Cpu) = nfs4rc::f_8004dbc0;
/// Called by address (the host watches or replaces them): `DrawShape_SubtractNFS4RectEdges`,
/// `SetDrawArea`, `PSXDrawSquare`, `PSXDrawTransSquare`.
const SUBTRACT_RECT_EDGES: u32 = 0x8004_C2E0;
pub(crate) const SET_DRAW_AREA: u32 = 0x800E_EC70;
pub(crate) const DRAW_SQUARE: u32 = 0x8004_F12C;
pub(crate) const DRAW_TRANS_SQUARE: u32 = 0x8004_F3B8;
/// The mirrored blitter `Font_SetBlitter` takes for the reflections.
const MIRROR_BLITTER: u32 = 0x8004_F49C;
/// "%s".
const TEXT_FORMAT: u32 = 0x8001_0614;
/// The scratchpad's drawing state: the ordering table's entry, the next primitive.
pub(crate) const OT: u32 = 0x1F80_0000;
pub(crate) const NEXT_PRIM: u32 = 0x1F80_0004;

/// The short in a register's low half, sign-extended.
fn sx(v: u32) -> u32 {
    v as i16 as i32 as u32
}

fn lh(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as i16 as i32 as u32
}

fn lhu(m: &mut Mem, at: u32) -> u32 {
    m.r16(at) as u32
}

/// A 12-byte primitive (a draw area's) taken from the scratchpad's buffer and linked into the
/// ordering table.
pub(crate) fn add_prim(m: &mut Mem) -> u32 {
    let prim = m.r32(NEXT_PRIM);
    let ot = m.r32(OT);
    let v = (m.r32(prim) & 0xFF00_0000) | (m.r32(ot) & 0x00FF_FFFF);
    m.w32(prim, v);
    let entry = m.r32(ot);
    m.w32(NEXT_PRIM, prim.wrapping_add(12));
    m.w32(ot, (entry & 0xFF00_0000) | (prim & 0x00FF_FFFF));
    prim
}

/// A RECT's 8 bytes copied (the original's `lwl`/`lwr` pairs).
fn copy_rect(m: &mut Mem, from: u32, to: u32) {
    for k in 0..8 {
        let b = m.r8(from + k);
        m.w8(to + k, b);
    }
}

/// `MenuNFS4_SetHelpPos` (FEMENUEXTENDED.CPP, 0x8001a128): where the help box goes.
pub fn set_help_pos(m: &mut Mem, rect: u32) {
    for k in 0..4 {
        let v = m.r16(rect + 2 * k);
        m.w16(HELP_POS + 2 * k, v);
    }
}

pub fn abi_set_help_pos(c: &mut Cpu) {
    set_help_pos(&mut c.m, c.r[4]);
}

/// `MenuNFS4_DrawTextBox` (FEMENUEXTENDED.CPP, 0x8001a158): an item's box at `rect`: its
/// text (when it has one) twice, the second `width` (at least the text's) to the left, `offset`
/// across, in the item's fade (`fade_by_y`: faded by its height on the screen too), between
/// arrows when `arrows`, clipped to the screen; then the box's border and its square.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_box(c: &mut Cpu, text: u32, rect: u32, width: u32, offset: u32, fade: u32, arrows: u32, fade_by_y: u32) {
    abi::frame(c, 168, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = fade;
        r.0[1] = text;
        r.0[3] = rect;
        r.0[5] = width;
        r.0[6] = offset;
        r.0[7] = fade_by_y;
        let (view, flip) = (c.m.r32(PLAYER1_VIEW), c.m.r32(FLIP));
        let env = r.call_leaving(c, GET_DRAWENV, &[view, flip], 0x8001_a1b0);
        r.0[8] = env;
        let color = r.call_leaving(c, CALC_FADE_VAL, &[0x00B5_4200, 0xBEBE, sx(fade)], 0x8001_a1c8);
        r.0[2] = fade;
        c.m.w32(sp + 44, color);
        if fade_by_y != 0 {
            let v = 224i32.wrapping_sub(lh(&mut c.m, rect + 2) as i32);
            let v = if v < 0 { 0 } else if v >= 129 { 128 } else { v };
            let color = c.m.r32(sp + 44);
            let color = r.call_leaving(c, CALC_FADE_VAL, &[0, color, v as u32], 0x8001_a20c);
            c.m.w32(sp + 44, color);
        }
        r.call_ptr_leaving(c, SUBTRACT_RECT_EDGES, &[rect], 0x8001_a218);
        if (text as i32) >= 0 {
            // the whole screen's draw area
            let m = &mut c.m;
            m.w16(sp + 32, 0);
            let y = m.r16(env + 2);
            m.w16(sp + 36, 512);
            m.w16(sp + 38, 240);
            m.w16(sp + 34, y);
            let prim = add_prim(m);
            r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, sp + 32], 0x8001_a294);
            r.call_leaving(c, SET_FONT, &[0], 0x8001_a29c);
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_a2a4);
            let buf = sp + 64;
            r.0[0] = buf;
            r.call_leaving(c, SPRINTF, &[buf, TEXT_FORMAT, word], 0x8001_a2bc);
            r.call_leaving(c, S_UPPER, &[buf], 0x8001_a2c4);
            let pixels = r.call_leaving(c, TEXT_PIXELS, &[buf], 0x8001_a2cc);
            r.0[0] = pixels;
            let len = r.call_leaving(c, STRLEN, &[buf], 0x8001_a2d8);
            let shown = pixels.wrapping_sub(len);
            r.0[4] = shown;
            let gap = (if (shown as i32) < (width as i32) { width } else { shown }).wrapping_add(25);
            r.0[5] = gap;
            let mut col = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[1, sx(fade), 0], 0x8001_a304);
            r.0[0] = col;
            if fade_by_y != 0 {
                let v = 240u32.wrapping_sub(lh(&mut c.m, rect + 2));
                col = r.call_leaving(c, CALC_FADE_VAL, &[0, col, v], 0x8001_a324);
                r.0[0] = col;
            }
            for (dx, ras) in [(0, [0x8001_a330, 0x8001_a364]), (gap, [0x8001_a36c, 0x8001_a3a4])] {
                let word = r.call_leaving(c, TEXT_WORD, &[text], ras[0]);
                let m = &mut c.m;
                let x = sx(lhu(m, rect).wrapping_add(offset).wrapping_sub(dx));
                let y = sx(lhu(m, rect + 2).wrapping_add(4));
                r.call_leaving(c, FULL_TEXT_RGB, &[word, x, y, col, 0, 0], ras[1]);
            }
            if arrows != 0 {
                let m = &mut c.m;
                let mut y = lh(m, rect + 2).wrapping_add(((m.r16(rect + 6) as i16) >> 1) as i32 as u32);
                if fade_by_y == 0 {
                    y = y.wrapping_add(2);
                }
                r.0[2] = y;
                let shape = sp + 40;
                let dx = sx(offset);
                r.0[1] = shape;
                r.0[0] = dx;
                let marks = [
                    (10, (0u32, 0u32, 10u32), 0x8001_a404),
                    (11, (0, shown, 0u32.wrapping_sub(8)), 0x8001_a430),
                    (10, (gap, 0, 10), 0x8001_a45c),
                    (11, (gap, shown, 0u32.wrapping_sub(8)), 0x8001_a48c),
                ];
                for (id, (left, right, back), ra) in marks {
                    let x = lh(&mut c.m, rect).wrapping_add(dx).wrapping_sub(left).wrapping_add(right).wrapping_sub(back);
                    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[id, 280, x, y, 0, 0, shape], ra);
                }
            }
            // the box's own draw area
            let m = &mut c.m;
            copy_rect(m, rect, sp + 32);
            let v = lhu(m, sp + 34).wrapping_add(lhu(m, env + 2));
            m.w16(sp + 34, v as u16);
            let (x, w) = (lhu(m, sp + 32).wrapping_add(2), lhu(m, sp + 36).wrapping_sub(4));
            m.w16(sp + 32, x as u16);
            m.w16(sp + 36, w as u16);
            let prim = add_prim(m);
            r.call_ptr_leaving(c, SET_DRAW_AREA, &[prim, sp + 32], 0x8001_a530);
        }
        // the border, and the square
        let m = &mut c.m;
        copy_rect(m, rect, sp + 32);
        let shape = m.r32(SHAPES).wrapping_add(960);
        r.0[0] = shape;
        let y = lhu(m, sp + 34).wrapping_add(1);
        m.w16(sp + 34, y as u16);
        let h = lhu(m, sp + 38).wrapping_sub(2);
        m.w16(sp + 38, h as u16);
        let w = lhu(m, sp + 36).wrapping_sub(1).wrapping_sub(((m.r16(shape + 16) as i16) >> 1) as i32 as u32);
        let x = sx(w).wrapping_add(lh(m, sp + 32));
        m.w16(sp + 36, w as u16);
        r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[30, 8, x, sx(y), 0, 0, 0], 0x8001_a5c0);
        let m = &mut c.m;
        let args = [0, lh(m, sp + 32), lh(m, sp + 34), lh(m, sp + 36), lh(m, shape + 18)];
        r.call_ptr_leaving(c, DRAW_SQUARE, &args, 0x8001_a5dc);
    })
}

pub fn abi_draw_text_box(c: &mut Cpu) {
    let (fade, arrows, fade_by_y) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    draw_text_box(c, c.r[4], c.r[5], c.r[6], c.r[7], fade, arrows, fade_by_y);
}

/// The fade between disabled and enabled at `at` stepped (12 a frame, up when enabled) and
/// kept in 0..128.
fn step_enabled_fade(m: &mut Mem, this: u32, at: u32) {
    let v = m.r16(this + at);
    let v = if m.r32(this) & ITEM_DISABLED == 0 { v.wrapping_add(12) } else { v.wrapping_sub(12) };
    m.w16(this + at, v);
    let v = m.r16(this + at) as i16;
    if v < 0 {
        m.w16(this + at, 0);
    } else if v > 128 {
        m.w16(this + at, 128);
    }
}

/// The text box's width: 115 scaled by the lesser of the item's transition and enabled fades
/// (`a`, `b`: the one read first wins a tie).
fn box_width(m: &mut Mem, this: u32, a: u32, b: u32) -> u16 {
    let v = if (m.r16(this + b) as i16) < (m.r16(this + a) as i16) { m.r16(this + b) } else { m.r16(this + a) };
    let v = (v as i16 as i32).wrapping_mul(115);
    ((if v < 0 { v + 127 } else { v }) >> 7) as u16
}

/// The slide toward the item's place at `at` (2 a frame selected, 4 not; from the text's width
/// past the box when it would go past 0): `pixels` the text's width.
fn slide(m: &mut Mem, this: u32, at: u32, selected: u32, pixels: u32) {
    let back = if (pixels as i16) < 140 { 165 } else { pixels.wrapping_add(25) };
    let v = lhu(m, this + at);
    m.w16(this + at, v.wrapping_sub(2) as u16);
    if selected == 0 {
        m.w16(this + at, v.wrapping_sub(4) as u16);
    }
    if (m.r16(this + at) as i16) < 0 {
        let v = lhu(m, this + at).wrapping_add(back);
        m.w16(this + at, v as u16);
    }
}

/// `tMenuItemGoToMenuNFS4Button::tMenuItemGoToMenuNFS4Button` (FEMENUEXTENDED.CPP,
/// 0x8001a60c): a button going to `menu` (sliding in from 14), with a button image of
/// `frames` frames.
pub fn go_to_menu_nfs4_button(c: &mut Cpu, this: u32, text: u32, menu: u32, on_press: u32, image: u32, frames: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, frames, image]);
        r.enter(c, 0x8002_5348, 0x8001_a630, |c| femenu::go_to_menu_button(c, this, text, menu, on_press));
        let m = &mut c.m;
        m.w32(this + 24, GO_TO_MENU_NFS4_BUTTON_VT);
        m.w16(this + 36, 14);
        m.w16(this + 38, 0);
        m.w16(this + 40, 0);
        m.w16(this + 42, 0);
        m.w32(this + 16, frames);
        m.w32(this + 12, image);
        this
    })
}

pub fn abi_go_to_menu_nfs4_button(c: &mut Cpu) {
    let (image, frames) = (abi::arg(c, 4), abi::arg(c, 5));
    c.r[2] = go_to_menu_nfs4_button(c, c.r[4], c.r[5], c.r[6], c.r[7], image, frames);
}

/// A destructor: the vtable set back to the class's (when there's one), then the base's
/// (`base` at `base_addr`), given the caller's flags; `ra` its return.
pub(crate) fn destroy(c: &mut Cpu, this: u32, vptr_vt: Option<(u32, u32)>, base_addr: u32, ra: u32, base: fn(&mut Cpu)) {
    abi::frame(c, 24, |c, _| {
        if let Some((vptr, vt)) = vptr_vt {
            c.m.w32(this + vptr, vt);
        }
        let flags = c.r[5];
        Saved::of(c).enter(c, base_addr, ra, |c| {
            c.r[4] = this;
            c.r[5] = flags;
            base(c)
        });
    })
}

/// `tMenuItemGoToMenuNFS4Button::~tMenuItemGoToMenuNFS4Button` (FEMENUEXTENDED.CPP,
/// 0x8001a674).
pub fn abi_go_to_menu_nfs4_button_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((ITEM_VPTR, GO_TO_MENU_NFS4_BUTTON_VT)), 0x8002_539c, 0x8001_a68c, femenu::abi_go_to_menu_button_dtor);
}

/// `tMenuItemGoToMenuNFS4Button::Draw` (FEMENUEXTENDED.CPP, 0x8001a69c): the button's box at
/// (`x`, `y`), sliding in (the selected one first, at its text's width), fading as it is
/// enabled or not and as it transitions; its reflection below (flag 0x200).
pub fn go_to_menu_nfs4_button_draw(c: &mut Cpu, this: u32, x: u32, y: u32, selected: u32) {
    abi::frame(c, 128, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[2] = selected;
        r.0[3] = x;
        r.0[4] = y;
        if selected == 0 && (lh(&mut c.m, this + 36) as i32).wrapping_sub(14) < 2 {
            c.m.w16(this + 36, 14);
        } else {
            r.call_leaving(c, SET_FONT, &[0], 0x8001_a6ec);
            let text = c.m.r32(this + 4);
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_a6f8);
            let buf = sp + 40;
            r.0[0] = buf;
            r.call_leaving(c, SPRINTF, &[buf, TEXT_FORMAT, word], 0x8001_a710);
            r.call_leaving(c, S_UPPER, &[buf], 0x8001_a718);
            let pixels = r.call_leaving(c, TEXT_PIXELS, &[buf], 0x8001_a720);
            r.0[0] = pixels;
            let len = r.call_leaving(c, STRLEN, &[buf], 0x8001_a72c);
            let shown = pixels.wrapping_sub(len);
            r.0[0] = shown;
            slide(&mut c.m, this, 36, selected, shown);
        }
        step_enabled_fade(&mut c.m, this, 42);
        r.0[2] = 11;
        r.0[0] = 115;
        for (mirror, ras) in [(0u32, [0, 0x8001_a854]), (1, [0x8001_a870, 0x8001_a8f0])] {
            if mirror == 1 {
                if c.m.r32(this) & 0x200 == 0 {
                    return;
                }
                r.call_leaving(c, SET_BLITTER, &[MIRROR_BLITTER], ras[0]);
            }
            let m = &mut c.m;
            m.w16(sp + 32, x as u16);
            m.w16(sp + 34, if mirror == 0 { y } else { 280u32.wrapping_sub(y) } as u16);
            m.w16(sp + 36, 115);
            m.w16(sp + 38, 11);
            let w = box_width(m, this, 38, 42);
            m.w16(sp + 36, w);
            let (text, offset, fade) = (m.r32(this + 4), lh(m, this + 36), lh(m, this + 8));
            r.call_leaving(c, draw_text_box_entry, &[text, sp + 32, 140, offset, fade, 0, mirror], ras[1]);
        }
        r.call_leaving(c, RESET_BLITTER, &[], 0x8001_a8f8);
    })
}

/// `MenuNFS4_DrawTextBox` entered as the original's callers `jal` it.
fn draw_text_box_entry(c: &mut Cpu) {
    abi::leave_prologue(c, 0x8001_a158, None, &[]);
    abi_draw_text_box(c);
}

pub fn abi_go_to_menu_nfs4_button_draw(c: &mut Cpu) {
    go_to_menu_nfs4_button_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// A sliding item's `TransitionOn`, `TransitionOff`, `TransitionIsFinished` and
/// `UpdateTransition` (the button's at +38, the choice's at +34: the transition, then its
/// speed).
fn transition_on(m: &mut Mem, this: u32, at: u32) {
    // (the enabled fade starts again too)
    m.w16(this + at + 4, 0);
    m.w16(this + at + 2, 12);
}

fn transition_off(m: &mut Mem, this: u32, at: u32) {
    m.w16(this + at + 2, (-12i16) as u16);
}

fn transition_is_finished(m: &mut Mem, this: u32, at: u32) -> u32 {
    (m.r16(this + at + 2) == 0) as u32
}

fn update_transition(c: &mut Cpu, this: u32, at: u32, selected: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let v = m.r16(this + at).wrapping_add(m.r16(this + at + 2));
        m.w16(this + at, v);
        let v = v as i16;
        if v < 0 {
            m.w16(this + at, 0);
            m.w16(this + at + 2, 0);
        } else if v > 128 {
            m.w16(this + at, 128);
            m.w16(this + at + 2, 0);
        }
        Saved::of(c).enter(c, 0x8002_47c8, ra, |c| femenu::menu_item_update_transition(c, this, selected));
    })
}

/// `tMenuItemGoToMenuNFS4Button::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001a918).
pub fn abi_button_transition_on(c: &mut Cpu) {
    transition_on(&mut c.m, c.r[4], 38);
}

/// `tMenuItemGoToMenuNFS4Button::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001a928).
pub fn abi_button_transition_off(c: &mut Cpu) {
    transition_off(&mut c.m, c.r[4], 38);
}

/// `tMenuItemGoToMenuNFS4Button::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001a934).
pub fn abi_button_transition_is_finished(c: &mut Cpu) {
    c.r[2] = transition_is_finished(&mut c.m, c.r[4], 38);
}

/// `tMenuItemGoToMenuNFS4Button::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001a940).
pub fn abi_button_update_transition(c: &mut Cpu) {
    update_transition(c, c.r[4], 38, c.r[5], 0x8001_a98c);
}

/// `tMenuItemNFS4LeftRightChoice::tMenuItemNFS4LeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001a99c): a left-right choice that slides in, with a button image of `frames` frames.
pub fn nfs4_left_right_choice(c: &mut Cpu, this: u32, text: u32, data: u32, image: u32, frames: u32) -> u32 {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[..3].copy_from_slice(&[this, image, frames]);
        r.enter(c, 0x8002_4924, 0x8001_a9c0, |c| femenu::left_right_choice(c, this, text, data));
        let m = &mut c.m;
        m.w32(this + 24, NFS4_LEFT_RIGHT_CHOICE_VT);
        m.w32(this + 12, image);
        m.w16(this + 32, 14);
        m.w16(this + 34, 0);
        m.w16(this + 36, 0);
        m.w32(this + 16, frames);
        this
    })
}

pub fn abi_nfs4_left_right_choice(c: &mut Cpu) {
    let frames = abi::arg(c, 4);
    c.r[2] = nfs4_left_right_choice(c, c.r[4], c.r[5], c.r[6], c.r[7], frames);
}

/// `tMenuItemNFS4LeftRightChoice::~tMenuItemNFS4LeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001aa00).
pub fn abi_nfs4_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((ITEM_VPTR, NFS4_LEFT_RIGHT_CHOICE_VT)), 0x8002_4974, 0x8001_aa18, femenu::abi_left_right_choice_dtor);
}

/// `tMenuItemNFS4LeftRightChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001aa28): as the button's, the
/// text's width straight from the text.
pub fn nfs4_left_right_choice_draw(c: &mut Cpu, this: u32, x: u32, y: u32, selected: u32) {
    abi::frame(c, 64, |c, sp| {
        let mut r = Saved::of(c);
        r.0[..4].copy_from_slice(&[this, selected, x, y]);
        if selected == 0 && (lh(&mut c.m, this + 32) as i32).wrapping_sub(14) < 2 {
            c.m.w16(this + 32, 14);
        } else {
            r.call_leaving(c, SET_FONT, &[0], 0x8001_aa74);
            let text = c.m.r32(this + 4);
            let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_aa80);
            let pixels = r.call_leaving(c, TEXT_PIXELS, &[word], 0x8001_aa88);
            slide(&mut c.m, this, 32, selected, pixels);
        }
        step_enabled_fade(&mut c.m, this, 38);
        for (mirror, ras) in [(0u32, [0, 0x8001_abd0]), (1, [0x8001_abec, 0x8001_ac98])] {
            if mirror == 1 {
                if c.m.r32(this) & 0x200 == 0 {
                    return;
                }
                r.call_leaving(c, SET_BLITTER, &[MIRROR_BLITTER], ras[0]);
            }
            let m = &mut c.m;
            if mirror == 0 {
                m.w16(sp + 36, 115);
                m.w16(sp + 32, x as u16);
                m.w16(sp + 34, y as u16);
                m.w16(sp + 38, 11);
            } else {
                m.w16(sp + 34, 280u32.wrapping_sub(y) as u16);
                m.w16(sp + 36, 115);
                m.w16(sp + 32, x as u16);
                m.w16(sp + 38, 11);
            }
            let w = box_width(m, this, 34, 38);
            m.w16(sp + 36, w);
            let (offset, fade) = (lh(m, this + 32), lh(m, this + 8));
            // the arrows unless sliding in unselected
            let arrows = (selected != 0 || m.r16(this + 32) as i16 != 14) as u32;
            let text = m.r32(this + 4);
            r.call_leaving(c, draw_text_box_entry, &[text, sp + 32, 140, offset, fade, arrows, mirror], ras[1]);
        }
        r.call_leaving(c, RESET_BLITTER, &[], 0x8001_aca0);
    })
}

pub fn abi_nfs4_left_right_choice_draw(c: &mut Cpu) {
    nfs4_left_right_choice_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemNFS4LeftRightChoice::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001acbc).
pub fn abi_choice_transition_on(c: &mut Cpu) {
    transition_on(&mut c.m, c.r[4], 34);
}

/// `tMenuItemNFS4LeftRightChoice::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001accc).
pub fn abi_choice_transition_off(c: &mut Cpu) {
    transition_off(&mut c.m, c.r[4], 34);
}

/// `tMenuItemNFS4LeftRightChoice::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001acd8).
pub fn abi_choice_transition_is_finished(c: &mut Cpu) {
    c.r[2] = transition_is_finished(&mut c.m, c.r[4], 34);
}

/// `tMenuItemNFS4LeftRightChoice::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001ace4).
pub fn abi_choice_update_transition(c: &mut Cpu) {
    update_transition(c, c.r[4], 34, c.r[5], 0x8001_ad30);
}

/// The options screen's choices' arrows either side of the value and the rounded box round the
/// row (`OptionsLeftRightChoice::Draw`'s and `OptionsTwoItemChoice::Draw`'s ends); `r` the
/// registers at their calls, `shapes` the shapes' 41st.
#[allow(clippy::too_many_arguments)]
fn options_arrows_and_box(c: &mut Cpu, r: &Saved, sp: u32, this: u32, x: u32, y: u32, selected: u32, shapes: u32, ras: [u32; 4]) {
    let fade = lh(&mut c.m, this + 8);
    let color = r.call_leaving(c, CALC_FADE_VAL, &[0x00B5_4200, 0xBEBE, fade], ras[0]);
    c.m.w32(sp + 44, color);
    let lit = (selected == 0) as u32;
    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[10, 280, x.wrapping_add(166), y.wrapping_add(6), 0, lit, sp + 40], ras[1]);
    let right = x.wrapping_sub(lh(&mut c.m, shapes + 16)).wrapping_add(303);
    r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[11, 280, right, y.wrapping_add(6), 0, lit, sp + 40], ras[2]);
    let m = &mut c.m;
    m.w16(sp + 32, x as u16);
    m.w16(sp + 34, y as u16);
    m.w16(sp + 36, 297);
    let h = m.r16(shapes + 18);
    m.w16(sp + 38, h);
    r.call_leaving(c, ROUND_RECTANGLE, &[u32::MAX, sp + 32, sx(selected)], ras[3]);
}

/// `tMenuItemOptionsLeftRightChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001ad40): the item's text
/// and its value's (centred on the value's place) between the arrows, in the rounded box.
pub fn options_left_right_choice_draw(c: &mut Cpu, this: u32, x: u32, y: u32, selected: u32) {
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[4] = x;
        r.0[5] = y;
        r.0[6] = selected;
        let shapes = c.m.r32(SHAPES).wrapping_add(1312);
        r.0[3] = shapes;
        let fade = lh(&mut c.m, this + 8);
        let col = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, fade, 0], 0x8001_ad94);
        r.0[2] = col;
        let text = c.m.r32(this + 4);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_ada0);
        let ty = sx(y.wrapping_add(3));
        r.0[0] = ty;
        r.0[7] = 1;
        r.call_leaving(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(148)), ty, col, 0, 1], 0x8001_add4);
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, ITERATOR_VPTR, TEXT_VALUE);
        let id = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_adf8);
        let word = r.call_leaving(c, TEXT_WORD, &[sx(id)], 0x8001_ae04);
        let half = ((c.m.r16(shapes + 16) as i16) >> 1) as i32 as u32;
        let vx = sx(half.wrapping_add(x).wrapping_add(217));
        r.call_leaving(c, FULL_TEXT_RGB, &[word, vx, ty, col, 0, 2], 0x8001_ae3c);
        options_arrows_and_box(c, &r, sp, this, x, y, selected, shapes, [0x8001_ae50, 0x8001_ae84, 0x8001_aec0, 0x8001_aeec]);
    })
}

pub fn abi_options_left_right_choice_draw(c: &mut Cpu) {
    options_left_right_choice_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuItemOptionsTwoItemChoice::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001af18): the on-off
/// fade at its value's end.
pub fn two_item_choice_transition_on(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let data = c.m.r32(this + 28);
        let (obj, f) = method(&mut c.m, data, ITERATOR_VPTR, VALUE);
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX], 0x8001_af54);
        c.m.w16(this + 32, (((v & 0xFF) != 0) as u16) << 7);
    })
}

pub fn abi_two_item_choice_transition_on(c: &mut Cpu) {
    two_item_choice_transition_on(c, c.r[4]);
}

/// `tMenuItemOptionsTwoItemChoice::Draw` (FEMENUEXTENDED.CPP, 0x8001af74): the item's text and
/// both of its choices (the list's two texts), the on-off fade moving toward the value's (64 a
/// frame), between the arrows, in the rounded box.
pub fn two_item_choice_draw(c: &mut Cpu, this: u32, x: u32, y: u32, selected: u32) {
    abi::frame(c, 104, |c, sp| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[4] = x;
        r.0[5] = y;
        r.0[6] = selected;
        let m = &mut c.m;
        let data = m.r32(this + 28);
        let vt = m.r32(data + ITERATOR_VPTR);
        let (obj, f) = method(m, data, ITERATOR_VPTR, VALUE);
        let shapes = m.r32(SHAPES).wrapping_add(1312);
        r.0[3] = shapes;
        let v = r.call_ptr_leaving(c, f, &[obj, u32::MAX, vt], 0x8001_afd4);
        let m = &mut c.m;
        let fade = m.r16(this + 32);
        m.w16(this + 32, if v & 0xFF != 0 { fade.wrapping_add(64) } else { fade.wrapping_sub(64) });
        let v = m.r16(this + 32) as i16;
        let clamped = v.clamp(0, 128) as u16;
        m.w16(this + 32, clamped);
        let fade = lh(m, this + 8);
        let col = r.call_leaving(c, CALC_TEXT_FADE_SEL_TO_HI, &[6, fade, 0], 0x8001_b034);
        r.0[1] = col;
        let m = &mut c.m;
        let (on_off, fade) = (lh(m, this + 32), lh(m, this + 8));
        r.call_leaving(c, CALC_ON_OFF_FADE, &[6, on_off, fade, 0, sp + 64, sp + 68], 0x8001_b05c);
        let text = c.m.r32(this + 4);
        let ty = sx(y.wrapping_add(3));
        r.0[0] = y.wrapping_add(3);
        let word = r.call_leaving(c, TEXT_WORD, &[text], 0x8001_b068);
        r.0[0] = ty;
        r.0[1] = 1;
        r.call_leaving(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(148)), ty, col, 0, 1], 0x8001_b098);
        // the two choices' texts, in the on-off fade's colours
        let list = |m: &mut Mem| {
            let data = m.r32(this + 28);
            m.r32(data)
        };
        let at = list(&mut c.m);
        let first = lh(&mut c.m, at);
        let word = r.call_leaving(c, TEXT_WORD, &[first], 0x8001_b0b4);
        let color = c.m.r32(sp + 68);
        r.call_leaving(c, FULL_TEXT_RGB, &[word, sx(x.wrapping_add(176)), ty, color, 0, 0], 0x8001_b0d8);
        let at = list(&mut c.m) + 2;
        let second = lh(&mut c.m, at);
        let word = r.call_leaving(c, TEXT_WORD, &[second], 0x8001_b0f4);
        let m = &mut c.m;
        let sx2 = sx(x.wrapping_sub(lhu(m, shapes + 16)).wrapping_add(294));
        let color = m.r32(sp + 64);
        r.call_leaving(c, FULL_TEXT_RGB, &[word, sx2, ty, color, 0, 1], 0x8001_b120);
        options_arrows_and_box(c, &r, sp, this, x, y, selected, shapes, [0x8001_b134, 0x8001_b168, 0x8001_b1a4, 0x8001_b1d0]);
    })
}

pub fn abi_two_item_choice_draw(c: &mut Cpu) {
    two_item_choice_draw(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

// tMenuNFS4: +108 in an item's transition, +112 in the menu's, +116 its transition, +118 the
// direction, +119 the last item, +120 how many.

/// The menu's `k`th item.
fn item(m: &mut Mem, this: u32, k: u32) -> u32 {
    m.r32(this.wrapping_add(16).wrapping_add(k.wrapping_mul(4)))
}

/// The NFS4 menus' constructors: `tMenu`'s with the caller's stack arguments (options, back,
/// title: copied to this frame's outgoing ones), then (`items`) the items from the caller's
/// `first` and the list after it; `base` the constructor (`tMenu`'s or `tMenuNFS4`'s) and `ras`
/// its call's and the item list's returns; `set_vt` the class's vtable written where the
/// original does it.
#[allow(clippy::too_many_arguments)]
fn construct(c: &mut Cpu, frame: u32, this: u32, base: u32, stack: u32, items: Option<(u32, u32)>, vt: u32, ras: [u32; 2], extra: &[u32]) -> u32 {
    abi::frame(c, frame, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        for (k, &v) in extra.iter().enumerate() {
            r.0[1 + k] = v;
        }
        let (flags, screen, next) = (c.r[5], c.r[6], c.r[7]);
        let m = &mut c.m;
        let (options, on_back, title) = (m.r32(stack), m.r32(stack + 4), sx(m.r32(stack + 8)));
        m.w32(sp + 16, options);
        m.w32(sp + 20, on_back);
        m.w32(sp + 24, title);
        if base == 0x8002_548c {
            r.enter(c, base, ras[0], |c| femenu::menu(&mut c.m, this, flags, screen, next, options, on_back, title));
        } else {
            r.enter(c, base, ras[0], |c| menu_nfs4_bare(c, this, flags, screen, next, options, on_back, title));
        }
        match items {
            Some((first, rest)) => {
                c.m.w32(this + MENU_VPTR, vt);
                if vt == MENU_NFS4_TWO_PLAYER_VT {
                    c.m.w32(this + 88, 0);
                }
                femenu::menu_constructor(&mut c.m, this, first, rest);
            }
            None => c.m.w32(this + MENU_VPTR, vt),
        }
        this
    })
}

/// `tMenuNFS4::tMenuNFS4` (with items: FEMENUEXTENDED.CPP, 0x8001b1f8).
pub fn abi_menu_nfs4(c: &mut Cpu) {
    let (this, sp) = (c.r[4], c.r[29]);
    let first = c.m.r32(sp + 28);
    c.r[2] = construct(c, 40, this, 0x8002_548c, sp + 16, Some((first, sp + 32)), MENU_NFS4_VT, [0x8001_b230, 0x8001_b24c], &[]);
}

/// `tMenuNFS4::tMenuNFS4` (without items: FEMENUEXTENDED.CPP, 0x8001b260).
#[allow(clippy::too_many_arguments)]
pub fn menu_nfs4_bare(c: &mut Cpu, this: u32, flags: u32, screen: u32, next: u32, options: u32, on_back: u32, title: u32) -> u32 {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        let m = &mut c.m;
        m.w32(sp + 20, on_back);
        m.w32(sp + 16, options);
        m.w32(sp + 24, sx(title));
        r.enter(c, 0x8002_548c, 0x8001_b298, |c| femenu::menu(&mut c.m, this, flags, screen, next, options, on_back, sx(title)));
        c.m.w32(this + MENU_VPTR, MENU_NFS4_VT);
        this
    })
}

pub fn abi_menu_nfs4_bare(c: &mut Cpu) {
    let (options, on_back, title) = (abi::arg(c, 4), abi::arg(c, 5), abi::arg(c, 6));
    c.r[2] = menu_nfs4_bare(c, c.r[4], c.r[5], c.r[6], c.r[7], options, on_back, title);
}

/// `tMenuNFS4::~tMenuNFS4` (FEMENUEXTENDED.CPP, 0x8001b2b8).
pub fn abi_menu_nfs4_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((MENU_VPTR, MENU_NFS4_VT)), 0x8002_54d0, 0x8001_b2d0, femenu::abi_menu_dtor);
}

/// `tMenuNFS4::Initialize` (FEMENUEXTENDED.CPP, 0x8001b2e0): `tMenu`'s; no transitions, the
/// last item the current one, its items counted (flag 0x200: each reflected).
pub fn menu_nfs4_initialize(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        femenu::initialize(&mut c.m, this);
        let m = &mut c.m;
        let cur = m.r8(this + 8);
        let first = item(m, this, 0);
        m.w32(this + 108, 0);
        m.w32(this + 112, 0);
        m.w8(this + 120, 0);
        m.w8(this + 119, cur);
        if first != 0 {
            loop {
                let n = m.r8(this + 120).wrapping_add(1);
                m.w8(this + 120, n);
                if item(m, this, n as u32) == 0 {
                    break;
                }
            }
        }
        if m.r32(this) & 0x200 != 0 {
            let mut k = 0u32;
            loop {
                let it = item(m, this, k as i16 as i32 as u32);
                if it == 0 {
                    break;
                }
                k = k.wrapping_add(1);
                let flags = m.r32(it);
                m.w32(it, flags | 0x200);
            }
        }
    })
}

pub fn abi_menu_nfs4_initialize(c: &mut Cpu) {
    menu_nfs4_initialize(c, c.r[4]);
}

/// `tMenuNFS4::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001b390): `tMenu`'s.
pub fn menu_nfs4_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 24, |c, _| {
        Saved::of(c).enter(c, 0x8002_5598, 0x8001_b3a0, |c| femenu::process_input(c, this, player, key, command));
    })
}

pub fn abi_menu_nfs4_process_input(c: &mut Cpu) {
    menu_nfs4_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// Each item's virtual `slot`, in order (`TransitionOff`'s, `TransitionOn`'s and
/// `TransitionIsFinished`'s loop: the item alone); its results and'd together.
fn each_item(c: &mut Cpu, r: &mut Saved, this: u32, slot: u32, ra: u32, k_reg: usize, all: Option<usize>) -> u32 {
    let mut done = 1u32;
    if item(&mut c.m, this, 0) == 0 {
        return done;
    }
    let mut k = 0u32;
    loop {
        r.0[k_reg] = k;
        let it = item(&mut c.m, this, k as i16 as i32 as u32);
        let (obj, f) = method(&mut c.m, it, ITEM_VPTR, slot);
        let v = r.call_ptr_leaving(c, f, &[obj], ra);
        if let Some(reg) = all {
            done = ((done & v) != 0) as u32;
            r.0[reg] = done;
        }
        k = k.wrapping_add(1);
        if item(&mut c.m, this, k as i16 as i32 as u32) == 0 {
            return done;
        }
    }
}

/// `tMenuNFS4::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001b3b0): each item's.
pub fn abi_menu_nfs4_transition_off(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = 0;
        each_item(c, &mut r, this, ITEM_TRANSITION_OFF, 0x8001_b404, 0, None);
    })
}

/// `tMenuNFS4::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001b438): each item's.
pub fn abi_menu_nfs4_transition_on(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        r.0[0] = 0;
        each_item(c, &mut r, this, ITEM_TRANSITION_ON, 0x8001_b48c, 0, None);
    })
}

/// `tMenuNFS4::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001b4c0): whether every item's is.
pub fn abi_menu_nfs4_transition_is_finished(c: &mut Cpu) {
    let this = c.r[4];
    c.r[2] = abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        r.0[0] = 1;
        r.0[1] = 0;
        each_item(c, &mut r, this, ITEM_TRANSITION_IS_FINISHED, 0x8001_b51c, 1, Some(0))
    });
}

/// `tMenuNFS4::UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001b560): each item's, told whether it
/// is the current one.
pub fn abi_menu_nfs4_update_transition(c: &mut Cpu) {
    let this = c.r[4];
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        if item(&mut c.m, this, 0) == 0 {
            return;
        }
        let mut k = 0u32;
        loop {
            r.0[0] = k;
            let m = &mut c.m;
            let i = k as i16 as i32 as u32;
            let it = item(m, this, i);
            let selected = (i == m.r32(this + 8)) as u32;
            let vt = m.r32(it + ITEM_VPTR);
            let (obj, f) = method(m, it, ITEM_VPTR, ITEM_UPDATE_TRANSITION);
            r.call_ptr_leaving(c, f, &[obj, selected, vt], 0x8001_b5c0);
            k = k.wrapping_add(1);
            if item(&mut c.m, this, k as i16 as i32 as u32) == 0 {
                return;
            }
        }
    });
}

/// An item's `Draw(int, int, bool)` at (`x`, `y`), told whether it is the current one; `ra`
/// the call's return.
fn draw_item_at(c: &mut Cpu, this: u32, k: u32, x: u32, y: u32, ra: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        let it = item(m, this, k);
        let selected = (k == m.r32(this + 8)) as u32;
        let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DRAW_AT);
        Saved::of(c).call_ptr_leaving(c, f, &[obj, x, y, selected], ra);
    })
}

/// `tMenuNFS4::DrawItem` (FEMENUEXTENDED.CPP, 0x8001b5f4): item `k` at (10, 43 + 18k).
pub fn abi_menu_nfs4_draw_item(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5]);
    draw_item_at(c, this, k, 10, k.wrapping_mul(18).wrapping_add(43), 0x8001_b644);
}

/// `tMenuNFS4::Draw` (FEMENUEXTENDED.CPP, 0x8001b654): its title, the current item's button
/// image (animated: a frame every 16 ticks; lower for the second player), each item
/// (`DrawItem`).
pub fn menu_nfs4_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 72, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let title = lh(&mut c.m, this + 4);
        if (title as i32) >= 0 {
            r.call_leaving(c, TITLE, &[title], 0x8001_b680);
        }
        femenu::initialize(&mut c.m, this);
        let m = &mut c.m;
        let cur = m.r32(this + 8);
        let it = item(m, this, cur);
        let (image, frames) = (m.r32(it + 12), m.r32(it + 16));
        if (image as i32) >= 0 && (frames as i32) > 0 {
            let frame = ((m.r32(GAME_TICKS) as i32) >> 4).wrapping_rem(frames as i32) as u32;
            m.w32(sp + 36, 0x00CE_C844);
            let app = m.r32(FE_APP);
            let y = if m.r8(app + 556) != 0 { 121 } else { 16 };
            r.call_leaving(c, DRAW_SHAPE_EXTENDED, &[image.wrapping_add(frame), 1040, 16, y, 0, 0, sp + 32], 0x8001_b738);
        }
        if item(&mut c.m, this, 0) == 0 {
            return;
        }
        let mut k = 0u32;
        loop {
            r.0[0] = k;
            let (obj, f) = method(&mut c.m, this, MENU_VPTR, MENU_DRAW_ITEM);
            r.call_ptr_leaving(c, f, &[obj, k as i16 as i32 as u32], 0x8001_b768);
            k = k.wrapping_add(1);
            if item(&mut c.m, this, k as i16 as i32 as u32) == 0 {
                return;
            }
        }
    })
}

pub fn abi_menu_nfs4_draw(c: &mut Cpu) {
    menu_nfs4_draw(c, c.r[4]);
}

/// A derived NFS4 menu's constructor with items (two-player, bottom, options): `tMenuNFS4`'s
/// without, then the vtable and the items.
fn construct_nfs4(c: &mut Cpu, vt: u32, ras: [u32; 2]) -> u32 {
    let (this, sp) = (c.r[4], c.r[29]);
    let first = c.m.r32(sp + 28);
    construct(c, 40, this, 0x8001_b260, sp + 16, Some((first, sp + 32)), vt, ras, &[])
}

/// `tMenuNFS4TwoPlayer::tMenuNFS4TwoPlayer` (FEMENUEXTENDED.CPP, 0x8001b79c).
pub fn abi_menu_two_player(c: &mut Cpu) {
    c.r[2] = construct_nfs4(c, MENU_NFS4_TWO_PLAYER_VT, [0x8001_b7d4, 0x8001_b7f4]);
}

/// `tMenuNFS4TwoPlayer::~tMenuNFS4TwoPlayer` (FEMENUEXTENDED.CPP, 0x8001b808).
pub fn abi_menu_two_player_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((MENU_VPTR, MENU_NFS4_TWO_PLAYER_VT)), 0x8001_b2b8, 0x8001_b820, abi_menu_nfs4_dtor);
}

/// `tMenuNFS4TwoPlayer::DrawItem` (FEMENUEXTENDED.CPP, 0x8001b830): item `k` in the player's
/// half of the screen.
pub fn abi_menu_two_player_draw_item(c: &mut Cpu) {
    let (this, k) = (c.r[4], c.r[5]);
    let app = c.m.r32(FE_APP);
    let top = if c.m.r8(app + 556) == 1 { 148 } else { 43 };
    draw_item_at(c, this, k, 10, k.wrapping_mul(18).wrapping_add(top), 0x8001_b898);
}

/// `tMenuNFS4Bottom::tMenuNFS4Bottom` (FEMENUEXTENDED.CPP, 0x8001b8a8).
pub fn abi_menu_bottom(c: &mut Cpu) {
    c.r[2] = construct_nfs4(c, MENU_NFS4_BOTTOM_VT, [0x8001_b8e0, 0x8001_b8fc]);
}

/// `tMenuNFS4Bottom::~tMenuNFS4Bottom` (FEMENUEXTENDED.CPP, 0x8001b910).
pub fn abi_menu_bottom_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((MENU_VPTR, MENU_NFS4_BOTTOM_VT)), 0x8001_b2b8, 0x8001_b928, abi_menu_nfs4_dtor);
}

/// `tMenuNFS4Bottom::Draw` (FEMENUEXTENDED.CPP, 0x8001b938): the help at the bottom, each item
/// (where it puts itself).
pub fn menu_bottom_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 40, |c, sp| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let m = &mut c.m;
        for (k, v) in [57u16, 198, 114, 11].into_iter().enumerate() {
            m.w16(sp + 16 + 2 * k as u32, v);
        }
        set_help_pos(m, sp + 16);
        if item(m, this, 0) == 0 {
            return;
        }
        let mut k = 0u32;
        loop {
            r.0[0] = k;
            let m = &mut c.m;
            let i = k as i16 as i32 as u32;
            let it = item(m, this, i);
            let selected = (i == m.r32(this + 8)) as u32;
            let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DRAW_AT);
            r.call_ptr_leaving(c, f, &[obj, 0, 0, selected], 0x8001_b9c4);
            k = k.wrapping_add(1);
            if item(&mut c.m, this, k as i16 as i32 as u32) == 0 {
                return;
            }
        }
    })
}

pub fn abi_menu_bottom_draw(c: &mut Cpu) {
    menu_bottom_draw(c, c.r[4]);
}

/// `tMenuBlank::tMenuBlank` (FEMENUEXTENDED.CPP, 0x8001b9f8): a menu of no items.
pub fn abi_menu_blank(c: &mut Cpu) {
    let (this, sp) = (c.r[4], c.r[29]);
    let r = construct(c, 40, this, 0x8001_b260, sp + 16, None, MENU_BLANK_VT, [0x8001_ba30, 0], &[]);
    c.m.w32(this + 12, 1);
    c.m.w16(this + 100, 0);
    c.r[2] = r;
}

/// `tMenuBlank::~tMenuBlank` (FEMENUEXTENDED.CPP, 0x8001ba5c).
pub fn abi_menu_blank_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((MENU_VPTR, MENU_BLANK_VT)), 0x8001_b2b8, 0x8001_ba74, abi_menu_nfs4_dtor);
}

/// `tMenuBlank::Draw` (FEMENUEXTENDED.CPP, 0x8001ba84): in a transition, it steps.
pub fn menu_blank_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let mut r = Saved::of(c);
        r.0[0] = this;
        if c.m.r32(this + 112) == 0 {
            return;
        }
        let (obj, f) = method(&mut c.m, this, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_bac0);
        let m = &mut c.m;
        let v = m.r16(this + 116).wrapping_add(m.r8(this + 118) as i8 as i16 as u16);
        m.w16(this + 116, v);
    })
}

pub fn abi_menu_blank_draw(c: &mut Cpu) {
    menu_blank_draw(c, c.r[4]);
}

/// `tMenuBlank::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001bae8): up and down are used up; other
/// keys go to `tMenu`'s, the items forgotten first.
pub fn menu_blank_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 24, |c, _| {
        let k = c.m.r32(key);
        if k == 512 || k == 1024 {
            c.m.w32(key, KEY_NONE);
            return;
        }
        c.m.w32(this + 16, 0);
        Saved::of(c).enter(c, 0x8002_5598, 0x8001_bb1c, |c| femenu::process_input(c, this, player, key, command));
    })
}

pub fn abi_menu_blank_process_input(c: &mut Cpu) {
    menu_blank_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuBlank::Initialize`, `UpdateTransition` (FEMENUEXTENDED.CPP, 0x8001bb2c, 0x8001c058),
/// `tMenuItemGoToMenuNFS4Button::Draw(bool)` (0x8001c0a0): nothing.
pub fn abi_nothing(_: &mut Cpu) {}

/// `tMenuBlank::DebounceKeys` (FEMENUEXTENDED.CPP, 0x8001bb34): every key.
pub fn abi_menu_blank_debounce_keys(c: &mut Cpu) {
    c.r[2] = u32::MAX;
}

/// `tMenuBlank::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001bb3c): sliding down from -112.
pub fn abi_menu_blank_transition_off(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w8(this + 118, 8);
    m.w32(this + 112, 1);
    m.w16(this + 116, (-112i16) as u16);
}

/// `tMenuBlank::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001bb58): sliding up from 0.
pub fn abi_menu_blank_transition_on(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    m.w8(this + 118, (-8i8) as u8);
    m.w32(this + 112, 1);
    m.w16(this + 116, 0);
}

/// `tMenuBlank::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001bb70): done sliding down at 0,
/// up at -112.
pub fn abi_menu_blank_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let pos = m.r16(this + 116) as i16;
    let moving = if (m.r8(this + 118) as i8) > 0 { (pos < 0) as u32 } else { (pos >= -111) as u32 };
    m.w32(this + 112, moving);
    c.r[2] = m.r32(this + 112) ^ 1;
}

/// `tMenuOptions::tMenuOptions` (FEMENUEXTENDED.CPP, 0x8001bbac): an options menu for `player`.
pub fn abi_menu_options(c: &mut Cpu) {
    let (this, sp) = (c.r[4], c.r[29]);
    let (player, first) = (c.m.r32(sp + 28), c.m.r32(sp + 32));
    c.r[2] = construct(c, 48, this, 0x8001_b260, sp + 16, Some((first, sp + 36)), MENU_OPTIONS_VT, [0x8001_bbec, 0x8001_bc08], &[player]);
    c.m.w16(this + 128, player as u16);
}

/// `tMenuOptions::~tMenuOptions` (FEMENUEXTENDED.CPP, 0x8001bc24).
pub fn abi_menu_options_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], Some((MENU_VPTR, MENU_OPTIONS_VT)), 0x8001_b2b8, 0x8001_bc3c, abi_menu_nfs4_dtor);
}

/// `tMenuOptions::Draw` (FEMENUEXTENDED.CPP, 0x8001bc4c): the options box (320 wide, 18 a
/// line of the enabled items; growing out of or into the middle over 32 ticks in a
/// transition), centred (the player's half of the screen for one of two), with its title and
/// the enabled items in it; the box's back and edges.
pub fn menu_options_draw(c: &mut Cpu, this: u32) {
    abi::frame(c, 56, |c, _| {
        let mut r = Saved::of(c);
        r.0[2] = this;
        let n = femenu::get_number_enabled_items(&mut c.m, this);
        r.0[6] = 320;
        r.0[0] = n;
        let (obj, f) = method(&mut c.m, this, MENU_VPTR, MENU_TRANSITION_IS_FINISHED);
        r.call_ptr_leaving(c, f, &[obj], 0x8001_bc98);
        let n = n as i16 as i32 as u32;
        r.0[0] = n;
        let mut width = 320u32;
        let mut height = n.wrapping_mul(18);
        let m = &mut c.m;
        let direction = |m: &mut Mem| m.r8(this + 118) as i8;
        if m.r32(this + 112) != 0 {
            let mut t = m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 124));
            if t >= 33 {
                t = 32;
                m.w32(this + 112, 0);
                if direction(m) < 0 {
                    return;
                }
            }
            let grown = t.wrapping_mul(320) >> 5;
            let grown_h = height.wrapping_mul(t) >> 5;
            if direction(m) < 0 {
                width = 320u32.wrapping_sub(grown);
                height = height.wrapping_sub(grown_h);
            } else {
                width = grown;
                height = grown_h;
            }
        } else if direction(m) < 0 {
            return;
        }
        let height = height.wrapping_add(18);
        let rest = 240u32.wrapping_sub(height);
        let mut top = ((rest as i32) >> 1) as u32;
        let left = ((m.r32(SCREEN_WIDTH).wrapping_sub(width) as i32) >> 1) as u32;
        match m.r16(this + 128) as i16 {
            0 => top = top.wrapping_sub(((rest as i32) >> 2) as u32),
            1 => top = top.wrapping_add(((rest as i32) >> 2) as u32),
            _ => {}
        }
        r.0[3] = top;
        r.0[4] = left;
        r.0[5] = height;
        r.0[6] = width;
        if m.r32(this + 112) == 0 {
            let title = lh(m, this + 4);
            if (title as i32) >= 0 {
                let x = sx(left.wrapping_add(((width as i32) >> 1) as u32));
                r.call_leaving(c, MENU_TEXT_POSITIONED_JUSTIFY, &[title, x, sx(top.wrapping_add(2)), 2, 2, 9], 0x8001_bde4);
            }
            let mut y = top.wrapping_add(18);
            r.0[1] = y;
            let mut k = 0u32;
            loop {
                r.0[0] = k;
                let m = &mut c.m;
                let i = k as i16 as i32 as u32;
                let it = item(m, this, i);
                if it == 0 {
                    break;
                }
                if m.r32(it) & ITEM_DISABLED == 0 {
                    let at = y;
                    y = y.wrapping_add(18);
                    r.0[1] = y;
                    let selected = (i == m.r32(this + 8)) as u32;
                    let (obj, f) = method(m, it, ITEM_VPTR, ITEM_DRAW_AT);
                    r.call_ptr_leaving(c, f, &[obj, left.wrapping_add(10), at, selected], 0x8001_be50);
                }
                k = k.wrapping_add(1);
            }
        }
        r.call_ptr_leaving(c, DRAW_SQUARE, &[0, left, top, width, 12], 0x8001_be74);
        r.0[0] = 1;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, left, top.wrapping_add(height), width, (-2i32) as u32, 1], 0x8001_be98);
        let (edge_top, edge_h) = (top.wrapping_add(12), height.wrapping_sub(14));
        r.0[2] = edge_top;
        r.0[1] = edge_h;
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, left, edge_top, 4, edge_h, 1], 0x8001_bebc);
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, left.wrapping_add(width), edge_top, (-4i32) as u32, edge_h, 1], 0x8001_bed8);
        r.call_ptr_leaving(c, DRAW_TRANS_SQUARE, &[0, left, top, width, height, 1], 0x8001_bef4);
        r.call_leaving(c, SET_ABR_MODE, &[0], 0x8001_befc);
    })
}

pub fn abi_menu_options_draw(c: &mut Cpu) {
    menu_options_draw(c, c.r[4]);
}

/// `tMenuOptions::TransitionOff` (FEMENUEXTENDED.CPP, 0x8001bf24): shrinking, from now, with a
/// sound.
pub fn menu_options_transition_off(c: &mut Cpu, this: u32) {
    abi::frame(c, 24, |c, _| {
        let m = &mut c.m;
        m.w8(this + 118, 0xFF);
        let t = m.r32(GAME_TICKS);
        m.w32(this + 112, 1);
        m.w32(this + 124, t);
        Saved::of(c).call_leaving(c, PLAY_FE_SFX, &[18], 0x8001_bf50);
    })
}

pub fn abi_menu_options_transition_off(c: &mut Cpu) {
    menu_options_transition_off(c, c.r[4]);
}

/// `tMenuOptions::TransitionOn` (FEMENUEXTENDED.CPP, 0x8001bf60): each enabled item's, then
/// growing, from now, with a sound.
pub fn menu_options_transition_on(c: &mut Cpu, this: u32) {
    abi::frame(c, 32, |c, _| {
        let mut r = Saved::of(c);
        r.0[1] = this;
        let mut at = this;
        loop {
            r.0[0] = at;
            let it = c.m.r32(at + 16);
            if it == 0 {
                break;
            }
            if c.m.r32(it) & ITEM_DISABLED == 0 {
                let (obj, f) = method(&mut c.m, it, ITEM_VPTR, ITEM_TRANSITION_ON);
                r.call_ptr_leaving(c, f, &[obj], 0x8001_bfbc);
            }
            at = at.wrapping_add(4);
        }
        let m = &mut c.m;
        m.w8(this + 118, 1);
        let t = m.r32(GAME_TICKS);
        m.w32(this + 112, 1);
        m.w32(this + 124, t);
        r.call_leaving(c, PLAY_FE_SFX, &[15], 0x8001_bfe4);
    })
}

pub fn abi_menu_options_transition_on(c: &mut Cpu) {
    menu_options_transition_on(c, c.r[4]);
}

/// `tMenuOptions::TransitionIsFinished` (FEMENUEXTENDED.CPP, 0x8001bff8): 32 ticks after it
/// began.
pub fn abi_menu_options_transition_is_finished(c: &mut Cpu) {
    let (m, this) = (&mut c.m, c.r[4]);
    let moving = (m.r32(GAME_TICKS).wrapping_sub(m.r32(this + 124)) < 32) as u32;
    m.w32(this + 112, moving);
    c.r[2] = moving ^ 1;
}

/// `tMenuOptions::ProcessInput` (FEMENUEXTENDED.CPP, 0x8001c01c): options as start; then
/// `tMenuNFS4`'s.
pub fn menu_options_process_input(c: &mut Cpu, this: u32, player: u32, key: u32, command: u32) {
    abi::frame(c, 24, |c, _| {
        if c.m.r32(key) == 8 {
            c.m.w32(key, 16);
        }
        Saved::of(c).enter(c, 0x8001_b390, 0x8001_c040, |c| menu_nfs4_process_input(c, this, player, key, command));
    })
}

pub fn abi_menu_options_process_input(c: &mut Cpu) {
    menu_options_process_input(c, c.r[4], c.r[5], c.r[6], c.r[7]);
}

/// `tMenuOptions::IsSubMenu` (FEMENUEXTENDED.CPP, 0x8001c050): it is.
pub fn abi_menu_options_is_sub_menu(c: &mut Cpu) {
    c.r[2] = 1;
}

/// `tMenuItemOptionsTwoItemChoice::~tMenuItemOptionsTwoItemChoice`,
/// `tMenuItemOptionsLeftRightChoice::~tMenuItemOptionsLeftRightChoice` (FEMENUEXTENDED.CPP,
/// 0x8001c060, 0x8001c080): the left-right choice's (the vtable left as it is).
pub fn abi_two_item_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], None, 0x8002_4974, 0x8001_c070, femenu::abi_left_right_choice_dtor);
}

pub fn abi_options_left_right_choice_dtor(c: &mut Cpu) {
    destroy(c, c.r[4], None, 0x8002_4974, 0x8001_c090, femenu::abi_left_right_choice_dtor);
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::femenu::tests::{self as base, ADDR, Case, COMMAND, ITEM_SIZE, KEY, MENU2, NEW, NOTHING, TOP, pick, player, via_abi};
    use crate::test_util::{Rng, against_original_in, fe_ram, same_stack_in};
    use std::sync::OnceLock;

    /// The test's structures after FEMENU's: an item of each of the four classes here, a menu
    /// of each of the five, a RECT.
    const EXT_ITEMS: u32 = 0x8014_5000;
    const EXT_MENUS: u32 = 0x8014_6000;
    const MENU_SIZE: u32 = 0x100;
    const RECT: u32 = 0x8014_7000;
    const BUTTON: u32 = EXT_ITEMS;
    const CHOICE: u32 = EXT_ITEMS + ITEM_SIZE;
    const OPTIONS_CHOICE: u32 = EXT_ITEMS + 2 * ITEM_SIZE;
    const TWO_ITEM: u32 = EXT_ITEMS + 3 * ITEM_SIZE;
    /// The options screen's choices' vtables (no constructor of their own builds them).
    const OPTIONS_LEFT_RIGHT_CHOICE_VT: u32 = 0x8001_0850;
    const TWO_ITEM_CHOICE_VT: u32 = 0x8001_07F8;
    const ITEM_VTS: [u32; 4] = [GO_TO_MENU_NFS4_BUTTON_VT, NFS4_LEFT_RIGHT_CHOICE_VT, OPTIONS_LEFT_RIGHT_CHOICE_VT, TWO_ITEM_CHOICE_VT];
    const MENU_VTS: [u32; 5] = [MENU_NFS4_VT, MENU_NFS4_TWO_PLAYER_VT, MENU_NFS4_BOTTOM_VT, MENU_BLANK_VT, MENU_OPTIONS_VT];

    fn menu_of(k: u32) -> u32 {
        EXT_MENUS + MENU_SIZE * k
    }

    /// Texts short enough for the 64 bytes the text box and the button format them into on
    /// their stacks (a longer one overwrites their saved registers).
    fn short_texts() -> &'static [u32] {
        static TEXTS: OnceLock<Vec<u32>> = OnceLock::new();
        TEXTS.get_or_init(|| {
            let ram = fe_ram().unwrap_or_default();
            if ram.is_empty() {
                return vec![0];
            }
            let mut c = crate::test_util::cpu_with(&ram);
            (0..400).filter(|&id| base::text_len(&mut c.m, id as i16) < 56).collect()
        })
    }

    /// Texts like a two-item choice's (ON, OFF, YES, NO...): `FETextRender_FullTextRGB`
    /// overruns its own frame on some longer ones (the saved s0 and s4 its caller gets back).
    fn tiny_texts() -> &'static [u32] {
        static TEXTS: OnceLock<Vec<u32>> = OnceLock::new();
        TEXTS.get_or_init(|| {
            let ram = fe_ram().unwrap_or_default();
            if ram.is_empty() {
                return vec![0];
            }
            let mut c = crate::test_util::cpu_with(&ram);
            (0..400).filter(|&id| (1..8).contains(&base::text_len(&mut c.m, id as i16))).collect()
        })
    }

    fn text(rng: &mut Rng) -> u32 {
        let t = short_texts();
        t[pick(rng, t.len() as u32) as usize]
    }

    /// FEMENU's structures, then: the four items (disabled, reflected or not; a short text, the
    /// selection's fade, a button image and frames, an iterator or a function; their slides,
    /// transitions, speeds and enabled fades anywhere near their ranges), the five menus of
    /// them (the first enabled; their transitions, directions, the time one began, the player),
    /// a RECT, the clock and the front end's player.
    fn shuffle(c: &mut Cpu, rng: &mut Rng) {
        base::shuffle(c, rng);
        let texts = short_texts();
        let m = &mut c.m;
        // (the list's texts like a choice's: see `tiny_texts`; a two-item choice draws the first
        // two, so neither is a negative id, whose text -1 is one of 296 characters)
        let tiny = tiny_texts();
        for k in 0..12 {
            let at = base::LIST + 2 * k;
            let v = m.r16(at) as i16;
            if v > 0 || (k < 2 && v < 0) {
                m.w16(at, tiny[pick(rng, tiny.len() as u32) as usize] as u16);
            }
        }
        for (k, vt) in ITEM_VTS.into_iter().enumerate() {
            let it = EXT_ITEMS + ITEM_SIZE * k as u32;
            let flags = (pick(rng, 4) == 0) as u32 | if pick(rng, 3) == 0 { 0x200 } else { 0 } | [0, 0x40][pick(rng, 2) as usize];
            m.w32(it, flags);
            m.w32(it + 4, texts[pick(rng, texts.len() as u32) as usize]);
            m.w16(it + 8, pick(rng, 140) as u16);
            m.w32(it + 12, if pick(rng, 3) == 0 { u32::MAX } else { 10 + pick(rng, 10) });
            m.w32(it + 16, pick(rng, 4));
            m.w32(it + 20, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(it + 24, vt);
            let data = if vt == GO_TO_MENU_NFS4_BUTTON_VT { [0, NOTHING][pick(rng, 2) as usize] } else { base::iterator(rng) };
            m.w32(it + 28, data);
            for f in (32..44).step_by(2) {
                let v = match pick(rng, 6) {
                    0 => pick(rng, 200) as i32 - 20,
                    1 => [-12, 0, 12][pick(rng, 3) as usize],
                    2 => 14,
                    _ => pick(rng, 140) as i32 - 6,
                };
                m.w16(it + f, v as u16);
            }
        }
        let first = m.r32(BUTTON);
        m.w32(BUTTON, first & !ITEM_DISABLED);
        for (k, vt) in MENU_VTS.into_iter().enumerate() {
            let menu = menu_of(k as u32);
            m.w32(menu + MENU_VPTR, vt);
            m.w32(menu, [0, 1, 2, 4, 0x200][pick(rng, 5) as usize]);
            m.w16(menu + 4, if pick(rng, 2) == 0 { u16::MAX } else { texts[pick(rng, texts.len() as u32) as usize] as u16 });
            let n = 1 + pick(rng, 4);
            m.w32(menu + 8, pick(rng, n));
            m.w32(menu + 12, (pick(rng, 6) == 0) as u32);
            m.w32(menu + 16, BUTTON);
            for i in 1..n {
                m.w32(menu + 16 + 4 * i, EXT_ITEMS + ITEM_SIZE * pick(rng, 4));
            }
            m.w32(menu + 16 + 4 * n, 0);
            m.w32(menu + 84, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(menu + 92, [0, MENU2][pick(rng, 2) as usize]);
            m.w32(menu + 96, [0, NOTHING][pick(rng, 2) as usize]);
            m.w32(menu + 108, pick(rng, 2));
            m.w32(menu + 112, pick(rng, 2));
            m.w16(menu + 116, (pick(rng, 140) as i32 - 125) as u16);
            m.w8(menu + 118, [1, 0xFF, 8, 0xF8, 0][pick(rng, 5) as usize]);
            m.w8(menu + 119, pick(rng, 4) as u8);
            m.w8(menu + 120, n as u8);
            let now = m.r32(GAME_TICKS);
            m.w32(menu + 124, now.wrapping_sub(pick(rng, 40)));
            m.w16(menu + 128, pick(rng, 3) as u16);
        }
        let ticks = m.r32(GAME_TICKS).wrapping_add(rng.next() as u32 & 0xFFF);
        m.w32(GAME_TICKS, ticks);
        let app = m.r32(FE_APP);
        m.w8(app + 556, pick(rng, 2) as u8);
        for (k, v) in [pick(rng, 300), pick(rng, 240), pick(rng, 200), 8 + pick(rng, 12)].into_iter().enumerate() {
            m.w16(RECT + 2 * k as u32, v as u16);
        }
    }

    fn item(rng: &mut Rng) -> u32 {
        EXT_ITEMS + ITEM_SIZE * pick(rng, 4)
    }

    fn menu(rng: &mut Rng) -> u32 {
        menu_of(pick(rng, 5))
    }

    fn cases() -> Vec<Case> {
        vec![
            ("SetHelpPos", 0x8001_a128, nfs4rc::f_8001a128, |_, _| vec![RECT]),
            ("DrawTextBox", 0x8001_a158, nfs4rc::f_8001a158, |_, r| {
                let t = if pick(r, 5) == 0 { u32::MAX } else { text(r) };
                vec![t, RECT, pick(r, 200), pick(r, 60), pick(r, 140), pick(r, 2), pick(r, 2)]
            }),
            ("Button", 0x8001_a60c, nfs4rc::f_8001a60c, |_, r| vec![NEW, text(r), MENU2, NOTHING, 10 + pick(r, 10), pick(r, 4)]),
            ("~Button", 0x8001_a674, nfs4rc::f_8001a674, |_, r| vec![BUTTON, 2 * pick(r, 2)]),
            ("Button::Draw", 0x8001_a69c, nfs4rc::f_8001a69c, |_, r| vec![BUTTON, pick(r, 300), pick(r, 240), pick(r, 2)]),
            ("Button::TransitionOn", 0x8001_a918, nfs4rc::f_8001a918, |_, _| vec![BUTTON]),
            ("Button::TransitionOff", 0x8001_a928, nfs4rc::f_8001a928, |_, _| vec![BUTTON]),
            ("Button::TransitionIsFinished", 0x8001_a934, nfs4rc::f_8001a934, |_, _| vec![BUTTON]),
            ("Button::UpdateTransition", 0x8001_a940, nfs4rc::f_8001a940, |_, r| vec![BUTTON, pick(r, 2)]),
            ("Choice", 0x8001_a99c, nfs4rc::f_8001a99c, |_, r| vec![NEW, text(r), base::iterator(r), 10 + pick(r, 10), pick(r, 4)]),
            ("~Choice", 0x8001_aa00, nfs4rc::f_8001aa00, |_, r| vec![CHOICE, 2 * pick(r, 2)]),
            ("Choice::Draw", 0x8001_aa28, nfs4rc::f_8001aa28, |_, r| vec![CHOICE, pick(r, 300), pick(r, 240), pick(r, 2)]),
            ("Choice::TransitionOn", 0x8001_acbc, nfs4rc::f_8001acbc, |_, _| vec![CHOICE]),
            ("Choice::TransitionOff", 0x8001_accc, nfs4rc::f_8001accc, |_, _| vec![CHOICE]),
            ("Choice::TransitionIsFinished", 0x8001_acd8, nfs4rc::f_8001acd8, |_, _| vec![CHOICE]),
            ("Choice::UpdateTransition", 0x8001_ace4, nfs4rc::f_8001ace4, |_, r| vec![CHOICE, pick(r, 2)]),
            ("OptionsChoice::Draw", 0x8001_ad40, nfs4rc::f_8001ad40, |_, r| vec![OPTIONS_CHOICE, pick(r, 300), pick(r, 240), pick(r, 2)]),
            ("TwoItem::TransitionOn", 0x8001_af18, nfs4rc::f_8001af18, |_, _| vec![TWO_ITEM]),
            ("TwoItem::Draw", 0x8001_af74, nfs4rc::f_8001af74, |_, r| vec![TWO_ITEM, pick(r, 300), pick(r, 240), pick(r, 2)]),
            ("tMenuNFS4", 0x8001_b1f8, nfs4rc::f_8001b1f8, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 300), item(r), item(r), 0]),
            ("tMenuNFS4(bare)", 0x8001_b260, nfs4rc::f_8001b260, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 70000)]),
            ("~tMenuNFS4", 0x8001_b2b8, nfs4rc::f_8001b2b8, |_, r| vec![menu(r), 2 * pick(r, 2)]),
            ("tMenuNFS4::Initialize", 0x8001_b2e0, nfs4rc::f_8001b2e0, |_, r| vec![menu(r)]),
            ("tMenuNFS4::ProcessInput", 0x8001_b390, nfs4rc::f_8001b390, |_, r| vec![menu(r), player(r), KEY, COMMAND]),
            ("tMenuNFS4::TransitionOff", 0x8001_b3b0, nfs4rc::f_8001b3b0, |_, r| vec![menu(r)]),
            ("tMenuNFS4::TransitionOn", 0x8001_b438, nfs4rc::f_8001b438, |_, r| vec![menu(r)]),
            ("tMenuNFS4::TransitionIsFinished", 0x8001_b4c0, nfs4rc::f_8001b4c0, |_, r| vec![menu(r)]),
            ("tMenuNFS4::UpdateTransition", 0x8001_b560, nfs4rc::f_8001b560, |_, r| vec![menu(r)]),
            ("tMenuNFS4::DrawItem", 0x8001_b5f4, nfs4rc::f_8001b5f4, |c, r| {
                let menu = menu(r);
                let n = c.m.r8(menu + 120) as u32;
                vec![menu, pick(r, n.max(1))]
            }),
            ("tMenuNFS4::Draw", 0x8001_b654, nfs4rc::f_8001b654, |_, r| vec![menu_of(pick(r, 2))]),
            ("TwoPlayer", 0x8001_b79c, nfs4rc::f_8001b79c, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 300), item(r), item(r), 0]),
            ("~TwoPlayer", 0x8001_b808, nfs4rc::f_8001b808, |_, r| vec![menu_of(1), 2 * pick(r, 2)]),
            ("TwoPlayer::DrawItem", 0x8001_b830, nfs4rc::f_8001b830, |c, _| {
                let n = c.m.r8(menu_of(1) + 120) as u32;
                vec![menu_of(1), n.saturating_sub(1)]
            }),
            ("Bottom", 0x8001_b8a8, nfs4rc::f_8001b8a8, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 300), item(r), 0]),
            ("~Bottom", 0x8001_b910, nfs4rc::f_8001b910, |_, r| vec![menu_of(2), 2 * pick(r, 2)]),
            ("Bottom::Draw", 0x8001_b938, nfs4rc::f_8001b938, |_, _| vec![menu_of(2)]),
            ("Blank", 0x8001_b9f8, nfs4rc::f_8001b9f8, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 300)]),
            ("~Blank", 0x8001_ba5c, nfs4rc::f_8001ba5c, |_, r| vec![menu_of(3), 2 * pick(r, 2)]),
            ("Blank::Draw", 0x8001_ba84, nfs4rc::f_8001ba84, |_, _| vec![menu_of(3)]),
            ("Blank::ProcessInput", 0x8001_bae8, nfs4rc::f_8001bae8, |_, r| vec![menu_of(3), player(r), KEY, COMMAND]),
            ("Blank::DebounceKeys", 0x8001_bb34, nfs4rc::f_8001bb34, |_, _| vec![menu_of(3)]),
            ("Blank::TransitionOff", 0x8001_bb3c, nfs4rc::f_8001bb3c, |_, _| vec![menu_of(3)]),
            ("Blank::TransitionOn", 0x8001_bb58, nfs4rc::f_8001bb58, |_, _| vec![menu_of(3)]),
            ("Blank::TransitionIsFinished", 0x8001_bb70, nfs4rc::f_8001bb70, |_, _| vec![menu_of(3)]),
            ("Options", 0x8001_bbac, nfs4rc::f_8001bbac, |_, r| vec![NEW, pick(r, 8), 0x8014_5800, MENU2, MENU2, NOTHING, pick(r, 300), pick(r, 3), item(r), item(r), 0]),
            ("~Options", 0x8001_bc24, nfs4rc::f_8001bc24, |_, r| vec![menu_of(4), 2 * pick(r, 2)]),
            ("Options::Draw", 0x8001_bc4c, nfs4rc::f_8001bc4c, |_, _| vec![menu_of(4)]),
            ("Options::TransitionOff", 0x8001_bf24, nfs4rc::f_8001bf24, |_, _| vec![menu_of(4)]),
            ("Options::TransitionOn", 0x8001_bf60, nfs4rc::f_8001bf60, |_, _| vec![menu_of(4)]),
            ("Options::TransitionIsFinished", 0x8001_bff8, nfs4rc::f_8001bff8, |_, _| vec![menu_of(4)]),
            ("Options::ProcessInput", 0x8001_c01c, nfs4rc::f_8001c01c, |_, r| vec![menu_of(4), player(r), KEY, COMMAND]),
            ("Options::IsSubMenu", 0x8001_c050, nfs4rc::f_8001c050, |_, _| vec![menu_of(4)]),
            ("~TwoItem", 0x8001_c060, nfs4rc::f_8001c060, |_, r| vec![TWO_ITEM, 2 * pick(r, 2)]),
            ("~OptionsChoice", 0x8001_c080, nfs4rc::f_8001c080, |_, r| vec![OPTIONS_CHOICE, 2 * pick(r, 2)]),
        ]
    }

    #[test]
    fn femenuextended_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            if std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            ADDR.set(addr);
            let args_rng = std::cell::RefCell::new(Rng(0xFE70 ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            against_original_in(&ram, name, 300, 0xFE71 + k as u64, shuffle, original, &|c| args(c, &mut args_rng.borrow_mut()), via_abi);
        }
    }

    #[test]
    fn femenuextended_stack_fuzz() {
        let Some(ram) = fe_ram() else { return };
        for (k, (name, addr, original, args)) in cases().into_iter().enumerate() {
            let frame = (0..8).map(|i| ram[(addr & 0x1F_FFFF) as usize + 4 * i..][..4].to_vec()).any(|w| w[2..4] == [0xBD, 0x27]);
            if !frame || std::env::var("FEMENU_ONLY").is_ok_and(|v| v != name) {
                continue;
            }
            same_stack_in(&ram, name, addr, original, 0x8001_3000, TOP, 150, 0xFE91 + k as u64, shuffle, &args);
        }
    }
}
